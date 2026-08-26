//! 手写的最小 HTTP/1.1 —— **只解析中转必须懂的那几样**，别的一律当字节。
//!
//! `K8`/`D4` 那条「优先手写最小 HTTP，别引框架」的白名单在 `裁-1` 里**一个字没松**：
//! 这里没有任何 HTTP 库，只有请求行 + 头 + `Content-Length` + chunked 拆帧。

use std::io::{BufRead, Read};

/// `Content-Length` 读出来的三张脸。**刻意不是 `Option<usize>`** —— 理由见
/// `RequestHead::content_length` 的头注（「没有这个头」与「读不懂」挤在同一个 `None` 里
/// 会让请求体被静默丢掉）。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BodyLen {
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
pub(crate) struct RequestHead {
    pub(crate) method: String,
    pub(crate) target: String,
    pub(crate) headers: Vec<(String, String)>,
}

impl RequestHead {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// 请求体长度。**只认 `Content-Length`**：chunked 的请求体本中转不接
    /// （claude 发的是带 `Content-Length` 的 `POST`）。不认的形状由调用方回 4xx。
    ///
    /// ★★ **返回三值，不是 `Option`**〔回修轮之五 08-25，D3 `重要-2(D3)`〕：
    /// 先前这里是 `…parse().ok()`，于是「**没有这个头**」与「**有这个头但读不懂**」
    /// 挤进了同一个 `None`，而调用点把 `None` 当成「没有请求体」
    /// ⇒ 一发 `Content-Length: 7abc` + 7 字节体 ⇒ **请求体被静默丢掉**、
    /// 上游收到空体、下游拿到一条正常的 200，全程零日志零 4xx（D3 实测，住址 `audits/K-H1-D3.md#6.1`）。
    /// 中转搬的正是 `POST /v1/messages` 的载荷，丢了它 claude 当场坏而门禁全绿。
    /// ⇒ 今天两件事分成两张脸，`Unparsable` 由调用方回 **400**。
    pub(crate) fn content_length(&self) -> BodyLen {
        let Some(raw) = self.header("content-length") else {
            return BodyLen::Absent;
        };
        match raw.trim().parse::<usize>() {
            Ok(n) => BodyLen::Exact(n),
            Err(_) => BodyLen::Unparsable,
        }
    }

    pub(crate) fn is_chunked_body(&self) -> bool {
        self.header("transfer-encoding")
            .is_some_and(|v| v.to_ascii_lowercase().contains("chunked"))
    }
}

/// **逐字节**读到 `\r\n\r\n` 为止。不用 `BufReader::read_line`：中转要在读完头之后
/// 把**剩下的字节一个不差**地当请求体，而带缓冲的读会把请求体的头几字节吞进缓冲区。
///
/// `cap` 是头部字节上限，超了返回 `None`（而不是无限吃内存）。
///
/// ⚠ 订正〔回修轮 08-25，承接件文件 `判不了-6` / D1 `建议-9`〕：先前这里逐字写「回 **431**」，
/// **盘上没有 431**。`None` 有**两个**来源（超上限 · 头没读完就 EOF），本函数**不区分**它们，
/// 而两个调用点各自按自己的语境回：请求那一侧 `server.rs` 回 **400 Bad Request**，
/// 响应那一侧回 **502 Bad Gateway**。要真回 431 得先让本函数把两个来源分开。
pub(crate) fn read_head<R: Read>(r: &mut R, cap: usize) -> std::io::Result<Option<Vec<u8>>> {
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
pub(crate) fn parse_request(raw: &[u8]) -> Option<RequestHead> {
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

pub(crate) fn is_hop_by_hop(name: &str) -> bool {
    HOP_BY_HOP.iter().any(|h| name.eq_ignore_ascii_case(h))
}

/// 响应体的**解码视图** —— 只喂 tee，**不影响下游拿到的字节**。
///
/// 下游拿到的永远是上游原样的字节（含 chunked 分帧）；tee 要看见 SSE 文本，
/// 所以这里把 chunked 拆掉。两条路吃的是同一批字节，但**下游那条不经过这里**。
pub(crate) enum BodyView {
    Identity,
    Chunked(ChunkedView),
}

impl BodyView {
    pub(crate) fn for_response(headers: &[(String, String)]) -> Self {
        let chunked = headers.iter().any(|(k, v)| {
            k.eq_ignore_ascii_case("transfer-encoding")
                && v.to_ascii_lowercase().contains("chunked")
        });
        if chunked {
            BodyView::Chunked(ChunkedView::default())
        } else {
            BodyView::Identity
        }
    }

    /// 喂一段原始字节，拿回其中的**解码后**载荷。
    ///
    /// `cap` 是**攒着还没成形的那截**的上限，见 `ChunkedView::feed`。
    pub(crate) fn feed(&mut self, raw: &[u8], cap: usize) -> Vec<u8> {
        match self {
            BodyView::Identity => raw.to_vec(),
            BodyView::Chunked(v) => v.feed(raw, cap),
        }
    }

    /// 取走并清零「本视图因超 `cap` 丢掉的字节数」。**丢的只是 tee 那一路** ——
    /// 下游拿到的是上游原样的字节，一个都不经过本模块（见本类型头注）。
    pub(crate) fn take_dropped(&mut self) -> u64 {
        match self {
            BodyView::Identity => 0,
            BodyView::Chunked(v) => std::mem::take(&mut v.dropped),
        }
    }
}

/// 增量 chunked 拆帧。**只拆不攒**：喂进来多少就尽量吐多少。
#[derive(Default)]
pub(crate) struct ChunkedView {
    buf: Vec<u8>,
    /// 当前块还剩多少字节（含结尾的 `\r\n` 由 `crlf_left` 单管）。
    left: usize,
    crlf_left: usize,
    done: bool,
    /// 超 `cap` 时丢掉的字节数（累计，由 `BodyView::take_dropped` 取走）。
    dropped: u64,
}

impl ChunkedView {
    /// # `cap` 管的是**攒着还没成形的那截**〔回修轮之五 08-25，`阻-1(D3)` 的同职面〕
    ///
    /// `left > 0` 那一支每次都把能拿的**全部**吐出去，`buf` 不会累积；
    /// 真正会无界涨的只有**块长度行还没读到 `\r\n`** 那一支 —— 它把收到的一切原样留在 `buf` 里。
    /// 一个坏掉/有敌意的上游发一条**永不结束的块长度行**就能让它一直涨。
    ///
    /// ⚠ 它与 `阻-1` **不同族，别混**：`阻-1` 是「拿外部给的**一个数**去分配」（攻击方一个字节
    /// 都不用发），这一条是「按**真实收到的字节**增长」（要涨到 N 就得真发 N 字节）。
    /// 严重度差一档，但同样是「外部输入决定内存上界」⇒ 一起收口。
    ///
    /// 超了怎么办：**丢掉攒着的那截并计数**（`dropped`），解码就此收工（`done = true`）——
    /// tee 少一段，**下游的字节一个不少**。计数由 `server.rs::handle` 取走并写进 tee 流的
    /// `__dropped__` 行 ⇒ **不是静默丢**。
    fn feed(&mut self, raw: &[u8], cap: usize) -> Vec<u8> {
        self.buf.extend_from_slice(raw);
        if self.buf.len() > cap {
            self.dropped += self.buf.len() as u64;
            self.buf.clear();
            self.done = true;
            return Vec::new();
        }
        let mut out = Vec::new();
        loop {
            if self.done {
                break;
            }
            if self.crlf_left > 0 {
                let take = self.crlf_left.min(self.buf.len());
                self.buf.drain(..take);
                self.crlf_left -= take;
                if self.crlf_left > 0 {
                    break;
                }
                continue;
            }
            if self.left > 0 {
                let take = self.left.min(self.buf.len());
                out.extend_from_slice(&self.buf[..take]);
                self.buf.drain(..take);
                self.left -= take;
                if self.left == 0 {
                    self.crlf_left = 2;
                }
                if self.buf.is_empty() {
                    break;
                }
                continue;
            }
            // 读块长度行
            let Some(pos) = self.buf.windows(2).position(|w| w == b"\r\n") else {
                break;
            };
            let line = String::from_utf8_lossy(&self.buf[..pos]).to_string();
            self.buf.drain(..pos + 2);
            let size_hex = line.split(';').next().unwrap_or("").trim().to_string();
            match usize::from_str_radix(&size_hex, 16) {
                Ok(0) => {
                    self.done = true;
                    break;
                }
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
pub(crate) fn read_response_head<R: Read>(
    r: &mut R,
    cap: usize,
) -> std::io::Result<Option<Vec<u8>>> {
    read_head(r, cap)
}

/// 解析响应头部，返回 `(状态行, 头表)`。
pub(crate) fn parse_response(raw: &[u8]) -> Option<(String, Vec<(String, String)>)> {
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

/// 状态行是不是 **1xx 中间响应**〔回修轮之五 08-25，D3 `重要-1(D3)`〕。
///
/// 1xx 是「还没完，后面还有一条真的」，**不是最终响应**。判法：状态行的第 2 段是
/// **恰好三位数字且首位是 `1`**。
///
/// ⚠ 它**排除**了什么，写清楚：
/// - `HTTP/1.1 1` / `HTTP/1.1 1000` / `HTTP/1.1 1xx` 这类**不是三位数字**的一律**不算** 1xx
///   —— 那种状态行本来就是畸形的，交给调用方按「最终响应」往下走、由后续解析去红，
///   总好过在这里替它猜。
/// - 它**不判**这条 1xx 是哪一种（100 / 101 / 103），调用方对 1xx 一视同仁（丢弃再读下一条）；
///   `101` 的结局单独写在 `server.rs::handle` 那段头注里。
pub(crate) fn is_interim_status(status_line: &str) -> bool {
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
/// # ★★ 为什么不再是 `vec![0u8; n]`〔回修轮之五 08-25，D3 `阻-1(D3)`〕
///
/// 先前这一行是 `let mut body = vec![0u8; n];`，而 `n` 来自 `content_length()`，
/// 值域是 **`usize` 全域，没有任何上界**（`HEAD_CAP` 只管头**字节数**，管不到它）。
/// 一条 `Content-Length: 1000000000000` 的下游请求 ⇒ 分配失败 ⇒ `handle_alloc_error`
/// ⇒ **abort（SIGABRT）**。**abort 不走 unwind** ⇒ `panic = "unwind"` 与 `catch_unwind`
/// 一概接不住；而本件的形状是「**一个进程**服务 N 个会话」（`K9` 裁定二第 1 条）
/// ⇒ 这一发打掉的不是一条连接，是**当时所有会话的在途流**。
/// 我自己重打过这一刀，逐字读数（`memory allocation of 1000000000000 bytes failed` ·
/// `signal: 6, SIGABRT` · 退出码 101 · 判定行 **0**）见件文件 §8.20.1。
///
/// 今天两道，缺一不可：
/// 1. **`n > cap` 当场拒收**，一个字节不读、一个字节不分配；
/// 2. 即便 `n <= cap`，也**按真的读到的字节增长**（`take(n).read_to_end`），**不按 `n` 预分配**
///    —— 否则一条「只声明 `Content-Length: <cap>`、一个字节不发」的请求照样能逼出
///    `cap` 那么大的一块内存，而攻击方一个字节的代价都没付。
///
/// 死值验与射程见 `an_oversized_content_length_is_refused_without_allocating_it` 与件文件 §8.20.2。
pub(crate) fn read_exact_body<R: BufRead>(
    r: &mut R,
    n: usize,
    cap: usize,
) -> std::io::Result<Option<Vec<u8>>> {
    if n > cap {
        return Ok(None);
    }
    let mut body = Vec::new();
    r.by_ref().take(n as u64).read_to_end(&mut body)?;
    if body.len() != n {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "请求体比 Content-Length 声明的短",
        ));
    }
    Ok(Some(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 「这条判据不测上限那一格」的写法：给一个**永远触发不了**的上限。
    /// ⚠ 刻意不写成 `TEE_DECODE_CAP` / `BODY_CAP` —— 拿被测的那个常量当期望值，
    /// 判据就跟着它一起漂（本仓的「期望值必须手写」同一条纪律）。
    const NO_CAP: usize = usize::MAX;

    /// ★★ `阻-1(D3)`：**一个数就能把整个中转进程 abort 掉**这一格，今天有牙。
    ///
    /// # 它钉的两件事，缺一不可
    ///
    /// ㈠ **超上限要拒收**（`Ok(None)` ⇒ 调用方回 413）。死值验用的是 `BODY_CAP + 1`，
    ///    不是 `1e12` —— 后者在**没有上限**的版本上会让进程 **SIGABRT**，那是 **CRASH 不是红**
    ///    （判定行掉成 0），死值验拿不到「恰好这一格红」的读数。⇒ 用一个「超了但分配得动」的值。
    ///
    /// ㈡ **一个字节都不许按 `n` 分配**。这一格用 `alloc_probe`（**线程级**分配高水位量具，
    ///    住 `crate::alloc_probe`，`VmHWM` 是进程级的、会把邻居测试算进来 —— 那份头注写着来历）。
    ///    没有 ㈡ 的话，「先 `vec![0u8; n]` 再判 `n > cap`」这种写法照样过 ㈠，
    ///    而它**仍然会 abort** —— 顺序错一行就前功尽弃，而 ㈠ 看不见顺序。
    ///
    /// # 分母与非空对照
    ///
    /// - `1e12` 那一形：**必须**一个字节不分配（阈值 1 MiB，比它小 6 个数量级）。
    /// - 非空对照：同一把尺子量一条**正常**的读（8 MiB 的体）⇒ 高水位**必须**涨到 8 MiB 以上。
    ///   没有它，「峰值 = 0」可能只是量具坏了（那正是 `alloc_probe` 头注里逐字警告的
    ///   「源码落地不等于效果落地」）。
    #[test]
    fn an_oversized_content_length_is_refused_without_allocating_it() {
        const CAP: usize = 4 * 1024 * 1024; // 手写字面量，不引 BODY_CAP
        const HUGE: usize = 1_000_000_000_000;

        // ㈠ 超上限 ⇒ 拒收，且**一个字节都没从流里读走**。
        let src: &[u8] = b"hi";
        let mut r = std::io::Cursor::new(src);
        let base = crate::alloc_probe::reset_peak();
        let got = read_exact_body(&mut r, HUGE, CAP).expect("超上限不是 IO 错误，是一个答案");
        let peak = crate::alloc_probe::peak_since(base);
        assert!(got.is_none(), "超 cap 必须拒收（回 None ⇒ 调用方回 413）");
        assert_eq!(r.position(), 0, "拒收那一支不许从流里读走任何字节");
        assert!(
            peak < 1024 * 1024,
            "拒收那一支的本线程分配高水位是 {peak} 字节 —— 它不许随 `n` 走（n = {HUGE}）"
        );

        // ㈡ 刚好在上限上 ⇒ 收（边界是 `>`，不是 `>=`）。
        let body = vec![b'x'; CAP];
        let mut r = std::io::Cursor::new(&body[..]);
        let got = read_exact_body(&mut r, CAP, CAP).expect("io");
        assert_eq!(got.map(|v| v.len()), Some(CAP), "`n == cap` 必须收，边界是 `>`");

        // ㈢ 非空对照：**正常**的读真的会把高水位顶上去 —— 否则上面那条「峰值不涨」是空真。
        let body = vec![b'y'; 8 * 1024 * 1024];
        let mut r = std::io::Cursor::new(&body[..]);
        let base = crate::alloc_probe::reset_peak();
        let got = read_exact_body(&mut r, body.len(), CAP * 4).expect("io");
        let peak = crate::alloc_probe::peak_since(base);
        assert_eq!(got.map(|v| v.len()), Some(8 * 1024 * 1024));
        assert!(
            peak >= 8 * 1024 * 1024,
            "非空对照：真读 8 MiB 时高水位只有 {peak} 字节 —— 量具没在量这条路"
        );

        // ㈣ 流比声明的短 ⇒ `Err(UnexpectedEof)`，**不是** `Ok(None)`（那是超上限**独占**的答案）。
        let mut r = std::io::Cursor::new(&b"abc"[..]);
        let e = read_exact_body(&mut r, 10, CAP).expect_err("短流必须是错误");
        assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof);
    }

    /// ★ `重要-2(D3)`：`Content-Length` 的三张脸必须分得开。
    ///
    /// 分母 = 我列出的这 **7** 形。先前实现是 `…parse().ok()`，
    /// 于是下表 `Unparsable` 那 4 形与 `Absent` 那 1 形**挤在同一个 `None` 里**
    /// ⇒ 调用点把它们一律当成「没有请求体」，请求体被静默丢掉。
    #[test]
    fn content_length_tells_absent_apart_from_unparsable() {
        let head = |h: &str| {
            parse_request(format!("POST /x HTTP/1.1\r\n{h}\r\n\r\n").as_bytes()).expect("parse")
        };
        // 期望值全是手写字面量。
        assert_eq!(head("Host: x").content_length(), BodyLen::Absent, "没有这个头");
        assert_eq!(head("Content-Length: 7").content_length(), BodyLen::Exact(7));
        assert_eq!(
            head("Content-Length:   7  ").content_length(),
            BodyLen::Exact(7),
            "两端空白要吃掉"
        );
        for bad in ["7abc", "abc", "-1", "7, 7"] {
            assert_eq!(
                head(&format!("Content-Length: {bad}")).content_length(),
                BodyLen::Unparsable,
                "`{bad}` 必须是**读不懂**，不许退化成「没有请求体」"
            );
        }
    }

    /// ★ `重要-1(D3)` 的**判别器**那一格。分母 = 我列出的这 **9** 形。
    #[test]
    fn only_a_three_digit_1xx_status_counts_as_interim() {
        for yes in ["HTTP/1.1 100 Continue", "HTTP/1.1 103 Early Hints", "HTTP/1.0 101"] {
            assert!(is_interim_status(yes), "{yes} 该算 1xx 中间响应");
        }
        for no in [
            "HTTP/1.1 200 OK",
            "HTTP/1.1 500 Internal Server Error",
            "HTTP/1.1 1",     // 不是三位
            "HTTP/1.1 1000",  // 不是三位
            "HTTP/1.1 1xx",   // 不是三位数字
            "HTTP/1.1",       // 根本没有第二段
        ] {
            assert!(!is_interim_status(no), "{no} 不该算 1xx 中间响应");
        }
    }

    #[test]
    fn parses_a_post_with_headers() {
        let raw = b"POST /s/agentA/sid/v1/messages?beta=true HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\nAuthorization: Bearer T\r\n\r\n";
        let h = parse_request(raw).expect("应当解析成功");
        assert_eq!(h.method, "POST");
        assert_eq!(h.target, "/s/agentA/sid/v1/messages?beta=true");
        assert_eq!(h.content_length(), BodyLen::Exact(3));
        assert_eq!(
            h.header("AUTHORIZATION"),
            Some("Bearer T"),
            "头名大小写不敏感"
        );
        assert!(!h.is_chunked_body());
    }

    #[test]
    fn rejects_malformed_request_lines() {
        // 分母 = 我列出的这 5 形。
        for bad in [
            &b"GET\r\n\r\n"[..],
            &b"GET /x\r\n\r\n"[..],
            &b"GET /x FTP/1.0\r\n\r\n"[..],
            &b"GET x HTTP/1.1\r\n\r\n"[..],
            &b"GET /x HTTP/1.1\r\nbad header\r\n\r\n"[..],
        ] {
            assert!(parse_request(bad).is_none(), "这一形不该被接受");
        }
    }

    #[test]
    fn read_head_stops_exactly_at_the_blank_line() {
        let mut src = std::io::Cursor::new(b"GET /x HTTP/1.1\r\nA: b\r\n\r\nBODYBYTES".to_vec());
        let head = read_head(&mut src, 4096).expect("io").expect("有头");
        assert_eq!(head.len(), "GET /x HTTP/1.1\r\nA: b\r\n\r\n".len());
        // ★ 这条是要害：读头**不许多吃一个字节**，否则请求体会缺头。
        let mut rest = Vec::new();
        std::io::Read::read_to_end(&mut src, &mut rest).expect("io");
        assert_eq!(rest, b"BODYBYTES");
    }

    /// 数「**真的**从源里读走了多少字节」的读源。
    ///
    /// `read_head` 的两个 `Ok(None)` 出口**返回值本身分不开** ⇒ 想让「上限」那道门
    /// 有自己的判据，只能量它**消耗了多少**。
    struct CountingReader {
        inner: std::io::Cursor<Vec<u8>>,
        consumed: usize,
    }

    impl Read for CountingReader {
        fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
            let n = self.inner.read(b)?;
            self.consumed += n;
            Ok(n)
        }
    }

    /// ★ **上限那道门，单断**（回修轮之三，承接 D1 `重要-1`）。
    ///
    /// `read_head` 有**两个** `Ok(None)` 出口 —— ①超上限 ②头没读完就 EOF ——
    /// 而**返回值本身分不开它们**。先前那一条判据（`head_cap_is_enforced`）只断 `is_none()`，
    /// 于是两个出口**互相兜底**：上限整个失效、读到 EOF 照样 `None` ⇒ **绿**（审计 `CE`）；
    /// EOF 出口改成 `Some`、上限照样先拦住 ⇒ 也**绿**（审计 `CE2`）；**只有两刀同切才红**（`CE3`）。
    /// ⇒ 它买到的是「目录级塌陷」，而名字说的是「enforced」〔`brief` 9：N 个独立源要 N 格单断〕。
    ///
    /// 这一条改断**它到底读走了多少字节**：上限拦住 ⇒ 只该消耗 `CAP` 个，
    /// **不是**把源里 9000 个全吃完。上限一旦失效，这个数当场对不上。
    #[test]
    fn the_cap_door_stops_the_read_at_exactly_cap_bytes() {
        const CAP: usize = 128;
        let mut src = CountingReader {
            inner: std::io::Cursor::new(vec![b'a'; 9000]),
            consumed: 0,
        };
        assert!(
            read_head(&mut src, CAP).expect("io").is_none(),
            "超上限的头不该被当成一个头返回"
        );
        assert_eq!(
            src.consumed, CAP,
            "上限拦住时只该消耗 {CAP} 字节；把源里 9000 字节全读完说明上限没生效"
        );
    }

    /// ★ **EOF 那道门，单断**。头**没有**空行就断流 ⇒ 不许当成一个头返回。
    ///
    /// 源**短于**上限 ⇒ 上限那道门这一趟根本不会触发 ⇒ 能让它红的只有 EOF 这一道。
    #[test]
    fn the_eof_door_refuses_a_head_that_never_terminates() {
        const CAP: usize = 128;
        // 源：**短于** `CAP` 且没有空行 ⇒ 上限那道门这一趟根本不触发。
        const SRC: &[u8] = b"GET /x HTTP/1.1\r\nA: b\r\n";
        let mut src = CountingReader {
            inner: std::io::Cursor::new(SRC.to_vec()),
            consumed: 0,
        };
        assert!(
            read_head(&mut src, CAP).expect("io").is_none(),
            "没读到空行就断流的头不该被当成一个头返回"
        );
        // 非空对照：这一趟**真的**是一路读到源尽头才停的（撞 EOF），不是撞上限停的。
        //
        // ⚠ 订正〔回修轮之四 08-25，D2 `建议-1`〕：先前这里写的是 `assert!(src.consumed < CAP)`
        // —— 源是手写字面量 **23** 字节、`CAP` 是同一个函数里的手写常量 **128**
        // ⇒ **结构性恒真**，没有任何生产改动能让它红（`relay/` 里同族的第三处；
        //   前两处是 §8.15.8 自查逮到的 `自-1`/`自-2`）。**动机（做非空对照）是对的，
        //   写法永远不会失败** —— 一条永远不会红的断言不是对照，是装饰。
        // 今天改成断**恰好等于源长度**，两个方向都真会红：
        //   · 上限那道门要是提前拦住（例如 `while buf.len() < cap` 被改小）⇒ 消耗量 < 源长 ⇒ **红**；
        //   · 夹具哪天被写长过 `CAP` ⇒ 上限先触发、消耗量 = `CAP` ≠ 源长 ⇒ 也**红**
        //     （夹具漂移正是先前那一条想守的东西，今天它真守得住了）。
        assert_eq!(
            src.consumed,
            SRC.len(),
            "这一趟必须一路读到源尽头才停（源 {} 字节，上限 {CAP}）",
            SRC.len()
        );
    }

    /// **全断对照** —— 两道门**至少有一道**拦住了。
    ///
    /// 它红 ⇒ 两道门**同时**坏了（审计 `CE3` 那一刀）。**单断哪一道它都不红**，
    /// 这正是它替不了上面那两条的原因 ⇒ 留着只作对照，**名字也不再说「enforced」**。
    #[test]
    fn an_over_long_head_never_comes_back_as_a_head() {
        let mut src = std::io::Cursor::new(vec![b'a'; 9000]);
        assert!(read_head(&mut src, 128).expect("io").is_none());
        // 非空对照：`read_head` 在**正常**的头上真的会返回 `Some` —— 不然上面那句是空真。
        let mut ok = std::io::Cursor::new(b"GET /x HTTP/1.1\r\nA: b\r\n\r\n".to_vec());
        assert!(read_head(&mut ok, 128).expect("io").is_some());
    }

    #[test]
    fn hop_by_hop_set_is_exactly_the_eight_we_named() {
        assert_eq!(HOP_BY_HOP.len(), 8, "改这张表要说明理由");
        assert!(is_hop_by_hop("Transfer-Encoding"));
        assert!(!is_hop_by_hop("Authorization"));
    }

    #[test]
    fn chunked_view_decodes_across_arbitrary_split_points() {
        let wire = b"5\r\nhello\r\n5\r\nworld\r\n0\r\n\r\n";
        // 逐字节喂：拆帧必须对**任意切点**成立，否则「逐块透传」一进来就碎。
        let mut v = ChunkedView::default();
        let mut out = Vec::new();
        for b in wire.iter() {
            out.extend_from_slice(&v.feed(&[*b], NO_CAP));
        }
        assert_eq!(out, b"helloworld");
        // 一次性喂：同样的答案
        let mut v2 = ChunkedView::default();
        assert_eq!(v2.feed(wire, NO_CAP), b"helloworld");
    }

    /// ★ `TEE_DECODE_CAP` 在 `ChunkedView` 这一侧的那一格〔回修轮之五 08-25，`阻-1(D3)` 同职面〕。
    ///
    /// 会无界涨的是**块长度行还没读到 `\r\n`** 那一支：上游发一条永不结束的长度行，
    /// `buf` 就把收到的一切原样留着。分母 = 我列出的这 **2** 形（超上限丢+计数 · 没超一个字节不丢）。
    #[test]
    fn an_endless_chunk_size_line_is_dropped_and_counted_instead_of_growing_forever() {
        const CAP: usize = 64; // 手写字面量，不引生产常量

        // 非空对照：没超上限时，正常的 chunked 流一个字节都不丢。
        let mut ok = BodyView::Chunked(ChunkedView::default());
        assert_eq!(ok.feed(b"5\r\nhello\r\n0\r\n\r\n", CAP), b"hello");
        assert_eq!(ok.take_dropped(), 0, "正常的流不许丢");

        // 正题：一条永不结束的块长度行。
        let mut v = BodyView::Chunked(ChunkedView::default());
        let endless = vec![b'a'; CAP * 2];
        assert!(v.feed(&endless, CAP).is_empty(), "读不出块长度就不该吐东西");
        let dropped = v.take_dropped();
        assert!(
            dropped >= CAP as u64,
            "丢了 {dropped} 字节 —— 超上限的那截必须被丢掉**并计数**（不许静默）"
        );
        assert_eq!(v.take_dropped(), 0, "取走之后账要清零，不许重复报");
    }

    #[test]
    fn body_view_picks_chunked_only_when_the_header_says_so() {
        let plain = vec![("Content-Type".to_string(), "text/event-stream".to_string())];
        assert!(matches!(BodyView::for_response(&plain), BodyView::Identity));
        let ch = vec![("Transfer-Encoding".to_string(), "chunked".to_string())];
        assert!(matches!(BodyView::for_response(&ch), BodyView::Chunked(_)));
    }

    #[test]
    fn parses_a_response_head() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n";
        let (status, headers) = parse_response(raw).expect("应当解析成功");
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert_eq!(headers.len(), 1);
    }
}
