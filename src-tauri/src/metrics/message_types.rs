use rusqlite::{params, Connection, OptionalExtension};

/// Разрез «тип сообщения» — одна раскладка на все счётные метрики (макет
/// «Дашборд», ход `t6`). 12 типов на этот срез, без реакций/полноценной
/// социальной метрики — docs/SPEC.md §9 уже разносит реакции в отдельную
/// будущую метрику со своей таблицей, здесь только то, что видно на одном
/// сообщении.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageTypeKind {
    Photo,
    Sticker,
    VoiceMessage,
    VideoMessage,
    VideoFile,
    Animation,
    File,
    Forwarded,
    Edited,
    Poll,
    Location,
    Contact,
}

impl MessageTypeKind {
    pub const ALL: [MessageTypeKind; 12] = [
        MessageTypeKind::Photo,
        MessageTypeKind::Sticker,
        MessageTypeKind::VoiceMessage,
        MessageTypeKind::VideoMessage,
        MessageTypeKind::VideoFile,
        MessageTypeKind::Animation,
        MessageTypeKind::File,
        MessageTypeKind::Forwarded,
        MessageTypeKind::Edited,
        MessageTypeKind::Poll,
        MessageTypeKind::Location,
        MessageTypeKind::Contact,
    ];

    /// SQL-предикат над колонками `messages` (голые имена — вызывающая
    /// сторона может использовать его что с алиасом таблицы, что без,
    /// коллизий с колонками `people` нет). Единственное место в коде,
    /// которое знает, какая колонка отвечает за какой тип.
    fn predicate_sql(self) -> &'static str {
        match self {
            MessageTypeKind::Photo => "has_photo = 1",
            MessageTypeKind::Sticker => "media_type = 'sticker'",
            MessageTypeKind::VoiceMessage => "media_type = 'voice_message'",
            MessageTypeKind::VideoMessage => "media_type = 'video_message'",
            MessageTypeKind::VideoFile => "media_type = 'video_file'",
            MessageTypeKind::Animation => "media_type = 'animation'",
            MessageTypeKind::File => "has_file = 1",
            MessageTypeKind::Forwarded => "forwarded_from IS NOT NULL",
            MessageTypeKind::Edited => "edited_ts IS NOT NULL",
            MessageTypeKind::Poll => "has_poll = 1",
            MessageTypeKind::Location => "has_location = 1",
            MessageTypeKind::Contact => "has_contact = 1",
        }
    }

    /// Голосовые/кружочки несут длительность — своя раскладка сводки
    /// (макет 6b): суммарно/медиана/самое длинное вместо частота/пик/лидер.
    fn is_duration_based(self) -> bool {
        matches!(
            self,
            MessageTypeKind::VoiceMessage | MessageTypeKind::VideoMessage
        )
    }
}

/// Гранулярность именно этого блока — три уровня, не пять, как у
/// `granularity::Granularity` (граф активности), и намеренно с ним не
/// связана. `Month`/`Day` — абсолютные календарные бакеты (временной
/// ряд, как у графика активности), `HourOfDay` — агрегат по часу суток
/// (`strftime('%H', ...)`, 24 бакета) — это распределение «в какое время
/// суток», а не временной ряд, принципиально другая ось.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeTimeView {
    Month,
    Day,
    HourOfDay,
}

impl TypeTimeView {
    fn bucket_expr(self) -> &'static str {
        match self {
            TypeTimeView::Month => "%Y-%m",
            TypeTimeView::Day => "%Y-%m-%d",
            TypeTimeView::HourOfDay => "%H",
        }
    }
}

/// Ниже этого числа дней разбивка по месяцам вырождается в единицы точек
/// — не зашито на «годы», это порог того же порядка, что и NINETY_DAYS в
/// granularity.rs, подобранный по той же логике (docs/SPEC.md §7).
const MONTH_VIEW_MIN_SPAN_DAYS: i64 = 60;

/// Минимум различных бакетов, чтобы «пик» вообще что-то значил — на
/// двух точках любая из них тривиально «пиковая» (docs/SPEC.md §7:
/// честная пометка «мало данных», а не бессмысленное число).
const MIN_BUCKETS_FOR_PEAK: usize = 3;

#[derive(Debug, Clone, serde::Serialize)]
pub struct TypeBucket {
    pub bucket: String,
    pub count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MessageTypeCount {
    pub kind: MessageTypeKind,
    pub count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MessageTypeOverview {
    /// `false` — чат импортирован до появления этой метрики: колонки уже
    /// есть (ALTER TABLE добавил их всем строкам), но не заполнены, а
    /// отличить «медиа не было» от «ещё не посчитано» по одним только
    /// новым колонкам нельзя. Фронт должен показать «Обновить выгрузку»,
    /// а не частоты с лживыми нулями.
    pub available: bool,
    pub total_messages: i64,
    pub counts: Vec<MessageTypeCount>,
}

/// Сводные счётчики по всем 12 типам — один проход по `messages`, чтобы
/// нарисовать ряд пилюль со счётчиками сразу при открытии блока.
pub fn get_message_type_overview(
    conn: &Connection,
    chat_id: i64,
) -> rusqlite::Result<MessageTypeOverview> {
    let row: Option<(i64, i64)> = conn
        .query_row(
            "SELECT message_count, message_types_ready FROM chats WHERE id = ?1",
            params![chat_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((total_messages, ready)) = row else {
        return Ok(MessageTypeOverview {
            available: false,
            total_messages: 0,
            counts: vec![],
        });
    };
    if ready == 0 {
        return Ok(MessageTypeOverview {
            available: false,
            total_messages,
            counts: vec![],
        });
    }

    let case_columns: Vec<String> = MessageTypeKind::ALL
        .iter()
        .map(|k| format!("SUM(CASE WHEN {} THEN 1 ELSE 0 END)", k.predicate_sql()))
        .collect();
    let sql = format!(
        "SELECT {} FROM messages WHERE chat_id = ?1",
        case_columns.join(", ")
    );
    let counts = conn.query_row(&sql, params![chat_id], |row| {
        let mut counts = Vec::with_capacity(MessageTypeKind::ALL.len());
        for (i, kind) in MessageTypeKind::ALL.iter().enumerate() {
            let count: Option<i64> = row.get(i)?;
            counts.push(MessageTypeCount {
                kind: *kind,
                count: count.unwrap_or(0),
            });
        }
        Ok(counts)
    })?;

    Ok(MessageTypeOverview {
        available: true,
        total_messages,
        counts,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessageTypeSummaryStats {
    Frequency {
        rate_granularity: TypeTimeView,
        rate_value: f64,
        peak: Option<TypeBucket>,
        top_sender: Option<String>,
    },
    Duration {
        total_seconds: i64,
        median_seconds: f64,
        max_seconds: i64,
    },
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum MessageTypeParticipantExtra {
    PerThousand(f64),
    TotalMinutes(f64),
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MessageTypeParticipantStat {
    pub person_id: i64,
    pub display_name: Option<String>,
    pub count: i64,
    pub share_of_own_pct: f64,
    pub extra: MessageTypeParticipantExtra,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MessageTypeDetail {
    pub kind: MessageTypeKind,
    pub total_count: i64,
    pub pct_of_all: f64,
    pub summary: MessageTypeSummaryStats,
    pub views_available: Vec<TypeTimeView>,
    pub selected_view: TypeTimeView,
    pub buckets: Vec<TypeBucket>,
    pub participants: Vec<MessageTypeParticipantStat>,
}

fn median(sorted_ascending: &[i64]) -> f64 {
    let n = sorted_ascending.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        sorted_ascending[n / 2] as f64
    } else {
        (sorted_ascending[n / 2 - 1] as f64 + sorted_ascending[n / 2] as f64) / 2.0
    }
}

/// Разбор одного типа: сводка + гистограмма + таблица участников.
/// `None` — у типа нет ни одного сообщения (пилюля и не должна быть
/// кликабельной, но бэкенд не полагается на фронт в этом).
/// `view` — предпочтение фронта; если оно недоступно для этого чата
/// (например «по месяцам» на трёхдневном чате), тихо откатываемся на
/// разумный дефолт, а не ошибаемся.
pub fn get_message_type_detail(
    conn: &Connection,
    chat_id: i64,
    kind: MessageTypeKind,
    view: Option<TypeTimeView>,
) -> rusqlite::Result<Option<MessageTypeDetail>> {
    let predicate = kind.predicate_sql();

    let chat_row: Option<(i64, Option<i64>, Option<i64>)> = conn
        .query_row(
            "SELECT message_count, first_msg_ts, last_msg_ts FROM chats WHERE id = ?1",
            params![chat_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((total_messages, first_ts, last_ts)) = chat_row else {
        return Ok(None);
    };

    let total_count: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM messages WHERE chat_id = ?1 AND {predicate}"),
        params![chat_id],
        |row| row.get(0),
    )?;
    if total_count == 0 {
        return Ok(None);
    }
    let pct_of_all = if total_messages > 0 {
        total_count as f64 / total_messages as f64 * 100.0
    } else {
        0.0
    };

    let span_days = match (first_ts, last_ts) {
        (Some(a), Some(b)) => (b - a).max(0) / 86_400,
        _ => 0,
    };
    let month_available = span_days >= MONTH_VIEW_MIN_SPAN_DAYS;
    let mut views_available = Vec::with_capacity(3);
    if month_available {
        views_available.push(TypeTimeView::Month);
    }
    views_available.push(TypeTimeView::Day);
    views_available.push(TypeTimeView::HourOfDay);

    let rate_granularity = if month_available {
        TypeTimeView::Month
    } else {
        TypeTimeView::Day
    };
    let selected_view = view
        .filter(|v| views_available.contains(v))
        .unwrap_or(rate_granularity);

    // «Пик»/«рейт» считаются по rate_granularity независимо от того, что
    // сейчас выбрано для гистограммы — это разные вопросы: «какой темп в
    // среднем» и «что показать на графике прямо сейчас».
    let rate_buckets: Vec<(String, i64)> = {
        let expr = rate_granularity.bucket_expr();
        let sql = format!(
            "SELECT strftime('{expr}', ts, 'unixepoch') b, COUNT(*) FROM messages \
             WHERE chat_id = ?1 AND ts IS NOT NULL AND {predicate} GROUP BY b ORDER BY b"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![chat_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let peak = if rate_buckets.len() >= MIN_BUCKETS_FOR_PEAK {
        rate_buckets
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(bucket, count)| TypeBucket {
                bucket: bucket.clone(),
                count: *count,
            })
    } else {
        None
    };
    let rate_value = if rate_buckets.is_empty() {
        0.0
    } else {
        total_count as f64 / rate_buckets.len() as f64
    };

    let summary = if kind.is_duration_based() {
        let durations: Vec<i64> = {
            let sql = format!(
                "SELECT duration_seconds FROM messages WHERE chat_id = ?1 AND {predicate} \
                 AND duration_seconds IS NOT NULL ORDER BY duration_seconds"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![chat_id], |row| row.get(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let total_seconds: i64 = durations.iter().sum();
        let max_seconds = durations.iter().copied().max().unwrap_or(0);
        MessageTypeSummaryStats::Duration {
            total_seconds,
            median_seconds: median(&durations),
            max_seconds,
        }
    } else {
        let top_sender: Option<String> = {
            let sql = format!(
                "SELECT p.display_name FROM messages m JOIN people p ON p.id = m.person_id \
                 WHERE m.chat_id = ?1 AND p.merged_into IS NULL AND p.is_hidden = 0 AND {predicate} \
                 GROUP BY p.id ORDER BY COUNT(*) DESC LIMIT 1"
            );
            conn.query_row(&sql, params![chat_id], |row| row.get(0))
                .optional()?
                .flatten()
        };
        MessageTypeSummaryStats::Frequency {
            rate_granularity,
            rate_value,
            peak,
            top_sender,
        }
    };

    let buckets: Vec<TypeBucket> = if selected_view == rate_granularity {
        rate_buckets
            .into_iter()
            .map(|(bucket, count)| TypeBucket { bucket, count })
            .collect()
    } else {
        let expr = selected_view.bucket_expr();
        let sql = format!(
            "SELECT strftime('{expr}', ts, 'unixepoch') b, COUNT(*) FROM messages \
             WHERE chat_id = ?1 AND ts IS NOT NULL AND {predicate} GROUP BY b ORDER BY b"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![chat_id], |row| {
            Ok(TypeBucket {
                bucket: row.get(0)?,
                count: row.get(1)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let participants: Vec<MessageTypeParticipantStat> = {
        let sql = format!(
            "SELECT p.id, p.display_name, \
                    SUM(CASE WHEN {predicate} THEN 1 ELSE 0 END) cnt, \
                    COUNT(*) total_own, \
                    SUM(CASE WHEN {predicate} THEN COALESCE(duration_seconds, 0) ELSE 0 END) dur \
             FROM messages m JOIN people p ON p.id = m.person_id \
             WHERE m.chat_id = ?1 AND p.merged_into IS NULL AND p.is_hidden = 0 \
             GROUP BY p.id ORDER BY cnt DESC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![chat_id], |row| {
            let person_id: i64 = row.get(0)?;
            let display_name: Option<String> = row.get(1)?;
            let cnt: i64 = row.get(2)?;
            let total_own: i64 = row.get(3)?;
            let dur: i64 = row.get(4)?;
            Ok((person_id, display_name, cnt, total_own, dur))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .map(|(person_id, display_name, cnt, total_own, dur)| {
                let share_of_own_pct = if total_own > 0 {
                    cnt as f64 / total_own as f64 * 100.0
                } else {
                    0.0
                };
                let extra = if kind.is_duration_based() {
                    MessageTypeParticipantExtra::TotalMinutes(dur as f64 / 60.0)
                } else {
                    let per_thousand = if total_own > 0 {
                        cnt as f64 * 1000.0 / total_own as f64
                    } else {
                        0.0
                    };
                    MessageTypeParticipantExtra::PerThousand(per_thousand)
                };
                MessageTypeParticipantStat {
                    person_id,
                    display_name,
                    count: cnt,
                    share_of_own_pct,
                    extra,
                }
            })
            .collect()
    };

    Ok(Some(MessageTypeDetail {
        kind,
        total_count,
        pct_of_all,
        summary,
        views_available,
        selected_view,
        buckets,
        participants,
    }))
}
