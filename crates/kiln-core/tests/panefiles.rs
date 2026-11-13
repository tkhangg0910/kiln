//! checks/panefiles.cs: the doc, page and image chips. Which docs a transcript says were
//! written, when a prompt was sent, and which pasted images are still waiting. Temp folders only.

use chrono::{DateTime, Duration, Utc};
use kiln_core::images;
use kiln_core::sessions::tail::TranscriptTail;
use std::io::Write as _;
use std::path::{Path, PathBuf};

fn esc(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

fn ts(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// The time a timestamp written by [`ts`] reads back as (milliseconds).
fn ms(t: DateTime<Utc>) -> DateTime<Utc> {
    kiln_core::time::parse_iso(&ts(t)).unwrap()
}

fn write_line(path: &Path, at: DateTime<Utc>) -> String {
    format!(
        "{{\"type\":\"assistant\",\"timestamp\":\"{}\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"tool_use\",\"name\":\"Write\",\"input\":{{\"file_path\":{},\"content\":\"x\"}}}}]}}}}",
        ts(at),
        esc(&path.to_string_lossy())
    )
}

fn prompt(at: DateTime<Utc>) -> String {
    format!("{{\"type\":\"user\",\"timestamp\":\"{}\",\"message\":{{\"role\":\"user\",\"content\":\"see docs/a.md\"}}}}", ts(at))
}

fn tool_result(at: DateTime<Utc>) -> String {
    format!(
        "{{\"type\":\"user\",\"timestamp\":\"{}\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"tool_result\",\"content\":\"ok\"}}]}}}}",
        ts(at)
    )
}

fn patch(path: &str, at: DateTime<Utc>) -> String {
    format!(
        "{{\"timestamp\":\"{}\",\"type\":\"response_item\",\"payload\":{{\"type\":\"custom_tool_call\",\"name\":\"apply_patch\",\"input\":{}}}}}",
        ts(at),
        esc(&format!("*** Begin Patch\n*** Add File: {path}\n+# hi\n*** End Patch\n"))
    )
}

fn codex_user(text: &str, at: DateTime<Utc>) -> String {
    format!(
        "{{\"timestamp\":\"{}\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":[{{\"type\":\"input_text\",\"text\":{}}}]}}}}",
        ts(at),
        esc(text)
    )
}

fn append(path: &Path, text: &str) {
    std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap().write_all(text.as_bytes()).unwrap();
}

struct Folder(PathBuf);

impl Folder {
    /// A doc the transcript names, as it is on disk.
    fn doc(&self, parts: &[&str]) -> PathBuf {
        let mut p = self.0.clone();
        p.extend(parts);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "# doc").unwrap();
        p
    }

    fn at(&self, parts: &[&str]) -> PathBuf {
        let mut p = self.0.clone();
        p.extend(parts);
        p
    }
}

fn names(paths: &[PathBuf]) -> String {
    paths.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>().join(",")
}

#[test]
fn claude_tail() {
    let dir = tempfile::tempdir().unwrap();
    let folder = Folder(dir.path().join("proj"));
    std::fs::create_dir_all(&folder.0).unwrap();
    let opened = Utc::now();
    let file = dir.path().join("t.jsonl");

    append(&file, &(write_line(&folder.at(&["GONE.md"]), opened - Duration::minutes(6)) + "\n"));
    let mut tail = TranscriptTail::new(&folder.0, opened, false);
    assert!(!tail.read(&file), "a doc that was deleted since: ignored");

    let old = folder.doc(&["OLD.md"]);
    append(&file, &format!("{}\n{}\n", write_line(&old, opened - Duration::minutes(5)), prompt(opened - Duration::minutes(4))));
    assert!(tail.read(&file), "a doc from before the pane opened (resumed): seen");
    assert_eq!(tail.doc(), Some(old.as_path()));
    assert_eq!(tail.prompt_at(), None, "  but a prompt from before then is not");
    folder.doc(&["docs", "PLAN.md"]);
    folder.doc(&["HALF.md"]);
    folder.doc(&["notes", "README.md"]);

    append(&file, &(write_line(&folder.at(&["docs", "PLAN.md"]), opened + Duration::seconds(1)) + "\n"));
    assert!(tail.read(&file), "a doc written since: seen");
    assert_eq!(tail.doc(), Some(folder.at(&["docs", "PLAN.md"]).as_path()), "  as its full path");
    assert!(!tail.read(&file), "nothing new: nothing to say");

    append(&file, &(write_line(&dir.path().join("elsewhere.md"), opened + Duration::seconds(2)) + "\n"));
    assert!(!tail.read(&file), "a doc outside the folder: ignored");
    append(&file, &(write_line(&folder.at(&["code.cs"]), opened + Duration::seconds(2)) + "\n"));
    assert!(!tail.read(&file), "a file that is not markdown: ignored");

    append(&file, &write_line(&folder.at(&["HALF.md"]), opened + Duration::seconds(3)));
    assert!(!tail.read(&file), "a line still being written: waits");
    append(&file, "\n");
    assert!(tail.read(&file), "  and is read once whole");
    assert_eq!(tail.doc(), Some(folder.at(&["HALF.md"]).as_path()));

    assert_eq!(tail.prompt_at(), None, "no prompt yet");
    let sent = opened + Duration::seconds(4);
    append(&file, &format!("{}\n{}\n", prompt(sent), tool_result(sent + Duration::seconds(5))));
    tail.read(&file);
    assert_eq!(tail.prompt_at(), Some(ms(sent)), "a prompt you sent is its time, a tool result is not");
    let meta = prompt(sent + Duration::seconds(6)).replace("{\"type\":\"user\",", "{\"type\":\"user\",\"isMeta\":true,");
    append(&file, &(meta + "\n"));
    tail.read(&file);
    assert_eq!(tail.prompt_at(), Some(ms(sent)), "  nor one of Claude's meta lines");

    // Pages: an .html the session wrote in .kiln/pages, and nowhere else.
    let page_a = folder.doc(&[".kiln", "pages", "a.html"]);
    let page_b = folder.doc(&[".kiln", "pages", "b.html"]);
    folder.doc(&["site", "index.html"]);
    assert_eq!(tail.page(), None, "no page yet");
    append(&file, &(write_line(&page_a, opened + Duration::seconds(10)) + "\n"));
    assert!(tail.read(&file), "a page written in .kiln/pages: seen");
    assert_eq!(tail.page(), Some(page_a.as_path()));
    assert_eq!(tail.doc(), Some(folder.at(&["HALF.md"]).as_path()), "  and the doc stays the doc");
    append(&file, &(write_line(&folder.at(&["site", "index.html"]), opened + Duration::seconds(11)) + "\n"));
    assert!(!tail.read(&file), "an .html outside .kiln/pages: not a page");
    append(
        &file,
        &format!("{}\n{}\n", write_line(&page_b, opened + Duration::seconds(12)), write_line(&page_a, opened + Duration::seconds(13))),
    );
    tail.read(&file);
    assert_eq!(names(tail.pages()), "b.html,a.html", "a page written again is the newest, listed once");
    assert_eq!(tail.page_writes(), 3, "  and each write is counted");
}

#[test]
fn codex_tail() {
    let dir = tempfile::tempdir().unwrap();
    let folder = Folder(dir.path().join("proj"));
    std::fs::create_dir_all(&folder.0).unwrap();
    let opened = Utc::now();
    let readme = folder.doc(&["notes", "README.md"]);
    let page_a = folder.doc(&[".kiln", "pages", "a.html"]);
    folder.doc(&["site", "index.html"]);
    std::fs::write(dir.path().join("escape.md"), "# out").unwrap();

    let cfile = dir.path().join("c.jsonl");
    append(&cfile, &(patch("notes/README.md", opened + Duration::seconds(1)) + "\n"));
    let mut ctail = TranscriptTail::new(&folder.0, opened, true);
    assert!(ctail.read(&cfile), "codex: a patch adding a doc, by relative path");
    assert_eq!(ctail.doc(), Some(readme.as_path()));
    append(&cfile, &(patch("../escape.md", opened + Duration::seconds(2)) + "\n"));
    assert!(!ctail.read(&cfile), "codex: a path leaving the folder: ignored");
    // Escaped backslashes in the JSON: "\notes" must not read as the patch's line break.
    append(&cfile, &(patch(&readme.to_string_lossy(), opened + Duration::seconds(2)) + "\n"));
    assert!(ctail.read(&cfile), "codex: a patch by full Windows path");
    assert_eq!(ctail.doc(), Some(readme.as_path()));

    // Code mode (0.162): the patch sits in a JS string in exec's input, its line breaks escaped
    // twice; the FileChange that follows names the file plainly.
    let scratch = folder.doc(&["docs", "scratch", "codex-test.md"]);
    let at = ts(opened + Duration::seconds(3));
    let js = format!(
        "const r = await tools.apply_patch({});",
        esc("*** Begin Patch\n*** Add File: docs/scratch/codex-test.md\n+# hi\n*** End Patch\n")
    );
    append(
        &cfile,
        &format!(
            "{{\"timestamp\":\"{at}\",\"type\":\"response_item\",\"payload\":{{\"type\":\"custom_tool_call\",\"name\":\"exec\",\"input\":{}}}}}\n",
            esc(&js)
        ),
    );
    append(
        &cfile,
        &format!(
            "{{\"timestamp\":\"{at}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"item_completed\",\"item\":{{\"type\":\"FileChange\",\"changes\":{{{}:{{\"type\":\"add\",\"content\":\"# hi\\n\"}}}}}}}}}}\n",
            esc(&scratch.to_string_lossy())
        ),
    );
    assert!(ctail.read(&cfile), "codex: a patch run from exec (code mode)");
    assert_eq!(ctail.doc(), Some(scratch.as_path()));

    append(&cfile, &(patch(".kiln/pages/a.html", opened + Duration::seconds(3)) + "\n"));
    assert!(ctail.read(&cfile), "codex: a patch adding a page");
    assert_eq!(ctail.page(), Some(page_a.as_path()));
    append(&cfile, &(patch("site/index.html", opened + Duration::seconds(3)) + "\n"));
    assert!(!ctail.read(&cfile), "codex: an .html outside .kiln/pages: not a page");

    append(&cfile, &(codex_user("<environment_context>cwd</environment_context>", opened + Duration::seconds(3)) + "\n"));
    ctail.read(&cfile);
    assert_eq!(ctail.prompt_at(), None, "codex: its own context is not a prompt");
    let csent = opened + Duration::seconds(4);
    append(&cfile, &(codex_user("what is in this screenshot?", csent) + "\n"));
    ctail.read(&cfile);
    assert_eq!(ctail.prompt_at(), Some(ms(csent)), "codex: a prompt you sent is its time");
}

#[test]
fn first_read_skips_to_the_last_512_kib() {
    let dir = tempfile::tempdir().unwrap();
    let folder = Folder(dir.path().join("proj"));
    let early = folder.doc(&["EARLY.md"]);
    let file = dir.path().join("t.jsonl");
    let t0 = Utc::now();
    let pad = format!("{{\"type\":\"assistant\",\"pad\":\"{}\"}}\n", "x".repeat(4000));
    append(&file, &(write_line(&early, t0) + "\n" + &pad.repeat(140)));
    let mut tail = TranscriptTail::new(&folder.0, t0, false);
    assert!(!tail.read(&file), "a doc beyond the first window is not read");
    assert_eq!(tail.doc(), None);
}

fn touch(path: &Path, at: DateTime<Utc>) {
    std::fs::write(path, "").unwrap();
    std::fs::File::options().write(true).open(path).unwrap().set_modified(at.into()).unwrap();
}

#[test]
fn pasted_images() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("claude");
    let imgs = root.join("C--proj").join("sess-1").join("images");
    assert_eq!(images::find_dir(&root, "sess-1"), None, "no paste yet: no folder");
    std::fs::create_dir_all(&imgs).unwrap();
    assert_eq!(images::find_dir(&root, "sess-1"), Some(imgs.clone()), "found under its project");
    assert_eq!(images::find_dir(&root, r"..\C--proj\sess-1"), None, "an id that is a path finds nothing");

    let since = Utc::now();
    for (name, age) in [("1.png", -60), ("10.png", 5), ("2.webp", 5), ("notes.txt", 5)] {
        touch(&imgs.join(name), since + Duration::seconds(age));
    }
    assert_eq!(names(&images::since(&imgs, since, "*")), "2.webp,10.png", "only images since then, in tag order");
    assert_eq!(images::since(&dir.path().join("none"), since, "*").len(), 0, "a folder that is gone: none");

    // Codex: loose clipboard files in %TEMP%, among everything else there.
    let temp = dir.path().join("temp");
    std::fs::create_dir_all(&temp).unwrap();
    for (name, age) in [("codex-clipboard-b.png", 8), ("codex-clipboard-a.png", 4), ("codex-clipboard-old.png", -60), ("other.png", 5)] {
        touch(&temp.join(name), since + Duration::seconds(age));
    }
    assert_eq!(
        names(&images::since(&temp, since, images::CODEX_PATTERN)),
        "codex-clipboard-a.png,codex-clipboard-b.png",
        "codex: only its clipboard files since then, oldest first"
    );
}

#[test]
fn image_roots_from_env() {
    use kiln_core::paths::MapEnv;
    let env = MapEnv::default().with("TEMP", r"C:\t");
    assert_eq!(images::claude_root(&env), Path::new(r"C:\t").join("claude"));
    assert_eq!(images::codex_dir(&env), PathBuf::from(r"C:\t"));
    let env = env.with("CLAUDE_CODE_TMPDIR", r"D:\cc");
    assert_eq!(images::claude_root(&env), Path::new(r"D:\cc").join("claude"));
}
