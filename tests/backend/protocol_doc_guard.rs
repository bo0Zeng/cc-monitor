//! 协议面本身的几条判据（不是文档对拍：协议文档的逐格那一半从代码生成，见 `protocol_doc_gen`）。
//!
//! - 子命令分派的文件名单（[`DISPATCH_FILES`]）：`dispatch_registry_is_complete` 反向核对名单没漏文件。
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
    // 🔴 `lib.rs`：子命令选项词那张表（`SUBCOMMAND_OPTIONS`）住库面 ⇒ 按本名单的口径（「生产段里出现 `"--`」）它就是一份。
    //    ⚠ **不是我判断它该进来，是下面 `dispatch_registry_is_complete` 自己算出来的**。
    //    ⚠ 漏登它的后果逐字写在本名单头注里：那份文件里的 `--子命令` **全部不受
    //      IPC-PROTOCOL.md 对拍约束**，而对拍判据照常报绿。
    ("lib.rs", include_str!("../../src/backend/lib.rs")),
    // const1：`lib.rs` 里那几个旗标的字面量（流模式显式词 · 子命令后面的选项词）住址挪了 —— 词进 `wire/cli_words.rs`，
    //   答题 hook 那个子命令的词进 `agents/asks.rs`（写进 hook 设置的是适配层）。同 `lib.rs` 那一行：本判据自己算出来的。
    (
        "wire/cli_words.rs",
        include_str!("../../src/backend/wire/cli_words.rs"),
    ),
    // 期限口的读法（`within_of`）从 `stream/cli.rs` 挪进读入参那一处（`--resident-call` 也读它）：本判据自己算出来的。
    (
        "control/cli_args.rs",
        include_str!("../../src/backend/control/cli_args.rs"),
    ),
    (
        "agents/asks.rs",
        include_str!("../../src/backend/agents/asks.rs"),
    ),
    // `control/launch.rs`：终端管理送字那一段里有 `"--"` 字面量（`send-keys -l -- <字>` · `set-buffer -- <字>`：
    // 显式结束旗标，免得以 `--` 开头的字被 tmux 当成选项）。登记，不改判据。
    (
        "control/launch.rs",
        include_str!("../../src/backend/control/launch.rs"),
    ),
    // P4d：控制面的 CLI 入口。它**不做 match 分派**（认哪些 flag 由
    // `stream::cli::spec_for` 从 `inbound::REGISTRY` 派生），但读选项值那几处有 `"--"` 字面量
    // （`starts_with("--")`：下一格是不是另一个旗标）—— 派生的文件集因此把它扫了进来。登记，不改判据。
    (
        "stream/cli.rs",
        include_str!("../../src/backend/stream/cli.rs"),
    ),
    // 手写的一次性子命令那张 `(子命令, 实现)` 表。
    (
        "stream/oneshot.rs",
        include_str!("../../src/backend/stream/oneshot.rs"),
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
    // 它不做 match 分派，只在用法串里提自己的名字；登记在这里，它生产段里的 `"--` 字面量
    // （连藏在常量里的）照样受对拍约束。
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
    // claude 那一家在 app 里登录一个号 · 一个 MCP 服务器（`sign_in::FACE.args` 里的 `--claudeai` · `MCP_FACE.args` 里的 `--no-browser`）：发给 claude 这个子进程的旗标，同上。
    (
        "agents/claudecode/sign_in.rs",
        include_str!("../../src/backend/agents/claudecode/sign_in.rs"),
    ),
    // claude 那一家的开窗那一句（`WARM.args` 里的 `--model` · `--tools` …）：发给 claude 这个子进程的旗标，同上。
    (
        "agents/claudecode/mod.rs",
        include_str!("../../src/backend/agents/claudecode/mod.rs"),
    ),
    // codex 那一家的起会话事实（`LAUNCH_ARGS = ["--no-daemon"]`）：发给 codex 这个子进程的旗标，同上。
    (
        "agents/codex/resume.rs",
        include_str!("../../src/backend/agents/codex/resume.rs"),
    ),
    // 计划读面起 pb（`pb dump --node` 按节点取 agent 视角）：发给 pb 这个子进程的旗标，同上。
    (
        "plan/dump.rs",
        include_str!("../../src/backend/plan/dump.rs"),
    ),
];

/// 🔴 **终端命令面**的文件 —— 它们持有 `--旗标` 字面量，但那些**不是 wire 子命令**。
///
/// # 为什么要有这张表
///
/// `dispatch_registry_is_complete` 的判法是「生产段里出现 `"--` 字面量的文件集
/// == [`DISPATCH_FILES`]」，而 [`DISPATCH_FILES`] 里的每个 token 都得落进
/// `src/doc/IPC-PROTOCOL.md` §10 的代码跨度。那份文档是 **monitor↔backend 的冻结线上契约**，
/// 读者是手机端，改一个字就是改协议。
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
        "ccm_line/argv.rs",
        "`ccm` 这套终端 argv 的唯一解析口。旗标字面量**只住这一个文件**\
         （由 `ccm_line::argv::tests::ccm_flag_literals_live_only_in_argv_rs` 钉），\
         它们是终端命令面、不是 wire 协议面。",
    ),
    // `ccm …` 调用行的渲染器（从 monitor 搬进后端）：它**写**那套终端旗标给用户的 shell 跑，不是后端分派的 argv。
    (
        "control/launch_render/ccm_invocation.rs",
        "`ccm …` 调用行的渲染器：它写的是用户终端里那一行 `ccm` 的旗标（终端命令面），不是后端自己分派的 wire 子命令；\
         接盘判据 `ccm_invocation_tests::every_rendered_ccm_line_is_accepted_by_the_ccm_argv`（渲出的每一行都过真解析器 `argv::parse_layered`）。",
    ),
];

/// 🔴 **子进程旗标** —— 后端**发给它转调的那个子进程**的 `--旗标`，不是后端自己分派的 argv。
///
/// # 为什么要有这一类
///
/// [`DISPATCH_FILES`] 里**每一个** `"--x"` 字面量都算「后端自己认的 token」，要进 `IPC-PROTOCOL.md §10`。
/// 而后端转调 `cc-bus list` 时要发 `--tsv` —— 那是**cc-bus list 的**旗标：塞进 `SUBCOMMAND_OPTIONS`
/// 会改 `is_query_mode` 的行为（`--tsv` 打头的 argv 从此被当成「子命令选项脱离了子命令」），
/// 写进 §10 会让冻结契约描述一件它不负责的事。
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
/// 🔴 **不许**把旗标拼成运行期字符串（`format!("--{}", "tool")`）来躲 [`dashdash_literals`] ——
/// 那是把扫描面静默挖空；正路就是登记在这里、让 ①② 接住。
pub(crate) const CHILD_PROCESS_FLAGS: &[(&str, &str, &[&str], &str)] = &[
    // 一份文件发给几个子进程的旗标（一份文件可以登记几行，① 按文件取并集）。
    // 子进程是仓外的 `claude` ⇒ ② 那一侧读金样 `agent-profile-golden.tsv` 里 flag 形的 `resume_token`
    //   （那张表记的就是各家 agent 的命令形，见接盘判据里按扩展名分的那一支）。
    (
        "agents/claudecode/resume.rs",
        "tests/__fixtures__/agent-profile-golden.tsv",
        &["--permission-mode", "--resume", "--session-id", "--settings"],
        "`claude` 那一家 resume 的 flag 形字面量（`agents/claudecode/resume.rs::RESUME_TOKEN`）、起新会话先定 sid 的旗标\
         （`SESSION_ID_FLAG`，新起都带）、起会话选权限档的旗标（`PERMISSION_MODE_FLAG`）与挂会话级答题 hook 的旗标（`SETTINGS_FLAG`，ccm 起的都带）：都是 claude 这个子进程的命令面，后端 argv 从不认它们，\
         线上契约里也没有它们的位置。",
    ),
    (
        "agents/claudecode/sign_in.rs",
        "tests/__fixtures__/fake-claude-auth-login.sh",
        &["--claudeai"],
        "`claude auth login` 选订阅号那一支的旗标（`agents/claudecode/sign_in.rs::FACE`）：claude 这个子进程的命令面，后端 argv 从不认它，\
         线上契约里也没有它的位置。仓外的真 claude 读不到 ⇒ ② 那一侧读判据用的假登录子命令（它照实只认这一个）。",
    ),
    (
        "agents/claudecode/sign_in.rs",
        "tests/__fixtures__/fake-claude-mcp-login.sh",
        &["--no-browser"],
        "`claude mcp login <名>` 不开浏览器、只打印授权链接那一支的旗标（`agents/claudecode/sign_in.rs::MCP_FACE`）：claude 这个子进程的命令面，\
         后端 argv 从不认它，线上契约里也没有它的位置。仓外的真 claude 读不到 ⇒ ② 那一侧读判据用的假 MCP 登录子命令（它照实只认这一个）。",
    ),
    (
        "agents/claudecode/mod.rs",
        "tests/__fixtures__/fake-claude-warm.sh",
        &[
            "--model",
            "--no-session-persistence",
            "--output-format",
            "--setting-sources",
            "--strict-mcp-config",
            "--system-prompt",
            "--tools",
        ],
        "开窗那一句（`agents/claudecode/mod.rs::WARM`，`accounts::quota::warm` 让本二进制当 ccm 交给 claude）：haiku · 不给工具 · \
         只读项目级设置 · 不起 MCP · 不落会话记录 · 换系统提示。claude 这个子进程的命令面，后端 argv 从不认它们，线上契约里也没有它们的位置。\
         仓外的真 claude 读不到 ⇒ ② 那一侧读判据用的假子进程（它照实只认这几个）。",
    ),
    (
        "agents/codex/resume.rs",
        "tests/__fixtures__/agent-profile-golden.tsv",
        &["--no-daemon"],
        "`codex` 那一家起会话时垫在最前面的参数（`agents/codex/resume.rs::LAUNCH_ARGS`，不连共享后台）：\
         它是 codex 这个子进程的命令面，后端 argv 从不认它，线上契约里也没有它的位置。",
    ),
    // 子进程是仓外的 pb（planned-build 插件）⇒ ② 那一侧读判据用的假 pb（`tests/backend/plan/fixture.rs::fake_pb`，它照实只认这几个）。
    (
        "plan/dump.rs",
        "tests/backend/plan/fixture.rs",
        &["--domains", "--node", "--refs"],
        "`pb dump --node <仓库> <编号> <工作区>`：点到一格才取 agent 站在那个节点时 pb 印的那一段（`plan/dump.rs::run_node`）；\
         `pb dump --refs <仓库> <工作区>`：会话消息里提到哪几个节点，正文从标准输入给（`plan/dump.rs::run_refs`）；\
         `pb dump --domains <目录>`：新计划框问那个目录能选哪几档领域（`plan/dump.rs::run_domains`）。\
         是 pb 的命令面，后端 argv 从不认它，线上契约里也没有它的位置。",
    ),
];

/// 子进程认、而后端**刻意不发**的旗标 —— 每条带理由（②那一向的另一半）。
pub(crate) const CHILD_FLAGS_NOT_SENT: &[(&str, &str, &str)] = &[];

/// 一份源码生产段里的 `"--x"` 字面量（按出现序去重）—— 分派名单反向核对与
/// 子进程旗标那条接盘判据**共用这一处取法**（两处各写一份的话，一处放宽、一处没跟，就会各说各话）。
pub(crate) fn dashdash_literals(raw: &str) -> Vec<String> {
    let src = guard_core::production_code(raw);
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
        let root = guard_core::addr::backend_src_root();
        let mut found = Vec::new();
        for rel in guard_core::files_by_extension(&root, "rs") {
            let raw = std::fs::read_to_string(root.join(&rel)).unwrap_or_default();
            // 整份文件都是 `#![cfg(test)]` 的（护栏自己就是）：release 构建里
            // 根本不存在，不可能分派任何东西。`production_code` 只剥
            // `#[cfg(test)] mod` 块，剥不掉文件级的内层属性 —— 得在这里跳。
            if raw.contains("#![cfg(test)]") {
                continue;
            }
            // 判据是「生产段里出现 `"--` 字面量」，不按分派语法探测：
            // 把 `pub const CTRL_FLAG: &str = "--ccm-hidden-ctrl";` 放进一份没有 match 臂的文件，
            // 按语法探测的两条护栏会全绿。只要**提到**一个 `--token` 就得登记。
            if guard_core::production_code(&raw).contains("\"--") {
                found.push(rel);
            }
        }
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
    /// ⚠ 买不到：「旗标的**值**对不对」—— 那一格由子进程自己 rc=2 判，
    /// 行为判据住各子进程自己那一侧（真跑）。
    #[test]
    fn child_process_flags_are_exactly_what_the_file_sends_and_what_the_child_accepts() {
        assert!(
            !CHILD_PROCESS_FLAGS.is_empty(),
            "登记表空了 —— 要么真没有子进程旗标了（那就把这张表连同本条一起删），\
             要么是被人掏空了"
        );
        let repo = guard_core::addr::backend_src_root()
            .parent()
            .and_then(|p| p.parent())
            .expect("从 src/backend 回不到仓根")
            .to_path_buf();
        for (file, child, flags, _) in CHILD_PROCESS_FLAGS {
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
                // flag 形的 `resume_token` ＋ `launch_args` ＋ 起新会话先定 sid 的 `preset_sid` ＋ 选权限档的 `permission` ＋ 挂答题 hook 的 `answer_hook_flag`。
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
                            && matches!(
                                f[1],
                                "resume_token"
                                    | "launch_args"
                                    | "preset_sid"
                                    | "permission"
                                    | "answer_hook_flag"
                            ))
                        .then(|| {
                            f[2].split(' ')
                                .filter(|w| w.starts_with("--"))
                                .map(str::to_string)
                                .collect::<Vec<_>>()
                        })
                    })
                    .flatten()
                    .collect()
            } else if child.ends_with(".rs") {
                // 子进程在仓外、判据用的假子进程写在 Rust 夹具里（假 pb 的入口脚本嵌在 `format!` 里）：
                //   它认的旗标 ＝ 那份夹具里每一个 `"--x"` 字面量（整份都是判据，不剥测试段）。
                literal_flags(&script)
            } else if script.lines().any(|l| l.trim() == "while true; do") {
                shell_loop_flags(&script)
            } else {
                shell_first_arg_flags(&script)
            };
            accepts.sort();
            accepts.dedup();
            // 假子进程夹具（`.rs`）一份认几份文件发的旗标（各登记一行）⇒ ② 比的是这个子进程那几行的并集（同 ① 按文件取并集）；
            //   金样（`.tsv`）按家分，照旧逐行比。
            let mut want: Vec<String> = if child.ends_with(".rs") {
                CHILD_PROCESS_FLAGS
                    .iter()
                    .filter(|(_, c, ..)| c == child)
                    .flat_map(|(_, _, fl, _)| fl.iter().map(|s| s.to_string()))
                    .collect()
            } else {
                reg.clone()
            };
            for (c, f, _) in CHILD_FLAGS_NOT_SENT {
                if c == child {
                    want.push(f.to_string());
                }
            }
            want.sort();
            want.dedup();
            assert_eq!(
                accepts, want,
                "\n`{child}` 认的旗标，与「后端发的 ∪ 刻意不发的」对不上。\n\
                 左边多 ⇒ 子进程新认了一个旗标、这边没表态（发不发都要写一句）；\n\
                 右边多 ⇒ 后端发了一个子进程**不认**的旗标（它会被当成位置参数吃掉，\
                 后果多半是「目录不存在：--xxx」那一类）。"
            );
        }
    }

    /// 判据夹具里的假子进程那一形（Rust 源里嵌着它的脚本）：整份文件里 `"--x"` 字面量（不剥测试段：它整份就是判据）。
    fn literal_flags(script: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (at, _) in script.match_indices("\"--") {
            let tok: String = script[at + 1..]
                .chars()
                .take_while(|c| *c == '-' || c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if tok.len() > 2 && script[at + 1 + tok.len()..].starts_with('"') && !out.contains(&tok)
            {
                out.push(tok);
            }
        }
        out
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

    /// 没有旗标循环的 shell 子进程：它只认第一个参数那一形 `[ "${1:-}" = --x ]`（`cc-bus list` 的 `--tsv`）。
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
        // 🔴 **读的那份换了。**
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
        // 🔴 **原来的针是 `include_str!("argv.rs")` —— 针里嵌着那条相对路径的全文。**
        //    剖分把那条判据搬去了 `tests/backend/control/ccm_tests.rs`，剖分器**按原语义
        //    重定向**了它的 `include_str!`，于是那一行今天逐字是
        //    `include_str!("../../../src/backend/ccm_line/argv.rs")` ⇒ 整串针失配
        //    （现打红：「接盘判据不再扫 `argv.rs` 了」）。
        //    ⇒ 针改成**只认路径的收尾文件名**：它认的是「有一处 include 读的是 argv.rs」，
        //      而不是「那条相对路径逐字长这样」。前者是事实，后者是位置。
        // ⚠ 针要**跳过左括号与引号之间的空白**：`cargo fmt` 会把长的嵌入调用折行
        //   （左括号一行、路径字面量另一行），那时「宏名紧跟引号」就不再连写。
        //   格式化那一趟本条就是这么红过 —— 又一处「按位置认的针」。
        //
        // ⚠⚠ **这段注里刻意不写那个宏调用的完整形**：
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
        // 渲染器那一格的接盘判据：渲出的每一行都过真解析器（`argv::tests::parse` 即没有配置层的 `parse_layered`）。
        let render_side = include_str!("control/launch_render/ccm_invocation_tests.rs");
        assert!(
            render_side.contains("fn every_rendered_ccm_line_is_accepted_by_the_ccm_argv")
                && render_side.contains("argv::tests::parse("),
            "渲染器那一格的接盘判据不在了 —— `ccm_invocation.rs` 的旗标从此没人管"
        );
        for (f, _) in TERMINAL_SURFACE_FILES {
            assert!(
                f.starts_with("ccm_line/")
                    || f.starts_with("control/ccm/")
                    || *f == "control/launch_render/ccm_invocation.rs",
                "`{f}` 不在 ccm 那一族（`ccm_line/` · `control/ccm/`）下 —— 今天那条接盘判据只扫得到那一族，\n                 别的文件放进这张表等于没人管它。"
            );
        }
    }

    /// ★ **R2（登记制）**：`control/` 与 `observe/` 里每一个 serde 类型都必须**逐条登记**。
    ///
    /// # 为什么是登记制，不是「白名单恰好一条」
    ///
    /// 原本的提法是「`control/`/`observe/` 不许 derive serde，白名单**恰好一条**
    /// `resolve_query.rs`」。**实测那个前提是错的** —— 今天有 **3 个文件、6 个类型**，
    /// 而且后两个**搬进 `wire/`（流协议的家）是错误归类**：
    ///
    /// - `resolve_query.rs` 的几个：命令 `resolve` 的入参与成品（10-26 随那条没人调的帧命令删了）；
    /// - `fork_write.rs` 的几个：分叉的出参（当时是一次性子命令 `--fork-session` 的出参，今天是 typed 帧命令 `session-fork`），不是流协议。
    /// （当时还有账号清单那一侧解析清单**文件**的一个类型；10-24 账号库清单收成 `accounts/manage` 一个读口，它删了。）
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
                "control/fork_write.rs",
                "ForkReq",
                "typed 帧命令 `session-fork` 的入参（形状从类型生成进协议参考与 TS），命令应答不是流协议帧",
            ),
            ("control/fork_write.rs", "Forked", "同上，出参"),
            ("control/fork_launch.rs", "AccountFact", "同上，号那一格的值（`null` ＝ 账号 0）"),
            (
                "observe/accounts_query.rs",
                "Interrupts",
                "typed 帧命令 `machine-interrupts` 的回包（形状从类型生成进协议参考与 TS），命令应答不是流协议帧",
            ),
            (
                "observe/recognize/index_row.rs",
                "IndexRow",
                "骨架索引的一行（`history-index` 按列装运之前的那一形），\
                 不是流协议帧；形状登记在 `IPC-PROTOCOL.md` §10.3",
            ),
            (
                "observe/query/locate.rs",
                "RecordProbe",
                "同上，帧命令 `history-record` 的应答 `data`",
            ),
            (
                "observe/search_query.rs",
                "FindHit",
                "typed 帧命令 `history-find` 回包 `hits[]` 的一条（`faces/history_face.rs`；生成类型从它来）",
            ),
            (
                "history/listing.rs",
                "Saved",
                "**文件 schema**：历史清单缓存落盘那一份（数据目录 `cache/listing.json`），只本机后端自己读写，不上线",
            ),
            ("history/listing.rs", "SavedRow", "同上，一份记录那一行"),
            (
                "history/listing_scan.rs",
                "SessionScan",
                "同上，一份记录累计的那几格",
            ),
            (
                "history/listing_scan.rs",
                "SessionRow",
                "同上，清单缓存里的一行（`history_list` 读它拼成线上那一形 `HistoryRawRow`，它自己不上线）",
            ),
        ];

        let files: &[(&str, &str)] = &[
            (
                "control/launch.rs",
                include_str!("../../src/backend/control/launch.rs"),
            ),
            (
                "control/fork_write.rs",
                include_str!("../../src/backend/control/fork_write.rs"),
            ),
            (
                "control/fork_launch.rs",
                include_str!("../../src/backend/control/fork_launch.rs"),
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
                "observe/query/locate.rs",
                include_str!("../../src/backend/observe/query/locate.rs"),
            ),
            (
                "observe/recognize/index_row.rs",
                include_str!("../../src/backend/observe/recognize/index_row.rs"),
            ),
            (
                "history/listing.rs",
                include_str!("../../src/backend/history/listing.rs"),
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
                "history/listing_scan.rs",
                include_str!("../../src/backend/history/listing_scan.rs"),
            ),
            (
                "observe/tasks_query.rs",
                include_str!("../../src/backend/observe/tasks_query.rs"),
            ),
        ];

        let mut found: Vec<(String, String)> = Vec::new();
        for (name, raw) in files {
            let src = guard_core::production_code(raw);
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
    /// # 为什么要分层
    ///
    /// `bad_request` 今天一词两义：协议级（信封 JSON 坏了 ⇒「**客户端代码写错了**，别重试」）
    /// vs 命令级（参数不合适 ⇒ 可能是用户输入）。客户端拿到的只有 `code` 字符串，分不出来。
    /// 命令级的形状错误叫 `bad_args`（全部命令一个码）；本条把这条分层**钉住**。
    ///
    /// 闭集整个进本条，一个例外都没有。
    #[test]
    fn protocol_level_codes_are_never_emitted_from_the_control_layer() {
        // 协议级闭集：协议参考的第 3 节从同一张表生成。**只有 `stream/inbound/` 可以发这些。**
        let protocol_codes: Vec<&str> = crate::protocol_doc_gen::PROTOCOL_CODES
            .iter()
            .map(|(c, _)| *c)
            .collect();
        // 逐个文件扫 `control/`（`observe/` 不产 code，不在本条范围）。
        let files: &[(&str, &str)] = &[
            (
                "control/launch.rs",
                include_str!("../../src/backend/control/launch.rs"),
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
            let prod = guard_core::production_code(raw);
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

        // 反面：注册表里登记的命令级 code 不许与协议级重名。
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
        let root = guard_core::addr::backend_src_root();
        let mut hits = Vec::new();
        let mut control = false;
        for (at, raw) in guard_core::scan_tree!(&root, &["rs"]) {
            let prod = guard_core::production_code(&raw);
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
