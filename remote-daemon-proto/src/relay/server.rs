//! 中转本体：**一个进程**、只听回环、按路径前缀分流、逐块透传、同时 tee。

use super::http1::{self, BodyView, RequestHead};
use super::route;
use super::tee::{SseSplitter, TeeSink};
use super::upstream::{self, Base};
use std::io::{BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// 只听回环。**这是一个字面量常量，不是拼出来的** —— 拼出来的地址源码扫描看不见
/// （`DoD-4` 那条 acceptor 的第一个瞎法就是这个）。行为那半由 `DoD-4㈡` 兜底。
const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// 默认端口。形状抄 `control/cc_bus.rs` 的 `timeout_secs()`：**写死一个默认 + 环境变量能盖**。
/// 端口被占怎么办本仓零先例 ⇒ 本刀的处置是**起不来就退出并出声**，不自己换端口。
const DEFAULT_PORT: u16 = 8788;

const ENV_PORT: &str = "CCM_RELAY_PORT";
const ENV_UPSTREAM: &str = "CCM_RELAY_UPSTREAM";
const DEFAULT_UPSTREAM: &str = "https://api.anthropic.com";

/// 请求头部字节上限。
const HEAD_CAP: usize = 64 * 1024;
/// 每次从上游读多少 —— **上限**，不是「要凑满这么多」。
/// `std::io::Read::read` 本来就是「有多少给多少」，不循环凑满。
const READ_CHUNK: usize = 64 * 1024;

/// 中转的运行期状态。**一个进程一份**，跨连接共享。
pub(crate) struct Relay {
    base: Base,
    tee: TeeSink,
    /// 本进程服务过的请求数 —— `DoD-1㈢`「两个键由同一个中转进程服务」量的就是它。
    served: AtomicU64,
    /// 每条连接透传收尾时落一笔 —— `DoD-2` acceptor ㈡「下游读到的块数 ≈ 上游发出的块数」量的就是它。
    ///
    /// `Some(n)` = 干净 EOF 收尾，`n` 是**写给下游并 flush 成功的次数**；
    /// `None` = `pump` 以错误收尾（上游 RST 那一路）。
    ///
    /// ★ 这一格是回修轮补的。先前 `pump` 把块数**算出来了**（`Ok(writes)`），
    /// 而调用点写的是 `pump(...)?;` —— 返回值**整个丢掉，没有任何消费者**
    /// ⇒ 「块数对账」量得到、没人量。审计 `K4` 那一刀（只透第 1 块、其余攒到流末、
    /// 一个字节不丢）因此 384 条判据全绿，而它正是「TUI 出一个 token 然后卡住」那个形状。
    pumps: std::sync::Mutex<Vec<Option<u64>>>,
}

impl Relay {
    pub(crate) fn new(base: Base, tee: TeeSink) -> Self {
        Self {
            base,
            tee,
            served: AtomicU64::new(0),
            pumps: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// 消费 `pump` 的返回值。**生产段唯一的落点** —— 没有它，返回值就又成了死值。
    fn note_pump(&self, outcome: &std::io::Result<u64>) {
        if let Ok(mut g) = self.pumps.lock() {
            g.push(outcome.as_ref().ok().copied());
        }
    }

    /// 透传收尾账。**只给判据用** —— 生产路径不读它。
    #[cfg(test)]
    pub(crate) fn pumps(&self) -> Vec<Option<u64>> {
        self.pumps.lock().expect("lock").clone()
    }

    /// `DoD-1㈢` 的量点：本进程服务过几个请求。**只给判据用** ——
    /// 生产路径不读它（读了就成了「为了让守卫闭嘴而加的功能」）。
    #[cfg(test)]
    pub(crate) fn served(&self) -> u64 {
        self.served.load(Ordering::SeqCst)
    }
}

/// 起监听。返回真实绑定的地址（端口给 0 时由内核选，测试用）。
pub(crate) fn listen(port: u16) -> std::io::Result<TcpListener> {
    let listener = TcpListener::bind(SocketAddr::new(LOOPBACK, port))?;
    Ok(listener)
}

/// 接受循环。**阻塞在 `accept()` 上** —— 那是内核事件，不是定时器。
pub(crate) fn serve(listener: TcpListener, relay: Arc<Relay>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        let relay = Arc::clone(&relay);
        // 每连接一个线程。与参考实现同形（它用 ThreadingHTTPServer）。
        let _ = std::thread::Builder::new()
            .name("ccm-relay-conn".to_string())
            .spawn(move || {
                if let Err(e) = handle(stream, &relay) {
                    // ⚠ 只印错误本身，**永不印请求头**（`K9` 裁定四第 1 条）。
                    eprintln!("[relay] connection ended: {e}");
                }
            });
    }
}

fn respond_status(down: &mut TcpStream, status: &str) -> std::io::Result<()> {
    let body = format!("{status}\n");
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    down.write_all(head.as_bytes())?;
    down.write_all(body.as_bytes())?;
    down.flush()
}

/// 处理一条下游连接：解析 → 分流 → 连上游 → 逐块透传 + tee。
fn handle(down: TcpStream, relay: &Relay) -> std::io::Result<()> {
    down.set_nodelay(true)?;
    let mut down_w = down.try_clone()?;
    let mut down_r = BufReader::new(down);

    let Some(raw_head) = http1::read_head(&mut down_r, HEAD_CAP)? else {
        return respond_status(&mut down_w, "400 Bad Request");
    };
    let Some(head) = http1::parse_request(&raw_head) else {
        return respond_status(&mut down_w, "400 Bad Request");
    };
    if head.is_chunked_body() {
        return respond_status(&mut down_w, "411 Length Required");
    }
    let Some(r) = route::parse(&head.target) else {
        return respond_status(&mut down_w, "404 Not Found");
    };
    let body = match head.content_length() {
        Some(n) => http1::read_exact_body(&mut down_r, n)?,
        None => Vec::new(),
    };

    relay.served.fetch_add(1, Ordering::SeqCst);

    let mut up = match upstream::connect(&relay.base) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[relay] upstream connect failed: {e}");
            return respond_status(&mut down_w, "502 Bad Gateway");
        }
    };
    up.write_all(&render_upstream_request(
        &head,
        &r.rest,
        &relay.base,
        body.len(),
    ))?;
    if !body.is_empty() {
        up.write_all(&body)?;
    }
    up.flush()?;

    let Some(raw_resp) = http1::read_response_head(&mut up, HEAD_CAP)? else {
        return respond_status(&mut down_w, "502 Bad Gateway");
    };
    let Some((_status, headers)) = http1::parse_response(&raw_resp) else {
        return respond_status(&mut down_w, "502 Bad Gateway");
    };
    down_w.write_all(&rewrite_response_head(&raw_resp))?;
    down_w.flush()?;

    let mut view = BodyView::for_response(&headers);
    let mut splitter = SseSplitter::default();
    relay.tee.open(&r.agent, &r.key);
    // ★ 返回值**必须落地**：它是 `DoD-2㈡`「块数对账」的唯一量点。
    // 写成 `pump(...)?;` 就等于把它丢掉 —— 那正是审计 `K4` 能全绿的原因。
    let outcome = pump(&mut up, &mut down_w, &mut |raw| {
        for payload in splitter.feed(&view.feed(raw)) {
            relay.tee.event(&r.agent, &r.key, &payload);
        }
    });
    relay.note_pump(&outcome);
    outcome?;
    Ok(())
}

/// ★★ **本件的全部意义就在这个函数里。**
///
/// 一次 `read` 拿到多少，就立刻 `write_all` + `flush` 多少，然后才回头读下一次。
/// **绝不**先把响应体读完再写（那会让 TUI 卡住不出字）。
///
/// 它哪天会变瞎：只要有人在这里先攒一个 `Vec` 再一次性写出去，
/// **最终内容一模一样**，只有时序能分开两者 —— 所以它的 acceptor 量的是时序，不是内容。
/// 〔这句头注在 08-25 兑现了：审计切出的 `K4` 正是这一形，而当时 384 条判据全绿。
///  接住它的两条判据见 `every_chunk_reaches_the_client_before_upstream_sends_the_next_one`。〕
///
/// # 返回值的口径（别改这一条）
///
/// 返回的是**写给下游并 `flush` 成功的次数**，**不是**从上游读的次数。两者今天恒等
/// （读一块写一块），而**恰恰是它们分家的那一天**这个数才有用：先攒后写的实现
/// 读 N 次、只写 1 次 ⇒ 拿「读的次数」当返回值，对账那条判据就又瞎了。
fn pump<R: Read, W: Write>(
    up: &mut R,
    down: &mut W,
    on_chunk: &mut dyn FnMut(&[u8]),
) -> std::io::Result<u64> {
    let mut buf = vec![0u8; READ_CHUNK];
    let mut writes = 0u64;
    loop {
        let n = match up.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        down.write_all(&buf[..n])?;
        down.flush()?;
        writes += 1;
        on_chunk(&buf[..n]);
    }
    Ok(writes)
}

/// 渲染给上游的请求行 + 头。
///
/// - 路由前缀已被剥掉，`target` 是下游原样的**真路径 + 查询串**。
/// - 逐跳头不转发；`Host` 换成上游的。
/// - `Accept-Encoding` 收窄成 `identity`（见 `super` 头注㈢）。
/// - ⚠ **其余头一律原样转发，包括 auth 头** —— 转发但**不记录**（`K11` 裁定一）。
fn render_upstream_request(
    head: &RequestHead,
    target: &str,
    base: &Base,
    body_len: usize,
) -> Vec<u8> {
    let mut out = format!("{} {} HTTP/1.1\r\n", head.method, target);
    out.push_str(&format!("Host: {}\r\n", base.host_header()));
    out.push_str("Accept-Encoding: identity\r\n");
    out.push_str("Connection: close\r\n");
    for (k, v) in &head.headers {
        if http1::is_hop_by_hop(k)
            || k.eq_ignore_ascii_case("host")
            || k.eq_ignore_ascii_case("accept-encoding")
            || k.eq_ignore_ascii_case("content-length")
        {
            continue;
        }
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    if body_len > 0 {
        out.push_str(&format!("Content-Length: {body_len}\r\n"));
    }
    out.push_str("\r\n");
    out.into_bytes()
}

/// 响应头**逐字节原样**回给下游，只把连接管理那一条换成 `close`。
/// 保留 `Transfer-Encoding` 是有意的：响应体是原样透传的，分帧不能丢。
fn rewrite_response_head(raw: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(raw);
    let mut out = String::with_capacity(text.len() + 24);
    for (i, line) in text.split("\r\n").enumerate() {
        if line.is_empty() {
            break;
        }
        if i > 0 {
            let lower = line.to_ascii_lowercase();
            if lower.starts_with("connection:") || lower.starts_with("keep-alive:") {
                continue;
            }
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    out.push_str("Connection: close\r\n\r\n");
    out.into_bytes()
}

/// `--relay` 的入口。配置面只有环境变量（daemon 今天没有配置文件面）。
pub(crate) fn run(_args: &[String]) -> i32 {
    let port = std::env::var(ENV_PORT)
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    let raw_base = std::env::var(ENV_UPSTREAM).unwrap_or_else(|_| DEFAULT_UPSTREAM.to_string());
    let Some(base) = Base::parse(&raw_base) else {
        eprintln!("[relay] bad upstream base url");
        return 2;
    };
    let listener = match listen(port) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[relay] cannot bind loopback port {port}: {e}");
            return 2;
        }
    };
    match listener.local_addr() {
        Ok(a) => eprintln!("[relay] listening on {a}"),
        Err(e) => eprintln!("[relay] listening (addr unknown: {e})"),
    }
    serve(listener, Arc::new(Relay::new(base, TeeSink::to_stdout())));
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
    use std::sync::mpsc;

    /// 假上游发几个事件块。终止块另算 ⇒ 一条响应的**块数** = `UPSTREAM_EVENTS + 1`。
    const UPSTREAM_EVENTS: usize = 3;

    /// 下游打过去的请求体。**期望值是手写字面量**，判据不许拿被测代码算它（那样自证、恒绿）。
    const REQUEST_BODY: &str = "{\"m\":1}";

    /// 一个最小假上游。**它只监听回环，全程没有一个字节出本机。**
    ///
    /// # `gate`：**每一块**都要等下游确认（回修轮改的）
    ///
    /// 给了 `gate` 时，假上游发**每一块**之前都先 `recv()` 一次。
    /// ⇒ 中转只要攒住任何一块，下游就再也等不到下一块，测试在读期限上红。
    /// 〔旧版只在**第 1 块之后**卡一次门闩 ⇒ 「第 1 块立刻透、其余攒到流末」那一形
    ///  一条判据都撞不上（审计 `K4` 实测 384 全绿）。那正是 `K9` 裁定四要防的形状。〕
    struct FakeUpstream {
        addr: SocketAddr,
        seen: Arc<std::sync::Mutex<Vec<String>>>,
        /// 上游**真的收到**的请求体，逐连接一条。
        bodies: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
        /// 上游**真的发出**的响应体块数（每块一次 `write_all` + `flush`）。
        /// 「上游发出的块数」是这个数，**不是**判据里写死的常量。
        sent: Arc<AtomicU64>,
    }

    impl FakeUpstream {
        fn sent(&self) -> u64 {
            self.sent.load(Ordering::SeqCst)
        }
    }

    /// 发一块 —— **一次** `write_all` + `flush`。
    ///
    /// 长度行 / 数据 / CRLF 分三次写会让「一块」在网线上散成三段，
    /// 那时「下游读到的块数」就不再是上游发出的块数 ⇒ 对账那条判据要的是 1:1。
    fn send_chunk(s: &mut TcpStream, sent: &AtomicU64, payload: &[u8]) {
        let mut frame = format!("{:x}\r\n", payload.len()).into_bytes();
        frame.extend_from_slice(payload);
        frame.extend_from_slice(b"\r\n");
        s.write_all(&frame).expect("chunk");
        s.flush().expect("flush");
        sent.fetch_add(1, Ordering::SeqCst);
    }

    fn spawn_fake_upstream(gate: Option<mpsc::Receiver<()>>) -> FakeUpstream {
        let listener = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind fake upstream");
        let addr = listener.local_addr().expect("addr");
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let bodies = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sent = Arc::new(AtomicU64::new(0));
        let seen_c = Arc::clone(&seen);
        let bodies_c = Arc::clone(&bodies);
        let sent_c = Arc::clone(&sent);
        std::thread::spawn(move || {
            let gate = gate;
            for s in listener.incoming() {
                let Ok(mut s) = s else { continue };
                let mut r = BufReader::new(s.try_clone().expect("clone"));
                let mut line = String::new();
                r.read_line(&mut line).expect("read request line");
                let mut auth = false;
                let mut clen = 0usize;
                loop {
                    let mut h = String::new();
                    let n = r.read_line(&mut h).expect("read header");
                    if n == 0 || h == "\r\n" {
                        break;
                    }
                    let lower = h.to_ascii_lowercase();
                    if lower.starts_with("authorization:") {
                        auth = true;
                    }
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        clen = v.trim().parse().unwrap_or(0);
                    }
                }
                // ★ **真的把请求体读进来**（回修轮补的）。不读它有两条后果，本仓都实测过：
                //   ① 「请求体到不到得了上游」**零判据** —— 把 `read_exact_body(...)` 换成
                //      `Vec::new()`（请求体整个丢掉）⇒ 384 条全绿（审计 `CG5`）。
                //   ② 迭代结束 drop 时接收队列里还压着未读数据 ⇒ 内核发 **RST 而不是 FIN**
                //      ⇒ `pump` 的干净 EOF 路 `Ok(0) => break` 一次都走不到，
                //         走的全是 `Err(e) => return Err(e)`。
                // 读期限是**兜底**：中转若少发字节，这里要在对账那条判据上红，**不许挂住**。
                s.set_read_timeout(Some(std::time::Duration::from_millis(2000)))
                    .expect("upstream read deadline");
                let mut body = vec![0u8; clen];
                let mut filled = 0usize;
                while filled < clen {
                    match r.read(&mut body[filled..]) {
                        Ok(0) => break,
                        Ok(k) => filled += k,
                        Err(_) => break,
                    }
                }
                body.truncate(filled);
                bodies_c.lock().expect("lock").push(body);
                seen_c
                    .lock()
                    .expect("lock")
                    .push(format!("{} auth={}", line.trim(), auth));
                s.set_nodelay(true).expect("nodelay");
                s.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n",
                )
                .expect("head");
                s.flush().expect("flush");
                for i in 1..=UPSTREAM_EVENTS {
                    if let Some(g) = gate.as_ref() {
                        // 等下游确认收到**上一块**。这是逐块门闩。
                        let _ = g.recv();
                    }
                    send_chunk(
                        &mut s,
                        &sent_c,
                        format!("data: {{\"i\":{i}}}\n\n").as_bytes(),
                    );
                }
                if let Some(g) = gate.as_ref() {
                    let _ = g.recv();
                }
                s.write_all(b"0\r\n\r\n").expect("end");
                s.flush().expect("flush");
                sent_c.fetch_add(1, Ordering::SeqCst);
                // 请求体已读干净 ⇒ 这里 drop 发的是 **FIN 不是 RST**。
            }
        });
        FakeUpstream {
            addr,
            seen,
            bodies,
            sent,
        }
    }

    /// 起一个中转，返回 `(地址, Relay 句柄, tee 收集器)`。
    fn spawn_relay(up: SocketAddr) -> (SocketAddr, Arc<Relay>, Arc<std::sync::Mutex<Vec<u8>>>) {
        let sink = Arc::new(std::sync::Mutex::new(Vec::new()));
        struct Shared(Arc<std::sync::Mutex<Vec<u8>>>);
        impl Write for Shared {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().expect("lock").extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let base = Base::parse(&format!("http://127.0.0.1:{}", up.port())).expect("base");
        let relay = Arc::new(Relay::new(
            base,
            TeeSink::new(Box::new(Shared(Arc::clone(&sink)))),
        ));
        let listener = listen(0).expect("listen");
        let addr = listener.local_addr().expect("addr");
        let r2 = Arc::clone(&relay);
        std::thread::spawn(move || serve(listener, r2));
        (addr, relay, sink)
    }

    fn send_request(addr: SocketAddr, target: &str, extra: &str) -> TcpStream {
        let mut c = TcpStream::connect(addr).expect("connect relay");
        c.set_nodelay(true).expect("nodelay");
        let body = REQUEST_BODY;
        let req = format!(
            "POST {target} HTTP/1.1\r\nHost: relay\r\n{extra}Content-Length: {}\r\n\r\n{body}",
            body.len()
        );
        c.write_all(req.as_bytes()).expect("write req");
        c.flush().expect("flush");
        c
    }

    /// ★ `DoD-1`：按路径前缀分流。上游必须收到**剥掉前缀之后**的真路径，
    /// 两个键各自成行且不交叉，且两次由**同一个中转进程**服务。
    #[test]
    fn routes_two_keys_through_one_process_and_strips_the_prefix() {
        let up = spawn_fake_upstream(None);
        // ★ **一个**中转实例，**一个**监听面 —— 两个键都从这里走（`K9` 裁定二第 1 条）。
        let (relay_addr, relay, sink) = spawn_relay(up.addr);
        let mut c = send_request(relay_addr, "/s/agentA/sid-AAA/v1/messages?beta=true", "");
        let mut got = Vec::new();
        c.read_to_end(&mut got).expect("read a");
        let mut c2 = send_request(relay_addr, "/s/agentB/sid-BBB/v1/messages?beta=true", "");
        let mut got2 = Vec::new();
        c2.read_to_end(&mut got2).expect("read b");

        let seen = up.seen.lock().expect("lock").clone();
        assert_eq!(seen.len(), 2, "两个键都要打到上游：{seen:?}");
        // ★ 期望值是**手写字面量**，不是拿被测的切分函数算出来的（否则本断言自证、恒绿）。
        // 断的是「路径是什么」，不是「有没有到达」—— 后者剥不剥前缀都过。
        for line in &seen {
            assert_eq!(line, "POST /v1/messages?beta=true HTTP/1.1 auth=false");
        }

        assert_eq!(relay.served(), 2, "两个键必须由同一个中转实例服务");
        let tee = String::from_utf8(sink.lock().expect("lock").clone()).expect("utf8");
        // ★ 只认**事件行**，不认 meta 行。
        //
        // 这一条是变异台逼出来的：第一版按「行里出现这个键」认，而每个响应开头那行
        // `__meta__` 本来就带真键 ⇒ 把**事件**的落点写死成一个键，两边照样各自有行、全绿。
        // 那正是「断言从『落在哪个键上』滑成『有没有到达』」那个瞎法，只是滑在 tee 这一侧。
        let events: Vec<&str> = tee.lines().filter(|l| l.contains("\"event\"")).collect();
        let a: Vec<&&str> = events.iter().filter(|l| l.contains("sid-AAA")).collect();
        let b: Vec<&&str> = events.iter().filter(|l| l.contains("sid-BBB")).collect();
        assert!(!a.is_empty(), "A 键必须有自己的**事件**行：{events:?}");
        assert!(!b.is_empty(), "B 键必须有自己的**事件**行：{events:?}");
        assert_eq!(
            a.len() + b.len(),
            events.len(),
            "每条事件行必须恰好属于一个键，不许有第三种落点：{events:?}"
        );
        for l in &a {
            assert!(!l.contains("sid-BBB"), "两个键的 tee 行不许交叉：{l}");
        }
        // ★ `阻-2`：请求体必须**逐字节**到得了上游。
        //
        // 这一格先前**零判据**：把 `read_exact_body(&mut down_r, n)?` 换成 `Vec::new()`
        // （请求体整个丢掉）⇒ 384 条判据全绿（审计 `CG5`）。成因是假上游从不读请求体
        // ⇒ 没有任何一处看得见 body。中转搬的正是 `POST /v1/messages` 的载荷，
        // 丢了它 claude 当场坏，而门禁全绿。
        //
        // 期望值是**手写字面量**，不是拿被测代码算出来的（否则本断言自证、恒绿）。
        let bodies = up.bodies.lock().expect("lock").clone();
        assert_eq!(
            bodies.len(),
            2,
            "两发请求都要在上游侧留下请求体记录：{bodies:?}"
        );
        for b in &bodies {
            assert_eq!(
                String::from_utf8_lossy(b),
                "{\"m\":1}",
                "上游收到的请求体必须与下游发出的逐字节相同（丢了 / 截了都在这里红）"
            );
        }
        assert!(!got.is_empty());
    }

    #[test]
    fn an_unroutable_path_is_refused_and_never_reaches_upstream() {
        let up = spawn_fake_upstream(None);
        let (relay_addr, _relay, _sink) = spawn_relay(up.addr);
        // ★ **非空对照先打一发**：不然「上游没被碰」是空真 ——
        // 假上游的记录面坏掉、或中转根本没起来，这条照样绿。
        // （本仓纪律：「差集为空 / 没有变化」要附一个非空对照。）
        let mut warmup = send_request(relay_addr, "/s/agentA/sid-AAA/v1/messages", "");
        let mut sink0 = Vec::new();
        warmup.read_to_end(&mut sink0).expect("read warmup");
        assert_eq!(
            up.seen.lock().expect("lock").len(),
            1,
            "非空对照：一条**可路由**的请求必须真的打到上游"
        );

        let mut c = send_request(relay_addr, "/v1/messages", "");
        let mut got = String::new();
        c.read_to_string(&mut got).expect("read");
        assert!(got.starts_with("HTTP/1.1 404"), "应当 404：{got}");
        assert_eq!(
            up.seen.lock().expect("lock").len(),
            1,
            "不可路由的那一发不该再碰上游（计数必须还是 1）"
        );
    }

    /// ★★ `DoD-2`：**逐块透传绝不缓冲**（`K9` 裁定四第 2 条逐字：「这是本方案唯一真正的技术点」）。
    ///
    /// # 这条判据先前只守住了**第 1 块** —— 回修轮补的就是这个
    ///
    /// 旧版：门闩只卡一次（第 1 块之后），块数那一格是 `assert!(reads >= 1)`。
    /// 那一行**结构性恒真**（`reads += 1` 在循环唯一那个 `break` 之前）⇒ 没有任何生产改动能让它红。
    /// ⇒ 「第 1 块立刻透、其余全部攒到流末、一个字节不丢」那一形 **384 条判据全绿**（审计 `K4` 实测），
    /// 而它正是要防的形状本身：TUI 出一个 token 然后卡住，直到整条响应结束才一次性吐完。
    ///
    /// # 今天有两条互相独立的判据，任一条都会红
    ///
    /// ㈠ **逐块门闩（因果证明）**：假上游发**每一块**之前都要等下游确认收到上一块。
    ///    ⇒ 中转攒住**任何**一块，下游就再也等不到下一块 ⇒ 读期限到 ⇒ 红。
    ///    比「比较两个时间戳」更不 flaky，而且射程覆盖到最后一块。
    /// ㈡ **块数对账**（`DoD-2` acceptor ㈡ 逐字「下游读到的块数 ≈ 上游发出的块数」）：
    ///    三个数必须是**同一个** —— 上游真的发出的块数（夹具自己数的）
    ///    · 下游自己数到的读次数 · `pump` 返回的「写给下游并 flush 成功的次数」。
    ///
    /// # 为什么门闩是**逐块**的，而不是靠「上游 RST 时丢尾巴」
    ///
    /// 基线上假上游**从不读请求体** ⇒ 内核发 RST 不是 FIN ⇒ 先攒后写的实现会在那条
    /// `Err(e) => return Err(e)` 上把攒着的尾巴丢掉，于是「后续块也要到」那条内容断言碰巧红。
    /// **那是时序巧合，不是判据**：把错误路改成 `Err(_) => break`（照样吐出去、一个字节不丢），
    /// 同一条测试当场全绿。今天假上游**读请求体**（`阻-2`）⇒ 收尾是**干净 EOF**，
    /// 那条巧合的红**没有了**，接住 `K4` 的是上面 ㈠ ㈡ 两条真判据。
    #[test]
    fn every_chunk_reaches_the_client_before_upstream_sends_the_next_one() {
        let (gate_tx, gate_rx) = mpsc::channel();
        let up = spawn_fake_upstream(Some(gate_rx));
        let (relay_addr, relay, _sink) = spawn_relay(up.addr);
        let t0 = std::time::Instant::now();
        let mut c = send_request(relay_addr, "/s/agentA/sid-AAA/v1/messages", "");
        c.set_read_timeout(Some(std::time::Duration::from_millis(4000)))
            .expect("read deadline");

        let mut buf = [0u8; 4096];
        let mut acc = Vec::new();
        // ★ 只数**响应体**的块，响应头那一段不算（它由 `handle` 在 pump 之前写出去）。
        let mut body_reads = 0u64;

        // 第 1 段：响应头。收到它才放行第 1 块。
        let n = c.read(&mut buf).expect("响应头必须先到");
        assert!(n > 0, "响应头读到 EOF");
        acc.extend_from_slice(&buf[..n]);
        assert!(
            String::from_utf8_lossy(&acc).starts_with("HTTP/1.1 200"),
            "第 1 段必须是响应头：{:?}",
            String::from_utf8_lossy(&acc)
        );
        gate_tx.send(()).expect("open the gate");

        let mut t_first = None;
        // ㈠ 逐块门闩：读到第 i 块 ⇒ 才放行第 i+1 块。
        for i in 1..=UPSTREAM_EVENTS {
            let n = c.read(&mut buf).expect(
                "上游卡在门闩上，只有中转把上一块透出来下游才会有下一块 —— \
                 读超时说明中转攒住了某一块（DoD-2 红）",
            );
            assert!(n > 0, "读到 EOF 而没拿到第 {i} 块");
            body_reads += 1;
            if t_first.is_none() {
                t_first = Some(t0.elapsed());
            }
            acc.extend_from_slice(&buf[..n]);
            assert!(
                String::from_utf8_lossy(&acc).contains(&format!("{{\"i\":{i}}}")),
                "第 {i} 块必须在这一次 read 里就到齐"
            );
            gate_tx.send(()).expect("open the gate");
        }
        // 终止块（chunked 的 `0\r\n\r\n`）也是上游发出的一块。
        let n = c.read(&mut buf).expect("终止块必须到");
        assert!(n > 0, "终止块读到 EOF");
        body_reads += 1;
        acc.extend_from_slice(&buf[..n]);

        let mut tail = Vec::new();
        let _ = c.read_to_end(&mut tail);
        let t_last = t0.elapsed();
        acc.extend_from_slice(&tail);
        let text = String::from_utf8_lossy(&acc).to_string();
        for i in 1..=UPSTREAM_EVENTS {
            assert!(
                text.contains(&format!("{{\"i\":{i}}}")),
                "第 {i} 块的内容也要到：{text:?}"
            );
        }

        // ㈡ 块数对账 —— 三个数必须是同一个。
        let up_chunks = up.sent();
        // 非空对照：夹具自己得真发出这么多块，否则下面两条是空真。
        assert_eq!(
            up_chunks,
            UPSTREAM_EVENTS as u64 + 1,
            "夹具自检：上游必须真发出 {UPSTREAM_EVENTS} 个事件块 + 1 个终止块"
        );
        assert_eq!(
            body_reads, up_chunks,
            "下游数到的块数必须等于上游发出的块数（{up_chunks}）—— 少一块就是中转把它攒住了"
        );
        // `Some(_)` 同时断言这一趟是**干净 EOF** 收尾；`None` = pump 以错误收尾（上游 RST 那一路）。
        assert_eq!(
            relay.pumps(),
            vec![Some(up_chunks)],
            "中转写给下游的块数必须等于上游发出的块数，且必须以干净 EOF 收尾"
        );
        assert!(
            t_first.expect("首块时刻") <= t_last,
            "首块时刻必须不晚于末块时刻"
        );
        println!(
            "[DoD-2] 首块 {:?} · 末块 {t_last:?} · 上游发出 {up_chunks} 块 · 下游数到 {body_reads} 块 · pump 写出 {:?}",
            t_first.expect("首块时刻"),
            relay.pumps()
        );
    }

    /// ★ `DoD-3㈡`（活体）：哨兵头必须**转发得到上游**，却**一个字节都不进 tee / 不进日志**。
    #[test]
    fn the_auth_header_is_forwarded_but_never_teed() {
        const SENTINEL: &str = "SECRET-TOKEN-DO-NOT-LEAK";
        let up = spawn_fake_upstream(None);
        let (relay_addr, _relay, sink) = spawn_relay(up.addr);
        let mut c = send_request(
            relay_addr,
            "/s/agentA/sid-AAA/v1/messages",
            &format!("Authorization: Bearer {SENTINEL}\r\n"),
        );
        let mut got = Vec::new();
        c.read_to_end(&mut got).expect("read");
        // 非空对照：这个 token 真的走过这条路（上游看见了 auth 头）。
        let seen = up.seen.lock().expect("lock").clone();
        assert_eq!(seen.len(), 1);
        assert!(
            seen[0].ends_with("auth=true"),
            "auth 头必须被转发：{seen:?}"
        );
        let tee = String::from_utf8(sink.lock().expect("lock").clone()).expect("utf8");
        // ★ 活体条件要按**事件行**数，不是按「tee 非空」。
        //
        // 这一条也是变异台逼出来的（与上面路由那条同族）：每个响应开头那行 `__meta__`
        // 本来就会写出去 ⇒ 把 tee 的**事件**那一路整个掏空，「tee 非空」照样成立，
        // 本断言就退化成**空真**（闸死了 `[] == []` 也成立）。7u 那一趟实测到了这个形状。
        let events = tee.lines().filter(|l| l.contains("\"event\"")).count();
        assert!(
            events >= 1,
            "tee 里必须真有事件行，否则本断言是空真：{tee:?}"
        );
        assert!(!tee.contains(SENTINEL), "哨兵串泄漏进了 tee");
        assert!(!tee.contains("Authorization"), "tee 里不该有任何头名");
    }

    #[test]
    fn upstream_request_drops_hop_by_hop_and_narrows_accept_encoding() {
        let head = http1::parse_request(
            b"POST /s/a/k/v1/x HTTP/1.1\r\nHost: relay\r\nConnection: keep-alive\r\nAccept-Encoding: gzip, br\r\nAuthorization: Bearer T\r\nContent-Length: 3\r\n\r\n",
        )
        .expect("parse");
        let base = Base::parse("https://api.example.com").expect("base");
        let out =
            String::from_utf8(render_upstream_request(&head, "/v1/x", &base, 3)).expect("utf8");
        assert!(out.starts_with("POST /v1/x HTTP/1.1\r\n"));
        assert!(out.contains("Host: api.example.com\r\n"));
        assert!(out.contains("Accept-Encoding: identity\r\n"));
        assert!(!out.contains("gzip"), "上游的 Accept-Encoding 必须被收窄");
        assert!(
            out.contains("Authorization: Bearer T\r\n"),
            "auth 头原样转发"
        );
        assert_eq!(out.matches("Content-Length:").count(), 1);
        assert!(!out.contains("keep-alive"), "逐跳头不转发");
    }

    #[test]
    fn response_head_keeps_framing_and_forces_close() {
        let raw =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n";
        let out = String::from_utf8(rewrite_response_head(raw)).expect("utf8");
        assert!(out.contains("Transfer-Encoding: chunked\r\n"), "分帧不能丢");
        assert!(!out.contains("keep-alive"));
        assert_eq!(out.matches("Connection:").count(), 1);
        assert!(out.ends_with("\r\n\r\n"));
    }

    /// ★ `DoD-4㈡` 行为那半：从**非回环**地址连不上中转的端口。
    /// 机器上没有非回环地址时**跳过并出声**，不静默当绿。
    #[test]
    fn the_relay_port_is_not_reachable_from_a_non_loopback_address() {
        let up = spawn_fake_upstream(None);
        let (relay_addr, _relay, _sink) = spawn_relay(up.addr);
        let Some(local) = first_non_loopback_v4() else {
            println!("[DoD-4㈡] SKIP：本机没有非回环 IPv4 地址，这一格今天量不了");
            return;
        };
        // 从本机的非回环地址出发去连中转 —— 绑到那个地址再 connect。
        let sock =
            std::net::TcpStream::connect(SocketAddr::new(IpAddr::V4(local), relay_addr.port()));
        assert!(
            sock.is_err() || sock.expect("checked").peer_addr().is_err(),
            "中转不该在非回环地址上可达（本机地址 {local}）"
        );
    }

    fn first_non_loopback_v4() -> Option<Ipv4Addr> {
        // 不引依赖：从 /proc/net/fib_trie 之外最简单的路是连一个不发包的 UDP socket。
        let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
        s.connect("192.0.2.1:9").ok()?;
        match s.local_addr().ok()?.ip() {
            IpAddr::V4(v) if !v.is_loopback() => Some(v),
            _ => None,
        }
    }
}
