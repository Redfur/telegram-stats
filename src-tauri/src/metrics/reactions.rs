use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};

use super::precompute::with_write_lock;

/// Сколько участников показываем в "кто ставит"/"кому ставят" на карточке
/// сводки — витрина, не полный список (как топ-эмодзи, которых обычно
/// мало, или как топ сообщений ниже).
const TOP_PERSON_LIMIT: i64 = 10;
const TOP_REACTED_MESSAGES_LIMIT: i64 = 10;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReactionEmojiCount {
    pub kind: String,
    pub emoji: Option<String>,
    pub document_id: Option<String>,
    pub count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReactionPersonCount {
    pub person_id: i64,
    pub display_name: Option<String>,
    pub count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReactionsSummary {
    pub total_reactions: i64,
    pub reacted_message_count: i64,
    pub total_messages: i64,
    pub top_emoji: Vec<ReactionEmojiCount>,
    pub given: Vec<ReactionPersonCount>,
    pub received: Vec<ReactionPersonCount>,
    /// `true` — хотя бы у одной реакции в чате `count` больше числа
    /// известных акторов: Telegram отдаёт в `recent` не полный список
    /// реагировавших (docs/SPEC.md §8.5). "Кому ставят" (`received`)
    /// остаётся точным в любом случае — считается по авторитетному
    /// `count`, а не по `recent`; страдает только "кто ставит" (`given`).
    pub given_data_incomplete: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReactionsOverview {
    /// Чат импортирован до появления этой метрики. В отличие от «Типов
    /// сообщений» это не self-heal-пересчёт: реакции нигде, кроме самого
    /// `result.json` на момент импорта, не хранятся (миграция `0008`) —
    /// без повторного импорта данных попросту нет.
    NeedsReimport,
    /// Чат прошёл актуальный импортер, но исходный экспорт не нёс поля
    /// `reactions` ни на одном сообщении — старая версия Telegram Desktop
    /// (docs/SPEC.md §7/§8.5). Повторный импорт того же файла не поможет,
    /// нужен более новый экспорт.
    NotInExport,
    Available(ReactionsSummary),
}

fn top_person_stats(
    conn: &Connection,
    chat_id: i64,
    count_column: &str,
) -> rusqlite::Result<Vec<ReactionPersonCount>> {
    let sql = format!(
        "SELECT rps.person_id, p.display_name, rps.{count_column} \
         FROM reaction_person_stats rps \
         JOIN people p ON p.id = rps.person_id \
         WHERE rps.chat_id = ?1 AND rps.{count_column} > 0 \
           AND p.merged_into IS NULL AND p.is_hidden = 0 \
         ORDER BY rps.{count_column} DESC \
         LIMIT {TOP_PERSON_LIMIT}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![chat_id], |row| {
        Ok(ReactionPersonCount {
            person_id: row.get(0)?,
            display_name: row.get(1)?,
            count: row.get(2)?,
        })
    })?;
    rows.collect()
}

/// Сводка блока «Реакции» — читает precomputed `reaction_emoji_stats`/
/// `reaction_person_stats`/денормализованные `chats.reaction_total`/
/// `reacted_message_count` (см. `recompute_reaction_metrics`), не сканирует
/// `message_reactions` заново на каждое открытие блока.
pub fn get_reactions_overview(
    conn: &Connection,
    chat_id: i64,
) -> rusqlite::Result<ReactionsOverview> {
    let row: Option<(i64, i64, i64, i64, i64)> = conn
        .query_row(
            "SELECT message_count, reactions_ready, reactions_supported, \
                    reaction_total, reacted_message_count \
             FROM chats WHERE id = ?1",
            params![chat_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?;
    let Some((total_messages, ready, supported, total_reactions, reacted_message_count)) = row
    else {
        return Ok(ReactionsOverview::NeedsReimport);
    };
    if ready == 0 {
        return Ok(ReactionsOverview::NeedsReimport);
    }
    if supported == 0 {
        return Ok(ReactionsOverview::NotInExport);
    }

    let mut emoji_stmt = conn.prepare(
        "SELECT kind, emoji, document_id, count FROM reaction_emoji_stats \
         WHERE chat_id = ?1 ORDER BY count DESC",
    )?;
    let top_emoji = emoji_stmt
        .query_map(params![chat_id], |row| {
            Ok(ReactionEmojiCount {
                kind: row.get(0)?,
                emoji: row.get(1)?,
                document_id: row.get(2)?,
                count: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let given = top_person_stats(conn, chat_id, "given_count")?;
    let received = top_person_stats(conn, chat_id, "received_count")?;

    let given_data_incomplete: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM message_reactions \
         WHERE chat_id = ?1 AND count > known_actor_count)",
        params![chat_id],
        |row| row.get::<_, i64>(0).map(|v| v != 0),
    )?;

    Ok(ReactionsOverview::Available(ReactionsSummary {
        total_reactions,
        reacted_message_count,
        total_messages,
        top_emoji,
        given,
        received,
        given_data_incomplete,
    }))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TopReactedMessage {
    pub message_id: i64,
    pub person_id: Option<i64>,
    pub display_name: Option<String>,
    pub ts: Option<i64>,
    pub preview_text: Option<String>,
    pub total_count: i64,
}

/// Топ-10 самых залайканных сообщений — простой indexed-запрос к
/// `message_reactions` (по одной строке на реагировавшее сообщение и тип
/// реакции, не по всем `messages`), с превью текста, снятым во время
/// импорта (`message_reaction_previews`, `messages` сам текст не хранит).
/// Отдельной precompute-таблицы под топ не заведено — `message_reactions`
/// уже ограничен только сообщениями с реакциями, GROUP BY по нему дёшев.
pub fn get_top_reacted_messages(
    conn: &Connection,
    chat_id: i64,
) -> rusqlite::Result<Vec<TopReactedMessage>> {
    let ready: Option<i64> = conn
        .query_row(
            "SELECT reactions_ready FROM chats WHERE id = ?1",
            params![chat_id],
            |row| row.get(0),
        )
        .optional()?;
    if ready != Some(1) {
        return Ok(vec![]);
    }

    let mut stmt = conn.prepare(
        "SELECT mr.message_id, m.person_id, p.display_name, m.ts, mrp.preview_text, SUM(mr.count) \
         FROM message_reactions mr \
         JOIN messages m ON m.id = mr.message_id \
         LEFT JOIN people p ON p.id = m.person_id \
         LEFT JOIN message_reaction_previews mrp \
           ON mrp.chat_id = mr.chat_id AND mrp.message_id = mr.message_id \
         WHERE mr.chat_id = ?1 \
         GROUP BY mr.message_id \
         ORDER BY SUM(mr.count) DESC \
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![chat_id, TOP_REACTED_MESSAGES_LIMIT], |row| {
        Ok(TopReactedMessage {
            message_id: row.get(0)?,
            person_id: row.get(1)?,
            display_name: row.get(2)?,
            ts: row.get(3)?,
            preview_text: row.get(4)?,
            total_count: row.get(5)?,
        })
    })?;
    rows.collect()
}

/// Пересчитывает `reaction_emoji_stats`/`reaction_person_stats` и
/// денормализованные `chats.reaction_total`/`reacted_message_count` из уже
/// сохранённых `message_reactions`/`reaction_actors` — вызывается в конце
/// импорта (та же транзакция, что и `recompute_dashboard_metrics`/
/// `recompute_message_type_metrics`). Не self-heal: сырых
/// `message_reactions`/`reaction_actors` у чата, импортированного раньше
/// миграции `0008`, попросту нет и пересчитать их неоткуда — см.
/// `ReactionsOverview::NeedsReimport`. Безопасна к конкурентным вызовам
/// для одного `chat_id` — см. `precompute::with_write_lock`.
pub fn recompute_reaction_metrics(conn: &Connection, chat_id: i64) -> rusqlite::Result<()> {
    with_write_lock(conn, || {
        conn.execute(
            "UPDATE chats SET \
                reaction_total = (SELECT COALESCE(SUM(count), 0) FROM message_reactions WHERE chat_id = ?1), \
                reacted_message_count = \
                    (SELECT COUNT(DISTINCT message_id) FROM message_reactions WHERE chat_id = ?1) \
             WHERE id = ?1",
            params![chat_id],
        )?;

        conn.execute(
            "DELETE FROM reaction_emoji_stats WHERE chat_id = ?1",
            params![chat_id],
        )?;
        conn.execute(
            "INSERT INTO reaction_emoji_stats (chat_id, kind, emoji, document_id, count) \
             SELECT chat_id, kind, emoji, document_id, SUM(count) \
             FROM message_reactions WHERE chat_id = ?1 \
             GROUP BY kind, emoji, document_id",
            params![chat_id],
        )?;

        recompute_reaction_person_stats(conn, chat_id)?;

        Ok(())
    })
}

/// Только `reaction_person_stats` — вызывается из `recompute_reaction_metrics`
/// при полном пересчёте, а также сама по себе из `merge_people`: слияние
/// переносит и `messages.person_id` (меняет "кому ставят"), и
/// `reaction_actors.person_id` (меняет "кто ставит") на `primary_id`, из-за
/// чего старая агрегация по обеим сторонам перестаёт соответствовать
/// исходным таблицам. Не трогает `reaction_emoji_stats` — она
/// person-agnostic, слияние участников на неё не влияет. Безопасна к
/// конкурентным вызовам для одного `chat_id` — см. `precompute::with_write_lock`.
pub fn recompute_reaction_person_stats(conn: &Connection, chat_id: i64) -> rusqlite::Result<()> {
    with_write_lock(conn, || {
        conn.execute(
            "DELETE FROM reaction_person_stats WHERE chat_id = ?1",
            params![chat_id],
        )?;

        let mut given: HashMap<i64, i64> = HashMap::new();
        {
            let mut stmt = conn.prepare(
                "SELECT person_id, COUNT(*) FROM reaction_actors \
                 WHERE chat_id = ?1 AND person_id IS NOT NULL GROUP BY person_id",
            )?;
            let rows = stmt.query_map(params![chat_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
            for r in rows {
                let (pid, count): (i64, i64) = r?;
                given.insert(pid, count);
            }
        }

        let mut received: HashMap<i64, i64> = HashMap::new();
        {
            let mut stmt = conn.prepare(
                "SELECT m.person_id, SUM(mr.count) FROM message_reactions mr \
                 JOIN messages m ON m.id = mr.message_id \
                 WHERE mr.chat_id = ?1 AND m.person_id IS NOT NULL GROUP BY m.person_id",
            )?;
            let rows = stmt.query_map(params![chat_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
            for r in rows {
                let (pid, count): (i64, i64) = r?;
                received.insert(pid, count);
            }
        }

        let mut all_ids: HashSet<i64> = given.keys().copied().collect();
        all_ids.extend(received.keys().copied());

        let mut insert = conn.prepare(
            "INSERT INTO reaction_person_stats (chat_id, person_id, given_count, received_count) \
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for pid in all_ids {
            insert.execute(params![
                chat_id,
                pid,
                given.get(&pid).copied().unwrap_or(0),
                received.get(&pid).copied().unwrap_or(0),
            ])?;
        }

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_overview_serializes_with_flattened_fields_next_to_status_tag() {
        let overview = ReactionsOverview::Available(ReactionsSummary {
            total_reactions: 5,
            reacted_message_count: 2,
            total_messages: 10,
            top_emoji: vec![],
            given: vec![],
            received: vec![],
            given_data_incomplete: false,
        });
        let json = serde_json::to_string(&overview).unwrap();
        eprintln!("{json}");
        assert!(json.contains("\"status\":\"available\""));
        assert!(json.contains("\"total_reactions\":5"));
    }
}
