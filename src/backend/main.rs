//! Phase-0 SSH-remote backend prototype.
//!
//! The remote half of the steel thread: it resolves `~/.claude`, emits a single
//! `Hello` frame, then tails session JSONL files and streams `line` /
//! `session_added` / `session_removed` frames as one JSON object per line on
//! stdout. The client end is the cc-monitor Tauri app over an SSH pipe.
//!
//! Runtime target is Linux (inotify); the code is cross-platform and compiles +
//! runs a basic file-watch smoke on Windows (`notify` is portable).
//!
//! ## Two-task design (the §5.4 slow-consumer guard)
//!
//! - The **reader** ([`observe::watcher::spawn`]) runs the filesystem watcher on a
//!   blocking thread and pushes frames into a *bounded* channel with `try_send`
//!   (never blocking the inotify callback).
//! - The **writer** (this file's [`writer_task`]) drains the channel and writes
//!   wire lines to stdout. A slow SSH pipe back-pressures the channel — and the
//!   bound stops the back-pressure at the channel, so it never reaches the
//!   inotify reader. This split is the single most-cited Phase-0 accident
//!   source; keeping it real is the point.

// 🔴 〔步 9 · 09-19〕模块声明、`PROTO_VERSION`、身份块（`BUILD_ID` ＋ 戳）、`SUBCOMMANDS`
//    都搬进了 `lib.rs`，理由逐字写在那份文件的头注里（一句话：**in-process 那条路没有
//    这个 `main.rs`**，身份与模块跟着它一起消失）。**本文件只留分派**（规格 `00 §1.5.4` 逐字）。
// ⚠ 用 glob 而不是逐项列 —— 本拍是**纯机械搬家**，逐项列会让 diff 里混进
//    「哪些项对外可见」这个**语义**决定，那是另一件事（`4b` 定 API 面时再收窄）。
use cc_monitor_backend::*;

use std::path::PathBuf;
use tokio::io::{AsyncWriteExt, BufWriter};
use wire::{to_line, Frame};

// 本测块紧邻被测的第四条面（就近可读）、不挪文件尾；显式 allow 让 clippy --all-targets 净
//（同上面 `stream_flag_tests` 那一块的理由）。

// `K-P4`：拉窗构件的**落点判据**（PM 09-04 裁的那一条，见 `K-P4-PM.md §五㈠`）。
//
// # 它管的是哪一维 —— 与已有三道护栏**不重叠**
//
// 摸底那一拍现打过：Win32 那几个构件在三张针表里**一个都没有**
//（`readonly_guard` 的 11 条写盘针 · `no_timer_guard` 的 5 条周期唤醒针 + 8 条调用形态）
// ⇒ 一道拦写盘、一道拦「自己醒来」，**拉窗那一段没有任何后端判据看着它**。
// 而 `platform/fallback_guard` **只扫 `src/platform/`**，且它自陈「人群比性质小」、
// 真守住的是那一层里**内联写法**的 ⇒ 它本来就不是「管平台原语」的那把尺子。
// 🔴 PM 因此裁：**拉窗构件落 `platform/` 之外，并同拍补一条判据管那一维** —— 就是本模块。
//
// # 今天它扫到的真实命中是 **0**（这一句必须写在前面）
//
// backend crate 里今天一个 Win32 拉窗构件都没有（`Cargo.toml` 里 `windows`/`winapi` 命中 0）。
// ⇒ 上面两条正题断言今天**都在空转**，真正有读数的是**空转自检**那两条合成样本。
// 本模块是**在搬家之前**先把闸门立起来：等 `control/focus.rs` 那一段真落进来的那天，
// 它是第一个开口的人。

// U6b-2 **argv 三分表**：backend 认识的每个 `--token` 恰好属于其中一类。
//
// # 为什么要有这张表
//
// 在它之前是**二分**：剥掉流 flag，剩下非空就当一次性查询。后果实测：
//
// ```text
// $ cc-monitor-backend --some-future-flag
// cc-monitor-backend query error: unknown argument: --some-future-flag
// rc=2
// ```
//
// **未知 flag 在流位置 ⇒ exit 2、一个字节都不输出、没有 hello。** monitor 那头看到的
// 和「backend 崩了」无法区分 ⇒ 重连 ⇒ 发同一个 flag ⇒ **死循环**。这正是 2026-07-09
// 事故的形状。`every_capability_token_is_strippable` 挡不住它——那条只覆盖**与已声明
// 能力绑定**的 flag，「monitor 因为别的原因发了个新 flag」不在它的判据里。
//
// # 这张表**漏一项**的后果比旧行为更糟，所以必须有完备性机检
//
// 判据改成「`args[0]` ∈ [`SUBCOMMANDS`] ⇒ 查询模式」之后，漏登记一条子命令不再是
// exit 2（吵，但看得见），而是**那条子命令静默变成「起了个流」**——调用方拿到一堆
// jsonl 行而不是查询结果。这是 v3.4.0 `--account-trust-zero` 漏登记那次事故的**加强版**。
// ⇒ `every_dispatched_token_is_classified` 是本组的核心交付，不是附属品。

// 本测块紧邻被测的 split_stream_flags（就近可读）、不挪文件尾；显式 allow 让 clippy
// --all-targets 净（审计：门槛此前只跑默认 target、漏 test-target lint）。

#[tokio::main]
async fn main() {
    // ★★ `K-R48`（09-11）：**当 `ccm` 用的那一趟，在这里就整条分出去。**
    //
    // 〔用@09-11 `K33`〕「后端**只有一个**，**不要有什么 bash 脚本**，**不要有什么单独的 ccm**。」
    // ⇒ 终端里敲的 `ccm` 就是本二进制（别名 / 软链指过来，或 `cc-monitor-backend ccm …`）。
    //
    // 🔴 **三个「必须排在前面」，一个都不是排版**：
    //   ① 排在 `tracing_subscriber` 之前 —— 一次性模式的 stderr 是给人看的，
    //      混进后端的日志行就把「正常路径一个字都不说」这条契约破了；
    //   ② 排在 `split_stream_flags` 之前 —— 那一步会把 `--with-bg` / `--tail-only`
    //      从 argv **任意位置**剥掉，而 `ccm -- --tail-only` 里那个要原样透传给 agent；
    //   ③ 排在 `resolve_agent_home()` 之前 —— 一次性模式不必去解析 agent 家目录。
    {
        let argv0 = std::env::args().next().unwrap_or_default();
        let rest: Vec<String> = std::env::args().skip(1).collect();
        if let Some(ccm_args) = control::ccm::intercept(&argv0, &rest) {
            std::process::exit(control::ccm::run(&ccm_args));
        }
    }

    // Log to stderr so it never corrupts the stdout wire stream.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let agent_home = resolve_agent_home();

    // issue #16：带参数 = 一次性历史查询模式，干完即退，不进流式协议。
    // 旧后端不认参数会照常发 hello 进流模式——monitor 以"首行是 hello 帧"
    // 识别旧版并提示升级（优雅降级，无协议版本协商负担）。
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Batch7-F24/Batch8-F25：流模式 flag 集合，先剥离再判一次性查询模式
    // （否则误入 query 分支——INVARIANT §26）。纯函数化供单测（审计 D）。
    let (args_rest, with_bg, tail_only, with_rbind_token) = split_stream_flags(args);
    let args = args_rest;
    if is_query_mode(&args) {
        // 一次性查询模式：--search 全文搜索（#28）/
        // --resolve advisor（backend-04，读 stdin ResumeSpec→stdout CommandPlan），其余走历史查询（#16）。
        let code = match args.first().map(String::as_str) {
            // P4b：hook 子进程走这条 —— 校验身份后给后端发 SIGUSR1，**不碰文件系统**。
            Some("--tmux-notify") => control::tmux_hook::notify(&args),
            // `K-R86`：只读抓屏原语。**抓一次、立刻返回** —— 轮询归 `K-R87`，
            // 零定时器铁律（`no_timer_guard`）看着本 crate 的每一份生产段。
            Some("--capture-pane") => control::capture_pane::run(&args),
            // `K-R87`：起一个到点自己会死的一次性会话。看门狗是**外部进程**，
            // 不在本 crate 的源码文本里 —— 零定时器铁律的人群逐字排除「被起进程的行为」。
            Some("--search") => observe::search_query::run(&agent_home, &args),
            // P7c-1：列一个父会话的 subagent 候选。**只列不挑**（匹配与排序留在 monitor）。
            Some("--list-subagents") => observe::history_query::list_subagents(&agent_home, &args),
            Some("--resolve") => control::resolve_query::run(&agent_home, &args),
            // G2（branch-anywhere）：从指定消息处分叉出一个新会话文件。
            // **backend 唯一的写盘入口**，护栏白名单层单独盯着它（readonly_guard）。
            Some("--fork-session") => control::fork_write::run(&agent_home, &args),
            // K-H1：HTTP 中转。**常驻**，起来就不返回；配置面只有环境变量。
            // K-H2a：多传一个 `agent_home` —— 这个进程的账号层（层 2）要从 `<home>/claudecode-frontend/` 下
            // 读那份凭据文件。**不新开子命令、不动 `SUBCOMMANDS`** ⇒ 不逼出 BUILD_ID bump。
            Some("--relay") => accounts::apikey::run_relay(&agent_home, &args),
            // K-P6b：拨号代理。**常驻**，起来就搬字节直到某一头断开。
            // 它不认 `agent_home`（不读任何 agent 的东西），也不碰 `listen::Admit`
            // —— `K-P7` 逐字写着 E 成立的条件就是「不复用 `listen::Admit`」。
            Some("--dial") => dial::run(&args).await,
            // ★ 这几个字面量必须与 `observe::accounts_query::run` 自己认的子命令**完全一致**。
            // v3.4.0 出过一次事故：`--account-trust-zero` 在 accounts_query 里实现完整，
            // 但这里漏列 ⇒ 落进下面的 `_` 臂走历史查询 ⇒ `unknown argument` + exit 2，
            // 而 monitor 的账号 0 路径**真的在发这条命令**。
            // 测试当时抓不到，是因为它们直接调 `observe::accounts_query::run`、**绕过了本处调度**。
            // 现由 `observe::accounts_query::tests::main_dispatches_every_subcommand_we_handle` 钉住。
            // 〔`A3` 第二波〕本机那一侧的 `cc-acct-iso` 两问 —— 住账号层（不住 `observe/`：shellinit 要起进程）。
            // ⚠ 两条臂各写一行（不合成 `A | B`）：合起来超宽，`cargo fmt` 会把臂体折成块，
            //   而 `argv_table_guard` 按行取臂体、块体会被判成「不是一次调用」。
            Some("--acct-iso-status") => emit_answer(accounts::iso::answer(&args)),
            Some("--acct-iso-shellinit") => emit_answer(accounts::iso::answer(&args)),
            Some("--list-accounts")
            | Some("--session-accounts")
            | Some("--account-trust")
            | Some("--account-trust-zero") => observe::accounts_query::run(&agent_home, &args),
            // ★ P4d：控制面的 CLI 入口。**这条臂刻意不写命令字面量** ——
            // 认哪些 flag 由 `cli_control::spec_for` 从 `inbound::REGISTRY` 派生，
            // 于是「帧面加一条命令」不需要回来改这里。上面 `--resolve` 那条臂**故意留在前面**：
            // 它的信封与仓外 aterm 冻结在 2026-07-18，走原路一个字节都不动
            // （两条路的输出实为同一个 `CommandPlan`，但冻结的契约不拿「实际上一样」去赌）。
            // ⚠ **必须写成单行臂**：`argv_table_guard::every_dispatch_arm_actually_calls_an_implementation`
            // 按**行**取 `=>` 右边的臂体，块体臂会被抽成 `""` 当场红（实测）。
            // 那条约束是保守的（宁可假红），照它写就是了 —— 单行形式下它钉的
            // 「臂体是一次真调用」也确实成立。
            Some(f) if control::cli_control::handles(f) => control::cli_control::run(&args).await,
            _ => observe::history_query::run(&agent_home, &args),
        };
        std::process::exit(code);
    }

    // 一次性查询已 exit；到此必是流模式。agent_home 日志放此（审计 correctness-重要①：
    // --resolve/一次性查询模式 stderr 只承载结构化错误/查询结果，不掺 info——兑现协议 v1 §3
    // 「错误 exit2 + stderr 纯 {code,message} JSON」，客户端可整段 JSON-parse stderr）。
    tracing::info!("agent_home = {}", agent_home.display());

    // ★★ `K-P1`：**同一个流模式，两种载体**。
    //
    // 「脱离宿主」本身不难（`launch.rs` 里三份现成的范例）；难的是**脱离之后还怎么跟它对话**
    // —— 今天讲协议走的就是那对 stdio 管道，一脱离管道就没了。
    // ⇒ 换载体**不换协议**：`wire.rs` 头注那句「exactly one UTF-8 JSON object per line」
    // 在两条载体上逐字成立；`inbound::spawn<R: AsyncRead>` 与 `writer_task<W: AsyncWrite>`
    // 本来就是泛型的，喂 socket 的两半与喂 stdin/stdout **在类型上无差别**。
    //
    // ⚠ **由环境决定，不由 argv 决定**：`SUBCOMMANDS` 那张表一动就要 bump `BUILD_ID`
    // 并进 `IPC-PROTOCOL.md` 的对拍面（`build_id_guard` / `protocol_doc_guard` 各钉一半），
    // 而本件**一条子命令都没加** —— 它换的是同一个流模式的载体。
    // 判定住 [`listen::mode_from`]（**纯函数**，所以「只写了一半」那两条错误支都测得到）。
    let mode = match listen::mode_from(&|k| std::env::var(k).ok()) {
        Ok(m) => m,
        Err(e) => {
            // **fail closed**：宁可不起，也不要起一个不设防的口 —— 回环 TCP 没有权限位。
            tracing::error!("监听口配置不成立 ⇒ 拒绝起：{e}");
            std::process::exit(listen::EXIT_BAD_LISTEN_CONFIG);
        }
    };

    // (b) Emit the Hello handshake FIRST, flushed, before anything else.
    let hello = build_hello(&agent_home);

    match mode {
        listen::Mode::Stdio => {
            run_over_stdio(hello, agent_home, with_bg, tail_only, with_rbind_token).await
        }
        listen::Mode::Listen { port, token } => {
            // 停机信号只挂**一次**（不在 accept 循环里每轮重装一个 SIGTERM 处理器）。
            tokio::select! {
                _ = serve_listening(
                    port,
                    token,
                    hello,
                    agent_home,
                    with_bg,
                    tail_only,
                    with_rbind_token,
                ) => {}
                _ = shutdown_signal() => {
                    tracing::info!("shutdown signal received; exiting");
                }
            }
        }
    }

    // ★ **必须显式 exit，不能让 runtime 自然 drop。**
    //
    // `tokio::io::stdin()` 走的是**阻塞线程池**。`inbound_task.abort()` 只取消那个 async
    // task，**阻塞中的 `read(0)` 不受影响**；而 `#[tokio::main]` 展开出来的 runtime 在 drop
    // 时会等所有 blocking 任务结束 ⇒ 只要对端还开着 stdin，进程就永远停在这一行。
    //
    // D 审计实测（U6b-1 引入入方向之后，相对父提交的**回归**）：
    // stdin 接一条开着但没数据的 FIFO，发 SIGTERM ⇒ 3/3 复现「5s 后仍未退出」，
    // stderr 末行已经打了 "shutdown signal received; exiting" —— 清理跑完了，就是不退。
    // 父提交同一脚本 100ms 内退出。
    //
    // 爆炸半径正是生产形状：monitor 经 SSH exec 连着时 stdin 一直开着。
    // 远端手工 kill 一个卡住的后端、部署脚本替换在跑的二进制，今天都会失效。
    //
    // 为什么 exit 是安全的：**流模式后端没有任何待落盘状态** —— 它只读；
    // 唯一的写盘入口 `control/fork_write.rs` 在一次性查询模式，那条路早就 exit 了。
    // stdout 也不欠 flush：`writer_task` 每帧写完即 flush。
    std::process::exit(0);
}

/// 造那一帧 hello。**抽出来是因为两条载体都要发它**，而它必须只有一份 ——
/// 两份 hello 会各自漂，而这一帧是仓外 aterm 按精确字节在读的东西。
/// 〔`A3` 第二波〕把账号层产出的一次查询答案写出去 —— 进程的 stdout / stderr 归入口这一处。
///
/// 账号层（`accounts/`）不自己 `print`：它同时是 `--relay` 进程的层 2，那一层的每一条输出都在
/// 中转日志白名单底下（`relay::creds_guard`），而查询的输出不是日志。理由全文见
/// `accounts::iso::Answer` 头注。
fn emit_answer(a: accounts::iso::Answer) -> i32 {
    print!("{}", a.stdout);
    if let Some(e) = a.stderr {
        eprintln!("{e}");
    }
    a.code
}

fn build_hello(agent_home: &std::path::Path) -> Frame {
    Frame::Hello {
        v: PROTO_VERSION,
        build_id: BUILD_ID.to_string(),
        host_arch: std::env::consts::ARCH.to_string(),
        // ⚠ **左边的字段名与右边的变量名刻意不一致**〔`S4b`〕，这不是笔误：
        // 左边 `claude_dir` 是 **wire 字段**，冻结兼容（真在线上、仓外 aterm 在读），
        // 登记在 `agent_boundary_guard::FROZEN_COMPAT`，带解锁条件，**不许改名**；
        // 右边 `agent_home` 是**仓内的参数名**，`S4b` 已把它从 `claude_dir` 改过来
        //（通用层不该在标识符里叫得出某个 agent 的名字 —— `D3` 的同一条道理往仓内推）。
        // ⇒ 这一行正是两条纪律的交界处：**字段名归契约，变量名归架构**。
        claude_dir: agent_home.to_string_lossy().into_owned(),
        // `S4`（`D3`）：wire 面已换成通用的 `homes`（`[{agent_kind, path}]`）。
        //
        // ★★〔`S5` 08-14〕**这一行是空表，但已经不是因为"做不到"了。**
        // `agents::visible_homes()` 今天就能答出这台机器看得见哪些 agent
        //（判准：home 目录存在；理由与被排除的另两条候选写在那个函数的头注里）。
        // 换过去只要改这一行 —— `S5` 的口径是 **能填不真填**：
        //   填 = 一次**跨仓契约变更**（仓外 aterm 的 hello fixture 按精确字节对），
        //   而本机没有 aterm 仓、验不了它的运行时（前提 `P3`）
        //   ⇒ 把"何时真填"留成一次**纯发布决策**，而不是顺手改过去。
        // 真填那天要同轮做的三件事：① 换这一行；② 更新
        //   `dg3_codex_fields_skipped_when_absent_claude_byte_equivalent` 的期望串；
        //   ③ **bump `BUILD_ID`**（那天线上字节真的变了，已部署的远端得被判 stale）。
        // ⚠ 无论如何**不要再加第二个目录字段** —— 那正是 `D3` 排除掉的路。
        // 这一行由 `production_hello_leaves_homes_empty_so_claude_bytes_stay_frozen` 钉住
        //（它会在那天**故意变红**：那是提醒，不是障碍）；旁边那条
        // `the_backend_can_already_discover_homes_it_just_does_not_send_them`
        // 钉的是另一半 —— 空表不等于没能力。
        homes: Vec::new(),
        capabilities: CAPABILITIES.iter().map(|s| s.to_string()).collect(),
        emits: EMITS.iter().map(|s| s.to_string()).collect(),
        commands: inbound::COMMANDS.iter().map(|s| s.to_string()).collect(),
        // ★★〔`K-P4` 09-04〕**握手帧第四条面：「我做得到什么」。这一行也是空表，
        // 而它同样已经不是因为"做不到"了。** `unavailable_here()` 今天就能答出这台机器上
        // 哪几条命令做不到（判准 = `tmux` 在不在 `PATH` 上，与真调用那一刻同一个判准；
        // 表本身从 `inbound::REGISTRY` 的 `codes` **派生**，不是手写的第二份真相）。
        //
        // 换过去只要改这一行 —— 口径与 `homes` 那一行逐字相同：**能填不真填**。
        //   填 = 一次**跨仓契约变更**（仓外 aterm 的 hello fixture 按精确字节对，
        //   契约冻结 2026-07-18），而本机没有 aterm 仓、验不了它的运行时
        //   ⇒ 把「何时真填」留成一次**纯发布决策**。
        // 真填那天要同轮做的三件事写在 `wire.rs` 那个字段的头注里（第三件是 **bump `BUILD_ID`**）。
        //
        // 🔴 **真填之前，这一格买到的不是「事前协商」本身，是它的形状 + 一条能验的填法。**
        // 别把「字段加上了」读成「界面已经不会画死按钮了」——那要等消费侧接线。
        // 这一行由 `production_hello_leaves_unavailable_empty_so_the_wire_bytes_stay_frozen`
        // 钉住（它会在那天**故意变红**：那是提醒，不是障碍）；旁边那条
        // `the_answer_is_a_function_of_the_machine_not_of_the_build` 钉的是另一半 ——
        // **空表不等于这个字段是个编译期常量**。
        unavailable: Vec::new(),
    }
}

/// 今天那条路：**stdin/stdout 一对管道**。宿主一退读端就断，backend 153ms 内自己走。
///
/// ⚠ 本函数体是 `K-P1` 之前 `main()` 的那一段**原样搬过来的**，一行行为都没改 ——
/// 常驻是**加一条载体**，不是把这条改掉。改这一段之前先问：另一条载体要不要跟着改？
async fn run_over_stdio(
    hello: Frame,
    agent_home: PathBuf,
    with_bg: bool,
    tail_only: bool,
    with_rbind_token: bool,
) {
    let mut stdout = BufWriter::new(tokio::io::stdout());
    // U6b-3：写 + flush 一步到位，**并拿到 `HelloFlushed` 见证**。
    // 那个见证是 `inbound::spawn` 的必填参数 ⇒「reader 抢在 Hello 之前起来」
    // 变成编译期不可表示（此前靠一条比较字节位置的机检，被普通函数抽取绕过）。
    let hello_flushed = match wire::write_and_flush_hello(&mut stdout, &hello).await {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("failed to write/flush hello frame: {e}");
            return;
        }
    };

    // (b2) U6b-1：**入方向 reader。位置不可上移。**
    //
    // ★ 必须在上面那个 Hello 已经 flush **之后**才起 —— 客户端要先读到 Hello 才知道
    // 对面是什么版本、有什么能力；在那之前就收命令，等于在能力协商之前执行它。
    // 这是**时序约束**，两边单看都合理、合起来才错（同 U6a 抓到的 PS 握手顺序那一族）。
    // U6b-3 起它**编译期不可表示**：`inbound::spawn` 要一个 `wire::HelloFlushed` 见证，
    // 而那个见证只能由 `write_and_flush_hello` 产出 —— 上一行拿到的就是它。
    // （此前是一条比较 `main.rs` 里两个字符串字节位置的机检，被一次普通的函数抽取绕过，已删。
    //  U8a-2a 顺带订正了本注释与 `src/doc/IPC-PROTOCOL.md` 里对那条已删机检的指名。）
    //
    // 应答走**独立通道**：出方向丢一条**内容帧**可恢复（行还在远端 jsonl 里），
    // ⚠ 而**状态增量帧**丢了别处没有 —— 那半靠 `Overflow.lost` 带身份让客户端重同步（audit-0805 F03），
    // 丢一条应答会让客户端永远等下去。混在一个通道里，实时行的洪峰会把应答挤掉。
    let (reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(inbound::REPLY_CHANNEL_CAPACITY);
    let inbound_task = inbound::spawn(tokio::io::stdin(), reply_tx.clone(), hello_flushed);

    // (c) Start the watcher reader; it returns the receiving half of the
    // bounded frame channel.
    let (rx, poke) = observe::watcher::spawn(agent_home, with_bg, tail_only, with_rbind_token);

    // (c2) **P4：SIGUSR1 = 「tmux 那边有事，赶紧重探一次」。**
    //
    // ★ **这一步必须先于任何 hook 安装落地** —— `SIGUSR1` 的**默认处置是终止进程**。
    // 先装 hook 再装处理器，等于给一个会自杀的后端装了自杀触发器。
    // 本轮（P4 backend 侧）刻意只做这一半：没有 hook 在发信号，它完全惰性。
    //
    // 为什么是信号而不是别的：原方案让 hook 追加事件日志、backend inotify 读增量，
    // **撞红线 I7「backend 只读」**（`readonly_guard` 当场拦下）。信号通路让后端的
    // 文件系统写归零，且会话名根本不经 shell ⇒ 那条引号/注入面直接消失。
    // 代价是信号无载荷且会合并 —— 靠「重探 + 与上一份快照差分」天然免疫。
    // P5：留一份给停机用（下面 select 结束后要显式通知 reader）。
    let poke_for_shutdown = poke.clone();
    // `K-P1`：处理器认的是一个**槽**而不是句柄（另一条载体上 watcher 会换人）。
    // 这条路上槽里永远只装这一个 —— 形状统一，实现只有一份。
    let slot: PokeSlot = std::sync::Arc::new(std::sync::Mutex::new(Some(poke)));
    let poke_task = spawn_sigusr1_task(slot);

    // (d) Run the stdout writer until the channel closes or a signal fires.
    tokio::select! {
        _ = writer_task(stdout, rx, reply_rx) => {
            tracing::info!("writer task ended (channel closed)");
        }
        _ = shutdown_signal() => {
            tracing::info!("shutdown signal received; exiting");
        }
    }
    // P5：**显式告诉 reader 停** —— 删掉 8s ticker 之后，reader 那边的
    // `sink.is_closed()` 复查再没有定期醒来的机会，只会一直阻塞在 `recv()`。
    // 漏这一句不会红任何测试（进程退出时线程随之消亡），所以它与删 ticker 是同一步。
    poke_for_shutdown.shutdown();
    poke_task.abort();
    inbound_task.abort();
    drop(reply_tx);
}

/// SIGUSR1 处理器要 poke 的那个 watcher 住的**槽**。
///
/// ★ **为什么是槽而不是句柄**〔`K-P1`〕：常驻那条载体上 watcher 会**换人** ——
/// 每接上一个客户端换一份新的（理由见 [`serve_listening`]），而处理器活得比任何一个 watcher 都长。
/// 拿句柄的话，第二个客户端连上之后 SIGUSR1 会去 poke 一个**已经退掉的** watcher：
/// 那不会报错，它只是**再也不响应 tmux hook 了** —— 又一个「假信号不报错，它只是一直说是」。
type PokeSlot = std::sync::Arc<std::sync::Mutex<Option<observe::watcher::WatcherPoke>>>;

/// **P4：SIGUSR1 = 「tmux 那边有事，赶紧重探一次」。**
///
/// ★ **这一步必须先于任何 hook 安装落地** —— `SIGUSR1` 的**默认处置是终止进程**。
/// 先装 hook 再装处理器，等于给一个会自杀的后端装了自杀触发器。
///
/// 为什么是信号而不是别的：原方案让 hook 追加事件日志、backend inotify 读增量，
/// **撞红线 I7「backend 只读」**（`readonly_guard` 当场拦下）。信号通路让后端的
/// 文件系统写归零，且会话名根本不经 shell ⇒ 那条引号/注入面直接消失。
/// 代价是信号无载荷且会合并 —— 靠「重探 + 与上一份快照差分」天然免疫。
#[cfg(unix)]
fn spawn_sigusr1_task(slot: PokeSlot) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigusr1 = match signal(SignalKind::user_defined1()) {
            Ok(s) => s,
            Err(e) => {
                // ★★ 说出来，而且**说真话**〔用@08-27 现打逮到，`K-P1` 回修第 7 处〕。
                //
                // 这句话原先逐字是「⇒ tmux hook 通路不可用，退回定时探测」——
                // 而**那条退路今天不存在**：`P5` 删掉 8s ticker 之后本进程零定时器
                //（`observe/watcher.rs` 那条 `P0b-Y2` 头注逐字：「`P5` 删掉 8s ticker 之后
                // backend **零定时器**，之后每一拍都靠事件」；`no_timer_guard` 钉着它）。
                //
                // ★ 病根不是打错字：**`P5` 删掉了一个构件，而替那个构件说话的散文散在别处，
                //   没人回去改。**然后它继续以权威口吻骗下一个读者 ——
                //   「不可用但有兜底」与「不可用而且没兜底」是两件完全不同的事。
                // ⇒ 说清**什么事会变得发现不了**（`control/tmux_hook.rs` 头注逐字：
                //   「多个 tmux 会话里杀掉其中一个」是**唯一**没有内核事件源的场景）
                //   与**下一步能做什么**。
                tracing::warn!(
                    "装不上 SIGUSR1 处理器（{e}）⇒ tmux hook 通路整条不可用，而且没有兜底：\
                     「多个 tmux 会话里杀掉其中一个」是唯一没有内核事件源的场景\
                     （pidfd 只看 server 进程、socket inotify 只看 server 生死），\
                     它会一直显示成还在，直到 tmux server 自己起停或别的事件把本进程推醒；\
                     P5 删掉 8s ticker 之后本进程零定时器，没有任何东西会自己醒过来。\
                     下一步：把本机后端停掉再起一次（monitor 的「停」「起」），仍装不上就重开 monitor"
                );
                return;
            }
        };
        tracing::info!("SIGUSR1 处理器已就位（tmux hook 通路的后端侧）");
        while sigusr1.recv().await.is_some() {
            // 锁毒化不该让 tmux 通路整条哑掉 ⇒ `into_inner` 取回内容再用。
            if let Some(p) = slot.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                p.poke();
            }
        }
    })
}

#[cfg(not(unix))]
fn spawn_sigusr1_task(slot: PokeSlot) -> tokio::task::JoinHandle<()> {
    let _ = slot;
    tokio::spawn(async {})
}

/// 一条**已经认证通过**的连接，等着被接成流。
///
/// 三样一起交出去不是为了打包好看：`HelloFlushed` 是**类型级见证**，
/// 它只能由 `write_and_flush_hello` 产出，而握手那一步就发生在这条连接自己身上
/// ⇒ 「hello 先于 reader」这条时序在换了载体之后**逐字保留**，仍然编译期不可表示。
struct Attached {
    reader: tokio::io::BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: BufWriter<tokio::net::tcp::OwnedWriteHalf>,
    hello_flushed: wire::HelloFlushed,
}

/// 一条连接的**握手**：先写 hello，再读一行 attach 请求，然后分档。
///
/// # 为什么 hello 写在分档**之前**
///
/// 否则「这台机上有没有一个长驻后端」只能从「`connect()` 成没成」推 ——
/// 而 TCP 的 backlog 会让**没人 accept 的口照样连得上** ⇒ 那是个「一直说是」的假信号。
/// 写在前面之后，那一问的答案是**读一行**，协议一个字节都不用加
/// （`shared/ccm:1182-1185` 自陈「后者今天没有便宜的问法」，说的就是这一格）。
///
/// # `candidate` 是什么
///
/// accept 那一刻用一次 `swap(true)` 决出来的：**赢的那条**才有资格要流，
/// 输的那条最多只能读 hello。这样「谁占着流」不靠事后检查，
/// 而是**一次原子操作**决定的 —— 两条连接同时握手也不会都拿到流。
/// 赢了却没能接成流（对端只想读 hello / token 不对 / 写不出去）⇒ **必须把牌还回去**。
async fn handshake_one(
    sock: tokio::net::TcpStream,
    hello: Frame,
    token: String,
    candidate: bool,
    busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    attached: tokio::sync::mpsc::Sender<Attached>,
) {
    use std::sync::atomic::Ordering;
    let release = || {
        if candidate {
            busy.store(false, Ordering::SeqCst);
        }
    };
    let (r, w) = sock.into_split();
    let mut w = BufWriter::new(w);
    let hello_flushed = match wire::write_and_flush_hello(&mut w, &hello).await {
        Ok(x) => x,
        Err(e) => {
            tracing::warn!("往一条新连接写 hello 失败（{e}）；关掉它");
            release();
            return;
        }
    };
    let mut r = tokio::io::BufReader::new(r);
    // ⚠ **有上限地读** —— 对端是同机任何进程，它完全可以一直发字节不发换行，
    // 而无界读就是无界堆分配（backend 侧为同一形栽过一次实测，见 `inbound.rs` 头注）。
    let line = match listen::read_capped_line(&mut r, listen::ATTACH_LINE_CAP).await {
        Ok(listen::HandshakeLine::Line(l)) => l,
        Ok(listen::HandshakeLine::Eof) => {
            // 「只读 hello 就走」那一档：对端读完就关，是**正常**结局，不出声。
            release();
            return;
        }
        Ok(listen::HandshakeLine::TooLong(bytes)) => {
            tracing::warn!("一条 attach 请求 {bytes} 字节还没换行 ⇒ 整行丢弃并关连接");
            release();
            return;
        }
        Err(e) => {
            tracing::warn!("读 attach 请求失败（{e}）；关掉这条连接");
            release();
            return;
        }
    };
    match listen::admit(listen::attach_verdict(&line, &token), !candidate) {
        listen::Admit::Refuse(reason) => {
            // **出声地拒**（照 `relay::serve` 那条 503 的形状：宁可拒绝，也不静默 FIN）。
            // 静默 FIN 会让对端只能靠「等了很久没动静」去猜，而那是猜不出原因的。
            let _ = listen::write_line(&mut w, &listen::refusal_line(reason)).await;
            tracing::warn!("拒绝一条 attach 请求：{reason}");
            release();
        }
        listen::Admit::Stream => {
            if listen::write_line(&mut w, listen::ATTACH_OK_LINE)
                .await
                .is_err()
            {
                release();
                return;
            }
            if attached
                .send(Attached {
                    reader: r,
                    writer: w,
                    hello_flushed,
                })
                .await
                .is_err()
            {
                release();
            }
        }
    }
}

/// `K-P1`：**常驻形态的接受循环** —— 一条流 + 不限次的「只读 hello 就走」。
///
/// # 空转期为什么还留着一个 watcher
///
/// 常驻真正买到的是两样东西，第二样就在这里：
/// ① `ccm` 那一问有答案了（连一次 + 读一行 hello）；
/// ② **`@ccm_sid` 打标那段时间窗关掉了** —— 写 `@ccm_sid` 的是**正在跑的** backend
///   （`observe/watcher.rs` inotify `sessions/` → `identity_tag::tag`），
///   而 `shared/ccm` 那条每会话每秒的身份 poller 已经被 `U-NP④` **整条删掉、不留轮询退路**。
///   monitor 没开着的时候若这里不看 `sessions/`，②就一格都没买到。
/// ⇒ 空转期照样起一个 watcher，帧**读出来就丢**（没人要），打标那一半照常发生。
///
/// # 接上客户端时为什么**换一份新的** watcher，而不是把空转那份的帧转给他
///
/// `watch_loop` 的 Phase 1 是一次**同步初扫**（`WalkDir` 走一遍 `sessions/` 逐个
/// `process_session_added`），之后靠 `ReaderState` 去重。⇒ 半路接进来的客户端
/// **拿不到那次初扫**，屏上就是空的。换一份新的 = 他拿到一次完整快照，
/// 而这不需要动 `watcher.rs` 一个字节。
/// ⚠ 代价如实记：换人期间 inotify 有一个**极短的重装窗口**，那段时间的文件事件不会补发；
/// 而 Phase 1 的初扫恰好覆盖「换人之前已经存在的会话」⇒ 漏的只有「正好落在那一瞬的新会话」。
///
/// # 它**不做**什么
///
/// **不重起自己**（`K14` 裁定：第一档，自愈单独立成 `K-P3`）。宿主不在时**没有监护**，
/// 这是**如实登记的降级**，而且那句话要在 UI 上说出来（`KPY4` 钉它），不许只写在这条注释里。
async fn serve_listening(
    port: u16,
    token: String,
    hello: Frame,
    agent_home: PathBuf,
    with_bg: bool,
    tail_only: bool,
    with_rbind_token: bool,
) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let addr = std::net::SocketAddr::new(listen::LOOPBACK, port);
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            let in_use = e.kind() == std::io::ErrorKind::AddrInUse;
            // ★★ **绑不上就退出，绝不自己换端口。**
            // 换端口 = 每台机 N 个后端，各自往 tmux server 装 `[50]` 槽位的全局 hook
            // 互相盖（`control/tmux_hook.rs::install_hooks`，**没有关掉它的开关**，
            // 载荷里烤着那一个后端的 pid+starttime）⇒ 比今天更糟。
            tracing::error!(
                "绑不上 {addr}（{e}）⇒ 退出。\n\
                 这个口上已经有东西了：宿主该**连上去读一行 hello 比对**，\n\
                 对不上就出声并拒绝，**不许静默复用**，更不许换个口再起一个。"
            );
            std::process::exit(if in_use {
                listen::EXIT_ADDR_IN_USE
            } else {
                listen::EXIT_BAD_LISTEN_CONFIG
            });
        }
    };
    tracing::info!("常驻监听口已就位：{addr}（一条流 + 不限次「只读 hello 就走」）");

    let busy = Arc::new(AtomicBool::new(false));
    let poke_slot: PokeSlot = Arc::new(std::sync::Mutex::new(None));
    let _poke_task = spawn_sigusr1_task(Arc::clone(&poke_slot));
    let set_slot = |p: Option<observe::watcher::WatcherPoke>| {
        *poke_slot.lock().unwrap_or_else(|e| e.into_inner()) = p;
    };

    let (mut idle_rx, mut idle_poke) = {
        let (rx, poke) =
            observe::watcher::spawn(agent_home.clone(), with_bg, tail_only, with_rbind_token);
        set_slot(Some(poke.clone()));
        (Some(rx), Some(poke))
    };

    // 容量 1：同一时刻最多只有一条连接能通过认证（`busy` 那次 `swap` 保证的）。
    let (attached_tx, mut attached_rx) = tokio::sync::mpsc::channel::<Attached>(1);
    let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(1);

    loop {
        tokio::select! {
            // ① 有连接进来。**每条连接一个 task** —— 握手不许挡住 accept，
            //    否则一条只想读 hello 的连接会把整条监听线堵住。
            accepted = listener.accept() => {
                match accepted {
                    Ok((sock, _peer)) => {
                        let candidate = !busy.swap(true, Ordering::SeqCst);
                        tokio::spawn(handshake_one(
                            sock,
                            hello.clone(),
                            token.clone(),
                            candidate,
                            Arc::clone(&busy),
                            attached_tx.clone(),
                        ));
                    }
                    Err(e) => tracing::warn!("accept 失败（{e}）；继续听"),
                }
            }
            // ② 有人认证通过 ⇒ 退掉空转那份 watcher，换一份新的给他，起流。
            Some(att) = attached_rx.recv() => {
                if let Some(p) = idle_poke.take() {
                    p.shutdown();
                }
                idle_rx = None;
                let Attached { reader, writer, hello_flushed } = att;
                let (rx, poke) = observe::watcher::spawn(agent_home.clone(), with_bg, tail_only, with_rbind_token);
                set_slot(Some(poke.clone()));
                // 应答走**独立通道**：出方向丢一条内容帧可恢复，丢一条应答会让客户端永远等下去。
                let (reply_tx, reply_rx) =
                    tokio::sync::mpsc::channel::<Frame>(inbound::REPLY_CHANNEL_CAPACITY);
                let mut inbound_task = inbound::spawn(reader, reply_tx.clone(), hello_flushed);
                let done = done_tx.clone();
                tracing::info!("一条流已接上（认证通过）");
                tokio::spawn(async move {
                    // ★★ **两个事件都算「客户端走了」，缺一个就会把那一档永久占住。**
                    //
                    // ⚠ 这一格是 `K-P1` 的 e2e **实测**逼出来的，不是设计出来的：
                    // 只等 `writer_task`（它靠**写**拿到错误才结束）时，
                    // 一个**空闲**的后端根本没有东西可写 ⇒ 上一个 monitor 退了之后
                    // 那张牌**永远不还回来** ⇒ 下一个 monitor 拿到 `stream-busy`
                    // ⇒ 「换个 monitor 重开就没有本机后端了」。实测：等满 50×20ms 仍是 busy。
                    //
                    // ⇒ 再认一个事件：**入方向读到 EOF**（客户端关了它的写半边 / 进程没了）。
                    // 那与 stdio 那条载体上「stdin EOF」是同一个事实，只是这条载体上它**必须**被当真：
                    // socket 的对端关了就是走了，而 stdio 那边刻意对写端关闭不敏感
                    //（那是为了不让一次误关掉整个 backend —— 两条载体的取舍不同，写清楚）。
                    tokio::select! {
                        _ = writer_task(writer, rx, reply_rx) => {
                            tracing::info!("流结束：写不出去了（客户端走了）");
                        }
                        _ = &mut inbound_task => {
                            tracing::info!("流结束：入方向读到 EOF（客户端关了它的写半边）");
                        }
                    }
                    // P5：**显式告诉 reader 停** —— 没有 ticker 之后它只会一直阻塞在 `recv()`。
                    poke.shutdown();
                    inbound_task.abort();
                    drop(reply_tx);
                    let _ = done.send(()).await;
                });
            }
            // ③ 流结束（客户端走了）⇒ 把牌还回去，回到空转。
            Some(()) = done_rx.recv() => {
                tracing::info!("流结束 ⇒ 回到空转：口仍在听，sessions/ 仍在看");
                busy.store(false, Ordering::SeqCst);
                let (rx, poke) = observe::watcher::spawn(agent_home.clone(), with_bg, tail_only, with_rbind_token);
                set_slot(Some(poke.clone()));
                idle_rx = Some(rx);
                idle_poke = Some(poke);
            }
            // ④ 空转期把 watcher 的帧读出来丢掉。**没人要它们**，
            //    但不读的话 10_000 容量的通道会填满并开始记 `Overflow`，
            //    而那本账是给「有客户端在听」那一档记的 —— 空转期记它没有意义。
            f = async { idle_rx.as_mut().expect("上面刚判过 is_some").recv().await }, if idle_rx.is_some() => {
                if f.is_none() {
                    tracing::warn!("空转期的 watcher 自己结束了 ⇒ 这台机的 @ccm_sid 打标停了");
                    idle_rx = None;
                    idle_poke = None;
                    set_slot(None);
                }
            }
        }
    }
}

/// The stdout writer half of the §5.4 split: drain frames and write one wire
/// line each, flushing per frame so a connected client sees them promptly.
///
/// Awaiting `recv()` here is what back-pressures the bounded channel when the
/// SSH pipe is slow; that back-pressure never reaches the inotify reader.
/// 排空两条通道写 stdout。
///
/// **`biased` + 应答在前**：出方向的实时行可以有洪峰（`CHANNEL_CAPACITY` 是 10_000），
/// 公平轮询会让应答排在一万行后面，客户端那头看起来就是「命令没反应」。
/// 应答量极小（一条命令一两帧），优先它不会饿死行。
///
/// `reply_rx` 永远不会返回 `None`（`main` 自己留着一个 sender 到进程结束），
/// 所以这个 select 不会退化成 `None` 忙转。
async fn writer_task<W: tokio::io::AsyncWrite + Unpin>(
    mut out: W,
    mut rx: tokio::sync::mpsc::Receiver<Frame>,
    mut reply_rx: tokio::sync::mpsc::Receiver<Frame>,
) {
    // ★ 应答优先，但**有预算**。
    //
    // 第一版是无条件 `biased` + 应答在前，注释写着「应答量极小，优先它不会饿死行」——
    // **那个前提由不可信输入决定，不成立**。D 审计端到端实测（客户端只是往 stdin 灌
    // `{"id":"n","cmd":"n"}`）：500ms 内应答 70 789 条、出方向实时行 **4 条**，队列里一直排着。
    //
    // 机理是闭环的：读循环在应答通道满时阻塞（`send` 是 `.await`），于是通道**恒满**；
    // `biased` 每次都命中 `reply_rx`，`rx` 永远轮不到。生产里出方向是 10 000 容量，
    // 会先堆满再 `Overflow` 丢实时行 —— 一行命令就能让远端会话看起来「卡住」。
    //
    // 现在：连发 `REPLY_BURST` 条应答之后**强制让位一次**给出方向。
    // 仍然偏向应答（正常场景一条命令一两帧，够不到预算），但偏置是**有界**的。
    const REPLY_BURST: u32 = 8;
    let mut burst = 0u32;
    loop {
        let frame = if burst < REPLY_BURST {
            tokio::select! {
                biased;
                Some(f) = reply_rx.recv() => { burst += 1; f }
                f = rx.recv() => match f {
                    Some(f) => { burst = 0; f }
                    None => return, // 出方向通道关了 = 寿终
                },
            }
        } else {
            burst = 0;
            tokio::select! {
                biased;
                f = rx.recv() => match f {
                    Some(f) => f,
                    None => return,
                },
                Some(f) = reply_rx.recv() => f,
            }
        };
        if let Err(e) = write_frame(&mut out, &frame).await {
            // Broken pipe (client gone) is the normal end-of-life; stop quietly.
            tracing::warn!("stdout write failed ({e}); stopping writer");
            return;
        }
        if let Err(e) = out.flush().await {
            tracing::warn!("stdout flush failed ({e}); stopping writer");
            return;
        }
    }
}

/// Serialize one frame to its wire line and write it (no flush).
async fn write_frame<W: tokio::io::AsyncWrite + Unpin>(
    out: &mut W,
    frame: &Frame,
) -> std::io::Result<()> {
    let line =
        to_line(frame).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    out.write_all(line.as_bytes()).await
}

/// 解析会话数据根。
///
/// ⚠ `S3` 把**怎么解析**搬进了 `agents/claudecode/paths.rs`（环境变量名与目录名是
/// Claude 的知识）。这里只剩"去问适配层" —— 今天后端只服务一种 agent，所以是写死的一句。
///
/// ⚠〔`S5` 08-14 订正〕原注这里写着「`S5` 落地时它会变成按 kind 取」——**`S5` 没有那么做，
/// 而且这条订正比原话更要紧**：本函数要的是**恒定**答得出的那个 home（流式 watcher 与
/// 所有一次性子命令都拿它当根），而 `agents::visible_homes()` 只报**看得见**的那些
///（home 目录不存在就一条都不报）。两者语义不同 ——
/// 把这里换成"按 kind 取"会让 `~/.claude` 还没建出来的新机器上后端直接失根，
/// 而它原本是能正常起来、等 inotify 等到第一个会话的。
/// ⇒ 真正会变的是**别处**：`main` 里 `homes:` 那一行（见上）。归 `S6`/`L2` 的接口那轮再看。
fn resolve_agent_home() -> PathBuf {
    agents::claudecode::paths::resolve_home()
}

/// Resolve when a SIGTERM or SIGINT (Ctrl-C) is received, for clean shutdown.
///
/// On Unix this listens for both SIGTERM and SIGINT; on other platforms it
/// falls back to Ctrl-C only (sufficient for the Windows smoke).
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("failed to install SIGTERM handler: {e}");
                // Fall back to Ctrl-C only so we still shut down on SIGINT.
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        let mut sigint = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("failed to install SIGINT handler: {e}");
                let _ = sigterm.recv().await;
                return;
            }
        };
        tokio::select! {
            _ = sigterm.recv() => {}
            _ = sigint.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
