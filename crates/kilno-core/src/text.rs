//! Small text helpers shared by the transcript readers and command builders.

/// Trim; text that starts with `<` and holds a `>` (the agents' own context blocks), or starts
/// with "Caveat:", is not a prompt and becomes "".
pub fn clean(s: &str) -> String {
    let t = s.trim();
    if (t.starts_with('<') && t.contains('>')) || t.starts_with("Caveat:") {
        return String::new();
    }
    t.to_string()
}

/// Collapse whitespace runs to one space; text longer than `max` chars is cut, trimmed and
/// gets `…` (U+2026).
pub fn shorten(s: &str, max: usize) -> String {
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max {
        return collapsed;
    }
    let cut: String = collapsed.chars().take(max).collect();
    format!("{}\u{2026}", cut.trim_end())
}

/// Quote for PowerShell: wrap in single quotes, doubling `'` and the typographic quotes
/// PowerShell also treats as quotes (U+2018, U+2019, U+201A, U+201B).
pub fn ps_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// A lowercase hyphenated GUID ("D" format), or None if `s` is not a GUID.
pub fn guid(s: &str) -> Option<String> {
    uuid::Uuid::parse_str(s.trim())
        .ok()
        .map(|u| u.hyphenated().to_string())
}

/// A new random lowercase hyphenated GUID.
pub fn new_guid() -> String {
    uuid::Uuid::new_v4().hyphenated().to_string()
}

/// Paths typed at a prompt (InsertPaths): space separated, quoted when one holds a space, with
/// a trailing space to carry on typing; no Enter. Empty when there are no paths.
pub fn insert_paths<S: AsRef<str>>(paths: &[S]) -> String {
    let text = paths
        .iter()
        .map(|p| {
            let p = p.as_ref();
            if p.contains(' ') {
                format!("\"{p}\"")
            } else {
                p.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if text.is_empty() {
        text
    } else {
        text + " "
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_paths_quotes_and_trails() {
        assert_eq!(insert_paths::<&str>(&[]), "");
        assert_eq!(insert_paths(&[r"C:\a.txt", r"C:\my docs\b.md"]), "C:\\a.txt \"C:\\my docs\\b.md\" ");
    }

    #[test]
    fn quoting() {
        assert_eq!(ps_quote("x'; calc; '"), "'x''; calc; '''");
        assert_eq!(
            ps_quote("C:\\Users\\O\u{2019}Brien\u{2018}\u{201A}\u{201B}"),
            "'C:\\Users\\O\u{2019}\u{2019}Brien\u{2018}\u{2018}\u{201A}\u{201A}\u{201B}\u{201B}'"
        );
    }

    #[test]
    fn shortening() {
        assert_eq!(shorten("a  b\n c", 10), "a b c");
        assert_eq!(shorten("abcdef ghij", 7), "abcdef\u{2026}");
        assert_eq!(clean("  <x>y</x> "), "");
        assert_eq!(clean("Caveat: z"), "");
        assert_eq!(clean(" hi "), "hi");
    }

    #[test]
    fn guids() {
        assert_eq!(
            guid("3F136615-E5BA-45B2-9766-6CC4D88EF5D9").as_deref(),
            Some("3f136615-e5ba-45b2-9766-6cc4d88ef5d9")
        );
        assert_eq!(guid("..\\..\\evil"), None);
    }
}
