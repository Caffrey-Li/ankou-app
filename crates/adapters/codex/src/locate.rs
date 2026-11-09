use std::path::{Path, PathBuf};

const EXTRA_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];

pub fn find_codex_cli(path_env: &str) -> Option<PathBuf> {
    find_codex_cli_in(path_env, EXTRA_DIRS)
}

pub fn find_codex_cli_in(path_env: &str, extras: &[&str]) -> Option<PathBuf> {
    let separator = if cfg!(windows) { ';' } else { ':' };
    let name = if cfg!(windows) { "codex.exe" } else { "codex" };
    let mut dirs: Vec<&str> = path_env.split(separator).filter(|dir| !dir.is_empty()).collect();
    for extra in extras {
        if !dirs.contains(extra) {
            dirs.push(*extra);
        }
    }
    dirs.into_iter().find_map(|dir| exact_executable(Path::new(dir), name))
}

fn exact_executable(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        if entry.file_name() != name {
            continue;
        }
        let path = entry.path();
        if executable(&path) {
            return Some(path);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn extra_dirs_are_only_the_two_fixed_bins() {
        assert_eq!(super::EXTRA_DIRS, ["/opt/homebrew/bin", "/usr/local/bin"]);
        let source = include_str!("locate.rs");
        assert!(!source.contains(&format!("Contents/{}", "MacOS")));
        assert!(!source.contains(&format!("Contents/{}", "Resources")));
    }
}

fn executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return path
            .metadata()
            .map(|meta| meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    #[cfg(not(unix))]
    {
        true
    }
}
