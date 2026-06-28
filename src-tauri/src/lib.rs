mod audio;
mod commands;
mod db;
mod enhance;
mod export;
mod models;
mod paths;
mod state;
mod text;
mod whisper;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::ShortcutState;

/// Bring the main window back to the foreground and hide the pill.
fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    if let Some(o) = app.get_webview_window("overlay") {
        let _ = o.hide();
    }
}

/// Bring the main window back maximized (from the pill's right-click menu).
fn maximize_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.maximize();
        let _ = w.set_focus();
    }
    if let Some(o) = app.get_webview_window("overlay") {
        let _ = o.hide();
    }
}

/// Hide the main window into the floating pill (shared by close + minimize).
fn hide_to_pill(window: &tauri::Window) {
    let _ = window.hide();
    if let Some(o) = window.app_handle().get_webview_window("overlay") {
        let _ = o.show();
    }
}

/// Flip between the main window and the floating pill (the configurable global
/// shortcut). Visible main folds into the pill; a hidden main comes back.
fn toggle_main_pill(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
            if let Some(o) = app.get_webview_window("overlay") {
                let _ = o.show();
            }
        } else {
            show_main(app);
        }
    }
}

use crate::db::Db;
use crate::paths::AppPaths;
use crate::state::AppState;

/// Build the tray context menu, ticking whichever output mode is active. Rebuilt
/// (not mutated) on every change so the checkmarks always reflect the setting.
fn build_tray_menu(app: &tauri::AppHandle, mode: &str) -> tauri::Result<Menu<tauri::Wry>> {
    use tauri::menu::{CheckMenuItem, PredefinedMenuItem};
    let show = MenuItem::with_id(app, "show", "Open EchoFlow", true, None::<&str>)?;
    let raw = CheckMenuItem::with_id(app, "mode_raw", "Raw text", true, mode == "raw", None::<&str>)?;
    let polish =
        CheckMenuItem::with_id(app, "mode_polish", "Clean writing", true, mode == "polish", None::<&str>)?;
    let prompt =
        CheckMenuItem::with_id(app, "mode_prompt", "Prompt mode", true, mode == "prompt", None::<&str>)?;
    let translate = CheckMenuItem::with_id(
        app,
        "mode_translate",
        "Translate to English",
        true,
        mode == "translate",
        None::<&str>,
    )?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    Menu::with_items(app, &[&show, &sep1, &raw, &polish, &prompt, &translate, &sep2, &quit])
}

/// Rebuild the tray menu so its mode checkmarks match the current setting. Safe
/// to call from anywhere (tray click, UI settings save).
pub fn refresh_tray_menu(app: &tauri::AppHandle) {
    let mode = app.state::<AppState>().settings.read().output_mode.clone();
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(menu) = build_tray_menu(app, &mode) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

/// Apply an output mode chosen from the tray: persist it and tell the UI so the
/// header switcher stays in sync. Translate always targets English from the tray.
fn apply_output_mode(app: &tauri::AppHandle, mode: &str) {
    let state = app.state::<AppState>();
    let settings = {
        let mut s = state.settings.write();
        s.output_mode = mode.to_string();
        if mode == "translate" {
            s.translate_target = "English".to_string();
        }
        s.clone()
    };
    let _ = state.db.save_settings(&settings);
    refresh_tray_menu(app);
    let _ = app.emit("settings-changed", settings);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init()
        .ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    let (mode, recording, ptt_key, toggle_key) = {
                        let state = app.state::<AppState>();
                        let s = state.settings.read();
                        (
                            s.capture_mode.clone(),
                            state.is_recording(),
                            s.ptt_hotkey.clone(),
                            s.toggle_hotkey.clone(),
                        )
                    };
                    // Both shortcuts arrive through this one handler; match the
                    // fired accelerator against the configured strings to dispatch.
                    let fired = |key: &str| {
                        key.parse::<tauri_plugin_global_shortcut::Shortcut>()
                            .map(|sc| &sc == shortcut)
                            .unwrap_or(false)
                    };

                    // Window <-> pill toggle: act once, on press.
                    if fired(&toggle_key) {
                        if event.state() == ShortcutState::Pressed {
                            toggle_main_pill(app);
                        }
                        return;
                    }

                    // Push-to-talk / toggle-record.
                    if fired(&ptt_key) {
                        match event.state() {
                            ShortcutState::Pressed => {
                                if mode == "toggle" {
                                    if recording {
                                        stop_and_notify(app);
                                    } else {
                                        start_and_notify(app);
                                    }
                                } else if !recording {
                                    // hold / push-to-talk: press starts
                                    start_and_notify(app);
                                }
                            }
                            ShortcutState::Released => {
                                if mode == "hold" && recording {
                                    stop_and_notify(app);
                                }
                            }
                        }
                    }
                })
                .build(),
        )
        // Main window: the X button and the OS minimize button both fold the app
        // into the floating pill (and tray) instead of closing or sitting on the
        // taskbar. The pill, the tray, or "Maximize" bring it back.
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    hide_to_pill(window);
                }
                tauri::WindowEvent::Resized(_) => {
                    if let Ok(true) = window.is_minimized() {
                        // Clear the minimized state so a later show() restores cleanly,
                        // then hide off the taskbar and pop the pill.
                        let _ = window.unminimize();
                        hide_to_pill(window);
                    }
                }
                _ => {}
            }
        })
        // Pill right-click menu (ids set in commands::show_pill_menu). The
        // pill_mode_* items switch output mode and reuse the same apply path as
        // the tray, so the header switcher and tray ticks stay in sync.
        .on_menu_event(|app, event| match event.id.as_ref() {
            "pill_open" => show_main(app),
            "pill_max" => maximize_main(app),
            "pill_quit" => app.exit(0),
            "pill_mode_raw" => apply_output_mode(app, "raw"),
            "pill_mode_polish" => apply_output_mode(app, "polish"),
            "pill_mode_prompt" => apply_output_mode(app, "prompt"),
            "pill_mode_translate" => apply_output_mode(app, "translate"),
            _ => {}
        })
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("voice-app"));
            let paths = AppPaths::new(data_dir).map_err(|e| e.to_string())?;
            let db = Db::open(&paths.db_path).map_err(|e| e.to_string())?;
            let settings = db.load_settings();
            let hotkey = settings.ptt_hotkey.clone();
            let toggle_hotkey = settings.toggle_hotkey.clone();
            let retention = settings.retention_days;

            app.manage(AppState::new(paths, db, settings));
            let _ = commands::register_shortcuts(app.handle(), &hotkey, &toggle_hotkey);

            // Enforce the retention policy on launch: drop old recordings + audio
            // so storage never grows without bound.
            {
                let st = app.state::<AppState>();
                if let Ok(paths) = st.db.purge_older_than(retention) {
                    for p in paths {
                        let _ = std::fs::remove_file(p);
                    }
                }
            }

            // Park the floating overlay pill at bottom-center of the primary screen.
            if let Some(overlay) = app.get_webview_window("overlay") {
                if let Ok(Some(mon)) = overlay.primary_monitor() {
                    let sf = mon.scale_factor();
                    let sz = mon.size();
                    let sw = sz.width as f64 / sf;
                    let sh = sz.height as f64 / sf;
                    let _ = overlay
                        .set_position(tauri::LogicalPosition::new((sw - 150.0) / 2.0, sh - 96.0));
                }
            }

            // Tray icon: the app keeps living here when minimized to the pill,
            // so it stays off the taskbar but is one click away. Right-click also
            // switches output mode (raw / clean / prompt / translate) without
            // opening the window.
            let mode = app.state::<AppState>().settings.read().output_mode.clone();
            let menu = build_tray_menu(app.handle(), &mode)?;
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("EchoFlow")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => app.exit(0),
                    "mode_raw" => apply_output_mode(app, "raw"),
                    "mode_polish" => apply_output_mode(app, "polish"),
                    "mode_prompt" => apply_output_mode(app, "prompt"),
                    "mode_translate" => apply_output_mode(app, "translate"),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon().cloned() {
                tray = tray.icon(icon);
            }
            let _ = tray.build(app)?;

            // Warm up the model off the UI thread so the first dictation is fast.
            let warm = app.handle().clone();
            std::thread::spawn(move || warm.state::<AppState>().warmup());

            // Idle monitor: free the model from RAM after the configured idle
            // window so the app stays light when it's not in use.
            let idle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(60));
                let st = idle.state::<AppState>();
                let mins = st.settings.read().idle_unload_minutes;
                if mins > 0 {
                    st.maybe_unload_idle(std::time::Duration::from_secs((mins as u64) * 60));
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_input_devices,
            commands::is_recording,
            commands::get_level,
            commands::start_recording,
            commands::stop_recording,
            commands::cancel_recording,
            commands::list_recordings,
            commands::get_recording,
            commands::delete_recording,
            commands::set_pinned,
            commands::clear_recordings,
            commands::save_prompt,
            commands::list_prompts,
            commands::delete_prompt,
            commands::export_recording,
            commands::get_settings,
            commands::update_settings,
            commands::list_models,
            commands::download_model,
            commands::delete_model,
            commands::get_usage,
            commands::reset_usage,
            commands::app_status,
            commands::show_pill_menu,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Start capture from the global hotkey and tell the UI (for sound + state).
fn start_and_notify(app: &tauri::AppHandle) {
    log::info!("hotkey: start recording");
    let state = app.state::<AppState>();
    match commands::begin_recording(state.inner(), None) {
        Ok(()) => {
            commands::spawn_silence_guard(app.clone());
            let _ = app.emit("rec-started", ());
        }
        Err(e) => {
            log::warn!("hotkey: start failed: {e}");
            let _ = app.emit("rec-error", e);
        }
    }
}

/// Stop capture from the global hotkey, then transcribe + type on a worker
/// thread so the hotkey handler returns immediately.
fn stop_and_notify(app: &tauri::AppHandle) {
    log::info!("hotkey: stop -> transcribe + auto-type");
    let _ = app.emit("rec-stopped", ());
    let app2 = app.clone();
    std::thread::spawn(move || {
        let state = app2.state::<AppState>();
        match commands::end_recording(state.inner(), true) {
            Ok(Some(res)) => {
                log::info!("hotkey: result ({}) '{}'", res.language, res.full_text);
                let _ = app2.emit("dictation-result", res);
            }
            Ok(None) => {
                log::info!("hotkey: discarded (no speech)");
                let _ = app2.emit("rec-canceled", "no-speech");
            }
            Err(e) => {
                log::warn!("hotkey: transcribe failed: {e}");
                let _ = app2.emit("rec-error", e);
            }
        }
    });
}
