use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

static MOCK: AtomicBool = AtomicBool::new(false);

const SERVICE: &str = "local.wanxiang.assistant";
const ACCOUNT: &str = "deepseek";

#[derive(Debug, PartialEq, Eq)]
pub enum SecretError {
    Empty,
    Unavailable,
}

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Empty => "密钥为空",
            Self::Unavailable => "钥匙串不可用",
        })
    }
}

impl std::error::Error for SecretError {}

pub struct SecretStore;

impl SecretStore {
    pub fn new() -> Self {
        Self
    }

    pub fn install_mock_backend() {
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            keyring_core::set_default_store(keyring_core::mock::Store::new().expect("模拟钥匙串"));
            MOCK.store(true, Ordering::SeqCst);
        });
    }

    pub fn save(&self, key: &str) -> Result<(), SecretError> {
        if key.is_empty() {
            return Err(SecretError::Empty);
        }
        entry()?
            .set_password(key)
            .map_err(|_| SecretError::Unavailable)
    }

    pub fn delete(&self) -> Result<(), SecretError> {
        match entry()?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(_) => Err(SecretError::Unavailable),
        }
    }

    pub fn is_saved(&self) -> Result<bool, SecretError> {
        match entry()?.get_password() {
            Ok(secret) => {
                drop(secret);
                Ok(true)
            }
            Err(keyring_core::Error::NoEntry) => Ok(false),
            Err(_) => Err(SecretError::Unavailable),
        }
    }
}

fn entry() -> Result<keyring_core::Entry, SecretError> {
    if !MOCK.load(Ordering::SeqCst) {
        // v1 的构造函数会把默认仓库换成系统钥匙串。装过模拟后端之后不能再走这里。
        if keyring::v1::Entry::store_status().is_err() {
            return Err(SecretError::Unavailable);
        }
    }
    keyring_core::Entry::new(SERVICE, ACCOUNT).map_err(|_| SecretError::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_stays_on_the_mock_store() {
        SecretStore::install_mock_backend();
        assert!(!SecretStore::new().is_saved().unwrap());
        let probe = entry().unwrap();
        assert!(probe
            .as_any()
            .downcast_ref::<keyring_core::mock::Cred>()
            .is_some());
    }
}
