use rusqlite::{params, Connection, OptionalExtension};

use super::granularity::{pick_granularity, Granularity};
use super::precompute::ensure_dashboard_ready;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatTotals {
    pub message_count: i64,
    pub participant_count: i64,
    pub word_count: i64,
    pub char_count: i64,
    pub first_msg_ts: Option<i64>,
    pub last_msg_ts: Option<i64>,
}

/// Сводка чата — сообщений/слов/символов/период. `message_count`,
/// `word_count`, `char_count` читаются из денормализованных колонок
/// `chats` (без сканирования `messages` — досчитаны один раз при импорте
/// или лениво при первом открытии чата, импортированного раньше этого
/// среза, см. `precompute::ensure_dashboard_ready`). `participant_count` —
/// вживую по `people` (не денормализованная колонка `chats`, которая —
/// просто снимок на момент импорта и не отражает merge/hide из разбора
/// участников, макет 3d): дёшево при реалистичном числе участников,
/// всегда согласовано.
pub fn get_chat_totals(conn: &Connection, chat_id: i64) -> rusqlite::Result<Option<ChatTotals>> {
    ensure_dashboard_ready(conn, chat_id)?;
    conn.query_row(
        "SELECT c.message_count, \
                (SELECT COUNT(*) FROM people p WHERE p.chat_id = c.id AND p.merged_into IS NULL AND p.is_hidden = 0), \
                c.first_msg_ts, c.last_msg_ts, c.word_count, c.char_count \
         FROM chats c \
         WHERE c.id = ?1",
        params![chat_id],
        |row| {
            Ok(ChatTotals {
                message_count: row.get(0)?,
                participant_count: row.get(1)?,
                first_msg_ts: row.get(2)?,
                last_msg_ts: row.get(3)?,
                word_count: row.get(4)?,
                char_count: row.get(5)?,
            })
        },
    )
    .optional()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ActivityBucket {
    pub bucket: String,
    pub count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ActivitySeries {
    pub granularity: Granularity,
    pub buckets: Vec<ActivityBucket>,
}

/// Активность во времени, гранулярность выбирается один раз по периоду
/// чата (docs/SPEC.md §7). `None`, если у чата нет ни одного сообщения с
/// известным `ts` — графику нечего показывать, а не пустой график.
/// Бакеты читаются из `activity_buckets` — предвычислены при импорте той
/// же гранулярностью, что и выбирается здесь (см. `precompute.rs`).
pub fn get_activity_series(
    conn: &Connection,
    chat_id: i64,
) -> rusqlite::Result<Option<ActivitySeries>> {
    ensure_dashboard_ready(conn, chat_id)?;

    let bounds: Option<(Option<i64>, Option<i64>)> = conn
        .query_row(
            "SELECT first_msg_ts, last_msg_ts FROM chats WHERE id = ?1",
            params![chat_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    let Some((Some(first_ts), Some(last_ts))) = bounds else {
        return Ok(None);
    };

    let granularity = pick_granularity(first_ts, last_ts);
    let mut stmt = conn
        .prepare("SELECT bucket, count FROM activity_buckets WHERE chat_id = ?1 ORDER BY bucket")?;
    let buckets = stmt
        .query_map(params![chat_id], |row| {
            Ok(ActivityBucket {
                bucket: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(Some(ActivitySeries {
        granularity,
        buckets,
    }))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ParticipantStat {
    pub person_id: i64,
    pub display_name: Option<String>,
    pub message_count: i64,
    pub avg_len: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TopParticipants {
    /// Чат на двоих (или меньше) — рейтинг участников вырождается
    /// (docs/SPEC.md §7), фронт показывает соотношение, а не таблицу.
    pub degenerate: bool,
    pub participants: Vec<ParticipantStat>,
}

/// Топ участников — читает `person_message_stats`, а не сканирует
/// `messages` (см. `precompute.rs`). Таблица хранится по сырому
/// `person_id`, но остаётся согласованной с текущим состоянием `people`:
/// `merge_people` физически переносит `messages.person_id` на `primary_id`
/// и сразу пересчитывает `person_message_stats` целиком — под id
/// объединённого участника строка не задерживается, поэтому здесь
/// достаточно простого `JOIN` + фильтра `merged_into IS NULL`, без
/// дополнительного резолва цепочек слияний.
pub fn get_top_participants(conn: &Connection, chat_id: i64) -> rusqlite::Result<TopParticipants> {
    ensure_dashboard_ready(conn, chat_id)?;

    // Вживую, не из денормализованной chats.participant_count — та не
    // видит merge/hide из разбора участников (макет 3d).
    let participant_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM people WHERE chat_id = ?1 AND merged_into IS NULL AND is_hidden = 0",
        params![chat_id],
        |row| row.get(0),
    )?;

    let mut stmt = conn.prepare(
        "SELECT p.id, p.display_name, pms.message_count, \
                CAST(pms.text_len_sum AS REAL) / pms.message_count \
         FROM person_message_stats pms JOIN people p ON p.id = pms.person_id \
         WHERE pms.chat_id = ?1 AND p.merged_into IS NULL AND p.is_hidden = 0 \
         ORDER BY pms.message_count DESC",
    )?;
    let participants = stmt
        .query_map(params![chat_id], |row| {
            Ok(ParticipantStat {
                person_id: row.get(0)?,
                display_name: row.get(1)?,
                message_count: row.get(2)?,
                avg_len: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(TopParticipants {
        degenerate: participant_count <= 2,
        participants,
    })
}
