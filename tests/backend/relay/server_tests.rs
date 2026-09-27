use super::*;
// ⚠ 下面这几条 `use` 是**判据段自己的**，不放在 `server.rs` 文件顶上：放上面会在
//    非测试构建里变成 unused import。
//    ⚠⚠ 〔`设计/20 §7` 步 1〕`table` / `creds` 这两条先前是**跟着 `super::*` 蹭进来的**
//    —— 那时 `server.rs` 自己引着它们。上游选择搬走之后中转不再认识那两个模块，
//    判据要用就得自己写明白：**判据的人群从哪来，要看得见**。
use super::super::listen::{
    listen, resolve_port, run_reading, run_with, serve, RelayExec, DOWNSTREAM_DEADLINE,
    INFLIGHT_CONNECTIONS, UPSTREAM_DEADLINE,
};
use super::super::upstream;
use crate::accounts::upstream::creds;
use crate::accounts::upstream::{self as accounts, table::RoutingTable, Accounts};
use creds_core::SecretKey;
use std::io::BufRead;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc;

/// 判据里把一张表包成上游选择、再交给中转的那一步。
///
/// ⚠ 走的是**生产段那条真实的上游选择**（`Accounts`），不是判据自己造的一个假 `Destinations`
/// —— 造一个假的就等于「量的不是生产段那张决策表」。
fn dest_of(table: RoutingTable) -> Arc<dyn super::super::Destinations> {
    Arc::new(Accounts::new(table, upstreams_without_env()))
}

/// 没有任何环境变量时那一份「每 agent 一行的默认上游」—— 走生产段那条真实的解析。
fn upstreams_without_env() -> accounts::Upstreams {
    accounts::Upstreams::from_env(&|_| None).expect("内置默认必须解析得了")
}

/// 判据里那些行挂在谁名下（条 49：表的键是 agent ＋ 账号）。
///
/// ⚠ **它与 `send_request` 里那些 URL 的首段逐字对应**，名字刻意不是任何一家登记过的 agent ——
/// 这些判据量的是「表里有行」那几格，与哪家登没登记无关。
/// 子进程那几条读的是凭据文件，文件里的行挂在 claude-code 名下，所以它们的 URL 首段是那个名字。
const TEST_AGENT: &str = "agentA";
/// 第二家（`routes_two_keys…` 用它证「两个键走同一个进程」）。
const TEST_AGENT_B: &str = "agentB";

/// 拿**生产段那张决策表**（`accounts::upstream::decide`，与 `Accounts::resolve` 同一份实现）
/// 问一次，再把答案交给生产段那个渲染函数，返回它吐出来的那串字节。
///
/// ⚠⚠ 判据**不自己判**「这一行该不该换头 / 要不要丢掉客户端那份」——
/// 那是上游选择的活（`设计/20 §7` 步 1 之后它整条搬过去了）。判据自己再判一遍，
/// 量到的就是判据里那份副本，不是生产段那一份。
fn render_via_upstream_selection(
    t: &RoutingTable,
    account: &str,
    head: &RequestHead,
    rest: &str,
    body_len: usize,
) -> String {
    let key = super::super::RouteKey {
        // 条 49：`seg1` 今天是键的一半 ⇒ 必须是这张表里那些行挂的那一家。
        seg1: TEST_AGENT.to_string(),
        seg2: account.to_string(),
    };
    let mut out = None;
    let upstreams = upstreams_without_env();
    accounts::decide(
        t,
        &upstreams,
        super::super::Mode::Substitute,
        &key,
        &mut |d| {
            out = Some(match d {
                Destination::Refuse { status, .. } => {
                    panic!("{account} 那一行该在表里，上游选择却答了 Refuse {status}")
                }
                Destination::Passthrough { upstream } => {
                    render_upstream_request(head, rest, upstream, None, body_len)
                }
                // 〔`P16` 2026-09-22〕上游选择交下来的是一个 `AuthSwap`（头名 ＋ 完整头值 ＋
                // 要丢的头名全集），中转照写 ⇒ 这里原样把它递进渲染，**不许在判据里自己凑一份**。
                Destination::Substitute { upstream, auth } => {
                    render_upstream_request(head, rest, upstream, Some(&auth), body_len)
                }
            });
        },
    );
    String::from_utf8(out.expect("上游选择一次都没答")).expect("utf8")
}

/// 假上游发几个事件块。终止块另算 ⇒ 一条响应的**块数** = `UPSTREAM_EVENTS + 1`。
const UPSTREAM_EVENTS: usize = 3;

/// 判据里那个「配得到」的账号段。**与 `send_request` 里那些 URL 逐字对应。**
const ACCT_A: &str = "acctA";
/// 第二个账号段（`routes_two_keys…` 用它证「两个键走同一个进程」）。
const ACCT_B: &str = "acctB";

/// 造一张判据用的表。
///
/// ⚠ 它走的是**生产段那条真实的路** `RoutingTable::build` —— 判据不许自己另造一个
/// 同构的表，那样量的就不是生产段的那一份了。
fn table_of(rows: &[(&str, &Base, Option<&str>)]) -> RoutingTable {
    // ⚠ `K-R1`：这个薄封装给每一行填 `AuthStyle::DEFAULT`。
    //   它**不是**「本件不关心鉴权头形状」的意思 —— 它是「既有的这些判据量的都是
    //   **默认那条路**」，而默认那条路本件要求它逐字节不变。
    //   要量非默认形状的判据走 [`table_of_styled`]。
    table_of_styled(
        &rows
            .iter()
            .map(|(id, b, k)| (*id, *b, *k, creds_core::store::AuthStyle::DEFAULT))
            .collect::<Vec<_>>(),
    )
}

/// 带鉴权头形状的那一版〔`K-R1`〕。走的仍是生产段那条真实的 `RoutingTable::build`。
fn table_of_styled(
    rows: &[(&str, &Base, Option<&str>, creds_core::store::AuthStyle)],
) -> RoutingTable {
    table_under(
        &rows
            .iter()
            .map(|(id, b, k, s)| (TEST_AGENT, *id, *b, *k, *s))
            .collect::<Vec<_>>(),
    )
}

/// 最底下那一版：每一行**自己说**挂在哪一家名下（条 49）。走的仍是生产段那条真实的 `RoutingTable::build`。
fn table_under(
    rows: &[(
        &str,
        &str,
        &Base,
        Option<&str>,
        creds_core::store::AuthStyle,
    )],
) -> RoutingTable {
    RoutingTable::build(rows.iter().map(|(agent, id, b, k, s)| {
        (
            (*agent).to_string(),
            (*id).to_string(),
            (*b).clone(),
            k.map(SecretKey::new),
            *s,
        )
    }))
}

/// 判据里最常用的那张表：两个账号段，都指向同一个假上游，都不配 key
/// （⇒ 走「原样转发」那一支，与 `K-H1` 甲半的既有判据逐字同形）。
fn two_accounts_no_key(base: &Base) -> RoutingTable {
    // ⚠ 条 49 之后「两个键」是两对 `(agent, 账号)`：`routes_two_keys…` 打的是
    //   `/s/agentA/acctA` 与 `/s/agentB/acctB`，所以两行各挂一家。
    let d = creds_core::store::AuthStyle::DEFAULT;
    table_under(&[
        (TEST_AGENT, ACCT_A, base, None, d),
        (TEST_AGENT_B, ACCT_B, base, None, d),
    ])
}

/// 一个只属于本判据的临时目录。**名字中性**（不含被断言的字面），
/// 免得诊断把路径原样印进输出、让「输出里含某句话」靠路径恒真
/// （`brief` 12 逐字点名的那一形；隔壁 `creds.rs` 的同名助手记着那次真事故）。
fn tmpdir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "ccm-rl-{}-{}-{tag}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&d).expect("建临时目录");
    d
}

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
    /// 上游**真的收到**的 `Authorization:` 头**整行**，逐次一条〔`K-H2a` `KS3`〕。
    ///
    /// ⚠ 它与 `seen` 里那个 `auth=<bool>` **不是同一个量**：那个布尔在
    /// 「原样转发」与「换头」两种形状下**一模一样**，证不了换头真的发生了。
    auth_values: Arc<std::sync::Mutex<Vec<String>>>,
    /// 上游**真的发出**的 SSE **事件**数（终止块不算）〔回修轮之五 08-25，`阻-4(D3)`〕。
    ///
    /// ⚠ 它与 `sent` **不是同一个量**，别拿一个当另一个用：
    /// `sent` 数的是**网线上的块**（含终止块），`events` 数的是**上游发出的 `data:` 事件**。
    /// `DoD-2㈡` 的「块数对账」用前者，`DoD-3㈠` 的「`event` 数对账」用后者。
    events: Arc<AtomicU64>,
    /// **下游在响应写完之前就走了**、本桩因此丢掉的连接数〔`K-R126`〕。
    ///
    /// 桩那条 accept 线程的把手〔`K-R126`〕—— 判据拿它判「桩整个下线了没有」。
    ///
    /// ⚠ **为什么不探端口**：`listener` drop 之后那个临时端口回到内核的池子里，
    /// 同一个进程里别的判据 `bind(0)` 有机会抢到它 ⇒ `connect` 又通了
    /// ⇒ 「桩还在」这句话就成了假读数。**线程死没死**与端口池无关。
    /// ⚠ **为什么是 `is_finished()` 轮询而不是 `join()`**：真出了「错被吞掉」那一形时，
    /// 线程还在 accept 循环里，`join()` 会**永远挂住** —— CI 上挂死比红一条更坏。
    accept_thread: Option<std::thread::JoinHandle<()>>,
    /// ⚠ 它是一个**读数，不是一处被吞掉的错**：判据读得到它，
    /// [`a_peer_that_left_costs_the_stub_one_connection_not_its_listener`]
    /// 就是拿它先证「这一趟真的走到了那一支」，再去证 `listener` 还活着 ——
    /// 少了这个数，那一条判据的后半截是**空真**（`testing.md` 四⑷）。
    aborted: Arc<AtomicU64>,
}

impl FakeUpstream {
    fn sent(&self) -> u64 {
        self.sent.load(Ordering::SeqCst)
    }

    fn events(&self) -> u64 {
        self.events.load(Ordering::SeqCst)
    }

    /// 见字段头注〔`K-R126`〕。
    fn aborted(&self) -> u64 {
        self.aborted.load(Ordering::SeqCst)
    }
}

/// 对端（中转）在这一条连接上**先走了**。
///
/// ★★ `K-R126`：这**是合法的下游行为，不是夹具坏了**。`STUB_LAUNCHER` 只读一行状态行
/// （`head -n 1`）就退出，`bash` 一退出这条连接就没了 —— 每一发都在造这一形。
/// 中转那一侧照规矩把它记成 `connection ended: Broken pipe` 并收掉上游那条连接，
/// **假上游这一侧也必须认得它**，否则就会拿一条正常的客户端行为去炸自己的线程。
///
/// ⚠ 这个闭集**只装「对端走了」这三种**。别往里加第四种去「让红消失」——
/// 其余的错今天仍然**大声炸**（见 `spawn_fake_upstream` 的 accept 循环），
/// 响度一个分贝都没降：降了就成了把量具关掉。
///
/// ⚠ 成员表的**唯一住址**是 [`PEER_LEFT_KINDS`]，这里只读它，不再写第二遍
/// （`brief` 13b：闭集只许有一个住址）。
fn peer_is_gone(e: &std::io::Error) -> bool {
    PEER_LEFT_KINDS.contains(&e.kind())
}

/// 「对端走了」这个闭集的**唯一住址**〔`K-R126`〕。
///
/// 下游（`STUB_LAUNCHER` 的 `head -n 1`）在响应写完之前退出时，
/// 假上游这一侧真正会撞上的就是这几种。判据
/// `a_peer_that_left_costs_the_stub_one_connection_not_its_listener`
/// 拿它当**地板**（成员数对不上就红），别往里塞第四种去让红消失。
const PEER_LEFT_KINDS: [std::io::ErrorKind; 3] = [
    std::io::ErrorKind::BrokenPipe,
    std::io::ErrorKind::ConnectionReset,
    std::io::ErrorKind::ConnectionAborted,
];

/// 让假上游的**第 `nth` 条连接**（从 1 数）不管真实 I/O 如何，都按 `kind` 这个错收场
/// 〔`K-R126`〕。
///
/// # 为什么要注入 —— 这一条是本件最贵的一个读数，别删
///
/// 本件头一版判据是**有机**的：让下游只读一行状态行就走，等桩自己撞上 `Broken pipe`。
/// 它在 `--test-threads=1` 下跑 200 趟 0 红，**而整族并发 16 线程跑 10 趟红了 5 趟**
/// —— 因为「对端已经走了」这件事要等 `RST` 被内核投递到桩那一侧才看得见，
/// 机器一忙，桩的 4 次写就**全部先写完**（`aborted` 停在 0），判据自己的采集面自检翻红。
/// ⇒ **那一版判据本身就是一个新的 flake 源**，与本件要治的病同形。
///
/// 注入把「错**什么时候**来」这一维整个拿掉：判据要钉的本来就不是内核的时序，
/// 而是**收场那一档的策略**（对端走了 ⇒ 只丢这条连接；别的错 ⇒ 照旧大声炸）。
/// 有机那一半没有丢，它住在 `tests/evidence/K-R126-deathvalue.md` 的复现台面上：
/// 修前收场 ＋ 复现刀 ⇒ CI 上红的那两条**逐趟必红 5/5**。
#[derive(Clone, Copy)]
struct StubFault {
    nth: u64,
    kind: std::io::ErrorKind,
}

/// 发一块 —— **一次** `write_all` + `flush`。
///
/// 长度行 / 数据 / CRLF 分三次写会让「一块」在网线上散成三段，
/// 那时「下游读到的块数」就不再是上游发出的块数 ⇒ 对账那条判据要的是 1:1。
///
/// ★ `K-R126` 把 `.expect()` 换成 `?`：**写不出去不是 panic 的理由** ——
/// 收场归调用方那一处统一判（对端走了 ⇒ 丢这一条连接；其余 ⇒ 照旧炸）。
/// `sent` 只在**真写出去了**之后才 +1，这一条一个字没动。
fn send_chunk(s: &mut TcpStream, sent: &AtomicU64, payload: &[u8]) -> std::io::Result<()> {
    let mut frame = format!("{:x}\r\n", payload.len()).into_bytes();
    frame.extend_from_slice(payload);
    frame.extend_from_slice(b"\r\n");
    s.write_all(&frame)?;
    s.flush()?;
    sent.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

fn spawn_fake_upstream(gate: Option<mpsc::Receiver<()>>) -> FakeUpstream {
    spawn_fake_upstream_faulted(gate, None)
}

/// 带注入口的那一版〔`K-R126`〕。`fault` 给 `None` 时与 [`spawn_fake_upstream`]
/// **逐字节同路**（同一个函数体），既有的那几十条判据一个字都不用改。
fn spawn_fake_upstream_faulted(
    gate: Option<mpsc::Receiver<()>>,
    fault: Option<StubFault>,
) -> FakeUpstream {
    let listener = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind fake upstream");
    let addr = listener.local_addr().expect("addr");
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let bodies = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sent = Arc::new(AtomicU64::new(0));
    let events = Arc::new(AtomicU64::new(0));
    let aborted = Arc::new(AtomicU64::new(0));
    let aborted_c = Arc::clone(&aborted);
    let auth_values = Arc::new(std::sync::Mutex::new(Vec::new()));
    let auth_values_c = Arc::clone(&auth_values);
    let seen_c = Arc::clone(&seen);
    let bodies_c = Arc::clone(&bodies);
    let sent_c = Arc::clone(&sent);
    let events_c = Arc::clone(&events);
    let accept_thread = std::thread::spawn(move || {
        let gate = gate;
        let mut conn_no: u64 = 0;
        for s in listener.incoming() {
            let Ok(mut s) = s else { continue };
            conn_no += 1;
            // ★★★ `K-R126` —— **一条连接怎么收场，分两档**。读数住件文件 `§3`。
            //
            // 在此之前这里是一串 `.expect(...)`：下游只要在响应写完之前走掉
            //（`STUB_LAUNCHER` 的 `head -n 1` **每一发都这样**），
            // 桩这条 accept 线程就 panic ⇒ 连带把 `listener` 一起 drop 掉
            // ⇒ **同一条判据里后面每一发都是 `Connection refused`**。
            // 那正是 CI 上连红两趟的根因（run `34939805688` 两趟 attempt 逐字同形：
            // 桩线程炸在终止块那一写、测试线程红在**第二个账号**上的 502）。
            //
            // ⚠ **`Broken pipe` 是根、`Connection refused` 是果**，别读反了：
            //   桩不是「还没 listen 上」—— `bind` 在 `spawn` **之前**、
            //   `addr` 就是从 `listener` 上取的 ⇒ 地址存在的那一刻它已经在听了。
            //   它是**听着听着被自己炸没的**。
            //
            // ⚠ **这不是「把错吞掉」**：
            //   · 对端走了 ⇒ 记进 `aborted`（判据读得到的一个数），只丢这一条连接；
            //   · 其余任何错 ⇒ **照旧 panic**，响度一分贝没降。
            let outcome = (|| -> std::io::Result<()> {
                let mut r = BufReader::new(s.try_clone().expect("clone"));
                let mut line = String::new();
                r.read_line(&mut line)?;
                let mut auth = false;
                let mut clen = 0usize;
                loop {
                    let mut h = String::new();
                    let n = r.read_line(&mut h)?;
                    if n == 0 || h == "\r\n" {
                        break;
                    }
                    let lower = h.to_ascii_lowercase();
                    if lower.starts_with("authorization:") {
                        auth = true;
                        // `K-H2a` `KS3`：把**值**也收下来。
                        // 只有 `auth=true` 这个布尔证不了「换头真的发生了」——
                        // 原样转发和换头**在这个布尔上一模一样**。
                        auth_values_c
                            .lock()
                            .expect("lock")
                            .push(h[..].trim().to_string());
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
?;
                s.flush()?;
                for i in 1..=UPSTREAM_EVENTS {
                    if let Some(g) = gate.as_ref() {
                        // 等下游确认收到**上一块**。这是逐块门闩。
                        let _ = g.recv();
                    }
                    send_chunk(
                        &mut s,
                        &sent_c,
                        format!("data: {{\"i\":{i}}}\n\n").as_bytes(),
                    )?;
                    // ★ 事件数**由夹具自己数**（`阻-4(D3)`）：判据拿它当分母，
                    //   而不是拿 `UPSTREAM_EVENTS` 这个常量 —— 夹具少发了也要看得见。
                    events_c.fetch_add(1, Ordering::SeqCst);
                }
                if let Some(g) = gate.as_ref() {
                    let _ = g.recv();
                }
                s.write_all(b"0\r\n\r\n")?;
                s.flush()?;
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
                Ok(())
            })();
            // ★ 注入口〔`K-R126`〕：**只有**判据显式要了 `fault` 才走这里，
            //   `None` 那条路（其余每一条判据）一个分支都不改。
            let outcome = match fault {
                Some(f) if f.nth == conn_no => Err(std::io::Error::new(
                    f.kind,
                    "K-R126 判据注入的错 —— 这一行出现在**绿**的一趟里也是对的",
                )),
                _ => outcome,
            };
            match outcome {
                Ok(()) => {}
                // 下游先走 —— 合法的客户端行为，只丢这一条连接，`listener` 照常接下一发。
                Err(e) if peer_is_gone(&e) => {
                    aborted_c.fetch_add(1, Ordering::SeqCst);
                }
                // 不是「对端走了」那一档 ⇒ 夹具自己坏了，仍然大声炸。
                Err(e) => panic!("假上游这一条连接坏在**不是对端走了**的地方：{e:?}"),
            }
        }
    });
    FakeUpstream {
        addr,
        seen,
        auth_values,
        bodies,
        sent,
        events,
        aborted,
        accept_thread: Some(accept_thread),
    }
}

/// 往假上游发一整发、并把响应**整条**读回来。只给 `K-R126` 那两条判据用。
fn one_whole_shot(addr: SocketAddr, what: &str) -> String {
    let mut c = TcpStream::connect(addr)
        .unwrap_or_else(|e| panic!("{what}：连不上假上游 —— 它的 `listener` 已经不在了（{e:?}）"));
    let req = format!(
        "POST /v1/messages HTTP/1.1\r\nContent-Length: {}\r\n\r\n{REQUEST_BODY}",
        REQUEST_BODY.len()
    );
    c.write_all(req.as_bytes()).expect("发请求");
    c.flush().expect("flush");
    c.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .expect("读期限");
    let mut buf = Vec::new();
    c.read_to_end(&mut buf).expect("读响应");
    String::from_utf8_lossy(&buf).into_owned()
}

/// ★★★ `K-R126` 的牙之一 —— **一条连接上「对端走了」，只该丢掉那一条连接，
/// 不该把假上游的 `listener` 一起带走。**
///
/// # 它钉的是 CI 上连红两趟的那个根因
///
/// `main` 上同一份代码，run `34939805688` 连红两趟、每趟红的不是同一条，
/// 而**两趟的桩线程都炸在同一处**（终止块那一写，`end: BrokenPipe`）：
///   ① 下游（`STUB_LAUNCHER` 的 `head -n 1`）读到状态行就退出 ⇒ 中转写下游 `Broken pipe`；
///   ② 中转因此收掉上游那条连接 ⇒ 桩接着写 ⇒ `Broken pipe` ⇒ 桩线程 panic；
///   ③ panic 把 `listener` 一起 drop 掉 ⇒ **同一条判据里第二发 `Connection refused`** ⇒ 502。
/// ⇒ **`Broken pipe` 是根、`Connection refused` 是果**，不是「桩还没 listen 上」——
///   `bind` 发生在 `spawn` 之前，地址存在的那一刻它已经在听了。
///
/// # 为什么用注入，而不是让下游真的中途走人
///
/// 有机那一版**自己就是一个 flake 源**，实测：`--test-threads=1` 跑 200 趟 0 红，
/// 而整族 16 线程跑 10 趟**红 5 趟** —— 「对端已经走了」要等 `RST` 投递到桩这一侧
/// 才看得见，机器一忙桩的四次写全都先写完了。见 [`StubFault`] 头注与
/// `tests/evidence/K-R126-deathvalue.md`。有机那一半没有丢：它是那份文档里的**复现台面**
/// （修前收场 ＋ 复现刀 ⇒ CI 上红的那两条 5/5 必红）。
#[test]
fn a_peer_that_left_costs_the_stub_one_connection_not_its_listener() {
    // 判据自己那份输入表。**刻意是第二处写下这三个名字**，理由就在下面那条地板断言：
    // 判据若也去读 `PEER_LEFT_KINDS`，闭集少一个成员它就跟着少跑一轮（空真）,
    // 那样「成员被人摘掉了」这一形永远钉不住。
    let cases = [
        std::io::ErrorKind::BrokenPipe,
        std::io::ErrorKind::ConnectionReset,
        std::io::ErrorKind::ConnectionAborted,
    ];
    // ★ 地板（`testing.md` 三⑺）：两边**一样多**。闭集加了第四种而这里没跟上 ⇒ 红；
    //   闭集被摘掉一种 ⇒ 也红。**现算两边的 `len()`，不写死那个基数**（`brief` 13b）。
    assert_eq!(
        cases.len(),
        PEER_LEFT_KINDS.len(),
        "「对端走了」那个闭集变了，而这条判据的输入表没跟上 —— 它会静默地少量一格"
    );

    for kind in cases {
        let up = spawn_fake_upstream_faulted(None, Some(StubFault { nth: 1, kind }));

        // 头一发：网线上一切正常，**这一条连接的收场被注入成「对端走了」**。
        let first = one_whole_shot(up.addr, "头一发");
        assert!(
            first.starts_with("HTTP/1.1 200"),
            "头一发没拿到响应（{kind:?}）：{first:?}"
        );

        // 采集面自检（`testing.md` 四⑷）：桩**真的**走到了「对端走了」那一支。
        // 少了这一步，下面「listener 还活着」是空真 —— 它根本没撞上那一形。
        // ⚠ 这是一个**单向**条件（记上了就不会再变回去）⇒ 机器越慢它只是多等几轮，
        //   判不出两种结果。本件治的正是「靠来得及」的判据，别在这里把它请回来。
        assert!(
            wait_until(|| up.aborted() == 1),
            "桩没把这一条连接记进 `aborted`（{kind:?}）\
                 ⇒ 下面那几条是空真，这一趟量的不是该量的东西"
        );

        // ★ 正题：桩**还在 listen**，第二发要走完一整条响应。
        //   修之前这里就是 CI 上那一发：`Connection refused` ⇒ 502。
        let second = one_whole_shot(up.addr, "第二发");
        assert!(
            second.starts_with("HTTP/1.1 200"),
            "第二发没拿到一条完整响应（{kind:?}）：{second:?}"
        );
        assert!(
            second.ends_with("0\r\n\r\n"),
            "第二发缺终止块 ⇒ 桩没把这一条走完（{kind:?}）：{second:?}"
        );
        // 两发**都**到了桩这里 —— 「listener 还活着」不是靠第二发被静默吃掉换来的。
        let seen = up.seen.lock().expect("lock").clone();
        assert_eq!(seen.len(), 2, "桩收到的是（{kind:?}）：{seen:?}");
        // 第二发是**一条好连接**：它不许也被记成 abort（否则修法把好连接一起丢了）。
        assert_eq!(
            up.aborted(),
            1,
            "第二发那条好连接也被记成 abort 了（{kind:?}）"
        );
    }
}

/// ★★★ `K-R126` 的牙之二 —— **不是「对端走了」的那些错，响度一分贝都不许降。**
///
/// 上面那条判据买的是「别把一条正常的客户端行为读成夹具坏了」；
/// 这一条买的是它的**反面**：修法不许顺手把**别的**错也一起吞掉。
/// 少了这一条，「`peer_is_gone` 只装三种」那句话没有任何东西在守 ——
/// 把它改成恒 `true`（夹具从此再也不会大声炸）能一路全绿。
#[test]
fn a_stub_failure_that_is_not_the_peer_leaving_still_brings_the_stub_down_loudly() {
    // 取一种**不在**闭集里的错。「它不在」这句话**现算**，不写死。
    let kind = std::io::ErrorKind::InvalidData;
    assert!(
        !PEER_LEFT_KINDS.contains(&kind),
        "这一条判据挑的错跑进闭集里了 ⇒ 它量的不再是「不该吞的那一档」"
    );

    let mut up = spawn_fake_upstream_faulted(None, Some(StubFault { nth: 1, kind }));
    let first = one_whole_shot(up.addr, "头一发");
    assert!(
        first.starts_with("HTTP/1.1 200"),
        "头一发没拿到响应：{first:?}"
    );

    // 桩这条 accept 线程该**炸**。「炸了就不会活回来」是**单向**的
    // ⇒ 这是收敛的等待，不是掷骰子。判的是**线程**不是端口，理由见 `accept_thread` 头注。
    let th = up
        .accept_thread
        .take()
        .expect("桩的线程把手不在了 —— 这一条判据够不着它要判的东西");
    assert!(
        wait_until(|| th.is_finished()),
        "桩吞掉了一个**不该吞**的错：那条 accept 线程还活着 ⇒ 响度降了，量具被关掉一格"
    );
    assert!(
        th.join().is_err(),
        "那条线程是**正常结束**的，不是炸掉的 —— 不该吞的错被吞了"
    );
    assert_eq!(
        up.aborted(),
        0,
        "不在闭集里的错被记成了「对端走了」—— 那正是把量具关掉"
    );
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
/// 一个**保证不存在**的 claude 家目录。
///
/// ⚠ 判据里凡是会走到「读凭据」那一步的，都得喂它 —— 否则会去读**跑判据这台机器上
/// 用户真实的那份凭据文件**。那既是越界，又会让读数随机器而变。
/// 名字取中性（不含任何被断言的字面），免得路径原样印进输出、让断言靠路径恒真。
fn nowhere_home() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "ccm-rc-nohome-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ))
}

fn spawn_relay(up: SocketAddr) -> (SocketAddr, Arc<Relay>, TeeTap) {
    spawn_relay_with_sink(up, None)
}

/// 同上，但可以塞一个**自己造的** tee 落点（`阻-2` 那条判据要一个**会阻塞**的落点）。
fn spawn_relay_with_sink(
    up: SocketAddr,
    custom: Option<Box<dyn Write + Send>>,
) -> (SocketAddr, Arc<Relay>, TeeTap) {
    let (addr, relay, tee, _inflight) = spawn_relay_counted(up, custom);
    (addr, relay, tee)
}

/// 同上，另交回监听面那份在途计数（〔NET2〕它住宿主 `listen.rs`，不在 `Relay` 身上）。
fn spawn_relay_counted(
    up: SocketAddr,
    custom: Option<Box<dyn Write + Send>>,
) -> (SocketAddr, Arc<Relay>, TeeTap, Arc<AtomicUsize>) {
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
    let relay = Arc::new(Relay::new(
        dest_of(two_accounts_no_key(&base)),
        door::Key::for_tests(),
        TeeSink::new(w),
        DOWNSTREAM_DEADLINE,
        UPSTREAM_DEADLINE,
    ));
    let listener = listen(0).expect("listen");
    let addr = listener.local_addr().expect("addr");
    let r2 = Arc::clone(&relay);
    let inflight = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&inflight);
    std::thread::spawn(move || serve(listener, r2, counted));
    (addr, relay, TeeTap { buf, rx }, inflight)
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
        // 〔RK1〕过门：路径前面挂上夹具那把钥匙、`Host` 用回环字面量（门那三问见 `door.rs`）。
        "POST /{}{target} HTTP/1.1\r\nHost: 127.0.0.1\r\n{extra}Content-Length: {}\r\n\r\n{body}",
        door::door_tests::TEST_KEY,
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
    // 〔V141〕流标签取自请求自带的会话标识头（路径里没有会话段）。
    let mut c = send_request(
        relay_addr,
        "/s/agentA/acctA/v1/messages?beta=true",
        "x-claude-code-session-id: sid-AAA\r\n",
    );
    let mut got = Vec::new();
    c.read_to_end(&mut got).expect("read a");
    let mut c2 = send_request(
        relay_addr,
        "/s/agentB/acctB/v1/messages?beta=true",
        "x-claude-code-session-id: sid-BBB\r\n",
    );
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
    let mut warm = send_request(relay_addr, "/s/agentA/acctA/v1/messages", "");
    let mut sink0 = Vec::new();
    warm.read_to_end(&mut sink0).expect("read warmup");
    assert!(
        String::from_utf8_lossy(&sink0).starts_with("HTTP/1.1 200"),
        "非空对照：一发正常请求必须拿到 200"
    );
    assert_eq!(
        up.seen.lock().expect("lock").len(),
        1,
        "非空对照：真打到上游"
    );

    // 正题：一条 `Content-Length: 1e12`，其余**一个字节都不发**。
    let (got, clean) = send_raw(
        relay_addr,
        "POST /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 1000000000000\r\n\r\n",
    );
    assert!(
        got.starts_with("HTTP/1.1 413"),
        "超 `BODY_CAP` 必须回 413（拿到的是：{got:?}）"
    );
    assert!(
        clean,
        "413 要**送得到**：连接得干净收尾，不是被 RST 打断（拿到的是：{got:?}）"
    );
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

    let mut warm = send_request(relay_addr, "/s/agentA/acctA/v1/messages", "");
    let mut sink0 = Vec::new();
    warm.read_to_end(&mut sink0).expect("read warmup");
    assert_eq!(
        up.seen.lock().expect("lock").len(),
        1,
        "非空对照：真打到上游"
    );
    assert_eq!(
        up.bodies.lock().expect("lock")[0],
        REQUEST_BODY.as_bytes(),
        "非空对照：上游那一侧真的看得见请求体（否则下面那条是空真）"
    );

    let (got, clean) = send_raw(
        relay_addr,
        "POST /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 7abc\r\n\r\n{\"m\":1}",
    );
    assert!(
        got.starts_with("HTTP/1.1 400"),
        "读不懂的 Content-Length 必须回 400（拿到的是：{got:?}）"
    );
    assert!(
        clean,
        "400 要**送得到**：连接得干净收尾，不是被 RST 打断（拿到的是：{got:?}）"
    );
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
        let mut c = send_request(relay_addr, "/s/agentA/acctA/v1/messages", "");
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
/// ⇒ 这一条喂 **9 条**（上限的手写字面量 8 + 1），断它回 **504**
/// （`设计/20 §3.1a`：中转自己的传输失败；先前这一格回 502，与上游选择 `Refuse` 撞码）。
///
/// ⚠ 期望值 `9` 是**手写字面量**，不是拿 `INTERIM_RESPONSES_ALLOWED` 算的
/// —— 拿被测常量算期望值，改了常量本条会跟着漂、永远绿。
#[test]
fn too_many_interim_responses_are_refused_with_504() {
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
    let mut c = send_request(relay_addr, "/s/agentA/acctA/v1/messages", "");
    let mut got = String::new();
    c.read_to_string(&mut got).expect("read");
    assert!(
        got.starts_with("HTTP/1.1 504 Gateway Timeout\r\n"),
        "1xx 多到超过上限就该回 504（拿到的是：{got:?}）"
    );
    assert!(
        got.contains("一直不给最终响应。卡在读响应这一步。"),
        "504 那一发没说清卡在哪（拿到的是：{got:?}）"
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
    let mut c = send_request(relay_addr, "/s/agentA/acctA/v1/messages", "");
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
    let (relay_addr, _relay, _tee) = spawn_relay_with_sink(
        up.addr,
        Some(Box::new(WedgedSink(std::sync::Arc::clone(&writes)))),
    );

    let mut ms = Vec::new();
    for key in ["sid-AAA", "sid-BBB"] {
        let t0 = std::time::Instant::now();
        let mut c = send_request(
            relay_addr,
            "/s/agentA/acctA/v1/messages",
            &format!("x-claude-code-session-id: {key}\r\n"),
        );
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
    //
    // ⚠⚠ **它必须 `wait_until`，不许瞬时读**〔`K-R24` 读出来的机制，09-04〕：
    // tee 是**队列 + 写线程**，而上面两发是从**这条测试线程**上量完就往下走的
    // ⇒ 瞬时读会落在「转发已经走完、写线程还没写第一笔」那个窗口里，
    // 读到 0 ⇒ 间歇性红，报文逐字是「卡住的那个落点一次都没被写过」。
    // 同模块上面几条对同类前提（`inflight` 那几格）用的就是 `wait_until`，这里照抄。
    // ⚠ 它**不改本条量的东西**：`ms[0]`/`ms[1]` 在这一句**之前**就量完了，
    //   而 `wait_until` 的 4s 上限与那两条 1500ms 阈值量的是**两件事**。
    assert!(
        wait_until(|| writes.load(Ordering::SeqCst) >= 1),
        "非空对照：卡住的那个落点一次都没被写过 ⇒ 本条量的不是 tee 这条路"
    );
    println!(
        "[阻-2] tee 卡 3000ms 时：A 耗时 {} ms · B 耗时 {} ms",
        ms[0], ms[1]
    );
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
    let (relay_addr, _relay, _tee, inflight) = spawn_relay_counted(up.addr, None);

    // ㈠ 半开一条：只发半个请求头，**永不**发结尾空行、不关连接。
    let mut half = TcpStream::connect(relay_addr).expect("connect");
    half.write_all(b"POST /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n")
        .expect("half head");
    half.flush().expect("flush");
    assert!(
        wait_until(|| inflight.load(Ordering::SeqCst) >= 1),
        "半开连接必须算进在途数（今天是 {}）",
        inflight.load(Ordering::SeqCst)
    );

    // ㈢ 非空对照先做：`上限 - 1` 时同一发请求必须拿到 200。
    inflight.store(INFLIGHT_CONNECTIONS - 1, Ordering::SeqCst);
    let (got, _clean) = send_raw(
        relay_addr,
        "POST /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 7\r\n\r\n{\"m\":1}",
    );
    assert!(
        got.starts_with("HTTP/1.1 200"),
        "非空对照：还没到上限时这一发必须走得通：{got:?}"
    );
    // ⚠ 先等上一发那条连接线程**真的收工**（它收工时会 `fetch_sub` 一次）。
    //    不等就 `store` 的话，那一次减会落在我们设的值**之后** ⇒ 计数被减回 255，
    //    下面那一发就不会被拒 —— 本条第一版正是这么红的（拿到的是一条正常的 200）。
    assert!(
        wait_until(|| inflight.load(Ordering::SeqCst) == INFLIGHT_CONNECTIONS - 1),
        "上一发的连接线程没收工（在途数停在 {}）",
        inflight.load(Ordering::SeqCst)
    );

    // ㈡ 顶到上限：新连接必须拿到 **503**，不是一个没有任何响应的 FIN。
    inflight.store(INFLIGHT_CONNECTIONS, Ordering::SeqCst);
    let (got, clean) = send_raw(
        relay_addr,
        "POST /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 7\r\n\r\n{\"m\":1}",
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
    inflight.store(0, Ordering::SeqCst);
    let mut c = send_request(relay_addr, "/s/agentA/acctA/v1/messages", "");
    let mut sink = Vec::new();
    c.read_to_end(&mut sink).expect("read");
    assert!(
        wait_until(|| inflight.load(Ordering::SeqCst) == 0),
        "连接走完之后在途数必须回落（今天是 {}）",
        inflight.load(Ordering::SeqCst)
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
    // `run()` 自己去读 `CCM_RELAY_PORT` / `CCM_AGENT_UPSTREAM_CLAUDE_CODE`。成功那条路永不返回。
    // ⚠ home 走**生产段那条**解析（`resolve_home` 认 `CLAUDE_CONFIG_DIR`）——
    //   而凭据那份文件的位置由 `CCM_APIKEY_CREDENTIALS` 覆盖，父进程一定会设它
    //   （见 `spawn_relay_child_with_creds`）。**绝不能让判据去读用户真实的那份凭据。**
    // ⚠ 〔上游选择搬出 `relay/` 那一拍〕走的是 `main.rs` 的 `--relay` 那一臂**真调的那一个**
    //   （`accounts::upstream::run_relay` = 中转的 `run` ＋ 上游选择那只手），不是中转的 `run` 本身 ——
    //   后者今天要调用方递一个 `Startup` 进来，判据自己递就不是生产段那条接线了。
    std::process::exit(crate::accounts::upstream::run_relay(
        &crate::agents::claudecode::paths::resolve_home(),
        &[],
    ));
}

/// 一个跑在**真子进程**里的中转，连同它 stdout / stderr 的全量收集面。
struct RelayChild {
    child: std::process::Child,
    addr: SocketAddr,
    /// 〔RK1〕子进程的**夹具家目录**（钥匙文件在它底下，预先放好 `door::door_tests::TEST_KEY`）。
    home: std::path::PathBuf,
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
/// ⚠⚠ **`K-H2` 改了这个夹具，经过必须写下来**：先前它指到一条**不存在**的路径，
/// 靠的是「没配凭据 ⇒ 中转是一条全局透传路」——**那条隐式行为已经不存在了**
/// （见 `render_upstream_request` 头注的订正）。今天不给它一份凭据文件的话，
/// 表是空的 ⇒ **每一发都是 404**，而这几条判据要的是一趟**真转发**。
///
/// ⇒ 今天写一份**只有一条空账号**的真文件：`{"accounts":{"acctA":{}}}`
/// —— keyless、用默认上游（`CCM_AGENT_UPSTREAM_CLAUDE_CODE` 指着假上游），
/// 正好等价于先前那条隐式透传，但**是显式的一条路**。
fn spawn_relay_child(up: SocketAddr) -> RelayChild {
    // 判据绝不许去碰用户真实的那份凭据文件 ⇒ 自己造一份临时的。
    let dir = tmpdir(&format!("child-{}", up.port()));
    let p = dir.join("apikey-credentials.json");
    std::fs::write(&p, b"{\n  \"accounts\": {\n    \"acctA\": {}\n  }\n}\n")
        .expect("写子进程的凭据夹具");
    spawn_relay_child_with_creds(up, &p)
}

/// 起一个子进程中转，并**指定它从哪儿读凭据**。
///
/// ★ `KS3` 的金丝雀走的就是这条路：真子进程 · 真文件 · 真转发 —— 不是在一个 crate 里自问自答。
fn spawn_relay_child_with_creds(up: SocketAddr, creds_path: &std::path::Path) -> RelayChild {
    // 〔RK1〕中转绑上口之后会去 `$HOME/.cc-monitor/relay-key` 拿钥匙（没有就铸一把落盘）——
    //   **不给夹具家目录，子进程就会去用户真实的家目录里写**。⇒ 一律给，并预先放好夹具那一把。
    let home = door::door_tests::seed_test_home(&tmpdir(&format!("child-home-{}", up.port())));
    spawn_relay_child_at(up, creds_path, home)
}

/// 同上，家目录由调用方给（〔RK1〕泄露判据要一个**空**家目录：让子进程自己铸一把、判据事后才读到它）。
fn spawn_relay_child_at(
    up: SocketAddr,
    creds_path: &std::path::Path,
    home: std::path::PathBuf,
) -> RelayChild {
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
        .env(
            "CCM_AGENT_UPSTREAM_CLAUDE_CODE",
            format!("http://127.0.0.1:{}", up.port()),
        )
        .env(creds::ENV_CREDENTIALS, creds_path)
        .env("HOME", &home)
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
            // ★★ **次序是承重的：先写缓冲，再 `send`**〔`K-R24` 读出来的机制，09-04〕。
            //
            // 反过来（先 `send` 再写缓冲）的话，`rx.recv_timeout` 就是判据的**同步点**，
            // 而它返回的那一刻 banner 这一行**可能还没进 `err`** ⇒ 一条读
            // 「`err` 里有没有 / 有几行 `listening on`」的判据会**间歇性**红，
            // 报文里 `{err:?}` 印出来的正是一个空串。
            // ⇒ 今天由**构造**兜住：`send` 发生时缓冲里必然已经有它了，
            // 而 `send` 是判据唯一等得到的那个信号。
            // ⚠ 它买的是**这一个**窗口，不是「err 里所有内容都齐了」——
            //   banner **之后**那些行仍然是异步进来的，读它们的判据仍要自己等。
            err_c.lock().expect("lock").push_str(&line);
            err_c.lock().expect("lock").push('\n');
            if line.contains("listening on") {
                let _ = tx.send(line.clone());
            }
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
        home,
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
        "/s/claude-code/acctA/v1/messages",
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
    assert!(
        seen[0].ends_with("auth=true"),
        "auth 头必须被转发：{seen:?}"
    );
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
    assert!(
        !err.contains(SENTINEL),
        "哨兵串泄漏进了中转的 stderr：{err:?}"
    );
    assert!(
        !out.contains(SENTINEL),
        "哨兵串泄漏进了中转的 stdout：{out:?}"
    );
    assert!(
        !err.contains("Authorization"),
        "中转的 stderr 里出现了请求头名：{err:?}"
    );
    assert!(
        !out.contains("Authorization"),
        "中转的 stdout 里出现了请求头名：{out:?}"
    );
}

/// ★★★ **`KS3`：金丝雀走完整路径，四个出口都不许出现它。**
///
/// # 它与隔壁那条哨兵判据**守的是两扇门**，别读成一件事
///
/// `a_sentinel_auth_header_shows_up_in_neither_the_relay_processs_stderr_nor_its_stdout`
/// 喂进去的是**客户端发来的**那个头 —— 它证的是「**进来的**东西没被记下来」。
/// 本条喂的是**中转进程的上游选择从那份文件里读出来、替客户端换上去的那把 key**，
/// 那是**另一个值、从另一条路进来**。件计划 `§0` 逐字：**判据守的是前门，key 从后门进**，
/// 而那一形「同时骗过了 PM 与一路审计」。⇒ **两条缺一都不成立。**
///
/// # 走的是真实的那条路（不是在一个 crate 里自问自答）
///
/// 凭据是**裸 `fs::write` 写的一份 JSON**（= 人拿编辑器写的），中转是**真子进程**，
/// 它自己走 `creds::resolve_path` → `creds::load` → `Relay::with_key` →
/// `render_upstream_request` 这条生产段的路。**客户端一个凭据都不配。**
///
/// # 四个出口，逐个说它怎么量的
///
/// ㈠ **标准输出**：子进程 stdout（tee 的真落点）全量收集面。
/// ㈡ **回给前端的每一帧**：tee 的 NDJSON 行（在 stdout 里）**加上**回给下游客户端的原始字节。
/// ㈢ **错误消息**：子进程 stderr 全量收集面 **加上** 两条真实错误响应的响应体
///    （404 路由不认 · 400 请求体长度读不懂）。
///    ⚠ `KS3` 逐字「**这一条不许只测正常流程**」，所以这两发是必须的。
/// ㈣ **panic 消息**：panic 落的也是子进程 stderr（㈢ 已全量扫）。
///    ⚠ **如实说它证到哪儿**：本条**没有构造出一次真 panic**，
///    所以㈣是「**如果 panic 了，它的文字也在我扫的那条流上**」，
///    **不是**「我打过一次 panic 且它没泄漏」。另一半由 `SecretKey` 手写的 `Debug`
///    恒为遮蔽形兜（`creds-core` 的 `a_debug_print_never_carries_the_plaintext_or_its_length`）。
///
/// # 它**证不了**什么
///
/// - 上游那一跳之后的事（TLS、真 API）—— `C7` 禁「绝不起真 claude」。
/// - 上游连不上那条错误支（要一台死掉的上游，得再起一个子进程）。**登记为没测**
///   〔下一条 `each_account_gets_its_own_key_…` 的 ㈢ 补上了；今天那一支回 504，`设计/20 §3.1a`〕。
#[test]
fn the_substituted_key_never_shows_up_in_any_of_the_four_exits() {
    // 金丝雀取一个**不可能自然出现**的串，且**不含任何路径成分**
    //（诊断常把路径原样印进输出 ⇒ 那时「输出里含某句话」会靠路径恒真）。
    const CANARY: &str = "sk-ant-CANARY-MUST-NEVER-LEAVE-THIS-PROCESS";

    let dir = tmpdir("canary");
    let creds_path = dir.join("apikey-credentials.json");
    // ← 这一行就是「人拿编辑器写了一份 JSON 放进去」。没有界面、没有 IPC、没有迁移步骤。
    std::fs::write(
        &creds_path,
        format!("{{\n  \"_note\": \"hand written\",\n  \"api_key\": \"{CANARY}\"\n}}\n"),
    )
    .expect("写凭据夹具");

    let up = spawn_fake_upstream(None);
    let relay = spawn_relay_child_with_creds(up.addr, &creds_path);

    // ── 正常流程：客户端**一个凭据都不配** ──────────────────────────────
    // ⚠ `K-H2`：账号段是 `default` —— 这份夹具走的是**顶层一把 key** 那个旧形状
    //   （`K-H2a` 交付时的样子），它被读成一条 id 逐字是 `default` 的**有名字的行**。
    //   ⇒ 本条同时是「旧文件升级之后照常能用」的端到端判据。
    let mut c = send_request(relay.addr, "/s/claude-code/default/v1/messages", "");
    let mut got = Vec::new();
    c.read_to_end(&mut got).expect("read");
    let downstream = String::from_utf8_lossy(&got).to_string();
    assert!(
        downstream.starts_with("HTTP/1.1 200"),
        "这一趟得真走完一条转发：{downstream:?}"
    );

    // ── 错误路径（`KS3` 逐字：不许只测正常流程）──────────────────────────
    let (not_found, _) = send_raw(relay.addr, "GET /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/nope HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
    assert!(
        not_found.starts_with("HTTP/1.1 404"),
        "非空对照：这一发该是 404，实得 {not_found:?}"
    );
    let (bad_len, _) = send_raw(
        relay.addr,
        "POST /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/claude-code/default/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 7abc\r\n\r\n",
    );
    assert!(
        bad_len.starts_with("HTTP/1.1 400"),
        "非空对照：这一发该是 400，实得 {bad_len:?}"
    );

    // ── 非空对照 A：**换头真的发生了** ────────────────────────────────
    // 没有这一格，下面四条「零出现」可能只是因为那把 key 压根没被用过。
    let auths = up.auth_values.lock().expect("lock").clone();
    assert_eq!(auths.len(), 1, "上游应当恰好收到一次鉴权头：{auths:?}");
    assert!(
        auths[0].contains(CANARY),
        "上游收到的鉴权头里没有那把 key —— 换头没发生，本条下面全是空真：{auths:?}"
    );
    assert!(
        auths[0].starts_with("Authorization: Bearer "),
        "换上去的头形状不对：{auths:?}"
    );

    // ── 非空对照 B：两条采集面都是活的 ──────────────────────────────
    assert!(
        wait_until(|| relay.out().contains("\"event\"")),
        "非空对照：子进程 stdout 上一条事件行都没有 —— 采集面是死的，\
             下面那条「零出现」就是空真。stdout 现在是：{:?}",
        relay.out()
    );
    let err = relay.err();
    let out = relay.out();
    assert!(
        err.contains("listening on"),
        "非空对照：子进程 stderr 一个字都没收到 —— 采集面是死的：{err:?}"
    );
    // 非空对照 C：子进程**真的读了那份文件**（凭据那条路跑过了，不是被跳过）。
    assert!(
        err.contains("credentials: configured"),
        "非空对照：子进程没报告它读到了凭据 —— 那条路没跑过：{err:?}"
    );

    // ── 正题：四个出口，一个字节都不许有 ────────────────────────────
    assert!(
        !out.contains(CANARY),
        "㈠ 标准输出（tee）里出现了 key：{out:?}"
    );
    assert!(
        !downstream.contains(CANARY),
        "㈡ 回给下游客户端的字节里出现了 key：{downstream:?}"
    );
    assert!(
        !err.contains(CANARY),
        "㈢ stderr（含错误与 panic）里出现了 key：{err:?}"
    );
    assert!(
        !not_found.contains(CANARY) && !bad_len.contains(CANARY),
        "㈢ 错误响应里出现了 key：404={not_found:?} / 400={bad_len:?}"
    );
    // 顺带：连**文件路径**都不该带着 key（有人把整份文件内容印出来的话会撞这条）。
    assert!(
        !err.contains("hand written"),
        "stderr 里出现了凭据文件的**内容**（不只是路径）：{err:?}"
    );
    // ㈣ 的另一半：这一趟里子进程没 panic（panic 了上面那条 stderr 断言仍会扫到它）。
    assert!(
        !err.contains("panicked at"),
        "子进程 panic 了 —— 这一趟的读数按 CRASH 记，不是「零出现」：{err:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ============================================================ `K-H2` `KH2` / `KH4`

/// 起一个中转，**表由调用方给**。tee 丢进黑洞（本族判据量的不是 tee）。
fn spawn_relay_with_table(table: RoutingTable) -> SocketAddr {
    let relay = Arc::new(Relay::new(
        dest_of(table),
        door::Key::for_tests(),
        TeeSink::new(Box::new(std::io::sink())),
        DOWNSTREAM_DEADLINE,
        UPSTREAM_DEADLINE,
    ));
    let listener = listen(0).expect("listen");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || serve(listener, relay, Default::default()));
    addr
}

/// 一个**没人监听**的回环地址：绑一个再立刻放掉。
///
/// ⚠ 严格说这是一个 TOCTOU（放掉之后到用之前，内核可能把它分给别人）——
/// 本机跑一趟判据的那几毫秒里这概率可以忽略，**但它不是零**，如实记着。
/// 临时端口段的下界。**低于它的端口，内核永远不会分配给 `bind(port 0)`。**
///
/// Linux 默认 `32768 60999`（`/proc/sys/net/ipv4/ip_local_port_range`，本机现打一致）。
/// ⚠ 这里写死一个**保守**的常量而不是去读 `/proc`：读 `/proc` 是 Linux 专有，
/// 而本条要的只是「够低」。真实下界比它高时本常量照样安全（更保守）；
/// 真实下界比它低的机器上，下面那条判据会红并印出真值。
const EPHEMERAL_FLOOR: u16 = 32768;

/// 一个**没人监听、而且不可能被临时端口分配撞上**的回环端口。
///
/// # 🔴 上一版是一条 TOCTOU，而它会让这一族间歇性红
///
/// 上一版逐字是「`bind(port 0)` 拿一个号 → `drop(l)` 放掉 → 把号交出去」。
/// **放掉的那一刻起，那个号就回到了临时端口池里** —— 而 `cargo test` 并行跑，
/// 同一个进程里别的判据（假上游、中转子进程）全都在 `bind(port 0)`
/// ⇒ 内核完全可能把**刚放掉的那个号**分给它们。
///
/// 后果具体：`acct-dead` 那一行的 `base_url` 指着的不再是「没人听」，
/// 而是**另一个真在听的进程**。若那个进程恰好是**中转自己**，
/// 它会收到一条没有 `/s/` 前缀的 `GET /v1/messages`
/// ⇒ `route::parse` 回 `None` ⇒ **404**，而期望是 504（`设计/20 §3.1a` 之前是 502）。
///
/// 🔴 2026-09-22 门禁现打抓到过一次这一形（`each_account_gets_its_own_key_…`
/// 实得 `HTTP/1.1 404 Not Found` ＋ `Content-Length: 14`）。
/// ⚠ **那一次到底是不是这条路径，分不出来** —— 中转的两处 404
/// （表里没这一行 / 路由键解析不了）走的是同一个 `respond_status`，
/// **回的字节逐字节相同**。⇒ 本拍把这条 TOCTOU 结构性地消掉（它本来就是错的），
/// 同时把那条断言的诊断加强到下次能分辨（见它自己那一处）。
///
/// # 修法：挑一个**低于临时端口段下界**的号
///
/// 低于 [`EPHEMERAL_FLOOR`] 的端口内核永远不会分配给 `bind(0)`
/// ⇒ 本族任何一条判据都**抢不走**它。剩下的只是「此刻确实没人监听」，
/// 现探一遍（连上去被拒才算数）。
fn a_port_nobody_listens_on() -> u16 {
    // 从一个不常用的低段往上找。1024 以下要 root 才 bind 得了 ⇒ 更安全，
    // 但有些环境里 1–1023 会被别的服务占，所以直接从 1024 往上探。
    for p in 1024u16..EPHEMERAL_FLOOR {
        // 连得上 = 有人在听 ⇒ 换下一个。连不上（拒绝 / 超时）= 就是它。
        let refused = std::net::TcpStream::connect_timeout(
            &SocketAddr::new(LOOPBACK, p),
            std::time::Duration::from_millis(50),
        )
        .is_err();
        if refused {
            return p;
        }
    }
    panic!("1024..{EPHEMERAL_FLOOR} 之间一个没人监听的回环端口都找不到 —— 这台机器不对劲");
}

/// ★★★ **`KH2` 的正主**：一条路由键在表里**找不到** ⇒ **404，且一个字节都不到上游**。
///
/// # 为什么断「上游一次都没被连」而不是「上游没收到 Authorization」
///
/// 后者弱：它容许「连上了、发了请求、只是没带鉴权头」。而本件最坏的失效形态是
/// **请求本身跑到了另一个账号的端点上** —— 那时就算没带头，请求体（一整份会话上下文）
/// 已经出去了。⇒ 断的是**上游的连接数为 0**。
///
/// ⚠⚠ **非空对照承重**：同一个进程、同一把尺子，**配得到**的那条路上
/// 必须看得见「上游收到了 1 次，而且那一次带着这一行自己的 key」。
/// 没有这一格，上面那个 0 可能只是因为中转整个是死的。
#[test]
fn an_account_that_is_not_in_the_table_gets_404_and_nothing_reaches_upstream() {
    let up = spawn_fake_upstream(None);
    let base = Base::parse(&format!("http://127.0.0.1:{}", up.addr.port())).expect("base");
    // 表里**只有** `acctA`。
    let addr = spawn_relay_with_table(table_of(&[(ACCT_A, &base, Some("KEY-OF-A"))]));

    // ── ㈠ 表里没有的那个账号段 ────────────────────────────────
    let mut c = send_request(
        addr,
        "/s/agentA/acctB/v1/messages",
        "Authorization: Bearer THEIRS\r\n",
    );
    let mut got = Vec::new();
    c.read_to_end(&mut got).expect("read");
    let miss = String::from_utf8_lossy(&got).to_string();

    // ── ㈠ 上游**一次都没被连**（比「没收到鉴权头」强）────────────
    //
    // ⚠⚠ **顺序是刻意的，经过记下来**：这两条先前是「先断状态码、再断上游」，
    //    而变异实测发现**两刀都先炸状态码那条** ⇒ 上游那条根本没被求值，
    //    「它有没有牙」一次都没被证明过。断言是顺序求值的 ——
    //    **把承重的那条放前面**，两条才各自有各自的死值验。
    //    （`MU-KH2a` 回落 ⇒ 本条红；`MU-KH2b` 改回 502 ⇒ 本条绿而下面那条红。）
    assert_eq!(
        up.seen.lock().expect("lock").len(),
        0,
        "查不到的那一发跑到上游去了 —— 那就是回落。下游拿到的是：{miss:?}"
    );
    assert_eq!(
        up.auth_values.lock().expect("lock").len(),
        0,
        "查不到的那一发把鉴权头发出去了"
    );

    // ── ㈡ 而且回的是 404 ──────────────────────────────────────
    assert!(
        miss.starts_with("HTTP/1.1 404"),
        "表里查不到的账号段该回 404，实得：{miss:?}"
    );

    // ── ★★ 非空对照：配得到的那条路走得通，而且带的是**它自己那把** key ──
    let mut c2 = send_request(
        addr,
        "/s/agentA/acctA/v1/messages",
        "Authorization: Bearer THEIRS\r\n",
    );
    let mut got2 = Vec::new();
    c2.read_to_end(&mut got2).expect("read");
    let hit = String::from_utf8_lossy(&got2).to_string();
    assert!(
        hit.starts_with("HTTP/1.1 200"),
        "非空对照失败：配得到的那条路也不通，上面那个 0 证不了什么：{hit:?}"
    );
    let seen = up.seen.lock().expect("lock").clone();
    assert_eq!(seen.len(), 1, "上游应当恰好被连一次：{seen:?}");
    let auths = up.auth_values.lock().expect("lock").clone();
    assert_eq!(auths.len(), 1, "上游应当恰好收到一次鉴权头：{auths:?}");
    assert!(
        auths[0].contains("KEY-OF-A"),
        "换上去的不是这一行自己那把 key：{auths:?}"
    );
    assert!(
        !auths[0].contains("THEIRS"),
        "客户端那份鉴权头没被丢掉：{auths:?}"
    );
}

/// ★★★ **`KH4`：金丝雀的多账号版。**配两个账号、两把**不同**的假 key，
/// 走**真子进程 + 真转发**，断言：
/// ① 每条路由键收到的是**它自己那把**（**不是另一把**）；
/// ② 两把 key 都不出现在 stdout（tee）/ 回给下游的字节 / stderr / 错误响应里。
///
/// ★ **①比②更要紧**：② 是 `K-H2a` 已经买到的，① 是本件新增的风险。
///
/// # ⚠ 它顺带补上了 `K-H2a` 留下的**连不上上游那一格**
///
/// 件计划逐字记着 `K-H2a` 的诚实边界：「**502 那条错误支没测**（已测 404/400）」。
/// 这里第三个账号 `acct-dead` 的 `base_url` 指着一个**没人监听**的回环端口
/// ⇒ 连上游失败 ⇒ 走「中转传输失败」那一支（`设计/20 §3.1a` 之后回 **504**，先前是 502），
/// 而它的 stderr 那一行（`[relay] upstream failed: …`）也一并进了下面四个出口的扫描面。
///
/// # ⚠ 它**仍然没有**补上的那一格
///
/// **panic 那一格没构造出真 panic** —— 与 `K-H2a` 逐字相同，本件也没有构造它的路子。
/// 下面那条 `!err.contains("panicked at")` 断的是「这一趟没 panic」，
/// **不是**「panic 了也不泄漏」。后一句今天仍是**判不了**，原样抬进上报口。
#[test]
fn each_account_gets_its_own_key_and_neither_key_shows_up_in_any_exit() {
    // 两把金丝雀：**不可能自然出现**，且不含任何路径成分。
    const CANARY_A: &str = "sk-ant-CANARY-ACCOUNT-AAA-MUST-NEVER-LEAK";
    const CANARY_B: &str = "sk-ant-CANARY-ACCOUNT-BBB-MUST-NEVER-LEAK";
    const CANARY_DEAD: &str = "sk-ant-CANARY-ACCOUNT-DEAD-MUST-NEVER-LEAK";

    let dead_port = a_port_nobody_listens_on();
    let dir = tmpdir("canary-multi");
    let creds_path = dir.join("apikey-credentials.json");

    // ★★★ **两个不同的活上游**〔`D1` 阻-2 回修，08-28〕。
    //
    // ⚠⚠ **先前这里只有一个假上游，两条账号都不写 `base_url`** ⇒ 它们**共用同一个端点**
    //    ⇒「A 的 key 发到了 B 的端点」这一向**恒真、量不到**：不论表怎么错，
    //    字节都落在同一个进程上，两条断言看不出任何差别。
    // ★ 而**本件题目就是接第三方 API，每行端点不同才是正常形态** ——
    //    先前那个夹具用的恰恰是最不正常的那一种。
    // ⇒ 今天两行各指各的活上游，下面按**哪个上游收到了什么**对账。
    let up_a = spawn_fake_upstream(None);
    let up_b = spawn_fake_upstream(None);
    let (port_a, port_b) = (up_a.addr.port(), up_b.addr.port());
    // 反空真自检：两个上游**真的不是同一个**（否则下面整族断言恒真）。
    assert_ne!(port_a, port_b, "两个假上游撞到同一个端口 —— 本条整族在空转");

    // ← 人拿编辑器写的一份**多账号** JSON。没有界面、没有 IPC、没有迁移步骤。
    std::fs::write(
        &creds_path,
        format!(
            "{{\n  \"_note\": \"hand written multi\",\n  \"accounts\": {{\n\
                 \x20   \"acct-a\": {{ \"api_key\": \"{CANARY_A}\", \"base_url\": \"http://127.0.0.1:{port_a}\" }},\n\
                 \x20   \"acct-b\": {{ \"api_key\": \"{CANARY_B}\", \"base_url\": \"http://127.0.0.1:{port_b}\" }},\n\
                 \x20   \"acct-dead\": {{ \"api_key\": \"{CANARY_DEAD}\", \"base_url\": \"http://127.0.0.1:{dead_port}\" }}\n\
                 \x20 }}\n}}\n"
        ),
    )
    .expect("写凭据夹具");

    // 子进程那个 `CCM_AGENT_UPSTREAM_CLAUDE_CODE` 只当**默认上游**用；本判据里三行都写了
    // 自己的 `base_url` ⇒ 默认那一格在这里**一次都用不上**（这正是要的）。
    let relay = spawn_relay_child_with_creds(up_a.addr, &creds_path);

    // ── ㈠ 两条路各走一趟 ──────────────────────────────────────
    let mut downstream = String::new();
    for acct in ["acct-a", "acct-b"] {
        let mut c = send_request(
            relay.addr,
            &format!("/s/claude-code/{acct}/v1/messages"),
            "",
        );
        let mut got = Vec::new();
        c.read_to_end(&mut got).expect("read");
        let text = String::from_utf8_lossy(&got).to_string();
        assert!(
            text.starts_with("HTTP/1.1 200"),
            "{acct} 那一发得真走完一条转发：{text:?}"
        );
        downstream.push_str(&text);
    }

    // ── ★★★ ① **每一发都到它自己那个端点，带着它自己那把 key** ──────
    //
    // ⚠ 这一族有**两个自变量**（端点 · key），先前的夹具只量得到后者。
    //   今天两行各有各的活上游 ⇒ 「A 的 key 发到了 B 的端点」这一向**量得到了**。
    //   ⚠⚠ **承重的那条排最前**（本件已经栽过两次「最后那条从没被求值」）：
    //   先断**落点**，再断**内容**。
    let auths_a = up_a.auth_values.lock().expect("lock").clone();
    let auths_b = up_b.auth_values.lock().expect("lock").clone();

    // ㈠-1 落点：两个上游**各收到恰好一发**。
    assert_eq!(
        up_a.seen.lock().expect("lock").len(),
        1,
        "A 那个端点应当恰好收到一发；A={auths_a:?} B={auths_b:?}"
    );
    assert_eq!(
        up_b.seen.lock().expect("lock").len(),
        1,
        "B 那个端点应当恰好收到一发 —— 两发都落到 A 上就是「端点没跟着行走」；\
             A={auths_a:?} B={auths_b:?}"
    );

    // ㈠-2 内容：**落在哪个端点，带的就是那个端点那一行的 key**。
    assert_eq!(auths_a.len(), 1, "A 端点收到的鉴权头条数不对：{auths_a:?}");
    assert_eq!(auths_b.len(), 1, "B 端点收到的鉴权头条数不对：{auths_b:?}");
    assert!(
        auths_a[0].contains(CANARY_A) && !auths_a[0].contains(CANARY_B),
        "A 的端点上收到的不是 A 那把 key：{auths_a:?}"
    );
    assert!(
        auths_b[0].contains(CANARY_B) && !auths_b[0].contains(CANARY_A),
        "B 的端点上收到的不是 B 那把 key —— 这正是「A 的 key 发到 B 的端点」那一形：{auths_b:?}"
    );
    // 反空真：两把确实**不一样**、两个端点也确实**不是同一个**（否则上面全恒真）。
    assert_ne!(CANARY_A, CANARY_B);
    assert_ne!(port_a, port_b);

    // ── ㈡ 表里没有的账号段 ⇒ **两个上游那本账都一次都没涨**，然后才是 404 ──
    let (not_found, _) = send_raw(
        relay.addr,
        "GET /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/claude-code/acct-nope/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    );
    assert_eq!(
        up_a.auth_values.lock().expect("lock").len(),
        1,
        "404 那一发跑到 A 的端点去了 —— 那就是回落"
    );
    assert_eq!(
        up_b.auth_values.lock().expect("lock").len(),
        1,
        "404 那一发跑到 B 的端点去了 —— 那就是回落"
    );
    assert!(
        not_found.starts_with("HTTP/1.1 404"),
        "表里查不到的账号段该回 404：{not_found:?}"
    );

    // ── ㈢ **连不上上游那一支**（`K-H2a` 留下的那一格；`设计/20 §3.1a` 之后回 504）──────────
    let (bad_gateway, _) = send_raw(
        relay.addr,
        "GET /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/claude-code/acct-dead/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    );
    assert!(
        bad_gateway.starts_with("HTTP/1.1 504"),
        "上游连不上那一支该回 504：{bad_gateway:?}\n\
         ★ 实得 404 时**先分辨是哪一种** —— 中转的两处 404 回的字节逐字节相同：\n\
           ① 表里没有 `acct-dead`（`decide` 的 `Refuse`，`why` 是「代入模式要求表里有这一行」）；\n\
           ② 那条「没人听」的端口上**其实有人听**，而那个人回了 404\n\
              （最坏的一种：它就是中转自己 ⇒ 收到没有 `/s/` 前缀的路径 ⇒ `route::parse` 回 `None`）。\n\
         ⇒ 现打这一趟的三个号，按它们判：dead={dead_port} relay={} a={port_a} b={port_b}。\n\
           **dead 与其中任何一个相等 = 第 ② 种**（2026-09-22 之前 `a_port_nobody_listens_on`\n\
           是 `bind(0)` 拿号再放掉，那个号会回到临时端口池 ⇒ 被并行的别人抢走）。\n\
           三个都不等 ⇒ 第 ① 种，去看中转子进程 stderr 上有没有「被拒的行」。",
        relay.addr.port()
    );

    // ── 非空对照：两条采集面都是活的 ───────────────────────────
    assert!(
        wait_until(|| relay.out().contains("\"event\"")),
        "非空对照：子进程 stdout 上一条事件行都没有 —— 采集面是死的，\
             下面那几条「零出现」就是空真。stdout 现在是：{:?}",
        relay.out()
    );
    let out = relay.out();
    let err = relay.err();
    assert!(
        err.contains("listening on"),
        "非空对照：子进程 stderr 一个字都没收到 —— 采集面是死的：{err:?}"
    );
    assert!(
        err.contains("credentials: configured"),
        "非空对照：子进程没报告它读到了凭据 —— 那条路没跑过：{err:?}"
    );
    // 非空对照：tee 行里带着**账号**那一格（路由键三段都落到了 tee 上）。
    assert!(
        out.contains("\"account\":\"acct-a\"") || out.contains("\"account\": \"acct-a\""),
        "tee 行里没有账号那一格：{out:?}"
    );

    // ── ★★ ② 两把 key，四个出口，一个字节都不许有 ─────────────
    //
    // ⚠⚠ **② 排在最后是刻意留的，不是漏扫**〔`D2` `§三㈡` 点名要这句话，`C-补` 08-28 补〕。
    // `D1-M6` 逮到的那一形是「**承重的行为断言排在后面 ⇒ 一次都没被求值 ⇒ 它有没有牙从没被证过**」，
    // 本件为它**全件扫过一遍**；这一处是那次扫描里**唯一一处留在原位**的行为断言。理由两条：
    //   ① 挪它要先重排 ㈡/㈢ —— 它读的 `not_found` / `bad_gateway` 两个变量在那两段里才拿到；
    //   ② 件计划 `§1 KH4` 逐字写着「**①比②更要紧**」（① 是本件新增的风险，② 是 `K-H2a` 已经买到的）
    //      ⇒ ① 排最前**符合它自己的优先级**。
    // ⇒ 代价照实说：前面几条炸掉时 ② 不被求值（`D1` 五刀里有四刀都在它之前炸）。
    //   **而这一格已经单独还上了** —— ② 的死值验由 `D1-M5`（把整条上游请求头印进 stderr）承担：
    //   **484 passed / 4 failed**，红在「㈢ stderr 里出现了 A 的 key」（`D1` 报的住址：
    //   `server_tests.rs::the_substituted_key_never_shows_up_in_any_of_the_four_exits` 的 ㈢ 那条断言）。
    //   ⇒ **② 有牙，是被那一刀单独证过的，不是靠这里的排序证的。**
    for (who, canary) in [("A", CANARY_A), ("B", CANARY_B), ("dead", CANARY_DEAD)] {
        assert!(
            !out.contains(canary),
            "㈠ 标准输出（tee）里出现了 {who} 的 key：{out:?}"
        );
        assert!(
            !downstream.contains(canary),
            "㈡ 回给下游客户端的字节里出现了 {who} 的 key：{downstream:?}"
        );
        assert!(
            !err.contains(canary),
            "㈢ stderr 里出现了 {who} 的 key：{err:?}"
        );
        assert!(
            !not_found.contains(canary) && !bad_gateway.contains(canary),
            "㈢ 错误响应里出现了 {who} 的 key：404={not_found:?} / 502={bad_gateway:?}"
        );
    }
    // 顺带：连凭据文件的**内容**都不该被印出来（只许印路径）。
    assert!(
        !err.contains("hand written multi"),
        "stderr 里出现了凭据文件的内容（不只是路径）：{err:?}"
    );
    // ㈣ 的另一半：这一趟里子进程没 panic。
    // ⚠ 它**不是**「panic 了也不泄漏」——那一格今天仍然判不了（见本判据头注）。
    assert!(
        !err.contains("panicked at"),
        "子进程 panic 了 —— 这一趟的读数按 CRASH 记，不是「零出现」：{err:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// `KS2` 的行为那一半：**配了 key 就换头，没配就原样转发** —— 两支都要有判据。
///
/// ⚠⚠ **`K-H2` 之后这两支的意思变了，别按旧的读**：
/// 「没配」不再是「**这个中转**没配 key」，而是「**这一行**没配 key」——
/// 那是订阅登录那一档的**合法状态**（`table::Row::key` 的头注逐字）。
/// 「表里根本没有这一行」是**另一件事**，归 `KH2` 的 404，判据在
/// `an_account_that_is_not_in_the_table_gets_404_and_nothing_reaches_upstream`。
#[test]
fn a_configured_key_replaces_the_clients_header_instead_of_being_appended() {
    const MINE: &str = "sk-ant-MINE";
    let head = http1::parse_request(
        b"POST /s/a/acct/v1/x HTTP/1.1\r\nHost: relay\r\nAuthorization: Bearer THEIRS\r\nContent-Length: 3\r\n\r\n",
    )
    .expect("parse");
    let base = Base::parse("https://api.example.com").expect("base");
    // 两行：一行配了 key，一行没配。**同一张表**里取，走的是生产段那条真实的路。
    let t = table_of(&[("with", &base, Some(MINE)), ("without", &base, None)]);

    let with = render_via_upstream_selection(&t, "with", &head, "/v1/x", 3);
    // ★ **恰好一个** `Authorization` —— 追加一条会让上游看见两个，那是未定义行为。
    assert_eq!(
        with.matches("Authorization:").count(),
        1,
        "换头之后鉴权头不止一个：{with:?}"
    );
    assert!(with.contains(&format!("Authorization: Bearer {MINE}\r\n")));
    assert!(
        !with.contains("THEIRS"),
        "客户端那份鉴权头没被丢掉：{with:?}"
    );

    // 非空对照：**这一行没配** key 时是原样转发（不是恒替换）。
    let without = render_via_upstream_selection(&t, "without", &head, "/v1/x", 3);
    assert!(without.contains("Authorization: Bearer THEIRS\r\n"));
    assert!(!without.contains(MINE));
}

/// ★★★ **`K-R1` 的正主之一**：**鉴权头形状跟着那一行走** ——
/// 拿 A 风格的行发不出 B 风格的头。
///
/// # 死值验落在哪一格
///
/// 把 `accounts::upstream::auth_header_of`（`P16` 之后住上游选择）里 `XApiKey` 那一支改成 `Some(("Authorization", "Bearer "))`
/// （形状对、恒答默认那张脸）⇒ 本条的 `x-api-key` 那几格当场红，
/// 而**默认那一行**那几格仍绿 ⇒ 这一刀是**单断**，不是目录级塌陷。
///
/// # ⚠ 它证不了什么
///
/// 证不了「某一家上游真的认这个头」—— 那要打真网，本轮禁（沙箱默认断网）。
/// 它证的是「**文件里写什么，线上就发什么**」。
/// ⚠ **分母**（自查订正，09-04）：初稿在这里写「那正是先前**唯一**没人量过的一格」——
/// 那是一句没有分母的全称。准确的说法是：**我读过的 `relay/` 那一面里**，
/// 「换哪个头」此前没有任何判据问过（换头那一行的**值**由 `KS2`/`KS3` 那几条盯着，
/// 而**头名**是写死的字面量，没人问）。「先前还有多少格没人量过」我没数，也数不出。
#[test]
fn the_auth_header_shape_follows_the_row_and_not_a_process_wide_guess() {
    const MINE: &str = "sk-ROW-OWN-KEY";
    use creds_core::store::AuthStyle;
    let head = http1::parse_request(
        b"POST /s/a/acct/v1/x HTTP/1.1\r\nHost: relay\r\nAuthorization: Bearer THEIRS\r\nx-api-key: THEIRS-XAK\r\nContent-Length: 3\r\n\r\n",
    )
    .expect("parse");
    let base = Base::parse("https://api.example.com").expect("base");
    // 三行**同一把 key、同一个端点**，只有鉴权头形状不同 ⇒ 量到的差别只能来自那一格。
    let t = table_of_styled(&[
        ("bearer", &base, Some(MINE), AuthStyle::Bearer),
        ("xapikey", &base, Some(MINE), AuthStyle::XApiKey),
        ("noauth", &base, None, AuthStyle::NoAuth),
    ]);
    let render = |id: &str| render_via_upstream_selection(&t, id, &head, "/v1/x", 3);

    let b = render("bearer");
    let x = render("xapikey");
    let n = render("noauth");
    // ★★ **反空真排最前**：三份渲染两两不同（一样的话下面整族断言恒真）。
    assert_ne!(b, x, "两种形状渲染出同一份请求头 —— 那一格没被读");
    assert_ne!(b, n);
    assert_ne!(x, n);

    // ㈠ Bearer 那一行：期望值是**手写字面量**。
    assert!(
        b.contains(&format!("Authorization: Bearer {MINE}\r\n")),
        "{b:?}"
    );
    assert_eq!(
        b.matches("Authorization:").count(),
        1,
        "同名鉴权头出现了两次：{b:?}"
    );
    // ㈡ x-api-key 那一行：换的是**那个头**，而且**不许**顺手也写一个 Authorization。
    assert!(x.contains(&format!("x-api-key: {MINE}\r\n")), "{x:?}");
    assert_eq!(
        x.matches("x-api-key:").count(),
        1,
        "同名鉴权头出现了两次：{x:?}"
    );
    assert!(
        !x.contains("Authorization"),
        "x-api-key 那一行还带了 Authorization —— 「拿 A 风格的行发 B 风格的头」正是本格要拦的：{x:?}"
    );
    // ㈢ 无鉴权那一行：**一个鉴权头都没有**，客户端那两份也没转过去。
    assert!(!n.contains("Authorization"), "{n:?}");
    assert!(!n.contains("x-api-key"), "{n:?}");
    assert!(
        !n.contains("THEIRS"),
        "客户端那份鉴权头被转给了本地端点：{n:?}"
    );
    // ㈣ 三行都**没有**把客户端那两份带上（它们有自己的 key / 声明了不发）。
    for (id, r) in [("bearer", &b), ("xapikey", &x)] {
        assert!(
            !r.contains("THEIRS"),
            "{id} 那一行把客户端的凭据一起送上去了：{r:?}"
        );
    }
    // ㈤ 非鉴权的头照旧原样转发（本条不许顺手变成一把大扫帚）。
    for r in [&b, &x, &n] {
        assert!(r.contains("Accept-Encoding: identity\r\n"));
        assert_eq!(r.matches("Content-Length:").count(), 1);
    }
}

/// 〔`P16` 2026-09-22〕**`every_header_this_relay_may_write_is_in_the_set_it_clears_first`
/// 搬去 `creds_guard` 了** —— 墓碑，别在这里重建一份。
///
/// 它焊的两端（「我可能写出来的头名」＝ `auth_header_of` · 「先丢掉哪几个」＝那份名单）
/// 本来都住中转。`P16` 把映射按 `C2` 搬去上游选择、名单改成由映射**派生**之后，
/// 两端都在上游选择 ⇒ 判据跟着搬到上游选择那侧（`creds_guard`），**正题一个字没松**，
/// 而且多买了一格：它现在还断言那个集合是**全集**（射程不许缩）。
///
/// ★★★ **`K-R1` 的正主之二**：`base_url` 里那一段路径前缀
/// **真的到了发给上游的请求行上**。
///
/// # 改前的读数（死值验的另一半）
///
/// 改前 `Base` 存不下路径 ⇒ 配 `https://gw/anthropic` 的人，请求实际打到
/// `https://gw/v1/messages`（前缀被静默丢掉、不记 `Rejected`、不 `announce`）。
/// ⇒ 把 `Row::upstream_target` 的返回改成 `rest.to_string()`（形状对、恒答
/// 「没有前缀」那张脸）= 把改前那一版原样装回来 ⇒ 本条当场红，
/// 而没配前缀的那一族判据**仍然全绿**（单断）。
#[test]
fn the_path_prefix_from_the_base_url_really_reaches_the_request_line() {
    let head = http1::parse_request(
        b"POST /s/a/acct/v1/messages HTTP/1.1\r\nHost: relay\r\nContent-Length: 0\r\n\r\n",
    )
    .expect("parse");
    let prefixed = Base::parse("https://gw.example.com/anthropic").expect("带前缀那一形");
    let bare = Base::parse("https://gw.example.com").expect("不带前缀那一形");
    let t = table_of(&[("with-prefix", &prefixed, None), ("no-prefix", &bare, None)]);
    let render = |id: &str, rest: &str| render_via_upstream_selection(&t, id, &head, rest, 0);

    // ★★ 承重的那一格排最前：期望值是**手写字面量**的整条请求行。
    let with = render("with-prefix", "/v1/messages");
    assert!(
        with.starts_with("POST /anthropic/v1/messages HTTP/1.1\r\n"),
        "前缀没到请求行上（改前那一版就是这个读数）：{with:?}"
    );
    // 查询串跟着走，一个字节不改。
    let q = render("with-prefix", "/v1/messages?beta=true");
    assert!(
        q.starts_with("POST /anthropic/v1/messages?beta=true HTTP/1.1\r\n"),
        "{q:?}"
    );
    // ★ 非空对照 + 单断：**没配前缀**的那一行与改前逐字节相同。
    let without = render("no-prefix", "/v1/messages");
    assert!(
        without.starts_with("POST /v1/messages HTTP/1.1\r\n"),
        "没配前缀的那一路被改了字节 —— 那一路本件要求它一个字节不动：{without:?}"
    );
    // `Host:` 头里**没有**那一段（前缀属于请求行）。
    assert!(with.contains("Host: gw.example.com\r\n"), "{with:?}");
    assert!(!with.contains("Host: gw.example.com/anthropic"), "{with:?}");
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
            "/s/claude-code/acctA/v1/messages?beta=true",
            &format!("x-claude-code-session-id: {key}\r\n"),
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
    let mut warmup = send_request(relay_addr, "/s/agentA/acctA/v1/messages", "");
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
    let mut c = send_request(relay_addr, "/s/agentA/acctA/v1/messages", "");
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
        "/s/agentA/acctA/v1/messages",
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
        b"POST /s/a/acct/v1/x HTTP/1.1\r\nHost: relay\r\nConnection: keep-alive\r\nAccept-Encoding: gzip, br\r\nAuthorization: Bearer T\r\nContent-Length: 3\r\n\r\n",
    )
    .expect("parse");
    let base = Base::parse("https://api.example.com").expect("base");
    // 这一行**没配 key** ⇒ 原样转发那一支（`K-H1` 甲半的形状）。换头那一支见下一条判据。
    let t = table_of(&[("acct", &base, None)]);
    let out = render_via_upstream_selection(&t, "acct", &head, "/v1/x", 3);
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
    let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n";
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
/// 〔AR1：那条源码扫描 `nodelay_guard` 已按 `设计/15 §2.1` B3 退役，本格是「关了 Nagle」唯一的判据〕
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
            "POST /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{body}",
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
    let relay = Relay::new(
        dest_of(two_accounts_no_key(&base)),
        door::Key::for_tests(),
        TeeSink::new(Box::new(std::io::sink())),
        DOWNSTREAM_DEADLINE,
        UPSTREAM_DEADLINE,
    );
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
    match upstream::connect(&base2, UPSTREAM_DEADLINE).expect("connect upstream") {
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
            "POST /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{body}",
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
    let relay = Relay::new(
        dest_of(two_accounts_no_key(&base)),
        door::Key::for_tests(),
        TeeSink::new(Box::new(std::io::sink())),
        DOWNSTREAM_DEADLINE,
        UPSTREAM_DEADLINE,
    );
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
    match upstream::connect(&base2, UPSTREAM_DEADLINE).expect("connect upstream") {
        upstream::Conn::Plain(s) => {
            assert_eq!(
                s.read_timeout().expect("getsockopt"),
                Some(UPSTREAM_DEADLINE),
                "上游方向：`upstream::connect()` 必须装上**读**期限"
            );
            assert_eq!(
                s.write_timeout().expect("getsockopt"),
                Some(UPSTREAM_DEADLINE),
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
    half.write_all(b"POST /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n")
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
        .write_all(b"POST /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
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
        UPSTREAM_DEADLINE > DOWNSTREAM_DEADLINE,
        "上游期限（{:?}）必须比下游（{:?}）宽：下游对端就在本机、慢是**异常**；\
             上游等的是模型在想、慢是**正常**。两个数写成一样就会掐断正常的长流。",
        UPSTREAM_DEADLINE,
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
    // ⚠ 〔「中转层里没有账号」的前置〕本条先前还量**上游**那一格（默认值 · 旋钮盖得住 ·
    //   路径前缀不丢 · 认不出回 `None`）。那是**上游选择的配置**，今天由上游选择自己解析 ——
    //   四格原样搬去了 `table_tests::each_agents_env_knob_overrides_only_that_agents_default`
    //   （手写字面量期望、带前缀与不带的非空对照都在那边）。本条只剩中转自己的端口。
    assert_eq!(resolve_port(None), 8788, "默认端口");
    assert_eq!(
        resolve_port(Some("19999")),
        19999,
        "CCM_RELAY_PORT 必须盖得住默认"
    );
    // 端口读不懂 ⇒ **回默认**，不是 0、也不是崩。
    for bad in ["not-a-port", "70000", "-1", ""] {
        assert_eq!(
            resolve_port(Some(bad)),
            8788,
            "读不懂的端口 {bad:?} 必须回默认"
        );
    }
}

/// ★★ `重要-3(D2)`：`run()` 那一层的**接线** —— 哪个环境变量喂给哪个配置位。
///
/// D2 实测：把 `run()` 里那两行 `std::env::var(...)` **对调** ⇒ **389 条判据全绿**
/// （`D2RUN`），而真机后果是 `--relay` 整个起不来。今天那条接线住 `run_reading`，
/// 取值器与执行体都注入 ⇒ 本条打得到它，**且不碰进程环境**。
///
/// 期望值全是**手写字面量**，不拿被测的 `ENV_PORT` / 上游选择那张表里的旋钮名去算。
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
    // ⚠ 〔条 60〕执行体的第 2 个位今天是**取值器本身**，不是「上游那个变量的值」：
    //   上游旋钮每家一个、名字住上游选择那张表，中转只把取值器原样递过去。
    //   ⇒ 本条量两件事：① 端口位读的是 `CCM_RELAY_PORT`；② 递下去的取值器**就是**入口收到的
    //   那一个（拿它问上游那个变量名，答回来的是**入口那个取值器**的答案）。
    let seen: std::sync::Mutex<Vec<(Option<String>, Option<String>)>> =
        std::sync::Mutex::new(Vec::new());
    let exec: &RelayExec<'_> = &|p, get, _home| {
        seen.lock()
            .expect("lock")
            .push((p.map(str::to_string), get("CCM_AGENT_UPSTREAM_CLAUDE_CODE")));
        7
    };

    // ㈠ 取值器把**变量名原样**当值返回 ⇒ 接线一旦对调，下面这句当场对不上。
    let echo: &dyn Fn(&str) -> Option<String> = &|k| Some(k.to_string());
    assert_eq!(
        run_reading(echo, &nowhere_home(), exec),
        7,
        "入口必须把执行体的退出码原样带回"
    );
    assert_eq!(
        seen.lock().expect("lock").clone(),
        vec![(
            Some("CCM_RELAY_PORT".to_string()),
            Some("CCM_AGENT_UPSTREAM_CLAUDE_CODE".to_string())
        )],
        "第 1 个配置位必须读 CCM_RELAY_PORT；递下去的取值器必须是入口收到的那一个"
    );

    // ㈡ 两个变量都没设 ⇒ 两个位都是 None，不是把变量名当默认值塞进去。
    seen.lock().expect("lock").clear();
    let none: &dyn Fn(&str) -> Option<String> = &|_| None;
    assert_eq!(run_reading(none, &nowhere_home(), exec), 7);
    assert_eq!(
        seen.lock().expect("lock").clone(),
        vec![(None, None)],
        "没设环境变量时两个配置位都该是 None"
    );

    // ㈢ 上游那个变量名**每家一个**，而读它的是上游选择：拿上游选择那张表问一遍，它问的正是那个名字。
    //    （期望值仍是手写字面量；被测的是 `Upstreams::from_env` 真的去问了它。）
    let asked: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let _ = accounts::Upstreams::from_env(&|k| {
        asked.lock().expect("lock").push(k.to_string());
        None
    });
    assert_eq!(
        asked.lock().expect("lock").clone(),
        vec!["CCM_AGENT_UPSTREAM_CLAUDE_CODE".to_string()],
        "上游选择该问的上游旋钮（今天只登记了 claude-code 一家）不是这一个"
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
fn relay_entry_exit_code_within_5s(port_env: Option<String>, upstream_env: Option<String>) -> i32 {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let get = move |k: &str| match upstream_env.as_deref() {
            Some(v) if k == "CCM_AGENT_UPSTREAM_CLAUDE_CODE" => Some(v.to_string()),
            _ => None,
        };
        let _ = tx.send(run_with(
            port_env.as_deref(),
            &get,
            &nowhere_home(),
            &accounts::Boot,
        ));
    });
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .expect("`--relay` 入口必须**返回** —— 超时说明它没退出，而是进了 serve()")
}

/// ★ `重要-6` 之二：**起不来就退出并出声**（`server.rs::DEFAULT_PORT` 头注承诺的处置）。
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
        resolve_port(Some(&port.to_string())),
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

// ─────────────────────────────────────────────────────────────────────────
// ★★★ `K-H2b` `KH2B1`：**一发真请求**从「起会话那条命令」走到中转、
//      被按账号路由到上游，并带着**那个账号那一行**的 key
// ─────────────────────────────────────────────────────────────────────────

/// 桩启动器。**它不是 claude**（红线 `C7`：绝不起真 claude），它只做一件事：
/// 读 `$ANTHROPIC_BASE_URL`、照它发一发 HTTP、把响应首行印出来。
///
/// ⇒ 这一趟里 **env 是真的、中转是真的（真子进程）、上游是真的**，
/// **只有 agent 是桩**。它买的是「env → 中转 → 上游」这一截。
///
/// ⚠⚠ **它买不到的那一截，写在这里**：`claude` 拿到这个变量之后到底怎么走
///（订阅号的 OAuth 刷新会不会仍打官方域名 · 只设 base URL 不设 token 会不会拒启 ·
/// `/v1/messages` 之外还打哪些路径）—— 仓里零证据、红线也禁止实测 ⇒ **`判不了`**。
/// 别把这条判据的绿读成「claude 会照它走」。
#[cfg(unix)]
const STUB_LAUNCHER: &str = r#"#!/usr/bin/env bash
set -eu
# 没被注入就**大声失败**（`:?`）—— 这正是「把注入点删掉」那一刀要撞上的地方。
url=${ANTHROPIC_BASE_URL:?ANTHROPIC_BASE_URL mei you bei zhu ru}
rest=${url#http://}
hostport=${rest%%/*}
path=/${rest#*/}
host=${hostport%%:*}
port=${hostport##*:}
exec 3<>/dev/tcp/$host/$port
body=hi
printf 'POST %s/v1/messages HTTP/1.1\r\nHost: %s\r\nContent-Length: %d\r\nConnection: close\r\n\r\n%s' $path $hostport ${#body} $body >&3
head -n 1 <&3
"#;

/// 〔RK1〕一条**不带钥匙**的中转 URL（monitor 侧 `payload::relay_base_url_in` 的产物形状）
/// → 生产渲染器那一形的 `export` 前缀：钥匙那一段是**读钥匙文件的命令替换**，在跑它的 shell 里展开。
///
/// ⚠ 与 monitor 侧 `payload::relay_env_prefix_posix` **同形、两处各写一份**（两半之间没有编译期边，
/// 下面 `KH2B1` 头注那条「没买到的缝」原样成立）；monitor 那一侧由它自己的判据钉「真 shell 展开后等于带钥匙的 URL」。
fn rendered_export(url: &str) -> String {
    let (origin, path) = url
        .strip_prefix("http://")
        .and_then(|r| r.split_once('/'))
        .map(|(hp, p)| (format!("http://{hp}/"), format!("/{p}")))
        .expect("夹具 URL 是 http://主机:口/路径 那一形");
    format!(
        "export ANTHROPIC_BASE_URL='{origin}'\"$(cat \"$HOME/{}\")\"'{path}'; ",
        door::KEY_FILE_REL
    )
}

/// ★★★ `KH2B1`。**判定不是「渲染串里含 `ANTHROPIC_BASE_URL`」** ——
/// 是「假上游真的收到了那一发，且它带的 `Authorization` 是**那个账号那一行**的 key」。
///
/// # 这一趟真实到什么程度（逐段说清，别读宽）
///
/// | 段 | 真的假的 |
/// |---|---|
/// | 起会话那条命令串 | **真的 shell**（`bash -c '<env 前缀><launcher>'`），形状与生产 POSIX 前缀同形 |
/// | env | **真的**（子进程自己从环境里读） |
/// | 中转 | **真子进程**（`spawn_relay_child_with_creds`：真 `Command::new(exe)` · 端口 0 从 stderr 读回） |
/// | 上游 | **真的** TCP 假上游，`auth_values` 收的是**整行** `Authorization:` |
/// | agent | **桩**（红线：绝不起真 claude） |
///
/// # ⚠ 一条**没买到**的缝，必须写下来
///
/// 这里的 env 前缀是**本判据自己拼的**，不是 monitor 侧
/// `backend::control::payload::relay_env_prefix_posix` 的返回值 ——
/// 两半之间的编译期边（`include_str!`）**必须登记进 `src/bridge/src/cross_half_edge_registry.rs`**，
/// 而那个文件不在本件写区里。⇒ **两侧今天靠「同一个形状写了两遍」，没有判据对拍。**
/// monitor 那一侧自己那半由 `the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched`
/// 与 `only_an_account_that_has_a_row_in_the_apikey_table_gets_the_base_url_prefix` 钉着。
/// **这一格如实登记为「没买到」，不许读成「对上了」。**
#[cfg(unix)]
#[test]
fn a_launch_command_carrying_the_relay_env_prefix_reaches_upstream_with_that_accounts_key() {
    let up = spawn_fake_upstream(None);
    let dir = tmpdir("kh2b1");
    let creds = dir.join("apikey-credentials.json");
    // 两条**各自带 key** 的行 —— 「拿 A 的 key 发 B 的请求」是本族最坏的失效形态，
    // 一行是量不出来的。
    // ⚠ 写法照 `spawn_relay_child` 那份夹具（转义的普通串，不是 `r#"…"#`）——
    //   源码扫描型守卫的剥法认的是普通字符串的 `\"` 转义，
    //   一个内含裸 `"` 的原始串会让它在这里失步、把整段测试当成生产段
    //   （本轮实测：`readonly_guard` / `bind_guard` / spawn 登记表三条一起红）。
    std::fs::write(
        &creds,
        b"{\n  \"accounts\": {\n    \"acct-a\": { \"api_key\": \"KEY-FOR-A\" },\n              \"acct-b\": { \"api_key\": \"KEY-FOR-B\" }\n  }\n}\n",
    )
    .expect("写两条账号的凭据夹具");
    let relay = spawn_relay_child_with_creds(up.addr, &creds);

    let stub = dir.join("stub-launcher.sh");
    std::fs::write(&stub, STUB_LAUNCHER).expect("写桩启动器");

    // 起两发：同一条起会话路径，**只有账号段不同**。
    for (acct, want_key) in [("acct-a", "KEY-FOR-A"), ("acct-b", "KEY-FOR-B")] {
        // 形状与 monitor 侧 `payload::relay_base_url` / `relay_env_prefix_posix` 同形
        //（那一侧自己有判据钉着；两侧之间没有，见头注那条「没买到的缝」）。
        let url = format!(
            "http://127.0.0.1:{}/s/claude-code/{acct}",
            relay.addr.port()
        );
        let cmd = format!("{}bash {}", rendered_export(&url), stub.to_string_lossy());
        let out = std::process::Command::new("bash")
            .env("HOME", &relay.home)
            .arg("-c")
            .arg(&cmd)
            .output()
            .expect("起桩启动器");
        let so = String::from_utf8_lossy(&out.stdout);
        let se = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "桩启动器没跑成 —— 注入点可能整个不在了。\n\
                 命令：{cmd}\nstdout：{so:?}\nstderr：{se:?}"
        );
        assert!(
            so.starts_with("HTTP/1.1 200"),
            "这一发没走完一条转发（账号 {acct}）：{so:?} / {se:?}"
        );
    }

    // ★ 正题①：**假上游真的收到了两发**，而且真路径是原样透传的那一条。
    let seen = up.seen.lock().expect("lock").clone();
    assert_eq!(
        seen.len(),
        2,
        "上游没收到两发 —— 「那条线接上了」这句话在这一趟里就是假的：{seen:?}"
    );
    for line in &seen {
        assert_eq!(
            line, "POST /v1/messages HTTP/1.1 auth=true",
            "路由键那几段没有被剥掉、或者鉴权头没换上：{seen:?}"
        );
    }
    // ★ 正题②：**两个账号各拿各的 key**（`KH2` 逐字点名的最坏失效形态的反面）。
    let auths = up.auth_values.lock().expect("lock").clone();
    assert_eq!(
        auths,
        vec![
            "Authorization: Bearer KEY-FOR-A".to_string(),
            "Authorization: Bearer KEY-FOR-B".to_string(),
        ],
        "两发拿到的 key 不是各自那一行的 —— 「拿 A 的 key 发 B 的请求，而两边都显示成功」\n\
             正是 `KH2` 逐字点名的最坏那一形。实得：{auths:?}"
    );

    // ★ 正题③（`KL7` 第 2 条）：**表里查不到的账号 ⇒ 404 且一个字节不发上游**。
    //   非空对照就是上面那两发 —— 同一条路、同一个桩，只有账号段不同。
    let url = format!(
        "http://127.0.0.1:{}/s/claude-code/acct-not-in-the-table",
        relay.addr.port()
    );
    let out = std::process::Command::new("bash")
        .env("HOME", &relay.home)
        .arg("-c")
        .arg(format!(
            "{}bash {}",
            rendered_export(&url),
            stub.to_string_lossy()
        ))
        .output()
        .expect("起桩启动器");
    let so = String::from_utf8_lossy(&out.stdout);
    assert!(
        so.starts_with("HTTP/1.1 404"),
        "表里查不到的账号没有回 404：{so:?}"
    );
    assert_eq!(
        up.seen.lock().expect("lock").len(),
        2,
        "查不到的那一发**漏到上游去了** —— 那是 `KL7` 第 2 条逐字禁的回落"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// ★★★ `K-H2c` `KH2C2`：**写侧产出的那份文件**能让那个账号的会话走到中转
// ─────────────────────────────────────────────────────────────────────────

/// 照 monitor 写侧那两步造一份凭据文件的内容。
///
/// # ★★ 它为什么不手写 JSON（这一格是本条判据的地基）
///
/// `KH2C2` 要证的是「**写侧产出的那份文件**能让那个账号的会话走到中转」。
/// 手写一份 JSON 只能证「**我以为写侧会产出的那个形状**能走通」——
/// 写侧哪天换个形状（换个字段名 / 换一层嵌套 / 换个 id），这条判据**照绿**。
///
/// ⇒ 这里调的是写侧生产段里逐字那两个纯函数（〔GP1 · 第四波〕写侧 ＝ 每台机器那台后端的 `accounts/upstream/file_face.rs`，
/// 本机也是；monitor 那侧当年的写口 `write_key_at`〔散文墓碑〕删了）
/// （`store::merge_account_key` + `store::to_pretty_json`），两侧因此**在 `creds-core`
/// 这个共同祖先上会合**：backend 单向依赖 `src/bridge/crates/*`，够得着它们。
///
/// # ⚠⚠ 它**买不到**什么 —— 逐字落在这里，别读宽〔PM `裁二`，09-02〕
///
/// > `KH2C2` 要读成「**写侧产出的那份文件**能让那个账号的会话走到中转」，
/// > **不是**「点了保存按钮之后」。那一跳归 `KH2C1`。
///
/// ⇒ **两条 DoD 合起来才是那条链**，各自都别读宽。这里没有被证到的两跳是：
/// ① 界面那条 IPC 命令真的被点出去（`KH2C1` 前端那两堵墙，住 `accounts-section` 那一侧）；
/// ② 写口里**写盘那一段**（tmp / 原子替换 / 收窄）——
///    本条只走它算内容的那两步，写盘由 monitor 侧那几条既有判据分管。
///
/// ⚠ 另有一跳**本来就不归本条**：id 是怎么从 `configDir` 推出来的
/// （`history::apikey_account_id_of_dir`，住 monitor，backend 够不着）——
/// 那一格由 `file_face_tests::us1_what_the_write_side_wrote_is_exactly_the_row_the_launch_answer_uses` 钉（〔US1〕写口 → 人群 → 成品，都在后端）。
/// **本条从「已经有了一个 id」那一刻接手。**
#[cfg(unix)]
fn creds_text_the_write_side_would_produce(rows: &[(&str, &str)]) -> String {
    let mut doc = serde_json::Map::new();
    for (id, key) in rows {
        // 一行一次，正是界面上「保存」按一次的那一步（同一个函数、同一个顺序）。
        doc = creds_core::store::merge_account_key(&doc, id, &SecretKey::new(*key));
    }
    creds_core::store::to_pretty_json(&doc)
}

/// ★★★ `KH2C2`。判定**不是**「那份文件里有那一行」——
/// 是「假上游真的收到了那一发，且它带的 `Authorization` 是**那个账号那一行**的 key」。
///
/// # 这一趟真实到什么程度（逐段说清，别读宽）
///
/// | 段 | 真的假的 |
/// |---|---|
/// | 那份凭据文件 | **写侧那两步真的算出来的**（`merge_account_key` + `to_pretty_json`），不是手写 JSON |
/// | 起会话那条命令串 | **真的 shell**（`bash -c '<env 前缀><launcher>'`），与 `KH2B1` 同一套 |
/// | env | **真的**（子进程自己从环境里读） |
/// | 中转 | **真子进程**（`spawn_relay_child_with_creds`：真 `Command::new(exe)` · 端口 0 从 stderr 读回） |
/// | 上游 | **真的** TCP 假上游 |
/// | agent | **桩**（红线：绝不起真 claude） |
///
/// ⚠ 红线的例外口径逐字〔PM 09-02〕：**由测试自己拉起、跑在沙箱容器内、端口 0、
/// 用完即杀的中转子进程，不算「起真后端」**。它**不覆盖**那个会碰 tmux 的 backend ·
/// 在宿主上拉任何进程 · 手工起后端冒烟。
#[cfg(unix)]
#[test]
fn a_credentials_file_produced_by_the_write_side_routes_that_account_to_the_upstream() {
    let up = spawn_fake_upstream(None);
    let dir = tmpdir("kh2c2");
    let creds = dir.join("apikey-credentials.json");

    // ★ 两条 —— 一条量不出「拿 A 的 key 发 B 的请求」，那是本族最坏的失效形态。
    //   id 取中性名：断言里用的是 key 那个值，不是目录名（`brief` 12 那条）。
    std::fs::write(
        &creds,
        creds_text_the_write_side_would_produce(&[("row-one", "KEY-ONE"), ("row-two", "KEY-TWO")]),
    )
    .expect("写凭据夹具");
    // 采集面自检：写侧那两步**真的产出了一份能解析的、带那两行的文件**。
    // 切歪了 / 产出空的时候，下面那几条会红在「404」上而指不出原因。
    // ⚠ 这里**按结构判，不按子串判**：`needle_anchor_registry` 那条递减棘轮逐字禁
    //   「语料变量上的裸 `contains`」（本轮实测撞过一次：35 > 上限 33），
    //   而它禁的理由与这里要的东西正好同向 —— 子串在 `{"api_key":"…"}` 换成
    //   任何别的字段名时照样命中，那就不是「写侧产出的形状」了。
    let on_disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&creds).expect("读回"))
            .expect("写侧那两步产出的不是合法 JSON —— 夹具坏了，下面全是空真");
    assert_eq!(
        on_disk[creds_core::store::ACCOUNTS_FIELD]["row-one"][creds_core::store::KEY_FIELD],
        "KEY-ONE",
        "写侧那两步产出的东西里没有那一行 —— 夹具坏了，下面全是空真：{on_disk}"
    );

    let relay = spawn_relay_child_with_creds(up.addr, &creds);
    let stub = dir.join("stub-launcher.sh");
    std::fs::write(&stub, STUB_LAUNCHER).expect("写桩启动器");

    // 起两发：同一条起会话路径，**只有账号段不同**。
    for acct in ["row-one", "row-two"] {
        let url = format!(
            "http://127.0.0.1:{}/s/claude-code/{acct}",
            relay.addr.port()
        );
        let out = std::process::Command::new("bash")
            .env("HOME", &relay.home)
            .arg("-c")
            .arg(format!(
                "{}bash {}",
                rendered_export(&url),
                stub.to_string_lossy()
            ))
            .output()
            .expect("起桩启动器");
        let so = String::from_utf8_lossy(&out.stdout);
        let se = String::from_utf8_lossy(&out.stderr);
        assert!(
            so.starts_with("HTTP/1.1 200"),
            "写侧产出的那份文件里明明有 `{acct}` 这一行，这一发却没走完一条转发：\
                 {so:?} / {se:?}\n中转 stderr：{:?}",
            relay.err()
        );
    }

    // ★ 正题①：假上游**真的收到了两发**，路由键那几段被剥掉了。
    let seen = up.seen.lock().expect("lock").clone();
    assert_eq!(
        seen.len(),
        2,
        "上游没收到两发 —— 「配完之后那条链真的通」这句话在这一趟里就是假的：{seen:?}"
    );
    for line in &seen {
        assert_eq!(
            line, "POST /v1/messages HTTP/1.1 auth=true",
            "实得：{seen:?}"
        );
    }
    // ★ 正题②：**两个账号各拿各的 key**。
    let auths = up.auth_values.lock().expect("lock").clone();
    assert_eq!(
        auths,
        vec![
            "Authorization: Bearer KEY-ONE".to_string(),
            "Authorization: Bearer KEY-TWO".to_string(),
        ],
        "两发拿到的 key 不是各自那一行的 —— 「拿 A 的 key 发 B 的请求，而两边都显示成功」\n\
             正是 `KH2` 逐字点名的最坏那一形。实得：{auths:?}"
    );

    // ★ 正题③（非空对照 + `KL7` 第 2 条）：写侧**没写过**的那个账号 ⇒ 404，
    //   且一个字节不发上游。⇒ 上面那两发的 200 不是「什么都能过」。
    //   ⚠ 这一条同时钉住 `KH2C3` 在**端到端**那一面：写侧不再落 `default` 那一行
    //   ⇒ 一个没配过的账号段**不会**被那一行顶上。
    for miss in ["row-three", creds_core::store::LEGACY_ACCOUNT_ID] {
        let url = format!(
            "http://127.0.0.1:{}/s/claude-code/{miss}",
            relay.addr.port()
        );
        let out = std::process::Command::new("bash")
            .env("HOME", &relay.home)
            .arg("-c")
            .arg(format!(
                "{}bash {}",
                rendered_export(&url),
                stub.to_string_lossy()
            ))
            .output()
            .expect("起桩启动器");
        let so = String::from_utf8_lossy(&out.stdout);
        assert!(
            so.starts_with("HTTP/1.1 404"),
            "写侧没写过的账号段 `{miss}` 没有回 404：{so:?}"
        );
    }
    assert_eq!(
        up.seen.lock().expect("lock").len(),
        2,
        "查不到的那两发**漏到上游去了** —— 那是 `KL7` 第 2 条逐字禁的回落"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// ★★★ `K-H2b` `D1 阻-2`：**配完 key 不用重启中转** —— 那张表不是启动快照。
///
/// # 它治的是什么
///
/// 先前 `load_credentials` 只在 `run_with` 里跑一次，而且在**永不返回**的 `serve()` 之前
/// ⇒ 用户在界面上按下「保存 key」之后，中转进程里的上游选择手上还是启动那一刻的表 ⇒
/// 那个账号**每一发都是 404**。而 404 与「账号 id 打错」**同形**，指不向原因。
///
/// # 这一趟真到什么程度
///
/// 真子进程中转 · 真文件（裸 `fs::write`，等价于人拿编辑器改 / 界面那条 IPC 写）·
/// 真 TCP 请求 · 真上游。**中转全程没有重启**（同一个 `RelayChild`）。
#[test]
fn a_row_added_after_the_relay_started_is_picked_up_without_a_restart() {
    let up = spawn_fake_upstream(None);
    let dir = tmpdir("reload");
    let creds = dir.join("apikey-credentials.json");
    // 起手只有一条空账号（等价于 `spawn_relay_child` 那份夹具）。
    std::fs::write(&creds, b"{\n  \"accounts\": {\n    \"acctA\": {}\n  }\n}\n")
        .expect("写起手的凭据夹具");
    let relay = spawn_relay_child_with_creds(up.addr, &creds);

    // ① 起手：那个还没配的账号 **404**（非空对照排最前 —— 证明这把尺子分得出两种结局）。
    let mut c = send_request(relay.addr, "/s/claude-code/acctLate/v1/messages", "");
    let mut got = String::new();
    c.read_to_string(&mut got).expect("read");
    assert!(
        got.starts_with("HTTP/1.1 404"),
        "还没配的账号应当 404，实得：{got:?}"
    );
    assert_eq!(
        up.seen.lock().expect("lock").len(),
        0,
        "404 那一发漏到上游去了 —— 那是 `KL7` 第 2 条逐字禁的回落"
    );

    // ② **中转不重启**，只把那份文件改掉（多一条带 key 的行）。
    std::fs::write(
        &creds,
        b"{\n  \"accounts\": {\n    \"acctA\": {},\n    \"acctLate\": { \"api_key\": \"KEY-LATE\" }\n  }\n}\n",
    )
    .expect("重写凭据夹具");

    // ③ 同一个中转进程、同一条路：这一发必须**走通**，且带的是新那一行的 key。
    let mut c2 = send_request(relay.addr, "/s/claude-code/acctLate/v1/messages", "");
    let mut got2 = String::new();
    c2.read_to_string(&mut got2).expect("read");
    assert!(
        got2.starts_with("HTTP/1.1 200"),
        "配完之后仍然不认这一行 —— 那张表还是启动快照（用户得重启中转才生效，\n             而不重启的症状是一个静默的 404）。实得：{got2:?}"
    );
    let auths = up.auth_values.lock().expect("lock").clone();
    assert_eq!(
        auths,
        vec!["Authorization: Bearer KEY-LATE".to_string()],
        "重载之后换上的不是新那一行的 key：{auths:?}"
    );
    // 非空对照：中转**确实没重启**（同一个子进程，stderr 上只有一句 `listening on`）。
    assert_eq!(
        relay.err().matches("listening on").count(),
        1,
        "中转重启过 —— 那这一条量的就不是「不重启也生效」"
    );
}

/// ★★★ `D2 阻-2`：**重载时解析失败，不许把表换成空。**
///
/// 空表的行为是**全部 404** ⇒ 用户手编那份 JSON 少一个逗号，症状就是
/// 「我明明配好了、刚才还能用，现在每一发都 404」。
/// ⚠ **这一形是「表可重载」之后新长出来的** —— 表是启动快照时，坏文件只影响下一次启动。
#[test]
fn a_broken_credentials_file_keeps_the_last_good_table_instead_of_emptying_it() {
    let up = spawn_fake_upstream(None);
    let dir = tmpdir("reload-bad");
    let creds = dir.join("apikey-credentials.json");
    std::fs::write(
        &creds,
        b"{\n  \"accounts\": {\n    \"acctA\": { \"api_key\": \"KEY-A\" }\n  }\n}\n",
    )
    .expect("写起手的凭据夹具");
    let relay = spawn_relay_child_with_creds(up.addr, &creds);

    // 非空对照：起手这一发走得通（否则下面「仍然走得通」是空真）。
    let mut c = send_request(relay.addr, "/s/claude-code/acctA/v1/messages", "");
    let mut got = String::new();
    c.read_to_string(&mut got).expect("read");
    assert!(got.starts_with("HTTP/1.1 200"), "起手就不通：{got:?}");

    // ★ 把文件改坏（人手编少一个逗号那一形）。
    std::fs::write(&creds, b"{ \"accounts\": { \"acctA\": { } ").expect("写坏文件");

    // 正题：**仍然走得通** —— 上一张能用的表还在。
    let mut c2 = send_request(relay.addr, "/s/claude-code/acctA/v1/messages", "");
    let mut got2 = String::new();
    c2.read_to_string(&mut got2).expect("read");
    assert!(
        got2.starts_with("HTTP/1.1 200"),
        "文件读坏了就把整张表换成空了 —— 那是**全部 404**，\n\
             而用户看到的是「刚才还能用，现在每一发都 404」。实得：{got2:?}"
    );
    // 而且**不是静默的**：那一句必须出现在中转的诊断流里。
    assert!(
        wait_until(|| relay.err().contains("保留上一张表不动")),
        "读坏了却一声不吭 —— 静默换表与静默丢一行是同一族。stderr 现在是：{:?}",
        relay.err()
    );
    // 两把 key 都还是那一行的（表没被换掉的第二个读数）。
    let auths = up.auth_values.lock().expect("lock").clone();
    assert_eq!(
        auths,
        vec![
            "Authorization: Bearer KEY-A".to_string(),
            "Authorization: Bearer KEY-A".to_string()
        ],
        "实得：{auths:?}"
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

/// 🔴〔`P25` 2026-09-22〕**那个「没人听」的端口，抢不走。**
///
/// # 它钉的是一条 TOCTOU 的结构性消除
///
/// 上一版 `a_port_nobody_listens_on` 是 `bind(0)` 拿号再 `drop` 放掉 ——
/// 放掉的那一刻那个号就回到临时端口池，而 `cargo test` 并行跑、同一个进程里
/// 别的判据（假上游 · 中转子进程）全在 `bind(0)` ⇒ 内核可能把它分出去。
/// 那时 `acct-dead` 指着的不再是「没人听」，而是另一个真在听的进程
/// ⇒ 502 那一支变成别人回的 404。
///
/// ⇒ 修法是挑一个**低于临时端口段下界**的号：内核永远不会把它分配给 `bind(0)`。
/// 本条钉那个性质本身（不是钉某个具体端口号）。
#[test]
fn the_dead_port_is_one_the_kernel_can_never_hand_out() {
    let p = a_port_nobody_listens_on();
    assert!(
        p < EPHEMERAL_FLOOR,
        "挑到的号 {p} 落在临时端口段里（下界 {EPHEMERAL_FLOOR}）—— \
         并行的 `bind(0)` 随时会抢走它"
    );
    // 真的没人听（否则这个「死端口」是活的，502 那一支量的就不是它该量的东西）。
    assert!(
        std::net::TcpStream::connect_timeout(
            &SocketAddr::new(LOOPBACK, p),
            std::time::Duration::from_millis(100),
        )
        .is_err(),
        "{p} 上有人在听 —— 它不是一个「连不上」的上游"
    );

    // 🔴 本机那个下界**真的**不比我们这个常量低（低了的话上面那一比是假的安全感）。
    // ⚠ 只在读得到的时候判（`/proc` 是 Linux 专有）；读不到就把这一格如实跳过**并说出来**。
    match std::fs::read_to_string("/proc/sys/net/ipv4/ip_local_port_range") {
        Ok(raw) => {
            let low: u16 = raw
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| panic!("读不懂临时端口段：{raw:?}"));
            assert!(
                low >= EPHEMERAL_FLOOR,
                "本机临时端口段下界是 {low}，比我们那个常量 {EPHEMERAL_FLOOR} **还低** \
                 ⇒ 上面那一比给的是假的安全感。现打原文：{raw:?}"
            );
        }
        Err(e) => {
            // 不是「跳过」—— 把判不了这件事印出来（本仓那条：跳过与过了长得一样）。
            println!(
                "〔读数〕读不到 /proc/sys/net/ipv4/ip_local_port_range（{e}）\
                 ⇒ 「本机下界不低于 {EPHEMERAL_FLOOR}」这一格本趟**判不了**"
            );
        }
    }
}

// ============================================================ `设计/20 §3.1a`：中转自己的传输失败回 504

/// 一个**读完请求、回一串给定字节、然后按 `hold` 决定关不关**的假上游。
///
/// `hold = true`：回完之后**攥着连接不放**（一个字节都不再写），给「等响应超时」那一格用。
/// 只接一条连接 —— 每条判据自己起一个。
fn spawn_replying_upstream(reply: &'static [u8], hold: bool) -> SocketAddr {
    let listener = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        let Some(Ok(mut s)) = listener.incoming().next() else {
            return;
        };
        let mut r = BufReader::new(s.try_clone().expect("clone"));
        let mut clen = 0usize;
        loop {
            let mut h = String::new();
            let n = r.read_line(&mut h).unwrap_or(0);
            if n == 0 || h == "\r\n" {
                break;
            }
            if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                clen = v.trim().parse().unwrap_or(0);
            }
        }
        // 把请求体读干净 ⇒ 关的时候发 FIN 不是 RST（理由同 `spawn_fake_upstream`）。
        let mut body = vec![0u8; clen];
        let _ = r.read_exact(&mut body);
        let _ = s.write_all(reply);
        let _ = s.flush();
        if hold {
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
    });
    addr
}

/// 起一个中转，上游期限**由判据给**（「等响应超时」那一格要一个短期限，不能等 10 分钟）。
fn spawn_relay_with_upstream_deadline(
    up: SocketAddr,
    upstream_deadline: std::time::Duration,
) -> SocketAddr {
    let base = Base::parse(&format!("http://127.0.0.1:{}", up.port())).expect("base");
    let relay = Arc::new(Relay::new(
        dest_of(two_accounts_no_key(&base)),
        door::Key::for_tests(),
        TeeSink::new(Box::new(std::io::sink())),
        DOWNSTREAM_DEADLINE,
        upstream_deadline,
    ));
    let listener = listen(0).expect("listen");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || serve(listener, relay, Default::default()));
    addr
}

/// ★★★ `设计/20 §3.1a`：**中转自己的传输失败回 504，body 里一句话说清上游是谁、卡在哪一跳。**
///
/// # 分母：五跳，逐跳一格
///
/// | 跳 | 怎么造 | 先前下游拿到什么 |
/// |---|---|---|
/// | 建立连接 | 上游指着一个没人听的端口 | 502 |
/// | 没回应就断开 | 上游读完请求一个字节不回就关 | 502 |
/// | 等响应超时 | 上游读完请求攥着不回，中转的上游期限设 300ms | **什么都没有**（`?` 往上抛，静默 FIN）|
/// | 回的不是 HTTP | 上游回一行垃圾 | 502 |
/// | 只有中间响应 | 上游回 9 条 `100 Continue`（上限 8） | 502（那一格另有一条专门的判据） |
///
/// # 期望值是**手写字面量**
///
/// 状态行与那句话都是本条手写的，不是拿 `server::FailedAt::words` 算的 —— 拿被测函数算期望值，
/// 改了文案本条会跟着漂、永远绿。
///
/// # 与上游选择那两个码**不同**（`D7` 可区分性）
///
/// 同一个中转、同一张表：表里没这一行 ⇒ 404；上游连不上 ⇒ 504。两个码不同，本条末尾顺带断一次。
#[test]
fn relay_transport_failures_answer_504_saying_who_and_where() {
    const STATUS_LINE: &str = "HTTP/1.1 504 Gateway Timeout\r\n";

    // ① 建立连接：没人听的端口。
    let dead = a_port_nobody_listens_on();
    let (relay, _r, _t) = spawn_relay(SocketAddr::new(LOOPBACK, dead));
    let (got, _) = send_raw(
        relay,
        "GET /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    );
    assert!(got.starts_with(STATUS_LINE), "① 连不上该回 504：{got:?}");
    let want = format!("上游 127.0.0.1:{dead} 连不上。卡在建立连接这一步。");
    assert!(
        got.contains(&want),
        "① 没说清是谁、卡在哪：want {want:?} got {got:?}"
    );
    // ★ 可区分性（`D7`）：**同一个中转**上，表里没有的那一行回的是 404，不是 504。
    let (miss, _) = send_raw(
        relay,
        "GET /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/nosuch/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    );
    assert!(
        miss.starts_with("HTTP/1.1 404 Not Found\r\n"),
        "同一个中转上「表里没这一行」该回 404：{miss:?}"
    );

    // ② 没回应就断开。
    let up = spawn_replying_upstream(b"", false);
    let (relay, _r, _t) = spawn_relay(up);
    let (got, _) = send_raw(
        relay,
        "GET /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    );
    assert!(
        got.starts_with(STATUS_LINE),
        "② 上游不回就关该回 504：{got:?}"
    );
    let want = format!(
        "上游 127.0.0.1:{} 没回应就断开了。卡在等响应这一步。",
        up.port()
    );
    assert!(got.contains(&want), "② want {want:?} got {got:?}");

    // ③ 等响应超时：先前这一格**一个 HTTP 字节都不回**。
    let up = spawn_replying_upstream(b"", true);
    let relay = spawn_relay_with_upstream_deadline(up, std::time::Duration::from_millis(300));
    let (got, _) = send_raw(
        relay,
        "GET /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    );
    assert!(
        got.starts_with(STATUS_LINE),
        "③ 等响应超时该回 504（先前是静默 FIN）：{got:?}"
    );
    let want = format!("上游 127.0.0.1:{} 没有回应。卡在等响应这一步。", up.port());
    assert!(got.contains(&want), "③ want {want:?} got {got:?}");

    // ④ 回的不是 HTTP。
    let up = spawn_replying_upstream(b"garbage\r\n\r\n", false);
    let (relay, _r, _t) = spawn_relay(up);
    let (got, _) = send_raw(
        relay,
        "GET /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    );
    assert!(
        got.starts_with(STATUS_LINE),
        "④ 回的不是 HTTP 该回 504：{got:?}"
    );
    let want = format!(
        "上游 127.0.0.1:{} 回的不是 HTTP 响应。卡在读响应这一步。",
        up.port()
    );
    assert!(got.contains(&want), "④ want {want:?} got {got:?}");
    // ⑤「只有中间响应」那一跳由 `too_many_interim_responses_are_refused_with_504` 量。
}

/// 本 crate 生产段里**每一个** HTTP 状态码字面量的住址〔`设计/20 §3.1a` ②，`D2`〕。
///
/// `(相对 src/backend 的文件, 字面量, 属于哪一组)`。**手写**，与盘上现扫出来的两向相等。
/// 同一个字面量出现两行 = 两个家（今天只有 404：中转的「路径不是路由形状」与上游选择的
/// 「表里没这一行」同属「路由不成立」一组，下游读到的字节逐字节相同 —— `wire_golden` ③④）。
const STATUS_HOMES: &[(&str, &str, StatusGroup)] = &[
    (
        "relay/server.rs",
        "400 Bad Request",
        StatusGroup::Unreadable,
    ),
    (
        "relay/server.rs",
        "411 Length Required",
        StatusGroup::Unreadable,
    ),
    (
        "relay/server.rs",
        "413 Payload Too Large",
        StatusGroup::Unreadable,
    ),
    ("relay/server.rs", "404 Not Found", StatusGroup::NoRoute),
    (
        "accounts/upstream/mod.rs",
        "404 Not Found",
        StatusGroup::NoRoute,
    ),
    (
        "accounts/upstream/mod.rs",
        "502 Bad Gateway",
        StatusGroup::NoRoute,
    ),
    (
        "relay/server.rs",
        "503 Service Unavailable",
        StatusGroup::Busy,
    ),
    (
        "relay/server.rs",
        "504 Gateway Timeout",
        StatusGroup::UpstreamFailed,
    ),
    // 〔RK1〕门拒绝那两个码：它们住门那一份文件（`door.rs::FORBIDDEN` / `MISDIRECTED`）。
    ("relay/door.rs", "403 Forbidden", StatusGroup::Door),
    (
        "relay/door.rs",
        "421 Misdirected Request",
        StatusGroup::Door,
    ),
];

/// 我们自己造的码分几组（`20 §3.1a` 那张表 ＋ 下游请求读不懂那一组）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum StatusGroup {
    /// 下游的请求读不懂（400 / 411 / 413）。`20 §3.1a` 没列它：那是**下游**的错，与三组都不相干。
    Unreadable,
    /// 路由不成立（上游选择 `Refuse` ＋ 中转的「路径不是路由形状」）。
    NoRoute,
    /// 在飞上界 —— 我们这侧现在吃不下。
    Busy,
    /// 中转自己的传输失败 —— 上游那侧。
    UpstreamFailed,
    /// 〔RK1〕门拒绝（没钥匙 / 错钥匙 / 带 Origin ⇒ 403 · Host 非回环 ⇒ 421）。**与 404 不相交** ——
    /// 「钥匙不对」与「钥匙对、表里没这一行」必须可分（`INVARIANTS §48.1a`）。
    Door,
}

/// 一段源码里所有形如 `"NNN Xxx…"` 的字符串字面量（HTTP 状态行的码 ＋ 原因短语）。
fn status_literals(src: &str) -> Vec<String> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 6 <= b.len() {
        if b[i] == b'"'
            && b[i + 1].is_ascii_digit()
            && b[i + 2].is_ascii_digit()
            && b[i + 3].is_ascii_digit()
            && b[i + 4] == b' '
            && b[i + 5].is_ascii_uppercase()
        {
            if let Some(end) = src[i + 1..].find('"') {
                out.push(src[i + 1..i + 1 + end].to_string());
                i += end + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// ★★★ `设计/20 §3.1a` 那两条可机检的形状：
/// ① 我们自己造的三组码（上游选择 `Refuse` · 在飞上界 · 中转传输失败）**两两不相交**；
/// ② 每个码**只有一处常量**，不散在各个返回点上。
///
/// # 量法（两向相等，不是地板）
///
/// - 盘上：扫 `src/backend` 整棵树的**生产段**，抠出每一个 `"NNN Xxx"` 字面量，记 `(文件, 字面量)`；
/// - 登记：[`STATUS_HOMES`]（手写）；
/// - 两边做**多重集相等**。散到返回点上的第二处（比如谁又在某个返回点写一遍 `"502 Bad Gateway"`）
///   会让盘上多一行 ⇒ 红。
///
/// 然后在登记表上断：三组的码**恰好**是 `20 §3.1a` 那张表的值（相等，手写字面量），且两两不相交。
#[test]
fn every_status_we_make_has_one_home_and_the_three_groups_are_disjoint() {
    // 尺子自检（反空真）：认得该认的、不认不该认的。
    assert_eq!(
        status_literals(
            r#"a("404 Not Found"); b = "504 Gateway Timeout"; c = "x 404 Not"; "12 Ab""#
        ),
        vec![
            "404 Not Found".to_string(),
            "504 Gateway Timeout".to_string()
        ]
    );

    let root = crate::guard_support::src_root();
    let files = guard_core::scan_tree_excluding(&root, &["rs"], &[]);
    assert!(files.len() >= 30, "只扫到 {} 份 —— 取法坏了", files.len());
    let mut on_disk: Vec<(String, String)> = Vec::new();
    for (p, raw) in &files {
        let rel = p
            .strip_prefix(&root)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/");
        for lit in status_literals(&crate::guard_support::production_code(raw)) {
            on_disk.push((rel.clone(), lit));
        }
    }
    on_disk.sort();
    let mut registered: Vec<(String, String)> = STATUS_HOMES
        .iter()
        .map(|(f, l, _)| ((*f).to_string(), (*l).to_string()))
        .collect();
    registered.sort();
    assert_eq!(
        on_disk, registered,
        "盘上的状态码字面量与 `STATUS_HOMES` 对不上（多重集相等）。\n\
         盘上多一行 = 有人把一个码**散到了返回点上**（`20 §3.1a` ②：只许有一处常量）；\n\
         登记多一行 = 表腐了。"
    );

    // 三组的码 —— 期望值手写（`20 §3.1a` 那张表）。
    let code = |l: &str| l[..3].parse::<u16>().expect("三位数");
    let codes_of = |g: StatusGroup| -> std::collections::BTreeSet<u16> {
        STATUS_HOMES
            .iter()
            .filter(|(_, _, x)| *x == g)
            .map(|(_, l, _)| code(l))
            .collect()
    };
    let set = |v: &[u16]| {
        v.iter()
            .copied()
            .collect::<std::collections::BTreeSet<u16>>()
    };
    assert_eq!(
        codes_of(StatusGroup::NoRoute),
        set(&[404, 502]),
        "路由不成立那一组"
    );
    assert_eq!(codes_of(StatusGroup::Busy), set(&[503]), "在飞上界那一组");
    assert_eq!(
        codes_of(StatusGroup::UpstreamFailed),
        set(&[504]),
        "中转传输失败那一组"
    );
    assert_eq!(
        codes_of(StatusGroup::Unreadable),
        set(&[400, 411, 413]),
        "请求读不懂那一组"
    );
    assert_eq!(
        codes_of(StatusGroup::Door),
        set(&[403, 421]),
        "〔RK1〕门拒绝那一组"
    );
    // ① 两两不相交（`D7`：同码 ⇒ 分不清是我们配错了还是上游挂了）。
    let groups = [
        StatusGroup::Unreadable,
        StatusGroup::NoRoute,
        StatusGroup::Busy,
        StatusGroup::UpstreamFailed,
        StatusGroup::Door,
    ];
    for (i, a) in groups.iter().enumerate() {
        for b in &groups[i + 1..] {
            let both: Vec<u16> = codes_of(*a).intersection(&codes_of(*b)).copied().collect();
            assert!(
                both.is_empty(),
                "{a:?} 与 {b:?} 共用了码 {both:?} —— 可区分性没了"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 〔RK1 · `INVARIANTS §48.1a` 中转口的钥匙〕门：走真 socket 的那一半
//   设计住 `调研/第四波记录/RK1.md §1.3`；纯判定那一半住 `door_tests.rs`。
// ─────────────────────────────────────────────────────────────────────────────

/// 一发带自定头的请求（`Host` 由调用方给，**不**替它补）。
fn rk1_send(addr: SocketAddr, target: &str, headers: &str) -> String {
    let body = "{}";
    send_raw(
        addr,
        &format!(
            "POST {target} HTTP/1.1\r\n{headers}Content-Length: {}\r\n\r\n{body}",
            body.len()
        ),
    )
    .0
}

/// ① 没钥匙 / 错钥匙 / 钥匙前缀 / 多一截 / 大小写 / 空段 ⇒ **403**（体里说「relay key」），`/s/` `/t/` 都拒，
///    **一个字节都不到上游**；钥匙对、表里没这一行 ⇒ **404** —— 两者可分。
///    ② 钥匙对、表里有这一行 ⇒ 真转发（200），上游收到的请求行**不含**钥匙。
#[test]
fn rk1_the_door_refuses_without_the_key_and_that_is_not_a_404() {
    let up = spawn_fake_upstream(None);
    let (relay_addr, _relay, _tee) = spawn_relay(up.addr);
    let k = door::door_tests::TEST_KEY;
    let wrong = format!("{}0", &k[..k.len() - 1]);
    let host = "Host: 127.0.0.1\r\n";
    for target in [
        "/s/agentA/acctA/v1/messages".to_string(),
        "/t/agentA/acctA/v1/messages".to_string(),
        format!("/{wrong}/s/agentA/acctA/v1/messages"),
        format!("/{}/s/agentA/acctA/v1/messages", &k[..32]),
        format!("/{k}0/s/agentA/acctA/v1/messages"),
        format!("/{}/s/agentA/acctA/v1/messages", k.to_ascii_uppercase()),
        "//s/agentA/acctA/v1/messages".to_string(),
        format!("/{k}"),
    ] {
        let got = rk1_send(relay_addr, &target, host);
        assert!(
            got.starts_with("HTTP/1.1 403 Forbidden\r\n") && got.contains("relay key"),
            "{target:?} 应被门以 403「relay key」拒，实得 {got:?}"
        );
    }
    assert_eq!(
        up.seen.lock().expect("lock").len(),
        0,
        "门拒掉的那几发漏到上游去了"
    );
    // 钥匙对、表里没这一行 ⇒ 404（不是 403）—— 可分。
    let miss = rk1_send(
        relay_addr,
        &format!("/{k}/s/agentA/nosuch/v1/messages"),
        host,
    );
    assert!(
        miss.starts_with("HTTP/1.1 404"),
        "钥匙对、表里无行 ⇒ 404，实得 {miss:?}"
    );
    assert!(
        !miss.contains("relay key"),
        "404 那一格不该说钥匙：{miss:?}"
    );
    // ② 钥匙对、表里有行 ⇒ 真转发，上游看不到钥匙。
    let ok = rk1_send(
        relay_addr,
        &format!("/{k}/s/agentA/acctA/v1/messages"),
        host,
    );
    assert!(
        ok.starts_with("HTTP/1.1 200"),
        "钥匙对 ⇒ 真转发，实得 {ok:?}"
    );
    let seen = up.seen.lock().expect("lock").clone();
    assert_eq!(
        seen,
        vec!["POST /v1/messages HTTP/1.1 auth=false".to_string()],
        "上游恰好收到一发、路径剥干净（钥匙与路由键都不上游）"
    );
}

/// ③ 带 `Origin` ⇒ 403（体里说 Origin）；`Host` 非回环 / 缺 ⇒ 421；claude CLI 现打的那一形头（`RK1.md §5.1`）⇒ 放行。
///    拒掉的一发都不到上游。
#[test]
fn rk1_browser_and_rebinding_requests_are_refused_but_the_cli_shape_passes() {
    let up = spawn_fake_upstream(None);
    let (relay_addr, _relay, _tee) = spawn_relay(up.addr);
    let target = format!("/{}/s/agentA/acctA/v1/messages", door::door_tests::TEST_KEY);
    let origin = rk1_send(
        relay_addr,
        &target,
        "Host: 127.0.0.1\r\nOrigin: https://evil.example\r\n",
    );
    assert!(
        origin.starts_with("HTTP/1.1 403 Forbidden\r\n") && origin.contains("Origin"),
        "带 Origin ⇒ 403，实得 {origin:?}"
    );
    for h in [
        "Host: evil.example\r\n",
        "Host: evil.example:8788\r\n",
        "Host: 127.0.0.1.evil.example\r\n",
        "",
    ] {
        let got = rk1_send(relay_addr, &target, h);
        assert!(
            got.starts_with("HTTP/1.1 421 Misdirected Request\r\n"),
            "{h:?} ⇒ 421，实得 {got:?}"
        );
    }
    assert_eq!(up.seen.lock().expect("lock").len(), 0, "拒掉的漏到上游了");
    // claude CLI 2.1.282 真发的那一形（头名集合现打；值是夹具）：Host 带口、无 Origin、带 SDK 那一串头。
    let cli = rk1_send(
        relay_addr,
        &format!("{target}?beta=true"),
        &format!(
            "Host: 127.0.0.1:{}\r\naccept: application/json\r\nanthropic-version: 2023-06-01\r\n\
             anthropic-dangerous-direct-browser-access: true\r\ncontent-type: application/json\r\n\
             user-agent: claude-cli/0 (external, cli)\r\nx-app: cli\r\nx-stainless-lang: js\r\n",
            relay_addr.port()
        ),
    );
    assert!(
        cli.starts_with("HTTP/1.1 200"),
        "CLI 那一形被门拒了：{cli:?}"
    );
    assert_eq!(
        up.seen.lock().expect("lock").len(),
        1,
        "CLI 那一发恰好到上游一次"
    );
}

/// ④ 泄露判据：**真子进程**（`--relay` 那一臂，tee 落 stdout）从**空家目录**起、自己铸一把钥匙；
///    判据事后从钥匙文件读到那把值，带着它（与一把错的）打几发，然后逐面全文搜那个值 ——
///    子进程 stdout（tee）· stderr（日志）· `/proc/<pid>/cmdline` · `/proc/<pid>/environ` · 上游收到的全部字节：**零命中**。
///    正控：钥匙文件里恰好 1 次；判据自己发出去的那一发请求里恰好 1 次；各采集面都是活的。
#[cfg(target_os = "linux")]
#[test]
fn rk1_the_minted_key_never_shows_up_in_logs_tee_argv_env_or_upstream() {
    // 抓全部字节的假上游：收一条、回一段带 SSE 事件的 200、关。
    let raw_seen = Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let l = TcpListener::bind("127.0.0.1:0").expect("假上游");
    let up = l.local_addr().expect("地址");
    let raw_c = Arc::clone(&raw_seen);
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(10)));
            let mut buf = Vec::new();
            let mut one = [0u8; 1];
            while !buf.ends_with(b"\r\n\r\n") {
                match s.read(&mut one) {
                    Ok(1) => buf.push(one[0]),
                    _ => break,
                }
            }
            let mut body = [0u8; 2];
            let _ = s.read_exact(&mut body);
            buf.extend_from_slice(&body);
            raw_c.lock().expect("lock").extend_from_slice(&buf);
            let sse = "data: {\"type\":\"ping\"}\n\n";
            let _ = s.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{sse}",
                    sse.len()
                )
                .as_bytes(),
            );
        }
    });
    let dir = tmpdir("rk1-leak");
    let creds = dir.join("apikey-credentials.json");
    std::fs::write(&creds, b"{\n  \"accounts\": {\n    \"acctA\": {}\n  }\n}\n").expect("凭据夹具");
    let home = dir.join("home");
    std::fs::create_dir_all(&home).expect("空家目录");
    let relay = spawn_relay_child_at(up, &creds, home.clone());
    let key = std::fs::read_to_string(home.join(door::KEY_FILE_REL)).expect("子进程铸的钥匙在盘上");
    assert!(door::key_shape_ok(&key), "子进程铸出来的钥匙形状不对");
    assert_ne!(
        key,
        door::door_tests::TEST_KEY,
        "这一条要的是**现铸**的那把，不是夹具值"
    );
    let good_req = format!(
        "POST /{key}/s/claude-code/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\n\r\n{{}}"
    );
    assert_eq!(
        good_req.matches(&key).count(),
        1,
        "正控：我们发出去的那一发里恰好 1 次"
    );
    let (ok, _) = send_raw(relay.addr, &good_req);
    assert!(
        ok.starts_with("HTTP/1.1 200"),
        "带现铸的钥匙 ⇒ 真转发：{ok:?}"
    );
    // 〔09-25 修偶发红（HX2 报）〕原先恒把末位换成 `0`：真钥匙末位恰是 `0` 时两者相等，按构造 1/16 红。改成「末位换成一个必不同的十六进制字符」。
    let last = key.as_bytes()[key.len() - 1];
    let wrong = format!(
        "{}{}",
        &key[..key.len() - 1],
        if last == b'0' { '1' } else { '0' }
    );
    assert_ne!(wrong, key, "错钥匙必须与真钥匙不同");
    let (bad, _) = send_raw(
        relay.addr,
        &format!("POST /{wrong}/s/claude-code/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\n\r\n{{}}"),
    );
    assert!(bad.starts_with("HTTP/1.1 403"), "错钥匙 ⇒ 403：{bad:?}");
    // 等子进程把「门拒了一条」那句日志写出来（异步采集；它是本条 stderr 的活性正控）。
    let mut waited = 0;
    while !relay.err().contains("refused at the door") && waited < 100 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        waited += 1;
    }
    let pid = relay.child.id();
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).expect("读 cmdline");
    let environ = std::fs::read(format!("/proc/{pid}/environ")).expect("读 environ");
    let (out, err) = (relay.out(), relay.err());
    let upstream = String::from_utf8_lossy(&raw_seen.lock().expect("lock")).to_string();
    // 正控：每一面都是活的。
    assert_eq!(
        std::fs::read_to_string(home.join(door::KEY_FILE_REL))
            .expect("读")
            .matches(&key)
            .count(),
        1
    );
    assert!(
        err.contains("listening on") && err.contains("refused at the door"),
        "stderr 采集面是死的：{err:?}"
    );
    assert!(
        out.contains("__meta__"),
        "stdout（tee 落点）采集面是死的：{out:?}"
    );
    assert!(
        String::from_utf8_lossy(&cmdline).contains("relay_child_process_entry_point"),
        "cmdline 读错进程了"
    );
    assert!(
        String::from_utf8_lossy(&environ).contains("CCM_RELAY_PORT"),
        "environ 读错进程了"
    );
    assert!(
        upstream.contains("/v1/messages"),
        "上游那一面是死的：{upstream:?}"
    );
    // 零命中。
    for (face, text) in [
        ("stdout（tee）", out.clone()),
        ("stderr（日志）", err.clone()),
        ("cmdline", String::from_utf8_lossy(&cmdline).to_string()),
        ("environ", String::from_utf8_lossy(&environ).to_string()),
        ("上游收到的字节", upstream.clone()),
    ] {
        assert_eq!(text.matches(&key).count(), 0, "钥匙出现在 {face} 里");
        assert_eq!(
            text.matches(&wrong).count(),
            0,
            "那把错钥匙出现在 {face} 里（路径被印出去了）"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
