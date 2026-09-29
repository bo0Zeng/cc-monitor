//! 要求住址：主会话 09-28 裁 MIG-3b 报备 3 —— `panorama-edit` 进后端（〔RM1d〕V110「引擎只算、文件管理来写」）。
//! 〔MIG-3b 续〕问 · 交那一环的判据原住 `tests/frontend/shell/panorama_call_tests.rs`（monitor 那一跳），随实现搬来、期望一字未改；
//! 门换成这台真的文件管理面（`inbound::LocalFiles`，落在临时目录当仓），「算」用替身按顺序交计划、数被问了几次。
use super::*;
use crate::assets::door::Door;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// 这台文件管理面 ＋ 记账：每一条 `files-*` 记下来；`interfere` 里有货 ⇒ 下一次写之前先把目标改成它（造 `stale`）。
#[derive(Clone)]
struct RepoDoor {
    root: PathBuf,
    log: Arc<Mutex<Vec<(String, Value)>>>,
    interfere: Arc<Mutex<Vec<String>>>,
}

impl RepoDoor {
    fn new(tag: &str) -> RepoDoor {
        let root = std::env::temp_dir().join(format!("mig3b-pano-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        RepoDoor {
            root,
            log: Arc::new(Mutex::new(Vec::new())),
            interfere: Arc::new(Mutex::new(Vec::new())),
        }
    }
    fn repo(&self) -> String {
        self.root.display().to_string()
    }
    fn writes(&self) -> Vec<(String, Value)> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| c == "files-put" || c == "files-delete")
            .cloned()
            .collect()
    }
}

impl Drop for RepoDoor {
    fn drop(&mut self) {
        if Arc::strong_count(&self.log) == 1 {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

impl Door for RepoDoor {
    fn ask(&self, cmd: &str, args: Value) -> Result<Value, (String, String)> {
        self.log
            .lock()
            .unwrap()
            .push((cmd.to_string(), args.clone()));
        if cmd == "files-put" || cmd == "files-delete" {
            if let Some(v) = self.interfere.lock().unwrap().pop() {
                let rel = args["rel"].as_str().unwrap();
                std::fs::write(self.root.join(rel), v).unwrap();
            }
        }
        crate::stream::inbound::LocalFiles.ask(cmd, args)
    }
}

/// 替身「算」：按顺序交一份计划（包成 `{result}`），记下被问了几次、问的是什么。
#[derive(Clone)]
struct Plans {
    queue: Arc<Mutex<Vec<Value>>>,
    asked: Arc<Mutex<Vec<Value>>>,
}

impl Plans {
    fn new(mut v: Vec<Value>) -> Plans {
        v.reverse();
        Plans {
            queue: Arc::new(Mutex::new(v)),
            asked: Arc::new(Mutex::new(Vec::new())),
        }
    }
    fn ask(&self) -> impl Fn(Value) -> std::future::Ready<Result<Value, (String, String)>> + '_ {
        move |a: Value| {
            self.asked.lock().unwrap().push(a);
            let p = self.queue.lock().unwrap().pop().expect("计划被多问了一次");
            std::future::ready(Ok(json!({ "result": p })))
        }
    }
    fn times(&self) -> usize {
        self.asked.lock().unwrap().len()
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

fn args(d: &RepoDoor, op: &str) -> Value {
    json!({ "repo": d.repo(), "op": op, "args": { "x": 1 } })
}

/// ★ 计划原样交给写口：`after` 是全文、`expect` 是计划里的 `before`、`parents` 原样、不要备份；落下的就是计划里的全文。
/// 「算」问的是那一种写对应的 `plan_*`、带着仓与原样的 `args`。
#[tokio::test]
async fn the_plan_is_handed_to_the_door_verbatim() {
    let d = RepoDoor::new("put");
    let rel = ".codepicture/annotations/abc.json";
    let plans = Plans::new(vec![plan(
        json!("abc"),
        rel,
        None,
        Some("{\"id\":\"abc\"}"),
        true,
    )]);
    let got = answer_with(d.clone(), &args(&d, "add_annotation"), plans.ask())
        .await
        .unwrap();
    assert_eq!(got, json!("abc"), "回的是计划里的 value");
    assert_eq!(
        plans.asked.lock().unwrap()[0],
        json!({ "op": "plan_add_annotation", "repo": d.repo(), "args": { "x": 1 } })
    );
    let w = d.writes();
    assert_eq!(w.len(), 1);
    assert_eq!(
        (
            w[0].1["rel"].clone(),
            w[0].1["expect"].clone(),
            w[0].1["backup"].clone(),
            w[0].1["parents"].clone()
        ),
        (json!(rel), Value::Null, json!(false), json!(true))
    );
    assert_eq!(
        std::fs::read_to_string(d.root.join(rel)).unwrap(),
        "{\"id\":\"abc\"}"
    );
}

/// ★ 没有要写的（`edit = null`）⇒ 一次写都不发。
#[tokio::test]
async fn nothing_to_write_sends_nothing() {
    let d = RepoDoor::new("noop");
    let plans = Plans::new(vec![json!({"value": false, "edit": null})]);
    let got = answer_with(d.clone(), &args(&d, "remove_annotation"), plans.ask())
        .await
        .unwrap();
    assert_eq!(got, json!(false));
    assert!(d.writes().is_empty());
}

/// ★ `stale`（算完之后盘上那份被别人改了）⇒ **重新要一份计划**，不拿旧计划硬写；趟数有上限（码 `stale`）。
#[tokio::test]
async fn stale_means_plan_again_not_write_anyway() {
    let d = RepoDoor::new("stale");
    std::fs::write(d.root.join("d.md"), "v1").unwrap();
    d.interfere.lock().unwrap().push("v2".to_string());
    let plans = Plans::new(vec![
        plan(json!(null), "d.md", Some("v1"), Some("v1+link"), false),
        plan(json!(null), "d.md", Some("v2"), Some("v2+link"), false),
        json!({ "ok": true }), // 写成之后刷文档关联那一问
    ]);
    answer_with(d.clone(), &args(&d, "write_doc_link"), plans.ask())
        .await
        .unwrap();
    assert_eq!(
        plans.times(),
        3,
        "stale 之后要重新问一次计划，写成之后再刷一次文档关联"
    );
    assert_eq!(
        plans.asked.lock().unwrap()[2]["op"],
        json!(REFRESH_DOC_LINKS)
    );
    assert_eq!(
        std::fs::read_to_string(d.root.join("d.md")).unwrap(),
        "v2+link"
    );

    // 次次都 stale ⇒ 停在上限，说清楚，不无限重来。
    let n = crate::assets::door::EDIT_ATTEMPTS;
    for i in 0..n {
        d.interfere.lock().unwrap().push(format!("x{i}"));
    }
    let plans = Plans::new(
        (0..n)
            .map(|_| plan(json!(null), "d.md", Some("nope"), Some("y"), false))
            .collect(),
    );
    let (code, e) = answer_with(d.clone(), &args(&d, "add_annotation"), plans.ask())
        .await
        .unwrap_err();
    assert_eq!(plans.times(), n);
    assert_eq!(code, "stale");
    assert!(e.contains(&n.to_string()), "{e}");
}

/// ★ 删（`after = null`）：交给门的 `expect` 就是计划的 `before`；门回 `stale`（盘上不是那一份）⇒ 不删、重算；删前零 `peek`。
#[tokio::test]
async fn delete_hands_the_planned_bytes_to_the_door_as_expect() {
    let d = RepoDoor::new("del");
    std::fs::write(d.root.join("a.json"), "old").unwrap();
    let plans = Plans::new(vec![
        plan(json!(true), "a.json", Some("stale-view"), None, false),
        plan(json!(true), "a.json", Some("old"), None, false),
    ]);
    let got = answer_with(d.clone(), &args(&d, "remove_annotation"), plans.ask())
        .await
        .unwrap();
    assert_eq!(got, json!(true));
    assert_eq!(plans.times(), 2, "门说盘上不是计划里那一份 ⇒ 重算，不硬删");
    let expects: Vec<Value> = d
        .writes()
        .iter()
        .map(|(_, a)| a["expect"].clone())
        .collect();
    assert_eq!(
        expects,
        vec![json!("stale-view"), json!("old")],
        "每一次删都带着那一份计划的 before"
    );
    assert!(
        !d.log.lock().unwrap().iter().any(|(c, _)| c == "files-peek"),
        "删那一支又先 peek 了一趟"
    );
    assert!(!d.root.join("a.json").exists());
}

/// 计划形状不对（两端版本对不上）⇒ 说清楚，不猜；写之外的 op ⇒ `bad_args`、一次都不算。
#[tokio::test]
async fn a_plan_of_the_wrong_shape_or_an_unknown_op_is_refused() {
    let d = RepoDoor::new("shape");
    let plans = Plans::new(vec![json!({"value": 1, "edit": {"path": "x"}})]);
    let (code, e) = answer_with(d.clone(), &args(&d, "add_annotation"), plans.ask())
        .await
        .unwrap_err();
    assert_eq!(code, "failed");
    assert!(e.contains("形状不对"), "{e}");
    assert!(d.writes().is_empty());
    let plans = Plans::new(vec![]);
    let (code, _) = answer_with(d.clone(), &args(&d, "index"), plans.ask())
        .await
        .unwrap_err();
    assert_eq!((code.as_str(), plans.times()), ("bad_args", 0));
}

/// 「算」那一步的码原样往外交（`not_installed` / `unsupported` 是界面放字节再问一次的触发条件），一个字节不写。
#[tokio::test]
async fn the_plan_step_code_is_handed_out_as_is() {
    let d = RepoDoor::new("code");
    let ask =
        |_a: Value| std::future::ready(Err(("not_installed".to_string(), "没装".to_string())));
    let (code, _) = answer_with(d.clone(), &args(&d, "add_annotation"), ask)
        .await
        .unwrap_err();
    assert_eq!(code, "not_installed");
    assert!(d.writes().is_empty());
}

/// 一个 `pub struct X { pub a: T, … }` 的字段名（生产段里按大括号配平取体）。
fn struct_fields(src: &str, name: &str) -> Vec<String> {
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

/// ★ 计划的线上形状：本侧两个结构的字段 == 上游 `edits::Planned` / `edits::FileEdit` 的字段（两向；读 vendored 上游源码）。
#[test]
fn the_plan_shape_matches_the_upstream_one() {
    // 运行时读（不 `include_str!`：上游在 monitor 那一半的 vendor 树里，编译期跨半边的边有登记表管着）。
    let read = |rel: &str| {
        let p = crate::guard_support::repo_root().join(rel);
        let s = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}：{e}"));
        assert!(s.len() > 2_000, "{p:?} 只有 {} 字节", s.len());
        crate::guard_support::production_code(&s)
    };
    let up = read("src/panorama-engine/vendor/code-picture-core/src/edits.rs");
    let ours = read("src/backend/control/panorama_edit.rs");
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

/// ★ 「算」op：[`EDITS`] 第二列 ＋ [`REFRESH_DOC_LINKS`] == `panorama::OPS` 里 `plan_` 开头的 ＋ 刷文档关联那一个（两向，同一个 crate 直接比）。
#[test]
fn the_edit_table_matches_the_plan_ops() {
    let mut theirs: Vec<&str> = super::super::panorama::OPS
        .iter()
        .map(|(n, _)| *n)
        .filter(|op| op.starts_with("plan_") || *op == REFRESH_DOC_LINKS)
        .collect();
    theirs.sort_unstable();
    let mut ours: Vec<&str> = EDITS.iter().map(|(_, p, _)| *p).collect();
    ours.push(REFRESH_DOC_LINKS);
    ours.sort_unstable();
    assert!(theirs.len() >= 6, "只抽到 {theirs:?}");
    assert_eq!(ours, theirs, "写那几种的「算」op 两边对不上");
}
