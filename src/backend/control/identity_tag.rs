//! `U-NP④`：**会话身份打标（`@ccm_sid`）** —— 从 `shared/ccm` 的每秒轮询搬到后端。
//!
//! # 它接的是谁的班
//!
//! `shared/ccm` 里原来有一条**与会话同寿、每秒一轮**的后台循环：读
//! `<claude_dir>/sessions/<自己的 PID>.json` 拿 `sessionId`，写进 tmux 会话级 option
//! `@ccm_sid`。那是全仓唯一一条「每会话一条、跑在远端机器上」的轮询
//!（开五个会话 = 每秒五条循环）。用户原话：「**可以动 ccm. 不要轮询**」＋
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
//! `@ccm_sid` 是 **tmux 窗格级** option（一个会话里可以跑几个 claude，各挂各的），所以必须先知道那个 claude 进程住在哪个窗格里。
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
//! # 起进程
//!
//! 探测复用 [`super::gate::probe`]（**零新增起进程点**），只有真要写时才多起一个
//! `tmux set-option`。已登记进 `readonly_guard::spawn_registry`。
//! 改的是 **tmux server 的运行期状态**，不是后端自己写用户既有数据（同 `tmux_hook`）。

/// 一次打标的结局。**返回而不是吞掉** —— 唯一调用点（`observe/watcher.rs::process_session_added`）
/// 先经 [`Outcome::failure_note`] 把打不上的那两形说出来，再取 [`Outcome::container`]。
///
/// `#[must_use]`：`@ccm_sid` 是破坏性动作**唯一认的事实**（打错 ＝ 杀错），
/// 标没写上 ⇒ Gate 2 不过 ⇒ kill / 送键回 `wrong_owner`，而「为什么进不去」整条链零线索 ——
/// 这个结局被整个丢掉时编译器要说话。
#[must_use = "打标的结局要经 `failure_note` 说出来（打不上 ⇒ 之后 kill / 送键被身份门拒）"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// 打上了（或从「没有」变成了新值）。带上那个窗格的终端句柄（同 `terminals-list`）。
    Tagged(String),
    /// 已经是这个值了 —— **不重复写**。启动重扫时每个会话都会走一遍这里，
    /// 免掉「已经对了还再起一次 `tmux`」。同样带终端句柄。
    AlreadyCurrent(String),
    /// 这个进程不在 tmux 里：环境**读得到**，`TMUX_PANE` 没设（或是空串）⇒ 没有会话可打。
    ///
    /// 这一格从此**只**说这一件事 —— 它是 `session_added.container = "none"`
    /// 的唯一来源（`observe::watcher::container_of`）。环境读不到 / pane id 形状不对的那两支
    /// 挪进 [`Outcome::PaneUnknown`]：它们说的是「不知道」，不是「不在」。
    NotInTmux,
    /// **不知道它在不在 tmux 里**：环境这一刻读不到（`EnvRead::Unreadable`：exec 窗口、
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
    /// **打不上的那两形说出来**：一句给日志的话；其余五形回 `None`。
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
            | Outcome::AlreadyCurrent(_)
            | Outcome::NotInTmux
            | Outcome::PaneUnknown
            | Outcome::NoSuchPane => return None,
        };
        Some(format!(
            "会话身份标没打上（pid {pid} · sid {sid}）：{why} —— 之后对这个会话的 kill / 送键会被身份门拒（wrong_owner）"
        ))
    }

    /// 这一次真往 tmux 里写了（对账据此决定要不要再探一次快照）。
    pub(crate) fn wrote(&self) -> bool {
        matches!(self, Outcome::Tagged(_))
    }

    /// **打标那一次探测的结局 → 这条会话的容器**（`session_added.container`）。
    ///
    /// | 结局 | 容器 | 为什么 |
    /// |---|---|---|
    /// | `Tagged` / `AlreadyCurrent` | `{host: tmux, terminal: 句柄}` | tmux 认得这个进程所在的 pane |
    /// | `NotInTmux` | `{host: none}` | 环境读得到，`TMUX_PANE` 没设 |
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
    pub(crate) fn container(&self) -> Option<crate::stream::wire::SessionContainer> {
        use crate::stream::wire::{SessionContainer, TerminalHost};
        match self {
            Outcome::Tagged(t) | Outcome::AlreadyCurrent(t) => Some(SessionContainer::Hosted {
                host: TerminalHost::Tmux,
                terminal: Some(t.clone()),
            }),
            Outcome::NotInTmux => Some(SessionContainer::None),
            Outcome::PaneUnknown
            | Outcome::NoSuchPane
            | Outcome::RejectedSid
            | Outcome::Failed(_) => None,
        }
    }
}

/// sid 的字符集。**与 `control::launch::parse_request` 同一条**：它会被写进 tmux 会话级 option，
/// 收紧到确定安全的字符集。
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
/// （原先是一个 `.value()?`，换成三臂 `match` 之后这条性质不变）
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
/// 返回值从 `Option` 换成三态（[`PaneRead`]）：`None` 原先把「没设」与「读不到 /
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

/// tmux 注给 pane 的那个变量名（键名是常量、不是参数：本文件不回传整份环境）。
const TMUX_PANE_ENV: &str = "TMUX_PANE";

/// **把 `sid` 打到 `pid` 所在的那个 tmux 会话上。**
///
/// 调用点只有一处：`observe/watcher.rs::process_session_added`，在「这个 pid 确实是写
/// 那份 pidfile 的那个 claude」被证过之后。跨层边已登记进 `layering_guard`。
pub(crate) fn tag(pid: u32, sid: &str) -> Outcome {
    // 先取口再做别的：测试构建里没注入假 tmux 就在这里炸，与走不走得到 tmux 无关（人群按「调了 tag」算）。
    let probe_cmd = door::tmux();
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
    let probed = match super::gate::probe_with(probe_cmd, &pane) {
        Ok(Some(p)) => p,
        Ok(None) => return Outcome::NoSuchPane,
        Err((code, msg)) => return Outcome::Failed(format!("{code}: {msg}")),
    };
    // 容器那一格报的句柄：与 `terminals-list` 同一个函数、同一个窗格。
    let terminal = super::terminals::handle_of(&probed.session_id, Some(&pane));
    if probed.ccm_sid == sid {
        return Outcome::AlreadyCurrent(terminal);
    }
    // ★ 对窗格**句柄**（`%N`，server 生命周期内唯一、不复用）下手，不对名字 —— 与 `gate` / `kill` 同一条纪律。
    // 打在窗格上：同一个会话里的几个 claude 各挂各的，不互相覆盖（会话级单值会让对账来回改写、重探不停）。
    set_sid(door::tmux(), pane, sid, terminal)
}

/// 打标路上起 tmux 的唯一口（探测与写都经 `door::tmux`）。
///
/// 测试构建里整个口换成 `tests/` 那一份：只交**本线程注入的假 tmux**，没注入就炸（`INVARIANTS §48.3`）——
/// 进程内测试用自己的 pid 造 pidfile 时，`TMUX_PANE` 是跑测试那个终端的 ⇒ 不隔离就是往用户真 tmux 上打标（09-27 `bg-sid` 事故）。
#[cfg_attr(test, path = "../../../tests/backend/control/identity_tag_door.rs")]
pub(crate) mod door;

/// 真写那一下：`set-option -p -t <窗格句柄> <事实键> <sid>`。
///
/// 〔同形〕tmux 的 stderr **收下来进原因**（原先丢进 `Stdio::null()`，
/// 失败只剩一个退出码 —— 「为什么没打上」要靠猜）。`cmd` 由调用方造（生产 = `Command::new("tmux")`），
/// 判据换一个假 tmux 的绝对路径进来，不碰进程级 `PATH`（`gate_tests` 头注写过为什么不能 `set_var`）。
fn set_sid(mut cmd: std::process::Command, target: String, sid: &str, terminal: String) -> Outcome {
    match cmd
        .args(["set-option", "-p", "-t", &target, "@ccm_sid", sid])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
    {
        Ok(out) if out.status.success() => Outcome::Tagged(terminal),
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
pub(crate) mod tests; // `pub(crate)`：起 `sleep` 的夹具（`tests::spawn_settled_sleep`）给 watcher 的单测共用
