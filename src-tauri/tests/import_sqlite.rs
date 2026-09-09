use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};

use rusqlite::{params, Connection};
use telegram_stats_lib::db::{self, import_into_db, ImportContext, ImportPhase, ImportProgress};
use telegram_stats_lib::import::{parse_export, ImportError, ParsedEvent};
use telegram_stats_lib::metrics::{
    get_activity_series, get_chat_totals, get_message_type_detail, get_message_type_overview,
    get_participant_timeline, get_reaction_person_breakdown, get_reactions_overview,
    get_top_participants, get_top_reacted_messages, granularity::pick_granularity, MessageTypeKind,
    MessageTypeSummaryStats, ReactionsOverview, TimelineOverview, TypeTimeView,
};

fn people_ordered_by_name(conn: &Connection, chat_id: i64) -> Vec<db::PersonRow> {
    let mut people = db::get_people(conn, chat_id).unwrap();
    people.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    people
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn fresh_db() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    conn.execute_batch(include_str!("../src/db/migrations/0001_init.sql"))
        .expect("apply migration 0001");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0002_chat_source_size.sql"
    ))
    .expect("apply migration 0002");
    conn.execute_batch(include_str!("../src/db/migrations/0003_people_aliases.sql"))
        .expect("apply migration 0003");
    conn.execute_batch(include_str!("../src/db/migrations/0004_message_types.sql"))
        .expect("apply migration 0004");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0005_dashboard_precompute.sql"
    ))
    .expect("apply migration 0005");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0006_message_type_precompute.sql"
    ))
    .expect("apply migration 0006");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0007_person_activity_span.sql"
    ))
    .expect("apply migration 0007");
    conn.execute_batch(include_str!("../src/db/migrations/0008_reactions.sql"))
        .expect("apply migration 0008");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0009_reaction_actor_emoji.sql"
    ))
    .expect("apply migration 0009");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0010_reaction_favorite_emoji.sql"
    ))
    .expect("apply migration 0010");
    conn.execute_batch(include_str!("../src/db/migrations/0011_timeline.sql"))
        .expect("apply migration 0011");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0012_timeline_indexes.sql"
    ))
    .expect("apply migration 0012");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0013_person_fk_indexes.sql"
    ))
    .expect("apply migration 0013");
    conn
}

fn import(conn: &mut Connection, name: &str) -> db::ChatSummary {
    import_with_size(conn, name, None)
}

fn import_with_size(conn: &mut Connection, name: &str, size: Option<i64>) -> db::ChatSummary {
    let file = File::open(fixture(name)).expect("fixture file should exist");
    let bytes_read = AtomicU64::new(0);
    let cancel = AtomicBool::new(false);
    let mut ctx = ImportContext {
        bytes_read: &bytes_read,
        on_progress: |_| {},
        cancel: &cancel,
    };
    import_into_db(conn, file, name, size, &mut ctx).expect("import should succeed")
}

#[test]
fn import_reports_finalizing_exactly_once_after_all_reading_events() {
    // Фронт крутит "подсчитываем..."-подписи, пока фаза Finalizing — она
    // должна прийти ровно один раз, строго после всех Reading-событий
    // (иначе фронт мог бы решить, что чтение файла ещё не кончилось), и
    // с уже финальными message_count/people_count, а не промежуточными.
    let mut conn = fresh_db();
    let file = File::open(fixture("group_50_members.json")).expect("fixture file should exist");
    let bytes_read = AtomicU64::new(0);
    let cancel = AtomicBool::new(false);
    let mut phases: Vec<ImportProgress> = Vec::new();
    let mut ctx = ImportContext {
        bytes_read: &bytes_read,
        on_progress: |p: ImportProgress| phases.push(p),
        cancel: &cancel,
    };
    let summary = import_into_db(&mut conn, file, "group_50_members.json", None, &mut ctx)
        .expect("import should succeed");

    let finalizing_count = phases
        .iter()
        .filter(|p| p.phase == ImportPhase::Finalizing)
        .count();
    assert_eq!(
        finalizing_count, 1,
        "Finalizing must be reported exactly once"
    );

    let finalizing_index = phases
        .iter()
        .position(|p| p.phase == ImportPhase::Finalizing)
        .unwrap();
    assert!(
        phases[..finalizing_index]
            .iter()
            .all(|p| p.phase == ImportPhase::Reading),
        "every event before Finalizing must be Reading, not interleaved"
    );
    assert_eq!(
        finalizing_index,
        phases.len() - 1,
        "Finalizing must be the last progress event"
    );

    let finalizing = &phases[finalizing_index];
    assert_eq!(finalizing.message_count, summary.message_count);
    assert_eq!(finalizing.people_count, summary.participant_count);
}

#[test]
fn three_day_chat_imports_and_picks_hourly_granularity() {
    let mut conn = fresh_db();
    let summary = import(&mut conn, "three_day_chat.json");
    assert_eq!(summary.message_count, 16);
    assert_eq!(summary.participant_count, 3);

    let (first, last) = (summary.first_msg_ts.unwrap(), summary.last_msg_ts.unwrap());
    assert_eq!(
        pick_granularity(first, last),
        telegram_stats_lib::metrics::Granularity::Hour
    );
}

#[test]
fn group_50_members_message_count_matches_parser_directly() {
    let mut conn = fresh_db();

    // независимая сверка: считаем ParsedEvent::Message напрямую через parse_export,
    // не хардкодим магическое число
    let file = File::open(fixture("group_50_members.json")).unwrap();
    let mut expected = 0i64;
    parse_export(file, |event| {
        if matches!(event, ParsedEvent::Message(_)) {
            expected += 1;
        }
        Ok(())
    })
    .unwrap();

    let summary = import(&mut conn, "group_50_members.json");
    assert_eq!(summary.message_count, expected);
    assert_eq!(summary.participant_count, 50);
}

#[test]
fn personal_chat_participants_are_degenerate() {
    let mut conn = fresh_db();
    let summary = import(&mut conn, "personal_chat.json");
    assert_eq!(summary.participant_count, 2);

    let top = get_top_participants(&conn, summary.id).unwrap();
    assert!(top.degenerate);
    assert_eq!(top.participants.len(), 2);
    // 4 сообщения (2 message-событий на service-звонок между ними) поровну на двоих
    let total: i64 = top.participants.iter().map(|p| p.message_count).sum();
    assert_eq!(total, 4);
    assert!(top.participants.iter().all(|p| p.message_count == 2));
}

#[test]
fn no_reactions_old_export_imports_cleanly() {
    let mut conn = fresh_db();
    let summary = import(&mut conn, "no_reactions_old_export.json");
    assert_eq!(summary.message_count, 18);
}

#[test]
fn reimporting_the_same_chat_updates_instead_of_duplicating() {
    let mut conn = fresh_db();
    let first = import(&mut conn, "three_day_chat.json");
    let second = import(&mut conn, "three_day_chat.json");

    assert_eq!(
        first.id, second.id,
        "re-import must reuse the same chat row"
    );
    assert_eq!(second.message_count, 16, "message_count must not double");

    let chats = db::list_chats(&conn).unwrap();
    let matching: Vec<_> = chats.iter().filter(|c| c.id == first.id).collect();
    assert_eq!(
        matching.len(),
        1,
        "exactly one row should survive the re-import"
    );
}

#[test]
fn source_file_size_round_trips_and_updates_on_reimport() {
    let mut conn = fresh_db();
    let first = import_with_size(&mut conn, "three_day_chat.json", Some(1000));
    assert_eq!(first.source_file_size, Some(1000));

    let second = import_with_size(&mut conn, "three_day_chat.json", Some(2000));
    assert_eq!(
        second.source_file_size,
        Some(2000),
        "must reflect the newest import, not the first"
    );

    let chats = db::list_chats(&conn).unwrap();
    let row = chats.iter().find(|c| c.id == first.id).unwrap();
    assert_eq!(row.source_file_size, Some(2000));
}

#[test]
fn deleting_a_chat_removes_its_messages_and_people() {
    let mut conn = fresh_db();
    let doomed = import(&mut conn, "three_day_chat.json");
    let survivor = import(&mut conn, "personal_chat.json");

    db::delete_chat(&conn, doomed.id).unwrap();

    let chats = db::list_chats(&conn).unwrap();
    assert_eq!(chats.len(), 1, "only the deleted chat's row should be gone");
    assert_eq!(chats[0].id, survivor.id);

    let messages_left: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE chat_id = ?1",
            [doomed.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(messages_left, 0);
    let people_left: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM people WHERE chat_id = ?1",
            [doomed.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(people_left, 0);

    let survivor_messages: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE chat_id = ?1",
            [survivor.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        survivor_messages, survivor.message_count,
        "the other chat's rows must be untouched"
    );
}

#[test]
fn a_json_object_without_messages_is_reported_as_not_a_telegram_export() {
    let mut conn = fresh_db();
    let bytes_read = AtomicU64::new(0);
    let cancel = AtomicBool::new(false);
    let mut ctx = ImportContext {
        bytes_read: &bytes_read,
        on_progress: |_| {},
        cancel: &cancel,
    };

    // валидный JSON, но не экспорт чата — нет поля `messages` вовсе
    let not_an_export = std::io::Cursor::new(br#"{"foo": "bar", "chats": {"list": []}}"#.to_vec());
    let result = import_into_db(
        &mut conn,
        not_an_export,
        "not_an_export.json",
        None,
        &mut ctx,
    );

    assert!(matches!(result, Err(ImportError::NotTelegramExport)));
    assert!(
        db::list_chats(&conn).unwrap().is_empty(),
        "a rejected import must not leave a stray chat row"
    );
}

#[test]
fn cancelling_mid_import_leaves_no_trace() {
    let mut conn = fresh_db();
    let bytes_read = AtomicU64::new(0);
    let cancel = AtomicBool::new(true); // отменено ещё до старта — проверяем самый ранний выход
    let mut ctx = ImportContext {
        bytes_read: &bytes_read,
        on_progress: |_| {},
        cancel: &cancel,
    };

    let file = File::open(fixture("three_day_chat.json")).expect("fixture file should exist");
    let result = import_into_db(&mut conn, file, "three_day_chat.json", None, &mut ctx);

    assert!(matches!(result, Err(ImportError::Cancelled)));
    assert!(
        db::list_chats(&conn).unwrap().is_empty(),
        "a cancelled import must not commit a partial chat"
    );
}

#[test]
fn merging_people_moves_messages_and_drops_the_duplicate_row() {
    let mut conn = fresh_db();
    let chat = import(&mut conn, "three_day_chat.json");
    let people = people_ordered_by_name(&conn, chat.id);
    assert_eq!(people.len(), 3, "fixture has 3 distinct participants");

    let primary = &people[0];
    let duplicate = &people[1];
    let primary_before = primary.message_count;
    let duplicate_before = duplicate.message_count;

    db::merge_people(&conn, chat.id, primary.id, &[duplicate.id]).unwrap();

    let after = people_ordered_by_name(&conn, chat.id);
    assert_eq!(after.len(), 2, "the merged person must no longer appear");
    let merged_primary = after.iter().find(|p| p.id == primary.id).unwrap();
    assert_eq!(
        merged_primary.message_count,
        primary_before + duplicate_before
    );

    let top = get_top_participants(&conn, chat.id).unwrap();
    assert!(
        top.participants.iter().all(|p| p.person_id != duplicate.id),
        "metrics must not surface a merged-away person"
    );
}

#[test]
fn merge_and_hide_survive_a_reimport_of_the_same_chat() {
    let mut conn = fresh_db();
    let first = import(&mut conn, "three_day_chat.json");
    let people = people_ordered_by_name(&conn, first.id);
    let (primary, duplicate, hidden_one) = (&people[0], &people[1], &people[2]);
    let combined_before_reimport = primary.message_count + duplicate.message_count;

    db::merge_people(&conn, first.id, primary.id, &[duplicate.id]).unwrap();
    db::hide_person(&conn, first.id, hidden_one.id, true).unwrap();

    let visible_before_reimport = people_ordered_by_name(&conn, first.id);
    assert_eq!(
        visible_before_reimport.len(),
        2,
        "get_people drops merged-away rows but still lists hidden ones (hidden is a display flag, not a merge)"
    );

    let second = import(&mut conn, "three_day_chat.json");
    assert_eq!(second.id, first.id);

    // Объединённая строка не должна воскреснуть как отдельный участник,
    // а скрытая должна остаться скрытой — иначе повторная выгрузка того
    // же чата тихо стирает ручную разметку из шага разбора участников.
    let visible_after_reimport = people_ordered_by_name(&conn, second.id);
    assert_eq!(
        visible_after_reimport.len(),
        2,
        "merge must survive re-import: the duplicate must not come back as a new row"
    );
    // Сообщения при повторном импорте не накапливаются, а заменяются
    // (тот же принцип, что и в reimporting_the_same_chat_updates_instead_of_duplicating) —
    // поэтому ожидаем тот же общий счёт, что и до реимпорта, а не удвоенный.
    let primary_after = visible_after_reimport
        .iter()
        .find(|p| p.id == primary.id)
        .unwrap();
    assert_eq!(
        primary_after.message_count, combined_before_reimport,
        "re-imported messages for the merged-away duplicate must land on the primary, not vanish from every metric"
    );

    let totals = get_top_participants(&conn, second.id).unwrap();
    assert!(
        totals
            .participants
            .iter()
            .all(|p| p.person_id != hidden_one.id),
        "a hidden participant must stay hidden after re-import"
    );
}

#[test]
fn aliases_accumulate_distinct_display_names_for_the_same_person() {
    let mut conn = fresh_db();
    let bytes_read = AtomicU64::new(0);
    let cancel = AtomicBool::new(false);
    let mut ctx = ImportContext {
        bytes_read: &bytes_read,
        on_progress: |_| {},
        cancel: &cancel,
    };

    let export = br#"{
        "name": "Test", "type": "private_group", "id": 1,
        "messages": [
            {"type": "message", "id": 1, "date_unixtime": "1000", "from": "Old Name", "from_id": "user1", "text": "hi"},
            {"type": "message", "id": 2, "date_unixtime": "2000", "from": "New Name", "from_id": "user1", "text": "hi again"}
        ]
    }"#;
    let chat = import_into_db(
        &mut conn,
        std::io::Cursor::new(export.to_vec()),
        "aliases.json",
        None,
        &mut ctx,
    )
    .expect("import should succeed");

    let people = db::get_people(&conn, chat.id).unwrap();
    assert_eq!(people.len(), 1);
    let mut aliases = people[0].aliases.clone();
    aliases.sort();
    assert_eq!(
        aliases,
        vec!["New Name".to_string(), "Old Name".to_string()]
    );
}

#[test]
fn message_type_overview_counts_every_kind_in_english_chat() {
    let mut conn = fresh_db();
    let chat = import(&mut conn, "english_chat.json");

    let overview = get_message_type_overview(&conn, chat.id).unwrap();
    assert!(overview.available);
    assert_eq!(overview.total_messages, chat.message_count);
    assert_eq!(overview.counts.len(), MessageTypeKind::ALL.len());
    // Фикстура специально устроена так, что у каждого из 12 не-текстовых
    // типов ровно одно вхождение — легко проверить, что ни один не
    // потерялся и не задвоился в другой категории (например файл vs
    // документ-без-медиатипа). Text не эксклюзивен с остальными — считает
    // все сообщения с непустым текстом, включая подпись к фото (id 8).
    for c in &overview.counts {
        let expected = if c.kind == MessageTypeKind::Text {
            7
        } else {
            1
        };
        assert_eq!(c.count, expected, "kind {:?} count mismatch", c.kind);
    }
}

#[test]
fn a_photo_with_a_caption_counts_as_both_photo_and_text() {
    // id 8 в фикстуре — фото с непустым text. Photo/Text не эксклюзивны
    // (в отличие от media_type-типов друг с другом), одно сообщение
    // должно засчитаться в обе пилюли одновременно.
    let mut conn = fresh_db();
    let chat = import(&mut conn, "english_chat.json");

    let both: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE chat_id = ?1 AND has_photo = 1 AND text_len > 0",
            [chat.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        both, 1,
        "the photo-with-caption fixture message must satisfy both predicates at once"
    );
}

#[test]
fn message_type_detail_uses_duration_variant_for_voice_and_video_message() {
    let mut conn = fresh_db();
    let chat = import(&mut conn, "english_chat.json");

    for kind in [MessageTypeKind::VoiceMessage, MessageTypeKind::VideoMessage] {
        let detail = get_message_type_detail(&conn, chat.id, kind, None)
            .unwrap()
            .unwrap_or_else(|| panic!("{kind:?} should have a detail"));
        assert!(
            matches!(detail.summary, MessageTypeSummaryStats::Duration { .. }),
            "{kind:?} must use the duration-stats variant, not frequency"
        );
    }
}

#[test]
fn message_type_detail_uses_frequency_variant_for_everything_else() {
    let mut conn = fresh_db();
    let chat = import(&mut conn, "english_chat.json");

    for kind in MessageTypeKind::ALL {
        if matches!(
            kind,
            MessageTypeKind::VoiceMessage | MessageTypeKind::VideoMessage
        ) {
            continue;
        }
        let detail = get_message_type_detail(&conn, chat.id, kind, None)
            .unwrap()
            .unwrap_or_else(|| panic!("{kind:?} should have a detail"));
        assert!(
            matches!(detail.summary, MessageTypeSummaryStats::Frequency { .. }),
            "{kind:?} must use the frequency-stats variant"
        );
    }
}

#[test]
fn message_type_detail_is_none_for_a_kind_with_zero_occurrences() {
    let mut conn = fresh_db();
    // three_day_chat.json не содержит опросов вовсе.
    let chat = import(&mut conn, "three_day_chat.json");

    let detail = get_message_type_detail(&conn, chat.id, MessageTypeKind::Poll, None).unwrap();
    assert!(
        detail.is_none(),
        "a kind with zero matches must return None, not an empty/zeroed detail"
    );
}

#[test]
fn short_chat_disables_month_view_and_degrades_the_peak() {
    let mut conn = fresh_db();
    // ~2 дня — короче MONTH_VIEW_MIN_SPAN_DAYS, "по месяцам" должно быть
    // недоступно (docs/SPEC.md §7 — честная деградация, не пустой график).
    let chat = import(&mut conn, "three_day_chat.json");

    // ровно одно отредактированное сообщение в фикстуре — что бы то ни
    // было, на одном бакете "пик" не может быть содержательным числом.
    let detail = get_message_type_detail(&conn, chat.id, MessageTypeKind::Edited, None)
        .unwrap()
        .expect("fixture has exactly one edited message");

    assert!(
        !detail.views_available.contains(&TypeTimeView::Month),
        "a ~2-day chat must not offer a month view"
    );
    assert_eq!(
        detail.selected_view,
        TypeTimeView::DayOfWeek,
        "must fall back to the next available view"
    );
    match detail.summary {
        MessageTypeSummaryStats::Frequency { peak, .. } => {
            assert!(
                peak.is_none(),
                "too few buckets for a meaningful peak — must degrade, not lie"
            );
        }
        MessageTypeSummaryStats::Duration { .. } => panic!("edited is not duration-based"),
    }
}

#[test]
fn requesting_an_unavailable_view_falls_back_instead_of_erroring() {
    let mut conn = fresh_db();
    let chat = import(&mut conn, "three_day_chat.json");

    // Явно просим "по месяцам" на чате, где он недоступен — не должно падать.
    let detail = get_message_type_detail(
        &conn,
        chat.id,
        MessageTypeKind::Edited,
        Some(TypeTimeView::Month),
    )
    .unwrap()
    .expect("fixture has exactly one edited message");
    assert_eq!(detail.selected_view, TypeTimeView::DayOfWeek);
}

#[test]
fn day_of_week_view_buckets_by_weekday_monday_first_not_by_calendar_date() {
    // 2024-01-01 — понедельник, 2024-01-03 — среда (внешний факт
    // календаря, не зависит от кода). Два сообщения в один и тот же
    // день недели (оба понедельника, разные даты) должны схлопнуться в
    // один бакет "0" — это и отличает day_of_week от старого поведения
    // "по дням" (абсолютная календарная дата, каждый день — свой бакет).
    let mut conn = fresh_db();
    let bytes_read = AtomicU64::new(0);
    let cancel = AtomicBool::new(false);
    let mut ctx = ImportContext {
        bytes_read: &bytes_read,
        on_progress: |_| {},
        cancel: &cancel,
    };

    let export = br#"{
        "name": "Weekday test", "type": "private_group", "id": 1,
        "messages": [
            {"type": "message", "id": 1, "date_unixtime": "1704103200", "from": "A", "from_id": "user1", "text": "monday one"},
            {"type": "message", "id": 2, "date_unixtime": "1704708000", "from": "A", "from_id": "user1", "text": "monday two, a week later"},
            {"type": "message", "id": 3, "date_unixtime": "1704276000", "from": "A", "from_id": "user1", "text": "wednesday"}
        ]
    }"#;
    let chat = import_into_db(
        &mut conn,
        std::io::Cursor::new(export.to_vec()),
        "weekday.json",
        None,
        &mut ctx,
    )
    .expect("import should succeed");

    let detail = get_message_type_detail(
        &conn,
        chat.id,
        MessageTypeKind::Text,
        Some(TypeTimeView::DayOfWeek),
    )
    .unwrap()
    .expect("all three messages have text");

    assert_eq!(detail.selected_view, TypeTimeView::DayOfWeek);
    let by_bucket: std::collections::HashMap<String, i64> = detail
        .buckets
        .into_iter()
        .map(|b| (b.bucket, b.count))
        .collect();
    assert_eq!(
        by_bucket.get("0"),
        Some(&2),
        "both Mondays must collapse into weekday index 0"
    );
    assert_eq!(
        by_bucket.get("2"),
        Some(&1),
        "Wednesday must land on weekday index 2"
    );
    assert_eq!(by_bucket.len(), 2, "only two distinct weekdays occurred");
}

#[test]
fn chat_imported_before_the_message_types_migration_reports_unavailable() {
    // Симулируем чат, импортированный до появления этой метрики: миграции
    // 0001-0003 применены (и через них были вставлены messages/chats),
    // 0004 применяется отдельно поверх уже существующих строк — ровно то,
    // что происходит с реальной базой при апдейте приложения. Разница с
    // fresh_db()+import(): здесь message_types_ready остаётся 0, потому
    // что строка chats никогда не проходила через import_into_db после
    // того, как эта колонка появилась.
    let conn = Connection::open_in_memory().expect("open in-memory db");
    conn.execute_batch(include_str!("../src/db/migrations/0001_init.sql"))
        .expect("apply migration 0001");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0002_chat_source_size.sql"
    ))
    .expect("apply migration 0002");
    conn.execute_batch(include_str!("../src/db/migrations/0003_people_aliases.sql"))
        .expect("apply migration 0003");

    conn.execute(
        "INSERT INTO chats (tg_chat_id, name, chat_type, imported_at, message_count, participant_count) \
         VALUES (1, 'Old chat', 'private_group', 0, 5, 2)",
        [],
    )
    .expect("insert pre-existing chat row");
    let chat_id = conn.last_insert_rowid();

    conn.execute_batch(include_str!("../src/db/migrations/0004_message_types.sql"))
        .expect("apply migration 0004 on top of the pre-existing chat");

    let overview = get_message_type_overview(&conn, chat_id).unwrap();
    assert!(
        !overview.available,
        "a chat that never went through the updated importer must not report counts"
    );
    assert_eq!(
        overview.total_messages, 5,
        "still surfaces the chat's known message_count, just without a type breakdown"
    );
    assert!(overview.counts.is_empty());
}

#[test]
fn precomputed_totals_and_activity_match_a_live_scan_of_messages() {
    // Независимая сверка: числа из precomputed-таблиц (chats.word_count/
    // char_count, activity_buckets, person_message_stats) не должны
    // разъезжаться с тем, что даёт прямой SQL-скан messages — иначе
    // оптимизация тихо портит данные, а не просто ускоряет их чтение.
    for fixture_name in [
        "three_day_chat.json",
        "group_50_members.json",
        "english_chat.json",
    ] {
        let mut conn = fresh_db();
        let chat = import(&mut conn, fixture_name);

        let (expected_words, expected_chars): (i64, i64) = conn
            .query_row(
                "SELECT COALESCE(SUM(word_count), 0), COALESCE(SUM(text_len), 0) \
                 FROM messages WHERE chat_id = ?1",
                [chat.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let totals = get_chat_totals(&conn, chat.id).unwrap().unwrap();
        assert_eq!(
            totals.word_count, expected_words,
            "{fixture_name}: word_count mismatch"
        );
        assert_eq!(
            totals.char_count, expected_chars,
            "{fixture_name}: char_count mismatch"
        );

        let messages_with_ts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE chat_id = ?1 AND ts IS NOT NULL",
                [chat.id],
                |row| row.get(0),
            )
            .unwrap();
        let activity = get_activity_series(&conn, chat.id).unwrap();
        let bucketed_total: i64 =
            activity.map_or(0, |series| series.buckets.iter().map(|b| b.count).sum());
        assert_eq!(
            bucketed_total, messages_with_ts,
            "{fixture_name}: activity_buckets must cover every timestamped message exactly once"
        );

        let expected_participant_total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE chat_id = ?1 AND person_id IS NOT NULL",
                [chat.id],
                |row| row.get(0),
            )
            .unwrap();
        let top = get_top_participants(&conn, chat.id).unwrap();
        let participant_total: i64 = top.participants.iter().map(|p| p.message_count).sum();
        assert_eq!(
            participant_total, expected_participant_total,
            "{fixture_name}: top participants must account for every attributed message"
        );

        // get_people (экран «Участники») — своя precomputed-таблица
        // (person_message_stats.first_ts/last_ts), сверяем per-person
        // MIN/MAX независимым запросом.
        for person in db::get_people(&conn, chat.id).unwrap() {
            let (expected_count, expected_first, expected_last): (i64, Option<i64>, Option<i64>) =
                conn.query_row(
                    "SELECT COUNT(*), MIN(ts), MAX(ts) FROM messages WHERE person_id = ?1",
                    [person.id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            assert_eq!(
                person.message_count, expected_count,
                "{fixture_name}/{:?}: get_people message_count mismatch",
                person.display_name
            );
            assert_eq!(
                person.first_msg_ts, expected_first,
                "{fixture_name}/{:?}: get_people first_msg_ts mismatch",
                person.display_name
            );
            assert_eq!(
                person.last_msg_ts, expected_last,
                "{fixture_name}/{:?}: get_people last_msg_ts mismatch",
                person.display_name
            );
        }
    }
}

#[test]
fn dashboard_metrics_self_heal_for_a_chat_imported_before_the_precompute_migration() {
    // Тот же приём, что и в chat_imported_before_the_message_types_migration_reports_unavailable:
    // симулируем чат, чья строка chats никогда не проходила через
    // import_into_db после появления precompute (миграция 0005 применяется
    // на уже существующую строку, dashboard_ready остаётся 0 по умолчанию).
    // В отличие от message_types_ready это НЕ новая метрика — сумма слов/
    // символов/активность/топ участников уже работали, поэтому здесь не
    // должно быть "недоступно": первое чтение обязано лениво досчитать.
    let conn = Connection::open_in_memory().expect("open in-memory db");
    conn.execute_batch(include_str!("../src/db/migrations/0001_init.sql"))
        .expect("apply migration 0001");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0002_chat_source_size.sql"
    ))
    .expect("apply migration 0002");
    conn.execute_batch(include_str!("../src/db/migrations/0003_people_aliases.sql"))
        .expect("apply migration 0003");
    conn.execute_batch(include_str!("../src/db/migrations/0004_message_types.sql"))
        .expect("apply migration 0004");

    conn.execute(
        "INSERT INTO chats (tg_chat_id, name, chat_type, imported_at, first_msg_ts, last_msg_ts, \
         message_count, participant_count, message_types_ready) \
         VALUES (1, 'Old chat', 'private_group', 0, 1000, 2000, 2, 1, 1)",
        [],
    )
    .expect("insert pre-existing chat row");
    let chat_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO people (chat_id, tg_person_id, display_name) VALUES (?1, 'user1', 'Alice')",
        [chat_id],
    )
    .expect("insert pre-existing person row");
    let person_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO messages (chat_id, tg_msg_id, person_id, ts, text_len, word_count) \
         VALUES (?1, 1, ?2, 1000, 5, 1), (?1, 2, ?2, 2000, 7, 2)",
        params![chat_id, person_id],
    )
    .expect("insert pre-existing messages");

    conn.execute_batch(include_str!(
        "../src/db/migrations/0005_dashboard_precompute.sql"
    ))
    .expect("apply migration 0005 on top of the pre-existing chat");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0006_message_type_precompute.sql"
    ))
    .expect("apply migration 0006 on top of the pre-existing chat");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0007_person_activity_span.sql"
    ))
    .expect("apply migration 0007 on top of the pre-existing chat");

    let ready_before: i64 = conn
        .query_row(
            "SELECT dashboard_ready FROM chats WHERE id = ?1",
            [chat_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        ready_before, 0,
        "a pre-existing chat must start out not-yet-precomputed"
    );

    let totals = get_chat_totals(&conn, chat_id).unwrap().unwrap();
    assert_eq!(
        totals.word_count, 3,
        "must self-heal instead of reporting a stale zero"
    );
    assert_eq!(totals.char_count, 12);

    let activity = get_activity_series(&conn, chat_id)
        .unwrap()
        .expect("has timestamped messages");
    let bucketed_total: i64 = activity.buckets.iter().map(|b| b.count).sum();
    assert_eq!(bucketed_total, 2);

    let top = get_top_participants(&conn, chat_id).unwrap();
    assert_eq!(top.participants.len(), 1);
    assert_eq!(top.participants[0].message_count, 2);

    // get_people (экран «Участники») читает ту же person_message_stats —
    // тот self-heal, что и остальные, должен покрыть и её.
    let people = db::get_people(&conn, chat_id).unwrap();
    assert_eq!(people.len(), 1);
    assert_eq!(people[0].message_count, 2);
    assert_eq!(people[0].first_msg_ts, Some(1000));
    assert_eq!(people[0].last_msg_ts, Some(2000));

    let ready_after: i64 = conn
        .query_row(
            "SELECT dashboard_ready FROM chats WHERE id = ?1",
            [chat_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        ready_after, 1,
        "self-heal must mark the chat ready so it isn't recomputed on every open"
    );
}

#[test]
fn merging_people_keeps_person_message_stats_consistent_not_just_the_live_view() {
    // Не только get_top_participants (который и так проверяется в
    // merging_people_moves_messages_and_drops_the_duplicate_row) — сама
    // таблица person_message_stats не должна держать призрачную строку
    // под объединённым person_id после merge_people (иначе следующий
    // merge поверх неё уже не гарантированно корректен).
    let mut conn = fresh_db();
    let chat = import(&mut conn, "three_day_chat.json");
    let people = people_ordered_by_name(&conn, chat.id);
    let (primary, duplicate) = (&people[0], &people[1]);

    db::merge_people(&conn, chat.id, primary.id, &[duplicate.id]).unwrap();

    let stale_row_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM person_message_stats WHERE person_id = ?1",
            [duplicate.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        stale_row_count, 0,
        "the merged-away person_id must not linger in the precomputed table"
    );
}

#[test]
fn merging_people_combines_activity_span_not_just_message_count() {
    // get_people отдаёт first_msg_ts/last_msg_ts из person_message_stats
    // (recompute_dashboard_metrics) — после слияния диапазон должен
    // расшириться до объединения обоих участников, а не остаться
    // диапазоном одного только primary.
    let mut conn = fresh_db();
    let chat = import(&mut conn, "three_day_chat.json");
    let people = people_ordered_by_name(&conn, chat.id);
    let (primary, duplicate) = (&people[0], &people[1]);
    let expected_first = primary
        .first_msg_ts
        .unwrap()
        .min(duplicate.first_msg_ts.unwrap());
    let expected_last = primary
        .last_msg_ts
        .unwrap()
        .max(duplicate.last_msg_ts.unwrap());

    db::merge_people(&conn, chat.id, primary.id, &[duplicate.id]).unwrap();

    let merged = people_ordered_by_name(&conn, chat.id)
        .into_iter()
        .find(|p| p.id == primary.id)
        .unwrap();
    assert_eq!(merged.first_msg_ts, Some(expected_first));
    assert_eq!(merged.last_msg_ts, Some(expected_last));
}

#[test]
fn precomputed_message_type_counts_match_a_live_scan() {
    // Та же независимая сверка, что и для totals/activity/участников, но
    // для message_type_counts — предикаты продублированы явно (как и в
    // a_photo_with_a_caption_counts_as_both_photo_and_text), не через
    // приватный predicate_sql.
    for fixture_name in [
        "three_day_chat.json",
        "group_50_members.json",
        "english_chat.json",
    ] {
        let mut conn = fresh_db();
        let chat = import(&mut conn, fixture_name);

        let overview = get_message_type_overview(&conn, chat.id).unwrap();
        assert!(overview.available);
        for c in &overview.counts {
            let predicate = match c.kind {
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
            };
            let expected: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM messages WHERE chat_id = ?1 AND {predicate}"),
                    [chat.id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(
                c.count, expected,
                "{fixture_name}/{:?} count mismatch",
                c.kind
            );
        }
    }
}

#[test]
fn message_type_metrics_self_heal_for_a_chat_marked_ready_before_this_precompute_existed() {
    // message_types_ready=1 (сырые колонки типов уже заполнены — чат
    // прошёл актуальный импортер), но message_type_counts никогда не
    // считалась для этой строки (симулирует апгрейд: миграция 0006
    // добавила таблицы, но строка chats старше этого среза). Это не
    // "недоступно" (как chat_imported_before_the_message_types_migration_
    // reports_unavailable, где ready=0) — первое чтение обязано лениво
    // досчитать, а не тихо всегда отдавать нули.
    let conn = fresh_db();
    conn.execute(
        "INSERT INTO chats (tg_chat_id, name, chat_type, imported_at, first_msg_ts, last_msg_ts, \
         message_count, participant_count, message_types_ready) \
         VALUES (1, 'Old chat', 'private_group', 0, 1000, 2000, 2, 1, 1)",
        [],
    )
    .expect("insert pre-existing chat row");
    let chat_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO people (chat_id, tg_person_id, display_name) VALUES (?1, 'user1', 'Alice')",
        [chat_id],
    )
    .expect("insert pre-existing person row");
    let person_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO messages (chat_id, tg_msg_id, person_id, ts, text_len, word_count, has_photo) \
         VALUES (?1, 1, ?2, 1000, 5, 1, 1), (?1, 2, ?2, 2000, 0, 0, 0)",
        params![chat_id, person_id],
    )
    .expect("insert pre-existing messages");

    let has_rows_before: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM message_type_counts WHERE chat_id = ?1",
            [chat_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        has_rows_before, 0,
        "precompute must not have run yet for this pre-existing row"
    );

    let overview = get_message_type_overview(&conn, chat_id).unwrap();
    assert!(overview.available);
    let photo_count = overview
        .counts
        .iter()
        .find(|c| c.kind == MessageTypeKind::Photo)
        .unwrap()
        .count;
    assert_eq!(
        photo_count, 1,
        "must self-heal instead of reporting a stale zero"
    );

    let detail = get_message_type_detail(&conn, chat_id, MessageTypeKind::Photo, None)
        .unwrap()
        .expect("the one photo message must produce a detail");
    assert_eq!(detail.total_count, 1);
}

#[test]
fn merging_people_keeps_message_type_participant_stats_consistent() {
    // Тот же приём, что и merging_people_keeps_person_message_stats_consistent_not_just_the_live_view,
    // но для message_type_participant_stats: merge_people физически
    // переносит messages.person_id, эта precomputed-таблица не должна
    // держать строку под уже объединённым person_id ни под одним kind.
    let mut conn = fresh_db();
    let chat = import(&mut conn, "three_day_chat.json");
    let people = people_ordered_by_name(&conn, chat.id);
    let (primary, duplicate) = (&people[0], &people[1]);

    db::merge_people(&conn, chat.id, primary.id, &[duplicate.id]).unwrap();

    let stale_row_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM message_type_participant_stats WHERE person_id = ?1",
            [duplicate.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        stale_row_count, 0,
        "the merged-away person_id must not linger under any kind"
    );
}

#[test]
fn message_type_metrics_never_panic_across_the_full_fixture_set() {
    for fixture_name in [
        "three_day_chat.json",
        "group_50_members.json",
        "no_reactions_old_export.json",
        "personal_chat.json",
        "english_chat.json",
    ] {
        let mut conn = fresh_db();
        let chat = import(&mut conn, fixture_name);

        let overview = get_message_type_overview(&conn, chat.id).unwrap();
        assert!(
            overview.available,
            "{fixture_name}: freshly imported chats are always ready"
        );

        for kind in MessageTypeKind::ALL {
            let detail = get_message_type_detail(&conn, chat.id, kind, None).unwrap();
            if let Some(detail) = detail {
                assert!(
                    detail.total_count > 0,
                    "{fixture_name}/{kind:?}: Some(..) implies a non-zero count"
                );
                for view in [
                    TypeTimeView::Month,
                    TypeTimeView::DayOfWeek,
                    TypeTimeView::HourOfDay,
                ] {
                    // Явный запрос любого из трёх видов не должен падать,
                    // даже если он недоступен для этого чата (тихий фолбэк).
                    get_message_type_detail(&conn, chat.id, kind, Some(view)).unwrap();
                }
            }
        }
    }
}

// group_50_members.json несёт 24 сообщения с реакциями: 22 с одним 👍
// (count=2, recent=[Аня,Боря]) и одно (id=28, от Маши) с 👍 намеренно
// count=5 при recent той же длины 2 — синтетический edge case §8.5
// (Telegram отдаёт в recent не полный список реагировавших), плюс 5 из
// них несут ещё custom_emoji (count=1, recent=[Вера]), плюс одно (id=59,
// от Ани, чистое фото без подписи) с 🔥 count=4/recent=[Боря] — второй
// edge case неполноты и одновременно фикстура для media-label ниже.
// Ожидаемые суммы посчитаны python-скриптом при подготовке фикстуры, не
// хардкод наугад: total_reactions = 22*2+5(edge case)+5*1(custom)+4(🔥) = 58.
#[test]
fn reactions_overview_reports_aggregates_top_emoji_and_incomplete_given_flag() {
    let mut conn = fresh_db();
    let chat = import(&mut conn, "group_50_members.json");

    let overview = get_reactions_overview(&conn, chat.id).unwrap();
    let ReactionsOverview::Available(summary) = overview else {
        panic!("a freshly imported chat with reactions must report Available");
    };

    assert_eq!(summary.total_reactions, 58);
    assert_eq!(summary.reacted_message_count, 24);
    assert!(
        summary.given_data_incomplete,
        "message 28's 👍 has count=5 but only 2 known actors in `recent`"
    );

    let thumbs_up = summary
        .top_emoji
        .iter()
        .find(|e| e.emoji.as_deref() == Some("👍"))
        .expect("thumbs up must be present");
    assert_eq!(thumbs_up.count, 49);
    let custom = summary
        .top_emoji
        .iter()
        .find(|e| e.kind == "custom_emoji")
        .expect("custom_emoji must be present");
    assert_eq!(custom.count, 5);
    assert_eq!(custom.emoji, None, "custom_emoji has no plain emoji glyph");
    let fire = summary
        .top_emoji
        .iter()
        .find(|e| e.emoji.as_deref() == Some("🔥"))
        .expect("fire must be present");
    assert_eq!(fire.count, 4);

    // Таблица «Обмен реакциями» — без ограничения сверху (пользователь
    // должен видеть весь список, не только самых активных). Ожидаемое
    // число участников с любой активностью: 23 автора реагировавших
    // текстовых сообщений (id 28-50) + Аня (автор фото id 59, входит и
    // в given, и в received) + Боря + Вера (только given, никогда не
    // авторы реагировавших сообщений) = 23 + 1 + 2 = 26.
    assert_eq!(summary.exchange.len(), 26);
    assert!(
        summary
            .exchange
            .iter()
            .all(|r| r.given_count > 0 || r.received_count > 0),
        "no zero-activity rows should leak into the exchange table"
    );

    let ana = summary
        .exchange
        .iter()
        .find(|r| r.display_name.as_deref() == Some("Аня"))
        .expect("Аня must appear in `exchange`");
    assert_eq!(ana.given_count, 23);
    assert_eq!(ana.received_count, 4, "from her own photo message's 🔥");
    let ana_favorite = ana
        .favorite_given
        .as_ref()
        .expect("Аня must have a favorite emoji");
    assert_eq!(ana_favorite.emoji.as_deref(), Some("👍"));

    let borya = summary
        .exchange
        .iter()
        .find(|r| r.display_name.as_deref() == Some("Боря"))
        .expect("Боря must appear in `exchange`");
    assert_eq!(borya.given_count, 24);
    assert_eq!(borya.received_count, 0);
    let borya_favorite = borya
        .favorite_given
        .as_ref()
        .expect("Боря must have a favorite emoji");
    assert_eq!(
        borya_favorite.emoji.as_deref(),
        Some("👍"),
        "23 👍 vs 1 🔥 — 👍 must win"
    );

    // Кому ставят: точное число, не зависит от неполноты `recent` — у
    // Маши оно должно отразить count=5 (не len(recent)=2) на её сообщении.
    let masha = summary
        .exchange
        .iter()
        .find(|r| r.display_name.as_deref() == Some("Маша"))
        .expect("Маша must appear in `exchange`");
    assert_eq!(
        masha.received_count, 6,
        "5 (👍, edge case) + 1 (custom_emoji)"
    );
    assert_eq!(masha.given_count, 0);
    assert!(
        masha.favorite_given.is_none(),
        "Маша never reacts to anything in this fixture"
    );
}

#[test]
fn reactions_overview_reports_not_in_export_for_old_export_fixture() {
    let mut conn = fresh_db();
    let chat = import(&mut conn, "no_reactions_old_export.json");

    let overview = get_reactions_overview(&conn, chat.id).unwrap();
    assert!(
        matches!(overview, ReactionsOverview::NotInExport),
        "an export with no `reactions` field on any message must not report zeros as if measured"
    );
}

#[test]
fn reactions_overview_reports_needs_reimport_for_a_chat_imported_before_the_reactions_migration() {
    // Тот же приём, что и chat_imported_before_the_message_types_migration_
    // reports_unavailable, но для реакций: в отличие от message_types это
    // не self-heal-пересчёт — сырых message_reactions/reaction_actors для
    // такого чата попросту нет и никогда не появится без повторного
    // импорта исходного файла.
    let conn = Connection::open_in_memory().expect("open in-memory db");
    conn.execute_batch(include_str!("../src/db/migrations/0001_init.sql"))
        .expect("apply migration 0001");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0002_chat_source_size.sql"
    ))
    .expect("apply migration 0002");
    conn.execute_batch(include_str!("../src/db/migrations/0003_people_aliases.sql"))
        .expect("apply migration 0003");
    conn.execute_batch(include_str!("../src/db/migrations/0004_message_types.sql"))
        .expect("apply migration 0004");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0005_dashboard_precompute.sql"
    ))
    .expect("apply migration 0005");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0006_message_type_precompute.sql"
    ))
    .expect("apply migration 0006");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0007_person_activity_span.sql"
    ))
    .expect("apply migration 0007");

    conn.execute(
        "INSERT INTO chats (tg_chat_id, name, chat_type, imported_at, message_count, participant_count) \
         VALUES (1, 'Old chat', 'private_group', 0, 5, 2)",
        [],
    )
    .expect("insert pre-existing chat row");
    let chat_id = conn.last_insert_rowid();

    conn.execute_batch(include_str!("../src/db/migrations/0008_reactions.sql"))
        .expect("apply migration 0008 on top of the pre-existing chat");

    let overview = get_reactions_overview(&conn, chat_id).unwrap();
    assert!(
        matches!(overview, ReactionsOverview::NeedsReimport),
        "a chat that never went through the updated importer must ask for a re-import, not show zeros"
    );
}

#[test]
fn get_top_reacted_messages_ranks_by_total_count_and_carries_preview_text() {
    let mut conn = fresh_db();
    let chat = import(&mut conn, "group_50_members.json");

    let top = get_top_reacted_messages(&conn, chat.id).unwrap();
    assert_eq!(
        top.len(),
        10,
        "limited to the top 10 of 24 reacted messages"
    );
    assert!(
        top.windows(2).all(|w| w[0].total_count >= w[1].total_count),
        "must be sorted by total reaction count, descending"
    );

    let leader = &top[0];
    assert_eq!(leader.total_count, 6);
    assert_eq!(leader.display_name.as_deref(), Some("Маша"));
    assert_eq!(
        leader.tg_msg_id, 28,
        "must be the export's own message id, not the internal SQLite rowid"
    );
    assert_eq!(
        leader.preview_text.as_deref(),
        Some("привет всем, я Маша"),
        "preview must be captured at import time, since `messages` itself does not store text"
    );
    assert_eq!(
        leader.content_label, None,
        "a plain text message has no media kind"
    );
    assert_eq!(
        leader.reactions.len(),
        2,
        "Маша's message has 👍 (count=5) and custom_emoji (count=1)"
    );
    assert_eq!(leader.reactions[0].emoji.as_deref(), Some("👍"));
    assert_eq!(leader.reactions[0].count, 5);
    assert_eq!(leader.reactions[1].kind, "custom_emoji");
    assert_eq!(leader.reactions[1].count, 1);

    // Ани фото без подписи (id 59, 🔥 count=4) — единственное сообщение с
    // count=4, второе по счёту после Маши.
    let photo_entry = &top[1];
    assert_eq!(photo_entry.total_count, 4);
    assert_eq!(photo_entry.tg_msg_id, 59);
    assert_eq!(photo_entry.display_name.as_deref(), Some("Аня"));
    assert_eq!(
        photo_entry.preview_text.as_deref(),
        Some(""),
        "an empty caption still gets a (empty) preview row, not NULL"
    );
    assert_eq!(
        photo_entry.content_label.as_deref(),
        Some("Фото"),
        "a textless message must surface what kind of content it was, not just an em dash"
    );
    assert_eq!(photo_entry.reactions.len(), 1);
    assert_eq!(photo_entry.reactions[0].emoji.as_deref(), Some("🔥"));
    assert_eq!(photo_entry.reactions[0].count, 4);
}

#[test]
fn merging_people_recombines_given_reaction_counts_not_just_received() {
    // Отдельно от merging_people_keeps_person_message_stats_consistent_
    // not_just_the_live_view: там проверяется messages.person_id (влияет
    // на "кому ставят"), здесь — reaction_actors.person_id ("кто
    // ставит"), которую тот тест не касается вовсе.
    let mut conn = fresh_db();
    let chat = import(&mut conn, "group_50_members.json");
    let people = people_ordered_by_name(&conn, chat.id);
    let ana = people
        .iter()
        .find(|p| p.display_name.as_deref() == Some("Аня"))
        .unwrap();
    let borya = people
        .iter()
        .find(|p| p.display_name.as_deref() == Some("Боря"))
        .unwrap();

    db::merge_people(&conn, chat.id, ana.id, &[borya.id]).unwrap();

    let ReactionsOverview::Available(summary) = get_reactions_overview(&conn, chat.id).unwrap()
    else {
        panic!("chat must still report Available after a merge");
    };
    let merged = summary
        .exchange
        .iter()
        .find(|r| r.person_id == ana.id)
        .expect("the surviving person must carry the combined given count");
    assert_eq!(merged.given_count, 47, "23 (Аня) + 24 (Боря)");
    assert_eq!(
        merged
            .favorite_given
            .as_ref()
            .expect("must still have a favorite emoji after the merge")
            .emoji
            .as_deref(),
        Some("👍"),
        "combined 46 👍 (23 own + 23 Борины) vs 1 🔥 (Борин) — 👍 must still win"
    );
    assert!(
        summary.exchange.iter().all(|r| r.person_id != borya.id),
        "the merged-away person must not linger in `exchange`"
    );
}

#[test]
fn reaction_metrics_never_panic_across_the_full_fixture_set() {
    for fixture_name in [
        "three_day_chat.json",
        "group_50_members.json",
        "no_reactions_old_export.json",
        "personal_chat.json",
        "english_chat.json",
    ] {
        let mut conn = fresh_db();
        let chat = import(&mut conn, fixture_name);
        get_reactions_overview(&conn, chat.id).unwrap();
        get_top_reacted_messages(&conn, chat.id).unwrap();
    }
}

#[test]
fn get_reaction_person_breakdown_groups_by_emoji_type() {
    let mut conn = fresh_db();
    let chat = import(&mut conn, "group_50_members.json");
    let people = people_ordered_by_name(&conn, chat.id);

    // Боря — единственный, кто ставит оба типа реакций (👍 на все 23
    // текстовых сообщения, 🔥 на фото Ани) — должен получить две
    // отдельные строки, не слитые в одну "неизвестную" категорию.
    let borya = people
        .iter()
        .find(|p| p.display_name.as_deref() == Some("Боря"))
        .unwrap();
    let borya_breakdown = get_reaction_person_breakdown(&conn, chat.id, borya.id).unwrap();
    assert_eq!(borya_breakdown.given_total, 24);
    let thumbs_up = borya_breakdown
        .given
        .iter()
        .find(|e| e.emoji.as_deref() == Some("👍"))
        .expect("👍 must appear in Борин given breakdown");
    assert_eq!(thumbs_up.count, 23);
    let fire = borya_breakdown
        .given
        .iter()
        .find(|e| e.emoji.as_deref() == Some("🔥"))
        .expect("🔥 must appear in Борин given breakdown");
    assert_eq!(fire.count, 1);
    assert_eq!(
        borya_breakdown.received_total, 0,
        "Боря never authored a reacted message in this fixture"
    );
    assert!(borya_breakdown.received.is_empty());

    // Маша — только получает (никогда не реагирует сама в этой фикстуре).
    let masha = people
        .iter()
        .find(|p| p.display_name.as_deref() == Some("Маша"))
        .unwrap();
    let masha_breakdown = get_reaction_person_breakdown(&conn, chat.id, masha.id).unwrap();
    assert_eq!(masha_breakdown.given_total, 0);
    assert!(masha_breakdown.given.is_empty());
    assert_eq!(masha_breakdown.received_total, 6);
    let masha_thumbs_up = masha_breakdown
        .received
        .iter()
        .find(|e| e.emoji.as_deref() == Some("👍"))
        .unwrap();
    assert_eq!(masha_thumbs_up.count, 5);
}

#[test]
fn get_reaction_person_breakdown_reports_total_without_rows_for_pre_migration_data() {
    // Симулирует reaction_actors, вставленный до миграции 0009 (kind/
    // emoji/document_id тогда ещё не существовали, ALTER TABLE оставляет
    // NULL для старых строк) — общий счётчик уже верный (посчитан не из
    // этой разбивки), а разбивки по эмодзи нет и не может быть без
    // повторного импорта.
    let mut conn = fresh_db();
    let chat = import(&mut conn, "three_day_chat.json");
    let people = people_ordered_by_name(&conn, chat.id);
    let someone = &people[0];

    conn.execute(
        "INSERT INTO reaction_actors (chat_id, message_id, person_id, kind, emoji, document_id) \
         SELECT ?1, id, ?2, NULL, NULL, NULL FROM messages WHERE chat_id = ?1 LIMIT 1",
        params![chat.id, someone.id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO reaction_person_stats (chat_id, person_id, given_count, received_count) \
         VALUES (?1, ?2, 3, 0) \
         ON CONFLICT(chat_id, person_id) DO UPDATE SET given_count = given_count + 3",
        params![chat.id, someone.id],
    )
    .unwrap();

    let breakdown = get_reaction_person_breakdown(&conn, chat.id, someone.id).unwrap();
    assert_eq!(breakdown.given_total, 3);
    assert!(
        breakdown.given.is_empty(),
        "a NULL-kind row (pre-migration data) must not surface as a fake emoji category"
    );
}

#[test]
fn recompute_reaction_person_stats_leaves_favorite_given_null_for_untyped_reaction_actors() {
    // Тот же приём, что и в get_reaction_person_breakdown_reports_total_
    // without_rows_for_pre_migration_data, но через настоящий пересчёт
    // (recompute_reaction_person_stats), а не руками собранную строку —
    // NULL-kind реакция должна посчитаться в given_count (реакция была),
    // но не может стать "любимой", потому что у неё нет типа.
    //
    // group_50_members.json (не three_day_chat.json): нужен чат с
    // reactions_supported=1, иначе get_reactions_overview вернёт
    // NotInExport ещё до того, как дело дойдёт до exchange. Катя (id
    // ранняя, среди первых участников) в этой фикстуре никогда не
    // реагирует и не получает реакций — чистый лист для синтетической
    // NULL-kind строки.
    let mut conn = fresh_db();
    let chat = import(&mut conn, "group_50_members.json");
    let people = people_ordered_by_name(&conn, chat.id);
    let katya = people
        .iter()
        .find(|p| p.display_name.as_deref() == Some("Катя"))
        .unwrap();

    conn.execute(
        "INSERT INTO reaction_actors (chat_id, message_id, person_id, kind, emoji, document_id) \
         SELECT ?1, id, ?2, NULL, NULL, NULL FROM messages WHERE chat_id = ?1 LIMIT 1",
        params![chat.id, katya.id],
    )
    .unwrap();

    telegram_stats_lib::metrics::recompute_reaction_person_stats(&conn, chat.id).unwrap();

    // Катя ставит всего 1 реакцию в этой фикстуре — не попадёт в топ-8
    // exchange (там уже 8 человек с 3+), поэтому читаем строку
    // reaction_person_stats напрямую, а не через get_reactions_overview.
    let (given_count, top_given_kind): (i64, Option<String>) = conn
        .query_row(
            "SELECT given_count, top_given_kind FROM reaction_person_stats \
             WHERE chat_id = ?1 AND person_id = ?2",
            params![chat.id, katya.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(given_count, 1);
    assert!(
        top_given_kind.is_none(),
        "a NULL-kind reaction_actors row must not produce a fake favorite emoji"
    );
}

// --- Таймлайн участников (docs/PLAN.md Фаза 3, секция 05 дашборда) ---
//
// group_50_members.json: `create_group` (актор Аня) → Аня "joined";
// `invite_members` девяти имён (Боря..Катя) → девять "joined", related =
// Аня; `remove_members` ["Жора"] (актор Аня, Жора не отправлял событие
// сам за себя) → "removed", related = Аня. Остальные ~40 участников
// фикстуры (включая Жору до его удаления) присутствуют только через
// сообщения, без явного входа — `since_start`. `edit_group_photo` и
// `migrate_to_supergroup` — групповые события без членского смысла.

#[test]
fn group_50_members_populates_membership_and_chat_events() {
    let mut conn = fresh_db();
    let chat = import(&mut conn, "group_50_members.json");
    let people = people_ordered_by_name(&conn, chat.id);
    let anya = people
        .iter()
        .find(|p| p.display_name.as_deref() == Some("Аня"))
        .unwrap();
    let zhora = people
        .iter()
        .find(|p| p.display_name.as_deref() == Some("Жора"))
        .unwrap();

    let joined_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM membership_events WHERE chat_id = ?1 AND kind = 'joined'",
            params![chat.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        joined_count, 10,
        "founder from create_group + 9 names from invite_members"
    );

    let (zhora_kind, zhora_related): (String, Option<i64>) = conn
        .query_row(
            "SELECT kind, related_person_id FROM membership_events \
             WHERE chat_id = ?1 AND person_id = ?2",
            params![chat.id, zhora.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        zhora_kind, "removed",
        "Жора was named in remove_members by a different actor (Аня), not himself"
    );
    assert_eq!(zhora_related, Some(anya.id));

    let unresolved: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM membership_events WHERE chat_id = ?1 AND person_id IS NULL",
            params![chat.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(unresolved, 0, "every name in this fixture is unambiguous");

    let chat_event_kinds: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT kind FROM chat_events WHERE chat_id = ?1 ORDER BY ts")
            .unwrap();
        stmt.query_map(params![chat.id], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(
        chat_event_kinds,
        vec!["created", "photo_changed", "migrated"],
        "pin_message is not a timeline-relevant action, must not appear here"
    );
}

#[test]
fn participant_timeline_reports_founder_invitees_and_a_removal() {
    let mut conn = fresh_db();
    let chat = import(&mut conn, "group_50_members.json");
    let people = people_ordered_by_name(&conn, chat.id);
    let anya = people
        .iter()
        .find(|p| p.display_name.as_deref() == Some("Аня"))
        .unwrap();
    let zhora = people
        .iter()
        .find(|p| p.display_name.as_deref() == Some("Жора"))
        .unwrap();

    let overview = get_participant_timeline(&conn, chat.id).unwrap();
    let TimelineOverview::Available(summary) = overview else {
        panic!("a freshly imported chat with service events must report Available");
    };

    assert_eq!(
        summary.total, 50,
        "everyone who ever authored a message or has a resolved join event"
    );
    assert_eq!(summary.left, 1, "only Жора has a closing event");
    assert_eq!(summary.current, 49);
    assert_eq!(summary.group_event_count, 3);
    assert_eq!(summary.unresolved_event_count, 0);
    assert!(
        summary.peak_headcount >= 10,
        "must at least reflect the founder + 9 invitees, composition also counts since_start joins"
    );

    let anya_row = summary
        .people
        .iter()
        .find(|p| p.person_id == anya.id)
        .expect("Аня must have a timeline row");
    assert_eq!(anya_row.segments.len(), 1);
    assert!(
        anya_row
            .events
            .iter()
            .any(|e| e.kind == "joined" && e.ts == anya_row.segments[0].start_ts),
        "a resolved join event must exist at the segment's start"
    );
    assert!(
        anya_row.segments[0].end_ts.is_none(),
        "Аня was never removed or reported as leaving"
    );

    let zhora_row = summary
        .people
        .iter()
        .find(|p| p.person_id == zhora.id)
        .expect("Жора must have a timeline row");
    assert_eq!(zhora_row.segments.len(), 1);
    assert!(
        !zhora_row
            .events
            .iter()
            .any(|e| e.kind == "joined" && e.ts == zhora_row.segments[0].start_ts),
        "Жора has no recorded join event in this fixture — segment must start from his first message, not a fabricated join"
    );
    assert!(
        zhora_row
            .events
            .iter()
            .any(|e| e.kind == "removed" && Some(e.ts) == zhora_row.segments[0].end_ts),
        "a resolved removal event must exist at the segment's end"
    );
}

#[test]
fn merged_accounts_produce_one_continuous_timeline_row() {
    // Жора (только сообщения, без входа, "removed" в конце) сливается в
    // Аню (founder, "joined" на create_group, всё ещё в чате). После
    // merge_people оба набора событий должны оказаться на одной строке
    // Ани, и поскольку Аня всё это время оставалась в чате, покрытие не
    // должно рваться на моменте выхода Жоры — линия непрерывная, хотя
    // событие "removed" остаётся видно в events.
    let mut conn = fresh_db();
    let chat = import(&mut conn, "group_50_members.json");
    let people = people_ordered_by_name(&conn, chat.id);
    let anya = people
        .iter()
        .find(|p| p.display_name.as_deref() == Some("Аня"))
        .unwrap();
    let zhora = people
        .iter()
        .find(|p| p.display_name.as_deref() == Some("Жора"))
        .unwrap();

    db::merge_people(&conn, chat.id, anya.id, &[zhora.id]).unwrap();

    let overview = get_participant_timeline(&conn, chat.id).unwrap();
    let TimelineOverview::Available(summary) = overview else {
        panic!("must still be available after a merge");
    };

    assert_eq!(
        summary.total, 49,
        "Жора is absorbed into Аня, not a separate person anymore"
    );
    assert_eq!(
        summary.left, 0,
        "the merged identity's coverage is continuous and still open — nobody counts as having left"
    );
    assert_eq!(summary.current, 49);

    assert!(
        !summary.people.iter().any(|p| p.person_id == zhora.id),
        "Жора's old id must not appear as its own row after being merged"
    );
    let anya_row = summary
        .people
        .iter()
        .find(|p| p.person_id == anya.id)
        .expect("Аня must still have a timeline row");
    assert_eq!(
        anya_row.segments.len(),
        1,
        "one continuous segment, not a gap at Жора's removal"
    );
    assert!(anya_row.segments[0].end_ts.is_none(), "must still be open");
    assert!(
        anya_row.events.iter().any(|e| e.kind == "joined"),
        "Аня's own join event must still be visible as a marker"
    );
    assert!(
        anya_row.events.iter().any(|e| e.kind == "removed"),
        "Жора's removal event must still be visible as a marker, even though it no longer breaks the line"
    );
}

#[test]
fn personal_chat_has_no_participant_timeline() {
    // personal_chat.json несёt только звонки (`phone_call`/`group_call`)
    // как сервисные события — они не членские и не групповые, поэтому
    // после актуального импортера membership_events/chat_events остаются
    // пустыми для этого чата (docs/SPEC.md §8.7 — в личных чатах нет
    // событий вступления/выхода).
    let mut conn = fresh_db();
    let chat = import(&mut conn, "personal_chat.json");
    let overview = get_participant_timeline(&conn, chat.id).unwrap();
    assert!(matches!(overview, TimelineOverview::NotApplicable));
}

#[test]
fn participant_timeline_reports_needs_reimport_for_a_chat_imported_before_the_timeline_migration() {
    // Тот же приём, что и у reactions_overview_reports_needs_reimport_...:
    // база собрана вручную только по миграциям до появления таймлайна, в
    // неё вставлен "старый" чат, и только после этого накатывается
    // 0011_timeline.sql — imitates a chat that was imported before this
    // feature shipped, whose service events were never persisted at all.
    let conn = Connection::open_in_memory().expect("open in-memory db");
    conn.execute_batch(include_str!("../src/db/migrations/0001_init.sql"))
        .expect("apply migration 0001");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0002_chat_source_size.sql"
    ))
    .expect("apply migration 0002");
    conn.execute_batch(include_str!("../src/db/migrations/0003_people_aliases.sql"))
        .expect("apply migration 0003");
    conn.execute_batch(include_str!("../src/db/migrations/0004_message_types.sql"))
        .expect("apply migration 0004");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0005_dashboard_precompute.sql"
    ))
    .expect("apply migration 0005");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0006_message_type_precompute.sql"
    ))
    .expect("apply migration 0006");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0007_person_activity_span.sql"
    ))
    .expect("apply migration 0007");
    conn.execute_batch(include_str!("../src/db/migrations/0008_reactions.sql"))
        .expect("apply migration 0008");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0009_reaction_actor_emoji.sql"
    ))
    .expect("apply migration 0009");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0010_reaction_favorite_emoji.sql"
    ))
    .expect("apply migration 0010");

    conn.execute(
        "INSERT INTO chats (tg_chat_id, name, chat_type, imported_at, message_count, participant_count) \
         VALUES (1, 'Old chat', 'private_group', 0, 5, 2)",
        [],
    )
    .expect("insert pre-existing chat row");
    let chat_id = conn.last_insert_rowid();

    conn.execute_batch(include_str!("../src/db/migrations/0011_timeline.sql"))
        .expect("apply migration 0011 on top of the pre-existing chat");

    let overview = get_participant_timeline(&conn, chat_id).unwrap();
    assert!(
        matches!(overview, TimelineOverview::NeedsReimport),
        "a chat that never went through the updated importer must ask for a re-import, not show empty/zero data"
    );
}

#[test]
fn reimporting_the_same_chat_does_not_duplicate_membership_and_chat_events() {
    // Баг, найденный на реальном использовании: membership_events/
    // chat_events не каскадируют от messages (нет message_id) и не
    // стирались явно на переимпорт — старые строки оставались лежать под
    // old_id рядом со свежими, задваиваясь на каждый повторный импорт.
    let mut conn = fresh_db();
    import(&mut conn, "group_50_members.json");
    let second = import(&mut conn, "group_50_members.json");

    let membership_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM membership_events WHERE chat_id = ?1",
            params![second.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        membership_count, 11,
        "must not double: founder + 9 invitees + Жора's removal, same as a single import"
    );

    let chat_event_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM chat_events WHERE chat_id = ?1",
            params![second.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        chat_event_count, 3,
        "must not double: created/photo/migrated"
    );
}

#[test]
fn participant_timeline_never_panics_across_the_full_fixture_set() {
    for fixture_name in [
        "three_day_chat.json",
        "group_50_members.json",
        "no_reactions_old_export.json",
        "personal_chat.json",
        "english_chat.json",
    ] {
        let mut conn = fresh_db();
        let chat = import(&mut conn, fixture_name);
        get_participant_timeline(&conn, chat.id).unwrap();
    }
}
