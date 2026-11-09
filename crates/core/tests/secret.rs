use wanxiang_core::{SecretError, SecretStore};

fn store() -> SecretStore {
    SecretStore::install_mock_backend();
    SecretStore::new()
}

#[test]
fn save_query_and_delete_never_return_the_key() {
    let secrets = store();
    let key = "fixture-key";
    assert!(!secrets.is_saved().unwrap());
    secrets.save(key).unwrap();
    let saved = secrets.is_saved().unwrap();
    assert!(saved);
    assert!(!format!("{saved:?}").contains(key));
    secrets.delete().unwrap();
    assert!(!secrets.is_saved().unwrap());
    let err = secrets.save("").unwrap_err();
    let text = format!("{err} {err:?}");
    assert!(!text.contains(key));
    assert!(matches!(err, SecretError::Empty));
}
