//! 端口转发（`-L`）的账住本机常驻后端：起 · 停 · 列三条帧命令，界面经
//! `chan.call(<local>, "forward-*", …)` 直接问。
//!
//! # 形状
//!
//! ```text
//!  界面 ── forward-start {origin, localPort, remoteHost, remotePort} ──▶ 本机后端
//!           ① 规格先过围栏（任何查表 / 拨号之前）
//!           ② 查可达表（`remote_ask::REACH`：monitor 在每台远端流握手那一刻登记的「怎么够到那台」）；
//! 表里没有（那台的流没起来）⇒ 按界面一并交来的那台配置自己组请求（`ssh-dial/machine.rs`），不拒
//!           ③ 绑本机回环口 `127.0.0.1:localPort`（本后端绑，拨号核心不绑端口）：绑不上 ⇒ 当场回那一句、不拨
//!           ④ 把那份拨号请求改成 `use: forward`，连同绑好的口 ⇒ `ssh_dial::uses::run_forward`（池里那条 SSH 连接）
//!           ⑤ 等 ack：连上了才算起来；不成 ⇒ 原话回、不进账（口随链路那一侧一起放掉）
//!           ⑥ 进账：一个任务读「接进了第 n 条连接」那几行，计数照抄
//!  界面 ── forward-stop {id} ──▶ 从账上摘掉 ⇒ 那个任务被收 ⇒ 链路被丢 ⇒ 放掉本地口、收掉在飞隧道
//!  界面 ── forward-list ──▶ {forwards:[{id, origin, localPort, remoteHost, remotePort, state, connCount}]}
//! ```
//!
//! **账是本进程一张**（不是每条流连接一张）：常驻后端活得比界面长，界面重开之后列得出、停得掉上次开的转发。
//! v1 仍是即席（不落盘）：后端重启 ⇒ 账空、口全放。
//!
//! 零定时器：没有「过一会儿再看看」—— 状态是读出来的（那个任务收工了没有），不是轮询出来的。

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cap_core::{Cap, Over};
use comms_inward::line::{read_capped_line, CappedLine};
use copy_core::copy_text;
use serde_json::{json, Value};
use tokio::io::{BufReader, DuplexStream};

use crate::common::said::Said;
use crate::dial::remote_ask::{self, AbortOnDrop};
use crate::dial::DialRequest;

/// 账上最多几条（有界资源；每条占一个本机口，一个人开不出这么多）。
pub(crate) const MAX_FORWARDS: usize = 64;

/// ack 那一行的上限。超了：拒收＋回错（这条转发起不来，原因带上限）。
const ACK_CAP: Cap = Cap {
    bytes: 64 * 1024,
    over: Over::Reject,
};

/// 计数那一行（`{"accepted":n}`）的上限。超了：降级＋说清（这条转发收工，列表里读成 `error`）。
const COUNT_CAP: Cap = Cap {
    bytes: 4 * 1024,
    over: Over::DegradeAndSay,
};

/// 读链路那一侧的一行（带上限；超了是错，不截断）：`Ok(None)` ＝ 管子关了。
async fn line<R: tokio::io::AsyncBufRead + Unpin>(
    r: &mut R,
    cap: Cap,
) -> Result<Option<String>, String> {
    let mut buf = Vec::new();
    match read_capped_line(r, &mut buf, cap).await {
        Ok(CappedLine::Eof) => Ok(None),
        Ok(CappedLine::Line) => Ok(Some(String::from_utf8_lossy(&buf).trim_end().to_string())),
        Ok(CappedLine::TooLong(_)) => Err(copy_text(
            "beRemoteAsk.cappedLine.tooLong",
            &[("cap", &cap.bytes.to_string())],
        )),
        // 这一形的失败只有一句话的位：系统原话记一行日志。
        Err(e) => Err(crate::common::said::IntoNote::into_note(Said::with_raw(
            copy_text("beRemoteAsk.cappedLine.readFailed", &[]),
            e,
        ))),
    }
}

/// 一条转发的规格（界面交来的那四格）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Spec {
    pub(crate) origin: String,
    pub(crate) local_port: u16,
    pub(crate) remote_host: String,
    pub(crate) remote_port: u16,
}

/// `forward-start` 的入参：规格四格 ＋ 那台的名字（怎么拨照这一端的机器表组，线上不交那台的配置）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ForwardStartReq {
    /// 那台的名字。
    pub(crate) origin: cmd_wire::Machine,
    /// 绑本机 `127.0.0.1:localPort`：用户敲进去的原文，这里解（界面不判是不是端口）。
    pub(crate) local_port: String,
    /// 每接进一条连接开一条到那台 `remoteHost:remotePort` 的 direct-tcpip。
    pub(crate) remote_host: String,
    /// 同 `localPort`：原文。
    pub(crate) remote_port: String,
}

/// 一格端口原文 ⇒ 1–65535（两头空白不算）；空 · 不是数 · 0 · 越界 ⇒ `None`。
fn port_of(text: &str) -> Option<u16> {
    text.trim().parse::<u16>().ok().filter(|p| *p != 0)
}

/// 读规格并过围栏 —— **在任何查表 / 拨号之前**（`INVARIANTS §47`：外部值交给对端之前本侧先判）。
/// 没选那台 · 端口不是 1–65535 · 远端 host 空白 ⇒ `bad_spec` 带那一句（界面照抄）；缺格 / 类型不对在解入参那一步就回了（`bad_args`）。
pub(crate) fn spec_of(req: &ForwardStartReq) -> Result<Spec, Said> {
    let bad = |said: String| Said::new("bad_spec", said);
    if req.origin.to_string().trim().is_empty() {
        return Err(bad(copy_text("beForwards.spec.origin", &[])));
    }
    let local_port =
        port_of(&req.local_port).ok_or_else(|| bad(copy_text("beForwards.spec.localPort", &[])))?;
    if req.remote_host.trim().is_empty() {
        return Err(bad(copy_text("beForwards.spec.remoteHost", &[])));
    }
    let remote_port = port_of(&req.remote_port)
        .ok_or_else(|| bad(copy_text("beForwards.spec.remotePort", &[])))?;
    Ok(Spec {
        origin: req.origin.to_string(),
        local_port,
        remote_host: req.remote_host.trim().to_string(),
        remote_port,
    })
}

/// 把照机器表组好的那份拨号请求改成「隧道到那个远端口」（纯；抽出来是为了判据）。本机口不进请求：本后端绑好另交（[`start_with`]）。
pub(crate) fn forward_request(dial: &Value, spec: &Spec) -> Result<DialRequest, String> {
    let mut v = dial.clone();
    let obj = v
        .as_object_mut()
        .ok_or_else(|| crate::common::contract::malformed("dial request is not an object"))?;
    obj.insert("use".into(), json!("forward"));
    obj.insert(
        "forward".into(),
        json!({
            "remote_host": spec.remote_host,
            "remote_port": spec.remote_port,
        }),
    );
    obj.insert("stages".into(), json!(false));
    obj.insert("probe".into(), json!(false));
    crate::dial::read_request(&v).map_err(|s| s.said)
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

/// `forward-start` 的本体：账与那几台由调用方给，`serve` 是链路那一侧（生产 = `ssh_dial::uses::run_forward`；判据用替身）。
/// `serve` 拿到的是：改好的拨号请求 · 绑好的本机口（绑不上就不叫它）· 上行读端（本侧一个字节都不写，丢了 = 界面走了）· 下行写端（ack ＋ 计数行）。
pub(crate) async fn start_with<F, Fut>(
    req: &ForwardStartReq,
    reach: &remote_ask::Table,
    ledger: &Ledger,
    serve: F,
) -> Result<ForwardStarted, Said>
where
    F: FnOnce(DialRequest, tokio::net::TcpListener, DuplexStream, DuplexStream) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let spec = spec_of(req)?;
    // 照这一端的机器表按名字组；流起没起来都一样拨（与起流同一个池）。
    //   表里没有那台 ⇒ `unreachable`；跳板成环 · 找不到 ⇒ `bad_jump`（组法在 `ssh-dial/machine.rs`）。
    let dial = reach
        .dial(&spec.origin)
        .map_err(|s| match s.code.as_deref() {
            Some("bad_jump") => Said::new("bad_jump", s.said),
            Some("unreachable") => Said::new("unreachable", s.said),
            _ => Said::new("bad_args", s.said),
        })?;
    if ledger.rows().len() >= MAX_FORWARDS {
        return Err(Said::new(
            "full",
            copy_text(
                "beForwards.start.full",
                &[("max", &MAX_FORWARDS.to_string())],
            ),
        ));
    }
    let req = forward_request(&dial, &spec).map_err(|e| Said::new("failed", e))?;
    let failed = |said: String| {
        Said::new(
            "failed",
            copy_text(
                "beForwards.start.failed",
                &[("machine", &spec.origin), ("why", &said)],
            ),
        )
    };
    // 先绑口、再拨：口被占 ⇒ 当场回那一句，不白拨一趟。只绑回环（不对外暴露）。
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", spec.local_port))
        .await
        .map_err(|e| {
            failed(crate::common::said::IntoNote::into_note(Said::with_raw(
                copy_text(
                    "beForwards.start.bindFailed",
                    &[
                        ("port", &spec.local_port.to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                e,
            )))
        })?;
    let (up_w, up_r) = tokio::io::duplex(64);
    let (down_w, down_r) = tokio::io::duplex(16 * 1024);
    // 本函数（的 future）被丢 ⇒ 链路那一侧一起收（帧期限到点 / `cancel`）。
    let link = AbortOnDrop(tokio::spawn(serve(req, listener, up_r, down_w)));
    let mut rd = BufReader::new(down_r);
    let ack = line(&mut rd, ACK_CAP)
        .await
        .map_err(failed)?
        .ok_or_else(|| failed(copy_text("beForwards.start.unconfirmed", &[])))?;
    // 照拨号核心写的那一形读（`DialAck`），不手写键名。
    let ack: ssh_dial::DialAck = serde_json::from_str(&ack).map_err(|e| failed(e.to_string()))?;
    if !ack.ok {
        let why = ack
            .said
            .unwrap_or_else(|| copy_text("beForwards.start.unconfirmed", &[]));
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
            match line(&mut rd, COUNT_CAP).await {
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
    Ok(ForwardStarted { id: id_text(n) })
}

/// `forward-start` 的应答：这条转发的号。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ForwardStarted {
    /// `fwd-<n>`，本进程内单调。
    pub(crate) id: String,
}

/// `forward-stop` 的入参 · 应答（应答回显那个号）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct ForwardId {
    /// 转发号（`fwd-<n>`）。
    pub(crate) id: String,
}

/// `forward-stop` 的本体：从账上摘掉（丢掉那一条 ⇒ 收掉链路 ⇒ 后端放掉本地口、收掉在飞隧道）。
pub(crate) fn stop_with(
    req: ForwardId,
    ledger: &Ledger,
) -> Result<ForwardId, crate::common::said::Said> {
    let entry = id_num(&req.id)
        .and_then(|n| ledger.rows().remove(&n))
        .ok_or_else(|| {
            Said::new(
                "not_found",
                copy_text("beForwards.stop.notFound", &[("id", &req.id)]),
            )
        })?;
    drop(entry);
    Ok(req)
}

/// 账上一条转发此刻的样子。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ForwardRow {
    pub(crate) id: String,
    /// 那台的名字。
    pub(crate) origin: cmd_wire::Machine,
    pub(crate) local_port: u16,
    pub(crate) remote_host: String,
    pub(crate) remote_port: u16,
    /// `running`（链路还在）· `error`（链路自己收工了，留在账上等人停）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "\"running\" | \"error\""))]
    pub(crate) state: &'static str,
    /// 累计接进的连接数。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub(crate) conn_count: u64,
}

/// `forward-list` 的应答。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Forwards {
    pub(crate) forwards: Vec<ForwardRow>,
}

/// `forward-list` 的本体：账上每一条的规格 · 状态（`running` = 链路还在 · `error` = 链路收工了）· 累计连接数。
pub(crate) fn list_with(ledger: &Ledger) -> Forwards {
    let forwards = ledger
        .rows()
        .iter()
        .map(|(n, e)| ForwardRow {
            id: id_text(*n),
            origin: cmd_wire::Machine::named(e.spec.origin.clone()),
            local_port: e.spec.local_port,
            remote_host: e.spec.remote_host.clone(),
            remote_port: e.spec.remote_port,
            state: if e.pump.0.is_finished() {
                "error"
            } else {
                "running"
            },
            conn_count: e.conn_count.load(Ordering::Relaxed),
        })
        .collect();
    Forwards { forwards }
}

/// 账上通往 `origin` 那台、链路还在的几条（`machine-interrupts` 的 `forwards` 一格）。
pub(crate) fn running_to(ledger: &Ledger, origin: &str) -> u32 {
    ledger
        .rows()
        .iter()
        .filter(|(_, e)| e.spec.origin == origin && !e.pump.0.is_finished())
        .count() as u32
}

/// 生产的那一份：进程里那张账。
pub fn running_to_machine(origin: &str) -> u32 {
    running_to(&LEDGER, origin)
}

/// 账上链路还在的全部几条（「现在重启 cc-monitor」而这台后端随它一起停时，这些都断）。
pub fn running_all() -> u32 {
    LEDGER
        .rows()
        .iter()
        .filter(|(_, e)| !e.pump.0.is_finished())
        .count() as u32
}

crate::wire::typed::codes! {
    /// `forward-start` 的码：规格不对 · 那台不在可达表里也没交配置 · 跳板查无 / 成环 · 账满了 · 链路没起来。
    pub enum ForwardStartErr {
        BadSpec = "bad_spec",
        Unreachable = "unreachable",
        BadJump = "bad_jump",
        Full = "full",
        Failed = "failed",
    }
}

/// `forward-start`：起一条本地端口转发。真异步（查可达表 · 池里那条 SSH 上开 `use: forward` 链路 · 等 ack），`cancel` 能在 await 点打断。
pub struct ForwardStart;

impl crate::wire::typed::Command for ForwardStart {
    const NAME: &'static str = "forward-start";
    type Req = ForwardStartReq;
    type Reply = ForwardStarted;
    type Err = ForwardStartErr;
}

impl crate::wire::typed::Handle for ForwardStart {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: ForwardStartReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<ForwardStarted, ForwardStartErr> {
        start_with(
            &req,
            &remote_ask::REACH,
            &LEDGER,
            |req, listener, up_r, mut down_w| async move {
                let stages = crate::dial::StageSink::new(false);
                crate::dial::host();
                ssh_dial::uses::run_forward(&req, listener, &stages, up_r, &mut down_w).await;
            },
        )
        .await
        .map_err(crate::wire::typed::of_fail_with(ForwardStartErr::Failed))
    }
}

crate::wire::typed::codes! {
    /// `forward-stop` 的码：账上没有这个号。
    pub enum ForwardStopErr {
        NotFound = "not_found",
    }
}

/// `forward-stop`：停一条转发（纯内存，一把锁）。
pub struct ForwardStop;

impl crate::wire::typed::Command for ForwardStop {
    const NAME: &'static str = "forward-stop";
    type Req = ForwardId;
    type Reply = ForwardId;
    type Err = ForwardStopErr;
}

impl crate::wire::typed::Handle for ForwardStop {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: ForwardId,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl Future<Output = Result<ForwardId, ForwardStopErr>> + Send + 'static {
        std::future::ready(
            stop_with(req, &LEDGER)
                .map_err(crate::wire::typed::of_fail_with(ForwardStopErr::NotFound)),
        )
    }
}

crate::wire::typed::codes! {
    /// `forward-list` 没有别的码。
    pub enum ForwardListErr {}
}

/// `forward-list`：列转发（纯内存，一把锁）。
pub struct ForwardList;

impl crate::wire::typed::Command for ForwardList {
    const NAME: &'static str = "forward-list";
    const TAKES_INPUT: bool = false;
    type Req = crate::wire::typed::NoArgs;
    type Reply = Forwards;
    type Err = ForwardListErr;
}

impl crate::wire::typed::Handle for ForwardList {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: crate::wire::typed::NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl Future<Output = Result<Forwards, ForwardListErr>> + Send + 'static {
        std::future::ready(Ok(list_with(&LEDGER)))
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_forwards_tests.rs"]
mod tests;
