use std::path::Path;

use wanxiang_core::{harness_executable_names, TrashFault, Trasher};

pub struct Observed {
    pub bundle_ids: Option<Vec<String>>,
    pub process_names: Option<Vec<String>>,
    pub harness_executables: Option<Vec<String>>,
}

pub fn trash_supported() -> bool {
    cfg!(target_os = "macos")
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

#[cfg(not(target_os = "macos"))]
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

#[cfg(not(target_os = "macos"))]
fn process_names() -> Option<Vec<String>> {
    None
}

fn harness_names() -> Option<Vec<String>> {
    match harness_executable_names(Path::new("/Applications/DeepSeek Harness.app")) {
        Ok(names) if !names.is_empty() => Some(names),
        _ => None,
    }
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

#[cfg(not(target_os = "macos"))]
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
        assert_eq!(super::trash_supported(), cfg!(target_os = "macos"));
    }
}
