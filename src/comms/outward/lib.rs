//! `K-H1` 甲半：**HTTP 中转 · 搬字节**。纯基础设施 —— 它不懂任何 agent 的语义。
//!
//! # 🔴 常驻后端这**一个进程同时承载中转与上游选择**，而两者的判断口必须分开
//!
//! 〔第五条 🔴〕
//! 中转只有一层（搬字节）；原「层 2（账号）/ 账号层」改名**上游选择**〔散文墓碑〕：
//! 账号域里按号决定这一发的上游与凭据的那个口（订阅号 → 官方 ＋ claude 自己的登录；API 号 → 自选 URL ＋ key）。
//!
//! | 件 | 是什么 | 归哪 | 代码里叫什么 |
//! |---|---|---|---|
//! | **中转** | HTTP 把上游的 `text/event-stream` **逐块转回下游**。HTTP 层存在的**唯一目的** | 通信层 · 面 B | `relay` |
//! | **上游选择** | 这个账号有没有第三方 key、要不要改写端点、拒不拒 | **后端 · 账号域** | `accounts::upstream_select` —— ❌ **不许叫「中转」** |
//!
//! **进程可以是一个**（就是常驻后端这一个），但**问句不许合并**：
//! 「中转起来了吗」问的是中转；「这个号能不能代入 key」问的是上游选择。
//! 一句话把两者混起来 ⇒ 读到它的人判不出影响面 —— `local_backend_host.rs` 那条
//! `GaveUp` 日志就是因此在步 8 里被拆成了两句。
//!
//! # 🔴 毛病②（**机械**：中英双名并行）的分工 —— 与上面那条**分开治**
//!
//! 两个毛病**刻意不混成一次替换**：混了之后没人能复核
//! 「这一处到底是改对了层，还是只是换了个词」。分工逐字：
//!
//! - **散文 / 界面 / 日志 → 用中文「中转」**，`relay` 这个词**不许裸着出现在散文里**；
//! - **代码标识符 → 用 `relay`**（模块名 · 函数名 · 环境变量 · 文件名），
//!   在散文里引用它们时**加反引号**，那样读的人一眼知道那是个符号不是个词。
//!
//! 现打（步 8 收口）：全仓散文里裸着的 `relay` 只剩 **1 处** ——
//! 当时 `relay/tee.rs` 头注里那条 NDJSON 样例的 `"source":"relay"`（线上字段的值，不是散文）。
//! 那个 NDJSON 落点随独立 `--relay` 删了，那一处随之没了。
//!
//! ⚠ 落在用户机器上的 `relay` 字面量今天只剩**一个**：`CCM_RELAY_PORT`（中转监听的端口 —— 中转的，
//! 名字是对的；两个 workspace 间**今天不由任何东西对拍**，见 `payload.rs` 那句逐字）。
//! 先前这里还列着两个「明写不许改」的：凭据文件的路径旋钮与凭据文件名 ——
//! 它们是**上游选择**的（读那份文件的是上游选择），账号就账号、中转就中转，不留兼容读旧名，
//! 今天文件叫 `apikey-credentials.json`、住家里（另指它位置的那个变量删了，位置只跟着家走）。盘上那份旧文件由用户一次性挪名。
//!
//! ⚠ **〔，本轮搬了〕** 上一版这里逐字写着「本层里真正属于上游选择的住户
//! 是 `creds.rs` 与 `table.rs`……**本轮不搬**」。它们先搬进 `relay/accounts/`，2026-09-24 再搬出中转层、住 `src/backend/accounts/`，
//! 与 `impl Destinations`（那张决策表的唯一实现）、热重载同一层。
//! ⚠ 下面这三行记的是搬出之前那一拍的状态，**今天已不成立**（`relay/` 里一份上游选择文件都没有，
//! `upstream_selection_guard` ㈡ 零命中）：「它们**仍在 `relay/` 这棵目录树下** —— 那是写区边界，不是设计终点：
//! 目标树要它们**出 `relay/`**，那一步归改名/归属那一路。」
//!
//! # 它做四件事（`K9` 裁定二那四条硬要求，逐字）
//!
//! 1. **一个进程** —— 一个监听面服务 N 个会话，不是每会话一个。
//! 2. **按路径前缀分流** —— 路由键塞在 base URL 的路径里
//!    （`/s/<agent>/<account>/<真路径>`；`<account>` 那一段是 `K-H2` 加的，
//!    理由与代价逐条住 `route.rs` 头注；先前的 `<key>` 段退役，流标签取自请求头）。
//! 3. **逐块透传绝不缓冲** —— 上游每给一块就立刻写下游并 flush，从不攒整个响应体。
//!    这是本方案**唯一真正的技术点**：缓冲了 TUI 会卡住不出字。
//! 4. **一边流回 CLI 一边 tee** —— 同一批字节既原样写回下游，又抄一份进 tee 流。
//!
//! # 三条**有意的偏离**，别当成移植漏了
//!
//! ## ㈠ tee 不落文件，落宿主交下来的 **tap 口**
//!
//! `裁-2`（PM 08-25）要 tee 走「每次响应一个新文件（`O_EXCL` 新建）」；只读护栏的默认层把标准库里「新建文件」的每一种
//! 写法都禁掉了，绕开它的三条路（把调用放进白名单模块 / 类型别名改写 / 直走 `libc`）都是「让护栏变瞎」⇒ 不落文件。
//! 中转住常驻后端进程里（`listen::host`，本机与远端同形），那个进程的 stdout 在 stdio 载体上**就是 wire** ⇒ 也不落 stdout：
//! 落宿主交下来的 tap 口（`TeeSink::to_port`，宿主 `crate::stream::tap`），事件变成 `tap` 帧走 wire 自己那条有界通道。
//! 挡着它的判据：`host_tests::the_production_wiring_hosts_the_relay_and_never_writes_tee_lines_to_stdout`
//! （真子进程走生产接线，转发之后 stdout 上零 tee 输出）。
//! 先前独立的 `--relay` 进程把 tee 写成 NDJSON 行落自己的 stdout（零消费者）—— 那一形随它删了。
//!
//! ## ㈡ 每件**不带 `t_ns`**（`裁-3`）
//!
//! 参考实现每行带一个跨响应单调的 `t_ns`（`Instant::now` 的等价物），而
//! `Instant::now` 在零定时器护栏的禁用清单里**且没有登记口**。⇒ 第一刀不带它。
//! **如实说这丢了什么**：`t_ns` 在参考实现里是一条**修过的 bug**（跨响应单调），
//! 不带它 = 暂时放弃那个修复面 ⇒ **任何要按时间对齐 tee 事件的下游都得等这条解锁**。
//! 解锁条件：真出现需要它的消费者时，单独立一件去处置那个守卫。
//!
//! ## ㈢ 上游 `Accept-Encoding` 收窄成 `identity`，不是 `gzip`
//!
//! 参考实现收窄成 `gzip` 并自己流式解压，因此要一条解压依赖。Rust 重写改成收窄成
//! `identity`：tee 侧拿到的直接是明文 SSE，**整条解压依赖不需要**。
//! 代价是上游那一跳的**响应体**不再压缩（真实带宽，不是回环）。
//!
//! 〔2026-09-25 本路裁：**保留**〕审计 B §3 说这是「为一个没人读的功能放弃了压缩」——tee 今天有了第一个消费者
//! （`tap` 帧 → 前端活卡），那条前提没了。保留的理由（读数与全文住仓外）：
//! 1. `Accept-Encoding` 只管**响应**那一半；请求体（整份上下文）由 claude 自己决定、中转原样搬。本机语料按 `message.id`
//!    去重的 `usage`：请求侧 token : 响应侧 token ≈ **330 : 1** ⇒ 就算 SSE 压 5–10 倍，省下的不到这一跳字节的 0.3%；
//! 2. 上游若对 `text/event-stream` 做 gzip，压缩器按块攒字节会推迟 token 到下游的时刻 —— 伤的正是本层唯一的技术点
//!    「逐块透传绝不缓冲」；这一形要真上游才量得到，本仓零读数；
//! 3. 恢复压缩要在 tee 那一份上流式解压并在解压流上重证「不缓冲」（立件的事），不是顺手的；
//! 4. 可逆：改动只在 `server.rs::render_upstream_request` 那一行 ＋ tee 解码一处。
//!
//! # 还有一条**没做**的，写在这里免得被读成「做了」
//!
//! ⚠ 订正〔回修轮之六 08-26〕：这一节先前的第一条逐字写着「**不设任何读写期限**」——
//! **那句话今天是假的，已收口**。经过：`阻-3(D3)` 的后半段（期限）在回修轮之五因为
//! `no_timer_guard.rs` 不在写区而交回 PM（§8.20.4），PM 收 R5 时**扩了写区一格**并派了 R6（§8.21.3）。
//! 今天的形状是**顶不满 + 拒绝有声 + 顶住的会自己散**，三样齐了：
//! - 上界与出声：`listen.rs::INFLIGHT_CONNECTIONS`（超了回 503；从 `server.rs` 挪来）；
//! - 会自己散：`listen.rs::DOWNSTREAM_DEADLINE`（本机对端，**30 秒**）与
//!   `listen.rs::UPSTREAM_DEADLINE`（模型在想是正常的，**600 秒**，就是这里记的那个参考数），
//!   两条 socket 的**读写两个方向**都装。
//!   ⚠ 这两个**值**先前住 `server.rs` 与 `upstream.rs`（中转）——
//!   按（「**期限值**全部由后端交给它」）搬去了监听面。
//!   **装它们的那两手仍在中转**（`server::apply_downstream_deadline` / `upstream::connect`，
//!   都改成收入参）⇒ 「值归后端 · 执行归通信层」。
//!   写法上没有走 `Duration::from_secs`（它在零定时器护栏的**禁用清单**里），
//!   走的是 `Duration::from_millis` + **往 `no_timer_guard.rs::REGISTERED_DURATION_USES` 加两行**
//!   并逐条写明「socket 期限是一次阻塞的上限，不是周期性唤醒」。
//!   ⚠ 往一张「**恰好相等**」的表里加行是**收紧**（逼你说清那个 `Duration` 干什么用），
//!   与放宽那条断言方向相反 —— 断言本身**一个字节都没动**。
//! - ⚠ **仍然没有端到端等满 30 秒的判据**（那要跑 30 秒）：这个结论由两格拼出来 ——
//!   `both_peers_really_carry_their_read_and_write_deadline_on_the_socket`（那个数真装上了）
//!   + `a_socket_deadline_makes_a_half_open_read_return_instead_of_wedging_the_thread`（装上之后读会报错返回、无人重试）。
//!
//! - **`connect` 那一步没有期限**。`upstream::connect` 用 `TcpStream::connect((host, port))`。
//!   故意不做：读/写是**真无界**（对端不发就永不返回），而 connect 有内核 SYN 重试上限
//!   与解析器自己的上限兜着 —— **有上限**。⚠ 那个上限具体多少**我没量** ⇒
//!   只敢说「不是无界」，不敢说「够小」。真要收紧得先量它（且 `connect_timeout` 要 `SocketAddr`，
//!   得先自己解析），是另一件活。
//! - **不碰凭据**。请求头原样转发，**一个都不落进 tee、不落进日志**。
//!   ⚠ 订正〔回修轮之五 08-25，D3 `重要-3(D3)`〕：这一条先前引 `K11 裁定一`「不记录 ≠ 不持有」
//!   当依据，而 `K11 裁定一` **08-25 已被改判**，现行正文逐字要的是
//!   「中转在转发时**替换** `Authorization` 头」。⇒ 那条裁定支持的是「不记录」，
//!   **不支持**「原样转发」。准确说法两句：①本件（甲半·搬字节）**原样转发、不替换**；
//!   ②「换头」是**下一件**的活，而 `K11 裁定一` 自己写着硬前置，逐字「**没有那条新判据之前，
//!   不许把 key 接进中转**」。逐条订正住 `server.rs::render_upstream_request` 的头注。

//! # 边界：中转认识谁
//!
//! 中转是一个独立的 crate，普通依赖只有 `copy-core` · `relay-route-core` · `upstream-url-core` · `rustls` · `webpki-roots`
//! （边界判据按依赖图判，第一方业务 crate 一个都不许出现）⇒ 后端的观测面 · 控制面 · 适配层 · 调用口它在编译期就够不着。
//! 它要的东西都由宿主递进来（`Relay::new`：上游选择那只手 · 钥匙 · tee 落点 · 两个期限值）。
//! 对外的口就是本文件里 `pub` 的那几样，逐条登记在后端 `layering_guard::RELAY_EXPORTS`（两向相等）。

// 上游选择不在这里：它住后端 `src/backend/accounts/`（用户逐字「中转层不要有账号, 账号就账号中转就中转」）。
// 绑口 · 在途上界 · 两个期限值 · 钥匙落盘也不在这里：都归后端（`src/backend/relay/`），中转只收它交下来的。
mod door; // 中转口的门：钥匙的形状 · 进门三问（Origin / Host / 钥匙）· 定长比对
mod framer; // 唯一的增量分帧器（游标，不 drain）—— `tee.rs` 与 `http1.rs` 是它的两个客户
mod http1;
mod route;
mod server; // 一条下游连接的一来一回（解析 → 问上游选择 → 连上游 → 逐块透传 ＋ tee）
mod tee;
mod upstream;

/// 后端起中转那一侧用的：钥匙由后端读好交进来（[`Key`]）· 一条连接交给 [`serve_one`] ·
/// 在途满了由后端回一声 [`refuse_busy`] · 接下连接先装下游期限（[`apply_downstream_deadline`]，值由后端给）。
pub use door::{tokens_match, Key, Keys};
pub use server::{apply_downstream_deadline, refuse_busy, serve_one, Relay};

/// tee 的第二个落点的口与它交出去的那件事（宿主 `stream::tap` 实现口、把事件转成 `tap` 帧）；
/// [`TeeSink`] 是中转手里那个落点（后端起中转时用 `TeeSink::to_port` 把口交进来）。
/// 字段语义与「位置号原位说缺口」住 `tee.rs` 头注「第二个落点」。
pub use tee::{TapBody, TapEvent, TapPort, TeeSink};

// ══════════════════════════════════════════════════════════════════════════
//  层间契约—— 中转问一句，上游选择答一句，**中转不做任何判断**
// ══════════════════════════════════════════════════════════════════════════

/// 「这一段当得了路由段吗」—— 上游选择装表时判账号 id 用的与本层切键用的是**同一个谓词**
/// （`route.rs` 头注逐字论证过为什么不许各写一份）。同上，经这里交出去。
pub use route::segment_is_safe;
/// 中转的**传输原语**：一行的上游是什么。上游选择解析它、焊进行里、原样交回（`Destination` 带着它）。
/// ⚠ 它**经这里**交给上游选择（`upstream` 模块本身仍是私有的）—— 契约面上的每一样都住这个文件。
pub use upstream::{fetch, Base, Fetched};

/// 两个前缀 = 两种模式（「为什么用两个前缀而不是一个哨兵段」）。
///
/// **意图写在线上**，两条路不可能互相静默降级：`/s/` 永远 fail-closed（表里没这一行
/// 就是 404），`/t/` 从来不代入 auth。用一个前缀 ＋「查不到就直通」的话，
/// **一次账号段打字错误**就会从「该代入却没代入（loud 404）」变成
/// 「静默用了下游自己的凭据」—— 那正是 `KH2` 在治的病的镜像。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `/s/` —— 代入模式：表里必须有这一行，没有就是 404。
    Substitute,
    /// `/t/` —— 直通模式：中转**永不**代入 auth。
    Passthrough,
}

/// 中转手里的键：两个**不透明**段，中转不知道它们是什么意思。
///
/// # 🔴 条 48：为什么叫 `seg1`/`seg2` 而不是 `agent`/`account`
///
/// **中转的类型里不出现业务名。** 它要的只是
/// 「路径的第 1/2 段」，原样交给策略那半 —— 那两个业务词**只出现在上游选择的实现里**
/// （`accounts/`，它自己把 `seg1`/`seg2` 读成 agent 与账号）。
/// ⇒ （搬字节那层不许出现业务词）**一字不改、豁免仍为零、能力零损失**。
/// 原先那个签名 `resolve(mode, agent, account)` 必然命中 `C1`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteKey {
    pub seg1: String,
    pub seg2: String,
}

/// 地址里紧跟第 2 段的来处段（`~<来处>[~<父>]`，语法住 `relay_route_core::parse_origin`）。中转**不解释**它：
/// 原样放进 [`Ask::origin`] 交给上游选择（会话血缘在那一层认）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteOrigin {
    pub token: String,
    pub parent: Option<String>,
}

/// tee 那条流的身份：**一个流标签**。**不用业务名、不带路由键**（① 不问账号）——
/// `stream` 取自请求自己带的那个头（[`Destinations::stream_label_headers`]），不是路径段；没有 ⇒ 空串。
/// 先前还带着路由键（NDJSON 行那一形要它），那一形随独立 `--relay` 删了。
///
/// 借用形（不是 `String`）：每条请求两次调用都不该为此多分配一次。
#[derive(Debug, Clone, Copy)]
pub(crate) struct StreamId<'a> {
    pub(crate) stream: &'a str,
    /// 第二个标签：名单（[`Destinations::stream_owner_headers`]）里第一个出现在请求里、值过段闸的头的值；没有 ⇒ 空串。中转不解释它。
    pub(crate) owner: &'a str,
}

/// 上游选择算好的**换头材料**。中转拿到只做两件事：**照丢 ＋ 照写**。
///
/// # 🔴 它为什么存在（2026-09-22）
///
/// 上一版这一格是 `(Option<&SecretKey>, AuthStyle)` —— **两个都是 `creds-core` 的类型**，
/// 而 `creds-core` 在 `C2` 名单里被特意加粗。
/// ⇒ 「收一张表」在**句柄**那一层早就收完了（中转手里只有一个 `Arc<dyn Destinations>`），
/// 而**表里元素的类型**还从业务 crate 来 ⇒ `C2` 照咬。本类型是那一格的收口：
/// 中转的类型面上从此只有 `&'static str` 与 `&str`。
///
/// # 两个字段各买什么
///
/// - [`Self::clear`]：换头**之前**要整条丢掉的下游头名。
///   🔴 **它是全集，不是「这一趟要写的那一个」** —— 这是一条**安全**性质：
///   客户端自带的 `Authorization` 必须被丢掉，不许和我们写上去的 `x-api-key` 并存
///   （HTTP 允许同名头出现多次 ⇒ 上游看见两个鉴权头时谁赢**没有定义**）。
///   把它缩成「只丢要写的那一个」是**行为变更**，不是优化。
/// - [`Self::write`]：这一趟要写的那一条，`(头名, **完整**头值)`。
///   值里的前缀（`Bearer ` 那一类）**已经由上游选择拼好** ⇒ 中转不知道也不需要知道
///   「这一家上游要什么形状的鉴权头」。
///
/// # ⚠ 买不到
///
/// - **不买「那把 key 是对的」** —— 中转连它是不是一把 key 都不知道，它只看见一个串。
///   「A 的端点配 B 的 key」由「一次只给一个 `Destination`」挡着，不由本类型挡。
/// - **不买「`clear` 真的是全集」** —— 那由上游选择那侧的判据钉（射程不许缩那一条）。
///   本类型只保证**中转没有第二处可以自己凑一份名单**。
#[derive(Debug, Clone, Copy)]
pub struct AuthSwap<'a> {
    /// 换头前要整条丢掉的下游头名（**全集**，比对走 `eq_ignore_ascii_case`）。
    pub clear: &'a [&'static str],
    /// 这一趟要写的那一条：`(头名, 完整头值)`。`None` = 剥掉之后一个头都不写。
    pub write: Option<(&'static str, &'a str)>,
}

/// 上游选择给中转的答案。**中转拿到就照做**。
///
/// ⚠ 借用形（`&Base` / [`AuthSwap`] 里那个 `&str`）而不是按值 —— 理由整段写在
/// [`Destinations::resolve`] 的头注里（明文那个串是上游选择在自己的锁里现拼的，
/// 按值返回就得把它 `String` 化一份带出来 —— 那是**多一份明文**，不是搬家）。
pub enum Destination<'a> {
    /// 发到这个上游，**下游送来的 auth 头原样转发**。中转手里没有任何 key。
    ///
    /// `tag`：上游选择给这个去处贴的不透明标签；中转不解读，回包头到了交 [`Destinations::observe`] 时原样递回
    /// （同一条流上并发的几发各自认得出是哪个去处答的）。
    Passthrough { upstream: &'a Base, tag: &'a str },
    /// 发到这个上游，**剥掉下游 auth、按上游选择交下来的那份材料换头**。
    ///
    /// ★★ 买到的那一格就在这里：上游与鉴权材料是**同一个变体的两个字段**，
    /// 一次请求只拿到一个 `Destination` ⇒ 「A 的端点配 B 的 key」**在这条路上凑不出来**
    /// （要凑得先有两个 `Destination` 同时在作用域里，而 `resolve` 只给一个）。
    ///
    /// # 🔴 与那段伪码的**第二处形状差异**：鉴权那一格是 `AuthSwap`，不是一把 key
    ///
    /// 规格那个枚举有**两**种鉴权处置（原样转发 / 代入一把），而**今天盘上有三种** ——
    /// 第三种是「把下游那几份鉴权头丢掉，而且一个头都不写」（`K-R1` 的「本地部署那一格」）。
    /// 它不是 `Passthrough`（那一支逐字是「原样转发」），也没有 key 可代入。
    /// ⇒ 把 `Substitute` 的含义写准：「**这一行的鉴权由表说了算**（先把下游那份剥掉）」，
    /// [`AuthSwap::write`] 给 `None` 表示「剥掉之后什么都不写」。
    /// 整张对照表住 `accounts` 那一侧那个派发函数的头注。
    Substitute {
        upstream: &'a Base,
        auth: AuthSwap<'a>,
        /// 换掉整份请求体（`None` ＝ 原样发下游送来的那一份）。中转不解读它，只照发、照算长度。
        body: Option<&'a [u8]>,
        tag: &'a str,
    },
    /// 这条路由不成立 ⇒ 中转回这个状态码，**一个字节都不发上游**。
    Refuse {
        /// 一律 4xx（我们拒的是 4xx，5xx 只留给上游那侧）。
        status: &'static str,
        /// 原因头的值（ASCII 短词，`server::REASON_HEADER`）：下游据它分清「中转拒的」与「上游原样回的同一个码」。
        reason: &'static str,
        /// 为什么拒，一句人话：进响应体第二行（出声，不只给一个码）。
        why: &'static str,
    },
    /// 这一发不发上游，回这份现成的回包（状态行 · 头 · 体都由上游选择给，中转不解读，**一个字节都不发上游**）。
    /// 中转只补 `Content-Length` · 原因头 · `Connection: close`。回包头不交 [`Destinations::observe`]（上游没答过）。
    Reply {
        /// 状态行里状态码那一截（如 `429 Too Many Requests`）。
        status: &'a str,
        /// 原因头的值（ASCII 短词，同 [`Destination::Refuse::reason`]）。
        reason: &'static str,
        headers: &'a [(String, String)],
        body: &'a [u8],
    },
}

/// 这一发请求里中转交给上游选择看的那几样（只读）：流标签 ＋ 整份请求体 ＋ 请求头的名字。
#[derive(Debug, Clone, Copy)]
pub struct Ask<'a> {
    /// 请求头里取出的流标签（[`Destinations::stream_label_headers`]）；没有 ⇒ 空串。
    pub label: &'a str,
    /// 地址里的来处段（[`RouteOrigin`]）；没有 ⇒ `None`。
    pub origin: Option<&'a RouteOrigin>,
    /// 下游送来的整份请求体（中转先收全了才问去处）。
    pub body: &'a [u8],
    /// 这一发带的请求头的**名字**（原样大小写，只有名字、没有值）：同一家按带没带某个头选上游时看它。
    pub names: &'a [&'a str],
}

/// 上游回包头读完那一刻中转手里的东西。
#[derive(Debug, Clone, Copy)]
pub struct Heard<'a> {
    /// 答的那个去处的标签（[`Destination::Passthrough::tag`] 原样）。
    pub tag: &'a str,
    /// 状态行里的三位数字；读不出 ⇒ 0。
    pub status: u16,
    pub headers: &'a [(String, String)],
}

/// 上游选择对中转的**唯一**一个口。
pub trait Destinations: Send + Sync {
    /// 上游选择自己把 `seg1`/`seg2` 读成 agent 与账号 —— 那两个词只出现在它的实现里。
    ///
    /// # ⚠ 与那段伪码的**一处形状差异**，理由写死在这里
    ///
    /// 规格写的是 `fn resolve(&self, mode: Mode, key: &RouteKey) -> Destination;`
    /// ——**按值返回**。今天做不到，挡着的是一条**刻意的**性质：`Substitute` 要带那条
    /// 已经拼好的头值，而那个串是上游选择**在自己的锁里现拼**的（它的原料
    /// `SecretKey` 刻意没有 `Clone` —— `K-H2a`：少一条能复制明文的路就少一个出口）。
    /// 按值返回就得把那个串再 `String` 化一份带出锁外 —— 那是**多一份明文**，不是搬家。
    ///
    /// ⇒ 改成**借用 ＋ 一次性访问者**。买到的东西一样：答案仍是同一个枚举、
    /// 仍然是**一次请求只拿到一个**（`§6` 第 1 行要的正是这一句），
    /// 只是上游选择在自己的锁里把它递过去一次。
    ///
    /// # 实现方的两条硬约束
    ///
    /// 1. **`act` 恰好被调用一次**（三支各一次）。少调 = 中转既没连上游也没回状态码，
    ///    下游会拿到一个没有任何 HTTP 响应的 FIN —— 那正是 `阻-3(D3)` 点名的静默拒绝。
    /// 2. **`act` 里不许做流式转发**。上游选择的锁（`RwLock` 写优先）活到 `act` 返回为止；
    ///    把 `pump` 搬进来 = 「配一次 key」会被堵在最长那条在飞流后面（`D2 阻-4`）。
    ///    钉这一条的判据：`table_guard::the_upstream_selection_lock_does_not_outlive_the_streaming_pump`。
    fn resolve(
        &self,
        mode: Mode,
        key: &RouteKey,
        ask: &Ask<'_>,
        act: &mut dyn FnMut(Destination<'_>),
    );

    /// 上游的回包头读完那一刻（还没往下游写一个字节）交回来看一眼：**只读**，答什么都不改中转的做法。缺省 ⇒ 不看。
    fn observe(&self, _mode: Mode, _key: &RouteKey, _ask: &Ask<'_>, _seen: &Heard<'_>) {}

    /// [`Self::observe`] 之后紧接着问：要不要换一个去处、用同一份请求体再发一次（下游一个字节都还没收到）。
    /// 要 ⇒ 调一次 `act`（约束同 [`Self::resolve`]）；不要 ⇒ 不调（中转把手上这个回包原样往下游送）。
    /// `tried` ＝ 这一发已经发过的各个去处的标签（先发的在前，最后一个就是 `seen.tag`）；上限由实现方定，
    /// 中转另有一个硬上限防打转。缺省 ⇒ 不换。
    fn retry(
        &self,
        _mode: Mode,
        _key: &RouteKey,
        _ask: &Ask<'_>,
        _seen: &Heard<'_>,
        _tried: &[&str],
        _act: &mut dyn FnMut(Destination<'_>),
    ) {
    }

    /// 哪几个请求头给流打标签（第一个在请求里、值过段闸的那个）。中转不知道它们是谁的什么头，
    /// 只照这份名单取 —— 会话 id 归 agent 自己，启动器不往地址里塞（路由第 3 段随之退役）。
    fn stream_label_headers(&self) -> Vec<&'static str>;

    /// 哪几个请求头给流打第二个标签（同上一条的取法；中转同样不知道它们是谁的什么头）。
    fn stream_owner_headers(&self) -> Vec<&'static str>;

    /// 每个请求要记下「在不在」的那几项：（头名, 逗号列表里某一项的前缀）。中转不知道它们是什么意思，
    /// 只把「这条流的这一发带没带」交给 tap 口（[`tee::TapPort::note_marks`]），值本身不出去。缺省 ⇒ 不记。
    fn request_marks(&self) -> Vec<(&'static str, &'static str)> {
        Vec::new()
    }
}

/// 中转起来那一刻，上游选择交给中转的**另一只手**（`host` 的启动路径）。
///
/// # 它为什么存在（「中转层里没有账号」那一刀的前置）
///
/// 先前 `listen.rs` 在启动路径上**直呼**上游选择的五个名字（读哪个上游旋钮 · 解析默认上游 ·
/// 读凭据装表 · 起上游选择 · 接热重载）—— 请求路径上中转只认 [`Destinations`] 一个口，
/// 启动路径上它却认识上游选择的整套装配。⇒ 把那五个名字收成这两步：中转只知道
/// 「起监听**之前**问一次行不行」与「起监听**之后**要一个 [`Destinations`]」，
/// 至于那里面是账号、凭据还是别的什么，**它不知道**。
///
/// ⚠ 两步而不是一步，是**顺序**逼的（`listen::run_with` 头注那两条退 2）：
/// 配置读不懂要在**绑端口之前**就退（一个字节都不监听）；而装表要在**绑端口之后**
/// （否则端口起不来那条支会先把与它不相干的东西印出来）。
pub trait Startup: Sync {
    /// 起监听**之前**：拿这份取值器验上游选择自己的配置。认不出 ⇒ `None` ⇒ 中转出声并退 2。
    ///
    /// ⚠ 取值器是**注入的**，中转原样递过来：上游选择要读哪几个变量，中转连名字都不知道。
    fn check(&self, get: &dyn Fn(&str) -> Option<String>) -> Option<Box<dyn Ready>>;
}

/// [`Startup::check`] 过了之后手里那一份。
pub trait Ready {
    /// 起监听**之后**、进接受循环**之前**：装好、把该说的话说到 `out`，交出 [`Destinations`]。
    fn into_destinations(
        self: Box<Self>,
        get: &dyn Fn(&str) -> Option<String>,
        out: &mut dyn std::io::Write,
    ) -> std::sync::Arc<dyn Destinations>;
}

/// 后端那几条组合判据（真中转 ＋ 生产段的上游选择 / tap / 内存探针）要够到的内部件。
/// 只由后端的测试档开（`test-support`），发布构建里没有这一块。
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod test_support {
    pub mod door {
        pub use crate::door::*;
    }
    pub mod framer {
        pub use crate::framer::*;
    }
    pub mod http1 {
        pub use crate::http1::*;
    }
    pub mod route {
        pub use crate::route::*;
    }
    pub mod server {
        pub use crate::server::*;
    }
    pub mod tee {
        pub use crate::tee::*;
    }
    pub mod upstream {
        pub use crate::upstream::*;
    }
}
