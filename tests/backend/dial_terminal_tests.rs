//! 要求住址：`设计/99 §2.1 ⑬`「待迁」最后一行 ——「远端拉起那串的 ssh 外壳（`ssh -t -J … host '<串>'` · PowerShell 窗口载荷）
//! 由本机后端渲，monitor 只开终端」（FIX4 题面第 1 条）。
//!
//! 期望原样搬自 monitor `tests/frontend/shell/launch_tests.rs` 钉 ssh 外壳的那六条（基本形态 · 钥匙与口 · IPv6 · 跳板参数 · 坏输入 ·
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

/// 流进 `terminal-ssh` 的远端命令的全部来路（`ccm …` 调用行 · 载荷 / tmux 外层三格，后者带不带中转前缀）×
/// 入库三份夹具的每条请求 × 典型工作目录 ⇒ `(哪条, 渲出来的命令)`。渲不出来的（降级 / 拒）不在里面。
fn every_rendered_remote_command(cwds: &[&str]) -> Vec<(String, String)> {
    use crate::control::launch_render::wire;
    const RELAY: &str = "http://127.0.0.1:8788/s/claude-code/acct-a";
    let fixture = |s: &str| -> Vec<Value> {
        serde_json::from_str::<Value>(s).unwrap()["cases"]
            .as_array()
            .unwrap()
            .clone()
    };
    let mut out = Vec::new();
    for c in fixture(include_str!(
        "../../src/backend/control/launch_render/fixtures/cli-golden.json"
    )) {
        let caps: std::collections::BTreeSet<String> = c["caps"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        for cwd in std::iter::once(c["req"]["cwd"].clone()).chain(cwds.iter().map(|d| json!(d))) {
            let mut req = c["req"].clone();
            req["cwd"] = cwd.clone();
            let got = wire::render_ccm_launch_with(
                serde_json::from_value(req).unwrap(),
                &caps,
                !c["caps"].is_null(),
            );
            if let Some(cmd) = got.cmd {
                out.push((format!("cli {} · cwd {cwd}", c["name"]), cmd));
            }
        }
    }
    for c in fixture(include_str!(
        "../../src/backend/control/launch_render/fixtures/payload-golden.json"
    ))
    .into_iter()
    .chain(fixture(include_str!(
        "../../src/backend/control/launch_render/fixtures/tmux-outer-golden.json"
    ))) {
        let mode = c["req"]["outer"]["mode"].as_str().map(str::to_string);
        let cwd_at: Option<&[&str]> = match mode.as_deref() {
            None => Some(&["cwd"]),
            Some("create") => Some(&["outer", "cwd"]),
            _ => None,
        };
        let mut cwd_vals = vec![None];
        if cwd_at.is_some() {
            cwd_vals.extend(cwds.iter().map(|d| Some(*d)));
        }
        for cwd in cwd_vals {
            for relay in [false, mode.as_deref() != Some("attach")] {
                let mut req = c["req"].clone();
                if let (Some(at), Some(d)) = (cwd_at, cwd) {
                    let slot = at.iter().fold(&mut req, |v, k| &mut v[*k]);
                    *slot = json!(d);
                }
                if relay {
                    req["env"].as_array_mut().unwrap().insert(
                        0,
                        json!({ "kind": "export-relay-base-url", "value": RELAY }),
                    );
                }
                if let Ok(cmd) = wire::render_launch_payload(serde_json::from_value(req).unwrap()) {
                    out.push((format!("{} · cwd {cwd:?} · 中转 {relay}", c["name"]), cmd));
                }
            }
        }
    }
    out
}

/// ★ 住址：`设计/99 §2.3`「高危四条（… G Windows 开远端会话被自家守卫拒 …）发版前修」· `第四波记录/WIN3.md §2` G。
/// 后端自己渲出、会交给 `terminal-ssh` 的每一条远端命令（[`every_rendered_remote_command`]：典型工作目录 ——
/// 空格 · 中文 · 弯引号 · 单引号）都过这道守卫（零命中）；带中转前缀的那几条真的在人群里。
/// 守卫不放宽（正控）：同一条路上工作目录带 `"` 的照旧拒。
#[test]
fn every_remote_command_the_backend_renders_passes_the_terminal_guard() {
    let cwds = [
        "/home/u/c c",
        "/home/u/文档/项目",
        "/home/u/a\u{2019}b",
        "/home/u/it's",
    ];
    let all = every_rendered_remote_command(&cwds);
    let refused: Vec<String> = all
        .iter()
        .filter_map(|(what, cmd)| {
            run(machine("h", "u", 22, None), cmd)
                .err()
                .map(|e| format!("  {what}\n    {cmd}\n    ⇒ {e:?}"))
        })
        .collect();
    assert!(
        refused.is_empty(),
        "后端自己渲的远端命令被开终端那道守卫拒了（Windows 上这一趟就起不来）：\n{}",
        refused.join("\n")
    );
    let relayed = all.iter().filter(|(w, _)| w.ends_with("中转 true")).count();
    assert!(relayed > 0, "带中转前缀的那几条一条都没渲出来 —— 人群塌了");
    assert_eq!(
        all.iter()
            .filter(|(_, c)| c.contains("export ANTHROPIC_BASE_URL="))
            .count(),
        relayed,
        "带中转那几条没真带上前缀"
    );
    let dq = every_rendered_remote_command(&["/home/u/a\"b"]);
    let with_dq: Vec<&String> = dq
        .iter()
        .map(|(_, c)| c)
        .filter(|c| c.contains('"'))
        .collect();
    assert!(!with_dq.is_empty(), "正控没造出带双引号的命令");
    for c in with_dq {
        assert_eq!(
            run(machine("h", "u", 22, None), c).unwrap_err().0,
            "refused",
            "守卫被放宽了：{c}"
        );
    }
}
