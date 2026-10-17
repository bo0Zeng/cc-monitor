//! 历史页平铺清单（`history-list`）的判据。
//!
//! # 守的要求（住址）
//!
//! 一份跨项目的平铺清单，每行带显示标题（没标题没第一句也给一个）· 真实目录分组 · 机器 · 那一家 ·
//! 状态 · 注解 · 分叉父子 · 分身 · 条数 · 时间，以及「这一行能做什么」；标题 / 第一句 / 项目名的搜索在后端；远端每台由那台出、本机并注解。
//!
//! # 判据
//!
//! 1. **这台的清单**：记录树各目录的行带 `projectDir` / `group`（按真实目录）· 读不了的目录进 `failed` · 合成历史按 `<kind>:<cwd>` · 上次的号逐 sid。
//! 2. **注解并的是这台的**（远端行也并）；`label` = 改过的标题 ＞ 标题 ＞ 第一句；都没有 ⇒ `untitled`。
//! 3. **能做什么**：在跑 ⇒ 恢复 `switch`、删 `live`；分身 ⇒ `bg`、不能分叉；判不了活 ⇒ 删 `unsure`；没有账号维的那一家 ⇒ `accounts:false`。
//! 4. **筛**：隐藏的默认不出（`hidden:true` 才出）· 时间窗 · 搜索词只比显示标题 / 第一句 / 项目名、不分大小写。
//! 5. **分叉父会话被筛掉** ⇒ 照样带上、标 `context`（也是一行，算进总数）。
//! 6. **排与窗口**：`activity` 按最后活动、`created` 按开始，`at` 就是那个键；`range` 只回那一窗、总数是整份的。
//! 7. **分组**：有在跑的 → 有星标的 → 最近动过的；读不了的目录是一组、带原因。
//! 8. **远端**：那台的清单由这台问那台的常驻取来记着（`remote_listing.rs`，判据在 `remote_listing_tests`），这台并 · 筛 · 排、行组带 `origin`；
//!    够不着那台 ⇒ `unreachable`；`fresh` 不带 `origin` ⇒ `bad_args`；旧的 `raw` · `listing` 两格不认（`bad_args`）。
//! 10. **按会话 ID 要一行**（`sid`：独立查看窗开任意一个会话）：只回那一行（隐藏的、出了时间窗的也回）、不补父会话；
//!     形状不对 ⇒ `bad_args`（先于 IO）；没有这个会话 ⇒ 空清单。
//! 11. **带记号再问**（`common/views.rs`）：同一问第二次 ⇒ `same`；一条会话写了一行 ⇒ `up` 只它一行（先后没变不带 `order`）；
//!     换问法 ⇒ `whole`；帧面那一臂带着 `held` 来照样答。
//! 9. **跨语言金样** `tests/__fixtures__/history-list.golden.json`（TS 严格解码器读同一份，`tests/frontend/ui/history-list-reads.vitest.ts`）。
//!
//! # 买不到
//!
//! - 🔴 真远端（界面经长连接问那台的那一跳）· 真盘上上万份记录时的耗时。

use super::*;
use crate::history::history_annotations::{Entry, Table};
use serde_json::json;
use std::path::PathBuf;

// 判据照线上的样子读（成品与清单都过一遍真序列化器）：下面四个把有类型的那几个口包成 JSON 进出，盖住 `super::*` 里的同名口。

fn listing_from(
    tree: Vec<(String, Result<Vec<Value>, String>)>,
    synth: &[(
        &'static str,
        Vec<crate::agents::SynthSession>,
        fn(&std::path::Path) -> String,
    )],
    live: &LiveSet,
    last: &Value,
) -> Value {
    serde_json::to_value(super::listing_from(tree, synth, live, last)).unwrap()
}

/// 整份那一答（新开一份备忘、不带记号）照线上读，摊成「行（照 `order`）· 组 · 注解那一句 · 总数」给逐格的判据看。
fn answer_from(
    listing: &Value,
    origin: Option<&str>,
    ann: Result<&Table, String>,
    ask: &Ask,
    now_ms: i64,
    tz: &crate::Tz,
) -> Value {
    let views = Views::new(8);
    flat(&patch(&views, listing, origin, ann, ask, None, now_ms, tz))
}

/// 经数据层答一问（[`super::patch_from`]），照线上读。
#[allow(clippy::too_many_arguments)]
fn patch(
    views: &Views,
    listing: &Value,
    origin: Option<&str>,
    ann: Result<&Table, String>,
    ask: &Ask,
    held: Option<&Value>,
    now_ms: i64,
    tz: &crate::Tz,
) -> Value {
    let l: Listing = serde_json::from_value(listing.clone()).expect("清单读不回来");
    let held: Option<Held> = held.map(|h| serde_json::from_value(h.clone()).expect("记号"));
    serde_json::to_value(super::patch_from(
        views,
        &l,
        origin,
        ann,
        ask,
        held.as_ref(),
        now_ms,
        tz,
    ))
    .unwrap()
}

/// 整份那一答摊平：行照 `order` 从 `up` 取。不是整份 ⇒ 判据写错了。
fn flat(p: &Value) -> Value {
    assert_eq!(p["whole"], true, "摊平只给整份那一答：{p}");
    let rows: Vec<Value> = p["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| p["up"][k.as_str().unwrap()].clone())
        .collect();
    json!({
        "rows": rows,
        "groups": p["head"]["info"]["groups"],
        "notice": p["head"]["info"]["notice"],
        "total": p["head"]["total"],
    })
}

fn with_own_annotations(listing: &mut Value, loaded: &crate::history::history_annotations::Loaded) {
    let mut l: Listing = serde_json::from_value(listing.clone()).expect("清单读不回来");
    super::with_own_annotations(&mut l, loaded);
    *listing = serde_json::to_value(l).unwrap();
}

/// 入参照帧面那一处解（typed：形状不对 ⇒ `bad_args`）再核值域，盖住 `super::*` 里的同名口。
fn parse_ask(args: &Value) -> Result<Ask, (&'static str, String)> {
    let req: HistoryListReq =
        crate::wire::typed::take(args).map_err(|f| ("bad_args", f.message))?;
    super::parse_ask(&req)
}

/// 帧面那一臂（入参照 typed 那一处解、回包过一遍真序列化器），盖住 `super::*` 里的同名口。
async fn answer_wire(args: Value) -> Result<Value, (&'static str, String)> {
    let req: HistoryListReq =
        crate::wire::typed::take(&args).map_err(|f| ("bad_args", f.message))?;
    super::answer(req, Default::default(), None)
        .await
        .map(|r| serde_json::to_value(r).unwrap())
}

/// 帧面那一臂的整份那一答摊平（[`flat`]）。
async fn answer_flat(args: Value) -> Result<Value, (&'static str, String)> {
    answer_wire(args).await.map(|v| flat(&v))
}

fn listing_annotations(listing: &Value) -> Result<Table, String> {
    super::listing_annotations(&serde_json::from_value(listing.clone()).expect("清单读不回来"))
}

fn fixtures() -> PathBuf {
    guard_core::addr::repo_root().join("tests/__fixtures__")
}

/// 这台的注解：S1 星标 · S2 隐藏 · S4 星标 ＋ 隐藏（S3 S5 没有注解）。
fn ann() -> Table {
    let e = |starred, hidden: bool| Entry {
        starred,
        hide: hidden.then_some(crate::history::history_annotations::Hide::On),
        ..Entry::default()
    };
    [
        (S1, e(true, false)),
        (S2, e(false, true)),
        (S4, e(true, true)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

const S1: &str = "0000aaaa-0000-4000-8000-000000000001";
const S2: &str = "0000aaaa-0000-4000-8000-000000000002";
const S3: &str = "0000aaaa-0000-4000-8000-000000000003";
const S4: &str = "0000aaaa-0000-4000-8000-000000000004";
const S5: &str = "0000aaaa-0000-4000-8000-000000000005";

fn row(
    sid: &str,
    cwd: &str,
    started: i64,
    updated: i64,
    title: Option<&str>,
    said: &str,
    bg: bool,
    fork: Option<&str>,
) -> Value {
    json!({
        "sessionId": sid,
        "jsonlPath": format!("/h/.claude/projects/-w/{sid}.jsonl"),
        "startedAtMs": started,
        "updatedAtMs": updated,
        "messageCountApprox": 3,
        "firstUserExcerpt": said,
        "aiTitle": title,
        "cwd": cwd,
        "isBg": bg,
        "forkedFromSessionId": fork,
        "forkedFromMessageUuid": fork.map(|_| "m-1"),
    })
}

fn no_excerpt(_: &std::path::Path) -> String {
    "列出 Codex 的会话".to_string()
}

/// 这台的清单：两个记录目录（`-w-alpha` 里两组真实目录）＋ 一个读不了的 ＋ 一条合成历史。
fn listing() -> Value {
    let tree = vec![
        (
            "-w-alpha".to_string(),
            Ok(vec![
                row(
                    S1,
                    "/w/alpha",
                    100,
                    900,
                    Some("支付回调验签"),
                    "看一下回调",
                    false,
                    None,
                ),
                row(
                    S2,
                    "/w/alpha",
                    200,
                    800,
                    None,
                    "隐藏起来的那条",
                    false,
                    None,
                ),
                row(S3, "/w/alpha-二", 300, 700, None, "", false, None),
            ]),
        ),
        (
            "-w-beta".to_string(),
            Ok(vec![
                row(
                    S4,
                    "/w/beta",
                    400,
                    600,
                    Some("分叉的父会话"),
                    "父",
                    false,
                    None,
                ),
                row(
                    S5,
                    "/w/beta",
                    500,
                    1000,
                    Some("从父会话分出来的"),
                    "子",
                    true,
                    Some(S4),
                ),
            ]),
        ),
        (
            "-w-gamma".to_string(),
            Err("读不了 /h/.claude/projects/-w-gamma".to_string()),
        ),
    ];
    let synth = vec![(
        "codex",
        vec![crate::agents::SynthSession {
            sid: "019a0000-0000-7000-8000-00000000c0de".into(),
            path: PathBuf::from("/h/.codex/sessions/2026/09/25/rollout-x.jsonl"),
            cwd: "/w/delta".into(),
            mtime_ms: 650,
        }],
        no_excerpt as fn(&std::path::Path) -> String,
    )];
    let live = LiveSet(
        [(
            S1.to_string(),
            Some(crate::agents::SessionActivity::Working),
        )]
        .into_iter()
        .collect(),
    );
    listing_from(tree, &synth, &live, &json!({ S1: "work", S3: "home" }))
}

fn ask(args: Value) -> Ask {
    parse_ask(&args).expect("入参")
}

fn answer(args: Value) -> Value {
    answer_from(
        &listing(),
        None,
        Ok(&ann()),
        &ask(args),
        1_000,
        &Default::default(),
    )
}

fn sids(v: &Value) -> Vec<String> {
    v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["sessionId"].as_str().unwrap().to_string())
        .collect()
}

fn find<'a>(v: &'a Value, sid: &str) -> &'a Value {
    v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["sessionId"] == sid)
        .unwrap_or_else(|| panic!("{sid} 不在清单里"))
}

/// ★ 判据 1：这台的清单逐格。
#[test]
fn the_machine_listing_carries_group_dir_failures_synth_and_last_accounts() {
    let l = listing();
    let rows = l["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 6, "记录树 5 条 ＋ 合成 1 条");
    let s3 = rows.iter().find(|r| r["sessionId"] == S3).unwrap();
    assert_eq!(s3["projectDir"], "-w-alpha");
    assert_eq!(
        s3["group"], "claude:/w/alpha-二",
        "同一记录目录里的不同真实目录各是一组（R5W-H09）"
    );
    assert_eq!(s3["lastAccount"], "home");
    let s1 = rows.iter().find(|r| r["sessionId"] == S1).unwrap();
    assert_eq!(s1["group"], "claude:/w/alpha");
    assert_eq!(s1["lastAccount"], "work");
    assert!(rows
        .iter()
        .find(|r| r["sessionId"] == S2)
        .unwrap()
        .get("lastAccount")
        .is_none());
    let cx = rows.iter().find(|r| r["agent"] == "codex").unwrap();
    assert_eq!(cx["projectDir"], "codex:/w/delta");
    assert_eq!(
        cx["life"],
        Value::Null,
        "合成历史判不了活 ⇒ null，不是 false"
    );
    assert_eq!(
        answer(json!({}))["rows"][0].get("life"),
        None,
        "成品里活不活只在整份脸里"
    );
    assert_eq!(
        l["failed"],
        json!([{"projectDir": "-w-gamma", "error": "读不了 /h/.claude/projects/-w-gamma"}])
    );
}

/// ★ 判据 2 ＋ 3：注解 · 标题 · 能做什么。
#[test]
fn rows_carry_annotations_label_and_what_can_be_done() {
    let v = answer(json!({"hidden": true}));
    let s1 = find(&v, S1);
    assert_eq!(s1["starred"], true);
    assert_eq!(s1["label"], "支付回调验签");
    assert_eq!(s1["face"]["live"]["is"], "yes");
    assert_eq!(
        s1["can"],
        json!({"resume": "switch", "accounts": true, "fork": true, "delete": "live"})
    );
    let s3 = find(&v, S3);
    assert_eq!(s3["untitled"], true, "没标题没第一句 ⇒ untitled（R5W-H04）");
    assert_eq!(
        s3["can"],
        json!({"resume": "yes", "accounts": true, "fork": true, "delete": "yes"})
    );
    let s2 = find(&v, S2);
    assert_eq!(s2["label"], "隐藏起来的那条", "没标题 ⇒ 第一句");
    assert_eq!(s2["untitled"], false);
    let s5 = find(&v, S5);
    assert_eq!(s5["can"]["resume"], "bg");
    assert_eq!(s5["can"]["fork"], false, "分身会话不给分叉");
    let cx = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["agent"] == "codex")
        .unwrap();
    assert_eq!(cx["face"]["dot"], "unknown", "合成历史判不了活 ⇒ 状态不明");
    assert_eq!(cx["agentTag"], "Codex", "不是默认那一家 ⇒ 行上画它的叫法");
    assert_eq!(s1["agentTag"], Value::Null, "默认那一家不画");
    assert_eq!(
        cx["can"],
        json!({"resume": "yes", "accounts": false, "fork": false, "delete": "unsure"})
    );
    // 改过的标题压过原标题（正控：注解里 S1 没改名、label 是原标题；这里给它一条改名）。
    let mut t = ann();
    t.get_mut(S1).unwrap().custom_title = Some("改过的".into());
    let w = answer_from(
        &listing(),
        None,
        Ok(&t),
        &ask(json!({})),
        1_000,
        &Default::default(),
    );
    assert_eq!(find(&w, S1)["label"], "改过的");
    assert_eq!(find(&w, S1)["customTitle"], "改过的");
}

/// ★ 判据 4 ＋ 5：筛（隐藏 · 时间 · 搜索词）与被筛掉的分叉父会话。
#[test]
fn filters_and_context_parents() {
    let v = answer(json!({}));
    assert!(!sids(&v).contains(&S2.to_string()), "隐藏的默认不出");
    // S4 星标 ＋ 隐藏、S5 是它分出来的 ⇒ S4 照样带上、标 context（也是一行，算进 total）。
    assert_eq!(find(&v, S4)["context"], true);
    assert_eq!(v["total"], 5, "S1 S3 S5 codex ＋ context 的 S4");
    let v = answer(json!({"hidden": true}));
    assert!(
        find(&v, S4).get("context").is_none(),
        "显示隐藏的 ⇒ 父会话自己就在"
    );
    assert_eq!(v["total"], 6);
    // 搜索词：标题 · 第一句 · 项目名，不分大小写；不比路径。
    assert_eq!(sids(&answer(json!({"query": "回调"}))), vec![S1]);
    assert_eq!(sids(&answer(json!({"query": "ALPHA-二"}))), vec![S3]);
    assert!(
        sids(&answer(json!({"query": "/h/.claude"}))).is_empty(),
        "路径不进搜索"
    );
    let q = answer(json!({"query": "分出来"}));
    assert_eq!(
        sids(&q),
        vec![S5, S4],
        "子会话命中、父会话被筛掉 ⇒ 父会话以 context 带上"
    );
    // 时间窗：now = 1000，within 1 天 ⇒ 全在；正控：给一个 now 远在之后的 ⇒ 全出窗。
    let far = answer_from(
        &listing(),
        None,
        Ok(&ann()),
        &ask(json!({"within_days": 1})),
        1_000 + 2 * 86_400_000,
        &Default::default(),
    );
    assert_eq!(far["total"], 0);
}

/// ★ 判据 10：按会话 ID 要一行。
#[test]
fn one_session_by_id_ignores_the_other_filters() {
    // S2 被隐藏；S5 有被隐藏的父会话 S4 —— 按 ID 要都只回那一行。
    let v = answer(json!({"sid": S2, "query": "不会命中", "within_days": 1}));
    assert_eq!(sids(&v), vec![S2], "隐藏的、搜索词不中的也回");
    assert_eq!(v["total"], 1);
    assert_eq!(find(&v, S2)["hidden"]["by"], "self");
    let v = answer(json!({"sid": S5}));
    assert_eq!(sids(&v), vec![S5], "不补父会话");
    assert!(find(&v, S5).get("context").is_none());
    assert_eq!(
        find(&v, S5)["jsonlPath"]
            .as_str()
            .map(|p| p.ends_with(".jsonl")),
        Some(true)
    );
    // 正控：没有 sid ⇒ 照常（S2 隐藏不出）。
    assert!(!sids(&answer(json!({}))).contains(&S2.to_string()));
    assert!(sids(&answer(
        json!({"sid": "0000dead-0000-4000-8000-000000000000"})
    ))
    .is_empty());
    for bad in [json!(""), json!("a/b"), json!(7), json!("x".repeat(65))] {
        let e = parse_ask(&json!({ "sid": bad })).expect_err("形状不对");
        assert_eq!(e.0, "bad_args");
    }
}

/// ★ 判据 6：排与窗口。
#[test]
fn sort_key_and_limit() {
    let v = answer(json!({"hidden": true}));
    let at: Vec<i64> = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["at"].as_i64().unwrap())
        .collect();
    assert_eq!(
        at,
        vec![1000, 900, 800, 700, 650, 600],
        "activity ⇒ updatedAt 倒序"
    );
    let v = answer(json!({"hidden": true, "sort": "created"}));
    assert_eq!(
        sids(&v),
        vec!["019a0000-0000-7000-8000-00000000c0de", S5, S4, S3, S2, S1],
        "created ⇒ startedAt 倒序（合成历史的开始 = 修改时刻 650）"
    );
    let v = answer(json!({"hidden": true, "range": {"from": 1, "limit": 2}}));
    assert_eq!(sids(&v), vec![S1, S2], "只回那一窗");
    assert_eq!(v["total"], 6, "总数是整份的");
    for bad in [
        json!({"sort": "x"}),
        json!({"range": {"from": 0, "limit": 0}}),
        json!({"limit": 2}),
        json!({"within_days": "7"}),
        json!({"query": 1}),
        json!({"hidden": "y"}),
        json!({"extra": 1}),
    ] {
        assert_eq!(parse_ask(&bad).unwrap_err().0, "bad_args", "{bad}");
    }
}

/// ★ 判据 7：分组。
#[test]
fn groups_rank_live_then_starred_then_recent_and_keep_failed_dirs() {
    let v = answer(json!({"hidden": true}));
    let keys: Vec<&str> = v["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        vec![
            "claude:/w/alpha",
            "codex:/w/delta",
            "claude:/w/beta",
            "claude:/w/alpha-二",
            "dir:-w-gamma"
        ],
        "在跑的（alpha）→ 说不清的（codex）→ 有星标的（beta）→ 最近的；读不了的目录殿后"
    );
    let g = &v["groups"][0];
    assert_eq!(g["count"], 2);
    assert_eq!(g["hasLive"], true);
    assert_eq!(
        v["groups"][1]["hasLive"],
        Value::Null,
        "判不了活的那一组不说「都没在跑」"
    );
    assert_eq!(v["groups"][2]["hasLive"], false);
    assert_eq!(v["groups"][2]["failed"], Value::Null);
    assert_eq!(
        v["groups"][4]["failed"],
        "读不了 /h/.claude/projects/-w-gamma"
    );
    // `order` 与上面这个序逐个同序（界面只按它把几台的组并成一列）。
    let order: Vec<i64> = v["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["order"].as_i64().unwrap())
        .collect();
    assert!(order.windows(2).all(|w| w[0] >= w[1]), "{order:?}");
    assert!(order[2] > order[3], "有星标的压过只是更近的");
}

/// ★ 判据 8：问那一台 ⇒ 经 `remote_listing` 取那台的清单（这台的可达表里没有它 ⇒ `unreachable`，照实说、不凭空答）；
/// `fresh` 不带 `origin` ⇒ `bad_args`；界面当信差的那两格（`raw` · `listing`）不认了 ⇒ `bad_args`。
#[tokio::test]
async fn a_remote_listing_comes_from_that_machine_or_fails_plainly() {
    // 等多久只归发起方：问那一台却没带期限 ⇒ `bad_args`（后端不自己定一个）；带了 ⇒ 照它去问（这台够不着那台 ⇒ `unreachable`）。
    let req: HistoryListReq =
        crate::wire::typed::take(&json!({"origin": "never-reached"})).unwrap();
    let e = super::answer(req.clone(), Default::default(), None)
        .await
        .unwrap_err();
    assert_eq!(e.0, "bad_args", "没带期限却去问了那台：{e:?}");
    assert!(e.1.contains("within_ms"), "没说缺的是哪一格：{e:?}");
    let until = crate::platform::child::Until::after(crate::platform::child::Deadline::secs(5));
    let e = super::answer(req, Default::default(), until)
        .await
        .unwrap_err();
    assert_eq!(e.0, "unreachable", "{e:?}");
    for bad in [
        json!({"fresh": true}),
        json!({"raw": true}),
        json!({"origin": "never-reached", "listing": listing()}),
        json!({"origin": ""}),
    ] {
        let e = answer_wire(bad.clone()).await.unwrap_err();
        assert_eq!(e.0, "bad_args", "{bad} 应是 bad_args：{e:?}");
    }
}

/// ★ 判据 8′：远端那一份只有一处取（`remote_listing.rs`），问的是那台的常驻（`ask_resident`），不是那台的一次性 CLI 面（`ask_json`）。
/// 正控：把一次性那一问塞回去的合成语料要被认出。
#[test]
fn the_remote_listing_asks_the_resident_only() {
    fn cold(prod: &str) -> bool {
        prod.contains("ask_json") || !prod.contains("ask_resident")
    }
    let here =
        guard_core::production_code(include_str!("../../../src/backend/history/history_list.rs"));
    assert!(
        !here.contains("remote_ask"),
        "history-list 自己又去问那台了（只经 remote_listing 一处）"
    );
    let prod = guard_core::production_code(include_str!(
        "../../../src/backend/history/remote_listing.rs"
    ));
    assert!(!cold(&prod), "远端清单又去问那台的一次性 CLI 面了");
    assert!(cold(&format!(
        "{prod}\nfn x() {{ crate::dial::remote_ask::ask_json(); }}"
    )));
}

/// ★ 判据 9：跨语言金样（远端一台的成品，带 context 父会话与读不了的那一组）。
#[test]
fn the_product_matches_the_cross_language_golden() {
    let mut got = patch(
        &Views::new(8),
        &listing(),
        Some("dev"),
        Ok(&ann()),
        &ask(json!({})),
        None,
        1_000,
        &Default::default(),
    );
    // 记号里有进程随机的那一段：金样里换成定值（TS · 手机只存它、原样交回）。
    got["held"] = json!("h1");
    crate::golden::assert_golden(
        "tests/__fixtures__/history-list.golden.json",
        &format!("{}\n", serde_json::to_string_pretty(&got).unwrap()),
        "成品形状变了：TS 解码器读的是同一份金样，两边一起改",
    );
}

/// 注解读不到 / 读不懂 ⇒ `notice` 说一句为什么，行一条不少（星标 · 隐藏按没有算）。
#[test]
fn unreadable_annotations_say_so_and_the_rows_still_come() {
    use crate::history::history_annotations::Loaded;
    let all = listing()["rows"].as_array().map(Vec::len);
    for loaded in [Loaded::NoPath, Loaded::Unreadable("坏了".into())] {
        let v = answer_from(
            &listing(),
            None,
            annotations(&loaded),
            &ask(json!({})),
            1_000,
            &Default::default(),
        );
        assert!(
            v["notice"].as_str().is_some_and(|s| !s.is_empty()),
            "注解读不到却没出声：{v}"
        );
        assert_eq!(
            v["rows"].as_array().map(Vec::len),
            all,
            "注解读不到不许少行：{v}"
        );
    }
    assert_eq!(
        answer(json!({}))["notice"],
        Value::Null,
        "读得到注解时不出声"
    );
}

/// 真记录树：临时家目录里两个记录目录 ⇒ 每行带对记录目录 · 真实目录 · 那一家；判活照给的那一份。
#[test]
fn a_real_record_tree_becomes_the_machine_listing() {
    let home = std::env::temp_dir().join(format!("ccm-hlist-tree-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    for (proj, cwd, sids) in [
        ("-w-alpha", "/w/alpha", vec![S1, S2]),
        ("-w-beta", "/w/beta", vec![S4]),
    ] {
        let d = home.join("projects").join(proj);
        std::fs::create_dir_all(&d).unwrap();
        for s in sids {
            std::fs::write(
                d.join(format!("{s}.jsonl")),
                format!("{{\"cwd\":\"{cwd}\"}}\n"),
            )
            .unwrap();
        }
    }
    let tree = crate::history::listing::sessions_by_dir(&home)
        .expect("读得了")
        .expect("记录树在");
    let live = LiveSet([(S4.to_string(), None)].into_iter().collect());
    let v = listing_from(tree, &[], &live, &json!({}));
    let _ = std::fs::remove_dir_all(&home);
    let agent = crate::agents::record_tree_kind().unwrap_or_default();
    let mut got: Vec<(String, String, String, bool)> = v["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|r| {
            assert_eq!(r["agent"], agent, "记录树那一行是记录树那一家：{r}");
            (
                r["sessionId"].as_str().unwrap_or_default().to_string(),
                r["projectDir"].as_str().unwrap_or_default().to_string(),
                r["projectPath"].as_str().unwrap_or_default().to_string(),
                r["life"]["is"] == "live",
            )
        })
        .collect();
    got.sort();
    let want = |s: &str, d: &str, p: &str, l| (s.to_string(), d.to_string(), p.to_string(), l);
    assert_eq!(
        got,
        vec![
            want(S1, "-w-alpha", "/w/alpha", false),
            want(S2, "-w-alpha", "/w/alpha", false),
            want(S4, "-w-beta", "/w/beta", true),
        ]
    );
}

/// 〔perfC #1〕**按 sid 问清单只整份扫那一份，答案与整台扫逐字相同。**
/// 夹具挑会让「只扫一份」答错的形状：同一记录目录里一份读不出目录（分组取最近那份的目录）· 分叉子会话（父会话在别的目录）·
/// 两个目录里各有一份同名会话 · 一个读不了的目录（分组里那一格 `failed`）。逐个 sid 比两份答案；
/// 再看只扫一份那一形里别的会话真没被整份扫（行里没有记录路径 —— 那一格只有整份扫出来的行才带）。
#[cfg(unix)]
#[test]
fn asking_by_sid_scans_one_session_and_answers_the_same() {
    use std::os::unix::fs::PermissionsExt;
    let home = std::env::temp_dir().join(format!("ccm-hlist-sid-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let put = |dir: &str, name: &str, body: &str| {
        let d = home.join("projects").join(dir);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(format!("{name}.jsonl")), body).unwrap();
    };
    put("-w-alpha", S1, "{\"cwd\":\"/w/alpha\",\"type\":\"user\",\"timestamp\":\"2026-10-01T08:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"甲\"}}\n{\"type\":\"ai-title\",\"aiTitle\":\"甲的标题\"}\n");
    // 读不出目录的那份：归同目录里最近修改的那份的目录。
    put("-w-alpha", S2, "{\"type\":\"user\",\"timestamp\":\"2026-10-01T09:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"乙\"}}\n");
    put("-w-beta", S4, &format!("{{\"cwd\":\"/w/beta\",\"type\":\"user\",\"forkedFrom\":{{\"sessionId\":\"{S1}\",\"messageUuid\":\"m1\"}},\"message\":{{\"role\":\"user\",\"content\":\"丁\"}}}}\n"));
    put("-w-gamma", S1, "{\"cwd\":\"/w/gamma\",\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"同名的另一份\"}}\n");
    let locked = home.join("projects").join("-w-locked");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let live = LiveSet([(S4.to_string(), None)].into_iter().collect());
    let listing = |only: Option<&str>| {
        let tree = tree_for(&home, only).expect("读得了").expect("记录树在");
        listing_from(tree, &[], &live, &json!({ S1: "work" }))
    };
    let whole = listing(None);
    for sid in [S1, S2, S4, "no-such-session"] {
        let a = ask(json!({ "sid": sid }));
        let narrow = listing(Some(sid));
        let want = answer_from(&whole, None, Ok(&ann()), &a, 1_000, &Default::default());
        let got = answer_from(&narrow, None, Ok(&ann()), &a, 1_000, &Default::default());
        assert_eq!(got, want, "按 sid {sid} 问：只扫一份与整台扫答得不一样");
        // 反空真：真有行可比（不在的那个除外），只扫一份那一形里别的会话没被整份扫。
        if sid != "no-such-session" {
            assert!(
                !want["rows"].as_array().unwrap().is_empty(),
                "{sid} 一行都没有，比了个空"
            );
        }
        for r in narrow["rows"].as_array().unwrap() {
            let scanned = r["jsonlPath"].as_str().is_some_and(|p| !p.is_empty());
            // 分叉来的那一条（S4 分自 S1）⇒ 父会话那一份也整份扫（那一行要父会话的显示标题）。
            let parent = sid == S4 && r["sessionId"] == S1;
            assert_eq!(
                scanned,
                r["sessionId"] == sid || parent,
                "只扫一份那一形扫了别的会话：{r}"
            );
        }
    }
    // 读不了的目录照样进分组（`failed` 那一格）。
    assert!(
        whole["failed"].as_array().is_some_and(|f| !f.is_empty()),
        "夹具没造出读不了的目录"
    );
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 注解跟着会话住在那台：一个远端会话，桌面经本机后端（`chan.call(那台, history-annotate)`）标星 ⇒ 写进**那台**的注解文件；
/// 再读：直连那台的 history-list（`origin` 缺席）与本机后端代问那台的 history-list（`origin` = 那台）读到的是同一个星。
/// 并的那张表只从清单自己带来的那份取（[`listing_annotations`]）：那台没标的 S2 读出来就是没标。
#[test]
fn a_remote_sessions_annotation_lives_on_that_machine() {
    let dir = std::env::temp_dir().join(format!("ccm-hl-ann-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let there = dir.join("history-metadata.json");
    crate::history::history_annotations::annotate_at(
        &crate::platform::fs::Own::issue_data_at(&dir),
        crate::wire::typed::take(&json!({"sid": S1, "patch": {"starred": true}})).expect("入参"),
        5,
    )
    .expect("那台写下");
    // 那台出自己的清单（`history-listing` 答的那一份）：自己并上自己那份注解。
    let mut raw = listing();
    with_own_annotations(
        &mut raw,
        &crate::history::history_annotations::load_at(&there),
    );
    let starred = |v: &Value, sid: &str| {
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["sessionId"] == sid)
            .map(|r| r["starred"].clone())
    };
    let direct = answer_from(
        &raw,
        None,
        listing_annotations(&raw).as_ref().map_err(Clone::clone),
        &ask(json!({})),
        1_000,
        &crate::Tz::default(),
    );
    let via_here = answer_from(
        &raw,
        Some("devbox"),
        listing_annotations(&raw).as_ref().map_err(Clone::clone),
        &ask(json!({})),
        1_000,
        &crate::Tz::default(),
    );
    assert_eq!(starred(&direct, S1), Some(json!(true)), "直连那台读到星");
    assert_eq!(
        starred(&via_here, S1),
        starred(&direct, S1),
        "经本机后端读到的与直连那台一样"
    );
    assert_eq!(
        starred(&via_here, S2),
        Some(json!(false)),
        "那台没标的就是没标"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// ★ 手机读的这一条，线上字节钉住：换成带键集合那一形时重钉过一回（记号换成定值 `h1`）；字节存在 `tests/__fixtures__/history-list.wire-bytes.txt`（每行 `名字<TAB>字节`），
/// 之后逐字节比（经 `serde_json::Value` 那一跳，同帧面）。没有重写开关：字节变了就是线上形状变了，先问手机那一侧。
/// A 手机那一问（不带 `origin`）· B 注解读不到 ＋ 搜索词 · C 按开始排 ＋ 一窗 ＋ 隐藏的也出。
#[test]
fn the_wire_bytes_the_phone_reads_stay_the_same() {
    use crate::history::history_annotations::Loaded;
    let pinned: BTreeMap<String, String> =
        std::fs::read_to_string(fixtures().join("history-list.wire-bytes.txt"))
            .expect("钉住的字节")
            .lines()
            .map(|l| {
                let (n, b) = l.split_once('\t').expect("名字<TAB>字节");
                (n.to_string(), b.to_string())
            })
            .collect();
    let wire = |v: Value| serde_json::to_string(&v).unwrap();
    let tz = crate::Tz::default();
    let pin = |mut v: Value| {
        v["held"] = json!("h1");
        v
    };
    let whole = |ann: Result<&Table, String>, args: Value| {
        pin(patch(
            &Views::new(8),
            &listing(),
            None,
            ann,
            &ask(args),
            None,
            1_000,
            &tz,
        ))
    };
    let a = whole(Ok(&ann()), json!({}));
    let b = whole(annotations(&Loaded::NoPath), json!({"query": "回调"}));
    let c = whole(
        Ok(&ann()),
        json!({"range": {"from": 0, "limit": 2}, "sort": "created", "hidden": true}),
    );
    let got: BTreeMap<String, String> = [("A", a), ("B", b), ("C", c)]
        .into_iter()
        .map(|(n, v)| (n.to_string(), wire(v)))
        .collect();
    assert_eq!(got.len(), pinned.len(), "钉住的条数对不上");
    for (n, want) in &pinned {
        assert_eq!(got.get(n), Some(want), "`{n}` 那一份的线上字节变了");
    }
    // 手机嵌的 Rust 核心按 `cmd-wire` 那一份（`HistoryList`）解每一份：解得开、按类型重写出来逐字节相同。
    for (n, b) in &got {
        let v: Value = serde_json::from_str(b).unwrap();
        match cmd_wire::replies::decode_reply("history-list", v) {
            Some(Ok(back)) => assert_eq!(&wire(back), b, "`{n}` 重写出来不一样"),
            other => panic!("`{n}`：手机解不开后端写的那一份：{other:?}"),
        }
    }
}

/// 隐藏的判定照那一台的事实（那台清单带来的 `--tab` · 主会话 · 偏好）：子会话按偏好隐藏、悬停那一句说主会话叫什么（照清单里它那一行）；
/// 单个那一格「取消隐藏」压过规则；不隐藏的那一行不带这一格。
#[test]
fn rows_carry_the_hidden_cell_judged_from_that_machines_facts() {
    let mut l: Listing = serde_json::from_value(listing()).unwrap();
    for r in &mut l.rows {
        if r.session_id == S3 {
            r.parent = Some(S1.to_string());
        }
        if r.session_id == S5 {
            r.launch_tab = Some(crate::control::launch_account::LaunchTab {
                tab: cmd_wire::session::Tab::Hide,
                alias: Some("cct".into()),
            });
        }
    }
    let run = |l: &Listing, ann: &Table| {
        answer_from(
            &serde_json::to_value(l).unwrap(),
            None,
            Ok(ann),
            &ask(json!({"hidden": true})),
            1_000,
            &Default::default(),
        )
    };
    let v = run(&l, &ann());
    let s1_label = find(&v, S1)["label"].as_str().unwrap().to_string();
    assert_eq!(find(&v, S3)["hidden"]["by"], "child");
    assert_eq!(
        find(&v, S3)["hidden"]["said"],
        copy_core::copy_text("beSession.hidden.child", &[("parent", &s1_label)])
    );
    assert_eq!(find(&v, S5)["hidden"]["by"], "alias");
    assert_eq!(
        find(&v, S1).get("hidden"),
        Some(&serde_json::Value::Null),
        "不隐藏的那一行那一格得是 null（格恒在）"
    );
    // 默认清单里没有它们（隐藏的不进）。
    let plain = answer_from(
        &serde_json::to_value(&l).unwrap(),
        None,
        Ok(&ann()),
        &ask(json!({})),
        1_000,
        &Default::default(),
    );
    assert!(!sids(&plain).contains(&S3.to_string()));
    // 那台偏好子会话显示 ⇒ 不隐藏；单个那一格写了「取消隐藏」⇒ 别名那一格也不算。
    l.rules.child_tab = cmd_wire::session::Tab::Show;
    let mut a = ann();
    a.insert(
        S5.to_string(),
        Entry {
            hide: Some(crate::history::history_annotations::Hide::Off),
            ..Entry::default()
        },
    );
    let v = run(&l, &a);
    assert!(find(&v, S3)["hidden"].is_null());
    assert!(find(&v, S5)["hidden"].is_null());
}

/// 不在跑、而会话账本说它可重连（tmux 里还挂着它）⇒ 那一行 `reconnectable`、状态 `reconnectable`；在跑的照 `live`；
/// 账本没说的照 `ended`。
#[test]
fn a_session_the_ledger_calls_reconnectable_is_listed_as_reconnectable() {
    let sid = "9e9e9e9e-0000-4000-8000-000000000001";
    let other = "9e9e9e9e-0000-4000-8000-000000000002";
    let v = |s: &str| json!({"sessionId": s, "cwd": "/w/a", "jsonlPath": "/x.jsonl"});
    let none = LiveSet(Default::default());
    crate::history::live::set_reconnectable(&[sid.to_string()].into_iter().collect());
    let r = session_row(&v(sid), "-w-a", &none).unwrap();
    let o = session_row(&v(other), "-w-a", &none).unwrap();
    let live = session_row(
        &v(sid),
        "-w-a",
        &LiveSet([(sid.to_string(), None)].into_iter().collect()),
    )
    .unwrap();
    crate::history::live::set_reconnectable(&Default::default());
    assert_eq!(
        [&r, &o, &live].map(|x| x.life),
        [
            Some(RowLife::Reconnectable),
            Some(RowLife::Ended),
            Some(RowLife::Live { activity: None })
        ]
    );
}

/// ★ 分叉来的那一行带父会话的显示标题（同父会话自己那一行的 `label`，父会话被筛掉 / 隐藏也照它）；父会话不在这台的清单里 ⇒ 缺；
/// 不是分叉来的 ⇒ 缺。按项目看的每一组带最近活动写好的字（同行尾那一格的写法）；读不了的那一组 ⇒ `null`。
#[test]
fn a_forked_row_names_its_parent_and_groups_say_when() {
    let tz = crate::Tz::default();
    let v = answer(json!({"query": "分出来"}));
    assert_eq!(
        find(&v, S5)["forkedFrom"],
        json!({"label": find(&answer(json!({"hidden": true})), S4)["label"]})
    );
    assert!(find(&v, S4).get("forkedFrom").is_none(), "不是分叉来的");
    let mut l = listing();
    let rows = l["rows"].as_array_mut().unwrap();
    rows.retain(|r| r["sessionId"] != S4);
    let orphan = answer_from(&l, None, Ok(&ann()), &ask(json!({})), 1_000, &tz);
    assert!(
        find(&orphan, S5).get("forkedFrom").is_none(),
        "父会话不在这台的清单里 ⇒ 缺"
    );
    let all = answer(json!({"hidden": true}));
    for g in all["groups"].as_array().unwrap() {
        let want = if g["failed"].is_null() {
            json!(
                crate::common::time::history_at_text(
                    g["lastActivity"].as_i64().unwrap(),
                    1_000,
                    &tz
                )
                .0
            )
        } else {
            Value::Null
        };
        assert_eq!(g["atText"], want, "{g}");
    }
}

/// ★ 判据 11：带记号再问。同一问第二次 ⇒ `same`；一条会话写了一行 ⇒ `up` 只它一行；跳到最前 ⇒ 另带 `order`；换问法 ⇒ `whole`。
#[test]
fn asking_again_with_the_mark_answers_only_what_changed() {
    let views = Views::new(8);
    let tz = crate::Tz::default();
    let go = |l: &Value, args: Value, held: Option<&Value>| {
        patch(&views, l, None, Ok(&ann()), &ask(args), held, 1_000, &tz)
    };
    let l = listing();
    let first = go(&l, json!({}), None);
    assert_eq!(first["whole"], true, "头一问 ⇒ 整份");
    let again = go(&l, json!({}), Some(&first["held"]));
    assert_eq!(again["same"], true, "什么都没变 ⇒ same：{again}");
    assert_eq!(again["up"], json!({}));
    // S3 写了一行：最后活动 700 → 750、条数多一条（仍排在 S1 之后、codex 之前 ⇒ 先后不变）。
    let mut wrote = l.clone();
    for r in wrote["rows"].as_array_mut().unwrap() {
        if r["sessionId"] == S3 {
            r["updatedAt"] = json!(750);
            r["messageCountApprox"] = json!(4);
        }
    }
    let one = go(&wrote, json!({}), Some(&again["held"]));
    assert_eq!(one["whole"], false);
    assert_eq!(one["same"], false);
    let keys: Vec<&String> = one["up"].as_object().unwrap().keys().collect();
    assert_eq!(keys, vec![S3], "只它一行：{one}");
    assert_eq!(one["up"][S3]["messageCountApprox"], 4);
    assert!(one["order"].is_null(), "先后没变不带 order");
    // 正控：它跳到最前（最后活动最新）⇒ 照样只它一行，另带新的先后。
    let mut top = wrote.clone();
    for r in top["rows"].as_array_mut().unwrap() {
        if r["sessionId"] == S3 {
            r["updatedAt"] = json!(2_000);
        }
    }
    let moved = go(&top, json!({}), Some(&one["held"]));
    let keys: Vec<&String> = moved["up"].as_object().unwrap().keys().collect();
    assert_eq!(keys, vec![S3]);
    assert_eq!(moved["order"][0], S3, "带了新的先后");
    // 换问法（排法 · 搜索词 · 隐藏的也出）⇒ 整份，哪怕带着记号。
    for other in [
        json!({"sort": "created"}),
        json!({"query": "回调"}),
        json!({"hidden": true}),
    ] {
        let p = go(&top, other.clone(), Some(&moved["held"]));
        assert_eq!(p["whole"], true, "{other} ⇒ 整份");
    }
}

/// ★ 判据 11（帧面那一臂）：带着上次的 `held` 来问 ⇒ 没变就 `same`；入参里的 `held` 形状不对 ⇒ `bad_args`。
#[tokio::test]
async fn the_frame_arm_takes_the_mark() {
    // 问这台（远端那份怎么来归 `remote_listing_tests`）：判据环境里这台的记录树不动 ⇒ 第二问没变。
    let first = answer_wire(json!({})).await.unwrap();
    assert_eq!(first["whole"], true);
    let again = answer_wire(json!({"held": first["held"]}))
        .await
        .unwrap();
    assert_eq!(again["same"], true, "{again}");
    let e = answer_wire(json!({"held": 7}))
        .await
        .unwrap_err();
    assert_eq!(e.0, "bad_args");
}
