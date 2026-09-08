use std::collections::HashMap;
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
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

/// person_id → отображаемое имя → (первое, последнее) время встречи.
type AliasSeen = HashMap<i64, HashMap<String, (Option<i64>, Option<i64>)>>;

/// Read-обёртка, считающая прочитанные байты в общий счётчик — честный
/// процент прогресса по объёму файла, без второго прохода только ради
/// подсчёта сообщений (второй проход противоречил бы цели «импорт 1M <
/// 3 мин», SPEC §11).
pub struct CountingReader<R> {
    inner: R,
    count: Arc<AtomicU64>,
}

impl<R: Read> CountingReader<R> {
    /// Возвращает обёрнутый reader и разделяемый счётчик прочитанных байт.
    pub fn new(inner: R) -> (Self, Arc<AtomicU64>) {
        let count = Arc::new(AtomicU64::new(0));
        (
            CountingReader {
                inner,
                count: count.clone(),
            },
            count,
        )
    }
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportProgress {
    pub bytes_read: u64,
    pub total_bytes: Option<i64>,
    pub message_count: i64,
    pub people_count: i64,
}

/// Прогресс и отмена для одного вызова `import_into_db`. `on_progress`
/// вызывается на каждое сообщение — дёшево (вызов замыкания), решение
/// придержать/пробросить его наружу (throttling IPC) остаётся за
/// вызывающей стороной (Tauri-командой). `cancel` проверяется тем же
/// местом цикла — отмена не оставляет мусора: транзакция просто не
/// коммитится (docs/SPEC.md §3).
pub struct ImportContext<'a, F: FnMut(ImportProgress)> {
    pub bytes_read: &'a AtomicU64,
    pub on_progress: F,
    pub cancel: &'a AtomicBool,
}

/// Возвращает per-person `person_id`, заводя строку в `people` при первом
/// появлении `tg_person_id` в этом импорте. Кэш — единственный буфер в
/// памяти: по числу участников, не по числу сообщений.
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
pub fn import_into_db<F: FnMut(ImportProgress)>(
    conn: &mut Connection,
    reader: impl Read,
    source_filename: &str,
    source_file_size: Option<i64>,
    ctx: &mut ImportContext<'_, F>,
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
    // person_id → отображаемое имя → (первое, последнее) время встречи
    // этого имени. Буфер по числу уникальных пар (человек, имя), не по
    // числу сообщений — тот же принцип, что и у `people_cache`.
    let mut alias_seen: AliasSeen = HashMap::new();
    let mut message_count: i64 = 0;
    let mut min_ts: Option<i64> = None;
    let mut max_ts: Option<i64> = None;

    let meta = {
        let mut insert_msg = tx
            .prepare(
                "INSERT INTO messages (chat_id, tg_msg_id, person_id, ts, text_len, word_count, \
                 media_type, has_photo, has_file, has_poll, has_location, has_contact, \
                 forwarded_from, edited_ts, duration_seconds) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            )
            .map_err(db_err)?;

        parse_export(reader, |event| {
            if ctx.cancel.load(Ordering::Relaxed) {
                return Err(ImportError::Cancelled);
            }

            let ParsedEvent::Message(m) = event else {
                return Ok(()); // service/unrecognized — не персистятся в MVP
            };

            let mut alias_pid_name: Option<(i64, String)> = None;
            let person_id = match m.from.as_ref() {
                Some(p) => {
                    let pid = resolve_person(
                        &tx,
                        &mut people_cache,
                        chat_id,
                        &p.id,
                        p.display_name.as_deref(),
                    )
                    .map_err(db_err)?;
                    if let Some(name) = p.display_name.clone() {
                        alias_pid_name = Some((pid, name));
                    }
                    Some(pid)
                }
                None => None,
            };
            let text_len = m.text.plain.chars().count() as i64;
            let word_count = m.text.plain.split_whitespace().count() as i64;

            // "photo"/"poll"/"location_information"/"contact_information" не
            // смоделированы явно на Message (docs/SPEC.md — Telegram кладёт их
            // рядом с прочими полями сообщения, без общего media_type) —
            // единственный способ их увидеть — заглянуть в catch-all `extra`
            // (model.rs). `has_file` — только когда media_type пуст, иначе
            // документ с явным медиа-типом (видео/голосовое/etc.) задвоился
            // бы ещё и в «Файлы».
            let has_photo = m.extra.contains_key("photo");
            let has_poll = m.extra.contains_key("poll");
            let has_location = m.extra.contains_key("location_information");
            let has_contact = m.extra.contains_key("contact_information");
            let has_file = m.media_type.is_none() && m.extra.contains_key("file");

            insert_msg
                .execute(params![
                    chat_id,
                    m.tg_id,
                    person_id,
                    m.date_unixtime,
                    text_len,
                    word_count,
                    m.media_type,
                    has_photo,
                    has_file,
                    has_poll,
                    has_location,
                    has_contact,
                    m.forwarded_from,
                    m.edited_unixtime,
                    m.duration_seconds,
                ])
                .map_err(db_err)?;

            message_count += 1;
            if let Some(ts) = m.date_unixtime {
                min_ts = Some(min_ts.map_or(ts, |cur| cur.min(ts)));
                max_ts = Some(max_ts.map_or(ts, |cur| cur.max(ts)));
            }
            if let Some((pid, name)) = alias_pid_name {
                let entry = alias_seen
                    .entry(pid)
                    .or_default()
                    .entry(name)
                    .or_insert((None, None));
                if let Some(ts) = m.date_unixtime {
                    entry.0 = Some(entry.0.map_or(ts, |cur| cur.min(ts)));
                    entry.1 = Some(entry.1.map_or(ts, |cur| cur.max(ts)));
                }
            }

            (ctx.on_progress)(ImportProgress {
                bytes_read: ctx.bytes_read.load(Ordering::Relaxed),
                total_bytes: source_file_size,
                message_count,
                people_count: people_cache.len() as i64,
            });

            Ok(())
        })?
    };

    {
        let mut insert_alias = tx
            .prepare(
                "INSERT INTO aliases (person_id, name, first_seen, last_seen) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(person_id, name) DO UPDATE SET \
                   first_seen = MIN(COALESCE(first_seen, excluded.first_seen), COALESCE(excluded.first_seen, first_seen)), \
                   last_seen  = MAX(COALESCE(last_seen,  excluded.last_seen),  COALESCE(excluded.last_seen,  last_seen))",
            )
            .map_err(db_err)?;
        for (pid, names) in &alias_seen {
            for (name, (first, last)) in names {
                insert_alias
                    .execute(params![pid, name, first, last])
                    .map_err(db_err)?;
            }
        }
    }

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

            // people, в отличие от messages, не стираем и не перезаливаем
            // временные строки — merged_into/is_hidden/алиасы старых строк
            // должны пережить повторный импорт (иначе любая ручная разметка
            // участников терялась бы на следующей выгрузке того же чата).
            // Сопоставляем по tg_person_id: если у старого чата уже есть
            // такой человек — его строка выигрывает (id/merged_into/
            // is_hidden сохраняются, обновляется только имя), временная
            // удаляется; если человек новый — просто перевешиваем временную
            // строку на старый chat_id, как и раньше.
            let new_people: Vec<(i64, String, Option<String>)> = {
                let mut stmt = tx
                    .prepare("SELECT id, tg_person_id, display_name FROM people WHERE chat_id = ?1")
                    .map_err(db_err)?;
                let rows = stmt
                    .query_map(params![chat_id], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                    })
                    .map_err(db_err)?;
                let collected: rusqlite::Result<Vec<_>> = rows.collect();
                collected.map_err(db_err)?
            };

            for (new_person_id, tg_person_id, display_name) in new_people {
                let matched_old: Option<i64> = tx
                    .query_row(
                        "SELECT id FROM people WHERE chat_id = ?1 AND tg_person_id = ?2",
                        params![old_id, tg_person_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(db_err)?;

                if let Some(old_person_id) = matched_old {
                    // Старая строка сама могла быть объединена в кого-то
                    // ещё до этого повторного импорта (merge_people, макет
                    // 3d) — новые сообщения должны сразу попасть на
                    // конечного «главного» участника, а не осесть на
                    // объединённой строке, где их не увидит ни одна
                    // метрика (WHERE merged_into IS NULL).
                    let merged_into: Option<i64> = tx
                        .query_row(
                            "SELECT merged_into FROM people WHERE id = ?1",
                            params![old_person_id],
                            |row| row.get(0),
                        )
                        .map_err(db_err)?;
                    let target_id = merged_into.unwrap_or(old_person_id);

                    tx.execute(
                        "UPDATE people SET display_name = ?1 WHERE id = ?2",
                        params![display_name, old_person_id],
                    )
                    .map_err(db_err)?;
                    tx.execute(
                        "UPDATE messages SET person_id = ?1 WHERE person_id = ?2",
                        params![target_id, new_person_id],
                    )
                    .map_err(db_err)?;
                    tx.execute(
                        "UPDATE OR IGNORE aliases SET person_id = ?1 WHERE person_id = ?2",
                        params![target_id, new_person_id],
                    )
                    .map_err(db_err)?;
                    // ON DELETE CASCADE подчистит алиасы временной строки,
                    // которые не смогли перевеситься из-за конфликта UNIQUE.
                    tx.execute("DELETE FROM people WHERE id = ?1", params![new_person_id])
                        .map_err(db_err)?;
                } else {
                    tx.execute(
                        "UPDATE people SET chat_id = ?1 WHERE id = ?2",
                        params![old_id, new_person_id],
                    )
                    .map_err(db_err)?;
                }
            }

            tx.execute(
                "UPDATE messages SET chat_id = ?1 WHERE chat_id = ?2",
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
         message_count=?6, participant_count=?7, source_file_size=?8, message_types_ready=1 \
         WHERE id=?9",
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

    crate::metrics::recompute_dashboard_metrics(&tx, chat_id).map_err(db_err)?;
    crate::metrics::recompute_message_type_metrics(&tx, chat_id).map_err(db_err)?;

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
