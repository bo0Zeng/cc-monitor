//! 要求住址：`设计/99 §2.1 ⑬`「待迁」最后一行 ——「远端拉起那串的 ssh 外壳（`ssh -t -J … host '<串>'` · PowerShell 窗口载荷）
//! 由本机后端渲，monitor 只开终端」（FIX4 题面第 1 条）。
//!
//! 期望原样搬自 monitor `tests/bridge/launch_tests.rs` 钉 ssh 外壳的那六条（基本形态 · 钥匙与口 · IPv6 · 跳板参数 · 坏输入 ·
//! 单引号过两层）＋「拒双引号只拦 PowerShell 那条送法」的远端那一半 ＋「同一个载荷只多两层包装」的远端那一半 —— 被测对象搬了家、期望一个字没改；
//! 新多两格：地址取竞速顺序第一条（`prefer`）· 跳板经 `machine::resolve`（查无 / 环 ⇒ `bad_jump`）。

use super::*;
use serde_json::json;

fn machine(host: &str, user: &str, port: u16, key: Option<&str>) -> Value {
    json!({ "host": host, "user": user, "port": port, "keyPath": key, "label": host })
}

fn run(m: Value, cmd: &str) -> Result<String, CmdErr> {
    answer(&json!({ "machine": m, "command": cmd }))
        .map(|v| v["command"].as_str().unwrap().to_string())
}

/// 剥 PowerShell 单引号那一层（`'…'`，内部 `''` → `'`）。
fn unps(s: &str) -> String {
    s.strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .expect("ps quoted")
        .replace("''", "'")
}

#[test]
fn the_basic_shape_goes_through_the_agent_and_wraps_the_payload_only_twice() {
    let remote = "unset X; cd '/home/pi' && claude --resume s1";
    let got = run(machine("pi.local", "pi", 22, None), remote).unwrap();
    assert!(
        got.starts_with("& ssh -t -p 22 pi@pi.local -- "),
        "基本形态（agent 无 -i）: {got}"
    );
    let inner = unps(got.rsplit_once("-- ").unwrap().1);
    assert_eq!(
        inner,
        format!("bash -lic {}", shell_quote_core::posix_quote(remote))
    );
    // 剥净两层包装（PS 单引号 · `bash -lic '…'`）⇒ 就是交进来的那一串，逐字节。
    let unwrapped = inner
        .strip_prefix("bash -lic '")
        .and_then(|s| s.strip_suffix('\''))
        .expect("bash -lic 层")
        .replace(r"'\''", "'");
    assert_eq!(unwrapped, remote);
}

#[test]
fn a_key_and_a_port_render_as_flags_and_the_key_is_ps_quoted() {
    let got = run(
        machine("10.0.0.2", "u", 2222, Some(r"C:\Users\z's\id_ed25519\")),
        "claude --resume s1",
    )
    .unwrap();
    assert!(
        got.starts_with("& ssh -t -p 2222 -i 'C:\\Users\\z''s\\id_ed25519' u@10.0.0.2 -- "),
        "{got}"
    );
    assert!(got.contains("bash -lic ''claude --resume s1''"), "{got}");
    assert!(run(machine("[::1]", "u", 22, None), "claude --resume s1").is_ok());
}

#[test]
fn single_quotes_in_the_payload_survive_both_layers() {
    let remote = r"cd '/a'\''b' && claude --resume s1";
    let got = run(machine("h", "u", 22, None), remote).unwrap();
    assert_eq!(
        unps(got.rsplit_once("-- ").unwrap().1),
        format!("bash -lic {}", shell_quote_core::posix_quote(remote))
    );
}

#[test]
fn the_address_is_the_first_in_race_order_so_the_last_winner_is_used() {
    let m = json!({ "host": "lan.example", "user": "u", "port": 22, "label": "m", "addresses": ["10.0.0.9:2200"] });
    let plain = answer(&json!({ "machine": m, "command": "x" })).unwrap();
    assert!(plain["command"]
        .as_str()
        .unwrap()
        .starts_with("& ssh -t -p 22 u@lan.example -- "));
    let won = answer(
        &json!({ "machine": m, "prefer": { "host": "10.0.0.9", "port": 2200 }, "command": "x" }),
    )
    .unwrap();
    assert!(
        won["command"]
            .as_str()
            .unwrap()
            .starts_with("& ssh -t -p 2200 u@10.0.0.9 -- "),
        "上次赢的那条没排首：{won}"
    );
}

#[test]
fn the_jump_hop_renders_as_dash_j_and_a_missing_or_looping_jump_is_refused() {
    let m = json!({ "host": "t", "user": "u", "port": 22, "label": "target", "jump": "bastion" });
    let j =
        |port: u16| json!({ "host": "jump.local", "user": "pi", "port": port, "label": "bastion" });
    let got = answer(&json!({ "machine": m, "jump": j(22), "command": "x" })).unwrap();
    assert!(
        got["command"]
            .as_str()
            .unwrap()
            .starts_with("& ssh -t -J pi@jump.local -p 22 u@t -- "),
        "{got}"
    );
    let got = answer(&json!({ "machine": m, "jump": j(2222), "command": "x" })).unwrap();
    assert!(
        got["command"]
            .as_str()
            .unwrap()
            .starts_with("& ssh -t -J pi@jump.local:2222 -p 22 u@t -- "),
        "{got}"
    );
    // fail-closed：跳板交不来 ⇒ 拒（绝不静默直连目标）；指自己 ⇒ 环。
    assert_eq!(
        answer(&json!({ "machine": m, "command": "x" }))
            .unwrap_err()
            .0,
        "bad_jump"
    );
    let looped =
        json!({ "host": "t", "user": "u", "port": 22, "label": "target", "jump": "target" });
    assert_eq!(
        answer(&json!({ "machine": looped, "command": "x" }))
            .unwrap_err()
            .0,
        "bad_jump"
    );
    let bad_user =
        json!({ "host": "jump.local", "user": "bad user", "port": 22, "label": "bastion" });
    assert_eq!(
        answer(&json!({ "machine": m, "jump": bad_user, "command": "x" }))
            .unwrap_err()
            .0,
        "refused"
    );
    let bad_host = json!({ "host": "h;rm -rf", "user": "u", "port": 22, "label": "bastion" });
    assert_eq!(
        answer(&json!({ "machine": m, "jump": bad_host, "command": "x" }))
            .unwrap_err()
            .0,
        "refused"
    );
}

#[test]
fn bad_inputs_are_refused_and_say_which_cell() {
    let c = || machine("h", "u", 22, None);
    for (cmd, why) in [
        ("", "空命令"),
        ("a\nb", "控制字符"),
        ("cc --x \"y\"", "双引号（PS native 畸变面）"),
    ] {
        assert_eq!(run(c(), cmd).unwrap_err().0, "refused", "{why}");
    }
    assert_eq!(
        run(c(), &"a".repeat(MAX_COMMAND + 1)).unwrap_err().0,
        "refused",
        "超长"
    );
    assert_eq!(
        run(machine("h", "u ser", 22, None), "x").unwrap_err().0,
        "refused",
        "user 空格"
    );
    assert_eq!(
        run(machine("h; rm", "u", 22, None), "x").unwrap_err().0,
        "refused",
        "host 注入"
    );
    // 契约错：缺命令 / 缺机器 / 机器缺 user。
    assert_eq!(
        answer(&json!({ "machine": c() })).unwrap_err().0,
        "invalid_args"
    );
    assert_eq!(
        answer(&json!({ "command": "x" })).unwrap_err().0,
        "invalid_args"
    );
    assert_eq!(
        answer(&json!({ "machine": { "host": "h" }, "command": "x" }))
            .unwrap_err()
            .0,
        "invalid_args"
    );
}
