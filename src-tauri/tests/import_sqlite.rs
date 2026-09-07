use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};

use rusqlite::Connection;
use telegram_stats_lib::db::{self, import_into_db, ImportContext};
use telegram_stats_lib::import::{parse_export, ImportError, ParsedEvent};
use telegram_stats_lib::metrics::{get_top_participants, granularity::pick_granularity};

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
