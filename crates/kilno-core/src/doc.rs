//! The doc viewer's text (a markdown file the session wrote): read with a size cap, and made
//! safe to show. Nothing in a doc reaches the network (PORTING.md D4): a remote image becomes
//! its alt text; an image file inside the session's folder is shown from disk; `data:` images
//! stay. Links open only in the browser, only http(s), only after asking.

use crate::paths::{is_inside, normalize};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::path::Path;

/// Bigger than any doc a session writes; past it the viewer would only stall (TP:603).
pub const DOC_LIMIT: u64 = 2 * 1024 * 1024;

/// The markdown extensions the viewer renders (GFM, as marked did).
pub fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_FOOTNOTES
}

/// The doc's text, or a one-line note in its place: too big, or unreadable (TP:618-629).
pub fn read(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let read = || -> std::io::Result<String> {
        if std::fs::metadata(path)?.len() > DOC_LIMIT {
            return Ok(format!("*{name} is too big to show here.*"));
        }
        let bytes = std::fs::read(path)?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        Ok(text.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(text))
    };
    read().unwrap_or_else(|e| format!("*Couldn't read {name}: {e}*"))
}

/// `uri` when it is an absolute http or https link (TP:326-331); anything else opens nothing.
pub fn web_link(uri: &str) -> Option<&str> {
    let uri = uri.trim();
    let lower = uri.to_ascii_lowercase();
    let rest = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://"))?;
    let host_ok = rest.chars().next().is_some_and(|c| !matches!(c, '/' | '\\' | '?' | '#'));
    (host_ok && !uri.chars().any(char::is_whitespace)).then_some(uri)
}

/// The question asked before a link leaves Kilno (TH:93-97, 274-280).
pub fn confirm_text(uri: &str) -> String {
    format!("Open this link in your browser?\n\n{uri}")
}

/// The markdown with every image made safe to show: `data:` images stay; an image file inside
/// `root` (relative paths are against the doc's folder) becomes an absolute `file://` link;
/// anything else, remote images first, becomes its alt text.
pub fn prepare(markdown: &str, doc_dir: &Path, root: &Path) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut at = 0;
    let mut events = Parser::new_ext(markdown, options()).into_offset_iter();
    while let Some((event, range)) = events.next() {
        let Event::Start(Tag::Image { dest_url, .. }) = event else { continue };
        // The alt text is everything up to the image's end.
        let mut alt = String::new();
        let mut depth = 0;
        for (e, _) in events.by_ref() {
            match e {
                Event::Start(Tag::Image { .. }) => depth += 1,
                Event::End(TagEnd::Image) if depth == 0 => break,
                Event::End(TagEnd::Image) => depth -= 1,
                Event::Text(t) | Event::Code(t) => alt.push_str(&t),
                _ => {}
            }
        }
        if range.start < at {
            continue; // inside an image already replaced
        }
        out.push_str(&markdown[at..range.start]);
        at = range.end;
        if dest_url.trim_start().to_ascii_lowercase().starts_with("data:image/") {
            out.push_str(&markdown[range]);
            continue;
        }
        match local_image(&dest_url, doc_dir, root) {
            Some(file) => {
                out.push_str("![");
                out.push_str(&escape(&alt));
                // `file:///C:/x` on Windows: two slashes would name a network host (`\\C:\x`).
                out.push_str(if file.starts_with('/') { "](<file://" } else { "](<file:///" });
                out.push_str(&file);
                out.push_str(">)");
            }
            None => out.push_str(&escape(if alt.trim().is_empty() { "image" } else { &alt })),
        }
    }
    out.push_str(&markdown[at..]);
    out
}

/// The image file a destination names, with forward slashes, when it is a path (not a URL)
/// to an existing image inside `root`.
fn local_image(dest: &str, doc_dir: &Path, root: &Path) -> Option<String> {
    let dest = dest.trim().replace("%20", " ");
    // A scheme of two letters or more is a URL (http:, file:, javascript:); one is a drive.
    if let Some(i) = dest.find(':') {
        if i > 1 && dest[..i].chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
            return None;
        }
    }
    if dest.is_empty() || dest.starts_with("//") || dest.starts_with("\\\\") {
        return None;
    }
    let full = normalize(&doc_dir.join(&dest));
    let ok = is_inside(root, &full) && full.is_file() && crate::images::is_image(&full);
    // No `%`: egui's file loader would also try the percent-decoded name, a different file.
    ok.then(|| full.to_string_lossy().replace('\\', "/")).filter(|p| !p.contains(['<', '>', '\n', '%']))
}

/// Text that reads as itself in markdown.
fn escape(text: &str) -> String {
    let mut s = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii_punctuation() {
            s.push('\\');
        }
        s.push(if c == '\n' { ' ' } else { c });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_links_only() {
        assert_eq!(web_link("https://example.com/x"), Some("https://example.com/x"));
        assert_eq!(web_link(" HTTP://a.b "), Some("HTTP://a.b"));
        assert_eq!(web_link("file:///C:/x"), None);
        assert_eq!(web_link("javascript:alert(1)"), None);
        assert_eq!(web_link("https:///x"), None);
        assert_eq!(web_link("http://"), None);
        assert_eq!(web_link("#top"), None);
        assert_eq!(web_link("notes.md"), None);
        assert_eq!(confirm_text("https://a.b"), "Open this link in your browser?\n\nhttps://a.b");
    }

    #[test]
    fn remote_images_become_alt_text() {
        let dir = tempfile::tempdir().unwrap();
        let md = "a ![the logo](https://x.y/logo.png) b ![](http://x/y.gif) c";
        assert_eq!(prepare(md, dir.path(), dir.path()), "a the logo b image c");
        // Inside a link, the link stays.
        let md = "[![shot](https://x/a.png)](https://x)";
        assert_eq!(prepare(md, dir.path(), dir.path()), "[shot](https://x)");
    }

    #[test]
    fn data_images_stay() {
        let dir = tempfile::tempdir().unwrap();
        let md = "![d](data:image/png;base64,iVBORw0KGgo=)";
        assert_eq!(prepare(md, dir.path(), dir.path()), md);
    }

    #[test]
    fn local_images_inside_the_folder_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        std::fs::create_dir_all(root.join("docs").join("img")).unwrap();
        std::fs::write(root.join("docs").join("img").join("a b.png"), b"x").unwrap();
        std::fs::write(dir.path().join("out.png"), b"x").unwrap();
        let docs = root.join("docs");
        let shown = prepare("![A](img/a%20b.png)", &docs, &root);
        let want = normalize(&root.join("docs").join("img").join("a b.png")).to_string_lossy().replace('\\', "/");
        let slashes = if want.starts_with('/') { "//" } else { "///" };
        assert_eq!(shown, format!("![A](<file:{slashes}{want}>)"), "a local file URI, never a host");
        // Outside the session's folder, missing, not an image, or a file: URL: alt text.
        assert_eq!(prepare("![B](../../out.png)", &docs, &root), "B");
        assert_eq!(prepare("![C](img/none.png)", &docs, &root), "C");
        assert_eq!(prepare("![D](file:///C:/x.png)", &docs, &root), "D");
        assert_eq!(prepare("![a_b]", &docs, &root), "![a_b]");
        assert_eq!(prepare("![x*y](https://h/i.png)", &docs, &root), "x\\*y");
    }

    #[test]
    fn reads_with_a_cap_and_a_note_on_error() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("notes.md");
        std::fs::write(&p, "\u{feff}# Hi").unwrap();
        assert_eq!(read(&p), "# Hi");
        let big = dir.path().join("big.md");
        std::fs::write(&big, vec![b'a'; DOC_LIMIT as usize + 1]).unwrap();
        assert_eq!(read(&big), "*big.md is too big to show here.*");
        let gone = read(&dir.path().join("gone.md"));
        assert!(gone.starts_with("*Couldn't read gone.md: "), "{gone}");
        assert!(gone.ends_with('*'));
    }
}
