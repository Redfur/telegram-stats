use rusqlite::{params, Connection, OptionalExtension};

use super::granularity::{pick_granularity, strftime_bucket_expr};

/// Оборачивает пересчёт в `BEGIN IMMEDIATE`/`COMMIT`, если соединение
/// сейчас не внутри чужой транзакции (`conn.is_autocommit()`) — иначе
/// (например внутри `import_into_db`, которая уже держит свою `tx`)
/// просто выполняет `f` как есть, второй `BEGIN` на той же связке SQLite
/// не разрешает.
///
/// Нужно из-за пула соединений (`db::build_pool`): раньше единственный
/// `Mutex<Connection>` на всё приложение сериализовал вообще все команды,
/// поэтому DELETE-потом-INSERT внутри пересчёта был безопасен без
/// собственной транзакции — два вызова просто не могли перемежаться. С
/// пулом это больше не так: два конкурентных self-heal одного и того же
/// ещё не посчитанного чата (два разных читающих соединения) могут
/// перемежать свои DELETE/INSERT и словить `UNIQUE constraint failed`.
/// `IMMEDIATE` берёт лок писателя сразу на `BEGIN`, а не лениво на первую
/// запись — конкурирующий вызов блокируется на этом `BEGIN`
/// (`busy_timeout` в `db::build_pool` ждёт, а не сразу отдаёт
/// `SQLITE_BUSY`) и после переиграет DELETE+INSERT поверх уже
/// зафиксированных данных — лишняя работа, но не гонка.
pub(super) fn with_write_lock<T>(
    conn: &Connection,
    f: impl FnOnce() -> rusqlite::Result<T>,
) -> rusqlite::Result<T> {
    if conn.is_autocommit() {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        match f() {
            Ok(value) => {
                conn.execute_batch("COMMIT")?;
                Ok(value)
            }
            Err(err) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    } else {
        f()
    }
}

/// Пересчитывает предвычисленные таблицы дашборда чата с нуля —
/// `chats.word_count`/`char_count`, `activity_buckets`, `person_message_stats`
/// — и выставляет `chats.dashboard_ready=1`. Идемпотентна: сперва чистит
/// старые строки этого `chat_id`, потом заливает свежие, поэтому её можно
/// звать и после первичного импорта (в той же транзакции, что и остальные
/// метаданные чата), и после `merge_people` (переезд `messages.person_id`
/// делает старую агрегацию по участникам недействительной), и лениво из
/// `ensure_dashboard_ready` для чата, импортированного до появления этого
/// среза (self-healing — без явного «переимпортируйте чат»: в отличие от
/// разбора типов сообщений это не новая метрика, а уже рабочая часть
/// дашборда, деградировать её нельзя, docs/SPEC.md). Безопасна к
/// конкурентным вызовам для одного `chat_id` — см. `with_write_lock`.
pub fn recompute_dashboard_metrics(conn: &Connection, chat_id: i64) -> rusqlite::Result<()> {
    with_write_lock(conn, || {
        conn.execute(
            "DELETE FROM activity_buckets WHERE chat_id = ?1",
            params![chat_id],
        )?;
        conn.execute(
            "DELETE FROM person_message_stats WHERE chat_id = ?1",
            params![chat_id],
        )?;

        conn.execute(
            "UPDATE chats SET \
                word_count = (SELECT COALESCE(SUM(word_count), 0) FROM messages WHERE chat_id = ?1), \
                char_count = (SELECT COALESCE(SUM(text_len), 0) FROM messages WHERE chat_id = ?1), \
                dashboard_ready = 1 \
             WHERE id = ?1",
            params![chat_id],
        )?;

        let bounds: Option<(Option<i64>, Option<i64>)> = conn
            .query_row(
                "SELECT first_msg_ts, last_msg_ts FROM chats WHERE id = ?1",
                params![chat_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((Some(first_ts), Some(last_ts))) = bounds {
            let expr = strftime_bucket_expr(pick_granularity(first_ts, last_ts));
            conn.execute(
                &format!(
                    "INSERT INTO activity_buckets (chat_id, bucket, count) \
                     SELECT ?1, strftime('{expr}', ts, 'unixepoch'), COUNT(*) \
                     FROM messages WHERE chat_id = ?1 AND ts IS NOT NULL \
                     GROUP BY strftime('{expr}', ts, 'unixepoch')"
                ),
                params![chat_id],
            )?;
        }

        conn.execute(
            "INSERT INTO person_message_stats (chat_id, person_id, message_count, text_len_sum) \
             SELECT chat_id, person_id, COUNT(*), COALESCE(SUM(text_len), 0) \
             FROM messages WHERE chat_id = ?1 AND person_id IS NOT NULL \
             GROUP BY person_id",
            params![chat_id],
        )?;

        Ok(())
    })
}

/// Ленивый self-heal: чат мог пройти импорт до появления этого среза
/// (`dashboard_ready` тогда 0 по умолчанию из миграции) — досчитываем один
/// раз при первом открытии, а не требуем переимпорт. Несуществующий
/// `chat_id` — не ошибка, просто ничего не делает; вызывающий запрос сам
/// корректно вернёт `None`/пусто.
pub fn ensure_dashboard_ready(conn: &Connection, chat_id: i64) -> rusqlite::Result<()> {
    let ready: Option<i64> = conn
        .query_row(
            "SELECT dashboard_ready FROM chats WHERE id = ?1",
            params![chat_id],
            |row| row.get(0),
        )
        .optional()?;
    if ready == Some(0) {
        recompute_dashboard_metrics(conn, chat_id)?;
    }
    Ok(())
}
