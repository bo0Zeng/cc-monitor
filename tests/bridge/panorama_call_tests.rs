//! 〔RM1c · 第四波〕`panorama_call.rs` 的判据。

use super::*;

/// 后端适配层里一个 `const NAME: u64 = <数>;` 的值（运行时读，异源）。
fn backend_const(src: &str, name: &str) -> u64 {
    let key = format!("const {name}: u64 = ");
    let at = src
        .find(&key)
        .unwrap_or_else(|| panic!("后端适配层里找不到 `{key}` —— 改了写法，本条跟着改"));
    let rest = &src[at + key.len()..];
    rest[..rest.find(';').expect("常量没收尾")]
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("`{name}` 不是一个数：{e}"))
}

/// ★ 两档期限**长于**后端给子进程的期限（加上那一次探测），而且「哪几个 op 是建索引那一档」两边相等。
///
/// 异源：一侧是本文件的常量，另一侧运行时读 `src/backend/control/panorama.rs`（不走 `include_str!`：
/// 那会给 `cross_half_edge_registry` 添一条跨半边 —— 同 `panorama_locus_guard` 的取舍）。
#[test]
fn the_budgets_outlast_the_backend_deadlines() {
    let p = crate::guard_support::repo_src_root().join("backend/control/panorama.rs");
    let src = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}：{e}"));
    assert!(src.len() > 3_000, "{p:?} 只有 {} 字节", src.len());
    let prod = guard_core::production_code(&src);
    let probe = backend_const(&prod, "PROBE_DEADLINE_SECS");
    let build = backend_const(&prod, "BUILD_DEADLINE_SECS");
    let query = backend_const(&prod, "QUERY_DEADLINE_SECS");
    assert!(
        budget_for("index").as_secs() > probe + build,
        "建索引那一档：这边等 {} s，后端探测 {probe} s ＋ 子进程 {build} s —— 这边会先超时，\
         人听到的是「没回来」而不是后端那句 `timed_out`",
        budget_for("index").as_secs()
    );
    assert!(
        budget_for("overview").as_secs() > probe + query,
        "查询那一档：这边等 {} s，后端探测 {probe} s ＋ 子进程 {query} s",
        budget_for("overview").as_secs()
    );
    // 建索引那一档的 op：后端 OPS 表里期限写成 `BUILD_DEADLINE_SECS` 的那几行。
    let at = prod
        .find("const OPS: &[(&str, u64)] = &[")
        .expect("后端 op 表改了写法");
    let body = &prod[at..at + prod[at..].find("];").unwrap()];
    let mut theirs: Vec<&str> = body
        .lines()
        .filter(|l| l.contains("BUILD_DEADLINE_SECS"))
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("(\"")?;
            Some(&rest[..rest.find('"')?])
        })
        .collect();
    theirs.sort();
    let mut ours = BUILD_OPS.to_vec();
    ours.sort();
    assert!(
        !theirs.is_empty(),
        "后端那一侧一个建索引的 op 都没抽到 —— 抽取坏了"
    );
    assert_eq!(ours, theirs, "「哪几个 op 是建索引那一档」两边对不上");
}

#[test]
fn the_frame_payload_carries_only_what_was_given() {
    assert_eq!(
        frame_args("diagram_kinds", None, None),
        json!({"op": "diagram_kinds"})
    );
    assert_eq!(
        frame_args("node", Some("/r"), Some(json!({"symbol": "a#f"}))),
        json!({"op": "node", "repo": "/r", "args": {"symbol": "a#f"}})
    );
}

// ── 〔RM1d〕写：问 · 交（`edit_via`）────────────────────────────────────────────────

use crate::user_files::tests::{temp_home, DiskDoor, PutCall};
use std::cell::RefCell;

/// 计划的替身：按顺序一份一份交（每问一次弹一份），记下被问了几次。
struct Plans {
    queue: RefCell<Vec<Value>>,
    asked: RefCell<usize>,
}
impl Plans {
    fn new(mut plans: Vec<Value>) -> Self {
        plans.reverse();
        Plans {
            queue: RefCell::new(plans),
            asked: RefCell::new(0),
        }
    }
    fn next(&self) -> impl std::future::Future<Output = Result<Value, String>> {
        *self.asked.borrow_mut() += 1;
        let p = self.queue.borrow_mut().pop().expect("计划被多问了一次");
        std::future::ready(Ok(p))
    }
}

fn plan(
    value: Value,
    rel: &str,
    before: Option<&str>,
    after: Option<&str>,
    parents: bool,
) -> Value {
    json!({"value": value, "edit": {"rel": rel, "before": before, "after": after, "parents": parents}})
}

/// ★ 计划原样交给写口：`after` 是全文、`expect` 是计划里的 `before`、`parents` 原样、不要备份；
/// 写口那一侧落下的就是计划里的全文。
#[test]
fn the_plan_is_handed_to_the_door_verbatim() {
    let h = temp_home("pc-put");
    let repo = h.display().to_string();
    let door = DiskDoor::new(&h);
    let rel = ".codepicture/annotations/abc.json";
    let plans = Plans::new(vec![plan(
        json!("abc"),
        rel,
        None,
        Some("{\"id\":\"abc\"}"),
        true,
    )]);
    let got = futures::executor::block_on(edit_via(&door, &repo, || plans.next())).unwrap();
    assert_eq!(got, json!("abc"), "回的是计划里的 value");
    assert_eq!(
        door.puts.borrow().as_slice(),
        &[PutCall {
            rel: rel.to_string(),
            content: "{\"id\":\"abc\"}".to_string(),
            expect: None,
            backup: false,
            parents: true,
        }]
    );
    assert_eq!(
        std::fs::read_to_string(h.join(rel)).unwrap(),
        "{\"id\":\"abc\"}"
    );
    std::fs::remove_dir_all(&h).ok();
}

/// ★ 没有要写的（`edit = null`）⇒ 一次写都不发。
#[test]
fn nothing_to_write_sends_nothing() {
    let h = temp_home("pc-noop");
    let door = DiskDoor::new(&h);
    let plans = Plans::new(vec![json!({"value": false, "edit": null})]);
    let got =
        futures::executor::block_on(edit_via(&door, &h.display().to_string(), || plans.next()))
            .unwrap();
    assert_eq!(got, json!(false));
    assert!(door.puts.borrow().is_empty() && door.deleted.borrow().is_empty());
    std::fs::remove_dir_all(&h).ok();
}

/// ★ `stale`（算完之后盘上那份被别人改了）⇒ **重新要一份计划**，不拿旧计划硬写；趟数有上限。
#[test]
fn stale_means_plan_again_not_write_anyway() {
    let h = temp_home("pc-stale");
    let repo = h.display().to_string();
    std::fs::write(h.join("d.md"), "v1").unwrap();
    let door = DiskDoor::new(&h);
    door.interfere.borrow_mut().push("v2".to_string());
    let plans = Plans::new(vec![
        plan(json!(null), "d.md", Some("v1"), Some("v1+link"), false),
        plan(json!(null), "d.md", Some("v2"), Some("v2+link"), false),
    ]);
    futures::executor::block_on(edit_via(&door, &repo, || plans.next())).unwrap();
    assert_eq!(*plans.asked.borrow(), 2, "stale 之后要重新问一次计划");
    assert_eq!(std::fs::read_to_string(h.join("d.md")).unwrap(), "v2+link");

    // 次次都 stale ⇒ 停在上限，说清楚，不无限重来。
    let door = DiskDoor::new(&h);
    let n = crate::user_files::EDIT_ATTEMPTS;
    for i in 0..n {
        door.interfere.borrow_mut().push(format!("x{i}"));
    }
    let plans = Plans::new(
        (0..n)
            .map(|_| plan(json!(null), "d.md", Some("nope"), Some("y"), false))
            .collect(),
    );
    let e = futures::executor::block_on(edit_via(&door, &repo, || plans.next())).unwrap_err();
    assert_eq!(*plans.asked.borrow(), n);
    assert!(e.contains(&n.to_string()), "{e}");
    std::fs::remove_dir_all(&h).ok();
}

/// ★ 删（`after = null`）：盘上还是算的那一份 ⇒ 删；已经变了 ⇒ 不删、重算。
#[test]
fn delete_checks_the_disk_still_holds_what_was_planned() {
    let h = temp_home("pc-del");
    let repo = h.display().to_string();
    std::fs::write(h.join("a.json"), "old").unwrap();
    let door = DiskDoor::new(&h);
    let plans = Plans::new(vec![
        plan(json!(true), "a.json", Some("stale-view"), None, false),
        plan(json!(true), "a.json", Some("old"), None, false),
    ]);
    let got = futures::executor::block_on(edit_via(&door, &repo, || plans.next())).unwrap();
    assert_eq!(got, json!(true));
    assert_eq!(
        *plans.asked.borrow(),
        2,
        "盘上不是计划里那一份 ⇒ 先重算，不删"
    );
    assert_eq!(door.deleted.borrow().as_slice(), &["a.json".to_string()]);
    assert!(!h.join("a.json").exists());
    std::fs::remove_dir_all(&h).ok();
}

/// 计划形状不对（两端版本对不上）⇒ 说清楚，不猜。
#[test]
fn a_plan_of_the_wrong_shape_is_refused() {
    let h = temp_home("pc-shape");
    let door = DiskDoor::new(&h);
    let plans = Plans::new(vec![json!({"value": 1, "edit": {"path": "x"}})]);
    let e = futures::executor::block_on(edit_via(&door, &h.display().to_string(), || plans.next()))
        .unwrap_err();
    assert!(e.contains("形状不对"), "{e}");
    assert!(door.puts.borrow().is_empty());
    std::fs::remove_dir_all(&h).ok();
}

/// 一个 `pub struct X { pub a: T, … }` 的字段名（生产段里按大括号配平取体）。
fn struct_fields(src: &str, name: &str) -> Vec<String> {
    // 可见性两边不同（上游 `pub`、本侧 `pub(crate)`），泛型参数也不同 ⇒ 认 `struct <名>` 后接 `<` 或空格。
    let at = [format!("struct {name} "), format!("struct {name}<")]
        .iter()
        .find_map(|k| src.find(k.as_str()))
        .unwrap_or_else(|| panic!("找不到 `struct {name}` —— 改了写法，本条跟着改"));
    let open = at + src[at..].find('{').expect("结构体没有体");
    let close = open + src[open..].find('}').expect("结构体没收尾");
    let mut out: Vec<String> = src[open + 1..close]
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            let t = t
                .strip_prefix("pub(crate) ")
                .or_else(|| t.strip_prefix("pub "))?;
            Some(t[..t.find(':')?].trim().to_string())
        })
        .collect();
    out.sort();
    out
}

/// ★ 计划的线上形状：本侧两个结构的字段 == 上游 `edits::Planned` / `edits::FileEdit` 的字段（两向；
/// 异源：一侧是本文件，一侧运行时读 vendored 上游源码 —— 上游改了形状、re-vendor 之后这里当场红）。
#[test]
fn the_plan_shape_matches_the_upstream_one() {
    let read = |p: std::path::PathBuf| {
        let s = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}：{e}"));
        assert!(s.len() > 2_000, "{p:?} 只有 {} 字节", s.len());
        guard_core::production_code(&s)
    };
    let up = read(
        crate::guard_support::repo_src_root().join("bridge/vendor/code-picture-core/src/edits.rs"),
    );
    let ours = read(crate::guard_support::repo_src_root().join("bridge/src/panorama_call.rs"));
    for name in ["Planned", "FileEdit"] {
        let theirs = struct_fields(&up, name);
        assert!(
            theirs.len() >= 2,
            "上游 `{name}` 只抽到 {theirs:?} —— 抽取坏了"
        );
        assert_eq!(
            struct_fields(&ours, name),
            theirs,
            "`{name}` 的字段两边对不上"
        );
    }
}

/// ★ 「算」op：本侧 [`EDITS`] 第二列 ＋ [`REFRESH_DOC_LINKS`] == 后端适配层 `OPS` 里除查询以外的那几个
/// （`plan_` 开头的 ＋ 刷文档关联那一个，两向；运行时读后端源码，异源）。
#[test]
fn the_edit_table_matches_the_backend_plan_ops() {
    let p = crate::guard_support::repo_src_root().join("backend/control/panorama.rs");
    let src = guard_core::production_code(&std::fs::read_to_string(&p).expect("读后端适配层"));
    let at = src
        .find("const OPS: &[(&str, u64)] = &[")
        .expect("后端 op 表改了写法");
    let body = &src[at..at + src[at..].find("];").unwrap()];
    let mut theirs: Vec<String> = body
        .lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("(\"")?;
            Some(rest[..rest.find('"')?].to_string())
        })
        .filter(|op| op.starts_with("plan_") || op == REFRESH_DOC_LINKS)
        .collect();
    theirs.sort();
    let mut ours: Vec<String> = EDITS.iter().map(|(_, p, _)| p.to_string()).collect();
    ours.push(REFRESH_DOC_LINKS.to_string());
    ours.sort();
    assert!(theirs.len() >= 6, "后端那一侧只抽到 {theirs:?}");
    assert_eq!(ours, theirs, "写那几种的「算」op 两边对不上");
}
