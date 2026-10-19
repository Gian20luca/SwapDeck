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
            "mini_hide",
            "mini_set_compact",
            "mini_is_compact",
            "mini_fit_height",
            "get_snapshot",
            "list_mappings",
            "get_engine_state",
            "set_engine_rules",
            "get_engine_profiles",
            "apply_engine_profile",
            "get_preferences",
            "set_preferences",
            "get_notification_settings",
            "set_notification_settings",
            "get_history_events",
            "get_usage_breakdown",
            "get_diagnostics",
            "open_logs_folder",
            "diagnose_setup",
            "install_cswap",
            "upgrade_cswap",
            "check_cswap_update",
            "dismiss_cswap_update",
        ]),
    ))
    // `{:?}` di anyhow stampa tutta la catena, compreso il file coinvolto (per esempio in uso).
    .unwrap_or_else(|e| panic!("build di Tauri non riuscita: {e:?}"));
}
