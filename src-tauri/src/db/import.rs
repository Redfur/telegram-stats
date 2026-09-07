use std::collections::HashMap;
use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::import::{parse_export, ImportError, ParsedEvent};

use super::ChatSummary;

fn now_unixtime() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn db_err(e: rusqlite::Error) -> ImportError {
    ImportError::Sink(e.to_string())
}

/// Возвращает `person_id`, заводя строку в `people` при первом появлении
/// `tg_person_id` в этом импорте. Кэш — единственный буфер в памяти:
/// по числу участников, не по числу сообщений.
fn resolve_person(
    tx: &rusqlite::Transaction,
    cache: &mut HashMap<String, i64>,
    chat_id: i64,
    tg_person_id: &str,
    display_name: Option<&str>,
) -> rusqlite::Result<i64> {
    if let Some(&id) = cache.get(tg_person_id) {
        return Ok(id);
    }
    tx.execute(
        "INSERT INTO people (chat_id, tg_person_id, display_name) VALUES (?1, ?2, ?3)",
        params![chat_id, tg_person_id, display_name],
    )?;
    let id = tx.last_insert_rowid();
    cache.insert(tg_person_id.to_string(), id);
    Ok(id)
}

/// Потоково пишет `result.json` в SQLite: одна транзакция на весь импорт,
/// `person_id` резолвится через HashMap в памяти (по участникам, не по
/// сообщениям), `Vec<Message>` нигде не собирается (docs/SPEC.md §6).
/// Сервисные события и `Unrecognized` не персистятся в MVP — ни одна
/// текущая метрика их не читает (см. docs/PLAN.md).
///
/// Повторный импорт того же чата (совпадает `tg_chat_id`) заменяет
/// существующую запись, а не дублирует её (docs/SPEC.md §3), и делает это
/// не создавая новый `chats.id` — рельс/дашборд держат ссылку на чат по
/// этому id, он обязан пережить повторный импорт. Пока идёт стриминг,
/// строки пишутся под временный `chat_id` (сам `ChatMeta.tg_chat_id`
/// известен только по завершении парсинга). Когда он известен и находится
/// старая запись с тем же `tg_chat_id` — её `messages`/`people` стираются,
/// свежие переезжают на её (стабильный) `id` через `UPDATE ... SET
/// chat_id`, временная строка удаляется, а не наоборот.
pub fn import_into_db(
    conn: &mut Connection,
    reader: impl Read,
    source_filename: &str,
    source_file_size: Option<i64>,
) -> Result<ChatSummary, ImportError> {
    let imported_at = now_unixtime();
    let tx = conn.transaction().map_err(db_err)?;

    tx.execute(
        "INSERT INTO chats (tg_chat_id, name, chat_type, source_filename, imported_at, source_file_size) \
         VALUES (NULL, NULL, '', ?1, ?2, ?3)",
        params![source_filename, imported_at, source_file_size],
    )
    .map_err(db_err)?;
    let mut chat_id = tx.last_insert_rowid();

    let mut people_cache: HashMap<String, i64> = HashMap::new();
    let mut message_count: i64 = 0;
    let mut min_ts: Option<i64> = None;
    let mut max_ts: Option<i64> = None;

    let meta = {
        let mut insert_msg = tx
            .prepare(
                "INSERT INTO messages (chat_id, tg_msg_id, person_id, ts, text_len, word_count) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(db_err)?;

        parse_export(reader, |event| {
            let ParsedEvent::Message(m) = event else {
                return Ok(()); // service/unrecognized — не персистятся в MVP
            };

            let person_id = match m.from.as_ref() {
                Some(p) => Some(
                    resolve_person(
                        &tx,
                        &mut people_cache,
                        chat_id,
                        &p.id,
                        p.display_name.as_deref(),
                    )
                    .map_err(db_err)?,
                ),
                None => None,
            };
            let text_len = m.text.plain.chars().count() as i64;
            let word_count = m.text.plain.split_whitespace().count() as i64;

            insert_msg
                .execute(params![
                    chat_id,
                    m.tg_id,
                    person_id,
                    m.date_unixtime,
                    text_len,
                    word_count
                ])
                .map_err(db_err)?;

            message_count += 1;
            if let Some(ts) = m.date_unixtime {
                min_ts = Some(min_ts.map_or(ts, |cur| cur.min(ts)));
                max_ts = Some(max_ts.map_or(ts, |cur| cur.max(ts)));
            }
            Ok(())
        })?
    };

    if let Some(tg_chat_id) = meta.tg_chat_id {
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM chats WHERE tg_chat_id = ?1 AND id != ?2",
                params![tg_chat_id, chat_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_err)?;
        if let Some(old_id) = existing {
            // Старая запись держит id, на который уже могли ссылаться
            // (рельс, открытый дашборд) — переносим свежие данные на
            // него, а не наоборот, чтобы этот id пережил повторный импорт.
            tx.execute("DELETE FROM messages WHERE chat_id = ?1", params![old_id])
                .map_err(db_err)?;
            tx.execute("DELETE FROM people WHERE chat_id = ?1", params![old_id])
                .map_err(db_err)?;
            tx.execute(
                "UPDATE messages SET chat_id = ?1 WHERE chat_id = ?2",
                params![old_id, chat_id],
            )
            .map_err(db_err)?;
            tx.execute(
                "UPDATE people SET chat_id = ?1 WHERE chat_id = ?2",
                params![old_id, chat_id],
            )
            .map_err(db_err)?;
            tx.execute("DELETE FROM chats WHERE id = ?1", params![chat_id])
                .map_err(db_err)?;
            chat_id = old_id;
        }
    }

    let participant_count = people_cache.len() as i64;
    tx.execute(
        "UPDATE chats SET tg_chat_id=?1, name=?2, chat_type=?3, first_msg_ts=?4, last_msg_ts=?5, \
         message_count=?6, participant_count=?7, source_file_size=?8 WHERE id=?9",
        params![
            meta.tg_chat_id,
            meta.name,
            meta.chat_type,
            min_ts,
            max_ts,
            message_count,
            participant_count,
            source_file_size,
            chat_id
        ],
    )
    .map_err(db_err)?;

    tx.commit().map_err(db_err)?;

    Ok(ChatSummary {
        id: chat_id,
        tg_chat_id: meta.tg_chat_id,
        name: meta.name,
        chat_type: meta.chat_type,
        message_count,
        participant_count,
        first_msg_ts: min_ts,
        last_msg_ts: max_ts,
        imported_at,
        source_file_size,
    })
}
