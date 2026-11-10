use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use wanxiang_core::{
    authorize_summary, cursor_prompt_url, organize_local, place_deeplink, prepare_deeplink, redeem_summary,
    summary_body, write_export, write_export_if_chosen, Capability, Clock, ConfirmToken, DeeplinkError, Guard,
    PathScope, SummaryError, ToolId, ToolScheduler, CONFIRM_TTL, DEEPLINK_CHARS,
};

struct Nop;

impl ToolScheduler for Nop {
    fn on_revoked(&self, _tool: ToolId, _capability: Capability) {}
}

struct FakeClock {
    now: Mutex<SystemTime>,
}

impl FakeClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            now: Mutex::new(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
        })
    }

    fn advance(&self, by: Duration) {
        *self.now.lock().unwrap() += by;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> SystemTime {
        *self.now.lock().unwrap()
    }
}

fn guard(clock: Arc<dyn Clock>) -> Guard {
    Guard::new(Arc::new(Nop), clock)
}

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("wanxiang-m3-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn summary_needs_account_and_key_but_not_insert_content() {
    let guard = guard(Arc::new(wanxiang_core::SystemClock));
    let body = summary_body("材料");
    assert!(matches!(
        authorize_summary(&guard, false, true, body.as_bytes()),
        Err(SummaryError::NoAccount)
    ));
    assert!(matches!(
        authorize_summary(&guard, true, false, body.as_bytes()),
        Err(SummaryError::NoKey)
    ));
    let token = authorize_summary(&guard, true, true, body.as_bytes()).unwrap();
    redeem_summary(&guard, &token, body.as_bytes()).unwrap();
    assert!(matches!(
        redeem_summary(&guard, &token, body.as_bytes()),
        Err(SummaryError::Token(wanxiang_core::GuardError::ConfirmationUsed))
    ));
}

#[test]
fn summary_token_rejects_a_different_body() {
    let guard = guard(Arc::new(wanxiang_core::SystemClock));
    let body = summary_body("材料");
    let token = authorize_summary(&guard, true, true, body.as_bytes()).unwrap();
    let err = redeem_summary(&guard, &token, b"other").unwrap_err();
    assert!(matches!(err, SummaryError::Token(wanxiang_core::GuardError::ContentMismatch)));
    assert!(redeem_summary(&guard, &token, body.as_bytes()).is_err());
}

#[test]
fn summary_token_expires() {
    let clock = FakeClock::new();
    let guard = guard(clock.clone());
    let body = summary_body("材料");
    let token = authorize_summary(&guard, true, true, body.as_bytes()).unwrap();
    clock.advance(CONFIRM_TTL + Duration::from_secs(1));
    assert!(matches!(
        redeem_summary(&guard, &token, body.as_bytes()),
        Err(SummaryError::Token(wanxiang_core::GuardError::ConfirmationExpired))
    ));
}

#[test]
fn deeplink_needs_create_conversation_and_a_token() {
    let dir = temp("deeplink");
    let guard = guard(Arc::new(wanxiang_core::SystemClock));
    assert!(matches!(prepare_deeplink(&guard, true, "短文"), Err(DeeplinkError::NotGranted)));
    guard
        .grant(
            ToolId::Cursor,
            Capability::CreateConversation,
            PathScope {
                root: dir.clone(),
                pattern: "sessions/**/*.jsonl".to_string(),
            },
        )
        .unwrap();
    assert!(matches!(prepare_deeplink(&guard, false, "短文"), Err(DeeplinkError::Off)));
    let token = prepare_deeplink(&guard, true, "短文").unwrap();
    let mut opened = Vec::new();
    place_deeplink(&guard, &token, "短文", |url| {
        opened.push(url.to_string());
        true
    })
    .unwrap();
    assert_eq!(opened.len(), 1);
    assert!(opened[0].starts_with("cursor://anysphere.cursor-deeplink/prompt?text="));
    assert!(place_deeplink(&guard, &token, "短文", |_| {
        opened.push("again".to_string());
        true
    })
    .is_err());
    assert_eq!(opened.len(), 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn place_deeplink_does_not_open_without_a_prepared_token() {
    let dir = temp("place");
    let guard = guard(Arc::new(wanxiang_core::SystemClock));
    guard
        .grant(
            ToolId::Cursor,
            Capability::CreateConversation,
            PathScope {
                root: dir.clone(),
                pattern: "sessions/**/*.jsonl".to_string(),
            },
        )
        .unwrap();
    let mut opened = 0;
    let unknown = ConfirmToken::from_hex("00112233445566778899aabbccddeeff").unwrap();
    assert!(place_deeplink(&guard, &unknown, "短文", |_| {
        opened += 1;
        true
    })
    .is_err());
    let token = prepare_deeplink(&guard, true, "短文").unwrap();
    assert!(place_deeplink(&guard, &token, "另一段", |_| {
        opened += 1;
        true
    })
    .is_err());
    assert_eq!(opened, 0, "没有令牌或哈希不一致时不会打开");
    let again = prepare_deeplink(&guard, true, "短文").unwrap();
    place_deeplink(&guard, &again, "短文", |_| {
        opened += 1;
        true
    })
    .unwrap();
    assert_eq!(opened, 1);
    assert!(place_deeplink(&guard, &again, "短文", |_| {
        opened += 1;
        true
    })
    .is_err());
    assert_eq!(opened, 1, "令牌重复使用时不会再次打开");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn prepare_deeplink_does_not_redeem_in_the_same_call() {
    let dir = temp("split");
    let guard = guard(Arc::new(wanxiang_core::SystemClock));
    guard
        .grant(
            ToolId::Cursor,
            Capability::CreateConversation,
            PathScope {
                root: dir.clone(),
                pattern: "sessions/**/*.jsonl".to_string(),
            },
        )
        .unwrap();
    let token = prepare_deeplink(&guard, true, "短文").unwrap();
    place_deeplink(&guard, &token, "短文", |_| true).unwrap();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_writes_the_chosen_path_without_a_token() {
    let dir = temp("export");
    let path = dir.join("out.md");
    write_export(&path, "交接".as_bytes()).unwrap();
    assert_eq!(fs::read(&path).unwrap(), "交接".as_bytes());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_dialog_cancel_writes_nothing() {
    let dir = temp("cancel");
    assert!(!write_export_if_chosen("交接正文", None).unwrap());
    assert!(fs::read_dir(&dir).unwrap().next().is_none());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn handoff_text_does_not_land_in_the_database_or_logs() {
    let dir = temp("persist");
    let db = dir.join("wanxiang.sqlite");
    let logs = dir.join("logs");
    fs::write(&db, b"seed").unwrap();
    fs::create_dir(&logs).unwrap();
    let marker = "交接正文标记-不会入库";
    let before = fs::read(&db).unwrap();
    let _ = organize_local(marker);
    let body = summary_body(marker);
    assert!(body.contains(marker));
    assert_eq!(fs::read(&db).unwrap(), before);
    assert!(fs::read_dir(&logs).unwrap().next().is_none());
    assert!(!fs::read(&db).unwrap().windows(marker.len()).any(|window| window == marker.as_bytes()));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn eight_thousand_ascii_characters_build_a_url() {
    let text = "a".repeat(DEEPLINK_CHARS);
    let link = cursor_prompt_url(&text);
    assert!(link.url.is_some());
    assert!(!link.over_chars);
    let over = cursor_prompt_url(&"a".repeat(DEEPLINK_CHARS + 1));
    assert!(over.url.is_none());
    assert!(over.over_chars);
}

#[test]
fn chinese_characters_are_not_counted_as_bytes() {
    let text = "中".repeat(DEEPLINK_CHARS);
    assert_eq!(text.chars().count(), DEEPLINK_CHARS);
    assert!(text.len() > DEEPLINK_CHARS);
    let link = cursor_prompt_url(&text);
    assert!(!link.over_chars);
    assert!(link.over_url);
    assert!(link.url.is_none());
    let short = cursor_prompt_url(&"中".repeat(20));
    assert!(short.url.is_some(), "少量中文仍应能构造深链");
}

proptest::proptest! {
    #[test]
    fn character_limit_ignores_byte_length(extra in 0usize..40) {
        let text = "中".repeat(8000 + extra);
        let link = cursor_prompt_url(&text);
        proptest::prop_assert_eq!(link.over_chars, text.chars().count() > 8000);
        proptest::prop_assert!(text.len() > 8000);
    }

    #[test]
    fn summary_tokens_reject_mismatch_reuse_and_expiry(body in "[a-z]{1,24}", extra in "[a-z]{1,8}") {
        let clock = FakeClock::new();
        let guard = guard(clock.clone());
        let first = authorize_summary(&guard, true, true, body.as_bytes()).unwrap();
        let other = format!("{body}{extra}");
        let mismatch = redeem_summary(&guard, &first, other.as_bytes());
        proptest::prop_assert!(mismatch.is_err());
        let second = authorize_summary(&guard, true, true, body.as_bytes()).unwrap();
        redeem_summary(&guard, &second, body.as_bytes()).unwrap();
        proptest::prop_assert!(redeem_summary(&guard, &second, body.as_bytes()).is_err());
        let third = authorize_summary(&guard, true, true, body.as_bytes()).unwrap();
        clock.advance(CONFIRM_TTL + Duration::from_secs(1));
        proptest::prop_assert!(redeem_summary(&guard, &third, body.as_bytes()).is_err());
    }
}
