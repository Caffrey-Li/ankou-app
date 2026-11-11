use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use wanxiang_core::ToolId;

/// 已从本机应用包的 Info.plist 确认：`com.deepseek.dsh` 的 CFBundleName 是 DeepSeek Harness。
const HARNESS_BUNDLE_ID: &str = "com.deepseek.dsh";
/// 已从本机应用包的 Info.plist 确认：`com.openai.codex` 的 CFBundleName 是 ChatGPT。不按文件夹名查找。
const CODEX_BUNDLE_ID: &str = "com.openai.codex";

#[derive(Debug, Serialize)]
pub struct ToolIcon {
    pub tool: ToolId,
    pub png_base64: Option<String>,
}

struct Located {
    harness: Option<PathBuf>,
    cursor: Option<PathBuf>,
    codex: Option<PathBuf>,
}

#[cfg(target_os = "macos")]
pub fn tool_icons(cache: &Path) -> Vec<ToolIcon> {
    let found = locate(Path::new("/Applications"), bundle_identifier);
    assemble(cache, found, &mut render_png)
}

#[cfg(target_os = "windows")]
pub fn tool_icons(cache: &Path) -> Vec<ToolIcon> {
    let _ = cache;
    // 未实现
    null_icons()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn tool_icons(cache: &Path) -> Vec<ToolIcon> {
    let _ = cache;
    null_icons()
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn null_icons() -> Vec<ToolIcon> {
    [
        ToolId::DeepSeekHarness,
        ToolId::DsBot,
        ToolId::Cursor,
        ToolId::Codex,
    ]
    .into_iter()
    .map(|tool| ToolIcon {
        tool,
        png_base64: None,
    })
    .collect()
}

fn locate(root: &Path, bundle_id: impl Fn(&Path) -> Option<String>) -> Located {
    Located {
        harness: app_with_bundle_id(root, HARNESS_BUNDLE_ID, &bundle_id),
        cursor: exact_named_app(root, "Cursor.app"),
        codex: app_with_bundle_id(root, CODEX_BUNDLE_ID, &bundle_id),
    }
}

fn exact_named_app(root: &Path, name: &str) -> Option<PathBuf> {
    let want = OsStr::new(name);
    fs::read_dir(root).ok()?.flatten().find_map(|entry| {
        if entry.file_name() != want {
            return None;
        }
        let path = entry.path();
        path.is_dir().then_some(path)
    })
}

fn app_with_bundle_id(
    root: &Path,
    want: &str,
    bundle_id: &impl Fn(&Path) -> Option<String>,
) -> Option<PathBuf> {
    wanxiang_adapter_codex::app_with_bundle_id(root, want, bundle_id)
}

fn assemble(
    cache: &Path,
    found: Located,
    read: &mut impl FnMut(&Path) -> Option<Vec<u8>>,
) -> Vec<ToolIcon> {
    let mut icon = |path: Option<PathBuf>| {
        path.and_then(|path| cached_png(cache, &path, read))
            .map(base64_encode)
    };
    let harness = icon(found.harness);
    vec![
        ToolIcon {
            tool: ToolId::DeepSeekHarness,
            png_base64: harness.clone(),
        },
        ToolIcon {
            tool: ToolId::DsBot,
            png_base64: harness,
        },
        ToolIcon {
            tool: ToolId::Cursor,
            png_base64: icon(found.cursor),
        },
        ToolIcon {
            tool: ToolId::Codex,
            png_base64: icon(found.codex),
        },
    ]
}

fn cached_png(
    cache: &Path,
    app: &Path,
    read: &mut impl FnMut(&Path) -> Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    let modified = fs::metadata(app).ok()?.modified().ok()?;
    let stamp = stamp_text(app, modified);
    let key = cache_key(app);
    let png_path = cache.join(format!("{key}.png"));
    let stamp_path = cache.join(format!("{key}.stamp"));
    if fs::read_to_string(&stamp_path).ok().as_deref() == Some(stamp.as_str()) {
        if let Some(bytes) = fs::read(&png_path)
            .ok()
            .filter(|bytes| png_size(bytes) == Some((128, 128)))
        {
            return Some(bytes);
        }
    }
    let bytes = read(app).filter(|bytes| png_size(bytes) == Some((128, 128)))?;
    if fs::create_dir_all(cache).is_ok() {
        let _ = fs::write(&png_path, &bytes);
        let _ = fs::write(&stamp_path, &stamp);
    }
    Some(bytes)
}

fn stamp_text(app: &Path, modified: SystemTime) -> String {
    let dur = modified.duration_since(UNIX_EPOCH).unwrap_or_default();
    format!(
        "{}\n{}.{}",
        app.display(),
        dur.as_secs(),
        dur.subsec_nanos()
    )
}

/// ponytail: 文件名用 FNV-1a 64。stamp 里写完整路径，撞名就重读，不会把另一张图交出去。要更稳就换成 sha256。
fn cache_key(path: &Path) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in path.as_os_str().as_encoded_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return None;
    }
    Some((
        u32::from_be_bytes(bytes[16..20].try_into().ok()?),
        u32::from_be_bytes(bytes[20..24].try_into().ok()?),
    ))
}

fn base64_encode(bytes: Vec<u8>) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let value = ((bytes[index] as u32) << 16)
            | ((bytes[index + 1] as u32) << 8)
            | bytes[index + 2] as u32;
        out.push(TABLE[((value >> 18) & 63) as usize] as char);
        out.push(TABLE[((value >> 12) & 63) as usize] as char);
        out.push(TABLE[((value >> 6) & 63) as usize] as char);
        out.push(TABLE[(value & 63) as usize] as char);
        index += 3;
    }
    match bytes.len() - index {
        1 => {
            let value = (bytes[index] as u32) << 16;
            out.push(TABLE[((value >> 18) & 63) as usize] as char);
            out.push(TABLE[((value >> 12) & 63) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let value = ((bytes[index] as u32) << 16) | ((bytes[index + 1] as u32) << 8);
            out.push(TABLE[((value >> 18) & 63) as usize] as char);
            out.push(TABLE[((value >> 12) & 63) as usize] as char);
            out.push(TABLE[((value >> 6) & 63) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

#[cfg(target_os = "macos")]
pub(crate) fn bundle_identifier(app: &Path) -> Option<String> {
    use objc2_foundation::{NSBundle, NSString};

    objc2::rc::autoreleasepool(|_| {
        let path = NSString::from_str(&app.to_string_lossy());
        let bundle = NSBundle::bundleWithPath(&path)?;
        Some(bundle.bundleIdentifier()?.to_string())
    })
}

#[cfg(target_os = "macos")]
fn render_png(app: &Path) -> Option<Vec<u8>> {
    use objc2::runtime::AnyObject;
    use objc2::AllocAnyThread;
    use objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSCompositingOperation, NSDeviceRGBColorSpace,
        NSGraphicsContext, NSWorkspace,
    };
    use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

    if app.parent() != Some(Path::new("/Applications")) {
        return None;
    }
    objc2::rc::autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();
        let icon = workspace.iconForFile(&NSString::from_str(&app.to_string_lossy()));
        icon.setSize(NSSize::new(128.0, 128.0));
        let rep = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(),
                std::ptr::null_mut(),
                128,
                128,
                8,
                4,
                true,
                false,
                NSDeviceRGBColorSpace,
                0,
                0,
            )
        }?;
        let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
        NSGraphicsContext::saveGraphicsState_class();
        NSGraphicsContext::setCurrentContext(Some(&context));
        icon.drawInRect_fromRect_operation_fraction(
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(128.0, 128.0)),
            NSRect::ZERO,
            NSCompositingOperation::SourceOver,
            1.0,
        );
        NSGraphicsContext::restoreGraphicsState_class();
        let props: objc2::rc::Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::new();
        let data =
            unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &props) }?;
        Some(data.to_vec())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::time::Duration;

    fn temp_apps(label: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("wanxiang-icons-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn png128() -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&128u32.to_be_bytes());
        bytes.extend_from_slice(&128u32.to_be_bytes());
        bytes
    }

    #[test]
    fn names_must_match_case_and_harness_needs_its_bundle_id() {
        let root = temp_apps("locate");
        fs::create_dir(root.join("cursor.app")).unwrap();
        fs::create_dir(root.join("Codex.app")).unwrap();
        fs::create_dir(root.join("Other.app")).unwrap();
        fs::create_dir(root.join("Harness.app")).unwrap();
        fs::create_dir(root.join("Desktop.app")).unwrap();
        let found = locate(&root, |path| {
            if path.ends_with("Harness.app") {
                Some(HARNESS_BUNDLE_ID.to_string())
            } else if path.ends_with("Desktop.app") {
                Some(CODEX_BUNDLE_ID.to_string())
            } else if path.ends_with("Codex.app") {
                Some("com.example.not-the-app".to_string())
            } else {
                None
            }
        });
        assert!(found.cursor.is_none());
        assert_eq!(found.codex.unwrap().file_name().unwrap(), "Desktop.app");
        assert_eq!(found.harness.unwrap().file_name().unwrap(), "Harness.app");
        let again = locate(&root, |_| Some("com.example.not-harness".to_string()));
        assert!(again.harness.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn two_harness_bundles_are_not_guessed() {
        let root = temp_apps("two");
        fs::create_dir(root.join("A.app")).unwrap();
        fs::create_dir(root.join("B.app")).unwrap();
        let found = locate(&root, |_| Some(HARNESS_BUNDLE_ID.to_string()));
        assert!(found.harness.is_none());
        assert!(found.codex.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ds_bot_reuses_the_harness_icon_and_stays_empty_without_it() {
        let root = temp_apps("bot");
        let app = root.join("Harness.app");
        fs::create_dir(&app).unwrap();
        let cache = root.join("icon-cache");
        let reads = Cell::new(0);
        let mut read = |_: &Path| {
            reads.set(reads.get() + 1);
            Some(png128())
        };
        let rows = assemble(
            &cache,
            Located {
                harness: Some(app),
                cursor: None,
                codex: None,
            },
            &mut read,
        );
        let harness = rows
            .iter()
            .find(|icon| icon.tool == ToolId::DeepSeekHarness)
            .unwrap();
        let bot = rows.iter().find(|icon| icon.tool == ToolId::DsBot).unwrap();
        assert_eq!(harness.png_base64, bot.png_base64);
        assert!(harness.png_base64.is_some());
        assert_eq!(reads.get(), 1);
        let empty = assemble(
            &cache,
            Located {
                harness: None,
                cursor: None,
                codex: None,
            },
            &mut read,
        );
        assert!(empty
            .iter()
            .any(|icon| icon.tool == ToolId::DsBot && icon.png_base64.is_none()));
        assert_eq!(reads.get(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ds_bot_stays_empty_and_cache_follows_mtime() {
        let root = temp_apps("cache");
        let app = root.join("Cursor.app");
        fs::create_dir(&app).unwrap();
        fs::File::open(&app)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
            .unwrap();
        let cache = root.join("icon-cache");
        let reads = Cell::new(0);
        let mut read = |_: &Path| {
            reads.set(reads.get() + 1);
            Some(png128())
        };
        let found = Located {
            harness: None,
            cursor: Some(app.clone()),
            codex: None,
        };
        let first = assemble(&cache, found, &mut read);
        assert!(first
            .iter()
            .any(|icon| icon.tool == ToolId::DsBot && icon.png_base64.is_none()));
        assert!(first
            .iter()
            .any(|icon| icon.tool == ToolId::Cursor && icon.png_base64.is_some()));
        assert_eq!(reads.get(), 1);
        let found = Located {
            harness: None,
            cursor: Some(app.clone()),
            codex: None,
        };
        let _ = assemble(&cache, found, &mut read);
        assert_eq!(reads.get(), 1);
        fs::File::open(&app)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_100))
            .unwrap();
        let found = Located {
            harness: None,
            cursor: Some(app),
            codex: None,
        };
        let _ = assemble(&cache, found, &mut read);
        assert_eq!(reads.get(), 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn base64_pads() {
        assert_eq!(base64_encode(b"Man".to_vec()), "TWFu");
        assert_eq!(base64_encode(b"Ma".to_vec()), "TWE=");
        assert_eq!(base64_encode(b"M".to_vec()), "TQ==");
    }

    #[test]
    fn windows_reader_is_unimplemented() {
        let body = include_str!("icons.rs").split("mod tests").next().unwrap();
        let start = body
            .find("#[cfg(target_os = \"windows\")]")
            .expect("windows cfg");
        let window = &body[start..start + 240];
        assert!(window.contains("未实现"), "{window}");
        assert!(window.contains("null_icons"), "{window}");
        assert!(null_icons().iter().all(|icon| icon.png_base64.is_none()));
        assert!(!body.contains("ChatGPT.app"));
        assert!(body.contains(CODEX_BUNDLE_ID));
        assert!(!body.contains("/.codex"));
        assert!(!body.contains("/.dsh"));
        assert!(!body.contains("home_dir"));
        assert!(body.contains("iconForFile"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn installed_harness_is_only_the_confirmed_bundle() {
        let found = locate(Path::new("/Applications"), bundle_identifier);
        let Some(path) = found.harness else {
            return;
        };
        assert_eq!(bundle_identifier(&path).as_deref(), Some(HARNESS_BUNDLE_ID));
        assert_eq!(path.parent(), Some(Path::new("/Applications")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn installed_codex_is_only_the_confirmed_bundle() {
        let found = locate(Path::new("/Applications"), bundle_identifier);
        let Some(path) = found.codex else {
            return;
        };
        assert_eq!(bundle_identifier(&path).as_deref(), Some(CODEX_BUNDLE_ID));
        assert_eq!(path.parent(), Some(Path::new("/Applications")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn chat_bundle_is_not_installed_and_codex_bundle_is() {
        let root = temp_apps("bundle-id");
        let home = root.join("home");
        fs::create_dir_all(&home).unwrap();
        for (label, bundle_id, installed) in [
            ("chat-apps", "com.openai.chat", false),
            ("codex-apps", "com.openai.codex", true),
        ] {
            let apps = root.join(label);
            let contents = apps.join("ChatGPT.app").join("Contents");
            fs::create_dir_all(&contents).unwrap();
            fs::write(
                contents.join("Info.plist"),
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>{bundle_id}</string></dict></plist>"#
                ),
            )
            .unwrap();
            assert_eq!(
                wanxiang_adapter_codex::installed(&home, &apps, bundle_identifier),
                installed,
                "{bundle_id}"
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn installed_cursor_icon_is_128_png() {
        let Some(path) = exact_named_app(Path::new("/Applications"), "Cursor.app") else {
            return;
        };
        let png = render_png(&path).expect("Cursor 图标");
        assert_eq!(png_size(&png), Some((128, 128)));
    }
}
