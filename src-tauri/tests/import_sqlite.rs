use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};

use rusqlite::Connection;
use telegram_stats_lib::db::{self, import_into_db, ImportContext};
use telegram_stats_lib::import::{parse_export, ImportError, ParsedEvent};
use telegram_stats_lib::metrics::{
    get_message_type_detail, get_message_type_overview, get_top_participants,
    granularity::pick_granularity, MessageTypeKind, MessageTypeSummaryStats, TypeTimeView,
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
    conn.execute_batch(include_str!(
        "../src/db/migrations/0003_people_aliases.sql"
    ))
    .expect("apply migration 0003");
    conn.execute_batch(include_str!(
        "../src/db/migrations/0004_message_types.sql"
    ))
    .expect("apply migration 0004");
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
    assert_eq!(second.source_file_size, Some(2000), "must reflect the newest import, not the first");

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
    let result = import_into_db(&mut conn, not_an_export, "not_an_export.json", None, &mut ctx);

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
    assert_eq!(merged_primary.message_count, primary_before + duplicate_before);

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
    let primary_after = visible_after_reimport.iter().find(|p| p.id == primary.id).unwrap();
    assert_eq!(
        primary_after.message_count, combined_before_reimport,
        "re-imported messages for the merged-away duplicate must land on the primary, not vanish from every metric"
    );

    let totals = get_top_participants(&conn, second.id).unwrap();
    assert!(
        totals.participants.iter().all(|p| p.person_id != hidden_one.id),
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
    assert_eq!(aliases, vec!["New Name".to_string(), "Old Name".to_string()]);
}

#[test]
fn message_type_overview_counts_every_kind_in_english_chat() {
    let mut conn = fresh_db();
    let chat = import(&mut conn, "english_chat.json");

    let overview = get_message_type_overview(&conn, chat.id).unwrap();
    assert!(overview.available);
    assert_eq!(overview.total_messages, chat.message_count);
    assert_eq!(overview.counts.len(), MessageTypeKind::ALL.len());
    // Фикстура специально устроена так, что у каждого из 12 типов ровно
    // одно вхождение — легко проверить, что ни один не потерялся и не
    // задвоился в другой категории (например файл vs документ-без-медиатипа).
    for c in &overview.counts {
        assert_eq!(c.count, 1, "kind {:?} should have exactly one match", c.kind);
    }
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
        if matches!(kind, MessageTypeKind::VoiceMessage | MessageTypeKind::VideoMessage) {
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
    assert!(detail.is_none(), "a kind with zero matches must return None, not an empty/zeroed detail");
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
    assert_eq!(detail.selected_view, TypeTimeView::Day, "must fall back to the coarsest available view");
    match detail.summary {
        MessageTypeSummaryStats::Frequency { peak, .. } => {
            assert!(peak.is_none(), "too few buckets for a meaningful peak — must degrade, not lie");
        }
        MessageTypeSummaryStats::Duration { .. } => panic!("edited is not duration-based"),
    }
}

#[test]
fn requesting_an_unavailable_view_falls_back_instead_of_erroring() {
    let mut conn = fresh_db();
    let chat = import(&mut conn, "three_day_chat.json");

    // Явно просим "по месяцам" на чате, где он недоступен — не должно падать.
    let detail = get_message_type_detail(&conn, chat.id, MessageTypeKind::Edited, Some(TypeTimeView::Month))
        .unwrap()
        .expect("fixture has exactly one edited message");
    assert_eq!(detail.selected_view, TypeTimeView::Day);
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
    conn.execute_batch(include_str!(
        "../src/db/migrations/0003_people_aliases.sql"
    ))
    .expect("apply migration 0003");

    conn.execute(
        "INSERT INTO chats (tg_chat_id, name, chat_type, imported_at, message_count, participant_count) \
         VALUES (1, 'Old chat', 'private_group', 0, 5, 2)",
        [],
    )
    .expect("insert pre-existing chat row");
    let chat_id = conn.last_insert_rowid();

    conn.execute_batch(include_str!(
        "../src/db/migrations/0004_message_types.sql"
    ))
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
        assert!(overview.available, "{fixture_name}: freshly imported chats are always ready");

        for kind in MessageTypeKind::ALL {
            let detail = get_message_type_detail(&conn, chat.id, kind, None).unwrap();
            if let Some(detail) = detail {
                assert!(detail.total_count > 0, "{fixture_name}/{kind:?}: Some(..) implies a non-zero count");
                for view in [TypeTimeView::Month, TypeTimeView::Day, TypeTimeView::HourOfDay] {
                    // Явный запрос любого из трёх видов не должен падать,
                    // даже если он недоступен для этого чата (тихий фолбэк).
                    get_message_type_detail(&conn, chat.id, kind, Some(view)).unwrap();
                }
            }
        }
    }
}
