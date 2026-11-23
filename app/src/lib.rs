use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
#[cfg(target_os = "windows")]
use tauri::Listener;
use tauri::{Emitter, Manager, RunEvent, Theme, WindowEvent};
use wanxiang_core::Supervisor;

#[cfg(test)]
mod fixture;

mod collect;
mod commands;
mod discover;
mod handoff;
mod icons;
mod measured;
mod runtime;
mod space;
mod system;

pub fn run() {
    commands::attach(
        tauri::Builder::default()
            .plugin(tauri_plugin_opener::init())
            .plugin(tauri_plugin_dialog::init())
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                None,
            ))
            .plugin(tauri_plugin_notification::init()),
    )
    .manage(Supervisor::new())
    .setup(|app| {
        app.manage(commands::Hub::open(app.handle())?);
        setup_app_menu(app.handle())?;
        setup_tray(app.handle())?;
        if let Err(error) = build_panel(app.handle()) {
            eprintln!("快速面板没有建立，点菜单栏图标改为打开主窗口：{error}");
        }
        if let Some(settings) = app
            .try_state::<commands::Hub>()
            .and_then(|hub| hub.store.settings().ok())
        {
            if let Err(error) = system::apply(app.handle(), &settings) {
                eprintln!("{error}");
            }
        }
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
        paint_window_backdrop(window);
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

/// 窗口本身是透明的（侧栏要透出系统材质），拖动改变大小时网页内容比窗口慢一拍，
/// 新露出的边缘没有内容，会透出后面的窗口，看起来是一片黑。
/// 给窗口垫一层与页面底色相同的浅灰：侧栏的系统材质在它上面，不受影响。
#[cfg(target_os = "macos")]
fn paint_window_backdrop(window: &impl raw_window_handle::HasWindowHandle) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_app_kit::NSView;
    use raw_window_handle::RawWindowHandle;

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    let root: &NSView = unsafe { appkit.ns_view.cast().as_ref() };
    unsafe {
        let ns_window: *mut AnyObject = msg_send![root, window];
        if ns_window.is_null() {
            return;
        }
        // 与 --color-window-bottom（#f4f4f6）一致
        let color: *mut AnyObject = msg_send![
            class!(NSColor),
            colorWithSRGBRed: 244.0_f64 / 255.0,
            green: 244.0_f64 / 255.0,
            blue: 246.0_f64 / 255.0,
            alpha: 1.0_f64
        ];
        if !color.is_null() {
            let () = msg_send![ns_window, setBackgroundColor: color];
        }
    }
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

static PANEL_HIDDEN_AT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Figma 21:369：点菜单栏图标弹出快速面板，点外部收起。
fn build_panel(app: &tauri::AppHandle) -> tauri::Result<()> {
    let panel = tauri::WebviewWindowBuilder::new(
        app,
        "tray",
        tauri::WebviewUrl::App("index.html#tray".into()),
    )
    .title("万象快速面板")
    .inner_size(362.0, 440.0)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .resizable(false)
    .skip_taskbar(true)
    .always_on_top(true)
    .visible_on_all_workspaces(true)
    .focused(false)
    .visible(false)
    .build()?;
    let handle = panel.clone();
    panel.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            if handle.is_visible().unwrap_or(false) {
                PANEL_HIDDEN_AT.store(now_ms(), std::sync::atomic::Ordering::SeqCst);
                let _ = handle.hide();
            }
        }
    });
    Ok(())
}

fn toggle_panel(app: &tauri::AppHandle, rect: tauri::Rect) {
    let Some(panel) = app.get_webview_window("tray") else {
        reveal(app);
        return;
    };
    if panel.is_visible().unwrap_or(false) {
        let _ = panel.hide();
        return;
    }
    // 点图标会先让面板失焦收起；紧接着的这次点击只是收起，不再打开
    if now_ms().saturating_sub(PANEL_HIDDEN_AT.load(std::sync::atomic::Ordering::SeqCst)) < 300 {
        return;
    }
    let scale = panel.scale_factor().unwrap_or(2.0);
    let icon = rect.position.to_physical::<f64>(scale);
    let icon_size = rect.size.to_physical::<f64>(scale);
    let width = panel
        .outer_size()
        .map(|size| f64::from(size.width))
        .unwrap_or(362.0 * scale);
    let mut x = icon.x + icon_size.width / 2.0 - width / 2.0;
    let y = icon.y + icon_size.height;
    if let Ok(Some(monitor)) = app.monitor_from_point(icon.x, icon.y) {
        let left = f64::from(monitor.position().x);
        let right = left + f64::from(monitor.size().width);
        x = x.min(right - width).max(left);
    }
    let _ = panel.set_position(tauri::PhysicalPosition::new(x, y));
    let _ = panel.emit("panel-open", ());
    let _ = panel.show();
    let _ = panel.set_focus();
}

/// 中文应用菜单，替换 Tauri 默认的英文菜单。
fn setup_app_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{AboutMetadata, PredefinedMenuItem, Submenu};
    let about = PredefinedMenuItem::about(
        app,
        Some("关于万象小助手"),
        Some(AboutMetadata {
            name: Some("万象小助手".to_string()),
            version: Some(app.package_info().version.to_string()),
            ..Default::default()
        }),
    )?;
    let settings = MenuItem::with_id(app, "app-settings", "设置…", true, Some("CmdOrCtrl+,"))?;
    let app_menu = Submenu::with_items(
        app,
        "万象小助手",
        true,
        &[
            &about,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some("隐藏万象小助手"))?,
            &PredefinedMenuItem::hide_others(app, Some("隐藏其他"))?,
            &PredefinedMenuItem::show_all(app, Some("全部显示"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, Some("退出万象小助手"))?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "编辑",
        true,
        &[
            &PredefinedMenuItem::undo(app, Some("撤销"))?,
            &PredefinedMenuItem::redo(app, Some("重做"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, Some("剪切"))?,
            &PredefinedMenuItem::copy(app, Some("拷贝"))?,
            &PredefinedMenuItem::paste(app, Some("粘贴"))?,
            &PredefinedMenuItem::select_all(app, Some("全选"))?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "窗口",
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some("最小化"))?,
            &PredefinedMenuItem::close_window(app, Some("关闭窗口"))?,
        ],
    )?;
    let menu = Menu::with_items(app, &[&app_menu, &edit, &window])?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        if event.id().as_ref() == "app-settings" {
            reveal(app);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.emit("navigate", "settings");
            }
        }
    });
    Ok(())
}

fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开万象", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "刷新", true, None::<&str>)?;
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
                rect,
                ..
            } = event
            {
                toggle_panel(tray.app_handle(), rect);
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
        if !matches!(
            page.as_str(),
            "usage"
                | "overview"
                | "handoff"
                | "handoff-preview"
                | "space"
                | "space-live"
                | "space-confirm"
                | "space-result"
                | "space-overview"
        ) {
            return;
        }
        let window = window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(700));
            let url = match page.as_str() {
                "handoff" => "http://127.0.0.1:1420/?page=handoff&preview=1&compare=1&fixture=m3".to_string(),
                "handoff-preview" => {
                    "http://127.0.0.1:1420/?page=handoff&preview=1&compare=1&fixture=m3&sheet=preview".to_string()
                }
                "space" => "http://127.0.0.1:1420/?page=space&preview=1&fixture=m4".to_string(),
                "space-live" => "http://127.0.0.1:1420/?page=space&preview=1".to_string(),
                "space-confirm" => "http://127.0.0.1:1420/?page=space&preview=1&fixture=m4&sheet=confirm".to_string(),
                "space-result" => "http://127.0.0.1:1420/?page=space&preview=1&fixture=m4&sheet=result".to_string(),
                "space-overview" => "http://127.0.0.1:1420/?page=overview&preview=1&fixture=m4".to_string(),
                _ => format!("http://127.0.0.1:1420/?page={page}&preview=1"),
            };
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
