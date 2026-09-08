use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, serde::Serialize)]
pub struct CompositionPoint {
    pub ts: i64,
    pub count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GroupEventRow {
    pub kind: String,
    pub ts: i64,
    pub actor_id: Option<i64>,
    pub actor_display_name: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentStartKind {
    /// Есть резолвленное `membership_events`-событие входа.
    Joined,
    /// Нет ни одного входного события — участник уже писал сообщения (или
    /// упомянут в другом событии) до самого раннего момента, который
    /// парсер вообще увидел. Не выдумываем дату входа (docs/SPEC.md §7) —
    /// сегмент открывается с первого наблюдения этого человека, без точки
    /// на конце линии.
    SinceStart,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentEndKind {
    Left,
    Removed,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Segment {
    pub start_ts: i64,
    pub start_kind: SegmentStartKind,
    pub end_ts: Option<i64>,
    pub end_kind: Option<SegmentEndKind>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PersonTimelineRow {
    pub person_id: i64,
    pub display_name: Option<String>,
    /// Обычно один элемент; больше одного — участник выходил и
    /// возвращался (промежуток между сегментами рисуется пунктиром).
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TimelineSummary {
    pub total: i64,
    pub current: i64,
    pub left: i64,
    /// `chats.last_msg_ts` — та же точка отсчёта, что бэкенд использует
    /// для срока всё ещё активных участников (median_tenure_days), нужна
    /// фронту, чтобы посчитать срок конкретного наведённого участника тем
    /// же способом, не системными часами (docs/SPEC.md §11 — метрики
    /// живут в периоде чата, не в wall-clock).
    pub as_of_ts: Option<i64>,
    pub median_tenure_days: Option<f64>,
    pub peak_headcount: i64,
    pub peak_headcount_ts: Option<i64>,
    /// Количество групповых событий (`chat_events`) — создание, смена
    /// названия/фото, миграция. Не включает членские события — те уже
    /// покрыты total/current/left.
    pub group_event_count: i64,
    pub last_group_event_ts: Option<i64>,
    /// Сколько строк `membership_events` не удалось сопоставить с
    /// конкретным участником (имя не найдено или неоднозначно среди
    /// известных людей чата) — предупреждение для фронта, аналог
    /// `given_data_incomplete` в реакциях.
    pub unresolved_event_count: i64,
    pub composition: Vec<CompositionPoint>,
    pub group_events: Vec<GroupEventRow>,
    /// Отсортированы по дате первого появления в чате.
    pub people: Vec<PersonTimelineRow>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TimelineOverview {
    /// Чат импортирован до появления этой метрики (`timeline_ready = 0`).
    /// Как и у реакций — не self-heal: сервисные события нигде, кроме
    /// самого `result.json` на момент импорта, не хранятся.
    NeedsReimport,
    /// Чат прошёл актуальный импортер, но ни одного членского или
    /// группового события не нашлось — личный чат, `saved_messages`, или
    /// экспорт, в котором участник переписки был единственным (docs/SPEC.md
    /// §7). Определяется по фактическому отсутствию строк, не по
    /// `chat_type` — эта строка не валидируется парсером (docs/SPEC.md §2).
    NotApplicable,
    Available(TimelineSummary),
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        Some((values[mid - 1] + values[mid]) / 2.0)
    } else {
        Some(values[mid])
    }
}

/// Таймлайн участников — дашборд, секция 05 (docs/PLAN.md Фаза 3). Объём
/// исходных данных (события на участника, не на сообщение) на порядки
/// меньше `messages`, поэтому вся сводка считается вживую по
/// `membership_events`/`chat_events`, без отдельных precompute-таблиц —
/// бюджет <200 мс (docs/SPEC.md §11) не под угрозой.
pub fn get_participant_timeline(
    conn: &Connection,
    chat_id: i64,
) -> rusqlite::Result<TimelineOverview> {
    let row: Option<(i64, Option<i64>)> = conn
        .query_row(
            "SELECT timeline_ready, last_msg_ts FROM chats WHERE id = ?1",
            params![chat_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((ready, last_msg_ts)) = row else {
        return Ok(TimelineOverview::NeedsReimport);
    };
    if ready == 0 {
        return Ok(TimelineOverview::NeedsReimport);
    }

    let event_count: i64 = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM membership_events WHERE chat_id = ?1) + \
                (SELECT COUNT(*) FROM chat_events WHERE chat_id = ?1)",
        params![chat_id],
        |row| row.get(0),
    )?;
    if event_count == 0 {
        return Ok(TimelineOverview::NotApplicable);
    }

    let unresolved_event_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM membership_events WHERE chat_id = ?1 AND person_id IS NULL",
        params![chat_id],
        |row| row.get(0),
    )?;

    let mut events_by_person: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT person_id, ts, kind FROM membership_events \
             WHERE chat_id = ?1 AND person_id IS NOT NULL ORDER BY person_id, ts",
        )?;
        let rows = stmt.query_map(params![chat_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for r in rows {
            let (pid, ts, kind) = r?;
            events_by_person.entry(pid).or_default().push((ts, kind));
        }
    }

    let mut first_message_ts: HashMap<i64, i64> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT person_id, MIN(ts) FROM messages \
             WHERE chat_id = ?1 AND person_id IS NOT NULL GROUP BY person_id",
        )?;
        let rows = stmt.query_map(params![chat_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        for r in rows {
            let (pid, ts) = r?;
            first_message_ts.insert(pid, ts);
        }
    }

    let mut candidate_ids: HashSet<i64> = first_message_ts.keys().copied().collect();
    candidate_ids.extend(events_by_person.keys().copied());

    // Живые (не объединённые, не скрытые) люди чата — тот же фильтр, что
    // и у остальных метрик (docs/CLAUDE.md «Конвенции»).
    let mut display_names: HashMap<i64, Option<String>> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, display_name FROM people \
             WHERE chat_id = ?1 AND merged_into IS NULL AND is_hidden = 0",
        )?;
        let rows = stmt.query_map(params![chat_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        for r in rows {
            let (pid, name) = r?;
            display_names.insert(pid, name);
        }
    }
    candidate_ids.retain(|pid| display_names.contains_key(pid));

    let mut people: Vec<PersonTimelineRow> = Vec::with_capacity(candidate_ids.len());
    let mut tenures_days: Vec<f64> = Vec::with_capacity(candidate_ids.len());
    // (ts, +1/-1) — композиция строится из тех же стартов/закрытий
    // сегментов, что и строки участников: `since_start`-сегменты дают
    // такой же вклад, как явные joined/left, иначе пик состава считал бы
    // только тех, у кого нашлось резолвленное событие входа.
    let mut composition_events: Vec<(i64, i64)> = Vec::new();
    let mut current_count: i64 = 0;

    for pid in candidate_ids {
        let mut evs = events_by_person.remove(&pid).unwrap_or_default();
        evs.sort_by_key(|(ts, _)| *ts);

        let mut segments: Vec<Segment> = Vec::new();
        let mut idx = 0usize;
        loop {
            let (start_ts, start_kind) = if idx < evs.len() && evs[idx].1 == "joined" {
                let ts = evs[idx].0;
                idx += 1;
                (ts, SegmentStartKind::Joined)
            } else {
                let ts = first_message_ts
                    .get(&pid)
                    .copied()
                    .unwrap_or_else(|| evs.get(idx).map(|(ts, _)| *ts).unwrap_or(0));
                (ts, SegmentStartKind::SinceStart)
            };
            let (end_ts, end_kind) =
                if idx < evs.len() && (evs[idx].1 == "left" || evs[idx].1 == "removed") {
                    let (ts, kind) = &evs[idx];
                    let end_kind = if kind == "left" {
                        SegmentEndKind::Left
                    } else {
                        SegmentEndKind::Removed
                    };
                    let ts = *ts;
                    idx += 1;
                    (Some(ts), Some(end_kind))
                } else {
                    (None, None)
                };

            composition_events.push((start_ts, 1));
            if let Some(ts) = end_ts {
                composition_events.push((ts, -1));
            }
            let effective_end = end_ts.unwrap_or_else(|| last_msg_ts.unwrap_or(start_ts));
            tenures_days.push((effective_end - start_ts).max(0) as f64 / 86_400.0);

            segments.push(Segment {
                start_ts,
                start_kind,
                end_ts,
                end_kind,
            });

            if idx >= evs.len() {
                break;
            }
        }

        let is_current = segments.last().is_some_and(|s| s.end_ts.is_none());
        if is_current {
            current_count += 1;
        }

        people.push(PersonTimelineRow {
            person_id: pid,
            display_name: display_names.remove(&pid).flatten(),
            segments,
        });
    }

    people.sort_by_key(|p| p.segments.first().map(|s| s.start_ts).unwrap_or(i64::MAX));

    composition_events.sort_by_key(|(ts, _)| *ts);
    let mut composition: Vec<CompositionPoint> = Vec::with_capacity(composition_events.len());
    let mut running: i64 = 0;
    let mut peak: i64 = 0;
    let mut peak_ts: Option<i64> = None;
    for (ts, delta) in &composition_events {
        running += delta;
        composition.push(CompositionPoint {
            ts: *ts,
            count: running,
        });
        if running > peak {
            peak = running;
            peak_ts = Some(*ts);
        }
    }

    let median_tenure_days = median(&mut tenures_days);
    let total = people.len() as i64;
    let left = total - current_count;

    let mut group_events: Vec<GroupEventRow> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT ce.kind, ce.ts, ce.actor_id, p.display_name, ce.detail \
             FROM chat_events ce LEFT JOIN people p ON p.id = ce.actor_id \
             WHERE ce.chat_id = ?1 ORDER BY ce.ts",
        )?;
        let rows = stmt.query_map(params![chat_id], |row| {
            Ok(GroupEventRow {
                kind: row.get(0)?,
                ts: row.get(1)?,
                actor_id: row.get(2)?,
                actor_display_name: row.get(3)?,
                detail: row.get(4)?,
            })
        })?;
        for r in rows {
            group_events.push(r?);
        }
    }
    let group_event_count = group_events.len() as i64;
    let last_group_event_ts = group_events.last().map(|e| e.ts);

    Ok(TimelineOverview::Available(TimelineSummary {
        total,
        current: current_count,
        left,
        as_of_ts: last_msg_ts,
        median_tenure_days,
        peak_headcount: peak,
        peak_headcount_ts: peak_ts,
        group_event_count,
        last_group_event_ts,
        unresolved_event_count,
        composition,
        group_events,
        people,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_overview_serializes_with_flattened_fields_next_to_status_tag() {
        let overview = TimelineOverview::Available(TimelineSummary {
            total: 5,
            current: 4,
            left: 1,
            as_of_ts: Some(100),
            median_tenure_days: Some(12.5),
            peak_headcount: 5,
            peak_headcount_ts: Some(100),
            group_event_count: 1,
            last_group_event_ts: Some(100),
            unresolved_event_count: 0,
            composition: vec![],
            group_events: vec![],
            people: vec![],
        });
        let json = serde_json::to_string(&overview).unwrap();
        assert!(json.contains("\"status\":\"available\""));
        assert!(json.contains("\"total\":5"));
    }

    #[test]
    fn median_handles_empty_odd_and_even() {
        assert_eq!(median(&mut []), None);
        assert_eq!(median(&mut [3.0]), Some(3.0));
        assert_eq!(median(&mut [1.0, 2.0, 3.0, 4.0]), Some(2.5));
    }
}
