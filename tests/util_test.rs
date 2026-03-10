use cc_speedy::util::path_last_n;

#[test]
fn test_path_last_n_two_segments() {
    assert_eq!(path_last_n("/home/user/ai/proj", 2), "ai/proj");
}

#[test]
fn test_path_last_n_one_segment() {
    assert_eq!(path_last_n("/single", 2), "single");
}

#[test]
fn test_path_last_n_empty_path() {
    assert_eq!(path_last_n("", 2), "");
}

#[test]
fn test_path_last_n_trailing_slash() {
    assert_eq!(path_last_n("/home/user/proj/", 2), "user/proj");
}

#[test]
fn test_path_last_n_three_segments() {
    assert_eq!(path_last_n("/a/b/c/d", 3), "b/c/d");
}

#[test]
fn test_path_last_n_request_more_than_available() {
    // Asking for 5 segments from a 2-segment path returns all of them
    assert_eq!(path_last_n("/foo/bar", 5), "foo/bar");
}
