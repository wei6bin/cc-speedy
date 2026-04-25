use cc_speedy::install::build_hook_entry;
use serde_json::Value;

#[test]
fn test_hook_entry_contains_summarize() {
    let entry = build_hook_entry("/usr/local/bin/cc-speedy");
    let s = serde_json::to_string(&entry).unwrap();
    assert!(s.contains("summarize"));
}

#[test]
fn test_hook_entry_contains_binary_path() {
    let entry = build_hook_entry("/usr/local/bin/cc-speedy");
    let cmd = entry["hooks"][0]["command"].as_str().unwrap();
    assert!(cmd.contains("/usr/local/bin/cc-speedy"));
    assert!(cmd.contains("summarize"));
}

#[test]
fn test_hook_entry_has_correct_structure() {
    let entry = build_hook_entry("/bin/cc-speedy");
    assert!(entry["hooks"].is_array());
    assert_eq!(entry["hooks"].as_array().unwrap().len(), 1);
    assert_eq!(entry["hooks"][0]["type"].as_str().unwrap(), "command");
}

#[test]
fn test_install_is_idempotent() {
    use std::fs;
    use tempfile::TempDir;

    // Create a temp settings file with an existing SessionEnd hook
    let tmp = TempDir::new().unwrap();
    let settings_path = tmp.path().join("settings.json");

    let existing = serde_json::json!({
        "hooks": {
            "SessionEnd": [{
                "hooks": [{
                    "type": "command",
                    "command": "\"/bin/cc-speedy\" summarize"
                }]
            }]
        }
    });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&existing).unwrap(),
    )
    .unwrap();

    // Run install with this path
    cc_speedy::install::install_to(&settings_path, "/bin/cc-speedy").unwrap();

    // Verify the hook was NOT duplicated
    let content = fs::read_to_string(&settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    let session_end = settings["hooks"]["SessionEnd"].as_array().unwrap();
    assert_eq!(session_end.len(), 1, "hook should not be duplicated");
}

#[test]
fn test_install_adds_hook_to_empty_settings() {
    use std::fs;
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let settings_path = tmp.path().join("settings.json");
    fs::write(&settings_path, "{}").unwrap();

    cc_speedy::install::install_to(&settings_path, "/bin/cc-speedy").unwrap();

    let content = fs::read_to_string(&settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    let session_end = settings["hooks"]["SessionEnd"].as_array().unwrap();
    assert_eq!(session_end.len(), 1);
    let cmd = session_end[0]["hooks"][0]["command"].as_str().unwrap();
    assert_eq!(cmd, "\"/bin/cc-speedy\" summarize");
}

#[test]
fn test_install_creates_new_settings_when_file_missing() {
    use std::fs;
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let settings_path = tmp.path().join("settings.json");
    // File does NOT exist — install_to should treat it as empty {}

    cc_speedy::install::install_to(&settings_path, "/bin/cc-speedy").unwrap();

    let content = fs::read_to_string(&settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    assert!(settings["hooks"]["SessionEnd"].as_array().is_some());
}

#[test]
fn test_install_errors_on_unreadable_file() {
    use std::fs;
    use tempfile::TempDir;

    // Create a file with content that is NOT valid JSON but IS readable
    let tmp = TempDir::new().unwrap();
    let settings_path = tmp.path().join("bad.json");
    fs::write(&settings_path, "this is not json {{{{").unwrap();

    let result = cc_speedy::install::install_to(&settings_path, "/bin/cc-speedy");
    // serde_json parse error — should propagate as Err
    assert!(
        result.is_err(),
        "expected error for invalid JSON settings file"
    );
}
