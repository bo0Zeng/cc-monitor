//! 历史页平铺清单（`history-list`）的判据。
//!
//! # 守的要求（住址）
//!
//! 设计稿「文件与历史」乙7 ①②④：一份跨项目的平铺清单，每行带显示标题（没标题没第一句也给一个）· 真实目录分组 · 机器 · 那一家 ·
//! 状态 · 注解 · 分叉父子 · 分身 · 条数 · 时间，以及「这一行能做什么」；标题 / 第一句 / 项目名的搜索在后端；远端每台由那台出、本机并注解。
//!
//! # 判据
//!
//! 1. **这台的清单**：记录树各目录的行带 `projectDir` / `group`（按真实目录）· 读不了的目录进 `failed` · 合成历史按 `<kind>:<cwd>` · 上次的号逐 sid。
//! 2. **注解并的是这台的**（远端行也并）；`label` = 改过的标题 ＞ 标题 ＞ 第一句；都没有 ⇒ `untitled`。
//! 3. **能做什么**：在跑 ⇒ 恢复 `switch`、删 `live`；分身 ⇒ `bg`、不能分叉；判不了活 ⇒ 删 `unsure`；没有账号维的那一家 ⇒ `accounts:false`。
//! 4. **筛**：隐藏的默认不出（`hidden:true` 才出）· 时间窗 · 搜索词只比显示标题 / 第一句 / 项目名、不分大小写。
//! 5. **分叉父会话被筛掉** ⇒ 照样带上、标 `context`，不算进 `total`。
//! 6. **排与截**：`activity` 按最后活动、`created` 按开始，`at` 就是那个键；`limit` 截 ⇒ `truncated`。
//! 7. **分组**：有在跑的 → 有星标的 → 最近动过的；读不了的目录是一组、带原因。
//! 8. **远端**：问那台的 `--history-list`（`raw`）一次，之后用记着的；`fresh` 再问；够不到 ⇒ `unreachable`。
//! 10. **按会话 ID 要一行**（`sid`：独立查看窗开任意一个会话）：只回那一行（隐藏的、出了时间窗的也回）、不补父会话；
//!     形状不对 ⇒ `bad_args`（先于 IO）；没有这个会话 ⇒ 空清单。
//! 9. **跨语言金样** `tests/__fixtures__/history-list.golden.json`（TS 严格解码器读同一份，`tests/frontend/ui/history-list-reads.vitest.ts`）。
//!
//! # 买不到
//!
//! - 🔴 真远端（`DialRemote` 那一跳）· 真盘上上万份记录时的耗时。

use super::*;
use crate::dial::remote_ask::{Remote, Table as ReachTable};
use crate::history::history_annotations::{Entry, Table};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Mutex;

fn fixtures() -> PathBuf {
    crate::guard_support::repo_root().join("tests/__fixtures__")
}

/// 这台的注解：S1 星标 · S2 隐藏 · S4 星标 ＋ 隐藏（S3 S5 没有注解）。
fn ann() -> Table {
    let e = |starred, hidden| Entry {
        starred,
        hidden,
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
    let live = LiveSet([S1.to_string()].into_iter().collect());
    listing_from(tree, &synth, &live, &json!({ S1: "work", S3: "home" }))
}

fn ask(args: Value) -> Ask {
    parse_ask(&args).expect("入参")
}

fn answer(args: Value) -> Value {
    answer_from(&listing(), None, Ok(&ann()), &ask(args), 1_000, &|t| t)
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
        cx["isLive"],
        Value::Null,
        "合成历史判不了活 ⇒ null，不是 false"
    );
    assert_eq!(
        answer(json!({}))["rows"][0].get("isLive"),
        None,
        "成品里活不活只出 status 一格"
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
    assert_eq!(s1["status"], "live");
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
    assert_eq!(cx["status"], "unknown");
    assert_eq!(cx["agentTag"], "Codex", "不是默认那一家 ⇒ 行上画它的叫法");
    assert_eq!(s1["agentTag"], Value::Null, "默认那一家不画");
    assert_eq!(
        cx["can"],
        json!({"resume": "yes", "accounts": false, "fork": false, "delete": "unsure"})
    );
    // 改过的标题压过原标题（正控：注解里 S1 没改名、label 是原标题；这里给它一条改名）。
    let mut t = ann();
    t.get_mut(S1).unwrap().custom_title = Some("改过的".into());
    let w = answer_from(&listing(), None, Ok(&t), &ask(json!({})), 1_000, &|t| t);
    assert_eq!(find(&w, S1)["label"], "改过的");
    assert_eq!(find(&w, S1)["customTitle"], "改过的");
}

/// ★ 判据 4 ＋ 5：筛（隐藏 · 时间 · 搜索词）与被筛掉的分叉父会话。
#[test]
fn filters_and_context_parents() {
    let v = answer(json!({}));
    assert!(!sids(&v).contains(&S2.to_string()), "隐藏的默认不出");
    // S4 星标 ＋ 隐藏、S5 是它分出来的 ⇒ S4 照样带上、标 context，不算进 total。
    assert_eq!(find(&v, S4)["context"], true);
    assert_eq!(v["total"], 4, "S1 S3 S5 codex；S4 是 context 不计");
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
        &|t| t,
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
    assert_eq!(find(&v, S2)["hidden"], true);
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

/// ★ 判据 6：排与截。
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
    let v = answer(json!({"hidden": true, "limit": 2}));
    assert_eq!(v["rows"].as_array().unwrap().len(), 2);
    assert_eq!(v["truncated"], true);
    assert_eq!(v["total"], 6);
    for bad in [
        json!({"sort": "x"}),
        json!({"limit": 0}),
        json!({"within_days": "7"}),
        json!({"query": 1}),
        json!({"hidden": "y"}),
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

/// 对面：数被问了几次、问的是什么；答 `raw` 那一份。
#[derive(Default)]
struct Far {
    seen: Mutex<Vec<String>>,
}

impl Remote for Far {
    fn run<'a>(
        &'a self,
        _dial: &'a Value,
        command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        self.seen.lock().unwrap().push(format!(
            "{command} <stdin {}>",
            stdin.unwrap_or_default().trim_end()
        ));
        let out = listing().to_string();
        Box::pin(async move { Ok(out) })
    }
}

fn reach(table: &ReachTable, origin: &str) {
    crate::dial::remote_ask::answer_reach_with(
        &json!({"origin": origin, "dial": {"machine": {"host": "h", "port": 22, "user": "u", "keyPath": "/k"}}}),
        table,
    )
    .unwrap();
}

/// ★ 判据 8：远端问那台 `raw` 一次，之后用记着的，`fresh` 再问；行与组带 `origin`；够不到 ⇒ `unreachable`、一次都不问。
#[tokio::test]
async fn a_remote_is_asked_raw_once_and_cached_until_fresh() {
    let table = ReachTable::default();
    // 每个判据用自己的机器名（缓存是进程级的一张表）。
    reach(&table, "list-dev");
    let far = Far::default();
    let v = answer_with(json!({"origin": "list-dev"}), &table, &far)
        .await
        .unwrap();
    assert!(v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["origin"] == "list-dev"));
    assert!(
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| ["atText", "sectionText", "spanText"]
                .iter()
                .all(|k| r[*k].as_str().is_some_and(|t| !t.is_empty()))),
        "帧面那一臂每行都带行尾 · 分段 · 时间段三格（这台本地钟写好）：{v}"
    );
    assert!(v["groups"]
        .as_array()
        .unwrap()
        .iter()
        .all(|g| g["origin"] == "list-dev"));
    let _ = answer_with(json!({"origin": "list-dev", "query": "回调"}), &table, &far)
        .await
        .unwrap();
    assert_eq!(
        *far.seen.lock().unwrap(),
        vec![format!(
            "{} <stdin {{\"raw\":true}}>",
            crate::dial::remote_ask::command_line(&["--history-list", "--stdin-line"])
        )],
        "敲字搜索不该每次都去那台整份扫"
    );
    let _ = answer_with(json!({"origin": "list-dev", "fresh": true}), &table, &far)
        .await
        .unwrap();
    assert_eq!(far.seen.lock().unwrap().len(), 2, "刷新 ⇒ 再问一次");
    let e = answer_with(json!({"origin": "list-nowhere"}), &table, &far)
        .await
        .unwrap_err();
    assert_eq!(e.0, "unreachable");
    assert_eq!(far.seen.lock().unwrap().len(), 2);
    let e = answer_with(json!({"origin": ""}), &table, &far)
        .await
        .unwrap_err();
    assert_eq!(e.0, "bad_args");
}

/// ★ 判据 9：跨语言金样（远端一台的成品，带 context 父会话与读不了的那一组）。`CCM_BLESS=1` 重写。
#[test]
fn the_product_matches_the_cross_language_golden() {
    let got = answer_from(
        &listing(),
        Some("dev"),
        Ok(&ann()),
        &ask(json!({})),
        1_000,
        &|t| t,
    );
    let path = fixtures().join("history-list.golden.json");
    if std::env::var_os("CCM_BLESS").is_some() {
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&got).unwrap()),
        )
        .unwrap();
    }
    let golden: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("金样")).unwrap();
    assert_eq!(
        got, golden,
        "成品形状变了：TS 解码器读的是同一份金样，两边一起改（CCM_BLESS=1 重写）"
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
            &|t| t,
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
    let tree = crate::observe::history_query::sessions_by_dir(&home)
        .expect("读得了")
        .expect("记录树在");
    let live = LiveSet([S4.to_string()].into_iter().collect());
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
                r["isLive"].as_bool().unwrap_or_default(),
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
    let live = LiveSet([S4.to_string()].into_iter().collect());
    let listing = |only: Option<&str>| {
        let tree = crate::observe::history_query::sessions_by_dir_for(&home, only)
            .expect("读得了")
            .expect("记录树在");
        listing_from(tree, &[], &live, &json!({ S1: "work" }))
    };
    let whole = listing(None);
    for sid in [S1, S2, S4, "no-such-session"] {
        let a = ask(json!({ "sid": sid }));
        let narrow = listing(Some(sid));
        let want = answer_from(&whole, None, Ok(&ann()), &a, 1_000, &|t| t);
        let got = answer_from(&narrow, None, Ok(&ann()), &a, 1_000, &|t| t);
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
            assert_eq!(
                scanned,
                r["sessionId"] == sid,
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
    crate::history::history_annotations::answer_annotate_at(
        &there,
        &json!({"sid": S1, "patch": {"starred": true}}),
        5,
    )
    .expect("那台写下");
    // 那台出自己的清单（raw 那一形）：自己并上自己那份注解。
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
        &|t| t,
    );
    let via_here = answer_from(
        &raw,
        Some("devbox"),
        listing_annotations(&raw).as_ref().map_err(Clone::clone),
        &ask(json!({})),
        1_000,
        &|t| t,
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
