//! DeepSeek Harness 与 DS Bot 的接入占位。
//! M0 不读取这两个工具的本机数据。

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
