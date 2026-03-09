use rfd::{MessageButtons, MessageDialog, MessageLevel};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;
use tauri::WebviewWindowBuilder;

use crate::commands::settings::sync_window_runtime_state_from_settings;
use crate::service_runtime::stop_service;

const TRAY_MENU_SHOW_MAIN: &str = "tray_show_main";
const TRAY_MENU_QUIT_APP: &str = "tray_quit_app";
pub(crate) const MAIN_WINDOW_LABEL: &str = "main";
pub(crate) static APP_EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);
pub(crate) static TRAY_AVAILABLE: AtomicBool = AtomicBool::new(false);
pub(crate) static CLOSE_TO_TRAY_ON_CLOSE: AtomicBool = AtomicBool::new(false);
pub(crate) static LIGHTWEIGHT_MODE_ON_CLOSE_TO_TRAY: AtomicBool = AtomicBool::new(false);
pub(crate) static KEEP_ALIVE_FOR_LIGHTWEIGHT_CLOSE: AtomicBool = AtomicBool::new(false);

pub(crate) fn handle_main_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() != MAIN_WINDOW_LABEL {
        return;
    }
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        if APP_EXIT_REQUESTED.load(Ordering::Relaxed) {
            return;
        }
        if !CLOSE_TO_TRAY_ON_CLOSE.load(Ordering::Relaxed) {
            return;
        }
        if !TRAY_AVAILABLE.load(Ordering::Relaxed) {
            CLOSE_TO_TRAY_ON_CLOSE.store(false, Ordering::Relaxed);
            return;
        }
        if LIGHTWEIGHT_MODE_ON_CLOSE_TO_TRAY.load(Ordering::Relaxed) {
            KEEP_ALIVE_FOR_LIGHTWEIGHT_CLOSE.store(true, Ordering::Relaxed);
            log::info!(
                "window close intercepted; lightweight mode enabled, closing main window to release webview"
            );
            return;
        }
        api.prevent_close();
        if let Err(err) = window.hide() {
            log::warn!("hide window to tray failed: {}", err);
        } else {
            log::info!("window close intercepted; app hidden to tray");
        }
        return;
    }
    if let tauri::WindowEvent::Destroyed = event {
        if should_keep_alive_for_lightweight_close() {
            log::info!("main window destroyed for lightweight tray mode");
            return;
        }
        stop_service();
    }
}

pub(crate) fn handle_run_event(app: &tauri::AppHandle, event: &tauri::RunEvent) {
    #[cfg(not(target_os = "macos"))]
    let _ = app;
    match event {
        tauri::RunEvent::ExitRequested { api, .. } => {
            if should_keep_alive_for_lightweight_close() {
                api.prevent_exit();
                log::info!("prevented app exit for lightweight tray mode");
                return;
            }
            APP_EXIT_REQUESTED.store(true, Ordering::Relaxed);
            KEEP_ALIVE_FOR_LIGHTWEIGHT_CLOSE.store(false, Ordering::Relaxed);
            stop_service();
        }
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => {
            show_main_window(app);
        }
        _ => {}
    }
}

pub(crate) fn notify_existing_instance_focused() {
    let _ = MessageDialog::new()
        .set_title("CodexManager")
        .set_description("CodexManager 已在运行，已切换到现有窗口。")
        .set_level(MessageLevel::Info)
        .set_buttons(MessageButtons::Ok)
        .show();
}

pub(crate) fn setup_tray(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    TRAY_AVAILABLE.store(false, Ordering::Relaxed);
    let show_main = MenuItem::with_id(app, TRAY_MENU_SHOW_MAIN, "显示主窗口", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, TRAY_MENU_QUIT_APP, "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_main, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_MENU_SHOW_MAIN => {
                show_main_window(app);
            }
            TRAY_MENU_QUIT_APP => {
                APP_EXIT_REQUESTED.store(true, Ordering::Relaxed);
                KEEP_ALIVE_FOR_LIGHTWEIGHT_CLOSE.store(false, Ordering::Relaxed);
                stop_service();
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(&tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    TRAY_AVAILABLE.store(true, Ordering::Relaxed);
    Ok(())
}

pub(crate) fn show_main_window(app: &tauri::AppHandle) {
    KEEP_ALIVE_FOR_LIGHTWEIGHT_CLOSE.store(false, Ordering::Relaxed);
    let Some(window) = ensure_main_window(app) else {
        return;
    };
    if let Err(err) = window.show() {
        log::warn!("show main window failed: {}", err);
        return;
    }
    let _ = window.unminimize();
    let _ = window.set_focus();
}

fn ensure_main_window(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        return Some(window);
    }

    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN_WINDOW_LABEL)
        .cloned()
        .or_else(|| app.config().app.windows.first().cloned())?;
    config.label = MAIN_WINDOW_LABEL.to_string();

    match WebviewWindowBuilder::from_config(app, &config).and_then(|builder| builder.build()) {
        Ok(window) => Some(window),
        Err(err) => {
            if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                return Some(window);
            }
            log::warn!("create main window failed: {}", err);
            None
        }
    }
}

pub(crate) fn should_keep_alive_for_lightweight_close() -> bool {
    !APP_EXIT_REQUESTED.load(Ordering::Relaxed)
        && KEEP_ALIVE_FOR_LIGHTWEIGHT_CLOSE.load(Ordering::Relaxed)
}

pub(crate) fn load_env_from_exe_dir() {
    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(err) => {
            log::warn!("Failed to resolve current exe path: {}", err);
            return;
        }
    };
    let Some(exe_dir) = exe_path.parent() else {
        return;
    };

    let candidates = ["codexmanager.env", "CodexManager.env", ".env"];
    let mut chosen = None;
    for name in candidates {
        let p = exe_dir.join(name);
        if p.is_file() {
            chosen = Some(p);
            break;
        }
    }
    let Some(path) = chosen else {
        return;
    };

    let bytes = match std::fs::read(&path) {
        Ok(v) => v,
        Err(err) => {
            log::warn!("Failed to read env file {}: {}", path.display(), err);
            return;
        }
    };
    let content = String::from_utf8_lossy(&bytes);
    let mut applied = 0usize;
    for (idx, raw_line) in content.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key_raw, value_raw)) = line.split_once('=') else {
            log::warn!(
                "Skip invalid env line {}:{} (missing '=')",
                path.display(),
                line_no
            );
            continue;
        };
        let key = key_raw.trim();
        if key.is_empty() {
            continue;
        }
        let mut value = value_raw.trim().to_string();
        if (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
            || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2)
        {
            value = value[1..value.len() - 1].to_string();
        }

        if std::env::var_os(key).is_some() {
            continue;
        }
        std::env::set_var(key, value);
        applied += 1;
    }

    if applied > 0 {
        log::info!("Loaded {} env vars from {}", applied, path.display());
    }
}

pub(crate) fn sync_startup_window_state() {
    if let Ok(mut settings) = codexmanager_service::app_settings_get_with_overrides(
        Some(
            codexmanager_service::current_close_to_tray_on_close_setting()
                && TRAY_AVAILABLE.load(Ordering::Relaxed),
        ),
        Some(TRAY_AVAILABLE.load(Ordering::Relaxed)),
    ) {
        sync_window_runtime_state_from_settings(&mut settings);
    }
}

