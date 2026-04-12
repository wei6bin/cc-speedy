use cc_speedy::summary::{parse_learning_output, build_combined_display};
use cc_speedy::store::LearningPoint;

#[test]
fn test_parse_learning_output_handles_trailing_colon_heading() {
    let md = "\
## Decision points:
- chose async over sync: better throughput
";
    let points = parse_learning_output(md);
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].category, "decision_points");
}

#[test]
fn test_parse_learning_output_extracts_bullets() {
    let md = "\
## Decision points
- chose SQLite over flat files: simpler migration
- used async spawn: keeps TUI responsive

## Lessons & gotchas
- lock poisoning crashes the thread if not handled

## Tools & commands discovered
- cargo test sessions: runs a single test file
";
    let points = parse_learning_output(md);
    assert_eq!(points.len(), 4);
    assert_eq!(points[0].category, "decision_points");
    assert_eq!(points[0].point, "chose SQLite over flat files: simpler migration");
    assert_eq!(points[2].category, "lessons_gotchas");
    assert_eq!(points[3].category, "tools_commands");
}

#[test]
fn test_parse_learning_output_skips_none() {
    let md = "\
## Decision points
- none

## Lessons & gotchas
- none

## Tools & commands discovered
- none
";
    let points = parse_learning_output(md);
    assert!(points.is_empty(), "should skip 'none' bullets");
}

#[test]
fn test_parse_learning_output_unknown_heading_ignored() {
    let md = "\
## Foobar section
- something

## Decision points
- real point
";
    let points = parse_learning_output(md);
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].category, "decision_points");
}

#[test]
fn test_build_combined_display_empty_learnings() {
    let combined = build_combined_display("## What was done\n- fixed bug", &[]);
    assert_eq!(combined, "## What was done\n- fixed bug");
}

#[test]
fn test_build_combined_display_includes_learnings() {
    let learnings = vec![
        LearningPoint { category: "decision_points".to_string(), point: "used tokio::spawn".to_string() },
        LearningPoint { category: "lessons_gotchas".to_string(), point: "mutex guard must be dropped".to_string() },
    ];
    let combined = build_combined_display("## What was done\n- fixed bug", &learnings);
    assert!(combined.contains("Knowledge Capture"), "should have knowledge section header");
    assert!(combined.contains("used tokio::spawn"));
    assert!(combined.contains("mutex guard must be dropped"));
    assert!(combined.contains("## Decision points"));
    assert!(combined.contains("## Lessons & gotchas"));
}
