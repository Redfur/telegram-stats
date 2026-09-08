pub mod db;
pub mod import;
pub mod metrics;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{Emitter, Manager};

use db::{ChatSummary, CountingReader, Db, ImportContext, ImportPhase, ImportProgress, PersonRow};
use import::ImportError;
use metrics::{
    ActivitySeries, ChatTotals, MessageTypeDetail, MessageTypeKind, MessageTypeOverview,
    ReactionsOverview, TopParticipants, TopReactedMessage, TypeTimeView,
};

/// Единственный текущий импорт можно отменить — конкурентных импортов
/// не поддерживаем (это ограничение UI/UX, не пула соединений — тот сам
/// по себе спокойно выдержал бы параллельный импорт), поэтому один
/// разделяемый флаг на всё приложение достаточен.
struct ImportCancelFlag(Arc<AtomicBool>);

/// Как часто реально шлём `ingest://progress` в JS — на каждое сообщение
/// слать неразумно (миллионы IPC-вызовов на большом чате), считаем
/// прогресс на каждое, эмитим — не чаще раза в ~150 мс.
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(150);

#[derive(serde::Serialize)]
struct ImportErrorInfo {
    kind: &'static str,
    message: String,
}

fn classify_import_error(err: &ImportError) -> ImportErrorInfo {
    let kind = match err {
        ImportError::NotTelegramExport => "not_telegram_export",
        ImportError::Cancelled => "cancelled",
        ImportError::Io(_) => "io",
        ImportError::Json {
            index: None,
            source,
        } => {
            use serde_json::error::Category;
            match source.classify() {
                Category::Syntax | Category::Eof => "invalid_json",
                Category::Data | Category::Io => "not_telegram_export",
            }
        }
        ImportError::Json { index: Some(_), .. } | ImportError::MissingField { .. } => {
            "message_error"
        }
        ImportError::Sink(_) => "io",
    };
    ImportErrorInfo {
        kind,
        message: err.to_string(),
    }
}

fn import_error_to_string(err: ImportError) -> String {
    serde_json::to_string(&classify_import_error(&err)).unwrap_or_else(|_| err.to_string())
}

#[tauri::command]
async fn import_chat(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    cancel: tauri::State<'_, ImportCancelFlag>,
    path: String,
) -> Result<ChatSummary, String> {
    cancel.0.store(false, Ordering::Relaxed);
    let pool = db.0.clone();
    let cancel_flag = cancel.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        let source_file_size = file.metadata().ok().map(|m| m.len() as i64);
        let (counting, bytes_read) = CountingReader::new(std::io::BufReader::new(file));

        let mut last_emit = Instant::now();
        let on_progress = |p: ImportProgress| {
            // Смена фазы шлётся всегда, throttle её не съедает — иначе
            // фронт мог не узнать, что чтение файла кончилось и дальше
            // идёт время без измеримого процента (docs/PLAN.md).
            let force = p.phase == ImportPhase::Finalizing;
            if force || last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
                let _ = app.emit("ingest://progress", p);
                last_emit = Instant::now();
            }
        };

        let mut conn = pool.get().map_err(|e| e.to_string())?;
        let mut ctx = ImportContext {
            bytes_read: &bytes_read,
            on_progress,
            cancel: &cancel_flag,
        };
        db::import_into_db(&mut conn, counting, &path, source_file_size, &mut ctx)
            .map_err(import_error_to_string)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cancel_import(cancel: tauri::State<'_, ImportCancelFlag>) {
    cancel.0.store(true, Ordering::Relaxed);
}

#[tauri::command]
fn list_chats(db: tauri::State<'_, Db>) -> Result<Vec<ChatSummary>, String> {
    let conn = db.0.get().map_err(|e| e.to_string())?;
    db::list_chats(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_chat(db: tauri::State<'_, Db>, chat_id: i64) -> Result<(), String> {
    let conn = db.0.get().map_err(|e| e.to_string())?;
    db::delete_chat(&conn, chat_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_people(db: tauri::State<'_, Db>, chat_id: i64) -> Result<Vec<PersonRow>, String> {
    let conn = db.0.get().map_err(|e| e.to_string())?;
    db::get_people(&conn, chat_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn merge_people(
    db: tauri::State<'_, Db>,
    chat_id: i64,
    primary_id: i64,
    other_ids: Vec<i64>,
) -> Result<(), String> {
    let conn = db.0.get().map_err(|e| e.to_string())?;
    db::merge_people(&conn, chat_id, primary_id, &other_ids).map_err(|e| e.to_string())
}

#[tauri::command]
fn hide_person(
    db: tauri::State<'_, Db>,
    chat_id: i64,
    person_id: i64,
    hidden: bool,
) -> Result<(), String> {
    let conn = db.0.get().map_err(|e| e.to_string())?;
    db::hide_person(&conn, chat_id, person_id, hidden).map_err(|e| e.to_string())
}

#[tauri::command]
fn rename_person(
    db: tauri::State<'_, Db>,
    chat_id: i64,
    person_id: i64,
    display_name: String,
) -> Result<(), String> {
    let conn = db.0.get().map_err(|e| e.to_string())?;
    db::rename_person(&conn, chat_id, person_id, &display_name).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_chat_totals(db: tauri::State<'_, Db>, chat_id: i64) -> Result<Option<ChatTotals>, String> {
    let conn = db.0.get().map_err(|e| e.to_string())?;
    metrics::get_chat_totals(&conn, chat_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_activity_series(
    db: tauri::State<'_, Db>,
    chat_id: i64,
) -> Result<Option<ActivitySeries>, String> {
    let conn = db.0.get().map_err(|e| e.to_string())?;
    metrics::get_activity_series(&conn, chat_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_top_participants(db: tauri::State<'_, Db>, chat_id: i64) -> Result<TopParticipants, String> {
    let conn = db.0.get().map_err(|e| e.to_string())?;
    metrics::get_top_participants(&conn, chat_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_message_type_overview(
    db: tauri::State<'_, Db>,
    chat_id: i64,
) -> Result<MessageTypeOverview, String> {
    let conn = db.0.get().map_err(|e| e.to_string())?;
    metrics::get_message_type_overview(&conn, chat_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_message_type_detail(
    db: tauri::State<'_, Db>,
    chat_id: i64,
    kind: MessageTypeKind,
    view: Option<TypeTimeView>,
) -> Result<Option<MessageTypeDetail>, String> {
    let conn = db.0.get().map_err(|e| e.to_string())?;
    metrics::get_message_type_detail(&conn, chat_id, kind, view).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_reactions_overview(
    db: tauri::State<'_, Db>,
    chat_id: i64,
) -> Result<ReactionsOverview, String> {
    let conn = db.0.get().map_err(|e| e.to_string())?;
    metrics::get_reactions_overview(&conn, chat_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_top_reacted_messages(
    db: tauri::State<'_, Db>,
    chat_id: i64,
) -> Result<Vec<TopReactedMessage>, String> {
    let conn = db.0.get().map_err(|e| e.to_string())?;
    metrics::get_top_reacted_messages(&conn, chat_id).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_dir)?;
            let db_path = app_dir.join("telegram-stats.sqlite3");
            // Миграции — один раз на отдельном соединении, до открытия
            // пула: PRAGMA user_version читается-пишется без гонки
            // нескольких соединений на пустой базе.
            db::open_and_migrate(&db_path)?;
            let pool = db::build_pool(&db_path)?;
            app.manage(Db(pool));
            app.manage(ImportCancelFlag(Arc::new(AtomicBool::new(false))));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            import_chat,
            cancel_import,
            list_chats,
            delete_chat,
            get_people,
            merge_people,
            hide_person,
            rename_person,
            get_chat_totals,
            get_activity_series,
            get_top_participants,
            get_message_type_overview,
            get_message_type_detail,
            get_reactions_overview,
            get_top_reacted_messages,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
