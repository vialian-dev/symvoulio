// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
mod commands;
mod pty;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(pty::PtyManager::default())
        .setup(|app| {
            app.manage(commands::AppState::initialize(app).map_err(std::io::Error::other)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            #[cfg(debug_assertions)]
            pty::pty_debug_log,
            pty::pty_open,
            pty::pty_write,
            pty::pty_paste,
            pty::pty_resize,
            pty::pty_close,
            pty::shell_info,
            commands::settings_get,
            commands::settings_set,
            commands::detect_providers,
            commands::stage_file,
            commands::stage_discard,
            commands::run_start,
            commands::run_restart,
            commands::run_subscribe,
            commands::run_pause,
            commands::run_resume,
            commands::run_stop,
            commands::run_decide,
            commands::run_decide_issue,
            commands::raw_output_set,
            commands::run_get,
            commands::runs_list,
            commands::run_delete,
            commands::run_export,
            commands::open_folder,
            commands::app_info
        ])
        .build(tauri::generate_context!())
        .expect("error while building Symvoulio")
        .run(|app, event| {
            if matches!(
                event,
                tauri::RunEvent::Exit
                    | tauri::RunEvent::WindowEvent {
                        event: tauri::WindowEvent::Destroyed,
                        ..
                    }
            ) {
                app.state::<commands::AppState>().stop_all();
                app.state::<pty::PtyManager>().close_all();
            }
        });
}
