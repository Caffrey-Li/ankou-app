fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "list_grants",
            "grant",
            "revoke",
            "get_settings",
            "set_settings",
            "save_deepseek_key",
            "delete_deepseek_key",
            "deepseek_key_saved",
            "clear_history",
            "discover_tools",
            "complete_setup",
            "get_usage",
            "refresh_usage",
        ]),
    ))
    .expect("生成 Tauri 权限失败");
}
