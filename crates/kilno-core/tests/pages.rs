//! Port of checks/pages.cs: how a page an agent wrote is sealed, and how its folder is kept out
//! of git. Temp folders only.

use std::fs;

use kilno_core::pages::{hide_from_git, seal, CSP};

const META: &str = "<meta http-equiv=\"Content-Security-Policy\"";
const PAGE: &str = "<!doctype html>\n<html lang=\"en\"><head>\n<meta charset=\"utf-8\">\n<link rel=\"stylesheet\" href=\"clayo-page.css\">\n</head><body>\n<script>var a = 1;</script>\n<script src=\"clayo-page.js\"></script>\n</body></html>";

#[test]
fn the_policy_comes_first() {
    let s = seal(PAGE);
    assert!(
        s.starts_with(&format!("<!doctype html>{META}")),
        "straight after the doctype"
    );
    assert!(
        s.find(META).unwrap() < s.find("<script").unwrap(),
        "before any script"
    );
    assert_eq!(s.matches(META).count(), 1, "once");
    assert!(
        s.ends_with("<script>var a = 1;</script>\n<script src=\"https://ccx.assets/pages/clayo-page.js\"></script>\n</body></html>"),
        "and the page follows unchanged"
    );
}

#[test]
fn a_page_with_no_head_and_no_doctype_starts_with_it() {
    assert!(seal("<h1>hi</h1><script>x()</script>").starts_with(META));
}

#[test]
fn a_doctype_in_other_case_is_kept_first() {
    assert!(seal("  <!DOCTYPE HTML><p>").starts_with(&format!("  <!DOCTYPE HTML>{META}")));
}

#[test]
fn a_page_cant_comment_the_policy_out() {
    let trick = seal("<!doctype html><!-- <head> --><script>fetch('https://example.com')</script>");
    assert!(trick.find(META).unwrap() < trick.find("<!--").unwrap());
}

#[test]
fn what_the_policy_allows() {
    assert!(CSP.contains("connect-src 'none'"), "no connections");
    assert!(CSP.contains("form-action 'none'"), "no form posts");
    assert!(CSP.starts_with("default-src 'none'"), "nothing by default");
    assert!(
        CSP.contains("img-src data: blob: https://ccx.assets;"),
        "images only from the page and Kilno"
    );
}

#[test]
fn the_stylesheet_link() {
    assert!(seal(PAGE).contains("href=\"https://ccx.assets/pages/clayo-page.css\""));
}

#[test]
fn single_quotes_and_dot_slash() {
    let s = seal("<link href='./clayo-page.css'><script src = './clayo-page.js'></script>");
    assert!(s[META.len()..].contains(
        "href='https://ccx.assets/pages/clayo-page.css'><script src = 'https://ccx.assets/pages/clayo-page.js'>"
    ));
}

#[test]
fn other_links_left_alone() {
    for other in [
        "<link rel=\"stylesheet\" href=\"other.css\">",
        "<link rel=\"stylesheet\" href=\"my-clayo-page.css\">",
        "<link rel=\"stylesheet\" href=\"sub/clayo-page.css\">",
        "<script src=\"https://cdn.jsdelivr.net/npm/d3@7.9.0/dist/d3.min.js\"></script>",
        "<a href=\"https://example.com/clayo-page.js\">x</a>",
        "<p>clayo-page.css</p>",
    ] {
        assert!(
            seal(other).ends_with(&format!(">{other}")),
            "left alone: {other}"
        );
    }
}

#[test]
fn hide_from_git_adds_kilno_to_the_repo_exclude_file() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    let sub = repo.join("src").join("app");
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::create_dir_all(&sub).unwrap();
    let exclude = repo.join(".git").join("info").join("exclude");

    hide_from_git(&repo);
    assert_eq!(
        fs::read_to_string(&exclude).unwrap(),
        ".kilno/\n",
        "a repo gets .kilno/"
    );
    hide_from_git(&repo);
    assert_eq!(fs::read_to_string(&exclude).unwrap(), ".kilno/\n", "once");

    fs::write(&exclude, "# local\n*.tmp").unwrap();
    hide_from_git(&sub);
    assert_eq!(
        fs::read_to_string(&exclude).unwrap(),
        "# local\n*.tmp\n.kilno/\n",
        "from a folder below the root, after what is there"
    );
}

#[test]
fn hide_from_git_outside_a_repo_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain");
    fs::create_dir_all(&plain).unwrap();
    hide_from_git(&plain);
    assert_eq!(fs::read_dir(&plain).unwrap().count(), 0);
}
