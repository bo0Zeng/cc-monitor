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
//! `doc/IPC-PROTOCOL.md` + 本文件 + `server.rs`，都是它自己和它的文档）
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
/// ⚠ **是条数不是体量** —— 它已在 `src-tauri/src/byte_cap_registry.rs` 的 `NOT_A_SIZE_CAP` 里
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
mod tests {
    use super::*;

    /// 「这条判据不测上限那一格」的写法：给一个**永远触发不了**的上限
    /// （理由同 `http1.rs` 那份 `NO_CAP`：期望值不许拿被测常量算）。
    const NO_CAP: usize = usize::MAX;

    #[test]
    fn splits_sse_data_lines_across_arbitrary_split_points() {
        let wire = b"event: message_start\ndata: {\"a\":1}\n\ndata: {\"b\":2}\n\ndata: [DONE]\n\n";
        let mut s = SseSplitter::default();
        let mut got = Vec::new();
        for b in wire.iter() {
            got.extend(s.feed(&[*b], NO_CAP));
        }
        // 期望值手写：两个事件，`[DONE]` 与非 data 行都不算。
        assert_eq!(got, vec!["{\"a\":1}".to_string(), "{\"b\":2}".to_string()]);
    }

    #[test]
    fn a_half_delivered_line_is_not_emitted_until_it_completes() {
        let mut s = SseSplitter::default();
        // ⚠ 这里刻意用中括号载荷：只读护栏的剥法按**大括号配平**，
        // 测试串里出现不配对的大括号会把剥除边界带偏（该护栏头注逐字警告过这个形状）。
        assert!(s.feed(b"data: [1,", NO_CAP).is_empty(), "半行不许吐");
        assert_eq!(s.feed(b"2]\n", NO_CAP), vec!["[1,2]".to_string()]);
    }

    /// ★ `TEE_DECODE_CAP` 在 `SseSplitter` 这一侧的那一格〔回修轮之五 08-25，`阻-1(D3)` 同职面〕。
    ///
    /// 分母 = 我列出的这 **3** 形：①超长半行被丢掉且**计数**；②它**不连坐**后面的完整行；
    /// ③没超上限时**一个字节都不丢**（非空对照 —— 没有它，「丢了 N 字节」可能只是它见谁丢谁）。
    #[test]
    fn an_endless_sse_line_is_dropped_and_counted_instead_of_growing_forever() {
        const CAP: usize = 64; // 手写字面量，不引生产常量
        let mut s = SseSplitter::default();

        // ③ 非空对照先做：正常的行一个字节都不许丢。
        assert_eq!(
            s.feed(b"data: {\"a\":1}\n", CAP),
            vec!["{\"a\":1}".to_string()]
        );
        assert_eq!(s.take_dropped(), 0, "正常的行不许丢");

        // ① 一条永不换行的超长行：吐不出东西，且**丢的字节数被记下来**。
        let long = vec![b'x'; CAP * 2];
        assert!(s.feed(&long, CAP).is_empty(), "超长的半行不许吐出来");
        let dropped = s.take_dropped();
        assert!(
            dropped >= CAP as u64,
            "丢了 {dropped} 字节 —— 超上限的半行必须被丢掉**并计数**（不许静默）"
        );
        assert_eq!(s.take_dropped(), 0, "取走之后账要清零，不许重复报");

        // ② 不连坐：紧跟其后的完整行照样拆得出来。
        assert_eq!(
            s.feed(b"tail\ndata: {\"b\":2}\n", CAP),
            vec!["{\"b\":2}".to_string()],
            "丢掉超长那一行之后，后面的完整行必须照常吐"
        );
    }

    /// 收集到内存里的 tee，用来在测试里读回写了什么。
    ///
    /// ★★ **它必须「能等」**〔回修轮之五 08-25，`阻-2(D3)` 的连带〕：
    /// 今天 tee 的写落在**另一条线程**上（那正是 `阻-2` 的修法）⇒ `event()` 返回**不代表已经写完**。
    /// 判据写完就读缓冲区 = 在读一个还没写完的东西，而「tee 是空的」与「还没写完」
    /// 在断言里**长得一模一样** ⇒ 那会是一条会随机说谎的判据。
    /// ⇒ 每写一行往通道投一条，判据用 `wait_lines` 等够行数；**等不到就当红**，不许当绿。
    ///
    /// ⚠ 等待逻辑刻意住在 `mod tests` 里、**不做成 `TeeSink` 的 `#[cfg(test)]` 方法**：
    /// `no_timer_guard` 的 `production_code()` 只剥 `#[cfg(test)] mod`，**不剥单个 `#[cfg(test)]` 函数**
    /// —— 我第一版就是那么写的，当场被它逮到两条红
    ///（`daemon_production_code_has_no_periodic_wakeups` 点名 `relay/tee.rs` 里的 `sleep(`，
    /// 以及 `every_duration_use_is_registered_as_non_timer` 说「生产段 `Duration::from_*` 有 2 处」）。
    /// 那两条红是**对的**：按那把尺子，我那个方法确实算生产段。
    #[derive(Clone)]
    struct MemSink {
        buf: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
        tick: std::sync::mpsc::Sender<()>,
    }
    impl Write for MemSink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.buf.lock().expect("lock").extend_from_slice(buf);
            let _ = self.tick.send(());
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// 造一个「能等」的 tee 落点：返回 `(TeeSink, 读回字节的句柄, 等行数用的接收端)`。
    fn waitable_sink() -> (
        TeeSink,
        std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
        std::sync::mpsc::Receiver<()>,
    ) {
        let (tick, rx) = std::sync::mpsc::channel();
        let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = TeeSink::new(Box::new(MemSink {
            buf: std::sync::Arc::clone(&buf),
            tick,
        }));
        (sink, buf, rx)
    }

    /// 等写线程写够 `n` 行。**等不到就 panic**（不许把「还没写完」读成「tee 是空的」）。
    ///
    /// 4 秒对回环内存写来说宽得离谱（实测这几条都在毫秒量级收工）⇒ 它只把**挂住**换成**红**。
    fn wait_lines(rx: &std::sync::mpsc::Receiver<()>, n: usize) {
        for i in 0..n {
            rx.recv_timeout(std::time::Duration::from_secs(4))
                .unwrap_or_else(|e| panic!("等 tee 的第 {} 行没等到：{e}", i + 1));
        }
    }

    #[test]
    fn meta_line_then_event_lines() {
        let (sink, buf, rx) = waitable_sink();
        let seq = sink.open("agentA", "acctA", "sid-AAA");
        assert_eq!(seq, 0);
        sink.event("agentA", "acctA", "sid-AAA", "{\"type\":\"x\"}");
        wait_lines(&rx, 2);
        let raw = buf.lock().expect("lock").clone();
        let text = String::from_utf8(raw).expect("utf8");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"__meta__\""), "首行必须是 meta");
        assert!(lines[0].contains("\"seq\":0"));
        // `event` 是**串**（订正见本文件头注）：上游那一段逐字保住，但不参与本行结构。
        assert!(
            lines[1].contains("\"event\":\"{\\\"type\\\":\\\"x\\\"}\""),
            "event 必须是转义过的串：{}",
            lines[1]
        );
        // ★ 这一条钉的是 `裁-3`：第一刀**不带** t_ns。带上它 = 动了零定时器护栏。
        assert!(!text.contains("t_ns"), "本刀不带 t_ns，见 super 头注㈡");
    }

    /// ⚠ **改名**〔回修轮之四 08-25，承接 D2 `建议-8`〕：旧名 `seq_is_**per_process**_and_monotonic`
    /// 里的「**per process**」是假的 —— `seq` 是 `TeeSink` 的**实例字段**，本条自己
    /// `TeeSink::new(...)` 造**一个**再断 0/1/2 ⇒ 它只证了**实例内**单调。
    /// 「一个进程一份」今天靠的是生产段 `run_with` 里那**唯一一个** `TeeSink::to_stdout()`
    /// 调用点，而**那一点没有任何判据钉着**（登记住址件文件 §8.18.9 `判不了-单例`）。
    /// ⇒ 名字只说它证得了的那一半。
    #[test]
    fn seq_is_monotonic_within_one_sink() {
        let (sink, _buf, _rx) = waitable_sink();
        assert_eq!(sink.open("a", "acctA", "k1"), 0);
        assert_eq!(sink.open("b", "acctB", "k2"), 1);
        assert_eq!(sink.open("a", "acctA", "k1"), 2);
    }

    /// ★★ **上游内容是敌手可控的** —— `data:` 后面那一段原样进这一行。
    ///
    /// 本条钉的是 `DoD-3㈠` acceptor 逐字那半句：「其后**每行可解析**」；
    /// 顺带钉住**路由键** —— `agent` / `key` 是下游按会话分流的唯一依据，
    /// 而 `serde_json` / `json.loads` 两侧解重复键都是 **last-wins**
    /// ⇒ 上游只要把 `,"agent":"…"` 拼进来，就能改写这一行的落点。
    ///
    /// 分母 = 我列出的这 **5** 形，每一形都是上游**一行 `data:`** 就发得出来的。
    /// 〔来历：D1 `重要-7` → D2 `阻-1(D2)`。这条判据是它今天的落点，登记住址见件文件 §8.18.1。〕
    #[test]
    fn an_upstream_payload_cannot_break_out_of_the_event_field() {
        // 每一形 = 上游 `data:` 后面那一段的**原样字节**。期望值全是手写字面量。
        let hostile = [
            // ㈠ 改写路由键：拼一段合法 JSON 片段，把 `agent` 顶掉
            "1,\"agent\":\"evil\"",
            // ㈡ 整行不再是合法 JSON：上游发的根本不是 JSON（SSE 的 data: 允许任意文本）
            "oops",
            // ㈢ 行尾多出垃圾（⚠ 刻意用**中括号**：只读护栏的剥法按大括号配平，
            //    测试串里写不配对的大括号会把剥除边界带偏 —— 上面 `:141` 那条注释
            //    逐字警告过这一形，而我这条夹具的第一版就是这么把它打红的）
            "[1,2]]",
            // ㈣ 裸控制字符：SSE 拆行器只吃掉 \r 与 \n，别的控制字符原样穿过来
            "{\"a\":\"x\u{1}y\"}",
            // ㈤ 以**反斜杠**结尾：不转义就会把这一行的结束引号吃掉
            //（这一形是回修轮之四复扫补的 —— 前四形都不含 `\`）
            "tail\\",
        ];
        for payload in hostile {
            let (sink, buf, rx) = waitable_sink();
            sink.event("realA", "realAcct", "sid-AAA", payload);
            wait_lines(&rx, 1);
            let raw = buf.lock().expect("lock").clone();
            let text = String::from_utf8(raw).expect("utf8");
            let line = text.trim_end_matches('\n');
            let v: serde_json::Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("tee 行必须可解析（DoD-3㈠）：{line:?} ⇒ {e}"));
            assert_eq!(v["agent"], "realA", "上游内容改写了路由键 agent：{line:?}");
            assert_eq!(v["key"], "sid-AAA", "上游内容改写了路由键 key：{line:?}");
            // ★ 原样透传**没丢**：`event` 的值逐字节还是上游那一段（`K9` 裁定二）。
            assert_eq!(v["event"], payload, "上游那一段必须逐字保住：{line:?}");
        }
    }

    /// ★★ **队列满了要丢，但丢必须说**〔回修轮之五 08-25，`阻-2(D3)` 的代价那一面〕。
    ///
    /// # 为什么这一条是承重的
    ///
    /// `阻-2` 的修法把「阻塞的写」换成了「有界队列」，代价是**队列满时要丢行**。
    /// 而 `DoD-3㈠` acceptor 要的是「`event` 数 == 上游事件数」——
    /// **静默地丢**会让那笔账永远对不上，还查不出是谁丢的。
    /// ⇒ 这一条钉的就是那句代价：丢了要在流里留下 `__dropped__`，**说清丢了几行**。
    ///
    /// # 量法
    ///
    /// 落点先卡住（写第一行时挡在门闩上），趁它卡着灌 `TEE_QUEUE_LINES + 64` 行 ——
    /// 队列必然满、必然丢。然后放行，再写一行，读回全文。
    ///
    /// 非空对照：`__dropped__` 里的 `lines` 必须 **> 0**（不是「有这个词就算」）。
    #[test]
    fn a_full_queue_drops_lines_but_says_so() {
        let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
        let (tick, rx) = std::sync::mpsc::channel();
        let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        struct Gated {
            buf: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
            tick: std::sync::mpsc::Sender<()>,
            gate: Option<std::sync::mpsc::Receiver<()>>,
        }
        impl Write for Gated {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                // 第一行卡在门闩上；放行之后正常写。
                if let Some(g) = self.gate.take() {
                    let _ = g.recv();
                }
                self.buf.lock().expect("lock").extend_from_slice(b);
                let _ = self.tick.send(());
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let sink = TeeSink::new(Box::new(Gated {
            buf: std::sync::Arc::clone(&buf),
            tick,
            gate: Some(gate_rx),
        }));

        // 灌到必然溢出。**期望值不拿 `TEE_QUEUE_LINES` 算**，只断「丢了 > 0 行」。
        for i in 0..(TEE_QUEUE_LINES + 64) {
            sink.event("agentA", "acctA", "sid-AAA", &format!("{{\"i\":{i}}}"));
        }
        gate_tx.send(()).expect("放行");
        // 等到那行补报出来（等不到就红，不许把「还没写完」读成「没有报」）。
        // `__dropped__` 由**写线程**在写完一行之后补 ⇒ 放行之后它自己会出来。
        let mut text = String::new();
        while rx.recv_timeout(std::time::Duration::from_secs(4)).is_ok() {
            text = String::from_utf8_lossy(&buf.lock().expect("lock").clone()).to_string();
            if text.contains("__dropped__") {
                break;
            }
        }
        assert!(
            text.contains("__dropped__"),
            "队列溢出必须在流里留下 `__dropped__`，**不许静默丢**：{}",
            &text[..text.len().min(400)]
        );
        let note = text
            .lines()
            .find(|l| l.contains("__dropped__"))
            .expect("那一行");
        let v: serde_json::Value = serde_json::from_str(note)
            .unwrap_or_else(|e| panic!("`__dropped__` 行必须可解析（DoD-3㈠）：{note:?} ⇒ {e}"));
        let lines = v["__dropped__"]["lines"].as_u64().expect("lines 是个数");
        assert!(lines > 0, "非空对照：报出来的丢行数必须 > 0：{note}");
    }

    /// `json_str` 的三条转义规则各一格。**期望值全是手写字面量。**
    ///
    /// ⚠ **补一格**〔回修轮之四 08-25 复扫补的〕：先前只有 `"` 与控制字符两格，
    /// **反斜杠那一格没有** —— 而 `\` 恰恰是最要命的一形：一段以 `\` 结尾的内容
    /// 不转义就会把**结束引号**吃掉，整行结构塌掉。本轮起 `event` 的载荷（**上游给的、
    /// 敌手可控的字节**）正是走这个函数 ⇒ 这一格从「顺手补的」变成了承重的。
    /// 分母 = `json_str` 的 `match` 里那 **3** 条规则（`"` · `\` · `< 0x20`）。
    #[test]
    fn json_escaping_closes_the_second_door() {
        assert_eq!(json_str("a\"b"), "\"a\\\"b\"");
        assert_eq!(json_str("a\nb"), "\"a\\u000ab\"");
        assert_eq!(json_str("a\\b"), "\"a\\\\b\"", "反斜杠必须转义");
        assert_eq!(
            json_str("x\\"),
            "\"x\\\\\"",
            "结尾的反斜杠不许把结束引号吃掉"
        );
    }
}
