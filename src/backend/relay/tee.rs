//! tee：把响应体里的 SSE 事件抄一份出去。**只抄响应体，永不抄请求头。**
//!
//! # 为什么落 stdout 而不是文件
//!
//! 见 `super` 的头注㈠：只读护栏的默认层把标准库里「新建文件」的每一种写法都禁掉了，
//! 而绕开它的三条路都是「让护栏变瞎」。⇒ 本轮 tee 写 stdout，要落文件由启动方重定向。
//!
//! # 行格式（契约是**这份文件**，不是代码）
//!
//! 每个响应先一行 meta，其后每个 SSE 事件一行：
//!
//! ```text
//! {"__meta__":{"source":"relay","proto":"passthrough-v0","agent":"…","account":"…","key":"…","seq":N}}
//! {"agent":"…","account":"…","key":"…","event":"<上游 data: 后面那段，转义成一个 JSON 串>"}
//! ```
//!
//! ⚠ `account` 那一格是 `K-H2` 加的（路由键从两段变三段）。**刻意不并进 `key`**：
//! 并了就是「一个值装了两件事」。`proto` 那个串**没有跟着 bump** ——
//! 今天这条流**零消费者**（现打 08-28：`passthrough-v0` / `__meta__` 全仓只命中
//! `src/doc/IPC-PROTOCOL.md` + 本文件 + `server.rs`，都是它自己和它的文档）
//! ⇒ 没有任何东西会因为多一格而读错。**第一个真消费者出现时，bump 那个串就成了硬要求。**
//!
//! **没有 `t_ns`** —— 理由与它丢掉了什么，见 `super` 的头注㈡。
//!
//! ## ⚠ 订正〔回修轮之四 08-25，D1 `重要-7` / D2 `阻-1(D2)`〕：`event` 是**串**，不是裸值
//!
//! 先前这里写的是 `"event":<上游 data: 后面那段，**原样**>` —— 那句话把「原样」落在了
//! **JSON 结构**这一层，而上游的字节是**敌手可控**的：
//! - 上游发 `data: 1,"agent":"evil"` ⇒ 整行成了
//!   `{"agent":"realA","key":"sid-AAA","event":1,"agent":"evil"}`，
//!   而重复键在 `serde_json` / `json.loads` 两侧都是 **last-wins** ⇒ **路由键被上游改写**。
//! - 上游发一段不是 JSON 的文本（SSE 的 `data:` 本来就允许任意文本）⇒ 整行**不可解析**，
//!   而件计划 `DoD-3㈠` 的 acceptor 逐字要「其后**每行可解析**」。
//!
//! ⇒ 今天 `event` 的值是**一个 JSON 串**：上游那一段逐字节保住（`K9` 裁定二「内容一律原样
//! 透传」在**内容**这一层照旧成立，中转仍然不解析它的任何字段），但它**不再参与本行的结构**。
//! 下游要拿里面的字段，自己对这个串再解一次。
//! 死值验与射程见 `an_upstream_payload_cannot_break_out_of_the_event_field` 与件文件 §8.18.1。
//!
//! ## ⚠ 订正〔回修轮之五 08-25，D3 `阻-2(D3)`〕：**「坏了」不是「阻塞」**，那句承诺只覆盖了一半
//!
//! `write_line` 先前逐字承诺「**tee 坏了不许拖垮转发**」，而它下面兑现的是
//! `let _ = w.write_all(...)` —— 那只兑现了「**返错**不拖垮」。
//! **管道满、消费者慢、终端 flow-control 停住时，`write` 是阻塞不是返错**，
//! 而那正是「tee 落 stdout、启动方把 stdout 接进一条管道」这个**本设计推荐的用法**下
//! 最常见的一半。⇒ 今天写落在一条专属线程上，两半都覆盖到了；形状与诚实边界见 `TeeSink` 头注。
//!
//! ## 还有一种行：`__dropped__`（本轮新增，写进契约）
//!
//! ```text
//! {"__dropped__":{"lines":N,"bytes":M}}
//! ```
//!
//! `lines` = 队列满而没写出去的**行数**；`bytes` = 因超解码上限（`TEE_DECODE_CAP`）
//! 丢掉的**字节数** + 那些行自身的字节数。**丢必须说**：`DoD-3㈠` 要的「`event` 数 ==
//! 上游事件数」只有在「丢是可见的」时候才对得上账。

use std::io::Write;

/// SSE 拆行器 —— 增量喂字节，吐出 `data:` 行的载荷。
///
/// 它只认 SSE 的**分帧**（行、`data:` 前缀），不认里面是什么。
#[derive(Default)]
pub(crate) struct SseSplitter {
    partial: Vec<u8>,
    /// 超 `cap` 时丢掉的字节数（累计，由 `take_dropped` 取走）。
    dropped: u64,
}

impl SseSplitter {
    /// # `cap` 管的是**半行**〔回修轮之五 08-25，`阻-1(D3)` 的同职面〕
    ///
    /// `partial` 只在**还没遇到 `\n`** 的时候留着字节。上游发一条永不换行的 `data:` 行，
    /// 它就一直涨 —— 与 `ChunkedView::feed` 那条同族（**按真实字节增长**，不是「拿一个数去分配」）。
    ///
    /// 超了怎么办：**丢掉这条半行并计数**，其后的字节从下一个 `\n` 重新开始拆
    /// —— tee 少一行，**下游的字节一个不少**（tee 是抄一份，不在转发那条路上）。
    /// 计数由 `server.rs::handle` 取走并写进 tee 流的 `__dropped__` 行 ⇒ **不是静默丢**。
    pub(crate) fn feed(&mut self, decoded: &[u8], cap: usize) -> Vec<String> {
        self.partial.extend_from_slice(decoded);
        // 只量**第一行**：它才是那条「攒着还没成形」的。其后的完整行照常拆，不许被连坐。
        let head_len = self
            .partial
            .iter()
            .position(|b| *b == b'\n')
            .map_or(self.partial.len(), |p| p + 1);
        if head_len > cap {
            self.dropped += head_len as u64;
            self.partial.drain(..head_len);
        }
        let mut out = Vec::new();
        loop {
            let Some(pos) = self.partial.iter().position(|b| *b == b'\n') else {
                break;
            };
            let line: Vec<u8> = self.partial.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\r', '\n']);
            let Some(payload) = line.strip_prefix("data:") else {
                continue;
            };
            let payload = payload.trim();
            if payload.is_empty() || payload == "[DONE]" {
                continue;
            }
            out.push(payload.to_string());
        }
        out
    }

    /// 取走并清零「因超 `cap` 丢掉的字节数」。
    pub(crate) fn take_dropped(&mut self) -> u64 {
        std::mem::take(&mut self.dropped)
    }
}

/// tee 的落点。一个进程只有一个，**跨连接共享**。
///
/// # ★★ 为什么写落在**另一条线程**上〔回修轮之五 08-25，D3 `阻-2(D3)`〕
///
/// 先前 `write_line` 是「`inner.lock()` **之后**做 `write_all` + `flush`」——
/// **阻塞的写在持锁状态下发生**，而这把锁是**跨连接共享**的（`TeeSink` 是 `Relay` 的字段，
/// `Relay` 走 `Arc`，`serve()` 每连接 `Arc::clone`），且 `event()` 是在 `pump` 的 `on_chunk` 里
/// **同步**调的 ⇒ 一个慢/卡住的 tee 消费者会把**每一条**连接的透传一起拖停。
/// D3 实测读数（住址 `audits/K-H1-D3.md#2.2`，我自己重打过，见件文件 §8.20.3）：
/// `tee 不卡时 B 耗时 1 ms · tee 卡 3000ms 时 B 耗时 2701 ms`。
///
/// ⚠ 而当时那句头注写的是「**tee 坏了不许拖垮转发**」，它下面兑现的却是「**返错**不拖垮」
/// （`let _ = w.write_all(...)`）。**「阻塞」不是「坏了」** —— 管道满、消费者慢、终端 flow-control
/// 停住，`write` 是**阻塞**不是返错。那句承诺只覆盖了一半失败面，而另一半正是
/// stdout 当落点、启动方把它接进一条管道时**最常见**的一半。
///
/// # 今天的形状
///
/// `open()` / `event()` 只把**成行的字符串**投进一条**有界**队列（`try_send`，永不阻塞），
/// 真正的 `write_all` + `flush` 由一条**专属写线程**做。⇒ 转发那条路上再没有任何阻塞的写。
///
/// - **一行不被另一行劈开**：写者只有那一条线程，整行整行地写 —— 比先前的锁更强，不是更弱。
/// - **顺序**：同一条连接的 `open`/`event` 在同一条线程上 `try_send`，通道保序 ⇒ 顺序不变。
/// - **队列满了怎么办**：投递方**丢这一行并计入账**（`write_line`）；账由**写线程**
///   在写完下一行之后报成一行 `{"__dropped__":{"lines":N,"bytes":M}}`（见 `TeeSink::new`）
///   ⇒ **不许静默丢**（`DoD-3㈠` 的「`event` 数 == 上游事件数」要靠这条才对得上账）。
///   ⚠ **报账的必须是写线程，不能是投递方** —— 投递方正是因为「投不进去」才在丢，
///   那行 `__dropped__` 它同样投不进去（第一版就是那么写的，实测那行永远补不出来）。
/// - **诚实边界**：进程被杀时队列里还没写出去的行会丢。先前的同步写没有这一格
///   —— 这是拿「一条卡住的消费者不再拖垮全部会话」换来的，写在这里，不藏。
pub(crate) struct TeeSink {
    tx: std::sync::mpsc::SyncSender<String>,
    seq: std::sync::atomic::AtomicU64,
    /// 队列满而被丢掉的**行数** / **字节数**（后者含解码缓冲那一路，见 `note_dropped_bytes`）。
    ///
    /// ⚠ 报账的是**写线程**，不是投递方。投递方那一侧是「队列满了」才丢的，
    /// 它当场**也投不进**那行 `__dropped__` —— 第一版就是那么写的，实测那行永远补不出来
    /// （逐字：`队列溢出必须在流里留下 __dropped__` 当场红，流里只有事件行）。
    /// ⇒ 谁有地方写谁报：写线程每写完一行就把账**取空**报一次。
    dropped_lines: std::sync::Arc<std::sync::atomic::AtomicU64>,
    dropped_bytes: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

/// tee 队列能排多少**行**。
///
/// ⚠ **是条数不是体量** —— 它已在 `src/bridge/src/byte_cap_registry.rs` 的 `NOT_A_SIZE_CAP` 里
/// 登记为「不是字节上限」（那张表默认拒绝：尺寸类常量不登记就红）。
const TEE_QUEUE_LINES: usize = 1024;

impl TeeSink {
    pub(crate) fn new(w: Box<dyn Write + Send>) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
        let (tx, rx) = std::sync::mpsc::sync_channel::<String>(TEE_QUEUE_LINES);
        let dropped_lines = std::sync::Arc::new(AtomicU64::new(0));
        let dropped_bytes = std::sync::Arc::new(AtomicU64::new(0));
        let (dl, db) = (
            std::sync::Arc::clone(&dropped_lines),
            std::sync::Arc::clone(&dropped_bytes),
        );
        // 写线程。`rx.recv()` 是**阻塞在通道上**，不是定时器：没有行就一直等，
        // 全部发送端 drop 之后 `recv` 回 `Err` ⇒ 把队列排干、然后自己退出。
        let _ = std::thread::Builder::new()
            .name("ccm-relay-tee".to_string())
            .spawn(move || {
                let mut w = w;
                while let Ok(line) = rx.recv() {
                    let _ = w.write_all(line.as_bytes());
                    // ★ 报账**紧跟在一行写完之后**：这时候我们手上有落点、写得动。
                    //   投递方那一侧报不了 —— 它正是因为「投不进去」才在丢。
                    let (l, b) = (dl.swap(0, SeqCst), db.swap(0, SeqCst));
                    if l > 0 || b > 0 {
                        let note = format!("{{\"__dropped__\":{{\"lines\":{l},\"bytes\":{b}}}}}\n");
                        let _ = w.write_all(note.as_bytes());
                    }
                    let _ = w.flush();
                }
            });
        Self {
            tx,
            seq: AtomicU64::new(0),
            dropped_lines,
            dropped_bytes,
        }
    }

    pub(crate) fn to_stdout() -> Self {
        Self::new(Box::new(std::io::stdout()))
    }

    /// 一个响应开头写一行 meta，返回这一响应的序号。
    ///
    /// ⚠ `K-H2` 加了 `account` 这一格 —— 路由键有三段，tee 行就该有三格。
    /// **不合并进 `key`**：那正是「一个值装了两件事」，本工作区最贵的那族病。
    pub(crate) fn open(&self, agent: &str, account: &str, key: &str) -> u64 {
        let seq = self.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let line = format!(
            "{{\"__meta__\":{{\"source\":\"relay\",\"proto\":\"passthrough-v0\",\"agent\":{},\"account\":{},\"key\":{},\"seq\":{}}}}}\n",
            json_str(agent),
            json_str(account),
            json_str(key),
            seq
        );
        self.write_line(&line);
        seq
    }

    /// 写一行事件。
    ///
    /// ★★ **`payload` 必须过 `json_str`**（回修轮之四 08-25，D1 `重要-7` / D2 `阻-1(D2)`）：
    /// 它是**上游**给的字节，敌手可控。先前这里把它**原样**拼进 JSON，实测两条后果 ——
    /// 上游发 `data: 1,"agent":"evil"` ⇒ 整行变成
    /// `{"agent":"realA","key":"sid-AAA","event":1,"agent":"evil"}`，而 `serde_json` /
    /// `json.loads` 两侧解重复键都是 **last-wins** ⇒ **这一行的路由键被上游改写**；
    /// 上游发一段不是 JSON 的文本 ⇒ 整行**不可解析**，而 `DoD-3㈠` 的 acceptor
    /// 逐字要「其后每行可解析」。判据见 `an_upstream_payload_cannot_break_out_of_the_event_field`。
    pub(crate) fn event(&self, agent: &str, account: &str, key: &str, payload: &str) {
        let line = format!(
            "{{\"agent\":{},\"account\":{},\"key\":{},\"event\":{}}}\n",
            json_str(agent),
            json_str(account),
            json_str(key),
            json_str(payload)
        );
        self.write_line(&line);
    }

    /// 解码那一路（`SseSplitter` / `ChunkedView` 超上限）丢掉的字节，记在同一本账上。
    /// 由 `server.rs::handle` 每块调一次（`n == 0` 是常态，直接返回）。
    ///
    /// ★★ **这一支当场就报，不等写线程**〔本轮实测逼出来的〕：写线程那条报账路是
    /// 「写完**下一行**之后顺带报」，而解码丢掉的那一形**不保证还有下一行** ——
    /// 一条超长的 `data:` 行被丢掉之后，这一条响应可能**一个事件行都没有了**，
    /// 于是那笔账永远等不到落点。
    /// 〔实测：第一版只累加不报，`an_over_cap_sse_line_is_reported_…` 当场红，
    ///  tee 流里只有一行 `__meta__`，丢掉的 9 MiB **一个字都没说**。〕
    ///
    /// 这一支与队列满那一支**不冲突**：这里是「投得进去」（丢的是解码缓冲，不是队列），
    /// 投不进去时把账**原样加回**，留给写线程报。
    pub(crate) fn note_dropped_bytes(&self, n: u64) {
        use std::sync::atomic::Ordering::SeqCst;
        if n == 0 {
            return;
        }
        self.dropped_bytes.fetch_add(n, SeqCst);
        let (l, b) = (
            self.dropped_lines.swap(0, SeqCst),
            self.dropped_bytes.swap(0, SeqCst),
        );
        if l == 0 && b == 0 {
            return;
        }
        let note = format!("{{\"__dropped__\":{{\"lines\":{l},\"bytes\":{b}}}}}\n");
        if self.tx.try_send(note).is_err() {
            self.dropped_lines.fetch_add(l, SeqCst);
            self.dropped_bytes.fetch_add(b, SeqCst);
        }
    }

    /// 把一行投进队列。**永不阻塞、永不持锁做写** —— 这就是 `阻-2(D3)` 的修法本身。
    ///
    /// 队列满 ⇒ 丢这一行并**计入账**；账由**写线程**在下一次写得动的时候报成一行
    /// `__dropped__`（见 `TeeSink::new`）。⇒ **丢是可见的**，不是静默的。
    ///
    /// **诚实边界**：一行都没写出去过（写线程从头到尾卡着）的时候，那笔账也报不出来
    /// —— 报账要有落点，而那时落点正卡着。
    fn write_line(&self, line: &str) {
        use std::sync::atomic::Ordering::SeqCst;
        if self.tx.try_send(line.to_string()).is_err() {
            self.dropped_lines.fetch_add(1, SeqCst);
            self.dropped_bytes.fetch_add(line.len() as u64, SeqCst);
        }
    }
}

/// 最小 JSON 串转义 —— 路由键已被白名单收窄过，这里是第二道。
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
#[path = "../../../tests/backend/relay/tee_tests.rs"]
mod tests;
