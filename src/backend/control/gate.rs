//! F03：**§34 Gate 2（identity）在后端侧的落地** —— 「这个 tmux 会话是不是本工具管的」。
//!
//! # 它补的是哪个洞
//!
//! F03 之前，[`super::launch`] 的 `send-into` **只核会话存在性**（`no_such_session`）就
//! `send-keys`。它建会话时 `set-option` **写** `@ccm_sid`，却从不**核验**它。
//! monitor 那条路带着 §34 的 Gate 2（`cc-*` 前缀命中 **或** 远端 `@ccm_sid` 已设），
//! 于是「把 send-keys/kill 改走后端」等于**静默丢掉一道门** ——
//! 功能看起来一样、门禁全绿，而「不许往别人的 tmux 里打字」那道门没了。
//! 这条路此前由 monitor 的 `tmux_backend_gate_guard`（前提触发器）挡着。
//!
//! **判定本身不在这里** —— 在 [`super::gate_rules`]（monitor 那一侧的门删了，共享 crate `gate-core` 收回本层）。
//! 本模块只负责这一侧的**承载**：怎么把 `@ccm_sid` 从本机 tmux 取回来。
//!
//! # ★ 用 `#{session_id}` 当句柄，把 TOCTOU 窗口关掉
//!
//! monitor 那条路是**一条原子远端命令**（`display-message` 与动作折进同一个 round-trip），
//! 刻意不给「查完再动」之间留窗口。backend 这边 argv 直传、没有 shell，做不到把两条
//! tmux 调用折成一条 —— 照抄「先查名字、再对名字下手」就会**引入一个 monitor 没有的窗口**。
//!
//! 处置：探测时**连 `#{session_id}` 一起取回**（tmux 的 `$N`，server 生命周期内唯一、不复用），
//! 之后一律对**那个句柄**下命令，不再对名字下命令。名字在窗口期内被重新绑定到别的会话，
//! 句柄仍然指着被核验过的那一个；那个会话若已消失，`send-keys` 自然失败
//! ⇒ 回 `typed_unconfirmed`（「会话在，但载荷未必落」的那一档），**不会打到别人身上**。
//!
//! ⚠ 这比 monitor 那条路**更硬**，不是权宜之计。F04 把 kill 搬过来时应当沿用同一形态。
//!
//! # ★ 为什么用 `display-message` 而不是 `show-options`
//!
//! 同 monitor 侧的理由：`show-options` 对**未设置**的 option 是 `rc=1` + stderr，
//! 要脆弱的 rc/stderr 联合判断；`display-message -p` 对未设置的 option 静默展开成空串。
//! 实测（私有 socket）：目标不存在时**整条输出为空但 `rc=0`** ——
//! 所以「目标在不在」的判据是**输出为空**，不是退出码。这与 monitor 侧
//! `[ -z "$info" ] → CCM_NO_SESSION` 是同一条判据，刻意保持一致。

use crate::platform::child::{Child, Deadline};
use copy_core::copy_text;

/// 身份探测 · 列窗格那一发只读 tmux 的期限：界面等 `kill` / 送键的预算是 10 s，tmux 一发 5 s（同 watcher 探测 tmux 的期限）。
const PROBE_TMUX_WITHIN: Deadline = Deadline::secs(5);

/// 命令级错误：`(code, message)`。与 [`super::launch`] 同型。
pub(crate) type CmdErr = (&'static str, String);

// ★★ **K-R12 下一拍（09-04）：这两个口径的家搬到了 `crate::common::tmux_utf8`。**
//
// 上一拍这里各写了一份（`UTF8_CLIENT_FLAG` 与 `tab_underflow`），头注里逐字登记着
// 「三份口径今天**靠人对齐**」，成因是 `layering_guard` 钉死 `control/` 不许引用 `observe/`
// ⇒ 共用的家只能是 `common/`，而它当时不在写区。本拍写区含 `common/` ⇒ 两份都归位。
//
// 口径本身、两种表示为什么是**一个**口径、以及「什么形态的调用点该用哪种表示」
// 全部只有一个住址：那个模块的头注。**这里不复述**（`brief` 13b：闭集只许一个住址）。
//
// 只留本调用点专属的两句：
//
// ① 本模块这一处是**本机 argv 直传** ⇒ 按那张表用**旗**，而且必须排在子命令**之前**。
//    〔`K-R96` 09-12：原本是两处，「列全部会话」那一处随 `R52` 裁定一搬去了
//     `common/session_snapshot.rs`，同一条纪律与判据一起搬。〕
// ② 🔴 **那条「放错位置是响的」在本模块要打个折**：这一处**刻意不看退出码**
//    （没有任何会话时 `tmux ls` 就是非零 + 空输出），于是 `rc=1 + unknown flag -u`
//    会被压成「一个会话都没有」/「探不到」—— **又变回一次静默失效**。
//    ⇒ 下面那条判据钉的是**位置**，不只是「有没有」，并带一句反向断言。
use crate::common::session_snapshot::SessionSnapshot;
use crate::common::tmux_utf8::{tab_underflow, UTF8_CLIENT_FLAG};

/// 探测回来的两样东西。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Probed {
    /// tmux 的 `#{session_id}`（形如 `$0`）。**后续一律对它下命令**，见模块头注。
    pub(crate) session_id: String,
    /// `@ccm_sid` 的值。未设置 ⇒ 空串。
    ///
    /// ⚠ 这是 `@ccm_sid`，**不是 `@ccm_sid_expect`**。刻意分了两个：
    /// 通道 A（`shared/ccm`）只写意图，只有通道 B 才写事实，而**破坏性动作只认事实**。
    /// 放宽到 `_expect` 就是把这道门拆了。
    /// ⚠ `U-NP④`（08-14）之后**通道 B 的写者是后端自己**（[`super::identity_tag`]，
    /// 由 pidfile inotify 驱动），不再是 ccm 里那条每秒轮询 —— 分离更硬了（事实的写者
    /// 变成独立第三方，且打标前已过 `procStart` 冒名检查），但两个 key 的语义一个字没变。
    pub(crate) ccm_sid: String,
    /// `#{session_windows}`。**Gate 3 只给破坏性动作用**（见 [`admit_destructive`]）。
    /// 解析不出来 ⇒ `0`，而 Gate 3 要求恰好 `1` ⇒ **fail closed**（不会误杀）。
    pub(crate) windows: u32,
    /// `@ccm_client`：起这个会话的那一方声明的名字（没设 ⇒ 空串）。「哪个前端的会话」那一维（`gate_rules::gate_client`）。
    pub(crate) client: String,
}

/// 列出本机所有 tmux 会话的**身份二元组**：`(会话名, @ccm_sid)`。
///
/// # 它是给「谁是真的」用的〔P4f 续刀 08-13，用户提的架构点〕
///
/// 用户逐字：「**那他不应该是身份空间的子集吗? 他应该去调用身份空间啊**」。
///
/// cc-bus 的 `agents.tsv` 是一份**第二套名单**：它记 `id → session:win.pane`，
/// 而那个地址**会过期**（会话名被重用是常态 —— `cc-spawn` 就按目录基名取名；
/// 用户盘上那份有 86 行、最早 07-18）。08-13 实测过它的后果：敲门文字被打进**陌生占用者**的屏幕。
///
/// ⇒ 正确的从属关系是：**总线成员 ⊆ 活着的 tmux 会话**。谁活着由**身份空间**说了算，
/// `agents.tsv` 只回答「谁登记过 + 邮箱里还有几条没读」。
///
/// # ★★ `K-R96`（09-12）：**这里不再自己起 tmux 了，改成问那张快照**
///
/// 用户逐字（`R52` 裁定一）：「**Gate 能不能改成直接读那份快照. 可以 /
/// 改为向快照发一次询问, 快照更新一次**」。
///
/// ⇒ 本函数今天是 [`crate::common::session_snapshot`] 的一层**投影**：
/// 问它一次、它更新一次、交出来的恒是**这一刻**的名单。
/// 🔴 **别在这里加缓存、也别退回自己起一条「列全部会话」的 tmux 命令** ——
/// （那个子命令的字面量今天在本文件的生产段里是**零命中**，由判据
/// `tests::listing_every_session_is_no_longer_this_modules_job` 钉着）
/// 判活拿陈值 = 把一个刚死的会话报成活的（`bus-list` 会把敲门文字打进陌生占用者的屏幕）。
///
/// ⚠ **`session_id` 那一列没了**：本函数的两个消费者（`cc_bus::join_identity` 与
/// `ccm` 的铸名避让）一个都不问它，而快照的另一个发布者（`observe/watcher`）的
/// `TMUX_LS_FMT` 里根本没有它。要句柄的人走 [`probe`]（那才是关 TOCTOU 窗口的那条路）。
///
/// `@ccm_sid` 空 = 那个会话不是 `ccm` 起的（或还没绑 sid）—— 如实回空串，不猜。
pub(crate) fn list_sessions() -> Result<Vec<(String, String)>, CmdErr> {
    list_sessions_from(crate::common::session_snapshot::global())
}

/// [`list_sessions`] 的本体，**快照由调用方给** —— 这样「拿到的值是不是这一刻的」
/// 才有得测（判据喂一个会变的假快照进来，见本模块测试段那条）。
fn list_sessions_from(snap: &SessionSnapshot) -> Result<Vec<(String, String)>, CmdErr> {
    Ok(snap
        .query()?
        .into_iter()
        .map(|r| (r.name, r.ccm_sid))
        .collect())
}

/// 探测格式串。**三个**字段用 TAB 分隔 —— `session_id` 恒是 `$<数字>`、不含 TAB，
/// `@ccm_sid` 的字符集在 `launch::parse_request` 里收到了 `[A-Za-z0-9_-]`，
/// `session_windows` 对一个存在的会话恒为正整数。
///
/// ⚠ **F04a 加了 `#{session_windows}`（Gate 3 用）—— 刻意加在同一次探测里**：
/// 多一次 `display-message` 就多一个 TOCTOU 窗口，而本模块的立身之本就是把那个窗口关掉。
/// 非破坏性动作（`admit`）**不看**这个字段，但照样取回来 —— 与 monitor 侧同一条纪律
/// （`K-R72`（09-12）之前，monitor 侧那条 SSH 回落的 `build_guarded_tmux_cmd` 头注写着
/// 同一条纪律；那条路删了之后，这条纪律在本仓只剩这一个家）。
const PROBE_FMT: &str = "#{session_id}\t#{@ccm_sid}\t#{session_windows}\t#{@ccm_client}";

/// `PROBE_FMT` 的列数 —— [`tab_underflow`] 的 N。
///
/// 🔴 **顺带订正一条上一拍写错的话**：`K-R12` 的 `§5.4` 说
/// 「`gate.rs` 那条 `it.next().is_some()` 在多出一段时静默丢掉整个会话 —— 一个带 TAB 的
/// 目录名就能触发」。**在本处不成立**：`PROBE_FMT` 里根本没有路径列
/// （`pane_current_path` 只在 `TMUX_LS_FMT` 里），三列各自的取值域都排除了真 TAB
/// （`$<数字>` / `[A-Za-z0-9_-]` / 正整数）。
/// ⇒ 本处的**过溢只可能来自「有人手工把 `@ccm_sid` 设成含 TAB 的值」或格式串被改**，
/// 那两种都该拒 ⇒ 既有的 fail-closed 处置是对的，**本拍不动它**。
const PROBE_FMT_FIELDS: usize = 4;

/// 跑一次 `tmux display-message -p -t <target> '<fmt>'` 并把 stdout 取回来。
///
/// `Ok(None)` = 目标不存在（输出为空，见模块头注：**不看退出码**）。
///
/// ⚠ `target` **不限于会话名**：tmux 的目标解析会把 pane id（`%N`）也归到它所属的会话上
///（08-14 私有 socket 实测）。[`super::identity_tag`] 正是拿 `TMUX_PANE` 当 target 调它 ——
/// 复用这一处等于**不新增起进程点**。⚠ 空串 target 会被 tmux 静默解析成「某个会话」，
/// 调用方必须自己挡（`identity_tag::pane_is_safe` 就是那道门）。
pub(crate) fn probe(target: &str) -> Result<Option<Probed>, CmdErr> {
    probe_with(tmux_on(None), target)
}

/// 本模块起 tmux 的唯一构造处。`socket`：`None` = 让 tmux 按环境解析默认 socket（**生产恒 `None`**）；
/// `Some(p)` = 显式 `-S <p>`（只为让「按 sid 找窗格」在隔离的 tmux server 上测得出来，同 `capture_pane::capture_on`）。
fn tmux_on(socket: Option<&str>) -> Child {
    let cmd = Child::new("tmux");
    match socket {
        Some(s) => cmd.args(["-S", s]),
        None => cmd,
    }
}

/// 一个会话里的一个窗格（[`panes_on`] 的一行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaneTag {
    /// `#{pane_id}`（`%N`，server 生命周期内不复用 ⇒ 对它下手同对 `#{session_id}` 下手一样关得住 TOCTOU）。
    pub(crate) pane: String,
    /// 根进程 pid（杀成之后按它认 cc-bus 名册）。
    pub(crate) pid: u32,
    /// 它挂着的 `@ccm_sid`（窗格上的；没有就是会话那一级的；都没有 ⇒ 空串）。
    pub(crate) sid: String,
    /// 是不是会话此刻的当前窗格（当前窗口的活动窗格）。
    pub(crate) current: bool,
}

/// 列一个会话全部窗格的格式：句柄 · 根进程 pid · `@ccm_sid` · 是不是当前窗格（窗格活动、窗口也活动 ⇒ `11`）。
const PANES_FMT: &str = "#{pane_id}\t#{pane_pid}\t#{@ccm_sid}\t#{pane_active}#{window_active}";

/// 这个会话（`#{session_id}` 或 `=名:`）的全部窗格。只读 tmux；问不到 ⇒ 空。
pub(crate) fn panes_on(socket: Option<&str>, session: &str) -> Vec<PaneTag> {
    tmux_on(socket)
        .args([
            UTF8_CLIENT_FLAG,
            "list-panes",
            "-F",
            PANES_FMT,
            "-s",
            "-t",
            session,
        ])
        .run(PROBE_TMUX_WITHIN)
        .map(|o| parse_panes(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

/// [`panes_on`] 的原文 ⇒ 行（纯函数）。读不懂的行丢掉（段数不对 = 打印通道被改写，宁可当它不在）。
pub(crate) fn parse_panes(text: &str) -> Vec<PaneTag> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            match f.as_slice() {
                [pane, pid, sid, cur] if pane.starts_with('%') => Some(PaneTag {
                    pane: pane.to_string(),
                    pid: pid.trim().parse().ok()?,
                    sid: sid.to_string(),
                    current: *cur == "11",
                }),
                _ => None,
            }
        })
        .collect()
}

/// 挂着 `sid` 的那个窗格：同一个 sid 挂在几个窗格上（会话那一级的旧标签往下透）⇒ 当前窗格那个，否则第一个。
/// 空 `sid` ⇒ 没有（没挂标签的窗格不算「挂着空 sid」）。
pub(crate) fn carrier<'a>(panes: &'a [PaneTag], sid: &str) -> Option<&'a PaneTag> {
    if sid.is_empty() {
        return None;
    }
    let mut hits = panes.iter().filter(|p| p.sid == sid);
    let first = hits.clone().next();
    hits.find(|p| p.current).or(first)
}

/// [`probe`] 的本体，`tmux` 由调用方造：[`super::identity_tag`] 经它自己那一个口递进来
/// （测试构建里那个口是注入的假 tmux，`INVARIANTS §48.3`）。
pub(crate) fn probe_with(tmux: Child, target: &str) -> Result<Option<Probed>, CmdErr> {
    let out = tmux
        // K-R12：`-u` 必须在子命令**之前**（`display-message -u -p` 是 rc=1 的响错）。
        .args([
            UTF8_CLIENT_FLAG,
            "display-message",
            "-p",
            "-t",
            target,
            PROBE_FMT,
        ])
        .run(PROBE_TMUX_WITHIN)
        .map_err(|e| {
            e.into_cmd_err("no_tmux", |e| {
                copy_text("beGate.probe.noTmux", &[("e", &e.to_string())])
            })
        })?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.trim_end_matches(['\n', '\r']);
    if line.is_empty() {
        return Ok(None);
    }
    // ★ K-R12 `J1`：**段数下溢 ⇒ 通道被改写**，出声 + 拒绝把这行当好数据。
    //
    // 🔴 不装这一条的后果，与 `K-R23` 在 monitor 侧治的是**同一个形状**：通道脏时
    // `session_id` 会拿到**整行**、`ccm_sid` 拿到**空串** ⇒ `admit` 报出来的话是
    // 「远端 `@ccm_sid` 也没设」—— 与「远端真的没设」**逐字相同**。
    // 两件不同的事共用一个读数，下一个人还得把成因重查一遍。
    // ⇒ 处置沿用既有的 `Ok(None)`（= 目标不存在，fail-closed，`admit` 回 `no_such_session`），
    //   **但话不一样**：这一条 warn 里带原样回包，一眼看得见远端到底吐了什么。
    if tab_underflow(line, PROBE_FMT_FIELDS) {
        tracing::warn!(
            "CCM_TMUX_UNPARSABLE display-message 切出 {} 段 < {PROBE_FMT_FIELDS} —— \
             tmux 打印通道被改写（K-R12：客户端不是 UTF-8 ⇒ TAB 变 `_`），\
             判成「探不到」而不是「@ccm_sid 没设」。原样回包：{line:?}",
            line.split('\t').count()
        );
        return Ok(None);
    }
    // 逐段切。**多出一段就是格式串被人动过了**，宁可判不存在也不猜。
    let mut it = line.split('\t');
    let session_id = it.next().unwrap_or_default().to_string();
    let ccm_sid = it.next().unwrap_or_default().to_string();
    // 解析不出来 ⇒ 0。Gate 3 要求恰好 1 ⇒ **fail closed**（拿不到窗口数就不许杀）。
    let windows = it
        .next()
        .unwrap_or_default()
        .trim()
        .parse::<u32>()
        .unwrap_or(0);
    let client = it.next().unwrap_or_default().to_string();
    if session_id.is_empty() || it.next().is_some() {
        return Ok(None);
    }
    Ok(Some(Probed {
        session_id,
        ccm_sid,
        windows,
        client,
    }))
}

/// 请求里自报的前端（`args.client`）：没给 ⇒ `None`；给了就得是一个合形状的名字（`gate_rules::client_name_ok`）。
pub(crate) fn requester_of(args: &serde_json::Value) -> Result<Option<String>, CmdErr> {
    match args.get("client") {
        None => Ok(None),
        Some(serde_json::Value::String(c)) if crate::control::gate_rules::client_name_ok(c) => {
            Ok(Some(c.clone()))
        }
        Some(other) => Err((
            "invalid_args",
            crate::common::contract::malformed(&format!(
                "`client` must be 1-32 chars of [a-z0-9-]: {other}"
            )),
        )),
    }
}

/// 请求里指的会话（`args.sid`，要的是挂着它的那个窗格）：没给 ⇒ `None`；给了就得是 `@ccm_sid` 的形状（1–128 个 `[A-Za-z0-9_-]`）。
pub(crate) fn sid_of(args: &serde_json::Value) -> Result<Option<String>, CmdErr> {
    match args.get("sid") {
        None => Ok(None),
        Some(serde_json::Value::String(s))
            if (1..=128).contains(&s.len())
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') =>
        {
            Ok(Some(s.clone()))
        }
        Some(other) => Err((
            "invalid_args",
            crate::common::contract::malformed(&format!(
                "`sid` must be 1-128 chars of [A-Za-z0-9_-]: {other}"
            )),
        )),
    }
}

/// 身份那一道（Gate 2 ＋「哪个前端的会话」那一维）的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Who {
    /// 放行。
    Pass,
    /// 名字规则没过（没声明哪个前端、名字不像我们铸的、`@ccm_sid` 也没设）。
    NotOurs,
    /// 声明了别的前端：只读。
    OtherClient,
}

/// 声明了别的前端 ⇒ 只读；声明归这次请求自报的前端 ⇒ 放；没声明 / 用户终端起的 ⇒ 照名字规则（`gate2`）。
pub(crate) fn identity(name: &str, p: &Probed, requester: Option<&str>) -> Who {
    use crate::control::gate_rules::{gate_client, ClientVerdict};
    match gate_client(&p.client, requester) {
        ClientVerdict::Own => Who::Pass,
        ClientVerdict::NotYours => Who::OtherClient,
        ClientVerdict::Open
            if crate::control::gate_rules::gate2(name, Some(&p.ccm_sid)).allowed() =>
        {
            Who::Pass
        }
        ClientVerdict::Open => Who::NotOurs,
    }
}

/// **过门。** 通过则返回后续该用的目标句柄（`#{session_id}`；给了 `sid` ⇒ 挂着它的那个窗格的 `#{pane_id}`）。
///
/// `name` 是用户/调用方给的会话名（Gate 2 的本地半支按它判）；
/// `target` 是 `launch::exact_target(name)` 产出的精确目标（`=name:`）。
/// `sid` ＝ 要的是这个会话里挂着它的那个窗格（一个 tmux 会话里可以有几个 claude 窗格）。
///
/// 三种结局：
/// - 目标不存在 ⇒ `no_such_session`（**语义不变** —— 新门不许把这一档吞掉）；
/// - Gate 2 不通过 / 哪个窗格都不挂那个 sid ⇒ `wrong_owner`；
/// - 通过 ⇒ `Ok(句柄)`。
///
/// `requester` ＝ 请求自报的前端（「哪个前端的会话」那一维，见 [`identity`]）；声明了别的前端 ⇒ 也是 `wrong_owner`，话不同。
pub(crate) fn admit(
    name: &str,
    target: &str,
    sid: Option<&str>,
    requester: Option<&str>,
) -> Result<String, CmdErr> {
    let Some(p) = probe(target)? else {
        return Err((
            "no_such_session",
            copy_text("beGate.admit.noSession", &[("name", &format!("{name:?}"))]),
        ));
    };
    let shown = format!("{name:?}");
    // 给了 sid ⇒ 落在挂着它的那个窗格、身份也按它判（活动窗格是谁不算数）；哪个窗格都不挂它 ⇒ 此刻跑的已经不是那条会话。
    let (who, at) = match sid {
        None => (p.clone(), None),
        Some(s) => {
            let panes = panes_on(None, &p.session_id);
            let Some(c) = carrier(&panes, s) else {
                return Err((
                    "wrong_owner",
                    copy_text("beGate.admit.otherSession", &[("name", &shown)]),
                ));
            };
            let at = Probed {
                ccm_sid: c.sid.clone(),
                ..p.clone()
            };
            (at, Some(c.pane.clone()))
        }
    };
    match identity(name, &who, requester) {
        Who::Pass => Ok(at.unwrap_or(p.session_id)),
        Who::NotOurs => Err((
            "wrong_owner",
            copy_text("beGate.admit.notOurs", &[("name", &shown)]),
        )),
        Who::OtherClient => Err((
            "wrong_owner",
            copy_text("beGate.admit.otherClient", &[("name", &shown)]),
        )),
    }
}

/// **破坏性动作的门**：Gate 2（身份）**再加** Gate 3（`windows == 1`）。
///
/// # 为什么破坏性动作要多一道门
///
/// Gate 2 只回答「这是不是本工具的会话」。但一个**本工具建的**会话也可能被用户
/// 自己扩出了额外窗口（在里面开了别的东西）——把它整个杀掉就连带毁掉用户的活。
/// ⇒ Gate 3：**只杀「干净的单窗口会话」**。多窗口 ⇒ 拒绝，让用户自己去那个 tmux 里处理。
///
/// 拒绝码与历史同族（`CCM_GUARD_REJECTED windows=<n>`）。
/// ⚠ `K-R72`（09-12）：这句话原先写「与 monitor 侧逐条同义」并点名那边那条 shell
/// 表达式 —— **那一侧今天没有了**（送键与杀会话的桌面侧回落已删），Gate 3 从
/// 「两处要对齐」变成**只有这一处**。拒绝码保持同族是为了历史日志读得懂，不是为了对拍。
///
/// ⚠ **Gate 3 只给破坏性动作**：`send-keys` 不删除任何东西，窗口数与它无关 ——
/// 给它加 Gate 3 会让「往一个多窗口会话里打字」被误拒（monitor 侧 F04 Phase D
/// 审计专门修过这个错法）。所以本函数与 [`admit`] **是两个入口，不是一个带 flag 的**。
///
/// 给了 `sid` ⇒ 按挂着它的那个窗格认（[`panes_on`]）：哪个窗格都不挂它 ⇒ 认完换了人（`wrong_owner`）；身份按那个窗格判；
/// 会话里还有窗格挂着别的 sid（别的 claude）⇒ 只结束挂着它的窗格（不关整个会话，Gate 3 不适用）；没有 ⇒ 整个会话（照常过 Gate 3）。
pub(crate) fn admit_destructive(
    name: &str,
    target: &str,
    sid: Option<&str>,
    requester: Option<&str>,
) -> Result<EndAt, CmdErr> {
    let Some(p) = probe(target)? else {
        return Err((
            "no_such_session",
            copy_text(
                "beGate.admitDestructive.noSession",
                &[("name", &format!("{name:?}"))],
            ),
        ));
    };
    let (who, only) = match sid {
        None => (p.clone(), Vec::new()),
        Some(s) => {
            let tags = panes_on(None, &p.session_id);
            let carrying: Vec<String> = tags
                .iter()
                .filter(|t| t.sid == s)
                .map(|t| t.pane.clone())
                .collect();
            if carrying.is_empty() {
                return Err((
                    "wrong_owner",
                    copy_text(
                        "beGate.admitDestructive.otherSession",
                        &[("name", &format!("{name:?}"))],
                    ),
                ));
            }
            let others = tags.iter().any(|t| !t.sid.is_empty() && t.sid != s);
            let at = Probed {
                ccm_sid: s.to_string(),
                ..p.clone()
            };
            (at, if others { carrying } else { Vec::new() })
        }
    };
    match identity(name, &who, requester) {
        Who::Pass => {}
        Who::NotOurs => {
            return Err((
                "wrong_owner",
                copy_text(
                    "beGate.admitDestructive.notOurs",
                    &[("name", &format!("{name:?}"))],
                ),
            ))
        }
        Who::OtherClient => {
            return Err((
                "wrong_owner",
                copy_text(
                    "beGate.admitDestructive.otherClient",
                    &[("name", &format!("{name:?}"))],
                ),
            ))
        }
    }
    if !only.is_empty() {
        return Ok(EndAt::Panes(p.session_id, only));
    }
    if p.windows != 1 {
        return Err((
            "too_many_windows",
            copy_text(
                "beGate.admitDestructive.manyWindows",
                &[
                    ("name", &format!("{name:?}")),
                    ("n", &p.windows.to_string()),
                ],
            ),
        ));
    }
    Ok(EndAt::Session(p.session_id))
}

/// 破坏性动作放行之后落在哪。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EndAt {
    /// 整个会话（`#{session_id}` 句柄）。
    Session(String),
    /// 只这几个窗格（`#{pane_id}` 句柄；同会话的 `#{session_id}` 在前）：会话里还跑着挂别的 sid 的 claude。
    Panes(String, Vec<String>),
}

#[cfg(test)]
#[path = "../../../tests/backend/control/gate_tests.rs"]
mod tests;
