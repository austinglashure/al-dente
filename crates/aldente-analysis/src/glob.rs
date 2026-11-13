//! Shell-style wildcard matching for user patterns such as `--stop-at`.

/// Whether `text` matches `pattern`, where `*` matches any run of characters
/// (including none) and `?` matches one character. Everything else is literal.
///
/// Example: `glob_match("app.log/*", "app.log/info")` is `true`;
/// `glob_match("a?c", "abbc")` is `false`.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0, 0);
    // Where the last `*` was and how much text it has swallowed so far.
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::glob_match;

    #[test]
    fn wildcards_and_literals() {
        assert!(glob_match("*", ""));
        assert!(glob_match("app.log/*", "app.log/info"));
        assert!(glob_match("*log*", "clj:.//app.log/info"));
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "abbc"));
        assert!(!glob_match("app", "app.core"));
        assert!(glob_match("a*b*c", "a-x-b-y-c"));
        assert!(!glob_match("a*b*c", "a-x-c-y-b"));
        assert!(glob_match("λ*", "λé"));
    }
}
