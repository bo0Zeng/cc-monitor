//! 「这条连接是这台电脑上哪个进程开的、它往上是谁」：系统那一趟换成合成的 JSON（与那段固定脚本打出的同形），期望值手写。

use super::*;
use serde_json::json;

/// 一份合成的系统应答：用户开的 Windows Terminal 里 PowerShell 起的 ssh，另有一条同地址、别的端口的 ssh（诱饵），
/// 一条别的程序连着跳板地址的连接，和一个父进程号被复用过的孤儿。
fn tables() -> String {
    json!({
        "tcp": [
            { "la": "192.0.2.5", "lp": 62415, "ra": "192.0.2.9", "rp": 22, "pid": 900 },
            { "la": "192.0.2.5", "lp": 62414, "ra": "192.0.2.9", "rp": 22, "pid": 700 },
            { "la": "192.0.2.5", "lp": 50001, "ra": "198.51.100.1", "rp": 443, "pid": 990 },
            { "la": "192.0.2.5", "lp": 50002, "ra": "203.0.113.7", "rp": 22, "pid": 910 },
            { "la": "::ffff:192.0.2.5", "lp": 50003, "ra": "192.0.2.20", "rp": 22, "pid": 920 }
        ],
        "proc": [
            { "pid": 4, "ppid": 0, "name": "System", "start": 0 },
            { "pid": 300, "ppid": 4, "name": "explorer.exe", "start": 1000 },
            { "pid": 500, "ppid": 300, "name": "WindowsTerminal.exe", "start": 2000 },
            { "pid": 600, "ppid": 500, "name": "powershell.exe", "start": 3000 },
            { "pid": 700, "ppid": 600, "name": "ssh.exe", "start": 4000 },
            { "pid": 900, "ppid": 600, "name": "ssh.exe", "start": 4100 },
            { "pid": 910, "ppid": 600, "name": "ssh.exe", "start": 4200 },
            { "pid": 920, "ppid": 950, "name": "ssh.exe", "start": 4300 },
            { "pid": 950, "ppid": 300, "name": "pwsh.exe", "start": 9000 },
            { "pid": 990, "ppid": 300, "name": "chrome.exe", "start": 5000 }
        ]
    })
    .to_string()
}

fn term(ca: &str, cp: u16, sa: &str, sp: u16) -> Value {
    json!({ "ssh": { "clientAddr": ca, "clientPort": cp, "serverAddr": sa, "serverPort": sp }, "activity": 1 })
}

fn ask(terminals: Vec<Value>) -> Value {
    answer_with(&json!({ "terminals": terminals }), || Ok(tables())).unwrap()
}

fn link(pid: u32, name: &str, start: u64) -> Value {
    json!({ "pid": pid, "name": name, "start": start })
}

/// ★ 四元组全等 ⇒ 开着那条连接的 ssh，往上数到桌面外壳之前；同地址别的端口的那条不认，只差一个端口的不认（不许认错）。
#[test]
fn the_exact_connection_names_its_owner_and_the_chain_above_it() {
    assert_eq!(
        ask(vec![term("192.0.2.5", 62414, "192.0.2.9", 22)]),
        json!({ "chain": [
            link(700, "ssh.exe", 4000),
            link(600, "powershell.exe", 3000),
            link(500, "WindowsTerminal.exe", 2000),
        ]})
    );
    // 只差对面端口 / 只差本机端口：这台有那个地址 ⇒ 对不上，不是认成别的进程。
    for t in [
        term("192.0.2.5", 62414, "192.0.2.9", 2222),
        term("192.0.2.5", 62416, "192.0.2.9", 22),
    ] {
        assert_eq!(ask(vec![t]), json!({ "chain": [], "why": "mismatch" }));
    }
    // 第一个对不上、第二个对得上 ⇒ 给第二个的链；那台写的是 IPv4 映射形也认。
    assert_eq!(
        ask(vec![
            term("198.51.100.9", 1, "192.0.2.9", 22),
            term("::ffff:192.0.2.5", 62415, "192.0.2.9", 22),
        ])["chain"][0],
        link(900, "ssh.exe", 4100)
    );
}

/// ★ 对不上时的原因只看这台的表：不是经 ssh · 这台没有那个地址也没有 ssh 连着它（带地址）· 这台有 ssh 连着那台看到的对面（跳板）·
/// 别的程序连着那个地址不算 · 都对不上 ⇒ 第一个的原因。
#[test]
fn every_miss_says_which_fact_it_is() {
    assert_eq!(
        ask(vec![json!({ "ssh": null, "activity": null })]),
        json!({ "chain": [], "why": "not-ssh" })
    );
    assert_eq!(
        ask(vec![term("203.0.113.8", 40000, "192.0.2.9", 22)]),
        json!({ "chain": [], "why": "elsewhere", "addr": "203.0.113.8" })
    );
    // 那台看到的对面是 203.0.113.7，这台的 ssh 正连着它 ⇒ 经跳板。
    assert_eq!(
        ask(vec![term("203.0.113.7", 41000, "192.0.2.9", 22)]),
        json!({ "chain": [], "why": "mismatch" })
    );
    // 连着 198.51.100.1 的是浏览器，不是 ssh ⇒ 不在这台电脑上。
    assert_eq!(
        ask(vec![term("198.51.100.1", 41000, "192.0.2.9", 22)])["why"],
        "elsewhere"
    );
    assert_eq!(
        ask(vec![
            term("203.0.113.8", 40000, "192.0.2.9", 22),
            json!({ "ssh": null })
        ])["why"],
        "elsewhere"
    );
}

/// 父进程比子进程晚起 ⇒ 那个进程号被复用过，链在那里断；系统那一趟没成 / 打出来的不是那张表 ⇒ 查询失败；入参不对 ⇒ 不去问系统。
#[test]
fn a_reused_parent_ends_the_chain_and_a_failed_query_is_said() {
    assert_eq!(
        ask(vec![term("192.0.2.5", 50003, "192.0.2.20", 22)]),
        json!({ "chain": [link(920, "ssh.exe", 4300)] })
    );
    let t = json!({ "terminals": [term("192.0.2.5", 62414, "192.0.2.9", 22)] });
    for q in [
        Err("exit Some(1): boom".to_string()),
        Ok("not json".to_string()),
        Ok("{}".to_string()),
    ] {
        assert_eq!(
            answer_with(&t, || q.clone()).unwrap(),
            json!({ "chain": [], "why": "query-failed" })
        );
    }
    for bad in [
        json!({}),
        json!({ "terminals": [] }),
        json!({ "terminals": [{ "activity": 1 }] }),
        json!({ "terminals": [term("not-an-ip", 1, "192.0.2.9", 22)] }),
        json!({ "terminals": [{ "ssh": { "clientAddr": "192.0.2.5", "clientPort": 70000, "serverAddr": "192.0.2.9", "serverPort": 22 } }] }),
    ] {
        let e = answer_with(&bad, || panic!("入参不对还去问了系统")).unwrap_err();
        assert_eq!(e.0, "bad_args", "{bad}");
    }
}

/// ★ Linux 那一形（`/proc` 读出来的表）：ssh · bash · 终端程序往上到 systemd 之前为止；这台的 ssh 叫 `ssh`，跳板那一条照样认得出。
#[test]
fn a_linux_shaped_table_reads_the_same_way() {
    let raw = json!({
        "tcp": [
            { "la": "192.0.2.5", "lp": 40100, "ra": "192.0.2.9", "rp": 22, "pid": 1300 },
            { "la": "192.0.2.5", "lp": 40200, "ra": "203.0.113.7", "rp": 22, "pid": 1310 }
        ],
        "proc": [
            { "pid": 1, "ppid": 0, "name": "systemd", "start": 1 },
            { "pid": 1000, "ppid": 1, "name": "systemd", "start": 500 },
            { "pid": 1100, "ppid": 1000, "name": "alacritty", "start": 600 },
            { "pid": 1200, "ppid": 1100, "name": "bash", "start": 700 },
            { "pid": 1300, "ppid": 1200, "name": "ssh", "start": 800 },
            { "pid": 1310, "ppid": 1200, "name": "ssh", "start": 810 }
        ]
    })
    .to_string();
    let ask = |t: Value| answer_with(&json!({ "terminals": [t] }), || Ok(raw.clone())).unwrap();
    assert_eq!(
        ask(term("192.0.2.5", 40100, "192.0.2.9", 22)),
        json!({ "chain": [link(1300, "ssh", 800), link(1200, "bash", 700), link(1100, "alacritty", 600)] })
    );
    assert_eq!(
        ask(term("203.0.113.7", 41000, "192.0.2.9", 22)),
        json!({ "chain": [], "why": "mismatch" })
    );
}

impl crate::guard_support::Shaped for Processes {
    fn samples() -> Vec<Self> {
        vec![Processes {
            chain: vec![Link {
                pid: 1,
                name: "sshd".into(),
                start: 2,
            }],
            why: Some("elsewhere"),
            addr: Some("203.0.113.2".into()),
        }]
    }
}
