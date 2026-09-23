use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, serde::Serialize)]
pub struct TitleHistorySegment {
    /// `None` — период до первого известного события (нет `created`,
    /// начало истории не восстановимо) — честная деградация вместо
    /// угадывания (docs/SPEC.md §7), не «пустая строка».
    pub name: Option<String>,
    pub start_ts: i64,
    /// `None` — текущий сегмент, действует по сей день.
    pub end_ts: Option<i64>,
    pub author_display_name: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TitleHistory {
    pub segments: Vec<TitleHistorySegment>,
}

struct TitleEvent {
    ts: i64,
    kind: String,
    title: Option<String>,
    author_display_name: Option<String>,
}

/// История переименований чата — полоса под названием в шапке дашборда
/// (макет «Дашборд», ход `t11`/вариант `11a`). Источник — `chat_events`
/// (`kind IN ('created', 'title_changed')`), заполняется один раз при
/// импорте, как и остальной таймлайн участников (`db/import.rs`,
/// миграция `0011_timeline.sql`) — self-heal невозможен: чат,
/// импортированный до этой миграции, просто не имеет строк. Такой чат и
/// чат, который ни разу не переименовывали, — оба возвращают `None`, без
/// различия: вырожденная метрика скрывается целиком (docs/SPEC.md §7),
/// объяснять читателю нечего.
pub fn get_title_history(
    conn: &Connection,
    chat_id: i64,
) -> rusqlite::Result<Option<TitleHistory>> {
    let mut stmt = conn.prepare(
        "SELECT ce.ts, ce.kind, ce.detail, p.display_name \
         FROM chat_events ce LEFT JOIN people p ON p.id = ce.actor_id \
         WHERE ce.chat_id = ?1 AND ce.kind IN ('created', 'title_changed') \
         ORDER BY ce.ts",
    )?;
    let events = stmt
        .query_map(params![chat_id], |row| {
            Ok(TitleEvent {
                ts: row.get(0)?,
                kind: row.get(1)?,
                title: row.get(2)?,
                author_display_name: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Название чата не сравнивалось ни разу (только `created`, либо
    // вовсе нет событий) — нечего показывать.
    if !events.iter().any(|e| e.kind == "title_changed") {
        return Ok(None);
    }

    let mut segments = Vec::new();
    let mut iter = events.into_iter().peekable();

    // Первое известное событие — не `created`: экспорт не захватил
    // создание чата (старая группа, либо история начинается не с
    // основания) — начало не восстановимо, сегмент честно без имени, от
    // первого наблюдаемого сообщения чата.
    if let Some(first) = iter.peek() {
        if first.kind != "created" {
            let first_ts = first.ts;
            let start_ts = conn
                .query_row(
                    "SELECT first_msg_ts FROM chats WHERE id = ?1",
                    params![chat_id],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .optional()?
                .flatten()
                .unwrap_or(first_ts);
            segments.push(TitleHistorySegment {
                name: None,
                start_ts,
                end_ts: Some(first_ts),
                author_display_name: None,
            });
        }
    }

    while let Some(event) = iter.next() {
        let end_ts = iter.peek().map(|next| next.ts);
        segments.push(TitleHistorySegment {
            name: event.title,
            start_ts: event.ts,
            end_ts,
            author_display_name: event.author_display_name,
        });
    }

    Ok(Some(TitleHistory { segments }))
}
