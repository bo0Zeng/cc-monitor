//! 拨号那一侧判据里引到上层的那几条（子命令闸门 · 提交那一侧的暂存根 · 起会话那一行的字节表 · `session-window` 那一面 ·
//! 问别台常驻那一跳）：拨号那一侧是独立的包（`backend-dial`）、够不到上层 ⇒ 挂在主 crate（`cross/dial.rs`）。
//! 其余拨号的判据住各自模块底下。

use crate::common::said::Said;
use serde_json::{json, Value};

/// ★ 子命令闸门不认 `--dial`：拨号只在本后端自己的几处用（各台的常驻流 · 问那台 · 传输 · 部署 · 开终端），
/// 没有一条入方向命令替界面开字节流。闸门认一个子命令却没有臂接它 ⇒ 半截入口。
#[test]
fn the_old_dial_entry_is_gone() {
    assert!(
        !crate::stream::cli::is_subcommand("--dial"),
        "子命令闸门认 `--dial` —— 拨号不开子命令入口"
    );
    // 正控：同一个闸门认得出一条真子命令（不是「这一问什么都答不认」）。
    assert!(
        crate::stream::cli::is_subcommand("--resident-ensure"),
        "正控 `--resident-ensure` 闸门不认 —— 本条在空转"
    );
}

/// 暂存区那一根（拨号核心装的远端写根）与提交那一侧（`files::commit`）是同一个串。
#[test]
fn the_staging_root_is_the_one_the_commit_side_reads() {
    assert_eq!(
        ssh_dial::sftp::STAGING_ROOT,
        crate::files::commit::STAGING_DIR
    );
}

// ── 开终端那道守卫 × 后端自己渲的每一条远端命令 ──

/// 线上那一形的一台。
fn machine(host: &str, user: &str, port: u16, key: Option<&str>) -> Value {
    json!({ "host": host, "user": user, "port": port, "keyPath": key, "label": host })
}

/// 那一台进这一端的机器表、线上只交它的名字，按 PowerShell 那一形渲（Windows 上开终端的那一行）⇒ `line` 那一格。
fn run(m: Value, cmd: &str) -> Result<String, Said> {
    let h = crate::dial::machine::loose(&m)?;
    let req = crate::wire::typed::take(&json!({ "machine": h.name(), "command": cmd }))
        .map_err(|f| Said::new("bad_args", f.said))?;
    let ssh =
        crate::platform::ssh_client::SshClient::At(r"C:\Windows\System32\OpenSSH\ssh.exe".into());
    crate::dial::terminal::answer_with(&req, &[h], &[], true, &ssh, None).map(|r| {
        serde_json::to_value(r).unwrap()["line"]
            .as_str()
            .unwrap()
            .to_string()
    })
}

/// 流进 `terminal-ssh` 的远端命令的全部来路（起会话只有那一行 `ccm …`，就地 resume 那一格外层包一层 tmux）×
/// 那一行的字节表的每一格 × 典型工作目录 ⇒ `(哪条, 渲出来的命令)`。渲不出来的（拒）不在里面。
fn every_rendered_remote_command(cwds: &[&str]) -> Vec<(String, String)> {
    use crate::control::launch_render::ccm_invocation::tests as table;
    let mut out = Vec::new();
    for c in table::cells() {
        for cwd in std::iter::once(c.1.cwd.clone()).chain(cwds.iter().map(|d| Some(d.to_string())))
        {
            let mut cell = c.clone();
            cell.1.cwd = cwd.clone();
            if let Ok(cmd) = table::render_cell(&cell) {
                out.push((format!("cli {} · cwd {cwd:?}", c.0), cmd));
            }
        }
    }
    out
}

/// ★ Windows 上开远端会话不许被自家守卫拒：后端自己渲出、会交给 `terminal-ssh` 的每一条远端命令（[`every_rendered_remote_command`]：典型工作目录 ——
/// 空格 · 中文 · 弯引号 · 单引号）都过这道守卫（零命中）；外层包了 tmux 的那一格真的在人群里。
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
    assert!(
        all.iter().any(|(_, c)| c.contains("--ccm-tmux=")),
        "建 tmux 会话的那几格一条都没渲出来 —— 人群塌了"
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
            run(machine("h", "u", 22, None), c)
                .unwrap_err()
                .code
                .as_deref(),
            Some("refused"),
            "守卫被放宽了：{c}"
        );
    }
}

// ── `session-window` 那一面：开窗那条 ssh 的进程链 ──

/// 这台的两张表（与那段固定脚本打出的同形）：一个 shell，底下一条 ssh 比它晚起 `ssh_after_shell` 格。
fn ssh_under_shell(ssh_after_shell: u64) -> String {
    json!({
        "tcp": [{ "la": "192.0.2.5", "lp": 62414, "ra": "203.0.113.7", "rp": 22, "pid": 700 }],
        "proc": [
            { "pid": 300, "ppid": 4, "name": "explorer.exe", "start": 1000 },
            { "pid": 500, "ppid": 300, "name": "WindowsTerminal.exe", "start": 2000 },
            { "pid": 600, "ppid": 500, "name": "powershell.exe", "start": 3000 },
            { "pid": 700, "ppid": 600, "name": "ssh.exe", "start": 3000 + ssh_after_shell }
        ]
    })
    .to_string()
}

/// 秒 ⇒ 进程表起始时刻的格数（Windows FILETIME 100ns 一格 · Linux USER_HZ）。
fn ticks(secs: u64) -> u64 {
    #[cfg(windows)]
    let per_sec = 10_000_000;
    #[cfg(not(windows))]
    let per_sec = crate::platform::proc::USER_HZ;
    secs * per_sec
}

/// 记录在 · shell 还是它 · 底下的 ssh 比 shell 晚起 40 秒 ⇒ 本机那一支就答，不问那台。
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

/// `session-window` 问 devbox 上的一个会话：本机记录是 `record`（`None` ＝ 没有）、这台的两张表是 `raw`，devbox 说会话没终端连着。
/// ⇒ （拉前的进程链 · 摆没摆原因 · 问了 devbox 几回）。
async fn window_face(record: Option<&'static str>, raw: String) -> (Vec<u32>, bool, usize) {
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

// ── 问别台常驻那一跳：发起方一走就收工 ──

/// ★ 问别台常驻那一跳（`remote_ask::ask_resident`）在那台起的 `--resident-call` 是一次性的：发起方一走（ssh 断开 ⇒ 它 stdout 那根管子的读端没人了），
/// 就不再等那台常驻、当场收工 —— 不攥着一条常驻连接空等到发起方给的期限（10-28 沙箱 sshd 实测：断开后照活到期限）。
/// 读端还在 ⇒ 照旧等到期限、报 `timed_out`（「发起方走了」不是一碰就响）；读端已关 ⇒ 交回「走了」，不等到期限。
/// 扮那台常驻的：绑了套接字、不收不说（连得上，hello 永远不来）。
#[tokio::test]
#[cfg(unix)]
async fn a_resident_call_stops_waiting_once_the_asker_is_gone() {
    use crate::control::resident::{call_while, stdout_gone};
    use crate::platform::child::Deadline;
    let dh = std::env::temp_dir().join("ra-gone");
    let _ = std::fs::remove_dir_all(&dh);
    let run = relay_route_core::listen_dir_for(&dh);
    std::fs::create_dir_all(&run).unwrap();
    std::fs::set_permissions(&run, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    let _mute = own_chan::bind(&relay_route_core::listen_socket_for(&dh)).expect("绑");
    let asked = crate::wire::listen::ResidentCall {
        cmd: "machines-list".into(),
        args: json!({}),
    };
    let tz = crate::common::time::Tz::default();

    // 读端一直在：等到期限。
    let (rd, wr) = std::io::pipe().unwrap();
    let got = call_while(
        &dh,
        &asked,
        Some(300),
        &tz,
        Deadline::millis(300),
        stdout_gone(wr.into()),
    )
    .await;
    let code = got.map(|r| r.map_err(|f| f.code));
    assert_eq!(
        code,
        Some(Err(crate::wire::listen::CALL_TIMED_OUT.to_string())),
        "读端还在却当成发起方走了"
    );
    drop(rd);

    // 读端已关：不等那 30 秒期限（还在等 ⇒ 30 秒后交回 `timed_out`，下面那一条红）。
    let (rd, wr) = std::io::pipe().unwrap();
    drop(rd);
    let got = call_while(
        &dh,
        &asked,
        Some(30_000),
        &tz,
        Deadline::secs(30),
        stdout_gone(wr.into()),
    )
    .await;
    let _ = std::fs::remove_dir_all(&dh);
    assert!(
        got.is_none(),
        "发起方走了（读端关了）还在等那台常驻、等到了期限：{:?}",
        got.map(|r| r.map_err(|f| f.code))
    );
}
