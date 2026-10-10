//! tee：把响应体里的 SSE 事件抄一份出去。**只抄响应体，永不抄请求头**
//! （请求那一侧只出一样：名单上那几项在不在，布尔，见 [`TapPort::note_marks`]）。
//!
//! # 落点：**tap**（常驻后端进程内那一份中转）
//!
//! 中转住常驻后端进程里（`listen::host`），那个进程的 stdout 是 wire（stdio 载体）或 null（脱离载体），不能写行 ⇒ 落点是一个
//! [`TapPort`]：每个 SSE 事件交一个 [`TapEvent`]（**结构体，不是格式串** —— 线上字段名住
//! `wire.rs::Frame::Tap` 的 serde 名，那道「字段名住哪」的答案），由宿主转成 `tap` 帧。
//!
//! - **四样东西**：`stream`（请求自带的会话标识头，中转不解释它）· `resp`（本进程第几个响应）·
//!   `n`（这一个响应里第几个事件，从 0 连续）· 事件原文 / 收尾方式。**不带路由那两段**：挂载物 ① 不问账号。
//! - **丢必须说，而且说在原位**：每个事件**先占号再投递**；单个事件超 [`TAP_DATA_CAP`] ⇒ 只交开头那一截、明标截断与原长
//!   （[`TapBody::Clipped`]；有的协议开头那一件就带整份应答对象）；投不进（宿主通道满 / 没人连着）、解码那一路丢了半行 ⇒ 号照占、事件没了 ⇒ 接收侧看 `n` 连不连得上就知道丢在哪两个号之间
//!   （`Gap{from_seq,to_seq}` 那一形，纯算术，不要旁路计数行）。收尾那一件带「一共占了几个号」⇒ 尾巴上的缺口也看得见。
//! - **永不阻塞转发**：`TapPort::offer` 的契约是「立刻答收没收」（宿主用 `try_send`）。
//! - **事件原文是敌手可控的字节**：原样交出去（内容一律原样透传），上线时是 `tap` 帧里的**一个 JSON 串**，
//!   不参与帧结构（序列化由 serde 做，不手拼）。
//!

/// SSE 拆行器 —— 增量喂字节，吐出 `data:` 行的载荷。
///
/// 它只认 SSE 的**分帧**（行、`data:` 前缀），不认里面是什么。
/// 切行交给 [`super::framer::LineFramer`]（`relay/` 里唯一的增量分帧器）——
/// 这里只剩 SSE 自己的那一层：`data:` 前缀、`[DONE]`、上限。
pub struct SseSplitter {
    pub framer: super::framer::LineFramer,
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
    /// # `cap` 管的是**半行**
    ///
    /// 分帧器只在**还没遇到 `\n`** 的时候留着字节。上游发一条永不换行的 `data:` 行，
    /// 它就一直涨 —— 与 `ChunkedView::feed` 那条同族（**按真实字节增长**，不是「拿一个数去分配」）。
    ///
    /// 超了怎么办：**丢掉这条半行并计数**，其后的字节从下一个 `\n` 重新开始拆
    /// —— tee 少一行，**下游的字节一个不少**（tee 是抄一份，不在转发那条路上）。
    /// 计数由 `server.rs::serve_one` 取走交给 [`TeeSink::note_dropped_bytes`]（占一个号不发 ⇒ 接收侧看得见缺口）⇒ **不是静默丢**。
    pub fn feed(&mut self, decoded: &[u8], cap: usize) -> Vec<String> {
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
    pub fn take_dropped(&mut self) -> u64 {
        std::mem::take(&mut self.dropped)
    }
}

/// tee 交给宿主的一件事（[`TapPort::offer`]）。字段语义见本文件头注「第二个落点」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TapEvent {
    /// 请求自带的会话标识头的值（中转不解释它；消费侧拿它对 sid）；没有 ⇒ 空串。
    pub stream: String,
    /// 第二个标签（请求自带的另一个头的值，中转不解释它）；没有 ⇒ 空串。
    pub owner: String,
    /// 本进程第几个响应（跨连接单调，[`TeeSink`] 那一个计数器）。
    pub resp: u64,
    /// 这一个响应里第几个事件（从 0 连续）；收尾那一件是「一共占了几个号」。
    pub n: u64,
    pub body: TapBody,
}

/// 一件事是什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TapBody {
    /// 一个 SSE 事件：`data:` 后面那段原文（敌手可控字节，原样；上线时是一个 JSON 串，不参与帧结构）。
    Data(String),
    /// 一个超 [`TAP_DATA_CAP`] 的事件：只交开头那 `head`（不超上限、在字符边界上切）＋ 原长 `len`（字节）。
    /// **它不是完整的事件**：接收侧只许从开头认那几格（类型 · 标识），不许当整件内容用。
    Clipped { head: String, len: u64 },
    /// 这个响应不会再有事件了。`broken` = 转发以错误收尾（下游 / 上游断了），否则上游正常说完。
    End { broken: bool },
}

/// tee 的第二个落点的**口**：宿主实现它（常驻后端的 `tap::TapHub`）。
///
/// ★ 契约：**立刻答收没收**，永不阻塞 —— 它在转发线程上被调（`pump` 的 `on_chunk` 里），
/// 阻塞 = 让「有它更好」变成「非它不可」。答 `false` 的那一件号已占，接收侧看得见缺口。
pub trait TapPort: Send + Sync {
    fn offer(&self, ev: TapEvent) -> bool;

    /// 一条流（`stream`）的一发请求里，名单上那几项各在不在（[`super::Destinations::request_marks`]）。
    /// 只交布尔，不交头的值。缺省 ⇒ 不收。
    fn note_marks(&self, _stream: &str, _marks: &[RequestMark]) {}
}

/// 一项的「在不在」：（头名, 那一项的前缀）＋ 这一发带没带。
pub type RequestMark = ((&'static str, &'static str), bool);

/// 单个事件原文的字节上限。超了只交开头这么多、标成截断（[`TapBody::Clipped`]）。
///
/// 值怎么定的：上游的 SSE 是 token 级增量，增量事件在 KiB 级；开头 / 收尾那一件在有的协议里带整份应答对象（含整段系统提示），
/// 远超这个数，但接收侧从它要的只有开头那几格。它同时把「宿主通道满载」封在 `容量 × 16 KiB`。
/// 登记住址 `tests/frontend/shell/byte_cap_registry.rs`（尺寸类常量不登记就红）。
pub const TAP_DATA_CAP: usize = 16 * 1024;

/// 一个响应在 tee 这一侧的游标：`resp` 与下一个要占的号 `n`。由 [`TeeSink::open`] 发出，
/// 同一个响应的 `event` / `note_dropped_bytes` / `close` 都拿它（响应之间互不相干，所以不放进共享的 `TeeSink`）。
#[derive(Debug)]
pub struct TeeStream {
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

/// tee 的落点。一个进程只有一个，**跨连接共享**（`TeeSink` 是 `Relay` 的字段，`serve()` 每连接 `Arc::clone`）。
///
/// 转发那条路上只做「占号 ＋ `offer`」：`offer` 立刻答收没收（[`TapPort`] 的契约）⇒ 转发不因抄一份而被拖住。
pub struct TeeSink {
    port: std::sync::Arc<dyn TapPort>,
    /// 本进程第几个响应（[`TapEvent::resp`]）。
    seq: std::sync::atomic::AtomicU64,
}

impl TeeSink {
    /// **tap 口**落点：常驻后端进程内那一份中转用它（`listen::host`）。
    ///
    /// 那个进程的 stdout 在 stdio 载体上**就是 wire**（一行一帧，`wire.rs` 头注），在脱离载体上是 null
    /// ⇒ 不写行，把每个事件交给宿主的 [`TapPort`]（宿主转成 `tap` 帧，走它自己那条有界通道）。
    pub fn to_port(port: std::sync::Arc<dyn TapPort>) -> Self {
        Self {
            port,
            seq: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// 一个响应开头：发这一响应自己的游标（`resp` 取本进程的下一个序号，位置号 `n` 从 0 起）。
    /// 没有「开头那一件」：`resp` 随每一件事走（接收侧按 `(stream, resp)` 分响应）。
    pub fn open(&self) -> TeeStream {
        let seq = self.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        TeeStream { resp: seq, n: 0 }
    }

    /// 这条流的这一发请求里名单上那几项在不在（没有流标签 / 名单是空的 ⇒ 不交）。
    pub fn mark(&self, stream: &str, marks: &[RequestMark]) {
        if !stream.is_empty() && !marks.is_empty() {
            self.port.note_marks(stream, marks);
        }
    }

    /// 一个 SSE 事件：先占号，再投递（投不进 ⇒ 号照占，缺口在接收侧可算；超界 ⇒ 交开头那一截、标截断）。
    pub(crate) fn event(&self, id: super::StreamId<'_>, at: &mut TeeStream, payload: &str) {
        let n = at.take();
        let body = if payload.len() <= TAP_DATA_CAP {
            TapBody::Data(payload.to_string())
        } else {
            let mut cut = TAP_DATA_CAP;
            while !payload.is_char_boundary(cut) {
                cut -= 1;
            }
            TapBody::Clipped {
                head: payload[..cut].to_string(),
                len: payload.len() as u64,
            }
        };
        let _ = self.port.offer(TapEvent {
            stream: id.stream.to_string(),
            owner: id.owner.to_string(),
            resp: at.resp,
            n,
            body,
        });
    }

    /// 解码那一路（`SseSplitter` / `ChunkedView` 超上限）丢掉了字节：那一截里至少有一个事件没成形
    /// ⇒ **占一个号不发**，接收侧当场看见缺口（它不需要知道丢了多少字节，只需要知道「这里断过」）。
    /// 由 `server.rs::serve_one` 每块调一次（`n == 0` 是常态，直接返回）。
    pub fn note_dropped_bytes(&self, at: &mut TeeStream, n: u64) {
        if n > 0 {
            at.take();
        }
    }

    /// 这个响应收尾了：交一件 `End`（`n` = 一共占了几个号 ⇒ 尾巴上的缺口看得见）；
    /// **一个号都没占过的响应不交**（非 SSE 的响应：`HEAD /api/hello`、JSON 错误体 —— 接收侧本来也认不出它们）。
    pub(crate) fn close(&self, id: super::StreamId<'_>, at: TeeStream, broken: bool) {
        if at.n > 0 {
            let _ = self.port.offer(TapEvent {
                stream: id.stream.to_string(),
                owner: id.owner.to_string(),
                resp: at.resp,
                n: at.n,
                body: TapBody::End { broken },
            });
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/comms/outward/tee_tests.rs"]
mod tests;
