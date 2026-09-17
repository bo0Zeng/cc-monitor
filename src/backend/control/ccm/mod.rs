//! `ccm` —— **后端的原生终端命令**（`K26` / `K33`）。
//!
//! 〔用@08-28 `K26` 逐字〕「**ccm 应当就是后端的原生命令，跟起会话那些东西一样，
//! 后端不是有原生命令吗？后端的原生命令就是 ccm，然后加各种参数实现其他功能**」。
//! 〔用@09-11 `K33` 逐字〕「后端**只有一个**……**不要有什么 bash 脚本**，
//! **不要有什么单独的 ccm**。**所有命令只许有一处**，其他都是**根据传参来调用**。」
//!
//! # 同一份实现两种模式
//!
//! - **常驻**：`cc-monitor-remote` 起来接流，别人连它（`main.rs` 的流模式）。
//! - **一次性**：在用户终端里认 argv，做完就走 —— **就是本模块**。
//!
//! # 怎么进到这里
//!
//! 两条，**都只经 [`intercept`] 这一处**：
//! ① `argv[0]` 的 basename 是 `ccm`（别名 / 软链 / 改名拷贝指过来；`shared/ccm-aliases.sh`
//!    里 `cc` / `cct` 那几个别名调的就是它）；
//! ② 显式子命令 `cc-monitor-remote ccm <argv…>`（给「二进制没改名」的场合，
//!    也给判据一个不依赖文件名的入口）。
//!
//! ⚠ **它不是一条 wire 子命令**，所以**不进 `main::SUBCOMMANDS`**、也不进
//! `doc/IPC-PROTOCOL.md` §10：那份文档是 monitor↔daemon 的**冻结线上契约**，
//! 而这里是**用户终端**的命令面，两者的读者与兼容性义务都不同。
//! 这个决定不是靠「没人查」成立的 —— `protocol_doc_guard::TERMINAL_SURFACE_FILES`
//! 把它登记成一个受管例外，并**另立一格**（每个旗标都要能在 [`USAGE`] 里找到）。
//!
//! # 本轮**没有**做到的，逐条写在这里（别读成做到了）
//!
//! - **预信任**（`~/.claude.json` / `~/.codex/config.toml` 那两处写入）：**没搬**。
//!   daemon 这个 crate 有一条「进程自身不许写用户既有数据」的红线
//!   （`readonly_guard`，白名单恰好一个模块）⇒ 搬它要先动那条红线，那是另一件活。
//!   后果：`claude` 起来可能弹信任框。`--print` 那条兜底轮询照旧在，捞得回来。
//! - **`$CCM_CONFIG` 是 bash 源文件**：旧实现 `. "$CCM_CONFIG"`（真 source 一段 bash）。
//!   这里只认 `KEY=value` 三个键（见 [`Env::from_process`]），**不是等价**。
//! - **`--help` 的正文**：旧实现是 `sed` 自己的注释块；这里是 [`USAGE`] 常量，**文本不同**。

pub(crate) mod argv;
pub(crate) mod plan;

use argv::{Die, Early, Parsed};
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
/// - `src-tauri/src/plugin_class_registry.rs` —— 数**个数**（那条断言里逐字写着
///   「这个数变了要顺手看一眼它们」）。加 token ⇒ **那个数要跟着改**，否则当场红。
/// - `src/backend/plugin/probe.rs` 的
///   [`crate::plugin::probe::tests::the_required_list_is_checked_against_what_the_real_plugin_declares`]
///   —— 它拿 [`probe_output`] **真吐出来的那一行** `capabilities=` 当活体语料，再数**个数**。
///   加 token ⇒ **那个数要跟着改**（与上一行同形，是本树内的第二处计数）。
///   〔依据：PM 刀 `P`（09-11）往本常量再加一个 token、别处一字不改，daemon 套
///   `682 → 681 passed / 1 failed`，**只红这一条**；monitor 套同刀 `1381 → 1380 / 1`，
///   只红上一行那条。⇒ 两处**各自最小面 1 条**，而它们是仅有的两处「数个数」的。〕
/// - `src-tauri/src/backend/control/ccm_invocation.rs` —— `CLI_REQUIRED_CAPS`
///   与判据自带的 `STATIC_CAPS_EXPECTED`，两处都是**子集检查** ⇒ 加 token 安全。
/// - `tests/e2e/ccm-contract-parity.sh` —— 数 `capabilities=` 覆不覆盖 TS 那一份，同样是**⊇**。
/// - `src-tauri/build.rs` 的 `extract_capabilities` —— ⚠ **它盖不到这里**：
///   它按 `const CAPABILITIES` 这一行去 `src/backend/main.rs` 里抠，
///   抠的是 daemon **流模式**那个同名常量（`bg` / `tail-only`），与本常量无关。
///   〔这句话是本轮实测的，不是推的：加了下面那个 token 之后 `DAEMON_CAPABILITIES` 逐字不变。〕
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
    "daemon-discover",
    "account-via-daemon",
    "base-url-across-tmux",
];

/// 撞名时那句话的**唯一格式串**。
/// ⚠ 结尾那两个字符是**反斜杠 + n**，不是一个真换行 —— 它是一条 **`printf` 格式串**：
/// 要被原样拼进 `--print` 吐的那条 shell 里（`printf '<本串>' '<名字>'`），
/// 由**那个 shell 里的 printf** 去解释它。写成真换行的话，`--print` 吐出来的命令会断成两行。
/// 自己要打这句话时（`execute` 的撞名出口）记得把它译回真换行。
pub(crate) const NAME_TAKEN_FMT: &str =
    "ccm: tmux 会话名 %s 已被占用 —— 拒绝静默接回别人的会话（C14：spawn 就是起）\\n";

/// 窗口标题的合成式 —— **让 tmux 自己从 `@ccm_sid` 合成**，与 pane 标题彻底分开。
///
/// monitor 靠扫窗口标题里的 `ccm-rbind-<sid>` 绑定终端窗口（`bind.rs`）。
/// 从前这里是 `#T`（窗口标题 = pane 标题），而 **claude 也在往 pane 标题写自己的状态**
/// ⇒ 两者抢同一个位置，真机实测忙碌那个会话的 marker 被冲成「⠐ 理解…」，
/// 点 ↗ 必弹「未绑定窗口」。⚠ 改它之前先读 `e2e` 那条已经删掉的套件在件文件 `§8` 里的登记。
pub(crate) const RBIND_TITLE_FORMAT: &str = "#{?@ccm_sid,ccm-rbind-#{@ccm_sid},#T}";

/// codex 的 cc-bus 身份配方。**输出的是配方不是值** —— 这样 `--print` 仍然不查实时 tmux 状态。
pub(crate) const BUS_ID_RECIPE: &str = "if [ -n \"${TMUX:-}\" ]; then _ccm_bus=\"$(tmux display-message -p \"#S\" 2>/dev/null)\"; [ -n \"$_ccm_bus\" ] && export CC_BUS_ID=\"$_ccm_bus\"; unset _ccm_bus; fi;";

/// `--help` 的正文。**每个认得的旗标都要在这里有一行** ——
/// 由 `protocol_doc_guard` 那条受管例外的配套判据机检。
pub(crate) const USAGE: &str = "\
用法：ccm [new|resume <sid>|attach <会话名>] [选项…] [-- 透传给 agent 的参数…]

  它就是后端本身的一次性模式：认 argv、做完就走。常驻模式是同一个二进制接流。

动作（位置参数，必须在最前；不给就是 new）
  new                起一个新会话
  resume <sid>       接着某个会话往下跑
  attach <会话名>    不起 agent，直接接回一个 tmux 会话

选项
  --resume <sid>     与位置形 `resume <sid>` 等价
  --tmux[=<名>]      把这条命令送进一个 tmux 容器里跑；给名就用那个名
  --tmux-base <基名> 以这个为底取名，撞了就退让（与 --tmux=<名> 互斥）
  --tmux-size <W>xH  新建会话的宽高（只在容器路有意义）
  --detach           建完就返回，不接进去（只在容器路有意义）
  --bus-register     把新会话登记上 cc-bus（需要 --detach）
  --bus-note <备注>  给上面那条登记带一行备注
  --account <名>     用这个账号的 configDir（与 --base 互斥）
  --base             显式不注入账号（issue #75 的逃生口）
  --cwd <目录>       工作目录；不给就是**当前目录**（ccm 不替你挑，想跳自己写这个参数或自己写别名）
  --agent <名>       claude | codex
  --model <名>       export ANTHROPIC_MODEL
  --launcher <命令>  覆盖默认启动器
  --ccm-sid <sid>    给这个会话打上意图标 @ccm_sid_expect
  --print            不跑，吐出等价的一行 shell（平价预言机）
  --ccm-probe        吐出 name= / version= / self= / capabilities= / agents= / build= 六行
  --version          印版本号
  --help, -h         这一段
";

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

/// 显式子命令形（`cc-monitor-remote ccm …`）的那个词。
///
/// ⚠ 刻意**不是** `--ccm`：那样它会长得像一条 wire 子命令，而它不是。
pub(crate) const SUBCOMMAND_WORD: &str = "ccm";

/// 这一趟是不是在当 `ccm` 用？是就返回**要交给 [`run`] 的那串 argv**。
///
/// 🔴 **它必须排在 `split_stream_flags` 之前**：那一步会把 `--with-bg` / `--tail-only`
/// 从 argv 里**任意位置**剥掉，而 `ccm -- --tail-only` 里那个是要原样透传给 agent 的。
pub(crate) fn intercept(argv0: &str, args: &[String]) -> Option<Vec<String>> {
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

/// 一次性模式的入口。返回**退出码**。
///
/// 退出码的四档（与旧实现逐字同义，消费者按码分支）：
/// `0` 正常 · `2` 用法错（`die`）· `3` 会话名被占 · `4` 起不来。
pub(crate) fn run(args: &[String]) -> i32 {
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
            print!("{USAGE}");
            0
        }
        Parsed::Early(Early::Probe) => {
            print!("{}", probe_output(&env.self_path));
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
pub(crate) fn probe_output(self_path: &str) -> String {
    format!(
        "name=ccm\nversion={CCM_VERSION}\nself={self_path}\ncapabilities={}\nagents={}\nbuild={}\n",
        CAPABILITIES.join(","),
        AGENTS.join(","),
        crate::BUILD_ID
    )
}

fn die(msg: &str) -> i32 {
    eprintln!("ccm: {msg}");
    2
}

/// `resume` 那一问：这个会话该怎么起。**在同一个进程里答** ——
/// 从前这里要跨一次进程去问 daemon（`--resolve`），那整段是 bash 与后端说话的税。
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
        Plan::Attach { .. } => exec_shell(&plan::render(&plan, None)),
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
                    eprintln!("ccm: 起不来 —— {code}: {msg}");
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
                exec_shell(&format!(":{tail}"))
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

/// 把一条渲好的命令串交给 `sh -c` 并**替换掉自己**。
///
/// 起进程点，已登记进 `readonly_guard::spawn_registry::ALLOWED`。
fn exec_shell(line: &str) -> i32 {
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c").arg(line);
    exec_or_spawn(cmd)
}

/// 在**本进程**里设好最终环境、`cd`，然后 `exec` 掉自己。
///
/// 🔴 这几步必须发生在**调用者那个进程**里：env 要落在最终 `exec` 的那个 shell 上，
/// 否则穿不过 tmux 的进程边界（旧 `cct` 正是死在这一步）。
fn exec_direct(d: &plan::Direct, resolved: Option<&str>) -> i32 {
    // 机器级 env（代理等）：它是一段 shell，只有 shell 解释得了 ⇒ 有它就整条走 `sh -c`。
    if !d.ccm_env.is_empty() || d.bus_id_recipe || resolved.is_some() {
        return exec_shell(&plan::render(&Plan::Direct(d.clone()), resolved));
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
        return die(&format!("无法进入目录: {}", d.cwd));
    }
    let Some((prog, rest)) = d.argv.split_first() else {
        return die("没有可执行的启动器");
    };
    let mut cmd = std::process::Command::new(prog);
    cmd.args(rest);
    exec_or_spawn(cmd)
}

/// POSIX 上就地 `exec`（不多一层进程）；其余平台退成「起它 + 等它 + 透传退出码」。
///
/// ⚠ **Windows 没有 `exec` 原语** —— `K26` 那句「就地 `exec`」在 POSIX 上成立、
/// 在 Windows 上不成立。这里不假装它成立，而是明确退成另一种形状。
fn exec_or_spawn(mut cmd: std::process::Command) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = cmd.exec();
        eprintln!("ccm: 起不来 —— {e}");
        return 4;
    }
    #[cfg(not(unix))]
    {
        match cmd.status() {
            Ok(s) => s.code().unwrap_or(1),
            Err(e) => {
                eprintln!("ccm: 起不来 —— {e}");
                4
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★★ **`KR96D2` 死值验第一刀：铸名避让只许问那一张快照，且算完就定死。**
    ///
    /// 「另起一份名字集合」有两种长法，这里各钉一条：
    ///
    /// | 长法 | 挡它的是什么 |
    /// |---|---|
    /// | 在这里自己去列一遍 tmux、拼一个 `Vec<String>` 喂给 `build` | **类型**：`plan::build` 只收 `TakenNames`，而它的字段是 `common::session_snapshot` 模块私有的 ⇒ **编译不过** |
    /// | 让 `build` 拿到名字之后，在 `execute` 里再退让一次（本文件从前正是这样） | **本条**：算完的名字一个字都不许再改 |
    ///
    /// 🔴 第二种是本件之前的真实形状 —— 后果不是「名字错了」，是
    /// **`--print` 与真跑吐的名字可以不一样**，而 `--print` 的全部意义就是当平价预言机。
    #[test]
    fn the_name_avoidance_has_exactly_one_source_and_the_plan_settles_it() {
        let prod = crate::guard_support::production_code(include_str!("mod.rs"));
        crate::guard_support::assert_no_test_code("control/ccm/mod.rs", &prod);

        let builds = prod.matches("plan::build(").count();
        assert_eq!(
            builds, 1,
            "本文件算了 {builds} 次计划（登记 1）—— `--print` 与真跑必须共用**同一次** \
             `plan::build` 的产物，算两次就是两条路各自铸一次名。"
        );
        assert!(
            prod.contains("session_snapshot::global().taken_names()"),
            "喂给 `plan::build` 的那份「已占用的名字」不是从会话快照来的。\n\
             ★ `R52` 裁定二：那张 hash 表只有一处住址（`common::session_snapshot`）。"
        );
        assert!(
            !prod.contains("next_free_name"),
            "本文件生产段里又出现了 `next_free_name` —— 退让回到了计划之外。\n\
             ★ 算完的名字就是最终名；在这里再退让一次 = `--print` 与真跑又分叉了。"
        );
        assert!(
            !prod.contains("gate::list_sessions"),
            "本文件又直接去问 `gate::list_sessions` 了 —— 那是判活那条投影，\n\
             铸名要的是 `TakenNames`（同一张快照，但过的是那个造不出第二份的类型）。"
        );
        // 反向自检：这几针不是靠「本文件恰好不含那些词」空转的。
        assert!(
            crate::guard_support::production_code(include_str!("plan.rs"))
                .contains("fn next_free_name"),
            "`plan.rs` 里找不到 `next_free_name` —— 退让规则搬家/改名了，本条在空转"
        );
    }

    /// 两条进入路都只经 [`intercept`]，而**别的写法一律进不来**。
    #[test]
    fn there_are_exactly_two_ways_in() {
        let none: Vec<String> = vec![];
        assert_eq!(intercept("/usr/local/bin/ccm", &none), Some(vec![]));
        assert_eq!(intercept("ccm", &none), Some(vec![]));
        assert_eq!(intercept("C:\\x\\ccm.exe", &none), Some(vec![]));
        let sub = vec!["ccm".to_string(), "resume".to_string()];
        assert_eq!(
            intercept("/opt/cc-monitor-remote", &sub),
            Some(vec!["resume".to_string()])
        );
        // 不是 ccm ⇒ 一律放行给流模式 / wire 子命令
        assert_eq!(intercept("/opt/cc-monitor-remote", &none), None);
        assert_eq!(
            intercept("/opt/cc-monitor-remote", &vec!["--ping".to_string()]),
            None
        );
        assert_eq!(intercept("/opt/ccmonitor", &none), None, "子串不算");
    }

    /// 〔搬自 `ccm-contract-parity` 的 `--ccm-probe` 那 5 条〕
    ///
    /// 这份输出是**外部契约**：`ccm_probe.rs::parse_probe_output` 按行解析
    /// `version=` / `capabilities=`。
    #[test]
    fn the_probe_output_is_the_shape_its_parser_expects() {
        let out = probe_output("/usr/local/bin/ccm");
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "name=ccm", "首行逐字 name=ccm 是判活依据");
        assert_eq!(lines[1], format!("version={CCM_VERSION}"));
        assert_eq!(lines[2], "self=/usr/local/bin/ccm");
        assert!(lines[3].starts_with("capabilities="));
        assert_eq!(lines[4], "agents=claude,codex");
        // 🔴 `K-R70`：**身份那一行，取自这个进程自己编进来的常量**。
        //   它与 `version=` 分开问是刻意的（前者「你是哪一份」、后者「你认得哪些参数」），
        //   理由全文住 `probe_output` 的头注。
        assert_eq!(lines[5], format!("build={}", crate::BUILD_ID));
        assert_eq!(lines.len(), 6, "多一行少一行都是契约变更，实得 {lines:?}");
        assert!(out.ends_with('\n'), "最后一行也要有换行");
        // 消费者今天要的那 8 个（`ccm_invocation.rs::CLI_REQUIRED_CAPS` ＋ account 维度）
        for c in [
            "new", "resume", "attach", "tmux", "cwd", "launcher", "ccm-sid", "account",
        ] {
            assert!(
                CAPABILITIES.contains(&c),
                "渲染器要的能力 `{c}` 不在 CAPABILITIES 里 ⇒ 那条路当场 NotInstalled"
            );
        }
    }

    /// 〔搬自 `ccm-cli` KCY4「行为/capabilities 变了 ⇒ 版本号跟着走」〕
    ///
    /// 这一版是**后端的原生命令**，不是那份 bash ⇒ 版本号必须比最后一版 bash（`4`）大。
    #[test]
    fn the_version_moved_because_the_implementation_did() {
        let n: u32 = CCM_VERSION.parse().expect("版本号是十进制");
        assert!(n > 4, "最后一版 bash 是 4，原生实现必须大于它，实得 {n}");
    }

    /// 〔搬自 `ccm-cli` KCY4「用法块里有那一行」〕
    ///
    /// **每个认得的旗标都要在 [`USAGE`] 里说得出** —— 这条同时是
    /// `protocol_doc_guard::TERMINAL_SURFACE_FILES` 那格受管例外的配套判据
    /// （那边保证「旗标字面量只住 argv.rs」，这边保证「住在那里的都说得出来」）。
    #[test]
    fn every_flag_we_accept_has_a_usage_line() {
        let src = crate::guard_support::production_code(include_str!("argv.rs"));
        let mut flags: Vec<String> = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = src[from..].find("\"--") {
            let at = from + rel + 1;
            let tail = &src[at..];
            if let Some(end) = tail[1..].find('"') {
                let tok = tail[..end + 1].to_string();
                // ⚠ 只收**像旗标的那一形**：`argv.rs` 里还有一堆以 `--` 开头的
                //   **报错文案**（「--account 与 --base 互斥」…），把它们当旗标
                //   会让这条判据去 USAGE 里找一整句话 —— 那是量具的射程画错了。
                let looks_like_a_flag = tok.len() > 2
                    && tok[2..]
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
                if looks_like_a_flag && !flags.contains(&tok) {
                    flags.push(tok);
                }
            }
            from = at + 2;
        }
        assert!(
            flags.len() >= 15,
            "只抽到 {} 个旗标 —— 抽取坏了，本断言在空转：{flags:?}",
            flags.len()
        );
        // 🔴 **匹配单位是「有没有属于它自己的那一行」，不是 `USAGE.contains(旗标)`。**
        //
        // 〔本轮变异台自查逮到，08-xx 那一族的又一形〕`contains` 那一版**是空转的**：
        // 把 `--detach` 自己那一行整行删掉，判据**照样绿** ——
        // 因为 `--bus-register` 那一行的括注里逐字写着「（需要 `--detach`）」。
        // 匹配单位（全文）比事实（它有没有自己的条目）**大**了一格，
        // 于是「顺带被别人提到一句」被读成了「说明了它」。
        let has_own_line = |flag: &str| {
            USAGE.lines().any(|l| {
                let t = l.trim_start();
                t.starts_with(flag)
                    && t[flag.len()..]
                        .chars()
                        .next()
                        .is_none_or(|c| c == ' ' || c == '[' || c == '=' || c == ',')
            })
        };
        let missing: Vec<&String> = flags.iter().filter(|f| !has_own_line(f)).collect();
        assert!(
            missing.is_empty(),
            "这些旗标认得、但 `--help` 里**没有属于它自己的那一行**：{missing:?}\n\
             （用户看得见的唯一一份说明就是 USAGE；认一个不说一个 = 隐藏开关。\n\
              ⚠ 在别的行的括注里被提一句**不算** —— 那一版实测是空转的。）"
        );
    }

    /// 🔴 `K-R61` 09-11：**`base-url-across-tmux` 这个 token 不是一句自称。**
    ///
    /// 它声明的那件事（把 `ANTHROPIC_BASE_URL` 带过 tmux 的进程边界）由本条
    /// **真去算一遍容器路的计划**来兑现 —— 量的是 [`plan::build`] 交出来的那一串载荷，
    /// 不是源码文本，也不是本条自己再写一遍的什么规则。
    ///
    /// ⇒ 两个方向都有牙：
    /// - 把 token 从 [`CAPABILITIES`] 里删掉 ⇒ 第一格红（**说了才算数**）；
    /// - 把 `plan.rs` 那句 `export ANTHROPIC_BASE_URL=…` 掏掉 ⇒ 第二格红（**做到了才许说**）。
    ///
    /// ⚠ **本条没买到的**：「变量真的穿过了一次**真** tmux 边界」要真机 tmux，本条量的是
    /// 载荷字符串。那一半归 e2e，别把本条读成「实测过了」。
    #[test]
    fn the_base_url_token_is_declared_because_the_tmux_path_really_forwards_it() {
        // ① 申报这一半。
        assert!(
            CAPABILITIES.contains(&"base-url-across-tmux"),
            "`base-url-across-tmux` 不在 CAPABILITIES 里了 —— 那么 monitor 侧\n\
             `history.rs::RELAY_KEEPS_THE_OLD_PATH` 的退役条件就**又没有落点了**，\n\
             而它正是 `K-R61` 立件的原因（前提指着一个已经被删掉的文件）。"
        );

        // ② 实现这一半 —— 真算一遍，只喂替身环境，一个字节不碰这台机器（`K31`）。
        let base_env = || Env {
            home: "/home/pi".into(),
            pwd: "/p".into(),
            accts_manifest: "/nonexistent/accounts.json".into(),
            account_env: "CLAUDE_CONFIG_DIR".into(),
            self_path: "/usr/local/bin/ccm".into(),
            ..Default::default()
        };
        let payload_of = |env: &Env| -> String {
            let args: Vec<String> = ["--tmux=n1", "--cwd", "/p"]
                .iter()
                .map(|s| s.to_string())
                .collect();
            let Parsed::Opts(o) = argv::parse(&args).expect("该解析得动") else {
                panic!("`--tmux=n1` 不该被解析成 Early")
            };
            match plan::build(&o, env, &AccountTable::default(), None).expect("该算得出计划")
            {
                Plan::Container(c) => c.payload,
                other => panic!("`--tmux=` 该走容器路，实得 {other:?}"),
            }
        };

        let mut with_relay = base_env();
        with_relay.anthropic_base_url = Some("https://relay.example/v1".into());
        let sent = payload_of(&with_relay);
        assert!(
            sent.contains("export ANTHROPIC_BASE_URL='https://relay.example/v1'"),
            "容器路的载荷里没有把中转地址显式化 ⇒ 它在 tmux 边界上会被吃掉，\n\
             而 CAPABILITIES 里那个 `base-url-across-tmux` 就成了一句**假申报**。\n\
             实得载荷：{sent}"
        );

        // ③ 反空真对照：不给这个变量，那一串里不许出现它。
        //    （没有这一格，上面那句 `contains` 可能是靠载荷恒带某段文本过的。）
        let clean = payload_of(&base_env());
        assert!(
            !clean.contains("ANTHROPIC_BASE_URL"),
            "没设中转地址，载荷却带上了它 —— 上面那一格此刻是恒真的：{clean}"
        );
    }

    /// 闭集只有一处住址：`AGENTS` 与那几个按 agent 分支的函数必须**逐个对得上**。
    #[test]
    fn the_agent_set_has_one_address_and_every_member_is_wired() {
        assert_eq!(AGENTS, &["claude", "codex"]);
        for a in AGENTS {
            assert!(!default_launcher(a).is_empty(), "{a} 没有默认启动器");
        }
        assert_eq!(resume_flag("claude"), Some("--resume"));
        assert_eq!(resume_flag("codex"), None, "codex 没有 resume flag");
        assert_eq!(nested_env("claude").len(), 4);
        assert!(
            nested_env("codex").is_empty(),
            "codex 不清 claude 的嵌套标记"
        );
        assert!(needs_bus_id("codex") && !needs_bus_id("claude"));
        assert!(has_identity("claude") && !has_identity("codex"));
    }

    /// 〔搬自 `ccm-rbind-title` 的 format 那一格〕
    ///
    /// ⚠ **如实边界**：那套 e2e 是**真起一个私有 socket 的 tmux**、把 pane 标题冲成
    /// 「⠐ 理解…」再读窗口标题的。这里只钉**那个格式串本身**，
    /// 「tmux 真的这么解释它」那一半**没有判据了** —— 登记在件文件 `§8`，别读成等价。
    #[test]
    fn the_window_title_is_synthesised_from_the_identity_tag_not_the_pane_title() {
        assert_eq!(RBIND_TITLE_FORMAT, "#{?@ccm_sid,ccm-rbind-#{@ccm_sid},#T}");
        assert!(
            RBIND_TITLE_FORMAT.starts_with("#{?@ccm_sid,"),
            "它必须是**条件式**：有 @ccm_sid 才出 marker，没有才回退 #T"
        );
        assert!(
            RBIND_TITLE_FORMAT.contains("ccm-rbind-#{@ccm_sid}"),
            "marker 必须逐字是 monitor 侧 bind.rs 要扫的那个前缀 + sid"
        );
        assert!(
            RBIND_TITLE_FORMAT.ends_with(",#T}"),
            "sid 还没回填时要回退 pane 标题，而不是产出一个空的 `ccm-rbind-`"
        );
    }

    /// 🔴 这个格式串**盘上有两份**（本常量 ＋ `control/launch.rs` 那一行的字面量），
    /// 而两份不许漂开。
    ///
    /// # 为什么不干脆收口成一份
    ///
    /// 试过。收口之后 monitor 侧
    /// `ccm_cli_contract::the_intent_tag_and_the_fact_tag_are_not_merged_by_the_move`
    /// 当场红：那条判据数的是 `control/launch.rs` **生产段里**「事实标记读点」的处数
    /// （登记 2 处 = 这一行里的条件头 `@ccm_sid` 与取值 `#{@ccm_sid}`），
    /// 收口成一个标识符之后它读到 **0**，而 0 的含义逐字是「标题回填没了」。
    /// ⇒ 收口会把一条真判据变瞎。**留两份 + 本条钉住它们逐字相同**，买到的比收口多。
    #[test]
    fn the_window_title_format_has_the_same_text_on_both_sides() {
        let launch = crate::guard_support::production_code(include_str!("../launch.rs"));
        assert!(
            launch.contains(RBIND_TITLE_FORMAT),
            "`control/launch.rs` 的生产段里找不到这个格式串的逐字副本：\n  {RBIND_TITLE_FORMAT}\n             两份已经漂开了（或者那一行被收口成了标识符 —— 别那么做，理由见本条头注）。"
        );
        assert!(
            launch.contains("set-titles-string"),
            "`launch.rs` 不再设 `set-titles-string` 了 —— 那是标题回填的落点"
        );
    }

    /// 〔搬自 `ccm-cli` WIRE/launch「发对了①–⑤」「缺省尺寸①②」「控制字符①–④」那几族〕
    ///
    /// 从前那几条测的是「`ccm` 编出来的那段 JSON 上线之后逐字节对不对」。同一个进程之下
    /// 没有「上线」这回事了 —— 剩下的真契约是**那几件事一件都不许丢**，
    /// 而且**必须经过 `parse_request` 那道门**（字段校验只长在它身上）。
    #[test]
    fn the_container_launch_goes_through_the_one_door_with_every_field_intact() {
        let c = plan::Container {
            name: "n1".into(),
            cwd: "/p".into(),
            agent: "claude".into(),
            ccm_sid: "p1".into(),
            size: Some(("220".into(), "50".into())),
            detach: true,
            payload: "'/usr/local/bin/ccm' '--cwd' '/p'".into(),
            trust_poll: true,
            bus: None,
        };
        let req = crate::control::launch::parse_request(&launch_args(&c)).expect("该过得了门");
        assert_eq!(req.name, "n1");
        assert_eq!(req.payload, c.payload, "载荷不许被改一个字节");
        assert_eq!(req.cwd.as_deref(), Some("/p"));
        assert_eq!(req.ccm_sid.as_deref(), Some("p1"), "意图标不许丢");
        assert_eq!(
            req.agent.as_deref(),
            Some("claude"),
            "@ccm_agent 不许丢（它丢过一次）"
        );
        assert_eq!(req.width.as_deref(), Some("220"));
        assert_eq!(req.height.as_deref(), Some("50"));
        assert!(matches!(
            req.mode,
            crate::control::launch::Mode::CreateOrAttach
        ));
        // 不给尺寸 ⇒ 请求里**没有** width/height（不是空串、不是 0）
        let mut c2 = c.clone();
        c2.size = None;
        let r2 = crate::control::launch::parse_request(&launch_args(&c2)).expect("该过得了门");
        assert!(r2.width.is_none() && r2.height.is_none());
        // 🔴 那道门真的在判：载荷里塞一个 ESC ⇒ 被**挡在这一侧**，不是发出去被拒
        let mut c3 = c.clone();
        c3.name = "n\u{1b}1".into();
        assert!(
            crate::control::launch::parse_request(&launch_args(&c3)).is_err(),
            "控制字符没被挡住 —— 那道门被绕过去了（直接造 LaunchRequest 就是这个后果）"
        );
    }

    /// 〔搬自 `ccm-cli` WIRE/launch 撞名那一族〕—— 撞名的那句话必须带上是哪个名字。
    #[test]
    fn the_name_taken_message_says_which_name() {
        let msg = NAME_TAKEN_FMT.replacen("%s", "cc-proj", 1);
        assert!(msg.contains("cc-proj"), "不带名字的报错等于没报：{msg}");
        assert_eq!(
            NAME_TAKEN_FMT.matches("%s").count(),
            1,
            "格式串只许有一个占位"
        );
        // 🔴 结尾必须是**字面的两个字符** `\` + `n`，不是一个真换行 —— 见常量头注。
        //   写成真换行 ⇒ `--print` 吐的那条命令会在这里断成两行。
        assert!(
            NAME_TAKEN_FMT.ends_with("\\n") && !NAME_TAKEN_FMT.ends_with('\n'),
            "它是 printf 的格式串，结尾要是字面 \\n：{NAME_TAKEN_FMT:?}"
        );
        assert_eq!(
            msg.replace("\\n", "\n").lines().count(),
            1,
            "译回真换行之后它是**一行**（末尾一个换行），不是两行"
        );
    }
}
