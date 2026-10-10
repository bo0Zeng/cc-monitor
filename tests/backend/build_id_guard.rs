//! E77：**加了子命令就必须 bump `BUILD_ID`** —— 把这条从「记性」变成机检。
//!
//! # 为什么需要它
//!
//! monitor 判「远端那台的后端该不该换」只有一条判据：
//! 那台报的 build_id ≠「我这一版」（monitor 手上那份内嵌字节自报的 id，由发版从本文件的
//! `BUILD_ID` 编出）。**不 bump ⇒ 已部署的旧后端报同一个 id ⇒ 不判 stale ⇒ 不自动重装
//! ⇒ 整轮改动在已部署的远端休眠。**
//!
//! 这一课在 `main.rs` 的版本谱系里被写过两遍（p1r 段、p1t 段），`src/doc/INVARIANTS.md` §41.5
//! 又写了一遍 —— **写了三遍，2026-08-01 的 Phase G 审计仍然逮到第三次漏做**（G2 加了
//! `--fork-session` 却没 bump）。⇒ 靠散文提醒是无效的。
//!
//! # 判据：子命令集的指纹 ↔ BUILD_ID 的历史表
//!
//! `SUBCOMMAND_HISTORY` 每行 = `(BUILD_ID, 那一版的子命令集指纹)`，只留**上一版 ＋ 当前版**两行
//! （更早的在 git 历史里）。本护栏断言两件事：
//!
//! 1. **当前算出来的指纹必须在表里**（不是「等于最后一行」——见下）；
//! 2. 表里**不许有重复的 BUILD_ID**，且恰好两行。
//!
//! 于是「加一个子命令」⇒ 指纹是新的 ⇒ 不在表里 ⇒ 红。要弄绿只能追加一行（并删最老那行）；
//! 而用**当前（未 bump 的）id** 追加会撞上第 2 条 ⇒ **只剩「bump + 追加」这一条路**，
//! 也就是本来就该做的那件事。
//!
//! **为什么不是「等于最后一行」**（头一版是那么写的）：那会让**因为别的原因 bump BUILD_ID**
//!（如 p1v 只加了个 wire 字段、子命令一个没动）也被逼着改这张表 ——
//! 而「改表」恰恰是本护栏最不想诱导的动作。
//!
//! # 这条护栏**挡不住**什么（说清楚，别让人以为它是证明）
//!
//! 1. **有人可以就地改最后一行的指纹而不 bump。** 表在同一个文件里，护栏读不到 git 历史。
//!    能做的只有把「正确动作」变成最省事的那个，并让错误信息把代价说清楚。
//!    真要堵死得靠 CI 比对 `git show HEAD~1` —— 那需要 CI 里有完整历史（`fetch-depth: 0`），
//!    成本高于收益，**如实登记为未做**。
//! 2. **「子命令集没变但行为变了」它不管**（p1r 那次就是：删轮询、加事件，argv 表面没动）。
//!    那类仍然要靠人判断。本护栏只覆盖 p1t / G2 这一类「加/改子命令」——
//!    而那恰好是本仓栽过的两次里的两次。
//!
//! # 🔴 `K-R70`（09-12）：本模块从此守**两件事**，别把它读成只守 bump
//!
//! 上面全篇讲的是「**该不该** bump」。`K-R68` 摸底逮出来的是另一格：
//! **拿起一份后端二进制，产品判断不了它是不是我们以为的那一份** ——
//! 三个载体的身份全靠旁边那个 `.build_id` 文本文件，而它是从同一处源码常量抠的标签
//! ⇒ 三份恒等 ⇒ 一格证据都不提供（`DECISIONS.md#R26` 裁定零）。
//! 本模块下半段（`the_build_stamp_is_byte_scannable_in_this_very_binary` 起）守的是
//! **身份长在二进制自己身上**这条性质。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
mod tests {
    /// `(BUILD_ID, 子命令集指纹)`：**只留两行 —— 上一版 ＋ 当前版**。更早的各版在 git 历史里
    /// （`git log -p -- tests/backend/build_id_guard.rs`），表里不再一版一整份地堆。
    ///
    /// 子命令集变了 ⇒ bump `BUILD_ID`，**追加**一行新的，并删掉最老那一行（表恒为两行，
    /// `history_has_no_duplicate_build_ids` 钉着）。因为别的原因 bump、子命令一条没动 ⇒ 表不动。
    ///
    /// 指纹 = 排序去重后的子命令名用 `\n` 连起来（见 `subcommand_fingerprint`）——
    /// 刻意用**明文**而不是哈希：出错时的诊断信息直接就是「多了/少了哪个」，
    /// 而哈希只会告诉你「不一样」。
    const SUBCOMMAND_HISTORY: &[(&str, &str)] = &[
        (
            "p9y-viewer-tz-skeleton",
            "--account-trust\n--account-trust-zero\n--accounts-add\n--accounts-init\n--accounts-list\n--accounts-login-cmd\n--accounts-mcp-pick\n--accounts-mcp-read\n--accounts-mcp-remove\n--accounts-mcp-sync\n--accounts-remove\n--accounts-repair\n--accounts-rollback\n--accounts-sessions\n--accounts-set-default\n--accounts-trust\n--accounts-verify\n--agent-home-check\n--aliases-block-install\n--aliases-block-remove\n--aliases-block-render\n--aliases-read\n--apikey-key-set\n--apikey-read\n--assets-catalog\n--assets-catalog-merge\n--assets-sync\n--authorized-keys-add\n--backend-log\n--backend-probe\n--bus-broadcast\n--bus-inbox\n--bus-kill\n--bus-list\n--bus-send\n--bus-spawn\n--bus-state\n--cc-bus-install\n--cc-bus-install-state\n--cells-catalog\n--chores-mark\n--data-report\n--deploy-plan\n--drift-report\n--exit-policy-read\n--exit-policy-set\n--ext-list-here\n--ext-note-set\n--ext-uninstall-apply\n--ext-uninstall-preview\n--files-browse\n--files-chmod\n--files-commit-text\n--files-commit-upload\n--files-copy\n--files-create\n--files-delete\n--files-delete-session\n--files-extract\n--files-find\n--files-grep\n--files-home\n--files-index-rebuild\n--files-index-status\n--files-link\n--files-ls\n--files-mkdir\n--files-peek\n--files-put\n--files-read-chunk\n--files-read-text\n--files-rename\n--files-size\n--files-stage-chunk\n--files-stat\n--files-write-text\n--find-in-session\n--first-run\n--footprint-report\n--fork-session\n--history-annotate\n--history-branch\n--history-facts\n--history-find\n--history-forget\n--history-index\n--history-last-accounts\n--history-lines\n--history-list\n--history-page\n--history-read\n--history-record\n--history-run\n--history-search\n--history-search-merge\n--history-tail\n--history-turns\n--history-user-inputs\n--hooks-diag\n--kill\n--last-seen-read\n--last-seen-write\n--launch\n--launch-render-cli\n--list-accounts\n--list-projects\n--list-sessions\n--list-user-inputs\n--machine-interrupts\n--mcp-read\n--mcp-server-put\n--mcp-server-remove\n--mcp-sync-apply\n--mcp-sync-plan\n--mcp-sync-preview\n--mcp-sync-source\n--ping\n--place-verdict\n--plan-ack\n--plan-cell-view\n--plan-command\n--plan-files\n--plan-list\n--plan-read\n--plan-return\n--plan-unack\n--powershell-policy-set\n--profiles-bases\n--profiles-impact\n--profiles-read\n--profiles-resolve\n--profiles-write\n--pubkey-push\n--quota-probe\n--quota-read\n--read-session\n--read-session-from-offset\n--read-session-tail\n--remote-reach\n--resident-attach\n--resident-ensure\n--resident-stop\n--resident-verdict\n--resolve\n--rotation-default-set\n--rotation-plan\n--rotation-rule-delete\n--rotation-rule-rename\n--rotation-rule-save\n--rotation-rules-read\n--rotation-session-read\n--rotation-session-set\n--rotation-switch\n--search\n--session-accounts\n--session-fork\n--session-interrupts\n--session-new\n--session-new-dir\n--session-new-facts\n--sessions-needs\n--sessions-start\n--sessions-stop\n--sessions-where\n--skill-install-apply\n--skill-install-plan\n--skill-install-record\n--skill-read\n--ssh-config-aliases\n--ssh-config-import\n--ssh-config-resolve\n--tasks-list\n--terminal-input\n--terminal-name-mint\n--terminal-preview\n--terminal-ssh\n--terminals-list\n--tmux-notify\n#channel\nch:accounts-add\nch:accounts-init\nch:accounts-list\nch:accounts-login-cmd\nch:accounts-mcp-pick\nch:accounts-mcp-read\nch:accounts-mcp-remove\nch:accounts-mcp-sync\nch:accounts-remove\nch:accounts-repair\nch:accounts-rollback\nch:accounts-sessions\nch:accounts-set-default\nch:accounts-trust\nch:accounts-verify\nch:agent-home-check\nch:aliases-block-install\nch:aliases-block-remove\nch:aliases-block-render\nch:aliases-read\nch:apikey-key-set\nch:apikey-read\nch:apikey-routing\nch:assets-catalog\nch:assets-catalog-merge\nch:assets-sync\nch:authorized-keys-add\nch:backend-log\nch:bus-broadcast\nch:bus-inbox\nch:bus-kill\nch:bus-list\nch:bus-send\nch:bus-spawn\nch:bus-state\nch:cancel\nch:cc-bus-install\nch:cc-bus-install-state\nch:ccm-print\nch:ccm-probe\nch:cells-catalog\nch:chores-mark\nch:data-report\nch:deploy-plan\nch:drift-report\nch:exit-policy-read\nch:exit-policy-set\nch:ext-hub-apply\nch:ext-hub-preview\nch:ext-list\nch:ext-list-here\nch:ext-note-set\nch:ext-uninstall-apply\nch:ext-uninstall-preview\nch:files-browse\nch:files-chmod\nch:files-commit-text\nch:files-commit-upload\nch:files-copy\nch:files-create\nch:files-delete\nch:files-delete-session\nch:files-extract\nch:files-find\nch:files-grep\nch:files-home\nch:files-index-rebuild\nch:files-index-status\nch:files-link\nch:files-ls\nch:files-mkdir\nch:files-peek\nch:files-put\nch:files-read-chunk\nch:files-read-text\nch:files-rename\nch:files-size\nch:files-stage-chunk\nch:files-stat\nch:files-write-text\nch:first-run\nch:footprint-report\nch:forward-list\nch:forward-start\nch:forward-stop\nch:history-annotate\nch:history-branch\nch:history-facts\nch:history-find\nch:history-forget\nch:history-index\nch:history-last-accounts\nch:history-lines\nch:history-list\nch:history-page\nch:history-read\nch:history-record\nch:history-run\nch:history-search\nch:history-search-merge\nch:history-tail\nch:history-turns\nch:history-user-inputs\nch:hooks-diag\nch:kill\nch:last-seen-read\nch:last-seen-write\nch:launch\nch:launch-local\nch:launch-render-cli\nch:link-close\nch:link-credit\nch:link-data\nch:link-open\nch:machine-interrupts\nch:mcp-read\nch:mcp-server-put\nch:mcp-server-remove\nch:mcp-sync-apply\nch:mcp-sync-plan\nch:mcp-sync-preview\nch:mcp-sync-source\nch:ping\nch:place-verdict\nch:plan-ack\nch:plan-cell-view\nch:plan-command\nch:plan-files\nch:plan-list\nch:plan-read\nch:plan-return\nch:plan-unack\nch:powershell-policy-set\nch:profiles-bases\nch:profiles-impact\nch:profiles-read\nch:profiles-resolve\nch:profiles-write\nch:pubkey-push\nch:quota-probe\nch:quota-read\nch:relay-optin\nch:remote-probe\nch:remote-reach\nch:resident-verdict\nch:resolve\nch:resync\nch:rotation-default-set\nch:rotation-plan\nch:rotation-rule-delete\nch:rotation-rule-rename\nch:rotation-rule-save\nch:rotation-rules-read\nch:rotation-session-read\nch:rotation-session-set\nch:rotation-switch\nch:session-fork\nch:session-interrupts\nch:session-new\nch:session-new-dir\nch:session-new-facts\nch:session-restart\nch:session-terminals\nch:sessions-needs\nch:sessions-start\nch:sessions-stop\nch:sessions-where\nch:skill-install-apply\nch:skill-install-plan\nch:skill-install-record\nch:skill-read\nch:ssh-config-aliases\nch:ssh-config-import\nch:ssh-config-resolve\nch:tasks-list\nch:terminal-follow\nch:terminal-follow-ack\nch:terminal-input\nch:terminal-name-mint\nch:terminal-preview\nch:terminal-processes\nch:terminal-ssh\nch:terminal-unfollow\nch:terminals-list\nch:transfer-download\nch:transfer-start\nch:transfer-stop\nch:transfer-upload\n#options\nopt:--after-ms\nopt:--args-b64\nopt:--from\nopt:--grace\nopt:--include-tools\nopt:--index\nopt:--limit\nopt:--query\nopt:--replace\nopt:--scope\nopt:--stdin-line\nopt:--text\nopt:--tz\nopt:--until\nopt:--view\nopt:--within-ms\nfrozen cae8b52b94ac8e53",
        ),
        (
            "p9z-stream-watch-codex",
            "--account-trust\n--account-trust-zero\n--accounts-add\n--accounts-init\n--accounts-list\n--accounts-login-cmd\n--accounts-mcp-pick\n--accounts-mcp-read\n--accounts-mcp-remove\n--accounts-mcp-sync\n--accounts-remove\n--accounts-repair\n--accounts-rollback\n--accounts-sessions\n--accounts-set-default\n--accounts-trust\n--accounts-verify\n--agent-home-check\n--aliases-block-install\n--aliases-block-remove\n--aliases-block-render\n--aliases-read\n--apikey-key-set\n--apikey-read\n--assets-catalog\n--assets-catalog-merge\n--assets-sync\n--authorized-keys-add\n--backend-log\n--backend-probe\n--bus-broadcast\n--bus-inbox\n--bus-kill\n--bus-list\n--bus-send\n--bus-spawn\n--bus-state\n--cc-bus-install\n--cc-bus-install-state\n--cells-catalog\n--chores-mark\n--data-report\n--deploy-plan\n--drift-report\n--exit-policy-read\n--exit-policy-set\n--ext-list-here\n--ext-note-set\n--ext-uninstall-apply\n--ext-uninstall-preview\n--files-browse\n--files-chmod\n--files-commit-text\n--files-commit-upload\n--files-copy\n--files-create\n--files-delete\n--files-delete-session\n--files-extract\n--files-find\n--files-grep\n--files-home\n--files-index-rebuild\n--files-index-status\n--files-link\n--files-ls\n--files-mkdir\n--files-peek\n--files-put\n--files-read-chunk\n--files-read-text\n--files-rename\n--files-size\n--files-stage-chunk\n--files-stat\n--files-write-text\n--find-in-session\n--first-run\n--footprint-report\n--fork-session\n--history-annotate\n--history-branch\n--history-facts\n--history-find\n--history-forget\n--history-index\n--history-last-accounts\n--history-lines\n--history-list\n--history-page\n--history-read\n--history-record\n--history-run\n--history-search\n--history-search-merge\n--history-tail\n--history-turns\n--history-user-inputs\n--hooks-diag\n--kill\n--last-seen-read\n--last-seen-write\n--launch\n--launch-render-cli\n--list-accounts\n--list-projects\n--list-sessions\n--list-user-inputs\n--machine-interrupts\n--mcp-read\n--mcp-server-put\n--mcp-server-remove\n--mcp-sync-apply\n--mcp-sync-plan\n--mcp-sync-preview\n--mcp-sync-source\n--ping\n--place-verdict\n--plan-ack\n--plan-cell-view\n--plan-command\n--plan-files\n--plan-list\n--plan-read\n--plan-return\n--plan-unack\n--powershell-policy-set\n--profiles-bases\n--profiles-impact\n--profiles-read\n--profiles-resolve\n--profiles-write\n--pubkey-push\n--quota-probe\n--quota-read\n--read-session\n--read-session-from-offset\n--read-session-tail\n--remote-reach\n--resident-attach\n--resident-ensure\n--resident-stop\n--resident-verdict\n--resolve\n--rotation-default-set\n--rotation-plan\n--rotation-rule-delete\n--rotation-rule-rename\n--rotation-rule-save\n--rotation-rules-read\n--rotation-session-read\n--rotation-session-set\n--rotation-switch\n--search\n--session-accounts\n--session-fork\n--session-interrupts\n--session-new\n--session-new-dir\n--session-new-facts\n--sessions-needs\n--sessions-start\n--sessions-stop\n--sessions-where\n--skill-install-apply\n--skill-install-plan\n--skill-install-record\n--skill-read\n--ssh-config-aliases\n--ssh-config-import\n--ssh-config-resolve\n--tasks-list\n--terminal-input\n--terminal-name-mint\n--terminal-preview\n--terminal-ssh\n--terminals-list\n--tmux-notify\n#channel\nch:accounts-add\nch:accounts-init\nch:accounts-list\nch:accounts-login-cmd\nch:accounts-mcp-pick\nch:accounts-mcp-read\nch:accounts-mcp-remove\nch:accounts-mcp-sync\nch:accounts-remove\nch:accounts-repair\nch:accounts-rollback\nch:accounts-sessions\nch:accounts-set-default\nch:accounts-trust\nch:accounts-verify\nch:agent-home-check\nch:aliases-block-install\nch:aliases-block-remove\nch:aliases-block-render\nch:aliases-read\nch:apikey-key-set\nch:apikey-read\nch:apikey-routing\nch:assets-catalog\nch:assets-catalog-merge\nch:assets-sync\nch:authorized-keys-add\nch:backend-log\nch:bus-broadcast\nch:bus-inbox\nch:bus-kill\nch:bus-list\nch:bus-send\nch:bus-spawn\nch:bus-state\nch:cancel\nch:cc-bus-install\nch:cc-bus-install-state\nch:ccm-print\nch:ccm-probe\nch:cells-catalog\nch:chores-mark\nch:data-report\nch:deploy-plan\nch:drift-report\nch:exit-policy-read\nch:exit-policy-set\nch:ext-hub-apply\nch:ext-hub-preview\nch:ext-list\nch:ext-list-here\nch:ext-note-set\nch:ext-uninstall-apply\nch:ext-uninstall-preview\nch:files-browse\nch:files-chmod\nch:files-commit-text\nch:files-commit-upload\nch:files-copy\nch:files-create\nch:files-delete\nch:files-delete-session\nch:files-extract\nch:files-find\nch:files-grep\nch:files-home\nch:files-index-rebuild\nch:files-index-status\nch:files-link\nch:files-ls\nch:files-mkdir\nch:files-peek\nch:files-put\nch:files-read-chunk\nch:files-read-text\nch:files-rename\nch:files-size\nch:files-stage-chunk\nch:files-stat\nch:files-write-text\nch:first-run\nch:footprint-report\nch:forward-list\nch:forward-start\nch:forward-stop\nch:history-annotate\nch:history-branch\nch:history-facts\nch:history-find\nch:history-forget\nch:history-index\nch:history-last-accounts\nch:history-lines\nch:history-list\nch:history-page\nch:history-read\nch:history-record\nch:history-run\nch:history-search\nch:history-search-merge\nch:history-tail\nch:history-turns\nch:history-user-inputs\nch:hooks-diag\nch:kill\nch:last-seen-read\nch:last-seen-write\nch:launch\nch:launch-local\nch:launch-render-cli\nch:link-close\nch:link-credit\nch:link-data\nch:link-open\nch:machine-interrupts\nch:mcp-read\nch:mcp-server-put\nch:mcp-server-remove\nch:mcp-sync-apply\nch:mcp-sync-plan\nch:mcp-sync-preview\nch:mcp-sync-source\nch:ping\nch:place-verdict\nch:plan-ack\nch:plan-cell-view\nch:plan-command\nch:plan-files\nch:plan-list\nch:plan-read\nch:plan-return\nch:plan-unack\nch:powershell-policy-set\nch:profiles-bases\nch:profiles-impact\nch:profiles-read\nch:profiles-resolve\nch:profiles-write\nch:pubkey-push\nch:quota-probe\nch:quota-read\nch:relay-optin\nch:remote-probe\nch:remote-reach\nch:resident-verdict\nch:resolve\nch:resync\nch:rotation-default-set\nch:rotation-plan\nch:rotation-rule-delete\nch:rotation-rule-rename\nch:rotation-rule-save\nch:rotation-rules-read\nch:rotation-session-read\nch:rotation-session-set\nch:rotation-switch\nch:session-fork\nch:session-interrupts\nch:session-new\nch:session-new-dir\nch:session-new-facts\nch:session-restart\nch:session-terminals\nch:sessions-needs\nch:sessions-start\nch:sessions-stop\nch:sessions-where\nch:skill-install-apply\nch:skill-install-plan\nch:skill-install-record\nch:skill-read\nch:ssh-config-aliases\nch:ssh-config-import\nch:ssh-config-resolve\nch:stream-watch\nch:tasks-list\nch:terminal-follow\nch:terminal-follow-ack\nch:terminal-input\nch:terminal-name-mint\nch:terminal-preview\nch:terminal-processes\nch:terminal-ssh\nch:terminal-unfollow\nch:terminals-list\nch:transfer-download\nch:transfer-start\nch:transfer-stop\nch:transfer-upload\n#options\nopt:--after-ms\nopt:--args-b64\nopt:--from\nopt:--grace\nopt:--include-tools\nopt:--index\nopt:--limit\nopt:--query\nopt:--replace\nopt:--scope\nopt:--stdin-line\nopt:--text\nopt:--tz\nopt:--until\nopt:--view\nopt:--within-ms\nfrozen aeb22641a46ee203",
        ),
    ];

    use crate::guard_support::{assert_no_test_code, production_code};

    /// 从 `main.rs` 的生产段抠出 `Some("--x")` 形态的子命令，排序去重。
    ///
    /// **剥测试段与剥注释两样都要**：`main.rs` 的散文里成篇地提到这些字面量（版本谱系那一大段
    /// 就是），不剥的话「注释里写了它」也能把指纹喂饱 —— 那正是安慰剂。
    ///
    /// # U-1（2026-08-01）：剥法换成 `guard_support`，指纹的来源变了
    ///
    /// 旧剥法锚 `"\n#[cfg(test)]\nmod tests"`，而 `main.rs` 的测试模块叫 `mod stream_flag_tests`
    /// ⇒ 匹配不上 ⇒ 整个文件被当生产段。**于是本护栏数到的九个子命令，一直来自
    /// `stream_flag_tests` 里那份副本，不是 `:275-291` 的真 dispatch。**
    /// （两个 bug 凑出一个看起来正确的结果：不剥 ⇒ 真 dispatch 也在里面 ⇒ 一直绿。）
    /// 换成逐个剥测试模块之后，指纹来自真 dispatch，**集合实测不变**（同为那九个）。
    /// ★★★ **两个命令面都要进指纹**。
    ///
    /// # 它此前只覆盖一半，而漏掉的那半从 0 长到了 5
    ///
    /// 本函数原来只抠 `main.rs` 里的 `Some("--`，也就是**一次性子命令**那一面。
    /// 而后端还有第二个命令面：[`crate::stream::inbound::REGISTRY`]（常驻通道命令，名字由 `command_names` 取）。
    /// 实测（`audit-0805` 的只读核实）：
    ///
    /// - `BUILD_ID` 从 `4617f34`（07-31，`p1v-attachable`）之后**再没变过**；
    /// - 而入方向从**零条**长到 **5 条**：`cancel`/`ping`/`resolve`（`8a13ba9`+`a361ff9`，08-02）、
    ///   `kill`（`899538a`，08-04）。`git merge-base --is-ancestor` 三条全 YES。
    ///
    /// ⇒ **加了整整一个命令面，一次 bump 都没被逼出来**，因为指纹结构上看不见它。
    /// 而部署判定（当年住 monitor 的 `sftp.rs`，今天住 `control/deploy_plan.rs` 的 `identity_decision`，原共享 crate `deploy-core` 的判定那一半）判「远端要不要换后端」时，**版本那一维**的唯一判据就是 build_id 字符串
    /// （backend 那条部署路另加了「落点文件在不在」这一维；今天它读那份字节自报的身份戳、旁挂标记退役；
    /// 本句的实质警告不变：**stale 但文件在**的后端仍只凭 build_id 判换不换）
    /// ⇒ 已部署的旧后端报同一个 id ⇒ 判 `Skip` ⇒ **整个控制面在远端静默不可用**。
    ///
    /// # 这不只是结构缺陷，本机实测到了它的后果
    ///
    /// 本机 `embedded-backends/cc-monitor-backend-x86_64`（08-01 构建）里
    /// **找不到入方向那一面会发射的任何一个错误码**（`not_cancellable` / `unknown_command` /
    /// `duplicate_id` / `handler_panicked` / `wrong_owner` / `too_many_windows`，`.rodata` 全 0 命中），
    /// 而它的清单写着 `p1v-attachable` = 期望值。
    /// ⚠ 那次探测**带对照组才算数**：同一批里「**会被发射**的串」9/9 全命中
    /// （`session_removed`/`overflow`/`hello`/`capabilities`…），
    /// 而「只被比较、从不发射」的串命中不稳（`--list-projects` 就是 0）——
    /// 上面那批错误码属前者（它们是写进应答 JSON 的值），所以 0 命中是可信的。
    ///
    /// # 为什么通道那半直接引用 const，而不照 CLI 那半去 scrape 文本
    ///
    /// CLI 那面只能 scrape（`main.rs` 的 dispatch 是 `match` 字面量，没有集中的 const）。
    /// 通道那面**有**单一源头常量 ⇒ 直接引用它更强：**没有抽取器可坏**，
    /// 改名/增删会自动反映到指纹里。两半的取法不同是刻意的，不是遗漏。
    fn subcommand_fingerprint() -> String {
        let prod = production_code(include_str!("../../src/backend/main.rs"));
        // 反向自检：剥完还得剩下真代码，否则下面数出来的空集会「恰好等于」某个错误期望。
        assert!(
            prod.len() > 3_000,
            "剥完 main 生产段只剩 {} 字节 —— 剥法坏了，本护栏此刻是无效的",
            prod.len()
        );
        // 反向自检之二：**测试段真的剥掉了**。旧的 `len` 自检光靠剥注释就满足，
        // 与测试段有没有剥掉毫无关系 —— 那正是本护栏扫了几个月测试代码没人发现的原因。
        assert_no_test_code("build_id_guard/main.rs", &prod);
        // **改用权威登记表 `SUBCOMMANDS`，不再自己抠 `Some("--`。**
        //
        // 原来那种抠法有两个洞，都是实测出来的：
        // ① **只认一种写法**：把新子命令写成 `Some(x) if x == "--foo" =>`，指纹不变
        //    （本条全绿；逮住它的是隔壁 `argv_table_guard` 与 `protocol_doc_guard`）；
        // ② **更要命的一条**：那种抠法今天只抠到 **9** 条，而 `SUBCOMMANDS` 登记着 **14** ——
        //    `--list-projects` / `--list-sessions` / `--read-session*` 这几条**改了也不会让指纹变**。
        //    也就是说 E77 的正题（「加了子命令就必须 bump」）对其中 5 条**本来就不成立**。
        //
        // 〔audit-0805 08-06 复核〕**这个前提已被实测验过，不再只是断言**：
        // 把一条子命令的 token 字面量搬到 `DISPATCH_FILES` 之外（`wire.rs`）再分派，
        // `protocol_doc_guard::dispatch_registry_is_complete` **当场红** ——
        // 因为「哪些文件参与分派」那一层是**派生**的，不是手写清单。
        // ⚠ 边界：token 连 `"--` 字面量都不出现（`concat!` 拼）时全绿，已登记（见那边头注）。
        // `SUBCOMMANDS` 是那一面的权威登记表，而且**有人守着它别漏**：
        // `argv_table_guard::every_dispatched_token_is_classified` 要求每个被分派的 token
        // 都在 `SUBCOMMANDS` / `SUBCOMMAND_OPTIONS` / `STREAM_FLAGS` 三张表之一里。
        // ⇒ 新子命令必须先进那张表，才轮得到本条 —— 「漏一条」这件事从此有两道门。
        let mut subs: Vec<String> = crate::SUBCOMMANDS.iter().map(|s| s.to_string()).collect();
        subs.sort_unstable();
        subs.dedup();
        // 反向自检：登记表被掏瘪 ⇒ 指纹退化，本护栏静默失效。
        assert!(
            subs.len() >= 10,
            "从 `SUBCOMMANDS` 只拿到 {} 条子命令 —— 登记表被掏了（08-06 实测 14 条）：{subs:?}",
            subs.len()
        );
        // ── 第二个命令面：常驻通道命令（`inbound::REGISTRY` 是它的单一源头，`command_names` 取名）──────
        // 排序后写成 `ch:<名>`，与 `--x` 那一面在同一个字符串里但**不会混淆**。
        let mut chans: Vec<String> = crate::stream::inbound::command_names()
            .iter()
            .map(|c| format!("ch:{c}"))
            .collect();
        chans.sort_unstable();
        // 反向自检：通道面空了 ⇒ 指纹会退化回「只覆盖一半」那个老样子而没人发现。
        assert!(
            !chans.is_empty(),
            "`inbound::command_names()` 抽到空集 —— 指纹会静默退回只覆盖 CLI 那一面"
        );
        let mut out = subs.join("\n");
        out.push_str("\n#channel\n");
        out.push_str(&chans.join("\n"));
        // 第三段：子命令的选项（`SUBCOMMAND_OPTIONS`）。W5-AUX 加 `--stdin-line` 时本条没红、
        //   p4f 靠人记得手动 bump —— 旧后端不认新选项同样会卡住调用方，所以它也算命令面。
        let mut opts: Vec<String> = crate::SUBCOMMAND_OPTIONS
            .iter()
            .map(|o| format!("opt:{o}"))
            .collect();
        opts.sort_unstable();
        out.push_str("\n#options\n");
        out.push_str(&opts.join("\n"));
        // 第四段：两个前端照着读的冻结格（格目录里 `frozen` 的每一格 ＋ 帧那张冻结表）。改了它们同样要打版本号 ——
        //   手机连上之后只核 `BUILD_ID` 这一处，不再读 `cells-catalog` 逐格核。写成一行摘要（格多，整份放进历史表太长）。
        out.push('\n');
        out.push_str(&frozen_fingerprint());
        out
    }

    /// 冻结格的摘要一行：`frozen <FNV-1a 64>`，摘的是排好序的 `成品.路径 类别 类型` 与 `帧.字段 类型`。
    fn frozen_fingerprint() -> String {
        let cat = crate::faces::cells_catalog::catalog();
        let mut lines: Vec<String> = Vec::new();
        for p in cat["products"].as_array().into_iter().flatten() {
            for c in p["cells"].as_array().into_iter().flatten() {
                if c["frozen"] == true {
                    lines.push(format!(
                        "{}.{} {} {}",
                        p["name"].as_str().unwrap_or(""),
                        c["path"].as_str().unwrap_or(""),
                        c["kind"].as_str().unwrap_or(""),
                        c["type"].as_str().unwrap_or("")
                    ));
                }
            }
        }
        for (k, f, t) in crate::stream::wire::tests::SECOND_FRONTEND_READS {
            lines.push(format!("frame:{k}.{f} {t}"));
        }
        // 反向自检：冻结格一格都没数到 ⇒ 这一段恒等、改了冻结格也不红。
        assert!(
            lines.len() >= 40,
            "冻结格只数到 {} 格 —— 摘要退化了",
            lines.len()
        );
        lines.sort_unstable();
        format!(
            "frozen {:016x}",
            crate::observe::record_page::line_hash(lines.join("\n").as_bytes())
        )
    }

    /// ★ E77 的正题。
    #[test]
    fn adding_a_subcommand_forces_a_build_id_bump() {
        let now = subcommand_fingerprint();
        // **判据是「当前指纹在不在表里」**，不是「等于最后一行」。
        //
        // 头一版写成「必须等于最后一行的指纹、且那行的 id 必须等于当前 BUILD_ID」——
        // 那会让**因为别的原因 bump BUILD_ID**（比如 p1v 只是加了个 wire 字段、子命令一个没动）
        // 也被逼着改这张表，而改表恰恰是本护栏最不想诱导的动作。
        //
        // 现在：加子命令 ⇒ 指纹是新的 ⇒ 不在表里 ⇒ 红。要弄绿只能追加一行；
        // 而**用当前（未 bump 的）id 追加会撞上 `history_has_no_duplicate_build_ids`** ⇒
        // 只剩「bump + 追加」这一条路。
        let (last_id, last_fp) = *SUBCOMMAND_HISTORY
            .last()
            .expect("SUBCOMMAND_HISTORY 不能为空");
        let _ = last_id;

        if !SUBCOMMAND_HISTORY.iter().any(|(_, fp)| *fp == now) {
            let old: Vec<&str> = last_fp.split('\n').collect();
            let new: Vec<&str> = now.split('\n').collect();
            let added: Vec<&&str> = new.iter().filter(|s| !old.contains(s)).collect();
            let removed: Vec<&&str> = old.iter().filter(|s| !new.contains(s)).collect();
            panic!(
                "backend 的子命令集或冻结格变了（+{added:?} / -{removed:?}；`frozen …` 那一行是两个前端照读的冻结格的摘要），而 BUILD_ID 还是 `{}`。\n\
                 \n\
                 **别只改这张表**。monitor 判「远端该不该换后端」只有一条判据：\n\
                 那台报的 build_id ≠ monitor 内嵌字节自报的 id。不 bump ⇒ 已部署的旧 backend\n\
                 报同一个 id ⇒ 不判 stale ⇒ 不自动重装 ⇒ **你这一轮的改动在已部署的远端休眠**，\n\
                 用户只会拿到「版本过旧」。本仓已经因为这个栽过三次（p1r / p1t / G2）。\n\
                 \n\
                 正确动作：① 在 lib.rs 里 bump `BUILD_ID`（并在版本谱系里加一段说清改了什么）；\n\
                 ② 在 `SUBCOMMAND_HISTORY` **追加**一行、并删掉最老那一行（表恒为两行）；\
                 要追加的就是下面 NEW-ROW 那一行（打版本号的脚本按它改表）：\n\
                 NEW-ROW: (\"<新 id>\", {now:?})",
                super::super::BUILD_ID
            );
        }
    }

    /// 历史表不许有重复 id —— 重复意味着「同一个 id 对应过两套子命令集」，
    /// 那正是本护栏要防的那件事被绕过去了。
    #[test]
    fn history_has_no_duplicate_build_ids() {
        let mut ids: Vec<&str> = SUBCOMMAND_HISTORY.iter().map(|(id, _)| *id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "历史表里有重复的 BUILD_ID：{ids:?}");
        assert_eq!(
            n, 2,
            "历史表只留「上一版 ＋ 当前版」两行（更早的在 git 历史里）：追加新行时删掉最老那行"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 🔴 `K-R70`（09-12）：**后端说得出自己是谁** —— 身份从字节里读得出，不是从旁边抄
    // ═══════════════════════════════════════════════════════════════════════
    //
    // 上面那几条守的是「**该不该** bump」。本节守的是另一件事，`K-R68` 摸底才逮出来的：
    // **拿起一份后端二进制，产品今天没有办法判断它是不是我们以为的那一份。**
    // 三个载体（`native-backend/` · `embedded-backends/` · `binaries/`）的身份
    // 全靠旁边那个 `.build_id` 文本文件，而那个文件是**从同一处源码常量抠出来的标签**
    // ⇒ 三份恒等 ⇒ 一格证据都不提供（`DECISIONS.md#R26` 裁定零）。
    //
    // 出路是两条**都长在二进制自己身上**的路，同源于 `BUILD_ID`：
    //   ① 跑得动它的人 —— `--ccm-probe` 的 `build=` 那一行；
    //   ② 跑不动它的人（交叉编译的 musl 二进制在 Windows 构建机上执行不了）——
    //      扫字节里的 `CC_MONITOR_BUILD_STAMP`。

    /// 把一段字节里的身份戳扫出来（**去重后的全部取值**）—— 扫法只有一份：`deploy_contract::identity_of_bytes`
    /// （monitor 构建期 · 运行期 · 后端部署计划同一个函数）。空串与非法字符一律不收：两个界标挨着
    /// （debug 构建 `.rodata` 里真会出现的那一形）不是一个戳。
    fn build_ids_in(bytes: &[u8]) -> Vec<String> {
        use deploy_contract::RemoteIdentity as Id;
        let marks = deploy_contract::Marks {
            open: deploy_contract::STAMP_OPEN,
            close: deploy_contract::STAMP_CLOSE,
        };
        match deploy_contract::identity_of_bytes(bytes, marks) {
            Id::Stamp(id) => vec![id],
            Id::Ambiguous(ids) => ids,
            _ => Vec::new(),
        }
    }

    /// 反向自检：**扫描器真的在扫**，不是恒答一个好看的答案。
    ///
    /// 一个「永远返回 `BUILD_ID`」的扫描器会让上面两条判据全绿而什么都没证。
    /// ⇒ 三格：没有戳 ⇒ 空 · 换个 id ⇒ 认出那个 id · 两个不同的戳 ⇒ 认出两个。
    #[test]
    fn the_stamp_scanner_actually_bites() {
        assert!(
            build_ids_in(b"nothing to see here").is_empty(),
            "没有戳的字节里也扫出东西 —— 扫描器在编答案"
        );
        let fake = format!(
            "{}zz-not-us{}",
            deploy_contract::STAMP_OPEN,
            deploy_contract::STAMP_CLOSE
        );
        assert_eq!(
            build_ids_in(fake.as_bytes()),
            vec!["zz-not-us".to_string()],
            "换一份字节，它必须报出**那一份**的身份，而不是本进程的"
        );
        let real = String::from_utf8_lossy(crate::CC_MONITOR_BUILD_STAMP.as_slice()).to_string();
        let two = format!("头{fake}中{real}尾");
        let mut want = vec![super::super::BUILD_ID.to_string(), "zz-not-us".to_string()];
        want.sort();
        assert_eq!(build_ids_in(two.as_bytes()), want, "两个戳要认出两个");
        // 界标挨着（debug 构建 `.rodata` 里真会出现的那一形）不许被当成一个戳。
        let adjacent = format!(
            "{}{}",
            deploy_contract::STAMP_OPEN,
            deploy_contract::STAMP_CLOSE
        );
        assert!(
            build_ids_in(adjacent.as_bytes()).is_empty(),
            "两个界标挨着被读成了一个身份 —— 那会让「有几个身份」这个读数变假"
        );
    }

    /// ★★ `KR70D1` 的正题：**身份真的在这一份二进制的字节里** —— 扫这个进程自己。
    ///
    /// # 为什么扫 `current_exe()` 而不是扫一份夹具
    ///
    /// 本条要证的性质是「**编译之后它还在、而且连续**」。夹具证不了这一点：
    /// 夹具是我们自己拼的字节，编译器没插手。⇒ 唯一说得上话的被测对象是
    /// **一份真的编出来的二进制**，而手边最便宜的那一份就是**跑着本测试的这个壳**
    /// （它由 `main.rs` 编来，生产段的 `#[used] static` 一样在里面）。
    ///
    /// # 它证的与不证的
    ///
    /// ✅ 证：这一份二进制里**恰好一个**身份戳，值 = `BUILD_ID`。
    /// ⚠ 不证：**发版那份 release 二进制**也如此 —— 那是另一套 profile
    ///   （`lto` / `strip` / `opt-level`）。开工时在沙箱里用一份 `lto=true, strip=true`
    ///   的 release 壳单独打过一趟、同样扫出恰好一处，**但那是一次实验不是一条常驻判据**；
    ///   常驻的那条在 `src/frontend/shell/build.rs`：内嵌任何一个载体之前都要从**它的字节**里
    ///   把身份扫出来，扫不出当场 panic ⇒ release 那一侧由发版路自己守。
    #[test]
    fn the_build_stamp_is_byte_scannable_in_this_very_binary() {
        let exe = std::env::current_exe().expect("拿不到本测试壳自己的路径");
        let bytes = std::fs::read(&exe).unwrap_or_else(|e| panic!("读不到 {}：{e}", exe.display()));
        assert!(
            bytes.len() > 100_000,
            "读到的字节只有 {} —— 读错文件了，本条会零命中地绿",
            bytes.len()
        );
        let ids = build_ids_in(&bytes);
        assert_eq!(
            ids,
            vec![super::super::BUILD_ID.to_string()],
            "从这一份二进制的字节里扫出来的身份是 {ids:?}，而 `BUILD_ID` 是 `{}`。\n\
             \n\
             · 扫到 **0 个** ⇒ 那个 `#[used] static CC_MONITOR_BUILD_STAMP` 被优化掉 / 被删了\n\
               ⇒ 「拿到一份二进制问得出它是谁」这条性质当场没了，而**旁边那个 `.build_id`\n\
               文件不算数**：它是从源码常量抄的标签，三个载体永远一致，一格证据都不提供\n\
               （`K-R68` 摸底 · `DECISIONS.md#R26` 裁定零）。\n\
             · 扫到 **多个** ⇒ 有第二处也在往二进制里写这个形状的串，身份不再唯一。\n\
             · 值不对 ⇒ 拼戳那条 `const fn` 与 `BUILD_ID` 脱钩了。",
            super::super::BUILD_ID
        );
    }

    /// ★★ `KR70D1` 的另一半：**跑得动它的人，直接问它**。
    ///
    /// `--ccm-probe` 那条握手此前只答得出 `version=`（**CLI 的契约版本**，`5`），
    /// 它答不出「你是哪一次构建」。本条钉住 `build=` 那一行**取自这个进程编进来的常量**。
    ///
    /// ⚠ 与 `control::ccm` 里那条形状判据**不重**：那边钉的是整份输出的**行序与行数**
    /// （外部契约），这边钉的是**这一行的值从哪来**（身份）。
    #[test]
    fn the_probe_answers_which_build_this_is() {
        let out = crate::control::ccm::probe_output("/somewhere/ccm");
        let line = out
            .lines()
            .find_map(|l| l.strip_prefix("build="))
            .unwrap_or_else(|| {
                panic!(
                    "`--ccm-probe` 的输出里没有 `build=` 那一行 —— \
                     「问一份二进制它是谁」这条路断了。实得：\n{out}"
                )
            });
        assert_eq!(
            line,
            super::super::BUILD_ID,
            "`build=` 报的是 `{line}`，而这一份的 `BUILD_ID` 是 `{}` —— \
             它没有取自本进程的常量（抄了别处 = 又一个标签）",
            super::super::BUILD_ID
        );
    }

    /// ★★ `KR70D2`：**`version = "0.0.0"` 是刻意的，而且有人在数它。**
    ///
    /// # 选的是件计划里那条 ②，代价写在这里
    ///
    /// ①（给它一个真版本、进 `release.yml` 那道「四处版本号与 tag 一致」的检查）**没选**。
    /// 代价现打：那道检查比的是**四处 == tag**，而后端换不换由**行为变没变**决定
    /// （`BUILD_ID` 的 bump 纪律，`SUBCOMMAND_HISTORY` 那张表就是它的账本），
    /// 不由发版节奏决定。把它绑上 tag ⇒ 每次发版这个 crate 的版本都动一格，
    /// 而 `BUILD_ID` 不动 ⇒ **后端从此有两个版本号，谁都不是权威** ——
    /// 那正是 `K33`「后端只有一个」在身份这一维要避开的东西。
    ///
    /// ②（明写刻意不用 + 判据钉住真身份住址）**选了**。它的代价也如实写：
    /// 后端的身份**不出现在** `cargo metadata` / `Cargo.lock` 那一面 ⇒
    /// 靠 crate 版本号做依赖管理的工具看它永远是 `0.0.0`。
    /// 今天没有任何消费者走那条路（本 crate 不发布、不被别的 crate 依赖，
    /// `[[bin]]` 是它唯一的产物），所以这一格是**已知且今天为空**的代价，不是漏。
    ///
    /// # 死值验：把身份住址改坏 ⇒ 必须红
    ///
    /// 三处住址逐个核：`const BUILD_ID` · 拼戳那段 · `--ccm-probe` 的 `build=`。
    /// 少任何一处，本条或它上面那两条当场红。
    #[test]
    fn the_crate_version_is_deliberately_zero_and_the_real_identity_has_a_home() {
        // ── ① `version = "0.0.0"` 还在，而且**恰好一处** ────────────────────
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let toml = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|e| panic!("读不到 {}：{e}", manifest.display()));
        let version_lines: Vec<&str> = toml
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("version = ") || l.starts_with("version="))
            .collect();
        assert_eq!(
            version_lines,
            vec![r#"version = "0.0.0""#],
            "本 crate 的 `[package] version` 实得 {version_lines:?}。\n\
             它**刻意**是 `0.0.0`（真身份住 `lib.rs` 的 `BUILD_ID`，理由逐字写在\n\
             `Cargo.toml` 那一行上方）。要改成别的数字，得先答一个问题：\n\
             **谁在数它？** 把一个假值换成另一个假值，是 `KR70D2` 逐字点名的失效方向。"
        );
        // ── ② 「刻意」这件事在盘上写着，不是只住在我脑子里 ──────────────────
        //    锚是**一句中性的话**，不取自任何夹具名（`brief` 12 那条）。
        const DELIBERATE: &str = "本 crate 刻意不用 Cargo.toml 的版本";
        assert!(
            toml.contains(DELIBERATE),
            "`Cargo.toml` 里找不到逐字「{DELIBERATE}」—— \n\
             `0.0.0` 于是退回成一个**没人解释过的假值**，而下一个人看到它只会顺手改掉。"
        );
        // ── ③ 真身份的住址逐个还在 ─────────────────────────────────────────
        // 🔴 住址从 `main.rs` 改成 `lib.rs` —— 身份按
        //    前置 2 搬进库面（in-process 那条路没有那个 `main.rs`，身份会跟着它消失）。
        // ⚠ 认的那一行现在是 `pub const BUILD_ID`（搬进库面时提了权）——
        //    `starts_with("const BUILD_ID")` 会**一条都不命中**而本条当场空真，
        //    所以下面按 `contains` 认，且**仍然要求恰好 1 处**。
        let lib_rs = include_str!("../../src/backend/lib.rs");
        let decls = lib_rs
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                t.starts_with("const BUILD_ID") || t.starts_with("pub const BUILD_ID")
            })
            .count();
        assert_eq!(
            decls, 1,
            "`lib.rs` 里 `const BUILD_ID` 的声明有 {decls} 处（应当 1）—— \n\
             ⚠ `release.yml` 与 `tests/scripts/re-embed.sh` 两处都按\n\
             「含 `const BUILD_ID` 的那一行」去抠**这份文件**；0 处 ⇒ 抠出 `unknown`，\n\
             多处 ⇒ 抠到哪一个看运气。\n\
             ⚠ 把它搬回 `main.rs` 也会让本条红 —— 那是刻意的：in-process 那条路**没有\n\
             那个 `main.rs`**，身份不能住在只有一个宿主看得见的地方。"
        );
        assert!(
            !super::super::BUILD_ID.is_empty(),
            "`BUILD_ID` 是空串 —— 落到用户盘上的名字会变成 `cc-monitor-backend-`（不带版本）"
        );
        // 戳的两个界标也是身份住址的一部分：它们一变，扫字节那一侧全瞎。
        assert!(
            !deploy_contract::STAMP_OPEN.is_empty() && !deploy_contract::STAMP_CLOSE.is_empty(),
            "身份戳的界标是空串 —— 扫字节那条路会把整份二进制当成一个戳"
        );
        assert!(
            build_ids_in(crate::CC_MONITOR_BUILD_STAMP.as_slice())
                == vec![super::super::BUILD_ID.to_string()],
            "那段 `static` 自己都扫不出 `BUILD_ID` —— 拼戳的 `const fn` 与常量脱钩了"
        );
    }

    /// 反向自检：判据**真的会抓人**。喂一个「多了一个子命令」的假指纹，比对必须不等。
    ///
    /// 直接喂字符串而不是去改 `main.rs` —— 后者要么污染工作区，要么因为改不进去而假绿
    /// （本仓已栽过：变异没落地却被当成「没覆盖」）。
    #[test]
    fn the_comparison_actually_bites() {
        let now = subcommand_fingerprint();
        let tampered = format!("{now}\n--brand-new-subcommand");
        assert_ne!(now, tampered, "比对形同虚设");
        // 也确认它认得出「少了一个」
        let shortened = now.split('\n').skip(1).collect::<Vec<_>>().join("\n");
        assert_ne!(now, shortened);
        // 选项那一段真在指纹里：每一个 `SUBCOMMAND_OPTIONS` 都得有自己的一行。
        for o in crate::SUBCOMMAND_OPTIONS {
            assert!(
                now.lines().any(|l| l == format!("opt:{o}")),
                "指纹里没有选项 `{o}` —— 加选项又会不逼 bump"
            );
        }
        // 以及：注释里的字面量不该被算进去（剥注释这一步是有效的）
        assert!(
            !now.contains("--account-trust-zero\n--account-trust-zero"),
            "同一个子命令被数了两次 —— 去重坏了"
        );
    }

    /// 打版本号的脚本（`tests/scripts/bump-build-id.sh`）在、能跑：拿一棵假仓 ＋ 假 `cargo`，
    /// **没变**（指纹那条绿）与**变了**（指纹那条红、印 NEW-ROW）两条路各走一次。
    /// 假 cargo 只替掉「跑测试」那一步；改 `BUILD_ID` · 认 NEW-ROW · 删最老那行追加新行都是真脚本在做。
    #[cfg(unix)]
    #[test]
    fn the_bump_script_walks_both_paths() {
        use std::process::Command;
        let script = crate::guard_support::repo_root().join("tests/scripts/bump-build-id.sh");
        assert!(script.is_file(), "打版本号的脚本不在：{}", script.display());
        // 脚本认的那一行与本文件红时印的那一行是同一个形状。
        assert!(
            include_str!("build_id_guard.rs").contains(r#"NEW-ROW: (\"<新 id>\", {now:?})"#),
            "指纹那条红时不再印 NEW-ROW 那一行 —— 脚本会认不出新行"
        );
        let lib0 = "pub const BUILD_ID: &str = \"p9b-two\";\n";
        let guard0 = "    const SUBCOMMAND_HISTORY: &[(&str, &str)] = &[\n        (\n            \"p9a-one\",\n            \"--a\",\n        ),\n        (\n            \"p9b-two\",\n            \"--a\\n--b\",\n        ),\n    ];\n";
        let run = |tag: &str, first: &str| -> (String, String, String) {
            let d = std::env::temp_dir().join(format!("bump-script-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            for sub in ["src/backend", "tests/backend", "bin"] {
                std::fs::create_dir_all(d.join(sub)).unwrap();
            }
            std::fs::write(d.join("src/backend/lib.rs"), lib0).unwrap();
            std::fs::write(d.join("tests/backend/build_id_guard.rs"), guard0).unwrap();
            std::fs::write(d.join("first.txt"), first).unwrap();
            let fake = format!(
                "#!/bin/sh\ncase \"$*\" in *adding_a_subcommand*) cat '{}' ;; *) echo 'test result: ok. 9 passed' ;; esac\n",
                d.join("first.txt").display()
            );
            std::fs::write(d.join("bin/cargo"), fake).unwrap();
            assert!(Command::new("chmod")
                .arg("+x")
                .arg(d.join("bin/cargo"))
                .status()
                .unwrap()
                .success());
            assert!(Command::new("git")
                .args(["init", "-q"])
                .current_dir(&d)
                .status()
                .unwrap()
                .success());
            let path = format!(
                "{}:{}",
                d.join("bin").display(),
                std::env::var("PATH").unwrap_or_default()
            );
            let out = Command::new("bash")
                .arg(&script)
                .arg("p9c-three")
                .current_dir(&d)
                .env("PATH", path)
                .env_remove("TMUX")
                .env_remove("TMUX_PANE")
                .output()
                .unwrap();
            let said = String::from_utf8_lossy(&out.stdout).into_owned();
            let lib = std::fs::read_to_string(d.join("src/backend/lib.rs")).unwrap();
            let guard = std::fs::read_to_string(d.join("tests/backend/build_id_guard.rs")).unwrap();
            let _ = std::fs::remove_dir_all(&d);
            (said, lib, guard)
        };
        // 没变：只改 BUILD_ID，表一个字节不动。
        let (said, lib, guard) = run("same", "test result: ok. 1 passed\n");
        assert!(
            said.trim_end().ends_with("BUMP: OK"),
            "没变那条路没走通：{said}"
        );
        assert_eq!(lib, "pub const BUILD_ID: &str = \"p9c-three\";\n");
        assert_eq!(guard, guard0, "子命令集没变，表却被改了");
        // 变了：删最老那行（p9a）、追加新行（p9c，指纹取自 NEW-ROW）。
        let red = "thread 'x' panicked:\nNEW-ROW: (\"<新 id>\", \"--a\\n--b\\n--c\")\ntest result: FAILED. 0 passed; 1 failed\n";
        let (said, lib, guard) = run("changed", red);
        assert!(
            said.trim_end().ends_with("BUMP: OK"),
            "变了那条路没走通：{said}"
        );
        assert_eq!(lib, "pub const BUILD_ID: &str = \"p9c-three\";\n");
        assert_eq!(
            guard,
            "    const SUBCOMMAND_HISTORY: &[(&str, &str)] = &[\n        (\n            \"p9b-two\",\n            \"--a\\n--b\",\n        ),\n        (\n            \"p9c-three\",\n            \"--a\\n--b\\n--c\",\n        ),\n    ];\n"
        );
        // 红时既不绿也不印 NEW-ROW ⇒ 脚本报红、不改表。
        let (said, _, guard) = run("broken", "error[E0425]: cannot find value\n");
        assert!(
            said.contains("BUMP: 红"),
            "指纹那条编不过时脚本没报红：{said}"
        );
        assert_eq!(guard, guard0);
    }
}
