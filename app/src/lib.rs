use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
#[cfg(target_os = "windows")]
use tauri::Listener;
use tauri::{Manager, RunEvent, Theme, WindowEvent};
use wanxiang_core::Supervisor;

#[cfg(test)]
mod fixture;

mod collect;
mod commands;
mod discover;
mod measured;

pub fn run() {
    commands::attach(tauri::Builder::default())
        .manage(Supervisor::new())
        .setup(|app| {
            app.manage(commands::Hub::open(app.handle())?);
            setup_tray(app.handle())?;
            if let Some(window) = app.get_webview_window("main") {
                apply_window_material(&window);
                open_preview(&window);
            }
            #[cfg(target_os = "windows")]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_icon(Image::from_bytes(include_bytes!(
                    "../icons/icon-windows-256.png"
                ))?);
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                let keep = window
                    .app_handle()
                    .try_state::<commands::Hub>()
                    .and_then(|hub| hub.store.settings().ok())
                    .map(|settings| settings.close_to_tray)
                    .unwrap_or(true);
                if keep {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    shutdown(window.app_handle());
                    window.app_handle().exit(0);
                }
            }
            WindowEvent::Resized(_) => {
                #[cfg(target_os = "macos")]
                {
                    let _ = place_sidebar_effect(window);
                }
            }
            _ => {}
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

fn apply_window_material(window: &tauri::WebviewWindow) {
    #[cfg(target_os = "macos")]
    {
        let _ = window.set_theme(Some(Theme::Light));
        match window_vibrancy::apply_vibrancy(
            window,
            window_vibrancy::NSVisualEffectMaterial::Sidebar,
            Some(window_vibrancy::NSVisualEffectState::Active),
            Some(16.0),
        ) {
            Ok(()) if place_sidebar_effect(window) => {
                let _ = window.eval("document.documentElement.dataset.vibrancy='sidebar'");
            }
            Ok(()) => {
                let _ = window_vibrancy::clear_vibrancy(window);
                eprintln!("无法把系统材质限制在侧栏内，侧栏改用半透明白");
            }
            Err(error) => {
                eprintln!("系统侧栏材质不可用，侧栏改用半透明白：{error}");
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        match window_vibrancy::apply_mica(window, Some(false)) {
            Ok(()) => {
                let _ = window.eval("document.documentElement.dataset.vibrancy='mica'");
            }
            Err(error) => {
                eprintln!("Mica 不可用，侧栏改用半透明白：{error}");
            }
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = window;
    }
}

#[cfg(target_os = "macos")]
fn place_sidebar_effect(window: &impl raw_window_handle::HasWindowHandle) -> bool {
    use objc2::msg_send;
    use objc2_app_kit::{NSAutoresizingMaskOptions, NSView};
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use raw_window_handle::RawWindowHandle;

    let Ok(handle) = window.window_handle() else {
        return false;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return false;
    };
    let root: &NSView = unsafe { appkit.ns_view.cast().as_ref() };
    let Some(effect) = root.viewWithTag(91376254) else {
        return false;
    };
    let bounds = root.bounds();
    let sidebar_height = (bounds.size.height - 20.0).max(0.0);
    let origin_y = if root.isFlipped() { 9.0 } else { 11.0 };
    unsafe {
        effect.setFrame(NSRect::new(
            NSPoint::new(9.0, origin_y),
            NSSize::new(224.0, sidebar_height),
        ));
        effect.setAutoresizingMask(NSAutoresizingMaskOptions::ViewHeightSizable);
        let () = msg_send![&*effect, setCornerRadius: 16.0_f64];
    }
    effect.frame().size.height > 0.0
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
            "refresh" => {
                if let Some(hub) = app.try_state::<commands::Hub>() {
                    collect::start_manual(
                        std::sync::Arc::clone(&hub.store),
                        std::sync::Arc::clone(&hub.gate),
                        std::sync::Arc::clone(&hub.emit),
                    );
                }
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

fn open_preview(window: &tauri::WebviewWindow) {
    #[cfg(debug_assertions)]
    {
        let Ok(page) = std::env::var("WANXIANG_PREVIEW") else {
            return;
        };
        if page != "usage" && page != "overview" {
            return;
        }
        let window = window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(700));
            let url = format!("http://127.0.0.1:1420/?page={page}&preview=1");
            if let Ok(url) = url.parse() {
                let _ = window.navigate(url);
            }
        });
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = window;
    }
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
