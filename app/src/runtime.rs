use std::path::Path;

use wanxiang_core::{harness_executable_names, TrashFault, Trasher};

pub struct Observed {
    pub bundle_ids: Option<Vec<String>>,
    pub process_names: Option<Vec<String>>,
    pub harness_executables: Option<Vec<String>>,
}

pub fn trash_supported() -> bool {
    cfg!(any(target_os = "macos", target_os = "windows"))
}

pub fn observe() -> Observed {
    Observed {
        bundle_ids: bundle_ids(),
        process_names: process_names(),
        harness_executables: harness_names(),
    }
}

pub struct SystemTrash;

impl Trasher for SystemTrash {
    fn trash(&mut self, path: &Path) -> Result<(), TrashFault> {
        trash_path(path)
    }
}

#[cfg(target_os = "macos")]
fn bundle_ids() -> Option<Vec<String>> {
    use objc2_app_kit::NSWorkspace;

    objc2::rc::autoreleasepool(|_| {
        let apps = NSWorkspace::sharedWorkspace().runningApplications();
        let mut ids = Vec::new();
        for app in apps.iter() {
            if let Some(id) = app.bundleIdentifier() {
                let text = id.to_string();
                if !text.is_empty() {
                    ids.push(text);
                }
            }
        }
        Some(ids)
    })
}

#[cfg(target_os = "windows")]
fn bundle_ids() -> Option<Vec<String>> {
    let names = process_names()?;
    let mut ids = Vec::new();
    for name in &names {
        let lower = name.to_ascii_lowercase();
        let id = match lower.as_str() {
            "cursor.exe" => Some("com.todesktop.230313mzl4w4u92"),
            "claude.exe" => Some("com.anthropic.claudefordesktop"),
            "chatgpt.exe" | "codex.exe" => Some("com.openai.codex"),
            "deepseek harness.exe" | "dsh.exe" => Some("com.deepseek.dsh"),
            "antigravity.exe" => Some("com.google.antigravity"),
            "gemini.exe" => Some("com.google.GeminiMacOS"),
            "workbuddy.exe" => Some("com.tencent.workbuddy.mac"),
            "grok bot.exe" => Some("com.anysphere.sand"),
            "muse.exe" => Some("com.meta.endo"),
            "codebuddy.exe" => Some("com.tencent.codebuddycn"),
            _ => None,
        };
        if let Some(id) = id {
            ids.push(id.to_string());
        }
    }
    Some(ids)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn bundle_ids() -> Option<Vec<String>> {
    None
}

#[cfg(target_os = "macos")]
fn process_names() -> Option<Vec<String>> {
    const PROC_ALL_PIDS: u32 = 1;

    #[link(name = "proc", kind = "dylib")]
    extern "C" {
        fn proc_listpids(
            kind: u32,
            typeinfo: u32,
            buffer: *mut std::ffi::c_void,
            buffersize: i32,
        ) -> i32;
        fn proc_name(pid: i32, buffer: *mut std::ffi::c_void, buffersize: u32) -> i32;
    }

    let bytes = unsafe { proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0) };
    if bytes <= 0 {
        return None;
    }
    let count = (bytes as usize / 4) + 16;
    let mut pids = vec![0i32; count];
    let wrote = unsafe {
        proc_listpids(
            PROC_ALL_PIDS,
            0,
            pids.as_mut_ptr().cast(),
            (pids.len() * 4) as i32,
        )
    };
    if wrote <= 0 {
        return None;
    }
    let mut names = Vec::new();
    for pid in pids.into_iter().take(wrote as usize / 4) {
        if pid <= 0 {
            continue;
        }
        let mut buffer = [0u8; 256];
        let len = unsafe { proc_name(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
        if len <= 0 {
            continue;
        }
        let end = (len as usize).min(buffer.len());
        let text = String::from_utf8_lossy(&buffer[..end])
            .trim_matches('\0')
            .trim()
            .to_string();
        if !text.is_empty() {
            names.push(text);
        }
    }
    Some(names)
}

#[cfg(target_os = "windows")]
fn process_names() -> Option<Vec<String>> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut names = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('"') {
            if let Some(end) = rest.find('"') {
                let name = rest[..end].trim();
                if !name.is_empty() {
                    names.push(name.to_string());
                }
            }
        }
    }
    Some(names)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn process_names() -> Option<Vec<String>> {
    None
}

#[cfg(target_os = "macos")]
fn harness_names() -> Option<Vec<String>> {
    match harness_executable_names(Path::new("/Applications/DeepSeek Harness.app")) {
        Ok(names) if !names.is_empty() => Some(names),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn harness_names() -> Option<Vec<String>> {
    Some(vec![
        "DeepSeek Harness.exe".to_string(),
        "dsh.exe".to_string(),
    ])
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn harness_names() -> Option<Vec<String>> {
    None
}

#[cfg(target_os = "macos")]
fn trash_path(path: &Path) -> Result<(), TrashFault> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    objc2::rc::autoreleasepool(|_| {
        let text = NSString::from_str(&path.to_string_lossy());
        let url = NSURL::fileURLWithPath(&text);
        NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&url, None)
            .map_err(|_| TrashFault::Other)
    })
}

#[cfg(target_os = "windows")]
fn trash_path(path: &Path) -> Result<(), TrashFault> {
    use std::os::windows::ffi::OsStrExt;

    #[repr(C)]
    struct ShFileOpStructW {
        hwnd: *mut std::ffi::c_void,
        w_func: u32,
        p_from: *const u16,
        p_to: *const u16,
        f_flags: u16,
        f_any_operations_aborted: i32,
        h_name_mappings: *mut std::ffi::c_void,
        lpsz_progress_title: *const u16,
    }

    #[link(name = "shell32")]
    extern "system" {
        fn SHFileOperationW(lp_file_op: *mut ShFileOpStructW) -> i32;
    }

    const FO_DELETE: u32 = 0x0003;
    const FOF_SILENT: u16 = 0x0004;
    const FOF_NOCONFIRMATION: u16 = 0x0010;
    const FOF_ALLOWUNDO: u16 = 0x0040;
    const FOF_NOERRORUI: u16 = 0x0400;

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    wide.push(0);

    let mut op = ShFileOpStructW {
        hwnd: std::ptr::null_mut(),
        w_func: FO_DELETE,
        p_from: wide.as_ptr(),
        p_to: std::ptr::null(),
        f_flags: FOF_SILENT | FOF_NOCONFIRMATION | FOF_ALLOWUNDO | FOF_NOERRORUI,
        f_any_operations_aborted: 0,
        h_name_mappings: std::ptr::null_mut(),
        lpsz_progress_title: std::ptr::null(),
    };
    let code = unsafe { SHFileOperationW(&mut op) };
    if code == 0 && op.f_any_operations_aborted == 0 {
        Ok(())
    } else if code == 5 {
        Err(TrashFault::Permission)
    } else if code == 32 {
        Err(TrashFault::Busy)
    } else {
        Err(TrashFault::Other)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn trash_path(path: &Path) -> Result<(), TrashFault> {
    let _ = path;
    Err(TrashFault::Other)
}

#[cfg(test)]
mod tests {
    #[test]
    fn process_reader_uses_names_only() {
        let source = include_str!("runtime.rs")
            .split("mod tests")
            .next()
            .unwrap();
        assert!(source.contains("proc_listpids"));
        assert!(source.contains("proc_name"));
        assert!(source.contains("runningApplications"));
        assert!(source.contains("trashItemAtURL"));
        for banned in ["proc_pidpath", "KERN_PROCARGS", "environ", "sysctl"] {
            assert!(!source.contains(banned), "{banned}");
        }
    }

    #[test]
    fn trash_support_matches_the_platform() {
        assert_eq!(
            super::trash_supported(),
            cfg!(any(target_os = "macos", target_os = "windows"))
        );
    }
}
