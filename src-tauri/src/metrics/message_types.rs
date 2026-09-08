use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};

use super::precompute::{ensure_dashboard_ready, with_write_lock};

/// Разрез «тип сообщения» — одна раскладка на все счётные метрики (макет
/// «Дашборд», ход `t6`). 13 типов на этот срез, без реакций/полноценной
/// социальной метрики — docs/SPEC.md §9 уже разносит реакции в отдельную
/// будущую метрику со своей таблицей, здесь только то, что видно на одном
/// сообщении. `Text` не эксклюзивен с медиа-типами (фото с подписью — и
/// `Photo`, и `Text` одновременно) — предикат просто `text_len > 0`, та же
/// колонка, что уже пишется для каждого сообщения независимо от миграции
/// `0004`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageTypeKind {
    Text,
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
    pub const ALL: [MessageTypeKind; 13] = [
        MessageTypeKind::Text,
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
            MessageTypeKind::Text => "text_len > 0",
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

    /// Ключ строки в precomputed-таблицах (`message_type_*`) — не то же
    /// самое, что `serde`-представление (то отдаёт JSON-строку в кавычках,
    /// неудобно как значение TEXT-колонки), но то же самое написание.
    fn as_key(self) -> &'static str {
        match self {
            MessageTypeKind::Text => "text",
            MessageTypeKind::Photo => "photo",
            MessageTypeKind::Sticker => "sticker",
            MessageTypeKind::VoiceMessage => "voice_message",
            MessageTypeKind::VideoMessage => "video_message",
            MessageTypeKind::VideoFile => "video_file",
            MessageTypeKind::Animation => "animation",
            MessageTypeKind::File => "file",
            MessageTypeKind::Forwarded => "forwarded",
            MessageTypeKind::Edited => "edited",
            MessageTypeKind::Poll => "poll",
            MessageTypeKind::Location => "location",
            MessageTypeKind::Contact => "contact",
        }
    }
}

/// Гранулярность гистограммы этого блока — три уровня, не пять, как у
/// `granularity::Granularity` (граф активности), и намеренно с ним не
/// связана. `Month` — абсолютный календарный бакет (временной ряд, как у
/// графика активности). `DayOfWeek`/`HourOfDay` — распределения, а не
/// временные ряды: «в какой день недели»/«в какое время суток» обычно
/// присылают этот тип, агрегат по всему периоду сразу, не по конкретной
/// неделе/дате.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeTimeView {
    Month,
    DayOfWeek,
    HourOfDay,
}

impl TypeTimeView {
    /// Полное SQL-выражение для колонки-бакета, не просто формат
    /// `strftime` — `DayOfWeek` нужен с понедельника (0..6), а «сырой»
    /// `%w` у SQLite отдаёт с воскресенья, это арифметика поверх
    /// `strftime`, а не подстановка формата.
    fn bucket_select_expr(self) -> &'static str {
        match self {
            TypeTimeView::Month => "strftime('%Y-%m', ts, 'unixepoch')",
            TypeTimeView::DayOfWeek => {
                "CAST((CAST(strftime('%w', ts, 'unixepoch') AS INTEGER) + 6) % 7 AS TEXT)"
            }
            TypeTimeView::HourOfDay => "strftime('%H', ts, 'unixepoch')",
        }
    }

    /// Ключ `view` в `message_type_time_buckets` — то же написание, что и
    /// у `serde`-представления, но без обёртки в JSON-строку.
    fn as_key(self) -> &'static str {
        match self {
            TypeTimeView::Month => "month",
            TypeTimeView::DayOfWeek => "day_of_week",
            TypeTimeView::HourOfDay => "hour_of_day",
        }
    }
}

/// `rate_buckets` короткого чата (`RateGranularity::Day`) хранится под
/// этим ключом `view` — `TypeTimeView` не предлагает «по дням» как
/// самостоятельный вид гистограммы (только неделя/час), но темп/пик всё
/// равно нужно на что-то опереться, когда «по месяцам» недоступен.
const RATE_DAY_VIEW_KEY: &str = "rate_day";
const RATE_MONTH_VIEW_KEY: &str = "month";

/// Абсолютная гранулярность «темпа»/«пика» в карточке сводки — не путать
/// с `TypeTimeView` (что сейчас выбрано для гистограммы). Это всегда
/// календарный временной ряд (месяц или день), независимо от того, что
/// пользователь листает на графике: вопрос «какой в среднем темп» не
/// должен скакать при переключении вида гистограммы на день недели/час
/// суток, у которых усреднение по календарным бакетам не имеет смысла.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateGranularity {
    Month,
    Day,
}

impl RateGranularity {
    fn bucket_select_expr(self) -> &'static str {
        match self {
            RateGranularity::Month => "strftime('%Y-%m', ts, 'unixepoch')",
            RateGranularity::Day => "strftime('%Y-%m-%d', ts, 'unixepoch')",
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

/// Сводные счётчики по всем 13 типам — читает `message_type_counts`
/// (предвычислена, см. `recompute_message_type_metrics`), а не сканирует
/// `messages` заново на каждое открытие блока.
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

    ensure_message_type_precomputed(conn, chat_id)?;

    let mut stmt =
        conn.prepare("SELECT kind, count FROM message_type_counts WHERE chat_id = ?1")?;
    let by_key: HashMap<String, i64> = stmt
        .query_map(params![chat_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let counts = MessageTypeKind::ALL
        .iter()
        .map(|kind| MessageTypeCount {
            kind: *kind,
            count: by_key.get(kind.as_key()).copied().unwrap_or(0),
        })
        .collect();

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
        rate_granularity: RateGranularity,
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

/// Разбор одного типа: сводка + гистограмма + таблица участников. Читает
/// precomputed `message_type_*` (см. `recompute_message_type_metrics`),
/// а не сканирует `messages` заново на каждый клик по пилюле.
/// `None` — у типа нет ни одного сообщения (пилюля и не должна быть
/// кликабельной, но бэкенд не полагается на фронт в этом), либо чат ещё
/// не прошёл актуальный импортер (`message_types_ready = 0`) — как и
/// `get_message_type_overview`, а не частично доступные типы через
/// колонки, которые для такого чата всё равно не заполнены.
/// `view` — предпочтение фронта; если оно недоступно для этого чата
/// (например «по месяцам» на трёхдневном чате), тихо откатываемся на
/// разумный дефолт, а не ошибаемся.
pub fn get_message_type_detail(
    conn: &Connection,
    chat_id: i64,
    kind: MessageTypeKind,
    view: Option<TypeTimeView>,
) -> rusqlite::Result<Option<MessageTypeDetail>> {
    let chat_row: Option<(i64, i64, Option<i64>, Option<i64>)> = conn
        .query_row(
            "SELECT message_count, message_types_ready, first_msg_ts, last_msg_ts \
             FROM chats WHERE id = ?1",
            params![chat_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((total_messages, ready, first_ts, last_ts)) = chat_row else {
        return Ok(None);
    };
    if ready == 0 {
        return Ok(None);
    }

    ensure_message_type_precomputed(conn, chat_id)?;

    let key = kind.as_key();
    let total_count: i64 = conn
        .query_row(
            "SELECT count FROM message_type_counts WHERE chat_id = ?1 AND kind = ?2",
            params![chat_id, key],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0);
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
    views_available.push(TypeTimeView::DayOfWeek);
    views_available.push(TypeTimeView::HourOfDay);

    let default_view = if month_available {
        TypeTimeView::Month
    } else {
        TypeTimeView::DayOfWeek
    };
    let selected_view = view
        .filter(|v| views_available.contains(v))
        .unwrap_or(default_view);

    let rate_granularity = if month_available {
        RateGranularity::Month
    } else {
        RateGranularity::Day
    };
    // «Пик»/«рейт» считаются по rate_granularity независимо от того, что
    // сейчас выбрано для гистограммы — это разные вопросы: «какой темп в
    // среднем» и «что показать на графике прямо сейчас» (которое может
    // быть распределением по дню недели/часу суток, где «пик» в
    // календарном смысле бессмысленен). Когда «по месяцам» доступно, эти
    // бакеты совпадают с видом `Month` буквально (то же выражение
    // группировки) — переиспользуем те же строки, а не храним дважды.
    let rate_view_key = if month_available {
        RATE_MONTH_VIEW_KEY
    } else {
        RATE_DAY_VIEW_KEY
    };
    let rate_buckets: Vec<(String, i64)> = read_time_buckets(conn, chat_id, key, rate_view_key)?;
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

    let participants: Vec<MessageTypeParticipantStat> = {
        let mut stmt = conn.prepare(
            "SELECT p.id, p.display_name, mtps.count, pms.message_count, mtps.duration_seconds \
             FROM message_type_participant_stats mtps \
             JOIN people p ON p.id = mtps.person_id \
             JOIN person_message_stats pms \
               ON pms.chat_id = mtps.chat_id AND pms.person_id = mtps.person_id \
             WHERE mtps.chat_id = ?1 AND mtps.kind = ?2 \
               AND p.merged_into IS NULL AND p.is_hidden = 0 \
             ORDER BY mtps.count DESC",
        )?;
        let rows = stmt.query_map(params![chat_id, key], |row| {
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

    let summary = if kind.is_duration_based() {
        let (total_seconds, median_seconds, max_seconds) = conn.query_row(
            "SELECT total_seconds, median_seconds, max_seconds \
             FROM message_type_durations WHERE chat_id = ?1 AND kind = ?2",
            params![chat_id, key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        MessageTypeSummaryStats::Duration {
            total_seconds,
            median_seconds,
            max_seconds,
        }
    } else {
        // Топ-отправитель — первый в уже отсортированном по count DESC
        // списке участников, отдельный запрос не нужен.
        let top_sender = participants.first().and_then(|p| p.display_name.clone());
        MessageTypeSummaryStats::Frequency {
            rate_granularity,
            rate_value,
            peak,
            top_sender,
        }
    };

    let buckets: Vec<TypeBucket> = read_time_buckets(conn, chat_id, key, selected_view.as_key())?
        .into_iter()
        .map(|(bucket, count)| TypeBucket { bucket, count })
        .collect();

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

fn read_time_buckets(
    conn: &Connection,
    chat_id: i64,
    kind_key: &str,
    view_key: &str,
) -> rusqlite::Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT bucket, count FROM message_type_time_buckets \
         WHERE chat_id = ?1 AND kind = ?2 AND view = ?3 ORDER BY bucket",
    )?;
    let rows = stmt.query_map(params![chat_id, kind_key, view_key], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;
    rows.collect()
}

/// Ленивый self-heal: чат мог пройти актуальный импортер
/// (`message_types_ready = 1`) до появления этого среза, когда
/// `message_type_*`-таблицы ещё не существовали — досчитываем один раз
/// при первом чтении, а не требуем повторный импорт (в отличие от самого
/// `message_types_ready`, который отличает уже реальное «данных нет»,
/// это здесь просто «ещё не закэшировано»). `message_type_counts` всегда
/// получает по одной строке на каждый из 13 типов при пересчёте (даже с
/// count=0) — поэтому отсутствие любых строк однозначно значит «не
/// считано», а не «всё нулевое».
fn ensure_message_type_precomputed(conn: &Connection, chat_id: i64) -> rusqlite::Result<()> {
    let ready: Option<i64> = conn
        .query_row(
            "SELECT message_types_ready FROM chats WHERE id = ?1",
            params![chat_id],
            |row| row.get(0),
        )
        .optional()?;
    if ready != Some(1) {
        return Ok(());
    }
    let has_rows: i64 = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM message_type_counts WHERE chat_id = ?1)",
        params![chat_id],
        |row| row.get(0),
    )?;
    if has_rows == 0 {
        recompute_message_type_metrics(conn, chat_id)?;
    }
    Ok(())
}

/// Пересчитывает `message_type_counts`/`message_type_time_buckets`/
/// `message_type_durations`/`message_type_participant_stats` с нуля для
/// `chat_id` — вызывается в конце импорта (та же транзакция, что и
/// `recompute_dashboard_metrics`) и лениво из `ensure_message_type_precomputed`.
/// Требует, чтобы `person_message_stats` (`metrics::precompute`) уже была
/// посчитана — `get_message_type_detail` резолвит через неё «сколько
/// всего сообщений у этого участника», поэтому досчитываем и её тоже,
/// если чат ещё не проходил через `ensure_dashboard_ready`. Безопасна к
/// конкурентным вызовам для одного `chat_id` — см.
/// `precompute::with_write_lock`.
pub fn recompute_message_type_metrics(conn: &Connection, chat_id: i64) -> rusqlite::Result<()> {
    ensure_dashboard_ready(conn, chat_id)?;

    with_write_lock(conn, || {
        conn.execute(
            "DELETE FROM message_type_counts WHERE chat_id = ?1",
            params![chat_id],
        )?;
        conn.execute(
            "DELETE FROM message_type_time_buckets WHERE chat_id = ?1",
            params![chat_id],
        )?;
        conn.execute(
            "DELETE FROM message_type_durations WHERE chat_id = ?1",
            params![chat_id],
        )?;

        let bounds: Option<(Option<i64>, Option<i64>)> = conn
            .query_row(
                "SELECT first_msg_ts, last_msg_ts FROM chats WHERE id = ?1",
                params![chat_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let span_days = match bounds {
            Some((Some(a), Some(b))) => (b - a).max(0) / 86_400,
            _ => 0,
        };
        let month_available = span_days >= MONTH_VIEW_MIN_SPAN_DAYS;

        for kind in MessageTypeKind::ALL {
            let predicate = kind.predicate_sql();
            let key = kind.as_key();

            let count: i64 = conn.query_row(
                &format!("SELECT COUNT(*) FROM messages WHERE chat_id = ?1 AND {predicate}"),
                params![chat_id],
                |row| row.get(0),
            )?;
            conn.execute(
                "INSERT INTO message_type_counts (chat_id, kind, count) VALUES (?1, ?2, ?3)",
                params![chat_id, key, count],
            )?;
            if count == 0 {
                continue;
            }

            let mut views: Vec<(&str, &str)> = vec![
                (
                    TypeTimeView::DayOfWeek.as_key(),
                    TypeTimeView::DayOfWeek.bucket_select_expr(),
                ),
                (
                    TypeTimeView::HourOfDay.as_key(),
                    TypeTimeView::HourOfDay.bucket_select_expr(),
                ),
            ];
            if month_available {
                views.push((
                    RATE_MONTH_VIEW_KEY,
                    TypeTimeView::Month.bucket_select_expr(),
                ));
            } else {
                views.push((RATE_DAY_VIEW_KEY, RateGranularity::Day.bucket_select_expr()));
            }
            for (view_key, expr) in views {
                let sql = format!(
                    "INSERT INTO message_type_time_buckets (chat_id, kind, view, bucket, count) \
                     SELECT ?1, ?2, ?3, {expr}, COUNT(*) FROM messages \
                     WHERE chat_id = ?1 AND ts IS NOT NULL AND {predicate} GROUP BY {expr}"
                );
                conn.execute(&sql, params![chat_id, key, view_key])?;
            }

            if kind.is_duration_based() {
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
                conn.execute(
                    "INSERT INTO message_type_durations \
                     (chat_id, kind, total_seconds, median_seconds, max_seconds) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![chat_id, key, total_seconds, median(&durations), max_seconds],
                )?;
            }
        }

        recompute_message_type_participant_stats(conn, chat_id)?;

        Ok(())
    })
}

/// Только `message_type_participant_stats` — вызывается после
/// `recompute_message_type_metrics` при пересчёте с нуля, а также сама по
/// себе из `merge_people`: слияние физически переносит
/// `messages.person_id` на `primary_id`, из-за чего эта (единственная
/// person-keyed среди `message_type_*`) таблица перестаёт соответствовать
/// `messages`, а счётчики/гистограммы по типам (person-agnostic) — нет.
/// Безопасна к конкурентным вызовам для одного `chat_id` — см.
/// `precompute::with_write_lock`.
pub fn recompute_message_type_participant_stats(
    conn: &Connection,
    chat_id: i64,
) -> rusqlite::Result<()> {
    with_write_lock(conn, || {
        conn.execute(
            "DELETE FROM message_type_participant_stats WHERE chat_id = ?1",
            params![chat_id],
        )?;
        for kind in MessageTypeKind::ALL {
            let predicate = kind.predicate_sql();
            let key = kind.as_key();
            let sql = format!(
                "INSERT INTO message_type_participant_stats \
                 (chat_id, kind, person_id, count, duration_seconds) \
                 SELECT ?1, ?2, person_id, \
                        SUM(CASE WHEN {predicate} THEN 1 ELSE 0 END), \
                        SUM(CASE WHEN {predicate} THEN COALESCE(duration_seconds, 0) ELSE 0 END) \
                 FROM messages WHERE chat_id = ?1 AND person_id IS NOT NULL \
                 GROUP BY person_id \
                 HAVING SUM(CASE WHEN {predicate} THEN 1 ELSE 0 END) > 0"
            );
            conn.execute(&sql, params![chat_id, key])?;
        }
        Ok(())
    })
}
