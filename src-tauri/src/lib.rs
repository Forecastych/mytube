mod commands;
mod db;
mod error;
mod feed;
mod google;
mod net;
mod player;
mod progress;
mod youtube;

use std::sync::Mutex;

use tauri::Manager;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol("stream", |ctx, req, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                responder.respond(player::proxy::handle(app, req).await);
            });
        })
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let conn = db::open(&dir.join("mytube.db"))?;
            app.manage(AppState { db: Mutex::new(conn), http: net::client(), player: Default::default() });
            app.manage(google::GoogleState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::favorites_list,
            commands::favorite_add,
            commands::favorite_remove,
            commands::favorites_feed,
            commands::watch_info,
            commands::stream_index,
            commands::sponsor_segments,
            commands::open_in_mpv,
            commands::mark_watched,
            commands::ytdlp_version,
            commands::ytdlp_update,
            commands::comments,
            commands::comments_more,
            progress::save_progress,
            progress::progress_list,
            youtube::search::search,
            youtube::related::related,
            youtube::channel_page::channel_page,
            feed::commands::algorithm_get,
            feed::commands::algorithm_set,
            feed::commands::blocked_channel_resolve,
            feed::commands::home_feed,
            google::commands::google_status,
            google::commands::google_set_client,
            google::commands::google_sign_in,
            google::commands::google_sign_in_cancel,
            google::commands::google_sign_out,
            google::commands::google_forget_client,
            google::commands::subscriptions_sync,
            google::commands::trending,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
