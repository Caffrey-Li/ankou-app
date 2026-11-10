use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

pub const CONFIRM_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolId {
    #[serde(rename = "deepseek_harness")]
    DeepSeekHarness,
    DsBot,
    Cursor,
    Codex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Usage,
    Conversation,
    AccountLink,
    CreateConversation,
    InsertContent,
    ProtectedLocation,
    Cleanup,
    SpaceScan,
}

impl Capability {
    pub fn is_write(self) -> bool {
        matches!(
            self,
            Self::CreateConversation | Self::InsertContent | Self::Cleanup
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GrantId(u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathScope {
    pub root: PathBuf,
    pub pattern: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    pub id: GrantId,
    pub tool: ToolId,
    pub capability: Capability,
    pub scope: PathScope,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileMeta {
    pub len: u64,
    pub modified: SystemTime,
    pub is_dir: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ConfirmToken {
    id: [u8; 16],
}

impl ConfirmToken {
    pub fn to_hex(&self) -> String {
        self.id.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    pub fn from_hex(text: &str) -> Option<Self> {
        if text.len() != 32 {
            return None;
        }
        let mut id = [0u8; 16];
        for (index, byte) in id.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).ok()?;
        }
        Some(Self { id })
    }
}

impl std::fmt::Debug for ConfirmToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConfirmToken(..)")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeychainEntry {
    pub service: String,
    pub account: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialKind {
    AuthJson,
    CredentialsYaml,
    CursorAuthKey,
    WorkBuddyModels,
    Keychain,
    CredentialsComponent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardError {
    RootMissing,
    RootNotAbsolute,
    RootNotDirectory,
    InvalidPattern,
    UnregisteredRoot,
    PatternMismatch,
    PathEscape,
    SymlinkEscape,
    NotAFile,
    CredentialDenied(CredentialKind),
    GrantNotFound,
    GrantRevoked,
    NotGranted,
    NotAWriteAction,
    ContentMismatch,
    ConfirmationVoid,
    ConfirmationUsed,
    ConfirmationUnknown,
    ConfirmationExpired,
    NeedsReauth,
    Io(io::ErrorKind),
}

impl std::fmt::Display for GuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::RootMissing => "根目录不存在",
            Self::RootNotAbsolute => "根目录不是绝对路径",
            Self::RootNotDirectory => "根目录不是目录",
            Self::InvalidPattern => "文件模式无效",
            Self::UnregisteredRoot => "路径不在已登记的根目录内",
            Self::PatternMismatch => "路径与文件模式不匹配",
            Self::PathEscape => "路径超出根目录",
            Self::SymlinkEscape => "根目录或符号链接已变化",
            Self::NotAFile => "路径不是文件",
            Self::CredentialDenied(_) => "凭据路径被拒绝",
            Self::GrantNotFound => "授权不存在",
            Self::GrantRevoked => "授权已撤销",
            Self::NotGranted => "没有这项授权",
            Self::NotAWriteAction => "这项能力不需要确认令牌",
            Self::ContentMismatch => "确认内容已变化",
            Self::ConfirmationVoid => "确认令牌已作废",
            Self::ConfirmationUsed => "确认令牌已使用",
            Self::ConfirmationUnknown => "确认令牌不存在",
            Self::ConfirmationExpired => "确认令牌已过期",
            Self::NeedsReauth => "需要重新授权",
            Self::Io(kind) => io_kind_text(*kind),
        })
    }
}

impl std::error::Error for GuardError {}

fn io_kind_text(kind: io::ErrorKind) -> &'static str {
    match kind {
        io::ErrorKind::NotFound => "文件不存在",
        io::ErrorKind::PermissionDenied => "没有权限",
        io::ErrorKind::AlreadyExists => "文件已存在",
        io::ErrorKind::InvalidInput => "路径无效",
        io::ErrorKind::InvalidData => "数据无效",
        _ => "文件访问失败",
    }
}

fn io_err(err: io::Error) -> GuardError {
    GuardError::Io(err.kind())
}

pub trait Clock: Send + Sync {
    fn now(&self) -> SystemTime;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

pub trait ToolScheduler: Send + Sync {
    fn on_revoked(&self, tool: ToolId, capability: Capability);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootStamp {
    pub path: PathBuf,
    pub dev: u64,
    pub ino: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileId {
    dev: u64,
    ino: u64,
}

#[derive(Clone)]
enum Seg {
    GlobStar,
    Wild(String),
}

#[derive(Clone)]
struct CompiledPattern {
    segs: Vec<Seg>,
}

struct GrantRecord {
    id: GrantId,
    tool: ToolId,
    capability: Capability,
    root: PathBuf,
    file_id: FileId,
    pattern_text: String,
    pattern: CompiledPattern,
    alive: Arc<AtomicBool>,
}

struct Snap {
    root: PathBuf,
    file_id: FileId,
    pattern: CompiledPattern,
    alive: Arc<AtomicBool>,
}

#[derive(Clone, Copy)]
enum TokenState {
    Live,
    Used,
    Void,
}

struct TokenRecord {
    tool: ToolId,
    capability: Capability,
    hash: [u8; 32],
    issued_at: SystemTime,
    grant_ids: Vec<GrantId>,
    state: TokenState,
    unscoped: bool,
}

struct Inner {
    next_id: u64,
    grants: Vec<GrantRecord>,
    tokens: HashMap<[u8; 16], TokenRecord>,
    deepseek_account: bool,
}

pub struct Guard {
    inner: Mutex<Inner>,
    scheduler: Arc<dyn ToolScheduler>,
    clock: Arc<dyn Clock>,
}

struct Opened {
    file: File,
    alive: Arc<AtomicBool>,
    len: u64,
    modified: SystemTime,
    is_dir: bool,
}

pub struct ScopedRead {
    file: File,
    alive: Arc<AtomicBool>,
}

impl std::fmt::Debug for ScopedRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedRead(..)")
    }
}

impl Guard {
    pub fn new(scheduler: Arc<dyn ToolScheduler>, clock: Arc<dyn Clock>) -> Self {
        Self {
            inner: Mutex::new(Inner {
                next_id: 0,
                grants: Vec::new(),
                tokens: HashMap::new(),
                deepseek_account: false,
            }),
            scheduler,
            clock,
        }
    }

    pub fn grant(
        &self,
        tool: ToolId,
        capability: Capability,
        scope: PathScope,
    ) -> Result<GrantId, GuardError> {
        let pattern = compile_pattern(&scope.pattern)?;
        let root = canonical_directory(&scope.root)?;
        let file_id = file_id_of(&root).map_err(io_err)?;
        let mut inner = self.lock();
        if let Some(existing) = inner.grants.iter().find(|grant| {
            grant.tool == tool
                && grant.capability == capability
                && grant.root == root
                && grant.pattern_text == scope.pattern
        }) {
            return Ok(existing.id);
        }
        inner.next_id += 1;
        let id = GrantId(inner.next_id);
        inner.grants.push(GrantRecord {
            id,
            tool,
            capability,
            root,
            file_id,
            pattern_text: scope.pattern,
            pattern,
            alive: Arc::new(AtomicBool::new(true)),
        });
        Ok(id)
    }

    pub fn stamp(&self, id: GrantId) -> Result<RootStamp, GuardError> {
        let inner = self.lock();
        let grant = inner
            .grants
            .iter()
            .find(|grant| grant.id == id)
            .ok_or(GuardError::GrantNotFound)?;
        Ok(RootStamp {
            path: grant.root.clone(),
            dev: grant.file_id.dev,
            ino: grant.file_id.ino,
        })
    }

    pub fn restore(
        &self,
        tool: ToolId,
        capability: Capability,
        scope: PathScope,
        expected: &RootStamp,
    ) -> Result<GrantId, GuardError> {
        let root = canonical_directory(&scope.root)?;
        let file_id = file_id_of(&root).map_err(io_err)?;
        if root != expected.path || file_id.dev != expected.dev || file_id.ino != expected.ino {
            return Err(GuardError::NeedsReauth);
        }
        self.grant(tool, capability, scope)
    }

    pub fn grants(&self) -> Vec<Grant> {
        self.lock().grants.iter().map(to_grant).collect()
    }

    pub fn grants_for(&self, tool: ToolId) -> Vec<Grant> {
        self.grants()
            .into_iter()
            .filter(|grant| grant.tool == tool)
            .collect()
    }

    pub fn revoke(&self, id: GrantId) -> Result<(), GuardError> {
        let (tool, capability) = {
            let mut inner = self.lock();
            let pos = inner
                .grants
                .iter()
                .position(|grant| grant.id == id)
                .ok_or(GuardError::GrantNotFound)?;
            let grant = inner.grants.remove(pos);
            grant.alive.store(false, Ordering::SeqCst);
            (grant.tool, grant.capability)
        };
        self.scheduler.on_revoked(tool, capability);
        Ok(())
    }

    pub fn account_link_active(&self, tool: ToolId) -> bool {
        matches!(tool, ToolId::DeepSeekHarness | ToolId::DsBot) && self.lock().deepseek_account
    }

    pub fn set_account_link(&self, active: bool) {
        self.lock().deepseek_account = active;
    }

    pub fn revoke_account_link(&self) {
        let was = {
            let mut inner = self.lock();
            let was = inner.deepseek_account;
            inner.deepseek_account = false;
            was
        };
        if was {
            self.scheduler
                .on_revoked(ToolId::DeepSeekHarness, Capability::AccountLink);
            self.scheduler
                .on_revoked(ToolId::DsBot, Capability::AccountLink);
        }
    }

    pub fn open_read(
        &self,
        tool: ToolId,
        capability: Capability,
        path: &Path,
    ) -> Result<ScopedRead, GuardError> {
        if capability == Capability::AccountLink || capability == Capability::SpaceScan {
            return Err(GuardError::NotGranted);
        }
        let opened = self.open_checked(tool, capability, path, false)?;
        Ok(ScopedRead {
            file: opened.file,
            alive: opened.alive,
        })
    }

    pub fn metadata(
        &self,
        tool: ToolId,
        capability: Capability,
        path: &Path,
    ) -> Result<FileMeta, GuardError> {
        let opened = self.open_checked(tool, capability, path, true)?;
        Ok(FileMeta {
            len: opened.len,
            modified: opened.modified,
            is_dir: opened.is_dir,
        })
    }

    pub fn issue_confirmation(
        &self,
        tool: ToolId,
        capability: Capability,
        content: &[u8],
    ) -> Result<ConfirmToken, GuardError> {
        if !capability.is_write() {
            return Err(GuardError::NotAWriteAction);
        }
        let mut id = [0u8; 16];
        getrandom::fill(&mut id).map_err(|_| GuardError::Io(io::ErrorKind::Other))?;
        let issued_at = self.clock.now();
        let mut inner = self.lock();
        let grant_ids: Vec<GrantId> = inner
            .grants
            .iter()
            .filter(|grant| grant.tool == tool && grant.capability == capability)
            .map(|grant| grant.id)
            .collect();
        if grant_ids.is_empty() {
            return Err(GuardError::NotGranted);
        }
        inner.tokens.insert(
            id,
            TokenRecord {
                tool,
                capability,
                hash: plan_hash(tool, capability, content),
                issued_at,
                grant_ids,
                state: TokenState::Live,
                unscoped: false,
            },
        );
        Ok(ConfirmToken { id })
    }

    pub fn has_grant(&self, tool: ToolId, capability: Capability) -> bool {
        let inner = self.lock();
        inner.grants.iter().any(|grant| {
            grant.tool == tool
                && grant.capability == capability
                && grant.alive.load(Ordering::Acquire)
        })
    }

    pub fn issue_summary_confirmation(&self, content: &[u8]) -> Result<ConfirmToken, GuardError> {
        let mut id = [0u8; 16];
        getrandom::fill(&mut id).map_err(|_| GuardError::Io(io::ErrorKind::Other))?;
        let issued_at = self.clock.now();
        let mut inner = self.lock();
        inner.tokens.insert(
            id,
            TokenRecord {
                tool: ToolId::DeepSeekHarness,
                capability: Capability::AccountLink,
                hash: content_hash(content),
                issued_at,
                grant_ids: Vec::new(),
                state: TokenState::Live,
                unscoped: true,
            },
        );
        Ok(ConfirmToken { id })
    }

    pub fn redeem_summary_confirmation(
        &self,
        token: &ConfirmToken,
        content: &[u8],
    ) -> Result<(), GuardError> {
        let now = self.clock.now();
        let mut inner = self.lock();
        let Some(record) = inner.tokens.get(&token.id) else {
            return Err(GuardError::ConfirmationUnknown);
        };
        if !record.unscoped {
            return Err(GuardError::ConfirmationUnknown);
        }
        match record.state {
            TokenState::Used => return Err(GuardError::ConfirmationUsed),
            TokenState::Void => return Err(GuardError::ConfirmationVoid),
            TokenState::Live => {}
        }
        if expired(record.issued_at, now) {
            return Err(GuardError::ConfirmationExpired);
        }
        if record.hash != content_hash(content) {
            inner.tokens.get_mut(&token.id).expect("token").state = TokenState::Void;
            return Err(GuardError::ContentMismatch);
        }
        inner.tokens.get_mut(&token.id).expect("token").state = TokenState::Used;
        Ok(())
    }

    pub fn redeem_confirmation(
        &self,
        token: &ConfirmToken,
        tool: ToolId,
        capability: Capability,
        content: &[u8],
    ) -> Result<(), GuardError> {
        let now = self.clock.now();
        let mut inner = self.lock();
        let Some((state, stored_tool, stored_capability, hash, issued_at, grant_ids, unscoped)) =
            inner.tokens.get(&token.id).map(|record| {
                (
                    record.state,
                    record.tool,
                    record.capability,
                    record.hash,
                    record.issued_at,
                    record.grant_ids.clone(),
                    record.unscoped,
                )
            })
        else {
            return Err(GuardError::ConfirmationUnknown);
        };
        if unscoped {
            return Err(GuardError::ConfirmationUnknown);
        }
        match state {
            TokenState::Used => return Err(GuardError::ConfirmationUsed),
            TokenState::Void => return Err(GuardError::ConfirmationVoid),
            TokenState::Live => {}
        }
        if expired(issued_at, now) {
            return Err(GuardError::ConfirmationExpired);
        }
        if tool != stored_tool
            || capability != stored_capability
            || hash != plan_hash(tool, capability, content)
        {
            inner.tokens.get_mut(&token.id).expect("token").state = TokenState::Void;
            return Err(GuardError::ContentMismatch);
        }
        let still_granted = grant_ids
            .iter()
            .any(|id| inner.grants.iter().any(|grant| grant.id == *id));
        if !still_granted {
            return Err(GuardError::NotGranted);
        }
        inner.tokens.get_mut(&token.id).expect("token").state = TokenState::Used;
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("guard lock")
    }

    fn open_checked(
        &self,
        tool: ToolId,
        capability: Capability,
        path: &Path,
        allow_dir: bool,
    ) -> Result<Opened, GuardError> {
        if has_parent_dir(path) {
            return Err(GuardError::PathEscape);
        }
        if let Some(kind) = credential_kind(path) {
            return Err(GuardError::CredentialDenied(kind));
        }
        let snaps: Vec<Snap> = {
            let inner = self.lock();
            inner
                .grants
                .iter()
                .filter(|grant| grant.tool == tool && grant.capability == capability)
                .map(Snap::from)
                .collect()
        };

        let mut lexical_hit: Option<(usize, PathBuf)> = None;
        let mut lexical_inside = false;
        for (index, snap) in snaps.iter().enumerate() {
            match lexical_within(&snap.root, path) {
                Ok(lexical) => {
                    lexical_inside = true;
                    root_still(&snap.root, snap.file_id)?;
                    let rel = relative_path(&snap.root, &lexical).unwrap_or_default();
                    if pattern_matches(&snap.pattern, &rel) {
                        lexical_hit = Some((index, lexical));
                        break;
                    }
                }
                Err(GuardError::UnregisteredRoot) => {}
                Err(err) => return Err(err),
            }
        }
        if lexical_inside && lexical_hit.is_none() {
            return Err(GuardError::PatternMismatch);
        }
        if lexical_hit.is_none() && !alias_inside_grant(path, &snaps)? {
            return Err(GuardError::UnregisteredRoot);
        }

        // 相对路径拼到授权根上再打开，避免落到进程当前目录。
        let open_path = lexical_hit
            .as_ref()
            .map(|(_, lexical)| lexical.as_path())
            .unwrap_or(path);
        let file = open_handle(open_path).map_err(io_err)?;
        let real = match path_from_handle(&file) {
            Ok(real) => strip_verbatim(&real),
            Err(err) => return Err(io_err(err)),
        };
        // ponytail: 硬链接用另一个名字指向凭据文件时，句柄返回的是打开时的路径，认不出另一个名字。
        // 升级路径是再按 (dev, inode) 比对已见过的凭据文件。这个缺口记在 REVIEW-M1a。
        if let Some(kind) = credential_kind(&real) {
            return Err(GuardError::CredentialDenied(kind));
        }

        let chosen = if let Some((index, _)) = lexical_hit {
            let snap = &snaps[index];
            if !is_within(&snap.root, &real) {
                return Err(GuardError::SymlinkEscape);
            }
            let rel = relative_path(&snap.root, &real).ok_or(GuardError::SymlinkEscape)?;
            if !pattern_matches(&snap.pattern, &rel) {
                return Err(GuardError::PatternMismatch);
            }
            root_still(&snap.root, snap.file_id)?;
            index
        } else {
            let mut found = None;
            for (index, snap) in snaps.iter().enumerate() {
                if root_still(&snap.root, snap.file_id).is_err() || !is_within(&snap.root, &real) {
                    continue;
                }
                let rel = relative_path(&snap.root, &real).unwrap_or_default();
                if pattern_matches(&snap.pattern, &rel) {
                    found = Some(index);
                    break;
                }
            }
            found.ok_or(GuardError::UnregisteredRoot)?
        };
        // File::metadata 在 Unix 上是 fstat。只接受普通文件；metadata 另外接受目录。
        let meta = file.metadata().map_err(io_err)?;
        let kind = meta.file_type();
        if !(kind.is_file() || (allow_dir && kind.is_dir())) {
            return Err(GuardError::NotAFile);
        }
        let modified = meta.modified().map_err(io_err)?;
        Ok(Opened {
            alive: Arc::clone(&snaps[chosen].alive),
            len: meta.len(),
            modified,
            is_dir: meta.is_dir(),
            file,
        })
    }
}

impl ScopedRead {
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, GuardError> {
        if !self.alive.load(Ordering::SeqCst) {
            return Err(GuardError::GrantRevoked);
        }
        self.file.read(buf).map_err(io_err)
    }
}

impl Snap {
    fn from(grant: &GrantRecord) -> Self {
        Self {
            root: grant.root.clone(),
            file_id: grant.file_id,
            pattern: grant.pattern.clone(),
            alive: Arc::clone(&grant.alive),
        }
    }
}

fn to_grant(grant: &GrantRecord) -> Grant {
    Grant {
        id: grant.id,
        tool: grant.tool,
        capability: grant.capability,
        scope: PathScope {
            root: grant.root.clone(),
            pattern: grant.pattern_text.clone(),
        },
    }
}

pub fn lexical_within(root: &Path, candidate: &Path) -> Result<PathBuf, GuardError> {
    if has_parent_dir(candidate) || has_parent_dir(root) {
        return Err(GuardError::PathEscape);
    }
    let root = strip_verbatim(root);
    let joined = if candidate.is_absolute() {
        strip_verbatim(candidate)
    } else {
        root.join(candidate)
    };
    // `\\?\` 前缀下 `..` 可能只是普通分量；去掉前缀后要再查一次。
    if has_parent_dir(&root) || has_parent_dir(&joined) {
        return Err(GuardError::PathEscape);
    }
    let normalized = collapse_curdir(&joined)?;
    if !is_within(&root, &normalized) {
        return Err(GuardError::UnregisteredRoot);
    }
    Ok(normalized)
}

pub fn classify_cursor_global_key(key: &str) -> Result<(), GuardError> {
    let lower = key.to_ascii_lowercase();
    if lower == "cursorauth" || lower.starts_with("cursorauth/") {
        Err(GuardError::CredentialDenied(CredentialKind::CursorAuthKey))
    } else {
        Ok(())
    }
}

pub fn classify_keychain(_entry: &KeychainEntry) -> Result<(), GuardError> {
    Err(GuardError::CredentialDenied(CredentialKind::Keychain))
}

fn alias_inside_grant(path: &Path, snaps: &[Snap]) -> Result<bool, GuardError> {
    if !path.is_absolute() {
        return Ok(false);
    }
    let canonical = match fs::canonicalize(path) {
        Ok(path) => strip_verbatim(&path),
        Err(err) => return Err(io_err(err)),
    };
    if let Some(kind) = credential_kind(&canonical) {
        return Err(GuardError::CredentialDenied(kind));
    }
    Ok(snaps.iter().any(|snap| {
        if root_still(&snap.root, snap.file_id).is_err() || !is_within(&snap.root, &canonical) {
            return false;
        }
        let rel = relative_path(&snap.root, &canonical).unwrap_or_default();
        pattern_matches(&snap.pattern, &rel)
    }))
}

fn canonical_directory(path: &Path) -> Result<PathBuf, GuardError> {
    if !path.is_absolute() {
        return Err(GuardError::RootNotAbsolute);
    }
    let real = match fs::canonicalize(path) {
        Ok(path) => strip_verbatim(&path),
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Err(GuardError::RootMissing),
        Err(err) => return Err(io_err(err)),
    };
    let meta = fs::metadata(&real).map_err(io_err)?;
    if !meta.is_dir() {
        return Err(GuardError::RootNotDirectory);
    }
    Ok(real)
}

fn root_still(root: &Path, file_id: FileId) -> Result<(), GuardError> {
    let meta = fs::symlink_metadata(root).map_err(|_| GuardError::SymlinkEscape)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(GuardError::SymlinkEscape);
    }
    let current = fs::canonicalize(root).map_err(|_| GuardError::SymlinkEscape)?;
    if !paths_equal(&strip_verbatim(&current), root) {
        return Err(GuardError::SymlinkEscape);
    }
    let current_id = file_id_of(root).map_err(|_| GuardError::SymlinkEscape)?;
    if current_id != file_id {
        return Err(GuardError::SymlinkEscape);
    }
    Ok(())
}

fn compile_pattern(pattern: &str) -> Result<CompiledPattern, GuardError> {
    if pattern.is_empty() || pattern.contains('\\') || Path::new(pattern).is_absolute() {
        return Err(GuardError::InvalidPattern);
    }
    let mut segs = Vec::new();
    for part in pattern.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(GuardError::InvalidPattern);
        }
        if part == "**" {
            segs.push(Seg::GlobStar);
        } else {
            segs.push(Seg::Wild(part.to_string()));
        }
    }
    Ok(CompiledPattern { segs })
}

fn pattern_matches(pattern: &CompiledPattern, rel: &Path) -> bool {
    let parts: Vec<String> = rel
        .components()
        .filter_map(|component| match component {
            Component::Normal(text) => Some(text.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    match_segs(&pattern.segs, &parts)
}

fn match_segs(segs: &[Seg], path: &[String]) -> bool {
    if segs.is_empty() {
        return path.is_empty();
    }
    match &segs[0] {
        Seg::GlobStar => {
            if match_segs(&segs[1..], path) {
                return true;
            }
            if path.is_empty() {
                return false;
            }
            match_segs(segs, &path[1..])
        }
        Seg::Wild(pattern) => {
            !path.is_empty() && seg_match(pattern, &path[0]) && match_segs(&segs[1..], &path[1..])
        }
    }
}

fn seg_match(pattern: &str, text: &str) -> bool {
    fn rec(pattern: &[u8], text: &[u8]) -> bool {
        if pattern.is_empty() {
            return text.is_empty();
        }
        if pattern[0] == b'*' {
            if rec(&pattern[1..], text) {
                return true;
            }
            return !text.is_empty() && rec(pattern, &text[1..]);
        }
        !text.is_empty() && pattern[0] == text[0] && rec(&pattern[1..], &text[1..])
    }
    rec(pattern.as_bytes(), text.as_bytes())
}

pub(crate) fn metadata_denied(path: &Path) -> bool {
    if credential_kind(path).is_some() {
        return true;
    }
    path.components().any(|component| match component {
        Component::Normal(text) => {
            normalize_name(&text.to_string_lossy()).eq_ignore_ascii_case("cursorauth")
        }
        _ => false,
    })
}

fn credential_kind(path: &Path) -> Option<CredentialKind> {
    let names: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(text) => Some(normalize_name(&text.to_string_lossy())),
            _ => None,
        })
        .collect();
    let last = names.last()?;
    if last.eq_ignore_ascii_case("auth.json") {
        return Some(CredentialKind::AuthJson);
    }
    if last.eq_ignore_ascii_case(".credentials.yaml") {
        return Some(CredentialKind::CredentialsYaml);
    }
    if last.eq_ignore_ascii_case("models.json")
        && names[..names.len() - 1]
            .iter()
            .any(|name| is_workbuddy_dir(name))
    {
        return Some(CredentialKind::WorkBuddyModels);
    }
    if names.iter().any(|name| {
        name.eq_ignore_ascii_case("credentials")
            || name.to_ascii_lowercase().starts_with(".credentials")
    }) {
        return Some(CredentialKind::CredentialsComponent);
    }
    None
}

fn is_workbuddy_dir(name: &str) -> bool {
    name.trim_start_matches('.')
        .eq_ignore_ascii_case("workbuddy")
}

fn normalize_name(name: &str) -> String {
    let mut text = match name.split_once(':') {
        Some((file, _)) => file.to_string(),
        None => name.to_string(),
    };
    while text.ends_with(' ') || text.ends_with('.') {
        text.pop();
    }
    text
}

fn has_parent_dir(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::ParentDir => true,
        Component::Normal(text) => text == "..",
        _ => false,
    })
}

fn collapse_curdir(path: &Path) -> Result<PathBuf, GuardError> {
    let mut out = PathBuf::new();
    for component in strip_verbatim(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => return Err(GuardError::PathEscape),
            Component::Normal(text) if text == ".." => return Err(GuardError::PathEscape),
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(std::path::MAIN_SEPARATOR_STR),
            Component::Normal(text) => out.push(text),
        }
    }
    Ok(out)
}

#[derive(PartialEq, Eq)]
enum Piece {
    Prefix(String),
    Root,
    Normal(String),
}

fn pieces(path: &Path) -> Vec<Piece> {
    strip_verbatim(path)
        .components()
        .filter_map(|component| match component {
            Component::Prefix(prefix) => Some(Piece::Prefix(fold(prefix.as_os_str()))),
            Component::RootDir => Some(Piece::Root),
            Component::CurDir => None,
            Component::ParentDir => Some(Piece::Normal("..".to_string())),
            Component::Normal(text) => Some(Piece::Normal(fold(text))),
        })
        .collect()
}

fn fold(text: &std::ffi::OsStr) -> String {
    let text = text.to_string_lossy();
    #[cfg(windows)]
    {
        text.to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        text.into_owned()
    }
}

fn is_within(root: &Path, path: &Path) -> bool {
    let root = pieces(root);
    let path = pieces(path);
    path.len() >= root.len() && path[..root.len()] == root[..]
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    pieces(left) == pieces(right)
}

fn relative_path(root: &Path, path: &Path) -> Option<PathBuf> {
    let root = pieces(root);
    let path = pieces(path);
    if path.len() < root.len() || path[..root.len()] != root[..] {
        return None;
    }
    let mut rel = PathBuf::new();
    for piece in &path[root.len()..] {
        if let Piece::Normal(name) = piece {
            rel.push(name);
        }
    }
    Some(rel)
}

fn strip_verbatim(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path.to_path_buf()
}

fn open_handle(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // macOS: O_NONBLOCK 0x4，O_CLOEXEC 0x01000000。
        // Linux: O_NONBLOCK 2048，O_CLOEXEC 0x80000。
        // 其余 Unix 用 BSD 的 0x4 和 0x00100000。
        #[cfg(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "tvos",
            target_os = "watchos"
        ))]
        let flags = 0x0000_0004 | 0x0100_0000;
        #[cfg(target_os = "linux")]
        let flags = 2048 | 0x8_0000;
        #[cfg(not(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "tvos",
            target_os = "watchos",
            target_os = "linux"
        )))]
        let flags = 0x4 | 0x0010_0000;
        options.custom_flags(flags);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0200_0000);
    }
    options.open(path)
}

fn file_id_of(path: &Path) -> io::Result<FileId> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = fs::metadata(path)?;
        Ok(FileId {
            dev: meta.dev(),
            ino: meta.ino(),
        })
    }
    #[cfg(windows)]
    {
        let file = open_handle(path)?;
        let (dev, ino) = windows_file_id(&file)?;
        Ok(FileId { dev, ino })
    }
}

fn path_from_handle(file: &File) -> io::Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        const F_GETPATH: i32 = 50;
        let mut buf = [0u8; 1024];
        let rc = unsafe { fcntl(file.as_raw_fd(), F_GETPATH, buf.as_mut_ptr()) };
        if rc == -1 {
            return Err(io::Error::last_os_error());
        }
        let text = std::ffi::CStr::from_bytes_until_nul(&buf)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "path"))?
            .to_str()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "path"))?;
        Ok(PathBuf::from(text))
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd()))
    }
    #[cfg(windows)]
    {
        windows_final_path(file)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = file;
        Err(io::Error::new(io::ErrorKind::Unsupported, "platform"))
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn fcntl(fd: i32, cmd: i32, ...) -> i32;
}

#[cfg(windows)]
fn windows_file_id(file: &File) -> io::Result<(u64, u64)> {
    use std::os::windows::io::AsRawHandle;
    let mut info = ByHandleFileInformation::zero();
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let ino = ((info.index_high as u64) << 32) | u64::from(info.index_low);
    Ok((u64::from(info.volume_serial), ino))
}

#[cfg(windows)]
fn windows_final_path(file: &File) -> io::Result<PathBuf> {
    use std::os::windows::io::AsRawHandle;
    let mut buf = vec![0u16; 32768];
    let n = unsafe {
        GetFinalPathNameByHandleW(file.as_raw_handle(), buf.as_mut_ptr(), buf.len() as u32, 0)
    };
    if n == 0 || n as usize >= buf.len() {
        return Err(io::Error::last_os_error());
    }
    let text = String::from_utf16(&buf[..n as usize])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "utf16"))?;
    Ok(PathBuf::from(text))
}

#[cfg(windows)]
#[repr(C)]
struct Filetime {
    low: u32,
    high: u32,
}

#[cfg(windows)]
#[repr(C)]
struct ByHandleFileInformation {
    attributes: u32,
    creation: Filetime,
    last_access: Filetime,
    last_write: Filetime,
    volume_serial: u32,
    size_high: u32,
    size_low: u32,
    links: u32,
    index_high: u32,
    index_low: u32,
}

#[cfg(windows)]
impl ByHandleFileInformation {
    fn zero() -> Self {
        Self {
            attributes: 0,
            creation: Filetime { low: 0, high: 0 },
            last_access: Filetime { low: 0, high: 0 },
            last_write: Filetime { low: 0, high: 0 },
            volume_serial: 0,
            size_high: 0,
            size_low: 0,
            links: 0,
            index_high: 0,
            index_low: 0,
        }
    }
}

#[cfg(windows)]
unsafe extern "system" {
    fn GetFileInformationByHandle(
        file: *mut std::ffi::c_void,
        info: *mut ByHandleFileInformation,
    ) -> i32;
    fn GetFinalPathNameByHandleW(
        file: *mut std::ffi::c_void,
        buf: *mut u16,
        size: u32,
        flags: u32,
    ) -> u32;
}

fn content_hash(content: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"summary");
    hasher.update((content.len() as u64).to_le_bytes());
    hasher.update(content);
    hasher.finalize().into()
}

fn plan_hash(tool: ToolId, capability: Capability, content: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([tool_tag(tool), capability_tag(capability)]);
    hasher.update((content.len() as u64).to_le_bytes());
    hasher.update(content);
    hasher.finalize().into()
}

fn tool_tag(tool: ToolId) -> u8 {
    match tool {
        ToolId::DeepSeekHarness => 1,
        ToolId::DsBot => 2,
        ToolId::Cursor => 3,
        ToolId::Codex => 4,
    }
}

fn capability_tag(capability: Capability) -> u8 {
    match capability {
        Capability::Usage => 1,
        Capability::Conversation => 2,
        Capability::AccountLink => 3,
        Capability::CreateConversation => 4,
        Capability::InsertContent => 5,
        Capability::ProtectedLocation => 6,
        Capability::Cleanup => 7,
        Capability::SpaceScan => 8,
    }
}

fn expired(issued_at: SystemTime, now: SystemTime) -> bool {
    match now.duration_since(issued_at) {
        Ok(elapsed) => elapsed >= CONFIRM_TTL,
        Err(_) => false,
    }
}
