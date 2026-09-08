use rusqlite::{params, Connection};

use crate::metrics::{
    ensure_dashboard_ready, recompute_dashboard_metrics, recompute_message_type_participant_stats,
    recompute_reaction_person_stats,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct PersonRow {
    pub id: i64,
    pub display_name: Option<String>,
    pub aliases: Vec<String>,
    pub message_count: i64,
    pub first_msg_ts: Option<i64>,
    pub last_msg_ts: Option<i64>,
    pub is_hidden: bool,
}

/// Участники чата для шага разбора (макет 3d) — без тех, кто уже
/// объединён в кого-то другого (`merged_into IS NOT NULL`), они больше
/// не самостоятельная строка. Скрытые (`is_hidden`) остаются в списке —
/// это шаг, где их и скрывают/возвращают.
///
/// Читает `person_message_stats` (`metrics::precompute`), а не сканирует
/// `messages` через `JOIN` на каждое открытие экрана «Участники» — там
/// раньше и подвисало на больших чатах, тем же паттерном, что и
/// дашборд/типы сообщений (`LEFT JOIN`, не `JOIN`: участник без единого
/// сообщения — не должен пропасть из списка). `ensure_dashboard_ready` —
/// тот же self-heal для чата, импортированного до появления этого среза.
pub fn get_people(conn: &Connection, chat_id: i64) -> rusqlite::Result<Vec<PersonRow>> {
    ensure_dashboard_ready(conn, chat_id)?;

    let mut stmt = conn.prepare(
        "SELECT p.id, p.display_name, p.is_hidden, \
                COALESCE(pms.message_count, 0), pms.first_ts, pms.last_ts \
         FROM people p \
         LEFT JOIN person_message_stats pms \
           ON pms.chat_id = p.chat_id AND pms.person_id = p.id \
         WHERE p.chat_id = ?1 AND p.merged_into IS NULL \
         ORDER BY COALESCE(pms.message_count, 0) DESC",
    )?;
    let mut rows = stmt
        .query_map(params![chat_id], |row| {
            Ok(PersonRow {
                id: row.get(0)?,
                display_name: row.get(1)?,
                is_hidden: row.get::<_, i64>(2)? != 0,
                message_count: row.get(3)?,
                first_msg_ts: row.get(4)?,
                last_msg_ts: row.get(5)?,
                aliases: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut alias_stmt = conn.prepare(
        "SELECT a.person_id, a.name FROM aliases a \
         JOIN people p ON p.id = a.person_id \
         WHERE p.chat_id = ?1 AND p.merged_into IS NULL \
         ORDER BY a.last_seen DESC",
    )?;
    let alias_rows = alias_stmt
        .query_map(params![chat_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (person_id, name) in alias_rows {
        if let Some(row) = rows.iter_mut().find(|r| r.id == person_id) {
            row.aliases.push(name);
        }
    }

    Ok(rows)
}

/// Объединяет `other_ids` в `primary_id`: сообщения и алиасы переезжают,
/// `other_ids` помечаются `merged_into` (мягкое удаление — строка
/// остаётся, чтобы при следующем повторном импорте тот же `tg_person_id`
/// снова опознался как уже объединённый, а не воскрес отдельной строкой).
/// Все id проверяются на принадлежность `chat_id`, даже притом что `id`
/// и так уникален глобально — конвенция CLAUDE.md «никаких запросов без
/// фильтра по chat_id».
///
/// Физически переносит `messages.person_id` и `reaction_actors.person_id`
/// на `primary_id` — из-за этого предвычисленные по сырому `person_id`
/// `person_message_stats` (`metrics::precompute`),
/// `message_type_participant_stats` (`metrics::message_types`) и
/// `reaction_person_stats` (`metrics::reactions` — обе стороны:
/// `reaction_actors.person_id` меняет "кто ставит", `messages.person_id`
/// меняет "кому ставят") перестают соответствовать исходным таблицам,
/// поэтому в конце пересчитываем все целиком (дёшево: слияние — редкое,
/// явное действие пользователя из экрана «Участники», не часть пути
/// открытия дашборда, который этот пересчёт как раз избавляет от полного
/// скана).
pub fn merge_people(
    conn: &Connection,
    chat_id: i64,
    primary_id: i64,
    other_ids: &[i64],
) -> rusqlite::Result<()> {
    for &other_id in other_ids {
        if other_id == primary_id {
            continue;
        }
        conn.execute(
            "UPDATE messages SET person_id = ?1 \
             WHERE person_id = ?2 AND chat_id = ?3",
            params![primary_id, other_id, chat_id],
        )?;
        conn.execute(
            "UPDATE reaction_actors SET person_id = ?1 \
             WHERE person_id = ?2 AND chat_id = ?3",
            params![primary_id, other_id, chat_id],
        )?;
        conn.execute(
            "UPDATE OR IGNORE aliases SET person_id = ?1 \
             WHERE person_id = ?2 \
               AND EXISTS (SELECT 1 FROM people WHERE id = ?2 AND chat_id = ?3)",
            params![primary_id, other_id, chat_id],
        )?;
        conn.execute(
            "UPDATE people SET merged_into = ?1 WHERE id = ?2 AND chat_id = ?3",
            params![primary_id, other_id, chat_id],
        )?;
    }
    recompute_dashboard_metrics(conn, chat_id)?;
    recompute_message_type_participant_stats(conn, chat_id)?;
    recompute_reaction_person_stats(conn, chat_id)?;
    Ok(())
}

pub fn hide_person(
    conn: &Connection,
    chat_id: i64,
    person_id: i64,
    hidden: bool,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE people SET is_hidden = ?1 WHERE id = ?2 AND chat_id = ?3",
        params![hidden, person_id, chat_id],
    )?;
    Ok(())
}

pub fn rename_person(
    conn: &Connection,
    chat_id: i64,
    person_id: i64,
    display_name: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE people SET display_name = ?1 WHERE id = ?2 AND chat_id = ?3",
        params![display_name, person_id, chat_id],
    )?;
    Ok(())
}
