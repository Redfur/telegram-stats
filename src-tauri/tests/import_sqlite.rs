use std::fs::File;
use std::path::PathBuf;

use rusqlite::Connection;
use telegram_stats_lib::db::{self, import_into_db};
use telegram_stats_lib::import::{parse_export, ParsedEvent};
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
    conn
}

fn import(conn: &mut Connection, name: &str) -> db::ChatSummary {
    let file = File::open(fixture(name)).expect("fixture file should exist");
    import_into_db(conn, file, name).expect("import should succeed")
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
