//! 开窗：策略交回 `warm` ⇒ 后端只收订阅号 · 发得出去 · 5 小时窗没在计时的；同一个号 15 分钟内只发一次。
//! 判走后端的 `turn`（策略是一段 Rhai），发那一下只认号（[`Warm::claim`]），不起进程、不往真号发一句。

use super::*;
use crate::accounts::quota::rules::code;
use crate::accounts::quota::rules::decide::{self, Ctx, Sigma, Strategy, Verdict};
use crate::accounts::quota::rules::Acct;
use crate::accounts::quota::strategy::{Fail, FailKind};

const T: u64 = 1_800_000_000;
const AGENT: &str = "claude-code";

struct Script(code::Code);

impl Strategy for Script {
    fn decide(&self, ctx: &Ctx<'_>) -> Result<Verdict, Fail> {
        self.0.decide(ctx)
    }
}

fn script(body: &str) -> Script {
    Script(code::compile(&format!("fn decide(ctx) {{ {body} }}")).expect("编得过"))
}

fn acct(id: &str, api: bool, resets5h: Option<u64>) -> Acct {
    Acct {
        agent: String::new(),
        id: id.into(),
        api,
        used: [0.0, 0.0],
        resets: [resets5h, None],
        refused_until: None,
        barred: None,
        rate: [None, None],
        per_pct: [None, None],
        weekly: false,
        others: None,
        past: Default::default(),
    }
}

fn judge(p: &Script, facts: &[Acct], t: u64) -> decide::Decision {
    let sigma = Sigma {
        start: "an".into(),
        current: "an".into(),
        ..Sigma::default()
    };
    decide::turn(&sigma, facts, &serde_json::json!({}), p, 0, t, &[])
}

/// ★ 策略交回 `warm: ["an", "q"]`：an 窗关着 ⇒ 收、发一句；q 窗开着 ⇒ 不收；15 分钟内再交一次 an ⇒ 不再发，过了 15 分钟又发。
#[test]
fn only_closed_windows_are_warmed_and_once_in_fifteen_minutes() {
    let p = script(r#"#{ order: ["an"], warm: ["an", "q"] }"#);
    let facts = [acct("an", false, None), acct("q", false, Some(T + 3600))];
    let d = judge(&p, &facts, T);
    assert_eq!(d.warm, ["an"]);
    assert_eq!(d.warm_dropped, ["q"]);
    let w = Warm::default();
    assert_eq!(w.claim(AGENT, &d.warm, T), ["an"], "头一次 ⇒ 发");
    let again = judge(&p, &facts, T + 600);
    assert_eq!(again.warm, ["an"], "判那一侧照事实收");
    assert!(
        w.claim(AGENT, &again.warm, T + 600).is_empty(),
        "15 分钟内 ⇒ 不再发"
    );
    assert!(w.claim(AGENT, &again.warm, T + AGAIN - 1).is_empty());
    assert_eq!(
        w.claim(AGENT, &again.warm, T + AGAIN),
        ["an"],
        "过了 15 分钟 ⇒ 又发"
    );
    assert!(
        w.claim("codex", &again.warm, T + 1).len() == 1,
        "另一家的同名号另算"
    );
}

/// ★ 不收的几种：按量号 · 不在号列表 · 被拒（发不出去）· 重置时刻已过的窗算关着、收；同一个号交两次只算一次。
#[test]
fn api_keys_unlisted_and_refused_accounts_are_not_warmed() {
    let p = script(r#"#{ order: ["an"], warm: ["k", "ghost", "r", "old", "old"] }"#);
    let mut r = acct("r", false, None);
    r.refused_until = Some(T + 600);
    let facts = [
        acct("an", false, None),
        acct("k", true, None),
        r,
        acct("old", false, Some(T - 1)),
    ];
    let d = judge(&p, &facts, T);
    assert_eq!(d.warm, ["old"]);
    assert_eq!(d.warm_dropped, ["k", "ghost", "r"]);
}

/// ★ 不交 `warm` ⇒ 一个都不发；`warm` 不是号的数组 ⇒ 形状不对（这一判走兜底，也不发）。
#[test]
fn warm_is_an_optional_list_of_account_ids() {
    let facts = [acct("an", false, None)];
    assert!(judge(&script(r#"#{ order: ["an"] }"#), &facts, T)
        .warm
        .is_empty());
    for bad in [
        r#"#{ order: ["an"], warm: "an" }"#,
        r#"#{ order: ["an"], warm: [1] }"#,
    ] {
        let s = script(bad);
        let (sigma, settings) = (Sigma::default(), serde_json::Value::Null);
        let ctx = decide::ctx_of(&sigma, &facts, &settings, 0, T);
        let e = s.decide(&ctx).expect_err(bad);
        assert_eq!(e.kind, FailKind::BadShape, "{bad}");
        let d = judge(&s, &facts, T);
        assert!(d.strategy_error.is_some() && d.warm.is_empty(), "{bad}");
    }
}

/// ★ 那一发回来了 ⇒ 记下回来的重置时刻；发出的时刻是认下的那一刻。
#[test]
fn a_landed_send_keeps_when_it_went_and_the_reset_it_brought() {
    let w = Warm::default();
    assert_eq!(w.sent(AGENT, "an"), None);
    w.claim(AGENT, &["an".to_string()], T);
    assert_eq!(
        w.sent(AGENT, "an"),
        Some(Sent {
            at: T,
            resets_at: None
        })
    );
    w.landed(AGENT, "an", Some(T + 5 * 3600));
    assert_eq!(
        w.sent(AGENT, "an"),
        Some(Sent {
            at: T,
            resets_at: Some(T + 5 * 3600)
        })
    );
}

/// ★ 那一发没成 ⇒ 当场放开（不占 15 分钟）、记下几点 · 为什么；不自动重发，下一次谁叫谁发。
/// 正在发的那一发照样挡着另一次；成了才占，成了之后「上次没成」那一条不再说。
#[test]
fn a_failed_send_does_not_hold_and_is_told_as_failed() {
    let w = Warm::default();
    let an = ["an".to_string()];
    assert_eq!(w.claim(AGENT, &an, T), ["an"]);
    assert!(w.claim(AGENT, &an, T + 1).is_empty(), "正在发 ⇒ 不另发一次");
    let why = Said::new("failed", "起不来".into()).and_raw(Some("exit 1: boom"));
    w.failed(AGENT, "an", T + 30, &why);
    assert_eq!(
        w.last_failed(AGENT, "an"),
        Some(WarmFailed {
            at: T + 30,
            why: crate::common::cells::Words("起不来".into()),
            raw: Some("exit 1: boom".into()),
        })
    );
    assert_eq!(w.sent(AGENT, "an"), None, "没成的不当发过");
    assert_eq!(w.claim(AGENT, &an, T + 31), ["an"], "没成的不占：再叫就发");
    w.landed(AGENT, "an", Some(T + 5 * 3600));
    assert_eq!(w.last_failed(AGENT, "an"), None, "成了 ⇒ 不再说上次没成");
    assert!(w.claim(AGENT, &an, T + 60).is_empty(), "成了才占");
}

/// ★ 认下那一发怎么收场只记一处（`booked`；`kick` 与命令经 [`send_booked`] 交给它）：没成 ⇒ 回错照旧、那一本记下没成、不占。
/// 这一家没有开窗那一形 ⇒ 不起进程就没成，拿它当没成的那一发。
#[test]
fn a_send_that_fails_is_booked_as_failed() {
    let home = std::env::temp_dir().join(format!("ccm-warm-booked-{}", std::process::id()));
    let own = crate::platform::fs::Own::issue_data_at(&home);
    let acct = format!("booked-{}", std::process::id());
    assert_eq!(
        book().claim("nobody", std::slice::from_ref(&acct), T),
        [acct.clone()]
    );
    let err = send_booked("nobody", &acct, &own).expect_err("没有开窗那一形");
    let f = book().last_failed("nobody", &acct).expect("记下没成");
    assert_eq!(f.why.0, err.said);
    assert_eq!(book().sent("nobody", &acct), None, "没成的不占");
}

/// ★ 交给 ccm 的那一行：适配层那几个参数在 `--` 左边（交那一家），右边点名号 · 工作目录 · 这一发不轮换；
/// Claude 那一形是 haiku 一句、不给工具、不落会话记录，起它时关掉思考。
#[test]
fn the_ccm_line_names_the_account_and_does_not_rotate() {
    let face = crate::agents::route_named(AGENT)
        .and_then(crate::agents::warm_of)
        .expect("Claude 有开窗那一形");
    let got: Vec<String> = line(
        &face,
        "an",
        std::path::Path::new("/h/.cc-monitor/autostart"),
    )
    .into_iter()
    .map(|s| s.to_string_lossy().into_owned())
    .collect();
    let end = got.iter().rposition(|w| w == "--").expect("有 --");
    assert_eq!(
        &got[end..],
        [
            "--",
            "--account",
            "an",
            "--cwd",
            "/h/.cc-monitor/autostart",
            "--no-rotation"
        ]
    );
    let left = &got[..end];
    for w in [
        "-p",
        "haiku",
        "--no-session-persistence",
        "--strict-mcp-config",
    ] {
        assert!(left.iter().any(|x| x == w), "{w} 不在左边：{left:?}");
    }
    let tools = left.iter().position(|x| x == "--tools").expect("--tools");
    assert_eq!(left[tools + 1], "", "不给工具");
    assert!(face.env_set.contains(&("MAX_THINKING_TOKENS", "0")));
}

/// ★ 认号同轮换那一个谓词（`relay_route_core::account_ok`）：过不了路由段闸的 · 「没说是哪个号」那一段（`_`）都当入参不对，不起进程。
#[test]
fn an_account_id_rotation_would_refuse_is_bad_args() {
    let home = std::env::temp_dir().join(format!("ccm-warm-ids-{}", std::process::id()));
    let own = crate::platform::fs::Own::issue_data_at(&home);
    for bad in ["_", "", "../x", "a/b"] {
        let req = QuotaWarmReq {
            agent: crate::agents::AgentKind::new("claude"),
            account: bad.into(),
        };
        let err = answer(&req, &own, T).expect_err(bad);
        assert_eq!(
            cmd_wire::failed::Failed::code_or_fallback(err.code.clone()),
            "bad_args",
            "{bad:?}"
        );
    }
}

/// ★ 开窗回的 5 小时重置时刻照事实那一处取窗口（[`crate::accounts::quota::facts::current`] ＋ `pick`）：
/// 那一格不在它来源最近一份里、又过了它自己的重置时刻 ⇒ 不回；没过 ⇒ 照它回。
#[test]
fn the_5h_reset_after_a_warm_is_taken_like_the_facts_take_it() {
    use crate::agents::{QuotaReading, QuotaWindow};
    let reading = |names: &[(&str, u64)]| QuotaReading {
        status: None,
        refused: false,
        limiting: None,
        resets_at: None,
        windows: names
            .iter()
            .map(|(n, r)| QuotaWindow {
                name: (*n).to_string(),
                used: Some(0.1),
                resets_at: Some(*r),
                warned_at: None,
            })
            .collect(),
        overage: None,
    };
    let l = super::super::ledger::Ledger::at(None);
    let own = crate::platform::fs::Own::for_tests();
    let rec = |r: QuotaReading, at: u64| {
        super::super::ledger::record_seen(&l, &own, None, AGENT, "q", r, at);
    };
    rec(
        reading(&[("five_hour", T + 1_000), ("seven_day", T + 90_000)]),
        T,
    );
    rec(reading(&[("seven_day", T + 90_000)]), T + 600);
    assert_eq!(resets_5h(&l, AGENT, "q", T + 900), Some(T + 1_000));
    assert_eq!(resets_5h(&l, AGENT, "q", T + 2_000), None);
}

/// ★ 这一家没有开窗那一形 ⇒ 那一句照传进来的注册表说那一家的叫法：路由名先认回那一家；认不出 ⇒ 照实那一句，不露路由名（替身家）。
#[test]
fn the_unsupported_refusal_names_that_family_not_its_route() {
    use crate::agents::fake;
    // 第二家的路由名与 kind 不同：照路由名直接当 kind 说的就说不出它的叫法。
    let mut b = fake::family_row("stand-b", "CCM_STAND_B", "sb", false, || None);
    b.launch = b.launch.map(|l| crate::agents::LaunchFace {
        adapter_id: "route-b",
        ..l
    });
    let reg = [
        fake::family_row("stand-a", "CCM_STAND_A", "sa", true, || None),
        b,
    ];
    let said =
        |agent: &str| copy_core::copy_text("beQuotaWarm.agent.unsupported", &[("agent", agent)]);
    for (route, want) in [
        ("route-b", said(fake::said_of("stand-b"))),
        ("stand-a", said(fake::said_of("stand-a"))),
        (
            "no-route",
            said(&copy_core::copy_text("rsAgents.family.unsaid", &[])),
        ),
    ] {
        let got = unsupported_among(&reg, route);
        assert_eq!(got.code.as_deref(), Some("unsupported"), "{route}");
        assert_eq!(got.said, want, "{route}");
    }
}

/// ★ 起不来那一句只一套原因词：同一个 IO 错，走「本程序路径取不到 · 开窗线程起不来」（[`run_failed`]）
/// 与「那一发的子进程起不来」（[`child_failed`]）两条路，出来同一句、同一个码、同一段原话。
#[test]
fn a_spawn_error_says_the_same_on_both_paths() {
    use crate::platform::child::ChildFail;
    use std::io::{Error, ErrorKind as K};
    for kind in [K::NotFound, K::PermissionDenied, K::Other] {
        let child = if kind == K::NotFound {
            ChildFail::NotFound(Error::new(kind, "boom"))
        } else {
            ChildFail::Io(Error::new(kind, "boom"))
        };
        assert_eq!(
            run_failed(&Error::new(kind, "boom")),
            child_failed(child),
            "{kind:?}"
        );
    }
}
