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

/// Текст сообщения не хранится в `messages` (только `text_len`/`word_count`)
/// — превью для "самых залайканных сообщений" снимается один раз здесь,
/// пока текст ещё есть в памяти парсера, и обрезается: это витрина, не
/// хранилище полного текста для каждого сообщения подряд.
const REACTION_PREVIEW_MAX_CHARS: usize = 200;

fn truncate_preview(text: &str) -> String {
    if text.chars().count() <= REACTION_PREVIEW_MAX_CHARS {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(REACTION_PREVIEW_MAX_CHARS).collect();
        s.push('…');
        s
    }
}

/// person_id → отображаемое имя → (первое, последнее) время встречи.
type AliasSeen = HashMap<i64, HashMap<String, (Option<i64>, Option<i64>)>>;

/// Сервисные события, которым таймлайн даёт значение (docs/PLAN.md Фаза 3,
/// секция 05 дашборда) — буферизуются во время стриминга и разрешаются в
/// строки `membership_events`/`chat_events` только после того, как
/// достроен полный `alias_seen`/`people` (см. `name_to_people` ниже):
/// `members[]` в `remove_members`/`invite_members` несёт только
/// отображаемые имена без id (docs/SPEC.md §8.2), а человек, которого
/// приглашают, чаще всего ещё не написал ни одного сообщения к моменту
/// самого события `invite_members`.
struct PendingServiceEvent {
    ts: Option<i64>,
    actor_person_id: Option<i64>,
    actor_display_name: Option<String>,
    action: String,
    members: Vec<String>,
    title: Option<String>,
}

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

/// `Reading` — процент честно считается по байтам файла. `Finalizing` —
/// файл уже дочитан, но перед коммитом остаются агрегации (участники,
/// precompute-метрики дашборда/типов сообщений), у которых нет
/// измеримого «процента» — фронт должен показать это отдельным
/// состоянием (крутящиеся подписи), а не застрявшим «100%».
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportPhase {
    Reading,
    Finalizing,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportProgress {
    pub phase: ImportPhase,
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
/// `Unrecognized` не персистится — будущая версия формата, ни одна
/// метрика её не читает. Сервисные события с action, значимым для
/// таймлайна участников (вход/выход/удаление/групповые события —
/// `PendingServiceEvent` выше), персистятся в `membership_events`/
/// `chat_events`; остальные action (`pin_message`, звонки, ...) по-прежнему
/// не персистятся — ни одна текущая метрика их не читает (см. docs/PLAN.md).
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
    let started = std::time::Instant::now();
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
    // `true`, если хотя бы одно сообщение несло поле `reactions` (даже
    // пустое) — отличает "экспорт поддерживает реакции, тут их просто
    // нет" от "экспорт сделан старой версией Telegram Desktop, поля нет
    // вовсе" (docs/SPEC.md §7/§8.5). Второй случай не лечится повторным
    // импортом этого же файла.
    let mut saw_reactions_field = false;
    let mut pending_services: Vec<PendingServiceEvent> = Vec::new();

    let meta = {
        let mut insert_msg = tx
            .prepare(
                "INSERT INTO messages (chat_id, tg_msg_id, person_id, ts, text_len, word_count, \
                 media_type, has_photo, has_file, has_poll, has_location, has_contact, \
                 forwarded_from, edited_ts, duration_seconds) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            )
            .map_err(db_err)?;
        let mut insert_reaction = tx
            .prepare(
                "INSERT INTO message_reactions \
                 (chat_id, message_id, kind, emoji, document_id, count, known_actor_count) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .map_err(db_err)?;
        let mut insert_reaction_actor = tx
            .prepare(
                "INSERT INTO reaction_actors \
                 (chat_id, message_id, person_id, kind, emoji, document_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(db_err)?;
        let mut insert_reaction_preview = tx
            .prepare(
                "INSERT INTO message_reaction_previews (chat_id, message_id, preview_text) \
                 VALUES (?1, ?2, ?3)",
            )
            .map_err(db_err)?;

        parse_export(reader, |event| {
            if ctx.cancel.load(Ordering::Relaxed) {
                return Err(ImportError::Cancelled);
            }

            let m = match event {
                ParsedEvent::Message(m) => m,
                ParsedEvent::Service(s) => {
                    // Только action, которые таймлайн умеет разобрать
                    // (см. обработку pending_services ниже) — резолвим
                    // actor через resolve_person только для них, чтобы не
                    // заводить строку people ради, например, pin_message
                    // от человека, который иначе ни разу не участвовал.
                    let relevant = matches!(
                        s.action.as_str(),
                        "create_group"
                            | "invite_members"
                            | "remove_members"
                            | "edit_group_title"
                            | "edit_group_photo"
                            | "migrate_to_supergroup"
                            | "migrate_from_group"
                            | "join_group_by_link"
                    );
                    if !relevant {
                        return Ok(());
                    }
                    let actor_person_id = match s.actor.as_ref() {
                        Some(p) if !p.id.is_empty() => Some(
                            resolve_person(
                                &tx,
                                &mut people_cache,
                                chat_id,
                                &p.id,
                                p.display_name.as_deref(),
                            )
                            .map_err(db_err)?,
                        ),
                        _ => None,
                    };
                    let members: Vec<String> = s
                        .extra
                        .get("members")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    let title = s
                        .extra
                        .get("title")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    pending_services.push(PendingServiceEvent {
                        ts: s.date_unixtime,
                        actor_person_id,
                        actor_display_name: s.actor.as_ref().and_then(|p| p.display_name.clone()),
                        action: s.action,
                        members,
                        title,
                    });
                    return Ok(());
                }
                ParsedEvent::Unrecognized { .. } => return Ok(()), // будущая версия формата — не персистится в MVP
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
            let message_id = tx.last_insert_rowid();

            if let Some(reactions) = m.reactions.as_ref() {
                saw_reactions_field = true;
                if !reactions.is_empty() {
                    insert_reaction_preview
                        .execute(params![
                            chat_id,
                            message_id,
                            truncate_preview(&m.text.plain)
                        ])
                        .map_err(db_err)?;
                    for r in reactions {
                        insert_reaction
                            .execute(params![
                                chat_id,
                                message_id,
                                r.kind,
                                r.emoji,
                                r.document_id,
                                r.count,
                                r.recent.len() as i64,
                            ])
                            .map_err(db_err)?;
                        for actor in &r.recent {
                            let actor_person_id = if actor.who.id.is_empty() {
                                None
                            } else {
                                Some(
                                    resolve_person(
                                        &tx,
                                        &mut people_cache,
                                        chat_id,
                                        &actor.who.id,
                                        actor.who.display_name.as_deref(),
                                    )
                                    .map_err(db_err)?,
                                )
                            };
                            insert_reaction_actor
                                .execute(params![
                                    chat_id,
                                    message_id,
                                    actor_person_id,
                                    r.kind,
                                    r.emoji,
                                    r.document_id,
                                ])
                                .map_err(db_err)?;
                        }
                    }
                }
            }

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
                phase: ImportPhase::Reading,
                bytes_read: ctx.bytes_read.load(Ordering::Relaxed),
                total_bytes: source_file_size,
                message_count,
                people_count: people_cache.len() as i64,
            });

            Ok(())
        })?
    };

    // Файл дочитан — дальше алиасы/реимпорт-дедуп/precompute-агрегации
    // (recompute_dashboard_metrics/recompute_message_type_metrics ниже),
    // без измеримого прогресса по байтам. Один сигнал смены фазы,
    // дальше фронт сам крутит подписи по таймеру.
    (ctx.on_progress)(ImportProgress {
        phase: ImportPhase::Finalizing,
        bytes_read: ctx.bytes_read.load(Ordering::Relaxed),
        total_bytes: source_file_size,
        message_count,
        people_count: people_cache.len() as i64,
    });
    log::info!(
        "импорт «{source_filename}»: чтение файла завершено — {message_count} сообщений, \
         {} участников, {:.1?}",
        people_cache.len(),
        started.elapsed()
    );

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

    // Имя → person_id для разрешения `members[]` в буферизованных
    // сервисных событиях (docs/SPEC.md §8.2 — там только отображаемые
    // имена, без id). Источник — итоговое `people.display_name` (первое
    // когда-либо увиденное имя, resolve_person не переписывает его) и все
    // исторические имена из alias_seen (человек мог сменить ник между
    // приглашением и сообщениями). Неоднозначное имя (два разных
    // person_id с одинаковым отображаемым именем) сознательно не
    // резолвится — см. `PendingServiceEvent` выше.
    let mut name_to_people: HashMap<String, Vec<i64>> = HashMap::new();
    {
        let mut stmt = tx
            .prepare("SELECT id, display_name FROM people WHERE chat_id = ?1")
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![chat_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
            })
            .map_err(db_err)?;
        for row in rows {
            let (pid, name) = row.map_err(db_err)?;
            if let Some(name) = name {
                let ids = name_to_people.entry(name).or_default();
                if !ids.contains(&pid) {
                    ids.push(pid);
                }
            }
        }
    }
    for (pid, names) in &alias_seen {
        for name in names.keys() {
            let ids = name_to_people.entry(name.clone()).or_default();
            if !ids.contains(pid) {
                ids.push(*pid);
            }
        }
    }
    let resolve_name = |name: &str, map: &HashMap<String, Vec<i64>>| -> Option<i64> {
        match map.get(name) {
            Some(ids) if ids.len() == 1 => Some(ids[0]),
            _ => None,
        }
    };

    {
        let mut insert_membership = tx
            .prepare(
                "INSERT INTO membership_events \
                 (chat_id, person_id, unresolved_name, kind, ts, related_person_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(db_err)?;
        let mut insert_chat_event = tx
            .prepare(
                "INSERT INTO chat_events (chat_id, kind, ts, actor_id, detail) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(db_err)?;

        for ev in &pending_services {
            if ctx.cancel.load(Ordering::Relaxed) {
                log::info!("импорт «{source_filename}»: отменено на разборе сервисных событий");
                return Err(ImportError::Cancelled);
            }
            let Some(ts) = ev.ts else {
                continue; // без даты класть на таймлайн нечего
            };

            match ev.action.as_str() {
                "create_group" => {
                    insert_chat_event
                        .execute(params![
                            chat_id,
                            "created",
                            ts,
                            ev.actor_person_id,
                            ev.title.as_deref()
                        ])
                        .map_err(db_err)?;
                    if let Some(actor_id) = ev.actor_person_id {
                        // Основатель считается вошедшим в момент создания.
                        insert_membership
                            .execute(params![
                                chat_id,
                                actor_id,
                                None::<&str>,
                                "joined",
                                ts,
                                None::<i64>
                            ])
                            .map_err(db_err)?;
                    }
                }
                "join_group_by_link" => {
                    if let Some(actor_id) = ev.actor_person_id {
                        insert_membership
                            .execute(params![
                                chat_id,
                                actor_id,
                                None::<&str>,
                                "joined",
                                ts,
                                None::<i64>
                            ])
                            .map_err(db_err)?;
                    }
                }
                "invite_members" => {
                    for name in &ev.members {
                        let resolved = resolve_name(name, &name_to_people);
                        let unresolved_name = resolved.is_none().then_some(name.as_str());
                        insert_membership
                            .execute(params![
                                chat_id,
                                resolved,
                                unresolved_name,
                                "joined",
                                ts,
                                ev.actor_person_id
                            ])
                            .map_err(db_err)?;
                    }
                }
                "remove_members" => {
                    for name in &ev.members {
                        let resolved = resolve_name(name, &name_to_people);
                        let unresolved_name = resolved.is_none().then_some(name.as_str());
                        // Telegram кодирует самостоятельный выход тем же
                        // action, что и удаление — отличаем по совпадению
                        // имени актора и удаляемого (docs/PLAN.md план
                        // «Таймлайн участников»), не по разрешённому
                        // person_id: работает и когда имя неоднозначно.
                        let is_self = ev.actor_display_name.as_deref() == Some(name.as_str());
                        let kind = if is_self { "left" } else { "removed" };
                        let related = if is_self { None } else { ev.actor_person_id };
                        insert_membership
                            .execute(params![
                                chat_id,
                                resolved,
                                unresolved_name,
                                kind,
                                ts,
                                related
                            ])
                            .map_err(db_err)?;
                    }
                }
                "edit_group_title" => {
                    insert_chat_event
                        .execute(params![
                            chat_id,
                            "title_changed",
                            ts,
                            ev.actor_person_id,
                            ev.title.as_deref()
                        ])
                        .map_err(db_err)?;
                }
                "edit_group_photo" => {
                    insert_chat_event
                        .execute(params![
                            chat_id,
                            "photo_changed",
                            ts,
                            ev.actor_person_id,
                            None::<&str>
                        ])
                        .map_err(db_err)?;
                }
                "migrate_to_supergroup" | "migrate_from_group" => {
                    insert_chat_event
                        .execute(params![
                            chat_id,
                            "migrated",
                            ts,
                            ev.actor_person_id,
                            None::<&str>
                        ])
                        .map_err(db_err)?;
                }
                _ => {}
            }
        }
    }
    log::debug!(
        "импорт «{source_filename}»: сервисные события разобраны ({} шт.), {:.1?}",
        pending_services.len(),
        started.elapsed()
    );

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
            // membership_events/chat_events, в отличие от message_reactions/
            // reaction_actors/message_reaction_previews, не каскадируют от
            // messages (у них нет message_id вовсе) — стираем старые строки
            // явно, иначе они остались бы лежать под old_id рядом со свежими
            // (переехавшими с временного chat_id ниже) и задваивались бы на
            // каждый повторный импорт.
            tx.execute(
                "DELETE FROM membership_events WHERE chat_id = ?1",
                params![old_id],
            )
            .map_err(db_err)?;
            tx.execute(
                "DELETE FROM chat_events WHERE chat_id = ?1",
                params![old_id],
            )
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
            log::info!(
                "импорт «{source_filename}»: реимпорт чата id={old_id} — сверка {} участников",
                new_people.len()
            );
            let reconcile_started = std::time::Instant::now();

            for (new_person_id, tg_person_id, display_name) in new_people {
                if ctx.cancel.load(Ordering::Relaxed) {
                    log::info!(
                        "импорт «{source_filename}»: отменено на сверке участников (id={old_id})"
                    );
                    return Err(ImportError::Cancelled);
                }
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
                    // `AND chat_id = ?3` — не просто уточнение, а обязательное
                    // условие для производительности: `messages` не имеет
                    // отдельного индекса на голый `person_id` (только
                    // составной `(chat_id, person_id)`, бесполезный без
                    // ведущего `chat_id`), а `person_id` здесь и так уникален
                    // только в границах чата. Без этого условия SQLite не
                    // может использовать индекс и гоняет скан `messages`
                    // ВСЕХ чатов в базе на каждого участника — на чате с
                    // тысячами участников превращало реконсиляцию переимпорта
                    // в многоминутное подвисание без ошибки (найдено на
                    // реальном использовании).
                    tx.execute(
                        "UPDATE messages SET person_id = ?1 WHERE person_id = ?2 AND chat_id = ?3",
                        params![target_id, new_person_id, chat_id],
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
            log::debug!(
                "импорт «{source_filename}»: сверка участников (id={old_id}) готова, {:.1?}",
                reconcile_started.elapsed()
            );

            tx.execute(
                "UPDATE messages SET chat_id = ?1 WHERE chat_id = ?2",
                params![old_id, chat_id],
            )
            .map_err(db_err)?;
            // message_reactions/reaction_actors/message_reaction_previews несут
            // свой денормализованный chat_id (конвенция CLAUDE.md — никаких
            // запросов без фильтра по chat_id) отдельно от messages.chat_id,
            // который уже переехал строкой выше — без этого их подхватит
            // ON DELETE CASCADE у следующего DELETE FROM chats (они всё ещё
            // ссылались бы на временный chat_id).
            tx.execute(
                "UPDATE message_reactions SET chat_id = ?1 WHERE chat_id = ?2",
                params![old_id, chat_id],
            )
            .map_err(db_err)?;
            tx.execute(
                "UPDATE reaction_actors SET chat_id = ?1 WHERE chat_id = ?2",
                params![old_id, chat_id],
            )
            .map_err(db_err)?;
            tx.execute(
                "UPDATE message_reaction_previews SET chat_id = ?1 WHERE chat_id = ?2",
                params![old_id, chat_id],
            )
            .map_err(db_err)?;
            // membership_events/chat_events — та же денормализация
            // chat_id, что и у реакционных таблиц выше (те же причины);
            // person_id/actor_id/related_person_id, унаследованные от
            // временных `people`-строк, которые слились в существующего
            // человека выше и были удалены, — тот же неисправленный
            // пробел, что и у `reaction_actors.person_id` (ON DELETE SET
            // NULL вместо переезда на target_id), не новый для этого среза.
            tx.execute(
                "UPDATE membership_events SET chat_id = ?1 WHERE chat_id = ?2",
                params![old_id, chat_id],
            )
            .map_err(db_err)?;
            tx.execute(
                "UPDATE chat_events SET chat_id = ?1 WHERE chat_id = ?2",
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
         message_count=?6, participant_count=?7, source_file_size=?8, message_types_ready=1, \
         reactions_ready=1, reactions_supported=?9, timeline_ready=1 \
         WHERE id=?10",
        params![
            meta.tg_chat_id,
            meta.name,
            meta.chat_type,
            min_ts,
            max_ts,
            message_count,
            participant_count,
            source_file_size,
            saw_reactions_field,
            chat_id
        ],
    )
    .map_err(db_err)?;

    // Отмена проверяется между тремя пересчётами, а не только до них —
    // они идут одним блоком без цикла в этой функции, дальше проверять
    // уже негде, кроме как внутри самого запроса (не делаем, см. план).
    let precompute_started = std::time::Instant::now();
    if ctx.cancel.load(Ordering::Relaxed) {
        log::info!("импорт «{source_filename}»: отменено перед пересчётом метрик дашборда");
        return Err(ImportError::Cancelled);
    }
    crate::metrics::recompute_dashboard_metrics(&tx, chat_id).map_err(db_err)?;
    log::debug!(
        "импорт «{source_filename}»: recompute_dashboard_metrics — {:.1?}",
        precompute_started.elapsed()
    );

    if ctx.cancel.load(Ordering::Relaxed) {
        log::info!("импорт «{source_filename}»: отменено перед пересчётом типов сообщений");
        return Err(ImportError::Cancelled);
    }
    let step_started = std::time::Instant::now();
    crate::metrics::recompute_message_type_metrics(&tx, chat_id).map_err(db_err)?;
    log::debug!(
        "импорт «{source_filename}»: recompute_message_type_metrics — {:.1?}",
        step_started.elapsed()
    );

    if ctx.cancel.load(Ordering::Relaxed) {
        log::info!("импорт «{source_filename}»: отменено перед пересчётом реакций");
        return Err(ImportError::Cancelled);
    }
    let step_started = std::time::Instant::now();
    crate::metrics::recompute_reaction_metrics(&tx, chat_id).map_err(db_err)?;
    log::debug!(
        "импорт «{source_filename}»: recompute_reaction_metrics — {:.1?}",
        step_started.elapsed()
    );

    tx.commit().map_err(db_err)?;
    log::info!(
        "импорт «{source_filename}»: коммит готов, всего {:.1?}",
        started.elapsed()
    );

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
