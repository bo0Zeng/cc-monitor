//! 要求住址：`INVARIANTS §34`（破坏性动作过三道门，门只住后端）·「共享 crate 只放契约、判定只在后端」。
//!
//! **§34 Gate 2（identity）与 tmux 会话名两条规则的唯一实现** ——「这个 tmux 会话是不是本工具管的？」「这个名字能不能建 / 能不能寻址？」
//!
//! 它从前是共享 crate `gate-core`（F03 立：当时 monitor 与后端各有一份门）。monitor 侧最后两处（Gate 1 前检 ·
//! `is_ccm_tmux_name` 的转调壳，只剩跨轨对拍锚点在用）删了之后，消费者只剩后端 `control/` 一层
//! （`gate.rs` 送键 / 杀会话之前那道门 · `launch.rs` · `kill.rs` · `launch_render/` · `ccm/plan.rs`）⇒ 收成后端模块，共享 crate 那一格没了。
//! 金表 `tests/__fixtures__/gate2-golden.tsv` 两个读者：`gate_tests.rs` 与 e2e `backend-gate2-acceptance.sh`。
//!
//! # 边界：本模块只判，不取
//!
//! 「怎么问那台 `@ccm_sid`」住 `gate.rs::probe`（就在那台机器上 argv 直传跑一次 `tmux display-message`）；
//! 本模块不起进程、不碰 tmux、不认识 shell。
//!
//! # ⚠ `@ccm_sid` 不是 `@ccm_sid_expect`
//!
//! 通道 A（意图）写 `@ccm_sid_expect`，只有通道 B（后端 `control/identity_tag.rs` 读会话文件确认后）才写 `@ccm_sid`。
//! 调用方喂进 [`gate2`] 的必须是 `@ccm_sid`。**放宽到 `_expect` 就是把这道门拆了。**

/// Gate 2 的判定结果。**三态而不是 bool** —— 两种「允许」的代价不同：
/// 名字命中是零 IO 的，`@ccm_sid` 命中是花了一次 round-trip 换来的。
/// 调用方要靠这个区别决定「值不值得先问一次远端」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Gate2 {
    /// 名字形状本身就是「这是我们的会话」的证明 —— **零 IO，不必问远端**。
    AllowedByName,
    /// 远端 `@ccm_sid` 已设 —— 问过远端才拿到的允许。
    AllowedByRemoteSid,
    /// 两支都不满足 ⇒ 拒绝。
    Rejected,
}

impl Gate2 {
    /// 允不允许动这个会话。
    pub(crate) fn allowed(self) -> bool {
        !matches!(self, Gate2::Rejected)
    }

    /// 跨轨对拍用的稳定名字（入库夹具的 `expect` 列、e2e 的比对值都用它）。
    ///
    /// ⚠ **不要用 `{:?}`** —— `Debug` 是给人看的、改它不算 breaking change，
    /// 而夹具里那一列一旦跟着变就成了「两侧一起漂」。这里显式钉死字面量。
    /// 生产段不读它（它是金表那一格的名字，读者是 `gate_tests.rs`）⇒ 精确 allow。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Gate2::AllowedByName => "allowed_by_name",
            Gate2::AllowedByRemoteSid => "allowed_by_remote_sid",
            Gate2::Rejected => "rejected",
        }
    }
}

/// 本工具建的 tmux 会话名判定：`cc-<体>` 前缀，或 `<X>-cc` / `<X>-cc-<N>` 后缀，
/// 且整名只含 `[A-Za-z0-9_-]`。
///
/// # 两种形状都要认，**一个都不许删**
///
/// - 新形 `<X>-cc`（撞名时 `<X>-cc-2`）是 S4b-3b（用户 2026-07-31）定的；
/// - 老形 `cc-<sid8>` 必须一并保留：F02 之前的老 `cc-*` 会话**没有 `@ccm_sid`**，
///   只靠这条前缀判据仍必须可 kill/send-keys。删了就是把用户**正在跑的**会话
///   变成 issue #76 那种「失管会话」。
///
/// # 字符集那一条不是洁癖
///
/// 名字会被拼进 `ccm …` 调用行那类穿过 shell 的命令串。虽然那条路另有 `posix_quote` 兜底，
/// 但**身份判定自己也拒绝元字符**，是为了让「名字命中 ⇒ 跳过远端核验」这条零 IO 快路
/// 不依赖下游的引号化正确性。
pub(crate) fn is_ccm_tmux_name(name: &str) -> bool {
    let charset_ok = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let old_prefix = name.starts_with("cc-") && name.len() > 3;
    // 后缀形态：`<X>-cc` 或 `<X>-cc-<N>`（撞名避让）。要求 `<X>` 非空，
    // 否则裸 `-cc` 这种退化名也会命中。
    let new_suffix = name
        .split("-cc")
        .next()
        .is_some_and(|head| !head.is_empty() && head.len() < name.len())
        && (name.ends_with("-cc")
            || name
                .rsplit_once("-cc-")
                .is_some_and(|(_, n)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())));
    charset_ok && (old_prefix || new_suffix)
}

// `needs_remote_sid`〔散文墓碑〕删：它是给「两侧同一个取反」立的名字，monitor 那一侧没了之后零调用方。

/// Gate 2 union：名字命中 **或** 远端 `@ccm_sid` 已设。
///
/// `remote_sid` 的三种取值都有意义，**别把它们折成 bool**：
/// - `None` = **没问 / 问不到**（会话不存在、tmux 不在、探测失败）；
/// - `Some("")` = 问了，**没设**；
/// - `Some(非空)` = 问了，设了。
///
/// `None` 与 `Some("")` 今天都判 `Rejected`（**fail closed**），
/// 但保留区别是为了让调用方能给出不同的诊断（「会话不存在」vs「不是本工具的会话」）——
/// 这正是 `gate.rs` 里 `no_such_session` 与 `wrong_owner` 的分界。
pub(crate) fn gate2(name: &str, remote_sid: Option<&str>) -> Gate2 {
    if is_ccm_tmux_name(name) {
        return Gate2::AllowedByName;
    }
    match remote_sid {
        Some(s) if !s.is_empty() => Gate2::AllowedByRemoteSid,
        _ => Gate2::Rejected,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// **tmux 会话名的形状 —— 全仓唯一一份**。
//
// 此前三份、规则互不相同（`DUP1.md §0.3`）：后端 `ccm/plan.rs::validate_tmux_name`（非空 · 不以 `-` 开头 · 无 `*?.:=` · 无控制符）·
// monitor 载荷 `TmuxTarget::check`（`Raw` 只放行 `[A-Za-z0-9_-]`、却放过前导 `-`；`Quoted` 拒控制符与欺骗字符）·
// 界面 `shell-quote.ts` 两个谓词 ＋ `launch-requests.ts` 两处内联式子（F01「不把 glob 建进名字」在载荷那条路上只有界面那一道）。
// 今天两条规则住这里，按「这个名字是不是本工具要**建**的」分：
//
// - **新建**（本工具铸的名，`INVARIANTS §47` ① 那一族）：非空 · 不以 `-` 开头（`new-session -s -x` 会被当选项）·
//   无 [`NEW_TMUX_NAME_REFUSED`]（`.` `:` `=` 是 tmux 目标语法、`*` `?` 是 glob —— F01「本工具永远不把 glob 字符建进会话名」·
//   F04b「别建一个主路杀不掉的名字」）· 无控制符与视觉欺骗字符 · ≤ [`NEW_TMUX_NAME_MAX`] 个字符；
// - **已有会话**（attach / 送进一个已在的会话，`§47` ② 那一形）：非空 · 无控制符与视觉欺骗字符（**拒绝集**，不是白名单：
//   那些名字不是我们建的，里面真有 glob 字符）。寻址恒走 tmux 精确匹配形 `=<名>:`（`INVARIANTS §31a`：`*` `?` 不被当通配、
//   前导 `-` 不被当选项 —— DUP2 在隔离 socket 上现打过，读数在）。
//
// 本模块只判、不说：各调用处按 [`TmuxNameIssue`] 用自己的文案出声（句子不进本模块）。
// 欺骗字符表是 `acct_core::is_deceptive_char` 那一张权威表（`§47` ② 的拒绝集），不在这里另抄一份。
// ═══════════════════════════════════════════════════════════════════════════

/// 本工具**新建**的会话名最长多少个字符（按 Unicode 标量数）。
pub(crate) const NEW_TMUX_NAME_MAX: usize = 128;

/// 新建会话名里不许出现的字符：tmux 目标语法 `.` `:` `=` 与 glob `*` `?`。
///
/// 刻意写成**一个字符串字面量**：创建路径那条跨轨判据（`backend_kill_tests.rs` 的 `VALIDATORS`）钉的就是这个表达式本身，
/// 并逐个核后端 kill 形状门拒的每个字符都在里面。
pub(crate) const NEW_TMUX_NAME_REFUSED: &str = "*?.:=";

/// 一个会话名为什么不行（调用处按它说自己的那一句）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TmuxNameIssue {
    /// 空串。
    Empty,
    /// 以 `-` 开头（只对新建判）。
    LeadingDash,
    /// 含 [`NEW_TMUX_NAME_REFUSED`] 里的一个字符（只对新建判）。
    TargetSyntax(char),
    /// 含控制字符（`char::is_control`：C0 · DEL · C1）。
    Control(char),
    /// 含视觉欺骗字符（`acct_core::is_deceptive_char`）。
    Deceptive(char),
    /// 超过 [`NEW_TMUX_NAME_MAX`] 个字符（只对新建判）。
    TooLong,
}

/// 两条规则共用的拒绝集：控制符 · 视觉欺骗字符。
fn refused_char(n: &str) -> Option<TmuxNameIssue> {
    if let Some(c) = n.chars().find(|c| c.is_control()) {
        return Some(TmuxNameIssue::Control(c));
    }
    n.chars()
        .find(|c| acct_core::is_deceptive_char(*c))
        .map(TmuxNameIssue::Deceptive)
}

/// **新建**会话名（本工具铸的 · 用户给的新名）过不过：`None` = 过。规则见本节头注。
pub(crate) fn new_tmux_name_issue(n: &str) -> Option<TmuxNameIssue> {
    if n.is_empty() {
        return Some(TmuxNameIssue::Empty);
    }
    if n.starts_with('-') {
        return Some(TmuxNameIssue::LeadingDash);
    }
    if let Some(c) = n.chars().find(|c| NEW_TMUX_NAME_REFUSED.contains(*c)) {
        return Some(TmuxNameIssue::TargetSyntax(c));
    }
    if let Some(issue) = refused_char(n) {
        return Some(issue);
    }
    if n.chars().count() > NEW_TMUX_NAME_MAX {
        return Some(TmuxNameIssue::TooLong);
    }
    None
}

/// **已有会话**的名字（attach · 送进一个已在的会话）过不过：`None` = 过。只拒空、控制符与视觉欺骗字符（②）。
pub(crate) fn existing_tmux_name_issue(n: &str) -> Option<TmuxNameIssue> {
    if n.is_empty() {
        return Some(TmuxNameIssue::Empty);
    }
    refused_char(n)
}

#[cfg(test)]
#[path = "../../../tests/backend/control/gate_rules_tests.rs"]
mod tests;
