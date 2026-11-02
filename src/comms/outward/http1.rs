//! 手写的最小 HTTP/1.1 —— **只解析中转必须懂的那几样**，别的一律当字节。
//!
//! `K8`/`D4` 那条「优先手写最小 HTTP，别引框架」的白名单在 `裁-1` 里**一个字没松**：
//! 这里没有任何 HTTP 库，只有请求行 + 头 + `Content-Length` + chunked 拆帧。

use copy_core::copy_text;
use std::io::{BufRead, Read};

/// `Content-Length` 读出来的三张脸。**刻意不是 `Option<usize>`** —— 理由见
/// `RequestHead::content_length` 的头注（「没有这个头」与「读不懂」挤在同一个 `None` 里
/// 会让请求体被静默丢掉）。
#[derive(Debug, PartialEq, Eq)]
pub enum BodyLen {
    /// 没有 `Content-Length` 头 ⇒ 没有请求体。
    Absent,
    /// 读得懂的长度。
    Exact(usize),
    /// **有这个头，但 `usize::from_str` 解不了**（`7abc` · 折叠成 `7, 7` 的重复头 · 带非法空白…）。
    /// 调用方**必须**当错误处理，**不许**当成「没有请求体」。
    Unparsable,
}

/// 请求头部（不含请求体）。
#[derive(Debug)]
pub struct RequestHead {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
}

impl RequestHead {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// 请求体长度。**只认 `Content-Length`**：chunked 的请求体本中转不接
    /// （claude 发的是带 `Content-Length` 的 `POST`）。不认的形状由调用方回 4xx。
    ///
    /// ★★ **返回三值，不是 `Option`**：
    /// 「**没有这个头**」与「**有这个头但读不懂**」挤进同一个 `None` 的话，调用点把 `None` 当成「没有请求体」
    /// ⇒ 一发 `Content-Length: 7abc` + 7 字节体 ⇒ **请求体被静默丢掉**、
    /// 上游收到空体、下游拿到一条正常的 200，全程零日志零 4xx。
    /// 中转搬的正是 `POST /v1/messages` 的载荷，丢了它 claude 当场坏而门禁全绿。
    /// ⇒ 两件事分成两张脸，`Unparsable` 由调用方回 **400**。
    pub fn content_length(&self) -> BodyLen {
        let Some(raw) = self.header("content-length") else {
            return BodyLen::Absent;
        };
        match raw.trim().parse::<usize>() {
            Ok(n) => BodyLen::Exact(n),
            Err(_) => BodyLen::Unparsable,
        }
    }

    pub fn is_chunked_body(&self) -> bool {
        self.header("transfer-encoding")
            .is_some_and(|v| v.to_ascii_lowercase().contains("chunked"))
    }
}

/// **逐字节**读到 `\r\n\r\n` 为止。不用 `BufReader::read_line`：中转要在读完头之后
/// 把**剩下的字节一个不差**地当请求体，而带缓冲的读会把请求体的头几字节吞进缓冲区。
///
/// `cap` 是头部字节上限，超了返回 `None`（而不是无限吃内存）。
///
/// ⚠ 不回 **431**。`None` 有**两个**来源（超上限 · 头没读完就 EOF），本函数**不区分**它们，
/// 而两个调用点各自按自己的语境回：请求那一侧 `server.rs` 回 **400 Bad Request**，
/// 响应那一侧回 **502 Bad Gateway**。要真回 431 得先让本函数把两个来源分开。
pub fn read_head<R: Read>(r: &mut R, cap: usize) -> std::io::Result<Option<Vec<u8>>> {
    let mut buf = Vec::with_capacity(1024);
    let mut one = [0u8; 1];
    while buf.len() < cap {
        let n = r.read(&mut one)?;
        if n == 0 {
            return Ok(None);
        }
        buf.push(one[0]);
        if buf.ends_with(b"\r\n\r\n") {
            return Ok(Some(buf));
        }
    }
    Ok(None)
}

/// 解析请求头部字节。头名保留原样，值去掉两端空白。
pub fn parse_request(raw: &[u8]) -> Option<RequestHead> {
    let text = std::str::from_utf8(raw).ok()?;
    let mut lines = text.split("\r\n");
    let mut start = lines.next()?.split(' ');
    let method = start.next()?.to_string();
    let target = start.next()?.to_string();
    let version = start.next()?;
    if !version.starts_with("HTTP/1.") || method.is_empty() || !target.starts_with('/') {
        return None;
    }
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        let (k, v) = line.split_once(':')?;
        if k.is_empty() || k.contains(' ') {
            return None;
        }
        headers.push((k.to_string(), v.trim().to_string()));
    }
    Some(RequestHead {
        method,
        target,
        headers,
    })
}

/// 逐跳头 —— **不转发**。转发它们会让上游读到一个关于「我们这一跳」的谎。
const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

pub fn is_hop_by_hop(name: &str) -> bool {
    HOP_BY_HOP.iter().any(|h| name.eq_ignore_ascii_case(h))
}

/// 响应体的**解码视图** —— 喂 tee，并说出这个应答收在哪；**不影响下游拿到的字节**。
///
/// 下游拿到的永远是上游原样的字节（含 chunked 分帧）；tee 要看见 SSE 文本，
/// 所以这里把 chunked 拆掉。两条路吃的是同一批字节，但**下游那条不经过这里**。
/// 收尾（[`BodyView::ended`]）是连接能不能接着用的依据：收在边上、后面没有多出来的字节，才还回连接池。
pub enum BodyView {
    /// 没有体：`HEAD` 的应答 · 204 · 304。头读完就收尾。
    Empty {
        over: bool,
    },
    /// `Content-Length` 定长：还差几个字节。
    Length {
        left: u64,
        over: bool,
    },
    /// 既不定长也不分块：读到对端关为止，收尾判不出。
    UntilClose,
    Chunked(ChunkedView),
}

impl BodyView {
    /// 按应答头选：分块 ⇒ `Chunked`；有读得懂的 `Content-Length` ⇒ `Length`；都没有 ⇒ `UntilClose`。
    pub fn for_response(headers: &[(String, String)]) -> Self {
        let chunked = headers.iter().any(|(k, v)| {
            k.eq_ignore_ascii_case("transfer-encoding")
                && v.to_ascii_lowercase().contains("chunked")
        });
        if chunked {
            return BodyView::Chunked(ChunkedView::default());
        }
        let length = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, v)| v.trim().parse::<u64>().ok());
        match length {
            Some(left) => BodyView::Length { left, over: false },
            None => BodyView::UntilClose,
        }
    }

    /// 同 [`BodyView::for_response`]，再按请求方法与状态码认出没有体的那几形（`HEAD` 的应答 · 204 · 304：带了 `Content-Length` 也没有体）。
    pub fn for_answer(method: &str, status: u16, headers: &[(String, String)]) -> Self {
        if method.eq_ignore_ascii_case("HEAD") || status == 204 || status == 304 {
            BodyView::Empty { over: false }
        } else {
            BodyView::for_response(headers)
        }
    }

    /// 喂一段原始字节，拿回其中的**解码后**载荷。收尾之后多出来的字节不算体（记下「多了」）。
    ///
    /// `cap` 是**攒着还没成形的那截**的上限，见 `ChunkedView::feed`。
    pub fn feed(&mut self, raw: &[u8], cap: usize) -> Vec<u8> {
        match self {
            BodyView::Empty { over } => {
                *over |= !raw.is_empty();
                Vec::new()
            }
            BodyView::Length { left, over } => {
                let take = usize::try_from(*left).unwrap_or(usize::MAX).min(raw.len());
                *left -= take as u64;
                *over |= take < raw.len();
                raw[..take].to_vec()
            }
            BodyView::UntilClose => raw.to_vec(),
            BodyView::Chunked(v) => v.feed(raw, cap),
        }
    }

    /// 这个应答的体收完了（定长的够数 · 分块的终止块连尾部头读完 · 没有体的那几形）。读到对端关为止的那一形永远答「没有」。
    pub fn ended(&self) -> bool {
        match self {
            BodyView::Empty { .. } => true,
            BodyView::Length { left, .. } => *left == 0,
            BodyView::UntilClose => false,
            BodyView::Chunked(v) => v.ended,
        }
    }

    /// 收在边上：收完了，而且收尾之后没有多出来的字节。只有这样，同一条连接上的下一个应答才从头开始。
    pub fn ended_cleanly(&self) -> bool {
        self.ended()
            && match self {
                BodyView::Empty { over } | BodyView::Length { over, .. } => !over,
                BodyView::UntilClose => false,
                BodyView::Chunked(v) => !v.over,
            }
    }

    /// 取走并清零「本视图因超 `cap` 丢掉的字节数」。**丢的只是 tee 那一路** ——
    /// 下游拿到的是上游原样的字节，一个都不经过本模块（见本类型头注）。
    pub fn take_dropped(&mut self) -> u64 {
        match self {
            BodyView::Chunked(v) => std::mem::take(&mut v.dropped),
            BodyView::Empty { .. } | BodyView::Length { .. } | BodyView::UntilClose => 0,
        }
    }
}

/// 增量 chunked 拆帧。**只拆不攒**：喂进来多少就尽量吐多少。
///
/// 攒字节、找块长度行交给 [`super::framer::LineFramer`]（`relay/` 里唯一的增量分帧器）
/// —— 这里只剩 chunked 自己的那一层：块长度、块尾 CRLF、终止块与它后面的尾部头、上限。
pub struct ChunkedView {
    pub framer: super::framer::LineFramer,
    /// 当前块还剩多少字节（含结尾的 `\r\n` 由 `crlf_left` 单管）。
    left: usize,
    crlf_left: usize,
    /// 读到了终止块，正在读尾部头（读到空行收尾）。
    trailers: bool,
    done: bool,
    /// 收尾了：终止块与尾部头都读完（超 `cap` 或块长度读不懂而停下的 `done` 不算）。
    ended: bool,
    /// 收尾之后又来了字节。
    over: bool,
    /// 超 `cap` 时丢掉的字节数（累计，由 `BodyView::take_dropped` 取走）。
    dropped: u64,
}

impl Default for ChunkedView {
    fn default() -> Self {
        Self {
            framer: super::framer::LineFramer::new(b"\r\n"),
            left: 0,
            crlf_left: 0,
            trailers: false,
            done: false,
            ended: false,
            over: false,
            dropped: 0,
        }
    }
}

impl ChunkedView {
    /// # `cap` 管的是**攒着还没成形的那截**
    ///
    /// `left > 0` 那一支每次都把能拿的**全部**吐出去，分帧器里不会累积；
    /// 真正会无界涨的只有**块长度行还没读到 `\r\n`** 那一支 —— 它把收到的一切原样留在分帧器里。
    /// 一个坏掉/有敌意的上游发一条**永不结束的块长度行**就能让它一直涨。
    ///
    /// ⚠ 它与 `阻-1` **不同族，别混**：`阻-1` 是「拿外部给的**一个数**去分配」（攻击方一个字节
    /// 都不用发），这一条是「按**真实收到的字节**增长」（要涨到 N 就得真发 N 字节）。
    /// 严重度差一档，但同样是「外部输入决定内存上界」⇒ 一起收口。
    ///
    /// 超了怎么办：**丢掉攒着的那截并计数**（`dropped`），解码就此收工（`done = true`）——
    /// tee 少一段，**下游的字节一个不少**。计数由 `server.rs::serve_one` 取走交给 tee
    /// （在 tap 上占一个号不发）⇒ **不是静默丢**。
    fn feed(&mut self, raw: &[u8], cap: usize) -> Vec<u8> {
        if self.ended {
            self.over |= !raw.is_empty();
            return Vec::new();
        }
        self.framer.push(raw);
        let pending = self.framer.pending();
        if pending > cap {
            self.dropped += pending as u64;
            self.framer.skip(pending);
            self.done = true;
            return Vec::new();
        }
        let mut out = Vec::new();
        loop {
            if self.done {
                break;
            }
            // 终止块之后：尾部头一行一行读掉（不进 tee），读到空行收尾；收尾之后还剩字节 ⇒ 多了。
            if self.trailers {
                let Some(line) = self.framer.next_line() else {
                    break;
                };
                if line.is_empty() {
                    self.done = true;
                    self.ended = true;
                    self.over = self.framer.pending() > 0;
                }
                continue;
            }
            if self.crlf_left > 0 {
                let take = self.crlf_left.min(self.framer.pending());
                self.framer.skip(take);
                self.crlf_left -= take;
                if self.crlf_left > 0 {
                    break;
                }
                continue;
            }
            if self.left > 0 {
                let data = self.framer.take(self.left);
                out.extend_from_slice(data);
                self.left -= data.len();
                if self.left == 0 {
                    self.crlf_left = 2;
                }
                if self.framer.pending() == 0 {
                    break;
                }
                continue;
            }
            // 读块长度行
            let Some(line) = self.framer.next_line() else {
                break;
            };
            let line = String::from_utf8_lossy(line).to_string();
            let size_hex = line.split(';').next().unwrap_or("").trim().to_string();
            match usize::from_str_radix(&size_hex, 16) {
                Ok(0) => self.trailers = true,
                Ok(n) => self.left = n,
                Err(_) => {
                    self.done = true;
                    break;
                }
            }
        }
        out
    }
}

/// 从一个已经建立的连接上读响应头部（与请求头同款逐字节读，理由相同）。
pub fn read_response_head<R: Read>(r: &mut R, cap: usize) -> std::io::Result<Option<Vec<u8>>> {
    read_head(r, cap)
}

/// 状态行里的三位数字（`HTTP/1.1 429 Too Many Requests` ⇒ 429）；不是三位数字 ⇒ `None`。
pub fn status_code(status_line: &str) -> Option<u16> {
    let code = status_line.split(' ').nth(1)?;
    (code.len() == 3 && code.bytes().all(|b| b.is_ascii_digit()))
        .then(|| code.parse().ok())
        .flatten()
}

/// 解析响应头部，返回 `(状态行, 头表)`。
pub fn parse_response(raw: &[u8]) -> Option<(String, Vec<(String, String)>)> {
    let text = std::str::from_utf8(raw).ok()?;
    let mut lines = text.split("\r\n");
    let status = lines.next()?.to_string();
    if !status.starts_with("HTTP/1.") {
        return None;
    }
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        headers.push((k.to_string(), v.trim().to_string()));
    }
    Some((status, headers))
}

/// 状态行是不是 **1xx 中间响应**。
///
/// 1xx 是「还没完，后面还有一条真的」，**不是最终响应**。判法：状态行的第 2 段是
/// **恰好三位数字且首位是 `1`**。
///
/// ⚠ 它**排除**了什么，写清楚：
/// - `HTTP/1.1 1` / `HTTP/1.1 1000` / `HTTP/1.1 1xx` 这类**不是三位数字**的一律**不算** 1xx
///   —— 那种状态行本来就是畸形的，交给调用方按「最终响应」往下走、由后续解析去红，
///   总好过在这里替它猜。
/// - 它**不判**这条 1xx 是哪一种（100 / 101 / 103），调用方对 1xx 一视同仁（丢弃再读下一条）；
///   `101` 的结局单独写在 `server.rs::serve_one` 那段头注里。
pub fn is_interim_status(status_line: &str) -> bool {
    let Some(code) = status_line.split(' ').nth(1) else {
        return false;
    };
    code.len() == 3 && code.starts_with('1') && code.bytes().all(|b| b.is_ascii_digit())
}

/// 把一个实现了 `BufRead` 的流读满 `n` 字节（请求体用；`n` 由 `Content-Length` 给）。
///
/// # 三张脸（口径写死在这里，别改）
///
/// | 返回 | 什么时候 | 调用方该怎么办 |
/// |---|---|---|
/// | `Ok(None)` | **`n > cap`** —— 唯一来源 | 回 **413 Payload Too Large**，且**一个字节都没读过**（连接上还压着那 `n` 字节，直接关） |
/// | `Ok(Some(v))` | 真读满了 `n` 字节 | 正常转发 |
/// | `Err(UnexpectedEof)` | 流在读满之前就结束 | 当连接错误处理 |
///
/// ⚠ 这里的 `None` 与 `read_head` 的 `None` **不同族**：那边一个 `None` 装了**两个**来源
/// （超上限 · 提前 EOF，它自己的头注登记着这一点），这里**只有一个**。
///
/// # ★★ 先判 `n > cap`，再按 `n` 分配
///
/// `n` 来自 `content_length()`，值域是 **`usize` 全域，没有任何上界**（`HEAD_CAP` 只管头**字节数**，管不到它）。
/// 先按 `n` 分配的话，一条 `Content-Length: 1000000000000` 的下游请求 ⇒ 分配失败 ⇒ `std::alloc::handle_alloc_error`
/// ⇒ **abort（SIGABRT）**。**abort 不走 unwind** ⇒ `panic = "unwind"` 与 `catch_unwind`
/// 一概接不住；而这里的形状是「**一个进程**服务 N 个会话」
/// ⇒ 这一发打掉的不是一条连接，是**当时所有会话的在途流**。
///
/// 两道：
/// 1. **`n > cap` 当场拒收**，一个字节不读、一个字节不分配；
/// 2. `n <= cap` ⇒ 按 `n` **一次分配**（`Vec::with_capacity(n)`），读满为止，不按读到的字节翻倍增长
///    （翻倍的话容量最多到体长的两倍，长的那一下新旧两块同时在手里）。
///    「只声明 `Content-Length: <cap>`、一个字节不发」逼出的那一块，由调用方读体之前记的那一笔账挡着
///    （`server.rs` 的 `BodyBudget`：在途的体合计有上限，记不下就不读）。
///
/// 死值验与射程见 `an_oversized_content_length_is_refused_without_allocating_it` 与件文件 §8.20.2。
pub fn read_exact_body<R: BufRead>(
    r: &mut R,
    n: usize,
    cap: usize,
) -> std::io::Result<Option<Vec<u8>>> {
    if n > cap {
        return Ok(None);
    }
    let mut body = Vec::with_capacity(n);
    r.by_ref().take(n as u64).read_to_end(&mut body)?;
    if body.len() != n {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            copy_text("beHttp1.readExactBody.shortBody", &[]),
        ));
    }
    Ok(Some(body))
}

#[cfg(test)]
#[path = "../../../tests/comms/outward/http1_tests.rs"]
mod tests;
