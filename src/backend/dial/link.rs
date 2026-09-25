//! 〔SR1a · 2026-09-24〕**链路**：monitor 经它与本机常驻后端之间**那条已有的流**开的、到某台远端的一条字节通道。
//!
//! # 它替掉的是什么
//!
//! C2 那一版每条链路是一个 `<后端> --dial` 子进程：请求放环境变量、字节走子进程的 stdin/stdout。
//! 用户裁「改成单一常驻后端」之后，那个子进程的活挪进**本机那一个常驻后端**里：
//! 请求从 `link-open` 的 `dial` 字段来，字节从 `link-data` 进、经 `link_data` 帧出。
//! 链路上的字节**与那个子进程的 stdout 逐字节同形**（阶段行 → 一行 ack → 按用法的字节，`dial/mod.rs` 头注），
//! 所以 monitor 那一侧读应答的 `ssh_link.rs` 一个字不用改。
//!
//! # 一条链路长什么样
//!
//! ```text
//!  link-data ──▶ 上行队列 ──[上行泵]──▶ 上行管子 ──▶ uses::run（拨号 / 池里复用 · 按用法服务）
//!                                                        │
//!  link_data 帧 ◀──[下行泵：扣信用 · 切块 · base64]◀── 下行管子
//! ```
//!
//! 两根管子是 `tokio::io::duplex`（内存里的，有界）。三个任务：拨号与服务 · 下行泵 · 上行泵；
//! `link-close` / 连接表被拆 ⇒ 三个一起 `abort`。
//!
//! # 表归谁
//!
//! **每条流连接一张**（[`Table`]，`inbound::spawn` 起读循环时建）。连接没了（monitor 走了）⇒ 读循环结束 ⇒
//! 表被丢 ⇒ 它开的链路全部 abort ⇒ 池里只被它们用着的 SSH 连接随之收掉（`pool.rs`：`Weak` 表）。
//!
//! # 不丢、流控、顺序（`wire.rs` `LinkData` 头注给了结论，这里是执行）
//!
//! - 链路帧走**应答那条独立通道**，阻塞发送 —— 丢一块下行字节 = 这条链路上的数据坏了。
//! - **下行逐链路信用**：`link-open` 给初始窗口，下行泵每发一块先扣这么多，扣不到就等；
//!   monitor 读走了再 `link-credit` 还回来 ⇒ 一条没人读的链路在管道上最多占一个窗口，堵不住别的。
//! - **上行**：`link-data` 在读循环里**就地**分派（不 `spawn`）⇒ 同一条链路的块按到达顺序进队；
//!   上行泵把一块写进管子**之后**才回它的应答 ⇒ monitor 那一侧同时只有一条在途（背压）。
//!
//! # 零定时器
//!
//! 本文件一个会自己醒的构件都没有：等信用是等信号量，等字节是等管子，收掉是 abort。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, Semaphore};

use crate::wire::{b64_decode, b64_encode, Frame};

/// 一块下行 / 上行字节的上限（解码后）。32 KiB：base64 之后 ≈ 43 KiB 一行，远在两侧单行上限之下
/// （monitor 读后端出方向 64 MiB，后端读入方向 1 MiB —— `inbound::MAX_LINE_BYTES`）。
pub const LINK_CHUNK_BYTES: usize = 32 * 1024;

/// 一条链路手里的信用（= 在途字节）的上限：初始窗口与累计还回来的都不许超过它。
/// **超了 ⇒ 拒收＋回错**（`invalid_args`）：对端不守约，不替它夹 —— 夹掉就是替它猜。
pub const MAX_WINDOW: u64 = 16 << 20;

/// 一条流连接上同时开着的链路数上限（有界资源：每条三个任务、两根管子）。
pub const MAX_LINKS_PER_CONNECTION: usize = 256;

/// 链路 id 的长度上限（不透明串，调用方给；只当键与回显用）。
pub const MAX_LINK_ID_BYTES: usize = 128;

/// 上行队列容量。monitor 那一侧同时只有一条在途，正常用法下这里最多 1 条 —— 满了就是对端不守约。
const UPSTREAM_QUEUE: usize = 4;

/// 两根内存管子各自的缓冲（写满了写方等 —— 背压，不丢不截；它不限任何总量）。
const PIPE_BUFFER: usize = 64 * 1024;

/// 一条在开着的链路。
struct Entry {
    credit: Arc<Semaphore>,
    up: mpsc::Sender<(Vec<u8>, String)>,
    serve: tokio::task::AbortHandle,
    down: tokio::task::AbortHandle,
    upstream: tokio::task::AbortHandle,
}

impl Entry {
    fn abort_all(&self) {
        self.serve.abort();
        self.down.abort();
        self.upstream.abort();
    }
}

type Links = Arc<Mutex<HashMap<String, Entry>>>;

/// 一条流连接的链路表。
pub struct Table {
    links: Links,
    replies: mpsc::Sender<Frame>,
}

impl Drop for Table {
    /// 连接没了 ⇒ 它开的链路一条不留（它们的字节再也送不到任何人手上）。
    fn drop(&mut self) {
        let mut g = lock(&self.links);
        for (_, e) in g.drain() {
            e.abort_all();
        }
    }
}

fn lock(m: &Links) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn ok(id: &str) -> Frame {
    Frame::Reply {
        id: id.to_string(),
        ok: true,
        code: None,
        message: None,
        data: None,
    }
}

fn err(id: &str, code: &str, message: &str) -> Frame {
    Frame::Reply {
        id: id.to_string(),
        ok: false,
        code: Some(code.to_string()),
        message: Some(message.to_string()),
        data: None,
    }
}

/// `args.link`：非空、不超长的串。
fn link_arg(args: &serde_json::Value) -> Result<String, String> {
    let link = args
        .get("link")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "缺 `link`（一个不透明的链路 id 串）".to_string())?;
    if link.is_empty() || link.len() > MAX_LINK_ID_BYTES {
        return Err(format!(
            "`link` 必须是 1..={MAX_LINK_ID_BYTES} 字节的串（实得 {} 字节）",
            link.len()
        ));
    }
    Ok(link.to_string())
}

impl Table {
    pub fn new(replies: mpsc::Sender<Frame>) -> Self {
        Table {
            links: Arc::new(Mutex::new(HashMap::new())),
            replies,
        }
    }

    /// 此刻开着的链路数。
    pub(crate) fn len(&self) -> usize {
        lock(&self.links).len()
    }

    /// `link-open`：登记、起三个任务。返回应答帧（**不等拨通** —— 拨通与否在链路字节里那一行 ack）。
    pub fn open(&self, id: &str, args: &serde_json::Value) -> Frame {
        let link = match link_arg(args) {
            Ok(l) => l,
            Err(m) => return err(id, "invalid_args", &m),
        };
        let Some(dial) = args.get("dial") else {
            return err(id, "invalid_args", "缺 `dial`（一份拨号请求）");
        };
        // 🔴 **原始子系统字节流不交给界面**（〔SR1b〕SFTP 住本机常驻后端 `dial/sftp.rs`：界面只有两条路 ——
        //    部署走 `use:"files"` 的一问一答、传输走 `transfer-*` 命令；把协议字节交出去 = SFTP 协议又回到界面进程，
        //    V89「界面进程零 SSH」当场破）。协议上认得这个词，说得出为什么不做。
        if dial.get("use").and_then(serde_json::Value::as_str) == Some("subsystem") {
            return err(id, "unsupported_use", "这台后端不支持这种连接");
        }
        let req = match super::parse_request_value(dial) {
            Ok(r) => r,
            Err(e) => return err(id, "invalid_args", &format!("拨号请求读不动：{e}")),
        };
        let window = match args.get("window").and_then(serde_json::Value::as_u64) {
            Some(w) if (LINK_CHUNK_BYTES as u64..=MAX_WINDOW).contains(&w) => w,
            _ => {
                return err(
                    id,
                    "invalid_args",
                    &format!(
                        "`window`（初始信用，字节）必须在 [{LINK_CHUNK_BYTES}, {MAX_WINDOW}] 之内"
                    ),
                )
            }
        };

        let reply = self.install(
            id,
            link.clone(),
            window,
            move |up_r, mut down_w| async move {
                let stages = super::StageSink::new(req.stages);
                super::uses::run(&req, &stages, up_r, &mut down_w).await;
            },
        );
        tracing::info!(
            "dial: 开链路 {link}（窗口 {window} 字节；这条连接上此刻 {} 条）",
            self.len()
        );
        reply
    }

    /// 登记一条链路、起三个任务。`serve` 拿上行读端与下行写端，干完就返回（返回 ⇒ 下行写端被丢 ⇒
    /// 下行泵读到 EOF ⇒ `link_end`）。**抽出来是为了判据**：链路的记账（流控 / 顺序 / 收尾）
    /// 不需要真 SSH 就验得动（`dial_link_tests` 拿回声 / 造字节的 `serve` 喂它）。
    fn install<F, Fut>(&self, id: &str, link: String, window: u64, serve: F) -> Frame
    where
        F: FnOnce(tokio::io::DuplexStream, tokio::io::DuplexStream) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let mut g = lock(&self.links);
        if g.contains_key(&link) {
            return err(id, "duplicate_link", "这个链路 id 还开着；换一个");
        }
        if g.len() >= MAX_LINKS_PER_CONNECTION {
            return err(
                id,
                "too_many_links",
                &format!("这条连接上已经开着 {MAX_LINKS_PER_CONNECTION} 条链路"),
            );
        }

        let (down_w, down_r) = tokio::io::duplex(PIPE_BUFFER);
        let (up_w, up_r) = tokio::io::duplex(PIPE_BUFFER);
        let credit = Arc::new(Semaphore::new(window as usize));
        let (up_tx, up_rx) = mpsc::channel::<(Vec<u8>, String)>(UPSTREAM_QUEUE);

        // ① 拨号与服务：返回时丢掉下行写端 ⇒ 下行泵读到 EOF ⇒ `link_end`。
        let serve = tokio::spawn(serve(up_r, down_w)).abort_handle();
        // ② 下行泵。
        let down = tokio::spawn(pump_down(
            link.clone(),
            down_r,
            Arc::clone(&credit),
            self.replies.clone(),
            Arc::clone(&self.links),
        ))
        .abort_handle();
        // ③ 上行泵。
        let upstream = tokio::spawn(pump_up(up_w, up_rx, self.replies.clone())).abort_handle();

        g.insert(
            link,
            Entry {
                credit,
                up: up_tx,
                serve,
                down,
                upstream,
            },
        );
        ok(id)
    }

    /// `link-data`：解码、进那条链路的上行队列。`None` = 应答由上行泵在写进管子之后发。
    pub fn data(&self, id: &str, args: &serde_json::Value) -> Option<Frame> {
        let link = match link_arg(args) {
            Ok(l) => l,
            Err(m) => return Some(err(id, "invalid_args", &m)),
        };
        let Some(text) = args.get("data").and_then(serde_json::Value::as_str) else {
            return Some(err(id, "invalid_args", "缺 `data`（base64 串）"));
        };
        let bytes = match b64_decode(text) {
            Ok(b) => b,
            Err(e) => return Some(err(id, "invalid_args", &e)),
        };
        if bytes.len() > LINK_CHUNK_BYTES {
            return Some(err(
                id,
                "invalid_args",
                &format!("一块 {} 字节，超过 {LINK_CHUNK_BYTES}", bytes.len()),
            ));
        }
        let g = lock(&self.links);
        let Some(e) = g.get(&link) else {
            return Some(err(id, "no_such_link", "没有这条链路（已经结束了？）"));
        };
        match e.up.try_send((bytes, id.to_string())) {
            Ok(()) => None,
            Err(mpsc::error::TrySendError::Full(_)) => Some(err(
                id,
                "link_busy",
                "这条链路上一块还没写完 —— 等它的应答再发下一块",
            )),
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Some(err(id, "link_closed", "这条链路的上行已经收工"))
            }
        }
    }

    /// `link-credit`：monitor 读走了这么多，还回来。
    pub fn credit(&self, id: &str, args: &serde_json::Value) -> Frame {
        let link = match link_arg(args) {
            Ok(l) => l,
            Err(m) => return err(id, "invalid_args", &m),
        };
        let Some(bytes) = args.get("bytes").and_then(serde_json::Value::as_u64) else {
            return err(id, "invalid_args", "缺 `bytes`（非负整数）");
        };
        let g = lock(&self.links);
        let Some(e) = g.get(&link) else {
            return err(id, "no_such_link", "没有这条链路（已经结束了？）");
        };
        // 累计信用不许超过上限：对端还的比它读走的多 = 不守约 ⇒ 拒收＋回错（不替它夹）。
        let have = e.credit.available_permits() as u64;
        if have.saturating_add(bytes) > MAX_WINDOW {
            return err(
                id,
                "invalid_args",
                &format!("还了 {bytes} 字节信用，累计会超过上限 {MAX_WINDOW}（手里已有 {have}）"),
            );
        }
        e.credit.add_permits(bytes as usize);
        ok(id)
    }

    /// `link-close`：界面走了。三个任务一起收；关一条不存在的链路是幂等的（同 `cancel`）。
    pub fn close(&self, id: &str, args: &serde_json::Value) -> Frame {
        let link = match link_arg(args) {
            Ok(l) => l,
            Err(m) => return err(id, "invalid_args", &m),
        };
        if let Some(e) = lock(&self.links).remove(&link) {
            e.abort_all();
        }
        ok(id)
    }
}

/// 下行泵：先等到信用 → 按手里的信用读（至多一块）→ 扣掉读到的那么多 → base64 → `link_data`；
/// EOF ⇒ `link_end`、从表里摘掉。
///
/// ★ **先等信用、再按信用读**，不是「先读一块、再等够一块的信用」：后者在窗口不是块长整数倍、
/// 或者管子里一次只给出零碎几段时，会停在「差几字节凑不够一块」上 ⇒ 发出去的字节**小于**窗口，
/// 而「不还信用时发出去的字节 == 窗口」正是这一层的判据（`dial_link_tests`）。
/// 信号量只有本任务在扣（`link-credit` 只加）⇒ 算出来的「此刻有多少」只会变多，后面那一次 `try_acquire_many` 必成。
async fn pump_down(
    link: String,
    mut from: tokio::io::DuplexStream,
    credit: Arc<Semaphore>,
    replies: mpsc::Sender<Frame>,
    links: Links,
) {
    let mut buf = vec![0u8; LINK_CHUNK_BYTES];
    let error = loop {
        // ① 至少一字节的信用。
        match credit.acquire().await {
            Ok(p) => p.forget(),
            Err(_) => break Some("链路的信用闸被关了".to_string()),
        }
        // ② 按手里的信用读，至多一块。
        let allow = (1 + credit.available_permits()).min(LINK_CHUNK_BYTES);
        let n = match from.read(&mut buf[..allow]).await {
            Ok(0) => break None,
            Ok(n) => n,
            Err(e) => break Some(format!("读链路下行失败：{e}")),
        };
        // ③ 扣掉读到的那么多（第一字节在 ① 里已经扣了）。
        if n > 1 {
            match credit.try_acquire_many((n - 1) as u32) {
                Ok(p) => p.forget(),
                Err(_) => break Some("链路的信用记账对不上（不该发生）".to_string()),
            }
        }
        let frame = Frame::LinkData {
            link: link.clone(),
            data: b64_encode(&buf[..n]),
        };
        if replies.send(frame).await.is_err() {
            // 连接没了：没人收，也就不必报 `link_end`。
            return;
        }
    };
    // 先报再摘：摘的时候会 abort 另外两个任务（不 abort 自己 —— 自己正要结束）。
    let _ = replies
        .send(Frame::LinkEnd {
            link: link.clone(),
            error,
        })
        .await;
    if let Some(e) = lock(&links).remove(&link) {
        e.serve.abort();
        e.upstream.abort();
    }
}

/// 上行泵：一块一块写进上行管子，**写进去之后**才回那一块的应答。
async fn pump_up(
    mut to: tokio::io::DuplexStream,
    mut rx: mpsc::Receiver<(Vec<u8>, String)>,
    replies: mpsc::Sender<Frame>,
) {
    while let Some((bytes, id)) = rx.recv().await {
        let frame = match to.write_all(&bytes).await {
            Ok(()) => ok(&id),
            Err(e) => err(&id, "link_closed", &format!("这条链路的上行已经收工：{e}")),
        };
        if replies.send(frame).await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_link_tests.rs"]
mod tests;
