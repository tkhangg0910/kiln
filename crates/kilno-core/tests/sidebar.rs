//! Sidebar rows and order (MainWindow.xaml.cs SessionRow / RefreshSessions / ApplyFilter /
//! LinkThreads / RecentFolderList / RecentSessions / BindCodexPanes). No C# check exists; one test
//! per rule in PORTING.md MAIN-046..074 and SES-083..099.

use chrono::{DateTime, FixedOffset, TimeDelta, TimeZone, Utc};
use kilno_core::agent::AgentKind;
use kilno_core::pane_status::PaneStatus;
use kilno_core::sessions::model::NO_PROMPT;
use kilno_core::sessions::SessionInfo;
use kilno_core::sidebar::*;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 13, 15, 0, 0).unwrap()
}

fn utc() -> FixedOffset {
    FixedOffset::east_opt(0).unwrap()
}

fn info(id: &str, dir: &str, ago_min: i64) -> SessionInfo {
    let t = now() - TimeDelta::minutes(ago_min);
    SessionInfo {
        session_id: id.into(),
        agent: AgentKind::Claude,
        parent_id: None,
        file: None,
        project_dir: dir.into(),
        preview: NO_PROMPT.into(),
        ai_title: None,
        last_prompt: None,
        last_activity: t,
        started: t,
        turn: None,
        status: None,
    }
}

fn pane(id: u64, status: PaneStatus, dir: &str) -> PaneRef {
    PaneRef {
        id,
        agent: AgentKind::Claude,
        status,
        working_dir: dir.into(),
    }
}

fn row_with(i: SessionInfo) -> SessionRow {
    let mut r = SessionRow::new(i.session_id.clone());
    r.folder = i.project_dir.clone();
    r.info = Some(i);
    r
}

fn ids(rows: &[&SessionRow]) -> Vec<String> {
    rows.iter().map(|r| r.session_id.clone()).collect()
}

// ------------------------------------------------------------------ names

#[test]
fn name_custom_wins() {
    let mut i = info("a", "C:\\p", 0);
    i.ai_title = Some("AI".into());
    i.preview = "first".into();
    let mut r = row_with(i);
    r.custom_name = Some("Mine".into());
    assert_eq!(r.name(), "Mine");
}

#[test]
fn name_ai_title_before_preview() {
    let mut i = info("a", "C:\\p", 0);
    i.ai_title = Some("AI".into());
    i.preview = "first".into();
    assert_eq!(row_with(i).name(), "AI");
}

#[test]
fn name_preview_before_fallback() {
    let mut i = info("a", "C:\\p", 0);
    i.preview = "first prompt".into();
    assert_eq!(row_with(i).name(), "first prompt");
}

#[test]
fn name_no_prompt_yet_falls_back() {
    let mut r = row_with(info("a", "C:\\p", 0));
    assert_eq!(r.name(), "new session");
    r.fallback_title = new_title("C:\\work\\proj\\");
    assert_eq!(r.name(), "new · proj");
}

#[test]
fn name_empty_preview_falls_back() {
    let mut i = info("a", "C:\\p", 0);
    i.preview = String::new();
    assert_eq!(row_with(i).name(), "new session");
}

#[test]
fn fallback_titles() {
    assert_eq!(new_title("D:\\code\\kilno"), "new · kilno");
    assert_eq!(branch_title("0123456789abcdef"), "branch of 01234567");
    assert_eq!(branch_title("abc"), "branch of abc");
}

#[test]
fn rename_rules() {
    let mut i = info("a", "C:\\p", 0);
    i.ai_title = Some("Generated".into());
    let mut r = row_with(i);
    assert_eq!(r.rename_to("  New name "), Some("New name".into()));
    assert_eq!(r.rename_to("   "), None, "blank clears");
    assert_eq!(
        r.rename_to("Generated"),
        None,
        "the generated title is no override"
    );
    assert_eq!(r.edit_text(), "Generated");
    r.custom_name = Some("Mine".into());
    assert_eq!(r.edit_text(), "Mine");
}

// ------------------------------------------------------------- age / status

#[test]
fn age_text_steps() {
    let tz = utc();
    let n = now();
    let ago = |d: TimeDelta| age_text(n - d, n, &tz);
    assert_eq!(ago(TimeDelta::seconds(59)), "just now");
    assert_eq!(
        ago(TimeDelta::seconds(-30)),
        "just now",
        "a time in the future"
    );
    assert_eq!(ago(TimeDelta::minutes(1)), "1 min ago");
    assert_eq!(ago(TimeDelta::seconds(59 * 60 + 59)), "59 min ago");
    assert_eq!(ago(TimeDelta::minutes(60)), "1 h ago");
    assert_eq!(ago(TimeDelta::minutes(23 * 60 + 59)), "23 h ago");
    assert_eq!(ago(TimeDelta::hours(24)), "1 d ago");
    assert_eq!(ago(TimeDelta::hours(6 * 24 + 23)), "6 d ago");
    assert_eq!(ago(TimeDelta::days(7)), "6 Nov");
    assert_eq!(
        age_text(Utc.with_ymd_and_hms(2026, 3, 1, 10, 0, 0).unwrap(), n, &tz),
        "1 Mar"
    );
}

#[test]
fn age_date_is_local() {
    let tz = FixedOffset::east_opt(7 * 3600).unwrap();
    let when = Utc.with_ymd_and_hms(2026, 10, 31, 20, 0, 0).unwrap();
    assert_eq!(age_text(when, now(), &tz), "1 Nov");
}

#[test]
fn when_is_now_without_transcript() {
    let r = SessionRow::new("x");
    assert_eq!(r.when(now()), now());
    assert_eq!(r.age(now(), &utc()), "just now");
}

#[test]
fn status_text_per_status() {
    use PaneStatus::*;
    for (s, t) in [
        (Working, "running"),
        (NeedsInput, "needs you"),
        (Done, "done"),
        (Error, "api error"),
        (Exited, "shell closed"),
        (Starting, "starting"),
    ] {
        assert_eq!(status_label(Some(s)), Some(t));
    }
    assert_eq!(status_label(None), None);
    let r = row_with(info("a", "C:\\p", 5));
    assert_eq!(
        r.status_text(now(), &utc()),
        "5 min ago",
        "no pane: the age"
    );
}

#[test]
fn status_key_per_status() {
    use PaneStatus::*;
    assert_eq!(status_key(Some(Working)), "Working");
    assert_eq!(status_key(Some(Starting)), "Working");
    assert_eq!(status_key(Some(NeedsInput)), "NeedsInput");
    assert_eq!(status_key(Some(Done)), "Done");
    assert_eq!(status_key(Some(Error)), "Failed");
    assert_eq!(status_key(Some(Exited)), "Ended");
    assert_eq!(status_key(None), "Dormant");
}

#[test]
fn subtitle_open_is_status_else_age() {
    let mut r = row_with(info("a", "C:\\p", 120));
    assert_eq!(r.subtitle(now(), &utc()), "2 h ago");
    r.pane = Some(pane(1, PaneStatus::NeedsInput, "C:\\p"));
    assert_eq!(r.subtitle(now(), &utc()), "needs you");
    assert_eq!(r.status_key(), "NeedsInput");
}

#[test]
fn tooltip_lines() {
    let mut i = info("abc", "C:\\work\\proj", 0);
    i.preview = "hello".into();
    let mut r = row_with(i);
    assert_eq!(r.tooltip(), "hello\nC:\\work\\proj\nabc");
    r.branch_of = Some("Parent".into());
    r.info.as_mut().unwrap().last_prompt = Some("do it".into());
    assert_eq!(
        r.tooltip(),
        "hello\nC:\\work\\proj\nabc\n\nBranched from: Parent\n\nLast: do it"
    );
    let mut bare = SessionRow::new("p1");
    bare.folder = "D:\\x".into();
    assert_eq!(bare.tooltip(), "new session\nD:\\x\np1");
}

#[test]
fn project_from_transcript_else_folder_leaf() {
    let r = row_with(info("a", "C:\\work\\proj\\", 0));
    assert_eq!(r.project(), "proj");
    let mut b = SessionRow::new("b");
    b.folder = "D:\\code\\thing".into();
    assert_eq!(b.project(), "thing");
}

#[test]
fn agent_tag_only_when_both_in_use() {
    let mut i = info("a", "C:\\p", 0);
    i.agent = AgentKind::Codex;
    let mut r = row_with(i);
    assert_eq!(r.agent_tag(), None);
    r.show_agent = true;
    assert_eq!(r.agent_tag(), Some("CX"));
    assert_eq!(r.agent_color(), "#7DBBEB");
    let mut c = SessionRow::new("c");
    c.show_agent = true;
    assert_eq!(
        c.agent_tag(),
        Some("CC"),
        "no transcript and no pane: Claude"
    );
    assert_eq!(c.agent_color(), "#E39A7E");
    c.pane = Some(PaneRef {
        agent: AgentKind::Codex,
        ..pane(1, PaneStatus::Starting, "C:\\")
    });
    assert_eq!(c.agent_tag(), Some("CX"), "the pane knows its agent");
}

// ---------------------------------------------------------------- buckets

#[test]
fn buckets_open_today_earlier() {
    let tz = utc();
    let mut open = row_with(info("o", "C:\\p", 60 * 24 * 30));
    open.pane = Some(pane(1, PaneStatus::Working, "C:\\p"));
    assert_eq!(open.bucket(now(), &tz), Bucket::Open);
    assert_eq!(
        row_with(info("t", "C:\\p", 14 * 60)).bucket(now(), &tz),
        Bucket::Today,
        "01:00 today"
    );
    assert_eq!(
        row_with(info("e", "C:\\p", 15 * 60 + 1)).bucket(now(), &tz),
        Bucket::Earlier,
        "23:59 yesterday"
    );
    assert!(Bucket::Open < Bucket::Today && Bucket::Today < Bucket::Earlier);
    assert_eq!(Bucket::Earlier.label(), "Earlier");
    assert!(Bucket::Open.expanded_by_default() && Bucket::Today.expanded_by_default());
    assert!(!Bucket::Earlier.expanded_by_default());
}

#[test]
fn today_uses_the_local_calendar() {
    // 15:00 UTC is 00:30 next day at +9:30; 14:00 UTC is then yesterday.
    let tz = FixedOffset::east_opt(9 * 3600 + 1800).unwrap();
    assert_eq!(
        row_with(info("a", "C:\\p", 60)).bucket(now(), &tz),
        Bucket::Earlier
    );
    assert_eq!(
        row_with(info("a", "C:\\p", 60)).bucket(now(), &utc()),
        Bucket::Today
    );
}

// ------------------------------------------------------------------- fold

#[test]
fn fold_keeps_rows_case_insensitively() {
    let mut sb = Sidebar::new();
    sb.open_pane(
        "ABC",
        pane(7, PaneStatus::Working, "C:\\p"),
        "new · p",
        None,
    );
    sb.fold(vec![info("abc", "C:\\p", 0)], false, |_| None);
    assert_eq!(sb.len(), 1);
    let r = sb.get("Abc").unwrap();
    assert!(
        r.info.is_some() && r.is_open(),
        "the pane keeps its row and gains the transcript"
    );
}

#[test]
fn fold_drops_vanished_unless_open() {
    let mut sb = Sidebar::new();
    sb.fold(
        vec![info("a", "C:\\p", 0), info("b", "C:\\p", 0)],
        false,
        |_| None,
    );
    sb.open_pane("b", pane(1, PaneStatus::Done, "C:\\p"), "x", None);
    sb.fold(vec![], false, |_| None);
    assert!(sb.get("a").is_none());
    assert!(sb.get("b").is_some());
}

#[test]
fn fold_sets_names_and_agent_tag() {
    let mut sb = Sidebar::new();
    sb.fold(vec![info("a", "C:\\p", 0)], true, |id| {
        (id == "a").then(|| "Named".to_string())
    });
    let r = sb.get("a").unwrap();
    assert_eq!(r.name(), "Named");
    assert!(r.show_agent);
    assert_eq!(r.folder, "C:\\p");
    sb.fold(vec![info("a", "C:\\p", 0)], false, |_| None);
    assert_eq!(
        sb.get("a").unwrap().custom_name,
        None,
        "reset name clears it"
    );
    assert!(!sb.get("a").unwrap().show_agent);
}

#[test]
fn open_pane_starts_a_row_and_close_pane_ends_it() {
    let mut sb = Sidebar::new();
    let r = sb.open_pane(
        "g1",
        pane(3, PaneStatus::Starting, "D:\\w\\proj"),
        &new_title("D:\\w\\proj"),
        None,
    );
    assert_eq!(r.name(), "new · proj");
    assert_eq!(r.subtitle(now(), &utc()), "starting");
    assert_eq!(r.bucket(now(), &utc()), Bucket::Open);
    sb.set_pane_status(3, PaneStatus::Working);
    assert_eq!(sb.get("g1").unwrap().status(), Some(PaneStatus::Working));
    sb.close_pane(3, false);
    let r = sb.get("g1").unwrap();
    assert!(!r.is_open());
    assert_eq!(r.status(), None);
}

#[test]
fn closing_an_unbound_codex_pane_drops_its_row() {
    let mut sb = Sidebar::new();
    sb.open_pane(
        "placeholder",
        pane(3, PaneStatus::Starting, "D:\\w"),
        "new · w",
        None,
    );
    sb.close_pane(3, true);
    assert!(sb.is_empty());
}

#[test]
fn rekey_moves_the_row() {
    let mut sb = Sidebar::new();
    sb.open_pane(
        "placeholder",
        pane(3, PaneStatus::Starting, "D:\\w"),
        "new · w",
        None,
    );
    sb.rekey("placeholder", "Real-Id");
    assert!(sb.get("placeholder").is_none());
    assert_eq!(sb.get("real-id").unwrap().session_id, "Real-Id");
    assert_eq!(sb.by_pane(3).unwrap().session_id, "Real-Id");
}

// --------------------------------------------------------------- threading

fn threaded() -> Sidebar {
    let mut sb = Sidebar::new();
    let mut child = info("child", "C:\\p", 5);
    child.parent_id = Some("root".into());
    sb.fold(
        vec![
            info("root", "C:\\p", 30),
            child,
            info("grand", "C:\\p", 1),
            info("other", "C:\\p", 10),
        ],
        false,
        |_| None,
    );
    sb
}

#[test]
fn threads_use_transcript_parent_then_recorded() {
    let mut sb = threaded();
    sb.link_threads(
        |id| (id == "grand").then(|| "child".to_string()),
        true,
        now(),
    );
    assert_eq!(sb.get("child").unwrap().parent_id.as_deref(), Some("root"));
    assert_eq!(sb.get("grand").unwrap().parent_id.as_deref(), Some("child"));
    assert_eq!(sb.get("child").unwrap().depth, 1);
    assert_eq!(sb.get("grand").unwrap().depth, 2);
    assert_eq!(sb.get("root").unwrap().depth, 0);
    assert_eq!(
        sb.get("grand").unwrap().branch_of.as_deref(),
        Some("new session")
    );
    assert!(sb.get("grand").unwrap().is_branch());
}

#[test]
fn transcript_parent_beats_recorded() {
    let mut sb = threaded();
    sb.link_threads(|_| Some("other".to_string()), true, now());
    assert_eq!(sb.get("child").unwrap().parent_id.as_deref(), Some("root"));
}

#[test]
fn missing_parent_keeps_marker_without_indent() {
    let mut sb = Sidebar::new();
    let mut i = info("orphan", "C:\\p", 0);
    i.parent_id = Some("gone".into());
    sb.fold(vec![i], false, |_| None);
    sb.link_threads(|_| None, true, now());
    let r = sb.get("orphan").unwrap();
    assert!(r.is_branch());
    assert_eq!(r.depth, 0);
    assert_eq!(r.branch_of, None);
}

#[test]
fn thread_order_concats_chain_from_root() {
    let mut sb = threaded();
    sb.link_threads(
        |id| (id == "grand").then(|| "child".to_string()),
        false,
        now(),
    );
    let stamp_of = |id: &str| stamp(sb.get(id).unwrap().when(now()), false);
    assert_eq!(
        sb.get("grand").unwrap().thread_order,
        format!(
            "{}root/{}child/{}grand/",
            stamp_of("root"),
            stamp_of("child"),
            stamp_of("grand")
        )
    );
}

#[test]
fn depth_capped_at_eight_on_a_cycle() {
    let mut sb = Sidebar::new();
    sb.fold(
        vec![info("a", "C:\\p", 0), info("b", "C:\\p", 0)],
        false,
        |_| None,
    );
    sb.link_threads(
        |id| Some(if id == "a" { "b" } else { "a" }.to_string()),
        true,
        now(),
    );
    assert_eq!(sb.get("a").unwrap().depth, MAX_BRANCH_DEPTH);
    assert_eq!(
        sb.get("a").unwrap().thread_order.matches('/').count(),
        MAX_BRANCH_DEPTH + 1
    );
}

#[test]
fn stamp_is_19_digit_ticks() {
    let unix0 = Utc.timestamp_opt(0, 0).unwrap();
    assert_eq!(stamp(unix0, false), "0621355968000000000");
    assert_eq!(
        stamp(unix0, true),
        format!(
            "{:019}",
            3_155_378_975_999_999_999i64 - 621_355_968_000_000_000
        )
    );
    let later = unix0 + TimeDelta::microseconds(1);
    assert!(stamp(later, false) > stamp(unix0, false));
    assert!(
        stamp(later, true) < stamp(unix0, true),
        "newest first sorts first"
    );
    assert_eq!(stamp(later, false).len(), 19);
}

// ------------------------------------------------------------- view / sort

fn opts(sort: SortMode, group: bool) -> ViewOptions {
    ViewOptions {
        filter: String::new(),
        group_by_folder: group,
        sort,
    }
}

#[test]
fn newest_threads_branches_under_parent() {
    let mut sb = threaded();
    sb.link_threads(|_| None, true, now());
    let v = sb.view(&opts(SortMode::Newest, false), now(), &utc());
    assert_eq!(ids(&v), ["grand", "other", "root", "child"]);
}

#[test]
fn oldest_threads_branches_under_parent() {
    let mut sb = threaded();
    sb.link_threads(|_| None, false, now());
    let v = sb.view(&opts(SortMode::Oldest, false), now(), &utc());
    assert_eq!(ids(&v), ["root", "child", "other", "grand"]);
}

#[test]
fn name_sort_is_alphabetical_not_threaded() {
    let mut sb = threaded();
    for (id, n) in [
        ("root", "b"),
        ("child", "Z"),
        ("grand", "a"),
        ("other", "C"),
    ] {
        sb.get_mut(id).unwrap().custom_name = Some(n.into());
    }
    sb.link_threads(|_| None, true, now());
    let v = sb.view(&opts(SortMode::Name, false), now(), &utc());
    assert_eq!(ids(&v), ["grand", "root", "other", "child"]);
    assert_eq!(v[3].depth, 1, "keeps its indent");
}

#[test]
fn buckets_come_first() {
    let mut sb = Sidebar::new();
    sb.fold(
        vec![info("today", "C:\\p", 10), info("old", "C:\\p", 60 * 48)],
        false,
        |_| None,
    );
    sb.open_pane("open", pane(1, PaneStatus::Working, "C:\\p"), "x", None);
    sb.fold(
        vec![
            info("today", "C:\\p", 10),
            info("old", "C:\\p", 60 * 48),
            info("open", "C:\\p", 60 * 72),
        ],
        false,
        |_| None,
    );
    sb.link_threads(|_| None, true, now());
    for sort in [SortMode::Newest, SortMode::Oldest, SortMode::Name] {
        let v = sb.view(&opts(sort, true), now(), &utc());
        assert_eq!(ids(&v), ["open", "today", "old"]);
    }
}

#[test]
fn group_by_folder_orders_by_folder_rank() {
    let mut sb = Sidebar::new();
    sb.fold(
        vec![
            info("a1", "C:\\alpha", 50),
            info("z1", "C:\\zeta", 10),
            info("a2", "C:\\alpha", 40),
            info("z2", "C:\\zeta", 60),
        ],
        false,
        |_| None,
    );
    sb.link_threads(|_| None, true, now());
    let v = sb.view(&opts(SortMode::Newest, true), now(), &utc());
    assert_eq!(
        ids(&v),
        ["z1", "z2", "a2", "a1"],
        "zeta's newest is newer than alpha's"
    );
    sb.link_threads(|_| None, false, now());
    let v = sb.view(&opts(SortMode::Oldest, true), now(), &utc());
    assert_eq!(
        ids(&v),
        ["a1", "a2", "z2", "z1"],
        "oldest: alpha's newest is older"
    );
}

#[test]
fn group_by_folder_tie_breaks_on_project_name() {
    let mut sb = Sidebar::new();
    sb.fold(
        vec![info("b", "C:\\Beta", 10), info("a", "C:\\alpha", 10)],
        false,
        |_| None,
    );
    sb.link_threads(|_| None, true, now());
    assert_eq!(
        ids(&sb.view(&opts(SortMode::Newest, true), now(), &utc())),
        ["a", "b"]
    );
}

#[test]
fn group_by_folder_name_sort_is_project_then_name() {
    let mut sb = Sidebar::new();
    sb.fold(
        vec![
            info("z", "C:\\zeta", 0),
            info("a2", "C:\\alpha", 60),
            info("a1", "C:\\alpha", 50),
        ],
        false,
        |_| None,
    );
    sb.get_mut("a2").unwrap().custom_name = Some("aaa".into());
    sb.get_mut("a1").unwrap().custom_name = Some("bbb".into());
    sb.link_threads(|_| None, true, now());
    assert_eq!(
        ids(&sb.view(&opts(SortMode::Name, true), now(), &utc())),
        ["a2", "a1", "z"]
    );
}

#[test]
fn folder_rank_is_case_insensitive_on_project() {
    let mut sb = Sidebar::new();
    sb.fold(
        vec![
            info("x", "C:\\Proj", 5),
            info("y", "D:\\proj", 100),
            info("q", "C:\\mid", 50),
        ],
        false,
        |_| None,
    );
    sb.link_threads(|_| None, true, now());
    let v = sb.view(&opts(SortMode::Newest, true), now(), &utc());
    assert_eq!(ids(&v), ["x", "y", "q"], "y ranks with x's newer time");
}

#[test]
fn filter_matches_name_project_or_id_prefix() {
    let mut sb = Sidebar::new();
    let mut a = info("1234-abcd", "C:\\Kilno", 0);
    a.preview = "Fix the Sidebar".into();
    sb.fold(vec![a, info("5678-efgh", "C:\\other", 0)], false, |_| None);
    sb.link_threads(|_| None, true, now());
    let f = |q: &str| {
        ids(&sb.view(
            &ViewOptions {
                filter: q.into(),
                ..Default::default()
            },
            now(),
            &utc(),
        ))
    };
    assert_eq!(
        f("  sidebar "),
        ["1234-abcd"],
        "name contains, trimmed, case-insensitive"
    );
    assert_eq!(f("KIL"), ["1234-abcd"], "project contains");
    assert_eq!(f("5678"), ["5678-efgh"], "id starts with");
    assert!(f("abcd").is_empty(), "id must start with it");
    assert_eq!(f("").len(), 2);
}

#[test]
fn default_view_options() {
    let o = ViewOptions::default();
    assert!(o.group_by_folder);
    assert_eq!(o.sort, SortMode::Newest);
}

#[test]
fn layout_signature_format() {
    let mut sb = threaded();
    sb.link_threads(|_| None, true, now());
    let v = sb.view(&opts(SortMode::Newest, true), now(), &utc());
    let sig = layout_signature(&v, true, now(), &utc());
    assert_eq!(
        sig,
        "grand|Today|p|0|\nother|Today|p|0|\nroot|Today|p|0|\nchild|Today|p|1|root"
    );
    let flat = layout_signature(&v, false, now(), &utc());
    assert!(flat.starts_with("grand|Today||0|\n"));
    // A status change does not change the signature; a new row does.
    sb.open_pane("new", pane(1, PaneStatus::Starting, "C:\\p"), "x", None);
    let v2 = sb.view(&opts(SortMode::Newest, true), now(), &utc());
    assert_ne!(layout_signature(&v2, true, now(), &utc()), sig);
}

#[test]
fn empty_state_text() {
    assert_eq!(EMPTY_STATE, "No transcripts found under ~\\.claude or ~\\.codex.\nStart a session and it will show up here.");
}

// ---------------------------------------------------------- recent folders

#[test]
fn recent_folders_current_first_then_by_activity() {
    let mut sb = Sidebar::new();
    sb.fold(
        vec![
            info("a", "C:\\old", 100),
            info("b", "C:\\new", 1),
            info("c", "c:\\NEW", 2),
            info("d", "C:\\gone", 0),
        ],
        false,
        |_| None,
    );
    let got = sb.recent_folders("C:\\here", 8, now(), |f| f != "C:\\gone");
    let paths: Vec<_> = got.iter().map(|c| c.full_path.as_str()).collect();
    assert_eq!(paths, ["C:\\here", "C:\\new", "C:\\old"]);
    assert_eq!(got[1].name, "new");
}

#[test]
fn recent_folders_max_and_current_dedup() {
    let mut sb = Sidebar::new();
    let infos: Vec<_> = (0..12)
        .map(|i| info(&format!("s{i}"), &format!("C:\\f{i}"), i))
        .collect();
    sb.fold(infos, false, |_| None);
    let got = sb.recent_folders("C:\\F0", 8, now(), |_| true);
    assert_eq!(got.len(), 8);
    assert_eq!(got[0].full_path, "C:\\F0");
    assert_eq!(got[1].full_path, "C:\\f1", "C:\\f0 is the current folder");
}

#[test]
fn recent_folders_skips_missing_current() {
    let sb = Sidebar::new();
    assert!(sb
        .recent_folders("C:\\nowhere", 8, now(), |_| false)
        .is_empty());
    assert!(sb.recent_folders("", 8, now(), |_| true).is_empty());
}

// ---------------------------------------------------------- recent sessions

#[test]
fn recent_sessions_mru_first_then_open_order() {
    let mut sb = Sidebar::new();
    let mut i = info("s1", "C:\\work\\proj", 0);
    i.preview = "one".into();
    sb.fold(vec![i], false, |_| None);
    sb.open_pane("s1", pane(1, PaneStatus::Done, "C:\\work\\proj"), "x", None);
    let open = [
        pane(1, PaneStatus::Done, "C:\\work\\proj"),
        pane(2, PaneStatus::Working, "C:\\a\\b"),
        pane(3, PaneStatus::NeedsInput, "C:\\c"),
    ];
    let got = sb.recent_sessions(&[2, 9, 2], &open, 3);
    assert_eq!(
        got.iter().map(|c| c.pane_id).collect::<Vec<_>>(),
        [2, 1, 3],
        "closed pane 9 skipped, no repeats"
    );
    assert_eq!(
        got[0],
        PickChoice {
            pane_id: 2,
            name: "new session".into(),
            folder: "b".into(),
            status: PaneStatus::Working
        }
    );
    assert_eq!(got[1].name, "one");
    assert_eq!(got[1].folder, "proj");
    assert_eq!(
        sb.recent_sessions(&[3], &open, 2)
            .iter()
            .map(|c| c.pane_id)
            .collect::<Vec<_>>(),
        [3, 1]
    );
}

// ---------------------------------------------------------- codex binding

fn t(min: i64) -> DateTime<Utc> {
    now() + TimeDelta::minutes(min)
}

fn ub(folder: &str, parent: Option<&str>, since: i64) -> UnboundPane {
    UnboundPane {
        folder: folder.into(),
        parent_id: parent.map(Into::into),
        since: t(since),
    }
}

#[test]
fn bind_window_capped_at_ten_minutes() {
    let panes = [ub("C:\\p", None, 0)];
    assert_eq!(codex_bind_window(&panes, 0), (t(0), t(10)));
}

#[test]
fn bind_window_ends_at_next_new_pane_in_same_folder() {
    let panes = [
        ub("C:\\p", None, 0),
        ub("c:\\P\\", None, 2),
        ub("C:\\other", None, 1),
    ];
    assert_eq!(
        codex_bind_window(&panes, 0),
        (t(0), t(2)),
        "other folder ignored, case/trailing slash ignored"
    );
    assert_eq!(
        codex_bind_window(&panes, 1),
        (t(2), t(12)),
        "only later panes count"
    );
}

#[test]
fn bind_window_forks_match_on_parent_any_folder() {
    let panes = [
        ub("C:\\p", Some("P1"), 0),
        ub("C:\\q", Some("P1"), 3),
        ub("C:\\p", Some("P2"), 1),
        ub("C:\\p", None, 2),
    ];
    assert_eq!(codex_bind_window(&panes, 0), (t(0), t(3)));
    assert_eq!(codex_bind_window(&panes, 2), (t(1), t(11)));
}

#[test]
fn bind_window_new_session_ignores_forks_in_same_folder() {
    let panes = [ub("C:\\p", None, 0), ub("C:\\p", Some("P1"), 1)];
    assert_eq!(codex_bind_window(&panes, 0), (t(0), t(10)));
}

#[test]
fn bind_window_parent_compare_is_case_sensitive() {
    let panes = [ub("C:\\p", Some("abc"), 0), ub("C:\\p", Some("ABC"), 1)];
    assert_eq!(codex_bind_window(&panes, 0), (t(0), t(10)));
}

// --------------------------------------------------------------- hiding

#[test]
fn hiding_a_session_hides_its_branches_but_not_an_open_one() {
    let mut sb = threaded();
    sb.link_threads(|id| (id == "grand").then(|| "child".to_string()), true, now());
    sb.mark_hidden(|id| id == "root");
    for id in ["root", "child", "grand"] {
        assert!(sb.get(id).unwrap().hidden, "{id}: hidden with its parent");
        assert_eq!(sb.get(id).unwrap().bucket(now(), &utc()), Bucket::Hidden);
    }
    assert!(!sb.get("other").unwrap().hidden, "an unrelated session stays");

    let rows = sb.view(&ViewOptions::default(), now(), &utc());
    assert_eq!(rows.last().unwrap().bucket(now(), &utc()), Bucket::Hidden, "the last section");
    assert!(!Bucket::Hidden.expanded_by_default(), "folded away");

    // Unhiding a branch needs its hidden parents too.
    assert_eq!(sb.lineage("grand"), ["grand", "child", "root"]);
    assert_eq!(sb.lineage("unknown"), ["unknown"]);

    sb.mark_hidden(|_| false);
    assert!(sb.rows().all(|r| !r.hidden), "unhidden");
}

#[test]
fn an_open_session_is_never_in_hidden() {
    let mut r = row_with(info("x", "C:\\p", 5));
    r.hidden = true;
    r.pane = Some(pane(1, PaneStatus::Working, "C:\\p"));
    assert_eq!(r.bucket(now(), &utc()), Bucket::Open);
}

// --------------------------------------------------------------- dragged order

#[test]
fn custom_sort_follows_dragged_ranks_and_keeps_threads() {
    let mut sb = threaded();
    sb.link_threads(|id| (id == "grand").then(|| "child".to_string()), true, now());
    // "other" dragged above the "root" thread; nothing else ranked.
    sb.mark_order(|id| match id {
        "other" => Some(0),
        "root" => Some(1),
        _ => None,
    });
    assert_eq!(sb.get("grand").unwrap().rank, Some(1), "a branch takes its session's rank");
    assert_eq!(sb.root_of("grand"), "root");
    let opts = ViewOptions { sort: SortMode::Custom, group_by_folder: false, ..ViewOptions::default() };
    let rows = sb.view(&opts, now(), &utc());
    let earlier: Vec<String> = rows.iter().filter(|r| r.bucket(now(), &utc()) == Bucket::Today).map(|r| r.session_id.clone()).collect();
    assert_eq!(earlier, ["other", "root", "child", "grand"], "dragged order, threads intact");
}
