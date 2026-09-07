mod import;
mod people;

pub use import::{import_into_db, CountingReader, ImportContext, ImportProgress};
pub use people::{get_people, hide_person, merge_people, rename_person, PersonRow};

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
    pub source_file_size: Option<i64>,
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
    if version < 2 {
        conn.execute_batch(include_str!("migrations/0002_chat_source_size.sql"))?;
        conn.pragma_update(None, "user_version", 2)?;
    }
    if version < 3 {
        conn.execute_batch(include_str!("migrations/0003_people_aliases.sql"))?;
        conn.pragma_update(None, "user_version", 3)?;
    }

    Ok(conn)
}

/// `participant_count` — вживую по `people` (не денормализованная
/// колонка, которая не видит merge/hide из разбора участников, макет 3d).
pub fn list_chats(conn: &Connection) -> rusqlite::Result<Vec<ChatSummary>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.tg_chat_id, c.name, c.chat_type, c.message_count, \
                (SELECT COUNT(*) FROM people p WHERE p.chat_id = c.id AND p.merged_into IS NULL AND p.is_hidden = 0), \
                c.first_msg_ts, c.last_msg_ts, c.imported_at, c.source_file_size \
         FROM chats c ORDER BY c.imported_at DESC",
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
            source_file_size: row.get(9)?,
        })
    })?;
    rows.collect()
}

/// Удаляет чат и всё, что от него зависит (`messages`/`people` —
/// каскадом через `ON DELETE CASCADE`, см. миграцию `0001`). Фильтр по
/// `id` — единственный чат, ничего больше затронуто не будет.
pub fn delete_chat(conn: &Connection, chat_id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM chats WHERE id = ?1", [chat_id])?;
    Ok(())
}
