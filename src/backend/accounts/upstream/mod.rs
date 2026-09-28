//! **上游选择**（apikey 端点改写那一块）：`resolve` 那张决策表的**唯一住址**（`设计/20 §3.1`）。
//! 〔V114 · 2026-09-25〕原叫「层 2 / 账号层」、模块名 `apikey`，改名上游选择 / `upstream`；纯命名，行为不变。〔散文墓碑〕
//!
//! 〔`A3` 第二波 · 2026-09-24〕从 `accounts/` 挪进子目录 `accounts/upstream/`：`accounts/` 是账号**域**，
//! 它下面「挂在中转上当上游选择的这一块」与「账号隔离工具的查询」（`accounts/iso.rs`）是两件事，
//! 不共用一张登记表（用户「账号就账号, 中转就中转」）。两块互不引用，由
//! `upstream_selection_guard::the_two_halves_of_the_account_domain_do_not_reference_each_other` 钉着。
//!
//! # 它知道什么、不知道什么
//!
//! | | |
//! |---|---|
//! | **知道** | 账号模型 · `apikey-credentials.json` · 上游 · `auth_style` · 热重载 · 哪个账号走哪个模式 |
//! | **也管** | 〔RM1a〕这台机器上那份凭据文件的写与文件级读（[`file_face`]，帧面两条命令）—— 上游选择自己的状态 |
//! | **不知道** | HTTP 怎么发、字节怎么泵 —— 那是中转（`server.rs` / `listen.rs` / `http1.rs`） |
//!
//! ⇒ **「agent」与「账号」这两个业务词只在这一层出现。** 中转手里只有
//! `RouteKey{ seg1, seg2 }` 两个不透明段（条 48 · `01 §2.1 C1`）——
//! 把它们读成 agent 与账号是**本模块**的事，就在下面 [`Accounts::resolve`] 里。
//!
//! # 🔴 表的键是 agent ＋ 账号，默认上游每 agent 一行，未登记直接拒〔条 49 / 条 59 / 条 60〕
//!
//! 先前这里逐字自陈两笔欠账：「`seg1`（agent）今天还不承重：查表仍然只按 `seg2`」·
//! 「`DEFAULT_UPSTREAM` 还是一个进程级的默认」。后果是具体的：**claude 的 3 号账号与
//! codex 的 3 号账号是同一行**，而它是「让所有会话都过中转」那一刀的硬前置 ——
//! 不先拆，codex 会话一注入就**每一发都错发到 Anthropic**。今天三件一起落：
//!
//! | 改什么 | 今天是 | 住址 |
//! |---|---|---|
//! | 表的键 | `(seg1, seg2)` 两段都是键，一段都不省 | `table::RoutingTable::lookup` |
//! | 默认上游 | **每 agent 一行**：`agent → (环境旋钮, 内置默认)`；进程级的那个常量**整删**；〔NT2 · V25〕那一格住适配层 | `agents::Adapter::upstream`（经 `agents::default_upstreams` 读） |
//! | 未登记的 agent | `/t/` 无行 ⇒ **502**；`/s/` 无行 ⇒ 404（不变）。**不回落到任何一家** | [`decide`] 最后那一支 |
//!
//! ⚠ **「未登记 ⇒ 502」不是没做完，是照 `§3.1` 第 4 行那条 🔴「不许回落到某一个写死的常量」
//! 做的 fail-closed** —— 先前它对**每一个** `seg1` 都成立（那张表还不存在），今天只对
//! 表里没有的那几家成立。别把它改成「查不到就透传」：透传到哪一家？那正是要拆掉的那个回落。
//!
//! ⚠ **今天只登记了一家**（`claude-code`，适配层那一格）。codex **刻意没登记** —— 它的默认上游
//! 是哪一个、它认不认 base URL 的覆盖，本仓**零证据**（`C7`：不起真 agent），
//! 而把一个猜的值写进这张表，就是把「未登记直接拒」换成「静默发去一个猜的地方」。
//! 登记它的那一天，改的只是注册表里 codex 那一行的 `upstream` 那一格，**形状不用改**。
//!
//! # ⚠ `设计/20 §7` 步 3 的「怎么验」那一栏与 `§3.1` 第 4 行 —— 今天**不再互斥**
//!
//! `§7` 步 3 的验收逐字是「`/t/` ＋ 表里无行 ⇒ **真的发到默认上游**且 auth 头逐字节原样」，
//! 而 `§3.1` 第 4 行（拍板 (b) 甲，比 `§7` 新）逐字加了「**不许回落到某一个写死的常量**」。
//! 先前两句在「每 agent 一行的默认上游」那张表落地之前不可能同时成立，本层取了后者（恒 502）。
//! 那张表今天落了 ⇒ **两句同时成立**：登记过的 agent 走**它自己那一行**（不是某一个进程级常量），
//! 未登记的仍是 502。两格的判据住 `tests/backend/relay/table_tests.rs`
//! （`decide` 被直接问，不经网络）；`wire_golden` 那一格用的 `seg1` 是一个**未登记**的名字，
//! 仍钉着 502 那一半。

pub(crate) mod creds; // `K-H2a`：从哪儿拿 key（**只读**）+ 读之前查一次权限（上游选择搬家带过来的）
                      // 〔RM1a · 第四波〕这台机器上那份凭据文件的**帧面读写口**（`apikey-key-set` / `apikey-read`）。
                      // 上游选择自己的状态文件，不是用户文件 ⇒ `readonly_guard` 第四层登记它，只从 `inbound.rs` 进来。
                      // ⚠ 它**不在**中转那条启动路径上：中转里的上游选择仍然只读（`creds`），写只在流模式的帧面上发生。
pub(crate) mod file_face;
// 〔US1 · 4D〕起会话那一发走哪、注入什么（帧面 `launch-endpoint`）· 界面「这几个号在表里有没有行」（`apikey-routing`）——
// `设计/20 §3.2` 那张表的唯一住址（先前住 monitor `payload::relay_endpoint_for`〔散文墓碑〕）。
pub(crate) mod endpoint;
mod policy; // 热重载（`20 §4`：`accounts/policy.rs`；今天住 `accounts/upstream/policy.rs`）
pub(crate) mod table; // `K-H2`：路由表 —— 账号段 → **上游与 key 焊死的一个值**

pub(crate) use policy::Reload;

use crate::relay::{AuthSwap, Base, Destination, Destinations, Mode, Ready, RouteKey, Startup};
use creds_core::store::AuthStyle;
use std::io::Write;
use table::{RoutingTable, Row};

/// 凭据文件（`apikey-credentials.json`）里那些行**挂在哪个 agent 名下**。
///
/// # 为什么要有这个值（条 49 拆键之后必然冒出来的一格）
///
/// 表的键改成 `(agent, 账号)` 之后，装表那一步得知道每一行属于谁 ——
/// 而 `creds-core` 那份文件格式**今天没有 agent 这一维**（只有账号 id）。
/// 那份文件是界面上给 **claude-code 的账号**配第三方 key 时写出来的 ⇒ 它的每一行今天都是这一家的。
///
/// 〔US1 · 4D〕它**只有这一份**：先前 monitor 那一侧决定「这次拉起要不要注入」时另写一份（`payload::APIKEY_TABLE_AGENT`〔散文墓碑〕），
/// 由一条跨半边判据现抠字面量对拍；那张决策表搬进本层（[`endpoint`]）之后，读它的只剩本层自己。
/// ⚠ 买不到：「那份文件**将来**会不会装进别家的行」—— 那要文件格式多一维（`creds-core`，
/// 不在本层），那一天本常量整删、换成逐行读出来的 agent。
pub(crate) const CREDENTIALS_FILE_AGENT: &str = "claude-code";

// 〔NT2 · V25〕这里原先是 `AgentUpstream` 与每 agent 一行的默认上游表 `AGENT_UPSTREAMS`〔散文墓碑〕。
// 用户 V25「写死, 跟着适配层」⇒ 那一格搬回 `agents::Adapter::upstream`（claude-code 那一行住 `agents/claudecode`），
// 本层只经 `agents::default_upstreams` 读 —— 形状（每家一行 · 每家一个旋钮 · 未登记即拒）一格不变，只换了住址。

/// 适配层那一格（`agents::Adapter::upstream`）解析之后的样子：`路由名 → Base`。**一个进程一份**，首次装表与每次重载共用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Upstreams {
    by_agent: std::collections::BTreeMap<&'static str, Base>,
}

impl Upstreams {
    /// 读每一家的环境旋钮、解析成 `Base`。**任何一家认不出就是 `None`** ——
    /// 调用方出声并退 2，不跳过那一家、不回落到别家的值。
    ///
    /// ⚠ 取值器是**注入的**（与 `listen::run_reading` 同一条纪律：判据不许去改进程环境）。
    /// ⚠ 为什么是「一家坏了整个起不来」而不是「那一家当未登记」：后者会把一次打错字
    ///   变成「那一家的每一发都 502」，而进程照常跑着、启动日志里只有一行 —— 那是静默降级。
    pub(crate) fn from_env(get: &dyn Fn(&str) -> Option<String>) -> Option<Self> {
        let mut by_agent = std::collections::BTreeMap::new();
        // 〔NT2 · V25〕默认上游只查适配层（`agents::Adapter::upstream`）。
        for a in crate::agents::default_upstreams() {
            let raw = get(a.env);
            let base = Base::parse(raw.as_deref().unwrap_or(a.fallback)).ok()?;
            by_agent.insert(a.route_id, base);
        }
        // 凭据文件那一家必须登记过，否则那份文件里的行**没有默认上游可取**。
        // 这是一条构造期的事实，由 `table_tests` 那条相等断言钉着；这里只是不让它静默成立。
        by_agent
            .contains_key(CREDENTIALS_FILE_AGENT)
            .then_some(Self { by_agent })
    }

    /// 这一家的默认上游。**未登记就是 `None`**，不许拿别家的顶上。
    pub(crate) fn of(&self, agent: &str) -> Option<&Base> {
        self.by_agent.get(agent)
    }

    /// 凭据文件那一家的默认上游。构造时已经查过它在 ⇒ 这里拿得到。
    fn of_credentials_file(&self) -> &Base {
        &self.by_agent[CREDENTIALS_FILE_AGENT]
    }
}

/// 〔RL1 · V107〕**常驻后端里的中转装配口**：中转的 `host` ＋ 上游选择那只手。`main.rs` 流模式那一处调它。
///
/// # 它为什么住上游选择（而不是中转）
///
/// 装配要同时叫得出两层的名字。依赖方向只许上游选择 → 中转（上游选择本来就用中转的契约类型），
/// 反过来就是「中转层里有账号」—— 那正是用户 2026-09-24 那句话要拆掉的。
/// ⇒ 中转的 `host` 收一个 `&dyn Startup`，本函数把 [`Boot`] 递进去；中转的生产段里
///   **一个上游选择的名字都没有**（`relay::upstream_selection_guard` ㈢ 零命中）。
///
/// 交没交端口、起没起来由中转答（`relay::Hosted`）；本函数一行逻辑都没有，只做接线 ——
/// 取值器是**真环境**。回一句给宿主日志看的话。
/// 〔DEL〕先前还有一条 `--relay`（独立中转进程）的装配口，随那一形删了；中转只剩这一个宿主。
/// 〔TAP · V124〕tee 的落点是进程级那一个 tap 口（`crate::tap::port`）—— 本函数只递，上游选择不碰它交出去的任何一件事
/// （`设计/20 §11` I2：② 不碰响应体）。
pub fn host_relay(home: &std::path::Path) -> String {
    crate::relay::host(&|k| std::env::var(k).ok(), home, &Boot, crate::tap::port()).to_string()
}

/// 上游选择在中转启动路径上交给中转的那一只手（[`Startup`]）。
///
/// ★ 中转**叫不出**它的名字：[`host_relay`] 把它递进中转的 `host`，中转只见得到
/// `Startup` / `Ready` / `Destinations` 三个契约口。钉这一条的判据：`upstream_selection_guard`（㈢ 零命中）。
pub(crate) struct Boot;

impl Startup for Boot {
    fn check(&self, get: &dyn Fn(&str) -> Option<String>) -> Option<Box<dyn Ready>> {
        Upstreams::from_env(get).map(|u| Box::new(u) as Box<dyn Ready>)
    }
}

impl Ready for Upstreams {
    /// 读一次凭据 → 装表 → 出声 → 起上游选择 → 接上热重载。
    ///
    /// ⚠ 这五步先前**长在中转的 `run_with` 里**（逐个直呼本层的名字）；今天是本层的私事。
    fn into_destinations(
        self: Box<Self>,
        get: &dyn Fn(&str) -> Option<String>,
        home: &std::path::Path,
        out: &mut dyn Write,
    ) -> std::sync::Arc<dyn Destinations> {
        let (table, creds_path, stamp) = load_credentials(get, home, &self, out);
        // `D1 阻-2`：把重载源接上 —— 没有这一行，那张表就是一张**启动快照**，
        // 用户在界面上配完 key 必须重启中转才生效（而不重启的症状是一个静默的 404）。
        std::sync::Arc::new(
            Accounts::new(table, *self).reloading_from(Reload::new(creds_path, stamp)),
        )
    }
}

/// 上游选择的实现：一张**可重载**的路由表。
///
/// ⚠ 这两个字段先前住 `struct Relay`（中转）。搬过来之后中转那个结构体里
/// 只剩下一个 `dest: Arc<dyn Destinations>` —— 上游选择整块藏在它后面（`20 §4`）。
pub(crate) struct Accounts {
    /// `(agent, 账号)` → 上游 + key。**决定这条请求发到哪儿、用哪把 key 的唯一住址。**
    ///
    /// ⚠ 它**不进任何 `Debug`**：`SecretKey` 手写的 `Debug` 恒为遮蔽形，
    /// 而本结构体**整个没有** `derive(Debug)`（`KS1` 的第二道）。
    table: std::sync::RwLock<RoutingTable>,
    /// 每 agent 一行的默认上游（条 59）。`/t/` 无行时按 `seg1` 查它；重载时给没写 `base_url` 的行取值。
    upstreams: Upstreams,
    /// 重载源。`None` = 判据自己造的表（不从文件来）⇒ 永不重载。
    reload: Option<Reload>,
}

impl Accounts {
    /// 从一张已经装好的表 ＋ 那张每 agent 一行的默认上游起一层上游选择。
    pub(crate) fn new(table: RoutingTable, upstreams: Upstreams) -> Self {
        Self {
            table: std::sync::RwLock::new(table),
            upstreams,
            reload: None,
        }
    }

    /// `D1 阻-2`：把「从哪儿重读那张表」接上。**只有 `run_with` 那条真路走它。**
    pub(crate) fn reloading_from(mut self, r: Reload) -> Self {
        self.reload = Some(r);
        self
    }

    /// 每条请求进来先问一次：那份凭据文件动过没有？动过就重读。
    ///
    /// ⚠ **它从中转搬过来了，位置也跟着变了一格**，如实写：先前 `handle` 在
    /// **读请求体之前**调它，现在它在 [`Accounts::resolve`] 的开头 ⇒ 落在**读完请求体之后**。
    /// 差别只有「那一次 `stat` 发生在哪一刻」，**线上一个字节都不变**
    /// （`wire_golden` 那三条线的金标准逐字节钉着）。搬的理由：热重载是**上游选择的私事**，
    /// 留在中转就等于中转认识「凭据文件」这个东西。
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
        let now = policy::stamp_of(r.path());
        if r.seen_is(&now) {
            return;
        }
        let mut loaded = creds::load(r.path());
        // ★★ `D2 阻-2`：**解析坏了就不换表。**
        //
        // `creds::load` 在「读不动 / 不是合法 JSON」时回的是 `accounts: 空 + problem: Some(_)`
        // ⇒ 照着装表就是**把整张表换成空**，而空表的行为是**全部 404**。
        // 用户那一侧看到的是「我明明配好了、刚才还能用，现在每一发都 404」——
        // 而成因是他刚才手编那份 JSON 少了一个逗号。
        // ⚠ 「一条都没配」与「读坏了」是两回事：前者 `problem` 是 `None`、accounts 空，
        //   那是一个**合法**状态（谁都不走中转），照换不误。
        if let Some(why) = loaded.problem.as_deref() {
            eprintln!("[apikey] 凭据文件读不成表，**保留上一张表不动**（不是换成空表）：{why}");
            // 印记也**不更新** —— 下次请求进来还会再试一次，人把文件改回来就自动恢复。
            return;
        }
        let (table, rejected, notes) = table::build(
            std::mem::take(&mut loaded.accounts),
            CREDENTIALS_FILE_AGENT,
            self.upstreams.of_credentials_file(),
        );
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
        r.remember(now);
    }
}

impl Destinations for Accounts {
    /// ★★★ **`20 §3.1` 那张决策表的唯一实现。** 中转不许在别处再判一次。
    ///
    /// # 这里把两个不透明段读成业务名 —— 就这一处
    ///
    /// `seg1` = agent · `seg2` = 账号 ⇒ **两段一起**是表的索引键（条 49）；
    /// `seg1` 另外还是「每 agent 一行的默认上游」那张表的键（条 59）。
    ///
    /// # 表里有那一行时答什么
    ///
    /// 那一格**不在这里**，在 [`dispatch_auth`] —— 三种鉴权处置与
    /// 「为什么 `Substitute` 的 key 是 `Option`」整段写在它头上。**一个事实一个住址。**
    fn resolve(&self, mode: Mode, key: &RouteKey, act: &mut dyn FnMut(Destination<'_>)) {
        // `D1 阻-2`：查表**之前**先看那份文件动过没有 —— 不然「界面上配完 key」要重启才生效，
        // 而不重启的症状是一个静默的 404（与「账号 id 打错」同形）。
        self.refresh_if_changed();
        // ⚠ 这个读锁活到本函数返回为止 —— 而 `resolve` 的契约禁止调用方在 `act` 里做
        //   流式转发（见 `Destinations::resolve` 头注第 2 条硬约束）⇒ 锁不跨 `pump`。
        let table = self.table.read().expect("lock");
        decide(&table, &self.upstreams, mode, key, act);
    }

    /// 〔V141〕名单只从适配层来（`agents::session_headers`，与默认上游同一张注册表）。
    fn stream_label_headers(&self) -> Vec<&'static str> {
        crate::agents::session_headers()
    }
}

/// 上游选择 `Refuse` 的两个码 —— **只有这一处**〔`设计/20 §3.1a` ②〕。
///
/// ⚠ 它们与中转自己造的那几个码（503 在飞上界 · 504 传输失败，住 `relay` 那一侧）
/// **必须两两不相交**（`D7`：同码 ⇒ agent 分不清是我们配错了还是上游挂了）。
/// 钉这一条的判据住中转那边（`server_tests::every_status_we_make_has_one_home_and_the_three_groups_are_disjoint`，
/// 它扫整个 crate 的生产段，本文件在它的人群里）。
///
/// `/s/` 表里没这一行（`§3.1` 第 2 行）。
const NO_ROW: &str = "404 Not Found";
/// `/t/` 表里没这一行、而这个 agent 没登记默认上游（`§3.1` 第 4 行）。
const AGENT_NOT_REGISTERED: &str = "502 Bad Gateway";

/// `20 §3.1` 那张决策表**本身**，从「谁持锁、什么时候重载」里剥出来。
///
/// ★ 剥出来只为一件事：判据要能**拿生产段这一份**去问，而不是自己再写一份同构的
/// 「这一行该不该换头」。[`Accounts::resolve`] 自己也只是「重载 → 取读锁 → 调它」
/// ⇒ **实现只有这一份**，没有第二本账。
pub(crate) fn decide(
    table: &RoutingTable,
    upstreams: &Upstreams,
    mode: Mode,
    key: &RouteKey,
    act: &mut dyn FnMut(Destination<'_>),
) {
    // ★ 两个不透明段在这里、**只在这里**被读成业务名。
    let (agent, account) = (key.seg1.as_str(), key.seg2.as_str());
    match (mode, table.lookup(agent, account)) {
        // ── `/s/` 有行 ⇒ 鉴权由这一行说了算（三种处置见 `dispatch_auth`）─────
        (Mode::Substitute, Some(row)) => dispatch_auth(row, act),

        // ── `/s/` 无行 ⇒ **404**（`§3.1` 第 2 行，今天的行为，一字不改）──────────
        (Mode::Substitute, None) => {
            // ★★★ **`K-H2` `KH2` 的正主**：路由键里那一段账号在表里查不到 ⇒ **404**。
            //
            //   ⚠⚠ 三条**不许**做的，逐条写死（`KH2` 逐字点名的最坏失效形态就在这里）：
            //     · 不许回落到别的账号的 key —— 那是**拿 A 的 key 发 B 的请求**；
            //     · 不许回落到默认上游 —— 「配错了」与「没配」会变成同一个结果；
            //     · 不许在这里「顺手补一行」。
            //   今天这三条靠的是：本支**只答 `Refuse`**，而中转手里没有任何可以回落的值
            //   （每 agent 一行的默认上游住本层，中转一个上游字面量都没有，`table_guard` 那条两向相等断言钉着）。
            act(Destination::Refuse {
                status: NO_ROW,
                why: copy_core::copy_static!("beUpstream.decide.noRow"),
            });
        }

        // ── `/t/` 有行 ⇒ **只取上游，绝不取 key**（`§3.1` 第 3 行）────────────────
        (Mode::Passthrough, Some(row)) => {
            // ⚠⚠ **这一支刻意不走 [`dispatch_auth`]**，而那正是它的全部意义：
            //   那个函数会在「这一行有 key」时答 `Substitute`（代入）。
            //   `/t/` 逐字是「**中转永不代入 auth**」⇒ 同一行在 `/s/` 与 `/t/` 下
            //   发出去的字节**必须不同**，下游那份鉴权头在这里逐字节原样上去。
            //   钉这一条的是 `wire_golden` 那一格：同一个 `acctA`（表里配着 key），
            //   走 `/t/` 时上游收到的是**客户端那把**，不是表里那把。
            act(Destination::Passthrough {
                upstream: row.base(),
            });
        }

        // ── `/t/` 无行 ⇒ 按 `seg1` 取该 agent 的默认上游；未登记 ⇒ **502** ────────
        (Mode::Passthrough, None) => match upstreams.of(agent) {
            // 🔴 **`§3.1` 第 4 行（2026-09-18 拍板 (b) 甲）逐字：「按 `seg1` 取该 agent
            //   的默认上游；`seg1` 未登记 ⇒ `Refuse { "502", "这个 agent 没有登记上游" }`。
            //   **不许回落到某一个写死的常量**」。
            //
            // ① 登记过 ⇒ 发到**这一家自己那一行**，下游那份鉴权头逐字节原样上去
            //   （`/t/` 从来不代入）。⚠ 取的是 `upstreams.of(agent)`，**不是**某一个进程级的值 ——
            //   那个进程级常量今天整删了。
            Some(base) => act(Destination::Passthrough { upstream: base }),
            // ② 未登记 ⇒ **502**。★ 这不是没做完，是照那条 🔴 做的 fail-closed：
            //   能选的只有「回落到某一家」（那条 🔴 明禁，后果逐字是「把 codex 的请求发给
            //   Anthropic」）与「拒」。选拒。
            None => act(Destination::Refuse {
                status: AGENT_NOT_REGISTERED,
                why: copy_core::copy_static!("beUpstream.decide.agentNotRegistered"),
            }),
        },
    }
}

/// 表里**有这一行**时，这一发的鉴权处置是什么。
///
/// # 三种鉴权处置，以及为什么 `Substitute` 的 key 是 `Option`
///
/// 🔴 **这是与 `20 §2` 那段伪码的第二处形状差异，理由要认下来**：规格那个枚举有
/// **两**种鉴权处置（原样转发 / 代入一把），而**今天盘上有三种**：
///
/// | 这一行 | 今天发给上游的字节 | 落到哪个变体 |
/// |---|---|---|
/// | 有 key，`style` 要写头 | 丢掉下游那几个 auth 头，写这一行自己的（按 `style`） | `Substitute { auth.write: Some(_) }` |
/// | 没 key，`style` 不是 `NoAuth` | 下游那份 auth 头**逐字节原样**上去（订阅登录那一档） | `Passthrough` |
/// | `style` 是 `NoAuth`（有没有 key 都一样） | **丢掉**下游那几个 auth 头，而且一个头都不写 | `Substitute { auth.write: None }` |
///
/// 第三行是 `K-R1` 的「本地部署那一格」（本机推理服务不校验凭据 ⇒ 把一把真 key
/// 发过去就是白送）。它**不是** `Passthrough`：`Passthrough` 逐字是「下游送来的 auth 头
/// 原样转发」。⇒ 两个变体装不下三种处置，而丢掉一种就是**行为变化**。
///
/// ⇒ 处置是把 `Substitute` 的含义写准：「**这一行的鉴权由表说了算**（先把下游那份剥掉）」，
/// [`AuthSwap::write`] 给 `None` 表示「剥掉之后什么都不写」。`Passthrough` 仍然逐字是「原样转发」。
/// 中转那边因此简化成一句话：`drop_client_auth == 这是不是 Substitute`
/// —— 先前那个 `(key.is_some() && …) || style == NoAuth` 的复合条件（`K-R1` 头注
/// 逐字警告过「只看前者的话 `NoAuth` 那一行会把客户端的真 key 原样送给一个声明了
/// 不校验凭据的本地端点」）**整条搬到了这里**，中转再也没有第二处可以判错。
fn dispatch_auth(row: &Row, act: &mut dyn FnMut(Destination<'_>)) {
    let style = row.auth_style();
    let clear = headers_to_clear();
    match (row.key(), auth_header_of(style)) {
        // ① 有 key，而这个形状**要写一个头** ⇒ 丢掉下游那几份，写这一行自己的。
        (Some(k), Some((name, prefix))) => {
            // ★★ **这是整个后端生产段里唯一一处把明文取出来的地方**（`KS2`）。
            //    它就在「拼这一行要写的那个鉴权头值」这一句上。
            //    ⚠⚠ 〔`P16` 2026-09-22〕它**从中转搬到了这里**，而搬的是**住址不是处数**：
            //      `creds_guard::the_plaintext_leaves_the_type_at_exactly_one_place_in_this_crate`
            //      那条「恰好 1 处」的相等断言**一个字节都没动**（它扫整个 crate，不写死文件名），
            //      `creds_store_tests::PLAINTEXT_EXIT_SITES` 那一行只改了住址栏。
            //      加第二处仍然是**放宽**，不许在实现里顺手把那条断言改大。
            //    ⚠ 为什么搬：中转的类型面上不许再出现 `creds-core` 的类型（`C2`）
            //      ⇒ 「把 `AuthStyle` 翻成 HTTP」与「把 key 拼成头值」**同属上游选择的判断**，
            //      中转只拿到一个 `(头名, 完整头值)` 照写。
            let value = format!("{prefix}{}", k.expose_for_auth_header());
            act(Destination::Substitute {
                upstream: row.base(),
                auth: AuthSwap {
                    clear,
                    write: Some((name, value.as_str())),
                },
            });
        }
        // ② 这个形状**不写任何头**（`AuthStyle::NoAuth`，`K-R1` 的「本地部署那一格」）
        //    ⇒ 仍然要丢掉下游那几份：把一把真 key 发给一个声明了不校验凭据的端点就是白送。
        //    ⚠ 两支合在一条臂上是**有意的**：有没有 key 在这一格不改变字节（都是「丢掉、不写」）。
        (_, None) => act(Destination::Substitute {
            upstream: row.base(),
            auth: AuthSwap { clear, write: None },
        }),
        // ③ 没 key，而这个形状本来要写头 ⇒ 没东西可代入 ⇒ **原样转发**（订阅登录那一档）。
        (None, Some(_)) => act(Destination::Passthrough {
            upstream: row.base(),
        }),
    }
}

/// 一种鉴权头形状 → `(头名, 值前缀)`；`None` = **不发鉴权头**。
///
/// # ★ 它是 `AuthStyle` → HTTP 的映射，而它今天住**上游选择**〔`P16` 2026-09-22 搬过来的〕
///
/// 先前它住 `server.rs`（中转），理由是「`creds-core` 那一侧刻意不认识 HTTP ⇒ 头名与前缀
/// 不许写在那边」。**那条理由今天仍然成立，而它并不推出「所以该住中转」** ——
/// `creds-core` 与中转之间还有上游选择，而上游选择正是「认识账号、也认识这一行要什么鉴权形状」
/// 的那一层。它收 `AuthStyle`（一个 `creds-core` 的类型）⇒ 按 `C2` 它**不可能**住中转。
///
/// ⇒ 今天的分工是三段而不是两段：`creds-core` 管**格式**（文件里那个词是什么）·
/// 本层管**翻译**（那个词对应哪个头、值前面加什么）· 中转管**照写**（它只看见一个串）。
///
/// ⚠ 穷尽 `match`：加一个成员**编译不过** —— 这一格是编译器买的，不是一条文本判据买的。
pub(crate) fn auth_header_of(style: AuthStyle) -> Option<(&'static str, &'static str)> {
    match style {
        AuthStyle::Bearer => Some(("Authorization", "Bearer ")),
        AuthStyle::XApiKey => Some(("x-api-key", "")),
        AuthStyle::NoAuth => None,
    }
}

/// 换头前要整条丢掉的下游鉴权头名 —— **从 [`auth_header_of`] 现算**。
///
/// # 🔴 它为什么不是一份手写名单（这一条是硬的）
///
/// 先前中转有一个手写的 `AUTH_HEADER_NAMES = ["authorization", "x-api-key"]`，
/// 靠一条判据与 [`auth_header_of`] 焊在一起。`P16` 把头材料改成由上游选择交下来之后，
/// 那条焊缝的两端会**分居两层**（集合在中转、映射在上游选择）——
/// 而缺焊的症状是**同名鉴权头出现两次，上游谁赢没有定义**。
/// ⇒ 处置不是把焊缝拉长，是**取消焊缝**：名单由映射**派生**，天然只有一个家（`D1`）。
///
/// # ⚠ 射程：它是**全集**，不是「这一趟要写的那一个」
///
/// 走的是 `AuthStyle::ALL`（`creds-core` 现算的那个闭集）⇒ 任何形状**可能**写出来的头名
/// 全在里面。所以配了 `x-api-key` 的那一行也会把客户端的 `Authorization` 丢掉 ——
/// **那是安全性质，不是顺手**。缩成「只丢要写的那一个」是行为变更。
/// 钉这一条的判据住 `creds_guard`（它自己独立地从 `AuthStyle::ALL` 派生一遍去比对）。
///
/// ⚠ 大小写：本表原样返回 [`auth_header_of`] 给的名字（有大写），比对一律
/// `eq_ignore_ascii_case`。先前那份手写名单要求全小写，理由是「表里混大小写会让读的人
/// 以为它区分大小写」—— **派生之后没有那张给人读的表了**，那条要求随之失效。
fn headers_to_clear() -> &'static [&'static str] {
    static CLEAR: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    CLEAR.get_or_init(|| {
        AuthStyle::ALL
            .iter()
            .filter_map(|s| auth_header_of(*s))
            .map(|(name, _)| name)
            .collect()
    })
}

/// 读一次凭据并**把该说的话说出去**，返回装好的表 ＋ 那份文件的路径与印记。
///
/// ★ 它为什么被抽成一个有名字的函数（同 `listen::resolve_port` / `run_reading` 那两次的理由）：
/// `run_with` 的尾巴是**永不返回**的 `serve()` ⇒ 长在里面的东西没有任何判据够得着。
/// 这里抽出来之后，`KS9②`（只放一份文件、一次界面都不开）与 `KS11`（过宽出声）
/// 打的都是**生产段真正跑的那一份**，不是一个同构的副本。
pub(crate) fn load_credentials(
    get: &dyn Fn(&str) -> Option<String>,
    home: &std::path::Path,
    upstreams: &Upstreams,
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
    let stamp = policy::stamp_of(&path);
    let mut loaded = creds::load(&path);
    // ★ 装表这一步（`K-H2`）**在出声之前**：`announce` 要印的「有几行进得了表」
    //   与「哪几行进不去、为什么」都是它算出来的。
    //   ⚠ `take` 是因为 `AccountEntry` 里装着 `SecretKey`，而那个类型**刻意不给 `Clone`**
    //     （`K-H2a`：少一条能复制明文的路就少一个出口）⇒ 只能把所有权交出去。
    let (table, rejected, notes) = table::build(
        std::mem::take(&mut loaded.accounts),
        CREDENTIALS_FILE_AGENT,
        upstreams.of_credentials_file(),
    );
    creds::announce(&loaded, table.len(), &rejected, &notes, out);
    (table, path, stamp)
}
