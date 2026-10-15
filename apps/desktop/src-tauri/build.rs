//! Genera il contesto di Tauri e i permessi dei comandi dell'app (`capabilities/`).

fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "list_accounts",
            "get_status",
            "get_config",
            "get_config_value",
            "get_history",
            "confirmation_for",
            "run_action",
            "get_app_settings",
            "set_autostart",
            "show_main",
        ]),
    ))
    .expect("build di Tauri non riuscita");
}
