//! 〔MIG-1 · `设计/99 §2.1 ⑬`〕**端口转发（F58，`-L`）的账住本机常驻后端**：起 · 停 · 列三条帧命令，界面经
//! `chan.call(<local>, "forward-*", …)` 直接问（monitor 那三条 Tauri 命令与它手里那张转发账退役）。
//!
//! # 形状
//!
//! ```text
//!  界面 ── forward-start {origin, localPort, remoteHost, remotePort} ──▶ 本机后端
//!           ① 规格先过围栏（任何查表 / 拨号之前）
//!           ② 查可达表（`remote_ask::REACH`：monitor 在每台远端流握手那一刻登记的「怎么够到那台」）；
//!              〔MIG-1 续〕表里没有（那台的流没起来）⇒ 按界面一并交来的那台配置自己组请求（`dial/machine.rs`），不拒
//!           ③ 把那份拨号请求改成 `use: forward` ⇒ `dial::uses::run`（池里那条 SSH 连接；绑本机回环口是它的事）
//!           ④ 等 ack：口绑好了、连上了才算起来；不成 ⇒ 原话回、不进账
//!           ⑤ 进账：一个任务读「接进了第 n 条连接」那几行，计数照抄
//!  界面 ── forward-stop {id} ──▶ 从账上摘掉 ⇒ 那个任务被收 ⇒ 链路被丢 ⇒ 放掉本地口、收掉在飞隧道
//!  界面 ── forward-list ──▶ {forwards:[{id, origin, localPort, remoteHost, remotePort, state, connCount}]}
//! ```
//!
//! **账是本进程一张**（不是每条流连接一张）：常驻后端活得比界面长（V105），界面重开之后列得出、停得掉上次开的转发。
//! v1 仍是即席（不落盘）：后端重启 ⇒ 账空、口全放。
//!
//! 零定时器：没有「过一会儿再看看」—— 状态是读出来的（那个任务收工了没有），不是轮询出来的。

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use copy_core::copy_text;
use serde_json::{json, Value};
use tokio::io::{BufReader, DuplexStream};

use crate::dial::DialRequest;
use crate::remote_ask::{self, AbortOnDrop};

/// 账上最多几条（有界资源；每条占一个本机口，一个人开不出这么多）。
pub const MAX_FORWARDS: usize = 64;

/// ack 那一行的上限（与 `remote_ask` 读 ack 同一个数）。
const ACK_CAP: u64 = 64 * 1024;

/// 计数那一行（`{"accepted":n}`）的上限。
const COUNT_CAP: u64 = 4 * 1024;

/// 一条转发的规格（界面交来的那四格）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Spec {
    pub(crate) origin: String,
    pub(crate) local_port: u16,
    pub(crate) remote_host: String,
    pub(crate) remote_port: u16,
}

fn invalid(detail: &str) -> (&'static str, String) {
    ("invalid_args", crate::common::contract::malformed(detail))
}

fn port_of(args: &Value, key: &str) -> Result<u16, (&'static str, String)> {
    let n = args
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid(&format!("missing `{key}` (a port number)")))?;
    u16::try_from(n).map_err(|_| invalid(&format!("`{key}` is not a port number")))
}

/// 读规格并过围栏 —— **在任何查表 / 拨号之前**（`INVARIANTS §47`：外部值交给对端之前本侧先判）。
/// 端口 0 · 远端 host 空白 ⇒ `bad_spec`（原 monitor `validate_spec` 那三句）；缺格 / 类型不对 ⇒ `invalid_args`。
pub(crate) fn parse_spec(args: &Value) -> Result<Spec, (&'static str, String)> {
    let origin = args
        .get("origin")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("missing `origin` (the machine name)"))?;
    let local_port = port_of(args, "localPort")?;
    let remote_port = port_of(args, "remotePort")?;
    let remote_host = args
        .get("remoteHost")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("missing `remoteHost`"))?;
    if local_port == 0 {
        return Err(("bad_spec", copy_text("beForwards.spec.localPort", &[])));
    }
    if remote_host.trim().is_empty() {
        return Err(("bad_spec", copy_text("beForwards.spec.remoteHost", &[])));
    }
    if remote_port == 0 {
        return Err(("bad_spec", copy_text("beForwards.spec.remotePort", &[])));
    }
    Ok(Spec {
        origin: origin.to_string(),
        local_port,
        remote_host: remote_host.to_string(),
        remote_port,
    })
}

/// 把可达表里那份拨号请求改成「绑这个本机口、隧道到那个远端口」（纯；抽出来是为了判据）。
pub(crate) fn forward_request(dial: &Value, spec: &Spec) -> Result<DialRequest, String> {
    let mut v = dial.clone();
    let obj = v.as_object_mut().ok_or(crate::common::contract::malformed(
        "dial request is not an object",
    ))?;
    obj.insert("use".into(), json!("forward"));
    obj.insert(
        "forward".into(),
        json!({
            "local_port": spec.local_port,
            "remote_host": spec.remote_host,
            "remote_port": spec.remote_port,
        }),
    );
    obj.insert("stages".into(), json!(false));
    obj.insert("probe".into(), json!(false));
    crate::dial::parse_request_value(&v).map_err(|(_, m)| m)
}

/// 账上一条。**丢掉它 = 收掉这条转发**（`pump` 是 [`AbortOnDrop`]，它手里攥着链路那一侧的任务）。
struct Entry {
    spec: Spec,
    pump: AbortOnDrop,
    conn_count: Arc<AtomicU64>,
}

/// 转发账（本进程一张）。
pub(crate) struct Ledger {
    next: AtomicU64,
    rows: Mutex<BTreeMap<u64, Entry>>,
}

impl Ledger {
    pub(crate) const fn new() -> Self {
        Ledger {
            next: AtomicU64::new(1),
            rows: Mutex::new(BTreeMap::new()),
        }
    }

    fn rows(&self) -> std::sync::MutexGuard<'_, BTreeMap<u64, Entry>> {
        self.rows.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// 生产那一张。
pub(crate) static LEDGER: Ledger = Ledger::new();

fn id_text(n: u64) -> String {
    format!("fwd-{n}")
}

fn id_num(id: &str) -> Option<u64> {
    id.strip_prefix("fwd-")?.parse().ok()
}

/// `forward-start` 的本体：账与可达表由调用方给，`serve` 是链路那一侧（生产 = `dial::uses::run`；判据用替身）。
/// `serve` 拿到的是：改好的拨号请求 · 上行读端（本侧一个字节都不写，丢了 = 界面走了）· 下行写端（ack ＋ 计数行）。
pub(crate) async fn start_with<F, Fut>(
    args: &Value,
    reach: &remote_ask::Table,
    ledger: &Ledger,
    serve: F,
) -> Result<Value, (&'static str, String)>
where
    F: FnOnce(DialRequest, DuplexStream, DuplexStream) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let spec = parse_spec(args)?;
    // 〔MIG-1 续 · 主会话裁〕那台的流握手过 ⇒ 用可达表里那份（带着 monitor 排好的 last-good 顺序与它的 agent 套接字）；
    //   没握手过（流没起来）⇒ **不拒**：按界面交来的那台配置自己组请求、自己拨（与起流同一个池，`dial/machine.rs`）。
    let reached = remote_ask::lock(reach)
        .get(&spec.origin)
        .map(|r| r.dial.clone());
    let dial = match reached {
        Some(d) => d,
        // 界面交来的那台原样的配置（线上那一份拨号的形状，组法在 `dial/machine.rs::resolve`）；先读一遍，坏了当场拒。
        None if args.get("machine").is_some_and(|m| !m.is_null()) => {
            super::machine::from_args(args)?;
            json!({ "machine": args.get("machine"), "saved": args.get("saved"), "jump": args.get("jump") })
        }
        None => return Err(("unreachable", remote_ask::unreachable_message(&spec.origin))),
    };
    if ledger.rows().len() >= MAX_FORWARDS {
        return Err((
            "full",
            copy_text(
                "beForwards.start.full",
                &[("max", &MAX_FORWARDS.to_string())],
            ),
        ));
    }
    let req = forward_request(&dial, &spec).map_err(|e| ("failed", e))?;
    let (up_w, up_r) = tokio::io::duplex(64);
    let (down_w, down_r) = tokio::io::duplex(16 * 1024);
    // 本函数（的 future）被丢 ⇒ 链路那一侧一起收（帧期限到点 / `cancel`）。
    let link = AbortOnDrop(tokio::spawn(serve(req, up_r, down_w)));
    let mut rd = BufReader::new(down_r);
    let failed = |said: String| {
        (
            "failed",
            copy_text(
                "beForwards.start.failed",
                &[("machine", &spec.origin), ("why", &said)],
            ),
        )
    };
    let ack = remote_ask::capped_line(&mut rd, ACK_CAP)
        .await
        .map_err(failed)?
        .ok_or_else(|| failed(copy_text("beForwards.start.noAck", &[])))?;
    let ack: Value = serde_json::from_str(&ack).map_err(|e| failed(e.to_string()))?;
    if ack.get("ok").and_then(Value::as_bool) != Some(true) {
        let why = ack
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| copy_text("beForwards.start.noAck", &[]));
        return Err(failed(why));
    }
    let conn_count = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&conn_count);
    let origin = spec.origin.clone();
    // 链路那一侧每接进一条连接报一行 —— 计数照抄它的数；管子关了 = 这条转发没了（状态读成 `error`）。
    let pump = AbortOnDrop(tokio::spawn(async move {
        let _link = link;
        let _up = up_w;
        loop {
            match remote_ask::capped_line(&mut rd, COUNT_CAP).await {
                // 超长 / 读不动 ⇒ 这条转发收工，列表里读成 `error`（降级，说清是哪一条）。
                Err(e) => {
                    tracing::warn!("端口转发 [{origin}] 的计数读不下去了，列表里这一条的 state 读成 error：{e}");
                    break;
                }
                Ok(Some(line)) => {
                    if let Some(n) = serde_json::from_str::<Value>(&line)
                        .ok()
                        .and_then(|v| v.get("accepted").and_then(Value::as_u64))
                    {
                        counter.store(n, Ordering::Relaxed);
                    }
                }
                Ok(None) => {
                    tracing::warn!(
                        "端口转发 [{origin}] 的链路收工了（远端断开 / 本地口 accept 失败）"
                    );
                    break;
                }
            }
        }
    }));
    let n = ledger.next.fetch_add(1, Ordering::Relaxed);
    ledger.rows().insert(
        n,
        Entry {
            spec,
            pump,
            conn_count,
        },
    );
    Ok(json!({ "id": id_text(n) }))
}

/// `forward-stop` 的本体：从账上摘掉（丢掉那一条 ⇒ 收掉链路 ⇒ 后端放掉本地口、收掉在飞隧道）。
pub(crate) fn stop_with(args: &Value, ledger: &Ledger) -> Result<Value, (&'static str, String)> {
    let id = args
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("missing `id`"))?;
    let entry = id_num(id)
        .and_then(|n| ledger.rows().remove(&n))
        .ok_or_else(|| {
            (
                "not_found",
                copy_text("beForwards.stop.notFound", &[("id", id)]),
            )
        })?;
    drop(entry);
    Ok(json!({ "id": id }))
}

/// `forward-list` 的本体：账上每一条的规格 · 状态（`running` = 链路还在 · `error` = 链路收工了）· 累计连接数。
pub(crate) fn list_with(ledger: &Ledger) -> Value {
    let rows: Vec<Value> = ledger
        .rows()
        .iter()
        .map(|(n, e)| {
            json!({
                "id": id_text(*n),
                "origin": e.spec.origin,
                "localPort": e.spec.local_port,
                "remoteHost": e.spec.remote_host,
                "remotePort": e.spec.remote_port,
                "state": if e.pump.0.is_finished() { "error" } else { "running" },
                "connCount": e.conn_count.load(Ordering::Relaxed),
            })
        })
        .collect();
    json!({ "forwards": rows })
}

/// 生产的 `forward-start`：进程里那张可达表 ＋ 那张账 ＋ 真拨号。
pub async fn answer_start(args: &Value) -> Result<Value, (&'static str, String)> {
    start_with(
        args,
        &remote_ask::REACH,
        &LEDGER,
        |req, up_r, mut down_w| async move {
            let stages = crate::dial::StageSink::new(false);
            crate::dial::uses::run(&req, &stages, up_r, &mut down_w).await;
        },
    )
    .await
}

/// 生产的 `forward-stop`。
pub fn answer_stop(args: &Value) -> Result<Value, (&'static str, String)> {
    stop_with(args, &LEDGER)
}

/// 生产的 `forward-list`。
pub fn answer_list() -> Value {
    list_with(&LEDGER)
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_forwards_tests.rs"]
mod tests;
