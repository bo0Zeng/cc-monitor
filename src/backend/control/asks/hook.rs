//! `ccm -- --answer-hook <agent>`：会话级答题 hook 的客户端（那一家的 hook 设置里写的就是它，exec 形、不经 shell）。
//!
//! 读 hook 的 stdin → 连这台家里的常驻后端套接字（同 `--resident-attach` 那一处）→ `needs-ask` 挂上、阻塞等 →
//! 拿到人的答案就把那一家要的那一整份打到 stdout、退 0。**铁律：没收到人的答案，一个字节都不打**（＝不给决定，框照常在终端里问）：
//! 常驻不在 · 断开 · 握手不对 · 不是要人答的那一种 · 连接断了 —— 一律空手退 0。
//!
//! 终端那边先答了，那一家不会给 hook 任何信号（实测：进程不被杀、写 stdout 也照样成功）⇒ 这里自己判：
//! 盯会话进程的状态文件，见过「在等」之后离开「在等」，或状态更新晚于自己起来那一刻而不在「在等」⇒ 判关了、空手退
//! （连接一断，常驻那一侧把挂着的那条撤掉）。挡着终端的那种（终端里根本没有框）不按它判。
//! 零定时器：只等套接字与文件事件。

use crate::agents::SessionActivity;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

/// stdin 上限（同 CLI 入参那一道）：超了整份拒。
const STDIN_CAP: u64 = 1_048_576;
/// 这一问的请求 id（一条连接只问这一次）。
const REQ_ID: &str = "answer-hook";

/// 入口（`args[0]` ＝ 子命令本身，`args[1]` ＝ 那一家的 kind）。恒退 0：hook 的退出码那一家不当决定看，空手退就是「不答」。
pub async fn run(args: &[String]) -> i32 {
    let Some(agent) = args.get(1) else { return 0 };
    let Some(face) = crate::agents::ask_face(agent) else {
        return 0;
    };
    let mut raw = Vec::new();
    // 多读一个字节分得出「正好到上限」与「超了」：超了整份拒（不给决定，框照常在终端里问），不截断着用。
    if tokio::io::stdin()
        .take(STDIN_CAP + 1)
        .read_to_end(&mut raw)
        .await
        .is_err()
        || raw.len() as u64 > STDIN_CAP
    {
        return 0;
    }
    let Ok(hook) = serde_json::from_slice::<Value>(&raw) else {
        return 0;
    };
    let Some(asked) = (face.read)(&hook) else {
        return 0;
    };
    // 经环境那一路来的 hook 子进程也继承得到：认不出是 ccm 起的那一个会话 ⇒ 空手退（那一发照那一家自己的默认办，
    // 会话里 Bash 起的 `claude -p` 不在这里挂着等）。
    if let Some(p) = face.plugin {
        let mine = std::env::var(p.env).ok();
        let ours = crate::platform::paths::home_dir()
            .map(|h| crate::agents::asks::plugin_dir_under(&h.to_string_lossy()))
            .unwrap_or_default();
        let asking = asking_pid();
        let in_table = asking.is_some_and(|pid| in_table(pid, &asked.sid));
        let marker = asking.map_or(crate::platform::proc::EnvRead::Unreadable, |pid| {
            crate::platform::proc::proc_env_var(pid, p.child_marker)
        });
        if !admitted(mine.as_deref(), &ours, in_table, marker) {
            return 0;
        }
    }
    let closed = (!asked.blocks_terminal)
        .then(|| watch_terminal(agent, &asked.sid))
        .flatten();
    if let Some(out) = ask_resident(agent, &hook, closed).await {
        let mut stdout = tokio::io::stdout();
        let _ = stdout.write_all(out.to_string().as_bytes()).await;
        let _ = stdout.flush().await;
    }
    0
}

/// 经常驻后端问；拿到决定 ⇒ `Some`。`closed` 先到 ⇒ `None`（丢掉连接 ＝ 撤）。
async fn ask_resident(
    agent: &str,
    hook: &Value,
    closed: Option<(
        crate::platform::watch_file::Watching,
        tokio::sync::oneshot::Receiver<()>,
    )>,
) -> Option<Value> {
    let dh = crate::platform::paths::data_home()?;
    let sock = own_chan::connect(&relay_route_core::listen_socket_for(&dh))
        .await
        .ok()?;
    let (rd, mut wr) = tokio::io::split(sock);
    let mut lines = tokio::io::BufReader::new(rd).lines();
    // 先读 hello，再交 attach 那一行（协议 §1 · §3）。
    let hello: Value = serde_json::from_str(&lines.next_line().await.ok()??).ok()?;
    if hello.get("kind").and_then(Value::as_str) != Some("hello") {
        return None;
    }
    wr.write_all(b"{\"attach\":true}\n").await.ok()?;
    let ack: Value = serde_json::from_str(&lines.next_line().await.ok()??).ok()?;
    if ack.get("attach").and_then(Value::as_str) != Some("ok") {
        return None;
    }
    let req = json!({ "id": REQ_ID, "cmd": "needs-ask", "args": { "agent": agent, "hook": hook } });
    wr.write_all(format!("{req}\n").as_bytes()).await.ok()?;
    let (_watching, mut closed) = match closed {
        Some((w, rx)) => (Some(w), Some(rx)),
        None => (None, None),
    };
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let line = line.ok()??;
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                if v.get("kind").and_then(Value::as_str) == Some("reply")
                    && v.get("id").and_then(Value::as_str) == Some(REQ_ID)
                {
                    if v.get("ok").and_then(Value::as_bool) != Some(true) {
                        return None;
                    }
                    return v.pointer("/data/decision").filter(|d| d.is_object()).cloned();
                }
            }
            r = async {
                match closed.as_mut() {
                    Some(rx) => rx.await,
                    None => std::future::pending().await,
                }
            } => {
                let _ = r;
                return None;
            }
        }
    }
}

/// 盯这条会话的状态文件，判「终端那边答了」：返回监听（活着才盯）与「关了」那一声。挂不上 ⇒ `None`（只能等人答或连接断）。
fn watch_terminal(
    agent: &str,
    sid: &str,
) -> Option<(
    crate::platform::watch_file::Watching,
    tokio::sync::oneshot::Receiver<()>,
)> {
    let local = crate::agents::local_face_among(crate::agents::REGISTRY, agent)?;
    let dir = (local.pidfile_dir)(&(local.home_at)(None, true));
    let started = now_ms();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let mut tx = Some(tx);
    let mut seen_waiting = false;
    let sid = sid.to_string();
    let read_dir = dir.clone();
    let w = crate::platform::watch_file::watch(
        &[(dir, false)],
        |p| p.extension().and_then(|e| e.to_str()) == Some("json"),
        "answer-hook-status",
        move |_paths| {
            if tx.is_none() {
                return;
            }
            let states = statuses(&read_dir, &sid);
            if closed_now(&states, started, &mut seen_waiting) {
                if let Some(tx) = tx.take() {
                    let _ = tx.send(());
                }
            }
        },
    )
    .ok()?;
    // 挂上之后先判一次此刻的样子（之前的那几次变动没赶上事件）。
    w.kick();
    Some((w, rx))
}

/// 这条会话各个进程此刻的（状态, 状态更新于）。
fn statuses(dir: &std::path::Path, sid: &str) -> Vec<(Option<SessionActivity>, Option<u64>)> {
    let Ok(rd) = crate::platform::fs::Ro::read_dir(dir) else {
        return Vec::new();
    };
    rd.flatten()
        .filter_map(|e| crate::platform::fs::Ro::read(e.path()).ok())
        .filter_map(|b| serde_json::from_slice::<Value>(&b).ok())
        .filter(|v| v.get("sessionId").and_then(Value::as_str) == Some(sid))
        .map(|v| {
            (
                crate::agents::pidfile_activity(&v),
                v.get("statusUpdatedAt").and_then(Value::as_u64),
            )
        })
        .collect()
}

/// 判关了没有（纯函数）：有一个进程在「在等」⇒ 没关（记下见过）；都不在「在等」且（见过「在等」或状态更新晚于 `started`）⇒ 关了。
/// 一个进程都没有（会话退了）且见过「在等」⇒ 关了。
pub(crate) fn closed_now(
    states: &[(Option<SessionActivity>, Option<u64>)],
    started: u64,
    seen_waiting: &mut bool,
) -> bool {
    if states
        .iter()
        .any(|(a, _)| *a == Some(SessionActivity::NeedsYou))
    {
        *seen_waiting = true;
        return false;
    }
    *seen_waiting || states.iter().any(|(_, at)| at.is_some_and(|t| t > started))
}

fn now_ms() -> u64 {
    crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// 发问的那个进程：hook 是它的直接子进程（exec 形，不经 shell）。说不出 ⇒ `None`。
fn asking_pid() -> Option<u32> {
    #[cfg(unix)]
    {
        Some(std::os::unix::process::parent_id())
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// 常驻那份「ccm 起的会话」表：ccm 给发问那个进程留过便条（`/clear` 换了会话 id 进程不变，照样认得）· 这个会话 id 记在起会话那份记录里。
fn in_table(pid: u32, sid: &str) -> bool {
    let Some(data) = crate::platform::paths::data_home() else {
        return false;
    };
    crate::control::launch_account::note_of(
        &data,
        pid,
        crate::platform::proc::start_epoch_from_ticks(crate::platform::proc::proc_starttime(pid)),
    )
    .is_some()
        || crate::control::launch_account::last_of(Some(&data), sid).is_some()
}

/// 这一发 hook 认不认（纯函数）：自己的环境里那个插件目录变量没带我们的目录 ⇒ 不是经环境来的（`--settings` 那一路不传给子进程），认；
/// 表里有 ⇒ 认；发问那个进程起来时的环境里没有那一家的嵌套标记 ⇒ 认（ccm 起会话前清掉了它）；带着 ⇒ 子进程；读不出 ⇒ 认不出。
pub(crate) fn admitted(
    own_dirs: Option<&str>,
    ours: &str,
    in_table: bool,
    marker: crate::platform::proc::EnvRead,
) -> bool {
    if !own_dirs.is_some_and(|l| !ours.is_empty() && crate::agents::asks::dirs_have(l, ours)) {
        return true;
    }
    in_table || marker == crate::platform::proc::EnvRead::Unset
}
