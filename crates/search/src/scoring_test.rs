//! Tests for scoring module

use super::scoring::{self, NodeContext, name_score};

// -- name_score unit tests ---------------------------------------------------

#[test]
fn exact_match_highest_name_score() {
    assert_eq!(
        name_score("config.yaml", "config.yaml"),
        1000 + 100u32.saturating_sub(11)
    );
}

#[test]
fn prefix_beats_substring() {
    let prefix = name_score("config.yaml", "config");
    let substring = name_score("myconfig.yaml", "config");
    assert!(
        prefix > substring,
        "prefix {prefix} should beat substring {substring}"
    );
}

// -- recency -----------------------------------------------------------------

#[test]
fn recency_today_vs_old() {
    let now = scoring::now_secs();
    let today = NodeContext::new("project/config.yaml", now);
    let old = NodeContext::new("project/config.yaml", now - 365 * 86400);

    let score_today = scoring::score_result(&today, "config.yaml", now);
    let score_old = scoring::score_result(&old, "config.yaml", now);
    assert!(
        score_today > score_old,
        "today {score_today} > old {score_old}"
    );
}

// -- depth -------------------------------------------------------------------

#[test]
fn depth_shallow_wins() {
    let now = scoring::now_secs();
    let shallow = NodeContext::new("docs/config.yaml", now);
    let deep = NodeContext::new("a/b/c/d/e/f/g/h/config.yaml", now);

    let s = scoring::score_result(&shallow, "config.yaml", now);
    let d = scoring::score_result(&deep, "config.yaml", now);
    assert!(s > d, "shallow {s} > deep {d}");
}

#[test]
fn deep_nesting_penalty_progressive() {
    let now = scoring::now_secs();
    let d3 = NodeContext::new("a/b/c/config.yaml", now);
    let d6 = NodeContext::new("a/b/c/d/e/f/config.yaml", now);
    let d12 = NodeContext::new("a/b/c/d/e/f/g/h/i/j/k/l/config.yaml", now);

    let s3 = scoring::score_result(&d3, "config.yaml", now);
    let s6 = scoring::score_result(&d6, "config.yaml", now);
    let s12 = scoring::score_result(&d12, "config.yaml", now);
    assert!(s3 > s6, "depth3 {s3} > depth6 {s6}");
    assert!(s6 > s12, "depth6 {s6} > depth12 {s12}");
}

// -- context penalties -------------------------------------------------------

#[test]
fn hidden_dot_directory_penalty() {
    let now = scoring::now_secs();
    let visible = NodeContext::new("project/config.yaml", now);
    let hidden = NodeContext::new(".config/project/config.yaml", now);

    let sv = scoring::score_result(&visible, "config.yaml", now);
    let sh = scoring::score_result(&hidden, "config.yaml", now);
    assert!(sv > sh, "visible {sv} > hidden {sh}");
}

#[test]
fn noise_directory_penalty() {
    let now = scoring::now_secs();
    let normal = NodeContext::new("project/src/config.yaml", now);
    let noise = NodeContext::new("project/node_modules/pkg/config.yaml", now);

    let sn = scoring::score_result(&normal, "config.yaml", now);
    let sno = scoring::score_result(&noise, "config.yaml", now);
    assert!(sn > sno, "normal {sn} > noise {sno}");
}

#[test]
fn library_path_penalty() {
    let now = scoring::now_secs();
    let normal = NodeContext::new("project/config.yaml", now);
    let library = NodeContext::new("/Library/Caches/config.yaml", now);

    let sn = scoring::score_result(&normal, "config.yaml", now);
    let sl = scoring::score_result(&library, "config.yaml", now);
    assert!(sn > sl, "normal {sn} > library {sl}");
}

#[test]
fn system_path_penalty() {
    let now = scoring::now_secs();
    let normal = NodeContext::new("project/config.yaml", now);
    let system = NodeContext::new("/usr/local/etc/config.yaml", now);

    let sn = scoring::score_result(&normal, "config.yaml", now);
    let ss = scoring::score_result(&system, "config.yaml", now);
    assert!(sn > ss, "normal {sn} > system {ss}");
}

// -- trash -------------------------------------------------------------------

#[test]
fn trash_detected_and_penalized() {
    let ctx = NodeContext::new("/Users/kingkong/.Trash/usage.csv", 0);
    assert!(ctx.is_trash);
    assert!(ctx.has_hidden_component);
}

#[test]
fn trash_always_ranks_last() {
    let now = scoring::now_secs();
    // Trash file modified today vs normal file modified a year ago
    let trash = NodeContext::new("/Users/kingkong/.Trash/report.pdf", now);
    let old = NodeContext::new("archive/old/deep/nested/report.pdf", now - 365 * 86400);

    let st = scoring::score_result(&trash, "report.pdf", now);
    let so = scoring::score_result(&old, "report.pdf", now);
    assert!(so > st, "old normal {so} > fresh trash {st}");
}

// -- user directory proximity ------------------------------------------------

#[test]
fn user_dir_proximity_detected() {
    let ctx = NodeContext::new("/Users/kingkong/Documents/report.pdf", 0);
    assert!(ctx.is_user_dir);

    let ctx2 = NodeContext::new("/Users/kingkong/Desktop/report.pdf", 0);
    assert!(ctx2.is_user_dir);
}

#[test]
fn user_dir_not_detected_for_code() {
    let ctx = NodeContext::new("/Users/kingkong/code/project/report.pdf", 0);
    assert!(!ctx.is_user_dir);
}

#[test]
fn user_dir_beats_deep_code_path() {
    let now = scoring::now_secs();
    let user = NodeContext::new("/Users/kingkong/Documents/usage.csv", now);
    let deep = NodeContext::new(
        "/Users/kingkong/code/tell/posthog/hogql/experiments/test/data/usage.csv",
        now,
    );

    let su = scoring::score_result(&user, "usage.csv", now);
    let sd = scoring::score_result(&deep, "usage.csv", now);
    assert!(su > sd, "~/Documents {su} > deep code path {sd}");
}

// -- realistic same-name ranking (the screenshot scenario) -------------------

#[test]
fn same_name_ranking_realistic() {
    let now = scoring::now_secs();

    // Scenario: searching "usage.csv" — 4 files, all exact name match, same mtime
    let docs = NodeContext::new("/Users/kingkong/Documents/usage.csv", now);
    let shallow_code = NodeContext::new("/Users/kingkong/code/myapp/usage.csv", now);
    let deep_code = NodeContext::new(
        "/Users/kingkong/code/tell/posthog/hogql/experiments/test/experiment_query_runner/data/usage.csv",
        now,
    );
    let trash = NodeContext::new("/Users/kingkong/.Trash/usage.csv", now);

    let s_docs = scoring::score_result(&docs, "usage.csv", now);
    let s_shallow = scoring::score_result(&shallow_code, "usage.csv", now);
    let s_deep = scoring::score_result(&deep_code, "usage.csv", now);
    let s_trash = scoring::score_result(&trash, "usage.csv", now);

    // Expected ranking: ~/Documents > shallow code > deep code > Trash
    assert!(
        s_docs > s_shallow,
        "Documents {s_docs} > shallow code {s_shallow}"
    );
    assert!(
        s_shallow > s_deep,
        "shallow code {s_shallow} > deep code {s_deep}"
    );
    assert!(s_deep > s_trash, "deep code {s_deep} > trash {s_trash}");
}

#[test]
fn same_name_cargo_registry_vs_user_project() {
    let now = scoring::now_secs();
    let user_proj = NodeContext::new("Documents/report.pdf", now);
    let cargo_reg = NodeContext::new(".cargo/registry/src/crate/report.pdf", now - 365 * 86400);

    let su = scoring::score_result(&user_proj, "report.pdf", now);
    let sc = scoring::score_result(&cargo_reg, "report.pdf", now);
    assert!(su > sc, "user project {su} > cargo registry {sc}");
}

// -- NodeContext field correctness -------------------------------------------

#[test]
fn node_context_depth_count() {
    let ctx = NodeContext::new("/Users/kingkong/code/project/src/main.rs", 0);
    assert_eq!(ctx.depth, 6);
    assert_eq!(ctx.name, "main.rs");
}

#[test]
fn node_context_bare_filename() {
    let ctx = NodeContext::new("readme.md", 0);
    assert_eq!(ctx.depth, 1);
    assert_eq!(ctx.name, "readme.md");
    assert!(!ctx.has_hidden_component);
    assert!(!ctx.has_noise_component);
}

#[test]
fn noise_case_insensitive() {
    let ctx = NodeContext::new("project/Node_Modules/pkg/index.js", 0);
    assert!(ctx.has_noise_component);
}
