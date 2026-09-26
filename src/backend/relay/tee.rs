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

//! ## 〔TAP · V124〕第二个落点：**tap**（常驻后端进程内那一份中转）
//!
//! `--relay` 独立进程照旧写上面那种 NDJSON 行（stdout，一字不改）。常驻后端进程内那一份中转
//! （`listen::host`）的 stdout 是 wire（stdio 载体）或 null（脱离载体），不能写行 ⇒ 它的落点是一个
//! [`TapPort`]：每个 SSE 事件交一个 [`TapEvent`]（**结构体，不是格式串** —— 线上字段名住
//! `wire.rs::Frame::Tap` 的 serde 名，`05 §9` 第 4 条那道「字段名住哪」的答案），由宿主转成 `tap` 帧。
//!
//! - **四样东西**：`stream`（路由第三段原样）· `resp`（本进程第几个响应，与 `__meta__` 的 `seq` 同一个数）·
//!   `n`（这一个响应里第几个事件，从 0 连续）· 事件原文 / 收尾方式。**不带前两段**：挂载物 ① 不问账号（`20 §11` I2）。
//! - **丢必须说，而且说在原位**：每个事件**先占号再投递**；投不进（宿主通道满 / 没人连着）、单个事件超
//!   [`TAP_DATA_CAP`]、解码那一路丢了半行 ⇒ 号照占、事件没了 ⇒ 接收侧看 `n` 连不连得上就知道丢在哪两个号之间
//!   （`05 §3.3.4` 的 `Gap{from_seq,to_seq}` 那一形，纯算术，不要旁路计数行）。收尾那一件带「一共占了几个号」⇒ 尾巴上的缺口也看得见。
//! - **永不阻塞转发**：`TapPort::offer` 的契约是「立刻答收没收」（宿主用 `try_send`）。
//!
//! 设计与读数住仓外 `调研/第四波记录/TAP.md`。

use std::io::Write;

/// SSE 拆行器 —— 增量喂字节，吐出 `data:` 行的载荷。
///
/// 它只认 SSE 的**分帧**（行、`data:` 前缀），不认里面是什么。
/// 切行交给 [`super::framer::LineFramer`]（`relay/` 里唯一的增量分帧器，`设计/17 §3.7`）——
/// 这里只剩 SSE 自己的那一层：`data:` 前缀、`[DONE]`、上限。
pub(crate) struct SseSplitter {
    pub(super) framer: super::framer::LineFramer,
    /// 超 `cap` 时丢掉的字节数（累计，由 `take_dropped` 取走）。
    dropped: u64,
}

impl Default for SseSplitter {
    fn default() -> Self {
        Self {
            framer: super::framer::LineFramer::new(b"\n"),
            dropped: 0,
        }
    }
}

impl SseSplitter {
    /// # `cap` 管的是**半行**〔回修轮之五 08-25，`阻-1(D3)` 的同职面〕
    ///
    /// 分帧器只在**还没遇到 `\n`** 的时候留着字节。上游发一条永不换行的 `data:` 行，
    /// 它就一直涨 —— 与 `ChunkedView::feed` 那条同族（**按真实字节增长**，不是「拿一个数去分配」）。
    ///
    /// 超了怎么办：**丢掉这条半行并计数**，其后的字节从下一个 `\n` 重新开始拆
    /// —— tee 少一行，**下游的字节一个不少**（tee 是抄一份，不在转发那条路上）。
    /// 计数由 `server.rs::handle` 取走并写进 tee 流的 `__dropped__` 行 ⇒ **不是静默丢**。
    pub(crate) fn feed(&mut self, decoded: &[u8], cap: usize) -> Vec<String> {
        self.framer.push(decoded);
        // 只量**第一行**：它才是那条「攒着还没成形」的。其后的完整行照常拆，不许被连坐。
        let head_len = self.framer.head_len();
        if head_len > cap {
            self.dropped += head_len as u64;
            self.framer.skip(head_len);
        }
        let mut out = Vec::new();
        while let Some(line) = self.framer.next_line() {
            let line = String::from_utf8_lossy(line);
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
/// 〔TAP〕tee 交给宿主的一件事（[`TapPort::offer`]）。字段语义见本文件头注「第二个落点」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TapEvent {
    /// 路由第三段原样（中转不解释它；消费侧拿它对 sid）。
    pub(crate) stream: String,
    /// 本进程第几个响应（与 NDJSON 那一形 `__meta__.seq` 同一个计数器）。
    pub(crate) resp: u64,
    /// 这一个响应里第几个事件（从 0 连续）；收尾那一件是「一共占了几个号」。
    pub(crate) n: u64,
    pub(crate) body: TapBody,
}

/// 〔TAP〕一件事是什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TapBody {
    /// 一个 SSE 事件：`data:` 后面那段原文（敌手可控字节，原样；上线时是一个 JSON 串，不参与帧结构）。
    Data(String),
    /// 这个响应不会再有事件了。`broken` = 转发以错误收尾（下游 / 上游断了），否则上游正常说完。
    End { broken: bool },
}

/// 〔TAP〕tee 的第二个落点的**口**：宿主实现它（常驻后端的 `tap::TapHub`）。
///
/// ★ 契约：**立刻答收没收**，永不阻塞 —— 它在转发线程上被调（`pump` 的 `on_chunk` 里），
/// 阻塞 = 让「有它更好」变成「非它不可」（`05 §4.5.3` ③）。答 `false` 的那一件号已占，接收侧看得见缺口。
pub(crate) trait TapPort: Send + Sync {
    fn offer(&self, ev: TapEvent) -> bool;
}

/// 〔TAP〕单个事件原文的字节上限。超了**不交**、号照占（缺口可见）。
///
/// 值怎么定的：Anthropic 的 SSE 是 token 级增量，`message_start` 带整份 usage 也在 KiB 级；
/// 16 KiB 以上的一个事件只可能来自不正常的上游。它同时把「宿主通道满载」封在 `容量 × 16 KiB`。
/// 登记住址 `src/bridge/src/byte_cap_registry.rs`（尺寸类常量不登记就红）。
pub(crate) const TAP_DATA_CAP: usize = 16 * 1024;

/// 〔TAP〕一个响应在 tee 这一侧的游标：`resp` 与下一个要占的号 `n`。由 [`TeeSink::open`] 发出，
/// 同一个响应的 `event` / `note_dropped_bytes` / `close` 都拿它（响应之间互不相干，所以不放进共享的 `TeeSink`）。
#[derive(Debug)]
pub(crate) struct TeeStream {
    resp: u64,
    n: u64,
}

impl TeeStream {
    /// 占一个号，返回它。
    fn take(&mut self) -> u64 {
        let n = self.n;
        self.n += 1;
        n
    }
}

/// 〔TAP〕tee 的落点：NDJSON 行（`--relay` 的 stdout）· tap 口（常驻后端进程内）。
enum Landing {
    Lines {
        tx: std::sync::mpsc::SyncSender<String>,
        /// 队列满而被丢掉的**行数** / **字节数**（后者含解码缓冲那一路，见 `note_dropped_bytes`）。
        ///
        /// ⚠ 报账的是**写线程**，不是投递方。投递方那一侧是「队列满了」才丢的，
        /// 它当场**也投不进**那行 `__dropped__` —— 第一版就是那么写的，实测那行永远补不出来
        /// （逐字：`队列溢出必须在流里留下 __dropped__` 当场红，流里只有事件行）。
        /// ⇒ 谁有地方写谁报：写线程每写完一行就把账**取空**报一次。
        dropped_lines: std::sync::Arc<std::sync::atomic::AtomicU64>,
        dropped_bytes: std::sync::Arc<std::sync::atomic::AtomicU64>,
    },
    Tap(std::sync::Arc<dyn TapPort>),
}

pub(crate) struct TeeSink {
    landing: Landing,
    seq: std::sync::atomic::AtomicU64,
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
            landing: Landing::Lines {
                tx,
                dropped_lines,
                dropped_bytes,
            },
            seq: AtomicU64::new(0),
        }
    }

    pub(crate) fn to_stdout() -> Self {
        Self::new(Box::new(std::io::stdout()))
    }

    /// 〔TAP · V124〕**tap 口**落点：常驻后端进程内那一份中转用它（`listen::host`）。
    ///
    /// 那个进程的 stdout 在 stdio 载体上**就是 wire**（一行一帧，`wire.rs` 头注）—— NDJSON 行写进去当场污染协议；
    /// 在脱离载体上是 null。⇒ 这一份不写行，把每个事件交给宿主的 [`TapPort`]（宿主转成 `tap` 帧，走它自己那条有界通道）。
    /// 〔先前这里是 `discard`（丢弃落点，RL1）：tee 零消费者时的形状；第一个消费者来了，它换成这个口。〕
    pub(crate) fn to_port(port: std::sync::Arc<dyn TapPort>) -> Self {
        Self {
            landing: Landing::Tap(port),
            seq: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// 一个响应开头写一行 meta，返回这一响应的序号。
    ///
    /// ⚠ `K-H2` 加了 `account` 这一格 —— 路由键有三段，tee 行就该有三格。
    /// **不合并进 `key`**：那正是「一个值装了两件事」，本工作区最贵的那族病。
    /// ⚠⚠ 🔴 **条 48：参数用位置名，线上那三个字段名一个字节都不动。**
    ///
    /// 收的是一个 [`super::StreamId`]（`{ key: RouteKey{seg1,seg2}, stream }`），
    /// 而写出去的仍然是 `"agent"` / `"account"` / `"key"` 三个字段 ——
    /// **那是线契约**（`src/doc/IPC-PROTOCOL.md` 那一行 ＋ tee 的下游消费者），
    /// 改它就是改行为。⇒ 改的只有**这一层的类型**，线上零变化。
    pub(crate) fn open(&self, id: super::StreamId<'_>) -> TeeStream {
        let seq = self.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        // 〔TAP〕tap 口那一形没有 meta 那一行：`resp` 随每一件事走（接收侧按 `(stream, resp)` 分响应）。
        if matches!(self.landing, Landing::Tap(_)) {
            return TeeStream { resp: seq, n: 0 };
        }
        let line = format!(
            "{{\"__meta__\":{{\"source\":\"relay\",\"proto\":\"passthrough-v0\",\"agent\":{},\"account\":{},\"key\":{},\"seq\":{}}}}}\n",
            json_str(&id.key.seg1),
            json_str(&id.key.seg2),
            json_str(id.stream),
            seq
        );
        self.write_line(&line);
        TeeStream { resp: seq, n: 0 }
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
    pub(crate) fn event(&self, id: super::StreamId<'_>, at: &mut TeeStream, payload: &str) {
        // 〔TAP〕先占号，再投递（投不进 / 超界 ⇒ 号照占，缺口在接收侧可算）。
        let n = at.take();
        if let Landing::Tap(port) = &self.landing {
            if payload.len() <= TAP_DATA_CAP {
                let _ = port.offer(TapEvent {
                    stream: id.stream.to_string(),
                    resp: at.resp,
                    n,
                    body: TapBody::Data(payload.to_string()),
                });
            }
            return;
        }
        let line = format!(
            "{{\"agent\":{},\"account\":{},\"key\":{},\"event\":{}}}\n",
            json_str(&id.key.seg1),
            json_str(&id.key.seg2),
            json_str(id.stream),
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
    pub(crate) fn note_dropped_bytes(&self, at: &mut TeeStream, n: u64) {
        use std::sync::atomic::Ordering::SeqCst;
        if n == 0 {
            return;
        }
        let (tx, dropped_lines, dropped_bytes) = match &self.landing {
            // 〔TAP〕tap 口那一形：解码丢掉的那一截里至少有一个事件没成形 ⇒ **占一个号不发**，
            //   接收侧当场看见缺口（它不需要知道丢了多少字节，只需要知道「这里断过」）。
            Landing::Tap(_) => {
                at.take();
                return;
            }
            Landing::Lines {
                tx,
                dropped_lines,
                dropped_bytes,
            } => (tx, dropped_lines, dropped_bytes),
        };
        dropped_bytes.fetch_add(n, SeqCst);
        let (l, b) = (dropped_lines.swap(0, SeqCst), dropped_bytes.swap(0, SeqCst));
        if l == 0 && b == 0 {
            return;
        }
        let note = format!("{{\"__dropped__\":{{\"lines\":{l},\"bytes\":{b}}}}}\n");
        if tx.try_send(note).is_err() {
            dropped_lines.fetch_add(l, SeqCst);
            dropped_bytes.fetch_add(b, SeqCst);
        }
    }

    /// 〔TAP〕这个响应收尾了。tap 口那一形交一件 `End`（`n` = 一共占了几个号 ⇒ 尾巴上的缺口看得见）；
    /// **一个号都没占过的响应不交**（非 SSE 的响应：`HEAD /api/hello`、JSON 错误体 —— 接收侧本来也认不出它们）。
    /// NDJSON 那一形没有收尾行（行格式是契约，一字不改）。
    pub(crate) fn close(&self, id: super::StreamId<'_>, at: TeeStream, broken: bool) {
        if let Landing::Tap(port) = &self.landing {
            if at.n > 0 {
                let _ = port.offer(TapEvent {
                    stream: id.stream.to_string(),
                    resp: at.resp,
                    n: at.n,
                    body: TapBody::End { broken },
                });
            }
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
        let Landing::Lines {
            tx,
            dropped_lines,
            dropped_bytes,
        } = &self.landing
        else {
            return;
        };
        if tx.try_send(line.to_string()).is_err() {
            dropped_lines.fetch_add(1, SeqCst);
            dropped_bytes.fetch_add(line.len() as u64, SeqCst);
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
