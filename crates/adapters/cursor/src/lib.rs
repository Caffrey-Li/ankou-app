//! Cursor 接入占位。
//! 开发时只用日后的 fixtures，不读取本机 Cursor 数据目录。

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
