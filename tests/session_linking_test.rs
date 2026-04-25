use cc_speedy::store::{load_all_links, set_link, unset_link};

fn open() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE links (
            session_id        TEXT PRIMARY KEY,
            parent_session_id TEXT NOT NULL,
            linked_at         INTEGER NOT NULL DEFAULT (strftime('%s','now'))
         );
         CREATE INDEX idx_links_parent ON links (parent_session_id);",
    )
    .unwrap();
    conn
}

#[test]
fn test_set_link_and_load() {
    let conn = open();
    set_link(&conn, "child", "parent").unwrap();
    let map = load_all_links(&conn).unwrap();
    assert_eq!(map.get("child"), Some(&"parent".to_string()));
    assert_eq!(map.len(), 1);
}

#[test]
fn test_set_link_refuses_self_link() {
    let conn = open();
    let res = set_link(&conn, "same", "same");
    assert!(res.is_err());
}

#[test]
fn test_set_link_twice_replaces() {
    let conn = open();
    set_link(&conn, "child", "p1").unwrap();
    set_link(&conn, "child", "p2").unwrap();
    let map = load_all_links(&conn).unwrap();
    assert_eq!(map.get("child"), Some(&"p2".to_string()));
    assert_eq!(map.len(), 1);
}

#[test]
fn test_unset_link_removes_row() {
    let conn = open();
    set_link(&conn, "child", "parent").unwrap();
    unset_link(&conn, "child").unwrap();
    let map = load_all_links(&conn).unwrap();
    assert!(map.is_empty());
}

#[test]
fn test_unset_link_noop_when_missing() {
    let conn = open();
    unset_link(&conn, "never_linked").unwrap(); // should not panic
    assert!(load_all_links(&conn).unwrap().is_empty());
}

#[test]
fn test_children_derivable_from_reverse_scan() {
    let conn = open();
    set_link(&conn, "c1", "p1").unwrap();
    set_link(&conn, "c2", "p1").unwrap();
    set_link(&conn, "c3", "p2").unwrap();
    let map = load_all_links(&conn).unwrap();
    let children_of_p1: Vec<&String> = map
        .iter()
        .filter(|(_, pv)| pv == &"p1")
        .map(|(k, _)| k)
        .collect();
    assert_eq!(children_of_p1.len(), 2);
}
