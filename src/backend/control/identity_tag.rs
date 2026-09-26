//! `U-NP④`：**会话身份打标（`@ccm_sid`）** —— 从 `shared/ccm` 的每秒轮询搬到后端。
//!
//! # 它接的是谁的班
//!
//! `shared/ccm` 里原来有一条**与会话同寿、每秒一轮**的后台循环：读
//! `<claude_dir>/sessions/<自己的 PID>.json` 拿 `sessionId`，写进 tmux 会话级 option
//! `@ccm_sid`。那是全仓唯一一条「每会话一条、跑在远端机器上」的轮询
//!（开五个会话 = 每秒五条循环）。用户 08-14 裁定：「**可以动 ccm. 不要轮询**」＋
//!「**ccm 做到必须走 backend**」⇒ 整条 poller 删掉、**不留轮询退路**，改由本模块打标。
//!
//! # 为什么后端干这件事**不需要**任何新的节拍
//!
//! backend 已经在 inotify `<claude_dir>/sessions/`（`observe/watcher.rs`）——
//! 那正是 pidfile 目录。它看到 `<PID>.json` 的那一刻，**pid 与 sid 同时在手**：
//! 文件名是 pid、内容里是 sid。`/clear`、`/branch` 会**原地重写同一个 pidfile**
//!（wire 上就是 `session_removed.cause="superseded"`），inotify 的 modify 照样送到
//! ⇒ 换 sid 这件事后端看得见，本模块跟着重打，与旧 poller 的「sid 变了立即刷」等效。
//!
//! # (pid, sid) → 「打到哪个 tmux 会话上」怎么解
//!
//! `@ccm_sid` 是 **tmux 会话级** option，所以必须先知道那个 claude 进程住在哪个会话里。
//!
//! **走 `/proc/<pid>/environ` 的 `TMUX_PANE`**，理由是它**没有陈旧的可能**：
//! 那个值属于**这个进程自己**（tmux 起 pane 时注进环境、`exec` 原样继承），
//! 与旧 poller「自己读自己的 pidfile、写自己的会话」是同一条自指性质。
//!
//! 实测（2026-08-14，私有 socket `-S`）：
//! - `exec` 掉的进程（pane 的直接子进程）：`TMUX_PANE=%0` ✓
//! - **孙子进程**（用户已在 pane 里，shell fork 出 ccm 再 exec ⇒ `pid != pane_pid`）：
//!   `TMUX_PANE=%1` 照样在 ✓ —— 所以**不能**改用 `#{pane_pid}` 去 join，那条只覆盖前一种。
//!
//! **排除掉的另一条**：让 ccm 自己写一个 `@ccm_pid=$$` 当 join 键，backend 用
//! `list-sessions -F` 反查。它跨平台（不依赖 `/proc`），但**有陈旧面**：ccm 退出后那个值
//! 留在会话里，PID 复用时会把新会话的 sid 打到旧会话上 —— 而 `@ccm_sid` 是破坏性动作
//!（kill）唯一认的事实 ⇒ 打错 = 杀错。`/proc` 那条没有这个面，故取它。
//! 平台代价为零：本模块的唯一调用点在 `process_session_added`，而那条路上游就有
//! `pid_alive`（非 Linux 分支是 `unimplemented!()`）⇒ **今天本来就只在 Linux 上跑**。
//!
//! # 两通道设计**没有被合并掉**
//!
//! 通道 A 写 `@ccm_sid_expect`（**意图**：「打算跑这个 sid」），
//! 通道 B 写 `@ccm_sid`（**事实**）。破坏性动作只认后者。搬家之后这条分离**更硬**了：
//! 事实的写者从「那个会话自己」变成了**独立的第三方**，而且后端是在
//! `process_session_added` 走完 `pid_alive` + `add_time_verdict`（procStart 冒名检查）
//! 之后才调本模块 —— 也就是说打标前已经证过「这个 pid 真的是写那份 pidfile 的那个 claude」。
//!
//! ⚠ **订正一句**〔`K-P2` C 第五拍，09-03；PM `§13 裁三` 裁「候选丙」〕：
//! 本段原来写「意图仍然**只由 ccm** 声明」。**那句话今天不成立** ——
//! [`super::launch`] 的 `create-or-attach` 臂（建会话那一刻，与建会话原子）也写
//! `@ccm_sid_expect`。它此前写的是**裸 `@ccm_sid`**，也就是**绕过本模块这道确认**
//! 直接授予事实身份（F04 修掉的 `R10` 形状），已在那一拍改掉。
//! ⇒ 准确的说法是：**意图由「建会话的那个人」声明**（`shared/ccm` 走 shell 那条路时是它，
//! 走后端的 `launch` 时是 `launch.rs`），而**事实只由本模块写**。
//! **本模块仍然一个字都不写 `@ccm_sid_expect`。**
//!
//! # 谁兜「标题被冲掉」
//!
//! 不需要人兜。`shared/ccm` 把 `set-titles-string` 设成 `#{?@ccm_sid,ccm-rbind-#{@ccm_sid},#T}`
//!（2026-07-31 真机踩坑后改的），窗口标题由 **tmux 自己从 `@ccm_sid` 合成**，
//! 与 claude 抢着写的 pane 标题彻底分开 ⇒ marker 常驻。
//! 实测（08-14，私有 socket + `script` 起真 pty client）：**外部** client 执行一次
//! `set-option @ccm_sid <sid>` 之后，tmux 主动往 client 推
//! `ESC ]0;ccm-rbind-<sid> BEL` —— 也就是说打标者是不是那个会话自己**无关紧要**。
//! 旧 poller 里那句「每 20 秒重打一次自愈」在这条 format 落地之后已是余物，随 poller 一起删。
//!
//! # 起进程
//!
//! 探测复用 [`super::gate::probe`]（**零新增起进程点**），只有真要写时才多起一个
//! `tmux set-option`。已登记进 `readonly_guard::spawn_registry`。
//! 改的是 **tmux server 的运行期状态**，不是后端自己写用户既有数据（同 `tmux_hook`）。
//!
//! ═══════════════════════════════════════════════════════════════════════════
//! # 🔴 本模块的第二张面：**启动期令牌**（`CCM_RBIND_TOKEN`）〔`设计/80 §8.7` 步 2，09-22〕
//! ═══════════════════════════════════════════════════════════════════════════
//!
//! 上面整篇讲的是「**把身份广播到本地**」—— 用户 09-22 逐字裁「**`--ccm-sid` 不要依赖
//! tmux**」，而 `设计/80 §8.1` 的核心判断逐字：
//!
//! > 🔴 **tmux 不是在做「发现身份」，是在做「把身份广播到本地」。**
//! > 而广播这件事，本仓已经有一条正经的、有分帧的、双向的通道 —— 后端的 wire 协议。
//! > ⇒ **解绑的全部内容就是：造出那个键。**
//!
//! 那个键就是 `CCM_RBIND_TOKEN`：**起会话的那一方**（monitor）生成一个 32 位十六进制的
//! 随机串，一半留在本地绑 HWND，一半随 `export` 进那条命令、被 claude 进程继承。
//! 本模块把它从 `/proc/<pid>/environ` 读出来，[`super::super::observe::watcher`] 随
//! `session_added` 帧报回去（字段 `rbind_token`）。**这条路上 tmux 一次都没出现。**
//!
//! ## 为什么这件事该由**本模块**做（不是新起一个文件）
//!
//! `设计/80 §8.3` 那张「零件都在盘上」的表点的就是本文件，逐条：
//!
//! | 零件 | 本轮现打核的结果 |
//! |---|---|
//! | 后端读 `/proc/<pid>/environ` | ✅ [`pane_of`] 为拿 `TMUX_PANE` 而读，走 `platform::proc::proc_env_var` |
//! | 「读 environ 不会陈旧」的论证 | ✅ **逐字适用**（见下） |
//! | 零新节拍 | ✅ 唯一调用点仍是 `process_session_added`，那是 `sessions/` inotify 的既有回调 |
//!
//! **那条论证为什么换个变量还成立**：上面「(pid, sid) → 打到哪个 tmux 会话上」那一段
//! 逐字写着「那个值属于**这个进程自己**（tmux 起 pane 时注进环境、`exec` 原样继承）」。
//! `CCM_RBIND_TOKEN` 是**同一条性质**，而且来源更硬 —— 注它的人是我们自己
//! （`export CCM_RBIND_TOKEN=… && claude …`），`exec` 之后原样继承，进程活着它就不变。
//! ⇒ 与 `TMUX_PANE` 一样**没有陈旧面**：读到的一定是这个进程自己的那个值。
//!
//! ## 三条硬约束，逐条落在代码上
//!
//! 1. **抠出来的全是写死的常量**（`INVARIANTS §1` 第 2 条那条铁律的同一口径）。
//!    〔措辞刻意避开那句话的针：`doc_claim_registry_tests` 按「同一行上同时出现
//!    『只抠』与那个量词」收人群（针拆成两半、分行写，理由同那边头注），
//!    照那句原话写，本文件就成了它描述的人群的一员。〕
//!    键名**不是参数**：本文件今天两处 `proc_env_var(pid, …)`，两个实参都是本文件里的
//!    `const`（[`TMUX_PANE_ENV`] / [`RBIND_TOKEN_ENV`]）。**绝不回传整个环境快照** ——
//!    那里面有用户全部的密钥类环境变量。这条由
//!    `identity_tag_tests::the_env_keys_this_file_reads_are_exactly_two_named_constants` 钉住。
//!    ⚠ 那条铁律的**计数词**说的是 `--session-accounts` 那条查询（`accounts_query.rs`，
//!    仍是两个键），**不是全后端**；本文件不在它的分母里
//!    （`accounts_query_tests.rs` 那条判据的头注逐字登记着这件事）。
//! 2. **令牌不许承载任何权限语义**（`设计/80 §8.6 ③` 逐字）。它**只能是一个不可猜的
//!    关联 id**：拿到它顶多能让某人的 ↗ 拉错窗口，**不能越权**。
//!    ⇒ 本模块对它**不做任何授权判断**，也不拿它当 join 键去做破坏性动作
//!    （破坏性动作唯一认的事实仍是 `@ccm_sid`，见上文两通道那一段）。
//! 3. **它是敏感数据 ⇒ 不进日志。** 令牌会出现在 `/proc/<pid>/environ`、
//!    `/proc/<pid>/cmdline`（`export … && claude` 这种前缀形）与 shell 历史里，
//!    但那几处都按进程属主设权限；**日志文件不是**。
//!    ⇒ [`rbind_token_of`] 的 `warn!` 只印**形状与长度**，一个字节的值都不印。
//!    ⚠ 本仓那条日志白名单判据（`relay/creds_guard.rs` 的 `KS4`）**扫的只有 `relay/`**
//!    （它头注第 1 条诚实边界逐字）⇒ 它够不到本文件。本模块因此自带一条
//!    `the_token_value_never_reaches_a_log_macro`。
//!
//! ## ⚠ 本模块**买不到**的那一维
//!
//! 端到端那条链（`token → HWND`）**本轮买不到**：`§8.7` 明写「1 与 2 之间有顺序依赖
//! （没有 token 进环境，后端读不到）」，而步 1（`EnvOp` 窄变体 ＋ 两个渲染器透出）
//! 与步 3（本地半 ps-await 认 token）都不在本刀射程里。
//! **本模块买到的是**：一个已经带着那个变量的进程，后端读得出它、并把它放上 wire。

/// 一次打标的结局。**返回而不是吞掉** —— 唯一调用点（`observe/watcher.rs::process_session_added`）
/// 先经 [`Outcome::failure_note`] 把打不上的那两形说出来，再取 [`Outcome::container`]。
///
/// 〔W5-VIS · `设计/15 §4.7 S2`〕`#[must_use]`：`@ccm_sid` 是破坏性动作**唯一认的事实**（打错 ＝ 杀错），
/// 标没写上 ⇒ Gate 2 不过 ⇒ kill / 送键回 `wrong_owner`，而「为什么进不去」整条链零线索 ——
/// 这个结局被整个丢掉时编译器要说话。
#[must_use = "打标的结局要经 `failure_note` 说出来（打不上 ⇒ 之后 kill / 送键被身份门拒，设计/15 §4.7 S2）"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// 打上了（或从「没有」变成了新值）。带上落地的 `#{session_id}` 句柄。
    Tagged(String),
    /// 已经是这个值了 —— **不重复写**。启动重扫时每个会话都会走一遍这里，
    /// 免掉「已经对了还再起一次 `tmux`」。
    AlreadyCurrent,
    /// 这个进程不在 tmux 里：环境**读得到**，`TMUX_PANE` 没设（或是空串）⇒ 没有会话可打。
    ///
    /// 〔U4b · 第四波〕这一格从此**只**说这一件事 —— 它是 `session_added.container = "none"`
    /// 的唯一来源（`observe::watcher::container_of`）。环境读不到 / pane id 形状不对的那两支
    /// 挪进 [`Outcome::PaneUnknown`]：它们说的是「不知道」，不是「不在」。
    NotInTmux,
    /// 〔U4b〕**不知道它在不在 tmux 里**：环境这一刻读不到（`EnvRead::Unreadable`：exec 窗口、
    /// 僵尸、非 Linux），或者 `TMUX_PANE` 有值但形状过不了 [`pane_is_safe`]。
    ///
    /// 打标上与 `NotInTmux` 一模一样（都不打）；分开它只为了容器那一格**不许把「不知道」报成「不在」**
    /// —— 那正是 `K-R21` 在 `EnvRead` 上治过的同一句假话。
    PaneUnknown,
    /// pane 还在环境里，但 tmux 说它不存在（会话已关 / server 换了）。
    NoSuchPane,
    /// sid 形状不对 —— **fail closed**，不往 tmux 里塞一个没核过的字符串。
    RejectedSid,
    /// 起不来 tmux / tmux 报错。
    Failed(String),
}

impl Outcome {
    /// 〔W5-VIS · `设计/15 §4.7 S2`〕**打不上的那两形说出来**：一句给日志的话；其余五形回 `None`。
    ///
    /// | 结局 | 说不说 | 为什么 |
    /// |---|---|---|
    /// | `Failed(原因)` | 说，带原因 | tmux 起不来 / 报错 ⇒ 标没写上 |
    /// | `RejectedSid` | 说 | sid 形状不对、fail closed ⇒ 标没写上 |
    /// | `Tagged` / `AlreadyCurrent` | 不说 | 打上了 |
    /// | `NotInTmux` / `PaneUnknown` / `NoSuchPane` | 不说 | 没有可打的会话（不在 tmux 里 / 不知道 / 默认 socket 不认），不是「打标失败」 |
    ///
    /// 后果那半句写死在这里：标没写上的会话，之后经后端的 kill 与送键过不了身份门（`wrong_owner`）。
    pub(crate) fn failure_note(&self, pid: u32, sid: &str) -> Option<String> {
        let why = match self {
            Outcome::Failed(why) => why.clone(),
            Outcome::RejectedSid => {
                "sid 的形状不对（只认字母、数字、`-`、`_`，长度 1–128），不往 tmux 里写".to_string()
            }
            Outcome::Tagged(_)
            | Outcome::AlreadyCurrent
            | Outcome::NotInTmux
            | Outcome::PaneUnknown
            | Outcome::NoSuchPane => return None,
        };
        Some(format!(
            "会话身份标没打上（pid {pid} · sid {sid}）：{why} —— 之后对这个会话的 kill / 送键会被身份门拒（wrong_owner）"
        ))
    }

    /// 〔U4b · 第四波〕**打标那一次探测的结局 → 这条会话的容器**（`session_added.container`）。
    ///
    /// | 结局 | 容器 | 为什么 |
    /// |---|---|---|
    /// | `Tagged` / `AlreadyCurrent` | `tmux` | tmux 认得这个进程所在的 pane |
    /// | `NotInTmux` | `none` | 环境读得到，`TMUX_PANE` 没设 |
    /// | `PaneUnknown` | 不知道 | 环境读不到 / pane id 形状不对 |
    /// | `NoSuchPane` | 不知道 | 环境说在某个 pane 里，默认 socket 上的 tmux 不认（私有 `-S` socket 之类）|
    /// | `RejectedSid` | 不知道 | sid 形状不对，压根没探 |
    /// | `Failed` | 不知道 | 起不来 tmux / tmux 报错 |
    ///
    /// **不知道就不报**（`None`）：「不在 tmux 里」是一句会改变界面措辞的话，没有正面证据不许说。
    ///
    /// 为什么是 `Outcome` 的方法、而不是观测侧的一个自由函数：判定要逐个认这个类型的变体，
    /// 放在观测侧就得让 `observe → control` 多一条跨层边（`layering_guard` 的登记表）；
    /// 调用方（`observe/watcher.rs::process_session_added`）今天只经 `tag(..)` 的返回值用它。
    pub(crate) fn container(&self) -> Option<crate::wire::SessionContainer> {
        use crate::wire::SessionContainer;
        match self {
            Outcome::Tagged(_) | Outcome::AlreadyCurrent => Some(SessionContainer::Tmux),
            Outcome::NotInTmux => Some(SessionContainer::None),
            Outcome::PaneUnknown
            | Outcome::NoSuchPane
            | Outcome::RejectedSid
            | Outcome::Failed(_) => None,
        }
    }
}

/// sid 的字符集。**与 `control::launch::parse_request` 同一条**：它会被拼进 tmux 的
/// 格式串（`ccm-rbind-#{@ccm_sid}`），收紧到确定安全的字符集。
///
/// 这里的 sid 来自 pidfile（claude 自己写的），不是用户输入 —— 但「来源可信」不是放行的
/// 理由：本仓栽过的那些坑里，最贵的一类就是「这个值不可能有问题」。
fn sid_is_safe(sid: &str) -> bool {
    !sid.is_empty()
        && sid.len() <= 128
        && sid
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// tmux 的 pane id 形状：`%` + 十进制数字。
///
/// ★ **空串必须挡住**，这条是实测撞出来的（08-14，私有 socket）：
/// `tmux display-message -p -t '' '#{session_name}'` **不报错**，它静默解析成
/// 「当前/最近的那个会话」⇒ 一个空的 `TMUX_PANE` 会让我们把 sid 打到**一个碰巧的会话**上。
/// 而 `@ccm_sid` 是破坏性动作唯一认的事实 —— 打错就是杀错。
fn pane_is_safe(pane: &str) -> bool {
    match pane.strip_prefix('%') {
        Some(n) => !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

/// 取这个进程所在的 tmux pane（`TMUX_PANE`），核过形状才回。
///
/// # ⚠ 上面那条「空串必须挡住」的防线，**在这条路上够不到**〔`K-R21` 09-03 现打，如实登记〕
///
/// `proc_env_var` 在**读侧**就把空串压成了 `EnvRead::Unset`（`platform/proc.rs`：
/// 「值是空串」与「压根没这个键」两支合并）⇒ 下面 `EnvRead::Unset` 那一臂一律早退成 `Outcome::NotInTmux`
/// （〔U4b〕原先是一个 `.value()?`，换成三臂 `match` 之后这条性质不变）
/// ⇒ **`pane_is_safe("")` 永远不会在生产路上被执行到**。
///
/// 🔴 它**不是坏的，是死的**：它挡的是「拿到空串」，而上游让它拿不到。
/// 今天还活着的只有它的单元测试（`pane_is_safe("")` 那几条直喂）。
///
/// ⚠ **`K-R21` 刻意没有把它治活**：治活要拆的是「键不在 / 值是空串」那两支，
/// 而那一拍选的「乙」明确**只拆「环境这一刻取不到」**、把那两支留在一起
/// （拆它们属于「甲」，已登记为后续清理）。**也刻意没删它** ——
/// 它是 08-14 一次真事故（`display-message -t ''` 静默解析成「当前会话」⇒ 打错标就杀错）
/// 撞出来的，那条早退（今天是 `Unset` 那一臂）哪天换成别的写法，它就是唯一还站着的那道门。
///
/// 〔U4b · 第四波〕返回值从 `Option` 换成三态（[`PaneRead`]）：`None` 原先把「没设」与「读不到 /
/// 形状不对」合在一起，打标对它们确实等价（都不打），但容器那一格不等价（「不在 tmux 里」vs「不知道」）。
/// 上面「空串那道防线够不到」那一段**照旧成立**：空串在读侧就成了 `EnvRead::Unset` ⇒ 走 `NotSet`。
fn pane_of(pid: u32) -> PaneRead {
    use crate::platform::proc::EnvRead;
    let raw = match crate::platform::proc::proc_env_var(pid, TMUX_PANE_ENV) {
        EnvRead::Value(v) => v,
        EnvRead::Unset => return PaneRead::NotSet,
        EnvRead::Unreadable => return PaneRead::Unknown,
    };
    let pane = raw.trim().to_string();
    if pane_is_safe(&pane) {
        PaneRead::Pane(pane)
    } else {
        PaneRead::Unknown
    }
}

/// [`pane_of`] 的三个结局。
enum PaneRead {
    /// 形状核过的 pane id。
    Pane(String),
    /// 环境读得到、`TMUX_PANE` 没设 ⇒ 不在 tmux 里。
    NotSet,
    /// 读不到，或者值的形状不对 ⇒ 不知道。
    Unknown,
}

// ═══════════════════════════════════════════════════════════════════════════
// 启动期令牌（`CCM_RBIND_TOKEN`）—— `设计/80 §8.7` 步 2 的读侧。整段论证见本文件头注
// 「本模块的第二张面」。
// ═══════════════════════════════════════════════════════════════════════════

/// tmux 注给 pane 的那个变量名。**从内联字面量提成常量**〔`设计/80 §8.7` 步 2〕：
/// 本文件从此读两个环境变量，而「键名不是参数」这条性质要看得见 —— 两个实参都是
/// 本文件里的 `const`，才能被一条判据数出来（见头注硬约束 1）。
const TMUX_PANE_ENV: &str = "TMUX_PANE";

/// 🔴 **启动期令牌的变量名 —— 两路共用的契约，钉死，不许改。**
///
/// 写侧住 monitor 的载荷渲染那一族（`设计/80 §8.7` 步 1，`EnvOp` 的窄变体
/// `export-rbind-token`，**另一路在做**）。读侧就是这里。
///
/// ⚠ 双写点的失效方向**极其安静**：两侧漂开 ⇒ 读侧恒 `None` ⇒ 而 `None` 在本查询里
/// 是**合法值**（「这条会话没有令牌」）⇒ **不会有任何东西报错**，↗ 只是永远降级。
/// 那正是 `K-P5f` 在 `CCM_LAUNCH_ID` 上栽过的同一个坑
/// （`accounts_query_tests.rs` 那条双写点判据的诊断逐字记着）。
/// ⇒ 〔订正 · 令牌步 3〕那条同型的双写点判据**已补**：
/// `payload_tests.rs::the_launch_token_env_var_has_the_same_name_on_both_halves`（两侧异源：读侧抠本行、写侧真跑渲染器）。
const RBIND_TOKEN_ENV: &str = "CCM_RBIND_TOKEN";

/// 令牌的形状：**恰好** 32 个小写十六进制字符（`[0-9a-f]{32}`）。两路共用的契约。
///
/// # 为什么 fail closed 到这个地步（不 `trim`、不认大写、不认长度相近）
///
/// 它的下游用途是**跨机器的 join 键**：本地那半按同一个串去查 HWND 表。
/// 一个「差不多对」的串在那张表里查不到，与查错一样糟，但**更难归因**
/// （`设计/80 §8.5 ②` 的全部价值就是把归因从「四档猜」收成**一个布尔**：
/// 这个 sid 有没有令牌 —— 那个布尔只有在「有 = 形状确定对」时才说得准）。
/// ⇒ 任何偏离一律当**没有**，而不是当「大概是它」。
///
/// ⚠ 与 [`pane_of`] 的 `trim` 刻意不同：pane id 是 tmux 注的、历史上见过带空白的读法；
/// 令牌是**我们自己注的**，我们知道它长什么样，没有任何理由去宽容它。
pub(crate) fn token_is_safe(token: &str) -> bool {
    token.len() == 32
        && token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// 〔S5 · 第四波〕令牌那个变量名，给 `ccm` 直路读**自己**的环境用（`control/ccm/plan.rs::Env::from_process`）。
///
/// ⚠ 刻意是一个函数、不把上面那行 `const` 改成 `pub(crate)`：那一行的写法被桥侧
/// `payload_tests.rs::the_launch_token_env_var_has_the_same_name_on_both_halves` 按行首逐字认。
/// 名字仍然只住那一行 —— 这里只是把它借出去，不是第二份。
pub(crate) fn rbind_token_env() -> &'static str {
    RBIND_TOKEN_ENV
}

/// **把 `pid` 那个进程的启动期令牌读出来**（`CCM_RBIND_TOKEN`），核过形状才回。
///
/// `None` 的含义是**「这条会话不作数」**，四支合并且**刻意不区分**：
/// 没设 / 形状过不了 [`token_is_safe`] / 环境这一刻取不到（exec 窗口、僵尸进程 ——
/// `platform::proc::EnvRead::Unreadable`）/ 非 Linux。
/// 合并的依据与 `EnvRead::value()` 那条头注逐字相同：本调用方是 **fail-closed** 的
/// （四条路都得同一个保守答案「没有令牌 ⇒ ↗ 降级回标题路」），所以对它确实等价。
///
/// 🔴 **不印值。** 令牌是敏感数据（`设计/80 §8.6 ③`）；下面那行 `warn!` 只印**长度**，
/// 让「注进去了但形状不对」这一格可诊断，而不泄漏关联 id 本身。
/// 这条由 `identity_tag_tests::the_token_value_never_reaches_a_log_macro` 钉住。
pub(crate) fn rbind_token_of(pid: u32) -> Option<String> {
    let raw = crate::platform::proc::proc_env_var(pid, RBIND_TOKEN_ENV).value()?;
    if !token_is_safe(&raw) {
        tracing::warn!(
            "pid {pid} 的 {RBIND_TOKEN_ENV} 形状过不了（长度 {}，要 32 个小写十六进制字符）\
             —— 当作没有令牌处理（fail closed）。**值刻意不印**：它是敏感的关联 id",
            raw.len()
        );
        return None;
    }
    Some(raw)
}

/// **把 `sid` 打到 `pid` 所在的那个 tmux 会话上。**
///
/// 调用点只有一处：`observe/watcher.rs::process_session_added`，在「这个 pid 确实是写
/// 那份 pidfile 的那个 claude」被证过之后。跨层边已登记进 `layering_guard`。
pub(crate) fn tag(pid: u32, sid: &str) -> Outcome {
    if !sid_is_safe(sid) {
        return Outcome::RejectedSid;
    }
    let pane = match pane_of(pid) {
        PaneRead::Pane(p) => p,
        PaneRead::NotSet => return Outcome::NotInTmux,
        PaneRead::Unknown => return Outcome::PaneUnknown,
    };
    // 探测复用 gate 那一处（**零新增起进程点**）。它顺带把当前 `@ccm_sid` 取回来 ⇒
    // 值没变就一次 `set-option` 都不用起。
    let probed = match super::gate::probe(&pane) {
        Ok(Some(p)) => p,
        Ok(None) => return Outcome::NoSuchPane,
        Err((code, msg)) => return Outcome::Failed(format!("{code}: {msg}")),
    };
    if probed.ccm_sid == sid {
        return Outcome::AlreadyCurrent;
    }
    // ★ 对 `#{session_id}` **句柄**下手，不对名字 —— 与 `gate` / `kill` 同一条纪律：
    // 名字在探测与动手之间可能被重新绑定到别的会话，句柄不会（server 生命周期内唯一、不复用）。
    let target = probed.session_id.clone();
    set_sid(std::process::Command::new("tmux"), target, sid)
}

/// 真写那一下：`set-option -t <句柄> <事实键> <sid>`。
///
/// 〔W5-VIS · `设计/15 §4.7 S4` 同形〕tmux 的 stderr **收下来进原因**（原先丢进 `Stdio::null()`，
/// 失败只剩一个退出码 —— 「为什么没打上」要靠猜）。`cmd` 由调用方造（生产 = `Command::new("tmux")`），
/// 判据换一个假 tmux 的绝对路径进来，不碰进程级 `PATH`（`gate_tests` 头注写过为什么不能 `set_var`）。
fn set_sid(mut cmd: std::process::Command, target: String, sid: &str) -> Outcome {
    match cmd
        .args(["set-option", "-t", &target, "@ccm_sid", sid])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
    {
        Ok(out) if out.status.success() => Outcome::Tagged(target),
        Ok(out) => Outcome::Failed(format!(
            "tmux set-option 退出码 {}：{}",
            out.status,
            super::launch::said_of(&out.stderr)
        )),
        Err(e) => Outcome::Failed(format!("起不来 tmux：{e}")),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/identity_tag_tests.rs"]
mod tests;
