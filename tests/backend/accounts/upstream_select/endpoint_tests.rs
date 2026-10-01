//! `accounts/upstream_select/endpoint.rs` 的判据 —— 「这个号这一发走哪、注入什么」的成品。
//!
//! 守的要求：「monitor 侧：注入什么」那张表（本路按与题面搬进后端，一行不改）·
//! 主会话 4D 裁 RT1 F5「不带账号的本机会话在全量注入下 ⇒ 走 `/t/<agent>/…` 直通」。
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
    let probe = |p: u16| {
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
    let down = relay_with("claude-code", &named, false, 9911, &rows, &|_| false).unwrap_err();
    assert!(down.contains("acct-a"), "拒的那一句没说是哪个号：{down}");
    // 有它更好（`/t/`）而没在听 ⇒ 这一发直连。
    let undeclared = LaunchAccount::Undeclared;
    assert_eq!(
        relay_with("claude-code", &undeclared, true, 9911, &rows, &|_| false).unwrap(),
        None
    );
    assert_eq!(
        relay_with("claude-code", &undeclared, true, 9911, &rows, &|_| true)
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

/// 钥匙拼进地址的那一形（钥匙在第一段，路由不变）；不是构造口的产物 ⇒ 拼不出。
#[test]
fn the_key_goes_into_the_first_segment_and_nowhere_else() {
    let key = "a".repeat(64);
    assert_eq!(
        keyed_base_url("http://127.0.0.1:8788/t/claude-code/_", &key).as_deref(),
        Some(format!("http://127.0.0.1:8788/{key}/t/claude-code/_").as_str())
    );
    assert_eq!(keyed_base_url("https://x/t/a/b", &key), None);
}

/// `apikey-routing`：回的是传进来的 configDir 原样（有行的那几个）＋ 探针的答案；别家 agent ⇒ 空集。
#[test]
fn us1_the_routing_answer_is_the_routed_dirs_and_the_probe() {
    let rows = vec!["acct-a".to_string()];
    let dirs = json!(["/h/.claude-alt/acct-a", "/h/.claude-alt/acct-b"]);
    assert_eq!(
        answer_routing_with(
            &json!({"agent":"claude-code","configDirs":dirs}),
            &rows,
            &|_| true
        )
        .unwrap(),
        json!({"routed":["/h/.claude-alt/acct-a"],"running":true})
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

/// ★★ 跨语言金样：两条成品（`apikey-read` · `apikey-routing`）对同一份夹具 ==
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
            &json!({"agent":"claude-code","configDirs":["/h/.claude-alt/work","/h/.claude-alt/bad-url","/h/.claude-alt/me"]}),
            &rows, &|_| true).unwrap(),
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
