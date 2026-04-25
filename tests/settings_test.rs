use cc_speedy::store::{get_setting_bool, open_db, set_setting_bool};
use tempfile::TempDir;

fn open_temp_db() -> (TempDir, rusqlite::Connection) {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("XDG_DATA_HOME", tmp.path());
    let conn = open_db().unwrap();
    (tmp, conn)
}

#[test]
fn test_bool_setting_default_when_missing() {
    let (_tmp, conn) = open_temp_db();
    assert_eq!(get_setting_bool(&conn, "missing_key", true), true);
    assert_eq!(get_setting_bool(&conn, "missing_key", false), false);
}

#[test]
fn test_bool_setting_round_trip_true() {
    let (_tmp, conn) = open_temp_db();
    set_setting_bool(&conn, "x", true).unwrap();
    assert_eq!(get_setting_bool(&conn, "x", false), true);
}

#[test]
fn test_bool_setting_round_trip_false() {
    let (_tmp, conn) = open_temp_db();
    set_setting_bool(&conn, "x", false).unwrap();
    assert_eq!(get_setting_bool(&conn, "x", true), false);
}

#[test]
fn test_bool_setting_overwrites_prior() {
    let (_tmp, conn) = open_temp_db();
    set_setting_bool(&conn, "x", true).unwrap();
    set_setting_bool(&conn, "x", false).unwrap();
    assert_eq!(get_setting_bool(&conn, "x", true), false);
}
