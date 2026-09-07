pub mod db;
pub mod import;
pub mod metrics;

use std::sync::{Arc, Mutex};

use tauri::Manager;

use db::{ChatSummary, Db};
use metrics::{ActivitySeries, ChatTotals, TopParticipants};

#[tauri::command]
async fn import_chat(db: tauri::State<'_, Db>, path: String) -> Result<ChatSummary, String> {
    let conn = db.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        let source_file_size = file.metadata().ok().map(|m| m.len() as i64);
        let mut conn = conn
            .lock()
            .map_err(|_| "database lock poisoned".to_string())?;
        db::import_into_db(
            &mut conn,
            std::io::BufReader::new(file),
            &path,
            source_file_size,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn delete_chat(db: tauri::State<'_, Db>, chat_id: i64) -> Result<(), String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned".to_string())?;
    db::delete_chat(&conn, chat_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_chats(db: tauri::State<'_, Db>) -> Result<Vec<ChatSummary>, String> {
    let conn =
        db.0.lock()
            .map_err(|_| "database lock poisoned".to_string())?;
    db::list_chats(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_chat_totals(db: tauri::State<'_, Db>, chat_id: i64) -> Result<Option<ChatTotals>, String> {
    let conn =
        db.0.lock()
            .map_err(|_| "database lock poisoned".to_string())?;
    metrics::get_chat_totals(&conn, chat_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_activity_series(
    db: tauri::State<'_, Db>,
    chat_id: i64,
) -> Result<Option<ActivitySeries>, String> {
    let conn =
        db.0.lock()
            .map_err(|_| "database lock poisoned".to_string())?;
    metrics::get_activity_series(&conn, chat_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_top_participants(db: tauri::State<'_, Db>, chat_id: i64) -> Result<TopParticipants, String> {
    let conn =
        db.0.lock()
            .map_err(|_| "database lock poisoned".to_string())?;
    metrics::get_top_participants(&conn, chat_id).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_dir)?;
            let conn = db::open_and_migrate(&app_dir.join("telegram-stats.sqlite3"))?;
            app.manage(Db(Arc::new(Mutex::new(conn))));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            import_chat,
            list_chats,
            delete_chat,
            get_chat_totals,
            get_activity_series,
            get_top_participants,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
