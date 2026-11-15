//! # 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md`「功能侧只读查询」节（任务列表 · 插件市场）
//!
//! 核原文：该节逐字「宿主是 `feature_face`（不是 `read_face`，理由在它头注），本体在 `observe/`」·「应答一律**按行**」；
//! `plugins-marketplaces` 条逐字「文件不在 ⇒ `file_absent: true`（诚实的空）；读 / 解析失败 ⇒ `failed`」；`tasks-list` 条 `sid` 行逐字
//! 「只许一段普通路径名（空 / 含分隔符 / `.` / `..` ⇒ `bad_args`）」—— 本族三条一一对上。〔JA1 点址 2026-09-24〕
//!
//! 功能侧帧面宿主的判据。夹具只造结构，不采任何真会话正文。

use super::*;
use serde_json::json;

/// `tasks-list` 照分派那一层解入参（`typed::take`）再交本体；回包投成线上那一份，失败只留码。
fn tasks(home: &std::path::Path, args: &Value) -> Result<Value, String> {
    use crate::wire::typed::Codes as _;
    let req = crate::wire::typed::take::<TasksListReq>(args)
        .map_err(|f| cmd_wire::failed::Failed::code_or_fallback(f.code.clone()))?;
    let got = tasks_at(home, req)
        .map_err(|e| cmd_wire::failed::Failed::code_or_fallback(e.into_said().code.clone()))?;
    Ok(serde_json::to_value(got).expect("回包总能序列化"))
}

#[test]
fn tasks_list_answers_the_product_and_refuses_a_missing_sid() {
    let h = test_home::scratch("tasks");
    let dir = crate::observe::tasks_query::tasks_root(&h).join("s");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("1.json"),
        r#"{"id":"1","subject":"占位","status":"pending"}"#,
    )
    .unwrap();
    let v = tasks(&h, &json!({"sid": "s"})).unwrap();
    // 应答是成品 `{tasks}`，不再是原样对象的 `lines`。
    let rows = v["tasks"].as_array().expect("tasks");
    assert_eq!(rows.len(), 1);
    assert!(v.get("lines").is_none(), "还在回原始行：{v}");
    // 反向：缺 sid ⇒ bad_args；sid 走出任务根 ⇒ bad_args（围栏在本体里，这里只证它透得上来）。
    assert_eq!(tasks(&h, &json!({})).unwrap_err(), "bad_args");
    assert_eq!(tasks(&h, &json!({"sid": "../s"})).unwrap_err(), "bad_args");
    // 多一格 ⇒ bad_args（入参是类型：`deny_unknown_fields`）。
    assert_eq!(
        tasks(&h, &json!({"sid": "s", "x": 1})).unwrap_err(),
        "bad_args"
    );
}

/// ★ `tasks-list` 的成品 == 金样（`tests/__fixtures__/tasks-list.golden.json`）。
///
/// 异源：金样里的 `files` 由**生产**路径（`tasks_at` → `session_tasks` → `task_entry`）现算，与手写的 `product` 逐格相等。
/// 界面按生成类型用这一份（问回来的与帧里带的同一个类型），形状由那条命令的指纹核。
/// 金样的 `files` 覆盖了旧口径（serde `TaskEntry`）的每一档：多余键不带 · BOM · `null` 可选格不出现 · id 不是串 / blocks 是 null /
/// 缺必填 / 半截 JSON / 不是数字名 ⇒ 都不算任务。
#[test]
fn the_tasks_product_matches_the_golden() {
    let g: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/tasks-list.golden.json")).unwrap();
    let h = test_home::scratch("tasks-golden");
    let sid = g["request"]["sid"].as_str().unwrap();
    let dir = crate::observe::tasks_query::tasks_root(&h).join(sid);
    std::fs::create_dir_all(&dir).unwrap();
    for f in g["files"].as_array().unwrap() {
        std::fs::write(
            dir.join(f["name"].as_str().unwrap()),
            f["body"].as_str().unwrap(),
        )
        .unwrap();
    }
    let got = tasks(&h, &g["request"]).unwrap();
    assert_eq!(got, g["product"], "成品与金样对不上");
    // 线上字节：帧里的回包先过 `to_value`（键排好序）再写出 ⇒ 与金样那一份重新序列化后逐字节相等。
    assert_eq!(
        serde_json::to_string(&got).unwrap(),
        serde_json::to_string(&g["product"]).unwrap()
    );
}

/// ★ `mcp-read` 每条写好的字（扩展页小标与抽屉那一行照抄）：需登录那一句（号名 · 几点记下）· 断开那一句（这台活会话里最近说它的那一条：
/// 会话名 · 几点 · 原话）· 小标取哪一种（断开压过需登录；停用的不连 ⇒ 什么都不标）。成品 == 金样 `marked`（界面读同一份）。
/// 要求：MCP 状态那一块「判定还是 mcp-read 一处，每条再加两格写好的字」。
#[test]
fn the_mcp_marks_are_written_once_on_the_backend() {
    use crate::agents::{McpEntry, McpRead, McpStatus};
    use crate::observe::accounts_query::LiveMcpFailed;
    let g: Value =
        serde_json::from_str(include_str!("../../__fixtures__/mcp-read.golden.json")).unwrap();
    let ms = |s: &str| crate::common::time::parse_iso8601_ms(s).unwrap();
    let http = json!({"type": "http", "url": "http://x"});
    let e = |name: &str, server: &Value, status, login_in: &[Option<&str>], seen: Option<u64>| {
        McpEntry {
            scope: "user",
            name: name.into(),
            server: server.clone(),
            source: "<CJ>".into(),
            status,
            login_in: login_in.iter().map(|s| s.map(str::to_string)).collect(),
            seen_ms: seen,
        }
    };
    let seen = Some(ms("2026-10-09T10:42:00Z") as u64);
    let read = McpRead {
        entries: vec![
            e(
                "s-login",
                &http,
                McpStatus::NeedsLogin,
                &[Some("acct-a"), Some("acct-b")],
                seen,
            ),
            e("s-both", &http, McpStatus::NeedsLogin, &[None], seen),
            e(
                "s-failed",
                &json!({"command": "s-failed"}),
                McpStatus::Unknown,
                &[],
                None,
            ),
            e("s-off", &http, McpStatus::Disabled, &[], None),
            e(
                "s-plain",
                &json!({"command": "s-plain"}),
                McpStatus::Unknown,
                &[],
                None,
            ),
        ],
        dirs: vec![],
        problems: vec![],
    };
    let f = |name: &str, at: Option<i64>, detail: Option<&str>, title: &str| LiveMcpFailed {
        name: name.into(),
        at_ms: at,
        detail: detail.map(str::to_string),
        title: title.into(),
    };
    let failed = vec![
        f(
            "s-both",
            Some(ms("2026-10-09T10:31:00Z")),
            Some("err-1"),
            "会话-甲",
        ),
        f("s-failed", None, None, "会话-乙"),
        f(
            "s-off",
            Some(ms("2026-10-09T10:00:00Z")),
            Some("err-2"),
            "会话-丙",
        ),
    ];
    let say = McpSay {
        registry: crate::agents::REGISTRY,
        failed: &failed,
        now_s: ms("2026-10-09T12:00:00Z") / 1000,
        tz: Default::default(),
        kind: "claude",
        tagged: false,
        project: None,
        homes: &[],
    };
    assert_eq!(
        serde_json::to_value(mcp_reply(&read, &say)).unwrap(),
        g["marked"]
    );
}

/// 两家都声明了 MCP 面：`mcp-read` 是整台机器的面 ⇒ 逐家各答一份、合成一份。
/// 每条带上家：默认那一家（`asset_catalog::tag_family` 同一个判法）不写，别的家写上。
/// 各家各要自己号的家（`look_of`），断开也只认那一家号里的（`failed_of` 收到的是那一家的家）。
#[test]
fn mcp_read_answers_every_family_each_tagged_with_its_own() {
    use crate::agents::{fake, McpLook};
    use crate::observe::accounts_query::LiveMcpFailed;
    let reg = [
        fake::family_row("alpha", "CCM_A_DIR", "a", false, || None),
        fake::family_row("beta", "CCM_B_DIR", "b", true, || None),
    ];
    let proj = test_home::scratch("mcp-read-per-family");
    std::fs::write(
        proj.join(".fake-mcp.json"),
        r#"{"servers":{"s":{"command":"x"}}}"#,
    )
    .unwrap();
    let home_of = |kind: &str| std::path::PathBuf::from(format!("/fixture/{kind}"));
    let asked = std::cell::RefCell::new(Vec::<String>::new());
    let got = mcp_read_with(
        &reg,
        Some(&proj),
        &McpReadReq {
            project_dir: Some(proj.display().to_string()),
        },
        &Default::default(),
        |kind| {
            asked.borrow_mut().push(kind.to_string());
            McpLook {
                homes: vec![(None, home_of(kind))],
                bare: Some(home_of(kind)),
                now_ms: 0,
            }
        },
        // 只有 alpha 号里的活会话说 `s` 断开。
        |homes| {
            if homes
                == [crate::agents::AgentHome {
                    kind: "alpha",
                    home: home_of("alpha"),
                }]
            {
                vec![LiveMcpFailed {
                    name: "s".into(),
                    at_ms: None,
                    detail: None,
                    title: "t".into(),
                }]
            } else {
                Vec::new()
            }
        },
    );
    // 先清目录再看结果：读不出时 `expect` 一断也不留下它。
    let _ = std::fs::remove_dir_all(&proj);
    let got = got.expect("读得出");
    assert_eq!(
        asked.into_inner(),
        vec!["alpha".to_string(), "beta".to_string()],
        "逐家各问一遍"
    );
    let v = serde_json::to_value(&got).unwrap();
    assert_eq!(
        v.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["entries"],
        "回包只一格条目：用过的项目目录 · 读不出的那几句没人读，不出"
    );
    let rows: Vec<(Value, Value)> = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e.get("agent").cloned().unwrap_or(Value::Null),
                e["mark"].clone(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            (json!("alpha"), json!("failed")),
            (Value::Null, Value::Null)
        ],
        "第二家那一条带 `agent`、断开只记在它自己那一家；默认那一家不写 `agent`"
    );
}

/// 一家答的一份：只一条用户级的 `name`（合成判据用）。
fn part(name: &str) -> McpReadReply {
    use crate::agents::{McpEntry, McpRead, McpStatus};
    let read = McpRead {
        entries: vec![McpEntry {
            scope: "user",
            name: name.into(),
            server: json!({"command": name}),
            source: "<CJ>".into(),
            status: McpStatus::Unknown,
            login_in: Vec::new(),
            seen_ms: None,
        }],
        ..Default::default()
    };
    mcp_reply(&read, &McpSay::default())
}

/// 上限量的是合成后的整份：一家单独恰在上限上不过，两家合起来过了 ⇒ `too_large`（不截断）。上限是入参，判据传一份的大小。
#[test]
fn mcp_read_caps_the_merged_reply_not_each_family() {
    let cap = serde_json::to_vec(&part("a")).unwrap().len();
    assert!(
        mcp_merge([part("a")], cap).is_ok(),
        "一家单独不过上限（否则下一句证不了量的是整份）"
    );
    let two = mcp_merge([part("a"), part("b")], cap);
    let size = serde_json::to_vec(&McpReadReply {
        entries: [part("a"), part("b")]
            .into_iter()
            .flat_map(|p| p.entries)
            .collect(),
    })
    .unwrap()
    .len();
    let said = copy_text(
        "beFeatureFace.answerAt.tooLarge",
        &[("size", &size.to_string()), ("cap", &cap.to_string())],
    );
    assert!(
        matches!(&two, Err(McpReadErr::TooLarge(s)) if *s == said),
        "那一句说的上限是传进来的那个：{two:?}"
    );
}

/// 需登录那一格的解法照那一条所在的那一家写（`McpSay.kind` 逐家）：两个替身家借 Claude 的 MCP 读面
/// （项目里一条 http 服务器 · 各家号的家里记着它要登录），借的时候去掉 app 内登录那一格 ⇒ 那一句说的是它自己那一家在 app 里登不了。
#[test]
fn mcp_read_writes_each_familys_login_fix_with_its_own_kind() {
    use crate::agents::{fake, Adapter, McpLook};
    let claude = crate::agents::REGISTRY
        .iter()
        .find(|a| a.kind == "claude")
        .expect("注册表里有 claude");
    let mcp = claude.mcp.map(|mut m| {
        m.sign_in = None;
        m
    });
    let reg = [
        Adapter {
            mcp,
            ..fake::family_row("alpha", "CCM_A_DIR", "a", false, || None)
        },
        Adapter {
            mcp,
            ..fake::family_row("beta", "CCM_B_DIR", "b", true, || None)
        },
    ];
    let root = test_home::scratch("mcp-read-login-kind");
    let proj = root.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(
        proj.join(".mcp.json"),
        r#"{"mcpServers":{"s":{"type":"http","url":"http://127.0.0.1:9/"}}}"#,
    )
    .unwrap();
    let now_ms: u64 = 1_800_000_000_000;
    for kind in ["alpha", "beta"] {
        let home = root.join(kind);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join("mcp-needs-auth-cache.json"),
            json!({"s": {"timestamp": now_ms - 1000}}).to_string(),
        )
        .unwrap();
    }
    let got = mcp_read_with(
        &reg,
        Some(&proj),
        &McpReadReq {
            project_dir: Some(proj.display().to_string()),
        },
        &Default::default(),
        |kind| McpLook {
            homes: vec![(None, root.join(kind))],
            bare: Some(root.join(kind)),
            now_ms,
        },
        |_| Vec::new(),
    );
    let _ = std::fs::remove_dir_all(&root);
    let why: Vec<Option<String>> = got
        .expect("读得出")
        .entries
        .iter()
        .filter(|e| e.scope == "project")
        .map(|e| e.login.as_ref().expect("需登录那一格").fix[0].why.clone())
        .collect();
    let said = |agent: &str| Some(copy_text("beManual.list.mcpNoFix", &[("agent", agent)]));
    // 各家认各家：那一句点的是传进来的注册表上那一家自己的叫法。
    assert_eq!(
        why,
        [said(fake::said_of("alpha")), said(fake::said_of("beta"))]
    );
}

/// `mcp-read` 一份成品投成 `(名字, 号名, 不一致)` 一行一条。
fn rows_of(v: &Value) -> Vec<(String, Vec<String>, bool)> {
    v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            let accounts = e
                .get("accounts")
                .map(|a| serde_json::from_value(a.clone()).expect("号名是串的数组"))
                .unwrap_or_default();
            let mismatch = e.get("mismatch") == Some(&json!(true));
            (e["name"].as_str().unwrap().to_string(), accounts, mismatch)
        })
        .collect()
}

/// 几个号的家各有一份用户级配置（号名 · 家的目录名 · 名 → 定义）：条目的 `source` 落在那个家底下；成品照 `McpSay.homes` 认号。
fn rows_over_homes(files: &[(Option<&str>, &str, Value)]) -> Vec<(String, Vec<String>, bool)> {
    rows_of(&reply_over_homes(files))
}

/// 同 [`rows_over_homes`]，交整份成品（JSON）。
fn reply_over_homes(files: &[(Option<&str>, &str, Value)]) -> Value {
    use crate::agents::{McpEntry, McpRead, McpStatus};
    let home = |dir: &str| std::path::PathBuf::from(format!("/fixture/{dir}"));
    let homes: Vec<(Option<String>, std::path::PathBuf)> = files
        .iter()
        .map(|(who, dir, _)| (who.map(str::to_string), home(dir)))
        .collect();
    let entries = files
        .iter()
        .flat_map(|(_, dir, body)| {
            let source = home(dir).join("fake-user-mcp.json").display().to_string();
            body.as_object()
                .expect("夹具是对象")
                .iter()
                .map(move |(name, server)| McpEntry {
                    scope: "user",
                    name: name.clone(),
                    server: server.clone(),
                    source: source.clone(),
                    status: McpStatus::Unknown,
                    login_in: Vec::new(),
                    seen_ms: None,
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let read = McpRead {
        entries,
        dirs: vec![],
        problems: vec![],
    };
    let say = McpSay {
        kind: "alpha",
        homes: &homes,
        ..Default::default()
    };
    serde_json::to_value(mcp_reply(&read, &say)).unwrap()
}

/// 每条写好的「在哪几个号里 · 不一致」那一格（`held`）投成 `(那一句, 悬停那一截)`；缺 ⇒ `None`。
fn held_of(v: &Value) -> Vec<Option<(String, Option<String>)>> {
    v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            e.get("held").filter(|h| !h.is_null()).map(|h| {
                (
                    h["said"].as_str().expect("said 是串").to_string(),
                    h["tip"].as_str().map(str::to_string),
                )
            })
        })
        .collect()
}

/// ★ 两个号同名同配置 ⇒ 一条，那一句点出两个号（后端写好，界面照抄）；不带悬停。
#[test]
fn mcp_read_says_which_accounts_hold_a_merged_server() {
    let s = json!({"command": "x"});
    let v = reply_over_homes(&[
        (Some("acct-b"), "b", json!({"s": s})),
        (Some("acct-a"), "a", json!({"s": s})),
    ]);
    let who = ["acct-a", "acct-b"].join(&copy_text("plan.page.listSep", &[]));
    assert_eq!(
        held_of(&v),
        vec![Some((
            copy_text("beMcp.mark.accounts", &[("accounts", &who)]),
            None
        ))]
    );
}

/// ★ 同名不一致 ⇒ 两条各带「不一致」那一句（点出各自的号）与悬停那一截。
#[test]
fn mcp_read_marks_each_differing_server_with_its_accounts() {
    let v = reply_over_homes(&[
        (Some("acct-a"), "a", json!({"s": {"command": "x"}})),
        (Some("acct-b"), "b", json!({"s": {"command": "y"}})),
    ]);
    let tip = Some(copy_text("beMcp.mark.mismatchTip", &[]));
    let said = |who: &str| copy_text("beMcp.mark.mismatch", &[("accounts", who)]);
    assert_eq!(
        held_of(&v),
        vec![
            Some((said("acct-a"), tip.clone())),
            Some((said("acct-b"), tip))
        ]
    );
}

/// ★ 没设账号那一份不一致 ⇒ 那一句不点号名；只在它里面、又一致的 ⇒ 那一格缺。
#[test]
fn mcp_read_bare_home_says_no_account() {
    let v = reply_over_homes(&[
        (
            None,
            "bare",
            json!({"s": {"command": "x"}, "lone": {"command": "z"}}),
        ),
        (Some("acct-a"), "a", json!({"s": {"command": "y"}})),
    ]);
    let tip = Some(copy_text("beMcp.mark.mismatchTip", &[]));
    assert_eq!(
        held_of(&v),
        vec![
            None,
            Some((copy_text("beMcp.mark.mismatchBare", &[]), tip.clone())),
            Some((
                copy_text("beMcp.mark.mismatch", &[("accounts", "acct-a")]),
                tip
            )),
        ]
    );
}

/// 号名经 `mcp-read` 那一趟从各号的家（`look_of`）一路带到成品：替身家项目里那一份落在一个号的家底下 ⇒ 那一条带那个号。
#[test]
fn mcp_read_names_the_account_whose_home_holds_the_entry() {
    use crate::agents::{fake, McpLook};
    let reg = [fake::family_row("alpha", "CCM_A_DIR", "a", true, || None)];
    let proj = test_home::scratch("mcp-read-account-name");
    std::fs::write(
        proj.join(".fake-mcp.json"),
        r#"{"servers":{"s":{"command":"x"}}}"#,
    )
    .unwrap();
    let got = mcp_read_with(
        &reg,
        Some(&proj),
        &McpReadReq {
            project_dir: Some(proj.display().to_string()),
        },
        &Default::default(),
        |_| McpLook {
            homes: vec![(Some("acct-a".to_string()), proj.clone())],
            bare: None,
            now_ms: 0,
        },
        |_| Vec::new(),
    );
    let _ = std::fs::remove_dir_all(&proj);
    let v = serde_json::to_value(got.expect("读得出")).unwrap();
    assert_eq!(
        rows_of(&v),
        vec![("s".to_string(), vec!["acct-a".to_string()], false)]
    );
}

/// ★ 两个号里同名同配置的一条 ⇒ 合成一条、带两个号（号名排好序，不照家的先后）；不一致那一格缺。
#[test]
fn mcp_read_merges_one_server_held_by_two_accounts() {
    let s = json!({"type": "http", "url": "http://127.0.0.1:9/"});
    let rows = rows_over_homes(&[
        (Some("acct-b"), "b", json!({"s": s})),
        (Some("acct-a"), "a", json!({"s": s})),
    ]);
    assert_eq!(
        rows,
        vec![(
            "s".to_string(),
            vec!["acct-a".to_string(), "acct-b".to_string()],
            false
        )]
    );
}

/// ★ 同名但两个号的配置不一样 ⇒ 照实分开两条、各带各的号、都标不一致。
#[test]
fn mcp_read_keeps_differing_configs_apart_and_marks_them() {
    let rows = rows_over_homes(&[
        (Some("acct-a"), "a", json!({"s": {"command": "x"}})),
        (Some("acct-b"), "b", json!({"s": {"command": "y"}})),
    ]);
    assert_eq!(
        rows,
        vec![
            ("s".to_string(), vec!["acct-a".to_string()], true),
            ("s".to_string(), vec!["acct-b".to_string()], true),
        ]
    );
}

/// ★ 没设账号的那一份：号名空；它与某号同名同配置 ⇒ 照样合成一条，号名只出那个号；只在它里面的 ⇒ 号名空、不标不一致。
#[test]
fn mcp_read_names_no_account_for_the_bare_home() {
    let s = json!({"command": "x"});
    let rows = rows_over_homes(&[
        (None, "bare", json!({"s": s, "lone": {"command": "z"}})),
        (Some("acct-a"), "a", json!({"s": s})),
    ]);
    assert_eq!(
        rows,
        vec![
            ("lone".to_string(), Vec::new(), false),
            ("s".to_string(), vec!["acct-a".to_string()], false),
        ]
    );
}

/// ★ 没设账号那一份与号 a 那一份同名同配置、都要登录 ⇒ 合成一条，解法两项：不带号那一位在前（`account: null`）、再号 a；
/// 不带号那一份「需登录」不随合并消失。
#[test]
fn mcp_read_keeps_the_bare_login_when_it_merges_with_an_account() {
    use crate::agents::{McpEntry, McpRead, McpStatus};
    let s = json!({"type": "http", "url": "http://x"});
    let homes = vec![(
        Some("acct-a".to_string()),
        std::path::PathBuf::from("/fixture/a"),
    )];
    let e = |source: &str, login_in| McpEntry {
        scope: "user",
        name: "s".into(),
        server: s.clone(),
        source: source.into(),
        status: McpStatus::NeedsLogin,
        login_in,
        seen_ms: Some(1_000),
    };
    let read = McpRead {
        entries: vec![
            e("/fixture/bare/.claude.json", vec![None]),
            e("/fixture/a/.claude.json", vec![Some("acct-a".to_string())]),
        ],
        dirs: vec![],
        problems: vec![],
    };
    let say = McpSay {
        kind: "alpha",
        homes: &homes,
        ..Default::default()
    };
    let v = serde_json::to_value(mcp_reply(&read, &say)).unwrap();
    let rows = v["entries"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{v}");
    let fix: Vec<Value> = rows[0]["login"]["fix"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["account"].clone())
        .collect();
    assert_eq!(fix, vec![Value::Null, json!("acct-a")]);
}

/// 项目级那几条不合也不标：同名的几层（Codex 从工作目录往上每层一份）是层层盖过，不是号与号之间不一致。
#[test]
fn mcp_read_leaves_project_layers_alone() {
    use crate::agents::{McpEntry, McpRead, McpStatus};
    let e = |source: &str, server: Value| McpEntry {
        scope: "project",
        name: "s".into(),
        server,
        source: source.into(),
        status: McpStatus::Unknown,
        login_in: Vec::new(),
        seen_ms: None,
    };
    let read = McpRead {
        entries: vec![
            e("/w/a/b/.layer", json!({"command": "x"})),
            e("/w/a/.layer", json!({"command": "y"})),
            e("/w/.layer", json!({"command": "y"})),
        ],
        dirs: vec![],
        problems: vec![],
    };
    let say = McpSay {
        kind: "alpha",
        ..Default::default()
    };
    let rows = rows_of(&serde_json::to_value(mcp_reply(&read, &say)).unwrap());
    let plain = ("s".to_string(), Vec::new(), false);
    assert_eq!(rows, vec![plain.clone(), plain.clone(), plain]);
}
