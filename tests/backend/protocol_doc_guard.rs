//! 协议面本身的几条判据（不是文档对拍：协议文档的逐格那一半从代码生成，见 `protocol_doc_gen`）。
//!
//! - 子命令分派的文件名单（[`DISPATCH_FILES`]）与「`--` 字面量」取词器 [`dispatched_subcommands`]：
//!   `main_argv_table_guard` 拿它核 argv 表；`dispatch_registry_is_complete` 反向核对名单没漏文件。
//! - 子进程旗标：发的与子进程收的恰好一致。
//! - `control/` · `observe/` 里每个 serde 类型都登记（上线类型该进 `wire.rs`）。
//! - 协议级错误码只有 `stream/inbound/` 发得出（R4）。
//! - `EMITS` 是 `Frame` 变体的子集，缺席的有名有姓。
//!
//! ⚠ 取词器认的是 `"--xxx"` 字符串字面量；连字面量都不出现的写法（`concat!` 拼）它看不见。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#![cfg(test)]

/// 做子命令分派的**全部**文件。
///
/// 第一版只有 `main.rs` —— U6a 的 D 审计当场抓到代价：`--read-session-from-offset`
/// 在 `observe/history_query.rs` 里分派、文档**零出现**，而护栏**结构上扫不到它**。
/// 同族还有 `--list-projects` / `--list-sessions` / `--read-session-tail`（也在 history_query）
/// 与 `--include-tools` / `--scope` / `--after-ms` / `--limit`（在 search_query）。
///
/// 「抽取面画小了」是比「少写一条」更隐蔽的失效：护栏照常报绿，而它压根没看那片地方。
/// 所以下面 `dispatch_registry_is_complete` 会**反向核对**这份名单没漏文件。
const DISPATCH_FILES: &[(&str, &str)] = &[
    ("main.rs", include_str!("../../src/backend/main.rs")),
    // 🔴 `lib.rs`：`SUBCOMMANDS` 那张表按前置 1
    //    搬进了库面 ⇒ 按本名单的口径（「生产段里出现 `"--`」）它现在就是一份。
    //    ⚠ **不是我判断它该进来，是下面 `dispatch_registry_is_complete` 自己算出来的** ——
    //      搬家当天它逐字报「左边多了 `lib.rs`」。本条登记的就是它算出的那个答案。
    //    ⚠ 漏登它的后果逐字写在本名单头注里：那份文件里的 `--子命令` **全部不受
    //      IPC-PROTOCOL.md 对拍约束**，而对拍判据照常报绿。
    ("lib.rs", include_str!("../../src/backend/lib.rs")),
    // P4f：`control/cc_bus.rs` 里有一个 `"--"` 字面量（调 `cc-send` 时显式结束旗标，
    // 免得收件人以 `--` 开头被当成选项）。派生的文件集按「生产段里出现 `"--`」收人，
    // 于是把它扫了进来 —— 那条判据自己写着「宁可多登记几个文件」。登记，不改判据。
    (
        "control/cc_bus.rs",
        include_str!("../../src/backend/control/cc_bus.rs"),
    ),
    // `control/launch.rs`：终端管理送字那一段里有 `"--"` 字面量（`send-keys -l -- <字>` · `set-buffer -- <字>`：
    // 显式结束旗标，免得以 `--` 开头的字被 tmux 当成选项）。同上一条 cc_bus.rs 的处置：登记，不改判据。
    (
        "control/launch.rs",
        include_str!("../../src/backend/control/launch.rs"),
    ),
    // P4d：控制面的 CLI 入口。它**不做 match 分派**（认哪些 flag 由
    // `cli_control::spec_for` 从 `inbound::REGISTRY` 派生），但它持有
    // `PROBE_FLAG = "--backend-probe"` 这个字面量 —— 派生的文件集因此把它扫了进来。
    // ⇒ 登记在这里，`--backend-probe` 才受 IPC-PROTOCOL.md 对拍约束。
    (
        "control/cli_control.rs",
        include_str!("../../src/backend/control/cli_control.rs"),
    ),
    // `control/resident.rs`：`--replace` 这个选项字面量（`--resident-ensure` 的「先停再起」）让派生的文件集把它扫了进来。
    (
        "control/resident.rs",
        include_str!("../../src/backend/control/resident.rs"),
    ),
    // 本机常驻后端经 capture 在远端跑 `--assets-catalog` / `--assets-catalog-merge`
    // （`asset_sync::PULL_FLAG` / `PUSH_FLAG`）—— 它不分派，是**发**这两个子命令的一方；
    // 派生的文件集按「生产段里出现 `"--`」把它扫了进来。登记，那两个字面量随之受对拍约束。
    (
        "assets/asset_sync.rs",
        include_str!("../../src/backend/assets/asset_sync.rs"),
    ),
    (
        "observe/history_query.rs",
        include_str!("../../src/backend/observe/history_query.rs"),
    ),
    // `accounts/iso.rs` 那一行摘了：`--acct-iso-status` / `--acct-iso-shellinit` 的 argv 形分派退役，
    //   两问上了帧面（CLI 面由 `cli_control` 从 `REGISTRY` 派生，那一份早在本表里）。
    (
        "observe/accounts_query.rs",
        include_str!("../../src/backend/observe/accounts_query.rs"),
    ),
    (
        "observe/search_query.rs",
        include_str!("../../src/backend/observe/search_query.rs"),
    ),
    // 只读查询的帧面宿主。它不做 match 分派，但把 `history-search` 的
    // JSON 选项摊回 `--include-tools` / `--scope` / `--after-ms` / `--limit` 那几个 token
    // （解析走 CLI 那一臂同一个 `parse_opts`）⇒ 派生的文件集把它扫了进来。登记，不改判据：
    // 那几个 token 本来就在 IPC-PROTOCOL.md 里，它们从此也在这里受对拍。
    (
        "faces/read_face.rs",
        include_str!("../../src/backend/faces/read_face.rs"),
    ),
    // `relay/machine.rs` 那一行摘了：它不再起 `--relay`（argv 字面量随脱离中转一族删了），派生的文件集不再扫到它。
    // 它不做 match 分派，只在用法串里提自己的名字 —— 但 D 审计正是把一个
    // `pub const CTRL_FLAG: &str = "--ccm-hidden-ctrl";` 藏在这里绕过了护栏。
    // 放宽后的探测把它揪了出来。
    (
        "control/tmux_hook.rs",
        include_str!("../../src/backend/control/tmux_hook.rs"),
    ),
    // claude 那一家的起会话事实（`RESUME_TOKEN = "--resume"`）：发给 claude 这个子进程的旗标，
    //   派生的文件集把它扫了进来 ⇒ 登记在 [`CHILD_PROCESS_FLAGS`]（子进程在仓外，② 那一侧读金样）。
    (
        "agents/claudecode/resume.rs",
        include_str!("../../src/backend/agents/claudecode/resume.rs"),
    ),
    // codex 那一家的起会话事实（`LAUNCH_ARGS = ["--no-daemon"]`）：发给 codex 这个子进程的旗标，同上。
    (
        "agents/codex/resume.rs",
        include_str!("../../src/backend/agents/codex/resume.rs"),
    ),
];

/// 🔴 **终端命令面**的文件 —— 它们持有 `--旗标` 字面量，但那些**不是 wire 子命令**。
///
/// # 为什么要有这张表（`K-R48` 09-11）
///
/// `dispatch_registry_is_complete` 的判法是「生产段里出现 `"--` 字面量的文件集
/// == [`DISPATCH_FILES`]」，而 [`DISPATCH_FILES`] 里的每个 token 都得落进
/// `src/doc/IPC-PROTOCOL.md` §10 的代码跨度。那份文档是 **monitor↔backend 的冻结线上契约**，
/// 读者在仓外（aterm），改一个字就是改协议。
///
/// `ccm` 那套旗标（`--tmux` / `--account` / `--cwd` …）**不属于那份契约**：
/// 它们是**用户在终端里敲的东西**，消费者是人与 `src/shared/ccm-aliases.sh` 里那三个别名，
/// 兼容性义务完全不同。把它们塞进 §10 会让那份文档开始描述一件它不负责的事。
///
/// # ⚠ 它**不是**豁免，是换了一格判据
///
/// 放进这张表的文件，`dispatch_registry_is_complete` 不再管它，
/// 但它必须被另一条判据接住 —— 今天那条是
/// `control::ccm::tests::every_flag_we_accept_has_a_usage_line`
/// （**每个认得的旗标都要在 `ccm::USAGE` 里说得出**），
/// 由下面 `every_terminal_surface_file_is_covered_by_its_own_guard` 钉住它真的存在。
/// 只登记不接住 = 把一片扫描面静默挖空，那正是本文件头注里 D 审计一击即破的那一族。
const TERMINAL_SURFACE_FILES: &[(&str, &str)] = &[
    (
        "control/ccm/argv.rs",
        "`ccm` 这套终端 argv 的唯一解析口。旗标字面量**只住这一个文件**\
         （由 `control::ccm::argv::tests::the_ccm_argv_is_parsed_in_exactly_one_place` 钉），\
         它们是终端命令面、不是 wire 协议面。",
    ),
    // `ccm …` 调用行的渲染器（从 monitor 搬进后端）：它**写**那套终端旗标给用户的 shell 跑，不是后端分派的 argv。
    (
        "control/launch_render/ccm_invocation.rs",
        "`ccm …` 调用行的渲染器：它写的是用户终端里那一行 `ccm` 的旗标（终端命令面），不是后端自己分派的 wire 子命令；\
         接盘判据 `launch_cli_parity_tests::every_rendered_ccm_line_is_accepted_by_the_ccm_argv`（渲出的每一行都过 `argv::parse`）。",
    ),
];

/// 🔴 **子进程旗标** —— 后端**发给它转调的那个子进程**的 `--旗标`，不是后端自己分派的 argv。
///
/// # 为什么要有这一类
///
/// [`dispatched_subcommands`] 把 [`DISPATCH_FILES`] 里**每一个** `"--x"` 字面量都当成
/// 「后端自己认的 token」，于是要求它进 `lib.rs` 的 argv 三分表、进 `IPC-PROTOCOL.md §10`。
/// 而 `bus-spawn` 转调 `cc-spawn` 时要发 `--tool` / `--account` / `--base` —— 那是**cc-spawn 的**
/// 旗标：塞进 `SUBCOMMAND_OPTIONS` 会改 `is_query_mode` 的行为（`--tool` 打头的 argv 从此被当成
/// 「子命令选项脱离了子命令」），写进 §10 会让冻结契约描述一件它不负责的事。
/// 旧模型里只有两类（wire 面 · 终端面 [`TERMINAL_SURFACE_FILES`]），这是第三类。
///
/// # ⚠ 它**不是**豁免，是换了一格判据（同 [`TERMINAL_SURFACE_FILES`] 的取法）
///
/// 登记在这里的旗标从上面两条对拍里**按文件**摘掉，但必须被
/// `child_process_flags_are_exactly_what_the_file_sends_and_what_the_child_accepts` 接住：
/// ① 那份文件里的 `"--x"` 字面量集合 **==** 登记的旗标集合（两向；多一个没登记的 token ⇒ 红，
///    不许靠这张表把一条真 wire 子命令藏起来）；
/// ② 子进程脚本旗标循环里认的旗标集合 **==** 登记的 ∪ [`CHILD_FLAGS_NOT_SENT`]（两向；
///    发了一个它不认的 ⇒ 红，它新认了一个而这边没表态 ⇒ 红）。
/// 两侧**异源**：①读 Rust 生产段的字面量，②读 shell 脚本的 `case` 臂。
///
/// 🔴 **不许**把旗标拼成运行期字符串（`format!("--{}", "tool")`）来躲 [`dispatched_subcommands`] ——
/// 那是把扫描面静默挖空；正路就是登记在这里、让 ①② 接住。
pub(crate) const CHILD_PROCESS_FLAGS: &[(&str, &str, &[&str], &str)] = &[
    (
        "control/cc_bus.rs",
        "src/shared/cc-bus/scripts/cc-spawn",
        &["--account", "--base", "--tool"],
        "`bus-spawn` 转调 `cc-spawn` 时的旗标（`control/cc_bus.rs::spawn_argv`）。\
         它们是 cc-spawn 的命令面：后端 argv 从不认它们，线上契约里也没有它们的位置。",
    ),
    // 同一份文件发给另两个子进程的旗标（一份文件可以登记几行，① 按文件取并集）。
    (
        "control/cc_bus.rs",
        "src/shared/cc-bus/scripts/cc-list",
        &["--tsv"],
        "`bus-state` / 杀会话顺手注销转调 `cc-list --tsv`（机器可读的名册）。是 cc-list 的命令面，不是后端的子命令。",
    ),
    (
        "control/cc_bus.rs",
        "src/shared/cc-bus/scripts/cc-agents",
        &["--tsv"],
        "`bus-state` 转调 `cc-agents --tsv`（机器可读的派生台账）。是 cc-agents 的命令面，不是后端的子命令。",
    ),
    // 子进程是仓外的 `claude` ⇒ ② 那一侧读金样 `agent-profile-golden.tsv` 里 flag 形的 `resume_token`
    //   （那张表记的就是各家 agent 的命令形，见接盘判据里按扩展名分的那一支）。
    (
        "agents/claudecode/resume.rs",
        "tests/__fixtures__/agent-profile-golden.tsv",
        &["--resume", "--session-id"],
        "`claude` 那一家 resume 的 flag 形字面量（`agents/claudecode/resume.rs::RESUME_TOKEN`）与起新会话先定 sid 的旗标\
         （`SESSION_ID_FLAG`，起会话框带规则时用；起会话事实的唯一住址）：都是 claude 这个子进程的命令面，后端 argv 从不认它们，\
         线上契约里也没有它们的位置。",
    ),
    (
        "agents/codex/resume.rs",
        "tests/__fixtures__/agent-profile-golden.tsv",
        &["--no-daemon"],
        "`codex` 那一家起会话时垫在最前面的参数（`agents/codex/resume.rs::LAUNCH_ARGS`，不连共享后台）：\
         它是 codex 这个子进程的命令面，后端 argv 从不认它，线上契约里也没有它的位置。",
    ),
];

/// 子进程认、而后端**刻意不发**的旗标 —— 每条带理由（②那一向的另一半）。
pub(crate) const CHILD_FLAGS_NOT_SENT: &[(&str, &str, &str)] = &[(
    "src/shared/cc-bus/scripts/cc-spawn",
    "--new",
    "P4b 之后 cc-spawn **默认就是新建**，`--new` 是为兼容外面的老调用方保留的 no-op；后端没有理由发它。",
)];

/// 一份源码生产段里的 `"--x"` 字面量（按出现序去重）—— [`dispatched_subcommands`] 与
/// 子进程旗标那条接盘判据**共用这一处取法**（两处各写一份的话，一处放宽、一处没跟，就会各说各话）。
pub(crate) fn dashdash_literals(raw: &str) -> Vec<String> {
    let src = crate::guard_support::production_code(raw);
    let mut out: Vec<String> = Vec::new();
    {
        let mut from = 0usize;
        while let Some(rel) = src[from..].find("\"--") {
            let i = from + rel + 1;
            let tail = &src[i..];
            if let Some(end) = tail[1..].find('"') {
                let tok = &tail[..end + 1];
                // 字符集必须**宽于**今天用到的形态。旧版是 `[a-z-]`，D 审计实测
                // `--control-v2`（数字）/ `--ccm_hidden`（下划线）/ 大写全被**静默丢弃**
                // —— 护栏对它们不是「查过觉得没问题」，是**根本没看见**。
                if tok.len() > 2
                    && tok[2..]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    let s = tok.to_string();
                    if !out.contains(&s) {
                        out.push(s);
                    }
                }
            }
            from = i + 2;
        }
    }
    out
}

/// 分派里出现的所有 `--子命令` / `--选项`（跨 [`DISPATCH_FILES`] 全部文件）。
///
/// ⚠ [`CHILD_PROCESS_FLAGS`] 里登记的那几个**按文件**摘掉 —— 同一个 token 出现在
/// 别的分派文件里照样算（摘的是「这份文件发给子进程的那几个」，不是这个字串）。
pub(crate) fn dispatched_subcommands() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (name, raw) in DISPATCH_FILES {
        let child: Vec<&str> = CHILD_PROCESS_FLAGS
            .iter()
            .filter(|(f, ..)| f == name)
            .flat_map(|(_, _, flags, _)| flags.iter().copied())
            .collect();
        for s in dashdash_literals(raw) {
            if !child.contains(&s.as_str()) && !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::{
        dashdash_literals, CHILD_FLAGS_NOT_SENT, CHILD_PROCESS_FLAGS, DISPATCH_FILES,
        TERMINAL_SURFACE_FILES,
    };

    /// 从 `at`（该处即 `open`）起找配平的闭合符，返回它的下标。
    fn balanced(b: &[u8], at: usize, open: u8, close: u8) -> Option<usize> {
        let mut depth = 0i32;
        for (i, &c) in b.iter().enumerate().skip(at) {
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
        None
    }

    /// ★ [`DISPATCH_FILES`] 必须囊括 `src/` 下**每一个**做子命令分派的文件。
    ///
    /// 这条是给上面那个抽取面兜底的：漏登记一个文件，等于那个文件里的子命令
    /// 永远不受文档对拍约束，而护栏**照常报绿**。U6a 之前就正是这个状态。
    #[test]
    fn dispatch_registry_is_complete() {
        fn walk(dir: &std::path::Path, hits: &mut Vec<String>, root: &std::path::Path) {
            for e in std::fs::read_dir(dir).expect("读 src/ 失败").flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, hits, root);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let raw = std::fs::read_to_string(&p).unwrap_or_default();
                    // 整份文件都是 `#![cfg(test)]` 的（护栏自己就是）：release 构建里
                    // 根本不存在，不可能分派任何东西。`production_code` 只剥
                    // `#[cfg(test)] mod` 块，剥不掉文件级的内层属性 —— 得在这里跳。
                    if raw.contains("#![cfg(test)]") {
                        continue;
                    }
                    // 判据放宽到「生产段里出现 `"--` 字面量」。
                    //
                    // 前两版按**分派语法**探测（`Some("--x") =>` / 裸 `"--x" =>`），
                    // D 审计一击即破：把 `pub const CTRL_FLAG: &str = "--ccm-hidden-ctrl";`
                    // 放进 `control/tmux_hook.rs`（那文件两种语法都没有）⇒ 两条护栏全绿。
                    //
                    // 现在只要**提到**一个 `--token` 就得登记。宁可多登记几个文件，
                    // 也不要让一个 token 因为「写法不是 match 臂」而隐形。
                    let prod = crate::guard_support::production_code(&raw);
                    if prod.contains("\"--") {
                        hits.push(
                            p.strip_prefix(root)
                                .unwrap()
                                .to_string_lossy()
                                .replace('\\', "/"),
                        );
                    }
                }
            }
        }
        let root = crate::guard_support::src_root();
        let mut found = Vec::new();
        walk(&root, &mut found, &root);
        found.sort();

        assert!(
            found.len() >= 4,
            "只扫到 {} 个分派文件 —— 抽取坏了，本断言在空转：{found:?}",
            found.len()
        );
        // 终端命令面那几份**不进这场对账** —— 它们由另一条判据接住（见那张表的头注）。
        found.retain(|f| !TERMINAL_SURFACE_FILES.iter().any(|(n, _)| n == f));
        let mut registered: Vec<String> =
            DISPATCH_FILES.iter().map(|(n, _)| n.to_string()).collect();
        registered.sort();
        assert_eq!(
            found, registered,
            "\n做子命令分派的文件集变了，但 `DISPATCH_FILES` 没跟上。\n\
             漏登记的文件里，所有 `--子命令` 都不受 IPC-PROTOCOL.md 对拍约束，\n\
             而 `every_dispatched_subcommand_appears_in_the_protocol_doc` **照常报绿**。\n\
             把新文件加进 `DISPATCH_FILES`（含 `include_str!`）。"
        );
    }

    /// 🔴 [`CHILD_PROCESS_FLAGS`] 的接盘判据：**①② 两向相等**（登记表头注写着为什么）。
    ///
    /// ⚠ 买不到：「旗标的**值**对不对」（`--tool` 后面跟的是什么）—— 那一格由 cc-spawn 自己 rc=2 判，
    /// 行为判据住 `control::cc_bus::tests`（argv 形状）与 `tests/e2e/backend-cc-bus.sh`（真跑）。
    #[test]
    fn child_process_flags_are_exactly_what_the_file_sends_and_what_the_child_accepts() {
        assert!(
            !CHILD_PROCESS_FLAGS.is_empty(),
            "登记表空了 —— 要么真没有子进程旗标了（那就连同 `dispatched_subcommands` 里那段摘除一起删），\
             要么是被人掏空了"
        );
        let repo = crate::guard_support::src_root()
            .parent()
            .and_then(|p| p.parent())
            .expect("从 src/backend 回不到仓根")
            .to_path_buf();
        for (file, child, flags, why) in CHILD_PROCESS_FLAGS {
            assert!(why.chars().count() > 30, "`{file}` 那行理由太短：{why}");
            // ① 这份文件里的 `"--x"` 字面量 == 登记的（两向）
            let raw = DISPATCH_FILES
                .iter()
                .find(|(n, _)| n == file)
                .unwrap_or_else(|| {
                    panic!("`{file}` 不在 `DISPATCH_FILES` 里 —— 摘除按文件认，那份文件得先被扫到")
                })
                .1;
            let mut sent = dashdash_literals(raw);
            sent.sort();
            // 一份文件可以对几个子进程各登记一行 ⇒ ① 比的是这份文件所有行的并集。
            let mut all: Vec<String> = CHILD_PROCESS_FLAGS
                .iter()
                .filter(|(f, ..)| f == file)
                .flat_map(|(_, _, fl, _)| fl.iter().map(|s| s.to_string()))
                .collect();
            all.sort();
            all.dedup();
            let mut reg: Vec<String> = flags.iter().map(|s| s.to_string()).collect();
            reg.sort();
            assert_eq!(
                sent, all,
                "\n`{file}` 里的 `--x` 字面量与登记的子进程旗标对不上。\n\
                 多出来的 ⇒ 要么是一条**真 wire 子命令**（那它不该躲在这张表里，去三分表与 §10），\n\
                 要么是新发给子进程的旗标（登记它）；少掉的 ⇒ 不再发了，把登记摘掉。"
            );
            // ② 子进程旗标循环里认的 == 登记的 ∪ 刻意不发的（两向）
            let script = std::fs::read_to_string(repo.join(child))
                .unwrap_or_else(|e| panic!("读不到子进程脚本 {child}：{e} —— 判不了，不许当成绿"));
            let mut accepts: Vec<String> = if child.ends_with(".tsv") {
                // 子进程在仓外（`claude` · `codex`）⇒ 它认的旗标取金样里那一家的 `--` 开头的那几个词：
                // flag 形的 `resume_token` ＋ `launch_args` ＋ 起新会话先定 sid 的 `preset_sid`。
                let agent = if file.contains("/codex/") {
                    "codex"
                } else {
                    "claude"
                };
                script
                    .lines()
                    .filter(|l| !l.trim_start().starts_with('#'))
                    .filter_map(|l| {
                        let f: Vec<&str> = l.split('\t').collect();
                        (f.len() == 3
                            && f[0] == agent
                            && matches!(f[1], "resume_token" | "launch_args" | "preset_sid"))
                        .then(|| {
                            f[2].split(' ')
                                .filter(|w| w.starts_with("--"))
                                .map(str::to_string)
                                .collect::<Vec<_>>()
                        })
                    })
                    .flatten()
                    .collect()
            } else if script.lines().any(|l| l.trim() == "while true; do") {
                shell_loop_flags(&script)
            } else {
                shell_first_arg_flags(&script)
            };
            accepts.sort();
            accepts.dedup();
            let mut want: Vec<String> = reg.clone();
            for (c, f, why) in CHILD_FLAGS_NOT_SENT {
                if c == child {
                    assert!(why.chars().count() > 20, "`{f}` 不发的理由太短：{why}");
                    want.push(f.to_string());
                }
            }
            want.sort();
            assert_eq!(
                accepts, want,
                "\n`{child}` 认的旗标，与「后端发的 ∪ 刻意不发的」对不上。\n\
                 左边多 ⇒ 子进程新认了一个旗标、这边没表态（发不发都要写一句）；\n\
                 右边多 ⇒ 后端发了一个子进程**不认**的旗标（它会被当成位置参数吃掉，\
                 `cc-spawn` 那条的后果是「目录不存在：--xxx」）。"
            );
        }
    }

    /// shell 子进程那一形：`while true; do … done` 旗标循环里 `case` 臂认的旗标。〔BS1b；RM1c 抽成函数〕
    fn shell_loop_flags(script: &str) -> Vec<String> {
        let mut accepts: Vec<String> = Vec::new();
        let mut in_loop = false;
        // ⚠ 块界用**整行相等**（与 `pin_line` 同一口径），不用前缀匹配：
        //   前缀 needle 在语料上会被撑大而照样绿（`needle_anchor_registry` 那条棘轮管的就是它）。
        // ⚠ 变量名刻意不叫 `line` / `t`：那条棘轮按**文件内变量名**认语料，
        //   同名会把本文件别处的无关匹配一起卷进去（现打：卷进过 2 处）。
        for sline in script.lines() {
            let st = sline.trim();
            if st == "while true; do" {
                in_loop = true;
                continue;
            }
            if in_loop && st == "done" {
                break;
            }
            if !in_loop {
                continue;
            }
            // case 臂的模式段：`--xxx)` 之前那一截，形如两个连字符 ＋ `[A-Za-z0-9-]+`
            let arm: String = st.chars().take_while(|c| *c != ')').collect();
            let dashes = arm.chars().take(2).filter(|c| *c == '-').count();
            if arm.chars().count() > 2
                && dashes == 2
                && arm
                    .chars()
                    .skip(2)
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
                && st.chars().count() > arm.chars().count()
            {
                accepts.push(arm);
            }
        }
        accepts
    }

    /// 没有旗标循环的 shell 子进程：它只认第一个参数那一形 `[ "${1:-}" = --x ]`（`cc-list` / `cc-agents` 的 `--tsv`）。
    fn shell_first_arg_flags(script: &str) -> Vec<String> {
        let pat = "[ \"${1:-}\" = --";
        script
            .match_indices(pat)
            .map(|(at, _)| {
                // 变量名刻意不叫 `rest`：棘轮按文件内变量名认语料，同名会卷进本文件别处的无关匹配。
                let flag_tail = &script[at + pat.len() - 2..];
                flag_tail
                    .chars()
                    .take_while(|c| *c == '-' || c.is_ascii_alphanumeric())
                    .collect()
            })
            .collect()
    }

    /// 🔴 [`TERMINAL_SURFACE_FILES`] 里的每一份，都必须**真的**被它声称的那条判据接住。
    ///
    /// 没有这一条，那张表就退化成一张免检章：往里加一行就能把一整个文件的
    /// `--旗标` 从两条判据底下同时抽走，而两边都报绿。
    ///
    /// ⚠ 它查的是**那条判据存在且扫的就是这份文件**，不查它判得对不对
    ///（同本文件其余各条那条登记过的边界）。
    #[test]
    fn every_terminal_surface_file_is_covered_by_its_own_guard() {
        assert!(
            !TERMINAL_SURFACE_FILES.is_empty(),
            "这张表空了 —— 要么真的没有终端命令面了（那就连同 `found.retain` 一起摘掉），\n             要么是被人掏空了。空表让 `retain` 变成 no-op，本条因此在空转。"
        );
        // 今天的接盘判据：`control::ccm` 的用法行判据。它必须①存在 ②扫的是同一份文件。
        //
        // 🔴 〔步 7c 剖分 2026-09-19 · C 类〕**读的那份换了。**
        // `own_source()` 回的是生产段那份 `control/ccm/mod.rs`；而接盘的那条判据
        // （`fn every_flag_we_accept_has_a_usage_line`）这一轮跟着测试段搬进了
        // `tests/backend/control/ccm_tests.rs` ⇒ 在 `own_source()` 里恒找不到。
        let ccm_mod = include_str!("control/ccm_tests.rs");
        assert!(
            ccm_mod.contains("fn every_flag_we_accept_has_a_usage_line"),
            "接盘判据 `every_flag_we_accept_has_a_usage_line` 不在 `control/ccm/mod.rs` 里了 —— \n             `TERMINAL_SURFACE_FILES` 那一格从此没人接，把那份文件放回 `DISPATCH_FILES`，\n             或者给它另找一条判据并把这里改掉。"
        );
        // ⚠ 针**运行时拼**：写成字面量的话本文件就多出一处「解析不出路径的 `include_*!`」，
        //   而 `cross_half_edge_registry` 的抽取器按文本数调用数 —— 那是一次现打逮到的假阳。
        //
        // 🔴 〔步 7c 剖分 2026-09-19 ·  第二条元教训〕
        //    **原来的针是 `include_str!("argv.rs")` —— 针里嵌着那条相对路径的全文。**
        //    剖分把那条判据搬去了 `tests/backend/control/ccm_tests.rs`，剖分器**按原语义
        //    重定向**了它的 `include_str!`，于是那一行今天逐字是
        //    `include_str!("../../../src/backend/control/ccm/argv.rs")` ⇒ 整串针失配
        //    （现打红：「接盘判据不再扫 `argv.rs` 了」）。
        //    ⇒ 针改成**只认路径的收尾文件名**：它认的是「有一处 include 读的是 argv.rs」，
        //      而不是「那条相对路径逐字长这样」。前者是事实，后者是位置。
        // ⚠ 针要**跳过左括号与引号之间的空白**：`cargo fmt` 会把长的嵌入调用折行
        //   （左括号一行、路径字面量另一行），那时「宏名紧跟引号」就不再连写。
        //   〔步 7c 现打：格式化那一趟本条就是这么红的 —— 又一处「按位置认的针」。〕
        //
        // ⚠⚠ **这段注里刻意不写那个宏调用的完整形**〔步 7c 第二次现打〕：
        //   `cross_half_edge_registry::every_non_literal_include_is_registered_with_a_reason`
        //   按文本数「嵌入宏的调用处」，而它数的是**整份文件**（含注释）。
        //   第一版在这里逐字写了 `宏名!(` 加一个转义换行 ⇒ 那条判据把这句话数成了
        //   「一处解析不出路径的调用」，当场红。**判据数到注释**，本仓记过多次。
        let opener = format!("include_str{}(", "!");
        let reads_argv = ccm_mod
            .match_indices(opener.as_str())
            .filter(|(i, _)| {
                let rest = ccm_mod[i + opener.len()..].trim_start();
                rest.strip_prefix('"')
                    .and_then(|r| r.split('"').next())
                    .is_some_and(|path| path.ends_with("argv.rs"))
            })
            .count();
        assert!(
            reads_argv >= 1,
            "接盘判据不再扫 `argv.rs` 了 —— 它声称覆盖的那份文件与它实际扫的对不上。\n\
             （本条只认 include 路径的**收尾文件名**，所以搬树改相对前缀不会让它假红；\n\
             它真红就是那条 include 没了或读了别的文件。）"
        );
        // 渲染器那一格的接盘判据：渲出的每一行都过 `argv::parse`。
        let render_side = include_str!("control/launch_render/launch_cli_parity_tests.rs");
        assert!(
            render_side.contains("fn every_rendered_ccm_line_is_accepted_by_the_ccm_argv")
                && render_side.contains("argv::parse("),
            "渲染器那一格的接盘判据不在了 —— `ccm_invocation.rs` 的旗标从此没人管"
        );
        for (f, why) in TERMINAL_SURFACE_FILES {
            assert!(
                f.starts_with("control/ccm/") || *f == "control/launch_render/ccm_invocation.rs",
                "`{f}` 不在 `control/ccm/` 下 —— 今天那条接盘判据只扫得到那一族，\n                 别的文件放进这张表等于没人管它。"
            );
            assert!(
                why.len() > 40,
                "`{f}` 那行理由太短，说不清它为什么不是 wire 面"
            );
        }
    }

    /// ★ **R2（登记制）**：`control/` 与 `observe/` 里每一个 serde 类型都必须**逐条登记**。
    ///
    /// # 为什么是登记制，不是「白名单恰好一条」
    ///
    /// 设计稿原本提的是「`control/`/`observe/` 不许 derive serde，白名单**恰好一条**
    /// `resolve_query.rs`」。**实测那个前提是错的** —— 今天有 **3 个文件、6 个类型**，
    /// 而且后两个**搬进 `wire/`（流协议的家）是错误归类**：
    ///
    /// - `resolve_query.rs` 的 4 个：与仓外 aterm **冻结在 2026-07-18** 的一次性契约；
    /// - `fork_write.rs::ForkResult`：**一次性子命令 `--fork-session` 的出参**，不是流协议；
    /// - `accounts_query.rs::RawAccount`：**根本不是 wire** —— 它在解析账号库的清单
    ///   **文件**（文件 schema）。
    ///
    /// ⇒ 目标不变（新增一个上线类型不许溜进来），形状改成 `spawn_registry` 那一套：
    /// **逐条列举 + 写明它是什么 + 机检**。
    ///
    /// 「上线类型收进 `src/wire/` 目录树」等**真出现第二个流协议类型文件**时再做 ——
    /// 今天只有一个（`wire.rs`），为一个文件建目录树是空转。
    #[test]
    fn every_serde_type_outside_wire_is_registered() {
        /// (文件, 类型名, 它是什么 —— 为什么不在 `wire.rs`)
        const ALLOWED: &[(&str, &str, &str)] = &[
            (
                "control/resolve_query.rs",
                "ResumeSpec",
                "一次性 `--resolve` 的入参。契约与仓外 aterm 冻结在 2026-07-18，逐字不动",
            ),
            (
                "control/resolve_query.rs",
                "Capabilities",
                "同上，`CommandPlan` 的一部分",
            ),
            ("control/resolve_query.rs", "CommandPlan", "同上，出参"),
            ("control/resolve_query.rs", "ResolveError", "同上，错误形状"),
            (
                "control/fork_write.rs",
                "ForkResult",
                "一次性子命令 `--fork-session` 的**出参**，不是流协议帧 —— 搬进 wire 是错误归类",
            ),
            (
                "observe/accounts_query.rs",
                "NeedsRow",
                "`sessions-needs` 一行的出参（会话 id ＋ `history-facts` 那一份 `needs`），命令应答不是流协议帧",
            ),
            (
                "observe/accounts_query.rs",
                "Badge",
                "`accounts-list` 每个号那一枚徽章（命令应答里的一块），不是流协议帧",
            ),
            (
                "observe/accounts_query.rs",
                "BadgeFacts",
                "**不上线**：徽章那一判照类型读回清单那一行的几格（读，不写）",
            ),
            (
                "observe/accounts_query.rs",
                "RawAccount",
                "**根本不是 wire**：它在解析账号库的清单**文件**（文件 schema）",
            ),
            (
                "observe/history_query.rs",
                "Can",
                "`history-list` 每行（与全文搜索每个会话）的「这一行能做什么」那一格：命令应答里的一块，不是流协议帧",
            ),
            (
                "observe/history_query.rs",
                "IndexRow",
                "一次性子命令 `--read-session-from-offset … --index` 的**出参行**（骨架索引），\
                 不是流协议帧；形状登记在 `IPC-PROTOCOL.md` §10.3",
            ),
            (
                "observe/history_query.rs",
                "TailPlan",
                "帧命令 `history-tail` 的应答 `data`（请求-应答，不是推送帧）；出参登在命令注册表，\
                 由 `every_command_declares_exactly_the_fields_it_puts_out` 与真序列化对拍",
            ),
            (
                "observe/history_query.rs",
                "RecordProbe",
                "同上，帧命令 `history-record` 的应答 `data`",
            ),
            (
                "observe/search_query.rs",
                "Merged",
                "同上，帧命令 `history-search-merge` 的应答 `data`",
            ),
            (
                "observe/listing_scan.rs",
                "ListingHead",
                "**只读不出**：扫历史清单时读 CLI 记录行的窄探针（记录文件 schema 里要的那几格），不上线",
            ),
            (
                "agents/claudecode/turn.rs",
                "Probe",
                "**只读不出**：turn-end 判词读 CLI 记录行的窄探针（记录文件 schema 的五格），不上线",
            ),
            ("agents/claudecode/turn.rs", "ProbeMessage", "同上，`message` 里那一格"),
            ("agents/claudecode/turn.rs", "LooseBool", "同上，形状不对当缺"),
            ("agents/claudecode/turn.rs", "LooseStr", "同上，形状不对当缺"),
        ];

        let files: &[(&str, &str)] = &[
            (
                "control/launch.rs",
                include_str!("../../src/backend/control/launch.rs"),
            ),
            (
                "control/resolve_query.rs",
                include_str!("../../src/backend/control/resolve_query.rs"),
            ),
            (
                "control/fork_write.rs",
                include_str!("../../src/backend/control/fork_write.rs"),
            ),
            (
                "control/tmux_hook.rs",
                include_str!("../../src/backend/control/tmux_hook.rs"),
            ),
            (
                "observe/watcher.rs",
                include_str!("../../src/backend/observe/watcher.rs"),
            ),
            (
                "observe/tmux_observe.rs", // 原 `watcher.rs` A 块（人群不缩）
                include_str!("../../src/backend/observe/tmux_observe.rs"),
            ),
            (
                "observe/accounts_query.rs",
                include_str!("../../src/backend/observe/accounts_query.rs"),
            ),
            (
                "observe/history_query.rs",
                include_str!("../../src/backend/observe/history_query.rs"),
            ),
            (
                "observe/search_query.rs",
                include_str!("../../src/backend/observe/search_query.rs"),
            ),
            (
                "agents/codex/parse.rs",
                include_str!("../../src/backend/agents/codex/parse.rs"),
            ),
            (
                "agents/claudecode/turn.rs",
                include_str!("../../src/backend/agents/claudecode/turn.rs"),
            ),
            (
                "observe/listing_scan.rs",
                include_str!("../../src/backend/observe/listing_scan.rs"),
            ),
        ];

        let mut found: Vec<(String, String)> = Vec::new();
        for (name, raw) in files {
            let src = crate::guard_support::production_code(raw);
            let mut from = 0usize;
            while let Some(rel) = src[from..].find("#[derive(") {
                let d = from + rel;
                let Some(attr_end) = balanced(src.as_bytes(), d + 1, b'[', b']') else {
                    break;
                };
                let is_ser = src[d..=attr_end].contains("Serialize")
                    || src[d..=attr_end].contains("Deserialize");
                if is_ser {
                    // 类型名 = 属性之后第一个 `struct X` / `enum X`。
                    let tail = &src[attr_end..];
                    let ty = ["struct ", "enum "]
                        .iter()
                        .filter_map(|kw| tail.find(kw).map(|k| (k, *kw)))
                        .min_by_key(|(k, _)| *k)
                        .map(|(k, kw)| {
                            tail[k + kw.len()..]
                                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                                .next()
                                .unwrap_or("")
                                .to_string()
                        })
                        .unwrap_or_default();
                    if !ty.is_empty() {
                        found.push((name.to_string(), ty));
                    }
                }
                from = attr_end + 1;
            }
        }
        assert!(
            found.len() >= 6,
            "只扫到 {} 个 serde 类型 —— 抽取坏了，本断言在空转：{found:?}",
            found.len()
        );
        let unregistered: Vec<&(String, String)> = found
            .iter()
            .filter(|(f, t)| !ALLOWED.iter().any(|(af, at, _)| af == f && at == t))
            .collect();
        assert!(
            unregistered.is_empty(),
            "这些 serde 类型没在受管清单里：{unregistered:?}\n\
             `control/`/`observe/` 里出现一个上线类型，多半意味着它该进 `wire.rs`（流协议）——\n\
             如果它确实不是流协议（一次性子命令出参 / 文件 schema），\n\
             把它加进 `ALLOWED` 并**写明它是什么**。"
        );
        // 幽灵条目：登记了但代码里已经没有了 ⇒ 清单越攒越松。
        let ghosts: Vec<&(&str, &str, &str)> = ALLOWED
            .iter()
            .filter(|(f, t, _)| !found.iter().any(|(ff, tt)| ff == f && tt == t))
            .collect();
        assert!(ghosts.is_empty(), "清单里有幽灵条目：{ghosts:?}");
        for (f, t, why) in ALLOWED {
            assert!(!why.is_empty(), "{f}::{t} 没写理由");
        }
    }

    /// ★ **R4**：协议级错误码是**闭集**，且只有 `stream/inbound/` 可以发。
    ///
    /// # 为什么要分层（设计审计 · 视角 A · P6）
    ///
    /// `bad_request` 今天一词两义：协议级（信封 JSON 坏了 ⇒「**客户端代码写错了**，别重试」）
    /// vs 命令级（参数不合适 ⇒ 可能是用户输入）。客户端拿到的只有 `code` 字符串，分不出来。
    /// 命令级的形状错误叫 `bad_args`（全部命令一个码）；本条把这条分层**钉住**。
    ///
    /// ⚠ **`resolve` 是登记在案的例外**：它的命令级 parse 错误仍叫 `bad_request`，
    /// 因为一次性 `--resolve` 与仓外 aterm 的契约冻结在 2026-07-18，两条路复用同一个纯函数。
    /// 改它会破坏那份契约 ⇒ 如实登记，不顺手改。
    #[test]
    fn protocol_level_codes_are_never_emitted_from_the_control_layer() {
        // 协议级闭集：协议参考的第 3 节从同一张表生成。**只有 `stream/inbound/` 可以发这些。**
        // `bad_request` 不进本条：`resolve` 的命令级 parse 错误也叫它（登记在案的例外，见上）。
        let protocol_codes: Vec<&str> = crate::protocol_doc_gen::PROTOCOL_CODES
            .iter()
            .map(|(c, _)| *c)
            .filter(|c| *c != "bad_request")
            .collect();
        // 逐个文件扫 `control/`（`observe/` 不产 code，不在本条范围）。
        let files: &[(&str, &str)] = &[
            (
                "control/launch.rs",
                include_str!("../../src/backend/control/launch.rs"),
            ),
            (
                "control/resolve_query.rs",
                include_str!("../../src/backend/control/resolve_query.rs"),
            ),
            (
                "control/fork_write.rs",
                include_str!("../../src/backend/control/fork_write.rs"),
            ),
            (
                "control/tmux_hook.rs",
                include_str!("../../src/backend/control/tmux_hook.rs"),
            ),
        ];
        // 匹配器自检：独立手写的样本必须命中。
        for sample in ["Err((\"unknown_command\", x))", "code: \"not_cancellable\""] {
            assert!(
                protocol_codes.iter().any(|c| sample.contains(c)),
                "匹配器漏了这种写法：{sample}"
            );
        }
        let mut violations: Vec<String> = Vec::new();
        for (name, raw) in files {
            let prod = crate::guard_support::production_code(raw);
            for c in &protocol_codes {
                if prod.contains(c) {
                    violations.push(format!("{name} 里出现了协议级 code `{c}`"));
                }
            }
        }
        assert!(
            violations.is_empty(),
            "{violations:?}\n\
             协议级 code 的语义是「客户端代码写错了，别重试」，只有 `stream/inbound/` 有资格判定它。\n\
             命令自己的失败请用命令级 code（并登记进 `CommandSpec::codes`）。"
        );

        // 反面：注册表里登记的命令级 code 不许与协议级重名（`resolve` 的 `bad_request` 例外）。
        for spec in crate::stream::inbound::REGISTRY {
            for c in spec.codes {
                assert!(
                    !protocol_codes.contains(c),
                    "`{}` 登记了协议级 code `{c}` —— 那一层不归命令管",
                    spec.name
                );
            }
        }
    }

    /// F06c：`wire.rs` 的 `enum Frame` **段界内**的变体名 → 线上 `kind`（snake_case）。
    ///
    /// ⚠ **必须先框段界再数**：`wire.rs` 里不止一个枚举，
    /// 摸底时用「行首缩进 + 大写开头」的正则数出 **14**，真值 **11** —— 多出来的是别的枚举。
    fn frame_variants() -> Vec<String> {
        let src =
            crate::guard_support::production_code(include_str!("../../src/backend/stream/wire.rs"));
        // 运行时拼，免得命中本文件自己的说明文字。
        let needle = format!("pub enum {}", "Frame");
        let at = src
            .find(needle.as_str())
            .unwrap_or_else(|| panic!("`wire.rs` 里找不到 `{needle}` —— 抽取坏了或枚举改名了"));
        let open = src[at..]
            .find('{')
            .map(|k| at + k)
            .expect("`enum Frame` 后面没有花括号");
        // 花括号配平框出段界（变体体里还有嵌套花括号）。
        let bytes = src.as_bytes();
        let mut depth = 0usize;
        let mut end = open;
        for (i, c) in bytes.iter().enumerate().skip(open) {
            match *c {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        assert!(
            end > open + 100,
            "`enum Frame` 段界只框出 {} 字节 —— 配平坏了",
            end - open
        );
        let span = &src[open + 1..end];
        // 只取**深度 1** 的变体名：行首（去缩进后）是大写字母开头的标识符，后跟 `{` 或 `,`。
        let mut out = Vec::new();
        let mut depth = 0usize;
        for line in span.lines() {
            let t = line.trim();
            if depth == 0 {
                if let Some(name) = t
                    .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .next()
                    .filter(|w| !w.is_empty() && w.starts_with(char::is_uppercase))
                {
                    let rest = t[name.len()..].trim_start();
                    if rest.starts_with('{') || rest.starts_with(',') || rest.is_empty() {
                        let mut snake = String::new();
                        for (i, ch) in name.chars().enumerate() {
                            if ch.is_uppercase() && i > 0 {
                                snake.push('_');
                            }
                            snake.extend(ch.to_lowercase());
                        }
                        out.push(snake);
                    }
                }
            }
            depth += t.matches('{').count();
            depth = depth.saturating_sub(t.matches('}').count());
        }
        out
    }

    /// ★★ **`EMITS` 必须是 `Frame` 变体的子集，且缺席的三个要有名有姓**
    /// 〔audit-0805 F18 / 报告 §4.2〕。
    ///
    /// # 它此前**一条判据都没有**
    ///
    /// `EMITS` 全仓只在 `main.rs` 被读一次（塞进 hello）。
    ///
    /// # 而它的**定义与值对不上**
    ///
    /// `src/doc/IPC-PROTOCOL.md` 原本把它定义成「本 backend **会发射的帧 kind 集**」，
    /// 而 `EMITS` 8 项**不含** `hello`/`reply`/`cancelled` —— 这三个 backend **确实会发**。
    /// ⇒ 按字面读，它是错的；按意图读，它是「**门控用**帧集」（握手与应答不需要门控：
    /// `hello` 是首帧、客户端必然收；`reply`/`cancelled` 是**应答**，只在你发过命令之后才来）。
    /// 本条把那个意图钉住：**子集 + 缺席者必须逐个有理由**。
    ///
    /// ⚠ 这不是「补个数字」——按定框 **E12**，散文数字的修法只有两种：
    /// 送进一条会红的判据，或删副本只留指针。本条是前者。
    #[test]
    fn emits_is_a_subset_of_frame_kinds_with_named_exemptions() {
        let variants = frame_variants();
        assert!(
            variants.len() >= 9,
            "只从 `enum Frame` 抽到 {} 个变体 —— 抽取坏了，本条会零命中地绿",
            variants.len()
        );

        // 从 main.rs 生产段抠 `const EMITS: &[&str] = &[ "a", "b", … ];`
        // `EMITS` 已搬进 `lib.rs` ⇒ 扫描面取**两份的全集**
        //（住址只有一处：`guard_support::backend_root_source`，理由见那函数头注）。
        let prod = guard_core::production_code(&crate::guard_support::backend_root_source());
        let start = prod
            .find("const EMITS")
            .expect("找不到 `const EMITS` —— 抽取器坏了，本条会零命中地绿");
        let end = prod[start..]
            .find("];")
            .expect("`const EMITS` 没有结尾 —— 抽取器坏了");
        let body = &prod[start..start + end];
        let emits: Vec<String> = body
            .match_indices('"')
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| body[c[0] + 1..c[1]].to_string())
            .collect();
        assert!(
            emits.len() >= 5,
            "只从 `EMITS` 抠到 {} 项（实测应为 8）—— 抠法坏了：{emits:?}",
            emits.len()
        );

        // ① 子集：`EMITS` 里每一项都得是真帧种（防写错名字后没人发现）。
        for e in &emits {
            assert!(
                variants.contains(e),
                "`EMITS` 里的 `{e}` 不是任何一个 `Frame` 变体的 kind。\n\
                 消费侧（aterm）拿它**门控消费** —— 声明一个不存在的帧种，\n\
                 对面会去等一个永远不来的东西。今天的变体：{variants:?}"
            );
        }

        // ② 缺席者必须逐个有名有姓，且理由写在这里（不是随便少几个）。
        const EXEMPT: &[(&str, &str)] = &[
            ("hello", "首帧、客户端必然收到，不需要门控"),
            (
                "reply",
                "应答：只在客户端发过命令之后才来，由 `commands` 那一轴管",
            ),
            ("cancelled", "同 `reply`，属入方向应答族"),
        ];
        let missing: Vec<&String> = variants.iter().filter(|v| !emits.contains(v)).collect();
        for m in &missing {
            assert!(
                EXEMPT.iter().any(|(k, _)| *k == m.as_str()),
                "帧种 `{m}` 既不在 `EMITS` 里、也不在本条的豁免表里。\n\
                 ★ 要么它该进 `EMITS`（backend 会发它、消费侧要据此门控），\n\
                 要么它是握手/应答那一族 —— **那就把理由写进 `EXEMPT`**。\n\
                 两者都不做 = `emits` 这个声明对下游又变回一句不可信的话。"
            );
        }
        // ③ 豁免表本身不许长草：列了却其实在 `EMITS` 里，说明表过期了。
        for (k, _) in EXEMPT {
            assert!(
                !emits.iter().any(|e| e == k),
                "`{k}` 已经在 `EMITS` 里了，却还留在豁免表 —— 豁免表过期了"
            );
        }
    }

    /// 「参数不对」只有一个码（`bad_args`）：注册表里没有一条命令声明 `invalid_args`，后端生产段也零处拼它
    /// （正控：`bad_args` 真有人声明；扫描器在同一批文件里找得到 `"bad_args"`）。
    #[test]
    fn bad_arguments_have_exactly_one_code() {
        let reg = crate::stream::inbound::REGISTRY;
        let both: Vec<&str> = reg
            .iter()
            .filter(|s| s.codes.contains(&"invalid_args"))
            .map(|s| s.name)
            .collect();
        assert!(both.is_empty(), "这几条还声明 `invalid_args`：{both:?}");
        assert!(
            reg.iter().any(|s| s.codes.contains(&"bad_args")),
            "正控：一条声明 `bad_args` 的命令都没有"
        );
        let root = crate::guard_support::src_root();
        let mut hits = Vec::new();
        let mut control = false;
        for (at, raw) in guard_core::scan_tree!(&root, &["rs"]) {
            let prod = crate::guard_support::production_code(&raw);
            control |= prod.contains("\"bad_args\"");
            if prod.contains("\"invalid_args\"") {
                hits.push(at.display().to_string());
            }
        }
        assert!(
            control,
            "正控：生产段里一处 `\"bad_args\"` 都没扫到 —— 扫描器坏了"
        );
        assert!(hits.is_empty(), "生产段还在发 `invalid_args`：{hits:?}");
    }
}
