mod auth;
mod background;
mod commands;
mod discord;
mod http;
mod i18n;
mod instance_manager;
mod java_manager;
mod launch_manager;
mod loader_manager;
mod logger;
mod modrinth;
mod paths;
mod secrets;
mod server_manager;
mod store;
mod updater;
mod version_manager;
mod world_manager;

use paths::AppPaths;
use secrets::SecretBox;
use std::fs;
use store::Store;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::webview::PageLoadEvent;
use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub fn run() {
    dotenvy::dotenv().ok();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let _ = dotenvy::from_path(dir.join(".env"));
        }
    }

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![commands::buran_invoke])
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            fs::create_dir_all(&app_data)?;
            let resource_dir = app.path().resource_dir().unwrap_or_else(|_| app_data.clone());
            let paths = AppPaths::new(app_data.clone(), resource_dir);
            logger::init(paths.logs_dir().join("main.log"));
            logger::info(&format!(
                "Buran Launcher {} başlatılıyor (veri: {})",
                env!("CARGO_PKG_VERSION"),
                app_data.display()
            ));

            let store = Store::load(paths.config_file());
            let secrets = SecretBox::load_or_create(paths.secrets_key());
            app.manage(store);
            app.manage(paths);
            app.manage(secrets);

            let window = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::App("index.html".into()),
            )
            .title("Buran Launcher")
            .inner_size(1280.0, 800.0)
            .min_inner_size(960.0, 600.0)
            .resizable(true)
            .decorations(false)
            .visible(false)
            .center()
            .initialization_script(include_str!("bridge.js"))
            .on_page_load(|window, payload| {
                if payload.event() == PageLoadEvent::Finished {
                    let _ = window.show();
                }
            })
            .build()?;

            let app_handle = app.handle().clone();
            let window_for_close = window.clone();
            window.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    let store = app_handle.state::<Store>();
                    if store.get_setting_bool("minimizeToTray", true) {
                        api.prevent_close();
                        let _ = window_for_close.hide();
                    }
                }
            });

            let show_item = MenuItem::with_id(app, "show", "Göster", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Çıkış", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&show_item, &separator, &quit_item])?;
            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or("Uygulama simgesi bulunamadı")?;

            TrayIconBuilder::with_id("main-tray")
                .icon(icon)
                .tooltip("Buran Launcher")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }
                    | TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } => {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        discord::destroy();
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            discord::init(app.handle().clone());
            Ok(())
        });

    let app = builder
        .build(tauri::generate_context!())
        .expect("Buran Launcher başlatılamadı");
    app.run(|_app_handle, event| {
        if matches!(
            event,
            RunEvent::ExitRequested { .. } | RunEvent::Exit
        ) {
            discord::destroy();
        }
    });
}