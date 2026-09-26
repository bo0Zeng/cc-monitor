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
//! 两条，**都只经 [`intercept`] 这一处**：
//! ① `argv[0]` 的 basename 是 `ccm`（别名 / 软链 / 改名拷贝指过来；`src/shared/ccm-aliases.sh`
//!    里 `cc` / `cct` 那几个别名调的就是它）；
//! ② 显式子命令 `cc-monitor-backend ccm <argv…>`（给「二进制没改名」的场合，
//!    也给判据一个不依赖文件名的入口）。
//!
//! ⚠ **它不是一条 wire 子命令**，所以**不进 `main::SUBCOMMANDS`**、也不进
//! `src/doc/IPC-PROTOCOL.md` §10：那份文档是 monitor↔backend 的**冻结线上契约**，
//! 而这里是**用户终端**的命令面，两者的读者与兼容性义务都不同。
//! 这个决定不是靠「没人查」成立的 —— `protocol_doc_guard::TERMINAL_SURFACE_FILES`
//! 把它登记成一个受管例外，并**另立一格**（每个旗标都要能在 [`USAGE`] 里找到）。
//!
//! # 本轮**没有**做到的，逐条写在这里（别读成做到了）
//!
//! - **预信任**（`~/.claude.json` / `~/.codex/config.toml` 那两处写入）：**没搬**。
//!   backend 这个 crate 有一条「进程自身不许写用户既有数据」的红线
//!   （`readonly_guard`，白名单恰好一个模块）⇒ 搬它要先动那条红线，那是另一件活。
//!   后果：`claude` 起来可能弹信任框。`--print` 那条兜底轮询照旧在，捞得回来。
//! - **`$CCM_CONFIG` 是 bash 源文件**：旧实现 `. "$CCM_CONFIG"`（真 source 一段 bash）。
//!   这里只认 `KEY=value` 三个键（见 [`Env::from_process`]），**不是等价**。
//! - **`--help` 的正文**：旧实现是 `sed` 自己的注释块；这里是 [`USAGE`] 常量，**文本不同**。

pub(crate) mod argv;
pub(crate) mod plan;

use argv::{Die, Early, Parsed};
use copy_core::copy_text;
use plan::{AccountTable, Env, Plan};

/// 这套 CLI 的版本号。**行为变了就要动它** —— 消费者（`ccm_probe.rs`）靠
/// `version=` 这一行分辨对面是哪一版。
///
/// `4` 是最后一版 bash 实现；`5` 起是**后端的原生命令**（本模块）。
pub(crate) const CCM_VERSION: &str = "5";

/// 认得的 agent。**闭集只有这一处住址**（`brief` 13b）。
pub(crate) const AGENTS: &[&str] = &["claude", "codex"];

/// 能力 token。消费者（`ccm_invocation.rs::CLI_REQUIRED_CAPS` / 前端）据此判断
/// 「这条命令渲出来对面认不认」。
///
/// # 🔴 加 / 删一个 token 之前：**谁在数它**（`K-R61` 09-11 现打**五处**）
///
/// ⚠ **这张表就是给下一个加 token 的人看的** ⇒ 它漏一行，下一个人就会被那一处打红一次。
/// 〔`K-R61` 09-11 现打过一次：初版这里只写了四处，漏的正是 `plugin/probe.rs` 那一行 ——
/// 而那一处本轮**真改过、也报过 PM**，就是没落进表里。PM 的刀 `P` 逮到它。〕
///
/// - `src/bridge/src/plugin_class_registry.rs` —— 数**个数**（那条断言里逐字写着
///   「这个数变了要顺手看一眼它们」）。加 token ⇒ **那个数要跟着改**，否则当场红。
/// - `src/backend/plugin/probe.rs` 的
///   [`crate::plugin::probe::tests::the_required_list_is_checked_against_what_the_real_plugin_declares`]
///   —— 它拿 [`probe_output`] **真吐出来的那一行** `capabilities=` 当活体语料，再数**个数**。
///   加 token ⇒ **那个数要跟着改**（与上一行同形，是本树内的第二处计数）。
///   〔依据：PM 刀 `P`（09-11）往本常量再加一个 token、别处一字不改，backend 套
///   `682 → 681 passed / 1 failed`，**只红这一条**；monitor 套同刀 `1381 → 1380 / 1`，
///   只红上一行那条。⇒ 两处**各自最小面 1 条**，而它们是仅有的两处「数个数」的。〕
/// - `src/bridge/src/backend/control/ccm_invocation.rs` —— `CLI_REQUIRED_CAPS`
///   与判据自带的 `STATIC_CAPS_EXPECTED`，两处都是**子集检查** ⇒ 加 token 安全。
/// - `tests/e2e/ccm-contract-parity.sh` —— 数 `capabilities=` 覆不覆盖 TS 那一份，同样是**⊇**。
/// - `src/bridge/build.rs` 的 `extract_capabilities` —— ⚠ **它盖不到这里**：
///   它按 `const CAPABILITIES` 这一行去 `src/backend/main.rs` 里抠，
///   抠的是 backend **流模式**那个同名常量（`bg` / `tail-only`），与本常量无关。
///   〔这句话是本轮实测的，不是推的：加了下面那个 token 之后 `BACKEND_CAPABILITIES` 逐字不变。〕
///
/// ⇒ 归一句：**「数个数」的两处必须跟着改（前两行）· 「子集检查」的两处加 token 安全，
/// 删 / 改名才危险 · `build.rs` 那一处与本常量无关。**
///
/// # `base-url-across-tmux` 是怎么来的〔`K-R61` 09-11〕
///
/// 它声明的是「**我会把 `ANTHROPIC_BASE_URL` 带过 tmux 的进程边界**」——
/// tmux server 的 `update-environment` 默认列表不含它，外层那句 `export`
/// 在边界上会被整个吃掉（`plan.rs` 那段注释逐字「账号注入 100% 失效，**实测过**」）。
///
/// 🔴 **这件事我们早就做到了，只是一直没说**：`plan.rs` 的容器分支把它显式化进载荷内侧。
/// 于是 monitor 那边 `history.rs::RELAY_KEEPS_THE_OLD_PATH` 按「探不到就不放行」照旧挡着，
/// **挡的却是一件我们自己已经做到的事** —— 「实现与申报不一致，而守它的东西看不见那个字段」。
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
/// 〔第三波 S3 · 2026-09-24〕它从汇总层（`lib.rs`，PR1 落地时的临时住址）搬回这里 ——
/// 一条能力一个住址：往 [`CAPABILITIES`] 加一条靠 tmux 活着的能力的人，就在这张表的正上方。
/// 「每一条都真在 `CAPABILITIES` 里」由 `target_parity_guard.rs` 的
/// `every_tmux_carried_ccm_capability_really_exists` 钉着（不许有幽灵）；
/// 反方向（漏登一条）今天没有判据，照旧记在那份文件头注「买不到」第 5 条。
pub(crate) const CCM_TMUX_CARRIED: &[&str] = &[
    "attach",               // 实现就是 `tmux attach`
    "base-url-across-tmux", // 名字就是「跨 tmux 的边界」
    "bus-register",         // `argv.rs`：要 `--detach`，而 `--detach` 要 `--tmux`
    "ccm-sid", // 直路上由启动期令牌承载（`plan::DirectIdentity`），而 Windows 上后端读不到别的进程的环境
    "detach",  // 实现就是 `tmux detach`
    "tmux",    // 它本身
    "tmux-base", // tmux 的 `base-index`
    "tmux-size", // tmux 窗格尺寸
];

/// 撞名时那句话的**唯一格式串**。
/// ⚠ 结尾那两个字符是**反斜杠 + n**，不是一个真换行 —— 它是一条 **`printf` 格式串**：
/// 要被原样拼进 `--print` 吐的那条 shell 里（`printf '<本串>' '<名字>'`），
/// 由**那个 shell 里的 printf** 去解释它。写成真换行的话，`--print` 吐出来的命令会断成两行。
/// 自己要打这句话时（`execute` 的撞名出口）记得把它译回真换行。
/// 〔CP2c〕句子住文案表（`beCcm.nameTaken.say`，占位符 `{name}` 在这里填成 printf 的 `%s`）。
pub(crate) static NAME_TAKEN_FMT: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beCcm.nameTaken.say", &[("name", "%s")]) + "\\n");

/// 〔CC1〕容器里那条内层命令**自检没过**时的那句话。形状与理由同 [`NAME_TAKEN_FMT`]
/// （`printf` 格式串，结尾是反斜杠 + n）。退出码 `4`（起不来）；它前面一行是自检那一趟**自己的原话**。
///
/// 会话**留着不收**：pane 里有同一句原话，是用户看得见的唯一现场；收会话是破坏性动作，
/// 有它自己的三道门（`§34`），不在这条路上顺手做。
pub(crate) static SELF_CHECK_FAILED_FMT: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beCcm.selfCheckFailed.say", &[("name", "%s")]) + "\\n");

/// 窗口标题的合成式 —— **让 tmux 自己从 `@ccm_sid` 合成**，与 pane 标题彻底分开。
///
/// monitor 靠扫窗口标题里的 `ccm-rbind-<sid>` 绑定终端窗口（`bind.rs`）。
///
/// 🔴 〔步 8 · `设计/90 §1.2`〕**`rbind` ＝ remote bind（远端终端窗口绑定）**。
/// 这个缩写不自明，`§1.2` 给了两条出路：「改成 `terminal_bind`」或「保留但每处加一句展开」。
/// ⇒ **这一拍走第二条，而且是被迫的**：`ccm-rbind-<sid>`（窗口标题 marker）与
/// `__ccm_rbind`（用户 shell profile 里那个注册原语）**都在 `ccm` 的对外面上** ——
/// 前者是 monitor↔远端 wrapper 之间已经在线的约定，后者已经装在用户机器上。
/// `设计/90 §1.1` 逐字：`ccm` 是「用户在终端里敲的命令名，属于产品对外接口」，**不许改**。
/// ⇒ 改的只有**内部标识符**（本常量 `RBIND_TITLE_FORMAT` → `TERMINAL_BIND_TITLE_FORMAT`）；
/// 线上那两个拼写一个字节没动。
/// 从前这里是 `#T`（窗口标题 = pane 标题），而 **claude 也在往 pane 标题写自己的状态**
/// ⇒ 两者抢同一个位置，真机实测忙碌那个会话的 marker 被冲成「⠐ 理解…」，
/// 点 ↗ 必弹「未绑定窗口」。⚠ 改它之前先读 `e2e` 那条已经删掉的套件在件文件 `§8` 里的登记。
pub(crate) const TERMINAL_BIND_TITLE_FORMAT: &str = "#{?@ccm_sid,ccm-rbind-#{@ccm_sid},#T}";

/// codex 的 cc-bus 身份配方。**输出的是配方不是值** —— 这样 `--print` 仍然不查实时 tmux 状态。
pub(crate) const BUS_ID_RECIPE: &str = "if [ -n \"${TMUX:-}\" ]; then _ccm_bus=\"$(tmux display-message -p \"#S\" 2>/dev/null)\"; [ -n \"$_ccm_bus\" ] && export CC_BUS_ID=\"$_ccm_bus\"; unset _ccm_bus; fi;";

/// `--help` 的正文。**每个认得的旗标都要在这里有一行** ——
/// 由 `protocol_doc_guard` 那条受管例外的配套判据机检。
/// 〔CP2c〕正文住文案表（`beCcm.usage.body`）。
pub(crate) static USAGE: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beCcm.usage.body", &[]));

/// 这个 agent 的默认启动器。
pub(crate) fn default_launcher(agent: &str) -> &'static str {
    match agent {
        "codex" => "codex",
        _ => "claude",
    }
}

/// 这个 agent 的 resume 旗标；`None` = 它不支持 resume。
pub(crate) fn resume_flag(agent: &str) -> Option<&'static str> {
    match agent {
        // 旗标字面量的住址是 `argv::flag`，这里只许引它（`KR48D2`）。
        "claude" => Some(argv::flag::RESUME),
        _ => None,
    }
}

/// 起这个 agent 之前要清掉的嵌套标记。
pub(crate) fn nested_env(agent: &str) -> Vec<String> {
    match agent {
        "claude" => [
            "CLAUDECODE",
            "CLAUDE_CODE_ENTRYPOINT",
            "CLAUDE_CODE_SESSION_ID",
            "CLAUDE_CODE_CHILD_SESSION",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        _ => Vec::new(),
    }
}

/// 这个 agent 够不着 tmux socket、要把会话名经 env 透进去吗。
///
/// # ⚠ 〔`P19` 09-22〕它答的是「**要不要**」，不是「**能不能**」—— 两者别混
///
/// 这一格为真只说明「codex 要一个 `CC_BUS_ID`」。**值从哪来**是另一件事：
/// 来源恒是 tmux 的会话名（[`BUS_ID_RECIPE`] 里那句 `display-message -p "#S"`）
/// ⇒ **没有 tmux 就没有这个值**，而这不是「codex 这一支做不到」，是**载体没了**
///（与 `base-url-across-tmux` 同一形，`lib.rs::TARGET_GAPS` 里 tmux 那一族 6 条已覆盖）。
///
/// 🔴 这两件事混在一起过一次，代价是一年的账：从前 `exec_direct` 拿本函数当
/// 「要不要请一个 `sh` 进来」的闸 ⇒ `--agent codex` **每一趟**都要 `sh`，
/// Windows 上因此 `EXIT=4 program not found`（真机现打住 `真相源/106 §3.3`）。
/// 今天那个闸在 [`needs_shell`]，而且它多问一句「配方**真有事可做**吗」。
pub(crate) fn needs_bus_id(agent: &str) -> bool {
    agent == "codex"
}

/// 这个 agent 有身份面（`@ccm_sid`）吗。
pub(crate) fn has_identity(agent: &str) -> bool {
    agent == "claude"
}

/// 本文件自己的源码，给别的护栏对账用。
///
/// ⚠ 刻意由本模块**自己**提供，而不是让调用方写 `include_str!("control/ccm/mod.rs")`：
/// 那种多段相对路径 `cross_half_edge_registry` 的抽取器解析不动，
/// 而它**跳过的时候是静默的** —— 一条跨界边就这么从扫描面里消失。
#[cfg(test)]
pub(crate) fn own_source() -> &'static str {
    include_str!("mod.rs")
}

/// 显式子命令形（`cc-monitor-backend ccm …`）的那个词。
///
/// ⚠ 刻意**不是** `--ccm`：那样它会长得像一条 wire 子命令，而它不是。
pub(crate) const SUBCOMMAND_WORD: &str = "ccm";

/// 这一趟是不是在当 `ccm` 用？是就返回**要交给 [`run`] 的那串 argv**。
///
/// 🔴 **它必须排在 `split_stream_flags` 之前**：那一步会把 `--with-bg` / `--tail-only`
/// 从 argv 里**任意位置**剥掉，而 `ccm -- --tail-only` 里那个是要原样透传给 agent 的。
pub fn intercept(argv0: &str, args: &[String]) -> Option<Vec<String>> {
    let base = argv0
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(argv0)
        .trim_end_matches(".exe");
    if base == SUBCOMMAND_WORD {
        return Some(args.to_vec());
    }
    if args.first().map(String::as_str) == Some(SUBCOMMAND_WORD) {
        return Some(args[1..].to_vec());
    }
    None
}

/// 「我是被怎么叫进 `ccm` 模式的」—— 进程 argv 里**排在 ccm 参数前面**的那一段。
///
/// 入口① ⇒ `[argv0]`；入口② ⇒ `[argv0, "ccm"]`。容器路要在 pane 里**把自己再叫一次**，
/// 叫法就是这一段 ＋ 内层参数（`plan::build` 那条 `inner`）。
///
/// # 🔴 〔CC1 · BS1b 现打〕这一段从前只取 `argv0`，丢了入口② 的那个子命令词
///
/// ⇒ 经 `cc-monitor-backend ccm …` 起的会话，pane 里逐字是 `cc-monitor-backend --cwd …`
/// ⇒ 被当后端直连口解析，当场「unknown argument: --cwd」，pane 退回空 bash，
/// 而 cc-spawn 照报成功、还登记上了总线（假成功）。
///
/// ⚠ **判「走的是哪个入口」只有 [`intercept`] 一处** —— 本函数不另判一次，只取
/// 「`intercept` 吃掉了 argv 的哪一段」：`argv` 总长减去它交出去的参数个数。
/// 往后 `intercept` 多认一种入口，这里**一个字不用改**就跟着对。
pub(crate) fn self_invocation(argv: &[String]) -> Vec<String> {
    let Some((argv0, rest)) = argv.split_first() else {
        return Vec::new();
    };
    let consumed = match intercept(argv0, rest) {
        Some(args) => argv.len() - args.len(),
        // 不在 ccm 模式（只有单测会这么问）⇒ 只剩 argv0 可说。
        None => 1,
    };
    argv[..consumed].to_vec()
}

/// 一次性模式的入口。返回**退出码**。
///
/// 退出码的四档（与旧实现逐字同义，消费者按码分支）：
/// `0` 正常 · `2` 用法错（`die`）· `3` 会话名被占 · `4` 起不来。
pub fn run(args: &[String]) -> i32 {
    let parsed = match argv::parse(args) {
        Ok(p) => p,
        Err(Die(msg)) => return die(&msg),
    };
    let env = Env::from_process();
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
            // `self=` 答的是「怎么叫我」—— 入口② 下那是两个词，一起报（只报 argv0 就是同一个缺陷的另一张脸）。
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
            // 账号维度的载体名只有**知道是哪一家**才问得出来 ⇒ 解析完 argv 再补这两格。
            let mut env = env;
            env.account_env = crate::agents::account_env_of(&o.agent)
                .unwrap_or_default()
                .to_string();
            env.inherited_config_dir = (!env.account_env.is_empty())
                .then(|| {
                    std::env::var(&env.account_env)
                        .ok()
                        .filter(|v| !v.is_empty())
                })
                .flatten();
            let table = if needs_account_table(&o, &env) {
                AccountTable::load(&env.accts_manifest)
            } else {
                AccountTable::default()
            };
            // ★★ `K-R96`（09-12）：**铸名避让在这里就问那张唯一的会话快照**。
            //
            // 用户 `R52` 裁定二逐字：「不就是先校验冲突然后取名吗? **搞个 hash 表**不就好了」。
            // 那张表就是 `common::session_snapshot`（`TakenNames` 的字段模块私有 ⇒
            // 本文件造不出第二份）。⇒ `--print` 与真跑从此**吐同一个名字**，
            // 而「纯」的口径改成**相对于快照**（`§0c`）。
            //
            // 问不到就 `None` ⇒ **不退让**（诚实降级，见 `plan::build` 头注）。
            // ⚠ 探测点仍然只有一个（快照那处），`readonly_guard::spawn_registry` 的数不变。
            let taken = crate::common::session_snapshot::global().taken_names().ok();
            let plan = match plan::build(&o, &env, &table, taken.as_ref()) {
                Ok(p) => p,
                Err(Die(msg)) => return die(&msg),
            };
            if o.print {
                println!("{}", plan::render(&plan, resolved(&plan).as_deref()));
                return 0;
            }
            execute(plan)
        }
    }
}

/// `--base` 与「已继承 `CLAUDE_CONFIG_DIR`」这两条路**不需要账号表** ——
/// 读它就是白付一次 IO，还会把「这台机器没有账号库」变成一句多余的话。
fn needs_account_table(o: &argv::Opts, env: &Env) -> bool {
    if !o.account.is_empty() {
        return true;
    }
    if o.use_base {
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
/// # 〔WIN1 · RT1 F7〕`capabilities=` 答的是「**在这台机器上**做得到哪几条」
///
/// 从前这一行是整张 [`CAPABILITIES`] 原样吐 —— 真 Win11 上 `ccm.exe --ccm-probe` 自报 `tmux` / `attach` /
/// `detach` / `tmux-size` / `tmux-base` / `bus-register` …（`RT1.md §8` F7），而那几条在 Windows 上
/// 一条都做不到（`TARGET_GAPS`）。⇒ 改吐本二进制那一档平台上的那一份：
/// [`crate::ccm_launcher_with`]`(`[`crate::TMUX_PLATFORM`]`)` —— 与能力账按 target 问的是**同一个函数**
/// （`96 §2`「能力清单从实现派生」），不在这里另写名单。Linux 上逐字不变。
pub(crate) fn probe_output(self_path: &str) -> String {
    probe_output_for(self_path, crate::TMUX_PLATFORM)
}

/// [`probe_output`] 的内核：平台档是入参 ⇒ 本机是 Linux 也能把「Windows 那一份会吐什么」算出来判。
pub(crate) fn probe_output_for(self_path: &str, platform: crate::TmuxPlatform) -> String {
    format!(
        "name=ccm\nversion={CCM_VERSION}\nself={self_path}\ncapabilities={}\nagents={}\nbuild={}\n",
        crate::ccm_launcher_with(platform).join(","),
        AGENTS.join(","),
        crate::BUILD_ID
    )
}

fn die(msg: &str) -> i32 {
    eprintln!("ccm: {msg}");
    2
}

/// `resume` 那一问：这个会话该怎么起。**在同一个进程里答** ——
/// 从前这里要跨一次进程去问后端（`--resolve`），那整段是 bash 与后端说话的税。
fn resolved(plan: &Plan) -> Option<String> {
    let Plan::Direct(d) = plan else { return None };
    let sid = d.resolve_sid.as_deref()?;
    let input = serde_json::json!({ "sessionId": sid }).to_string();
    let v = crate::control::resolve_query::resolve_json_for_inbound(&input).ok()?;
    v.get("command")?.as_str().map(|s| s.to_string())
}

/// 真跑。
fn execute(plan: Plan) -> i32 {
    match &plan {
        // attach / 容器路的收尾都是一条**已经渲好的命令串** ⇒ 交给 `sh -c`。
        // 它与 `--print` 吐的是**同一个渲染函数的产物**，两条路结构上不可能分叉。
        Plan::Attach { .. } => exec_shell(&plan::render(&plan, None), &WHY_SHELL_ATTACH),
        Plan::Container(c) => {
            // 🔴 〔`K-R96` 09-12〕**这里从前有一段退让** —— 它只发生在真跑这条路上，
            //    于是 `--print` 吐的名字与真跑起出来的名字**可以不一样**。
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
                // 收尾片段以 ` && ` / `; ` 开头（它在 `--print` 里是接在建会话那段后面的）
                // ⇒ 单独跑时前面补一个 `:`，**不重写一份**（重写就是第二处住址）。
                exec_shell(&format!(":{tail}"), &WHY_SHELL_CONTAINER_TAIL)
            }
        }
        Plan::Direct(d) => exec_direct(d, resolved(&plan).as_deref()),
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
    serde_json::Value::Object(m)
}

/// 「这一趟非得经 POSIX shell」的**归因**——五句，一句一处，**都在 [`needs_shell`] /
/// [`execute`] 的判定旁边取用**，不许在别处另写一句
///（「一句只许一处用」由 [`tests::every_reason_for_needing_a_shell_is_declared_once_and_used_once`]
/// 机检，人群从本文件生产段现打）。
///
/// 🔴 它们的用处不是好看：`sh` 不在这台机器上时，这句话就是用户唯一看得到的**原因**
///（`D7`：失败要显式、归因要准确）。从前那条路只吐 `ccm: 起不来 —— program not found`，
/// 于是真机读数（`真相源/106 §3.3`）**只能靠对比 `claude` 那趟 `EXIT=0` 反推**
/// 「找不到的不是 `hostname`，是 `sh`」—— 错的归因比失败本身更贵。
pub(crate) static WHY_SHELL_CCM_ENV: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beCcm.whyShell.ccmEnv", &[]));
/// 同上：后端答出了 `resume` 那一问。
pub(crate) static WHY_SHELL_RESOLVED: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beCcm.whyShell.resolved", &[]));
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
///（真机现打住 `真相源/106 §3.3` 的 `agent` 那一行）。
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
/// 另两条（`CCM_ENV` / `resume` 被后端答出来）**是真的要 shell**，本函数不假装它们不要
/// ⇒ 在没有 `sh` 的机器上它们照旧做不到，只是从今天起**说得出口**（`no_shell:`）。
fn needs_shell(d: &plan::Direct, resolved: Option<&str>) -> Option<&'static str> {
    if !d.ccm_env.is_empty() {
        return Some(WHY_SHELL_CCM_ENV.as_str());
    }
    if resolved.is_some() {
        return Some(WHY_SHELL_RESOLVED.as_str());
    }
    if d.bus_id_recipe && d.inside_tmux {
        return Some(WHY_SHELL_BUS_ID.as_str());
    }
    None
}

/// 把一条渲好的命令串交给 `sh -c` 并**替换掉自己**。`why` = 这一趟为什么非得经 shell。
///
/// 起进程点，已登记进 `readonly_guard::spawn_registry::ALLOWED`。
fn exec_shell(line: &str, why: &str) -> i32 {
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c").arg(line);
    exec_or_spawn(
        cmd,
        &format!(
            "{NO_SHELL}: {}",
            copy_text("beCcm.execShell.needsSh", &[("why", why)])
        ),
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
/// 〔S5〕直路上给了 `--ccm-sid` 却没有令牌时那一句（stderr，**不报错、照常起**）。
pub(crate) static DIRECT_SID_NO_CARRIER: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beCcm.directSidNoCarrier.say", &[]));

/// 直路上 `--ccm-sid` 那一格要不要出声（[`plan::DirectIdentity`]）。只有「无载体」出声。
pub(crate) fn direct_identity_note(d: &plan::Direct) -> Option<&'static str> {
    (d.identity == plan::DirectIdentity::NoCarrier).then_some(DIRECT_SID_NO_CARRIER.as_str())
}

fn exec_direct(d: &plan::Direct, resolved: Option<&str>) -> i32 {
    if let Some(note) = direct_identity_note(d) {
        eprintln!("{note}");
    }
    // 非得要 shell 的那几趟（判定与归因都只住 `needs_shell` 一处）⇒ 整条走 `sh -c`。
    if let Some(why) = needs_shell(d, resolved) {
        return exec_shell(&plan::render(&Plan::Direct(d.clone()), resolved), why);
    }
    for k in &d.nested {
        std::env::remove_var(k);
    }
    let cfg_env = &d.account_env;
    if !d.config_dir.is_empty() {
        std::env::set_var(cfg_env, &d.config_dir);
    }
    if d.unset_config_dir {
        std::env::remove_var(cfg_env);
    }
    if !d.model.is_empty() {
        std::env::set_var("ANTHROPIC_MODEL", &d.model);
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
    let mut cmd = std::process::Command::new(prog);
    cmd.args(rest);
    exec_or_spawn(cmd, &format!("'{prog}'"))
}

/// POSIX 上就地 `exec`（不多一层进程）；其余平台退成「起它 + 等它 + 透传退出码」。
///
/// ⚠ **Windows 没有 `exec` 原语** —— `K26` 那句「就地 `exec`」在 POSIX 上成立、
/// 在 Windows 上不成立。这里不假装它成立，而是明确退成另一种形状。
///
/// `subject` = **起不来的是什么**。🔴 它不是装饰（`D7`）：从前这里只吐
/// `ccm: 起不来 —— {e}`，而 `{e}` 在 Windows 上逐字是 `program not found`
/// ⇒ 那句话**指不出是哪个 program**。真机读数（`真相源/106 §3.3`）为此只能靠
/// 「同一个 launcher 在 `claude` 那趟 `EXIT=0`」反推出「找不到的是 `sh`」。
/// ⇒ 现在两处调用点各自把主语带进来（`sh -c` 那条还带上「为什么非得经它」）。
fn exec_or_spawn(mut cmd: std::process::Command, subject: &str) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = cmd.exec();
        eprintln!(
            "{}",
            copy_text(
                "beCcm.execOrSpawn.failed",
                &[("subject", subject), ("e", &e.to_string())]
            )
        );
        return 4;
    }
    #[cfg(not(unix))]
    {
        match cmd.status() {
            Ok(s) => s.code().unwrap_or(1),
            Err(e) => {
                eprintln!(
                    "{}",
                    copy_text(
                        "beCcm.execOrSpawn.failed",
                        &[("subject", subject), ("e", &e.to_string())]
                    )
                );
                4
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/ccm_tests.rs"]
mod tests;
