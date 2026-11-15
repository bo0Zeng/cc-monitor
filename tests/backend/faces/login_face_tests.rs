//! 会话外的需手动清单：账号清单成品（夹具，照类型造）· MCP 读面（夹具）⇒ 每一项带写好的那一句和怎么解。夹具全是合成的。
use super::*;
use crate::common::said::Said;
use crate::wire::typed::Codes;
use serde_json::{json, Value};

fn row(
    name: &str,
    kind: &str,
    exists: bool,
    ready: bool,
) -> crate::accounts::manage::library::AccountRow {
    crate::accounts::manage::library::test_support::row_with(name, |a| {
        a.auth_kind = kind.to_string();
        a.exists = exists;
        a.auth_ready = ready;
    })
}

fn entry(
    name: &str,
    status: crate::agents::McpStatus,
    login_in: &[Option<&str>],
) -> crate::agents::McpEntry {
    crate::agents::McpEntry {
        scope: "user",
        name: name.to_string(),
        server: json!({}),
        source: "/h/.claude.json".to_string(),
        status,
        login_in: login_in.iter().map(|s| s.map(str::to_string)).collect(),
        seen_ms: None,
    }
}

/// 需登录的号：订阅号 · 目录在 · 还没就绪的那几个（API 号 · 目录不在 · 已登录的不算）；每项 `fix` 指向 `login-start`。
/// 需登录的 MCP：每个号一项，没设账号的那一份 `account: null`；断开 · 停用的不算；每项 `fix` 指向 `login-start {kind: mcp}`（登在那个号上）。
/// 那一家不在 app 里登 MCP ⇒ `fix: null`（只能在会话里登，不列成「点了能解」），并带 `why`：为什么没有解法。
#[test]
fn the_manual_list_names_each_thing_and_how_to_fix_it() {
    let accounts = crate::accounts::manage::library::test_support::list(vec![
        row("work", "subscription", true, false),
        row("0", "subscription", true, false),
        row("api", "api-key", true, false),
        row("gone", "subscription", false, false),
        row("ok", "subscription", true, true),
    ]);
    let mcp = crate::accounts::manage::mcp_share_exec::needs_login_of(&[(
        None,
        crate::agents::McpRead {
            entries: vec![
                entry(
                    "notes",
                    crate::agents::McpStatus::NeedsLogin,
                    &[Some("work")],
                ),
                entry("bare", crate::agents::McpStatus::NeedsLogin, &[None]),
                entry("down", crate::agents::McpStatus::Failed, &[]),
            ],
            dirs: vec![],
            problems: vec![],
        },
    )]);
    let got = json!({ "items": manual_of(crate::agents::REGISTRY, "claude", &accounts, None, &[], Some(("claude", &mcp[..]))).expect("列得出") });
    // 号那一项的解法不带家（家由后端照号表认：同一个号从哪里点进来都是同一个目标）。
    let fix =
        |a: &str| json!({ "command": "login-start", "args": { "kind": "account", "account": a } });
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
              "said": copy_text("beManual.list.accountLogin", &[("account", &crate::accounts::quota::name_words::account_text_of("claude", "0"))]), "fix": fix("0") },
            { "kind": "mcpLogin", "account": "work", "server": "notes", "scope": "user",
              "said": copy_text("beManual.list.mcpLogin", &[("server", "notes"), ("account", "work")]),
              "fix": mcp_fix(json!("work"), "notes") },
            { "kind": "mcpLogin", "account": null, "server": "bare", "scope": "user",
              "said": copy_text("beManual.list.mcpLoginBare", &[("server", "bare")]),
              "fix": mcp_fix(Value::Null, "bare") },
        ]})
    );
    // 那一家不在 app 里登 ⇒ 不列需登录的号（没有解法的不列成「点了能解」）。
    let fake = json!({ "items": manual_of(crate::agents::REGISTRY, "no-such-agent", &accounts, None, &[], None).expect("列得出") });
    assert_eq!(fake, json!({ "items": [] }));
    let no_mcp_login = json!({ "items": manual_of(crate::agents::REGISTRY, "no-such-agent", &accounts, None, &[], Some(("fake", &mcp[..]))).expect("列得出") });
    assert_eq!(no_mcp_login["items"][0]["kind"], "mcpLogin");
    assert_eq!(
        no_mcp_login["items"][0]["fix"],
        Value::Null,
        "不在 app 里登 MCP 的那一家：没有解法"
    );
    assert_eq!(
        no_mcp_login["items"][0]["why"],
        json!(copy_text(
            "beManual.list.mcpNoFix",
            &[("agent", &copy_text("rsAgents.family.unsaid", &[]))]
        )),
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

/// 逐号读出来、两个号的家里各一份同名同配置的 MCP：每个号一项、不按读出的份数重复出项。
#[test]
fn a_server_read_from_two_homes_is_listed_once_per_account() {
    let accounts = crate::accounts::manage::library::test_support::list(vec![]);
    let from = |home: &str| crate::agents::McpEntry {
        source: format!("/h/{home}/.claude.json"),
        ..entry("notes", crate::agents::McpStatus::NeedsLogin, &[Some(home)])
    };
    let mcp = crate::accounts::manage::mcp_share_exec::needs_login_of(&[(
        None,
        crate::agents::McpRead {
            entries: vec![from("a"), from("b"), from("a")],
            dirs: vec![],
            problems: vec![],
        },
    )]);
    let got = json!(manual_of(
        crate::agents::REGISTRY,
        "claude",
        &accounts,
        None,
        &[],
        Some(("claude", &mcp[..]))
    )
    .expect("列得出"));
    let who: Vec<&Value> = got
        .as_array()
        .expect("items")
        .iter()
        .map(|i| &i["account"])
        .collect();
    assert_eq!(who, vec![&json!("a"), &json!("b")], "{got}");
}

/// 口径取全范围：这台活会话所在项目里的项目级 server 要登录 ⇒ 它也进需手动，解法带上那个项目（在那个项目里登）。
#[test]
fn a_project_server_needing_login_in_a_live_session_is_a_manual_item() {
    let accounts = crate::accounts::manage::library::test_support::list(vec![]);
    let project = crate::agents::McpRead {
        entries: vec![crate::agents::McpEntry {
            scope: "project",
            ..entry("tracker", crate::agents::McpStatus::NeedsLogin, &[None])
        }],
        dirs: vec![],
        problems: vec![],
    };
    let mcp = crate::accounts::manage::mcp_share_exec::needs_login_of(&[
        (None, crate::agents::McpRead::default()),
        (Some("/p/app".into()), project),
    ]);
    let got = json!(manual_of(
        crate::agents::REGISTRY,
        "claude",
        &accounts,
        None,
        &[],
        Some(("claude", &mcp[..]))
    )
    .expect("列得出"));
    assert_eq!(
        got,
        json!([{ "kind": "mcpLogin", "account": null, "server": "tracker", "scope": "project",
                 "said": copy_text("beManual.list.mcpLoginBare", &[("server", "tracker")]),
                 "fix": { "command": "login-start",
                          "args": { "kind": "mcp", "agent": "claude", "account": null, "server": "tracker", "projectDir": "/p/app" } } }])
    );
}

/// 盘上有登录、续令牌却被拒（换号那一侧记成「要重新登录」）的号 ⇒ 列「{号} 需重新登录」，解法同样是在 app 里登；
/// 没被点名的已登录号照旧不列。
#[test]
fn an_account_whose_renewal_was_refused_is_listed_as_needing_a_new_login() {
    let accounts = crate::accounts::manage::library::test_support::list(vec![
        row("q", "subscription", true, true),
        row("ok", "subscription", true, true),
    ]);
    let got = json!({ "items": manual_of(crate::agents::REGISTRY, "claude", &accounts, None, &["q".to_string()], None).expect("列得出") });
    assert_eq!(
        got,
        json!({ "items": [
            { "kind": "accountLogin", "agent": "claude", "account": "q",
              "said": copy_text("beManual.list.accountRelogin", &[("account", "q")]),
              "fix": { "command": "login-start", "args": { "kind": "account", "account": "q" } } },
        ]})
    );
}

/// 账号库清单在、却读不懂 ⇒ 回那一句为什么，不回空清单（空清单看起来像「没事」）。
/// 读法照账号清单那一份（坏 JSON 的清单 ⇒ `meta.error` 那一句）；没有清单（没启用多账号）照旧列得出。
#[test]
fn an_unreadable_account_library_is_said_not_listed_as_nothing() {
    use crate::accounts::manage::library::{library_unreadable, list_product_with};
    let root = std::env::temp_dir().join(format!("ccm-manual-unreadable-{}", std::process::id()));
    let accts = root.join("accts");
    std::fs::create_dir_all(&accts).unwrap();
    std::fs::write(accts.join("accounts.json"), "{not json").unwrap();
    let product = list_product_with(&accts, None, &[], &|_| None, "claude", "claude", false);
    let why = library_unreadable(&accts);
    assert_eq!(why, product.meta.error, "同账号清单那一句");
    let fail = manual_of(crate::agents::REGISTRY, "claude", &product, why, &[], None)
        .expect_err("读不懂 ⇒ 不是空清单");
    assert_eq!(Some(fail), product.meta.error, "那一句是为什么");
    let nope = root.join("nope");
    assert_eq!(
        library_unreadable(&nope),
        None,
        "没有清单 ＝ 没启用多账号，不是读不懂"
    );
    let absent = list_product_with(&nope, None, &[], &|_| None, "claude", "claude", false);
    assert_eq!(
        manual_of(crate::agents::REGISTRY, "claude", &absent, None, &[], None)
            .expect("没有清单照列"),
        Vec::new()
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// 入参按 `kind` 分两种；多一格 · 少了必填的 · 不认得的 `kind` 一律拒（不会照旧形状把 MCP 那一种当成号）。
#[test]
fn the_login_target_takes_exactly_its_two_shapes() {
    let ok: LoginStartReq =
        serde_json::from_value(json!({ "kind": "account", "account": "w" })).unwrap();
    assert_eq!(
        ok,
        LoginStartReq::Account {
            agent: None,
            account: "w".into()
        }
    );
    // 带了家照收（对不对得上号表在后端判，见 `an_account_asked_under_another_family_is_refused`）。
    let with: LoginStartReq =
        serde_json::from_value(json!({ "kind": "account", "agent": "claude", "account": "w" }))
            .unwrap();
    assert_eq!(
        with,
        LoginStartReq::Account {
            agent: Some("claude".into()),
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
        json!({ "kind": "account", "account": "w", "server": "x" }),
        json!({ "kind": "account", "agent": "claude" }),
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
        agent: None,
        account: "w".into(),
    };
    let base = json!({ "kind": "account", "account": "w" });
    let with = |v: Value| {
        let mut a = base.clone();
        a["browserHere"] = v;
        crate::wire::typed::take::<LoginStartArgs>(&a)
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
        crate::wire::typed::take::<LoginStartArgs>(&base).expect("收"),
        LoginStartArgs {
            target,
            browser_here: false
        },
        "缺 ⇒ 不在这台"
    );
    for bad in [json!("yes"), json!(1), Value::Null] {
        assert_eq!(
            with(bad.clone()).expect_err("拒").code.as_deref(),
            Some("bad_args"),
            "{bad}"
        );
    }
    let mut extra = base.clone();
    extra["server"] = json!("x");
    assert_eq!(
        crate::wire::typed::take::<LoginStartArgs>(&extra)
            .expect_err("拒")
            .code
            .as_deref(),
        Some("bad_args")
    );
}

/// 会不会自己登完照 `browserHere` 答：浏览器不在这台 ⇒ 回跳到不了，只能粘（不说「会自己登完」）。
/// 登录页是不是那一家自己开了（`opened`）：号那一形自己开浏览器，浏览器在这台才开得到人眼前；MCP 那一形带 `--no-browser`，从不自己开。
#[test]
fn it_says_it_finishes_by_itself_only_when_the_browser_is_here() {
    let face = crate::agents::accounts_face("claude")
        .and_then(|f| f.sign_in)
        .expect("claude 在 app 里登");
    let mcp = crate::agents::mcp_face("claude")
        .and_then(|f| f.sign_in)
        .expect("claude 在 app 里登 MCP");
    for here in [true, false] {
        let started_with = |face| {
            let args = LoginStartArgs {
                target: LoginStartReq::Account {
                    agent: None,
                    account: "w".into(),
                },
                browser_here: here,
            };
            let s = sign_in::Started {
                job: "login-1-0".into(),
                url: "https://claude.com/cai/oauth/authorize?state=s".into(),
                code: None,
            };
            started(s, face, &args)
        };
        assert_eq!(started_with(face).opened, here, "号那一形");
        assert!(!started_with(mcp).opened, "MCP 那一形");
    }
    for here in [true, false] {
        let args = LoginStartArgs {
            target: LoginStartReq::Account {
                agent: None,
                account: "w".into(),
            },
            browser_here: here,
        };
        let s = sign_in::Started {
            job: "login-1-0".into(),
            url: "https://claude.com/cai/oauth/authorize?state=s".into(),
            code: None,
        };
        assert_eq!(started(s, face, &args).auto, here);
    }
}

/// 显示码那一形照声明答（替身家声明，不认哪一家）：`paste: showCode`、回包带那段码、在哪台的浏览器里都登得完（`auto` 恒真）。
#[test]
fn a_shown_code_answers_as_declared() {
    for here in [true, false] {
        let args = LoginStartArgs {
            target: LoginStartReq::Account {
                agent: None,
                account: "w".into(),
            },
            browser_here: here,
        };
        let s = sign_in::Started {
            job: "login-1-0".into(),
            url: "https://login.fake.test/device?ticket=t1".into(),
            code: Some("ABCD-1234".into()),
        };
        let shown = started(s, crate::agents::fake::SHOW_CODE_FACE, &args);
        assert_eq!(
            (shown.paste, shown.code.as_deref(), shown.auto),
            (PasteKind::ShowCode, Some("ABCD-1234"), true)
        );
        assert_eq!(shown.elsewhere, None, "显示码在哪台都登得完");
    }
    assert_eq!(
        serde_json::to_value(PasteKind::ShowCode).unwrap(),
        json!("showCode")
    );
}

/// 不粘回那一形照声明答（替身家声明）：`paste: noPaste`、不带码、只有浏览器在这台才自己登完（`auto` · `opened` 照 `browserHere`）。
#[test]
fn a_no_paste_login_answers_as_declared() {
    for here in [true, false] {
        let args = LoginStartArgs {
            target: LoginStartReq::Account {
                agent: None,
                account: "w".into(),
            },
            browser_here: here,
        };
        let s = sign_in::Started {
            job: "login-1-0".into(),
            url: "https://login.fake.test/device?ticket=t1".into(),
            code: None,
        };
        let wait = started(s, crate::agents::fake::NO_PASTE_FACE, &args);
        assert_eq!(
            (wait.paste, wait.code.as_deref(), wait.auto, wait.opened),
            (PasteKind::NoPaste, None, here, here)
        );
        // 浏览器不在那一台 ⇒ 后端写好「需在那一台完成」那一句（两端照摆，不自己判）；在 ⇒ 没有。
        assert_eq!(
            wait.elsewhere,
            (!here).then(|| copy_text("beLogin.start.elsewhere", &[]))
        );
    }
    assert_eq!(
        serde_json::to_value(PasteKind::NoPaste).unwrap(),
        json!("noPaste")
    );
}

/// 两家混列的账号库（夹具，同 `accounts-list` 不按家过滤）：alpha 是默认起的那一家、beta 是另一家，各声明一形登录
/// （程序 · 号载体都不同）；gamma 不在 app 里登。还有一个认不出是哪一家的号、一个 API 号。
fn two_families() -> (
    Vec<crate::agents::Adapter>,
    crate::accounts::manage::library::AccountsList,
) {
    let with_face = |mut a: crate::agents::Adapter, face: crate::agents::SignInFace| {
        if let Some(f) = a.accounts.as_mut() {
            f.sign_in = Some(face);
        }
        a
    };
    let registry = vec![
        with_face(
            crate::agents::fake::family_row("alpha", "CCM_A_DIR", "a", true, || None),
            crate::agents::SignInFace {
                program: "alpha-login",
                dir_env: "CCM_A_DIR",
                ..crate::agents::fake::SHOW_CODE_FACE
            },
        ),
        with_face(
            crate::agents::fake::family_row("beta", "CCM_B_DIR", "b", false, || None),
            crate::agents::SignInFace {
                program: "beta-login",
                dir_env: "CCM_B_DIR",
                ..crate::agents::fake::SHOW_CODE_FACE
            },
        ),
        crate::agents::fake::family_row("gamma", "CCM_G_DIR", "g", false, || None),
    ];
    let of = |name: &str, kind: Option<&str>| {
        crate::accounts::manage::library::test_support::row_with(name, |a| {
            a.agent = kind.map(cmd_wire::AgentKind::new);
        })
    };
    let api = crate::accounts::manage::library::test_support::row_with("k1", |a| {
        a.agent = Some(cmd_wire::AgentKind::new("alpha"));
        a.auth_kind = acct_core::AUTH_KIND_API_KEY.to_string();
    });
    let list = crate::accounts::manage::library::test_support::list(vec![
        of("b1", Some("beta")),
        of("a1", Some("alpha")),
        of("g1", Some("gamma")),
        of("lost", None),
        api,
    ]);
    (registry, list)
}

/// 号 ⇒ 子命令照号表里那个号带的家拼（不照请求那一家）：混列清单里另一家的号用那一家的程序 · 号载体，默认那一家的号用它自己的；
/// 请求不带家照号表；带了家、对得上照收。登录 · 退出 · 状态都从这一处取。
#[test]
fn each_account_signs_in_with_its_own_family() {
    let (registry, list) = two_families();
    let at = |asked: Option<&str>, name: &str| {
        let a = account_at(&registry, &list, asked, name).expect("认得出");
        (a.agent, a.face.program, a.face.dir_env, a.dir)
    };
    assert_eq!(
        at(None, "b1"),
        ("beta", "beta-login", "CCM_B_DIR", Some("/h/b1".to_string()))
    );
    assert_eq!(
        at(None, "a1"),
        (
            "alpha",
            "alpha-login",
            "CCM_A_DIR",
            Some("/h/a1".to_string())
        )
    );
    assert_eq!(at(Some("beta"), "b1").1, "beta-login", "带了家、对得上");
}

/// 请求里报的家与号表不符 ⇒ `bad_args`（照实说这个号是哪一家的），不照请求那一家起子命令；
/// 没这个号 ⇒ `not_found`；那一家不在 app 里登 · 认不出是哪一家的号 · API 号 ⇒ `unsupported`。登录 · 退出 · 状态同一套码。
#[test]
fn an_account_asked_under_another_family_is_refused() {
    let (registry, list) = two_families();
    let refused = |asked: Option<&str>, name: &str| {
        account_at(&registry, &list, asked, name).err().expect("拒")
    };
    let codes = |r: AccountRefused| {
        (
            cmd_wire::failed::Failed::code_or_fallback(
                r.clone().for_start(&registry).into_said().code,
            ),
            cmd_wire::failed::Failed::code_or_fallback(r.for_once(&registry).into_said().code),
        )
    };
    let wrong = refused(Some("alpha"), "b1");
    assert_eq!(
        wrong.clone().for_once(&registry).into_said().said,
        copy_text(
            "beLogin.account.otherAgent",
            &[
                ("account", "b1"),
                ("own", crate::agents::fake::said_of("beta")),
                ("agent", crate::agents::fake::said_of("alpha")),
            ]
        ),
        "照实说：两家都说各自的叫法，不露 kind"
    );
    assert_eq!(
        refused(None, "g1").for_start(&registry).into_said().said,
        copy_text(
            "beLogin.agent.unsupported",
            &[("agent", crate::agents::fake::said_of("gamma"))]
        ),
        "不在 app 里登的那一家说它的叫法"
    );
    for (r, code) in [
        (wrong, "bad_args"),
        (refused(Some("no-such-agent"), "a1"), "bad_args"),
        (refused(None, "nobody-here"), "not_found"),
        (refused(Some("alpha"), "nobody-here"), "not_found"),
        (refused(None, "g1"), "unsupported"),
        (refused(None, "lost"), "unsupported"),
        (refused(None, "k1"), "unsupported"),
    ] {
        assert_eq!(codes(r), (code.to_string(), code.to_string()));
    }
}

/// MCP 登录带了号：只取请求那一家的号（家照号表比，同账号登录那一判）；号名在账号库里、却是另一家的号 ⇒ 拒（`bad_args`），
/// 不拿那一家的配置目录。同家照旧取目录；API 号也照取（MCP 登录与号怎么登无关）；没这个号 ⇒ `not_found`。
#[test]
fn a_mcp_login_takes_only_an_account_of_its_own_family() {
    let (registry, list) = two_families();
    let dir = |agent: &str, name: &str| mcp_dir_of(&registry, &list, agent, name);
    let code = |r: AccountRefused| {
        cmd_wire::failed::Failed::code_or_fallback(r.for_start(&registry).into_said().code.clone())
    };
    assert_eq!(dir("alpha", "a1").ok(), Some(Some("/h/a1".to_string())));
    assert_eq!(dir("beta", "b1").ok(), Some(Some("/h/b1".to_string())));
    assert_eq!(
        dir("alpha", "k1").ok(),
        Some(Some("/h/k1".to_string())),
        "API 号照取"
    );
    assert_eq!(
        code(dir("alpha", "b1").err().expect("别家的号不取")),
        "bad_args"
    );
    assert_eq!(
        code(dir("beta", "a1").err().expect("别家的号不取")),
        "bad_args"
    );
    assert_eq!(
        code(dir("alpha", "lost").err().expect("认不出家的号不取")),
        "bad_args"
    );
    assert_eq!(
        code(dir("alpha", "nobody-here").err().expect("没这个号")),
        "not_found"
    );
}

/// 两家各有一个同名的号：报了家 ⇒ 取那一家的那一行（别家那一行排在前面也不取）；MCP 登录 · 账号登录同一判。
#[test]
fn a_name_held_by_two_families_resolves_within_the_asked_one() {
    let (registry, _) = two_families();
    let twin = |kind: &str, dir: &str| {
        crate::accounts::manage::library::test_support::row_with("x", |a| {
            a.agent = Some(cmd_wire::AgentKind::new(kind));
            a.config_dir = Some(dir.to_string());
        })
    };
    let list = crate::accounts::manage::library::test_support::list(vec![
        twin("beta", "/h/x-beta"),
        twin("alpha", "/h/x-alpha"),
    ]);
    assert_eq!(
        mcp_dir_of(&registry, &list, "alpha", "x").ok(),
        Some(Some("/h/x-alpha".to_string()))
    );
    assert_eq!(
        mcp_dir_of(&registry, &list, "beta", "x").ok(),
        Some(Some("/h/x-beta".to_string()))
    );
    let at = account_at(&registry, &list, Some("alpha"), "x").expect("同家照取");
    assert_eq!(
        (at.agent, at.dir),
        ("alpha", Some("/h/x-alpha".to_string()))
    );
}

/// 退出 · 状态的入参只要号名；带了家照收（对不对得上同登录那一条）；缺号名 · 多一格照旧拒。
#[test]
fn sign_out_and_status_take_the_account_alone() {
    let take = |v: Value| crate::wire::typed::take::<LoginAccountReq>(&v);
    assert_eq!(
        take(json!({ "account": "w" })).expect("收"),
        LoginAccountReq {
            agent: None,
            account: "w".into()
        }
    );
    assert_eq!(
        take(json!({ "agent": "claude", "account": "w" }))
            .expect("收")
            .agent
            .as_deref(),
        Some("claude")
    );
    for bad in [json!({}), json!({ "account": "w", "server": "x" })] {
        assert_eq!(
            take(bad.clone()).expect_err("拒").code.as_deref(),
            Some("bad_args"),
            "{bad}"
        );
    }
}

/// 登录失败的回包说还能怎么办（`data.again`），每个码一个定值：接着粘 · 重来 · 没路。码表里每个码都有（`bad_args` 不带）。
#[test]
fn each_failure_says_what_can_be_done_next() {
    let again = |f: Said| {
        (
            cmd_wire::failed::Failed::code_or_fallback(f.code.clone()),
            f.data.map(|d| d["again"].clone()),
        )
    };
    let start = [
        LoginStartErr::not_found("x".into()),
        LoginStartErr::unsupported("x".into()),
        LoginStartErr::failed("x".into(), false),
    ]
    .map(|e| again(e.into_said()));
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
    .map(|e| again(e.into_said()));
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
    assert_eq!(&<LoginStartErr as Codes>::ALL[1..], seen(&start).as_slice());
    assert_eq!(
        &<LoginFinishErr as Codes>::ALL[1..],
        seen(&finish).as_slice()
    );
    // 换码没成：那一句之外，原话照旧进复制详情。
    let refused = LoginFinishErr::of(FinishFail::Refused(crate::common::said::Said::with_raw(
        "x".into(),
        "tail",
    )))
    .into_said();
    assert_eq!(
        (refused.said.as_str(), refused.raw.as_deref()),
        ("x", Some("tail"))
    );
}

/// 抽屉那一趟起不来时，失败的 `data` 说在终端窗口里登这条路在不在：只有号那一种目标起不来（`failed`）才在；
/// 没这个号 · 不在 app 里登 · 换码那几样一律不在（界面照它亮不亮那一颗，不按码猜）。
#[test]
fn only_a_failed_account_start_offers_the_terminal_window() {
    let terminal = |f: Said| f.data.map(|d| d["terminal"].clone());
    assert_eq!(
        terminal(LoginStartErr::failed("x".into(), true).into_said()),
        Some(json!(true))
    );
    assert_eq!(
        terminal(LoginStartErr::failed("x".into(), false).into_said()),
        Some(json!(false))
    );
    for e in [
        LoginStartErr::not_found("x".into()),
        LoginStartErr::unsupported("x".into()),
    ] {
        assert_eq!(terminal(e.into_said()), Some(json!(false)));
    }
    for e in [
        LoginFinishErr::of(FinishFail::BadPaste),
        LoginFinishErr::of(FinishFail::NoJob),
        LoginFinishErr::of(FinishFail::Slow),
    ] {
        assert_eq!(terminal(e.into_said()), Some(json!(false)));
    }
    // 号那一种起不来才算：目标形状 ⇒ 这一格。
    let account = LoginStartReq::Account {
        agent: None,
        account: "t1".into(),
    };
    let mcp = LoginStartReq::Mcp {
        agent: "claude".into(),
        account: None,
        server: "s1".into(),
        project_dir: None,
    };
    assert!(terminal_way(&account));
    assert!(!terminal_way(&mcp));
}

/// MCP 那一种：名字以 `-` 开头（会被子命令当成旗标）· 空名一律不认，配置读都不读。
#[test]
fn a_mcp_name_that_could_pass_for_a_flag_is_never_listed() {
    assert!(!mcp_listed("claude", None, "--help"));
    assert!(!mcp_listed("claude", None, ""));
}

/// 推来的「作业自己收了、没登成」那一帧说还能怎么办：作业已经没了 ⇒ 只能从头再起（`again: restart`），界面不用自己推断。
#[test]
fn the_pushed_failure_says_it_must_restart() {
    let ended = crate::accounts::sign_in::SignInEnd::Failed(crate::common::said::Said::with_raw(
        "x".into(),
        "tail",
    ));
    assert_eq!(
        serde_json::to_value(SignInState::of(&ended)).unwrap(),
        json!({"state": "failed", "said": "x", "detail": "tail", "again": "restart"})
    );
}

/// 自家秘密文件（API key 凭据文件 · 中转钥匙文件）权限太宽 ⇒ 会话外的需手动多一项：那一句（哪份文件 ＋ 路径 ＋ 宽在哪）·
/// 没有 app 里的解法（`fix: null`）· 第二行那一句是怎么修；`sessions-needs` 整单里照会话外那一截列出。
/// 不在 · 只给本人 ⇒ 不列；改回只给本人 ⇒ 那一项没了（每问一次现量，不记）。判法只在 `creds_core::perm`。
#[cfg(unix)]
#[test]
fn a_too_wide_secret_file_is_a_manual_item_until_it_is_fixed() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("ccm-secret-files-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let creds = dir.join("apikey-credentials.json");
    let key = dir.join("relay-key");
    let items = || secret_files(Some(&creds), Some(&key));
    assert!(items().is_empty(), "都不在 ⇒ 不列");
    std::fs::write(&creds, "{}").unwrap();
    std::fs::write(&key, "k").unwrap();
    let mode = |p: &std::path::Path, m| {
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap()
    };
    mode(&creds, 0o600);
    mode(&key, 0o600);
    assert!(items().is_empty(), "只给本人 ⇒ 不列");

    mode(&key, 0o644);
    let got = serde_json::to_value(items()).unwrap();
    let how = match creds_core::perm::judge(&creds_core::perm::Protection::Unix {
        mode: 0o644,
        owner: 0,
        owner_name: None,
        me: 0,
    }) {
        creds_core::perm::Verdict::TooWide { how, .. } => how,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        got,
        json!([{
            "kind": "secretFile",
            "said": copy_text("beManual.list.secretFile", &[
                ("what", &copy_text("credsPerm.notice.relayKey", &[])),
                ("path", &key.display().to_string()),
                ("how", &how),
            ]),
            "fix": null,
            "why": copy_text("credsPerm.judge.fixUnix", &[]),
        }])
    );
    // 进整单：会话外那一截。
    let list = crate::observe::recognize::facts::needs_list(vec![], &[], Ok(items()), vec![]);
    let whole = serde_json::to_value(&list).unwrap();
    assert_eq!(whole["items"][0]["kind"], "manual");
    assert_eq!(whole["items"][0]["manual"], got[0]);

    mode(&creds, 0o640);
    let got = items();
    assert_eq!(got.len(), 2, "{got:?}");
    let first = serde_json::to_value(&got[0]).unwrap();
    assert!(
        first["said"]
            .as_str()
            .unwrap()
            .starts_with(&copy_text("credsPerm.notice.apikey", &[])),
        "{first}"
    );

    mode(&creds, 0o600);
    mode(&key, 0o600);
    assert!(items().is_empty(), "改回只给本人 ⇒ 那一项没了");
    std::fs::remove_dir_all(&dir).ok();
}

/// 两家都声明了账号面与 MCP 面：要登录的 MCP 逐家各问一遍、各并进那一家自己那一份（不因为多了一家就一家都不认），
/// 每一项的解法认的是它自己那一家。替身家，账号清单空、各家要登录的那一条认得出是谁的。
#[test]
fn each_family_with_an_mcp_face_lists_its_own_servers_needing_login() {
    use crate::agents::fake;
    let reg = [
        fake::family_row("alpha", "CCM_A_DIR", "a", true, || None),
        fake::family_row("beta", "CCM_B_DIR", "b", false, || None),
    ];
    let items = logins_among(
        &reg,
        None,
        |_| Vec::new(),
        crate::accounts::manage::library::test_support::list(vec![]),
        |kind| {
            vec![crate::accounts::manage::mcp_share_exec::McpNeed {
                name: format!("{kind}-srv"),
                scope: "user",
                login_in: vec![None],
                project: None,
            }]
        },
    )
    .expect("列得出");
    let got: Vec<(String, Option<String>)> = items
        .iter()
        .filter_map(|i| match i {
            ManualItem::McpLogin { server, why, .. } => Some((server.clone(), why.clone())),
            _ => None,
        })
        .collect();
    let names: Vec<&str> = got.iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(
        names,
        vec!["alpha-srv", "beta-srv"],
        "两家各列各的：{got:?}"
    );
    // 替身家不在 app 里登 MCP ⇒ 只有 why；那一句点的是它自己那一家的叫法（传进来的注册表上的，各家各一个）。
    for (server, why) in &got {
        let kind = server.trim_end_matches("-srv");
        assert_eq!(
            why.as_deref(),
            Some(copy_text("beManual.list.mcpNoFix", &[("agent", fake::said_of(kind))]).as_str()),
            "{server} 的解法没认它自己那一家"
        );
    }
}

/// 一家只声明了 MCP 面、没声明账号面：它要登录的 MCP 照样进需手动清单（问的是声明了 MCP 面的每一家，不只问有账号面的）。
/// 没写是哪一家的号归缺省那一家（默认起的那一家，不是排在前面的那一家：前面那一家没有账号面 · 另一家有账号面却不是默认）。
/// 替身家，只有 MCP 面的那一家排在最前。
#[test]
fn a_family_with_only_an_mcp_face_still_lists_its_servers_needing_login() {
    use crate::agents::fake;
    let mut alpha = fake::family_row("alpha", "CCM_A_DIR", "a", true, || None);
    if let Some(f) = alpha.accounts.as_mut() {
        f.sign_in = Some(fake::SHOW_CODE_FACE);
    }
    let reg = [
        crate::agents::Adapter {
            accounts: None,
            ..fake::family_row("beta", "CCM_B_DIR", "b", false, || None)
        },
        fake::family_row("gamma", "CCM_G_DIR", "g", false, || None),
        alpha,
    ];
    let items = logins_among(
        &reg,
        None,
        |_| Vec::new(),
        // 号走真扫号那一路：没写家的照扫号单那一判归缺省那一家（夹具不直接造 `agent`）。
        scanned_list(&reg, "mcponly-family", &[("q", None)]),
        |kind| {
            vec![crate::accounts::manage::mcp_share_exec::McpNeed {
                name: format!("{kind}-srv"),
                scope: "user",
                login_in: vec![None],
                project: None,
            }]
        },
    )
    .expect("列得出");
    let servers: Vec<&str> = items
        .iter()
        .filter_map(|i| match i {
            ManualItem::McpLogin { server, .. } => Some(server.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        servers,
        vec!["beta-srv", "gamma-srv", "alpha-srv"],
        "{items:?}"
    );
    let accounts: Vec<(&str, &str)> = items
        .iter()
        .filter_map(|i| match i {
            ManualItem::AccountLogin { agent, account, .. } => {
                Some((agent.as_str(), account.as_str()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(accounts, vec![("alpha", "q")], "{items:?}");
}

/// 走真扫号那一路（`list_product_among`）的号单：临时号表里每个号一个隔离目录（没有凭据文件 ⇒ 订阅号需登录），
/// 家那一格照给的写（`None` ⇒ 不写）。号是哪一家的照扫号单那一判（`family_of`），不由夹具直接造。
fn scanned_list(
    reg: &[crate::agents::Adapter],
    tag: &str,
    rows: &[(&str, Option<&str>)],
) -> crate::accounts::manage::library::AccountsList {
    let root = test_home::scratch(tag);
    let accts = root.join("accts");
    let lines: Vec<String> = rows
        .iter()
        .map(|(n, agent)| {
            std::fs::create_dir_all(accts.join(n)).unwrap();
            let agent = agent.map_or(String::new(), |k| format!(r#","agent":"{k}""#));
            format!(
                r#"{{"name":"{n}","email":"","configDir":"{a}/{n}","mode":"isolated"{agent}}}"#,
                a = accts.display()
            )
        })
        .collect();
    crate::accounts::manage::library::test_support::write_manifest(
        &accts,
        &format!(
            r#"{{"version":1,"acctsDir":"{a}","accounts":[{l}]}}"#,
            a = accts.display(),
            l = lines.join(",")
        ),
    );
    crate::accounts::manage::library::list_product_among(
        reg,
        &accts,
        None,
        &[],
        &|_| None,
        None,
        reg[0].kind,
        false,
    )
}

/// 替身注册表只有 alpha（能在 app 里登）；号表里 z1 写成没注册的 zeta、n1 没写家。
fn unregistered_family_fixture(
    tag: &str,
) -> (
    [crate::agents::Adapter; 1],
    crate::accounts::manage::library::AccountsList,
) {
    use crate::agents::fake;
    let mut alpha = fake::family_row("alpha", "CCM_A_DIR", "a", true, || None);
    if let Some(f) = alpha.accounts.as_mut() {
        f.sign_in = Some(fake::SHOW_CODE_FACE);
    }
    let reg = [alpha];
    let list = scanned_list(&reg, tag, &[("z1", Some("zeta")), ("n1", None)]);
    (reg, list)
}

/// 写了没注册的家的号不并进任何一家（不列成缺省那一家的需登录）；没写家的照常归缺省那一家。
#[test]
fn an_unregistered_familys_account_is_not_folded_into_the_default_family() {
    let (reg, list) = unregistered_family_fixture("unknownfamily-fold");
    let items = logins_among(&reg, None, |_| Vec::new(), list, |_| Vec::new()).expect("列得出");
    let logins: Vec<(&str, &str)> = items
        .iter()
        .filter_map(|i| match i {
            ManualItem::AccountLogin { agent, account, .. } => {
                Some((agent.as_str(), account.as_str()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(logins, vec![("alpha", "n1")], "{items:?}");
}

/// 扫号单分清「没写家」与「写了、认不出」：没写的归缺省那一家；写了没注册的家不归任何一家，原样留着。
#[test]
fn the_scan_keeps_an_unregistered_family_apart_from_an_unwritten_one() {
    let (_, list) = unregistered_family_fixture("unknownfamily-scan");
    let got: Vec<(&str, Option<&str>, Option<&str>)> = list
        .accounts
        .iter()
        .map(|r| {
            (
                r.scanned.name.as_str(),
                r.scanned.agent.as_ref().map(cmd_wire::AgentKind::as_str),
                r.scanned.unknown_agent.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![("z1", None, Some("zeta")), ("n1", Some("alpha"), None)]
    );
}

/// 这台偏好那份文件在、格式比下限老（读口当没有、照缺省用）⇒ 会话外的需手动多一项（太老的文件照实说、不悄悄当没有）：
/// 那一句（路径 · 盘上第几版 · 此后端认哪几版 · 设置均未生效）· 没有 app 里的解法（`fix: null`）· 第二行那一句是怎么办与原文留在哪。
/// 不在 · 读得懂（含能升上来的旧版）⇒ 不列；每问一次现读，不记。
#[test]
fn an_older_prefs_file_is_a_manual_item_while_it_stays_older() {
    let dir = std::env::temp_dir().join(format!("ccm-own-files-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let prefs = dir.join("prefs.json");
    let _ = std::fs::remove_file(&prefs);
    let items = || own_files(Some(&prefs));
    assert!(items().is_empty(), "不在 ⇒ 不列");
    std::fs::write(&prefs, r#"{"version":1,"resumeInTmux":true}"#).unwrap();
    assert!(items().is_empty(), "读得懂的旧版（能升上来）⇒ 不列");
    std::fs::write(&prefs, r#"{"version":0}"#).unwrap();
    let path = prefs.display().to_string();
    let keep = dir.join("prefs.json.v0").display().to_string();
    assert_eq!(
        serde_json::to_value(items()).unwrap(),
        json!([{
            "kind": "ownFile",
            "said": copy_text("beManual.list.prefsOlder", &[("path", &path), ("theirs", "0"), ("mine", "1–2")]),
            "fix": null,
            "why": copy_text("beManual.why.prefsOlder", &[("keep", &keep)]),
        }])
    );
    // 进整单：会话外那一截。
    let list = crate::observe::recognize::facts::needs_list(vec![], &[], Ok(items()), vec![]);
    let whole = serde_json::to_value(&list).unwrap();
    assert_eq!(whole["items"][0]["kind"], "manual");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// 号表里那一格写了注册表里没有的家：那个号不归缺省那一家（不列成它的需登录），照实出一行
/// 「无法识别 {account} 是哪一家的号」（号表原名，同登录拒的那一句，一个意思一把键）。
#[test]
fn an_account_naming_an_unregistered_family_stays_on_the_manual_list() {
    let (reg, list) = unregistered_family_fixture("unknownfamily-manual");
    let mut list = list;
    list.accounts.retain(|r| r.scanned.name == "z1");
    let items = logins_among(&reg, None, |_| Vec::new(), list, |_| Vec::new()).expect("列得出");
    let said = |i: &ManualItem| match i {
        ManualItem::AccountLogin { said, .. }
        | ManualItem::McpLogin { said, .. }
        | ManualItem::AccountNoAgent { said, .. }
        | ManualItem::SecretFile { said, .. }
        | ManualItem::OwnFile { said, .. } => said.clone(),
    };
    let saids: Vec<String> = items.iter().map(said).collect();
    assert_eq!(
        saids,
        vec![copy_text("beLogin.account.noAgent", &[("account", "z1")])],
        "{items:?}"
    );
}

/// ★ 「这一家不在 app 里登」那一句的另两处（起 MCP 登录那一家没有登录形 · 退出 / 问状态那一家没声明那一条）
/// 照传进来的注册表说那一家的叫法；认不出 ⇒ 照实那一句，不露 kind（替身家）。
#[test]
fn the_not_declared_refusals_name_that_family_not_its_kind() {
    let reg = [
        crate::agents::fake::family_row("alpha", "CCM_A_DIR", "a", true, || None),
        crate::agents::fake::family_row("beta", "CCM_B_DIR", "b", false, || None),
    ];
    let said = |agent: &str| copy_text("beLogin.agent.unsupported", &[("agent", agent)]);
    for (kind, want) in [
        ("beta", said(crate::agents::fake::said_of("beta"))),
        ("alpha", said(crate::agents::fake::said_of("alpha"))),
        ("nope", said(&copy_text("rsAgents.family.unsaid", &[]))),
    ] {
        let start = mcp_unsupported_among(&reg, kind).into_said();
        assert_eq!(
            (start.code.as_deref(), start.said.as_str()),
            (Some("unsupported"), want.as_str()),
            "起登录 {kind}"
        );
        let once = not_declared_among(&reg, kind).into_said();
        assert_eq!(
            (once.code.as_deref(), once.said.as_str()),
            (Some("unsupported"), want.as_str()),
            "退出 · 问状态 {kind}"
        );
    }
}

/// 登录那一面拿的账号库（同 [`library`]：照盘上的库 · key 表那几行与它挂的那一家出成品，不按起的那一家过滤），注册表是夹具。
fn login_library_among(
    registry: &[crate::agents::Adapter],
    accts: &std::path::Path,
    rows: &[String],
    table_agent: &str,
) -> crate::accounts::manage::library::AccountsList {
    crate::accounts::manage::library::list_product_among(
        registry,
        accts,
        None,
        rows,
        &|_| None,
        None,
        table_agent,
        false,
    )
}

/// 盘上两家混列的账号库（alpha 是默认起的那一家）：key 表的行只属于它挂的那一家。另一家的号照它自己的家判——
/// 表挂 beta、beta 的号在表里 ⇒ 是 API 号（不在 app 里登）；表挂 alpha、同一个号名在表里 ⇒ 不算（行不是它那一家的）。
/// 新建会话那一格（newSession）也照号自己的家给，不只给默认那一家的号。
#[test]
fn an_account_of_the_other_family_is_judged_by_its_own_family() {
    let (registry, _) = two_families();
    let root = std::env::temp_dir().join(format!("ccm-login-own-family-{}", std::process::id()));
    let accts = root.join("accts");
    for n in ["a1", "b1"] {
        std::fs::create_dir_all(accts.join(n)).unwrap();
    }
    std::fs::write(
        accts.join("accounts.json"),
        format!(
            r#"{{"version":1,"acctsDir":"{a}","accounts":[
                {{"name":"a1","email":"","configDir":"{a}/a1","mode":"isolated","agent":"alpha"}},
                {{"name":"b1","email":"","configDir":"{a}/b1","mode":"isolated","agent":"beta"}}]}}"#,
            a = accts.display()
        ),
    )
    .unwrap();
    let rows = ["a1".to_string(), "b1".to_string()];
    let list = |table_agent: &str| login_library_among(&registry, &accts, &rows, table_agent);
    let code = |l: &crate::accounts::manage::library::AccountsList, name: &str| {
        account_at(&registry, l, None, name).err().map(|r| {
            cmd_wire::failed::Failed::code_or_fallback(r.for_start(&registry).into_said().code)
        })
    };
    let kind = |l: &crate::accounts::manage::library::AccountsList, name: &str| {
        let r = l
            .accounts
            .iter()
            .find(|r| r.scanned.name == name)
            .expect("列出");
        (r.scanned.auth_kind.clone(), r.new_session)
    };
    let beta_table = list("beta");
    assert_eq!(
        kind(&beta_table, "b1"),
        (acct_core::AUTH_KIND_API_KEY.to_string(), true),
        "表挂 beta ⇒ beta 的号照它自己的家判成 API 号；新建会话照它自己的家给"
    );
    assert_eq!(
        code(&beta_table, "b1").as_deref(),
        Some("unsupported"),
        "API 号不在 app 里登"
    );
    assert_eq!(
        kind(&beta_table, "a1").0,
        acct_core::AUTH_KIND_SUBSCRIPTION,
        "表挂 beta ⇒ alpha 的号不算在表里"
    );
    let alpha_table = list("alpha");
    assert_eq!(
        kind(&alpha_table, "b1"),
        (acct_core::AUTH_KIND_SUBSCRIPTION.to_string(), true),
        "表挂 alpha ⇒ beta 的号不算在表里（号名撞上也不算）"
    );
    assert_eq!(code(&alpha_table, "b1"), None, "照订阅号登");
    assert_eq!(
        kind(&alpha_table, "a1").0,
        acct_core::AUTH_KIND_API_KEY,
        "表挂 alpha ⇒ alpha 的号照它自己的家判成 API 号"
    );
    let launching_beta = crate::accounts::manage::library::list_product_among(
        &registry,
        &accts,
        None,
        &rows,
        &|_| None,
        Some("beta"),
        "alpha",
        false,
    );
    assert_eq!(
        kind(&launching_beta, "a1"),
        (acct_core::AUTH_KIND_API_KEY.to_string(), false),
        "起的是 beta ⇒ alpha 的号照样照它自己的家判成 API 号，只是不进 beta 的选号表"
    );
    let _ = std::fs::remove_dir_all(&root);
}
