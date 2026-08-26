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

/// `--relay` 的**配置面** —— 纯函数：不读环境、不起监听、不碰网络。
///
/// ★ 它为什么被抽出来（回修轮 08-25，D1 `重要-6`）：先前这一段整个长在 `run()` 里，
/// 而 `run()` 尾巴上是**永不返回**的 `serve()` ⇒ 没有任何判据调得动它。
/// 实测：把 `run()` 的函数体整个换成 `2`，384 条判据**全绿**（审计 `CG1`）——
/// `CCM_RELAY_PORT`/`CCM_RELAY_UPSTREAM` 的解析、两个默认值，**一样都没被量过**。
fn resolve_config(port_env: Option<&str>, upstream_env: Option<&str>) -> Option<(u16, Base)> {
    let port = port_env
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    let base = Base::parse(upstream_env.unwrap_or(DEFAULT_UPSTREAM))?;
    Some((port, base))
}

/// `run()` 剥掉「读环境变量」之后的那一半。
///
/// **起监听之前的处置全在这里** ⇒ 判据打得到「基址不认识就退 2」与
/// 「端口起不来就退出并出声」（`:16-17` 头注承诺的那条）两条。
/// 成功那一条尾巴上是永不返回的 `serve()` ⇒ 判据够不到，登记为 `判不了`。
fn run_with(port_env: Option<&str>, upstream_env: Option<&str>) -> i32 {
    let Some((port, base)) = resolve_config(port_env, upstream_env) else {
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

/// `run()` 的**接线面**：哪个环境变量喂给哪个配置位。取值器与执行体都是**注入的**
/// ⇒ 判据打得到这条接线本身，而**不必去改进程环境**（`std::env::set_var` 与并行跑的
/// 别的判据是竞态 —— 那不是判据该有的形状）。
///
/// ★ 它为什么被抽出来（回修轮之四 08-25，D2 `重要-3(D2)`）：
/// `重要-6` 那一轮把 `resolve_config`（纯函数）与 `run_with`（退 2 两条）抽了出来，
/// **最外面那一层 `run()` 自己仍然零判据**。实测把那两行 `std::env::var(...)` **对调**，
/// **389 条判据全绿**（D2 `D2RUN`），而真机后果是 `--relay` **整个起不来**：
/// 端口读不懂 ⇒ 回默认 8788、上游解析失败 ⇒ 退 2。
/// 判据见 `the_relay_entry_reads_each_env_var_into_its_own_config_slot`。
fn run_reading(
    get: &dyn Fn(&str) -> Option<String>,
    exec: &dyn Fn(Option<&str>, Option<&str>) -> i32,
) -> i32 {
    let port = get(ENV_PORT);
    let upstream = get(ENV_UPSTREAM);
    exec(port.as_deref(), upstream.as_deref())
}

/// `--relay` 的入口。配置面只有环境变量（daemon 今天没有配置文件面）。
///
/// 本函数今天**只剩一件事**：把「真取值器」与 `run_with` 接上。接线本身（哪个变量
/// 喂给哪个位）住 `run_reading`，那里有判据钉着。**别往里加逻辑**：加进来的就又没判据了
/// —— 本函数这一行今天是**判不了**的那一格，登记住址件文件 §8.18.3。
pub(crate) fn run(_args: &[String]) -> i32 {
    run_reading(&|k| std::env::var(k).ok(), &run_with)
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
        // ★ 风险 `5x` 的硬规矩：**任何走得到 `serve()` 的判据都必须带期限。**
        //
        // `spawn_relay` 在一条线程里跑 `serve()`，而 `serve()` **永不返回**。中转那边一旦
        // 卡住不回也不关连接，下面的 `read_to_end` / `read_to_string` 就**永远等下去**
        // ⇒ 整个测试台挂住，`^test result:` 条数 = **0** —— 那一屏与「跑完了、没有新红」
        // 几乎分不开（本件实测过一次：变异 `R3`，`cargo` 印
        // `has been running for over 60 seconds`，10 分钟后手动中止）。
        //
        // 10 秒对回环来说宽得离谱（这几条判据实测都在 10ms 量级收工）⇒ 它只会把
        // **挂住**换成**红**，不会把真失败盖掉。要更紧的期限由各判据自己再设（门闩那条设了 4s）。
        //
        // ⚠⚠ **订正分母**〔回修轮之四 08-25，D2 `重要-4(D2)`〕：先前这里（与件文件 §8.17.4、
        // 与 `28a73df` 的提交正文）逐字写的是「在 `send_request` 里统一加 10s 读期限
        //（**一处覆盖全部客户端 socket**）」，还写「既有 **3 条**走得到 `serve()`」。**两个数都错。**
        // 下面这两个分母是**本轮重打的今天的数**（D2 那天的盘面已经不是今天的盘面了）：
        //
        //   · **走得到 `serve()` 的判据 = 6 条**（不是 3 条）。量具：`grep -n 'spawn_relay(' relay/`
        //     去掉定义行与注释行 ⇒ **5** 处，逐条点名 —— `routes_two_keys…` ·
        //     `an_unroutable_path…` · `every_chunk_…` · `the_auth_header…` ·
        //     `the_relay_port_is_not_reachable…`；再加经 `run_with` 的
        //     `the_relay_entry_exits_with_two_when_it_cannot_start`（自带线程 + 5s `recv_timeout`）。
        //     **本函数覆盖其中 4 条**（前四条都调它），第 5 条不调本函数、自带 `connect_timeout`。
        //
        //   · **测试段客户端 socket 的创建点 = 4 处**（量具 `grep -n 'TcpStream::connect' relay/`
        //     去掉生产段那一处与注释行）：本处 `:506` ✔10s 读期限 ·
        //     `both_directions_really_disable_nagle_on_the_socket` 的客户端 ✔10s 读期限 ·
        //     同一条判据里那个**只做 `getsockopt` 不做 read** 的对照 socket（不需要读期限）·
        //     `the_relay_port_is_not_reachable…` 的直连 ✔10s `connect_timeout` + 读期限。
        //     ⇒ **本处覆盖 1/4**，另外三处各自带自己的期限（或根本不读）。
        //
        // ⇒ 别再写「一处覆盖全部客户端 socket」：那句话没有分母，而它今天是假的。
        c.set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .expect("read deadline（风险 5x：把挂住换成红）");
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
        let mut seen_chunks: Vec<String> = Vec::new();
        // ㈠ 逐块门闩 + 在下游侧**数**块：读到 EOF 为止。
        //
        // ⚠ 块数必须是**数出来的**，不是循环次数**构造出来的**。
        // 〔铁律 15 回打自己 —— 这一格我第一版就写错了：先写成 `for i in 1..=UPSTREAM_EVENTS`
        //  再在末尾补一次 `body_reads += 1`，于是走到断言那一刻 `body_reads` **恒等于**
        //  `UPSTREAM_EVENTS + 1`，而它下面那条对账拿它跟同一个常量比 ⇒ **结构性恒真**。
        //  那正是本轮要治的 `assert!(reads >= 1)` **同一种病**，长在治它的代码里。〕
        loop {
            let n = c.read(&mut buf).expect(
                "上游卡在门闩上，只有中转把上一块透出来下游才会有下一块 —— \
                 读超时说明中转攒住了某一块（DoD-2 红）",
            );
            if n == 0 {
                break;
            }
            body_reads += 1;
            if t_first.is_none() {
                t_first = Some(t0.elapsed());
            }
            acc.extend_from_slice(&buf[..n]);
            seen_chunks.push(String::from_utf8_lossy(&buf[..n]).to_string());
            // 放行下一块。上游只 `recv` 该收的那几次，多发的确认积在 channel 里，无害。
            gate_tx.send(()).expect("open the gate");
        }
        let t_last = t0.elapsed();
        let text = String::from_utf8_lossy(&acc).to_string();
        // 每一块**各自单独**到达，且**按序** —— 第 i 次读到的就是第 i 个事件。
        for i in 1..=UPSTREAM_EVENTS {
            let Some(chunk) = seen_chunks.get(i - 1) else {
                panic!(
                    "第 {i} 块根本没到（下游只数到 {} 块）：{text:?}",
                    seen_chunks.len()
                );
            };
            assert!(
                chunk.contains(&format!("{{\"i\":{i}}}")),
                "第 {i} 次读到的应当正是第 {i} 个事件，实际读到 {chunk:?}"
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
        // ⚠ 这两个时刻**只印不断言**：`t_first` 先量、时钟单调 ⇒ `t_first <= t_last` 恒真，
        // 断它等于加一条永远不会红的判据（同上，铁律 15 自查逮到的第二处）。
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

    /// ★★ `重要-5` 的**行为格**（回修轮之四 08-25，承接 D2 `重要-1(D2)`）。
    ///
    /// # 为什么源码扫描不够
    ///
    /// `nodelay_guard` 数的是**文本**：`production_code()` 只剥掉 `#[cfg(test)]` 段与**行首**
    /// `//` 的行，字符串字面量 / 行尾注释 / 块注释里的同形文本**照样被数进去**。
    /// D2 实测（`D2NG1`）：把 `handle` 里真的 `down.set_nodelay(true)?;` **整个删掉**、
    /// 只留一行 `let _nagle_note = "set_nodelay(true)";` ⇒ **389 条判据全绿** ——
    /// 「恰好两处」与「落点里要有 server.rs」两格**都被那行字符串喂饱了**。
    ///
    /// # 这一条判的是**真的调用**
    ///
    /// 量法：`try_clone()` 是 `dup` ⇒ 两个 fd 指向**同一个** socket，
    /// 生产段在它上面 `setsockopt(TCP_NODELAY)` 之后，这个探针 `getsockopt` 读得到。
    /// **两个方向各一格**，各自带非空对照。
    ///
    /// **它仍然不证明** p95 没塌 —— p95 那条要真流量，本轮禁打真 API（件文件 `判不了-4` 原样留着）。
    #[test]
    fn both_directions_really_disable_nagle_on_the_socket() {
        // ㈠ 下游方向：`handle()` 真的在**这一条** socket 上关掉 Nagle。
        let up = spawn_fake_upstream(None);
        let listener = listen(0).expect("listen");
        let addr = listener.local_addr().expect("addr");
        let client = std::thread::spawn(move || {
            let mut c = TcpStream::connect(addr).expect("connect relay");
            // 风险 `5x`：走得到中转的判据一律带读期限，把「挂住」换成「红」。
            c.set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .expect("read deadline（风险 5x）");
            let body = REQUEST_BODY;
            let req = format!(
                "POST /s/agentA/sid-AAA/v1/messages HTTP/1.1\r\nHost: relay\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            c.write_all(req.as_bytes()).expect("write req");
            c.flush().expect("flush");
            let mut got = Vec::new();
            c.read_to_end(&mut got).expect("read response");
            got
        });
        let (down, _peer) = listener.accept().expect("accept");
        let probe = down.try_clone().expect("clone");
        // 非空对照：进 `handle` **之前** Nagle 是开着的（内核默认 TCP_NODELAY = 0）。
        // 没有这一格，下面那句在「内核默认就关着」的系统上是**空真**。
        assert!(
            !probe.nodelay().expect("getsockopt"),
            "非空对照：accept 出来的 socket 默认该是开着 Nagle 的"
        );
        let base = Base::parse(&format!("http://127.0.0.1:{}", up.addr.port())).expect("base");
        let relay = Relay::new(base, TeeSink::new(Box::new(std::io::sink())));
        handle(down, &relay).expect("handle 必须走完一条转发");
        assert!(
            probe.nodelay().expect("getsockopt"),
            "下游方向：`handle()` 必须在下游 socket 上真的关掉 Nagle"
        );
        // ⚠ 先 drop 探针再 join：探针也是这条 socket 的一个 fd，不放手下游读不到 EOF。
        drop(probe);
        let got = client.join().expect("client thread");
        assert!(
            String::from_utf8_lossy(&got).starts_with("HTTP/1.1 200"),
            "这一趟得真走完一条转发，否则上面那两句量的是半条连接：{:?}",
            String::from_utf8_lossy(&got)
        );

        // ㈡ 上游方向：`upstream::connect()` 真的在**它自己**那条 socket 上关掉 Nagle。
        let peer = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind 假上游端");
        let a = peer.local_addr().expect("addr");
        let base2 = Base::parse(&format!("http://127.0.0.1:{}", a.port())).expect("base");
        match upstream::connect(&base2).expect("connect upstream") {
            upstream::Conn::Plain(s) => {
                assert!(
                    s.nodelay().expect("getsockopt"),
                    "上游方向：`upstream::connect()` 必须在上游 socket 上真的关掉 Nagle"
                );
                // 非空对照：同一把尺子量一条**没被生产段碰过**的 socket ⇒ 必须是开着的。
                let raw = TcpStream::connect(a).expect("connect 对照");
                assert!(
                    !raw.nodelay().expect("getsockopt"),
                    "非空对照：没经生产段的 socket 默认该是开着 Nagle 的"
                );
            }
            upstream::Conn::Tls(_) => panic!("`http://` 该走明文那一支"),
        }
    }

    /// ★ `DoD-4㈡` 行为那半：从**非回环**地址连不上中转的端口。
    /// 机器上没有非回环地址时**跳过并出声**，不静默当绿。
    #[test]
    fn the_relay_port_is_not_reachable_from_a_non_loopback_address() {
        let up = spawn_fake_upstream(None);
        let (relay_addr, _relay, _sink) = spawn_relay(up.addr);
        let Some(local) = first_non_loopback_v4() else {
            // ★★ **直写 handle，不许用 `println!`/`eprintln!`**（回修轮之四 08-25，D2 `重要-5(D2)`）。
            //
            // 件计划 `DoD-4` 的 acceptor 逐字要「拿不到非回环地址时必须**跳过并出声**
            //（skip 要印出来），**不许静默当绿**」。而 libtest 的捕获挂在 `print!`/`eprint!`
            // 这一族**宏**走的 `OUTPUT_CAPTURE` 上 ⇒ 默认跑法下这两样一个字都印不出来，
            // 这一格在没有非回环地址的机器（某些 CI 容器）上就是**静默的绿**。
            //
            // 我自己在这个 crate 上重打过（默认 `cargo test`，**不给** `--nocapture`，
            // 测试**通过**，分母 = 那一趟输出全文的 `grep -c`）：
            //   `println!` 命中 **0** · `eprintln!` 命中 **0** ·
            //   `std::io::stderr().write_all` 命中 **1** · `std::io::stdout().write_all` 命中 **1**。
            // 成因：`stderr()` / `stdout()` 返回的 handle **直接写 fd**，不经过 `OUTPUT_CAPTURE`。
            // ⇒ 直写 stderr，那句 acceptor 才真的被兑现。
            let _ = std::io::stderr().write_all(
                b"[DoD-4-2] SKIP: no non-loopback IPv4 on this host; this half is not measured here\n",
            );
            return;
        };
        // 连到**本机的非回环地址**上的中转端口 —— 中转只绑了回环，这一发就该连不上。
        // ⚠ 订正〔回修轮之四 08-25〕：先前这行注释写的是「**绑**到那个地址再 connect」，
        //    而代码从来没绑过源地址，它是把那个地址当**目的地**去连。措辞与实现漂开了。
        //
        // ★★ 风险 `5x` 的第 2 个落点（D2 `重要-4(D2)`）：**它缺的不是读期限，是 connect 期限。**
        // 本条也经 `spawn_relay` 起了 `serve()`，而 `serve()` **永不返回** ⇒ 本条一旦挂住，
        // 整个测试台跟着挂住，`^test result:` 条数掉成 **0** —— 那一屏与「跑完了、没有新红」
        // 几乎分不开。而它**只 connect、不 read** ⇒ `send_request` 里那条 10s 读期限
        // 对它是**空的**（它根本不调 `send_request`）。真正会挂住的形状：本机非回环地址上
        // 到中转端口的 SYN 被**丢弃**（DROP 而不是 RST，例如一条 `iptables -j DROP`）
        // ⇒ `TcpStream::connect` 会一路等到内核 SYN 重传耗尽。`connect_timeout` 把那一形换成红。
        let dest = SocketAddr::new(IpAddr::V4(local), relay_addr.port());
        let sock = std::net::TcpStream::connect_timeout(&dest, std::time::Duration::from_secs(10));
        // 万一真连上了，后面还要 `peer_addr()`：顺手给它一个读期限，别留第二个挂住口。
        if let Ok(ref s) = sock {
            let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(10)));
        }
        assert!(
            sock.is_err() || sock.expect("checked").peer_addr().is_err(),
            "中转不该在非回环地址上可达（本机地址 {local}）"
        );
    }

    /// ★ `重要-6` 之一：`--relay` 的**配置面**。期望值全是**手写字面量** ——
    /// 拿被测的那两个常量去算期望值，本判据就自证、恒绿。
    #[test]
    fn the_relay_entry_resolves_its_defaults_and_lets_env_override_them() {
        let (port, base) = resolve_config(None, None).expect("默认配置应当成立");
        assert_eq!(port, 8788, "默认端口");
        assert_eq!(
            base,
            Base {
                tls: true,
                host: "api.anthropic.com".to_string(),
                port: 443
            },
            "默认上游"
        );

        let (port, base) =
            resolve_config(Some("19999"), Some("http://127.0.0.1:1")).expect("env 覆盖应当成立");
        assert_eq!(port, 19999, "CCM_RELAY_PORT 必须盖得住默认");
        assert_eq!(
            base,
            Base {
                tls: false,
                host: "127.0.0.1".to_string(),
                port: 1
            },
            "CCM_RELAY_UPSTREAM 必须盖得住默认"
        );

        // 端口读不懂 ⇒ **回默认**，不是 0、也不是崩。
        for bad in ["not-a-port", "70000", "-1", ""] {
            assert_eq!(
                resolve_config(Some(bad), None).expect("应当回默认").0,
                8788,
                "读不懂的端口 {bad:?} 必须回默认"
            );
        }
        // 基址不认识 ⇒ None（调用方据此退 2）。
        assert!(resolve_config(None, Some("ftp://x")).is_none());
    }

    /// ★★ `重要-3(D2)`：`run()` 那一层的**接线** —— 哪个环境变量喂给哪个配置位。
    ///
    /// D2 实测：把 `run()` 里那两行 `std::env::var(...)` **对调** ⇒ **389 条判据全绿**
    /// （`D2RUN`），而真机后果是 `--relay` 整个起不来。今天那条接线住 `run_reading`，
    /// 取值器与执行体都注入 ⇒ 本条打得到它，**且不碰进程环境**。
    ///
    /// 期望值全是**手写字面量**，不拿被测的 `ENV_PORT` / `ENV_UPSTREAM` 去算。
    #[test]
    fn the_relay_entry_reads_each_env_var_into_its_own_config_slot() {
        let seen: std::sync::Mutex<Vec<(Option<String>, Option<String>)>> =
            std::sync::Mutex::new(Vec::new());
        let exec: &dyn Fn(Option<&str>, Option<&str>) -> i32 = &|p, u| {
            seen.lock()
                .expect("lock")
                .push((p.map(str::to_string), u.map(str::to_string)));
            7
        };

        // ㈠ 取值器把**变量名原样**当值返回 ⇒ 接线一旦对调，下面这句当场对不上。
        let echo: &dyn Fn(&str) -> Option<String> = &|k| Some(k.to_string());
        assert_eq!(run_reading(echo, exec), 7, "入口必须把执行体的退出码原样带回");
        assert_eq!(
            seen.lock().expect("lock").clone(),
            vec![(
                Some("CCM_RELAY_PORT".to_string()),
                Some("CCM_RELAY_UPSTREAM".to_string())
            )],
            "第 1 个配置位必须读 CCM_RELAY_PORT，第 2 个必须读 CCM_RELAY_UPSTREAM"
        );

        // ㈡ 两个变量都没设 ⇒ 两个位都是 None，不是把变量名当默认值塞进去。
        seen.lock().expect("lock").clear();
        let none: &dyn Fn(&str) -> Option<String> = &|_| None;
        assert_eq!(run_reading(none, exec), 7);
        assert_eq!(
            seen.lock().expect("lock").clone(),
            vec![(None, None)],
            "没设环境变量时两个配置位都该是 None"
        );
    }

    /// 把 `run_with` 扔进一条线程 + 读期限。
    ///
    /// ⚠⚠ **这是硬保险，不是装饰**：`run_with` 成功那一条路尾巴上是**永不返回**的 `serve()`。
    /// 任何让它走到那儿的改动都会让整个测试台**挂住** —— 而挂住是 **CRASH，不是红**
    ///（判定行直接掉成 0，读起来像「没有新红」）。
    /// **实测过**：变异 `R3`（让 `resolve_config` 不再认 `port_env`）会去绑一个**空闲**端口、
    /// 进 `serve()`，那一趟 `^test result:` 条数 = **0**，`cargo` 印的是
    /// `has been running for over 60 seconds`。⇒ 这里把「挂住」换成「5 秒后红」。
    fn relay_entry_exit_code_within_5s(
        port_env: Option<String>,
        upstream_env: Option<String>,
    ) -> i32 {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(run_with(port_env.as_deref(), upstream_env.as_deref()));
        });
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .expect("`--relay` 入口必须**返回** —— 超时说明它没退出，而是进了 serve()")
    }

    /// ★ `重要-6` 之二：**起不来就退出并出声**（`server.rs:16-17` 头注承诺的处置）。
    ///
    /// ⚠ 本条全程**只碰回环**：不打任何 API、不起任何 claude、不绑非回环地址。
    /// 成功那一条路（真起监听 + `serve()` + `TeeSink::to_stdout()` 接线）**判不了**，见件文件登记。
    #[test]
    fn the_relay_entry_exits_with_two_when_it_cannot_start() {
        // ㈠ 基址不认识 —— 连监听都不起。
        assert_eq!(
            relay_entry_exit_code_within_5s(None, Some("not-a-url".to_string())),
            2,
            "基址不认识必须退 2"
        );
        // ㈡ 端口被占。**非空对照**：这个端口刚刚被 `listen(0)` 绑成功过
        //    ⇒ 「绑不上」不是因为端口本来就不可用。
        let squatter = listen(0).expect("先自己占住一个回环端口");
        let port = squatter.local_addr().expect("addr").port();
        // ★ 先断「端口真被 env 盖住了」，**再**去调入口 —— 不然入口会去绑**别的**端口，
        //   绑得上就进 serve() 永不返回。这一条把那一形挡在门外，报错也更准。
        assert_eq!(
            resolve_config(Some(&port.to_string()), Some("http://127.0.0.1:1"))
                .expect("配置应当成立")
                .0,
            port,
            "端口必须被 env 盖住，否则下一步绑的是别的端口"
        );
        assert_eq!(
            relay_entry_exit_code_within_5s(
                Some(port.to_string()),
                Some("http://127.0.0.1:1".to_string())
            ),
            2,
            "端口起不来必须退 2（被占的端口 {port}）"
        );
        drop(squatter);
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
