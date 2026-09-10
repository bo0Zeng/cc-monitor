//! F03：**§34 Gate 2（identity）在 daemon 侧的落地** —— 「这个 tmux 会话是不是本工具管的」。
//!
//! # 它补的是哪个洞
//!
//! F03 之前，[`super::launch`] 的 `send-into` **只核会话存在性**（`no_such_session`）就
//! `send-keys`。它建会话时 `set-option` **写** `@ccm_sid`，却从不**核验**它。
//! monitor 那条路带着 §34 的 Gate 2（`cc-*` 前缀命中 **或** 远端 `@ccm_sid` 已设），
//! 于是「把 send-keys/kill 改走 daemon」等于**静默丢掉一道门** ——
//! 功能看起来一样、门禁全绿，而「不许往别人的 tmux 里打字」那道门没了。
//! 这条路此前由 monitor 的 `tmux_daemon_gate_guard`（前提触发器）挡着。
//!
//! **判定本身不在这里** —— 在 `gate-core`，monitor 与 daemon 共用同一份（定框 C1）。
//! 本模块只负责这一侧的**承载**：怎么把 `@ccm_sid` 从本机 tmux 取回来。
//!
//! # ★ 用 `#{session_id}` 当句柄，把 TOCTOU 窗口关掉
//!
//! monitor 那条路是**一条原子远端命令**（`display-message` 与动作折进同一个 round-trip），
//! 刻意不给「查完再动」之间留窗口。daemon 这边 argv 直传、没有 shell，做不到把两条
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

use std::process::{Command, Stdio};

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
// ① 本模块这两处是**本机 argv 直传** ⇒ 按那张表用**旗**，而且必须排在子命令**之前**。
// ② 🔴 **那条「放错位置是响的」在本模块要打个折**：这两处**刻意不看退出码**
//    （没有任何会话时 `tmux ls` 就是非零 + 空输出），于是 `rc=1 + unknown flag -u`
//    会被压成「一个会话都没有」/「探不到」—— **又变回一次静默失效**。
//    ⇒ 下面那条判据钉的是**位置**，不只是「有没有」，并带一句反向断言。
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
    /// ⚠ `U-NP④`（08-14）之后**通道 B 的写者是 daemon 自己**（[`super::identity_tag`]，
    /// 由 pidfile inotify 驱动），不再是 ccm 里那条每秒轮询 —— 分离更硬了（事实的写者
    /// 变成独立第三方，且打标前已过 `procStart` 冒名检查），但两个 key 的语义一个字没变。
    pub(crate) ccm_sid: String,
    /// `#{session_windows}`。**Gate 3 只给破坏性动作用**（见 [`admit_destructive`]）。
    /// 解析不出来 ⇒ `0`，而 Gate 3 要求恰好 `1` ⇒ **fail closed**（不会误杀）。
    pub(crate) windows: u32,
}

/// 列出本机所有 tmux 会话的**身份三元组**：`(会话名, session_id, @ccm_sid)`。
///
/// # 它是给「谁是真的」用的〔P4f 续刀 08-13，用户提的架构点〕
///
/// 用户逐字：「**那他不应该是身份空间的子集吗? 他应该去调用身份空间啊**」。
///
/// cc-bus 的 `agents.tsv` 是一份**第二套名单**：它记 `id → session:win.pane`，
/// 而那个地址**会过期**（会话名被重用是常态 —— `cc-spawn` 就按目录基名取名；
/// 用户盘上那份有 86 行、最早 07-18）。08-13 实测过它的后果：敲门文字被打进**陌生占用者**的屏幕。
///
/// ⇒ 正确的从属关系是：**总线成员 ⊆ 活着的 tmux 会话**。谁活着由**这里**说了算，
/// `agents.tsv` 只回答「谁登记过 + 邮箱里还有几条没读」。
///
/// ⚠ **一次调用列全部**，不是每个成员探一次：用户那台的总线有 86 行，
/// 逐个探就是 86 次起进程。
///
/// `@ccm_sid` 空 = 那个会话不是 `ccm` 起的（或还没绑 sid）—— 如实回空串，不猜。
pub(crate) fn list_sessions() -> Result<Vec<(String, String, String)>, CmdErr> {
    const LIST_FMT: &str = "#{session_name}\t#{session_id}\t#{@ccm_sid}";
    /// `LIST_FMT` 的列数 —— [`tab_underflow`] 的 N。三列都不可能含真 TAB
    /// （会话名被 tmux 转义成字面 `\t`；`session_id` 恒是 `$<数字>`；
    /// `@ccm_sid` 的字符集在 `launch::parse_request` 里收到了 `[A-Za-z0-9_-]`）
    /// ⇒ 这里**下溢与过溢都不会由合法内容触发**，下溢只可能是通道被改写。
    const LIST_FMT_FIELDS: usize = 3;
    let out = Command::new("tmux")
        // K-R12：`-u` 必须在子命令**之前**（放后面是 rc=1 的响错，见 `UTF8_CLIENT_FLAG`）。
        .args([UTF8_CLIENT_FLAG, "list-sessions", "-F", LIST_FMT])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| {
            (
                "no_tmux",
                format!("起不来 tmux（远端装了吗？PATH 里有吗？）：{e}"),
            )
        })?;
    // ⚠ **不看退出码**（同本模块 `probe`）：没有任何会话时 `tmux ls` 是非零 + 空输出，
    //   那不是错误，是「一个都没有」。
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            // K-R12 `J1`：段数下溢 ⇒ 通道被改写 ⇒ **出声 + 整行不当好数据**。
            // 空行不算下溢（它本来就该被下面那句 `name.is_empty()` 丢掉，不是病）。
            if !line.trim().is_empty() && tab_underflow(line, LIST_FMT_FIELDS) {
                tracing::warn!(
                    "CCM_TMUX_UNPARSABLE list-sessions 切出 {} 段 < {LIST_FMT_FIELDS} —— \
                     tmux 打印通道被改写（K-R12：客户端不是 UTF-8 ⇒ TAB 变 `_`），\
                     整行不当好数据。原样回包：{line:?}",
                    line.split('\t').count()
                );
                return None;
            }
            let mut it = line.split('\t');
            let name = it.next()?.trim();
            if name.is_empty() {
                return None;
            }
            Some((
                name.to_string(),
                it.next().unwrap_or_default().trim().to_string(),
                it.next().unwrap_or_default().trim().to_string(),
            ))
        })
        .collect())
}

/// 探测格式串。**三个**字段用 TAB 分隔 —— `session_id` 恒是 `$<数字>`、不含 TAB，
/// `@ccm_sid` 的字符集在 `launch::parse_request` 里收到了 `[A-Za-z0-9_-]`，
/// `session_windows` 对一个存在的会话恒为正整数。
///
/// ⚠ **F04a 加了 `#{session_windows}`（Gate 3 用）—— 刻意加在同一次探测里**：
/// 多一次 `display-message` 就多一个 TOCTOU 窗口，而本模块的立身之本就是把那个窗口关掉。
/// 非破坏性动作（`admit`）**不看**这个字段，但照样取回来 —— 与 monitor 侧同一条纪律
/// （那边的 `build_guarded_tmux_cmd` 头注写着「**总是**在格式串里带 `#{session_windows}`」）。
const PROBE_FMT: &str = "#{session_id}\t#{@ccm_sid}\t#{session_windows}";

/// `PROBE_FMT` 的列数 —— [`tab_underflow`] 的 N。
///
/// 🔴 **顺带订正一条上一拍写错的话**：`K-R12` 的 `§5.4` 说
/// 「`gate.rs` 那条 `it.next().is_some()` 在多出一段时静默丢掉整个会话 —— 一个带 TAB 的
/// 目录名就能触发」。**在本处不成立**：`PROBE_FMT` 里根本没有路径列
/// （`pane_current_path` 只在 `TMUX_LS_FMT` 里），三列各自的取值域都排除了真 TAB
/// （`$<数字>` / `[A-Za-z0-9_-]` / 正整数）。
/// ⇒ 本处的**过溢只可能来自「有人手工把 `@ccm_sid` 设成含 TAB 的值」或格式串被改**，
/// 那两种都该拒 ⇒ 既有的 fail-closed 处置是对的，**本拍不动它**。
/// 那条误伤是真的、但只在 `src-tauri/src/tmux.rs::parse_tmux_ls` 那一处（见该处头注）。
const PROBE_FMT_FIELDS: usize = 3;

/// 跑一次 `tmux display-message -p -t <target> '<fmt>'` 并把 stdout 取回来。
///
/// `Ok(None)` = 目标不存在（输出为空，见模块头注：**不看退出码**）。
///
/// ⚠ `target` **不限于会话名**：tmux 的目标解析会把 pane id（`%N`）也归到它所属的会话上
///（08-14 私有 socket 实测）。[`super::identity_tag`] 正是拿 `TMUX_PANE` 当 target 调它 ——
/// 复用这一处等于**不新增起进程点**。⚠ 空串 target 会被 tmux 静默解析成「某个会话」，
/// 调用方必须自己挡（`identity_tag::pane_is_safe` 就是那道门）。
pub(crate) fn probe(target: &str) -> Result<Option<Probed>, CmdErr> {
    let out = Command::new("tmux")
        // K-R12：`-u` 必须在子命令**之前**（`display-message -u -p` 是 rc=1 的响错）。
        .args([
            UTF8_CLIENT_FLAG,
            "display-message",
            "-p",
            "-t",
            target,
            PROBE_FMT,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| {
            (
                "no_tmux",
                format!("起不来 tmux（远端装了吗？PATH 里有吗？）：{e}"),
            )
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
    if session_id.is_empty() || it.next().is_some() {
        return Ok(None);
    }
    Ok(Some(Probed {
        session_id,
        ccm_sid,
        windows,
    }))
}

/// **过门。** 通过则返回后续该用的目标句柄（`#{session_id}`）。
///
/// `name` 是用户/调用方给的会话名（Gate 2 的本地半支按它判）；
/// `target` 是 `launch::exact_target(name)` 产出的精确目标（`=name:`）。
///
/// 三种结局：
/// - 目标不存在 ⇒ `no_such_session`（**语义不变** —— 新门不许把这一档吞掉）；
/// - Gate 2 不通过 ⇒ `wrong_owner`，消息里带 monitor 同族的 `CCM_GUARD_REJECTED`；
/// - 通过 ⇒ `Ok(session_id)`。
pub(crate) fn admit(name: &str, target: &str) -> Result<String, CmdErr> {
    let Some(p) = probe(target)? else {
        return Err((
            "no_such_session",
            format!("会话 {name:?} 不存在；send-into 只往**已存在**的会话键入，不新建"),
        ));
    };
    let verdict = gate_core::gate2(name, Some(&p.ccm_sid));
    if !verdict.allowed() {
        return Err((
            "wrong_owner",
            format!(
                "CCM_GUARD_REJECTED sid= —— 会话 {name:?} 既不是本工具的命名形状，\
                 远端 `@ccm_sid` 也没设 ⇒ 拒绝键入（§34 Gate 2）。\
                 这道门挡的是「往一个不是本工具管理的 tmux 会话里打字」。"
            ),
        ));
    }
    Ok(p.session_id)
}

/// **破坏性动作的门**：Gate 2（身份）**再加** Gate 3（`windows == 1`）。
///
/// # 为什么破坏性动作要多一道门
///
/// Gate 2 只回答「这是不是本工具的会话」。但一个**本工具建的**会话也可能被用户
/// 自己扩出了额外窗口（在里面开了别的东西）——把它整个杀掉就连带毁掉用户的活。
/// ⇒ Gate 3：**只杀「干净的单窗口会话」**。多窗口 ⇒ 拒绝，让用户自己去那个 tmux 里处理。
///
/// 与 monitor 侧逐条同义（`tmux.rs::build_guarded_tmux_cmd` 的 `[ "$w" = "1" ]`），
/// 拒绝码也保持同族（`CCM_GUARD_REJECTED windows=<n>`）。
///
/// ⚠ **Gate 3 只给破坏性动作**：`send-keys` 不删除任何东西，窗口数与它无关 ——
/// 给它加 Gate 3 会让「往一个多窗口会话里打字」被误拒（monitor 侧 F04 Phase D
/// 审计专门修过这个错法）。所以本函数与 [`admit`] **是两个入口，不是一个带 flag 的**。
pub(crate) fn admit_destructive(name: &str, target: &str) -> Result<String, CmdErr> {
    let Some(p) = probe(target)? else {
        return Err((
            "no_such_session",
            format!("会话 {name:?} 不存在；没有可杀的目标"),
        ));
    };
    if !gate_core::gate2(name, Some(&p.ccm_sid)).allowed() {
        return Err((
            "wrong_owner",
            format!(
                "CCM_GUARD_REJECTED sid= —— 会话 {name:?} 既不是本工具的命名形状，\
                 远端 `@ccm_sid` 也没设 ⇒ 拒绝杀它（§34 Gate 2）"
            ),
        ));
    }
    if p.windows != 1 {
        return Err((
            "too_many_windows",
            format!(
                "CCM_GUARD_REJECTED windows={} —— 会话 {name:?} 有 {} 个窗口，\
                 不是干净的单窗口会话 ⇒ 拒绝杀它（§34 Gate 3）。\
                 用户可能在里面开了别的东西；请到那个 tmux 里自行处理",
                p.windows, p.windows
            ),
        ));
    }
    Ok(p.session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★★ **K-R12：本模块两处起 tmux 的地方，`-u` 必须在子命令之前 —— 一处都不许漏。**
    ///
    /// # 为什么这一条只能是「扫源码」，以及它守不住什么
    ///
    /// 本模块的两处是 **argv 直传**（`Command::new("tmux")`），没有 builder 能把命令行取回来，
    /// 也没有办法在不污染整个测试进程 `PATH` 的前提下把它指向一个假 tmux
    /// （`Command::new` 走进程级 `PATH`，`std::env::set_var` 会波及并行跑的别的测试）。
    /// ⇒ **行为那一半的死值不在 cargo 里**，在 `evidence/K-R12-deathvalue.md`：
    /// 同样这两条 argv 对真 tmux 3.4 私有 socket 打过，改前 `段数=1`、改后 `段数=3`。
    ///
    /// 🔴 **本条守的是「别漏、别搬错位置」，不是「它真的生效了」**（「盘上有 ≠ 被走到」）。
    /// 位置这一维值得单独钉：`-u` 放到子命令**后面**是 `rc=1 + unknown flag -u`
    /// （实测），而本模块两处都**刻意不看退出码** ⇒ 那个响错在这里会退化成
    /// 「一个会话都没有」/「探不到」，**又变回一次静默失效**。
    ///
    /// # ★★ 09-09：这条判据原先把**排版**也一起断言了进去
    ///
    /// 原版的针是一整串跨元素的字面量：`.args([UTF8_CLIENT_FLAG, "<verb>"`。
    /// 那串里有一个空格与一个逗号 —— 也就是说它顺带断言了
    /// 「`-u` 与子命令必须在源码的**同一行**上」。而那一维**由 rustfmt 说了算**：
    /// 09-09 那趟 `cargo fmt --all` 把 `probe` 里的 `.args([…])` 拆成每元素一行，
    /// 本条当场红，判定行逐字「`display-message` 那一处没有把 `-u` 放在子命令**之前**
    /// （找不到 `.args([UTF8_CLIENT_FLAG, "display-message"`）」——
    /// **而 `-u` 一个字节都没挪过**。反向那两根针同时**静默失效**：
    /// 它们也是带空格的跨元素字面量，拆行之后永远零命中，
    /// 于是「有人把 `-u` 塞到子命令后面」这一格从此不会红。
    ///
    /// ⇒ 今天断言的是**次序关系本身**，与排版无关：
    ///
    /// - 正向：子命令那个字面量，与**它所属的那个 `.args([`** 之间，必须出现 `UTF8_CLIENT_FLAG`
    ///   （中间不许隔着 `]` —— 隔着就说明它压根不在那个数组里，本条在空转）；
    /// - 反向：把源码**全部空白删掉**之后，不许出现 `"<verb>",UTF8_CLIENT_FLAG`。
    #[test]
    fn both_tmux_call_sites_ask_for_a_utf8_client_before_the_subcommand() {
        // 本条数的是**源码文本**，所以要的是那个常量的**名字**，不是它的值。
        const FLAG_IDENT: &str = "UTF8_CLIENT_FLAG";
        const ARGS_OPEN: &str = ".args([";

        let prod = crate::guard_support::production_code(include_str!("gate.rs"));
        crate::guard_support::assert_no_test_code("control/gate.rs", &prod);

        let starts = prod.matches("Command::new(\"tmux\")").count();
        assert_eq!(
            starts, 2,
            "本模块起 tmux 的处数变了（实得 {starts}，登记 2）—— 新增的那一处也要带 `-u`，\
             并把这条判据的数一起改。**这张表不是豁免清单。**"
        );
        // 反向那一针用的人群：把排版这一维抹掉（`-u` 与子命令同不同行由 rustfmt 说了算）。
        let flat: String = prod.chars().filter(|c| !c.is_whitespace()).collect();
        for verb in ["list-sessions", "display-message"] {
            let quoted = format!("\"{verb}\"");
            // ── 正向：`-u` 排在子命令**之前** ──────────────────────────────
            let at = guard_core::find_pinned(&prod, &quoted).unwrap_or_else(|e| {
                panic!(
                    "`{verb}` 这个子命令在本模块生产段里定不了位（{e}）。\n\
                     ★ 起 tmux 那两处换了写法就来改本条 —— 别让它零命中地绿。"
                )
            });
            let open = prod[..at].rfind(ARGS_OPEN).unwrap_or_else(|| {
                panic!(
                    "`{verb}` 前面一个 `{ARGS_OPEN}` 都没有 —— 它已经不是 argv 直传了。\n\
                     ★ 换成别的传法（`arg()` 逐个加 / 拼 shell 串）本条就管不着了，回来改。"
                )
            });
            let before = &prod[open + ARGS_OPEN.len()..at];
            assert!(
                !before.contains(']'),
                "`{verb}` 与它前面那个 `{ARGS_OPEN}` 之间隔着一个右方括号 ——\n\
                 它根本不在那个数组里，本条此刻断言的是**别人的 argv**。"
            );
            assert!(
                before.contains(FLAG_IDENT),
                "`{verb}` 那一处没有把 `-u` 放在子命令**之前**\
                 （`{ARGS_OPEN}` 与它之间找不到 `{FLAG_IDENT}`）。\n\
                 放到后面是 rc=1 的响错，而本模块不看退出码 ⇒ 会退化成又一次静默失效。"
            );
            // ── 反向：也不许在子命令**后面**再塞一个（`tmux -u ls -u` 同样 rc=1）──
            let bad = format!("{quoted},{FLAG_IDENT}");
            assert!(!flat.contains(&bad), "`-u` 被放到了子命令后面：{bad}");
        }
    }

    /// ★★ **K-R12 `J1` 死值验（本模块这一侧）：段数下溢必须红。**
    ///
    /// 死值取自 `evidence/K-R12-deathvalue.md` ①：真 tmux 3.4 + POSIX 客户端下，
    /// `list-sessions` 那三列打出来是 `kr12_$0_cc-deadval1`、
    /// `display-message` 那三列打出来是 `$0_cc-deadval1_1` —— **TAB 全没了，段数 1**。
    ///
    /// ⚠ 过溢那一档**不在这里**：`PROBE_FMT`/`LIST_FMT` 的列里没有路径，
    /// 三列的取值域都排除真 TAB ⇒ 合法内容推不高段数（理由见 `PROBE_FMT_FIELDS` 头注）。
    /// 但判据仍写成「下溢」而不是「不等于」，与另外两处同一口径 —— **口径一致本身是要买的东西**。
    #[test]
    fn the_underflow_predicate_catches_the_real_dirty_bytes() {
        assert!(
            tab_underflow("kr12_$0_cc-deadval1", 3),
            "真 tmux 打出来的脏字节必须判下溢"
        );
        assert!(tab_underflow("$0_cc-deadval1_1", 3), "同上（probe 那一条）");
        assert!(
            !tab_underflow("kr12\t$0\tcc-deadval1", 3),
            "干净的三段必须放行"
        );
        assert!(
            !tab_underflow("$0\t\t1", 3),
            "🔴 `@ccm_sid` **没设**是合法的（中间那段是空串）—— 它与「拆不出」是两件事，不许判红"
        );
        assert!(
            !tab_underflow("a\tb\tc\td", 3),
            "过溢不许红（口径与另外两处一致：判的是下溢，不是不等于）"
        );
    }

    /// 判定表的**唯一真相源**，三条轨道各自独立读它（见文件头注）。
    const GOLDEN: &str =
        include_str!("../../../src-tauri/src/backend/control/fixtures/gate2-golden.tsv");

    fn golden_rows() -> Vec<(String, String, Option<String>, String)> {
        GOLDEN
            .lines()
            .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
            .map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                assert_eq!(f.len(), 4, "夹具行不是 4 列：{l:?}");
                let sid = match f[2] {
                    "<none>" => None,
                    "<unset>" => Some(String::new()),
                    v => Some(v.to_string()),
                };
                (f[0].to_string(), f[1].to_string(), sid, f[3].to_string())
            })
            .collect()
    }

    /// ★ 抽取器自检：夹具读不到 / 解析空时，下面那条会零命中地绿。
    #[test]
    fn the_golden_table_is_actually_read_from_the_monitor_side_fixture() {
        let rows = golden_rows();
        assert!(
            rows.len() >= 20,
            "只解析出 {} 行夹具 —— 路径或解析坏了（这份夹具住在 monitor 那边，跨仓相对路径）",
            rows.len()
        );
        // 三种结论都必须在表里出现，否则表本身是偏的。
        for want in ["allowed_by_name", "allowed_by_remote_sid", "rejected"] {
            assert!(
                rows.iter().any(|r| r.3 == want),
                "夹具里一行 `{want}` 都没有 —— 表偏了，下面那条测不到那一支"
            );
        }
    }

    /// ★ daemon 这一侧对同一张表给出同样的判定。
    ///
    /// ⚠ **不许改成「调 monitor 的实现来对拍」** —— 两侧一起错就全绿了。
    /// 两侧各自独立读这张表，才叫跨轨。
    #[test]
    fn the_daemon_side_agrees_with_the_golden_table() {
        let mut bad = Vec::new();
        for (id, name, sid, want) in golden_rows() {
            let got = gate_core::gate2(&name, sid.as_deref()).as_str();
            if got != want {
                bad.push(format!(
                    "  {id}: name={name:?} sid={sid:?} 期望={want} 实得={got}"
                ));
            }
        }
        assert!(
            bad.is_empty(),
            "daemon 侧与判定表不一致：\n{}",
            bad.join("\n")
        );
    }

    /// ★ 生产接线：`admit` 通过之后回的是**句柄**，不是名字 —— TOCTOU 那条的落点。
    ///
    /// 这里只钉「解析出来的形状」；真 tmux 上的行为由
    /// `e2e/daemon-gate2-acceptance.sh` 钉（那才是真二进制那一轨）。
    #[test]
    fn a_probe_line_parses_into_a_handle_and_a_sid() {
        // 直接构造探测输出的解析结果，不起进程（起进程是 e2e 的事）。
        let line = "$3\tabc123\t1";
        let mut it = line.split('\t');
        let p = Probed {
            session_id: it.next().unwrap().to_string(),
            ccm_sid: it.next().unwrap().to_string(),
            // F04a：第三段是 `#{session_windows}`。**解析不出来 ⇒ 0**，而 Gate 3 要求恰好 1
            // ⇒ fail closed（拿不到窗口数就不许杀）。
            windows: it.next().unwrap_or_default().parse().unwrap_or(0),
        };
        assert_eq!(p.session_id, "$3");
        assert_eq!(p.ccm_sid, "abc123");
        assert_eq!(p.windows, 1);
        assert!(
            p.session_id.starts_with('$'),
            "tmux 的 session_id 恒是 `$N` —— 不是这个形状就说明格式串被动过了"
        );
    }

    /// ★ F04a：**Gate 3 拿不到窗口数时 fail closed**（解析失败 ⇒ 0 ⇒ 拒绝）。
    ///
    /// 反向的错法（解析失败当 1）会把「探测被截断」变成「放行一次 kill」——
    /// 那是本仓最不能接受的一类默认值。
    #[test]
    fn gate3_fails_closed_when_the_window_count_is_unreadable() {
        for line in ["$1\tsid", "$1\tsid\t", "$1\tsid\tnot-a-number"] {
            let mut it = line.split('\t');
            it.next();
            it.next();
            let w: u32 = it.next().unwrap_or_default().trim().parse().unwrap_or(0);
            assert_ne!(
                w, 1,
                "{line:?} 解析出的窗口数不该等于 1（那会放行一次 kill）"
            );
        }
        let mut it = "$1\tsid\t1".split('\t');
        it.next();
        it.next();
        assert_eq!(it.next().unwrap().parse::<u32>().unwrap(), 1);
    }

    /// ★ 格式串里必须**同时**有句柄、sid 与窗口数：少了句柄就退回「对名字下手」＝TOCTOU 回归，
    /// 少了 sid 就等于没有 Gate 2，少了窗口数就等于没有 Gate 3。
    #[test]
    fn the_probe_format_asks_for_both_fields() {
        assert!(
            PROBE_FMT.contains("#{session_id}"),
            "少了句柄 ⇒ TOCTOU 窗口回来了"
        );
        assert!(
            PROBE_FMT.contains("#{@ccm_sid}"),
            "少了 sid ⇒ 这道门就是空的"
        );
        assert!(
            PROBE_FMT.contains("#{session_windows}"),
            "少了窗口数 ⇒ Gate 3 没有输入，破坏性动作会误杀多窗口会话"
        );
        assert!(
            !PROBE_FMT.contains("@ccm_sid_expect"),
            "**只认 `@ccm_sid`** —— `_expect` 是「声明了但未必跑起来」的意图，不是事实"
        );
    }

    // ── audit-0805 F20 下半：把「skip 不是通过」那个刻意决定钉住 ──────────────
    //
    // F20 的处境：`meta_dollar`（会话名 `cc-a$x`）在 CI 上报 `no_such_session`，
    // **根因至今未知** —— 复现要那台机器的 tmux，红线禁真 tmux、且已裁定不再推 CI。
    // 上半做的是「把一条误导性的失败改成一条**会自证**的失败」：建完用 `=name:` 复核，
    // 找不到就 **skip 并打出 tmux 实况**。
    //
    // 而 §3 那个决定 —— **地板不动（36），skip 会让 PASS 少一个 ⇒ 地板照样红** ——
    // 今天**只是一段散文**。有人为了让 CI 变绿把 36 改成 35，这个意图就静默消失了，
    // 而那正是本区 F25 那一族（一个刻意的决定没有判据看着）。
    //
    // ⚠ 本组**不修根因，也不假装修了** —— 它只保证那个决定不会被悄悄推翻。

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf()
    }

    /// ★ `daemon-gate2` 的地板**只许涨**，而且必须**盖得住判定表的行数**。
    ///
    /// 地板低于用例数时，「有一格没验到」就不会让任何东西变红 —— skip 变成了免费的。
    #[test]
    fn the_gate2_floor_still_makes_a_skip_hurt() {
        let root = repo_root();
        let ci =
            std::fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("ci.yml 读不到");
        let mark = "assert-pass-floor.sh daemon-gate2 ";
        let at = ci
            .find(mark)
            .expect("ci.yml 里没有 `assert-pass-floor.sh daemon-gate2 <地板>` 调用行");
        let floor: usize = ci[at + mark.len()..]
            .split_whitespace()
            .next()
            .and_then(|t| t.trim().parse().ok())
            .expect("地板值解析不出来 —— 调用行的形状变了");

        let golden = std::fs::read_to_string(
            root.join("src-tauri/src/backend/control/fixtures/gate2-golden.tsv"),
        )
        .expect("判定表读不到");
        let rows = golden
            .lines()
            .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
            .count();
        // 抽取器自检：判定表解析不出行时，下面那条会零命中地绿。
        assert!(
            rows >= 20,
            "判定表只解析出 {rows} 行（08-06 实测 25）—— 抽取器坏了，下面那条此刻是空转的"
        );

        // ★ 只许涨。08-06 实测 36：判定表 25 行 + 抽取器自检 + 其余固定项。
        const FLOOR_TODAY: usize = 36;
        assert!(
            floor >= FLOOR_TODAY,
            "`daemon-gate2` 的地板被降到了 {floor}（08-06 是 {FLOOR_TODAY}）。\n\
             ★ **降它就是把「skip 不是通过」这个决定推翻了**：`meta_dollar` 在某些 tmux 上会 skip，\n\
             PASS 因此少一个；地板不动 ⇒ 红 ⇒ 「有一格没验到」看得见。\n\
             地板降下去，那一格就静默消失了 —— 而**根因至今未知**（F20 §2）。\n\
             真要降，先把根因查清并说明为什么那一格不必再验。"
        );
        assert!(
            floor > rows,
            "地板 {floor} 没盖住判定表的 {rows} 行 —— skip 一格也不会让它红，\n\
             那个决定就成了空话。加用例时地板要跟着抬。"
        );
    }

    /// ★ 那个决定的**前提**：脚本里那条「建完复核、找不到就 skip 并打实况」还在。
    ///
    /// 它一没，失败就退回**误导性**的那种（看起来像 Gate 2 判错，实际是夹具没准备好）——
    /// 那正是 F20 上半修掉的东西。
    #[test]
    fn the_selfevidencing_skip_branch_is_still_there() {
        let sh = std::fs::read_to_string(repo_root().join("e2e/daemon-gate2-acceptance.sh"))
            .expect("e2e 脚本读不到");
        for needle in ["has-session -t \"=$name:\"", "实际会话："] {
            assert!(
                sh.contains(needle),
                "e2e 脚本里找不到 `{needle}` —— 「建完复核 + skip 时打出 tmux 实况」那段没了。\n\
                 ★ 它一没，`meta_dollar` 的失败就退回**误导性**的那种：\n\
                 看起来像「Gate 2 判错了」，实际是「夹具没准备好」。那是 F20 上半修掉的东西。\n\
                 ⚠ 同时上面那条地板判据也失去意义 —— 它保护的正是这条 skip 的可见性。"
            );
        }
    }
}
