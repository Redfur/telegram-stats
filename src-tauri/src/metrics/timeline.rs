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

/// Отрезок непрерывного присутствия — объединение покрытия по ВСЕМ
/// аккаунтам, объединённым в этого участника (`merge_people`), не
/// строгое чередование вход/выход одного `membership_events.person_id`:
/// если один аккаунт вышел, а другой (уже объединённый в того же
/// человека) в этот момент ещё состоял в чате, линия не рвётся. Для
/// самого раннего сегмента `start_ts` может быть моментом первого
/// наблюдения человека (первое сообщение), а не резолвленным входом —
/// не выдумываем дату входа (docs/SPEC.md §7); отличить такой случай от
/// настоящего входа можно по отсутствию совпадающей по времени точки в
/// `PersonTimelineRow.events`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Segment {
    pub start_ts: i64,
    pub end_ts: Option<i64>,
}

/// Сырое разрешённое событие (вход/выход/удаление) — рисуется точкой на
/// линии участника независимо от того, сдвигает ли оно границу сегмента:
/// у объединённых аккаунтов, состоявших в чате одновременно, выход
/// одного и вход другого не создают разрыва (см. `Segment`), но точки
/// обоих событий остаются видны.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PersonEvent {
    pub ts: i64,
    /// `"joined"` | `"left"` | `"removed"` — сырая строка из
    /// `membership_events.kind`, не enum: только для подписи точки.
    pub kind: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PersonTimelineRow {
    pub person_id: i64,
    pub display_name: Option<String>,
    /// Обычно один элемент; больше одного — реальный (не покрытый другим
    /// объединённым аккаунтом) промежуток отсутствия, рисуется пунктиром.
    pub segments: Vec<Segment>,
    /// Все резолвленные события этого участника (по всем объединённым в
    /// него аккаунтам), отсортированы по времени — для точек на линии.
    pub events: Vec<PersonEvent>,
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

/// Отрезки непрерывного присутствия из уже отсортированных по времени
/// событий одного участника (`joined`/`left`/`removed`) — объединение
/// покрытия, а не строгое чередование: считает активные "источники
/// присутствия" счётчиком (объединённые `merge_people` аккаунты могут
/// перекрываться во времени), сегмент закрывается только когда счётчик
/// возвращается к нулю. Так выход одного объединённого аккаунта, пока
/// другой ещё состоял в чате, не рвёт линию — ровно то же самое, что
/// делает `compositionStepPoints` на фронте для чата целиком, только на
/// уровне одного человека.
///
/// `first_message_ts` — если оно раньше первого события (или событий нет
/// вовсе), участник уже присутствовал на момент первого наблюдения —
/// сегмент открывается с этого момента без выдуманной даты входа
/// (docs/SPEC.md §7); реальные события всё равно видны отдельно в
/// `PersonTimelineRow.events`, поэтому такой "тихий" старт не теряет
/// информацию, только не изобретает её.
fn build_coverage_segments(evs: &[(i64, String)], first_message_ts: Option<i64>) -> Vec<Segment> {
    let implicit_start = match (first_message_ts, evs.first()) {
        (Some(fm), Some((first_ev_ts, _))) if fm < *first_ev_ts => Some(fm),
        (Some(fm), None) => Some(fm),
        _ => None,
    };

    let mut coverage: i64 = 0;
    let mut current_start: Option<i64> = None;
    let mut segments: Vec<Segment> = Vec::new();

    if let Some(start_ts) = implicit_start {
        coverage = 1;
        current_start = Some(start_ts);
    }

    for (ts, kind) in evs {
        if kind == "joined" {
            if coverage == 0 {
                current_start = Some(*ts);
            }
            coverage += 1;
        } else if coverage > 0 {
            coverage -= 1;
            if coverage == 0 {
                if let Some(start) = current_start.take() {
                    segments.push(Segment {
                        start_ts: start,
                        end_ts: Some(*ts),
                    });
                }
            }
        }
        // `left`/`removed` при coverage == 0 — рассогласованные данные
        // (например неоднозначное имя резолвилось на разных участников в
        // разных событиях); не уходим в отрицательное покрытие, просто
        // игнорируем лишний выход.
    }

    if coverage > 0 {
        if let Some(start) = current_start {
            segments.push(Segment {
                start_ts: start,
                end_ts: None,
            });
        }
    }

    if segments.is_empty() {
        // Ни одного входа не нашлось (только выход/удаление без
        // предшествующего входа и без сообщений вообще) — вырожденный
        // случай, точечный сегмент на месте события, а не пустая строка.
        if let Some((ts, _)) = evs.first() {
            segments.push(Segment {
                start_ts: *ts,
                end_ts: Some(*ts),
            });
        }
    }

    segments
}

/// Объединяет интервалы покрытия нескольких бывших аккаунтов одного
/// участника (`merge_people`) в непрерывные отрезки — тот же приём
/// объединения интервалов, что и `build_coverage_segments` для одного
/// аккаунта, но здесь входные интервалы уже готовые сегменты, а не сырые
/// события. Старты (`+1`) на одном и том же `ts` обрабатываются раньше
/// концов (`-1`) — иначе ровно совпадающие "конец одного аккаунта" и
/// "начало другого" на мгновение обнулили бы покрытие и создали лишний
/// (хоть и нулевой ширины) разрыв вместо одного непрерывного отрезка.
fn union_segments(segments: &[Segment]) -> Vec<Segment> {
    let mut points: Vec<(i64, i64)> = Vec::with_capacity(segments.len() * 2);
    for seg in segments {
        points.push((seg.start_ts, 1));
        if let Some(end) = seg.end_ts {
            points.push((end, -1));
        }
    }
    points.sort_by_key(|(ts, delta)| (*ts, -*delta));

    let mut merged: Vec<Segment> = Vec::new();
    let mut coverage: i64 = 0;
    let mut current_start: Option<i64> = None;
    for (ts, delta) in points {
        let before = coverage;
        coverage += delta;
        if before == 0 && coverage > 0 {
            current_start = Some(ts);
        } else if before > 0 && coverage == 0 {
            if let Some(start) = current_start.take() {
                merged.push(Segment {
                    start_ts: start,
                    end_ts: Some(ts),
                });
            }
        }
    }
    if coverage > 0 {
        if let Some(start) = current_start {
            merged.push(Segment {
                start_ts: start,
                end_ts: None,
            });
        }
    }
    merged
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

    // Группируем по СЫРОМУ `person_id`, не по объединённой идентичности:
    // `merge_people` намеренно не переносит `membership_events.person_id`
    // (см. её комментарий) — каждый бывший аккаунт держит свою историю
    // входов/выходов отдельно, чтобы её можно было разрешить в покрытие
    // самостоятельно (ниже) и только потом объединить интервалы.
    let mut events_by_raw: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
    // raw person_id → эффективная идентичность (COALESCE(merged_into, id)).
    let mut effective_of: HashMap<i64, i64> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT me.person_id, me.ts, me.kind, COALESCE(p.merged_into, p.id) \
             FROM membership_events me \
             JOIN people p ON p.id = me.person_id \
             WHERE me.chat_id = ?1 AND me.person_id IS NOT NULL \
             ORDER BY me.person_id, me.ts",
        )?;
        let rows = stmt.query_map(params![chat_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        for r in rows {
            let (raw_pid, ts, kind, effective_pid) = r?;
            events_by_raw.entry(raw_pid).or_default().push((ts, kind));
            effective_of.insert(raw_pid, effective_pid);
        }
    }
    let mut raws_by_effective: HashMap<i64, Vec<i64>> = HashMap::new();
    for (&raw_pid, &effective_pid) in &effective_of {
        raws_by_effective
            .entry(effective_pid)
            .or_default()
            .push(raw_pid);
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
    candidate_ids.extend(raws_by_effective.keys().copied());

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
    // (ts, +1/-1) — композиция строится из уже объединённых по покрытию
    // сегментов участников (см. build_segments), не из сырых событий:
    // после merge_people у одного человека могут быть события двух
    // бывших аккаунтов, и без объединения покрытия композиция считала бы
    // его дважды в моменты, когда оба аккаунта ещё числились в чате.
    let mut composition_events: Vec<(i64, i64)> = Vec::new();
    let mut current_count: i64 = 0;

    for pid in candidate_ids {
        // Один эффективный участник может стоять за несколькими сырыми
        // аккаунтами (merge_people) — считаем покрытие для каждого
        // отдельно (только у самого `pid`, то есть аккаунта-приёмника,
        // есть право на `since_start`-допущение по объединённому первому
        // сообщению; у слитых аккаунтов без собственного входа это
        // допущение неприменимо — их собственная история сообщений уже
        // неразличима после переноса `messages.person_id`), затем
        // объединяем интервалы и сырые события в одну строку.
        let raw_ids = raws_by_effective.get(&pid).cloned().unwrap_or_default();
        let mut per_account_segments: Vec<Segment> = Vec::new();
        let mut events: Vec<PersonEvent> = Vec::new();

        for raw_pid in &raw_ids {
            let mut evs = events_by_raw.remove(raw_pid).unwrap_or_default();
            evs.sort_by_key(|(ts, _)| *ts);
            events.extend(evs.iter().map(|(ts, kind)| PersonEvent {
                ts: *ts,
                kind: kind.clone(),
            }));
            let since_start_ts = if *raw_pid == pid {
                first_message_ts.get(&pid).copied()
            } else {
                None
            };
            per_account_segments.extend(build_coverage_segments(&evs, since_start_ts));
        }
        if raw_ids.is_empty() {
            // Ни одного зафиксированного события вообще — участник виден
            // только по сообщениям (тот же since_start, что и раньше).
            per_account_segments =
                build_coverage_segments(&[], first_message_ts.get(&pid).copied());
        }
        events.sort_by_key(|e| e.ts);

        let segments = union_segments(&per_account_segments);

        for seg in &segments {
            composition_events.push((seg.start_ts, 1));
            if let Some(ts) = seg.end_ts {
                composition_events.push((ts, -1));
            }
            let effective_end = seg
                .end_ts
                .unwrap_or_else(|| last_msg_ts.unwrap_or(seg.start_ts));
            tenures_days.push((effective_end - seg.start_ts).max(0) as f64 / 86_400.0);
        }

        let is_current = segments.last().is_some_and(|s| s.end_ts.is_none());
        if is_current {
            current_count += 1;
        }

        people.push(PersonTimelineRow {
            person_id: pid,
            display_name: display_names.remove(&pid).flatten(),
            segments,
            events,
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

    fn ev(ts: i64, kind: &str) -> (i64, String) {
        (ts, kind.to_string())
    }

    #[test]
    fn sequential_accounts_with_a_real_gap_produce_two_segments() {
        // Аккаунт A: вошёл в 100, вышел в 200. Аккаунт B (объединён
        // merge_people): вошёл в 500 — реальный промежуток без
        // присутствия между 200 и 500, разрыв должен остаться.
        let evs = vec![ev(100, "joined"), ev(200, "left"), ev(500, "joined")];
        let segments = build_coverage_segments(&evs, None);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].start_ts, 100);
        assert_eq!(segments[0].end_ts, Some(200));
        assert_eq!(segments[1].start_ts, 500);
        assert_eq!(segments[1].end_ts, None);
    }

    #[test]
    fn overlapping_merged_accounts_produce_one_continuous_segment() {
        // Аккаунт A: вошёл в 100, вышел в 300. Аккаунт B (тот же человек
        // после merge_people): вошёл в 200 — ещё до выхода A — значит в
        // момент 200-300 человек присутствовал под обоими аккаунтами.
        // Линия не должна рваться, хотя A формально "вышел" в 300.
        let evs = vec![
            ev(100, "joined"),
            ev(200, "joined"),
            ev(300, "left"),
            ev(400, "left"),
        ];
        let segments = build_coverage_segments(&evs, None);
        assert_eq!(
            segments.len(),
            1,
            "overlapping coverage from two merged accounts must not create a gap"
        );
        assert_eq!(segments[0].start_ts, 100);
        assert_eq!(segments[0].end_ts, Some(400));
    }

    #[test]
    fn since_start_fallback_when_first_message_predates_any_event() {
        let evs = vec![ev(200, "removed")];
        let segments = build_coverage_segments(&evs, Some(50));
        assert_eq!(segments.len(), 1);
        assert_eq!(
            segments[0].start_ts, 50,
            "must start from the first observed message, not the removal event"
        );
        assert_eq!(segments[0].end_ts, Some(200));
    }

    #[test]
    fn no_events_and_no_messages_never_panics() {
        assert_eq!(build_coverage_segments(&[], None).len(), 0);
    }
}
