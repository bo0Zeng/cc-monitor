//! `ccm` —— **后端的原生终端命令**（`K26` / `K33`）。
//!
//! 〔用@08-28 `K26` 逐字〕「**ccm 应当就是后端的原生命令，跟起会话那些东西一样，
//! 后端不是有原生命令吗？后端的原生命令就是 ccm，然后加各种参数实现其他功能**」。
//! 〔用@09-11 `K33` 逐字〕「后端**只有一个**……**不要有什么 bash 脚本**，
//! **不要有什么单独的 ccm**。**所有命令只许有一处**，其他都是**根据传参来调用**。」
//!
//! # 同一份实现两种模式
//!
//! - **常驻**：`cc-monitor-backend` 起来接流，别人连它（`main.rs` 的流模式）。
//! - **一次性**：在用户终端里认 argv，做完就走 —— **就是本模块**。
//!
//! # 怎么进到这里
//!
//! **只经 [`route`] 这一处**，分流看两样：
//! - **被叫成什么**（`argv[0]` 的名字）：是 ccm 自己（`ccm` / 后端本名，[`is_own_name`]）⇒ 往下看参数；
//!   是别的名字（`~/.cc-monitor/bin/<名>` 那条指向 ccm 的链接）⇒ 等同 `ccm @<名> …`，参数一律归 ccm 这一趟。
//! - **参数**：打头是 `--` 且紧跟一个后端词 ⇒ 后端（流 / 子命令）；其余一律是 ccm 这一趟
//!   （没有 `--` ⇒ 整行交给 claude；打头是 `@<名>` ⇒ 用配置文件里那一段，见 `assets/aliases/profile.rs`）。
//!
//! 实现只有这一处：链接、`@<名>`、直接敲的 `ccm …` 都落到同一个 [`run`]。
//!
//! ⚠ **它不是一条 wire 子命令**，所以**不进 `main::SUBCOMMANDS`**、也不进
//! `src/doc/IPC-PROTOCOL.md` §10：那份文档是 monitor↔backend 的**冻结线上契约**，
//! 而这里是**用户终端**的命令面，两者的读者与兼容性义务都不同。
//! 这个决定不是靠「没人查」成立的 —— `protocol_doc_guard::TERMINAL_SURFACE_FILES`
//! 把它登记成一个受管例外，并**另立一格**（每个旗标都要能在 [`USAGE`] 里找到）。
//!
//! # 本轮**没有**做到的，逐条写在这里（别读成做到了）
//!
//! - **信任这个目录吗**：ccm 不替用户答 —— 不写 `~/.claude.json` / `~/.codex/config.toml`，
//!   也不往会话里按键。`claude` 首次进一个目录会问，由用户在会话里自己答（那是它的安全检查）。
//! - **`--help` 的正文**：旧实现是 `sed` 自己的注释块；这里是 [`USAGE`] 常量，**文本不同**。

pub(crate) mod argv;
pub(crate) mod plan;

use crate::platform::child::Child;
use argv::{Die, Early, Parsed};
use copy_core::copy_text;
use plan::{AccountTable, Env, Plan};

/// 这套 CLI 的版本号。**行为变了就要动它** —— 消费者（`ccm_probe.rs`）靠
/// `version=` 这一行分辨对面是哪一版。
///
/// `4` 是最后一版 bash 实现；`5` 起是**后端的原生命令**（本模块）；`6` 起是 claude 的壳（
/// 位置动作取消、不认的词原样交 agent、诊断口改 `--ccm-*`）；`7` 起认 `@<名>` 与按被叫成的名字取配置文件里那一段。
pub(crate) const CCM_VERSION: &str = "7";

/// 认得的 agent。**闭集只有一处住址**（`brief` 13b）：注册表里带起会话事实的那几家
/// （`agents::launchable_kinds`）。从前这里是一份手写的闭集常量，与注册表那一格是同一件事的两处。
pub(crate) fn agents() -> Vec<&'static str> {
    crate::agents::launchable_kinds()
}

/// 能力 token。消费者（`ccm_invocation.rs::CLI_REQUIRED_CAPS` / 前端）据此判断
/// 「这条命令渲出来对面认不认」。
///
/// # 🔴 加 / 删一个 token 之前：**谁在数它**（`K-R61` 09-11 现打**五处**）
///
/// ⚠ **这张表就是给下一个加 token 的人看的** ⇒ 它漏一行，下一个人就会被那一处打红一次。
/// 〔`K-R61` 09-11 现打过一次：初版这里只写了四处，漏的正是 `plugin/probe.rs` 那一行 ——
/// 而那一处本轮**真改过、也报过 PM**，就是没落进表里。PM 的刀 `P` 逮到它。〕
///
/// - `src/frontend/shell/src/plugin_class_registry.rs` —— 数**个数**（那条断言里逐字写着
///   「这个数变了要顺手看一眼它们」）。加 token ⇒ **那个数要跟着改**，否则当场红。
/// - `src/backend/plugin/probe.rs` 的
///   [`crate::plugin::probe::tests::the_required_list_is_checked_against_what_the_real_plugin_declares`]
///   —— 它拿 [`probe_output`] **真吐出来的那一行** `capabilities=` 当活体语料，再数**个数**。
///   加 token ⇒ **那个数要跟着改**（与上一行同形，是本树内的第二处计数）。
///   〔依据：PM 刀 `P`（09-11）往本常量再加一个 token、别处一字不改，backend 套
///   `682 → 681 passed / 1 failed`，**只红这一条**；monitor 套同刀 `1381 → 1380 / 1`，
///   只红上一行那条。⇒ 两处**各自最小面 1 条**，而它们是仅有的两处「数个数」的。〕
/// - `src/backend/control/launch_render/ccm_invocation.rs` —— `CLI_REQUIRED_CAPS`
///   与判据自带的 `STATIC_CAPS_EXPECTED`，两处都是**子集检查** ⇒ 加 token 安全。
/// - `tests/e2e/ccm-contract-parity.sh` —— 数 `capabilities=` 覆不覆盖 TS 那一份，同样是**⊇**。
/// - `src/frontend/shell/build.rs` 的 `extract_capabilities` —— ⚠ **它盖不到这里**：
///   它按 `const CAPABILITIES` 这一行去 `src/backend/main.rs` 里抠，
///   抠的是 backend **流模式**那个同名常量（`bg` / `tail-only`），与本常量无关。
///   〔这句话是本轮实测的，不是推的：加了下面那个 token 之后 `BACKEND_CAPABILITIES` 逐字不变。〕
///
/// ⇒ 归一句：**「数个数」的两处必须跟着改（前两行）· 「子集检查」的两处加 token 安全，
/// 删 / 改名才危险 · `build.rs` 那一处与本常量无关。**
///
/// # `base-url-across-tmux` 是怎么来的
///
/// 它声明的是「**我会把 `ANTHROPIC_BASE_URL` 带过 tmux 的进程边界**」——
/// tmux server 的 `update-environment` 默认列表不含它，外层那句 `export`
/// 在边界上会被整个吃掉（`plan.rs` 那段注释逐字「账号注入 100% 失效，**实测过**」）。
/// 带过去的只有用户自己的端点；我们的中转地址属于某个号，pane 里那一趟按目标账号自己问。
///
/// 🔴 **这件事我们早就做到了，只是一直没说**：`plan.rs` 的容器分支把它显式化进载荷内侧。
/// 当时本机起会话那边按「探不到就不放行」照旧挡着，**挡的却是一件我们自己已经做到的事** ——
/// 「实现与申报不一致，而守它的东西看不见那个字段」（那道挡板随起会话只交一行 `ccm …` 删了）。
/// 本 token 补的就是**申报**那一半；驱动它的判据是
/// [`tests::the_base_url_token_is_declared_because_the_tmux_path_really_forwards_it`]。
pub(crate) const CAPABILITIES: &[&str] = &[
    "new",
    "resume",
    "attach",
    "tmux",
    "account",
    "model",
    "cwd",
    "agent",
    "launcher",
    "ccm-sid",
    "print",
    "detach",
    "tmux-size",
    "tmux-base",
    "bus-register",
    "backend-discover",
    "account-via-backend",
    "base-url-across-tmux",
];

/// 上面那张 [`CAPABILITIES`] 里，**载体是 tmux** 的那几条 —— 一条一条的依据：
///
/// ⚠ 这是一条关于**机制**的声明（「它靠什么活着」），不是差异登记：差异 = 本表 × 平台那一维，
/// 由汇总层 `lib.rs::capabilities_on` 现推；理由与档住 `lib.rs::TARGET_GAPS`。两张表回答的不是同一个问题。
///
/// 它从汇总层（`lib.rs`，PR1 落地时的临时住址）搬回这里 ——
/// 一条能力一个住址：往 [`CAPABILITIES`] 加一条靠 tmux 活着的能力的人，就在这张表的正上方。
/// 「每一条都真在 `CAPABILITIES` 里」由 `target_parity_guard.rs` 的
/// `every_tmux_carried_ccm_capability_really_exists` 钉着（不许有幽灵）；
/// 反方向（漏登一条）今天没有判据，照旧记在那份文件头注「买不到」第 5 条。
pub(crate) const CCM_TMUX_CARRIED: &[&str] = &[
    "attach",               // 实现就是 `tmux attach`
    "base-url-across-tmux", // 名字就是「跨 tmux 的边界」
    "bus-register",         // `argv.rs`：要 `--detach`，而 `--detach` 要 `--tmux`
    "ccm-sid",              // 写的是 tmux 会话上的意图标（`@ccm_sid_expect`）；直路上没有可写的地方
    "detach",               // 实现就是 `tmux detach`
    "tmux",                 // 它本身
    "tmux-base",            // tmux 的 `base-index`
    "tmux-size",            // tmux 窗格尺寸
];

/// 撞名时那句话的**唯一格式串**。
/// ⚠ 结尾那两个字符是**反斜杠 + n**，不是一个真换行 —— 它是一条 **`printf` 格式串**：
/// 要被原样拼进 `--ccm-print` 吐的那条 shell 里（`printf '<本串>' '<名字>'`），
/// 由**那个 shell 里的 printf** 去解释它。写成真换行的话，`--ccm-print` 吐出来的命令会断成两行。
/// 自己要打这句话时（`execute` 的撞名出口）记得把它译回真换行。
/// 句子住文案表（`beCcm.nameTaken.say`，占位符 `{name}` 在这里填成 printf 的 `%s`）。
pub(crate) static NAME_TAKEN_FMT: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beCcm.nameTaken.say", &[("name", "%s")]) + "\\n");

/// 容器里那条内层命令**自检没过**时的那句话。形状与理由同 [`NAME_TAKEN_FMT`]
/// （`printf` 格式串，结尾是反斜杠 + n）。退出码 `4`（起不来）；它前面一行是自检那一趟**自己的原话**。
///
/// 会话**留着不收**：pane 里有同一句原话，是用户看得见的唯一现场；收会话是破坏性动作，
/// 有它自己的三道门（`§34`），不在这条路上顺手做。
pub(crate) static SELF_CHECK_FAILED_FMT: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beCcm.selfCheckFailed.say", &[("name", "%s")]) + "\\n");

/// codex 的 cc-bus 身份配方。**输出的是配方不是值** —— 这样 `--ccm-print` 仍然不查实时 tmux 状态。
///
/// 〔`INVARIANTS §49`〕`#S` 是**会话名**，用户起的会话名可以是中文 ⇒ 读它的那个 tmux 客户端必须是
/// UTF-8 客户端（非 UTF-8 客户端下非 ASCII 被改写成 `_`，codex 的 cc-bus 身份就错了，且退出码仍是 0）。
/// 这是拼进 pane 里跑的命令串 ⇒ 按 `common/tmux_utf8.rs` 那张表用**旗**（`-u`，排在子命令之前）。
/// `export` / `unset` 的写法住 `platform::shell::posix`（原是一条 `const`，产出逐字节不变）。
pub(crate) static BUS_ID_RECIPE: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        "if [ -n \"${{TMUX:-}}\" ]; then _ccm_bus=\"$(tmux -u display-message -p \"#S\" 2>/dev/null)\"; [ -n \"$_ccm_bus\" ] && {}{}fi;",
        crate::platform::shell::posix::export("CC_BUS_ID", "\"$_ccm_bus\""),
        crate::platform::shell::posix::unset(&["_ccm_bus"]),
    )
});

/// `--help` 的正文。**每个认得的旗标都要在这里有一行** ——
/// 由 `protocol_doc_guard` 那条受管例外的配套判据机检。
/// 正文住文案表（`beCcm.usage.body`）。
pub(crate) static USAGE: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beCcm.usage.body", &[]));

// 按哪一家起会有什么不同（默认启动器 · 嵌套标记 · cc-bus 身份 · 身份面 · pidfile）全是适配层那一格
// （`agents::LaunchFace`），`plan::build_among` 一处问完；本模块不认识任何一家的名字。

/// 入口注入的「此刻在跑的会话」扫描：入参 = 账号配置目录（`None` = agent 默认家目录），出 `(sid, pid)`。
pub type RunningScan = fn(Option<&std::path::Path>) -> Vec<(String, u32)>;

/// 本文件自己的源码，给别的护栏对账用。
///
/// ⚠ 刻意由本模块**自己**提供，而不是让调用方写 `include_str!("control/ccm/mod.rs")`：
/// 那种多段相对路径 `cross_half_edge_registry` 的抽取器解析不动，
/// 而它**跳过的时候是静默的** —— 一条跨界边就这么从扫描面里消失。
#[cfg(test)]
pub(crate) fn own_source() -> &'static str {
    include_str!("mod.rs")
}

/// 这个二进制在终端里的名字（`~/.cc-monitor/bin/ccm`）。预览语境里「叫的是 `ccm`」就是它（`plan::Env::for_preview`）。
pub(crate) const SUBCOMMAND_WORD: &str = "ccm";

/// 用配置文件里那一段的写法：`@<名>` 打头（[`PROFILE_SIGIL`] ＋ 段名）。
pub(crate) const PROFILE_SIGIL: char = '@';

/// 这个名字是不是 ccm 自己（`ccm` · 后端本名 `cc-monitor-backend` 及旧版释放的 `cc-monitor-backend-<build>`）。
/// 不是 ⇒ 被叫成这个名字等同 `@<名>`（[`route`]）；配置文件也不收这几个名字当段名。
pub(crate) fn is_own_name(name: &str) -> bool {
    name == SUBCOMMAND_WORD || name.starts_with(env!("CARGO_PKG_NAME"))
}

/// 被叫成的那个名字若是一段配置的名字 ⇒ 那个名字（`argv[0]` 取最后一段；Windows 上去掉 `.exe`）。
/// 是 ccm 自己、或根本当不了配置名（形状不对）⇒ `None`，照 ccm 本身走。
pub(crate) fn profile_called(argv0: &str) -> Option<String> {
    let base = argv0.rsplit(['/', '\\']).next().unwrap_or(argv0);
    let base = base
        .len()
        .checked_sub(4)
        .filter(|&k| base.is_char_boundary(k) && base[k..].eq_ignore_ascii_case(".exe"))
        .map_or(base, |k| &base[..k]);
    (!is_own_name(base) && crate::assets::aliases::profile::name_ok(base).is_ok())
        .then(|| base.to_string())
}

/// 这一趟交给谁：ccm（壳）还是后端 —— **唯一的分流口**（`main.rs` 只认它）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// 当 `ccm` 用：交给 [`run`] 的那串 argv（`[@<名>] [交给 claude 的…] -- [ccm 自己的…]`）。
    Ccm(Vec<String>),
    /// 当后端用：一次性子命令或流模式的 argv（已去掉打头的 `--`）。
    Backend(Vec<String>),
}

/// 后端认得的第一个词：一次性子命令 ∪ 流模式旗标。只许紧跟**打头的** `--` 出现（`ccm -- --stream …`）。
pub(crate) fn is_backend_word(w: &str) -> bool {
    crate::SUBCOMMANDS.contains(&w) || crate::STREAM_FLAGS.contains(&w)
}

/// 分流：`argv0` 是这个进程被叫成的名字，`args` 是其后那一串。
/// - 被叫成一段配置的名字（[`profile_called`]）⇒ ccm，前面补上 `@<名>`（参数一个都不进后端）；
/// - 打头的 `--` 紧跟后端词（一次性子命令 / 流模式旗标）⇒ 后端（argv = `--` 之后那一串；
///   第一个 `--` 就是分隔，后端子命令自己的参数里再出现 `--` 也不会被误切）；
/// - 其余一律当 ccm（`argv::parse` 切两半，没有 `--` 整行交 claude）。零参数是「起一个 claude」。
///
/// 🔴 **它必须排在 `split_stream_flags` 之前**：那一步会把 `--with-bg` / `--tail-only`
/// 从 argv 里**任意位置**剥掉，而 `ccm --tail-only` 里那个是要原样交给 agent 的。
pub fn route(argv0: &str, args: &[String]) -> Entry {
    if let Some(name) = profile_called(argv0) {
        let mut v = vec![format!("{PROFILE_SIGIL}{name}")];
        v.extend(args.iter().cloned());
        return Entry::Ccm(v);
    }
    if args.first().map(String::as_str) == Some(argv::flag::END)
        && args.get(1).is_some_and(|w| is_backend_word(w))
    {
        return Entry::Backend(args[1..].to_vec());
    }
    Entry::Ccm(args.to_vec())
}

/// [`route`] 的 ccm 那一支（给只关心「是不是在当 ccm 用」的调用方）。
pub fn intercept(argv0: &str, args: &[String]) -> Option<Vec<String>> {
    match route(argv0, args) {
        Entry::Ccm(v) => Some(v),
        Entry::Backend(_) => None,
    }
}

/// 「我是被怎么叫进 `ccm` 模式的」—— 容器路要在 pane 里**把自己再叫一次**，叫法就是这一段 ＋ 内层参数（`plan::build` 那条 `inner`）。
/// 恒是 `[argv0]`；被叫成一段配置的名字时换成同一目录下的 `ccm`（内层参数已经是合并好的完整选项，再叫一次那个名字会把配置叠两遍）。
pub(crate) fn self_invocation(argv: &[String]) -> Vec<String> {
    argv.first()
        .map(|a0| match profile_called(a0) {
            Some(name) => format!("{}{SUBCOMMAND_WORD}", &a0[..a0.len() - name.len()]),
            None => a0.clone(),
        })
        .into_iter()
        .collect()
}

/// 这一趟的意图：`@<名>` 打头 ⇒ 配置文件里那一段合并上命令行当场给的（`book` 只在这一形才读）；否则照 [`argv::parse`]。
/// `@` 后面那段当不了配置名（`@README.md 讲讲`）⇒ 不是配置，整行照旧交 agent。
pub(crate) fn parse_entry(
    args: &[String],
    book: impl FnOnce() -> Result<crate::assets::aliases::profile::Book, String>,
) -> Result<Parsed, Die> {
    use crate::assets::aliases::profile;
    let named = args
        .first()
        .and_then(|a| a.strip_prefix(PROFILE_SIGIL))
        .filter(|n| profile::name_ok(n).is_ok());
    match named {
        Some(name) => {
            let b = book().map_err(Die)?;
            profile::resolve(&b, name, &args[1..])
                .map(|r| r.parsed)
                .map_err(Die)
        }
        None => argv::parse(args),
    }
}

/// 没有终端可交互、又没有 `--` ⇒ 不起 claude 时的退出码（与下面 [`run`] 那四档分开）。
pub const EXIT_NO_TERMINAL: i32 = 5;

/// [`route`] 落进「起一个 claude」那一支之后的第一道判断（纯函数）：没有 `--`（整行要交给 claude）、
/// stdin 与 stdout 都不是终端 ⇒ 不起。被别的程序经管道驱动的裸调用多半是要叫后端却漏了 `--`，
/// 起一个没人能交互的 claude 只会白花钱。有 `--` 的（`--ccm-print` · `--ccm-tmux --detach` · 诊断口）不看终端。
pub(crate) fn refuses_without_terminal(args: &[String], stdin_tty: bool, stdout_tty: bool) -> bool {
    argv::last_end(args).is_none() && !stdin_tty && !stdout_tty
}

/// 一次性模式的入口。返回**退出码**。
///
/// 退出码的五档（前四档与旧实现逐字同义，消费者按码分支）：
/// `0` 正常 · `2` 用法错（`die`）· `3` 会话名被占 · `4` 起不来 · `5` 没有终端又没有 `--`（[`EXIT_NO_TERMINAL`]）。
/// `process_argv` ＝ 这个进程的整条 argv，由调用方（`main.rs`，argv 只在那里取一次）交；
/// 本模块不自己读 `std::env::args`。
pub fn run(args: &[String], process_argv: &[String], running: RunningScan) -> i32 {
    use std::io::IsTerminal;
    if refuses_without_terminal(
        args,
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
    ) {
        eprintln!("{}", copy_text("beCcm.noTerminal.say", &[]));
        return EXIT_NO_TERMINAL;
    }
    let env = Env::from_process(process_argv);
    let parsed = match parse_entry(args, || crate::assets::aliases::profile::read_at(&env.home)) {
        Ok(p) => p,
        Err(Die(msg)) => return die(&msg),
    };
    match parsed {
        Parsed::Early(Early::Version) => {
            println!("ccm {CCM_VERSION}");
            0
        }
        Parsed::Early(Early::Help) => {
            print!("{}", *USAGE);
            0
        }
        Parsed::Early(Early::Probe) => {
            // `self=` 答的是「怎么叫我」—— 按 `self_invocation` 原样报（整段 join，不私自只取 argv0）。
            let me = env
                .self_argv
                .iter()
                .map(|a| plan::qarg(a))
                .collect::<Vec<_>>()
                .join(" ");
            print!("{}", probe_output(&me));
            0
        }
        Parsed::Opts(o) => {
            let mut env = env;
            // 这一家认不认得这份扫描由计划那一侧问适配层（`LaunchFace::has_pidfiles`）。
            env.running_sessions = o.resumes.is_some().then_some(running);
            let plan = match plan_of(&o, env, true) {
                Ok(p) => p,
                Err(Die(msg)) => return die(&msg),
            };
            if o.print {
                println!("{}", plan::render_for(&plan, plan::terminal_shell()));
                return 0;
            }
            execute(plan)
        }
    }
}

/// 账号载体与找上游的那两个变量名只有**知道是哪一家**才问得出来 ⇒ 解析完 argv 再补这四格（名字问注册表那一家的格）。
/// `inherit` ＝ 读继承值的那只手（`None` ＝ 不继承：别名预览）；没有这一维的家 ⇒ 名字空串、继承值不读。
fn agent_env(agent: &str, env: &mut Env, inherit: Option<&dyn Fn(&str) -> Option<String>>) {
    env.account_env = crate::agents::account_env_of(agent)
        .unwrap_or_default()
        .to_string();
    env.base_url_env = crate::agents::base_url_env_of(agent)
        .unwrap_or_default()
        .to_string();
    let read = |k: &str| {
        inherit
            .filter(|_| !k.is_empty())
            .and_then(|g| g(k))
            .filter(|v| !v.is_empty())
    };
    env.inherited_config_dir = read(&env.account_env);
    env.inherited_base_url = read(&env.base_url_env);
}

/// 解析好的 argv ＋ 一份环境 ⇒ 那一条计划。**真跑、`--ccm-print`、别名预览（[`answer_print`]）都从这里拿计划**，
/// 三条路结构上不可能各算各的。
///
/// `inherit_account`：要不要读**这个进程**环境里继承来的账号目录变量。一次性模式（用户终端里敲的）要；
/// 别名预览不要 —— 那一问答的是「从一个新终端敲这条别名」，常驻后端进程身上的变量不是那个终端的。
fn plan_of(o: &argv::Opts, mut env: Env, inherit_account: bool) -> Result<Plan, Die> {
    let get = |k: &str| std::env::var(k).ok();
    agent_env(
        &o.agent,
        &mut env,
        inherit_account.then_some(&get as &dyn Fn(&str) -> Option<String>),
    );
    let table = if needs_account_table(o, &env) {
        AccountTable::load(&env.accts_manifest)
    } else {
        AccountTable::default()
    };
    // ★★ `K-R96`（09-12）：**铸名避让在这里就问那张唯一的会话快照**。
    //
    // 用户 `R52` 裁定二逐字：「不就是先校验冲突然后取名吗? **搞个 hash 表**不就好了」。
    // 那张表就是 `common::session_snapshot`（`TakenNames` 的字段模块私有 ⇒
    // 本文件造不出第二份）。⇒ `--ccm-print` 与真跑从此**吐同一个名字**，
    // 而「纯」的口径改成**相对于快照**（`§0c`）。
    //
    // 问不到就 `None` ⇒ **不退让**（诚实降级，见 `plan::build` 头注）。
    // ⚠ 探测点仍然只有一个（快照那处），`readonly_guard::spawn_registry` 的数不变。
    let taken = crate::common::session_snapshot::global().taken_names().ok();
    plan::build(o, &env, &table, taken.as_ref())
}

/// 一条别名的预置参数最多几个词、每个词最长多少（帧面入参的上界；别名表单产出的远小于它）。
const PRINT_MAX_WORDS: usize = 64;
const PRINT_MAX_WORD_BYTES: usize = 4096;

/// 帧命令 `ccm-probe`：**这台的 `ccm` 会哪些** —— 与 `ccm --ccm-probe` 同一张名片（同一组常量）。
/// 为什么问后端而不是进交互 shell 查 `PATH`：`ccm` 就是这台后端本身、恒在 `~/.cc-monitor/bin/ccm`，
/// 「它会哪些」这一问退化成问它自己。
/// 应答是**成品** `{version, capabilities, agents, build}`（此前是那几行原文、由 monitor 解析）：
/// 界面经通道直问、按形状收（`src/ccm-probe.ts`，金样 `tests/__fixtures__/ccm-probe.golden.json`），monitor 那一跳删了。
/// 纯函数：不起进程、不碰盘。
pub(crate) fn answer_probe() -> serde_json::Value {
    answer_probe_for(crate::TMUX_PLATFORM)
}

/// [`answer_probe`] 的内核（平台档是入参，与 [`probe_output_for`] 同一组来源）。
pub(crate) fn answer_probe_for(platform: crate::TmuxPlatform) -> serde_json::Value {
    serde_json::json!({
        "version": CCM_VERSION,
        "capabilities": crate::ccm_launcher_with(platform),
        "agents": agents(),
        "build": crate::BUILD_ID,
    })
}

/// 帧命令 `ccm-print`：**一条别名实际会执行什么**（
/// 「`ccm --ccm-print` 不跑、吐出等价的一行 shell ⇒ 生成器旁边显示这条别名实际会执行什么，是真验证，不是前端拼串」）。
///
/// 入：`{ "args": [..] }`（一条别名的预置参数，原样 ccm argv）。出：`{ "line": "<--ccm-print 那一行>" }`。
/// 与 `ccm --ccm-print` 走**同一个** [`plan_of`] ＋ `plan::render`；差别只在环境 —— 用 [`Env::for_preview`]：
/// 「从这台机器家目录里的一个新终端敲这条别名」（叫的是 `ccm` · 不在 tmux 里 · 不继承账号目录），
/// 账号表与会话快照照这台机器的真值（撞名退让因此与真跑同一个名字）。
///
/// 错误码：`bad_args`（入参形状不对 / 超上界）· `refused`（ccm 自己拒了这组参数 —— 原话带回；
/// 或给的是 `--help` 这类不起会话的那一形）。只读：不起进程、不写盘。
pub(crate) fn answer_print(
    args: &serde_json::Value,
) -> Result<serde_json::Value, (&'static str, String)> {
    // 前四种拒是**契约错**（调用方是 monitor，用户手敲不出来）⇒ 英文诊断经 `contract::malformed`；
    // 「不起会话的那一形」是用户清单里真能出现的（手写进别名文件的 `--help`）⇒ 进表说人话。
    let words = args
        .get("args")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            (
                "bad_args",
                crate::common::contract::malformed("missing `args` (an array of strings)"),
            )
        })?;
    if words.len() > PRINT_MAX_WORDS {
        return Err((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "too many args: {} (max {PRINT_MAX_WORDS})",
                words.len()
            )),
        ));
    }
    let mut argv = Vec::with_capacity(words.len());
    for w in words {
        let w = w.as_str().ok_or_else(|| {
            (
                "bad_args",
                crate::common::contract::malformed("every element of `args` must be a string"),
            )
        })?;
        if w.len() > PRINT_MAX_WORD_BYTES {
            return Err((
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "an arg is too long: {} bytes (max {PRINT_MAX_WORD_BYTES})",
                    w.len()
                )),
            ));
        }
        argv.push(w.to_string());
    }
    let o = match parse_entry(&argv, || {
        crate::assets::aliases::profile::read_at(&Env::for_preview().home)
    }) {
        Ok(Parsed::Opts(o)) => o,
        Ok(Parsed::Early(_)) => {
            return Err((
                "refused",
                copy_core::copy_text("beCcm.preview.noSession", &[]),
            ))
        }
        Err(Die(msg)) => return Err(("refused", msg)),
    };
    let plan = plan_of(&o, Env::for_preview(), false).map_err(|Die(msg)| ("refused", msg))?;
    Ok(serde_json::json!({ "line": plan::render_for(&plan, plan::terminal_shell()) }))
}

/// 帧命令 `terminal-name-mint`：**起会话要一个终端名 —— 问这台**（这一版宿主只有 tmux，铸的是 tmux 会话名）。
///
/// 入：`{"cwd": "<目录>"}`（起新会话 / 全新 resume：基名 `<项目名>-cc`）或 `{"forkOf": "<源会话的 tmux 名，或它的 cwd>"}`
/// （分叉：基名 `<…>-fork-cc`），二者恰给一个。出：`{"name": "<最终名>"}`。
/// 避让问的是**这台**那张唯一的会话快照（[`crate::common::session_snapshot`]，问一次更新一次），与 `ccm` 起会话、
/// `--ccm-print` 同一份 [`plan::mint_tmux_name`] ⇒ 界面铸出来的名字与终端里敲 `ccm` 在同一目录铸的**同一个**。
/// 这台没装 tmux ⇒ 一个名字都没占 ⇒ 交基名（起不起得来归起会话那一步说）。
///
/// 码：`bad_args`（两格都没给 / 都给了 / 不是字符串）。前端问不到（链路断 · 那台后端比这一问老）⇒ 不铸名、不起、说清
/// （`src/frontend/ui/terminal-name-mint.ts`：空集铸名就是「不避让」，issue #76 的形状）。
pub(crate) fn answer_terminal_name_mint(
    args: &serde_json::Value,
) -> Result<serde_json::Value, (&'static str, String)> {
    terminal_name_mint_with(args, crate::common::session_snapshot::global())
}

/// [`answer_terminal_name_mint`] 的内核（快照是入参：测试拿脚本化探测器造一份）。
pub(crate) fn terminal_name_mint_with(
    args: &serde_json::Value,
    snap: &crate::common::session_snapshot::SessionSnapshot,
) -> Result<serde_json::Value, (&'static str, String)> {
    let text = |k: &str| args.get(k).and_then(serde_json::Value::as_str);
    let base = match (text("cwd"), text("forkOf")) {
        (Some(cwd), None) => plan::derive_tmux_name(cwd),
        (None, Some(source)) => plan::fork_tmux_base(source),
        _ => {
            return Err((
                "bad_args",
                crate::common::contract::malformed(
                    "give exactly one of `cwd` / `forkOf` (a string)",
                ),
            ))
        }
    };
    let name = match snap.taken_names() {
        Ok(taken) => plan::mint_tmux_name(&base, &taken),
        // 这台没装 tmux：没有会话 ⇒ 没有名字可撞。
        Err(("no_tmux", _)) => base,
        Err(e) => return Err(e),
    };
    Ok(serde_json::json!({ "name": name }))
}

/// `--base` 与「已继承 `CLAUDE_CONFIG_DIR`」这两条路**不需要账号表** ——
/// 读它就是白付一次 IO，还会把「这台机器没有账号库」变成一句多余的话。
fn needs_account_table(o: &argv::Opts, env: &Env) -> bool {
    // 没有账号这一维的那一家不选号。
    if env.account_env.is_empty() {
        return false;
    }
    if !o.account.is_empty() {
        return true;
    }
    if o.use_base || !o.account_dir.is_empty() {
        return false;
    }
    env.inherited_config_dir.is_none()
}

/// `--ccm-probe` 那几行。**首行逐字 `name=ccm`** —— `ccm_probe.rs::parse_probe_output`
/// 拿它当判活依据，改它等于把「装没装」这件事判瞎。
///
/// # 🔴 `K-R70`：`build=` 那一行答的是「**你是哪一份**」，与 `version=` 不是同一个问题
///
/// `version=` 是**这套 CLI 的契约版本**（[`CCM_VERSION`]：`4` 是最后一版 bash，`5` 起是
/// 后端本体）—— 它答「你认得哪些参数」，**答不出**「你是哪一次构建」。
/// 在此之前，「这份后端是谁」只能去读它**旁边**那个 `.build_id` 文本文件；
/// 而 `K-R68` 现打：三个载体的 `.build_id` 全从同一处源码常量抠出来 ⇒ 恒等 ⇒ 零证据。
/// ⇒ 本行把身份接到**这个进程自己**身上：能跑它的人直接问它，不看它旁边任何文件。
/// 〔另一半给「跑不了它的人」——交叉编译出来的 musl 二进制在 Windows 上没法执行 ——
///  那一半是 `crate::CC_MONITOR_BUILD_STAMP`（扫字节）。两半同源于 `crate::BUILD_ID`。〕
///
/// # `capabilities=` 答的是「**在这台机器上**做得到哪几条」
///
/// 从前这一行是整张 [`CAPABILITIES`] 原样吐 —— 真 Win11 上 `ccm.exe --ccm-probe` 自报 `tmux` / `attach` /
/// `detach` / `tmux-size` / `tmux-base` / `bus-register` …（`RT1.md §8` F7），而那几条在 Windows 上
/// 一条都做不到（`TARGET_GAPS`）。⇒ 改吐本二进制那一档平台上的那一份：
/// [`crate::ccm_launcher_with`]`(`[`crate::TMUX_PLATFORM`]`)` —— 与能力账按 target 问的是**同一个函数**
/// （「能力清单从实现派生」），不在这里另写名单。Linux 上逐字不变。
pub(crate) fn probe_output(self_path: &str) -> String {
    probe_output_for(self_path, crate::TMUX_PLATFORM)
}

/// [`probe_output`] 的内核：平台档是入参 ⇒ 本机是 Linux 也能把「Windows 那一份会吐什么」算出来判。
pub(crate) fn probe_output_for(self_path: &str, platform: crate::TmuxPlatform) -> String {
    format!(
        "name=ccm\nversion={CCM_VERSION}\nself={self_path}\ncapabilities={}\nagents={}\nbuild={}\n",
        crate::ccm_launcher_with(platform).join(","),
        agents().join(","),
        crate::BUILD_ID
    )
}

fn die(msg: &str) -> i32 {
    eprintln!("ccm: {msg}");
    2
}

/// 真跑。
fn execute(plan: Plan) -> i32 {
    match &plan {
        // attach / 容器路的收尾都是一条**已经渲好的命令串** ⇒ 交给 `sh -c`。
        // 它与 `--ccm-print` 吐的是**同一个渲染函数的产物**，两条路结构上不可能分叉。
        Plan::Attach { .. } | Plan::Rejoin { .. } => {
            if let Plan::Rejoin { name, sid, .. } = &plan {
                eprintln!(
                    "{}",
                    copy_text("beCcm.execute.rejoin", &[("sid", sid), ("name", name)])
                );
            }
            exec_shell(&plan::render(&plan), &WHY_SHELL_ATTACH, None)
        }
        Plan::Container(c) => {
            // 🔴 **这里从前有一段退让** —— 它只发生在真跑这条路上，
            //    于是 `--ccm-print` 吐的名字与真跑起出来的名字**可以不一样**。
            //    用户 `R52` 裁定二之后退让搬进了 `plan::build`（问同一张快照），
            //    计划里的 `name` 就是最终名 ⇒ **这里一个字都不许再改它**。
            //    往回加 = 「产名」与「避让」又变回两个人干的两件事，
            //    而那正是前端 `mintTmuxName` 那条纪律（F13）在后端这一侧的对侧。
            // 🔴 **走 `parse_request` 这道门，不许自己直接造 `LaunchRequest`。**
            //
            // 那道门上挂着字段校验（`check_field` 拒控制字符 · `check_size` 收窄宽高），
            // 而**校验只长在它身上** —— 直接构造结构体等于绕过去。
            // 那份已删的 bash `ccm` 从前也是把这一坨编成 JSON 发给后端的，走的就是同一道门；
            // 搬进同一个进程之后**别把门丢了**（迁移是强度悄悄下降的经典时机）。
            let req = match crate::control::launch::parse_request(&launch_args(c)) {
                Ok(r) => r,
                Err((code, msg)) => return die(&format!("{code}: {msg}")),
            };
            match crate::control::launch::run(&req) {
                Ok(out) if out.created => {}
                Ok(_) => {
                    // 撞名 ⇒ **响亮失败**，绝不静默接回别人的会话。
                    eprint!(
                        "{}",
                        NAME_TAKEN_FMT
                            .replacen("%s", &c.name, 1)
                            .replace("\\n", "\n")
                    );
                    return 3;
                }
                Err((code, msg)) => {
                    eprintln!(
                        "{}",
                        copy_text(
                            "beCcm.execute.launchFailed",
                            &[("kind", &code), ("message", &msg)]
                        )
                    );
                    return 4;
                }
            }
            // 会话名必须由**建它的人**说出来（`--detach` 的既有契约）。
            if c.detach {
                println!("ccm-session={}", c.name);
            }
            let tail = plan::render_container_tail(c);
            if tail.is_empty() {
                0
            } else {
                // 收尾片段以 ` && ` / `; ` 开头（它在 `--ccm-print` 里是接在建会话那段后面的）
                // ⇒ 单独跑时前面补一个 `:`，**不重写一份**（重写就是第二处住址）。
                exec_shell(&format!(":{tail}"), &WHY_SHELL_CONTAINER_TAIL, None)
            }
        }
        Plan::Direct(d) => exec_direct(d),
    }
}

/// 容器路 ⇒ `launch` 那道门认得的那份 `args`。
///
/// ⚠ **键名与 `launch::parse_request` 取的那几个逐字对应** —— 少一个键不会报错，
/// 只会**静默丢掉**那一件（`@ccm_agent` 就这么丢过一次）。
fn launch_args(c: &plan::Container) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    m.insert("mode".into(), "create-or-attach".into());
    m.insert("name".into(), c.name.clone().into());
    m.insert("payload".into(), c.payload.clone().into());
    m.insert("agent".into(), c.agent.clone().into());
    if !c.cwd.is_empty() {
        m.insert("cwd".into(), c.cwd.clone().into());
    }
    if !c.ccm_sid.is_empty() {
        m.insert("ccm_sid".into(), c.ccm_sid.clone().into());
    }
    if let Some((w, h)) = &c.size {
        m.insert("width".into(), w.clone().into());
        m.insert("height".into(), h.clone().into());
    }
    // ccm 起的会话是用户在终端里起的：不归任何一个前端（「哪个前端的会话」那一维，`gate_rules::CLIENT_TERMINAL`）。
    m.insert(
        "client".into(),
        crate::control::gate_rules::CLIENT_TERMINAL.into(),
    );
    serde_json::Value::Object(m)
}

/// 「这一趟非得经 POSIX shell」的**归因**——四句，一句一处，**都在 [`needs_shell`] /
/// [`execute`] 的判定旁边取用**，不许在别处另写一句
///（「一句只许一处用」由 [`tests::every_reason_for_needing_a_shell_is_declared_once_and_used_once`]
/// 机检，人群从本文件生产段现打）。
///
/// 🔴 它们的用处不是好看：`sh` 不在这台机器上时，这句话就是用户唯一看得到的**原因**
///（`D7`：失败要显式、归因要准确）。从前那条路只吐 `ccm: 起不来 —— program not found`，
/// 于是真机读数**只能靠对比 `claude` 那趟 `EXIT=0` 反推**
/// 「找不到的不是 `hostname`，是 `sh`」—— 错的归因比失败本身更贵。
pub(crate) static WHY_SHELL_CCM_ENV: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beCcm.whyShell.ccmEnv", &[]));
/// 同上：codex 的 cc-bus 身份配方，**而且这一趟真在 tmux 里**。
pub(crate) static WHY_SHELL_BUS_ID: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beCcm.whyShell.busId", &[]));
/// 同上：`attach`。
pub(crate) static WHY_SHELL_ATTACH: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beCcm.whyShell.attach", &[]));
/// 同上：容器路的收尾片段。
pub(crate) static WHY_SHELL_CONTAINER_TAIL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beCcm.whyShell.containerTail", &[]));

/// 🔴 **`P19`：直路这一趟非得经 `sh -c` 吗** —— 这个判定**只有这一处住址**。
///
/// 返回 `Some(why)` 时 `why` 就是报错时要说的那句归因（见上面那几条常量）。
///
/// # 从前这里有一条多余的闸，而它是 Windows 上 `--agent codex` 那句 `program not found`
///
/// 上一版的条件逐字是 `!d.ccm_env.is_empty() || d.bus_id_recipe || resolved.is_some()`
/// —— **`bus_id_recipe` 单独就把整条改走 `sh -c`**。而 `needs_bus_id("codex")` 恒真
/// ⇒ 每一趟 `--agent codex` 都要一个 `sh`，Windows 上没有 ⇒ `EXIT=4 program not found`
///（真机现打过 `agent` 那一行）。
///
/// 🔴 **收窄靠的是一条等价，不是一条近似**（`INVARIANTS §33` 那条「不得近似」）：
/// [`BUS_ID_RECIPE`] **整段**裹在 `if [ -n "${TMUX:-}" ]; then … fi;` 里，
/// 而 [`plan::Direct::inside_tmux`] 就是 `var("TMUX").ok().filter(|v| !v.is_empty()).is_some()`
/// ⇒ 这一格为假时，那个 shell 进来之后**在 `exec` 之前一个字都不做**：
/// 剩下的几件（`export` 账号目录 / `unset` / `cd` / `exec argv`）本函数下面那段
/// 逐条都有原生对应物。⇒ **省掉的是一个什么都不做的中间进程，不是一件功能。**
/// 这条等价由 [`tests::the_bus_id_recipe_is_wholly_guarded_by_tmux_so_skipping_the_shell_is_exact`]
/// 钉着 —— 谁把配方改成「无条件做点什么」，那条当场红。
///
/// # 它**没有**买到什么
///
/// 另一条（`CCM_ENV`）**是真的要 shell**，本函数不假装它不要
/// ⇒ 在没有 `sh` 的机器上它们照旧做不到，只是从今天起**说得出口**（`no_shell:`）。
fn needs_shell(d: &plan::Direct) -> Option<&'static str> {
    if !d.ccm_env.is_empty() {
        return Some(WHY_SHELL_CCM_ENV.as_str());
    }
    if d.bus_id_recipe && d.inside_tmux {
        return Some(WHY_SHELL_BUS_ID.as_str());
    }
    None
}

/// 把一条渲好的命令串交给 `sh -c` 并**替换掉自己**。`why` = 这一趟为什么非得经 shell。
///
/// 起进程点，已登记进 `readonly_guard::spawn_registry::ALLOWED`。
fn exec_shell(line: &str, why: &str, account: Option<&str>) -> i32 {
    exec_or_spawn(
        Child::new("sh").arg("-c").arg(line),
        &format!(
            "{NO_SHELL}: {}",
            copy_text("beCcm.execShell.needsSh", &[("why", why)])
        ),
        account,
    )
}

/// 命令级 code —— **「这台机器上没有 POSIX shell」**。
///
/// ⚠ 与 `crate::NO_TMUX` 同形（`ccm: 起不来 —— <code>: <话>`），但**不是**一条 wire code：
/// 它只出现在 `ccm` 这条用户终端命令的 stderr 上，`inbound::REGISTRY` 里没有它、
/// 也不该有 —— 那张表登记的是 monitor↔backend 帧面上的 code，而这里是一次性模式的出口。
pub(crate) const NO_SHELL: &str = "no_shell";

/// 在**本进程**里设好最终环境、`cd`，然后 `exec` 掉自己。
///
/// 🔴 这几步必须发生在**调用者那个进程**里：env 要落在最终 `exec` 的那个 shell 上，
/// 否则穿不过 tmux 的进程边界（旧 `cct` 正是死在这一步）。
fn exec_direct(d: &plan::Direct) -> i32 {
    if d.keeps_user_base_url {
        eprintln!("{}", copy_text("beCcm.relay.userBaseUrl", &[]));
    }
    // 用的是账号库里一个说得出名字的号 ⇒ 最终那一跳给 agent 那个进程留张便条（观测侧据它记「这条会话用哪个号起的」）。
    let account =
        Some(d.account_name.as_str()).filter(|n| !n.is_empty() && !d.config_dir.is_empty());
    // 非得要 shell 的那几趟（判定与归因都只住 `needs_shell` 一处）⇒ 整条走 `sh -c`（它最后一句 `exec`，pid 不变）。
    if let Some(why) = needs_shell(d) {
        return exec_shell(&plan::render(&Plan::Direct(d.clone())), why, account);
    }
    for k in &d.nested {
        std::env::remove_var(k);
    }
    let cfg_env = &d.account_env;
    if !d.config_dir.is_empty() {
        std::env::set_var(cfg_env, &d.config_dir);
    }
    // 没有账号载体的那一家：`--base` 什么都不做。
    if d.unset_config_dir && !cfg_env.is_empty() {
        std::env::remove_var(cfg_env);
    }
    if let Some(url) = &d.relay {
        // 钥匙从这台的钥匙文件读进 agent 进程环境（不进 argv、不进打印出来的命令）。读不到 ⇒ 不起（注进去每一发都被中转拒）。
        match crate::accounts::upstream_select::endpoint::keyed_for_exec(url, &|k| {
            std::env::var(k).ok()
        }) {
            Some(keyed) => std::env::set_var(&d.base_url_env, keyed),
            None => return die(&copy_text("beCcm.relay.noKey", &[])),
        }
    } else if d.clears_inherited_relay {
        std::env::remove_var(&d.base_url_env);
    }
    if !d.cwd.is_empty() && std::env::set_current_dir(&d.cwd).is_err() {
        return die(&copy_text(
            "beCcm.execDirect.noCwd",
            &[("cwd", &d.cwd.to_string())],
        ));
    }
    let Some((prog, rest)) = d.argv.split_first() else {
        return die(&copy_text("beCcm.execDirect.noLauncher", &[]));
    };
    exec_or_spawn(Child::new(prog).args(rest), &format!("'{prog}'"), account)
}

/// 最终那一跳（`Child::exec_replace`：POSIX 就地 `exec`，Windows 起它、等它、交回退出码）。
///
/// `subject` = **起不来的是什么**。🔴 它不是装饰（`D7`）：从前这里只吐
/// `ccm: 起不来 —— {e}`，而 `{e}` 在 Windows 上逐字是 `program not found`
/// ⇒ 那句话**指不出是哪个 program**。⇒ 两处调用点各自把主语带进来（`sh -c` 那条还带上「为什么非得经它」）。
///
/// `account` ＝ 这一趟用的号（账号库里说得出名字的那一形）：起好那一刻给那个 pid 留便条；写不成只出声、照常起。
fn exec_or_spawn(cmd: Child, subject: &str, account: Option<&str>) -> i32 {
    let note = |pid: u32| {
        let Some(name) = account else { return };
        let Some(home) = crate::platform::paths::data_home() else {
            return;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if let Err(e) = crate::control::launch_account::leave_note(&home, pid, name, now) {
            eprintln!("ccm: {e}");
        }
    };
    let e = match cmd.exec_replace(&note) {
        Ok(code) => return code,
        Err(e) => e,
    };
    eprintln!(
        "{}",
        copy_text(
            "beCcm.execOrSpawn.failed",
            &[("subject", subject), ("e", &e.to_string())]
        )
    );
    4
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/ccm_tests.rs"]
mod tests;

// 经 ccm 用某个号起一次 ⇒ 那个号记到那条会话名下（假启动器是 POSIX shell 脚本、ccm 在那一形上 exec 掉子进程）。
#[cfg(all(test, unix))]
#[path = "../../../../tests/backend/control/ccm/launch_note_e2e_tests.rs"]
mod launch_note_e2e;
