mod import;
mod people;

pub use import::{import_into_db, CountingReader, ImportContext, ImportPhase, ImportProgress};
pub use people::{get_people, hide_person, merge_people, rename_person, PersonRow};

use std::path::Path;
use std::time::Duration;

use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;

pub type DbPool = r2d2::Pool<SqliteConnectionManager>;

/// Управляемое состояние Tauri: пул соединений, не одно на всё
/// приложение под мьютексом. SQLite в WAL-режиме допускает сколько
/// угодно параллельных читателей — раньше единственный `Mutex<Connection>`
/// сериализовал вообще все команды, поэтому один тяжёлый self-heal
/// пересчёт метрик (`metrics::precompute`) для одного чата блокировал
/// запросы для всех остальных, пока не закончится: именно так ловился
/// полный фриз интерфейса при быстром переключении между чатами.
/// `busy_timeout` в `build_pool` — на случай редкой гонки двух пишущих
/// транзакций (например self-heal одного и того же ещё не посчитанного
/// чата, запущенный из двух конкурентных команд) — SQLite и так admits
/// только одного писателя разом, таймаут просто ждёт вместо мгновенной
/// ошибки "database is locked".
pub struct Db(pub DbPool);

/// Собирает пул поверх уже смигрированного файла БД (`open_and_migrate`
/// применяет миграции один раз на отдельном соединении до вызова этой
/// функции). `max_size` — с запасом на самый частый всплеск: открытие
/// одного чата бьёт до пяти команд дашборда почти одновременно.
pub fn build_pool(path: &Path) -> Result<DbPool, r2d2::Error> {
    let manager = SqliteConnectionManager::file(path).with_init(|conn| {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        Ok(())
    });
    r2d2::Pool::builder().max_size(8).build(manager)
}

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
/// (см. CLAUDE.md). Следующая миграция — такой же блок
/// `if version < N { ...; user_version = N }`.
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
    if version < 4 {
        conn.execute_batch(include_str!("migrations/0004_message_types.sql"))?;
        conn.pragma_update(None, "user_version", 4)?;
    }
    if version < 5 {
        conn.execute_batch(include_str!("migrations/0005_dashboard_precompute.sql"))?;
        conn.pragma_update(None, "user_version", 5)?;
    }
    if version < 6 {
        conn.execute_batch(include_str!("migrations/0006_message_type_precompute.sql"))?;
        conn.pragma_update(None, "user_version", 6)?;
    }
    if version < 7 {
        conn.execute_batch(include_str!("migrations/0007_person_activity_span.sql"))?;
        conn.pragma_update(None, "user_version", 7)?;
    }
    if version < 8 {
        conn.execute_batch(include_str!("migrations/0008_reactions.sql"))?;
        conn.pragma_update(None, "user_version", 8)?;
    }
    if version < 9 {
        conn.execute_batch(include_str!("migrations/0009_reaction_actor_emoji.sql"))?;
        conn.pragma_update(None, "user_version", 9)?;
    }
    if version < 10 {
        conn.execute_batch(include_str!("migrations/0010_reaction_favorite_emoji.sql"))?;
        conn.pragma_update(None, "user_version", 10)?;
    }
    if version < 11 {
        conn.execute_batch(include_str!("migrations/0011_timeline.sql"))?;
        conn.pragma_update(None, "user_version", 11)?;
    }
    if version < 12 {
        conn.execute_batch(include_str!("migrations/0012_timeline_indexes.sql"))?;
        conn.pragma_update(None, "user_version", 12)?;
    }
    if version < 13 {
        conn.execute_batch(include_str!("migrations/0013_person_fk_indexes.sql"))?;
        conn.pragma_update(None, "user_version", 13)?;
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
