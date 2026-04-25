use cc_speedy::store::{load_all_tags, load_tags, normalize_tag, open_db, parse_tags, set_tags};
use cc_speedy::tui::parse_filter_tokens;

#[test]
fn test_normalize_tag_trims_and_lowercases() {
    assert_eq!(normalize_tag("  WIP  "), Some("wip".to_string()));
    assert_eq!(
        normalize_tag("NeedsReview"),
        Some("needsreview".to_string())
    );
}

#[test]
fn test_normalize_tag_strips_invalid_chars() {
    assert_eq!(
        normalize_tag("needs review"),
        Some("needsreview".to_string())
    );
    assert_eq!(normalize_tag("foo!@#$bar"), Some("foobar".to_string()));
    assert_eq!(
        normalize_tag("foo-bar_baz"),
        Some("foo-bar_baz".to_string())
    );
}

#[test]
fn test_normalize_tag_empty_returns_none() {
    assert_eq!(normalize_tag(""), None);
    assert_eq!(normalize_tag("   "), None);
    assert_eq!(normalize_tag("!!!"), None);
}

#[test]
fn test_parse_tags_dedupes_and_skips_empty() {
    assert_eq!(
        parse_tags("wip, blocked, , WIP, wip"),
        vec!["wip".to_string(), "blocked".to_string()]
    );
}

#[test]
fn test_parse_tags_empty_input() {
    assert_eq!(parse_tags(""), Vec::<String>::new());
    assert_eq!(parse_tags("   ,,, "), Vec::<String>::new());
}

#[test]
fn test_parse_filter_tokens_splits_tags_and_text() {
    let (tags, texts) = parse_filter_tokens("#blocked auth");
    assert_eq!(tags, vec!["blocked".to_string()]);
    assert_eq!(texts, vec!["auth".to_string()]);
}

#[test]
fn test_parse_filter_tokens_multiple_of_each() {
    let (tags, texts) = parse_filter_tokens("foo #wip bar #blocked");
    assert_eq!(tags, vec!["wip".to_string(), "blocked".to_string()]);
    assert_eq!(texts, vec!["foo".to_string(), "bar".to_string()]);
}

#[test]
fn test_parse_filter_tokens_ignores_bare_hash() {
    let (tags, texts) = parse_filter_tokens("# foo");
    assert_eq!(tags, Vec::<String>::new());
    assert_eq!(texts, vec!["foo".to_string()]);
}

#[test]
fn test_set_and_load_tags_round_trip() {
    // Use an in-memory DB copy of the schema for isolation.
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE tags (session_id TEXT NOT NULL, tag TEXT NOT NULL, PRIMARY KEY (session_id, tag));
         CREATE INDEX idx_tags_tag ON tags (tag);",
    ).unwrap();
    set_tags(&conn, "s1", &["wip".to_string(), "blocked".to_string()]).unwrap();
    let got = load_tags(&conn, "s1").unwrap();
    assert_eq!(got, vec!["blocked".to_string(), "wip".to_string()]); // alphabetical
}

#[test]
fn test_set_tags_replaces_full_set() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE tags (session_id TEXT NOT NULL, tag TEXT NOT NULL, PRIMARY KEY (session_id, tag));",
    ).unwrap();
    set_tags(&conn, "s1", &["a".into(), "b".into(), "c".into()]).unwrap();
    set_tags(&conn, "s1", &["x".into()]).unwrap();
    assert_eq!(load_tags(&conn, "s1").unwrap(), vec!["x".to_string()]);
}

#[test]
fn test_load_all_tags_groups_by_session() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE tags (session_id TEXT NOT NULL, tag TEXT NOT NULL, PRIMARY KEY (session_id, tag));",
    ).unwrap();
    set_tags(&conn, "s1", &["wip".into()]).unwrap();
    set_tags(&conn, "s2", &["blocked".into(), "wip".into()]).unwrap();
    let map = load_all_tags(&conn).unwrap();
    assert_eq!(map.get("s1").unwrap(), &vec!["wip".to_string()]);
    assert_eq!(
        map.get("s2").unwrap(),
        &vec!["blocked".to_string(), "wip".to_string()]
    );
    let _ = open_db; // silence unused import
}
