//! `accounts/upstream_select/endpoint.rs` 的判据 —— 「这个号这一发走哪、注入什么」的成品。
//!
//! 守的要求：「monitor 侧：注入什么」那张表（本路按与题面搬进后端，一行不改）·
//! 主会话 4D 裁 RT1 F5「不带账号的本机会话在全量注入下 ⇒ 走 `/t/<agent>/…` 直通」。
//!
//! # 买到的
//!
//! - E1 决策表：每一格 × 账号三态 × 开关 × 登记与否 == 手写期望（期望不由 `decide_launch` 现算）。
//! - E11 F5：没表态 ⇒ `/t/…/_/…`；与 `_` 同名的行 ⇒ 不注入。
//! - 线上形状：两条应答的键集恒定（注入与不注入同一组键）；入参闸（缺字段 · 坏 `account`）。入参没有 `key`。
//! - `listening` 只在要注入时才探（不注入就一次都不连）。
//!
//! # 买不到的
//!
//! - 「表里有哪几行」答得对不对：那是 `file_face::rows_at` 的事（`file_face_tests` 的 US1 那条）。
//! - 真中转在不在：读宿主监听状态那一格（`relay::listen::our_relay_listening`，`host_tests`）。

use super::*;

fn named(d: &str) -> LaunchAccount {
    LaunchAccount::Named {
        config_dir: d.to_string(),
    }
}

fn inject(url: &str, when_down: WhenDown, account: Option<&str>) -> Endpoint {
    Endpoint::Inject {
        url: url.to_string(),
        when_down,
        account: account.map(str::to_string),
    }
}

/// E1 ＋ E11：决策表逐格。
#[test]
fn us1_the_launch_table_matches_the_hand_written_one() {
    let rows = vec!["acct-a".to_string(), "clash".to_string()];
    let s = "http://127.0.0.1:8788/s/claude-code";
    let t = "http://127.0.0.1:8788/t/claude-code";
    // (agent, 账号, 开关, 登记了没有, 期望)
    let cases: Vec<(&str, LaunchAccount, bool, bool, Endpoint)> = vec![
        // ① 有行 ⇒ /s/，开关关着也一样（非它不可不看开关）
        (
            "claude-code",
            named("/h/.claude-accts/acct-a"),
            false,
            true,
            inject(&format!("{s}/acct-a"), WhenDown::Refuse, Some("acct-a")),
        ),
        (
            "claude-code",
            named("/h/.claude-accts/acct-a"),
            true,
            true,
            inject(&format!("{s}/acct-a"), WhenDown::Refuse, Some("acct-a")),
        ),
        // 别家 agent 拿同一个 id：表里那一行不是它的 ⇒ 不走 /s/；开关开且登记了 ⇒ /t/ 但撞名 ⇒ 不注
        (
            "codex",
            named("/h/.claude-accts/acct-a"),
            true,
            false,
            Endpoint::None,
        ),
        // ② 开关关 ⇒ 不注
        (
            "claude-code",
            named("/h/.claude-accts/acct-b"),
            false,
            true,
            Endpoint::None,
        ),
        (
            "claude-code",
            LaunchAccount::Base,
            false,
            true,
            Endpoint::None,
        ),
        (
            "claude-code",
            LaunchAccount::Undeclared,
            false,
            true,
            Endpoint::None,
        ),
        // ③ 没登记默认上游 ⇒ 不注
        ("codex", LaunchAccount::Base, true, false, Endpoint::None),
        // ④ ⑥ 标签三态
        (
            "claude-code",
            named("/h/.claude-accts/acct-b"),
            true,
            true,
            inject(&format!("{t}/acct-b"), WhenDown::Direct, None),
        ),
        (
            "claude-code",
            LaunchAccount::Base,
            true,
            true,
            inject(&format!("{t}/0"), WhenDown::Direct, None),
        ),
        // E11 F5：没表态 ⇒ `_`
        (
            "claude-code",
            LaunchAccount::Undeclared,
            true,
            true,
            inject(&format!("{t}/_"), WhenDown::Direct, None),
        ),
        // ④ 推不出 id（空 configDir）⇒ 不注
        ("claude-code", named(""), true, true, Endpoint::None),
        // ④/⑥ 标签当不了路由段 ⇒ 不注（有它更好，不为它拒）
        (
            "claude-code",
            named("/h/.claude-accts/名字"),
            true,
            true,
            Endpoint::None,
        ),
    ];
    for (agent, account, all, reg, want) in cases {
        let got = decide_launch(agent, &account, all, &rows, reg);
        assert_eq!(
            got, want,
            "agent={agent} account={account:?} 开关={all} 登记={reg}"
        );
    }
    // ⑤ 撞名：标签与某一行同名 ⇒ 不注（`0` · `_` 两个固定标签各一次）
    for (label_row, account) in [("0", LaunchAccount::Base), ("_", LaunchAccount::Undeclared)] {
        let rows = vec![label_row.to_string()];
        assert_eq!(
            decide_launch("claude-code", &account, true, &rows, true),
            Endpoint::None,
            "标签 {label_row} 与表里一行同名却注入了"
        );
    }
    // 正控：同一张表、不撞名的那一格照样注入（上面那两条的 None 不是因为别的原因）
    assert_ne!(
        decide_launch(
            "claude-code",
            &LaunchAccount::Base,
            true,
            &["x".to_string()],
            true
        ),
        Endpoint::None
    );
}

/// 成品：不注入 ⇒ `{baseUrl:null}`；注入且在听 ⇒ 那个地址；注入而没在听 ⇒ 按「非它不可 / 有它更好」
/// 拒（`relay_down`）或直连（`null`）。只在要注入时才探中转。原先四格（`listening` / `whenDown` / `account`）交 monitor 再判，那一判收进这里。
#[test]
fn us1_the_launch_answer_is_the_product_and_probes_only_when_injecting() {
    let rows = vec!["acct-a".to_string()];
    let probes = std::cell::Cell::new(0u32);
    let probe = |p: u16| {
        assert_eq!(p, relay_route_core::PORT);
        probes.set(probes.get() + 1);
        true
    };
    let none = answer_launch(
        &json!({"agent":"claude-code","account":{"kind":"base"},"allSessions":false}),
    );
    assert_eq!(none.unwrap(), json!({"baseUrl":null}));
    let named = json!({"agent":"claude-code","account":{"kind":"named","configDir":"/h/.claude-accts/acct-a","name":"a"},"allSessions":false});
    assert_eq!(
        launch_relay_with(&named, &rows, &probe).unwrap().as_deref(),
        Some("http://127.0.0.1:8788/s/claude-code/acct-a")
    );
    assert_eq!(probes.get(), 1);
    assert_eq!(
        launch_relay_with(
            &json!({"agent":"claude-code","account":{"kind":"base"},"allSessions":false}),
            &rows,
            &probe
        )
        .unwrap(),
        None
    );
    assert_eq!(probes.get(), 1, "不注入也去探了中转");
    // 非它不可而没在听 ⇒ 拒，说得出是哪个号。
    let down = launch_relay_with(&named, &rows, &|_| false).unwrap_err();
    assert_eq!(down.0, "relay_down");
    assert!(
        down.1.contains("acct-a"),
        "拒的那一句没说是哪个号：{}",
        down.1
    );
    // 有它更好（`/t/`）而没在听 ⇒ 这一发直连。
    let direct = launch_relay_with(
        &json!({"agent":"claude-code","allSessions":true}),
        &rows,
        &|_| false,
    );
    assert_eq!(direct.unwrap(), None);
    assert_eq!(
        launch_relay_with(
            &json!({"agent":"claude-code","allSessions":true}),
            &rows,
            &|_| true
        )
        .unwrap()
        .as_deref(),
        Some("http://127.0.0.1:8788/t/claude-code/_")
    );
}

/// 入参闸：每一形都 `bad_args`，且不探中转。
#[test]
fn us1_bad_launch_args_are_refused_before_anything_is_probed() {
    let probe = |_: u16| -> bool { panic!("入参不对还去探了中转") };
    for bad in [
        json!({"account":null,"allSessions":true}),
        json!({"agent":"claude-code"}),
        json!({"agent":"claude-code","allSessions":"yes"}),
        json!({"agent":"claude-code","allSessions":true,"account":{"kind":"named"}}),
        json!({"agent":"claude-code","allSessions":true,"account":{"kind":"other"}}),
        json!({"agent":"claude-code","allSessions":true,"account":"base"}),
        // 注册表里没有的适配器 id（wire kind 不是适配器 id）⇒ 拒，不当成「这一家没有表」往下走。
        json!({"agent":"claude","allSessions":true}),
        json!({"agent":"claud-code","allSessions":true}),
    ] {
        let got = launch_relay_with(&bad, &[], &probe);
        assert!(matches!(got, Err(("bad_args", _))), "{bad} ⇒ {got:?}");
    }
}

/// `apikey-routing`：回的是传进来的 configDir 原样（有行的那几个）＋ 探针的答案；别家 agent ⇒ 空集。
#[test]
fn us1_the_routing_answer_is_the_routed_dirs_and_the_probe() {
    let rows = vec!["acct-a".to_string()];
    let dirs = json!(["/h/.claude-accts/acct-a", "/h/.claude-accts/acct-b"]);
    assert_eq!(
        answer_routing_with(
            &json!({"agent":"claude-code","configDirs":dirs}),
            &rows,
            &|_| true
        )
        .unwrap(),
        json!({"routed":["/h/.claude-accts/acct-a"],"running":true})
    );
    assert_eq!(
        answer_routing_with(&json!({"agent":"codex","configDirs":dirs}), &rows, &|_| {
            false
        })
        .unwrap(),
        json!({"routed":[],"running":false})
    );
    for bad in [
        json!({"configDirs":[]}),
        json!({"agent":"claude-code"}),
        json!({"agent":"claude-code","configDirs":[1]}),
    ] {
        assert!(
            matches!(
                answer_routing_with(&bad, &rows, &|_| true),
                Err(("bad_args", _))
            ),
            "{bad}"
        );
    }
}

/// ★★ 跨语言金样：三条成品（`apikey-read` · `apikey-routing` · `launch-endpoint`）对同一份夹具 ==
/// `tests/__fixtures__/apikey.golden.json`（夹具根替换成 `<root>`）。另一个读者是 TS 解码器（`tests/frontend/ui/apikey-reads.vitest.ts`）
/// ⇒ 两侧异源：后端改一个键名本条红，TS 解码器改一个键名那边红。金样手写落盘（本条红时印出现打的成品，人读过再改）。
/// 守的要求：「线上形状由一份跨语言金样钉住（后端测试产出 == 金样 · TS 解码器读同一份）」。
#[test]
fn us1_the_apikey_products_match_the_cross_language_golden() {
    let root = std::env::temp_dir().join(format!("ccm-us1-golden-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("apikey-credentials.json");
    std::fs::write(&path, r#"{"accounts":{"work":{"api_key":"sk-golden-0123456789"},"bad-url":{"api_key":"K","base_url":"ftp://x"}}}"#).unwrap();
    let rows = super::super::file_face::rows_at_with(&path, &|_| None);
    let mut read = super::super::file_face::read_at(&path);
    // 权限那一句随跑的机器的 umask 变（不是形状）⇒ 金样里固定成 null。
    read["notice"] = Value::Null;
    let got = json!({
        "apikey-read": read,
        "apikey-routing": answer_routing_with(
            &json!({"agent":"claude-code","configDirs":["/h/.claude-accts/work","/h/.claude-accts/bad-url","/h/.claude-accts/me"]}),
            &rows, &|_| true).unwrap(),
        "launch-endpoint": json!({ "baseUrl": launch_relay_with(
            &json!({"agent":"claude-code","account":{"kind":"named","configDir":"/h/.claude-accts/work","name":"work"},"allSessions":false}),
            &rows, &|_| true).unwrap() }),
    });
    let got: Value = serde_json::from_str(
        &got.to_string()
            .replace(&root.to_string_lossy().to_string(), "<root>"),
    )
    .unwrap();
    let want: Value =
        serde_json::from_str(include_str!("../../../__fixtures__/apikey.golden.json"))
            .expect("金样不是合法 JSON");
    let _ = std::fs::remove_dir_all(&root);
    assert!(!got.to_string().contains("sk-golden"), "明文进了成品");
    assert_eq!(
        got,
        want,
        "帧面成品与跨语言金样不一致。现打：\n{}",
        serde_json::to_string_pretty(&got).unwrap()
    );
}

// ── `relay-optin`：直接敲的 claude 也走中转（可选、生成让你贴）──────────────────────────
// 要求（用户原话）：「往每个号的 settings.json 里写 env / 这个可以变成可选, 像是生成命令一样让用户自己粘贴」——
//   后端只读那台的 `~/.claude/settings.json` 判装没装、对不对，生成的片段指向的端口与钥匙等于那台中转当前的值。
// 异源：钥匙文件与设置文件由本判据现写进临时家目录；期望的地址是手写字面量（不由构造口现算）。

/// 夹具家目录（临时目录；钥匙文件 ＋ claude 设置文件都由各条判据现写）。
fn optin_home(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("relay-optin-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join(".cc-monitor")).unwrap();
    std::fs::create_dir_all(d.join(".claude")).unwrap();
    d
}

const OPTIN_K1: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const OPTIN_K2: &str = "2222222222222222222222222222222222222222222222222222222222222222";

fn put_key(home: &std::path::Path, key: &str) {
    std::fs::write(home.join(".cc-monitor/relay-key"), key).unwrap();
}

fn put_settings(home: &std::path::Path, raw: &str) {
    std::fs::write(home.join(".claude/settings.json"), raw).unwrap();
}

fn optin(home: &std::path::Path, routed: &[String], registered: bool) -> Value {
    optin_at(
        home,
        "claude-code",
        &crate::agents::claudecode::paths::SETTINGS_ENV,
        routed,
        registered,
        &|p| p == 8788,
    )
}

/// 片段里那一个地址（解析片段本身，不比原文排版）。
fn snippet_url(v: &Value) -> String {
    let s = v["snippet"].as_str().expect("该有片段");
    let parsed: Value = serde_json::from_str(s).expect("片段不是 JSON");
    parsed["env"]["ANTHROPIC_BASE_URL"]
        .as_str()
        .expect("片段里没有那个键")
        .to_string()
}

/// ★ 五态 × 片段：没贴 ⇒ 没装 ＋ 片段指向这台中转的口与盘上那把钥匙；照片段合并进去 ⇒ 已装、不再带片段；
/// 中转换了钥匙 ⇒ 过期 ＋ 新片段带新钥匙；口不对 ⇒ 过期；别人的地址 ⇒ 另一态；读不了 / 读不懂 ⇒ 说读不了、不当没装。
#[test]
fn optin_judges_the_pasted_address_against_the_relay_port_and_key_on_disk() {
    let home = optin_home("states");
    put_key(&home, OPTIN_K1);
    let want1 = format!("http://127.0.0.1:8788/{OPTIN_K1}/t/claude-code/_");

    // 文件不在 ⇒ 没装。
    let v = optin(&home, &[], true);
    assert_eq!(v["state"], "absent", "{v}");
    assert_eq!(
        snippet_url(&v),
        want1,
        "片段的口与钥匙要等于这台中转现在那一份"
    );
    assert_eq!(v["listening"], true);
    assert_eq!(v["note"], "");
    assert_eq!(v["missing"], "");
    assert!(
        v["source"]
            .as_str()
            .unwrap()
            .ends_with(".claude/settings.json"),
        "{v}"
    );
    // 在而 env 里没那个键（别的设置都在）⇒ 仍是没装。
    put_settings(&home, r#"{"model":"x","env":{"OTHER":"1"}}"#);
    assert_eq!(optin(&home, &[], true)["state"], "absent");
    put_settings(&home, r#"{"env":{"ANTHROPIC_BASE_URL":""}}"#);
    assert_eq!(optin(&home, &[], true)["state"], "absent", "空串当没写");

    // 照片段合并进去（别的设置留着）⇒ 已装、不再带片段。
    put_settings(
        &home,
        &format!(r#"{{"model":"x","env":{{"OTHER":"1","ANTHROPIC_BASE_URL":"{want1}"}}}}"#),
    );
    let v = optin(&home, &[], true);
    assert_eq!(v["state"], "installed", "{v}");
    assert_eq!(v["snippet"], Value::Null, "已装 ⇒ 钥匙不再出这台");
    assert_eq!(v["missing"], "");

    // 中转换了钥匙 ⇒ 过期 ＋ 新片段带新钥匙。
    put_key(&home, OPTIN_K2);
    let v = optin(&home, &[], true);
    assert_eq!(v["state"], "stale", "{v}");
    assert_eq!(
        snippet_url(&v),
        format!("http://127.0.0.1:8788/{OPTIN_K2}/t/claude-code/_")
    );
    // 口不对（钥匙对）⇒ 过期；没带钥匙段的老形 ⇒ 过期。
    put_settings(
        &home,
        &format!(
            r#"{{"env":{{"ANTHROPIC_BASE_URL":"http://127.0.0.1:9999/{OPTIN_K2}/t/claude-code/_"}}}}"#
        ),
    );
    assert_eq!(optin(&home, &[], true)["state"], "stale");
    put_settings(
        &home,
        r#"{"env":{"ANTHROPIC_BASE_URL":"http://127.0.0.1:8788/t/claude-code/_"}}"#,
    );
    assert_eq!(optin(&home, &[], true)["state"], "stale");

    // 用户自己的端点 ⇒ 另一态（不说成「贴的过期了」），照给片段。
    put_settings(
        &home,
        r#"{"env":{"ANTHROPIC_BASE_URL":"https://p.example"}}"#,
    );
    let v = optin(&home, &[], true);
    assert_eq!(v["state"], "other", "{v}");
    assert!(v["snippet"].is_string());

    // 读不懂：坏 JSON · 顶层不是对象 · env 不是对象 · 值不是串 ⇒ 读不了，不当没装；照给片段。
    for bad in [
        "{ not json",
        "[]",
        r#"{"env":"x"}"#,
        r#"{"env":{"ANTHROPIC_BASE_URL":5}}"#,
    ] {
        put_settings(&home, bad);
        let v = optin(&home, &[], true);
        assert_eq!(v["state"], "unreadable", "{bad} ⇒ {v}");
        assert_ne!(v["note"], "", "读不了要说为什么：{bad}");
        assert!(v["snippet"].is_string());
    }
    // 读不了：那是个目录。
    std::fs::remove_file(home.join(".claude/settings.json")).unwrap();
    std::fs::create_dir_all(home.join(".claude/settings.json")).unwrap();
    let v = optin(&home, &[], true);
    assert_eq!(v["state"], "unreadable", "{v}");
    assert_ne!(v["note"], "");
    let _ = std::fs::remove_dir_all(&home);
}

/// 生成不了的两种：中转还没起来过（没有钥匙文件）· 决策表不给这一条（这一家没登记默认上游 / 表里有个号就叫 `_`）⇒
/// 不给片段、说为什么；文件里有我们那一形的地址仍判得出「不是现在那一条」。
#[test]
fn optin_without_a_key_or_a_route_gives_no_snippet_and_says_why() {
    let home = optin_home("nokey");
    let v = optin(&home, &[], true);
    assert_eq!(v["state"], "absent");
    assert_eq!(v["snippet"], Value::Null);
    assert_ne!(v["missing"], "", "没钥匙要说为什么生成不了");
    put_settings(
        &home,
        &format!(
            r#"{{"env":{{"ANTHROPIC_BASE_URL":"http://127.0.0.1:8788/{OPTIN_K1}/t/claude-code/_"}}}}"#
        ),
    );
    assert_eq!(
        optin(&home, &[], true)["state"],
        "stale",
        "没有现在那一把 ⇒ 贴过的那一把不是现在的"
    );

    put_key(&home, OPTIN_K1);
    for (routed, registered) in [(vec![], false), (vec!["_".to_string()], true)] {
        let v = optin(&home, &routed, registered);
        assert_eq!(v["snippet"], Value::Null, "{routed:?} {registered}");
        assert_ne!(v["missing"], "");
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// 成品两侧对拍：贴过、钥匙换了的那一台 ⇒ 整份成品 == 金样（界面解码器 `relay-optin-reads.ts` 读同一份）。
#[test]
fn optin_product_is_the_golden_the_ui_decodes() {
    let home = optin_home("golden");
    put_key(&home, OPTIN_K2);
    put_settings(
        &home,
        &format!(
            r#"{{"env":{{"ANTHROPIC_BASE_URL":"http://127.0.0.1:8788/{OPTIN_K1}/t/claude-code/_"}}}}"#
        ),
    );
    let got = serde_json::to_string_pretty(&optin(&home, &[], true))
        .unwrap()
        .replace(&home.display().to_string(), "/HOME");
    let want = include_str!("../../../__fixtures__/relay-optin.golden.json");
    assert_eq!(
        got.trim(),
        want.trim(),
        "成品形状变了 —— 界面解码器读的是同一份金样，两边要一起改"
    );
    let _ = std::fs::remove_dir_all(&home);
}
