//! **§34 Gate 2（identity）的唯一实现** —— 「这个 tmux 会话是不是本工具管的？」
//!
//! # 为什么要单独一个 crate（F03，定框 C1/C6）
//!
//! 这道门挡的是「往一个不是本工具管理的 tmux 会话里打字 / 把它杀掉」。
//! 在 F03 之前它**只活在 monitor 一侧**，而且被拆成了两半、两种语言：
//!
//! - 本地半支：`src-tauri/src/tmux.rs` 里一个私有的 `is_ccm_tmux_name`；
//! - 远端半支：`build_guarded_tmux_cmd` 拼出来的 **shell 串** 里那句 `[ -n "$sid" ]`。
//!
//! daemon 的 `control/launch.rs` 则**完全没有**这道门 —— 它建会话时 `set-option` **写**
//! `@ccm_sid`，却从不**核验**它。于是「把 send-keys/kill 改走 daemon」会**静默丢掉一道门**：
//! 功能看起来一样、门禁全绿，而门没了。那条路此前由 `tmux_daemon_gate_guard`
//! 这条**前提触发器**挡着（「daemon 一出现身份守卫就主动红，逼人回来重新裁定」）。
//!
//! ⇒ 定框 C1「backend 一份代码、两种承载」在这里的落地就是：
//! **判定收进本 crate，两侧各自负责怎么把 `remote_sid` 取回来。**
//!
//! # 边界：本 crate 只判，不取
//!
//! 「怎么问远端 `@ccm_sid`」两侧形态完全不同 ——
//! monitor 拼一条穿过 ssh + shell 的原子命令（`display-message` 与动作同一个 round-trip，
//! 防 TOCTOU）；daemon 就在那台机器上，argv 直传跑一次 `tmux display-message`。
//! 把取值也塞进来就得引入平台/进程/shell，共享当场破掉。
//!
//! # ⚠ `@ccm_sid` 不是 `@ccm_sid_expect`
//!
//! `shared/ccm` 刻意分了两个 option：通道 A（意图）写 `@ccm_sid_expect`，
//! 只有通道 B（poller 独立读会话文件确认后）才写 `@ccm_sid`。
//! 原注释逐字写着「**破坏性动作只认 `@ccm_sid`**」——
//! 因为一个「声明了但从未真正跑起来」的 sid 不该永久冒充事实。
//! ⇒ 调用方喂进 [`gate2`] 的必须是 `@ccm_sid`。**放宽到 `_expect` 就是把这道门拆了。**

/// Gate 2 的判定结果。**三态而不是 bool** —— 两种「允许」的代价不同：
/// 名字命中是零 IO 的，`@ccm_sid` 命中是花了一次 round-trip 换来的。
/// 调用方要靠这个区别决定「值不值得先问一次远端」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate2 {
    /// 名字形状本身就是「这是我们的会话」的证明 —— **零 IO，不必问远端**。
    AllowedByName,
    /// 远端 `@ccm_sid` 已设 —— 问过远端才拿到的允许。
    AllowedByRemoteSid,
    /// 两支都不满足 ⇒ 拒绝。
    Rejected,
}

impl Gate2 {
    /// 允不允许动这个会话。
    pub fn allowed(self) -> bool {
        !matches!(self, Gate2::Rejected)
    }

    /// 跨轨对拍用的稳定名字（入库夹具的 `expect` 列、e2e 的比对值都用它）。
    ///
    /// ⚠ **不要用 `{:?}`** —— `Debug` 是给人看的、改它不算 breaking change，
    /// 而夹具里那一列一旦跟着变就成了「两侧一起漂」。这里显式钉死字面量。
    pub fn as_str(self) -> &'static str {
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
/// monitor 侧会把名字拼进一条穿过 shell 的命令串。虽然那条路另有 `posix_quote` 兜底，
/// 但**身份判定自己也拒绝元字符**，是为了让「名字命中 ⇒ 跳过远端核验」这条零 IO 快路
/// 不依赖下游的引号化正确性。
pub fn is_ccm_tmux_name(name: &str) -> bool {
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

/// 名字没命中时才需要花一次 round-trip 去问远端 `@ccm_sid`。
///
/// 单独给它一个名字（而不是让调用方写 `!is_ccm_tmux_name(..)`），是为了让
/// 「要不要多一次 round-trip」这个决策在两侧**是同一个函数**，
/// 而不是两处各写一个取反 —— 那种形状漂起来不会红。
pub fn needs_remote_sid(name: &str) -> bool {
    !is_ccm_tmux_name(name)
}

/// Gate 2 union：名字命中 **或** 远端 `@ccm_sid` 已设。
///
/// `remote_sid` 的三种取值都有意义，**别把它们折成 bool**：
/// - `None` = **没问 / 问不到**（会话不存在、tmux 不在、探测失败）；
/// - `Some("")` = 问了，**没设**；
/// - `Some(非空)` = 问了，设了。
///
/// `None` 与 `Some("")` 今天都判 `Rejected`（**fail closed**），
/// 但保留区别是为了让调用方能给出不同的诊断（「会话不存在」vs「不是本工具的会话」）——
/// 这正是 monitor 侧 `CCM_NO_SESSION` 与 `CCM_GUARD_REJECTED` 的分界。
pub fn gate2(name: &str, remote_sid: Option<&str>) -> Gate2 {
    if is_ccm_tmux_name(name) {
        return Gate2::AllowedByName;
    }
    match remote_sid {
        Some(s) if !s.is_empty() => Gate2::AllowedByRemoteSid,
        _ => Gate2::Rejected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_cc_prefix_is_still_recognised() {
        // 删掉这一支 = 把用户正在跑的老会话变成失管会话（见 `is_ccm_tmux_name` 头注）。
        assert!(is_ccm_tmux_name("cc-abc12345"));
        assert!(is_ccm_tmux_name("cc-proj"));
        assert!(is_ccm_tmux_name("cc-abc12345-2")); // pickFreshTmuxName 的 -N 变体
        assert!(!is_ccm_tmux_name("cc-")); // 只前缀无体
    }

    #[test]
    fn new_cc_suffix_shape_is_recognised() {
        assert!(is_ccm_tmux_name("abc12345-cc"));
        assert!(is_ccm_tmux_name("abc12345-cc-2"));
        assert!(is_ccm_tmux_name("my-proj-cc"));
        assert!(!is_ccm_tmux_name("-cc")); // `<X>` 为空的退化名
        assert!(!is_ccm_tmux_name("foo-ccx"));
        assert!(!is_ccm_tmux_name("foo-cc-bar")); // `-cc-` 后面不是纯数字
    }

    #[test]
    fn other_peoples_sessions_are_not_ours() {
        assert!(!is_ccm_tmux_name("web")); // 用户自己的会话
        assert!(!is_ccm_tmux_name("mycc-x")); // 非前缀
        assert!(!is_ccm_tmux_name("foo_cc")); // cc-bus 的 `_cc` 命名空间（ROADMAP §6 待决 #2）
    }

    #[test]
    fn shell_metacharacters_never_pass_the_name_half() {
        // 这几条守的是「名字命中 ⇒ 跳过远端核验」那条零 IO 快路。
        assert!(!is_ccm_tmux_name("cc-a b")); // 空格
        assert!(!is_ccm_tmux_name("cc-a;rm")); // 分号
        assert!(!is_ccm_tmux_name("cc-a$x")); // 元字符
        assert!(!is_ccm_tmux_name("cc-a:b")); // tmux 目标语法
        assert!(!is_ccm_tmux_name("=cc-a")); // tmux 精确匹配前缀
    }

    #[test]
    fn the_union_allows_by_name_without_asking_the_remote() {
        // 名字命中时**连 remote_sid 都不看** —— 三种取值结论必须一致。
        for sid in [None, Some(""), Some("whatever")] {
            assert_eq!(gate2("cc-abc12345", sid), Gate2::AllowedByName);
        }
        assert!(!needs_remote_sid("cc-abc12345"));
    }

    #[test]
    fn the_union_allows_a_custom_name_only_when_the_remote_sid_is_set() {
        assert!(needs_remote_sid("e2e-custom"));
        assert_eq!(
            gate2("e2e-custom", Some("abc123")),
            Gate2::AllowedByRemoteSid
        );
        assert_eq!(gate2("e2e-custom", Some("")), Gate2::Rejected);
        assert_eq!(gate2("e2e-custom", None), Gate2::Rejected);
    }

    #[test]
    fn unknown_state_fails_closed() {
        // 探测失败（`None`）绝不能等价于「允许」。这条是 fail-closed 的落点。
        assert!(!gate2("someones-session", None).allowed());
        assert!(!gate2("someones-session", Some("")).allowed());
    }

    #[test]
    fn the_wire_names_are_pinned_not_derived_from_debug() {
        // 夹具与 e2e 都比这几个字面量；跟着 `Debug` 漂就成了「两侧一起漂」。
        assert_eq!(Gate2::AllowedByName.as_str(), "allowed_by_name");
        assert_eq!(Gate2::AllowedByRemoteSid.as_str(), "allowed_by_remote_sid");
        assert_eq!(Gate2::Rejected.as_str(), "rejected");
    }
}
