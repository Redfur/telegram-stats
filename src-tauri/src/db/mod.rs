mod import;

pub use import::import_into_db;

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

/// Управляемое состояние Tauri: одно соединение на всё приложение,
/// защищённое мьютексом (нужно для перемещения в `spawn_blocking`).
pub struct Db(pub Arc<Mutex<Connection>>);

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatSummary {
    pub id: i64,
    pub tg_chat_id: Option<i64>,
    pub name: Option<String>,
    pub chat_type: String,
    pub message_count: i64,
    pub participant_count: i64,
    pub first_msg_ts: Option<i64>,
    pub last_msg_ts: Option<i64>,
    pub imported_at: i64,
}

/// Открывает (создавая при необходимости) SQLite-файл и применяет
/// миграции по `PRAGMA user_version` — вперёд-только, пронумерованные
/// (см. CLAUDE.md). Пока только `0001_init.sql`; следующая миграция —
/// такой же блок `if version < N { ...; user_version = N }`.
pub fn open_and_migrate(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 1 {
        conn.execute_batch(include_str!("migrations/0001_init.sql"))?;
        conn.pragma_update(None, "user_version", 1)?;
    }

    Ok(conn)
}

pub fn list_chats(conn: &Connection) -> rusqlite::Result<Vec<ChatSummary>> {
    let mut stmt = conn.prepare(
        "SELECT id, tg_chat_id, name, chat_type, message_count, participant_count, \
         first_msg_ts, last_msg_ts, imported_at \
         FROM chats ORDER BY imported_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ChatSummary {
            id: row.get(0)?,
            tg_chat_id: row.get(1)?,
            name: row.get(2)?,
            chat_type: row.get(3)?,
            message_count: row.get(4)?,
            participant_count: row.get(5)?,
            first_msg_ts: row.get(6)?,
            last_msg_ts: row.get(7)?,
            imported_at: row.get(8)?,
        })
    })?;
    rows.collect()
}
