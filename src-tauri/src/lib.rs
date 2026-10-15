mod api;
mod audio;
mod commands;
pub mod engine;
pub mod models;
mod store;

use std::path::PathBuf;

use tauri::Manager;

use commands::AppState;
use engine::ffi::EngineLayout;
use engine::EngineHandle;
use store::Store;

/// Finds the engine directory: bundled resources in release builds, the staged
/// build output during development, or an explicit override for packagers.
fn engine_root(app: &tauri::App) -> PathBuf {
    if let Some(dir) = std::env::var_os("MURMUR_ENGINE_DIR") {
        return PathBuf::from(dir);
    }
    let bundled = app
        .path()
        .resource_dir()
        .map(|dir| dir.join("engine"))
        .ok()
        .filter(|dir| dir.join("lib").exists());
    bundled.unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/engine"))
}

extern "C" {
    #[link_name = "_exit"]
    fn libc_exit(code: i32) -> !;
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let store = Store::new(data_dir)?;
            let layout = EngineLayout::from_root(&engine_root(app));
            log::info!("engine library: {}", layout.library.display());

            let handle = app.handle().clone();
            let engine = EngineHandle::spawn(layout, move |event| {
                commands::forward_engine_event(&handle, event);
            });
            let state = AppState::new(engine, store);

            // Restore the last used model so the app is ready immediately.
            let (restore, api_enabled, api_lan, api_port) = {
                let s = state.settings.lock();
                (s.active_model.clone(), s.api_enabled, s.api_lan, s.api_port)
            };
            app.manage(state);
            app.state::<AppState>()
                .api
                .configure(app.handle(), api_enabled, api_lan, api_port);
            if let Some(id) = restore {
                let state = app.state::<AppState>();
                if let Err(e) = commands::load_model(state, id) {
                    log::warn!("could not restore model: {e}");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_settings,
            commands::update_settings,
            commands::api_status,
            commands::list_models,
            commands::download_model,
            commands::cancel_download,
            commands::delete_model,
            commands::load_model,
            commands::transcribe_file,
            commands::cancel_job,
            commands::start_live,
            commands::stop_live,
            commands::list_input_devices,
            commands::list_history,
            commands::get_transcript,
            commands::update_transcript,
            commands::delete_transcript,
            commands::export_transcript,
            commands::export_files,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Murmur")
        .run(|app, event| {
            // Cmd+Q / Quit ends in exit(), which runs ggml's static Metal teardown.
            // That aborts if model buffers are still alive, so release them first.
            if let tauri::RunEvent::Exit = event {
                let state = app.state::<AppState>();
                state.api.shutdown(app);
                if !state.engine.shutdown(std::time::Duration::from_secs(3)) {
                    log::warn!("engine did not stop in time; exiting without static teardown");
                    // SAFETY: terminates immediately; skips destructors that would abort.
                    unsafe { libc_exit(0) };
                }
            }
        });
}
