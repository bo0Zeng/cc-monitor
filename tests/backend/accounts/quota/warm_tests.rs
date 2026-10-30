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
    assert!(crate::agents::route_named("codex")
        .and_then(crate::agents::warm_of)
        .is_none());
}
