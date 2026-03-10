/// Return the last `n` non-empty path segments joined with "/".
///
/// Examples:
/// ```
/// use cc_speedy::util::path_last_n;
/// assert_eq!(path_last_n("/home/user/ai/proj", 2), "ai/proj");
/// assert_eq!(path_last_n("/single", 2), "single");
/// assert_eq!(path_last_n("", 2), "");
/// ```
pub fn path_last_n(path: &str, n: usize) -> String {
    let parts: Vec<&str> = path
        .trim_end_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let start = parts.len().saturating_sub(n);
    parts[start..].join("/")
}
