use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};

use super::precompute::with_write_lock;

const TOP_REACTED_MESSAGES_LIMIT: i64 = 10;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReactionEmojiCount {
    pub kind: String,
    pub emoji: Option<String>,
    pub document_id: Option<String>,
    pub count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReactionEmojiRef {
    pub kind: String,
    pub emoji: Option<String>,
    pub document_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReactionExchangeRow {
    pub person_id: i64,
    pub display_name: Option<String>,
    pub given_count: i64,
    pub received_count: i64,
    /// Самый частый тип реакции, который ставит этот участник (миграция
    /// `0010`) — `None`, если у него нет ни одной типизированной строки в
    /// `reaction_actors` (либо не ставил вовсе, либо все его реакции
    /// посчитаны до миграции `0009`, см. `PersonReactionBreakdown`).
    pub favorite_given: Option<ReactionEmojiRef>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReactionsSummary {
    pub total_reactions: i64,
    pub reacted_message_count: i64,
    pub total_messages: i64,
    pub top_emoji: Vec<ReactionEmojiCount>,
    pub exchange: Vec<ReactionExchangeRow>,
    /// `true` — хотя бы у одной реакции в чате `count` больше числа
    /// известных акторов: Telegram отдаёт в `recent` не полный список
    /// реагировавших (docs/SPEC.md §8.5). "Получает" в таблице обмена
    /// остаётся точным в любом случае — считается по авторитетному
    /// `count`, а не по `recent`; страдает только "ставит".
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

/// Все участники с ненулевой активностью для таблицы «Обмен реакциями» —
/// читает `reaction_person_stats` целиком за один проход (а не двумя
/// отдельными топами по каждой стороне, как раньше), подтягивает и
/// `top_given_*` (миграция `0010`) для колонки «Чаще ставит». Без LIMIT —
/// список отсортирован по убыванию активности, но урезать его самой
/// метрике не с руки (сколько показывать — дело фронта, если вообще
/// понадобится).
fn get_reaction_exchange(
    conn: &Connection,
    chat_id: i64,
) -> rusqlite::Result<Vec<ReactionExchangeRow>> {
    let mut stmt = conn.prepare(
        "SELECT rps.person_id, p.display_name, rps.given_count, rps.received_count, \
                rps.top_given_kind, rps.top_given_emoji, rps.top_given_document_id \
         FROM reaction_person_stats rps \
         JOIN people p ON p.id = rps.person_id \
         WHERE rps.chat_id = ?1 AND (rps.given_count > 0 OR rps.received_count > 0) \
           AND p.merged_into IS NULL AND p.is_hidden = 0 \
         ORDER BY (rps.given_count + rps.received_count) DESC",
    )?;
    let rows = stmt.query_map(params![chat_id], |row| {
        let top_given_kind: Option<String> = row.get(4)?;
        let top_given_emoji: Option<String> = row.get(5)?;
        let top_given_document_id: Option<String> = row.get(6)?;
        let favorite_given = top_given_kind.map(|kind| ReactionEmojiRef {
            kind,
            emoji: top_given_emoji,
            document_id: top_given_document_id,
        });
        Ok(ReactionExchangeRow {
            person_id: row.get(0)?,
            display_name: row.get(1)?,
            given_count: row.get(2)?,
            received_count: row.get(3)?,
            favorite_given,
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

    let exchange = get_reaction_exchange(conn, chat_id)?;

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
        exchange,
        given_data_incomplete,
    }))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TopReactedMessage {
    pub message_id: i64,
    /// Telegram-id сообщения (`messages.tg_msg_id`), не внутренний rowid
    /// — только он годится для ссылки `t.me/c/<tg_chat_id>/<tg_msg_id>`,
    /// строит её фронт (знает `tg_chat_id`/`chat_type` текущего чата).
    pub tg_msg_id: i64,
    pub person_id: Option<i64>,
    pub display_name: Option<String>,
    pub ts: Option<i64>,
    pub preview_text: Option<String>,
    /// Человекочитаемое описание типа сообщения ("Фото", "Голосовое", …) —
    /// заполнено, когда есть распознанный медиа-тип, вне зависимости от
    /// `preview_text` (у фото с подписью оба поля непустые, фронт сам
    /// решает, что показать). `None` для чистого текста без медиа.
    pub content_label: Option<String>,
    /// Разбивка по типам реакций на этом сообщении, по убыванию `count`
    /// (макет «Дашборд», ход t8/8a — карточка «Самые залайканные»
    /// показывает не только суммарное число, но и какие именно эмодзи).
    pub reactions: Vec<ReactionEmojiCount>,
    pub total_count: i64,
}

/// Метки те же 13 типов, что и в `message_types.rs` (`MessageTypeKind`),
/// но здесь — не SQL-предикат, а разбор уже прочитанных из БД колонок
/// одного сообщения в подпись для карточки. Дублирует часть логики
/// `MessageTypeKind::predicate_sql` в другой форме (row → label, не
/// kind → SQL) — не переиспользует её напрямую, чтобы не тащить
/// зависимость message_types.rs → reactions.rs ради одной строчки.
fn media_content_label(
    media_type: Option<&str>,
    has_photo: bool,
    has_file: bool,
    has_poll: bool,
    has_location: bool,
    has_contact: bool,
) -> Option<&'static str> {
    if has_photo {
        return Some("Фото");
    }
    match media_type {
        Some("sticker") => return Some("Стикер"),
        Some("voice_message") => return Some("Голосовое сообщение"),
        Some("video_message") => return Some("Видеосообщение"),
        Some("video_file") => return Some("Видео"),
        Some("animation") => return Some("Гифка"),
        _ => {}
    }
    if has_file {
        return Some("Файл");
    }
    if has_poll {
        return Some("Опрос");
    }
    if has_location {
        return Some("Локация");
    }
    if has_contact {
        return Some("Контакт");
    }
    None
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
        "SELECT mr.message_id, m.tg_msg_id, m.person_id, p.display_name, m.ts, mrp.preview_text, \
                SUM(mr.count), m.media_type, m.has_photo, m.has_file, m.has_poll, m.has_location, \
                m.has_contact \
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
    let mut messages: Vec<TopReactedMessage> = stmt
        .query_map(params![chat_id, TOP_REACTED_MESSAGES_LIMIT], |row| {
            let media_type: Option<String> = row.get(7)?;
            let content_label = media_content_label(
                media_type.as_deref(),
                row.get::<_, i64>(8)? != 0,
                row.get::<_, i64>(9)? != 0,
                row.get::<_, i64>(10)? != 0,
                row.get::<_, i64>(11)? != 0,
                row.get::<_, i64>(12)? != 0,
            )
            .map(str::to_string);
            Ok(TopReactedMessage {
                message_id: row.get(0)?,
                tg_msg_id: row.get(1)?,
                person_id: row.get(2)?,
                display_name: row.get(3)?,
                ts: row.get(4)?,
                preview_text: row.get(5)?,
                content_label,
                reactions: Vec::new(),
                total_count: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Разбивка по эмодзи для этих же top-N сообщений — тот же ранжирующий
    // подзапрос (LIMIT ?2), чтобы не тащить динамический список id через
    // отдельный `IN (...)`. `message_reactions` уже ограничен только
    // реагировавшими сообщениями — второй проход по нему дешёвый.
    let mut per_message: HashMap<i64, Vec<ReactionEmojiCount>> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT mr.message_id, mr.kind, mr.emoji, mr.document_id, mr.count \
             FROM message_reactions mr \
             WHERE mr.chat_id = ?1 AND mr.message_id IN ( \
               SELECT message_id FROM message_reactions WHERE chat_id = ?1 \
               GROUP BY message_id ORDER BY SUM(count) DESC LIMIT ?2 \
             ) \
             ORDER BY mr.message_id, mr.count DESC",
        )?;
        let rows = stmt.query_map(params![chat_id, TOP_REACTED_MESSAGES_LIMIT], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                ReactionEmojiCount {
                    kind: row.get(1)?,
                    emoji: row.get(2)?,
                    document_id: row.get(3)?,
                    count: row.get(4)?,
                },
            ))
        })?;
        for r in rows {
            let (message_id, emoji) = r?;
            per_message.entry(message_id).or_default().push(emoji);
        }
    }
    for m in &mut messages {
        if let Some(list) = per_message.remove(&m.message_id) {
            m.reactions = list;
        }
    }

    Ok(messages)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PersonReactionBreakdown {
    pub given: Vec<ReactionEmojiCount>,
    /// Из `reaction_person_stats.given_count` — сумма `given` совпадает с
    /// этим числом только для чата, целиком переимпортированного после
    /// миграции `0009`. Если `given_total > 0`, а `given` пуст — часть
    /// или все реакции этого участника были посчитаны до появления
    /// разбивки по эмодзи, и фронт должен предложить переимпорт, а не
    /// молча показать пустой список.
    pub given_total: i64,
    pub received: Vec<ReactionEmojiCount>,
    pub received_total: i64,
}

/// Разбивка по типам реакций для одного участника: какие эмодзи он сам
/// ставит (`given`, из `reaction_actors.kind/emoji/document_id` — миграция
/// `0009`) и какие эмодзи собирают его сообщения (`received`, из
/// `message_reactions`, точно, тем же приёмом, что и в
/// `get_reactions_overview`). Живой запрос, не precompute — обе исходные
/// таблицы уже ограничены только реакциями, а не всеми `messages`, и
/// фильтр по `person_id` использует `idx_reaction_actors_person_kind`/
/// `idx_reaction_actors_person`.
///
/// `given` не включает строки `reaction_actors`, вставленные до миграции
/// `0009` (`kind IS NULL`) — у них нет типа реакции и агрегировать их с
/// типизированными в одну "неизвестную" категорию значило бы смешивать
/// разнородные эмодзи под одной строкой. Пустой `given` при ненулевом
/// `reaction_person_stats.given_count` означает именно это — чат нужно
/// переимпортировать, чтобы увидеть разбивку (общий счётчик при этом уже
/// корректен и без переимпорта).
pub fn get_reaction_person_breakdown(
    conn: &Connection,
    chat_id: i64,
    person_id: i64,
) -> rusqlite::Result<PersonReactionBreakdown> {
    let mut given_stmt = conn.prepare(
        "SELECT kind, emoji, document_id, COUNT(*) FROM reaction_actors \
         WHERE chat_id = ?1 AND person_id = ?2 AND kind IS NOT NULL \
         GROUP BY kind, emoji, document_id ORDER BY COUNT(*) DESC",
    )?;
    let given = given_stmt
        .query_map(params![chat_id, person_id], |row| {
            Ok(ReactionEmojiCount {
                kind: row.get(0)?,
                emoji: row.get(1)?,
                document_id: row.get(2)?,
                count: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut received_stmt = conn.prepare(
        "SELECT mr.kind, mr.emoji, mr.document_id, SUM(mr.count) FROM message_reactions mr \
         JOIN messages m ON m.id = mr.message_id \
         WHERE mr.chat_id = ?1 AND m.person_id = ?2 \
         GROUP BY mr.kind, mr.emoji, mr.document_id ORDER BY SUM(mr.count) DESC",
    )?;
    let received = received_stmt
        .query_map(params![chat_id, person_id], |row| {
            Ok(ReactionEmojiCount {
                kind: row.get(0)?,
                emoji: row.get(1)?,
                document_id: row.get(2)?,
                count: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let (given_total, received_total): (i64, i64) = conn
        .query_row(
            "SELECT given_count, received_count FROM reaction_person_stats \
             WHERE chat_id = ?1 AND person_id = ?2",
            params![chat_id, person_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .unwrap_or((0, 0));

    Ok(PersonReactionBreakdown {
        given,
        given_total,
        received,
        received_total,
    })
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

        // Самая частая реакция, которую человек ставит (миграция `0010`,
        // колонка "Чаще ставит" в таблице обмена) — только по строкам с
        // известным типом (`kind IS NOT NULL`, т.е. вставленным после
        // миграции `0009`), тем же приёмом, что и `given_data_incomplete`/
        // `PersonReactionBreakdown` не путает старые NULL-строки с
        // отдельной "неизвестной" категорией.
        type EmojiKey = (String, Option<String>, Option<String>);
        let mut favorite_given: HashMap<i64, HashMap<EmojiKey, i64>> = HashMap::new();
        {
            let mut stmt = conn.prepare(
                "SELECT person_id, kind, emoji, document_id, COUNT(*) FROM reaction_actors \
                 WHERE chat_id = ?1 AND person_id IS NOT NULL AND kind IS NOT NULL \
                 GROUP BY person_id, kind, emoji, document_id",
            )?;
            let rows = stmt.query_map(params![chat_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?;
            for r in rows {
                let (pid, kind, emoji, document_id, count) = r?;
                favorite_given
                    .entry(pid)
                    .or_default()
                    .insert((kind, emoji, document_id), count);
            }
        }
        // Максимум по count, тай-брейк — по (kind, emoji) по алфавиту, для
        // детерминированного результата при равных счётчиках.
        let favorite_given: HashMap<i64, EmojiKey> = favorite_given
            .into_iter()
            .filter_map(|(pid, counts)| {
                counts
                    .into_iter()
                    .max_by(|(ka, ca), (kb, cb)| ca.cmp(cb).then_with(|| kb.cmp(ka)))
                    .map(|(key, _)| (pid, key))
            })
            .collect();

        let mut all_ids: HashSet<i64> = given.keys().copied().collect();
        all_ids.extend(received.keys().copied());

        let mut insert = conn.prepare(
            "INSERT INTO reaction_person_stats \
             (chat_id, person_id, given_count, received_count, \
              top_given_kind, top_given_emoji, top_given_document_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for pid in all_ids {
            let (top_kind, top_emoji, top_doc) = match favorite_given.get(&pid) {
                Some((kind, emoji, doc)) => (Some(kind.as_str()), emoji.as_deref(), doc.as_deref()),
                None => (None, None, None),
            };
            insert.execute(params![
                chat_id,
                pid,
                given.get(&pid).copied().unwrap_or(0),
                received.get(&pid).copied().unwrap_or(0),
                top_kind,
                top_emoji,
                top_doc,
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
            exchange: vec![],
            given_data_incomplete: false,
        });
        let json = serde_json::to_string(&overview).unwrap();
        eprintln!("{json}");
        assert!(json.contains("\"status\":\"available\""));
        assert!(json.contains("\"total_reactions\":5"));
    }
}
