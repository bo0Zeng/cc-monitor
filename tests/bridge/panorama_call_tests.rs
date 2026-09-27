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

/// ★ 删（`after = null`）：交给门的 `expect` 就是计划的 `before`；门回 `stale`（盘上不是那一份）⇒ 不删、重算；
/// 〔RM1e〕**删前零 `peek`** —— CAS 在写口（后端 `files-delete` 的 `expect`）闭合，不再先核一趟再删。
#[test]
fn delete_hands_the_planned_bytes_to_the_door_as_expect() {
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
        "门说盘上不是计划里那一份 ⇒ 重算，不硬删"
    );
    assert_eq!(
        door.deleted.borrow().as_slice(),
        &[
            ("a.json".to_string(), "stale-view".to_string()),
            ("a.json".to_string(), "old".to_string()),
        ],
        "每一次删都带着那一份计划的 before 交给门"
    );
    assert_eq!(*door.peeked.borrow(), 0, "删那一支又先 peek 了一趟");
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

// ── 〔RM1e〕没装 / 太旧 ⇒ 推字节再问一次（`ask_or_push`）──────────────────────────────
//
// 要求住址：用户 09-24 **V108**（`设计/99 §1`）逐字「一个只装全景引擎的独立小程序，随后端部署、
// **只传给开过远端全景的机器**」；推的形状与触发点见 `调研/第四波记录/RM1c.md §4 ①`、`RM1e.md §1.1`。

/// 问的替身：按顺序一份一份交，记下被问了几次。
struct Answers {
    queue: RefCell<Vec<Result<Value, Asked>>>,
    asked: RefCell<usize>,
}
impl Answers {
    fn new(mut v: Vec<Result<Value, Asked>>) -> Self {
        v.reverse();
        Answers {
            queue: RefCell::new(v),
            asked: RefCell::new(0),
        }
    }
    fn next(&self) -> impl std::future::Future<Output = Result<Value, Asked>> {
        *self.asked.borrow_mut() += 1;
        let a = self.queue.borrow_mut().pop().expect("被多问了一次");
        std::future::ready(a)
    }
}

fn coded(code: &str) -> Result<Value, Asked> {
    Err(Asked {
        code: Some(code.to_string()),
        said: format!("对端说 {code}"),
    })
}

/// ★ 「缺 / 旧」⇒ **恰推一次、恰再问一次**、交回第二问的结果；其余失败 ⇒ **零推**、原话带回。
#[test]
fn only_missing_or_old_bytes_trigger_exactly_one_push_and_one_retry() {
    // 期望取自题面（`RM1c.md §4 ①`「远端 `panorama` 回 `not_installed`（或 `unsupported` = 旧版，缺某个 op）时」），
    // 不取自 [`PUSH_ON`] —— 拿被测的表去驱动判它的用例，两侧同源恒真（死值验 K1 首刀就是这样没砍中的）。
    for code in ["not_installed", "unsupported"] {
        let asks = Answers::new(vec![coded(code), Ok(json!({"ok": 1}))]);
        let pushed = RefCell::new(0usize);
        let announced = RefCell::new(0usize);
        let got = futures::executor::block_on(ask_or_push(
            || asks.next(),
            || *announced.borrow_mut() += 1,
            || {
                // 〔RM1f · P1〕「正在装」那一句恰在推之前说过一次。
                assert_eq!(*announced.borrow(), 1, "推之前没说「正在装」");
                *pushed.borrow_mut() += 1;
                std::future::ready(Ok(()))
            },
        ));
        assert_eq!(got, Ok(json!({"ok": 1})), "{code}");
        assert_eq!((*pushed.borrow(), *asks.asked.borrow()), (1, 2), "{code}");
        assert_eq!(*announced.borrow(), 1, "{code}：「正在装」说了不止一次");
    }
    // 其余码与「没发出去」（没有码）：一次都不推，只问一次。
    let others = [
        coded("failed"),
        coded("bad_args"),
        coded("timed_out"),
        coded("too_large"),
        Err(Asked {
            code: None,
            said: "没有控制通道".into(),
        }),
        Ok(json!(null)),
    ];
    for first in others {
        let want = first.clone().map_err(|a| a.said);
        let asks = Answers::new(vec![first]);
        let pushed = RefCell::new(0usize);
        let announced = RefCell::new(0usize);
        let got = futures::executor::block_on(ask_or_push(
            || asks.next(),
            || *announced.borrow_mut() += 1,
            || {
                *pushed.borrow_mut() += 1;
                std::future::ready(Ok(()))
            },
        ));
        assert_eq!(got, want);
        assert_eq!((*pushed.borrow(), *asks.asked.borrow()), (0, 1));
        assert_eq!(
            *announced.borrow(),
            0,
            "〔RM1f · P1〕不推就不该说「正在装」"
        );
    }
}

/// ★ 推完仍说「缺 / 旧」⇒ 如实报、**不循环**（推 1 次、问 2 次）；推失败 ⇒ 原话 ＋ 推失败那句都在、不再问。
#[test]
fn a_push_that_does_not_help_or_fails_is_said_not_looped() {
    let asks = Answers::new(vec![coded("unsupported"), coded("unsupported")]);
    let pushed = RefCell::new(0usize);
    let e = futures::executor::block_on(ask_or_push(
        || asks.next(),
        || {},
        || {
            *pushed.borrow_mut() += 1;
            std::future::ready(Ok(()))
        },
    ))
    .unwrap_err();
    assert_eq!((*pushed.borrow(), *asks.asked.borrow()), (1, 2));
    assert!(
        e.contains("已经把这一版") && e.contains("对端说 unsupported"),
        "{e}"
    );

    let asks = Answers::new(vec![coded("not_installed")]);
    let e = futures::executor::block_on(ask_or_push(
        || asks.next(),
        || {},
        || std::future::ready(Err("围栏拒了".to_string())),
    ))
    .unwrap_err();
    assert_eq!(*asks.asked.borrow(), 1, "推没成就不再问");
    assert!(
        e.contains("对端说 not_installed") && e.contains("围栏拒了"),
        "{e}"
    );
}

/// 后端适配层 `answer_with` 里某一条语句（从 `start` 起到 `end` 止）映射出来的码：`("<码>"` 那几处。
fn codes_in_statement(prod: &str, start: &str, end: &str) -> Vec<String> {
    let at = prod
        .find(start)
        .unwrap_or_else(|| panic!("后端适配层里找不到 `{start}` —— 改了写法，本条跟着改"));
    let stmt = &prod[at..at + prod[at..].find(end).expect("语句没收尾")];
    let mut out = Vec::new();
    let mut rest = stmt;
    while let Some(i) = rest.find('(') {
        rest = &rest[i + 1..];
        let Some(lit) = rest.strip_prefix('"') else {
            continue;
        };
        let Some(j) = lit.find('"') else { break };
        let w = &lit[..j];
        if !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
            out.push(w.to_string());
        }
    }
    out
}

/// ★ [`PUSH_ON`] == 后端适配层把「① 找不到 / ② 问了不是它 · 缺能力」映射出来的码（两向集合相等）。
///
/// 异源：一侧是本文件的常量，另一侧运行时读 `src/backend/control/panorama.rs` 生产段里
/// `discover::find(…)` 与 `probe::negotiate(…)` 那两条语句。那边多一个「缺字节」的码而这边不推 ⇒ 红；
/// 这边多推一个与字节无关的码（如 `failed`）⇒ 红。
#[test]
fn push_on_equals_the_codes_the_backend_gives_for_missing_or_old_bytes() {
    let p = crate::guard_support::repo_src_root().join("backend/control/panorama.rs");
    let prod = guard_core::production_code(&std::fs::read_to_string(&p).expect("读后端适配层"));
    let mut theirs = codes_in_statement(&prod, "plugin::discover::find(", ";");
    theirs.extend(codes_in_statement(
        &prod,
        "plugin::probe::negotiate(",
        // 收尾按 `)?;` 认（`cargo fmt` 会把 `.map_err(|r| match …)` 折成两种排法，`})?;` 只认其中一种）。
        ")?;",
    ));
    theirs.sort();
    theirs.dedup();
    assert!(theirs.len() >= 2, "后端那一侧只抽到 {theirs:?} —— 抽取坏了");
    let mut ours: Vec<String> = PUSH_ON.iter().map(|s| s.to_string()).collect();
    ours.sort();
    assert_eq!(ours, theirs, "「缺 / 旧 ⇒ 推」的码两边对不上");
}

/// 一段源码里第一个 `json!({ … })` 的键（`"k":` 那几处）。
/// 调用方先把 `seg` 切到**那一次调用**为止（里面恰好一个 `json!({ … })`）。
fn json_keys(seg: &str) -> Vec<String> {
    let at = guard_core::find_pinned(seg, "json!({")
        .unwrap_or_else(|e| panic!("那一次调用里的 json!({{ 不是恰好一处：{e}"));
    let end = guard_core::find_pinned(seg, "})")
        .unwrap_or_else(|e| panic!("那一次调用里的 }}) 不是恰好一处：{e}"));
    let body = &seg[at..end];
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(i) = rest.find('"') {
        rest = &rest[i + 1..];
        let Some(j) = rest.find('"') else { break };
        let (w, after) = (&rest[..j], &rest[j + 1..]);
        if after.trim_start().starts_with(':') {
            out.push(w.to_string());
        }
        rest = after;
    }
    out.sort();
    out
}

/// ★〔RM1e〕删批注的 CAS 真的**上了线**：monitor 那扇门发 `files-delete` 时带的键 == 后端 `MANAGE_COMMANDS`
/// 里 `files-delete` 声明的参数 − `recursive`（门只删一份文件，从不删整棵树）。
///
/// 异源：一侧读 `user_files.rs` 生产段（`BackendDoor::delete` 那一处 `.ask`），一侧读后端 `files_write.rs`。
/// 门上漏发 `expect`（CAS 在线上就没了，替身门照样绿）⇒ 这里红；后端再加一个参数 ⇒ 这里红、逼人看一眼。
#[test]
fn the_door_sends_expect_with_every_delete() {
    let root = crate::guard_support::repo_src_root();
    let door = guard_core::production_code(
        &std::fs::read_to_string(root.join("bridge/src/user_files.rs")).expect("读门"),
    );
    let at = guard_core::find_pinned(&door, "\"files-delete\",")
        .unwrap_or_else(|e| panic!("门上 files-delete 那一问不是恰好一处：{e}"));
    let call = &door[at..];
    let call = &call[..call
        .lines()
        .take_while(|l| !l.contains(".await"))
        .map(|l| l.len() + 1)
        .sum::<usize>()];
    let ours = json_keys(call);
    let back = guard_core::production_code(
        &std::fs::read_to_string(root.join("backend/control/files_write.rs")).expect("读后端写面"),
    );
    let at = guard_core::find_pinned(&back, "name: \"files-delete\",")
        .unwrap_or_else(|e| panic!("后端写面 files-delete 那一条不是恰好一处：{e}"));
    let line = back[at..]
        .lines()
        .map(str::trim_start)
        .find(|l| l.starts_with("args: &["))
        .expect("那一条没有 args");
    let mut theirs: Vec<String> = line
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|a| *a != "recursive")
        .map(str::to_string)
        .collect();
    theirs.sort();
    assert!(theirs.len() >= 3, "后端那一侧只抽到 {theirs:?} —— 抽取坏了");
    assert_eq!(ours, theirs, "门发 files-delete 的键与后端声明的对不上");
}

/// ★〔RM1f · C5〕**撤票 ⇒ 那一问被丢掉**（不是等它自己回来）、交回「已取消」、票摘掉；
/// 没撤 ⇒ 原样交回、票同样摘掉；同一张票同时只许一问；撤一张不在飞的票 ⇒ `false`。
///
/// 「被丢掉」是承重的：`inbound_client::call` 的放弃守卫靠析构补发 `cancel`，后端才撤得掉建索引
/// （那一格由 `inbound_client_tests::abandoning_the_wait_fires_one_cancel_and_finishing_fires_none` 钉）。
#[tokio::test]
async fn a_cancelled_ticket_drops_the_ask_and_says_so() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    struct Flag(Arc<AtomicBool>);
    impl Drop for Flag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let dropped = Arc::new(AtomicBool::new(false));
    let flag = Flag(dropped.clone());
    // 一问永远不回来（模拟后端那一趟建索引），被丢时举旗。
    let never = async move {
        let _f = flag;
        std::future::pending::<Result<Value, String>>().await
    };
    let t = "rm1f-c5-a".to_string();
    let waiter = tokio::spawn(with_ticket(t.clone(), never));
    // 等它登记上（登记在第一次 poll 里做）。
    for _ in 0..200 {
        if lock_tickets().contains_key(&t) {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(lock_tickets().contains_key(&t), "票没登记上");
    assert!(panorama_cancel(t.clone()), "在飞的票撤不掉");
    let got = waiter.await.expect("task");
    assert_eq!(got, Err(CANCELLED_SAID.to_string()));
    assert!(
        dropped.load(Ordering::SeqCst),
        "撤了票，那一问却没被丢掉 —— 后端收不到 cancel"
    );
    assert!(!lock_tickets().contains_key(&t), "撤完票还留在表里");
    assert!(!panorama_cancel(t.clone()), "撤一张不在飞的票应当回 false");

    // 没撤：原样交回、票摘掉。
    let t2 = "rm1f-c5-b".to_string();
    let ok = with_ticket(t2.clone(), async { Ok(json!({"n": 1})) }).await;
    assert_eq!(ok, Ok(json!({"n": 1})));
    assert!(!lock_tickets().contains_key(&t2));

    // 同一张票同时只许一问。
    let t3 = "rm1f-c5-c".to_string();
    let first = tokio::spawn(with_ticket(
        t3.clone(),
        std::future::pending::<Result<Value, String>>(),
    ));
    for _ in 0..200 {
        if lock_tickets().contains_key(&t3) {
            break;
        }
        tokio::task::yield_now().await;
    }
    let dup = with_ticket(t3.clone(), async { Ok(json!(null)) }).await;
    assert!(dup.is_err(), "同一张票第二问没被拒：{dup:?}");
    assert!(panorama_cancel(t3.clone()));
    assert_eq!(first.await.expect("task"), Err(CANCELLED_SAID.to_string()));
}

/// ★〔RM1f · P1b〕「正在装」那一句：走远端健康通道（`kind` 是前端标题表认得的那一个），
/// 说的是哪台机器（机器名，不是 `<local>` 这类内部串），带着「装好会自己接着答」。
#[test]
fn the_install_notice_names_the_machine_and_rides_the_health_channel() {
    let p = install_notice("box1");
    assert_eq!(p.origin, "box1");
    assert_eq!(p.kind, INSTALL_NOTICE_KIND);
    assert!(
        p.message.contains("box1") && p.message.contains("正在"),
        "{}",
        p.message
    );
    // 前端 `remote-health.ts` 的标题表认得这个 kind（异源：读 TS 源码）。
    let ts = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../remote-health.ts"),
    )
    .expect("读 src/remote-health.ts");
    assert!(
        ts.contains(&format!("case \"{INSTALL_NOTICE_KIND}\":")),
        "前端标题表不认 `{INSTALL_NOTICE_KIND}` —— 那一句会落进通用的「远端提示」标题"
    );
}
