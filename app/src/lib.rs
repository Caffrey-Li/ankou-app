use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
#[cfg(target_os = "windows")]
use tauri::Listener;
use tauri::{Manager, RunEvent, Theme, WindowEvent};
use wanxiang_core::Supervisor;

pub fn run() {
    tauri::Builder::default()
        .manage(Supervisor::new())
        .setup(|app| {
            setup_tray(app.handle())?;
            #[cfg(target_os = "windows")]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_icon(Image::from_bytes(include_bytes!(
                    "../icons/icon-windows-256.png"
                ))?);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("启动万象小助手失败")
        .run(|app, event| match event {
            RunEvent::Exit => shutdown(app),
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => reveal(app),
            _ => {}
        });
}

fn shutdown(app: &tauri::AppHandle) {
    if let Some(supervisor) = app.try_state::<Supervisor>() {
        supervisor.shutdown();
    }
}

fn reveal(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开万象", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "刷新（未验证）", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &refresh, &quit])?;
    let theme = current_theme(app);
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("万象小助手")
        .icon(tray_image(theme)?)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => reveal(app),
            "quit" => {
                shutdown(app);
                app.exit(0);
            }
            "refresh" => {}
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                reveal(tray.app_handle());
            }
        });
    #[cfg(target_os = "macos")]
    {
        builder = builder.icon_as_template(true);
    }
    let tray = builder.build(app)?;

    #[cfg(target_os = "windows")]
    {
        let handle = app.clone();
        handle.listen_any("tauri://theme-changed", move |event| {
            let dark = event.payload().contains("dark");
            let theme = if dark { Theme::Dark } else { Theme::Light };
            if let Ok(icon) = tray_image(theme) {
                let _ = tray.set_icon(Some(icon));
            }
        });
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = tray;
    }
    Ok(())
}

fn current_theme(app: &tauri::AppHandle) -> Theme {
    app.get_webview_window("main")
        .and_then(|window| window.theme().ok())
        .unwrap_or(Theme::Light)
}

fn tray_image(theme: Theme) -> tauri::Result<Image<'static>> {
    #[cfg(target_os = "macos")]
    {
        let _ = theme;
        Image::from_bytes(include_bytes!("../icons/tray-macos-template.png"))
    }
    #[cfg(target_os = "windows")]
    {
        let bytes: &[u8] = if theme == Theme::Dark {
            include_bytes!("../icons/tray-windows-dark.png")
        } else {
            include_bytes!("../icons/tray-windows-light.png")
        };
        Image::from_bytes(bytes)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = theme;
        Image::from_bytes(include_bytes!("../icons/tray-windows-light.png"))
    }
}
