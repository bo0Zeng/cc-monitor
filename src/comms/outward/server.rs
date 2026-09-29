//! 中转 · **交换面**：一条请求从读头到收尾的全程 —— 读头 → 问上游选择（`resolve`）
//! → 连上游 → 逐块透传 + tee。
//!
//! # 🔴 通信层成员 `COMM-LAYER-MEMBER`〔`设计/99 §4 P16`，2026-09-22 用户裁〕
//!
//! 登记那一侧在 `tests/frontend/shell/comm_boundary_registry_tests.rs::REGISTERED`（两向集合相等）。
//! 盖上它 = **上锁**：本文件从此被 `C1`–`C5` ＋ `X1`–`X6` 十一条一起管着。
//!
//! **凭什么**：本文件第一行就是答案 —— 它是**中转本体**（`20 §4` 要的那个名字正是
//! `exchange.rs`），`设计/05 §4.3` 归通信层那一列点名了它。
//!
//! **它先前差两条，`P16` 同拍清掉**：`C2`（`Destination::Substitute` 不再带 `creds-core`
//! 的类型 ⇒ 连明文都碰不到了）＋ `X2`（那个 30 秒的**值**搬去 `listen.rs`，
//! 装它的那一手留在本文件、改成收入参）。
//!
//! ⚠ **它不买「这一层做得对」** —— 请求头拼得对不对由 `wire_golden` 的逐字节金标准与
//! `server_tests` 那一族负责；这枚标记只买「它没在这一层里长出业务、也没伸手去拿东西」。
//!
//! # ⚠⚠ 🔴 它的**名字**与 `设计/20 §4` 对不上，理由现打，别当成漏了
//!
//! `20 §4` 那张拆分表要的名字是 **`relay/exchange.rs`**。今天它仍叫 `server.rs`，
//! 挡着改名的是**写区外的登记**，拿这个路径当住址、改名当场红（〔R3〕今天剩下两行，第一行已不挡）：
//!
//! | 登记 | 它钉着什么 |
//! |---|---|
//! | `tests/frontend/shell/creds_store_tests.rs::PLAINTEXT_EXIT_SITES` | 〔R3 订正〕**今天已不钉本文件**：`expose_for_auth_header(` 恰好 1 处、住 `src/backend/accounts/upstream/mod.rs`（上游选择算好头材料交下来，本层碰不到明文）|
//! | `tests/frontend/shell/byte_cap_registry_tests.rs` | `HEAD_CAP` / `BODY_CAP` / `TEE_DECODE_CAP` 三条的住址栏都是这个路径 |
//! | `tests/frontend/shell/structural_scan_tests.rs` | `("src/comms/outward/server.rs", "handle_alloc_error", 1)` |
//!
//! 那几处都在 `tests/frontend/shell/` 下，**不在本拍的写区里**（写区逐字是
//! `src/backend/relay/` 及它下面新建的目录 ＋ `tests/backend/relay/`）。
//! ⇒ 本拍**只搬职责、不改文件名**：监听那半已经挪进 `listen.rs`，
//! 这里剩下的就是 `20 §4` 说的 `exchange`。改名要与那几处同拍，留给下一件。
//!
//! # 中转与上游选择的分界就在这一层里的一句话上
//!
//! `handle` 里那一句 `relay.dest.resolve(r.mode, &r.key, …)` —— 递过去的是两个
//! **不透明段**（条 48），拿回来的是一个 `Destination`，**照做，不做任何判断**。

use super::door;
use super::http1::{self, BodyView, RequestHead};
use super::route;
use super::tee::{SseSplitter, TeeSink};
use super::upstream::{self, Base, Conn};
use super::{AuthSwap, Destination, Destinations, StreamId};
use std::io::{BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// ⚠ 上游的环境旋钮与默认值**先搬去上游选择**（`20 §4`「常量跟着职责走」），**再被条 59 整删**成
//   每 agent 一行的表（`agents::Adapter::upstream`，〔NT2 · V25〕跟着适配层）。中转里**没有任何可以回落的默认上游**
//   —— 这一句由 `table_guard::the_relay_has_no_default_upstream_to_fall_back_to`
//   的**两向相等断言**钉着（中转零处 ＋ 上游选择恰好登记那几处），不是一条散文。

// ══ 下面这个常量的**职责在 `listen.rs`**（监听面），代码留在这里 ══════════════
//    〔NET2〕原先是三个：在途上界 `INFLIGHT_CONNECTIONS` 与在途计数已挪去 `listen.rs`（`设计/20 §4` · `§10` 第 7 条）。
//    理由**不是**职责，是一处**写区外的散文住址**逐字点着 `…/relay/server.rs::<常量名>`（〔DEL〕`DEFAULT_PORT` 随 `--relay` 删了），
//    而 `structural_scan::every_symbol_address_in_the_sources_still_resolves` 真的判得了
//    那种住址（现打：搬去 `listen.rs` 之后它当场红，诊断逐字「符号还在，但**搬家了**」）。
//    逐条登记在 `listen.rs` 的头注里。⇒ `listen.rs` `use` 它们。

/// 只听回环。**这是一个字面量常量，不是拼出来的** —— 拼出来的地址源码扫描看不见
/// （`DoD-4` 那条 acceptor 的第一个瞎法就是这个）。行为那半由 `DoD-4㈡` 兜底。
pub(super) const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

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
/// **登记住址** `src/frontend/shell/src/byte_cap_registry.rs`（那张表默认拒绝：不登记就红）。
const BODY_CAP: usize = 64 * 1024 * 1024;
/// **tee 侧解码缓冲**的字节上限（`SseSplitter` 的半行 · `ChunkedView` 攒着的那截）。
///
/// 它与 `BODY_CAP` **不同族**：那一条防的是「拿外部给的**一个数**去分配」（攻击方一个字节
/// 不用发），这一条防的是「按**真实收到的字节**无界增长」（上游发一条永不换行的 `data:` 行 /
/// 永不结束的块长度行）。超了**丢弃+带身份报告**：丢掉那一截并计数，
/// 由 tee 在 tap 上占一个号不发报出去（接收侧看得见缺口）—— **tee 少一段，下游的字节一个不少**。
const TEE_DECODE_CAP: usize = 8 * 1024 * 1024;
/// 每次从上游读多少 —— **上限**，不是「要凑满这么多」。
/// `std::io::Read::read` 本来就是「有多少给多少」，不循环凑满。
const READ_CHUNK: usize = 64 * 1024;

// ══ 中转**自己造**的状态码 —— 每个码只有这一处住址〔`设计/20 §3.1a` ②，`D2`〕═══════════
//
// 〔FIX3 · `99 §2.2 ⑫`〕照 HTTP 代理通行做法分两边：**我们拒的一律 4xx**（门 403/421 · 读不懂 400/411/413 ·
// 路由不成立 404，后者住上游选择那一侧）；**5xx 只说上游那侧**（连不上 / 断了 / 回的不是 HTTP ⇒ 502 · 超时 ⇒ 504）；
// 在飞上界 503 是我们这侧吃不下（代理的通行码）。上游自己的码原样透传 ⇒ 同一个码可能是我们说的、也可能是上游说的，
// 分开它们的是**原因头** [`REASON_HEADER`]：中转自己回的每一条响应都带它，透传的从来不带。
// 钉这两条的判据：`server_tests::every_status_we_make_has_one_home_and_the_three_groups_are_disjoint`
// （盘上扫出来的状态码字面量 ⇔ 登记表，两向相等；我们拒的全是 4xx、上游那侧全是 5xx）。

/// 〔FIX3 · `99 §2.2 ⑫`〕中转自己回的响应带的原因头：值是一个 ASCII 短词（`bad-key` · `no-account-row` · `upstream-connect` …）。
pub(super) const REASON_HEADER: &str = "X-Cc-Monitor-Reason";

/// 下游请求**读不懂**（头坏了 / `Content-Length` 读不懂）。
const BAD_REQUEST: &str = "400 Bad Request";
/// 下游用了 chunked 请求体（本中转只收定长请求体）。
const LENGTH_REQUIRED: &str = "411 Length Required";
/// 下游请求体超 `BODY_CAP`。
const PAYLOAD_TOO_LARGE: &str = "413 Payload Too Large";
/// 路径**根本不是路由的形状**。与上游选择那个 404（表里没这一行）同属「路由不成立」一组，原因头不同。
const NOT_A_ROUTE: &str = "404 Not Found";
/// 在飞连接顶满（`listen.rs::INFLIGHT_CONNECTIONS`）或起不了连接线程。**「我们这侧现在吃不下」**。
///
/// ⚠ 名字刻意不带 `CAP`/`MAX`/`LIMIT`/`BYTES`（那几个词是 `byte_cap_registry` 的钩子）。
pub(super) const BUSY: &str = "503 Service Unavailable";
/// 🔴 **中转自己的传输失败、而且不是超时**：上游连不上 · 发到一半断了 · 没回应就断 · 回的不是 HTTP · 只给 1xx〔`设计/20 §3.1a`〕。
/// 〔FIX3 · `99 §2.2 ⑫`〕先前一律 504（502 被上游选择的 `Refuse` 占着）；那个 `Refuse` 改成 4xx 之后 502 让回给它的本义。
const UPSTREAM_UNREACHABLE: &str = "502 Bad Gateway";
/// 🔴 **中转自己的传输失败、卡在超时上**（连接超时 · 等响应超时）。
const UPSTREAM_TOO_SLOW: &str = "504 Gateway Timeout";

/// 〔`P16` 2026-09-22〕**`DOWNSTREAM_DEADLINE` 的那个值搬去 `listen.rs` 了** —— 墓碑。
///
/// `设计/01 §2.1 C4` 逐字把「**期限值**」也算进那句「凭据、配置、路由表、期限值
/// 全部由后端**交给它**」⇒ `设计/05 §3.3.2` 的分工是「**值归后端 · 执行归通信层**」。
/// 搬走的只有**值**：装它的那一手（[`apply_downstream_deadline`]）**还在这一层**，
/// 只是改成收一个入参。⚠ 别把它读成「期限没人管了」—— 它换了个人交下来，
/// 而「两个调用点必须用同一个数」这件事今天由 [`Relay`] 那一格字段保证。
///

/// 把**后端交下来的**下游期限装到一条 socket 的**两个方向**上（`C4`：值归后端 · 执行归本层）。
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
pub(super) fn apply_downstream_deadline(
    s: &TcpStream,
    deadline: std::time::Duration,
) -> std::io::Result<()> {
    s.set_read_timeout(Some(deadline))?;
    s.set_write_timeout(Some(deadline))
}

/// 上游**中间响应**（1xx）最多容忍几条〔回修轮之五 08-25，D3 `重要-1(D3)`〕。
/// 超了回 [`UPSTREAM_UNREACHABLE`]（502）：那已经不是一个正常的上游。
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
///
/// ⚠⚠ **`设计/20 §7` 步 1 之后，上面这一整段的后半截要重读**：`table` / `reload`
/// 两个字段**已经不在本结构体里了**，它们随热重载一起搬进了 `accounts/`（上游选择）。
/// 今天 `Relay` 手里只剩一个 `dyn Destinations` —— 上游选择整块藏在它后面。
/// 那条文本棘轮**仍然留着**（它守的是「别把那两个字段加回来」），只是它守的窗口更小了。
pub(crate) struct Relay {
    /// 上游选择整块藏在这后面（`20 §4` 那张「之后」的图）。
    ///
    /// ★★ 中转对它**只会问一句** `resolve(mode, &RouteKey, …)`，拿到一个
    /// `Destination` 就照做。它**问不出**「表里有几行」「那一行的 key 是什么」
    /// 「有没有默认上游」—— 那些词在这一层根本不存在。
    dest: Arc<dyn Destinations>,
    /// 〔RK1〕这扇门的钥匙（`door.rs`）。**由监听面交下来**（`listen::prepare` 绑上口之后读回或铸）。
    /// ⚠ 字段名刻意不叫那个会被 `table_guard` 当成「进程级凭据」的字面：它不是上游的凭据，是**下游进门**的钥匙。
    door: door::Key,
    tee: TeeSink,
    /// 〔V141〕给流打标签的请求头名单（构造时向上游选择要一次，[`Destinations::stream_label_headers`]）。
    stream_headers: Vec<&'static str>,
    /// 下游那条 socket 的读写期限。**由后端交下来**（`C4`：值归后端 · 执行归本层）。
    ///
    /// ⚠ 它是**一个字段**而不是两处各写一次 —— [`apply_downstream_deadline`] 有两个
    /// 调用点，而它们**必须用同一个数**（先前那件事由「都引同一个 `const`」保证，
    /// 值搬走之后由这一格保证）。
    downstream_deadline: std::time::Duration,
    /// 上游那条 socket 的读写期限。同上，由后端交下来。
    upstream_deadline: std::time::Duration,
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
    /// **生产段唯一的构造入口**（`run_with` 走它）。
    ///
    /// ⚠ 先前有两个入口（`new` / `with_key`），`K-H2` 之后只剩一个：
    /// 「带不带 key」不再是**中转**的属性，而是**表里某一行**的属性。
    ///
    /// ⚠⚠ `设计/20 §7` 步 1：入参从一张**路由表**换成了一个 `dyn Destinations`
    /// —— 中转从此不认识「表」这个东西。
    pub(crate) fn new(
        dest: Arc<dyn Destinations>,
        door: door::Key,
        tee: TeeSink,
        downstream_deadline: std::time::Duration,
        upstream_deadline: std::time::Duration,
    ) -> Self {
        Self {
            stream_headers: dest.stream_label_headers(),
            dest,
            door,
            tee,
            downstream_deadline,
            upstream_deadline,
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

pub(super) fn respond_status(
    down: &mut TcpStream,
    status: &str,
    reason: &str,
) -> std::io::Result<()> {
    respond_body(down, status, reason, format!("{status}\n"))
}

/// 中转传输失败那一格：状态行（502 / 504，[`UpstreamFailure::status`]）＋ 原因头 ＋ **一句说得清是谁、卡在哪的话**（`20 §3.1a`）。
fn respond_upstream_failed(down: &mut TcpStream, why: &UpstreamFailure) -> std::io::Result<()> {
    // ⚠ 只印上游的主机与端口 ＋ 一句固定文案，**永不印请求头**（`K9` 裁定四第 1 条）。
    let upstream_failure = why.for_log();
    eprintln!("[relay] upstream failed: {upstream_failure}");
    let said = why.sentence();
    let status = why.status();
    respond_body(down, status, why.at.reason(), format!("{status}\n{said}\n"))
}

fn respond_body(
    down: &mut TcpStream,
    status: &str,
    reason: &str,
    body: String,
) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{REASON_HEADER}: {reason}\r\nConnection: close\r\n\r\n",
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
pub(super) fn respond_and_drain(
    down: &mut TcpStream,
    status: &str,
    reason: &str,
) -> std::io::Result<()> {
    let r = respond_status(down, status, reason);
    drain_arrived(down);
    r
}

/// 同 [`respond_and_drain`]，只是体里多一句为什么〔RK1：门拒绝那几格要说清是哪一问拒的，
/// 否则 403「钥匙不对」与 403「带了 Origin」在下游那一侧读起来一样〕。
fn respond_body_and_drain(
    down: &mut TcpStream,
    status: &str,
    reason: &str,
    body: String,
) -> std::io::Result<()> {
    let r = respond_body(down, status, reason, body);
    drain_arrived(down);
    r
}

/// 上两条共用的「排掉已经到了的」那一半（理由整段在 [`respond_and_drain`] 头注）。
fn drain_arrived(down: &mut TcpStream) {
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
}

/// 上游选择答完那一刻，中转手里的**三种**结局（下游看到的字节各不相同）。
///
/// | 结局 | 下游看到 | 上游收到过字节吗 |
/// |---|---|---|
/// | `Sent` | 上游那条响应，逐块透传 | 是 |
/// | `Refused` | 上游选择给的 4xx ＋ 原因头 ＋ 一句为什么（住 `accounts` 那一侧） | **否** |
/// | `UpstreamFailed` | 502 / 504 ＋ 原因头 ＋ 一句 `why`（[`UpstreamFailure`]） | 连不上 ⇒ 否；发到一半断了 ⇒ 发过一截 |
///
/// 〔FIX3 · `99 §2.2 ⑫`〕「发到一半断了」（先前的 `WriteFailed`）原先**什么都不回**（连接以错误收尾）：
/// 下游那时一个字节都还没收到，回一个码不会与已发的字节打架 ⇒ 并进传输失败、回 502 说清卡在发请求（出声，不静默）。
enum Answered {
    /// 已连上、请求已写完。带着**这是谁**（后面等响应那一段失败时 `why` 要说得出来）。
    Sent(Conn, Who),
    Refused {
        status: &'static str,
        reason: &'static str,
        why: &'static str,
    },
    UpstreamFailed(UpstreamFailure),
}

/// 中转传输失败时那句 `why` 的两半：**上游是谁 · 卡在哪一跳**〔`设计/20 §3.1a`〕。
///
/// # 它刻意只带这几样
///
/// - 上游：**主机 ＋ 端口**。`Base` 里那段路径前缀**不带** —— 那是凭据文件里的内容，
///   与 `table::Note::what` 刻意不印前缀是同一条理由。
/// - 哪一跳：一个固定文案（[`FailedAt`]）。
/// - 底层那条 `io::Error` **只进 stderr**，不进回给下游的字节（那是实现细节，`01 §6.9`：
///   对外的话不出现内部词）。
pub(super) struct UpstreamFailure {
    who: Who,
    at: FailedAt,
    cause: Option<std::io::Error>,
}

/// 上游是谁：主机 ＋ 端口（**不带**路径前缀，理由见 [`UpstreamFailure`]）。
///
/// ⚠ 它是从上游选择借给我们的那个 `&Base` 上**现抄**的一份：那个借用活不出上游选择的锁
/// （`Destinations::resolve` 的契约），而「等响应」那一段在锁外。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Who {
    host: String,
    port: u16,
}

impl Who {
    fn of(base: &Base) -> Self {
        Self {
            host: base.host.clone(),
            port: base.port,
        }
    }
}

/// 传输失败卡在哪一跳。**每一支一句固定文案**，先说结果、再说卡在哪（`01 §6.9`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FailedAt {
    /// 连接都没建立起来（拒绝连接 · 名字解析不了 · TLS 握手失败 · 连接超时）。
    Connect,
    /// 连上了，请求没发完连接就断了（先前的 `WriteFailed`，FIX3 起回码）。
    Send,
    /// 请求发过去了，对方没回响应就把连接关了。
    ClosedBeforeAnswer,
    /// 请求发过去了，等响应时出错或超时。
    NoAnswer,
    /// 对方回的不是认得出的 HTTP 响应。
    NotHttp,
    /// 对方只回中间响应（1xx），一直不给最终响应。
    OnlyInterim,
}

impl FailedAt {
    /// 这一跳的那句话。**两句**：结果 · 卡在哪。
    pub(super) fn words(self) -> (&'static str, &'static str) {
        match self {
            FailedAt::Connect => (
                copy_core::copy_static!("beServer.words.cantConnect"),
                copy_core::copy_static!("beServer.words.hopConnect"),
            ),
            FailedAt::Send => (
                copy_core::copy_static!("beServer.words.sendFailed"),
                copy_core::copy_static!("beServer.words.hopSend"),
            ),
            FailedAt::ClosedBeforeAnswer => (
                copy_core::copy_static!("beServer.words.closedBeforeAnswer"),
                copy_core::copy_static!("beServer.words.hopWait"),
            ),
            FailedAt::NoAnswer => (
                copy_core::copy_static!("beServer.words.noAnswer"),
                copy_core::copy_static!("beServer.words.hopWait"),
            ),
            FailedAt::NotHttp => (
                copy_core::copy_static!("beServer.words.notHttp"),
                copy_core::copy_static!("beServer.words.hopRead"),
            ),
            FailedAt::OnlyInterim => (
                copy_core::copy_static!("beServer.words.onlyInterim"),
                copy_core::copy_static!("beServer.words.hopRead"),
            ),
        }
    }
}

impl FailedAt {
    /// 原因头的值（[`REASON_HEADER`]）：每一跳一个 ASCII 短词。
    pub(super) fn reason(self) -> &'static str {
        match self {
            FailedAt::Connect => "upstream-connect",
            FailedAt::Send => "upstream-send",
            FailedAt::ClosedBeforeAnswer => "upstream-closed",
            FailedAt::NoAnswer => "upstream-no-answer",
            FailedAt::NotHttp => "upstream-not-http",
            FailedAt::OnlyInterim => "upstream-only-interim",
        }
    }
}

impl UpstreamFailure {
    fn new(who: &Who, at: FailedAt, cause: Option<std::io::Error>) -> Self {
        Self {
            who: who.clone(),
            at,
            cause,
        }
    }

    /// 〔FIX3 · `99 §2.2 ⑫`〕回哪个码：卡在超时上（底层错误是 `TimedOut` / `WouldBlock`——后者是 socket 读写期限到了的样子）⇒ 504，
    /// 其余上游那侧的失败 ⇒ 502。
    pub(super) fn status(&self) -> &'static str {
        let timed_out = self.cause.as_ref().is_some_and(|e| {
            matches!(
                e.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            )
        });
        if timed_out {
            UPSTREAM_TOO_SLOW
        } else {
            UPSTREAM_UNREACHABLE
        }
    }

    /// 回给下游的那句话。底层错误**不在这里**（见 [`UpstreamFailure`] 头注）。
    pub(super) fn sentence(&self) -> String {
        let (result, hop) = self.at.words();
        copy_core::copy_text(
            "beServer.sentence.say",
            &[
                ("host", &self.who.host),
                ("port", &self.who.port.to_string()),
                ("result", result),
                ("hop", hop),
            ],
        )
    }

    /// 进 stderr 的那一行：回给下游的那句话 ＋ 底层错误（有的话）。
    fn for_log(&self) -> String {
        let said = self.sentence();
        match &self.cause {
            Some(e) => format!("{said}（{e}）"),
            None => said,
        }
    }
}

/// 把一个**成立**的目的地兑现成一条「已连上、请求已写完」的上游连接。
///
/// ★★ **`upstream::connect(` 全后端生产段恰好一处，就是这里**
/// （`table_guard::the_only_place_that_opens_an_upstream_connection_is_the_exchange`
/// 那条相等断言钉着）⇒ 没有第二条路能绕过 `resolve` 把请求发出去。
///
/// ⚠⚠ **它先前住 `table::Row::connect`**（`设计/20 §7` 步 1 搬到这里）。搬的理由：
/// `20 §4` 逐字把「连上游」划给中转的 `exchange`，而中转手里只有 `Destination`
/// 里那个 `&Base`。**换到手里的东西**写在 `table::Row::base` 的头注里。
///
/// ⚠ `auth` 那一格**只能从调用方那一个 `Destination` 里解构出来**，
/// 不许由调用方自己凑 —— 凑得出来就等于「A 的端点配 B 的 key」又写得出来了。
fn send_upstream(
    base: &Base,
    auth: Option<&AuthSwap<'_>>,
    head: &RequestHead,
    rest: &str,
    body: &[u8],
    deadline: std::time::Duration,
) -> Answered {
    let mut up = match upstream::connect(base, deadline) {
        Ok(c) => c,
        Err(e) => {
            let who = Who::of(base);
            return Answered::UpstreamFailed(UpstreamFailure::new(
                &who,
                FailedAt::Connect,
                Some(e),
            ));
        }
    };
    let wrote = (|| -> std::io::Result<()> {
        up.write_all(&render_upstream_request(head, rest, base, auth, body.len()))?;
        if !body.is_empty() {
            up.write_all(body)?;
        }
        up.flush()
    })();
    match wrote {
        Ok(()) => Answered::Sent(up, Who::of(base)),
        Err(e) => Answered::UpstreamFailed(UpstreamFailure::new(
            &Who::of(base),
            FailedAt::Send,
            Some(e),
        )),
    }
}

/// 处理一条下游连接：解析 → 问上游选择 → 连上游 → 逐块透传 + tee。
pub(super) fn handle(down: TcpStream, relay: &Relay) -> std::io::Result<()> {
    down.set_nodelay(true)?;
    // ★★ `阻-3(D3)` 后半段的正主：没有这一句，一条半开连接（只发半个请求头就不动了）
    //    会把这条线程**永久**钉在下面 `read_head` 的读上。
    //    `try_clone` 是 `dup` ⇒ 下面 `down_w` 与 `down_r` 共用同一条 socket、同一份期限。
    apply_downstream_deadline(&down, relay.downstream_deadline)?;
    let mut down_w = down.try_clone()?;
    let mut down_r = BufReader::new(down);

    let Some(raw_head) = http1::read_head(&mut down_r, HEAD_CAP)? else {
        return respond_and_drain(&mut down_w, BAD_REQUEST, "bad-request-head");
    };
    let Some(head) = http1::parse_request(&raw_head) else {
        return respond_and_drain(&mut down_w, BAD_REQUEST, "bad-request-head");
    };
    // 🔴 〔RK1 · `INVARIANTS §48.1a`〕**进门三问排在一切之前**（读请求体之前、问上游选择之前）：
    //   Origin ⇒ 403 · Host 非回环 ⇒ 421 · 钥匙不对 ⇒ 403。过了才剥掉 `/<钥匙>`，余下的交给 `route::parse`
    //   ⇒ 「钥匙对、表里没这一行」仍是 404，与 403 可分。钥匙不进上游（转上去的是剥之后的路径）、不进 tee、不进日志。
    let target = match door::admit(&head, &relay.door) {
        door::Verdict::Pass(rest) => rest,
        refused => {
            let (status, reason, why) = refused.refusal().expect("非 Pass 那几格都有拒绝的说法");
            // ⚠ 只印是哪一问拒的，**永不印请求头 / 路径**（`K9` 裁定四第 1 条；路径里可能正是一把错钥匙）。
            eprintln!("[relay] refused at the door: {status}");
            return respond_body_and_drain(
                &mut down_w,
                status,
                reason,
                format!("{status}\n{why}\n"),
            );
        }
    };
    if head.is_chunked_body() {
        return respond_and_drain(&mut down_w, LENGTH_REQUIRED, "length-required");
    }
    let Some(r) = route::parse(&target) else {
        return respond_and_drain(&mut down_w, NOT_A_ROUTE, "not-a-route");
    };
    // ★ `阻-1(D3)` + `重要-2(D3)`：请求体这一格先前有**两个**洞，两个都在这几行上。
    //   ① 长度**无上界** ⇒ `Content-Length: 1e12` 把整个进程 abort 掉（SIGABRT，不走 unwind）；
    //   ② 长度**读不懂**（`7abc`）与「没有这个头」挤在同一个 `None` 里 ⇒ 请求体被静默丢掉、
    //      上游收到空体、下游拿到一条正常的 200。中转搬的正是 `POST /v1/messages` 的载荷。
    // ⚠ 顺序：它排在**问上游选择之前**（`D2 阻-4`）—— 读下游是一次可能很慢的 IO，
    //   而上游选择在 `resolve` 里握着自己那张表的读锁；握着锁去等下游，
    //   等于让「配一次 key」跟着它一起慢。
    let body = match head.content_length() {
        http1::BodyLen::Exact(n) => match http1::read_exact_body(&mut down_r, n, BODY_CAP)? {
            Some(b) => b,
            // 超 `BODY_CAP`：一个字节都没读过（连接上还压着那 n 字节）⇒ 说清楚再关。
            None => return respond_and_drain(&mut down_w, PAYLOAD_TOO_LARGE, "payload-too-large"),
        },
        http1::BodyLen::Absent => Vec::new(),
        // **有这个头但读不懂** ⇒ 400，**不许**当成「没有请求体」往上游发一条空体。
        http1::BodyLen::Unparsable => {
            return respond_and_drain(&mut down_w, BAD_REQUEST, "bad-content-length")
        }
    };

    relay.served.fetch_add(1, Ordering::SeqCst);

    // ★★★ **层间那一问就在这里**（`设计/20 §2`）：中转问一句，上游选择答一句，
    //      **中转不做任何判断，照答案做**。
    //
    //  ⚠ 递过去的是 `r.key`（`RouteKey{seg1,seg2}`）—— 两个**不透明段**。
    //    「谁是 agent、谁是账号」这两个词在本文件里一次都不出现（条 48）。
    //
    //  ⚠⚠ `D2 阻-4`：**上游选择的读锁活到 `act` 返回为止，所以 `act` 里不许有 `pump`。**
    //    `std::sync::RwLock` 是写优先的：一个在等的写者（= 用户刚配完一把 key，
    //    下一条请求触发重载）会挡住其后所有读者；而 `pump` 是流式转发，
    //    一条 SSE 长流可以跑几分钟 ⇒ `pump` 搬进来，「配一次 key」就会被堵在
    //    **最长那条在飞流**后面。⚠ 挂起时长**没实测**，这是读源码得出的形状。
    //    钉这一条的判据：`table_guard::the_upstream_selection_lock_does_not_outlive_the_streaming_pump`。
    let mut answered: Option<Answered> = None;
    relay.dest.resolve(r.mode, &r.key, &mut |d| {
        answered = Some(match d {
            // 路由不成立 ⇒ 回这个码 ＋ 原因头 ＋ 那句为什么，**一个字节都不发上游**。
            Destination::Refuse {
                status,
                reason,
                why,
            } => Answered::Refused {
                status,
                reason,
                why,
            },
            // 下游那份 auth 头**原样转发**。中转手里没有任何 key。
            Destination::Passthrough { upstream } => send_upstream(
                upstream,
                None,
                &head,
                &r.rest,
                &body,
                relay.upstream_deadline,
            ),
            // 剥掉下游 auth，按这一行自己的说法写（`key` 为 `None` ＝ 什么都不写，
            // 那是 `AuthSwap::write == None` 那一档，理由整段住 `accounts::upstream::dispatch_auth`）。
            // ★★ 上游与 key 取自**同一个变体**，不是两个各自取的值。
            Destination::Substitute { upstream, auth } => send_upstream(
                upstream,
                Some(&auth),
                &head,
                &r.rest,
                &body,
                relay.upstream_deadline,
            ),
        });
    });
    // 上游选择必须**恰好答一次**（契约写在 `Destinations::resolve` 头注里）。
    // 一次都不答 ＝ 下游会拿到一个没有任何 HTTP 响应的 FIN，那正是 `阻-3(D3)`
    // 点名的静默拒绝 ⇒ 宁可在这里当场炸，也不静默。
    let (mut up, who) = match answered
        .expect("上游选择一次都没答 —— `Destinations::resolve` 的契约被破了")
    {
        Answered::Sent(up, who) => (up, who),
        Answered::Refused {
            status,
            reason,
            why,
        } => {
            return respond_body_and_drain(
                &mut down_w,
                status,
                reason,
                format!("{status}\n{why}\n"),
            )
        }
        Answered::UpstreamFailed(why) => return respond_upstream_failed(&mut down_w, &why),
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
    // 超过 `INTERIM_RESPONSES_ALLOWED` 条就回 [`UPSTREAM_UNREACHABLE`]（那已经不是一个正常的上游）。
    // ⚠ `101 Switching Protocols` 也是 1xx：本中转**不支持**协议升级
    //   （`Upgrade` / `Connection` 都在逐跳表里、根本转不到上游），真收到 101 就会
    //   继续往下读，而其后是隧道字节不是 HTTP 头 ⇒ `parse_response` 失败 ⇒ **502**。
    //   那是个**定义好的**结局，不是「当成最终响应发下去」。
    //
    // 🔴 〔`设计/20 §3.1a`〕这一段的四种失败**全是中转自己的传输失败**（上游不答 / 答的不是
    //   HTTP），先前三支回 502、读出错那一支**什么都不回**（`?` 往上抛，下游拿到一个没有
    //   任何 HTTP 响应的 FIN）。今天四支一律回码（超时 504、其余 502，FIX3）＋ 原因头 ＋ 一句说得清卡在哪的话。
    //   ⚠ 读出错那一支（上游读期限到了 / 连接被重置）从「静默 FIN」变成「504（超时）或 502 ＋ why」：
    //     `05 §4.5.3` ① 逐字「把传输失败翻成一个 HTTP 响应，原样回给 agent」。
    //     下游那一侧此刻**一个字节都还没收到**（响应头还没写），所以回一个状态码不会与已发的字节打架。
    let (headers, raw_resp) = {
        let mut interim = 0usize;
        loop {
            let fail = |at, cause| UpstreamFailure::new(&who, at, cause);
            let raw = match http1::read_response_head(&mut up, HEAD_CAP) {
                Ok(Some(raw)) => raw,
                Ok(None) => {
                    let why = fail(FailedAt::ClosedBeforeAnswer, None);
                    return respond_upstream_failed(&mut down_w, &why);
                }
                Err(e) => {
                    return respond_upstream_failed(&mut down_w, &fail(FailedAt::NoAnswer, Some(e)))
                }
            };
            let Some((status, headers)) = http1::parse_response(&raw) else {
                return respond_upstream_failed(&mut down_w, &fail(FailedAt::NotHttp, None));
            };
            if http1::is_interim_status(&status) {
                interim += 1;
                if interim > INTERIM_RESPONSES_ALLOWED {
                    return respond_upstream_failed(
                        &mut down_w,
                        &fail(FailedAt::OnlyInterim, None),
                    );
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
    // ★ 路由键与流标签收成一个 `StreamId`（`20 §4`）—— 中转这一侧**没有业务名**；tee 只抄流标签（① 不问账号，`20 §11` I2）。
    //   〔V141〕流标签取自请求自己带的头（会话 id 归 agent），不是路径段。
    let id = StreamId {
        stream: stream_label(&head, &relay.stream_headers),
    };
    // 〔TAP〕`open` 发一个这一响应自己的游标（位置号 `n` 从 0 起），`event` / `note_dropped_bytes` / `close` 都拿它。
    let mut at = relay.tee.open();
    // ★ 返回值**必须落地**：它是 `DoD-2㈡`「块数对账」的唯一量点。
    // 写成 `pump(...)?;` 就等于把它丢掉 —— 那正是审计 `K4` 能全绿的原因。
    let outcome = pump(&mut up, &mut down_w, &mut |raw| {
        let decoded = view.feed(raw, TEE_DECODE_CAP);
        for payload in splitter.feed(&decoded, TEE_DECODE_CAP) {
            relay.tee.event(id, &mut at, &payload);
        }
        // 解码那一路超上限丢掉的字节要**报出去**，不许静默（见 `TEE_DECODE_CAP` 头注）。
        relay
            .tee
            .note_dropped_bytes(&mut at, view.take_dropped() + splitter.take_dropped());
    });
    relay.note_pump(&outcome);
    // 〔TAP〕收尾交一件：转发以错误收尾（claude 按了 Esc / 上游 RST / 下游写不动）⇒ `broken`。
    relay.tee.close(id, at, outcome.is_err());
    outcome?;
    Ok(())
}

/// 〔V141〕这条流的标签：名单里第一个出现在请求里、值过段闸的头的值；没有 ⇒ 空串（前端当匿名流）。
/// 过闸是因为它要进 tee 行与 tap 帧（同 `route::segment_is_safe` 那条理由）。
fn stream_label<'h>(head: &'h RequestHead, names: &[&str]) -> &'h str {
    names
        .iter()
        .find_map(|n| head.header(n).filter(|v| route::segment_is_safe(v)))
        .unwrap_or("")
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
/// `K11 裁定一` 在 **08-25 被改判**，现行正文三句：端点与 key 都归**按账号查的那张路由表**
/// （原文把那张表记在中转名下 —— 它今天是上游选择的 apikey 表）；转发时替换 `Authorization` 头；
/// 客户端一个凭据都不配。
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
/// 1. **配了 key ⇒ 换头**：下游那份 `Authorization` 被**丢掉**，换上**上游选择交下来的那一格头**
///    （上游选择按这一行的 key 算好的；本层手里没有 key）。〔R3 订正〕取明文的那一行今天住
///    `accounts::upstream`（先前住本文件），它是 `expose_for_auth_header` 在**整个后端生产段里唯一**的调用点
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
///    ⇒ 今天准确的说法只有两句：①**apikey 表里一个账号都没有 ⇒ `/s/` 请求全部 404**；
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
/// 1. **换哪个头由那一行的 `auth_style` 定**（见 `accounts::upstream::auth_header_of`，`P16` 搬去上游选择了）——
///    〔用 09-04〕逐字要「api做成通用的」⇒ 只押一种鉴权头 = 只接得上一半的上游，
///    而押错的症状是 **401**，与「key 打错了」同形。
/// 2. **这一行有自己的 key 时，客户端那份鉴权头一律不转发** ——
///    人群是 `AuthSwap::clear` 那个**由上游选择交下来**的闭集，不只 `Authorization` 那一个。
///    ⚠ 这是**行为改变**，理由两条：㈠ 我这一趟要写的那个头名可能正是客户端也带着的
///    （`x-api-key` 那一档），同名头出现两次是未定义行为 —— 那正是先前丢掉
///    `Authorization` 的理由，逐字同一条；㈡ 把客户端的凭据**连带**送给一个第三方上游，
///    是把一份不属于这一行的秘密多送出去一次。
///    ⚠ **`Proxy-Authorization` 仍然照旧转发** —— 它说的是「与代理之间」的鉴权，
///    不是与上游之间的，收掉它是另一件事。**登记为射程外，不假装覆盖了。**
/// 3. **这一行没有 key 时，一个字节都不动**（`Authorization` / `x-api-key` 全照旧转发）
///    —— 订阅登录那一档要的正是这条透传路。
///    ⚠ 例外是 `AuthStyle::NoAuth`：它逐字说的就是「一个鉴权头都不发」⇒ 客户端那份也不转发。
fn render_upstream_request(
    head: &RequestHead,
    rest: &str,
    base: &Base,
    auth: Option<&AuthSwap<'_>>,
    body_len: usize,
) -> Vec<u8> {
    // ★★ **签名变迁记两拍，别读成放宽**：
    //    · `K-H2`：从 `(&Base, Option<&SecretKey>)` 收成一个 `&Row` —— 那时「上游」与
    //      「key」是两个各取各的参数，「A 的端点 + B 的 key」**写得出来**；
    //    · `设计/20 §7` 步 1：从 `&Row` 换成 `(&Base, auth)` **两个参数**，
    //      而两个参数**只能从同一个 `Destination` 变体里解构出来**（`send_upstream`
    //      是唯一调用方，它自己也只从上游选择那一个答案里取）⇒ 同源这件事从
    //      「一个值的两个方法」换成了「一个变体的两个字段」，**没有变松**。
    //
    //    · 〔`P16` 2026-09-22〕从 `(Option<&SecretKey>, AuthStyle)` 换成一个
    //      `&AuthSwap` —— **两个 `creds-core` 类型退出中转的类型面**（`C2`），
    //      而同源那件事**更紧了一格**：先前是「一个变体的两个字段」，今天是
    //      「一个变体的**一个**字段」⇒ 连「从同一个变体里取两个、但取错搭配」都写不出来。
    //
    //  `auth` 的三态与它们各自的字节，整张表住 `accounts::upstream::dispatch_auth` 的头注：
    //    `None`                            ⇒ 下游那份鉴权头**原样转发**
    //    `Some(AuthSwap{ write: Some(_) })` ⇒ 丢掉 `clear` 那几份，写上游选择算好的那一条
    //    `Some(AuthSwap{ write: None })`    ⇒ 丢掉 `clear` 那几份，**一个头都不写**
    // ★★★ **`K-R1`：请求行的目标由那个基址自己算**（前缀 + 客户端的真路径）。
    //    ⚠ 参数名从 `target` 改成 `rest` 是有意的：进来的是**下游那一段**，
    //      发出去的目标是**算出来的**。留着旧名字会让下一个人以为它已经是最终目标。
    //    ⚠ 拼接刻意**不在这里做**（实现只有一份，住 `upstream::Base::upstream_target`）——
    //      「忘了拼前缀」这件事在这个签名上写不出来（`rest` 只有一条去处）。
    let target = base.upstream_target(rest);
    let mut out = format!("{} {} HTTP/1.1\r\n", head.method, target);
    out.push_str(&format!("Host: {}\r\n", base.host_header()));
    out.push_str("Accept-Encoding: identity\r\n");
    out.push_str("Connection: close\r\n");
    // ★ 这一趟要不要把客户端自带的鉴权头收掉 —— **答案就是「上游选择答的是不是 `Substitute`」**。
    //   ⚠⚠ 这一行先前是个复合条件 `(key.is_some() && auth_header_of(style).is_some())
    //      || style == NoAuth`，头注逐字警告过「两个条件都要，缺一格就漏一形」。
    //      那个判断**整条搬进上游选择的那一个 `match`** 了（`accounts::upstream::dispatch_auth`），
    //      中转这边因此再也没有第二处可以判错 —— 少一处能判错的地方，不是少一条判断。
    //   ⚠ 〔`P16`〕**丢哪几个头**也跟着搬走了（`AuthSwap::clear`）：中转从此
    //      连那份名单都没有 ⇒ 它也不可能自己凑一份缩水的。
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
        if auth.is_some_and(|a| a.clear.iter().any(|n| k.eq_ignore_ascii_case(n))) {
            continue;
        }
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    // ★★ **中转只照写。** 头名与**完整**头值都是上游选择算好的（`AuthSwap::write`）——
    //    中转不知道那个串里有没有前缀、是不是一把 key，它只看见一个串。
    //    ⚠⚠ 〔`P16` 2026-09-22〕把明文取出来的那一句**搬去上游选择了**（`accounts::upstream::dispatch_auth`）。
    //      搬的是住址不是处数：`creds_guard` 那条「恰好 1 处」的相等断言一个字节没动
    //      （它扫整个 crate，不写死文件名），`PLAINTEXT_EXIT_SITES` 只改了住址栏。
    //      ⇒ 中转从此**碰不到明文**，这是 `C2` 买到的一格实质东西，不只是类型好看。
    if let Some((name, value)) = auth.and_then(|a| a.write) {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    if body_len > 0 {
        out.push_str(&format!("Content-Length: {body_len}\r\n"));
    }
    out.push_str("\r\n");
    out.into_bytes()
}

/// 〔`P16` 2026-09-22〕**这里先前住着两样东西，两样都搬去上游选择了** —— 墓碑，别捡回来。
///
/// | 搬走的 | 新家 | 为什么不能留在这一层 |
/// |---|---|---|
/// | `const AUTH_HEADER_NAMES`（换头前先丢掉哪几个头） | `accounts::upstream::headers_to_clear()`，**由映射派生**，不再是手写名单 | 它与那个映射之间原本靠一条判据焊着，而映射按 `C2` 必须去上游选择 ⇒ 焊缝会**跨层**，而缺焊的症状是同名鉴权头出现两次、上游谁赢没有定义 |
/// | `fn auth_header_of`（`AuthStyle` → `(头名, 前缀)`） | `accounts::upstream::auth_header_of` | 它的入参是 `creds_core::store::AuthStyle` ⇒ 按 `C2`（不许依赖业务 crate）它**不可能**住这一层 |
///
/// ⚠ **不许在这一层重建任何一个。** 中转今天连「有哪几个鉴权头」都不知道 ——
/// 那正是 `P16` 买到的东西：它拿到 `AuthSwap` 就照丢照写，没有第二处可以判错、
/// 也没有第二处可以把那份名单凑窄。射程（`clear` 是全集而不是「这一趟要写的那一个」）
/// 由上游选择那侧的判据钉着，住 `creds_guard`。
///
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

#[cfg(test)]
#[path = "../../../tests/comms/outward/server_tests.rs"]
mod tests;
