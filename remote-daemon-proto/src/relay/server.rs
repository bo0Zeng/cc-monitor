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
/// **请求体**字节上限〔回修轮之五 08-25，D3 `阻-1(D3)`〕。
///
/// 它管的是**一条下游请求的请求体**，超了**拒收+回错**：回 `413 Payload Too Large`，
/// 且**一个字节都不读、一个字节都不分配**（见 `http1::read_exact_body` 的头注）。
///
/// ⚠ `HEAD_CAP` **管不到这个量** —— 它只管头**字节数**。先前这一格没有任何上限，
/// 一条 `Content-Length: 1000000000000` 就能把**整个中转进程** abort 掉
/// （`handle_alloc_error` ⇒ SIGABRT，不走 unwind），而本件是「一个进程服务 N 个会话」
/// ⇒ 打掉的是当时**所有会话**的在途流。
///
/// 值怎么定的：claude 搬的是 `POST /v1/messages` 的载荷 —— 一次会话的全部上下文 + 附件。
/// 64 MiB 比本仓见过的任何一次请求都宽两个量级以上，同时把「一个数就能耗尽内存」这条路堵死。
/// **登记住址** `src-tauri/src/byte_cap_registry.rs`（那张表默认拒绝：不登记就红）。
const BODY_CAP: usize = 64 * 1024 * 1024;
/// **tee 侧解码缓冲**的字节上限（`SseSplitter` 的半行 · `ChunkedView` 攒着的那截）。
///
/// 它与 `BODY_CAP` **不同族**：那一条防的是「拿外部给的**一个数**去分配」（攻击方一个字节
/// 不用发），这一条防的是「按**真实收到的字节**无界增长」（上游发一条永不换行的 `data:` 行 /
/// 永不结束的块长度行）。超了**丢弃+带身份报告**：丢掉那一截并计数，
/// 由 tee 流里的 `__dropped__` 行报出去 —— **tee 少一段，下游的字节一个不少**。
const TEE_DECODE_CAP: usize = 8 * 1024 * 1024;
/// 每次从上游读多少 —— **上限**，不是「要凑满这么多」。
/// `std::io::Read::read` 本来就是「有多少给多少」，不循环凑满。
const READ_CHUNK: usize = 64 * 1024;

/// 同时在途的下游连接数上限〔回修轮之五 08-25，D3 `阻-3(D3)` 的**做得到的那一半**〕。
///
/// ⚠ **是条数不是体量**，所以名字里刻意不带 `MAX`/`CAP`/`LIMIT`/`BYTES`
/// —— 那几个词是 `byte_cap_registry` 的钩子，带了会让它把一个**连接数**当成字节上限收进人群。
///
/// 超了怎么办：**回 `503 Service Unavailable` 并关连接**，不是静默 FIN。
/// 先前 `serve()` 是每连接无条件 spawn、且 `let _ = …spawn(…)` 把失败**整个吞掉**
/// ⇒ 线程顶满之后下游拿到的是一个**没有任何 HTTP 响应**的 FIN，而 `serve` 一个字都不印。
const INFLIGHT_CONNECTIONS: usize = 256;

/// **下游**那条 socket 的读写期限〔回修轮之六 08-25，D3 `阻-3(D3)` 的**后半段**〕。
///
/// # 它治的是什么（别读成「限流已经够了」）
///
/// `INFLIGHT_CONNECTIONS` 买的是「**顶不满、拒绝有声**」；这一条买的是
/// 「**顶住的那些会自己散**」。没有它，256 条半开连接能把中转**永久**钉死，
/// 而它会礼貌地回 503 —— **那一屏读起来像正常限流**。
/// （D3 实测：64 条半开 ⇒ 线程 4 → 68，全卡在 `read_head` 的 `r.read(&mut one)?` 上永不返回。
///  **这个数我没重打，住址 `audits/K-H1-D3.md` §2.3**。）
///
/// # 它不是定时器 —— 这句话就是 `no_timer_guard::REGISTERED_DURATION_USES` 里登记的那一行
///
/// `SO_RCVTIMEO` / `SO_SNDTIMEO` 说的是「**这一次**阻塞的读/写最多等多久」：
/// 有字节就**立刻**返回，没字节就**报错**返回。它不让任何线程**自己醒来**、不产生任何节拍。
/// ⭐ 配套硬约束：**期限到了就把这条连接结掉，任何一层都不许重试** ——
/// 一重试它就从「阻塞有上限」变成「轮询」，而轮询正是零定时器护栏要防的东西。
/// 今天靠的是：`pump` 与 `http1` 的读循环**只**对 `Interrupted`（EINTR）`continue`，其余一律 `return Err`。
///
/// # 值为什么是 30 秒（分母写在这里）
///
/// 对端**就在本机** —— `listen()` 绑的是 `LOOPBACK` 常量，`DoD-4㈡` 那条判据钉着它。
/// 分母是「回环上搬完一条**最大**请求体要多久」：`BODY_CAP` = 64 MiB，回环带宽是 GB/s 量级
/// ⇒ 零点零几秒。30 秒比它高**两到三个量级**。
/// ⚙ 那个「零点零几秒」是**按量级推的，我没实测本机回环吞吐** ⇒ 数量级论证，不是读数。
///
/// ⚙ **设错会怎样**（两个方向都坏，坏法不同）：
/// - **设长**（比如照抄上游那 600 秒）：半开连接确实会自己散，但要散 10 分钟
///   ⇒ 上界从 ∞ 降到 600 秒是真收益，但那个数**读起来仍像挂死**。
/// - **设短**（比如 1 秒）：一台负载高的机器上，一条**合法**的大请求体会被中转自己掐掉，
///   客户端看到「网络错误」而中转日志上是一条正常的连接结束 ⇒ **打断正常流量、且不好查**。
///
/// # 为什么**不**跟上游用同一个数
///
/// 「这个对端合法地可以多久不吭声」是**对端的属性**，不是方向的属性。
/// 下游是本机、请求在它内存里已经拼好了 ⇒ 慢是**异常**；
/// 上游是「模型在想」⇒ 慢是**正常**（见 `upstream::UPSTREAM_DEADLINE`）。
const DOWNSTREAM_DEADLINE: std::time::Duration = std::time::Duration::from_millis(30_000);

/// 把 `DOWNSTREAM_DEADLINE` 装到一条下游 socket 的**两个方向**上。
///
/// 抽成函数是因为它有**两个职责不同的调用点**，而它们必须用同一个数：
/// ㈠ `serve()` 刚 `accept` 出来那一刻 —— 覆盖**拒绝路径**（503），那一支跑在 **accept 线程**上，
///    也就是**唯一一条它卡住就全盘停摆**的线程；
/// ㈡ `handle()` 开头 —— D3 §2.3 逐字点名的住址（「`handle()` 只做 `set_nodelay`，
///    一个 `set_read_timeout` / `set_write_timeout` 都没有」），也是转发路径真正阻塞的地方。
///
/// ⚠ ㈡ 不是 ㈠ 的赘余：`handle()` 有一个**不经过 `serve()`** 的调用者（判据直接调它），
/// 而「转发路径上有期限」这句承诺必须由 `handle()` 自己兑现，不能挂在调用者身上。
fn apply_downstream_deadline(s: &TcpStream) -> std::io::Result<()> {
    s.set_read_timeout(Some(DOWNSTREAM_DEADLINE))?;
    s.set_write_timeout(Some(DOWNSTREAM_DEADLINE))
}

/// 上游**中间响应**（1xx）最多容忍几条〔回修轮之五 08-25，D3 `重要-1(D3)`〕。
/// 超了回 502：那已经不是一个正常的上游。
const INTERIM_RESPONSES_ALLOWED: usize = 8;

/// 中转的运行期状态。**一个进程一份**，跨连接共享。
pub(crate) struct Relay {
    base: Base,
    tee: TeeSink,
    /// 本进程服务过的请求数 —— `DoD-1㈢`「两个键由同一个中转进程服务」量的就是它。
    served: AtomicU64,
    /// 每条连接透传收尾时落一笔 —— `DoD-2` acceptor ㈡「下游读到的块数 ≈ 上游发出的块数」量的就是它。
    ///
    /// 同时在途的连接数〔回修轮之五 08-25，`阻-3(D3)`〕。`serve()` 进出各动一次。
    inflight: Arc<std::sync::atomic::AtomicUsize>,
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
            inflight: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
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
///
/// # ★ 在途连接数有上界，且拒绝是**出声**的〔回修轮之五 08-25，D3 `阻-3(D3)`〕
///
/// 先前这里是「每连接无条件 spawn」+ `let _ = …spawn(…)`：
/// - **无上界** —— 64 条半开连接（只发半个请求头、永不发结尾空行、不关连接）
///   就钉住 64 条线程（D3 实测 `半开 64 条之前线程 4 · 1.5s 之后线程 68`）；
/// - **spawn 失败被整个吞掉** —— 线程顶满之后 `stream` 当场 drop，下游拿到一个
///   **没有任何 HTTP 响应**的 FIN，而 `serve` 一个字都不印 ⇒ **静默拒绝**，不是 503。
///
/// 今天：超过 `INFLIGHT_CONNECTIONS` 就回 **503** 并关连接；spawn 失败同样回 503 并**出声**。
///
/// # ★★ 另一半也补上了〔回修轮之六 08-26〕：**顶住的那些会自己散**
///
/// ⚠ 订正：这里先前逐字写着「**这只是那条阻塞的一半，另一半我做不到**」——
/// 那句话在回修轮之五是真的（`no_timer_guard.rs` 当时不在写区，交回见件文件 §8.20.4），
/// **PM 收 R5 时扩了写区一格并派了 R6**（§8.21.3），今天它**已经不成立了**。
///
/// 补的是读写期限：`apply_downstream_deadline` 在**两个**调用点装 `DOWNSTREAM_DEADLINE`
/// —— ㈠ 这里，`accept` 出来那一刻（覆盖 503 那条支，它跑在 **accept 线程**上）；
/// ㈡ `handle()` 开头（转发路径真正阻塞的地方，也是 D3 §2.3 逐字点名的住址）。
/// 上游那条 socket 由 `upstream::connect` 装 `upstream::UPSTREAM_DEADLINE`。
///
/// ⇒ 三样齐了：**顶不满**（上界）· **拒绝有声**（503 而不是静默 FIN）· **顶住的会自己散**（期限）。
pub(crate) fn serve(listener: TcpListener, relay: Arc<Relay>) {
    use std::sync::atomic::Ordering::SeqCst;
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            continue;
        };
        // ★★ 期限**先装上，在分流之前**：下面那条 503 支跑在 accept 线程上，
        //    它卡住的话整个中转不再 accept 任何新连接 —— 那比钉住一条连接线程贵得多。
        //    装不上就**关掉这条连接并出声**：一条没有期限的连接正是 `阻-3` 那个缺陷本身，
        //    宁可拒绝，也不放一条永远散不掉的进来。
        if let Err(e) = apply_downstream_deadline(&stream) {
            eprintln!("[relay] cannot set connection deadline: {e}");
            continue;
        }
        if relay.inflight.load(SeqCst) >= INFLIGHT_CONNECTIONS {
            // ⚠ 只印数字与上限，**永不印请求头**（`K9` 裁定四第 1 条）——
            // 这一支根本还没读过一个字节，连请求头都还不存在。
            eprintln!("[relay] refusing: {INFLIGHT_CONNECTIONS} connections already in flight");
            let _ = respond_and_drain(&mut stream, "503 Service Unavailable");
            continue;
        }
        // ★ 先留一份 fd 副本：`spawn` 失败时 `stream` 已经被 move 进那个闭包、拿不回来，
        //   没有副本就只能眼看着它 drop 成一个**没有任何 HTTP 响应**的 FIN。
        //   `try_clone` 是一次 `dup`，成功那条路上它立刻 drop（dup 出来的 fd 关掉不关 socket）。
        let spare = stream.try_clone().ok();
        let relay = Arc::clone(&relay);
        let inflight = Arc::clone(&relay.inflight);
        inflight.fetch_add(1, SeqCst);
        // 每连接一个线程。与参考实现同形（它用 ThreadingHTTPServer）。
        let spawned = std::thread::Builder::new()
            .name("ccm-relay-conn".to_string())
            .spawn(move || {
                if let Err(e) = handle(stream, &relay) {
                    // ⚠ 只印错误本身，**永不印请求头**（`K9` 裁定四第 1 条）。
                    eprintln!("[relay] connection ended: {e}");
                }
                relay.inflight.fetch_sub(1, SeqCst);
            });
        if let Err(e) = spawned {
            inflight.fetch_sub(1, SeqCst);
            eprintln!("[relay] cannot spawn connection thread: {e}");
            if let Some(mut s) = spare {
                let _ = respond_and_drain(&mut s, "503 Service Unavailable");
            }
        }
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

/// 回一句状态行**并把已经到达、还没被读走的请求字节排掉**，然后就可以关了。
///
/// # ⚠ 排掉那一步不是装饰，它决定下游看不看得见这句话
///
/// TCP 上，`close` 的时候**接收队列里还压着没读的数据**，内核发的是 **RST 不是 FIN**
/// ⇒ 下游那边 `read` 拿到的是 `ConnectionReset`，而**不是**我们刚写的那句 503/413
/// —— 「拒绝」就又变回**静默**的了，而那正是 `阻-3(D3)` 点名的病。
/// 〔本轮第一版就是这么红的，逐字 `Os { code: 104, kind: ConnectionReset }`；
///  同一条 TCP 事实在 `spawn_fake_upstream` 的头注里也记着（那边是「不读请求体 ⇒ RST 不是 FIN」）。〕
///
/// # 为什么是**非阻塞**的排
///
/// 这一支面对的可能正是一条**半开**连接（`阻-3` 那一形）：它承诺过的字节可能永远不来。
/// 阻塞地排就等于给中转开一个新的挂死点。⇒ 只排「**已经到了的**」，`WouldBlock` 就收工。
/// 上限 `HEAD_CAP`：排也要有个头，不许被一条无限流住。
///
/// **诚实边界**：下游的字节要是**在我们排完之后**才到，`close` 照样发 RST。
/// 这一支不追求「一定送达」，只把常见那一形（请求已经整条发出来了）从静默变成有声。
fn respond_and_drain(down: &mut TcpStream, status: &str) -> std::io::Result<()> {
    let r = respond_status(down, status);
    let _ = down.set_nonblocking(true);
    let mut sink = [0u8; 4096];
    let mut left = HEAD_CAP;
    while left > 0 {
        match down.read(&mut sink) {
            Ok(0) => break,
            Ok(n) => left = left.saturating_sub(n),
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            // `WouldBlock` 也走这里：已经到的都排完了。
            Err(_) => break,
        }
    }
    let _ = down.set_nonblocking(false);
    r
}

/// 处理一条下游连接：解析 → 分流 → 连上游 → 逐块透传 + tee。
fn handle(down: TcpStream, relay: &Relay) -> std::io::Result<()> {
    down.set_nodelay(true)?;
    // ★★ `阻-3(D3)` 后半段的正主：没有这一句，一条半开连接（只发半个请求头就不动了）
    //    会把这条线程**永久**钉在下面 `read_head` 的读上。
    //    `try_clone` 是 `dup` ⇒ 下面 `down_w` 与 `down_r` 共用同一条 socket、同一份期限。
    apply_downstream_deadline(&down)?;
    let mut down_w = down.try_clone()?;
    let mut down_r = BufReader::new(down);

    let Some(raw_head) = http1::read_head(&mut down_r, HEAD_CAP)? else {
        return respond_and_drain(&mut down_w, "400 Bad Request");
    };
    let Some(head) = http1::parse_request(&raw_head) else {
        return respond_and_drain(&mut down_w, "400 Bad Request");
    };
    if head.is_chunked_body() {
        return respond_and_drain(&mut down_w, "411 Length Required");
    }
    let Some(r) = route::parse(&head.target) else {
        return respond_and_drain(&mut down_w, "404 Not Found");
    };
    // ★ `阻-1(D3)` + `重要-2(D3)`：请求体这一格先前有**两个**洞，两个都在这几行上。
    //   ① 长度**无上界** ⇒ `Content-Length: 1e12` 把整个进程 abort 掉（SIGABRT，不走 unwind）；
    //   ② 长度**读不懂**（`7abc`）与「没有这个头」挤在同一个 `None` 里 ⇒ 请求体被静默丢掉、
    //      上游收到空体、下游拿到一条正常的 200。中转搬的正是 `POST /v1/messages` 的载荷。
    let body = match head.content_length() {
        http1::BodyLen::Exact(n) => match http1::read_exact_body(&mut down_r, n, BODY_CAP)? {
            Some(b) => b,
            // 超 `BODY_CAP`：一个字节都没读过（连接上还压着那 n 字节）⇒ 说清楚再关。
            None => return respond_and_drain(&mut down_w, "413 Payload Too Large"),
        },
        http1::BodyLen::Absent => Vec::new(),
        // **有这个头但读不懂** ⇒ 400，**不许**当成「没有请求体」往上游发一条空体。
        http1::BodyLen::Unparsable => return respond_and_drain(&mut down_w, "400 Bad Request"),
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

    // ★★ `重要-1(D3)`：**1xx 是中间响应，不是最终响应**。
    //
    // 先前这里读到第一个 `\r\n\r\n` 就收工，而 `parse_response` 只校验 `HTTP/1.`
    // ⇒ 上游先发一条 `HTTP/1.1 100 Continue\r\n\r\n` 的话，那条被当成**最终响应头**写给下游，
    //   紧随其后的**真** `HTTP/1.1 200 OK` 连同全部响应头被 `pump` 当**响应体**透传。
    //   D3 实测下游逐字拿到
    //   `"HTTP/1.1 100 Continue\r\nConnection: close\r\n\r\nHTTP/1.1 200 OK\r\n…"`，
    //   ⚠ 而同一趟的 **tee 完全正常** ⇒ 这一形靠看日志/tee 发现不了。
    //   配套的另一半：下游发的 `Expect: 100-continue` 会被 `render_upstream_request`
    //   **原样转给上游**（它不在逐跳表里）⇒ 合规的上游正好回 100，正中这一形。
    //
    // 今天：1xx 一律**读掉丢弃**再读下一条，直到拿到非 1xx 的那条；
    // 超过 `INTERIM_RESPONSES_ALLOWED` 条就回 502（那已经不是一个正常的上游）。
    // ⚠ `101 Switching Protocols` 也是 1xx：本中转**不支持**协议升级
    //   （`Upgrade` / `Connection` 都在逐跳表里、根本转不到上游），真收到 101 就会
    //   继续往下读，而其后是隧道字节不是 HTTP 头 ⇒ `parse_response` 失败 ⇒ **502**。
    //   那是个**定义好的**结局，不是「当成最终响应发下去」。
    let (headers, raw_resp) = {
        let mut interim = 0usize;
        loop {
            let Some(raw) = http1::read_response_head(&mut up, HEAD_CAP)? else {
                return respond_status(&mut down_w, "502 Bad Gateway");
            };
            let Some((status, headers)) = http1::parse_response(&raw) else {
                return respond_status(&mut down_w, "502 Bad Gateway");
            };
            if http1::is_interim_status(&status) {
                interim += 1;
                if interim > INTERIM_RESPONSES_ALLOWED {
                    return respond_status(&mut down_w, "502 Bad Gateway");
                }
                continue;
            }
            break (headers, raw);
        }
    };
    down_w.write_all(&rewrite_response_head(&raw_resp))?;
    down_w.flush()?;

    let mut view = BodyView::for_response(&headers);
    let mut splitter = SseSplitter::default();
    relay.tee.open(&r.agent, &r.key);
    // ★ 返回值**必须落地**：它是 `DoD-2㈡`「块数对账」的唯一量点。
    // 写成 `pump(...)?;` 就等于把它丢掉 —— 那正是审计 `K4` 能全绿的原因。
    let outcome = pump(&mut up, &mut down_w, &mut |raw| {
        let decoded = view.feed(raw, TEE_DECODE_CAP);
        for payload in splitter.feed(&decoded, TEE_DECODE_CAP) {
            relay.tee.event(&r.agent, &r.key, &payload);
        }
        // 解码那一路超上限丢掉的字节要**报出去**，不许静默（见 `TEE_DECODE_CAP` 头注）。
        relay
            .tee
            .note_dropped_bytes(view.take_dropped() + splitter.take_dropped());
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
/// - ⚠ **其余头一律原样转发，包括 auth 头** —— 转发但**不记录**。
///
/// # ⚠⚠ 订正〔回修轮之五 08-25，D3 `重要-3(D3)`〕：上一行先前引 `K11 裁定一` 当依据，**那半句今天是假的**
///
/// `K11 裁定一` 在 **08-25 被改判**，现行正文逐字是：「**端点与 key 都归中转的路由表；
/// 中转在转发时替换 `Authorization` 头；客户端一个凭据都不配。**」
/// ⇒ 那条裁定只支持上面的**后半句（不记录）**，**前半句（原样转发 auth 头）已被它的现行版推翻**。
/// 08-24 那半（「key 归 `--settings` 覆盖层」）在 `MASTERPLAN` 里是**带删除线的来历段**，不是现行。
///
/// **今天盘上的真话，逐条**：
/// 1. 本函数**原样转发**下游那份 `Authorization`，**不替换** —— 这是本件（`K-H1` 甲半·搬字节）
///    的形状，**不是** `K11` 裁定一要的形状。
/// 2. 「换头」**本件不做**，落点是下一件；`K11 裁定一` 自己写着硬前置，逐字：
///    「**没有那条新判据之前，不许把 key 接进中转。**」而它要的那条判据（「key 不许出现在
///    任何日志、任何回给前端的帧里」）**今天只有一半有牙** —— 见下。
/// 3. 「不记录」这一半今天是**两条判据**在钉：
///    `the_auth_header_is_forwarded_but_never_teed`（tee 那一半）与
///    `a_sentinel_auth_header_shows_up_in_neither_the_relay_processs_stderr_nor_its_stdout`（**日志/stderr 那一半，
///    本轮才补上的**；先前那一格 D3 实测 0 红 / 392）。
/// 4. ⚠ 换头那天这里要连**判据**一起改：`the_auth_header_is_forwarded_but_never_teed`
///    查的是**进来的**哨兵串，而换头之后进来的那个会被换掉、真正该查的是**换上去的那个 key**
///    ⇒ 它会**在无声中换靶**（D3 `MU4` 实测：换头形状下红的是另外 2 条，这一条仍绿）。
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
/// 判据见 `each_env_var_name_goes_into_its_own_config_slot`。
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
        /// 上游**真的发出**的 SSE **事件**数（终止块不算）〔回修轮之五 08-25，`阻-4(D3)`〕。
        ///
        /// ⚠ 它与 `sent` **不是同一个量**，别拿一个当另一个用：
        /// `sent` 数的是**网线上的块**（含终止块），`events` 数的是**上游发出的 `data:` 事件**。
        /// `DoD-2㈡` 的「块数对账」用前者，`DoD-3㈠` 的「`event` 数对账」用后者。
        events: Arc<AtomicU64>,
    }

    impl FakeUpstream {
        fn sent(&self) -> u64 {
            self.sent.load(Ordering::SeqCst)
        }

        fn events(&self) -> u64 {
            self.events.load(Ordering::SeqCst)
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
        let events = Arc::new(AtomicU64::new(0));
        let seen_c = Arc::clone(&seen);
        let bodies_c = Arc::clone(&bodies);
        let sent_c = Arc::clone(&sent);
        let events_c = Arc::clone(&events);
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
                    // ★ 事件数**由夹具自己数**（`阻-4(D3)`）：判据拿它当分母，
                    //   而不是拿 `UPSTREAM_EVENTS` 这个常量 —— 夹具少发了也要看得见。
                    events_c.fetch_add(1, Ordering::SeqCst);
                }
                if let Some(g) = gate.as_ref() {
                    let _ = g.recv();
                }
                s.write_all(b"0\r\n\r\n").expect("end");
                s.flush().expect("flush");
                sent_c.fetch_add(1, Ordering::SeqCst);
                // ★★ **最后一块也要等下游确认**（回修轮之四 08-25，D2 `重要-1(D2)`）。
                //
                // 先前门闩到终止块**之前**就停了。门闩是因果链「攒住第 i 块 ⇒ 下游不发第 i+1
                // 次确认 ⇒ 上游卡住」，而**最后一块没有「第 i+1 块」** ⇒ 中转把终止块攒到流末
                // 再吐，下游照样数到 4 块、`pump` 照样返回 4 ⇒ **两个半格同时瞎在同一形上**
                //（D2 `D2M2` 实测 389 全绿；本轮重打 392 全绿）。而判据头注当时逐字写着
                // 「射程覆盖到最后一块」—— **那句话是假的**。
                //
                // 这里再 `recv` 一次、**带上限**，把那一形接住：
                //   · 中转正常透传 ⇒ 下游立刻确认 ⇒ 这一次 `recv` 立刻返回，什么都不耽误；
                //   · 中转攒住终止块 ⇒ 下游读不到、不确认 ⇒ 上游**不 drop、不发 FIN**
                //     ⇒ 中转的 `Ok(0) => break` 走不到、攒着的吐不出去
                //     ⇒ 下游那边 **4s 读期限**先到 ⇒ **红**。
                // ⚠ **6s > 4s 这个大小关系就是这一格的地基**：上限要严格大于下游的读期限，
                //   否则上游先放手、中转把攒着的吐出去，这一形又逃了。
                if let Some(g) = gate.as_ref() {
                    let _ = g.recv_timeout(std::time::Duration::from_millis(6000));
                }
                // 请求体已读干净 ⇒ 这里 drop 发的是 **FIN 不是 RST**。
            }
        });
        FakeUpstream {
            addr,
            seen,
            bodies,
            sent,
            events,
        }
    }

    /// ★★ **tee 的收集面必须「能等」**〔回修轮之五 08-25，`阻-2(D3)` 的连带〕：
    /// 今天 tee 的写落在**另一条线程**上（那正是 `阻-2` 的修法）⇒ 下游的响应读完了，
    /// tee 那几行**未必**已经落进这个 `Vec`。判据读完就断言 = 在读一个还没写完的缓冲区，
    /// 而「tee 是空的」与「还没写完」在断言里**长得一模一样** ⇒ 那是一条会随机说谎的判据。
    /// ⇒ 每写一行往通道投一条；判据用 `TeeTap::wait_lines` 等够行数，**等不到当红**。
    struct TeeTap {
        buf: Arc<std::sync::Mutex<Vec<u8>>>,
        rx: mpsc::Receiver<()>,
    }

    impl TeeTap {
        /// 等写线程写够 `n` 行；等不到就 panic（不许把「还没写完」读成「tee 是空的」）。
        fn wait_lines(&self, n: usize) {
            for i in 0..n {
                self.rx
                    .recv_timeout(std::time::Duration::from_secs(4))
                    .unwrap_or_else(|e| panic!("等 tee 的第 {} 行没等到：{e}", i + 1));
            }
        }

        fn text(&self) -> String {
            String::from_utf8(self.buf.lock().expect("lock").clone()).expect("utf8")
        }

        /// tee 里的**事件行**（不含 meta 行、不含 `__dropped__` 行）。
        fn event_lines(&self) -> Vec<String> {
            self.text()
                .lines()
                .filter(|l| l.contains("\"event\""))
                .map(str::to_string)
                .collect()
        }
    }

    /// 起一个中转，返回 `(地址, Relay 句柄, tee 收集器)`。
    fn spawn_relay(up: SocketAddr) -> (SocketAddr, Arc<Relay>, TeeTap) {
        spawn_relay_with_sink(up, None)
    }

    /// 同上，但可以塞一个**自己造的** tee 落点（`阻-2` 那条判据要一个**会阻塞**的落点）。
    fn spawn_relay_with_sink(
        up: SocketAddr,
        custom: Option<Box<dyn Write + Send>>,
    ) -> (SocketAddr, Arc<Relay>, TeeTap) {
        let buf = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (tick, rx) = mpsc::channel();
        struct Shared(Arc<std::sync::Mutex<Vec<u8>>>, mpsc::Sender<()>);
        impl Write for Shared {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().expect("lock").extend_from_slice(b);
                let _ = self.1.send(());
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let w: Box<dyn Write + Send> =
            custom.unwrap_or_else(|| Box::new(Shared(Arc::clone(&buf), tick)));
        let base = Base::parse(&format!("http://127.0.0.1:{}", up.port())).expect("base");
        let relay = Arc::new(Relay::new(base, TeeSink::new(w)));
        let listener = listen(0).expect("listen");
        let addr = listener.local_addr().expect("addr");
        let r2 = Arc::clone(&relay);
        std::thread::spawn(move || serve(listener, r2));
        (addr, relay, TeeTap { buf, rx })
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
        let (relay_addr, relay, tee) = spawn_relay(up.addr);
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
        // 两发响应，每发 1 行 meta + `UPSTREAM_EVENTS` 行事件 ⇒ 等够这么多行再读缓冲区。
        tee.wait_lines(2 * (1 + UPSTREAM_EVENTS));
        let text = tee.text();
        // ★ 只认**事件行**，不认 meta 行。
        //
        // 这一条是变异台逼出来的：第一版按「行里出现这个键」认，而每个响应开头那行
        // `__meta__` 本来就带真键 ⇒ 把**事件**的落点写死成一个键，两边照样各自有行、全绿。
        // 那正是「断言从『落在哪个键上』滑成『有没有到达』」那个瞎法，只是滑在 tee 这一侧。
        let events = tee.event_lines();
        let a: Vec<&String> = events.iter().filter(|l| l.contains("sid-AAA")).collect();
        let b: Vec<&String> = events.iter().filter(|l| l.contains("sid-BBB")).collect();
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

        // ★★ `阻-4(D3)`：`DoD-3㈠` acceptor 逐字那半句 ——
        //    「其后每行可解析且 **`event` 数 == 假上游发出的事件数**」。
        //
        // # 这一格先前**零判据**，而它的逃逸射程是实测出来的
        //
        // 上面那几条只断「非空 + 不交叉 + 无第三落点」⇒ **每批少抄若干条没有人发现**：
        // D3 `MU5`（`splitter.feed(...)` 取完 batch 再 `batch.pop()`，下游字节一个不少）
        // ⇒ **392 全绿**，非空对照里逐字有「这一批 3 条，丢掉 Some("{\"i\":3}")，剩 2」。
        // 而「**一条都不抄**」会红（PM 重打：390 passed / 2 failed）——
        // 那两条红是被「tee 非空」顺带接住的，**不是设计出来的**。
        // ⇒ 今天补的就是中间那一大段：**数对不上就红**。
        //
        // 分母：上游**自己数**的事件数（`up.events()`，夹具在每次 `send_chunk` 事件时 +1），
        // 不是判据里写死的常量 —— 拿常量当期望值，夹具少发了也照样绿。
        let up_events = up.events();
        // 非空对照：夹具自己得真发出这么多事件，否则下面那条是空真。
        assert_eq!(
            up_events,
            2 * UPSTREAM_EVENTS as u64,
            "夹具自检：两发响应共必须发出 {} 个事件",
            2 * UPSTREAM_EVENTS
        );
        assert_eq!(
            events.len() as u64,
            up_events,
            "tee 的事件行数必须等于上游发出的事件数（上游 {up_events} · tee {}）——\
             少一条就是 tee 漏抄了，而下游的字节可以一个不少：{events:?}",
            events.len()
        );
        // ⚠ 顺带钉住「丢是可见的」：本条这一趟不该有任何 `__dropped__` 行。
        //    队列 1024 行、这里一共 8 行 ⇒ 出现它就是别的地方坏了。
        assert!(
            !text.contains("__dropped__"),
            "这一趟不该丢任何行：{text:?}"
        );
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

    /// 发一条**自己拼的**请求（`send_request` 那条固定拼 `Content-Length`，这里要能拼坏的）。
    ///
    /// 返回 `(拿到的字节, 是不是干净收尾)`。**第二个值不是搭头**〔回修轮之五 08-25〕：
    /// 拒绝那几支要是没把请求字节排干净，`close` 发的是 **RST**，下游那边的 `read` 拿到
    /// `ConnectionReset` —— 有些内核/时序下**连已经缓冲的那句 503 都会被丢掉**。
    /// ⇒ 「拒绝有声」这条性质要的是「拿到 503 **且** 干净收尾」，两个都要断。
    /// 〔死值验：只断第一个值时，把 `respond_and_drain` 的排整个关掉 ⇒ **0 红 / 405**
    ///  —— 那条排就成了一处没人守的代码。补上这一格之后它才有牙（`MU13`）。〕
    fn send_raw(addr: SocketAddr, raw: &str) -> (String, bool) {
        let mut c = TcpStream::connect(addr).expect("connect relay");
        c.set_nodelay(true).expect("nodelay");
        // 风险 `5x`：走得到 `serve()` 的判据一律带读期限，把「挂住」换成「红」。
        c.set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .expect("read deadline（风险 5x）");
        c.write_all(raw.as_bytes()).expect("write req");
        c.flush().expect("flush");
        // ★ **读到出错为止，把已经拿到的留着** —— 不用 `read_to_string`（它把错误连同
        //   已读到的字节一起丢掉）。理由：拒绝那几支要是没把请求字节排干净，`close` 发的是
        //   **RST**，`read_to_string` 就只剩一个 `ConnectionReset`，而「拿到了 503 然后被 RST」
        //   与「一个字节都没拿到」在断言里会长得一模一样 —— 那正是本条要分开的两件事。
        //   〔生产侧的处置见 `respond_and_drain`；这里是判据侧不让读数被吃掉。〕
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        let clean = loop {
            match c.read(&mut buf) {
                Ok(0) => break true,
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(_) => break false,
            }
        };
        (String::from_utf8_lossy(&out).to_string(), clean)
    }

    /// ★★ `阻-1(D3)` 的**行为格**：一条下游请求不许把整个中转进程打掉。
    ///
    /// # 为什么这一条与 `http1` 那条**不是同一格**，两条都要
    ///
    /// `http1::…::an_oversized_content_length_is_refused_without_allocating_it` 判的是
    /// **那个函数**的三张脸；本条判的是 **`handle` 有没有把那张脸接对**
    /// —— 把 `None => return respond_status(…413…)` 改成 `None => Vec::new()`
    /// （「当成没有请求体」）时，那条单元判据**照样绿**，而下游会拿到一条**正常的 200**。
    ///
    /// # 分母与非空对照
    ///
    /// - 超上限用的值是 `1_000_000_000_000`（正是 D3 那一发）。它今天走不到分配那一步；
    ///   **没有上限的版本上这一发会让进程 SIGABRT**（我自己重打过，逐字读数见件文件 §8.20.1）。
    /// - 非空对照：**同一条路**上一发正常的请求必须拿到 200 且真的打到上游
    ///   —— 否则「上游没被碰」是空真（夹具坏了 / 中转没起来也会绿）。
    #[test]
    fn a_body_larger_than_the_cap_is_refused_with_413_and_never_reaches_upstream() {
        let up = spawn_fake_upstream(None);
        let (relay_addr, _relay, _tee) = spawn_relay(up.addr);

        // 非空对照先打一发：这条路是通的，上游的记录面是活的。
        let mut warm = send_request(relay_addr, "/s/agentA/sid-AAA/v1/messages", "");
        let mut sink0 = Vec::new();
        warm.read_to_end(&mut sink0).expect("read warmup");
        assert!(
            String::from_utf8_lossy(&sink0).starts_with("HTTP/1.1 200"),
            "非空对照：一发正常请求必须拿到 200"
        );
        assert_eq!(up.seen.lock().expect("lock").len(), 1, "非空对照：真打到上游");

        // 正题：一条 `Content-Length: 1e12`，其余**一个字节都不发**。
        let (got, clean) = send_raw(
            relay_addr,
            "POST /s/agentA/sid-AAA/v1/messages HTTP/1.1\r\nHost: relay\r\nContent-Length: 1000000000000\r\n\r\n",
        );
        assert!(
            got.starts_with("HTTP/1.1 413"),
            "超 `BODY_CAP` 必须回 413（拿到的是：{got:?}）"
        );
        assert!(clean, "413 要**送得到**：连接得干净收尾，不是被 RST 打断（拿到的是：{got:?}）");
        assert_eq!(
            up.seen.lock().expect("lock").len(),
            1,
            "被拒的那一发不该碰上游（计数必须还是 1）"
        );
    }

    /// ★ `重要-2(D3)` 的**行为格**：`Content-Length` 读不懂 ⇒ 400，**不许**静默把请求体丢掉。
    ///
    /// 先前这一形下：上游收到**空请求体**、下游拿到一条**完全正常的 200**、全程零日志零 4xx。
    /// 中转搬的正是 `POST /v1/messages` 的载荷 ⇒ 载荷整个消失而客户端毫不知情。
    ///
    /// 非空对照与上一条同族：先打一发正常的，证明这条路与上游的记录面都是活的。
    #[test]
    fn an_unparsable_content_length_is_refused_with_400_instead_of_dropping_the_body() {
        let up = spawn_fake_upstream(None);
        let (relay_addr, _relay, _tee) = spawn_relay(up.addr);

        let mut warm = send_request(relay_addr, "/s/agentA/sid-AAA/v1/messages", "");
        let mut sink0 = Vec::new();
        warm.read_to_end(&mut sink0).expect("read warmup");
        assert_eq!(up.seen.lock().expect("lock").len(), 1, "非空对照：真打到上游");
        assert_eq!(
            up.bodies.lock().expect("lock")[0],
            REQUEST_BODY.as_bytes(),
            "非空对照：上游那一侧真的看得见请求体（否则下面那条是空真）"
        );

        let (got, clean) = send_raw(
            relay_addr,
            "POST /s/agentA/sid-AAA/v1/messages HTTP/1.1\r\nHost: relay\r\nContent-Length: 7abc\r\n\r\n{\"m\":1}",
        );
        assert!(
            got.starts_with("HTTP/1.1 400"),
            "读不懂的 Content-Length 必须回 400（拿到的是：{got:?}）"
        );
        assert!(clean, "400 要**送得到**：连接得干净收尾，不是被 RST 打断（拿到的是：{got:?}）");
        assert_eq!(
            up.seen.lock().expect("lock").len(),
            1,
            "读不懂的那一发不该带着一个**空请求体**打到上游（计数必须还是 1）"
        );
    }

    /// 一个**按脚本发字节**的假上游：先原样吐 `script`，再吐一条最小 SSE 响应。
    /// 只给 `重要-1(D3)`（1xx）那条判据用 —— `spawn_fake_upstream` 的形状里塞不进「前缀」。
    fn spawn_scripted_upstream(script: &'static str) -> SocketAddr {
        let listener = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind");
        let addr = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            for s in listener.incoming() {
                let Ok(mut s) = s else { continue };
                let mut r = BufReader::new(s.try_clone().expect("clone"));
                let mut clen = 0usize;
                let mut line = String::new();
                r.read_line(&mut line).expect("request line");
                loop {
                    let mut h = String::new();
                    let n = r.read_line(&mut h).expect("header");
                    if n == 0 || h == "\r\n" {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                        clen = v.trim().parse().unwrap_or(0);
                    }
                }
                // 把请求体读干净 ⇒ drop 时发 FIN 不是 RST（理由同 `spawn_fake_upstream`）。
                let mut body = vec![0u8; clen];
                let _ = r.read_exact(&mut body);
                s.write_all(script.as_bytes()).expect("script");
                s.flush().expect("flush");
                s.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\ndata: {\"i\":1}\n\n",
                )
                .expect("real response");
                s.flush().expect("flush");
            }
        });
        addr
    }

    /// ★★ `重要-1(D3)`：上游的 **1xx 中间响应**不许被当成最终响应。
    ///
    /// # 先前是什么形状（D3 实测）
    ///
    /// 下游**逐字**拿到
    /// `"HTTP/1.1 100 Continue\r\nConnection: close\r\n\r\nHTTP/1.1 200 OK\r\n…"`
    /// —— 真正的 200 与它全部响应头**沦为响应体**，客户端拿到一条不可解析的响应。
    /// ⚠ 而同一趟的 **tee 完全正常** ⇒ 这一形靠看日志/tee 是发现不了的。
    ///
    /// # 分母
    ///
    /// 我打了 **3** 形：`100 Continue`（最常见，`Expect: 100-continue` 一转发就撞上）·
    /// `103 Early Hints`（带头的 1xx）· **连续两条** 1xx（跳一条不够）。
    /// 每一形都断**同一件事**：下游拿到的是 `HTTP/1.1 200`，且**响应体里不许再出现状态行**。
    #[test]
    fn an_interim_1xx_response_is_skipped_instead_of_being_sent_as_the_final_one() {
        // 分母 = 这 3 形。期望值全是手写字面量。
        let scripts = [
            "HTTP/1.1 100 Continue\r\n\r\n",
            "HTTP/1.1 103 Early Hints\r\nLink: </s>; rel=preload\r\n\r\n",
            "HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 100 Continue\r\n\r\n",
        ];
        for script in scripts {
            let up = spawn_scripted_upstream(script);
            let (relay_addr, _relay, _tee) = spawn_relay(up);
            let mut c = send_request(relay_addr, "/s/agentA/sid-AAA/v1/messages", "");
            let mut got = String::new();
            c.read_to_string(&mut got).expect("read");
            assert!(
                got.starts_with("HTTP/1.1 200"),
                "1xx 必须被跳过、下游要拿到真正的最终响应；脚本 {script:?} ⇒ 拿到 {got:?}"
            );
            // ★ 这一条才是要害：**响应体里不许再出现一条状态行**。
            //   只断开头是不够的 —— 「把 100 当最终响应」那一形里，真 200 就藏在体里。
            let body = got.split("\r\n\r\n").nth(1).unwrap_or("");
            assert!(
                !body.contains("HTTP/1.1"),
                "响应**体**里出现了状态行 ⇒ 有一条响应头被当成体透传了：{got:?}"
            );
            assert!(
                got.contains("data: {\"i\":1}"),
                "真正的那条响应体要完整到得了下游：{got:?}"
            );
        }
    }

    /// ★ `INTERIM_RESPONSES_ALLOWED` 那一格〔铁律 15 自查补的：**我自己写的那条上限先前零判据**〕。
    ///
    /// 上一条判据只喂到 **2** 条 1xx，够不到上限 ⇒ 把 `if interim > INTERIM_RESPONSES_ALLOWED`
    /// 整支拿掉，上一条照样绿，而真机后果是一个坏上游能让中转在那个循环里**一直读下去**。
    /// ⇒ 这一条喂 **9 条**（上限的手写字面量 8 + 1），断它回 **502**。
    ///
    /// ⚠ 期望值 `9` 是**手写字面量**，不是拿 `INTERIM_RESPONSES_ALLOWED` 算的
    /// —— 拿被测常量算期望值，改了常量本条会跟着漂、永远绿。
    #[test]
    fn too_many_interim_responses_are_refused_with_502() {
        // 9 条 1xx（上限是 8）。分母：`"HTTP/1.1 100 Continue\r\n\r\n"` 重复 9 次。
        let script = concat!(
            "HTTP/1.1 100 Continue\r\n\r\n",
            "HTTP/1.1 100 Continue\r\n\r\n",
            "HTTP/1.1 100 Continue\r\n\r\n",
            "HTTP/1.1 100 Continue\r\n\r\n",
            "HTTP/1.1 100 Continue\r\n\r\n",
            "HTTP/1.1 100 Continue\r\n\r\n",
            "HTTP/1.1 100 Continue\r\n\r\n",
            "HTTP/1.1 100 Continue\r\n\r\n",
            "HTTP/1.1 100 Continue\r\n\r\n",
        );
        assert_eq!(
            script.matches("100 Continue").count(),
            9,
            "夹具自检：得是 9 条"
        );
        let up = spawn_scripted_upstream(script);
        let (relay_addr, _relay, _tee) = spawn_relay(up);
        let mut c = send_request(relay_addr, "/s/agentA/sid-AAA/v1/messages", "");
        let mut got = String::new();
        c.read_to_string(&mut got).expect("read");
        assert!(
            got.starts_with("HTTP/1.1 502"),
            "1xx 多到超过上限就该回 502（拿到的是：{got:?}）"
        );
    }

    /// ★ `TEE_DECODE_CAP` 那条**接线**〔铁律 15 自查补的：两个上限各自有单元判据，
    /// 而「`handle` 有没有把丢掉的字节接到 tee 的 `__dropped__` 上」**先前零判据**〕。
    ///
    /// 把 `relay.tee.note_dropped_bytes(view.take_dropped() + splitter.take_dropped());`
    /// 整行换成一个 `let _ = …`，那两条单元判据**照样绿**，而真机后果是 tee 少了一大段
    /// 却**一个字都不说** —— 正是 `DoD-3㈠` 那笔账对不上的成因。
    ///
    /// # 量法与代价
    ///
    /// 上游发一条**永不换行**、比 `TEE_DECODE_CAP` 长的 `data:` 行（本条 9 MiB）。
    /// 这是本轮**最贵**的一条判据（要真搬 9 MiB 过回环，实测把全量从 1.1s 抬到 ~4s），
    /// 但它是唯一能碰到那条接线的路：上限是生产常量，注不进去。
    ///
    /// 非空对照：**下游必须一个字节不少地拿到那 9 MiB** ——
    /// tee 丢的那一路与转发那一路是两条路，这一格钉的就是「丢的只是 tee」。
    #[test]
    fn an_over_cap_sse_line_is_reported_on_the_tee_stream_while_downstream_keeps_every_byte() {
        const PAYLOAD: usize = 9 * 1024 * 1024; // 手写字面量，比 TEE_DECODE_CAP 大
        let listener = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind");
        let up_addr = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            for s in listener.incoming() {
                let Ok(mut s) = s else { continue };
                let mut r = BufReader::new(s.try_clone().expect("clone"));
                let mut clen = 0usize;
                let mut line = String::new();
                r.read_line(&mut line).expect("request line");
                loop {
                    let mut h = String::new();
                    let n = r.read_line(&mut h).expect("header");
                    if n == 0 || h == "\r\n" {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                        clen = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0u8; clen];
                let _ = r.read_exact(&mut body);
                s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n")
                    .expect("head");
                s.write_all(b"data: ").expect("prefix");
                // 一条**永不换行**的超长行。
                let block = vec![b'x'; 64 * 1024];
                let mut sent = 0usize;
                while sent < PAYLOAD {
                    s.write_all(&block).expect("payload");
                    sent += block.len();
                }
                s.flush().expect("flush");
            }
        });
        let (relay_addr, _relay, tee) = spawn_relay(up_addr);
        let mut c = send_request(relay_addr, "/s/agentA/sid-AAA/v1/messages", "");
        let mut got = Vec::new();
        c.read_to_end(&mut got).expect("read");
        // 非空对照：转发那一路一个字节都不许少（tee 丢的是**另一条路**）。
        assert!(
            got.len() >= PAYLOAD,
            "下游只拿到 {} 字节，上游至少发了 {PAYLOAD} —— 转发那一路被 tee 的丢连累了",
            got.len()
        );
        // 正题：tee 流上必须有一行 `__dropped__` 把丢掉的字节说出来。
        assert!(
            wait_until(|| tee.text().contains("__dropped__")),
            "超解码上限丢掉的字节必须在 tee 流上报出来，tee 现在是：{:?}",
            tee.text()
        );
        let text = tee.text();
        let note = text
            .lines()
            .find(|l| l.contains("__dropped__"))
            .expect("那一行");
        let v: serde_json::Value = serde_json::from_str(note)
            .unwrap_or_else(|e| panic!("`__dropped__` 行必须可解析（DoD-3㈠）：{note:?} ⇒ {e}"));
        let bytes = v["__dropped__"]["bytes"].as_u64().expect("bytes 是个数");
        assert!(
            bytes > 0,
            "非空对照：报出来的丢字节数必须 > 0（这一趟丢的是解码缓冲那一路）：{note}"
        );
    }

    /// ★★ `阻-2(D3)`：**一个卡住的 tee 消费者不许拖停转发**（同连接 + 跨连接两半都要）。
    ///
    /// # 先前是什么形状
    ///
    /// `write_line` 在**持锁**状态下做阻塞写，而那把锁跨连接共享、`event()` 又是在 `pump` 的
    /// `on_chunk` 里**同步**调的 ⇒ 一个慢消费者把**每一条**连接一起拖停。
    /// D3 实测逐字：`tee 不卡时 B 耗时 1 ms · tee 卡 3000ms 时 B 耗时 2701 ms`
    /// （住址 `audits/K-H1-D3.md#2.2`；我自己重打的读数见件文件 §8.20.3）。
    ///
    /// # 这条判据的量法与阈值是怎么定的
    ///
    /// tee 的落点在**第一次写**上睡 3000ms。断言两条连接**各自**在 1500ms 内走完。
    /// - 3000 / 1500 这个倍数就是这一格的地基：阈值要明显小于卡住的时长，
    ///   否则「没拖停」与「拖停了但没超阈值」分不开。
    /// - 失败信息把**实测毫秒数**印出来（本仓纪律：时序类断言必须印出实测值）。
    /// - ⚠ 它**不**证明 tee 的行最终写出去了 —— 那是别的判据的活（`routes_two_keys…` 在对账）。
    #[test]
    fn a_wedged_tee_consumer_stalls_neither_its_own_connection_nor_another() {
        /// 第一次写睡 3 秒的落点。**只睡第一次**：其后正常写。
        struct WedgedSink(std::sync::Arc<AtomicU64>);
        impl Write for WedgedSink {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                    std::thread::sleep(std::time::Duration::from_millis(3000));
                }
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let writes = std::sync::Arc::new(AtomicU64::new(0));
        let up = spawn_fake_upstream(None);
        let (relay_addr, _relay, _tee) =
            spawn_relay_with_sink(up.addr, Some(Box::new(WedgedSink(std::sync::Arc::clone(&writes)))));

        let mut ms = Vec::new();
        for key in ["sid-AAA", "sid-BBB"] {
            let t0 = std::time::Instant::now();
            let mut c = send_request(relay_addr, &format!("/s/agentA/{key}/v1/messages"), "");
            let mut got = Vec::new();
            c.read_to_end(&mut got).expect("read");
            let el = t0.elapsed().as_millis() as u64;
            assert!(
                String::from_utf8_lossy(&got).starts_with("HTTP/1.1 200"),
                "这一趟得真走完一条转发，否则下面量的是半条连接：{:?}",
                String::from_utf8_lossy(&got)
            );
            ms.push(el);
        }
        // 非空对照：那个落点**真的被写过**（否则「没卡住」可能只是 tee 整条路没走）。
        assert!(
            writes.load(Ordering::SeqCst) >= 1,
            "非空对照：卡住的那个落点一次都没被写过 ⇒ 本条量的不是 tee 这条路"
        );
        println!("[阻-2] tee 卡 3000ms 时：A 耗时 {} ms · B 耗时 {} ms", ms[0], ms[1]);
        assert!(
            ms[0] < 1500,
            "**同一条**连接被自己的 tee 拖停了：A 耗时 {} ms（tee 卡 3000ms）",
            ms[0]
        );
        assert!(
            ms[1] < 1500,
            "**另一条**连接被别人的 tee 拖停了（跨连接）：B 耗时 {} ms（tee 卡 3000ms）",
            ms[1]
        );
    }

    /// ★ `阻-3(D3)` 的**做得到的那一半**：在途连接数有上界，且**拒绝是出声的**。
    ///
    /// # 四格，逐格说它证什么
    ///
    /// ㈠ **半开也算在册** —— 一条只发半个请求头、永不发结尾空行的连接，`inflight` 必须涨。
    ///    （那正是 D3 那一形：`半开 64 条 ⇒ 线程 4 → 68`。）
    /// ㈡ **顶到上限 ⇒ 回 503**，不是静默 FIN。先前是 `let _ = …spawn(…)` 把失败整个吞掉。
    /// ㈢ **非空对照**：`上限 - 1` 的时候同一发请求必须拿到 **200** ——
    ///    没有它，「拿到 503」可能只是这条路本来就不通。
    /// ㈣ **计数会还** —— 一条正常连接走完之后 `inflight` 回到原值，否则上限会被慢慢耗光。
    ///
    /// ⚠⚠ **它证不了的那一半，写在这里**：真正能让半开连接**自己散掉**的是读/写期限，
    /// 而那要在生产段写 `Duration::from_*` ⇒ 要动 `no_timer_guard.rs::REGISTERED_DURATION_USES`，
    /// **那个文件不在本轮写区**。⇒ 今天的性质是「**顶不满、拒绝有声**」，
    /// **不是**「半开连接会自己散」。交回理由见件文件 §8.20.4。
    #[test]
    fn inflight_connections_are_capped_and_the_refusal_is_spoken() {
        let up = spawn_fake_upstream(None);
        let (relay_addr, relay, _tee) = spawn_relay(up.addr);

        // ㈠ 半开一条：只发半个请求头，**永不**发结尾空行、不关连接。
        let mut half = TcpStream::connect(relay_addr).expect("connect");
        half.write_all(b"POST /s/agentA/sid-AAA/v1/messages HTTP/1.1\r\nHost: relay\r\n")
            .expect("half head");
        half.flush().expect("flush");
        assert!(
            wait_until(|| relay.inflight.load(Ordering::SeqCst) >= 1),
            "半开连接必须算进在途数（今天是 {}）",
            relay.inflight.load(Ordering::SeqCst)
        );

        // ㈢ 非空对照先做：`上限 - 1` 时同一发请求必须拿到 200。
        relay
            .inflight
            .store(INFLIGHT_CONNECTIONS - 1, Ordering::SeqCst);
        let (got, _clean) = send_raw(
            relay_addr,
            "POST /s/agentA/sid-AAA/v1/messages HTTP/1.1\r\nHost: relay\r\nContent-Length: 7\r\n\r\n{\"m\":1}",
        );
        assert!(
            got.starts_with("HTTP/1.1 200"),
            "非空对照：还没到上限时这一发必须走得通：{got:?}"
        );
        // ⚠ 先等上一发那条连接线程**真的收工**（它收工时会 `fetch_sub` 一次）。
        //    不等就 `store` 的话，那一次减会落在我们设的值**之后** ⇒ 计数被减回 255，
        //    下面那一发就不会被拒 —— 本条第一版正是这么红的（拿到的是一条正常的 200）。
        assert!(
            wait_until(|| relay.inflight.load(Ordering::SeqCst) == INFLIGHT_CONNECTIONS - 1),
            "上一发的连接线程没收工（在途数停在 {}）",
            relay.inflight.load(Ordering::SeqCst)
        );

        // ㈡ 顶到上限：新连接必须拿到 **503**，不是一个没有任何响应的 FIN。
        relay.inflight.store(INFLIGHT_CONNECTIONS, Ordering::SeqCst);
        let (got, clean) = send_raw(
            relay_addr,
            "POST /s/agentA/sid-AAA/v1/messages HTTP/1.1\r\nHost: relay\r\nContent-Length: 7\r\n\r\n{\"m\":1}",
        );
        assert!(
            got.starts_with("HTTP/1.1 503"),
            "顶到上限必须回 503（**静默 FIN 会让这里拿到空串**）：{got:?}"
        );
        // ★ 这一格钉的是 `respond_and_drain` 里那段「非阻塞地把请求字节排掉」：
        //   不排的话 `close` 发 RST，下游拿到的可能只是一个 `ConnectionReset`
        //   —— 「拒绝有声」就又退回成「拒绝」。死值验见件文件 §8.20.5 的 `MU13`。
        assert!(
            clean,
            "503 要**送得到**：连接得干净收尾，不是被 RST 打断（拿到的是：{got:?}）"
        );

        // ㈣ 计数会还：把它放回 0，跑一发正常的，走完之后必须回到 0。
        relay.inflight.store(0, Ordering::SeqCst);
        let mut c = send_request(relay_addr, "/s/agentA/sid-AAA/v1/messages", "");
        let mut sink = Vec::new();
        c.read_to_end(&mut sink).expect("read");
        assert!(
            wait_until(|| relay.inflight.load(Ordering::SeqCst) == 0),
            "连接走完之后在途数必须回落（今天是 {}）",
            relay.inflight.load(Ordering::SeqCst)
        );
        drop(half);
    }

    /// 等一个条件成立，最多 4 秒。**等不到回 `false`**，调用方当红处理。
    /// （只住测试段：生产段一个 sleep 都不许有，`no_timer_guard` 钉着。）
    fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
        for _ in 0..2000 {
            if cond() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        cond()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // ★★ 真子进程的中转 —— `阻-5(D3)` 与 `重要-4(D3)` 的**唯一**可行台子
    // ─────────────────────────────────────────────────────────────────────────

    /// 子进程标记。**平时（没有它）那个入口判据直接返回**，不会有人不小心把测试台变成中转。
    const CHILD_MARK: &str = "CCM_RELAY_TEST_CHILD";
    /// `--exact` 要的全名。写错了会**红**而不会静默变绿：子进程一条测试都不跑
    /// ⇒ 永远等不到那句 `listening on` ⇒ 下面 `spawn_relay_child` 当场 panic。
    const CHILD_TEST_NAME: &str = "relay::server::tests::relay_child_process_entry_point";

    /// ★ 子进程入口：把**测试二进制自己**当中转进程重新拉起来。
    ///
    /// # 它不是判据，是一个入口 —— 所以标了 `#[ignore]`
    ///
    /// 标 `#[ignore]` 是刻意的：它在正常那一趟里**一条断言都不跑**，
    /// 算成 `passed` 就是往门禁里塞一条恒绿的仪式。⇒ 让它算 `ignored`。
    ///
    /// # 它走的是**真入口** `run()`
    ///
    /// 顺带把两格长期登记为「判不了」的东西变成**量得到**的（登记住址件文件 §8.18.3 / §8.18.9）：
    /// ①「`run()` 真的去读了那两个环境变量」—— 今天子进程只拿到环境变量，没有别的入口；
    /// ②「`run_with` 成功那一条路上 `TeeSink::to_stdout()` 那根接线」—— 子进程的 **stdout 就是 tee**，
    ///   判据在上面读得到真的事件行。
    #[test]
    #[ignore = "子进程入口：只在被父判据用 CCM_RELAY_TEST_CHILD 拉起时才当中转跑"]
    fn relay_child_process_entry_point() {
        if std::env::var(CHILD_MARK).is_err() {
            return;
        }
        // `run()` 自己去读 `CCM_RELAY_PORT` / `CCM_RELAY_UPSTREAM`。成功那条路永不返回。
        std::process::exit(run(&[]));
    }

    /// 一个跑在**真子进程**里的中转，连同它 stdout / stderr 的全量收集面。
    struct RelayChild {
        child: std::process::Child,
        addr: SocketAddr,
        out: Arc<std::sync::Mutex<String>>,
        err: Arc<std::sync::Mutex<String>>,
    }

    impl RelayChild {
        fn out(&self) -> String {
            self.out.lock().expect("lock").clone()
        }
        fn err(&self) -> String {
            self.err.lock().expect("lock").clone()
        }
    }

    impl Drop for RelayChild {
        fn drop(&mut self) {
            // 断言炸了也要把子进程收掉，别在门禁机器上留孤儿。
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// 起一个子进程中转，指向 `up`。**端口给 0**（内核选）——
    /// 判据从子进程 stderr 上那句 `listening on` 里读回真端口，
    /// 这样就没有「先探一个空闲端口再去绑」的竞态。
    fn spawn_relay_child(up: SocketAddr) -> RelayChild {
        let exe = std::env::current_exe().expect("测试二进制自己的路径");
        let mut child = std::process::Command::new(exe)
            .args([
                CHILD_TEST_NAME,
                "--exact",
                "--ignored",
                // ⚠ **`--nocapture` 是这台子的地基**：不给它，子进程里 libtest 会把
                //   `eprintln!` 收进自己的捕获（**连 spawn 出来的线程也收**，我实测过，
                //   读数见件文件 §8.20.1㈡）⇒ 管道上一个字节都读不到，
                //   这条「日志里不许有哨兵串」的判据就成了一条**空真**。
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD_MARK, "1")
            .env("CCM_RELAY_PORT", "0")
            .env("CCM_RELAY_UPSTREAM", format!("http://127.0.0.1:{}", up.port()))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("起中转子进程");

        let out = Arc::new(std::sync::Mutex::new(String::new()));
        let err = Arc::new(std::sync::Mutex::new(String::new()));
        let (tx, rx) = mpsc::channel::<String>();

        let so = child.stdout.take().expect("stdout pipe");
        let out_c = Arc::clone(&out);
        std::thread::spawn(move || {
            for line in BufReader::new(so).lines().map_while(Result::ok) {
                out_c.lock().expect("lock").push_str(&line);
                out_c.lock().expect("lock").push('\n');
            }
        });
        let se = child.stderr.take().expect("stderr pipe");
        let err_c = Arc::clone(&err);
        std::thread::spawn(move || {
            for line in BufReader::new(se).lines().map_while(Result::ok) {
                if line.contains("listening on") {
                    let _ = tx.send(line.clone());
                }
                err_c.lock().expect("lock").push_str(&line);
                err_c.lock().expect("lock").push('\n');
            }
        });

        // 风险 `5x` 的同一条纪律：等不到就**红**，不许挂住。
        let line = rx
            .recv_timeout(std::time::Duration::from_secs(20))
            .expect("子进程没在 20s 内印出 `listening on` —— 它可能根本没跑到中转那一支");
        let addr: SocketAddr = line
            .rsplit(' ')
            .next()
            .expect("listening 行的末段")
            .parse()
            .unwrap_or_else(|e| panic!("`listening on` 那行末段不是地址：{line:?} ⇒ {e}"));
        RelayChild {
            child,
            addr,
            out,
            err,
        }
    }

    /// ★★ `阻-5(D3)`：`DoD-3㈡` acceptor 逐字那半句 ——
    /// 「tee 文件 **+ 中转全部日志/stderr** 里**零出现**该哨兵串」。
    ///
    /// # 这一格先前**零牙**，而且是被自己那句「怎么让它失败」照出来的
    ///
    /// `DoD-3` 逐字写的「怎么让它失败」是「加一行**把 headers 也 dump 一份**」。
    /// D3 与 PM 各自打了这一刀（`eprintln!("[relay] … headers: {{:?}}", head.headers);`）：
    /// **392 全绿**，而同一趟 `--nocapture` 的输出里**逐字**印着
    /// `Authorization: Bearer SECRET-TOKEN-DO-NOT-LEAK`。
    /// 成因：`the_auth_header_is_forwarded_but_never_teed` 的采集面**只有 tee sink**，
    /// **一个字都不读 stderr**。
    ///
    /// # 为什么必须是**真子进程**（这是量出来的，不是选出来的）
    ///
    /// 进程内做不到：libtest 的输出捕获是**跟着 `std::thread::spawn` 传下去**的，
    /// 而 `serve()` 每连接 spawn 一条线程、`handle` 的 `eprintln!` 就在那条线程上。
    /// 我用 `libc::dup2` 把 fd 2 接到管道上量过两趟：默认跑法收到 **0** 字节、
    /// `--nocapture` 收到 **47** 字节（逐字读数见件文件 §8.20.1㈡）。
    /// ⇒ 进程内的 fd-2 重定向抓不到那一刀，做出来会是一条**看着很像判据的空判据**。
    ///
    /// # 采集面 = 子进程的 **stderr 全量 + stdout 全量**
    ///
    /// stdout 那一半不是搭头：子进程的 stdout **就是 tee**（`run_with` 里那唯一一处
    /// `TeeSink::to_stdout()`）⇒ 这一条同时覆盖了 `DoD-3㈡` 的两半，
    /// 而且是在**真的那根接线**上量的，不是在测试自己造的 `Vec<u8>` sink 上。
    ///
    /// # 三条非空对照（缺一条这里就可能是空真）
    ///
    /// ① 哨兵串**真的走过这条路** —— 假上游那侧看见 `auth=true`；
    /// ② stderr 的采集面**是活的** —— 里面有那句 `listening on`；
    /// ③ stdout 的采集面**是活的** —— 里面有真的 tee 事件行。
    #[test]
    fn a_sentinel_auth_header_shows_up_in_neither_the_relay_processs_stderr_nor_its_stdout() {
        const SENTINEL: &str = "SECRET-TOKEN-DO-NOT-LEAK";
        let up = spawn_fake_upstream(None);
        let relay = spawn_relay_child(up.addr);

        let mut c = send_request(
            relay.addr,
            "/s/agentA/sid-AAA/v1/messages",
            &format!("Authorization: Bearer {SENTINEL}\r\n"),
        );
        let mut got = Vec::new();
        c.read_to_end(&mut got).expect("read");
        assert!(
            String::from_utf8_lossy(&got).starts_with("HTTP/1.1 200"),
            "这一趟得真走完一条转发：{:?}",
            String::from_utf8_lossy(&got)
        );

        // 非空对照①：哨兵串真的经过了中转，上游那侧看见了 auth 头。
        let seen = up.seen.lock().expect("lock").clone();
        assert_eq!(seen.len(), 1, "上游必须收到那一发：{seen:?}");
        assert!(seen[0].ends_with("auth=true"), "auth 头必须被转发：{seen:?}");
        // 非空对照③：stdout（= tee）上真的出现了事件行 —— 等它到，等不到当红。
        assert!(
            wait_until(|| relay.out().contains("\"event\"")),
            "非空对照：子进程 stdout（tee 的真落点）上一条事件行都没有 —— \
             采集面是死的，下面那条「零出现」就是空真。stdout 现在是：{:?}",
            relay.out()
        );

        let err = relay.err();
        let out = relay.out();
        // 非空对照②：stderr 的采集面是活的。
        assert!(
            err.contains("listening on"),
            "非空对照：子进程 stderr 一个字都没收到 —— 采集面是死的：{err:?}"
        );
        // ★ 正题：两条流上都不许出现哨兵串，也不许出现头名。
        assert!(!err.contains(SENTINEL), "哨兵串泄漏进了中转的 stderr：{err:?}");
        assert!(!out.contains(SENTINEL), "哨兵串泄漏进了中转的 stdout：{out:?}");
        assert!(
            !err.contains("Authorization"),
            "中转的 stderr 里出现了请求头名：{err:?}"
        );
        assert!(
            !out.contains("Authorization"),
            "中转的 stdout 里出现了请求头名：{out:?}"
        );
    }

    /// ★ `重要-4(D3)`：`DoD-1㈢` acceptor 逐字那半句「两次请求由**同一个中转进程**服务
    /// （**进程数 = 1**）」。
    ///
    /// # 先前量的是别的东西
    ///
    /// `routes_two_keys…` 断的是 `relay.served() == 2` —— 那是「同一个 **`Relay` 实例**」。
    /// 全量测试跑在**一个**测试进程里 ⇒ 「进程数 = 1」这句话**没有任何判据在量**。
    ///
    /// # 今天量的是什么（三格，逐格说它证什么）
    ///
    /// ㈠ **只有一个中转进程**：子进程 stderr 里 `listening on` **恰好 1 行**
    ///    （一个端口只可能有一个监听者，本 crate 不用 `SO_REUSEPORT`）。
    /// ㈡ **两个键都从这一个进程走**：两发都到了上游，且路径都被剥了前缀。
    /// ㈢ ★ **两发共享同一份进程内状态**：tee 的两行 `__meta__` 的 `seq` 是 **0 和 1**。
    ///    `seq` 是 `TeeSink` 的实例字段，而 `TeeSink` 住在 `Relay` 里、由 `serve()` 跨连接
    ///    `Arc::clone` —— 谁要是改成「每连接一个新 `Relay`」，这里就会看到两个 `0`。
    ///    ⇒ 这一格才是**有牙**的那一格（死值验见件文件 §8.20.5）。
    ///
    /// ⚠ **它证不了**「换成多进程模型会红」—— 那种改动不是一次小改动，我构造不出
    /// 一刀**只打这一格**的最小面变异。如实写在这里，不写成「进程数=1 已验」。
    #[test]
    fn one_relay_process_serves_both_keys_and_shares_its_tee_sequence() {
        let up = spawn_fake_upstream(None);
        let relay = spawn_relay_child(up.addr);

        for key in ["sid-AAA", "sid-BBB"] {
            let mut c = send_request(
                relay.addr,
                &format!("/s/agentA/{key}/v1/messages?beta=true"),
                "",
            );
            let mut got = Vec::new();
            c.read_to_end(&mut got).expect("read");
            assert!(
                String::from_utf8_lossy(&got).starts_with("HTTP/1.1 200"),
                "{key} 那一发得走完：{:?}",
                String::from_utf8_lossy(&got)
            );
        }

        // ㈡ 两发都到了上游，路径都剥了前缀。期望值是**手写字面量**。
        let seen = up.seen.lock().expect("lock").clone();
        assert_eq!(seen.len(), 2, "两个键都要打到上游：{seen:?}");
        for line in &seen {
            assert_eq!(line, "POST /v1/messages?beta=true HTTP/1.1 auth=false");
        }

        // ㈢ 等两行 meta 都到 stdout（那是真的 `TeeSink::to_stdout()` 那根接线）。
        assert!(
            wait_until(|| relay.out().matches("\"__meta__\"").count() >= 2),
            "两发响应必须在 tee 上各留一行 meta，stdout 现在是：{:?}",
            relay.out()
        );
        let out = relay.out();
        let metas: Vec<&str> = out.lines().filter(|l| l.contains("\"__meta__\"")).collect();
        assert_eq!(metas.len(), 2, "meta 行必须恰好两行：{metas:?}");
        assert!(
            metas[0].contains("\"seq\":0") && metas[1].contains("\"seq\":1"),
            "两发必须共享**同一个进程里的同一份** tee 序号（该是 0 和 1）：{metas:?}"
        );
        assert!(
            metas[0].contains("sid-AAA") && metas[1].contains("sid-BBB"),
            "两行 meta 要各自带自己的键：{metas:?}"
        );

        // ㈠ 只有**一个**中转进程在听。
        let err = relay.err();
        assert_eq!(
            err.matches("listening on").count(),
            1,
            "起来的中转必须**只有一个**（`listening on` 恰好一行）：{err:?}"
        );
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
    /// ㈠ **逐块门闩（因果证明）**：假上游发**每一块**之前都要等下游确认收到上一块，
    ///    **发完最后一块之后再等一次**（带 6s 上限）。
    ///    ⇒ 中转攒住其中任何一块，下游就再也等不到下一块 ⇒ 读期限到 ⇒ 红。
    ///    比「比较两个时间戳」更不 flaky。
    ///
    ///    ⚠ **订正射程**〔回修轮之四 08-25，D2 `重要-1(D2)`〕：先前这里逐字写「而且射程
    ///    覆盖到最后一块」，**那句话是假的，而且没有分母**。分母 = 一条响应的全部
    ///    **4** 块（夹具自己定义「终止块另算 ⇒ 块数 = `UPSTREAM_EVENTS + 1`」）；
    ///    实测攒第 2 块红 · 攒第 3 块红 · **攒第 4 块（终止块）⇒ 全绿**（D2 `D2M2`，
    ///    本轮在 392 条的盘面上重打，读数同）。成因是门闩的因果链在最后一块上断了：
    ///    最后一块没有「第 i+1 块」可等。⇒ 今天在 `spawn_fake_upstream` 里
    ///    **发完终止块之后再等一次确认**，射程才真的是 **4/4**；
    ///    死值验见件文件 §8.18.7（同一刀今天 1/392 红）。
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
        let (relay_addr, _relay, tee) = spawn_relay(up.addr);
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
        tee.wait_lines(1 + UPSTREAM_EVENTS);
        let text = tee.text();
        // ★ 活体条件要按**事件行**数，不是按「tee 非空」。
        //
        // 这一条也是变异台逼出来的（与上面路由那条同族）：每个响应开头那行 `__meta__`
        // 本来就会写出去 ⇒ 把 tee 的**事件**那一路整个掏空，「tee 非空」照样成立，
        // 本断言就退化成**空真**（闸死了 `[] == []` 也成立）。7u 那一趟实测到了这个形状。
        //
        // ⚠ 订正一格〔回修轮之五 08-25，`阻-4(D3)`〕：先前这里是 `events >= 1` ——
        // 那只挡得住「**一条都不抄**」，挡不住「每批少抄若干条」（D3 `MU5` 实测 392 全绿）。
        // 今天按**上游自己数的事件数**对账，分母同 `routes_two_keys…` 那条。
        let events = tee.event_lines();
        let up_events = up.events();
        assert_eq!(
            up_events, UPSTREAM_EVENTS as u64,
            "夹具自检：上游必须真发出 {UPSTREAM_EVENTS} 个事件"
        );
        assert_eq!(
            events.len() as u64,
            up_events,
            "tee 的事件行数必须等于上游发出的事件数：{events:?}"
        );
        assert!(!text.contains(SENTINEL), "哨兵串泄漏进了 tee");
        assert!(!text.contains("Authorization"), "tee 里不该有任何头名");
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

    /// ★★ `阻-3(D3)` **后半段**的行为格㈠〔回修轮之六 08-25〕：
    /// 两条 socket 上**真的**装了读写期限 —— `getsockopt` 读回来的，不是数源码。
    ///
    /// # 为什么必须是行为格
    ///
    /// 同 `both_directions_really_disable_nagle_on_the_socket` 踩过的那个坑：`production_code()`
    /// 只剥 `#[cfg(test)]` 段与**行首** `//` 的行 ⇒ 一行
    /// `let _note = "set_read_timeout(Some(DOWNSTREAM_DEADLINE))";` 就能把任何源码扫描喂饱。
    /// 量法也同它：`try_clone()` 是 `dup` ⇒ 两个 fd 指向**同一条** socket，
    /// 生产段 `setsockopt` 之后这个探针 `getsockopt` 读得到。**四格，每格各带非空对照。**
    ///
    /// # 它的射程（照实写，别让名字承诺它没有的覆盖）
    ///
    /// 它买的是「**这个数真的装到了那两条 socket 的两个方向上**」。
    /// 它**不证明**「期限到点之后那条读真的会返回、且没有任何一层重试」—— 那一半住
    /// `a_socket_deadline_makes_a_half_open_read_return_instead_of_wedging_the_thread`。
    /// ⚠ 两条合起来才推出「顶住的那些会自己散」；**没有任何一条判据端到端等满 30 秒**
    /// （那要跑 30 秒）⇒ 这个结论是**两格拼出来的**，不是一格量出来的。
    #[test]
    fn both_peers_really_carry_their_read_and_write_deadline_on_the_socket() {
        // ㈠ 下游方向：`handle()` 真的在**这一条** socket 上装了期限。
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
        // 非空对照：进 `handle` **之前**两个方向都**没有**期限（内核默认 `SO_*TIMEO` = 0）。
        // 没有这两格，下面那两句在「内核默认就带期限」的系统上是**空真**。
        assert_eq!(
            probe.read_timeout().expect("getsockopt"),
            None,
            "非空对照：accept 出来的 socket 默认该是**没有**读期限的"
        );
        assert_eq!(
            probe.write_timeout().expect("getsockopt"),
            None,
            "非空对照：accept 出来的 socket 默认该是**没有**写期限的"
        );
        let base = Base::parse(&format!("http://127.0.0.1:{}", up.addr.port())).expect("base");
        let relay = Relay::new(base, TeeSink::new(Box::new(std::io::sink())));
        handle(down, &relay).expect("handle 必须走完一条转发");
        assert_eq!(
            probe.read_timeout().expect("getsockopt"),
            Some(DOWNSTREAM_DEADLINE),
            "下游方向：`handle()` 必须在下游 socket 上真的装上**读**期限"
        );
        assert_eq!(
            probe.write_timeout().expect("getsockopt"),
            Some(DOWNSTREAM_DEADLINE),
            "下游方向：`handle()` 必须在下游 socket 上真的装上**写**期限 ——\
             写那半钉的是「客户端不读了」那一形（响应体写不出去 ⇒ `write_all` 永久阻塞）"
        );
        // ⚠ 先 drop 探针再 join：探针也是这条 socket 的一个 fd，不放手下游读不到 EOF。
        drop(probe);
        let got = client.join().expect("client thread");
        assert!(
            String::from_utf8_lossy(&got).starts_with("HTTP/1.1 200"),
            "这一趟得真走完一条转发，否则上面那四句量的是半条连接：{:?}",
            String::from_utf8_lossy(&got)
        );

        // ㈡ 上游方向：`upstream::connect()` 真的在**它自己**那条 socket 上装了期限。
        let peer = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind 假上游端");
        let a = peer.local_addr().expect("addr");
        let base2 = Base::parse(&format!("http://127.0.0.1:{}", a.port())).expect("base");
        match upstream::connect(&base2).expect("connect upstream") {
            upstream::Conn::Plain(s) => {
                assert_eq!(
                    s.read_timeout().expect("getsockopt"),
                    Some(upstream::UPSTREAM_DEADLINE),
                    "上游方向：`upstream::connect()` 必须装上**读**期限"
                );
                assert_eq!(
                    s.write_timeout().expect("getsockopt"),
                    Some(upstream::UPSTREAM_DEADLINE),
                    "上游方向：`upstream::connect()` 必须装上**写**期限 ——\
                     写那半钉的是 `up.write_all(&body)`（请求体最大 `BODY_CAP` = 64 MiB）"
                );
                // 非空对照：同一把尺子量一条**没被生产段碰过**的 socket ⇒ 必须两个方向都没期限。
                let raw = TcpStream::connect(a).expect("connect 对照");
                assert_eq!(
                    raw.read_timeout().expect("getsockopt"),
                    None,
                    "非空对照：没经生产段的 socket 默认该是**没有**读期限的"
                );
                assert_eq!(
                    raw.write_timeout().expect("getsockopt"),
                    None,
                    "非空对照：没经生产段的 socket 默认该是**没有**写期限的"
                );
            }
            upstream::Conn::Tls(_) => panic!("`http://` 该走明文那一支"),
        }
    }

    /// ★★ `阻-3(D3)` **后半段**的行为格㈡〔回修轮之六 08-25〕：**机制那一半**。
    ///
    /// 它买两样，都是行为：
    /// 1. socket 上有读期限时，`read_head` 面对一条**半开**连接（只发半个请求头、
    ///    **不关**连接）会**报错返回**，而不是永久挂住；
    /// 2. 那条错误**没有被任何一层当成「再试一次」** —— `read_head` 直接把它交出来。
    ///    ⭐ 这第 2 条是本轮最要紧的一格：一旦哪层重试，期限就从「阻塞有上限」
    ///    退化成「轮询」，而轮询正是零定时器护栏要防的东西。
    ///
    /// # 为什么这里用**测试自己选的** 200ms，而不是生产那 30 秒
    ///
    /// 端到端等满 `DOWNSTREAM_DEADLINE` 要跑 30 秒。⇒ 分工：
    /// **「那个数装上了没有」**由上一条（`both_peers_really_carry_…`）用 `getsockopt` 买；
    /// **「装上之后读会不会返回」**由这一条用一条短得多的同类期限买。
    /// 测试段不受零定时器护栏管（`production_code()` 剥掉 `#[cfg(test)]`），所以这里能自由选值。
    ///
    /// # 非空对照
    ///
    /// 同一趟里再跑一条**发全了请求头**的连接：它必须 `Ok(Some(..))` 且**明显快过**那条期限
    /// —— 否则「报错返回」只说明这把尺子把什么都判成超时。
    #[test]
    fn a_socket_deadline_makes_a_half_open_read_return_instead_of_wedging_the_thread() {
        let deadline = std::time::Duration::from_millis(200);
        let l = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind");
        let a = l.local_addr().expect("addr");

        // ── ㈠ 半开：只发半个请求头（**没有**结尾空行），且**不关**连接。
        let mut half = TcpStream::connect(a).expect("connect 半开");
        half.write_all(b"POST /s/agentA/sid-AAA/v1/messages HTTP/1.1\r\nHost: relay\r\n")
            .expect("write 半个头");
        half.flush().expect("flush");
        let (mut srv, _p) = l.accept().expect("accept 半开");
        srv.set_read_timeout(Some(deadline)).expect("装期限");
        // ★★ 风险 `5x`：这条读**一旦有人重试就会无限自旋**，而自旋的表现是**挂住不是红**
        //    —— 那一屏与「跑完了、没有新红」几乎分不开（判定行会整条消失）。
        //    ⇒ 把读搬到一条工作线程上，用 `recv_timeout` 给它一个**远宽于**期限的上界（20 倍），
        //      超了就 `panic!` ⇒ **把挂住换成红**。这条纪律本文件 `送 5x` 那几处已经在用。
        //    〔本轮 `MU5` 实测：把 `http1::read_head` 的读循环改成「`WouldBlock` 也 `continue`」
        //      —— 没有这一层的话它整条判据挂死，有了这一层它**红**。〕
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let t0 = std::time::Instant::now();
            let r = http1::read_head(&mut srv, HEAD_CAP);
            let _ = tx.send((r, t0.elapsed()));
        });
        let (r, waited) = rx
            .recv_timeout(deadline * 20)
            .expect("`read_head` 在 20 倍期限之内一个字都没返回 —— 期限没起作用，或者有哪一层在**重试**（那就成了轮询）");
        let e = r.expect_err("半开连接上的 `read_head` 必须**报错返回**；挂住的话本判据根本跑不完");
        assert!(
            matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ),
            "报的该是期限到了那一族（Linux 上是 WouldBlock/EAGAIN，Windows 上是 TimedOut），\
             实测拿到的是 {:?}：{e}",
            e.kind()
        );
        // 它是**等满了期限**才返回的，不是立刻被别的错误（如 RST）弹回来的。
        assert!(
            waited >= deadline,
            "只等了 {waited:?} 就返回了（期限 {deadline:?}）—— 那不是期限在起作用，是别的错误"
        );
        drop(half);

        // ── ㈡ 非空对照：同一把尺子、同样的期限，一条**发全了**的连接必须走通且明显快。
        let mut whole = TcpStream::connect(a).expect("connect 完整");
        whole
            .write_all(b"POST /s/agentA/sid-AAA/v1/messages HTTP/1.1\r\nHost: relay\r\n\r\n")
            .expect("write 完整头");
        whole.flush().expect("flush");
        let (mut srv2, _p2) = l.accept().expect("accept 完整");
        srv2.set_read_timeout(Some(deadline)).expect("装期限");
        let t1 = std::time::Instant::now();
        let got = http1::read_head(&mut srv2, HEAD_CAP)
            .expect("完整的请求头不该报错")
            .expect("完整的请求头该读得出来");
        let fast = t1.elapsed();
        assert!(
            got.ends_with(b"\r\n\r\n"),
            "读出来的该是一整个请求头：{:?}",
            String::from_utf8_lossy(&got)
        );
        assert!(
            fast < deadline,
            "非空对照：数据已经在那儿了，`read_head` 该**立刻**返回而不是等满 {deadline:?}（实测 {fast:?}）\
             —— 等满了说明这把尺子把正常流量也判成了超时"
        );
        drop(whole);
    }

    /// ★ 两个期限的**方向**：上游那条必须比下游那条**宽**。
    ///
    /// 这不是仪式，它钉的是一种真实且省事的写错法：**把两个数写成同一个**。
    /// - 上游被抄成下游那 30 秒 ⇒ 一条正在正常吐字、只是中间想了 40 秒的 SSE 长流会被
    ///   **从中间掐断**，客户端拿到半条回答 ⇒ **比不设期限更坏**（不设的话那条流走得完）。
    /// - 下游被抄成上游那 600 秒 ⇒ 半开连接要 10 分钟才散，`阻-3` 只治好一半。
    ///
    /// 判据形状：**只断方向，不断具体的值** —— 断具体的值就变成把常量抄第二遍，
    /// 改一次数就要改两处，而它一个缺陷都逮不到。
    #[test]
    fn the_upstream_deadline_is_the_wider_one_because_a_silently_thinking_model_is_normal() {
        assert!(
            upstream::UPSTREAM_DEADLINE > DOWNSTREAM_DEADLINE,
            "上游期限（{:?}）必须比下游（{:?}）宽：下游对端就在本机、慢是**异常**；\
             上游等的是模型在想、慢是**正常**。两个数写成一样就会掐断正常的长流。",
            upstream::UPSTREAM_DEADLINE,
            DOWNSTREAM_DEADLINE
        );
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
    ///
    /// ⚠ **改名**〔回修轮之四 08-25，承接 D2 §7㈢〕：旧名
    /// `the_relay_entry_resolves_its_defaults_and_lets_**env**_override_them`
    /// 里的「**env**」是假的 —— 它调的是**纯函数** `resolve_config` 的**参数**，
    /// **一个环境变量都没读过**。今天的名字只说它证得了的那一半：默认值 + **入参**盖得住。
    /// 「哪个环境变量喂给哪个配置位」由 `each_env_var_name_goes_into_its_own_config_slot` 守。
    #[test]
    fn the_config_resolver_has_defaults_and_lets_its_inputs_override_them() {
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
    ///
    /// ⚠⚠ **名字只说它证得了的那一半**〔铁律 15 自查，本轮我自己写的第一版就犯了同一种病〕：
    /// 我第一版把它叫 `the_relay_entry_reads_each_env_var_into_its_own_config_slot`
    /// —— 「**reads env**」是假的，取值器是**注入的**，本条一个真环境变量都没读过。
    /// 那正是 D2 `重要-3(D2)` 逮 `the_relay_entry_resolves_…_and_lets_env_override_them`
    /// 的**同一种病**，而它长在了治它的代码里。⇒ 改成今天这个名字：它证的是
    /// **变量名 → 配置位**这条接线，**不是**「真的去读了环境」。
    /// 「`run()` 真的读了那两个环境变量」今天**判不了**，登记住址件文件 §8.18.9。
    #[test]
    fn each_env_var_name_goes_into_its_own_config_slot() {
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
        assert_eq!(
            run_reading(echo, exec),
            7,
            "入口必须把执行体的退出码原样带回"
        );
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
