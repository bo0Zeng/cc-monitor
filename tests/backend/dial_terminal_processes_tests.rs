//! 「这条连接是这台电脑上哪个进程开的、它往上是谁」：系统那一趟换成合成的 JSON（与那段固定脚本打出的同形），期望值手写。

use super::*;
use serde_json::json;

/// 线上那一份入参 ⇒ 本体 ⇒ 线上那一份回包（解入参同 typed 分派那一步：形状不对 ⇒ `bad_args`，不去问系统）。
fn answer_with(
    args: &Value,
    query: impl FnOnce() -> Result<String, String>,
) -> Result<Value, Said> {
    let req: TerminalProcessesReq =
        serde_json::from_value(args.clone()).map_err(|e| bad(&e.to_string()))?;
    super::answer_with(&req, query).map(|p| serde_json::to_value(p).unwrap())
}

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
        json!({ "terminals": [{ "ssh": null, "activity": 1, "extra": 1 }] }),
        // 产这一格的 `session-terminals` 恒写 `ssh`（不是经 ssh ⇒ `null`）⇒ 缺这一键是认不得的形，不当「不是经 ssh」。
        json!({ "terminals": [{ "activity": 1 }] }),
        json!({ "terminals": [term("not-an-ip", 1, "192.0.2.9", 22)] }),
        json!({ "terminals": [{ "ssh": { "clientAddr": "192.0.2.5", "clientPort": 70000, "serverAddr": "192.0.2.9", "serverPort": 22 } }] }),
    ] {
        let e = answer_with(&bad, || panic!("入参不对还去问了系统")).unwrap_err();
        assert_eq!(e.code.as_deref(), Some("bad_args"), "{bad}");
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

/// 本机开过的那个终端（`opened_chain`）：标签那个 shell 底下最早起的 ssh 往上；shell 把 ssh exec 了也认；对不上分「没了」「没开着 ssh」。
#[test]
fn a_terminal_opened_here_is_read_from_the_shell_its_label_names() {
    let t = parse_tables(&tables()).unwrap();
    let at = |pid, start| {
        opened_chain(
            &crate::common::seen_terminal::WindowLabel { pid, start },
            &t.procs,
        )
    };
    let pids = |o: OpenedHere| match o {
        OpenedHere::Chain(c) => c.iter().map(|l| l.pid).collect::<Vec<_>>(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        pids(at(600, 3000)),
        vec![700, 600, 500],
        "底下三个 ssh ⇒ 最早起的那个"
    );
    assert_eq!(
        pids(at(700, 4000)),
        vec![700, 600, 500],
        "shell exec 成了 ssh：同一个进程号与起始时刻"
    );
    assert_eq!(
        at(600, 3001),
        OpenedHere::Gone,
        "起始时刻对不上 ⇒ 进程号被复用了"
    );
    assert_eq!(at(601, 3000), OpenedHere::Gone, "那个进程不在");
    assert_eq!(at(500, 2000), OpenedHere::NoSsh, "底下只有 shell、没有 ssh");
}

/// 一个 shell，底下一条 ssh 比它晚起 `ssh_after_shell` 格（profile 慢 · 开窗之后人在窗口里又敲了一条，都是这一形）。
fn ssh_under_shell(ssh_after_shell: u64) -> Tables {
    parse_tables(
        &json!({
            "tcp": [{ "la": "192.0.2.5", "lp": 62414, "ra": "203.0.113.7", "rp": 22, "pid": 700 }],
            "proc": [
                { "pid": 300, "ppid": 4, "name": "explorer.exe", "start": 1000 },
                { "pid": 500, "ppid": 300, "name": "WindowsTerminal.exe", "start": 2000 },
                { "pid": 600, "ppid": 500, "name": "powershell.exe", "start": 3000 },
                { "pid": 700, "ppid": 600, "name": "ssh.exe", "start": 3000 + ssh_after_shell }
            ]
        })
        .to_string(),
    )
    .unwrap()
}

/// 秒 ⇒ 进程表起始时刻的格数（Windows FILETIME 100ns 一格 · Linux USER_HZ）。
fn ticks(secs: u64) -> u64 {
    #[cfg(windows)]
    let per_sec = 10_000_000;
    #[cfg(not(windows))]
    let per_sec = crate::platform::proc::USER_HZ;
    secs * per_sec
}

/// 记录在 ＝ 开窗那条 ssh 还开着（它一退，开窗那一串的收尾就删了记录）⇒ 不比 ssh 比 shell 晚起多久：
/// shell 起得慢（PowerShell profile 载入几十秒）、ssh 晚起的照认。
#[test]
fn an_ssh_started_after_a_slow_profile_is_still_the_one_it_opened() {
    let label = crate::common::seen_terminal::WindowLabel {
        pid: 600,
        start: 3000,
    };
    for secs in [0, 40, 600] {
        assert!(
            matches!(
                opened_chain(&label, &ssh_under_shell(ticks(secs)).procs),
                OpenedHere::Chain(ref c) if c.first().map(|l| l.pid) == Some(700)
            ),
            "shell 起来 {secs} 秒后才起的 ssh 照认"
        );
    }
}

/// 同一形走一遍 `session-window`：记录在 · shell 还是它 · 底下的 ssh 比 shell 晚起 40 秒 ⇒ 本机那一支就答，不问那台。
#[tokio::test]
async fn a_slow_profile_window_is_brought_forward_without_asking_the_machine() {
    let (chain, faced, asked) = window_face(Some("600-3000"), ssh_under_shell(ticks(40))).await;
    assert_eq!(chain, vec![700, 600, 500]);
    assert!(!faced);
    assert_eq!(asked, 0, "不问 devbox");
}

/// 开窗那条 ssh 退了 ⇒ 收尾删了记录；人在同一个窗口里又 ssh 了别台 ⇒ 没有记录，不拉这个窗口，照问 devbox（它说会话没终端连着）。
#[tokio::test]
async fn after_the_opened_ssh_returned_a_new_ssh_in_that_window_is_not_brought_forward() {
    let (chain, faced, asked) = window_face(None, ssh_under_shell(ticks(5))).await;
    assert!(chain.is_empty(), "不拉那个窗口：{chain:?}");
    assert!(faced, "devbox 的原因照摆");
    assert_eq!(asked, 1, "照问 devbox");
}

/// `session-window` 问 devbox 上的一个会话：本机记录是 `record`（`None` ＝ 没有）、这台的两张表是 `tables`，devbox 说会话没终端连着。
/// ⇒ （拉前的进程链 · 摆没摆原因 · 问了 devbox 几回）。
async fn window_face(record: Option<&'static str>, tables: Tables) -> (Vec<u32>, bool, usize) {
    use crate::dial::opened_windows::{Opened, Records};
    use crate::dial::remote_ask::test_peers::{until, Peers};
    struct One(Option<&'static str>);
    impl Records for One {
        fn get(&self, _sid: &str) -> Option<Opened> {
            self.0.map(|l| Opened {
                label: l.parse().unwrap(),
                machine: "devbox".into(),
            })
        }
        fn forget(&self, _sid: &str) {
            panic!("shell 还在 ⇒ 不删");
        }
    }
    let late = tables;
    let raw = json!({
        "tcp": late.conns.iter().map(|c| json!({
            "la": c.local.to_string(), "lp": c.local_port,
            "ra": c.remote.to_string(), "rp": c.remote_port, "pid": c.pid
        })).collect::<Vec<_>>(),
        "proc": late.procs.iter().map(|(pid, p)| json!({
            "pid": pid, "ppid": p.ppid, "name": p.name, "start": p.start
        })).collect::<Vec<_>>(),
    })
    .to_string();
    let peers = Peers::new(&[(
        "devbox",
        json!({ "terminals": [], "why": "detached", "tmux": true }).to_string(),
    )]);
    let req: crate::faces::session_window_face::SessionWindowReq = serde_json::from_value(
        json!({ "machine": "devbox", "sid": "0b9c3f2e-1111-4222-8333-944455556666" }),
    )
    .unwrap();
    let got = crate::faces::session_window_face::answer_with(
        &req,
        |_| unreachable!("远端会话不读本机的会话"),
        move || Ok(raw),
        &Default::default(),
        until(9_000),
        &Peers::table(&["devbox"]),
        &peers,
        One(record),
    )
    .await
    .expect("答得出");
    let asked = peers.asked().len();
    (
        got.chain.iter().map(|l| l.pid).collect(),
        got.face.is_some(),
        asked,
    )
}

/// 记录随用随长：开新终端那一下（建那一层目录时）顺手删掉 shell 已经不在的那几份；shell 还在的 · 正写到一半的旁名 · 不是会话 id 的不动。
#[test]
fn records_whose_shell_is_gone_are_swept_when_the_directory_is_made() {
    let data = std::env::temp_dir().join(format!("ccm-opened-sweep-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    std::fs::create_dir_all(&data).unwrap();
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let dir = data.join(crate::dial::opened_windows::DIR);
    std::fs::create_dir_all(&dir).unwrap();
    let alive = "0b9c3f2e-1111-4222-8333-944455556666";
    let gone = "0b9c3f2e-2222-4222-8333-944455556666";
    let reused = "0b9c3f2e-3333-4222-8333-944455556666";
    let unreadable = "0b9c3f2e-4444-4222-8333-944455556666";
    let part = format!("{alive}.part");
    std::fs::write(dir.join(alive), "600-3000\ndevbox\n").unwrap();
    std::fs::write(dir.join(gone), "601-3000\ndevbox\n").unwrap();
    std::fs::write(dir.join(reused), "600-2999\ndevbox\n").unwrap();
    std::fs::write(dir.join(unreadable), "not a record").unwrap();
    std::fs::write(dir.join(&part), "600-30").unwrap();
    std::fs::write(dir.join("notes.txt"), "x").unwrap();
    crate::dial::opened_windows::sweep(&own, &ssh_under_shell(1000).procs);
    let left = [alive, gone, reused, unreadable, part.as_str(), "notes.txt"]
        .map(|n| (n.to_string(), dir.join(n).exists()));
    let _ = std::fs::remove_dir_all(&data);
    assert_eq!(
        left,
        [
            (alive.to_string(), true),
            (gone.to_string(), false),
            (reused.to_string(), false),
            (unreadable.to_string(), false),
            (part.clone(), true),
            ("notes.txt".to_string(), true),
        ],
        "shell 还在的留 · 不在的 · 进程号被复用的 · 认不出的删 · 旁名与别的名字不动"
    );
}

#[test]
fn an_empty_process_table_sweeps_nothing() {
    let data = std::env::temp_dir().join(format!("ccm-opened-sweep-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let dir = data.join(crate::dial::opened_windows::DIR);
    std::fs::create_dir_all(&dir).unwrap();
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let sid = "0b9c3f2e-1111-4222-8333-944455556666";
    std::fs::write(dir.join(sid), "600-3000\ndevbox\n").unwrap();
    crate::dial::opened_windows::sweep(&own, &HashMap::new());
    let kept = dir.join(sid).exists();
    let _ = std::fs::remove_dir_all(&data);
    assert!(kept, "进程表读成空的是没读成，不当「全没了」");
}
