//! 中转本体：**一个进程**、只听回环、按路径前缀分流、逐块透传、同时 tee。

use super::creds;
use super::http1::{self, BodyView, RequestHead};
use super::route;
use super::table::{self, RoutingTable, Row};
use super::tee::{SseSplitter, TeeSink};
use super::upstream::Base;
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
/// **登记住址** `src/bridge/src/byte_cap_registry.rs`（那张表默认拒绝：不登记就红）。
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
/// 抽成函数是因为它有**两个调用点**，而它们必须用同一个数。
/// ⚠ 两个调用点**买的东西不一样，别当成一件事**：
///
/// ㈠ `handle()` 开头 —— **有牙的那个**。D3 §2.3 逐字点名的住址（「`handle()` 只做
///    `set_nodelay`，一个 `set_read_timeout` / `set_write_timeout` 都没有」），
///    也是转发路径真正阻塞的地方。删掉它，`both_peers_really_carry_…` 当场红（本轮 `MU1`）。
///    而且 `handle()` 有一个**不经过 `serve()`** 的调用者（判据直接调它）
///    ⇒ 这句承诺必须由 `handle()` 自己兑现，不能挂在调用者身上。
///
/// ㈡ `serve()` 刚 `accept` 出来那一刻 —— **纵深，没有牙，我说不出它失效会怎样**。
///    ⚙ 照实写：我找过「拒绝路径（503）会阻塞」的形状，**没构造出来** ——
///    那一支只写 ~90 字节，而一条刚握完手的连接**必然**有这么多接收窗口
///    （内核对 `SO_RCVBUF` 有下限，且对端已经 ACK 过握手）；随后的排字节那一步
///    `respond_and_drain` 自己就是**非阻塞**的（它的头注写着为什么）。
///    ⇒ 它买的**不是**一个实测过的挂死，而是一条更好守的不变式：
///    **每一条 `accept` 出来的 socket 从第一刻起就带着期限，不管它接下来走哪个分支**
///    —— 明天有人往拒绝路径上加一次阻塞读写时，这条不变式已经在那儿了。
///    ⚙ **本轮 `MU6` 实测：把这一块整个删掉，410 条判据零红。** 这个格子没有牙，别报成有。
fn apply_downstream_deadline(s: &TcpStream) -> std::io::Result<()> {
    s.set_read_timeout(Some(DOWNSTREAM_DEADLINE))?;
    s.set_write_timeout(Some(DOWNSTREAM_DEADLINE))
}

/// 上游**中间响应**（1xx）最多容忍几条〔回修轮之五 08-25，D3 `重要-1(D3)`〕。
/// 超了回 502：那已经不是一个正常的上游。
const INTERIM_RESPONSES_ALLOWED: usize = 8;

/// 中转的运行期状态。**一个进程一份**，跨连接共享。
///
/// # ⚠⚠ 这里**曾经**有两个字段，`K-H2` 把它们删掉了 —— 经过记在这里
///
/// 先前是 `base: Base` + `key: Option<SecretKey>`，两个**各自独立**的进程级字段：
/// `handle` 里一处取 `relay.base` 去连、另一处取 `relay.key` 去换头，**各取各的**，
/// 而路由键**一格都不参与**这个决定（它只喂 tee）。
/// ⇒ 那在语义上就是「**回落到默认上游 + 默认 key**」，而且是当时**唯一**的行为。
/// 一把 key 时无害；多账号之后，同一条代码路径就是 `KH2` 逐字点名的最坏失效形态：
/// **拿 A 账号的 key 去发 B 账号的请求，而两边看起来都成功了。**
///
/// # ⚠⚠⚠ 订正〔`D1` 阻-1 回修，08-28〕：**上一段先前的结论说大了，两句都是假的**
///
/// 先前这里逐字写着：「⇒ **进程里没有「默认上游」这个值可以回落**，而「A 的端点配 B 的 key」
/// 也**在类型上不可表示**……**这两条都是编译器买的**。」**两句都被 `D1` 实测证伪。**
///
/// | 先前那句 | 实测 | 读数 |
/// |---|---|---|
/// | 「进程里没有默认上游这个值」 | **假** | `DEFAULT_UPSTREAM` 就是本文件 `:24` 的 crate 常量；`Base` 三个字段**全是 `pub(crate)`**、`Base::parse` 也是 ⇒ 本 crate 里**一行就能造一个 `Base`** |
/// | 「A 的端点配 B 的 key 在类型上不可表示」 | **假** | 两行同时在作用域里、A 连 B 渲染，**编译通过**（`D1-M2`；我自己复打过，跑得通）。只有一条**行为**断言会红 |
///
/// **编译器真正买到的只有很窄的一条**：**从一个 `Row` 里拿不到 `&Base` 这个值**
/// （字段私有、住 `table::mod sealed`、没有 `base()` 访问器）。
/// ⇒ 它挡住的是「**顺手**把两行拆开拼」，**挡不住**「有意去重建一个 `Base`」——
/// `D1-M1` 实测：把签名换成收 `host: &str` + key、用 `row.host_header()` 把 `Base` 重建出来去连，
/// **488 passed / 0 failed，一条都没红。**
///
/// ⇒ 「不许再有进程级的上游 / key」这条性质今天**由一条文本棘轮守**
/// （`table_guard::the_relay_carries_no_process_wide_upstream_and_no_process_wide_key`，
/// 它扫的是 `struct Relay` 那个窗口里有没有 `base:` / `key:` 两个**字面**）。
/// **那是文本判据，不是编译器。**它认不出：换个字段名（`endpoint:` / `fallback:`）·
/// 把值藏进别的结构体再放进 `Relay` · 干脆用一个 `static`。
/// ⚠ **这几条我没有逐条实测**（`D1` 实测的是上表那两形）⇒ 它们是**读源码得出的形状，不是读数**。
/// 那份凭据文件**这一刻**的印记：(mtime, 字节数)。读不到就是 `None`。
///
/// ⚠ 两样一起取是有意的，理由见 [`Relay::refresh_if_changed`] 的「它买不到什么」。
fn stamp_of(path: &std::path::Path) -> Option<(std::time::SystemTime, u64)> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// `D1 阻-2`：那张表从哪儿重读。
///
/// ⚠ **`upstream_default` 不是「可回落的默认上游」**（那条棘轮禁的东西）——
/// 它是 `table::build` 的**入参**：表里某一行**没写 `base_url`** 时那一行取它。
/// 「行不在表里」仍然是 404，一个字节都不发上游。两件事别混。
pub(crate) struct Reload {
    path: std::path::PathBuf,
    upstream_default: Base,
    /// 上次读到的 mtime。`None` = 那时读不到（文件不在 / stat 失败）。
    seen: std::sync::Mutex<Option<(std::time::SystemTime, u64)>>,
}

impl Reload {
    pub(crate) fn new(
        path: std::path::PathBuf,
        upstream_default: Base,
        seen: Option<(std::time::SystemTime, u64)>,
    ) -> Self {
        Self {
            path,
            upstream_default,
            seen: std::sync::Mutex::new(seen),
        }
    }
}

pub(crate) struct Relay {
    /// 账号段 → 上游 + key。**决定这条请求发到哪儿、用哪把 key 的唯一住址。**
    ///
    /// ⚠ 它**不进任何 `Debug`**：`SecretKey` 手写的 `Debug` 恒为遮蔽形，
    /// 而本结构体**整个没有** `derive(Debug)`（`KS1` 的第二道）。
    ///
    /// ⚠⚠ `K-H2b` `D1 阻-2`：它**从启动快照变成了可重载的**。
    /// 先前 `load_credentials` 只在 `run_with` 里跑一次、且在**永不返回**的 `serve()` 之前
    /// ⇒ 用户在界面上配完 key **必须重启**才生效，而不重启的症状是
    /// **一个静默的 404**（与「账号 id 打错」同形，指不向原因）。
    /// ⇒ 换成 `RwLock` + [`Reload`]：每条请求进来先看那份文件的 mtime 变没变，变了就重读。
    table: std::sync::RwLock<RoutingTable>,
    /// 重载源。`None` = 判据自己造的表（不从文件来）⇒ 永不重载。
    reload: Option<Reload>,
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
    /// **生产段唯一的构造入口**（`run_with` 走它）。
    ///
    /// ⚠ 先前有两个入口（`new` / `with_key`），`K-H2` 之后只剩一个：
    /// 「带不带 key」不再是**中转**的属性，而是**表里某一行**的属性。
    pub(crate) fn new(table: RoutingTable, tee: TeeSink) -> Self {
        Self {
            table: std::sync::RwLock::new(table),
            reload: None,
            tee,
            served: AtomicU64::new(0),
            inflight: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            pumps: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// `D1 阻-2`：把「从哪儿重读那张表」接上。**只有 `run_with` 那条真路走它。**
    pub(crate) fn reloading_from(mut self, r: Reload) -> Self {
        self.reload = Some(r);
        self
    }

    /// 每条请求进来先问一次：那份凭据文件动过没有？动过就重读。
    ///
    /// # 为什么按 mtime 而不是「每次都读」
    ///
    /// 「每次都读」也对，但那是**每条请求一次磁盘读 + 一次 JSON 解析**；
    /// 按 mtime 只在**真的改过**之后付一次。
    /// ⚠ **它不是定时器**：没有任何线程自己醒来，读的是「这条请求进来的这一刻」的元数据。
    ///
    /// # 它买不到什么（照实写）
    ///
    /// 印记是 **(mtime, 字节数)** 两样，不是只有 mtime —— 因为 mtime 的粒度在某些文件系统上
    /// 是秒级，**同一秒内改两次**时它可能不动，而那一形的症状是「这一发还用旧表」
    /// 并且**会留下来**（文件不再变 ⇒ 永远不再重载），不是一次抖动。
    /// ⚠ 加上字节数**只是把那个窗口收窄，没有关掉它**：同一秒内改成**同样长**的另一份内容
    /// （比如把一把 key 换成等长的另一把）仍然看不见。**这一形我没量** —— 如实登记。
    fn refresh_if_changed(&self) {
        let Some(r) = self.reload.as_ref() else {
            return;
        };
        let now = stamp_of(&r.path);
        {
            let seen = r.seen.lock().expect("lock");
            if *seen == now {
                return;
            }
        }
        let mut loaded = creds::load(&r.path);
        // ★★ `D2 阻-2`：**解析坏了就不换表。**
        //
        // `creds::load` 在「读不动 / 不是合法 JSON」时回的是 `accounts: 空 + problem: Some(_)`
        // ⇒ 照着装表就是**把整张表换成空**，而空表的行为是**全部 404**。
        // 用户那一侧看到的是「我明明配好了、刚才还能用，现在每一发都 404」——
        // 而成因是他刚才手编那份 JSON 少了一个逗号。
        // ⚠ **换表之前这一形不存在**（表是启动快照，坏文件只影响下一次启动）⇒
        //   它是**本轮改动新长出来的**，处置写在这里：**留住上一张能用的表，只出声**。
        // ⚠ 「一条都没配」与「读坏了」是两回事：前者 `problem` 是 `None`、accounts 空，
        //   那是一个**合法**状态（谁都不走中转），照换不误。
        if let Some(why) = loaded.problem.as_deref() {
            eprintln!("[relay] 凭据文件读不成表，**保留上一张表不动**（不是换成空表）：{why}");
            // 印记也**不更新** —— 下次请求进来还会再试一次，人把文件改回来就自动恢复。
            return;
        }
        let (table, rejected, notes) =
            table::build(std::mem::take(&mut loaded.accounts), &r.upstream_default);
        // 重载也要**出声**：静默换掉一张表，与静默丢掉一行是同一族。
        // ⚠ `K-R1`：`notes` 也要跟着走这一趟 —— 一次重载把某一行改成非默认行为
        //   （加了路径前缀 / 换了鉴权头形状）而**只有第一次启动才说**的话，
        //   那句话就成了「说过一次的历史」，而不是「现在盘上是这样」。
        creds::announce(
            &loaded,
            table.len(),
            &rejected,
            &notes,
            &mut std::io::stderr(),
        );
        *self.table.write().expect("lock") = table;
        *r.seen.lock().expect("lock") = now;
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
        // 期限**先装上，在分流之前** —— 这一处是**纵深**：不变式是「每条 accept 出来的
        //    socket 从第一刻起就带期限，不管它接下来走哪个分支」。
        //    ⚙ 别把它读成「治了一个实测过的挂死」：下面那条 503 支只写 ~90 字节、
        //    排字节那步又是非阻塞的，我**没构造出**它阻塞的形状（理由全文见
        //    `apply_downstream_deadline` 头注㈡；本轮 `MU6` 实测删掉它**零红**）。
        //    装不上仍然**关连接并出声**：宁可拒绝，也不放一条来路不明的进来。
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
    // ★★★ **`K-H2` `KH2` 的正主**：路由键里那一段账号在表里查不到 ⇒ **404**。
    //
    //   ⚠ 位置是承重的：**排在读请求体之前、连上游之前**（`upstream` 那一跳在几十行之下）
    //   ⇒ 查不到的时候**一个字节都不会到上游**。这比「没发 Authorization」强，也更好断。
    //
    //   ⚠⚠ 三条**不许**做的，逐条写死（`KH2` 逐字点名的最坏失效形态就在这里）：
    //     · 不许回落到别的账号的 key —— 那是**拿 A 的 key 发 B 的请求**；
    //     · 不许回落到默认上游 —— 「配错了」与「没配」会变成同一个结果；
    //     · 不许在这里「顺手补一行」。
    //   今天这三条**不是靠这条注释守的**：`Relay` 里根本没有可回落的那个值（编译器兜），
    //   而「表里有几行」是文件说了算。这条注释只解释为什么这一支必须是 404。
    //
    //   ⚠ 与它**同族但不同**的一格：行**在**表里、只是那一行没配 key ⇒
    //   **原样转发下游那份鉴权头**（订阅登录那一档是合法状态）。那一格在
    //   `render_upstream_request` 里，**两条判据分开钉，不许合成一条**。
    // `D1 阻-2`：查表**之前**先看那份文件动过没有 —— 不然「界面上配完 key」要重启才生效，
    // 而不重启的症状是一个静默的 404（与「账号 id 打错」同形）。
    relay.refresh_if_changed();
    // ★★ `D2 阻-4`：**读锁的活法是承重的，写下来。**
    //
    // 这个守卫**只活到「请求头 + 请求体已经写给上游」为止**（下面那个 `}` 就是它的尽头），
    // **不跨 `pump`**。理由：`std::sync::RwLock` 是**写优先**的 —— 一个在等的写者
    // （= 用户刚配完一把 key，下一条请求触发重载）会挡住后面所有读者；
    // 而 `pump` 是**流式转发**，一条 SSE 长流可以跑几分钟。
    // ⇒ 守卫跨 `pump` 的话，「配一次 key」会把中转堵在**最长那条在飞流**后面。
    // ⚠ **挂起时长我没实测**（那要造一条长流再去配 key）—— 这是读源码得出的形状。
    // ⇒ 现在的写法让锁只覆盖「查表 → 连上游 → 写请求」这一小段，`pump` 在守卫之外跑。
    // ★ `阻-1(D3)` + `重要-2(D3)`：请求体这一格先前有**两个**洞，两个都在这几行上。
    //   ① 长度**无上界** ⇒ `Content-Length: 1e12` 把整个进程 abort 掉（SIGABRT，不走 unwind）；
    //   ② 长度**读不懂**（`7abc`）与「没有这个头」挤在同一个 `None` 里 ⇒ 请求体被静默丢掉、
    //      上游收到空体、下游拿到一条正常的 200。中转搬的正是 `POST /v1/messages` 的载荷。
    // ⚠ 顺序：它排在**取读锁之前**（`D2 阻-4`）—— 读下游是一次可能很慢的 IO，
    //   握着表的读锁去等它，等于让「配一次 key」跟着它一起慢。
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

    // ★★ `D2 阻-4`：**读锁的活法是承重的，写下来。**
    //
    // 这个守卫只活到**这个块结束**（请求头 + 请求体已经写给上游），**不跨 `pump`**。
    // 理由：`std::sync::RwLock` 是**写优先**的 —— 一个在等的写者（= 用户刚配完一把 key，
    // 下一条请求触发重载）会挡住后面所有读者；而 `pump` 是**流式转发**，
    // 一条 SSE 长流可以跑几分钟 ⇒ 守卫跨 `pump` 的话，「配一次 key」会被堵在
    // **最长那条在飞流**后面。⚠ **挂起时长我没实测** —— 这是读源码得出的形状，不是读数。
    let mut up = {
        let table = relay.table.read().expect("lock");
        let Some(row) = table.lookup(&r.account) else {
            return respond_and_drain(&mut down_w, "404 Not Found");
        };
        // ★ 连的是**这一行自己的**上游。
        //   ⚠ 订正〔`D1` 阻-1，08-28〕：先前这里逐字写着「所以『拿 A 的端点』这件事在这里
        //   **根本写不出来**」——**那句是假的**。`D1-M2` 实测：两行同时在作用域里、
        //   `a.connect()` 配 `render_upstream_request(…, b, …)`，**编译通过、跑得通**。
        //   今天这一行之所以对，靠的是**这个作用域里只有一行**这个事实，**不是类型**。
        //   真正量它的是 `KH2`/`KH4` 那几条走真转发的行为判据。
        let mut up = match row.connect() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[relay] upstream connect failed: {e}");
                return respond_status(&mut down_w, "502 Bad Gateway");
            }
        };
        // ★★ 上游与 key **同源**：这里递的是**同一个** `row`，不是两个各自取的值。
        up.write_all(&render_upstream_request(&head, &r.rest, row, body.len()))?;
        if !body.is_empty() {
            up.write_all(&body)?;
        }
        up.flush()?;
        up
    };

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
    relay.tee.open(&r.agent, &r.account, &r.key);
    // ★ 返回值**必须落地**：它是 `DoD-2㈡`「块数对账」的唯一量点。
    // 写成 `pump(...)?;` 就等于把它丢掉 —— 那正是审计 `K4` 能全绿的原因。
    let outcome = pump(&mut up, &mut down_w, &mut |raw| {
        let decoded = view.feed(raw, TEE_DECODE_CAP);
        for payload in splitter.feed(&decoded, TEE_DECODE_CAP) {
            relay.tee.event(&r.agent, &r.account, &r.key, &payload);
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
/// # ★★ 订正〔`K-H2a` 08-27〕：**上面那 4 条里的第 1、2 条今天已经不成立了 —— 换头做了**
///
/// 先前这里逐字写着「本函数**原样转发**下游那份 `Authorization`，**不替换**」+「『换头』**本件不做**，
/// 落点是下一件」。**`K-H2a` 就是那一件，它已经落地** ⇒ 那两句留着就会变成隔壁
/// `parity_ledger` 里反复记的那一形（**改了行为没回来改理由，账本当天就开始撒谎**）。
///
/// **今天盘上的真话，逐条重写**：
/// 1. **配了 key ⇒ 换头**：下游那份 `Authorization` 被**丢掉**，换上中转自己那把。
///    取明文的那一行就在下面，它是 `expose_for_auth_header` 在**整个后端生产段里唯一**的调用点
///    （`KS2`，由 `creds_guard::the_plaintext_leaves_the_type_at_exactly_one_place_in_this_crate` 相等断言钉住）。
/// 2. **这一行没配 key ⇒ 原样转发**（`K-H1` 甲半那个形状，一个字节不动）。
///
///    ⚠⚠ **订正〔`K-H2` 08-28〕：这一条先前的理由今天是假的，已收口。**
///    先前逐字写的是「这一支刻意留着：**中转在没配凭据时仍然是一条能用的透传路**」——
///    那句话描述的是一条**隐式的全局行为**（进程级 `key` 是 `None` ⇒ 所有路由键都透传）。
///    `K-H2` 把进程级那两个字段删掉之后，「全局」这个东西**不存在了**
///    ⇒ 那条隐式行为**必然消失**，而且必须消失：它就是「查不到也照发」的另一种写法。
///
///    **它没有被删掉，是被改成了显式的一条路**：在凭据文件里写一条空账号
///    （`{"accounts": {"passthrough": {}}}`）就得到一条 keyless 的透传行。
///    ⇒ 今天准确的说法只有两句：①**没有配任何账号的中转，全部请求 404**；
///    ②**透传要显式配一条**。判据分别在
///    `store::tests::an_unconfigured_file_yields_no_rows_at_all` 与
///    `store::tests::an_account_with_nothing_filled_in_is_still_a_row`。
/// 3. 「不记录」那一半照旧由两条判据钉：`the_auth_header_is_forwarded_but_never_teed`
///    与 `a_sentinel_auth_header_shows_up_in_neither_the_relay_processs_stderr_nor_its_stdout`。
/// 4. ★★ **第 4 条那个预言兑现了，而且必须在这里点名**：那两条判据喂进去的是
///    **客户端发来的那个头**，它们证的是「**进来的**东西没被记下来」；
///    而 `K-H2a` 要保的是**换上去的那个 key**，那是**另一个值、从另一条路（那份文件）进来**。
///    ⇒ **判据守的是前门，key 从后门进。**（件计划 `§0` 逐字：这一形骗过了 PM 与一路审计。）
///    接住后门那一格的是**新加的**那条金丝雀：
///    `the_substituted_key_never_shows_up_in_any_of_the_four_exits`（`KS3`）。
///    **两族缺一都不成立**，别把老那两条读成已经覆盖了新的。
///
/// # ⚠ 射程如实写：换的是**哪一个**头
///
/// ⚠⚠ **订正〔`K-R1` 09-04〕：这一节先前那两句今天是假的，逐字重写。**
/// 先前写的是「只换 `Authorization`（`K11 裁定一` 逐字点名的就是它）」+
/// 「客户端若自带 `x-api-key` / `Proxy-Authorization` 一类，本函数**照旧原样转发**」，
/// 并把后者登记成射程外、说那是 `K-H2` 正文的活。**`K-H2` 已签收，那一格没人接。**
///
/// **今天盘上的真话**：
/// 1. **换哪个头由那一行的 `auth_style` 定**（见 [`auth_header_of`]）——
///    〔用 09-04〕逐字要「api做成通用的」⇒ 只押一种鉴权头 = 只接得上一半的上游，
///    而押错的症状是 **401**，与「key 打错了」同形。
/// 2. **这一行有自己的 key 时，客户端那份鉴权头一律不转发** ——
///    人群是 [`AUTH_HEADER_NAMES`] 那个闭集，不只 `Authorization` 那一个。
///    ⚠ 这是**行为改变**，理由两条：㈠ 我这一趟要写的那个头名可能正是客户端也带着的
///    （`x-api-key` 那一档），同名头出现两次是未定义行为 —— 那正是先前丢掉
///    `Authorization` 的理由，逐字同一条；㈡ 把客户端的凭据**连带**送给一个第三方上游，
///    是把一份不属于这一行的秘密多送出去一次。
///    ⚠ **`Proxy-Authorization` 仍然照旧转发** —— 它说的是「与代理之间」的鉴权，
///    不是与上游之间的，收掉它是另一件事。**登记为射程外，不假装覆盖了。**
/// 3. **这一行没有 key 时，一个字节都不动**（`Authorization` / `x-api-key` 全照旧转发）
///    —— 订阅登录那一档要的正是这条透传路。
///    ⚠ 例外是 `AuthStyle::NoAuth`：它逐字说的就是「一个鉴权头都不发」⇒ 客户端那份也不转发。
fn render_upstream_request(head: &RequestHead, rest: &str, row: &Row, body_len: usize) -> Vec<u8> {
    // ★★ **`K-H2`：签名从 `(&Base, Option<&SecretKey>)` 收成了一个 `&Row`。**
    //    先前那个签名让「A 的端点 + B 的 key」**写得出来** —— 调用方各取各的，
    //    没有任何东西说它俩必须同源。今天它们是同一个值的两个方法，
    //    要拼错得先有两行同时在作用域里（`handle` 里只有一行）。
    let key = row.key();
    let style = row.auth_style();
    // ★★★ **`K-R1`：请求行的目标由那一行自己算**（前缀 + 客户端的真路径）。
    //    ⚠ 参数名从 `target` 改成 `rest` 是有意的：进来的是**下游那一段**，
    //      发出去的目标是**算出来的**。留着旧名字会让下一个人以为它已经是最终目标。
    //    ⚠ 拼接刻意**不在这里做** —— `&Base` 那个值不出 `Row` 的边界（见 `table.rs` 头注），
    //      而且「忘了拼前缀」这件事在这个签名上写不出来（`rest` 只有一条去处）。
    let target = row.upstream_target(rest);
    let mut out = format!("{} {} HTTP/1.1\r\n", head.method, target);
    out.push_str(&format!("Host: {}\r\n", row.host_header()));
    out.push_str("Accept-Encoding: identity\r\n");
    out.push_str("Connection: close\r\n");
    // ★ 这一趟要不要把客户端自带的鉴权头收掉：**我自己要发一个** 或 **这一行声明不发任何头**。
    //   ⚠ 两个条件都要，缺一格就漏一形：只看前者的话 `NoAuth` 那一行会把客户端的
    //     真 key 原样送给一个声明了不校验凭据的本地端点。
    let drop_client_auth = (key.is_some() && auth_header_of(style).is_some())
        || style == creds_core::store::AuthStyle::NoAuth;
    for (k, v) in &head.headers {
        if http1::is_hop_by_hop(k)
            || k.eq_ignore_ascii_case("host")
            || k.eq_ignore_ascii_case("accept-encoding")
            || k.eq_ignore_ascii_case("content-length")
        {
            continue;
        }
        // ★ 换头那一支：把下游那几份鉴权头**整条丢掉**。
        //   丢在这里而不是在下面覆盖，是因为 HTTP 允许同名头出现多次 ——
        //   「追加一条」会让上游看见**两个**同名鉴权头，那是未定义行为。
        if drop_client_auth && AUTH_HEADER_NAMES.iter().any(|n| k.eq_ignore_ascii_case(n)) {
            continue;
        }
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    // ★★ **这是整个后端生产段里唯一一处把明文取出来的地方**（`KS2`）。
    //    它就在「往上游请求写鉴权头」这一行上，与 `KS2` 的字面逐字对应。
    //    ⚠ 加第二处是**放宽**：必须先在件计划里说清那一处是什么，
    //      不许在实现里顺手把 `creds_guard` 那条相等断言改大。
    //    ⚠⚠ `K-R1` 把**头名与值前缀**变成了变量，而 `expose_for_auth_header(`
    //      这个调用点**仍然恰好一处** —— 那条相等断言一个字节都没动。
    //      〔这是有意的设计约束：一种新鉴权头形状不该换来一个新的明文出口。〕
    if let (Some(k), Some((name, prefix))) = (key, auth_header_of(style)) {
        out.push_str(&format!(
            "{name}: {prefix}{}\r\n",
            k.expose_for_auth_header()
        ));
    }
    if body_len > 0 {
        out.push_str(&format!("Content-Length: {body_len}\r\n"));
    }
    out.push_str("\r\n");
    out.into_bytes()
}

/// 中转**认得的鉴权头名**（小写）。**闭集，一个住址**〔`brief` 13b〕。
///
/// # 它是什么，不是什么
///
/// 它是「换头时要先丢掉的下游头」的人群 —— 也就是 [`auth_header_of`] **可能写出来**的
/// 那几个头名的全集。两者必须对得上，由
/// `every_header_this_relay_may_write_is_in_the_set_it_clears_first` 钉住
/// （否则会出现「我写了一个头，而同名的客户端那份没被丢掉」= 同名头出现两次）。
///
/// ⚠ **它不是「所有鉴权头」的枚举** —— 那个分母没人给得出（`Cookie` / 各家自定义 /
/// `Proxy-Authorization`…）。`creds_guard` 头注为同一件事逐字论证过为什么日志那边
/// 只能用白名单而不能用「除了 X 之外全记」。这里是**白名单方向**：
/// 只丢我可能自己写的那几个，别的原样转发。
const AUTH_HEADER_NAMES: &[&str] = &["authorization", "x-api-key"];

/// 一种鉴权头形状 → `(头名, 值前缀)`；`None` = **不发鉴权头**。
///
/// # ★ 这是 `AuthStyle` → HTTP 的映射，而它刻意住在这一侧
///
/// `creds-core` 那一侧**刻意不认识 HTTP**（`table.rs` 头注逐字：「本 crate 刻意不认识
/// HTTP」）⇒ 头名与前缀不许写在那边。那边只管**格式**（文件里那个词是什么），
/// 这边只管**协议**（那个词对应哪个头）。
///
/// ⚠ **诚实边界（自查订正，09-04）**：本节初稿逐字写「**唯一**一处映射」，而
/// **今天没有任何判据在数这个 1** —— 隔壁 `expose_for_auth_header(` 那个 1 有
/// `creds_guard` 的相等断言钉着，这一处**没有**。它靠的只是「加一个成员这里编译不过」，
/// 而那一格保证的是**这一处不漏支**，**不保证**别处不再写第二处映射。
/// ⇒ 准确的说法是「今天只有这一处，而这件事没人看着」。〔`brief` 12：给不出分母就别写全称。〕
///
/// ⚠ 穷尽 `match`：加一个成员**编译不过** —— 这一格是编译器买的，
/// 不是一条文本判据买的。〔`table.rs` 头注那张表逐条记着「读着像买断 ≠ 买断了」，
/// 这一处是真的那一档：新成员漏了这里，`cargo` 当场不过。〕
fn auth_header_of(style: creds_core::store::AuthStyle) -> Option<(&'static str, &'static str)> {
    use creds_core::store::AuthStyle;
    match style {
        AuthStyle::Bearer => Some(("Authorization", "Bearer ")),
        AuthStyle::XApiKey => Some(("x-api-key", "")),
        AuthStyle::NoAuth => None,
    }
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
    // ⚠ `K-R1`：`Base::parse` 现在带着**一句为什么**回来，而这里把它丢掉了
    //   —— 如实登记为射程外，不是漏掉：这一支的调用方（`run_with`）只印一句
    //   `[relay] bad upstream base url` 就退 2，而那条路上**还没有任何日志出口**能带这句话。
    //   真要带上，改的是 `run_with` 的报文与 `creds_guard::LOG_SITES` 那张表 ⇒ 另一拍。
    //   ★ 而**每一行**账号的 `base_url` 那句为什么，今天是真的印出去了（`table::build`）。
    let base = Base::parse(upstream_env.unwrap_or(DEFAULT_UPSTREAM)).ok()?;
    Some((port, base))
}

/// `run()` 剥掉「读环境变量」之后的那一半。
///
/// **起监听之前的处置全在这里** ⇒ 判据打得到「基址不认识就退 2」与
/// 「端口起不来就退出并出声」（`:16-17` 头注承诺的那条）两条。
/// 成功那一条尾巴上是永不返回的 `serve()` ⇒ 判据够不到，登记为 `判不了`。
/// 读一次凭据并**把该说的话说出去**，返回拿到的 key。
///
/// ★ 它为什么被抽成一个有名字的函数（同 `resolve_config` / `run_reading` 那两次的理由）：
/// `run_with` 的尾巴是**永不返回**的 `serve()` ⇒ 长在里面的东西没有任何判据够得着。
/// 这里抽出来之后，`KS9②`（只放一份文件、一次界面都不开）与 `KS11`（过宽出声）
/// 打的都是**生产段真正跑的那一份**，不是一个同构的副本。
fn load_credentials(
    get: &dyn Fn(&str) -> Option<String>,
    home: &std::path::Path,
    default_base: &Base,
    out: &mut dyn Write,
) -> (
    RoutingTable,
    std::path::PathBuf,
    Option<(std::time::SystemTime, u64)>,
) {
    let path = creds::resolve_path(get, home);
    // `D1 阻-2`：把**这一刻**那份文件的 mtime 一起记下来 —— 重载靠它判「动过没有」。
    // ⚠ 顺序：**先 stat 再读**。反过来的话，「读完到 stat 之间那次写」会被记成「已经读过了」，
    //   那一次修改就永远不会被重载看见（一个会留下来的错，不是一次抖动）。
    let stamp = stamp_of(&path);
    let mut loaded = creds::load(&path);
    // ★ 装表这一步（`K-H2`）**在出声之前**：`announce` 要印的「有几行进得了表」
    //   与「哪几行进不去、为什么」都是它算出来的。
    //   ⚠ `take` 是因为 `AccountEntry` 里装着 `SecretKey`，而那个类型**刻意不给 `Clone`**
    //     （`K-H2a`：少一条能复制明文的路就少一个出口）⇒ 只能把所有权交出去。
    //     `announce` 不读 `accounts` 这一格，它读的是路径 / 权限 / 问题，外加下面这两个参数。
    let (table, rejected, notes) = table::build(std::mem::take(&mut loaded.accounts), default_base);
    creds::announce(&loaded, table.len(), &rejected, &notes, out);
    (table, path, stamp)
}

fn run_with(
    port_env: Option<&str>,
    upstream_env: Option<&str>,
    creds_get: &dyn Fn(&str) -> Option<String>,
    home: &std::path::Path,
) -> i32 {
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
    // ⚠ 顺序：**起监听之后、进接受循环之前**。放在起监听之前的话，
    //   端口起不来那条支会先把凭据路径印出来，而那时它还不相干。
    let (table, creds_path, stamp) =
        load_credentials(creds_get, home, &base, &mut std::io::stderr());
    // `D1 阻-2`：把重载源接上 —— 没有这一行，那张表就是一张**启动快照**，
    // 用户在界面上配完 key 必须重启中转才生效（而不重启的症状是一个静默的 404）。
    let relay = Relay::new(table, TeeSink::to_stdout()).reloading_from(Reload::new(
        creds_path,
        base.clone(),
        stamp,
    ));
    serve(listener, Arc::new(relay));
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
type RelayExec<'a> = dyn Fn(Option<&str>, Option<&str>, &dyn Fn(&str) -> Option<String>, &std::path::Path) -> i32
    + 'a;

fn run_reading(
    get: &dyn Fn(&str) -> Option<String>,
    home: &std::path::Path,
    exec: &RelayExec<'_>,
) -> i32 {
    let port = get(ENV_PORT);
    let upstream = get(ENV_UPSTREAM);
    // ⚠ `get` 原样往下传：凭据那条路的取值器**必须与端口/上游是同一个**，
    //   否则判据喂进去的环境和生产段读的环境是两套（那正是「量具的作用域对不上事实」）。
    exec(port.as_deref(), upstream.as_deref(), get, home)
}

/// `--relay` 的入口。配置面只有环境变量（backend 今天没有配置文件面）。
///
/// 本函数今天**只剩一件事**：把「真取值器」与 `run_with` 接上。接线本身（哪个变量
/// 喂给哪个位）住 `run_reading`，那里有判据钉着。**别往里加逻辑**：加进来的就又没判据了
/// —— 本函数这一行今天是**判不了**的那一格，登记住址件文件 §8.18.3。
pub fn run(home: &std::path::Path, _args: &[String]) -> i32 {
    run_reading(&|k| std::env::var(k).ok(), home, &run_with)
}

#[cfg(test)]
#[path = "../../../tests/backend/relay/server_tests.rs"]
mod tests;
