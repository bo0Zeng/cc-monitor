//! 每号「1% ≈ 多少用量」：从读数的台阶起数，到又一个台阶时（涨够点数 · 数够发数）才给数；量不到的几种一律空着；
//! 换窗 · 读数往回走 · 有一发数不到 ⇒ 从头数。流事件照真协议的样子手写（只有用量那几格），不起中转。

use super::*;

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
    );
    s.heard(
        SID,
        &format!(r#"{{"type":"message_delta","usage":{{"output_tokens":{output}}}}}"#),
    );
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
