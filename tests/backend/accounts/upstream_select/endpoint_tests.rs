//! `accounts/upstream_select/endpoint.rs` 的判据 —— 「这个号这一发走哪、注入什么」的成品。
//!
//! 守的要求：「monitor 侧：注入什么」那张表（搬进后端，一行不改）·
//! 「不带账号的本机会话在全量注入下 ⇒ 走 `/t/<agent>/…` 直通」。
//!
//! # 买到的
//!
//! - E1 决策表：每一格 × 账号三态 × 开关 × 登记与否 == 手写期望（期望不由 `decide_launch` 现算）。
//! - E11 F5：没表态 ⇒ `/t/…/_/…`；与 `_` 同名的行 ⇒ 不注入。
//! - `listening` 只在要注入时才探（不注入就一次都不连）。
//!
//! # 买不到的
//!
//! - 「表里有哪几行」答得对不对：那是 `file_face::rows_at` 的事（`file_face_tests` 的 US1 那条）。
//! - 真中转在不在：读宿主监听状态那一格（`relay::listen::our_relay_listening`，`host_tests`）。

use super::*;
use serde_json::Value;

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
            named("/h/.claude-alt/acct-a"),
            false,
            true,
            inject(&format!("{s}/acct-a"), WhenDown::Refuse, Some("acct-a")),
        ),
        (
            "claude-code",
            named("/h/.claude-alt/acct-a"),
            true,
            true,
            inject(&format!("{s}/acct-a"), WhenDown::Refuse, Some("acct-a")),
        ),
        // 别家 agent 拿同一个 id：表里那一行不是它的 ⇒ 不走 /s/；开关开且登记了 ⇒ /t/ 但撞名 ⇒ 不注
        (
            "codex",
            named("/h/.claude-alt/acct-a"),
            true,
            false,
            Endpoint::None,
        ),
        // ② 开关关 ⇒ 不注
        (
            "claude-code",
            named("/h/.claude-alt/acct-b"),
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
            named("/h/.claude-alt/acct-b"),
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
            named("/h/.claude-alt/名字"),
            true,
            true,
            Endpoint::None,
        ),
    ];
    for (agent, account, all, reg, want) in cases {
        let got = decide_launch(agent, &account, all, relay_route_core::PORT, &rows, reg);
        assert_eq!(
            got, want,
            "agent={agent} account={account:?} 开关={all} 登记={reg}"
        );
    }
    // ⑤ 撞名：标签与某一行同名 ⇒ 不注（`0` · `_` 两个固定标签各一次）
    for (label_row, account) in [("0", LaunchAccount::Base), ("_", LaunchAccount::Undeclared)] {
        let rows = vec![label_row.to_string()];
        assert_eq!(
            decide_launch(
                "claude-code",
                &account,
                true,
                relay_route_core::PORT,
                &rows,
                true
            ),
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
            relay_route_core::PORT,
            &["x".to_string()],
            true
        ),
        Endpoint::None
    );
}

/// 成品：不注入 ⇒ `None`；注入且在听 ⇒ 那个地址；注入而没在听 ⇒ 按「非它不可 / 有它更好」拒或直连（`None`）。
/// 只在要注入时才探中转，探的是交进来的那个口（同机常驻后端被交的口）。
#[test]
fn us1_the_relay_answer_is_the_product_and_probes_only_when_injecting() {
    let rows = vec!["acct-a".to_string()];
    let probes = std::cell::Cell::new(0u32);
    let probe = |p: u16, _: &str| {
        assert_eq!(p, 9911, "探的不是交进来的那个口");
        probes.set(probes.get() + 1);
        true
    };
    let named = named("/h/.claude-alt/acct-a");
    assert_eq!(
        relay_with("claude-code", &named, false, 9911, &rows, &probe)
            .unwrap()
            .as_deref(),
        Some("http://127.0.0.1:9911/s/claude-code/acct-a")
    );
    assert_eq!(probes.get(), 1);
    assert_eq!(
        relay_with(
            "claude-code",
            &LaunchAccount::Base,
            false,
            9911,
            &rows,
            &probe
        )
        .unwrap(),
        None
    );
    assert_eq!(probes.get(), 1, "不注入也去探了中转");
    // 非它不可而没在听 ⇒ 拒，说得出是哪个号。
    let down = relay_with("claude-code", &named, false, 9911, &rows, &|_, _| false).unwrap_err();
    assert!(down.contains("acct-a"), "拒的那一句没说是哪个号：{down}");
    // 有它更好（`/t/`）而没在听 ⇒ 这一发直连。
    let undeclared = LaunchAccount::Undeclared;
    assert_eq!(
        relay_with("claude-code", &undeclared, true, 9911, &rows, &|_, _| false).unwrap(),
        None
    );
    assert_eq!(
        relay_with("claude-code", &undeclared, true, 9911, &rows, &|_, _| true)
            .unwrap()
            .as_deref(),
        Some("http://127.0.0.1:9911/t/claude-code/_")
    );
}

/// 全量开关读起 agent 那个进程自己的环境（缺席 / 其余值 ⇒ 开，`0` ⇒ 关）；口读常驻后端被交的那个（认不出 ⇒ 默认口）。
#[test]
fn the_switch_and_the_port_come_from_the_launching_process_env() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |k: &str| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    };
    assert!(all_sessions_on(&env(&[])));
    assert!(all_sessions_on(&env(&[("CCM_RELAY_ALL_SESSIONS", "1")])));
    assert!(!all_sessions_on(&env(&[("CCM_RELAY_ALL_SESSIONS", "0")])));
    assert_eq!(relay_port(&env(&[])), relay_route_core::PORT);
    assert_eq!(relay_port(&env(&[("CCM_RELAY_PORT", "9911")])), 9911);
    assert_eq!(
        relay_port(&env(&[("CCM_RELAY_PORT", "x")])),
        relay_route_core::PORT
    );
    assert_eq!(
        relay_port(&env(&[("CCM_RELAY_PORT", "0")])),
        relay_route_core::PORT
    );
}

/// 起 agent 那一下插的钥匙读自这台家目录下那份文件，插在第一段；钥匙不在 ⇒ 插不出（ccm 据此当中转不在）。
#[test]
fn the_exec_time_key_comes_from_the_home_key_file_and_goes_into_the_first_segment() {
    let home = std::env::temp_dir().join(format!("ep-key-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let get = |k: &str| (k == "HOME").then(|| home.to_string_lossy().into_owned());
    let url = "http://127.0.0.1:8788/t/claude-code/_";
    use crate::relay::KeyKind;
    assert_eq!(
        keyed_for_exec(url, KeyKind::Full, &get),
        None,
        "钥匙文件不在却插出来了"
    );
    let key = "a".repeat(64);
    let file = home.join(relay_route_core::KEY_FILE_REL);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, &key).unwrap();
    assert_eq!(
        keyed_for_exec(url, KeyKind::Full, &get).as_deref(),
        Some(format!("http://127.0.0.1:8788/{key}/t/claude-code/_").as_str())
    );
    // 只许直通那一把从根钥匙派生（盘上没有第二份）：插出来的不是根钥匙。
    let pass = crate::relay::pass_key_on_disk(&home).expect("根钥匙在就派生得出来");
    assert_ne!(pass, key, "只许直通那一把就是根钥匙本身");
    assert_eq!(
        keyed_for_exec(url, KeyKind::Pass, &get).as_deref(),
        Some(format!("http://127.0.0.1:8788/{pass}/t/claude-code/_").as_str())
    );
    assert_eq!(keyed_for_exec("https://x/t/a/b", KeyKind::Full, &get), None);
    // 插哪一把按那一家的注入格：地址拼进参数的那一家只拿直通那一把。
    assert_eq!(key_kind_of("codex"), KeyKind::Pass);
    assert_eq!(key_kind_of("claude-code"), KeyKind::Full);
    let _ = std::fs::remove_dir_all(&home);
}

// ── 直接敲的 claude 也走中转（可选、用户自己贴）──────────────────────────
// 要求：「往每个号的 settings.json 里写 env / 这个可以变成可选, 像是生成命令一样让用户自己粘贴」——
//   后端只读那台的 `~/.claude/settings.json` 判装没装、对不对，生成的地址指向的端口与钥匙等于那台中转当前的值。
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

/// 根钥匙派生出的只许直通那一把（照中转的派生法在这里现算：HMAC-SHA256(根, "ccm-relay/t") 的小写十六进制）。
fn pass_hex(root: &str) -> String {
    let k = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, root.as_bytes());
    ring::hmac::sign(&k, b"ccm-relay/t")
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn put_key(home: &std::path::Path, key: &str) {
    std::fs::write(home.join(".cc-monitor/relay-key"), key).unwrap();
}

fn put_settings(home: &std::path::Path, raw: &str) {
    std::fs::write(home.join(".claude/settings.json"), raw).unwrap();
}

/// 那一份报告写成 JSON（判据按格比，`url` 为空 ⇒ `null`）。
fn optin(home: &std::path::Path, routed: &[String], registered: bool) -> Value {
    let r = optin_report_at(
        home,
        "claude-code",
        &crate::agents::claudecode::paths::SETTINGS_ENV,
        routed,
        registered,
    );
    serde_json::json!({
        "state": r.state.name(),
        "source": r.source.display().to_string(),
        "url": r.url,
    })
}

/// 要贴的那条地址。
fn snippet_url(v: &Value) -> String {
    v["url"].as_str().expect("该有要贴的地址").to_string()
}

/// ★ 五态 × 地址：没贴 ⇒ 没装 ＋ 地址指向这台中转的口、插的是盘上根钥匙派生的只许直通那一把（那条地址恒是 `/t/`，全权那一把不出后端）；
/// 照旧贴过全权那一把的 ⇒ 过期；照地址合并进去 ⇒ 已装、不再带地址；
/// 中转换了钥匙 ⇒ 过期 ＋ 新地址带新钥匙；口不对 ⇒ 过期；别人的地址 ⇒ 另一态；读不了 / 读不懂 ⇒ 说读不了、不当没装。
#[test]
fn optin_judges_the_pasted_address_against_the_relay_port_and_key_on_disk() {
    let home = optin_home("states");
    put_key(&home, OPTIN_K1);
    let want1 = format!(
        "http://127.0.0.1:8788/{}/t/claude-code/_",
        pass_hex(OPTIN_K1)
    );

    // 文件不在 ⇒ 没装。
    let v = optin(&home, &[], true);
    assert_eq!(v["state"], "absent", "{v}");
    assert_eq!(
        snippet_url(&v),
        want1,
        "地址的口要等于这台中转的口、钥匙段要是只许直通那一把"
    );
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

    // 照地址合并进去（别的设置留着）⇒ 已装、不再带地址。
    put_settings(
        &home,
        &format!(r#"{{"model":"x","env":{{"OTHER":"1","ANTHROPIC_BASE_URL":"{want1}"}}}}"#),
    );
    let v = optin(&home, &[], true);
    assert_eq!(v["state"], "installed", "{v}");
    assert_eq!(v["url"], Value::Null, "已装 ⇒ 钥匙不再出这台");

    // 中转换了钥匙 ⇒ 过期 ＋ 新地址带新钥匙。
    put_key(&home, OPTIN_K2);
    let v = optin(&home, &[], true);
    assert_eq!(v["state"], "stale", "{v}");
    assert_eq!(
        snippet_url(&v),
        format!(
            "http://127.0.0.1:8788/{}/t/claude-code/_",
            pass_hex(OPTIN_K2)
        )
    );
    // 照旧贴的是全权那一把（根钥匙本身）⇒ 过期，要贴的换成直通那一把。
    put_settings(
        &home,
        &format!(
            r#"{{"env":{{"ANTHROPIC_BASE_URL":"http://127.0.0.1:8788/{OPTIN_K2}/t/claude-code/_"}}}}"#
        ),
    );
    let v = optin(&home, &[], true);
    assert_eq!(v["state"], "stale", "{v}");
    assert!(!snippet_url(&v).contains(OPTIN_K2), "{v}");
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

    // 用户自己的端点 ⇒ 另一态（不说成「贴的过期了」），照给地址。
    put_settings(
        &home,
        r#"{"env":{"ANTHROPIC_BASE_URL":"https://p.example"}}"#,
    );
    let v = optin(&home, &[], true);
    assert_eq!(v["state"], "other", "{v}");
    assert!(v["url"].is_string());

    // 读不懂：坏 JSON · 顶层不是对象 · env 不是对象 · 值不是串 ⇒ 读不了，不当没装；照给地址。
    for bad in [
        "{ not json",
        "[]",
        r#"{"env":"x"}"#,
        r#"{"env":{"ANTHROPIC_BASE_URL":5}}"#,
    ] {
        put_settings(&home, bad);
        let v = optin(&home, &[], true);
        assert_eq!(v["state"], "unreadable", "{bad} ⇒ {v}");
        assert!(v["url"].is_string());
    }
    // 读不了：那是个目录。
    std::fs::remove_file(home.join(".claude/settings.json")).unwrap();
    std::fs::create_dir_all(home.join(".claude/settings.json")).unwrap();
    let v = optin(&home, &[], true);
    assert_eq!(v["state"], "unreadable", "{v}");
    let _ = std::fs::remove_dir_all(&home);
}

/// 生成不了的两种：中转还没起来过（没有钥匙文件）· 决策表不给这一条（这一家没登记默认上游 / 表里有个号就叫 `_`）⇒
/// 不给地址；文件里有我们那一形的地址仍判得出「不是现在那一条」。
#[test]
fn optin_without_a_key_or_a_route_gives_no_snippet() {
    let home = optin_home("nokey");
    let v = optin(&home, &[], true);
    assert_eq!(v["state"], "absent");
    assert_eq!(v["url"], Value::Null);
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
        assert_eq!(v["url"], Value::Null, "{routed:?} {registered}");
    }
    let _ = std::fs::remove_dir_all(&home);
}
