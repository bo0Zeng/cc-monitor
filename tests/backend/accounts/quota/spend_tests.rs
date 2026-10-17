//! 每号「1% ≈ 多少用量」：从读数的台阶起数，到又一个台阶时（涨够点数 · 数够发数）才给数；量不到的几种一律空着；
//! 换窗 · 读数往回走 · 有一发数不到 ⇒ 从头数。按会话那一本：用速 · 起伏 · 上下文 · 写缓存那一档 · 每号缓存 · 别的会话。
//! 流事件照真协议的样子手写（只有用量那几格），不起中转。

use super::*;
use crate::accounts::quota::rules::decide::{Cached, Me};

const AGENT: &str = "claude-code";
const SID: &str = "s-spend";

fn win(used: f64, resets_at: u64) -> QuotaWindow {
    QuotaWindow {
        name: "five_hour".into(),
        used: Some(used),
        resets_at: Some(resets_at),
        warned_at: None,
    }
}

/// 这个号答了一发：回包头到（记下会话在哪个号上）＋ 流里开头那一件带 `input` 个输入 token、收尾前那一件带 `output` 个输出。
fn send(s: &Spend, acct: &str, input: u64, output: u64) {
    s.answered(Some(SID), AGENT, acct);
    s.heard(
        SID,
        &format!(r#"{{"type":"message_start","message":{{"usage":{{"input_tokens":{input}}}}}}}"#),
        T0,
    );
    s.heard(
        SID,
        &format!(r#"{{"type":"message_delta","usage":{{"output_tokens":{output}}}}}"#),
        T0,
    );
}

const T0: u64 = 1_800_000_000;

/// 会话 `sid` 在 `acct` 上于 `at` 说了一发：上下文 `ctx` 个 token（`write` 个是这一发写进缓存的，`tier` ＝ 写的哪一档：`"5m"` / `"1h"`；其余读缓存），输出 `out` 个。
#[allow(clippy::too_many_arguments)]
fn said(s: &Spend, sid: &str, acct: &str, at: u64, ctx: u64, write: u64, tier: &str, out: u64) {
    s.answered(Some(sid), AGENT, acct);
    let (w5, w1) = if tier == "1h" { (0, write) } else { (write, 0) };
    s.heard(
        sid,
        &format!(
            r#"{{"type":"message_start","message":{{"usage":{{"input_tokens":0,"cache_creation_input_tokens":{write},"cache_creation":{{"ephemeral_5m_input_tokens":{w5},"ephemeral_1h_input_tokens":{w1}}},"cache_read_input_tokens":{}}}}}}}"#,
            ctx - write
        ),
        at,
    );
    s.heard(
        sid,
        &format!(r#"{{"type":"message_delta","usage":{{"output_tokens":{out}}}}}"#),
        at,
    );
}

/// 一发折合多少（同协议的价比：读一折、写 1 小时档 2 倍、输出 5 倍）。
fn units(ctx: u64, write: u64, out: u64) -> f64 {
    (ctx - write) as f64 * 0.1 + write as f64 * 2.0 + out as f64 * 5.0
}

/// 读数到了：交「1% ≈ 多少」。
fn read(s: &Spend, acct: &str, used: f64) -> Option<u64> {
    s.pace(AGENT, acct, &win(used, 2_000_000_000))
}

/// ★ 量不到就空着：只有一次读数 · 读数没动 · 头一个台阶（起点还没落在台阶上）· 涨不够点数 · 数不够发数 —— 每一种都是 `None`。
#[test]
fn nothing_is_said_until_it_is_measured() {
    let s = Spend::default();
    assert_eq!(read(&s, "a", 0.10), None, "只有一次读数");
    for _ in 0..10 {
        send(&s, "a", 1_000, 0);
    }
    assert_eq!(read(&s, "a", 0.10), None, "读数没动");
    assert_eq!(read(&s, "a", 0.11), None, "头一个台阶只是起点");
    for _ in 0..10 {
        send(&s, "a", 1_000, 0);
    }
    assert_eq!(read(&s, "a", 0.13), None, "只涨了 2 点");
    let few = Spend::default();
    read(&few, "b", 0.10);
    send(&few, "b", 1, 0);
    read(&few, "b", 0.11);
    for _ in 0..(MIN_SENDS - 1) {
        send(&few, "b", 100_000, 0);
    }
    assert_eq!(read(&few, "b", 0.20), None, "发数不够");
}

/// ★ 量得到：从台阶起数，用量（输入 1 倍 · 输出 5 倍）÷ 涨的点数；之后接着累计、越量越准。
#[test]
fn the_rise_from_a_step_divides_the_usage_seen() {
    let s = Spend::default();
    read(&s, "a", 0.10);
    send(&s, "a", 50_000, 0);
    assert_eq!(read(&s, "a", 0.11), None);
    for _ in 0..5 {
        send(&s, "a", 10_000, 2_000);
    }
    // 5 × (10 000 ＋ 2 000 × 5) ＝ 100 000，涨 4 点。
    assert_eq!(read(&s, "a", 0.15), Some(25_000));
    for _ in 0..5 {
        send(&s, "a", 10_000, 2_000);
    }
    assert_eq!(read(&s, "a", 0.19), Some(25_000));
    assert_eq!(read(&s, "b", 0.19), None, "别的号没数到");
}

/// ★ 从头数的三种：换了窗口 · 读数往回走 · 中间有一发数不到（回包没带会话标识）。
#[test]
fn a_new_window_a_falling_reading_or_a_blind_send_start_over() {
    let warm = |s: &Spend| {
        read(s, "a", 0.10);
        read(s, "a", 0.11);
        for _ in 0..5 {
            send(s, "a", 10_000, 0);
        }
    };
    let s = Spend::default();
    warm(&s);
    assert_eq!(
        s.pace(AGENT, "a", &win(0.15, 2_000_000_000 + 5 * 3600)),
        None,
        "换了窗口"
    );
    let s = Spend::default();
    warm(&s);
    assert_eq!(read(&s, "a", 0.05), None, "读数往回走");
    let s = Spend::default();
    warm(&s);
    s.answered(None, AGENT, "a");
    assert_eq!(read(&s, "a", 0.15), None, "有一发数不到");
    let s = Spend::default();
    warm(&s);
    assert_eq!(
        read(&s, "a", 0.15),
        Some(12_500),
        "正控：三样都没有就量得到"
    );
}

/// ★ 本会话量出来的：只说了一发 ⇒ 只有上下文 · 写缓存那一档 · 那个号的缓存；用速（记了不到 10 分钟）· 起伏 · 每分钟长多少都空着。
/// 没记过的会话 ⇒ 全空。
#[test]
fn one_send_says_the_context_and_tier_but_no_pace() {
    let s = Spend::default();
    assert_eq!(s.me("nobody", T0), Me::default(), "没记过的会话全空");
    said(&s, "s1", "a", T0, 100_000, 2_000, "1h", 300);
    let me = s.me("s1", T0 + 60);
    assert_eq!(me.context, Some(100_000));
    assert_eq!(me.ttl, Some(3600));
    assert_eq!(me.write_extra, Some(2.0 - 0.1));
    assert_eq!(me.pace, None, "记了不到 10 分钟");
    assert_eq!(me.spread, None);
    assert_eq!(me.grow, None, "只有一份上下文");
    assert_eq!(
        me.cached,
        vec![Cached {
            account: "a".into(),
            until: T0 + 3600,
            tokens: 100_000
        }]
    );
    let only_read = Spend::default();
    said(&only_read, "s1", "a", T0, 100_000, 0, "1h", 300);
    let me = only_read.me("s1", T0 + 60);
    assert_eq!(
        (me.ttl, me.write_extra),
        (None, None),
        "只读缓存说不出哪一档"
    );
    assert_eq!(me.cached, Vec::new(), "说不出档就说不出缓存到几点");
}

/// ★ 用速 ＝ 最近 30 分钟的用量 ÷ 30；起伏 ＝ 每 10 分钟一段的 σ/μ（会话记到之后整段都在的那几段，至少 3 段）；
/// 最近 30 分钟一发都没有 ⇒ 用速空着（太旧）。
#[test]
fn pace_is_the_last_half_hour_and_spread_compares_ten_minute_bins() {
    let s = Spend::default();
    let u = units(50_000, 1_000, 200);
    for i in 0..60 {
        said(&s, "s1", "a", T0 + 60 * i, 50_000, 1_000, "1h", 200);
    }
    let now = T0 + 3600;
    let me = s.me("s1", now);
    let pace = me.pace.expect("量得到");
    assert!(
        (pace - u).abs() < 1.0,
        "每分钟一发 ⇒ 一发的量：{pace} vs {u}"
    );
    assert!(me.spread.expect("6 段都在") < 1e-9, "每段一样 ⇒ 0");
    let lumpy = Spend::default();
    for i in 0..60 {
        if (i / 10) % 2 == 0 {
            said(&lumpy, "s1", "a", T0 + 60 * i, 50_000, 1_000, "1h", 200);
        }
    }
    let spread = lumpy.me("s1", now).spread.expect("量得到");
    assert!(
        (spread - 1.0).abs() < 1e-9,
        "一段有一段没有 ⇒ σ/μ ＝ 1：{spread}"
    );
    let three = lumpy.me("s1", T0 + 1800).spread;
    assert!(three.is_some(), "整段 3 段 ⇒ 给");
    assert_eq!(lumpy.me("s1", T0 + 1799).spread, None, "整段不到 3 段");
    assert_eq!(
        s.me("s1", now + 1801).pace,
        None,
        "最近 30 分钟没有一发 ⇒ 太旧"
    );
}

/// ★ 上下文每分钟长多少 ＝ 最近 30 分钟里头一份与末一份之差 ÷ 时间；变小（压缩过）⇒ 从那一发重新记。
#[test]
fn grow_is_the_context_slope_and_restarts_after_compaction() {
    let s = Spend::default();
    for i in 0..5 {
        said(
            &s,
            "s1",
            "a",
            T0 + 120 * i,
            40_000 + 2_000 * i,
            500,
            "1h",
            100,
        );
    }
    let g = s.me("s1", T0 + 500).grow.expect("量得到");
    assert!(
        (g - 1_000.0).abs() < 1e-6,
        "两分钟长 2000 ⇒ 每分钟 1000：{g}"
    );
    said(&s, "s1", "a", T0 + 600, 10_000, 500, "1h", 100);
    assert_eq!(s.me("s1", T0 + 600).grow, None, "压缩过 ⇒ 从头记");
    said(&s, "s1", "a", T0 + 720, 13_000, 500, "1h", 100);
    let g = s.me("s1", T0 + 720).grow.expect("又有两份");
    assert!((g - 1_500.0).abs() < 1e-6, "{g}");
}

/// ★ 每个号最后一发的缓存留到几点：换号以后旧号那一份还在、到点就没了；档照最近说得出的那一档。
#[test]
fn each_account_keeps_its_cache_until_the_tier_runs_out() {
    let s = Spend::default();
    said(&s, "s1", "a", T0, 80_000, 80_000, "1h", 100);
    said(&s, "s1", "b", T0 + 600, 90_000, 90_000, "5m", 100);
    let me = s.me("s1", T0 + 700);
    assert_eq!(
        me.cached,
        vec![
            Cached {
                account: "a".into(),
                until: T0 + 3600,
                tokens: 80_000
            },
            Cached {
                account: "b".into(),
                until: T0 + 900,
                tokens: 90_000
            }
        ]
    );
    assert_eq!(me.ttl, Some(300), "最近那一发写的是 5 分钟档");
    assert_eq!(
        s.me("s1", T0 + 901)
            .cached
            .iter()
            .map(|c| c.account.as_str())
            .collect::<Vec<_>>(),
        vec!["a"],
        "b 那一份过了 5 分钟"
    );
    assert_eq!(s.me("s1", T0 + 3600).cached, Vec::new(), "a 那一份也过了");
}

/// ★ 这台记在一个号名下的用量（所有会话、最近 30 分钟 ÷ 30）· 别的会话（量得出用速的）此刻在哪个号上。
#[test]
fn here_sums_every_session_and_others_leaves_this_one_out() {
    let s = Spend::default();
    for i in 0..20 {
        said(&s, "s1", "a", T0 + 60 * i, 50_000, 1_000, "1h", 200);
        said(&s, "s2", "a", T0 + 60 * i, 50_000, 1_000, "1h", 200);
    }
    for i in 20..30 {
        said(&s, "s1", "a", T0 + 60 * i, 50_000, 1_000, "1h", 200);
        said(&s, "s2", "b", T0 + 60 * i, 50_000, 1_000, "1h", 200);
    }
    let now = T0 + 1800;
    let u = units(50_000, 1_000, 200);
    assert!((s.here(AGENT, "a", now) - u * 50.0 / 30.0).abs() < 1e-6);
    assert!((s.here(AGENT, "b", now) - u * 10.0 / 30.0).abs() < 1e-6);
    assert_eq!(s.here("codex", "a", now), 0.0, "别的一家不算");
    let o = s.others(AGENT, "s1", now);
    assert_eq!(o.len(), 1, "{o:?}");
    assert_eq!((o[0].0.as_str(), o[0].1.as_str()), ("s2", "b"));
    assert!((o[0].2 - u).abs() < 1.0, "s2 每分钟一发");
}
