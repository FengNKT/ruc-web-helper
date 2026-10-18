#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod branding;
mod cleanup;
mod credentials;
mod device_names;
mod portal;
mod protocol;
mod service;
mod storage;
mod update_install;
mod update_manifest;
mod update_network;
mod updater;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
use tauri_plugin_autostart::ManagerExt;

fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn main() {
    if update_install::helper_mode() {
        return;
    }
    tauri::Builder::default()
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name(branding::CHINESE_NAME)
                .args(["--autostart"])
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main(app);
        }))
        .setup(|app| {
            let executable = std::env::current_exe()?;
            let old_directory = executable.parent().ok_or("无法定位程序目录")?;
            let directory = storage::directory()?;
            storage::prepare(old_directory, &directory)?;
            branding::migrate_startup(&executable)?;
            let mut legacy = app.path().app_data_dir()?.join("settings-v2.json");
            if !legacy.exists() {
                if let Some(roaming) = std::env::var_os("APPDATA") {
                    // Preserve migration from the original application identifier.
                    legacy = std::path::PathBuf::from(roaming)
                        .join("cn.ruc.campusassistant.prototype")
                        .join("settings-v2.json");
                }
            }
            app.manage(service::Service::new(directory.clone(), legacy, || {
                app.autolaunch()
                    .enable()
                    .map_err(|_| "无法启用开机自启，请检查 Windows 启动项权限".to_string())
            })?);
            app.manage(updater::Updater::new(directory.clone()));
            let theme = app.state::<service::Service>().settings().theme.native();
            tauri::WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?
                .theme(theme)
                .data_directory(directory.join("webview-data"))
                .visible(!std::env::args().any(|arg| arg == "--autostart"))
                .build()?;
            let open = MenuItem::with_id(app, "open", "打开人大校园网助手", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip(format!(
                    "{} · {}",
                    branding::CHINESE_NAME,
                    branding::ENGLISH_NAME
                ))
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main(app),
                    "quit" => app.exit(0),
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
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            if std::env::args().any(|arg| arg == "--autostart") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            service::start_worker(app.handle().clone());
            update_install::acknowledge(&directory, &executable);
            updater::start_worker(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            service::get_preferences,
            service::clear_all_data,
            service::set_preferences,
            service::save_account,
            service::set_automatic_account,
            service::forget_account,
            service::switch_account,
            service::reset_accounts,
            service::get_snapshot,
            service::connect_network,
            service::disconnect_network,
            service::drop_device,
            service::get_records,
            service::get_traffic_data,
            service::set_device_name,
            service::set_device_ignored,
            service::open_official,
            service::open_privacy_feedback,
            updater::get_update_state,
            updater::set_update_preferences,
            updater::check_update,
            updater::download_update,
            updater::cancel_update,
            updater::skip_update,
            updater::install_update,
            updater::open_update_release
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // The tray remains available to restore or quit the application.
                if window.hide().is_ok() {
                    api.prevent_close();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("无法启动人大校园网助手");
}
