//! Live integration tests against a running Obsidian instance.
//!
//! Run manually with:
//!   OBSIDIAN_TEST_VAULT=my-vault cargo test --test obsidian_cli_integration_test -- --ignored
//!
//! Requirements:
//!  - Obsidian.app running with the named vault open.
//!  - Daily Notes core plugin enabled.
//!  - The `obsidian` CLI on PATH.

use cc_speedy::obsidian_cli::{daily_append, is_available, vault_is_running};

fn vault_name() -> Option<String> {
    std::env::var("OBSIDIAN_TEST_VAULT").ok()
}

#[test]
#[ignore]
fn live_is_available() {
    assert!(is_available(), "obsidian binary not on PATH");
}

#[test]
#[ignore]
fn live_vault_is_running() {
    let v = vault_name().expect("set OBSIDIAN_TEST_VAULT");
    assert!(vault_is_running(&v), "vault not open: {}", v);
}

#[test]
#[ignore]
fn live_daily_append_idempotent() {
    let v = vault_name().expect("set OBSIDIAN_TEST_VAULT");
    let marker = format!(
        "[[cc-speedy-integration-test-{}]]",
        chrono::Local::now().timestamp()
    );
    let line = format!("- {} test marker", marker);
    daily_append(&v, &line, Some(&marker)).unwrap();
    // Second call should be a no-op (marker now present).
    daily_append(&v, &line, Some(&marker)).unwrap();
}
