//! **截图台架的无头壳**：壳那几块真代码（本机那条 stdio 读循环 → `consume_local` → `session_book` → `event_replay` → 通道交格，
//! 以及 `chan_call` 那一跳 `call_via(InboundBackends)`）不带窗口跑起来，页里的假适配层（`tests/shots/fake/backend.ts`）经它说话。
//!
//! 只在特性 `shots` 下编（例子 `ccm-shots-shell`，由 `tests/shots/real/pool.mjs` 编、起）；发版构建里没有它
//! （判据 `tests/frontend/shell/shots_feature_guard_tests.rs`）。
//!
//! # 和产品哪里一样、哪里不一样
//! - 一样：每台机器的帧都经 [`crate::local_backend::local_stdio_consumer`]（产品本机那条读循环）进一个 [`crate::stream_source::consume_local`]，
//!   成品进同一本 `session_book`、同一个 [`EventReplay`]；出口接线是产品那一份 [`crate::wire_replay_outlets`]；页里拿到的格是
//!   [`crate::chan::webview::webview_item`] 翻的那一形；`chan_call` 走 [`crate::chan::webview::call_via`] ＋ [`InboundBackends`]，失败那一形是 [`crate::chan::webview::fail`]。
//! - 不一样：远端那几台在产品里走 ssh 那条 `stream_source::run`（握手 · 版本协商 · 重连退避），这里也走本机那条读循环（按 origin 区分）——
//!   截图台架覆盖不到 ssh 那条路。窗口只有一个（订阅都记在 [`LABEL`] 名下）。
//!
//! # 线上（标准输入 / 标准输出，一行一个 JSON）
//! 收：`{"t":"machines","machines":[{"origin","argv":[…]}]}`（起各台后端）·
//! `{"t":"sub","id","origin","kind","want"}` · `{"t":"want","id","more"}` · `{"t":"stop","id"}` · `{"t":"ready","priority_sid"}` ·
//! `{"t":"call","n","origin","op","payload","left_ms"}`（`payload` 是请求体原文）· `{"t":"kill","origin"}`（那台后端当场杀掉：演「断了」）·
//! `{"t":"offer","n","origin"}`（那台握上手没有；台架按它等各台起好，等法住台架那一侧）。
//! 发：`{"t":"items","sub","items"}`（同 `chan-items` 事件体）· `{"t":"reply","n","ok","body"|"fail"}` · `{"t":"offer","n","up"}` · `{"t":"health",…}`。

use crate::chan::host::InboundBackends;
use crate::chan::router::Backends as _;
use crate::event_replay::{EventReplay, ItemSink};
use crate::local_backend::StdioRoute;
use crate::local_lines::StdioOut;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

/// 页那一扇窗（订阅都记在它名下）。
pub const LABEL: &str = "shots";

#[derive(Deserialize)]
struct Machine {
    origin: crate::origin::Origin,
    argv: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
enum Cmd {
    Machines {
        machines: Vec<Machine>,
    },
    Sub {
        id: u64,
        origin: crate::origin::Origin,
        kind: String,
        want: u32,
    },
    Want {
        id: u64,
        more: u32,
    },
    Stop {
        id: u64,
    },
    Ready {
        priority_sid: Option<String>,
    },
    Call {
        n: u64,
        origin: crate::origin::Origin,
        op: String,
        payload: String,
        /// 出口的声明（页里那一问带的 `view`），原样交 `call_via`；没带 ⇒ 全量。
        view: Option<serde_json::Value>,
        left_ms: u64,
    },
    Kill {
        origin: crate::origin::Origin,
    },
    Offer {
        n: u64,
        origin: crate::origin::Origin,
    },
}

/// 标准输出：一行一个 JSON，一把锁（几条线程都往这里写）。
fn say(v: serde_json::Value) {
    let mut out = std::io::stdout().lock();
    // 管道断了 ⇒ 台架收场了，这里没人可说。
    if writeln!(out, "{v}").and_then(|()| out.flush()).is_err() {
        std::process::exit(0);
    }
}

struct StdoutSink;

impl ItemSink for StdoutSink {
    fn deliver(&self, _label: &str, sub: u64, items: Vec<crate::chan::wire::Item>) {
        let d = crate::chan::webview::Delivery {
            sub,
            items: items
                .into_iter()
                .map(crate::chan::webview::webview_item)
                .collect(),
        };
        say(serde_json::json!({ "t": "items", "sub": d.sub, "items": d.items }));
    }
}

/// 例子 `ccm-shots-shell` 的入口。
pub fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("CCM_SHOTS_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();
    let replay = Arc::new(EventReplay::new());
    replay.attach_sink(Arc::new(StdoutSink));
    crate::wire_replay_outlets(&replay);
    let health: crate::stream_source::HealthOut = Arc::new(|p| {
        say(serde_json::json!({ "t": "health", "payload": p }));
        Ok(())
    });
    crate::stream_source::install_local_health(health.clone());
    let children: Arc<Mutex<HashMap<String, crate::spawn_managed::ManagedChild>>> = Arc::default();

    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let cmd: Cmd = match serde_json::from_str(&line) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("shots_shell：认不出这一行（{e}）：{line}");
                continue;
            }
        };
        match cmd {
            Cmd::Machines { machines } => {
                crate::session_book::machines_changed(
                    machines
                        .iter()
                        .map(|m| m.origin.as_wire_str().to_string())
                        .collect(),
                );
                for m in machines {
                    start(&m, &replay, &health, &children);
                }
            }
            Cmd::Offer { n, origin } => {
                // 那台握上手没有（入方向通道登记了没有）：台架按它等各台起好。
                let up = InboundBackends.offer(&origin).is_some();
                say(serde_json::json!({ "t": "offer", "n": n, "up": up }));
            }
            Cmd::Sub {
                id,
                origin,
                kind,
                want,
            } => replay.subscribe(LABEL, id, &origin, &kind, None, want),
            Cmd::Want { id, more } => replay.want(LABEL, id, more),
            Cmd::Stop { id } => replay.stop(LABEL, id),
            Cmd::Ready { priority_sid } => {
                let replay = replay.clone();
                tauri::async_runtime::spawn(async move {
                    replay.ready_point(priority_sid.as_deref()).await;
                });
            }
            Cmd::Call {
                n,
                origin,
                op,
                payload,
                view,
                left_ms,
            } => {
                tauri::async_runtime::spawn(async move {
                    let r = crate::chan::webview::call_via(
                        &InboundBackends,
                        origin.clone(),
                        op.clone(),
                        payload.into_bytes(),
                        view,
                        std::time::Duration::from_millis(left_ms),
                        None,
                    )
                    .await;
                    say(match r {
                        Ok(body) => serde_json::json!({
                            "t": "reply", "n": n, "ok": true,
                            "body": String::from_utf8_lossy(&body.0),
                        }),
                        Err(e) => serde_json::json!({
                            "t": "reply", "n": n, "ok": false,
                            "fail": crate::chan::webview::fail(&origin, &op, e),
                        }),
                    });
                });
            }
            Cmd::Kill { origin } => {
                if let Some(mut c) = children
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(origin.as_wire_str())
                {
                    if let Err(e) = c.kill() {
                        tracing::warn!(
                            "shots_shell：杀 {} 的后端没杀成：{e}",
                            origin.as_wire_str()
                        );
                    }
                }
            }
        }
    }
    // 台架关了标准输入 ⇒ 各台后端一起收。
    for (origin, mut c) in children.lock().unwrap_or_else(|e| e.into_inner()).drain() {
        if let Err(e) = c.kill() {
            tracing::warn!("shots_shell：收场时杀 {origin} 的后端没杀成：{e}");
        }
    }
}

/// 起一台：那台的后端进程（台架给好整条命令，含沙箱）→ 产品本机那条 stdio 读循环（按 origin）→ 它自己的 `consume_local`。
fn start(
    m: &Machine,
    replay: &Arc<EventReplay>,
    health: &crate::stream_source::HealthOut,
    children: &Arc<Mutex<HashMap<String, crate::spawn_managed::ManagedChild>>>,
) {
    let Some((prog, args)) = m.argv.split_first() else {
        tracing::error!("shots_shell：{} 没给命令", m.origin.as_wire_str());
        return;
    };
    let mut cmd = std::process::Command::new(prog);
    cmd.args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    let mut child = match crate::spawn_managed::spawn_managed_cmd(
        &mut cmd,
        crate::spawn_managed::ConsolePolicy::Hidden,
        crate::spawn_managed::Lifetime::JobKillOnClose,
        crate::spawn_managed::StderrSink::Inherit,
    ) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("shots_shell：{} 的后端起不来：{e}", m.origin.as_wire_str());
            return;
        }
    };
    let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        tracing::error!("shots_shell：{} 的后端没有管道", m.origin.as_wire_str());
        return;
    };
    let (tx, rx) = tokio::sync::mpsc::channel(crate::local_lines::LOCAL_LINES_CAPACITY);
    tauri::async_runtime::spawn(crate::stream_source::consume_local(
        m.origin.as_wire_str().to_string(),
        rx,
        replay.clone(),
        health.clone(),
    ));
    let route = StdioRoute {
        origin: m.origin.clone(),
        place: m.origin.as_wire_str().to_string(),
        out: StdioOut::Own(tx),
    };
    std::thread::spawn(move || crate::local_backend::local_stdio_consumer(&route, stdin, stdout));
    children
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(m.origin.as_wire_str().to_string(), child);
}
