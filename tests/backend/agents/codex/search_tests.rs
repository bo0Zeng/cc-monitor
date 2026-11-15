//! Codex 的会话经全文搜索 · 会话内查找走一遍（注册表 Codex 那一格的文本面 ＋ 搜索走合成历史面的根）。
//! 夹具是结构占位（0.159 形状，`codex-rollout.turns.jsonl`），正文都是编的。
use crate::agents::codex::AGENT_KIND;
use crate::observe::search_query::test_support::{lines_into, opts_from_words};
use crate::observe::search_query::{scan_session_find, who_of, FindPage, SearchIndex, Trees};
use serde_json::Value;
use std::path::{Path, PathBuf};

const TURNS: &str = include_str!("../../../__fixtures__/codex-rollout.turns.jsonl");
const SID: &str = "0000beef-0000-4000-8000-0000000000d3";

/// 沙箱：一个没有记录树的家 ＋ 一个 Codex 会话根，里面一份 rollout。回（家，会话根，那一份）。
fn codex_world(tag: &str, body: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = test_home::scratch(tag);
    let home = root.join("claude");
    std::fs::create_dir_all(&home).unwrap();
    let sessions = root.join("codex/sessions");
    let day = sessions.join("2026/10/20");
    std::fs::create_dir_all(&day).unwrap();
    let file = day.join(format!("rollout-2026-10-20T00-00-00-{SID}.jsonl"));
    std::fs::write(&file, body).unwrap();
    (home, sessions, file)
}

/// 全文搜索那一问（`q` 已小写），走的是没有记录树的家 ＋ 那一个 Codex 会话根。
fn search(home: &Path, sessions: &Path, q: &str, words: &[&str]) -> Vec<Value> {
    let trees =
        Trees::among(home, &[(AGENT_KIND, sessions.to_path_buf())]).expect("有记录树那一家");
    let words: Vec<String> = words.iter().map(|w| w.to_string()).collect();
    let mut buf = Vec::new();
    SearchIndex::default()
        .search_in(
            &trees,
            q,
            &opts_from_words(&words),
            &crate::history::live_table::now(home),
            &mut lines_into(&mut buf),
        )
        .expect("search ok");
    String::from_utf8(buf)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn ms(iso: &str) -> i64 {
    crate::common::time::parse_iso8601_ms(iso).unwrap()
}

/// 搜一个只在 Codex 会话里出现的词 ⇒ 那一行是这一家的这条会话，命中落在人说的那一句上；工具输出搜得到、落在那次调用里。
#[test]
fn a_word_only_a_codex_session_says_is_found_with_its_record_and_time() {
    let (home, sessions, file) = codex_world("codex-search", TURNS);

    let rows = search(&home, &sessions, "count them", &[]);
    assert_eq!(rows.len(), 1, "{rows:?}");
    let row = &rows[0];
    assert_eq!(row["agent"], AGENT_KIND);
    assert_eq!(row["sessionId"], SID);
    assert_eq!(row["projectPath"], "/work/demo");
    assert_eq!(row["jsonlPath"], file.to_string_lossy().as_ref());
    let hit = &row["hits"][0];
    assert_eq!(hit["uuid"], "m-u2");
    assert_eq!(hit["who"], who_of(AGENT_KIND, "user"));

    let rows = search(&home, &sessions, "b.txt", &["--include-tools"]);
    let hit = &rows[0]["hits"][0];
    assert_eq!(
        (&hit["uuid"], &hit["who"], &hit["tool"]),
        (
            &Value::from("fo-1"),
            &Value::from(who_of(AGENT_KIND, "tool")),
            &Value::from("call-1")
        )
    );
    let _ = std::fs::remove_dir_all(home.parent().unwrap());
}

/// 会话内查找：命中带第几轮（人说的第二句 ⇒ 第 2 轮）与时刻。
#[test]
fn finding_in_a_codex_session_gives_the_turn_and_time() {
    let mut hits = Vec::new();
    scan_session_find(
        AGENT_KIND,
        std::io::Cursor::new(TURNS.as_bytes()),
        "count them",
        false,
        None,
        FindPage { skip: 0, limit: 50 },
        |h| {
            hits.push(h.clone());
            Ok(())
        },
    )
    .expect("扫得动");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].uuid, "m-u2");
    assert_eq!(hits[0].turn, 2);
    assert_eq!(hits[0].ts_ms, ms("2026-10-20T00:01:00.000Z"));
}

/// 记录树与 Codex 的根一趟搜：两家的会话各一行，照最近优先排（Codex 那一份更新 ⇒ 在前）。
#[test]
fn the_record_tree_and_the_codex_root_are_searched_in_one_pass() {
    let (home, sessions, codex_file) = codex_world("codex-search-both", TURNS);
    let proj = home.join("projects/-w");
    std::fs::create_dir_all(&proj).unwrap();
    let claude_file = proj.join("c0ffee00-0000-4000-8000-000000000001.jsonl");
    std::fs::write(
        &claude_file,
        "{\"type\":\"user\",\"uuid\":\"c-1\",\"timestamp\":\"2026-10-19T00:00:00.000Z\",\
         \"message\":{\"role\":\"user\",\"content\":\"please count them\"}}\n",
    )
    .unwrap();
    let at = |ms: u64| std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms);
    let set = |p: &Path, ms: u64| {
        std::fs::File::options()
            .append(true)
            .open(p)
            .unwrap()
            .set_modified(at(ms))
            .unwrap()
    };
    set(&claude_file, 1_800_000_000_000);
    set(&codex_file, 1_800_000_100_000);

    let rows = search(&home, &sessions, "count them", &[]);
    let got: Vec<(&str, &str)> = rows
        .iter()
        .map(|r| {
            (
                r["sessionId"].as_str().unwrap(),
                r["hits"][0]["uuid"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            (SID, "m-u2"),
            ("c0ffee00-0000-4000-8000-000000000001", "c-1")
        ]
    );
    let _ = std::fs::remove_dir_all(home.parent().unwrap());
}
