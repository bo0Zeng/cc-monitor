//! **链路的 monitor 这一侧**：经本机常驻后端那条已有的流开「链路」，
//! 把它包成一条普通的双工字节流（[`LinkStream`]：`AsyncRead + AsyncWrite`）交给调用方。
//!
//! # 它替掉的是什么
//!
//! C2 那一版里，拿一条到远端的字节流 = 起一个 `<本机后端> --dial` 子进程、要它那两根管子（`dial_host.rs`）。
//! 本机只常驻一个后端，SSH 连接由它持有并复用；monitor 这边
//! **不再起任何进程**，而是在它与本机后端之间那条已有的流上发 `link-open`，之后：
//!
//! ```text
//!  调用方 write ──▶ link-data（base64 一块，等应答 = 背压）────────────────────────▶ 后端
//!  调用方 read  ◀── 本链路的队列 ◀── 本机读循环分发 link_data / link_end 帧 ◀──────── 后端
//!                    └─ 读走半个窗口 ⇒ link-credit 还回去（下行流控）
//!  丢掉 / shutdown ──▶ link-close
//! ```
//!
//! 链路上的字节与 C2 拨号代理的 stdout **逐字节同形**（阶段行 → ack → 按用法的字节），
//! 所以读应答的 `ssh_link.rs` 一个字不用改 —— 它只是换了一根管子读。
//!
//! # 它为什么不是通信层成员
//!
//! 它要 `spawn` 任务（还信用 / 关链路 —— 那几件事发生在 `poll_read` / `Drop` 里，不能 await），
//! 而且直接拿 `inbound_client`（那个类型的家今天被 `C1` 咬着，在成员圈外）。
//! 它是**宿主**那一侧的多路复用件：与 `dial_host.rs` 同一类。
//!
//! # 不丢、不堵（后端那半在 `src/backend/dial/link.rs`，这里是另一半）
//!
//! - 本机读循环分发 `link_data` 时**从不阻塞**（进每链路的无界队列）—— 它的界由窗口给：
//!   「发过来、还没还信用」的字节 ≤ 窗口，而信用只在**调用方真读走之后**才还。
//!   对端超窗（不守约）⇒ 当场把那条链路判坏、出声结束它，不涨内存。
//! - 一条没人读的链路不还信用 ⇒ 后端那一侧停在信号量上，**堵不住**本机那条流上别的东西。
//! - 上行一次一块：`poll_write` 等上一块的应答回来才收下一块。

use crate::copy_table::copy_text;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use tokio::sync::mpsc;

use crate::backend_route::{route_call_error, Routed};
use crate::inbound_client::{CallError, InboundClient};

/// 一次链路命令失败 ⇒ 给人看的那句话。**走共用分流器**（`backend_route`）：链路没有第二条路可回落
/// （`D11`），但分流规则只许有一份 —— 同 `frame_query` / `cc_bus` 那几个发送端的理由。
fn said(e: &CallError) -> String {
    match route_call_error(e, |_code, message| {
        copy_text(
            "rsLinkMux.said.refused",
            &[("message", &message.to_string())],
        )
    }) {
        Routed::NoChannel(s) | Routed::Refused(s) => s,
    }
}

/// 上行一块的**步长**（`poll_write` 一次收多少；多出来的留给调用方下一次写，不丢不截）。
/// **与后端 `dial::link::LINK_CHUNK_BYTES` 同一个数** —— 那边它是上限（超了 `invalid_args`），
/// 这边按它切就永远打不到那条上限（`link_mux_tests::the_chunk_cap_is_the_same_number_on_both_sides`
/// 从后端源码现抠着对拍）。
pub(crate) const LINK_STEP: usize = 32 * 1024;

/// 每条链路给后端的下行窗口（信用，字节）：后端「发过来、还没还信用」的字节不许超过它。
/// 超了 = 对端不守约 ⇒ 当场把那条链路判坏（读端拿到一句「协议对不上」的错），不涨内存。读走一半就还一次。
pub(crate) const LINK_WINDOW_BYTES: u64 = 1 << 20;

/// 链路队列里的一件东西。
enum Piece {
    Data(Vec<u8>),
    /// 这条链路不会再有字节了。`Some` = 非正常收尾的那句人话。
    End(Option<String>),
}

/// 表里的一条链路（分发那一侧看到的样子）。
struct Slot {
    tx: mpsc::UnboundedSender<Piece>,
    /// 后端发过来、还没被还回信用的字节数（超窗判据用）：收到一块 `+`，还一次信用 `-`。
    /// 协议保证它 ≤ 窗口 —— 后端在管道上的在途字节不超过还给它的信用。
    outstanding: Arc<AtomicU64>,
    /// 开它的那条流（入方向客户端的身份）。流断了只结束**它自己**开的链路 ——
    /// 本机后端重连之后，新流上开的链路不许被旧流的收尾带走。
    owner: usize,
}

/// 一条入方向客户端的身份（指针值；只比相等，不解引用）。
fn owner_of(client: &Arc<InboundClient>) -> usize {
    Arc::as_ptr(client) as usize
}

fn table() -> &'static Mutex<HashMap<String, Slot>> {
    static T: OnceLock<Mutex<HashMap<String, Slot>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock() -> std::sync::MutexGuard<'static, HashMap<String, Slot>> {
    table().lock().unwrap_or_else(|e| e.into_inner())
}

/// 本进程内唯一的链路 id：`<进程号段>-<单调序号>`。后端只当键与回显用。
fn next_link_id() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    static NONCE: OnceLock<String> = OnceLock::new();
    let nonce = NONCE.get_or_init(|| {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        format!("L{t:x}")
    });
    format!("{nonce}-{}", N.fetch_add(1, Ordering::Relaxed))
}

// ════════════════════════════════════════════════════════════════════════════
//  本机读循环调的三个口（前两个的唯一调用方是 `local_backend::absorb_local_frame`，第三个是两条读循环的收尾）
// ════════════════════════════════════════════════════════════════════════════

/// 一块下行字节到了。不认识的链路（已经被调用方丢掉了）⇒ 静默丢（那是正常的收尾竞态）。
pub(crate) fn deliver_data(link: &str, bytes: Vec<u8>) {
    let mut g = lock();
    let Some(slot) = g.get(link) else {
        tracing::debug!(
            "link_mux: 链路 {link} 已经不在了，丢掉一块 {} 字节",
            bytes.len()
        );
        return;
    };
    let now = slot
        .outstanding
        .fetch_add(bytes.len() as u64, Ordering::SeqCst)
        + bytes.len() as u64;
    if now > LINK_WINDOW_BYTES {
        // 对端不守约（在途字节超过了还给它的信用）。**出声并结束这条链路**，不涨内存。
        if let Some(slot) = g.remove(link) {
            let _ = slot
                .tx
                .send(Piece::End(Some(copy_text("rsLinkMux.data.noCredit", &[]))));
        }
        tracing::warn!("link_mux: 链路 {link} 超窗（{now} > {LINK_WINDOW_BYTES}），已判坏");
        return;
    }
    if slot.tx.send(Piece::Data(bytes)).is_err() {
        g.remove(link);
    }
}

/// 一条链路收尾了（后端已忘掉这个 id）。
pub(crate) fn deliver_end(link: &str, error: Option<String>) {
    if let Some(slot) = lock().remove(link) {
        let _ = slot.tx.send(Piece::End(error));
    }
}

/// 本机那条流断了 ⇒ **经它开的**在飞链路全部带原因结束（不让调用方干等到超时）。
/// `client` = 那条流上的入方向客户端（链路就是经它开的）。
pub(crate) fn fail_owned_by(client: &Arc<InboundClient>, why: &str) {
    let owner = owner_of(client);
    let gone: Vec<Slot> = {
        let mut g = lock();
        let ids: Vec<String> = g
            .iter()
            .filter(|(_, s)| s.owner == owner)
            .map(|(k, _)| k.clone())
            .collect();
        ids.into_iter().filter_map(|k| g.remove(&k)).collect()
    };
    if !gone.is_empty() {
        tracing::warn!("link_mux: {why} —— 经它开的 {} 条链路一起结束", gone.len());
    }
    for slot in gone {
        let _ = slot.tx.send(Piece::End(Some(why.to_string())));
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  LinkStream
// ════════════════════════════════════════════════════════════════════════════

type Flight = tauri::async_runtime::JoinHandle<std::io::Result<()>>;

/// 一条链路：读端是后端送来的下行字节，写端送上行字节。**丢掉它 = 关链路**（`link-close`）。
pub struct LinkStream {
    id: String,
    client: Arc<InboundClient>,
    rx: mpsc::UnboundedReceiver<Piece>,
    outstanding: Arc<AtomicU64>,
    /// 手里正读着的那一块（`pos` 之前已经交出去了）。
    cur: Vec<u8>,
    pos: usize,
    /// 收到 `End` 之后：`None` = 正常 EOF，`Some` = 带原因的错。
    ended: Option<Option<String>>,
    /// 读走了、还没还回去的信用。
    unacked: u64,
    /// 每条入方向命令（`link-data` / `link-credit` / `link-close`）的期限。**值由宿主给**。
    budget: Duration,
    /// 在途的那一块上行（等它的应答）。
    flight: Option<Flight>,
    /// 已经发过 `link-close`（shutdown 或者 drop）。
    closed: bool,
}

impl LinkStream {
    /// 开一条链路：先在表里登记（帧可能比应答先到），再发 `link-open`。
    /// `dial` 是那份蛇形键的拨号请求（`dial_host::request` 造的）。
    pub(crate) async fn open(
        client: Arc<InboundClient>,
        dial: serde_json::Value,
        budget: Duration,
    ) -> Result<LinkStream, String> {
        let id = next_link_id();
        let (tx, rx) = mpsc::unbounded_channel();
        let outstanding = Arc::new(AtomicU64::new(0));
        lock().insert(
            id.clone(),
            Slot {
                tx,
                outstanding: Arc::clone(&outstanding),
                owner: owner_of(&client),
            },
        );
        let args = serde_json::json!({ "link": id, "window": LINK_WINDOW_BYTES, "dial": dial });
        if let Err(e) = client.call("link-open", args, budget).await {
            lock().remove(&id);
            return Err(copy_text(
                "rsLinkMux.open.refused",
                &[("said", &(said(&e)).to_string())],
            ));
        }
        Ok(LinkStream {
            id,
            client,
            rx,
            outstanding,
            cur: Vec::new(),
            pos: 0,
            ended: None,
            unacked: 0,
            budget,
            flight: None,
            closed: false,
        })
    }

    /// 调用方读走了 `n` 字节 ⇒ 攒够半个窗口就还一次信用。
    fn consumed(&mut self, n: usize) {
        self.unacked += n as u64;
        if self.unacked < LINK_WINDOW_BYTES / 2 || self.closed {
            return;
        }
        let bytes = std::mem::take(&mut self.unacked);
        // 先记账再还：后端要收到这次信用之后才会多发，所以这里先减只会更严、不会误判。
        self.outstanding.fetch_sub(bytes, Ordering::SeqCst);
        let client = Arc::clone(&self.client);
        let link = self.id.clone();
        let budget = self.budget;
        tauri::async_runtime::spawn(async move {
            let args = serde_json::json!({ "link": link, "bytes": bytes });
            if let Err(e) = client.call("link-credit", args, budget).await {
                tracing::debug!(
                    "link_mux: 链路 {link} 还信用没成（多半它已经结束了）：{}",
                    said(&e)
                );
            }
        });
    }

    /// 关链路（幂等）：从表里摘掉 ＋ 发一条 `link-close`（不等应答）。
    fn close_now(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        lock().remove(&self.id);
        let client = Arc::clone(&self.client);
        let link = self.id.clone();
        let budget = self.budget;
        tauri::async_runtime::spawn(async move {
            let args = serde_json::json!({ "link": link });
            if let Err(e) = client.call("link-close", args, budget).await {
                tracing::debug!("link_mux: 关链路 {link} 没收到应答：{}", said(&e));
            }
        });
    }
}

impl Drop for LinkStream {
    fn drop(&mut self) {
        self.close_now();
    }
}

impl tokio::io::AsyncRead for LinkStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        use std::task::Poll;
        loop {
            if self.pos < self.cur.len() {
                let n = (self.cur.len() - self.pos).min(buf.remaining());
                let start = self.pos;
                buf.put_slice(&self.cur[start..start + n]);
                self.pos += n;
                self.consumed(n);
                return Poll::Ready(Ok(()));
            }
            if let Some(end) = &self.ended {
                return Poll::Ready(match end {
                    None => Ok(()),
                    Some(why) => Err(std::io::Error::other(why.clone())),
                });
            }
            match self.rx.poll_recv(cx) {
                Poll::Ready(Some(Piece::Data(b))) => {
                    self.cur = b;
                    self.pos = 0;
                }
                Poll::Ready(Some(Piece::End(e))) => self.ended = Some(e),
                // 表里那一格被摘了而没留下 `End`（`shutdown` / `close_now` 之后）⇒ 当 EOF。
                Poll::Ready(None) => self.ended = Some(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl LinkStream {
    /// 等在途的那一块落地（应答回来）。
    fn poll_flight(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        use std::future::Future;
        use std::task::Poll;
        let Some(f) = self.flight.as_mut() else {
            return Poll::Ready(Ok(()));
        };
        let r = std::task::ready!(std::pin::Pin::new(f).poll(cx));
        self.flight = None;
        Poll::Ready(match r {
            Ok(inner) => inner,
            Err(e) => Err(std::io::Error::other(copy_text(
                "rsLinkMux.flight.unfinished",
                &[("e", &e.to_string())],
            ))),
        })
    }
}

impl tokio::io::AsyncWrite for LinkStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        use std::task::Poll;
        if self.closed {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                copy_text("rsLinkMux.write.closed", &[]),
            )));
        }
        std::task::ready!(self.poll_flight(cx))?;
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let n = buf.len().min(LINK_STEP);
        let args = serde_json::json!({ "link": self.id, "data": b64_encode(&buf[..n]) });
        let client = Arc::clone(&self.client);
        let budget = self.budget;
        // ⚠ 交给独立任务，**不**存一个待 poll 的 future：调用方写完这一块可能就去读了，
        //   一个没人 poll 的 future 连那一行都发不出去。
        self.flight = Some(tauri::async_runtime::spawn(async move {
            client
                .call("link-data", args, budget)
                .await
                .map(|_| ())
                .map_err(|e| {
                    std::io::Error::other(copy_text(
                        "rsLinkMux.write.notSent",
                        &[("said", &(said(&e)).to_string())],
                    ))
                })
        }));
        Poll::Ready(Ok(n))
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.poll_flight(cx)
    }

    /// 关写半边 = 关链路（C2 那一版：丢掉子进程的 stdin ⇒ 代理收工）。之后读端给 EOF。
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let flushed = std::task::ready!(self.poll_flight(cx));
        self.close_now();
        std::task::Poll::Ready(flushed)
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  base64（与后端 `wire::b64_encode` / `b64_decode` 同一个口径：RFC 4648 标准字母表、带补位）
// ════════════════════════════════════════════════════════════════════════════

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// 编码（上行块）。**两个 crate 各一份**（后端那份在 `wire.rs`）：monitor 与后端是两棵依赖树，
/// 为 20 行加一条共享依赖不值；两份各拿 RFC 4648 §10 的同一组向量核（异源是 RFC）。
pub(crate) fn b64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(B64[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// 解码（下行块，`stream_source::parse_frame` 解 `link_data` 时调）。严格：坏的就是坏的，不猜。
pub(crate) fn b64_decode(text: &str) -> Result<Vec<u8>, String> {
    let s = text.as_bytes();
    if s.len() % 4 != 0 {
        return Err(copy_text(
            "rsLinkMux.b64.badLength",
            &[("len", &(s.len()).to_string())],
        ));
    }
    let val = |c: u8| -> Option<u32> { B64.iter().position(|&x| x == c).map(|p| p as u32) };
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let quads = s.len() / 4;
    for (qi, q) in s.chunks(4).enumerate() {
        let pad = q.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && qi + 1 != quads) {
            return Err(copy_text("rsLinkMux.b64.badPadding", &[]));
        }
        let mut n: u32 = 0;
        for &c in &q[..4 - pad] {
            let v = val(c).ok_or_else(|| {
                copy_text(
                    "rsLinkMux.b64.badChar",
                    &[("char", &format!("{:?}", char::from(c)))],
                )
            })?;
            n = (n << 6) | v;
        }
        n <<= 6 * pad as u32;
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..3 - pad]);
    }
    Ok(out)
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/link_mux_tests.rs"]
mod tests;
