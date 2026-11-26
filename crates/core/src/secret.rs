use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

static MOCK: AtomicBool = AtomicBool::new(false);

const SERVICE: &str = "local.ankou.assistant";
const LEGACY_SERVICE: &str = "local.wanxiang.assistant";
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
        entry_for(SERVICE)?
            .set_password(key)
            .map_err(|_| SecretError::Unavailable)
    }

    pub fn delete(&self) -> Result<(), SecretError> {
        let mut unavailable = false;
        for service in [SERVICE, LEGACY_SERVICE] {
            match entry_for(service)?.delete_credential() {
                Ok(()) | Err(keyring_core::Error::NoEntry) => {}
                Err(_) => unavailable = true,
            }
        }
        if unavailable {
            Err(SecretError::Unavailable)
        } else {
            Ok(())
        }
    }

    pub fn load(&self) -> Result<String, SecretError> {
        match entry_for(SERVICE)?.get_password() {
            Ok(secret) => Ok(secret),
            Err(keyring_core::Error::NoEntry) => match entry_for(LEGACY_SERVICE)?.get_password() {
                Ok(secret) => {
                    // 兼容旧版「万象小助手」钥匙串：读到后顺手写入新服务名，不删旧条目。
                    let _ = entry_for(SERVICE).and_then(|e| {
                        e.set_password(&secret)
                            .map_err(|_| SecretError::Unavailable)
                    });
                    Ok(secret)
                }
                Err(keyring_core::Error::NoEntry) => Err(SecretError::Empty),
                Err(_) => Err(SecretError::Unavailable),
            },
            Err(_) => Err(SecretError::Unavailable),
        }
    }

    pub fn is_saved(&self) -> Result<bool, SecretError> {
        match entry_for(SERVICE)?.get_password() {
            Ok(secret) => {
                drop(secret);
                Ok(true)
            }
            Err(keyring_core::Error::NoEntry) => match entry_for(LEGACY_SERVICE)?.get_password() {
                Ok(secret) => {
                    drop(secret);
                    Ok(true)
                }
                Err(keyring_core::Error::NoEntry) => Ok(false),
                Err(_) => Err(SecretError::Unavailable),
            },
            Err(_) => Err(SecretError::Unavailable),
        }
    }
}

#[cfg(test)]
fn entry() -> Result<keyring_core::Entry, SecretError> {
    entry_for(SERVICE)
}

fn entry_for(service: &str) -> Result<keyring_core::Entry, SecretError> {
    if !MOCK.load(Ordering::SeqCst) {
        // v1 的构造函数会把默认仓库换成系统钥匙串。装过模拟后端之后不能再走这里。
        if keyring::v1::Entry::store_status().is_err() {
            return Err(SecretError::Unavailable);
        }
    }
    keyring_core::Entry::new(service, ACCOUNT).map_err(|_| SecretError::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 模拟钥匙串是进程内全局的，测试并行时会互相看到对方写的键，所以串行跑。
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn query_stays_on_the_mock_store() {
        let _serial = serial();
        SecretStore::install_mock_backend();
        assert!(!SecretStore::new().is_saved().unwrap());
        let probe = entry().unwrap();
        assert!(probe
            .as_any()
            .downcast_ref::<keyring_core::mock::Cred>()
            .is_some());
    }

    #[test]
    fn loads_and_migrates_legacy_key() {
        let _serial = serial();
        SecretStore::install_mock_backend();
        let store = SecretStore::new();
        store.delete().unwrap();
        entry_for(LEGACY_SERVICE)
            .unwrap()
            .set_password("sk-legacy-demo")
            .unwrap();
        assert!(store.is_saved().unwrap());
        assert_eq!(store.load().unwrap(), "sk-legacy-demo");
        assert_eq!(
            entry_for(SERVICE).unwrap().get_password().unwrap(),
            "sk-legacy-demo"
        );
        store.delete().unwrap();
    }
}
