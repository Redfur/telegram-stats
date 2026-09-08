use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use telegram_stats_lib::db;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Реальный файл на диске, не `:memory:` — пул (`db::build_pool`) открывает
/// несколько соединений к одному и тому же файлу, in-memory-соединения
/// его не разделяют между собой.
fn temp_db_path() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("telegram_stats_pool_test_{pid}_{n}.sqlite3"))
}

fn cleanup(path: &PathBuf) {
    std::fs::remove_file(path).ok();
    std::fs::remove_file(format!("{}-wal", path.display())).ok();
    std::fs::remove_file(format!("{}-shm", path.display())).ok();
}

/// Раньше `Db` был единственным `Mutex<Connection>` на всё приложение —
/// любая команда сериализовалась за ним, а self-heal пересчёт метрик
/// (`metrics::precompute`) для одного чата блокировал вообще все запросы.
/// Пул допускает параллельные читающие соединения (WAL); эта проверка
/// бьёт self-heal одного и того же ещё не посчитанного чата сразу с
/// нескольких потоков/соединений — конкурентные пишущие транзакции
/// внутри self-heal не должны ни падать (`busy_timeout` в
/// `db::build_pool`), ни давать рассогласованный результат.
#[test]
fn pool_serves_concurrent_self_heal_without_deadlock_or_corruption() {
    let path = temp_db_path();
    db::open_and_migrate(&path).expect("migrate");
    let pool = db::build_pool(&path).expect("build pool");

    let chat_id: i64 = {
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO chats (tg_chat_id, name, chat_type, imported_at, first_msg_ts, \
             last_msg_ts, message_count, participant_count) \
             VALUES (1, 'Concurrent', 'private_group', 0, 1000, 100000, 3, 1)",
            [],
        )
        .unwrap();
        let chat_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO people (chat_id, tg_person_id, display_name) VALUES (?1, 'user1', 'Alice')",
            [chat_id],
        )
        .unwrap();
        let person_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO messages (chat_id, tg_msg_id, person_id, ts, text_len, word_count) \
             VALUES (?1, 1, ?2, 1000, 5, 1), (?1, 2, ?2, 50000, 5, 1), (?1, 3, ?2, 100000, 5, 1)",
            rusqlite::params![chat_id, person_id],
        )
        .unwrap();
        chat_id
    };

    let handles: Vec<_> = (0..6)
        .map(|_| {
            let pool = pool.clone();
            thread::spawn(move || {
                let conn = pool.get().expect("pool.get must not deadlock");
                telegram_stats_lib::metrics::get_chat_totals(&conn, chat_id)
                    .expect("self-heal under concurrent access must not error")
            })
        })
        .collect();

    for totals in handles.into_iter().filter_map(|h| h.join().unwrap()) {
        assert_eq!(totals.message_count, 3);
        assert_eq!(
            totals.word_count, 3,
            "self-heal must not double-count under concurrent access"
        );
        assert_eq!(totals.char_count, 15);
    }

    let ready: i64 = pool
        .get()
        .unwrap()
        .query_row(
            "SELECT dashboard_ready FROM chats WHERE id = ?1",
            [chat_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(ready, 1);

    cleanup(&path);
}
