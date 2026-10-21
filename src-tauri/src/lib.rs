mod commands;
mod db;
mod error;
mod net;
mod youtube;

use std::sync::Mutex;

use tauri::Manager;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let conn = db::open(&dir.join("mytube.db"))?;
            app.manage(AppState { db: Mutex::new(conn), http: net::client() });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::favorites_list,
            commands::favorite_add,
            commands::favorite_remove,
            commands::favorites_feed,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
