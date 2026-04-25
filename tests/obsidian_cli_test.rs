use cc_speedy::obsidian_cli::escape_arg_value;

#[test]
fn test_escape_plain_text_unchanged() {
    assert_eq!(escape_arg_value("hello world"), "hello world");
}

#[test]
fn test_escape_double_quote() {
    assert_eq!(escape_arg_value(r#"say "hi""#), r#"say \"hi\""#);
}

#[test]
fn test_escape_newline() {
    assert_eq!(escape_arg_value("line1\nline2"), r"line1\nline2");
}

#[test]
fn test_escape_tab() {
    assert_eq!(escape_arg_value("a\tb"), r"a\tb");
}

#[test]
fn test_escape_backslash_first() {
    // backslash itself is doubled before the quote/newline rules apply
    assert_eq!(escape_arg_value(r"a\b"), r"a\\b");
}

#[test]
fn test_escape_combined() {
    assert_eq!(
        escape_arg_value("she said \"hi\"\nbye"),
        r#"she said \"hi\"\nbye"#,
    );
}
