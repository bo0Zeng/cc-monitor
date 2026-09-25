//! 〔AS1 · 第四波 4B〕`mcp_sync.rs`（monitor 这一侧的推 / 拉编排）的判据。
//!
//! 门用 `user_files` 的替身 [`DiskDoor`]（临时目录上「读 · CAS · 写」，两台机器各一扇），
//! 判定用替身 [`Canned`]（记下交过去的入参、回事先摆好的答案）—— 判定规则本身在后端判
//! （`tests/backend/mcp_sync_tests.rs`），这里只判**编排**：读的是哪两份、交给对面判的是什么、
//! 写过去的是什么、CAS 期望是哪一份、`stale` 之后有没有擅自重来。
//!
//! # 买不到
//!
//! - 🔴 真远端 / 真 Windows / 真后端：`BackendJudge` / `BackendDoor` 那一跳只在「没通道 ⇒ 说得出是哪台」上量过。

use super::*;
use crate::user_files::tests::{temp_home, DiskDoor};
use std::cell::RefCell;
use std::path::Path;

fn run<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

/// 替身判定：记下每一次交过去的入参，回一个事先摆好的答案（或拒）。
struct Canned {
    answer: Result<Value, String>,
    seen: RefCell<Vec<Value>>,
}

impl Canned {
    fn ok(v: Value) -> Self {
        Canned {
            answer: Ok(v),
            seen: RefCell::new(Vec::new()),
        }
    }
}

impl Judge for Canned {
    async fn plan(&self, args: Value) -> Result<Value, String> {
        self.seen.borrow_mut().push(args);
        self.answer.clone()
    }
}

fn target_in(dir: &Path) -> String {
    dir.join(".mcp.json").display().to_string()
}

/// 这边那份。
const SOURCE: &str = r#"{
  "mcpServers": {
    "zeta": { "command": "/opt/zeta", "args": ["--root=/home/u/data"] },
    "alpha": { "type": "http", "url": "https://example.invalid/mcp" }
  }
}"#;

#[test]
fn preview_reads_both_sides_through_their_own_doors_and_hands_both_texts_to_the_judge() {
    let (a, b) = (temp_home("as1-pv-a"), temp_home("as1-pv-b"));
    std::fs::write(a.join(".mcp.json"), SOURCE).unwrap();
    let (from, to) = (DiskDoor::new(&a), DiskDoor::new(&b));
    let judge = Canned::ok(json!({
        "rows": [
            {"name": "alpha", "state": "new", "suspects": []},
            {"name": "zeta", "state": "new", "suspects": [
                {"kind": "abs-path", "field": "command", "value": "/opt/zeta", "there": "absent"},
                {"kind": "command-relative", "field": "command", "value": "./x", "there": null},
            ]},
        ],
        "write": null,
    }));
    let p = run(preview_with(
        &from,
        &target_in(&a),
        &to,
        &target_in(&b),
        &judge,
    ))
    .unwrap();
    // 交给对面判的恰好是两份原文（对面不存在 ⇒ `null`，不是空串）。
    assert_eq!(
        judge.seen.borrow().as_slice(),
        &[json!({ "source": SOURCE, "target": null })]
    );
    assert_eq!(p.source_text, SOURCE);
    assert_eq!(p.target_text, None);
    assert_eq!(p.rows.len(), 2);
    let zeta = &p.rows[1];
    assert_eq!((zeta.name.as_str(), zeta.state.as_str()), ("zeta", "new"));
    assert_eq!(zeta.target, None);
    // 值取自原文（与对面那份一起给人看；键序不保，见模块头注）。
    let src: Value = serde_json::from_str(SOURCE).unwrap();
    assert_eq!(zeta.source.as_ref(), Some(&src["mcpServers"]["zeta"]));
    assert_eq!(
        zeta.suspects,
        vec![
            McpSyncSuspect {
                kind: "abs-path".into(),
                field: "command".into(),
                value: "/opt/zeta".into(),
                there: Some("absent".into()),
            },
            McpSyncSuspect {
                kind: "command-relative".into(),
                field: "command".into(),
                value: "./x".into(),
                there: None,
            },
        ]
    );
    // 一个字节都没写（两台都是）。
    assert!(from.puts.borrow().is_empty() && to.puts.borrow().is_empty());
}

#[test]
fn a_missing_source_file_is_said_not_pushed_as_nothing() {
    let (a, b) = (temp_home("as1-miss-a"), temp_home("as1-miss-b"));
    let judge = Canned::ok(json!({ "rows": [], "write": null }));
    let err = run(preview_with(
        &DiskDoor::new(&a),
        &target_in(&a),
        &DiskDoor::new(&b),
        &target_in(&b),
        &judge,
    ))
    .unwrap_err();
    assert!(err.contains("没有可以拷过去的条目"), "实得：{err}");
    assert!(judge.seen.borrow().is_empty(), "这边没有原文还去请对面判");
}

#[test]
fn apply_writes_exactly_the_judged_names_verbatim_with_the_preview_text_as_expectation() {
    let (b,) = (temp_home("as1-apply"),);
    let target =
        "{\n  \"keep\": true,\n  \"mcpServers\": {\n    \"theirs\": { \"command\": \"x\" }\n  }\n}";
    std::fs::write(b.join(".mcp.json"), target).unwrap();
    let to = DiskDoor::new(&b);
    // 判定说只写 zeta（alpha 被用户勾了但判定没放行的那种由后端管，这里只照单写）。
    let judge = Canned::ok(json!({ "rows": [], "write": ["zeta"] }));
    let done = run(apply_with(
        &to,
        &target_in(&b),
        &judge,
        SOURCE,
        Some(target),
        &["zeta".to_string(), "alpha".to_string()],
        &[],
    ))
    .unwrap();
    assert!(done.written);
    assert_eq!(done.names, vec!["zeta".to_string()]);
    // 交给判定的是看差异时那两份 ＋ 用户的两张单子。
    assert_eq!(
        judge.seen.borrow().as_slice(),
        &[
            json!({ "source": SOURCE, "target": target, "take": ["zeta", "alpha"], "overwrite": [] })
        ]
    );
    let puts = to.puts.borrow();
    assert_eq!(puts.len(), 1);
    assert_eq!(
        puts[0].expect.as_deref(),
        Some(target),
        "CAS 期望必须是看差异时读到的那一份"
    );
    assert!(!puts[0].backup && !puts[0].parents);
    let written: Value = serde_json::from_str(&puts[0].content).unwrap();
    let src: Value = serde_json::from_str(SOURCE).unwrap();
    assert_eq!(written["keep"], json!(true), "对面根上别的键不动");
    assert_eq!(
        written["mcpServers"]["theirs"],
        json!({ "command": "x" }),
        "对面别的条目不动"
    );
    assert_eq!(
        written["mcpServers"]["zeta"], src["mcpServers"]["zeta"],
        "值原样"
    );
    assert!(
        written["mcpServers"].get("alpha").is_none(),
        "判定没放行的不写"
    );
}

#[test]
fn a_target_changed_after_the_preview_stops_the_write_and_is_not_retried() {
    let b = temp_home("as1-stale");
    std::fs::write(b.join(".mcp.json"), "{}").unwrap();
    let to = DiskDoor::new(&b);
    to.interfere
        .borrow_mut()
        .push("{\"mcpServers\":{\"someone\":{}}}".into());
    let judge = Canned::ok(json!({ "rows": [], "write": ["zeta"] }));
    let err = run(apply_with(
        &to,
        &target_in(&b),
        &judge,
        SOURCE,
        Some("{}"),
        &["zeta".to_string()],
        &[],
    ))
    .unwrap_err();
    assert!(err.contains("重新看一次差异"), "实得：{err}");
    assert_eq!(to.puts.borrow().len(), 1, "stale 之后擅自重读重算了");
    assert_eq!(
        std::fs::read_to_string(b.join(".mcp.json")).unwrap(),
        "{\"mcpServers\":{\"someone\":{}}}",
        "别人那次改动被盖掉了"
    );
}

#[test]
fn nothing_judged_writable_means_nothing_written_and_a_refusal_reaches_the_user() {
    let b = temp_home("as1-none");
    let to = DiskDoor::new(&b);
    let judge = Canned::ok(json!({ "rows": [], "write": [] }));
    let done = run(apply_with(
        &to,
        &target_in(&b),
        &judge,
        SOURCE,
        None,
        &[],
        &[],
    ))
    .unwrap();
    assert!(!done.written && done.names.is_empty());
    let refusing = Canned {
        answer: Err("「zeta」两边不一样，还没说要盖掉对面那一条".into()),
        seen: RefCell::new(Vec::new()),
    };
    let err = run(apply_with(
        &to,
        &target_in(&b),
        &refusing,
        SOURCE,
        None,
        &["zeta".to_string()],
        &[],
    ))
    .unwrap_err();
    assert!(err.contains("还没说要盖"), "实得：{err}");
    assert!(to.puts.borrow().is_empty());
    assert!(!b.join(".mcp.json").exists(), "拒了还建了文件");
}

#[test]
fn a_reply_that_breaks_the_contract_is_an_error_not_a_guess() {
    let (a, b) = (temp_home("as1-bad-a"), temp_home("as1-bad-b"));
    std::fs::write(a.join(".mcp.json"), SOURCE).unwrap();
    for bad in [
        json!({}),
        json!({ "rows": [{ "name": "x" }] }),
        json!({ "rows": [{ "name": "x", "state": "new", "suspects": [{ "kind": "abs-path" }] }] }),
    ] {
        let err = run(preview_with(
            &DiskDoor::new(&a),
            &target_in(&a),
            &DiskDoor::new(&b),
            &target_in(&b),
            &Canned::ok(bad.clone()),
        ))
        .unwrap_err();
        assert!(err.contains("两端契约对不上"), "{bad} ⇒ {err}");
    }
    let err = run(apply_with(
        &DiskDoor::new(&b),
        &target_in(&b),
        &Canned::ok(json!({ "write": null })),
        SOURCE,
        None,
        &[],
        &[],
    ))
    .unwrap_err();
    assert!(err.contains("两端契约对不上"), "实得：{err}");
}

/// 🔴 头注「判定一处，住后端 · 这里一条规则都不写」：本模块生产段里**后端那几个闭集的线上名零命中**
/// （拿名字判 = 在这边长出第二份判定）。人群从后端源码现抠（异源：规则住 `src/backend/mcp_sync.rs`）；
/// 正控：塞一处 `== "differs"` 数得到 1。
#[test]
fn this_module_holds_no_sync_rule() {
    let backend = include_str!("../../src/backend/mcp_sync.rs");
    let consts = ["STATES", "SUSPECT_KINDS", "THERE"];
    let mut names: Vec<String> = Vec::new();
    for c in consts {
        let at = backend
            .find(&format!("pub(crate) const {c}: &[&str] = &["))
            .unwrap_or_else(|| panic!("后端没有 {c}"));
        let rest = &backend[at..];
        let body = &rest[rest.find("= &[").unwrap() + 4..rest.find("];").unwrap()];
        names.extend(
            body.split(',')
                .map(|s| s.trim().trim_matches('"').to_string())
                .filter(|s| !s.is_empty()),
        );
    }
    assert_eq!(
        names.len(),
        11,
        "闭集抽出来的条数不对 —— 抽取器坏了：{names:?}"
    );
    // 属性行不算（`ts(type = "unknown")` 是生成物的 TS 类型名，与闭集里的 `unknown` 同形不同义）。
    let count = |src: &str| {
        let prod = guard_core::production_code(src);
        prod.lines()
            .filter(|l| !l.trim_start().starts_with("#["))
            .map(|l| {
                names
                    .iter()
                    .map(|n| l.matches(&format!("\"{n}\"")).count())
                    .sum::<usize>()
            })
            .sum::<usize>()
    };
    let src = include_str!("../../src/bridge/src/mcp_sync.rs");
    assert_eq!(
        count(src),
        0,
        "monitor 这一侧出现了闭集里的线上名 —— 判定长出了第二个家"
    );
    let planted = src.replacen(
        "let names: Vec<String> = data",
        "let _ = data.get(\"state\") == Some(&json!(\"differs\"));\n    let names: Vec<String> = data",
        1,
    );
    assert_ne!(planted, src, "正控的锚没打中");
    assert_eq!(count(&planted), 1);
}

#[test]
fn the_command_and_its_fields_are_the_ones_the_backend_registers() {
    let src = include_str!("../../src/backend/inbound.rs");
    let needle = format!("name: \"{CMD}\",");
    assert_eq!(src.matches(&needle).count(), 1, "后端没有（或有两条）{CMD}");
    let rest = &src[src.find(&needle).unwrap()..];
    let f = rest.find("fields: &[").unwrap() + "fields: &[".len();
    let end = rest[f..].find(']').unwrap() + f;
    let mut fields: Vec<&str> = rest[f..end]
        .split(',')
        .map(|s| s.trim().trim_matches('"'))
        .filter(|s| !s.is_empty())
        .collect();
    fields.sort();
    // 本侧发的四格（source · target · take · overwrite）＋ 读的两格（rows · write），两向相等。
    let mut ours = vec!["source", "target", "take", "overwrite", "rows", "write"];
    ours.sort();
    assert_eq!(fields, ours);
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/mcp_sync.rs"));
    for f in &ours {
        assert!(
            prod.contains(&format!("\"{f}\"")),
            "本侧没用到 `{f}` —— 登记表与本侧对不上"
        );
    }
}

#[test]
fn a_machine_without_a_channel_is_named_and_nothing_moves() {
    let err = run(mcp_sync_preview(
        Origin("as1-no-such-host".to_string()),
        "/srv/p".to_string(),
        Origin::local(),
        "/srv/q".to_string(),
    ))
    .unwrap_err();
    assert!(err.contains("as1-no-such-host"), "实得：{err}");
    let err = run(mcp_sync_preview(
        Origin::local(),
        "/srv/p".to_string(),
        Origin::local(),
        "/srv/p/".to_string(),
    ))
    .unwrap_err();
    // 同一台同一个目录（尾斜杠不算不同）⇒ 当场拒，不去读。
    assert!(err.contains("同一份文件"), "实得：{err}");
}
