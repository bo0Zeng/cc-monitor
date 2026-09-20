//! 层 2 · **账号层**：`resolve` 那张决策表的**唯一住址**（`设计/20 §3.1`）。
//!
//! # 它知道什么、不知道什么
//!
//! | | |
//! |---|---|
//! | **知道** | 账号模型 · `relay-credentials.json` · 上游 · `auth_style` · 热重载 · 哪个账号走哪个模式 |
//! | **不知道** | HTTP 怎么发、字节怎么泵 —— 那是层 1（`server.rs` / `listen.rs` / `http1.rs`） |
//!
//! ⇒ **「agent」与「账号」这两个业务词只在这一层出现。** 层 1 手里只有
//! `RouteKey{ seg1, seg2 }` 两个不透明段（条 48 · `01 §2.1 C1`）——
//! 把它们读成 agent 与账号是**本模块**的事，就在下面 [`Accounts::resolve`] 里。
//!
//! # ⚠ 今天这一层还欠什么（照实登记，别读成「做完了」）
//!
//! - **`seg1`（agent）今天还不承重**：查表仍然只按 `seg2`。改它是**条 49**
//!   （表的键改成 agent＋账号 · 默认上游每 agent 一行住 `agents/` · 未登记直接拒绝），
//!   而条 49 是 `99 §4` 里 **14-ii** 的同拍前置，不是本拍（14-i 逐字要求「零行为变化」）。
//!   ⇒ 本拍**故意**保持「claude 的 3 号账号与 codex 的 3 号账号是同一行」这个今天的行为，
//!   并把它写在这里，免得下一个人以为条 49 已经落地。
//! - **`DEFAULT_UPSTREAM` 还是一个进程级的默认**，不是每 agent 一格（条 59）。同上，14-ii。

pub(crate) mod creds; // `K-H2a`：从哪儿拿 key（**只读**）+ 读之前查一次权限（层 2 搬家带过来的）
mod policy; // 热重载（`20 §4`：`accounts/policy.rs`）
pub(crate) mod table; // `K-H2`：路由表 —— 账号段 → **上游与 key 焊死的一个值**

pub(crate) use policy::Reload;

use super::upstream::Base;
use super::{Destination, Destinations, Mode, RouteKey};
use creds_core::store::AuthStyle;
use std::io::Write;
use table::{RoutingTable, Row};

/// 上游基址的环境变量名。**它住层 2** —— `20 §4`「常量跟着职责走」那一条。
///
/// ⚠ 它今天仍是**进程级**的一个旋钮。条 60 要把它改成「每家一个」（跟着 `agents::Adapter`
/// 走），那与条 59 同拍、归 14-ii。
pub(crate) const ENV_UPSTREAM: &str = "CCM_RELAY_UPSTREAM";

/// 没配 [`ENV_UPSTREAM`] 时用的上游。**它住层 2**。
///
/// # 🔴 `20 §4` 要的是「整删它，换成 `agents/` 里每 agent 一格」—— 那一刀**不在本拍**
///
/// 那是条 59（用户 2026-09-18 逐字「写死，跟着适配层」），而 `99 §4` 把条 49/59/60
/// 整包排在 **14-ii** 的同拍前置里。本拍（14-i）逐字要求**零行为变化**，而
/// 「未登记的 agent 直接拒绝」是一次**有**行为变化的改动。
/// ⇒ 本拍只做搬家：把它从层 1 挪到层 2，**值与语义一个字节不动**。
///
/// ⇒ 搬完之后层 1 里**没有任何可以回落的默认值**。这一句有判据钉着：
/// `table_guard::layer_one_has_no_default_upstream_to_fall_back_to`。
const DEFAULT_UPSTREAM: &str = "https://api.anthropic.com";

/// 解析「这个进程的默认上游」。认不出就是 `None`，调用方**出声并退 2**，不回落。
pub(crate) fn upstream_default(env: Option<&str>) -> Option<Base> {
    Base::parse(env.unwrap_or(DEFAULT_UPSTREAM)).ok()
}

/// 层 2 的实现：一张**可重载**的路由表。
///
/// ⚠ 这两个字段先前住 `struct Relay`（层 1）。搬过来之后层 1 那个结构体里
/// 只剩下一个 `dest: Arc<dyn Destinations>` —— 层 2 整块藏在它后面（`20 §4`）。
pub(crate) struct Accounts {
    /// 账号段 → 上游 + key。**决定这条请求发到哪儿、用哪把 key 的唯一住址。**
    ///
    /// ⚠ 它**不进任何 `Debug`**：`SecretKey` 手写的 `Debug` 恒为遮蔽形，
    /// 而本结构体**整个没有** `derive(Debug)`（`KS1` 的第二道）。
    table: std::sync::RwLock<RoutingTable>,
    /// 重载源。`None` = 判据自己造的表（不从文件来）⇒ 永不重载。
    reload: Option<Reload>,
}

impl Accounts {
    /// 从一张已经装好的表起一层账号层。
    pub(crate) fn new(table: RoutingTable) -> Self {
        Self {
            table: std::sync::RwLock::new(table),
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
    /// ⚠ **它从层 1 搬过来了，位置也跟着变了一格**，如实写：先前 `handle` 在
    /// **读请求体之前**调它，现在它在 [`Accounts::resolve`] 的开头 ⇒ 落在**读完请求体之后**。
    /// 差别只有「那一次 `stat` 发生在哪一刻」，**线上一个字节都不变**
    /// （`wire_golden` 那三条线的金标准逐字节钉着）。搬的理由：热重载是**层 2 的私事**，
    /// 留在层 1 就等于层 1 认识「凭据文件」这个东西。
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
            eprintln!("[relay] 凭据文件读不成表，**保留上一张表不动**（不是换成空表）：{why}");
            // 印记也**不更新** —— 下次请求进来还会再试一次，人把文件改回来就自动恢复。
            return;
        }
        let (table, rejected, notes) =
            table::build(std::mem::take(&mut loaded.accounts), r.upstream_default());
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
    /// ★★★ **`20 §3.1` 那张决策表的唯一实现。** 层 1 不许在别处再判一次。
    ///
    /// # 这里把两个不透明段读成业务名 —— 就这一处
    ///
    /// `seg1` = agent（⚠ 今天**还不参与查表**，见模块头注：那是条 49 / 14-ii）·
    /// `seg2` = 账号 ⇒ 表的索引键。
    ///
    /// # 表里有那一行时答什么
    ///
    /// 那一格**不在这里**，在 [`auth_disposition_of`] —— 三种鉴权处置与
    /// 「为什么 `Substitute` 的 key 是 `Option`」整段写在它头上。**一个事实一个住址。**
    fn resolve(&self, mode: Mode, key: &RouteKey, act: &mut dyn FnMut(Destination<'_>)) {
        // `D1 阻-2`：查表**之前**先看那份文件动过没有 —— 不然「界面上配完 key」要重启才生效，
        // 而不重启的症状是一个静默的 404（与「账号 id 打错」同形）。
        self.refresh_if_changed();
        // ⚠ 这个读锁活到本函数返回为止 —— 而 `resolve` 的契约禁止调用方在 `act` 里做
        //   流式转发（见 `Destinations::resolve` 头注第 2 条硬约束）⇒ 锁不跨 `pump`。
        let table = self.table.read().expect("lock");
        decide(&table, mode, key, act);
    }
}

/// `20 §3.1` 那张决策表**本身**，从「谁持锁、什么时候重载」里剥出来。
///
/// ★ 剥出来只为一件事：判据要能**拿生产段这一份**去问，而不是自己再写一份同构的
/// 「这一行该不该换头」。[`Accounts::resolve`] 自己也只是「重载 → 取读锁 → 调它」
/// ⇒ **实现只有这一份**，没有第二本账。
pub(crate) fn decide(
    table: &RoutingTable,
    mode: Mode,
    key: &RouteKey,
    act: &mut dyn FnMut(Destination<'_>),
) {
    let account = key.seg2.as_str();
    let Some(row) = table.lookup(account) else {
        // ★★★ **`K-H2` `KH2` 的正主**：路由键里那一段账号在表里查不到 ⇒ **404**。
        //
        //   ⚠⚠ 三条**不许**做的，逐条写死（`KH2` 逐字点名的最坏失效形态就在这里）：
        //     · 不许回落到别的账号的 key —— 那是**拿 A 的 key 发 B 的请求**；
        //     · 不许回落到默认上游 —— 「配错了」与「没配」会变成同一个结果；
        //     · 不许在这里「顺手补一行」。
        //   今天这三条靠的是：本支**只答 `Refuse`**，而层 1 手里没有任何可以回落的值
        //   （它连 `DEFAULT_UPSTREAM` 都看不见了，`table_guard` 那条两向相等断言钉着）。
        act(Destination::Refuse {
            status: "404 Not Found",
            why: "代入模式要求表里有这一行",
        });
        return;
    };
    let _ = mode; // 步 3 之前只有 `/s/` 一种模式，这里先不分支（`/t/` 那一支在步 3 加）
    act(auth_disposition_of(row));
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
/// | 有 key | 丢掉下游那几个 auth 头，写这一行自己的（按 `style`） | `Substitute { key: Some(_) }` |
/// | 没 key，`style` 不是 `NoAuth` | 下游那份 auth 头**逐字节原样**上去（订阅登录那一档） | `Passthrough` |
/// | 没 key，`style` 是 `NoAuth` | **丢掉**下游那几个 auth 头，而且一个头都不写 | `Substitute { key: None, style: NoAuth }` |
///
/// 第三行是 `K-R1` 的「本地部署那一格」（本机推理服务不校验凭据 ⇒ 把一把真 key
/// 发过去就是白送）。它**不是** `Passthrough`：`Passthrough` 逐字是「下游送来的 auth 头
/// 原样转发」。⇒ 两个变体装不下三种处置，而丢掉一种就是**行为变化**。
///
/// ⇒ 处置是把 `Substitute` 的含义写准：「**这一行的鉴权由表说了算**（先把下游那份剥掉）」，
/// key 给 `None` 表示「剥掉之后什么都不写」。`Passthrough` 仍然逐字是「原样转发」。
/// 层 1 那边因此简化成一句话：`drop_client_auth == 这是不是 Substitute`
/// —— 先前那个 `(key.is_some() && …) || style == NoAuth` 的复合条件（`K-R1` 头注
/// 逐字警告过「只看前者的话 `NoAuth` 那一行会把客户端的真 key 原样送给一个声明了
/// 不校验凭据的本地端点」）**整条搬到了这里**，层 1 再也没有第二处可以判错。
fn auth_disposition_of(row: &Row) -> Destination<'_> {
    let style = row.auth_style();
    match row.key() {
        Some(k) => Destination::Substitute {
            upstream: row.base(),
            key: Some(k),
            style,
        },
        None if style == AuthStyle::NoAuth => Destination::Substitute {
            upstream: row.base(),
            key: None,
            style,
        },
        None => Destination::Passthrough {
            upstream: row.base(),
        },
    }
}

/// 读一次凭据并**把该说的话说出去**，返回装好的表 ＋ 那份文件的路径与印记。
///
/// ★ 它为什么被抽成一个有名字的函数（同 `resolve_config` / `run_reading` 那两次的理由）：
/// `run_with` 的尾巴是**永不返回**的 `serve()` ⇒ 长在里面的东西没有任何判据够得着。
/// 这里抽出来之后，`KS9②`（只放一份文件、一次界面都不开）与 `KS11`（过宽出声）
/// 打的都是**生产段真正跑的那一份**，不是一个同构的副本。
pub(crate) fn load_credentials(
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
    let stamp = policy::stamp_of(&path);
    let mut loaded = creds::load(&path);
    // ★ 装表这一步（`K-H2`）**在出声之前**：`announce` 要印的「有几行进得了表」
    //   与「哪几行进不去、为什么」都是它算出来的。
    //   ⚠ `take` 是因为 `AccountEntry` 里装着 `SecretKey`，而那个类型**刻意不给 `Clone`**
    //     （`K-H2a`：少一条能复制明文的路就少一个出口）⇒ 只能把所有权交出去。
    let (table, rejected, notes) = table::build(std::mem::take(&mut loaded.accounts), default_base);
    creds::announce(&loaded, table.len(), &rejected, &notes, out);
    (table, path, stamp)
}
