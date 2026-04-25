use cc_speedy::settings::{load, save_obsidian_daily_push, save_obsidian_vault_name};
use cc_speedy::store::{get_setting_bool, open_db, set_setting_bool};
use std::sync::Mutex;
use tempfile::TempDir;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn open_temp_db() -> (
    TempDir,
    rusqlite::Connection,
    std::sync::MutexGuard<'static, ()>,
) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = TempDir::new().unwrap();
    std::env::set_var("XDG_DATA_HOME", tmp.path());
    let conn = open_db().unwrap();
    (tmp, conn, guard)
}

#[test]
fn test_bool_setting_default_when_missing() {
    let (_tmp, conn, _g) = open_temp_db();
    assert_eq!(get_setting_bool(&conn, "missing_key", true), true);
    assert_eq!(get_setting_bool(&conn, "missing_key", false), false);
}

#[test]
fn test_bool_setting_round_trip_true() {
    let (_tmp, conn, _g) = open_temp_db();
    set_setting_bool(&conn, "x", true).unwrap();
    assert_eq!(get_setting_bool(&conn, "x", false), true);
}

#[test]
fn test_bool_setting_round_trip_false() {
    let (_tmp, conn, _g) = open_temp_db();
    set_setting_bool(&conn, "x", false).unwrap();
    assert_eq!(get_setting_bool(&conn, "x", true), false);
}

#[test]
fn test_bool_setting_overwrites_prior() {
    let (_tmp, conn, _g) = open_temp_db();
    set_setting_bool(&conn, "x", true).unwrap();
    set_setting_bool(&conn, "x", false).unwrap();
    assert_eq!(get_setting_bool(&conn, "x", true), false);
}

#[test]
fn test_load_defaults_when_unset() {
    let (_tmp, conn, _g) = open_temp_db();
    let s = load(&conn);
    assert_eq!(s.obsidian_kb_path, None);
    assert_eq!(s.obsidian_vault_name, None);
    assert_eq!(s.obsidian_daily_push, true, "daily push default = true");
}

#[test]
fn test_save_and_load_vault_name() {
    let (_tmp, conn, _g) = open_temp_db();
    save_obsidian_vault_name(&conn, "my-vault").unwrap();
    let s = load(&conn);
    assert_eq!(s.obsidian_vault_name.as_deref(), Some("my-vault"));
}

#[test]
fn test_save_and_load_daily_push_off() {
    let (_tmp, conn, _g) = open_temp_db();
    save_obsidian_daily_push(&conn, false).unwrap();
    let s = load(&conn);
    assert_eq!(s.obsidian_daily_push, false);
}

#[test]
fn test_effective_vault_name_uses_explicit_when_set() {
    let s = cc_speedy::settings::AppSettings {
        obsidian_kb_path: Some("/some/path/vault-dir".to_string()),
        obsidian_vault_name: Some("explicit-name".to_string()),
        obsidian_daily_push: true,
    };
    assert_eq!(s.effective_vault_name().as_deref(), Some("explicit-name"));
}

#[test]
fn test_effective_vault_name_falls_back_to_path_basename() {
    let s = cc_speedy::settings::AppSettings {
        obsidian_kb_path: Some("/some/path/my-vault-dir".to_string()),
        obsidian_vault_name: None,
        obsidian_daily_push: true,
    };
    assert_eq!(s.effective_vault_name().as_deref(), Some("my-vault-dir"));
}

#[test]
fn test_effective_vault_name_empty_explicit_falls_back_to_basename() {
    let s = cc_speedy::settings::AppSettings {
        obsidian_kb_path: Some("/some/path/v".to_string()),
        obsidian_vault_name: Some(String::new()),
        obsidian_daily_push: true,
    };
    assert_eq!(s.effective_vault_name().as_deref(), Some("v"));
}

#[test]
fn test_effective_vault_name_returns_none_when_nothing_set() {
    let s = cc_speedy::settings::AppSettings::default();
    assert_eq!(s.effective_vault_name(), None);
}
