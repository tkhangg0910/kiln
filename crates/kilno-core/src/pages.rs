//! Pages an agent writes (`<project>\.kilno\pages\*.html`): sealed with a Content-Security-Policy
//! before they are shown, and their folder kept out of git.

use std::path::Path;
use std::sync::LazyLock;

use regex::{Captures, Regex};

/// Where the shared page assets are served from in the C# viewer.
pub const ASSETS_URL: &str = "https://ccx.assets/pages/";

/// Scripts and styles from the page itself, Kilno's assets and three CDNs; images and fonts only
/// from inside the page; no connections and no form posts. A page can't send anything anywhere.
pub const CSP: &str = "default-src 'none'; \
    script-src 'unsafe-inline' https://ccx.assets https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com; \
    style-src 'unsafe-inline' https://ccx.assets https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com; \
    img-src data: blob: https://ccx.assets; \
    font-src data: https://ccx.assets; \
    connect-src 'none'; form-action 'none'";

/// [`CSP`] for a sealed copy opened in the default browser (PORTING.md D3): Kilno's assets sit
/// beside the file, so where the viewer allowed `https://ccx.assets` this allows the copy's own
/// folder. `'self'` is what a browser that gives `file:` pages an origin matches; `file:` covers
/// the ones that make every file its own opaque origin. Still no connections and no form posts.
pub const BROWSER_CSP: &str = "default-src 'none'; \
    script-src 'unsafe-inline' 'self' file: https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com; \
    style-src 'unsafe-inline' 'self' file: https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com; \
    img-src data: blob: 'self' file:; \
    font-src data: 'self' file:; \
    connect-src 'none'; form-action 'none'";

/// The asset files a page links to, beside it in a browser copy.
pub const ASSET_FILES: [&str; 4] = [
    "clayo-page.css",
    "clayo-page.js",
    "chart.umd.min.js",
    "mermaid.min.js",
];

static DOCTYPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\s*<!doctype[^>]*>").expect("doctype regex"));

// href="clayo-page.css" and src="clayo-page.js", as the guide has them, with or without ./
// Both quotes are captured and must be the same (the regex crate has no backreferences).
static ASSET_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(href|src)(\s*=\s*)(["'])(?:\./)?(clayo-page\.(?:css|js))(["'])"#)
        .expect("asset link regex")
});

/// The page with the policy as the first thing in it, straight after the doctype, where the
/// browser makes it the first element of the head whatever the page's own markup says: a policy
/// put after a tag the page wrote could be commented out by the page. Links to Kilno's
/// stylesheet and script point at [`ASSETS_URL`].
pub fn seal(html: &str) -> String {
    seal_with(html, CSP, ASSETS_URL)
}

/// [`seal`] for a copy opened in the default browser, with Kilno's assets written beside it:
/// the asset links become `./clayo-page.css|js` and the policy is [`BROWSER_CSP`].
pub fn seal_for_browser(html: &str) -> String {
    seal_with(html, BROWSER_CSP, "./")
}

fn seal_with(html: &str, csp: &str, assets: &str) -> String {
    let meta = format!("<meta http-equiv=\"Content-Security-Policy\" content=\"{csp}\">");
    let at = DOCTYPE.find(html).map_or(0, |m| m.end());
    let html = format!("{}{meta}{}", &html[..at], &html[at..]);
    ASSET_LINK
        .replace_all(&html, |c: &Captures| {
            if c[3] != c[5] {
                return c[0].to_string();
            }
            format!("{}{}{}{assets}{}{}", &c[1], &c[2], &c[3], &c[4], &c[3])
        })
        .into_owned()
}

const EXCLUDE_LINE: &str = ".kilno/";

/// Keeps a project's pages out of git without touching its .gitignore: adds `.kilno/` to the
/// repo's own `.git\info\exclude`, which is never committed. The session's folder may sit below
/// the repo's root. Does nothing outside a git repo, when the line is already there, or when the
/// file can't be read or written.
pub fn hide_from_git(project_folder: &Path) {
    let Some(root) = project_folder.ancestors().find(|d| d.join(".git").is_dir()) else {
        return;
    };
    let exclude = root.join(".git").join("info").join("exclude");
    let text = match std::fs::read_to_string(&exclude) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return,
    };
    if text.split('\n').any(|l| l.trim() == EXCLUDE_LINE) {
        return;
    }
    let sep = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let _ = (|| -> std::io::Result<()> {
        use std::io::Write;
        std::fs::create_dir_all(root.join(".git").join("info"))?;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&exclude)?;
        writeln!(f, "{sep}{EXCLUDE_LINE}")
    })();
}

/// How long a browser copy is kept (PORTING.md D3), as the screenshots are.
pub const KEEP_FOR: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

/// Past this a page is carrying a library or a dataset it should not (TP:648).
pub const PAGE_LIMIT: u64 = 5 * 1024 * 1024;

/// `<data>\pages`: the sealed copies opened in the browser.
pub fn copies_dir(data: &crate::paths::DataDir) -> std::path::PathBuf {
    data.join("pages")
}

/// Writes a sealed copy of `page` for the default browser (PORTING.md D3) and returns its path:
/// `<root>\<hash of the page's path>\<name>`, the same folder each time for the same page, with
/// Kilno's `assets` (name, bytes) beside it, written only when missing or changed in size. A
/// page too big or unreadable becomes a dark note saying so (TP:643-677).
pub fn browser_copy(root: &Path, page: &Path, assets: &[(&str, &[u8])]) -> std::io::Result<std::path::PathBuf> {
    let name = page.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "page.html".into());
    let dir = root.join(path_hash(page));
    std::fs::create_dir_all(&dir)?;
    for (file, bytes) in assets {
        let at = dir.join(file);
        if std::fs::metadata(&at).map(|m| m.len()).ok() != Some(bytes.len() as u64) {
            std::fs::write(&at, bytes)?;
        }
    }
    let read = || -> std::io::Result<String> {
        if std::fs::metadata(page)?.len() > PAGE_LIMIT {
            return Ok(note(&format!("{name} is too big to show here.")));
        }
        Ok(seal_for_browser(&String::from_utf8_lossy(&std::fs::read(page)?)))
    };
    let html = read().unwrap_or_else(|e| note(&format!("Couldn't read {name}: {e}")));
    let out = dir.join(&name);
    std::fs::write(&out, html)?;
    Ok(out)
}

/// Deletes the browser copies none of whose files was written since `before`. A folder that
/// can't be read or deleted is left as it is.
pub fn prune_copies(root: &Path, before: std::time::SystemTime) {
    let Ok(dirs) = std::fs::read_dir(root) else { return };
    for d in dirs.flatten().filter(|d| d.file_type().is_ok_and(|t| t.is_dir())) {
        let Ok(files) = std::fs::read_dir(d.path()) else { continue };
        let newest = files.flatten().filter_map(|f| f.metadata().and_then(|m| m.modified()).ok()).max();
        if newest.is_none_or(|t| t < before) {
            let _ = std::fs::remove_dir_all(d.path());
        }
    }
}

/// FNV-1a over the lower-cased path: a short, stable folder name per page.
fn path_hash(page: &Path) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in page.to_string_lossy().to_lowercase().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// A page's note on Kilno's dark ground (TP:676).
fn note(text: &str) -> String {
    let text = text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    format!("<!doctype html><meta charset=\"utf-8\"><body style=\"margin:0;padding:18px 24px;background:#14171c;color:#7a828f;font:14px 'Segoe UI',sans-serif\">{text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_copy_is_sealed_with_its_assets() {
        let dir = tempfile::tempdir().unwrap();
        let page = dir.path().join("proj").join(".kilno").join("pages").join("report.html");
        std::fs::create_dir_all(page.parent().unwrap()).unwrap();
        std::fs::write(&page, "<!doctype html><link href=\"clayo-page.css\"><p>hi").unwrap();
        let root = dir.path().join("copies");
        let out = browser_copy(&root, &page, &[("clayo-page.css", b"body{}")]).unwrap();
        assert_eq!(out.file_name().unwrap(), "report.html");
        let html = std::fs::read_to_string(&out).unwrap();
        assert!(html.starts_with(&format!("<!doctype html>{META} content=\"{BROWSER_CSP}\">")));
        assert!(html.contains("href=\"./clayo-page.css\""));
        assert_eq!(std::fs::read(out.with_file_name("clayo-page.css")).unwrap(), b"body{}");
        // The same page, the same folder; another page, another one.
        let again = browser_copy(&root, &page, &[]).unwrap();
        assert_eq!(again, out);
        let other = browser_copy(&root, &page.with_file_name("b.html"), &[]).unwrap();
        assert_ne!(other.parent(), out.parent());
        assert!(std::fs::read_to_string(&other).unwrap().contains("Couldn't read b.html: "));
    }

    #[test]
    fn old_copies_are_pruned() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("a");
        let new = dir.path().join("b");
        for d in [&old, &new] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("x.html"), "x").unwrap();
        }
        let past = std::time::SystemTime::now() - KEEP_FOR - std::time::Duration::from_secs(60);
        std::fs::File::options().write(true).open(old.join("x.html")).unwrap().set_modified(past).unwrap();
        prune_copies(dir.path(), std::time::SystemTime::now() - KEEP_FOR);
        assert!(!old.exists());
        assert!(new.exists());
    }

    const META: &str = "<meta http-equiv=\"Content-Security-Policy\"";

    #[test]
    fn browser_copy_links_assets_beside_it() {
        let s = seal_for_browser(
            "<!doctype html><link href='clayo-page.css'><script src=\"./clayo-page.js\"></script>",
        );
        assert!(s.starts_with(&format!("<!doctype html>{META}")));
        assert!(s.contains("href='./clayo-page.css'"));
        assert!(s.contains("src=\"./clayo-page.js\""));
        assert!(!s.contains("ccx.assets"));
    }

    #[test]
    fn browser_policy_still_sends_nothing() {
        assert!(BROWSER_CSP.starts_with("default-src 'none'"));
        assert!(BROWSER_CSP.contains("connect-src 'none'"));
        assert!(BROWSER_CSP.contains("form-action 'none'"));
        assert!(BROWSER_CSP.contains("script-src 'unsafe-inline' 'self' file:"));
        assert!(!BROWSER_CSP.contains("ccx.assets"));
        let s = seal_for_browser("<p>");
        assert!(s.starts_with(&format!("{META} content=\"{BROWSER_CSP}\">")));
    }

    #[test]
    fn browser_copy_leaves_other_links_alone() {
        let other = "<script src=\"https://example.com/clayo-page.js\"></script>";
        assert!(seal_for_browser(other).ends_with(&format!(">{other}")));
    }

    #[test]
    fn mismatched_quotes_are_not_a_link() {
        let odd = "<link href=\"clayo-page.css'>";
        assert!(seal(odd).ends_with(&format!(">{odd}")));
    }
}
