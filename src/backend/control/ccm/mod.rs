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
//! # **没有**做到的，逐条写在这里（别读成做到了）
//!
//! - **信任这个目录吗**：ccm 自己不替用户答 —— 不写 `~/.claude.json` / `~/.codex/config.toml`，
//!   也不往会话里按键；用户在终端里直接敲的那一趟照样由 agent 自己问。cc-monitor 经后端起 / 重启会话时，
//!   后端在起之前把工作目录标进要用的那个号（`accounts::manage::trust_share_exec::pretrust`），不在这里。
//! - **`--help` 的正文**：旧实现是 `sed` 自己的注释块；这里是 [`USAGE`] 常量，**文本不同**。

pub(crate) mod on_path;
pub(crate) mod plan;

use crate::ccm_line::{argv, is_backend_word, is_own_name, PROFILE_SIGIL, SUBCOMMAND_WORD};
use crate::platform::child::Child;
use argv::{Die, Early, Parsed};
use copy_core::copy_text;
use plan::{AccountTable, Env, Plan};

/// 这套 CLI 的版本号（`--ccm-probe` 的 `version=` · 帧 `ccm-probe` 的 `version`）。**行为变了就要动它**。
///
/// `4` 是最后一版 bash 实现；`5` 起是**后端的原生命令**（本模块）；`6` 起是 claude 的壳（
/// 位置动作取消、不认的词原样交 agent、诊断口改 `--ccm-*`）；`7` 起认 `@<名>` 与按被叫成的名字取配置文件里那一段。
pub(crate) const CCM_VERSION: &str = "7";

/// 认得的 agent。**闭集只有一处住址**：注册表里带起会话事实的那几家
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
/// 而那一处**真改过、也报过 PM**，就是没落进表里。PM 的刀 `P` 逮到它。〕
///
/// - `tests/frontend/shell/plugin_class_registry.rs` —— 数**个数**（那条断言里逐字写着
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
///   〔这句话是实测的，不是推的：加了下面那个 token 之后 `BACKEND_CAPABILITIES` 逐字不变。〕
///
/// ⇒ 归一句：**「数个数」的两处必须跟着改（前两行）· 「子集检查」的两处加 token 安全，
/// 删 / 改名才危险 · `build.rs` 那一处与本常量无关。**
///
/// # `base-url-across-tmux` 是怎么来的
///
/// 它声明的是「**我会把那一家的地址变量（`ANTHROPIC_BASE_URL` 那一类）带过 tmux 的进程边界**」——
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

/// 分流：`argv0` 是这个进程被叫成的名字，`args` 是其后那一串。
/// - 被叫成一段配置的名字（[`profile_called`]）⇒ ccm，前面补上 `@<名>`（参数一个都不进后端）；
/// - 打头的 `--` 紧跟后端词（一次性子命令 / 流模式旗标）⇒ 后端（argv = `--` 之后那一串；
///   第一个 `--` 就是分隔，后端子命令自己的参数里再出现 `--` 也不会被误切）；
/// - 其余一律当 ccm（`argv::parse` 切两半，没有 `--` 整行交 claude）。零参数是「起一个 claude」。
///
/// 🔴 **它必须排在 `split_stream_flags` 之前**：那一步会把 `--stream` / `--tz` 从 argv 里**任意位置**剥掉，
/// 而 `ccm --tz x` 里那个是要原样交给 agent 的。
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

/// 这一趟的意图：`@<名>` 打头 ⇒ 配置文件里那一段合并上命令行当场给的（`book` 只在这一形才读）；否则照命令行（[`argv::parse_layered`] 不带配置层）。
/// `@` 后面那段当不了配置名（`@README.md 讲讲`）⇒ 不是配置，整行照旧交 agent。
/// 一并交回每一项来自哪一层（命令行那一层名字是空串；`--ccm-print` 的「来自」照它写）。
pub(crate) fn parse_entry(
    args: &[String],
    book: impl FnOnce() -> Result<crate::assets::aliases::profile::Book, String>,
) -> Result<(Parsed, Vec<argv::Origin>), Die> {
    use crate::assets::aliases::profile;
    let named = args
        .first()
        .and_then(|a| a.strip_prefix(PROFILE_SIGIL))
        .filter(|n| profile::name_ok(n).is_ok());
    match named {
        Some(name) => {
            let b = book().map_err(Die)?;
            profile::resolve(&b, name, &args[1..])
                .map(|r| (r.parsed, r.origins))
                .map_err(Die)
        }
        None => argv::parse_layered(&[], args),
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
pub fn run(
    args: &[String],
    process_argv: &[String],
    own: &crate::platform::fs::Own,
    running: RunningScan,
) -> i32 {
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
    let (parsed, origins) =
        match parse_entry(args, || crate::assets::aliases::profile::read_at(&env.home)) {
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
            // 这一家认不认得这份扫描由计划那一侧问适配层（`Adapter::live`）。
            env.running_sessions = o.resumes.is_some().then_some(running);
            let Planned { plan, env, table } = match plan_of(&o, env, true) {
                Ok(p) => p,
                Err(Die(msg)) => return die(&msg),
            };
            if o.print {
                // 那条命令进 stdout（照原样能交给 shell）；「来自」那一节进 stderr，每行是注释（混进去一起交给 shell 也不改那条命令）。
                println!("{}", plan::render_for(&plan, plan::terminal_shell()));
                eprint!(
                    "{}",
                    plan::render_from(&plan::from_rows(&o, &origins, &env, &table, &plan))
                );
                return 0;
            }
            execute(plan, own)
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
fn plan_of(o: &argv::Opts, mut env: Env, inherit_account: bool) -> Result<Planned, Die> {
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
    let plan = plan::build(o, &env, &table, taken.as_ref())?;
    Ok(Planned { plan, env, table })
}

/// [`plan_of`] 的成品：计划 ＋ 算它用的那份环境与账号表（`--ccm-print` 的「来自」按同一份写）。
struct Planned {
    plan: Plan,
    env: Env,
    table: AccountTable,
}

/// 一条别名的预置参数最多几个词、每个词最长多少（帧面入参的上界；别名表单产出的远小于它）。
const PRINT_MAX_WORDS: usize = 64;
const PRINT_MAX_WORD_BYTES: usize = 4096;

/// `ccm-probe` 的应答：这台的 `ccm` 会哪些（与名片 `ccm --ccm-probe` 同一组常量）＋ 终端里敲 `ccm` 走到哪。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct CcmCard {
    /// CLI 契约版本（`CCM_VERSION`）。
    pub(crate) version: &'static str,
    /// 这台 `ccm` 认得的能力（与名片的 `capabilities=` 行同一份）。
    pub(crate) capabilities: Vec<&'static str>,
    /// 认得的 agent 种类。
    pub(crate) agents: Vec<&'static str>,
    /// 这一份的 `BUILD_ID`。
    pub(crate) build: &'static str,
    /// 终端里敲 `ccm` 走到的是不是这一份，与那一句。
    #[serde(rename = "onPath")]
    pub(crate) on_path: on_path::OnPath,
}

/// 帧命令 `ccm-probe` 的成品：与 `ccm --ccm-probe` 同一张名片（同一组常量，[`probe_output_for`]）＋ 终端里敲 `ccm` 走到哪那一格
/// （[`on_path::here`]）。`ccm` 就是这台后端本身，「它会哪些」退化成问它自己。平台档与那一格是入参。
pub(crate) fn answer_probe_for(
    platform: crate::platform::tmux::TmuxPlatform,
    on_path: on_path::OnPath,
) -> CcmCard {
    CcmCard {
        version: CCM_VERSION,
        capabilities: crate::ccm_launcher_with(platform),
        agents: agents(),
        build: crate::BUILD_ID,
        on_path,
    }
}

crate::wire::typed::codes! {
    /// `ccm-probe` 没有别的码。
    pub(crate) enum ProbeErr {}
}

/// `ccm-probe`：这台的 `ccm` 会哪些 ＋ 终端里敲它走到哪（起一次登录 shell 问 `PATH`，在 `spawn_blocking` 里等）。
pub(crate) struct CcmProbe;

impl crate::wire::typed::Command for CcmProbe {
    const NAME: &'static str = "ccm-probe";
    const TAKES_INPUT: bool = false;
    type Req = crate::wire::typed::NoArgs;
    type Reply = CcmCard;
    type Err = ProbeErr;
}

impl crate::wire::typed::Handle for CcmProbe {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: crate::wire::typed::NoArgs,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<CcmCard, ProbeErr>> + Send + 'static {
        async move {
            let here = tokio::task::spawn_blocking(on_path::here)
                .await
                .unwrap_or_else(|_| on_path::on_path(&on_path::Reach::Unknown, ""));
            Ok(answer_probe_for(crate::platform::tmux::TMUX_PLATFORM, here))
        }
    }
}

/// `ccm-print` 的入参：一条别名的预置参数（原样 ccm argv，`<交给 agent 的…> -- <ccm 的…>` 那一形）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct CcmPrintReq {
    /// 最多 64 个、每个最长 4096 字节。
    pub(crate) args: Vec<String>,
}

/// `ccm-print` 的应答：`ccm --ccm-print` 那一行。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct CcmLine {
    pub(crate) line: String,
}

crate::wire::typed::codes! {
    /// `ccm-print` 的码：ccm 自己拒了这组参数（原话带回）· 给的是 `--help` 这类不起会话的那一形。
    pub(crate) enum PrintErr {
        Refused = "refused",
    }
}

/// `ccm-print`：一条别名实际会执行什么。只读（读账号库 manifest ＋ 问会话快照）⇒ 阻塞档。
pub(crate) struct CcmPrint;

impl crate::wire::typed::Command for CcmPrint {
    const NAME: &'static str = "ccm-print";
    type Req = CcmPrintReq;
    type Reply = CcmLine;
    type Err = PrintErr;
}

impl crate::wire::typed::Handle for CcmPrint {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: CcmPrintReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<CcmLine, PrintErr>> + Send + 'static {
        async move { answer_print(&req).map_err(crate::wire::typed::of_pair(PrintErr::Refused)) }
    }
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
pub(crate) fn answer_print(req: &CcmPrintReq) -> Result<CcmLine, (&'static str, String)> {
    // 前两种拒是**契约错**（调用方是 monitor，用户手敲不出来）⇒ 英文诊断经 `contract::malformed`；
    // 「不起会话的那一形」是用户清单里真能出现的（手写进别名文件的 `--help`）⇒ 进表说人话。
    let words = &req.args;
    if words.len() > PRINT_MAX_WORDS {
        return Err((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "too many args: {} (max {PRINT_MAX_WORDS})",
                words.len()
            )),
        ));
    }
    if let Some(w) = words.iter().find(|w| w.len() > PRINT_MAX_WORD_BYTES) {
        return Err((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "an arg is too long: {} bytes (max {PRINT_MAX_WORD_BYTES})",
                w.len()
            )),
        ));
    }
    let argv = words.clone();
    let o = match parse_entry(&argv, || {
        crate::assets::aliases::profile::read_at(&Env::for_preview().home)
    }) {
        Ok((Parsed::Opts(o), _)) => o,
        Ok((Parsed::Early(_), _)) => {
            return Err((
                "refused",
                copy_core::copy_text("beCcm.preview.noSession", &[]),
            ))
        }
        Err(Die(msg)) => return Err(("refused", msg)),
    };
    let plan = plan_of(&o, Env::for_preview(), false)
        .map_err(|Die(msg)| ("refused", msg))?
        .plan;
    Ok(CcmLine {
        line: plan::render_for(&plan, plan::terminal_shell()),
    })
}

/// 设置窗合并预览的「等于」那一行：合好的意图 ＋「假设在这个目录敲」（`at`：`~` 打头按这台家目录展开；空 ⇒ 家目录）。
/// 与 [`answer_print`] 同一个 [`plan_of`] ＋ `plan::render_for`，环境同 [`Env::for_preview`]，只换了敲的那个目录。
pub(crate) fn preview_resolved(
    p: Parsed,
    home: &str,
    at: Option<&str>,
) -> Result<(String, String), String> {
    let Parsed::Opts(o) = p else {
        return Err(copy_text("beCcm.preview.noSession", &[]));
    };
    let mut env = Env::for_preview_in(home.to_string());
    if let Some(a) = at.map(str::trim).filter(|a| !a.is_empty()) {
        env.pwd = plan::expand_home(a, &env.home);
    }
    let home = env.home.clone();
    let plan = plan_of(&o, env, false).map_err(|Die(m)| m)?.plan;
    Ok((
        plan::render_for(&plan, plan::terminal_shell()),
        plan::render_short(&plan, &home),
    ))
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
/// `terminal-name-mint` 的入参：工作目录 · 分叉的源，恰好给一个。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct TerminalNameMintReq {
    /// 工作目录 ⇒ `<项目名>-cc`。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) cwd: Option<String>,
    /// 分叉出来的那一条：源会话的终端名或工作目录 ⇒ `<…>-fork-cc`。
    #[serde(default, rename = "forkOf")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) fork_of: Option<String>,
}

/// `terminal-name-mint` 的应答：铸出来的那个名字。
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Minted {
    /// 铸出来的终端名（这台避让过）。
    pub(crate) name: String,
}

crate::wire::typed::codes! {
    /// `terminal-name-mint` 自己的码。
    pub(crate) enum TerminalNameMintErr {
        /// 问会话快照那一次 `tmux` 过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

/// `terminal-name-mint`：起会话要的终端名（这台派生基名、按它的会话快照避让）。阻塞档：快照问一次就起一次 `tmux`。
pub(crate) struct TerminalNameMint;

impl crate::wire::typed::Command for TerminalNameMint {
    const NAME: &'static str = "terminal-name-mint";
    type Req = TerminalNameMintReq;
    type Reply = Minted;
    type Err = TerminalNameMintErr;
}

impl crate::wire::typed::Handle for TerminalNameMint {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: TerminalNameMintReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Minted, TerminalNameMintErr>> + Send + 'static
    {
        async move {
            let base = match (req.cwd.as_deref(), req.fork_of.as_deref()) {
                (Some(cwd), None) => crate::control::session_batch::NameBase::Cwd(cwd),
                (None, Some(source)) => crate::control::session_batch::NameBase::ForkOf(source),
                _ => {
                    return Err(TerminalNameMintErr::BadArgs(
                        crate::common::contract::malformed("give exactly one of `cwd` / `forkOf`"),
                    ))
                }
            };
            // 快照那一层只回 `no_tmux`（下面那层接住了，不算失败）与 `child_timed_out` 两个码 ⇒ 到这里的只有后者。
            terminal_name_mint_with(base, crate::common::session_snapshot::global()).map_err(
                |(code, said)| {
                    debug_assert_eq!(code, crate::platform::child::TIMED_OUT);
                    TerminalNameMintErr::ChildTimedOut(said)
                },
            )
        }
    }
}

/// 铸名的内核（快照是入参：测试拿脚本化探测器造一份）。快照那一层的失败原样交回（码只有 `child_timed_out`；没 tmux 不算失败）。
pub(crate) fn terminal_name_mint_with(
    base: crate::control::session_batch::NameBase<'_>,
    snap: &crate::common::session_snapshot::SessionSnapshot,
) -> Result<Minted, (&'static str, String)> {
    let base = match base {
        crate::control::session_batch::NameBase::Cwd(cwd) => plan::derive_tmux_name(cwd),
        crate::control::session_batch::NameBase::ForkOf(source) => plan::fork_tmux_base(source),
    };
    let name = match snap.taken_names() {
        Ok(taken) => plan::mint_tmux_name(&base, &taken),
        // 这台没装 tmux：没有会话 ⇒ 没有名字可撞。
        Err(("no_tmux", _)) => base,
        Err(e) => return Err(e),
    };
    Ok(Minted { name })
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

/// `--ccm-probe` 那几行。**首行逐字 `name=ccm`**（认得出这是 ccm 的名片；`cc-spawn` 的能力协商读 `capabilities=` 那一行）。
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
/// `detach` / `tmux-size` / `tmux-base` / `bus-register` …，而那几条在 Windows 上
/// 一条都做不到（`TARGET_GAPS`）。⇒ 改吐本二进制那一档平台上的那一份：
/// [`crate::ccm_launcher_with`]`(`[`crate::platform::tmux::TMUX_PLATFORM`]`)` —— 与能力账按 target 问的是**同一个函数**
/// （「能力清单从实现派生」），不在这里另写名单。Linux 上逐字不变。
pub(crate) fn probe_output(self_path: &str) -> String {
    probe_output_for(self_path, crate::platform::tmux::TMUX_PLATFORM)
}

/// [`probe_output`] 的内核：平台档是入参 ⇒ 本机是 Linux 也能把「Windows 那一份会吐什么」算出来判。
pub(crate) fn probe_output_for(
    self_path: &str,
    platform: crate::platform::tmux::TmuxPlatform,
) -> String {
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
fn execute(plan: Plan, own: &crate::platform::fs::Own) -> i32 {
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
            exec_shell(&plan::render(&plan), &WHY_SHELL_ATTACH, Note::NONE, own)
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
            let req = match crate::control::launch::parse_request(launch_args(c)) {
                Ok(r) => r,
                Err(f) => return die(&format!("{}: {}", f.code, f.message)),
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
                Err(f) => {
                    // 命令行：那一句 ＋ tmux 原话一起上 stderr（终端里没有复制详情）。
                    let msg = crate::common::said::Said::from(f.fail.clone()).logged();
                    let after = after_launch_fail(&f, c.detach);
                    // 明确没送进 ⇒ 先收掉本趟建的那个只剩 shell 的会话，再报（不留空会话）。
                    let reaped = (after == AfterFail::ReapThenReport)
                        .then(|| crate::control::launch::reap(&c.name));
                    eprintln!(
                        "{}",
                        copy_text(
                            "beCcm.execute.launchFailed",
                            &[("kind", &f.code), ("message", &msg)]
                        )
                    );
                    match reaped {
                        Some(true) => eprintln!(
                            "{}",
                            copy_text("beCcm.execute.bareReaped", &[("name", &c.name)])
                        ),
                        Some(false) => eprintln!(
                            "{}",
                            copy_text("beCcm.execute.bareKept", &[("name", &c.name)])
                        ),
                        None => {}
                    }
                    // 送达未知、不带 `--detach` ⇒ 往下走收尾那一段（自检 → 接进会话），让人直接看画面。
                    if after != AfterFail::JoinAnyway {
                        return 4;
                    }
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
                exec_shell(
                    &format!(":{tail}"),
                    &WHY_SHELL_CONTAINER_TAIL,
                    Note::NONE,
                    own,
                )
            }
        }
        Plan::Direct(d) => {
            // 不轮换：起之前在轮换账本里给它记好（同 `session-new` 选了「不轮换」那一形）；记不成就不起 ——
            // 起了就可能被「新会话用」那一份换号，正是这一发要避开的。
            if let Some(nr) = &d.no_rotation {
                let route = match crate::agents::pick_route(Some(&crate::agents::AgentKind::new(
                    nr.agent.as_str(),
                ))) {
                    Ok(r) => r,
                    Err(say) => return die(&say),
                };
                let store = crate::accounts::quota::rotation::RotationStore::at(
                    crate::accounts::quota::rotation::path_now(),
                );
                if let Err(e) = crate::accounts::quota::rotation::preset(
                    &store,
                    own,
                    &nr.sid,
                    route,
                    "",
                    crate::accounts::quota::now_unix(),
                ) {
                    return die(&copy_text(
                        "beCcm.execute.noRotationUnwritten",
                        &[("why", &e.logged())],
                    ));
                }
            }
            exec_direct(d, own)
        }
    }
}

/// 容器路建会话 · 送启动命令没成之后怎么办。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfterFail {
    /// 报那一句、退 4。
    Report,
    /// 送达未知（tmux 没应答）、要接进去：先报那一句，照样接进会话 —— 命令可能已经送到，让人直接看画面。
    JoinAnyway,
    /// 明确没送进、会话是本趟建的：收掉那个只剩 shell 的会话，再报那一句、退 4。
    ReapThenReport,
}

fn after_launch_fail(f: &crate::control::launch::LaunchFail, detach: bool) -> AfterFail {
    use crate::control::launch::Delivery;
    match f.untyped {
        Some((Delivery::Unknown, true)) if !detach => AfterFail::JoinAnyway,
        Some((Delivery::NotDelivered, true)) => AfterFail::ReapThenReport,
        _ => AfterFail::Report,
    }
}

/// 容器路 ⇒ `launch` 那道门认得的那份入参（同帧面那一形 [`crate::control::launch::LaunchReq`]：
/// 少一格编不过，不会再「静默丢掉」哪一件 —— `@ccm_agent` 就这么丢过一次）。
fn launch_args(c: &plan::Container) -> crate::control::launch::LaunchReq {
    let some = |s: &str| (!s.is_empty()).then(|| s.to_string());
    crate::control::launch::LaunchReq {
        mode: crate::control::launch::Mode::CreateOrAttach,
        name: c.name.clone(),
        payload: c.payload.clone(),
        agent: Some(c.agent.clone()),
        cwd: some(&c.cwd),
        ccm_sid: some(&c.ccm_sid),
        width: c.size.as_ref().map(|(w, _)| w.clone()),
        height: c.size.as_ref().map(|(_, h)| h.clone()),
        // ccm 起的会话是用户在终端里起的：不归任何一个前端（「哪个前端的会话」那一维，`gate_rules::CLIENT_TERMINAL`）。
        client: Some(crate::control::gate_rules::CLIENT_TERMINAL.to_string()),
    }
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
fn exec_shell(line: &str, why: &str, account: Note<'_>, own: &crate::platform::fs::Own) -> i32 {
    exec_or_spawn(
        Child::new("sh").arg("-c").arg(line),
        &format!(
            "{NO_SHELL}: {}",
            copy_text("beCcm.execShell.needsSh", &[("why", why)])
        ),
        account,
        own,
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
fn exec_direct(d: &plan::Direct, own: &crate::platform::fs::Own) -> i32 {
    if d.keeps_user_base_url {
        eprintln!("{}", copy_text("beCcm.relay.userBaseUrl", &[]));
    }
    // 用的是账号库里一个说得出名字的号 ⇒ 最终那一跳给 agent 那个进程留张便条（观测侧据它记「这条会话用哪个号起的」）。
    let account =
        Some(d.account_name.as_str()).filter(|n| !n.is_empty() && !d.config_dir.is_empty());
    // 换了启动器那一支：插件目录先写好；写不成 ⇒ 不交那个变量、标挂不上（不让那一家拿到一个空目录）。
    let laid;
    let d = if d.plugin_dirs.is_some() {
        laid = lay_plugin(d, own);
        &laid
    } else {
        d
    };
    let note = Note {
        account,
        plugin: d.plugin_dirs.is_some(),
        tab: d.tab.clone(),
    };
    // 非得要 shell 的那几趟（判定与归因都只住 `needs_shell` 一处）⇒ 整条走 `sh -c`（它最后一句 `exec`，pid 不变）。
    if let Some(why) = needs_shell(d) {
        return exec_shell(&plan::render(&Plan::Direct(d.clone())), why, note, own);
    }
    for k in &d.nested {
        std::env::remove_var(k);
    }
    let cfg_env = &d.account_env;
    if !d.config_dir.is_empty() {
        std::env::set_var(cfg_env, &d.config_dir);
    }
    // 没有账号载体的那一家：`--base` 什么都不做。
    if let Some(off) = d.answer_hook_off {
        std::env::set_var(crate::agents::asks::ANSWER_HOOK_OFF_ENV, off.env_value());
    }
    if let Some((var, value)) = &d.plugin_dirs {
        std::env::set_var(var, value);
    }
    if d.unset_config_dir && !cfg_env.is_empty() {
        std::env::remove_var(cfg_env);
    }
    // 钥匙从这台的钥匙文件读进来、exec 那一刻才插（不进打印出来的命令）。读不到 ⇒ 不起（注进去每一发都被中转拒）。
    //   认地址环境变量的那一家：全权那一把插进那个变量；地址拼进参数的那一家：只许直通那一把插进参数里那个词。
    use crate::accounts::upstream_select::endpoint::{keyed_for_args, keyed_for_env};
    let get = |k: &str| std::env::var(k).ok();
    let mut argv = d.argv.clone();
    match (&d.relay, &d.relay_via) {
        (Some(url), plan::RelayVia::Env(var)) => match keyed_for_env(url, &get) {
            Some(keyed) => std::env::set_var(var, keyed),
            None => return die(&copy_text("beCcm.relay.noKey", &[])),
        },
        (Some(url), plan::RelayVia::Args) => {
            let Some(keyed) = keyed_for_args(url, &get) else {
                return die(&copy_text("beCcm.relay.noKey", &[]));
            };
            for a in argv.iter_mut() {
                if plan::split_at_key(a, url).is_some() {
                    *a = a.replacen(url.as_str(), &keyed, 1);
                }
            }
        }
        (None, plan::RelayVia::Env(var)) if d.clears_inherited_relay => std::env::remove_var(var),
        _ => {}
    }
    if !d.cwd.is_empty() && std::env::set_current_dir(&d.cwd).is_err() {
        return die(&copy_text(
            "beCcm.execDirect.noCwd",
            &[("cwd", &d.cwd.to_string())],
        ));
    }
    let Some((prog, rest)) = argv.split_first() else {
        return die(&copy_text("beCcm.execDirect.noLauncher", &[]));
    };
    let path = std::env::var("PATH").ok();
    let policy = || {
        crate::platform::shell::powershell::execution_policy(
            crate::platform::shell::PsHost::Desktop,
        )
        .effective
    };
    let (program, lead) = match final_program(
        prog,
        cfg!(windows),
        path.as_deref(),
        &|p| p.is_file(),
        &policy,
    ) {
        Ok(v) => v,
        Err(why) => return die(&why),
    };
    exec_or_spawn(
        Child::new(program).args(lead).args(rest),
        &format!("'{prog}'"),
        note,
        own,
    )
}

/// 最后那一跳给那个进程留的便条里写什么：用的号（说得出名字才有）· 答题插件是不是经环境交下去的 · `--tab`。
#[derive(Clone)]
struct Note<'a> {
    account: Option<&'a str>,
    plugin: bool,
    tab: Option<crate::control::launch_account::LaunchTab>,
}

impl Note<'_> {
    /// 不留便条。
    const NONE: Note<'static> = Note {
        account: None,
        plugin: false,
        tab: None,
    };
}

/// 把答题插件那几份写进我们的目录（内容一样就不动）。写不成 ⇒ 说一声，换成不交那个变量、标挂不上的那一份。
fn lay_plugin(d: &plan::Direct, own: &crate::platform::fs::Own) -> plan::Direct {
    match write_plugin(&d.plugin_files, own) {
        Ok(()) => d.clone(),
        Err(e) => {
            eprintln!("ccm: {}", e.logged());
            let mut d = d.clone();
            d.plugin_dirs = None;
            d.answer_hook_off = Some(crate::agents::asks::HookOff::Launcher);
            d
        }
    }
}

fn write_plugin(
    files: &[(&'static str, String)],
    own: &crate::platform::fs::Own,
) -> Result<(), crate::common::said::Said> {
    let failed = |p: &std::path::Path, e: &std::io::Error| {
        crate::common::said::Said::with_raw(
            copy_text(
                "beCcm.plugin.writeFailed",
                &[
                    ("path", &p.display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            e,
        )
    };
    let home = own.home_dir()?;
    let root = own.at_home(relay_route_core::ANSWER_PLUGIN_DIR_REL)?;
    for d in [&home, &root] {
        own.ensure_dir(d).map_err(|e| failed(d.as_path(), &e))?;
    }
    for (rel, body) in files {
        let file = root.join(rel)?;
        let dir = match rel.rsplit_once('/') {
            Some((sub, _)) => root.join(sub)?,
            None => root.clone(),
        };
        own.ensure_dir(&dir)
            .map_err(|e| failed(dir.as_path(), &e))?;
        if crate::platform::fs::Ro::read_to_string(file.as_path())
            .ok()
            .as_deref()
            == Some(body.as_str())
        {
            continue;
        }
        let held = own.lock(&dir)?;
        own.write(&file, &held, body.as_bytes())?;
    }
    Ok(())
}

/// 最后起 agent 那一下起的程序与前导参数。Windows 上裸名字照 `platform::shell::find_session_command` 换成找到的那个文件
/// （`.cmd` / `.bat` / `.ps1` 也起得来 —— 与足迹「装没装」、起新会话框同一个找法，认得出就起得来）；
/// unix（按名字 exec）· 带路径的（用户自己指的）· 找不到 / 判不了的 ⇒ 原样（起不来时那句报错说得出是哪个）。
/// 只有 `.ps1`、执行策略不运行本地脚本（`platform::shell::script_refused_by_policy`，`policy` 只在那一形才问）⇒ `Err(那句话)`，不起。
pub(crate) fn final_program(
    prog: &str,
    windows: bool,
    path_env: Option<&str>,
    is_file: &dyn Fn(&std::path::Path) -> bool,
    policy: &dyn Fn() -> Option<String>,
) -> Result<(std::ffi::OsString, Vec<std::ffi::OsString>), String> {
    use crate::platform::shell::{find_session_command, script_refused_by_policy, SessionCommand};
    if windows && !prog.contains(['/', '\\']) {
        let found = find_session_command(prog, path_env, windows, is_file);
        if let Some(word) = script_refused_by_policy(&found, policy) {
            let file = match &found {
                SessionCommand::Found { file, .. } => file.display().to_string(),
                _ => prog.to_string(),
            };
            return Err(copy_text(
                "beCcm.execDirect.scriptRefused",
                &[("file", &file), ("policy", &word)],
            ));
        }
        if let SessionCommand::Found { program, lead, .. } = found {
            return Ok((program, lead));
        }
    }
    Ok((prog.into(), Vec::new()))
}

/// 最终那一跳（`Child::exec_replace`：POSIX 就地 `exec`，Windows 起它、等它、交回退出码）。
///
/// `subject` = **起不来的是什么**。🔴 它不是装饰（`D7`）：从前这里只吐
/// `ccm: 起不来 —— {e}`，而 `{e}` 在 Windows 上逐字是 `program not found`
/// ⇒ 那句话**指不出是哪个 program**。⇒ 两处调用点各自把主语带进来（`sh -c` 那条还带上「为什么非得经它」）。
///
/// `account` ＝ 这一趟用的号（账号库里说得出名字的那一形）：起好那一刻给那个 pid 留便条；写不成只出声、照常起。
fn exec_or_spawn(
    cmd: Child,
    subject: &str,
    leave: Note<'_>,
    own: &crate::platform::fs::Own,
) -> i32 {
    let note = |pid: u32| {
        let now = crate::common::time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if leave.account.is_none() && !leave.plugin && leave.tab.is_none() {
            return;
        }
        let left = crate::control::launch_account::leave_note(
            own,
            pid,
            leave.account,
            leave.plugin,
            leave.tab.clone(),
            now,
        );
        if let Err(e) = left {
            eprintln!("ccm: {}", e.logged());
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
