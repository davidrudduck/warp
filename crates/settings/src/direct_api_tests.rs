use crate::Setting;
use crate::direct_api::DirectAPIRigBackendEnabled;
use warpui_extras::user_preferences::toml_backed::TomlBackedUserPreferences;

#[test]
fn direct_api_rig_backend_gate_defaults_off() {
    assert!(!DirectAPIRigBackendEnabled::default_value());
    assert!(!*DirectAPIRigBackendEnabled::new(None).value());
}

#[test]
fn direct_api_rig_backend_gate_writes_to_expected_toml_path() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("settings.toml");
    let (prefs, _) = TomlBackedUserPreferences::new(file_path.clone());

    let changed = DirectAPIRigBackendEnabled::write_to_preferences(&true, &prefs).unwrap();
    assert!(changed);

    let contents = std::fs::read_to_string(file_path).unwrap();
    assert!(contents.contains("[agents.direct_api.experimental]"));
    assert!(contents.contains("rig_backend_enabled = true"));
}
