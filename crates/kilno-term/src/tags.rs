//! `[Image #n]` tags still visible in the prompt (bottom screen, not scrollback). The pane
//! header's image chip shows only images whose tag is on screen (terminal.html sendTags).

use regex::Regex;
use std::collections::BTreeSet;
use std::sync::OnceLock;

fn re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[Image #(\d+)\]").expect("valid regex"))
}

/// Tag numbers found in these screen rows.
pub fn image_tags<S: AsRef<str>>(rows: &[S]) -> BTreeSet<u32> {
    rows.iter()
        .flat_map(|r| re().captures_iter(r.as_ref()).filter_map(|c| c[1].parse().ok()).collect::<Vec<u32>>())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_tags() {
        let rows = ["> [Image #1] what is [Image #10]", "", "[Image #x] [Image #2]"];
        assert_eq!(image_tags(&rows).into_iter().collect::<Vec<_>>(), vec![1, 2, 10]);
    }
}
