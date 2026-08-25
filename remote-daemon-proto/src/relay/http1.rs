//! 手写的最小 HTTP/1.1 —— **只解析中转必须懂的那几样**，别的一律当字节。
//!
//! `K8`/`D4` 那条「优先手写最小 HTTP，别引框架」的白名单在 `裁-1` 里**一个字没松**：
//! 这里没有任何 HTTP 库，只有请求行 + 头 + `Content-Length` + chunked 拆帧。

use std::io::{BufRead, Read};

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
    pub(crate) fn content_length(&self) -> Option<usize> {
        self.header("content-length")?.trim().parse().ok()
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
    pub(crate) fn feed(&mut self, raw: &[u8]) -> Vec<u8> {
        match self {
            BodyView::Identity => raw.to_vec(),
            BodyView::Chunked(v) => v.feed(raw),
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
}

impl ChunkedView {
    fn feed(&mut self, raw: &[u8]) -> Vec<u8> {
        self.buf.extend_from_slice(raw);
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

/// 把一个实现了 `BufRead` 的流读满 `n` 字节（请求体用；`n` 由 `Content-Length` 给）。
pub(crate) fn read_exact_body<R: BufRead>(r: &mut R, n: usize) -> std::io::Result<Vec<u8>> {
    let mut body = vec![0u8; n];
    r.read_exact(&mut body)?;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_post_with_headers() {
        let raw = b"POST /s/agentA/sid/v1/messages?beta=true HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\nAuthorization: Bearer T\r\n\r\n";
        let h = parse_request(raw).expect("应当解析成功");
        assert_eq!(h.method, "POST");
        assert_eq!(h.target, "/s/agentA/sid/v1/messages?beta=true");
        assert_eq!(h.content_length(), Some(3));
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
        let mut src = CountingReader {
            inner: std::io::Cursor::new(b"GET /x HTTP/1.1\r\nA: b\r\n".to_vec()),
            consumed: 0,
        };
        assert!(
            read_head(&mut src, CAP).expect("io").is_none(),
            "没读到空行就断流的头不该被当成一个头返回"
        );
        // 非空对照：这一趟**真的**是撞 EOF 停的，不是撞上限停的（否则本条测的是另一道门）。
        assert!(
            src.consumed < CAP,
            "源只有 {} 字节，必须短于上限 {CAP}",
            src.consumed
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
            out.extend_from_slice(&v.feed(&[*b]));
        }
        assert_eq!(out, b"helloworld");
        // 一次性喂：同样的答案
        let mut v2 = ChunkedView::default();
        assert_eq!(v2.feed(wire), b"helloworld");
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
