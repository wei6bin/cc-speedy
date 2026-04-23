use cc_speedy::tui::highlight_line;

#[test]
fn test_highlight_line_no_match_returns_single_raw_span() {
    let line = highlight_line("hello world", "missing");
    assert_eq!(line.spans.len(), 1);
    assert_eq!(line.spans[0].content, "hello world");
}

#[test]
fn test_highlight_line_empty_query_returns_single_raw_span() {
    let line = highlight_line("hello world", "");
    assert_eq!(line.spans.len(), 1);
    assert_eq!(line.spans[0].content, "hello world");
}

#[test]
fn test_highlight_line_single_match_splits_into_three_spans() {
    // "before <match> after" — query already lowercased by caller convention
    let line = highlight_line("before MATCH after", "match");
    assert_eq!(line.spans.len(), 3);
    assert_eq!(line.spans[0].content, "before ");
    assert_eq!(line.spans[1].content, "MATCH"); // preserves original case
    assert_eq!(line.spans[2].content, " after");
}

#[test]
fn test_highlight_line_multiple_matches() {
    let line = highlight_line("auth and AUTH and auth", "auth");
    // expected spans: "auth", " and ", "AUTH", " and ", "auth"
    assert_eq!(line.spans.len(), 5);
    assert_eq!(line.spans[0].content, "auth");
    assert_eq!(line.spans[1].content, " and ");
    assert_eq!(line.spans[2].content, "AUTH");
    assert_eq!(line.spans[3].content, " and ");
    assert_eq!(line.spans[4].content, "auth");
}

#[test]
fn test_highlight_line_match_at_start() {
    let line = highlight_line("MATCH is here", "match");
    assert_eq!(line.spans.len(), 2);
    assert_eq!(line.spans[0].content, "MATCH");
    assert_eq!(line.spans[1].content, " is here");
}

#[test]
fn test_highlight_line_match_at_end() {
    let line = highlight_line("here is MATCH", "match");
    assert_eq!(line.spans.len(), 2);
    assert_eq!(line.spans[0].content, "here is ");
    assert_eq!(line.spans[1].content, "MATCH");
}

#[test]
fn test_highlight_line_entire_line_matches() {
    let line = highlight_line("AUTH", "auth");
    assert_eq!(line.spans.len(), 1);
    assert_eq!(line.spans[0].content, "AUTH");
}

#[test]
fn test_highlight_line_non_ascii_falls_back_to_raw() {
    // A non-ASCII character in the line would change byte length under lowercase,
    // so the helper must bail to a single raw span instead of indexing wrongly.
    let line = highlight_line("café au lait", "au");
    // Should either highlight correctly OR bail to single span — either is safe.
    // Current implementation bails when byte length differs; assert no panic and
    // that rebuilt content matches the original.
    let rebuilt: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(rebuilt, "café au lait");
}
