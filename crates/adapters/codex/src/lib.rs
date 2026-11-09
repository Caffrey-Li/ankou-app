//! Codex 桌面应用接入占位。
//! M0 不读取 rollouts，也不连接 app-server。

pub fn availability() -> &'static str {
    "未验证"
}

#[cfg(test)]
mod tests {
    #[test]
    fn stays_unverified() {
        assert_eq!(super::availability(), "未验证");
    }
}
