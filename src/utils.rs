/// Truncate a string to `max_len` characters, appending "…" if truncated.
/// Returns the original string reference if no truncation is needed.
pub fn truncate_str(s: &str, max_len: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_len || max_len == 0 {
        return s.to_string();
    }
    let trunc_len = max_len.saturating_sub(1);
    let truncated: String = s.chars().take(trunc_len).collect();
    format!("{}…", truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_no_truncation() {
        assert_eq!(truncate_str("Hello", 10), "Hello");
    }

    #[test]
    fn test_truncate_exact_fit() {
        assert_eq!(truncate_str("Hello", 5), "Hello");
    }

    #[test]
    fn test_truncate_with_ellipsis() {
        assert_eq!(truncate_str("Hello World", 5), "Hell…");
    }

    #[test]
    fn test_truncate_zero_max() {
        assert_eq!(truncate_str("Hello", 0), "Hello");
    }

    #[test]
    fn test_truncate_empty() {
        assert_eq!(truncate_str("", 5), "");
    }
}
