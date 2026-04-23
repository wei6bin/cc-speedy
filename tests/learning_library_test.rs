use cc_speedy::store::LearningEntry;

fn mk(session_id: &str, category: &str, point: &str, at: i64) -> LearningEntry {
    LearningEntry {
        session_id: session_id.to_string(),
        category: category.to_string(),
        point: point.to_string(),
        captured_at: at,
    }
}

/// Mirror of the filter logic used in tui.rs::apply_library_filter.
fn filter_library(entries: &[LearningEntry], category: Option<&str>, query: &str) -> Vec<usize> {
    let q = query.to_lowercase();
    entries
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            if let Some(c) = category {
                if e.category != c { return false; }
            }
            q.is_empty() || e.point.to_lowercase().contains(&q)
        })
        .map(|(i, _)| i)
        .collect()
}

#[test]
fn test_no_filters_returns_all() {
    let entries = vec![
        mk("s1", "decision_points", "pick postgres", 1),
        mk("s1", "lessons_gotchas", "mock DB burned us", 2),
        mk("s2", "tools_commands", "git worktree", 3),
    ];
    assert_eq!(filter_library(&entries, None, ""), vec![0, 1, 2]);
}

#[test]
fn test_category_filter_only() {
    let entries = vec![
        mk("s1", "decision_points", "pick postgres", 1),
        mk("s1", "lessons_gotchas", "mock DB burned us", 2),
        mk("s2", "tools_commands", "git worktree", 3),
    ];
    assert_eq!(filter_library(&entries, Some("decision_points"), ""), vec![0]);
    assert_eq!(filter_library(&entries, Some("lessons_gotchas"), ""), vec![1]);
    assert_eq!(filter_library(&entries, Some("tools_commands"), ""), vec![2]);
}

#[test]
fn test_text_filter_case_insensitive() {
    let entries = vec![
        mk("s1", "decision_points", "pick POSTGRES over mysql", 1),
        mk("s1", "lessons_gotchas", "mock DB burned us", 2),
    ];
    assert_eq!(filter_library(&entries, None, "postgres"), vec![0]);
    assert_eq!(filter_library(&entries, None, "BURNED"), vec![1]);
    assert_eq!(filter_library(&entries, None, "nope"), Vec::<usize>::new());
}

#[test]
fn test_category_and_text_compose() {
    let entries = vec![
        mk("s1", "decision_points", "pick postgres", 1),
        mk("s2", "decision_points", "use kafka", 2),
        mk("s3", "lessons_gotchas", "postgres locking gotcha", 3),
    ];
    // category = decision_points AND query = postgres → only entry 0
    assert_eq!(filter_library(&entries, Some("decision_points"), "postgres"), vec![0]);
    // query alone = postgres → entries 0 and 2
    assert_eq!(filter_library(&entries, None, "postgres"), vec![0, 2]);
}
