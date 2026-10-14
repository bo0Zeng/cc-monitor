//! 会话外的需手动清单：账号清单成品（夹具，照 `accounts-list` 那一份写）· MCP 读面（夹具）⇒ 每一项带写好的那一句和怎么解。夹具全是合成的。
use super::*;
use serde_json::json;

fn row(name: &str, kind: &str, exists: bool, ready: bool) -> Value {
    json!({ "name": name, "authKind": kind, "exists": exists, "authReady": ready })
}

fn entry(
    name: &str,
    status: crate::agents::McpStatus,
    login_in: &[&str],
) -> crate::agents::McpEntry {
    crate::agents::McpEntry {
        scope: "user",
        name: name.to_string(),
        server: json!({}),
        source: "/h/.claude.json".to_string(),
        status,
        login_in: login_in.iter().map(|s| s.to_string()).collect(),
        seen_ms: None,
    }
}

/// 需登录的号：订阅号 · 目录在 · 还没就绪的那几个（API 号 · 目录不在 · 已登录的不算）；每项 `fix` 指向 `login-start`。
/// 需登录的 MCP：每个号一项，没设账号的那一份 `account: null`；断开 · 停用的不算；每项 `fix` 指向 `login-start {kind: mcp}`（登在那个号上）。
/// 那一家不在 app 里登 MCP ⇒ `fix: null`（只能在会话里登，不列成「点了能解」），并带 `why`：为什么没有解法。
#[test]
fn the_manual_list_names_each_thing_and_how_to_fix_it() {
    let accounts = json!({ "accounts": [
        row("work", "subscription", true, false),
        row("0", "subscription", true, false),
        row("api", "api-key", true, false),
        row("gone", "subscription", false, false),
        row("ok", "subscription", true, true),
    ]});
    let mcp = crate::agents::McpRead {
        entries: vec![
            entry("notes", crate::agents::McpStatus::NeedsLogin, &["work"]),
            entry("bare", crate::agents::McpStatus::NeedsLogin, &[]),
            entry("down", crate::agents::McpStatus::Failed, &[]),
        ],
        dirs: vec![],
        problems: vec![],
    };
    let got = serde_json::to_value(
        manual_of("claude", &accounts, None, Some(("claude", &mcp))).expect("列得出"),
    )
    .unwrap();
    let fix = |a: &str| json!({ "command": "login-start", "args": { "kind": "account", "agent": "claude", "account": a } });
    let mcp_fix = |a: Value, s: &str| {
        json!({ "command": "login-start",
                "args": { "kind": "mcp", "agent": "claude", "account": a, "server": s, "projectDir": null } })
    };
    assert_eq!(
        got,
        json!({ "items": [
            { "kind": "accountLogin", "agent": "claude", "account": "work",
              "said": copy_text("beManual.list.accountLogin", &[("account", "work")]), "fix": fix("work") },
            { "kind": "accountLogin", "agent": "claude", "account": "0",
              "said": copy_text("beManual.list.accountLogin", &[("account", &copy_text("acct.home.name", &[]))]), "fix": fix("0") },
            { "kind": "mcpLogin", "account": "work", "server": "notes", "scope": "user",
              "said": copy_text("beManual.list.mcpLogin", &[("server", "notes"), ("account", "work")]),
              "fix": mcp_fix(json!("work"), "notes") },
            { "kind": "mcpLogin", "account": null, "server": "bare", "scope": "user",
              "said": copy_text("beManual.list.mcpLoginBare", &[("server", "bare")]),
              "fix": mcp_fix(Value::Null, "bare") },
        ]})
    );
    // 那一家不在 app 里登 ⇒ 不列需登录的号（没有解法的不列成「点了能解」）。
    let fake =
        serde_json::to_value(manual_of("no-such-agent", &accounts, None, None).expect("列得出"))
            .unwrap();
    assert_eq!(fake, json!({ "items": [] }));
    let no_mcp_login = serde_json::to_value(
        manual_of("no-such-agent", &accounts, None, Some(("fake", &mcp))).expect("列得出"),
    )
    .unwrap();
    assert_eq!(no_mcp_login["items"][0]["kind"], "mcpLogin");
    assert_eq!(
        no_mcp_login["items"][0]["fix"],
        Value::Null,
        "不在 app 里登 MCP 的那一家：没有解法"
    );
    assert_eq!(
        no_mcp_login["items"][0]["why"],
        json!(copy_text("beManual.list.mcpNoFix", &[("agent", "fake")])),
        "没有解法时说为什么"
    );
    assert!(
        got["items"]
            .as_array()
            .expect("items")
            .iter()
            .all(|i| i.get("why").is_none()),
        "有解法的不带 `why`"
    );
}

/// 账号库清单在、却读不懂 ⇒ 回 `unreadable` ＋ 为什么，不回空清单（空清单看起来像「没事」）。
/// 读法照账号清单那一份（坏 JSON 的清单 ⇒ `meta.error` 那一句）；没有清单（没启用多账号）照旧列得出。
#[test]
fn an_unreadable_account_library_is_said_not_listed_as_nothing() {
    use crate::observe::accounts_query::{library_unreadable, list_product_with};
    let root = std::env::temp_dir().join(format!("ccm-manual-unreadable-{}", std::process::id()));
    let accts = root.join("accts");
    std::fs::create_dir_all(&accts).unwrap();
    std::fs::write(accts.join("accounts.json"), "{not json").unwrap();
    let product = list_product_with(&accts, None, &[], &[], "claude", "claude", false);
    let why = library_unreadable(&accts);
    assert_eq!(
        why.clone().map(Value::String),
        Some(product["meta"]["error"].clone()),
        "同账号清单那一句"
    );
    let fail = manual_of("claude", &product, why, None)
        .expect_err("读不懂 ⇒ 不是空清单")
        .into_fail();
    assert_eq!(fail.code, "unreadable");
    assert_eq!(
        json!(fail.message),
        product["meta"]["error"],
        "那一句是为什么"
    );
    let nope = root.join("nope");
    assert_eq!(
        library_unreadable(&nope),
        None,
        "没有清单 ＝ 没启用多账号，不是读不懂"
    );
    let absent = list_product_with(&nope, None, &[], &[], "claude", "claude", false);
    assert_eq!(
        serde_json::to_value(manual_of("claude", &absent, None, None).expect("没有清单照列"))
            .unwrap(),
        json!({ "items": [] })
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// 入参按 `kind` 分两种；多一格 · 少了必填的 · 不认得的 `kind` 一律拒（不会照旧形状把 MCP 那一种当成号）。
#[test]
fn the_login_target_takes_exactly_its_two_shapes() {
    let ok: LoginStartReq =
        serde_json::from_value(json!({ "kind": "account", "agent": "claude", "account": "w" }))
            .unwrap();
    assert_eq!(
        ok,
        LoginStartReq::Account {
            agent: "claude".into(),
            account: "w".into()
        }
    );
    let m: LoginStartReq =
        serde_json::from_value(json!({ "kind": "mcp", "agent": "claude", "server": "notes" }))
            .unwrap();
    assert_eq!(
        m,
        LoginStartReq::Mcp {
            agent: "claude".into(),
            account: None,
            server: "notes".into(),
            project_dir: None
        }
    );
    for bad in [
        json!({ "agent": "claude", "account": "w" }),
        json!({ "kind": "account", "agent": "claude", "account": "w", "server": "x" }),
        json!({ "kind": "mcp", "agent": "claude" }),
        json!({ "kind": "other", "agent": "claude", "account": "w" }),
    ] {
        assert!(
            serde_json::from_value::<LoginStartReq>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

/// `browserHere`：与目标同一层交、不进目标；缺 ⇒ 按「不在」答；不是布尔 ⇒ 拒。多一格照旧拒。
#[test]
fn whether_the_browser_is_here_rides_beside_the_target() {
    let target = LoginStartReq::Account {
        agent: "claude".into(),
        account: "w".into(),
    };
    let base = json!({ "kind": "account", "agent": "claude", "account": "w" });
    let with = |v: Value| {
        let mut a = base.clone();
        a["browserHere"] = v;
        LoginStartArgs::take(&a)
    };
    assert_eq!(
        with(json!(true)).expect("收"),
        LoginStartArgs {
            target: target.clone(),
            browser_here: true
        }
    );
    assert_eq!(
        with(json!(false)).expect("收"),
        LoginStartArgs {
            target: target.clone(),
            browser_here: false
        }
    );
    assert_eq!(
        LoginStartArgs::take(&base).expect("收"),
        LoginStartArgs {
            target,
            browser_here: false
        },
        "缺 ⇒ 不在这台"
    );
    for bad in [json!("yes"), json!(1), Value::Null] {
        assert_eq!(with(bad.clone()).expect_err("拒").code, "bad_args", "{bad}");
    }
    let mut extra = base.clone();
    extra["server"] = json!("x");
    assert_eq!(
        LoginStartArgs::take(&extra).expect_err("拒").code,
        "bad_args"
    );
}

/// 会不会自己登完照 `browserHere` 答：浏览器不在这台 ⇒ 回跳到不了，只能粘（不说「会自己登完」）。
#[test]
fn it_says_it_finishes_by_itself_only_when_the_browser_is_here() {
    let face = crate::agents::accounts_face("claude")
        .and_then(|f| f.sign_in)
        .expect("claude 在 app 里登");
    for here in [true, false] {
        let args = LoginStartArgs {
            target: LoginStartReq::Account {
                agent: "claude".into(),
                account: "w".into(),
            },
            browser_here: here,
        };
        let s = sign_in::Started {
            job: "login-1-0".into(),
            url: "https://claude.com/cai/oauth/authorize?state=s".into(),
        };
        assert_eq!(started(s, face, &args).auto, here);
    }
}

/// 登录失败的回包说还能怎么办（`data.again`），每个码一个定值：接着粘 · 重来 · 没路。码表里每个码都有（`bad_args` 不带）。
#[test]
fn each_failure_says_what_can_be_done_next() {
    let again = |f: Fail| (f.code.clone(), f.data.map(|d| d["again"].clone()));
    let start = [
        LoginStartErr::not_found("x".into()),
        LoginStartErr::unsupported("x".into()),
        LoginStartErr::failed("x".into()),
    ]
    .map(|e| again(e.into_fail()));
    assert_eq!(
        start,
        [
            ("not_found".to_string(), Some(json!("none"))),
            ("unsupported".to_string(), Some(json!("none"))),
            ("failed".to_string(), Some(json!("restart"))),
        ]
    );
    let finish = [
        LoginFinishErr::of(FinishFail::BadPaste),
        LoginFinishErr::of(FinishFail::Refused(crate::common::said::Said::with_raw(
            "x".into(),
            "tail",
        ))),
        LoginFinishErr::of(FinishFail::NoJob),
        LoginFinishErr::of(FinishFail::Slow),
    ]
    .map(|e| again(e.into_fail()));
    assert_eq!(
        finish,
        [
            ("bad_paste".to_string(), Some(json!("paste"))),
            ("refused".to_string(), Some(json!("restart"))),
            ("no_job".to_string(), Some(json!("restart"))),
            ("child_timed_out".to_string(), Some(json!("paste"))),
        ]
    );
    // 码表里除 `bad_args` 外每个码都在上面（漏一个 ⇒ 红：新加的码要定它的 `again`）。
    let seen = |rows: &[(String, Option<Value>)]| -> Vec<String> {
        rows.iter().map(|(c, _)| c.clone()).collect()
    };
    assert_eq!(&LOGIN_START_CODES[1..], seen(&start).as_slice());
    assert_eq!(
        &<LoginFinishErr as Codes>::ALL[1..],
        seen(&finish).as_slice()
    );
    // 换码没成：那一句之外，原话照旧进复制详情。
    let refused = LoginFinishErr::of(FinishFail::Refused(crate::common::said::Said::with_raw(
        "x".into(),
        "tail",
    )))
    .into_fail();
    assert_eq!(
        (refused.message.as_str(), refused.raw.as_deref()),
        ("x", Some("tail"))
    );
}

/// MCP 那一种：名字以 `-` 开头（会被子命令当成旗标）· 空名一律不认，配置读都不读。
#[test]
fn a_mcp_name_that_could_pass_for_a_flag_is_never_listed() {
    assert!(!mcp_listed("claude", None, "--help"));
    assert!(!mcp_listed("claude", None, ""));
}

impl crate::guard_support::Shaped for LoginStarted {
    fn samples() -> Vec<Self> {
        vec![LoginStarted {
            job: "login-1-0".into(),
            url: "https://claude.com/cai/oauth/authorize?state=s".into(),
            paste: PasteKind::Code,
            auto: true,
        }]
    }
}
