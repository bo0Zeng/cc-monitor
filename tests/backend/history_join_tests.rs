//! 〔C4d · 第四波 4B〕历史跨机 join（本机常驻后端出成品）的判据。
//!
//! # 守的要求（住址）
//!
//! 主会话 09-25 裁（`调研/第四波记录/C4d.md`「主会话裁」第 2 条，逐字）：「本机后端经 `remote_ask` 问远端那台的项目 / 会话清单、
//! 并上注解、出成品；前端经 `chan.call`。codex 合成的项目与会话一起进后端（join 只一个家）」。
//! `K-R83` / `K-R92`（从 monitor `remote_history_kr83_tests.rs` 搬来的那几条性质）：「不知道」不许与「真的是 0」长成一个样。
//!
//! # 判据
//!
//! 1. **并上的是那一份注解**：项目的星标数 / 隐藏数 == 按注解夹具逐 sid 数出来的（异源：期望由测试从夹具自己数）；
//!    注解读不到 ⇒ 两个数「不知道」（`null`）＋ `notice` 说为什么，**不是 0**。
//! 2. **「不知道」三形**（`K-R83`）：那一行没带 sid 清单 · 清单与条数对不上 ⇒ 三个数全 `null`；远端 / 合成历史判活 ⇒ `hasLive: null`。
//! 3. **本机判活有真值**：真相源说活 ⇒ `true`、说没活 ⇒ `false`（不是 `null`）。
//! 4. **一台只问一次**（`KR83D3` 的形状）：远端项目清单 N 个项目 ⇒ 对面恰被问 1 次、问的是 `--list-projects`；会话清单问的是 `--list-sessions <dir>`。
//! 5. **够不到就说**：可达表里没有那一台 ⇒ `unreachable`，对面一次都没被问；远端的项目目录名同样过形状闸。
//! 6. **合成历史**：`<kind>:<cwd>` 分组、名字、会话行逐格；**跨语言金样** `tests/__fixtures__/history-products.golden.json`
//!    （远端项目 · 远端会话 · 合成会话三份成品 —— TS 解码器读同一份，`tests/history-reads.vitest.ts`）。
//!
//! # 买不到
//!
//! - 🔴 真远端（`DialRemote` 那一跳）· 真 Windows 上的 pidfile 判活。

use super::*;
use crate::history_annotations::{load_at, Entry};
use crate::remote_ask::{Remote, Table as ReachTable};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Mutex;

fn fixtures() -> PathBuf {
    crate::guard_support::repo_root().join("tests/__fixtures__")
}

fn ann() -> Loaded {
    load_at(&fixtures().join("history-metadata.fixture.json"))
}

const S1: &str = "0000aaaa-0000-4000-8000-000000000001"; // 星标
const S2: &str = "0000aaaa-0000-4000-8000-000000000002"; // 隐藏
const S3: &str = "0000aaaa-0000-4000-8000-000000000003"; // 空注解
const S4: &str = "0000aaaa-0000-4000-8000-000000000004"; // 星标 ＋ 隐藏

/// 远端 `--list-projects` 的三行（结构占位）＋ 一行没有 `dirName` 的（跳过）。
fn projects_stdout() -> String {
    [
        format!(r#"{{"dirName":"-w-alpha","projectPath":"/w/alpha","sessionCount":4,"lastActivityMs":1000,"sessionIds":["{S1}","{S2}","{S3}","{S4}"]}}"#),
        r#"{"dirName":"-w-beta","projectPath":"","sessionCount":1,"lastActivityMs":2000}"#.to_string(),
        r#"{"dirName":"-w-gamma","projectPath":"C:\\w\\gamma","sessionCount":2,"lastActivityMs":500,"sessionIds":["x"]}"#.to_string(),
        r#"{"projectPath":"/no/dir","sessionCount":1}"#.to_string(),
        String::new(),
    ]
    .join("\n")
}

/// 远端 `--list-sessions -w-alpha` 的两行（一行带 fork 关系）。
fn sessions_stdout() -> String {
    [
        format!(r#"{{"sessionId":"{S1}","jsonlPath":"/h/.claude/projects/-w-alpha/{S1}.jsonl","startedAtMs":10,"updatedAtMs":20,"messageCountApprox":7,"firstUserExcerpt":"占位","aiTitle":"占位标题","cwd":"/w/alpha","isBg":false,"forkedFromSessionId":"{S3}","forkedFromMessageUuid":"m-1"}}"#),
        format!(r#"{{"sessionId":"{S4}","jsonlPath":"/h/.claude/projects/-w-alpha/{S4}.jsonl","startedAtMs":30,"updatedAtMs":40,"messageCountApprox":1,"firstUserExcerpt":"","aiTitle":null,"cwd":null,"isBg":true,"forkedFromSessionId":null,"forkedFromMessageUuid":null}}"#),
    ]
    .join("\n")
}

fn synth_fixture() -> crate::agents::SynthSession {
    crate::agents::SynthSession {
        sid: "019a0000-0000-7000-8000-00000000c0de".into(),
        path: PathBuf::from("/h/.codex/sessions/2026/09/25/rollout-x.jsonl"),
        cwd: "/w/delta".into(),
        mtime_ms: 777,
    }
}

/// ★ 判据 1 ＋ 2：远端项目成品 —— 注解逐 sid 数、三种「不知道」、没有 `dirName` 的跳过、排序。
#[test]
fn remote_projects_carry_the_annotation_counts_and_say_unknown_honestly() {
    let v = remote_projects_from("dev", &projects_stdout(), &ann(), &NoLiveness).unwrap();
    let rows = v["rows"].as_array().unwrap();
    let dirs: Vec<&str> = rows
        .iter()
        .map(|r| r["projectDir"].as_str().unwrap())
        .collect();
    assert_eq!(
        dirs,
        vec!["-w-alpha", "-w-beta", "-w-gamma"],
        "有星标的在前；其余按最近动过排（没有 dirName 的那行跳过）"
    );
    // 期望由测试从注解夹具自己数（S1 星标、S2 隐藏、S3 空、S4 星标＋隐藏）。
    let t = match ann() {
        Loaded::Read(t) => t,
        other => panic!("{other:?}"),
    };
    let want_star = [S1, S2, S3, S4]
        .iter()
        .filter(|s| t.get(**s).is_some_and(|e: &Entry| e.starred))
        .count();
    let want_hidden = [S1, S2, S3, S4]
        .iter()
        .filter(|s| t.get(**s).is_some_and(|e: &Entry| e.hidden))
        .count();
    assert_eq!(rows[0]["starredCount"], want_star);
    assert_eq!(rows[0]["hiddenCount"], want_hidden);
    assert_eq!(
        rows[0]["hasLive"],
        Value::Null,
        "远端判活是「不知道」，不是 false"
    );
    assert_eq!(rows[0]["origin"], "dev");
    assert_eq!(rows[0]["projectName"], "alpha");
    // 没带 sid 清单 / 清单对不上 ⇒ 三个数全 null（不是 0）。
    for r in &rows[1..] {
        assert_eq!(r["starredCount"], Value::Null, "{r}");
        assert_eq!(r["hiddenCount"], Value::Null, "{r}");
        assert_eq!(r["hasLive"], Value::Null, "{r}");
    }
    assert_eq!(
        rows[1]["projectName"], "-w-beta",
        "拿不到 cwd ⇒ 名字退回目录名"
    );
    assert_eq!(rows[2]["projectName"], "gamma", "反斜杠路径也取最后一段");
    assert_eq!(v["notice"], Value::Null);
}

/// ★ 判据 1（反向）：注解读不到 ⇒ 星标数 / 隐藏数「不知道」＋ `notice` 说为什么；会话行的星标照旧是 false（那一格装不下第三态）。
#[test]
fn unreadable_annotations_are_unknown_not_zero() {
    for loaded in [Loaded::NoPath, Loaded::Unreadable("坏了".into())] {
        let v = remote_projects_from("dev", &projects_stdout(), &loaded, &NoLiveness).unwrap();
        let alpha = v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["projectDir"] == "-w-alpha")
            .expect("alpha 那一行没了");
        assert_eq!(alpha["starredCount"], Value::Null, "{loaded:?}");
        assert_eq!(alpha["hiddenCount"], Value::Null, "{loaded:?}");
        assert!(
            v["notice"].as_str().is_some_and(|s| !s.is_empty()),
            "{loaded:?} 没出声"
        );
    }
}

/// 替身判活：答得出真值的那一个。
struct Says(&'static [&'static str]);

impl Liveness for Says {
    fn is_live(&self, sid: &str) -> Counted<bool> {
        Counted::Known(self.0.contains(&sid))
    }
}

/// ★ 判据 3：本机判活有真值（活 ⇒ true；都没活 ⇒ false，不是 null）；有一个答不出就不许说「都没活」。
#[test]
fn local_liveness_answers_true_and_false_and_unknown_is_its_own_bucket() {
    let row: Value = serde_json::from_str(projects_stdout().lines().next().unwrap()).unwrap();
    let t = match ann() {
        Loaded::Read(t) => t,
        other => panic!("{other:?}"),
    };
    let (p, _) = project_from_row(&row, Some(&t), None, &Says(&[S2])).unwrap();
    assert_eq!(p["hasLive"], true);
    assert!(
        p.get("origin").is_none(),
        "本机那一行不带 origin（线上从前就是省略）"
    );
    let (p, _) = project_from_row(&row, Some(&t), None, &Says(&[])).unwrap();
    assert_eq!(p["hasLive"], false, "查过了、都没活 ⇒ false");
    // 混着答不出的：不许被「其余都没活」盖成 false。
    struct Mixed;
    impl Liveness for Mixed {
        fn is_live(&self, sid: &str) -> Counted<bool> {
            if sid == S3 {
                Counted::Unknown(WhyUnknown::NoLivenessOracle)
            } else {
                Counted::Known(false)
            }
        }
    }
    let c = counts_over(&[S1, S3, S4], Some(&t), &Mixed);
    assert_eq!(c.has_live, Counted::Unknown(WhyUnknown::NoLivenessOracle));
    let c = counts_over(&[S3, S1], Some(&t), &Says(&[S1]));
    assert_eq!(c.has_live, Counted::Known(true), "一个确定活着的就够了");
}

/// 记账的替身对面：记下每次被交的命令，按子命令答一份 stdout。
#[derive(Default)]
struct Far {
    seen: Mutex<Vec<String>>,
    /// 〔GAP1〕`--session-accounts` 答什么；`None` ⇒ 那一问失败。
    live: Option<String>,
}

impl Remote for Far {
    fn run<'a>(
        &'a self,
        _dial: &'a Value,
        command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        assert_eq!(stdin, None, "历史跨机那几问全在 argv 里，不写 stdin");
        self.seen.lock().unwrap().push(command.clone());
        let out = if command.contains("--list-projects") {
            projects_stdout()
        } else if command.contains("--session-accounts") {
            match &self.live {
                Some(o) => o.clone(),
                None => return Box::pin(async { Err("那台问不到".to_string()) }),
            }
        } else {
            sessions_stdout()
        };
        Box::pin(async move { Ok(out) })
    }
}

fn reach(table: &ReachTable) {
    crate::remote_ask::answer_reach_with(
        &json!({"origin": "dev", "dial": {"host": "h", "port": 22, "user": "u", "key_path": "/k"}}),
        table,
    )
    .unwrap();
}

/// ★ 判据 4：一台只问一次，问的是 CLI 老子命令（远端不必升级）；会话那一问带着项目目录名（逐格引号）。
#[tokio::test]
async fn one_remote_is_asked_exactly_once_with_the_old_subcommands() {
    let table = ReachTable::default();
    reach(&table);
    let far = Far::default();
    let v = answer_projects_with(json!({"origin": "dev"}), &table, &far)
        .await
        .unwrap();
    assert_eq!(v["rows"].as_array().unwrap().len(), 3);
    assert_eq!(
        *far.seen.lock().unwrap(),
        vec![
            crate::remote_ask::command_line(&["--list-projects"]),
            crate::remote_ask::command_line(&["--session-accounts"]),
        ],
        "N 个项目 ⇒ 对面恰被问一次清单 ＋ 一次判活（为了拿星标补问 --list-sessions 就会在这里红）"
    );
    far.seen.lock().unwrap().clear();
    let v = answer_sessions_with(
        json!({"origin": "dev", "project_dir": "-w-alpha"}),
        &table,
        &far,
    )
    .await
    .unwrap();
    assert_eq!(v["rows"].as_array().unwrap().len(), 2);
    assert_eq!(
        *far.seen.lock().unwrap(),
        vec![
            crate::remote_ask::command_line(&["--list-sessions", "-w-alpha"]),
            crate::remote_ask::command_line(&["--session-accounts"]),
        ]
    );
}

/// ★ 判据 5：够不到就说；项目目录名过形状闸（远端那一支也过）；`origin` 空串拒。
#[tokio::test]
async fn unreachable_or_malformed_requests_are_refused_without_asking() {
    let table = ReachTable::default();
    let far = Far::default();
    let e = answer_projects_with(json!({"origin": "nowhere"}), &table, &far)
        .await
        .unwrap_err();
    assert_eq!(e.0, "unreachable");
    assert!(e.1.contains("[nowhere]"), "{e:?}");
    reach(&table);
    for bad in ["../x", "a/b", "a\\b", ""] {
        let e = answer_sessions_with(json!({"origin": "dev", "project_dir": bad}), &table, &far)
            .await
            .unwrap_err();
        assert_eq!(e.0, "bad_args", "{bad}");
    }
    let e = answer_projects_with(json!({"origin": ""}), &table, &far)
        .await
        .unwrap_err();
    assert_eq!(e.0, "bad_args");
    assert!(far.seen.lock().unwrap().is_empty(), "拒了还去问了");
}

/// ★ 判据 6：合成历史按 cwd 分组、`<kind>:<cwd>` 当键、空 cwd 归「(<kind>)」；星标 / 隐藏按注解数；判活「不知道」。
#[test]
fn synthesized_history_groups_by_cwd_under_the_kind_prefix() {
    let t = match ann() {
        Loaded::Read(t) => t,
        other => panic!("{other:?}"),
    };
    let mk = |sid: &str, cwd: &str, m: i64| crate::agents::SynthSession {
        sid: sid.into(),
        path: PathBuf::from(format!("/r/{sid}.jsonl")),
        cwd: cwd.into(),
        mtime_ms: m,
    };
    let rows = synth_projects(
        "kindx",
        &[
            mk("019a0000-0000-7000-8000-00000000c0de", "/w/delta", 5),
            mk("s-other", "/w/delta", 9),
            mk("s-nocwd", "", 3),
        ],
        Some(&t),
    );
    let got: Vec<(String, String, u64, i64, Value)> = rows
        .iter()
        .map(|(p, _)| {
            (
                p["projectDir"].as_str().unwrap().to_string(),
                p["projectName"].as_str().unwrap().to_string(),
                p["sessionCount"].as_u64().unwrap(),
                p["lastActivity"].as_i64().unwrap(),
                p["starredCount"].clone(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            ("kindx:".into(), "(kindx)".into(), 1, 3, json!(0)),
            ("kindx:/w/delta".into(), "delta".into(), 2, 9, json!(1)),
        ]
    );
    assert!(rows.iter().all(|(p, _)| p["hasLive"].is_null()));
}

/// 一趟本机项目清单：记录树 ＋ 合成历史都在；记录树那一支的行数 == 夹具里的项目目录数（异源：夹具自己数）。
#[test]
fn a_local_listing_joins_the_record_tree_and_the_synthesized_history() {
    let home = std::env::temp_dir().join(format!("ccm-join-local-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    for (proj, sids) in [("-w-alpha", vec![S1, S2]), ("-w-beta", vec![S4])] {
        let d = home.join("projects").join(proj);
        std::fs::create_dir_all(&d).unwrap();
        for s in sids {
            std::fs::write(d.join(format!("{s}.jsonl")), "{\"cwd\":\"/w/x\"}\n").unwrap();
        }
    }
    let v = local_projects_with(
        &home,
        &[("kindx", vec![synth_fixture()])],
        &ann(),
        &Says(&[S4]),
    )
    .unwrap();
    let mut dirs: Vec<&str> = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["projectDir"].as_str().unwrap())
        .collect();
    assert_eq!(dirs[0], "-w-beta", "有活会话的排最前");
    dirs.sort_unstable();
    assert_eq!(dirs, vec!["-w-alpha", "-w-beta", "kindx:/w/delta"]);
    let s = local_sessions_with(&home, "-w-alpha", &[], &ann(), &Says(&[])).unwrap();
    let mut sids: Vec<&str> = s["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["sessionId"].as_str().unwrap())
        .collect();
    sids.sort_unstable();
    assert_eq!(sids, vec![S1, S2]);
    assert!(s["rows"][0].get("origin").is_none());
    assert_eq!(
        local_sessions_with(&home, "nokind:/w", &[], &ann(), &Says(&[]))
            .unwrap_err()
            .0,
        "bad_args",
        "认不出的合成键要明拒，不许当成空清单"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 跨语言金样：三份成品（远端项目 · 远端会话 · 合成会话）== `history-products.golden.json`（TS 解码器读同一份）。
#[test]
fn the_products_match_the_cross_language_golden() {
    let got = json!({
        "remoteProjects": remote_projects_from("dev", &projects_stdout(), &ann(), &NoLiveness).unwrap(),
        "remoteSessions": remote_sessions_from("dev", "-w-alpha", &sessions_stdout(), &ann(), &NoLiveness).unwrap(),
        "synthSessions": {
            "rows": [synth_session("kindx", &synth_fixture(), "占位摘录".into(), match &ann() {
                Loaded::Read(t) => Some(t),
                _ => None,
            })],
            "notice": null,
        },
    });
    let golden: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures().join("history-products.golden.json")).expect("金样"),
    )
    .expect("金样不是 JSON");
    assert_eq!(
        got, golden,
        "后端出的成品漂了 —— 界面那一侧按金样收，两边会对不上"
    );
}

/// 会话行逐格：fork 关系有才带、注解并上、远端判活 null、cwd 缺 ⇒ 名字退回项目目录名。
#[test]
fn a_session_row_carries_fork_and_annotations() {
    let v =
        remote_sessions_from("dev", "-w-alpha", &sessions_stdout(), &ann(), &NoLiveness).unwrap();
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows[0]["forkedFromSessionId"], S3);
    assert_eq!(rows[0]["forkedFromMessageUuid"], "m-1");
    assert_eq!(rows[0]["starred"], true);
    assert_eq!(rows[0]["customTitle"], "占位标题一");
    assert_eq!(rows[0]["isLive"], Value::Null);
    assert!(
        rows[1].get("forkedFromSessionId").is_none(),
        "没有 fork 关系就不带那两格"
    );
    assert_eq!(rows[1]["projectName"], "-w-alpha");
    assert_eq!(rows[1]["hidden"], true);
    assert_eq!(rows[1]["isBg"], true);
}

/// ★ `K-R92`：排序时「不知道」自成一档，夹在「确定有」与「确定没有」之间（从 monitor `unknown_is_its_own_bucket_when_sorting`〔散文墓碑〕搬来）——
/// 「不知道」既不许冒充「活着」抢到最前，也不许被当成「确定没活」压到最后；星标那一档同理。
#[test]
fn projects_sort_unknown_between_known_true_and_known_false() {
    let row = |dir: &str, live: Value, star: Value, last: i64| json!({"projectDir": dir, "hasLive": live, "starredCount": star, "lastActivity": last});
    let mut rows = vec![
        row("dead", json!(false), json!(0), 900),
        row("unknown", Value::Null, json!(0), 100),
        row("live", json!(true), json!(0), 1),
    ];
    sort_projects(&mut rows);
    let order: Vec<&str> = rows
        .iter()
        .map(|r| r["projectDir"].as_str().unwrap())
        .collect();
    assert_eq!(
        order,
        vec!["live", "unknown", "dead"],
        "活 > 不知道 > 没活（不看时间）"
    );
    let mut rows = vec![
        row("zero", Value::Null, json!(0), 900),
        row("unknown", Value::Null, Value::Null, 100),
        row("starred", Value::Null, json!(2), 1),
    ];
    sort_projects(&mut rows);
    let order: Vec<&str> = rows
        .iter()
        .map(|r| r["projectDir"].as_str().unwrap())
        .collect();
    assert_eq!(
        order,
        vec!["starred", "unknown", "zero"],
        "有星标 > 不知道 > 查过了一个都没有"
    );
}

/// ★ 〔GAP1 · `设计/05 §14.5`「远端判活『不知道』（留口）」〕远端判活由那台后端答（`--session-accounts` 的 `alive`）：
/// 期望手写 —— S1 活、S4 死（`alive:false`）、别的 sid 不在 ⇒ 死；那一问失败 ⇒ 仍 `null`（不当成全死）。
#[tokio::test]
async fn remote_liveness_comes_from_that_machines_session_accounts() {
    let table = ReachTable::default();
    reach(&table);
    let live = [
        format!(r#"{{"pid":1,"sessionId":"{S1}","alive":true}}"#),
        format!(r#"{{"pid":2,"sessionId":"{S4}","alive":false}}"#),
        r#"{"pid":3,"sessionId":null,"alive":true}"#.to_string(),
    ]
    .join("\n");
    let far = Far {
        live: Some(live),
        ..Far::default()
    };
    let v = answer_projects_with(json!({"origin": "dev"}), &table, &far)
        .await
        .unwrap();
    let by_dir = |v: &Value, d: &str| {
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["projectDir"] == d)
            .unwrap()["hasLive"]
            .clone()
    };
    assert_eq!(by_dir(&v, "-w-alpha"), json!(true));
    assert_eq!(
        by_dir(&v, "-w-gamma"),
        Value::Null,
        "清单与条数对不上照旧「不知道」"
    );
    let s = answer_sessions_with(
        json!({"origin": "dev", "project_dir": "-w-alpha"}),
        &table,
        &far,
    )
    .await
    .unwrap();
    let live_of: Vec<Value> = s["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["isLive"].clone())
        .collect();
    assert_eq!(live_of, vec![json!(true), json!(false)], "S1 活 · S4 死");
    let dead = Far::default();
    let s = answer_sessions_with(
        json!({"origin": "dev", "project_dir": "-w-alpha"}),
        &table,
        &dead,
    )
    .await
    .unwrap();
    let live_of: Vec<Value> = s["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["isLive"].clone())
        .collect();
    assert_eq!(
        live_of,
        vec![Value::Null, Value::Null],
        "判活那一问失败 ⇒「不知道」，不是死"
    );
}
