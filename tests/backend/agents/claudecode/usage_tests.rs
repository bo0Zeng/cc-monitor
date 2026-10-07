//! 官方 claude 报用量（`claude -p /usage`）的读法：只采结构的夹具（号名与日期都是假的）→ 窗口；
//! 年份按「离此刻最近的将来」补；时区不是 UTC · 行对不上 · 一行都没有 ⇒ 整份读不懂（不猜）。

use super::*;

/// 夹具：照 10-05 那份真读数的形状（三档用量行 ＋ 本机会话的用量构成那一段），数与日期换成假的。
const FIXTURE: &str = include_str!("../../../__fixtures__/claude-usage.fixture.txt");

/// 2026-03-05 12:00 UTC。
const NOW: u64 = 1_772_712_000;

fn utc(y: i64, m: u32, d: u32, h: u32, min: u32) -> u64 {
    u64::try_from(
        crate::common::time::days_from_civil(y, m, d) * 86_400 + i64::from(h * 3600 + min * 60),
    )
    .expect("1970 之后")
}

/// ★ 三档照原名读成窗口：用量 · 重置时刻（按 UTC）；没写重置时刻的那一档没在计时（`resetsAt` 缺）；构成那一段不读。
#[test]
fn the_three_rows_read_into_windows_by_their_own_names() {
    assert_eq!(utc(2026, 3, 5, 12, 0), NOW);
    let got = read(FIXTURE, NOW).expect("读得懂");
    let names: Vec<&str> = got.iter().map(|w| w.name.as_str()).collect();
    assert_eq!(names, ["five_hour", "seven_day", "seven_day_fable"]);
    assert_eq!(got[0].used, Some(0.03));
    assert_eq!(got[0].resets_at, Some(utc(2026, 3, 7, 16, 0)));
    assert_eq!(got[1].used, Some(0.21));
    assert_eq!(got[1].resets_at, Some(utc(2026, 3, 9, 7, 30)));
    assert_eq!(got[2].used, Some(0.0));
    assert_eq!(got[2].resets_at, None, "没有重置时刻 ＝ 没在计时");
    // 窗口键由同一层给：按模型的周额度不并进 7d。
    let keys: Vec<Option<String>> = got
        .iter()
        .map(|w| crate::agents::claudecode::quota::key_of(&w.name))
        .collect();
    assert_eq!(
        keys,
        [
            Some("5h".into()),
            Some("7d".into()),
            Some("7d:fable".into())
        ]
    );
}

/// 「Sonnet only」照 claude 自己的对应读成 `seven_day_sonnet`；12am / 12pm；写了年份照年份。
#[test]
fn sonnet_only_noon_midnight_and_an_explicit_year() {
    let out = "Current session: 0% used · resets Mar 5, 12pm (UTC)\n\
               Current week (Sonnet only): 7% used · resets Jan 2, 2027, 12am (Etc/UTC)\n";
    let got = read(out, NOW).expect("读得懂");
    assert_eq!(got[0].name, "five_hour");
    assert_eq!(got[0].resets_at, Some(utc(2026, 3, 5, 12, 0)));
    assert_eq!(got[1].name, "seven_day_sonnet");
    assert_eq!(got[1].resets_at, Some(utc(2027, 1, 2, 0, 0)));
}

/// 没写年份 ⇒ 离此刻最近的将来：12 月底看到「Jan 1」⇒ 明年；年中看到「Mar 9」⇒ 今年。
#[test]
fn a_missing_year_is_the_nearest_future() {
    let dec30 = utc(2026, 12, 30, 23, 0);
    let got = read("Current session: 1% used · resets Jan 1, 3am (UTC)", dec30).expect("ok");
    assert_eq!(got[0].resets_at, Some(utc(2027, 1, 1, 3, 0)));
    let got = read("Current session: 1% used · resets Mar 9, 3am (UTC)", NOW).expect("ok");
    assert_eq!(got[0].resets_at, Some(utc(2026, 3, 9, 3, 0)));
}

/// ★ 读不懂 ⇒ 照实说哪一处，不猜：时区不是 UTC（起它时没生效）· 一行对不上 · 一行都没有 · 同一档出现两次。
#[test]
fn anything_off_shape_is_unreadable_not_guessed() {
    let cases = [
        (
            "Current session: 0% used · resets Oct 5, 10pm (America/Los_Angeles)",
            "America/Los_Angeles",
        ),
        ("Current session: about half used", "usage line"),
        ("Current session: 3% used · resets soon (UTC)", "reset time"),
        (
            "Current session: 3% used · resets Oct 5, 25pm (UTC)",
            "reset time",
        ),
        ("Current week (): 3% used", "usage line"),
        ("Invalid API key · Please run /login", "no usage line"),
        ("", "no usage line"),
        (
            "Current session: 1% used\nCurrent session: 2% used",
            "twice",
        ),
    ];
    for (out, said) in cases {
        let e = read(out, NOW).expect_err(out);
        assert!(e.contains(said), "{out:?} ⇒ {e}，应提到 {said}");
    }
}

/// 起法照这一家：`claude -p /usage` · 配置目录交 `CLAUDE_CONFIG_DIR` · `TZ=UTC` · 摘掉会盖掉那个号登录的那几格；登记在注册表上。
#[test]
fn the_face_is_registered_for_the_claude_route() {
    let f = crate::agents::usage_of("claude-code").expect("登记了");
    assert_eq!((f.program, f.args), ("claude", &["-p", "/usage"][..]));
    assert_eq!(f.dir_env, "CLAUDE_CONFIG_DIR");
    assert!(f.env_set.contains(&("TZ", "UTC")));
    assert!(f.env_remove.contains(&"ANTHROPIC_API_KEY"));
    assert!(crate::agents::usage_of("codex").is_none());
}
