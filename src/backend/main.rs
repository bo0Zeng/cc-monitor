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

// 🔴 模块声明、`PROTO_VERSION`、身份块（`BUILD_ID` ＋ 戳）、`SUBCOMMANDS`
//    都搬进了 `lib.rs`，理由逐字写在那份文件的头注里（一句话：**in-process 那条路没有
//    这个 `main.rs`**，身份与模块跟着它一起消失）。**本文件只留分派**（规格）。
// ⚠ 用 glob 而不是逐项列 —— 本拍是**纯机械搬家**，逐项列会让 diff 里混进
//    「哪些项对外可见」这个**语义**决定，那是另一件事（`4b` 定 API 面时再收窄）。
use cc_monitor_backend::control::cli_control;
use cc_monitor_backend::faces::read_face;
use cc_monitor_backend::stream::{inbound, listen, tap, topic_body, topic_hook, wire};
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

// 远端 musl 版换分配器：musl 自带的那个放掉的内存不还给系统（理由与读数住 `Cargo.toml` 那一段）。
// 只在本二进制定，不在库面定：本机 GUI 进程把后端当库链进去，库里定会连壳一起换。判据 `allocator_guard`。
#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() {
    // 帧里的小成品（`changed.body`）照主题表现算：装进发端调的那一层（理由住 `stream/topic_body.rs` 头注）。
    topic_hook::install(topic_body::body_now);
    // ★★ `K-R48`（09-11）：**当 `ccm` 用的那一趟，在这里就整条分出去。**
    //
    // 〔用@09-11 `K33`〕「后端**只有一个**，**不要有什么 bash 脚本**，**不要有什么单独的 ccm**。」
    // ⇒ 终端里敲的 `ccm` 就是本二进制；被叫成一段配置的名字（`~/.cc-monitor/bin/<名>` 那条链接）⇒ 等同 `ccm @<名>`。
    //
    // 🔴 **三个「必须排在前面」，一个都不是排版**：
    //   ① 排在 `tracing_subscriber` 之前 —— 一次性模式的 stderr 是给人看的，
    //      混进后端的日志行就把「正常路径一个字都不说」这条契约破了；
    //   ② 排在 `split_stream_flags` 之前 —— 那一步会把 `--with-bg` / `--tail-only`
    //      从 argv **任意位置**剥掉，而 `ccm -- --tail-only` 里那个要原样透传给 agent；
    //   ③ 排在 `resolve_agent_home()` 之前 —— 一次性模式不必去解析 agent 家目录。
    // 分流只经 `control::ccm::route`：当后端用时，后端认的 argv 是它交回来的那一串（去掉了打头的 `--`）。
    // argv 只在这里取一次：分流看 `[1..]`，ccm 那一趟要的「我被怎么叫的」由这里交（`ccm::run` 的 `process_argv`）。
    // 起第一个子进程之前：各家「我是哪个会话」的变量一律不往下传（常驻后端若在某个会话里起，它起的会话不许认错父）。
    agents::install_child_env_filter();
    let process_argv: Vec<String> = std::env::args().collect();
    let backend_args: Vec<String> = {
        let rest: Vec<String> = process_argv.iter().skip(1).cloned().collect();
        match control::ccm::route(process_argv.first().map_or("", String::as_str), &rest) {
            // resume 判「在别处跑着」用观测层那一份扫描（control 不引用 observe ⇒ 由入口注入）。
            control::ccm::Entry::Ccm(ccm_args) => {
                std::process::exit(control::ccm::run(&ccm_args, &process_argv, |dir| {
                    let home = agent_home(dir, false);
                    observe::watcher::running_sessions(&home)
                }))
            }
            control::ccm::Entry::Backend(a) => a,
        }
    };

    // issue #16：带参数 = 一次性历史查询模式，干完即退，不进流式协议。
    // 旧后端不认参数会照常发 hello 进流模式——monitor 以"首行是 hello 帧"
    // 识别旧版并提示升级（优雅降级，无协议版本协商负担）。
    let args: Vec<String> = backend_args;
    // Batch7-F24/Batch8-F25：流模式 flag 集合，先剥离再判一次性查询模式
    // （否则误入 query 分支——INVARIANT §26）。纯函数化供单测（审计 D）。
    let (args, wants) = split_stream_flags(args);
    init_tracing(is_query_mode(&args));

    let agent_home = resolve_agent_home();

    if is_query_mode(&args) {
        // 看的那一台的时区（`--tz`，剥流旗标那一步剥出来的）：一次性回包里的「几点」按它写。
        let z = &wants.tz;
        // 一次性查询模式：--search 全文搜索（#28）/
        // --resolve advisor（backend-04，读 stdin ResumeSpec→stdout CommandPlan），其余走历史查询（#16）。
        let code = match args.first().map(String::as_str) {
            // P4b：hook 子进程走这条 —— 校验身份后给后端发 SIGUSR1，**不碰文件系统**。
            Some("--tmux-notify") => control::tmux_hook::notify(&args),
            // `K-R87`：起一个到点自己会死的一次性会话。看门狗是**外部进程**，
            // 不在本 crate 的源码文本里 —— 零定时器铁律的人群逐字排除「被起进程的行为」。
            Some("--search") => observe::search_query::run(&agent_home, &args),
            Some("--resolve") => control::resolve_query::run(&agent_home, &args),
            // G2（branch-anywhere）：从指定消息处分叉出一个新会话文件。
            // **backend 唯一的写盘入口**，护栏白名单层单独盯着它（readonly_guard）。
            Some("--fork-session") => control::fork_write::run(&agent_home, &args, z),
            // `--relay`（独立的中转进程）那一臂删了：中转只住常驻后端进程里（本机远端同形）。
            // 远端常驻后端的起 · 找 / 停（`control/resident.rs` 头注）。
            // 远端中转住进远端常驻后端 —— 起它时交中转口（与本机宿主交的同一个常量）。
            Some("--resident-ensure") => control::resident::ensure(&args[1..]),
            Some("--resident-attach") => control::resident::attach().await,
            Some("--resident-stop") => control::resident::run_stop(&args[1..]),
            // `--dial` 那条拨号代理臂**删了**：拨号挪进本机那一个常驻后端，经流上的链路
            // （`link-*` 四条，`dial/link.rs`）做 —— 不再每条链路起一个进程。
            // ★ 这几个字面量必须与 `observe::accounts_query::run` 自己认的子命令**完全一致**。
            // v3.4.0 出过一次事故：`--account-trust-zero` 在 accounts_query 里实现完整，
            // 但这里漏列 ⇒ 落进下面的 `_` 臂走历史查询 ⇒ `unknown argument` + exit 2，
            // 而 monitor 的账号 0 路径**真的在发这条命令**。
            // 测试当时抓不到，是因为它们直接调 `observe::accounts_query::run`、**绕过了本处调度**。
            // 现由 `observe::accounts_query::tests::main_dispatches_every_subcommand_we_handle` 钉住。
            Some("--list-accounts")
            | Some("--session-accounts")
            | Some("--account-trust")
            | Some("--account-trust-zero") => observe::accounts_query::run(&agent_home, &args, z),
            // ★ P4d：控制面的 CLI 入口。**这条臂刻意不写命令字面量** ——
            // 认哪些 flag 由 `cli_control::spec_for` 从 `inbound::REGISTRY` 派生，
            // 于是「帧面加一条命令」不需要回来改这里。上面 `--resolve` 那条臂**故意留在前面**：
            // 它的信封与仓外 aterm 冻结在 2026-07-18，走原路一个字节都不动
            // （两条路的输出实为同一个 `CommandPlan`，但冻结的契约不拿「实际上一样」去赌）。
            // ⚠ **必须写成单行臂**：`argv_table_guard::every_dispatch_arm_actually_calls_an_implementation`
            // 按**行**取 `=>` 右边的臂体，块体臂会被抽成 `""` 当场红（实测）。
            // 那条约束是保守的（宁可假红），照它写就是了 —— 单行形式下它钉的
            // 「臂体是一次真调用」也确实成立。
            Some(f) if cli_control::handles(f) => cli_control::run(&args, z).await,
            _ => observe::history_query::run(&agent_home, &args, z),
        };
        std::process::exit(code);
    }

    // 一次性查询已 exit；到此必是流模式。agent_home 日志放此（审计 correctness-重要①：
    // --resolve/一次性查询模式 stderr 只承载结构化错误/查询结果，不掺 info——兑现协议 v1 §3
    // 「错误 exit2 + stderr 纯 {code,message} JSON」，客户端可整段 JSON-parse stderr）。
    tracing::info!("agent_home = {}", agent_home.display());

    // **脱离常驻那条载体的 stderr 落盘**：宿主交了路径才接（monitor 只在起脱离那条时交），先抢口、抢到了才接（[`claim_then_log`]）。
    //   放在一次性子命令全部 `exit` 之后：它们的 stderr 是给人 / 给 JSON 解析看的，不动。放在起中转之前：接上之后这一行起的
    //   每一句（中转那两句、host key 警告、panic）都落进那份文件。
    let env = |k: &str| std::env::var(k).ok();
    let (listening, installed) =
        match claim_then_log(&env, control::resident::account_home().as_deref(), || {
            stderr_log::install_from_env(&env)
        })
        .await
        {
            Ok(got) => got,
            Err(code) => std::process::exit(code),
        };
    tracing::info!("{}", installed.said());
    // 装上了 ⇒ 告诉只读面那份文件在哪（`backend-log`，机器页「日志」经它取回来看）。
    if let stderr_log::Installed::Logging(p) = &installed {
        read_face::note_backend_log(p.clone());
    }

    // **中转 ＋ 上游选择住常驻后端这个进程**：宿主交了端口才开
    //   （monitor 起本机后端时交；远端由 `--resident-ensure` 起常驻子进程时交；测试连接探针那一趟没人交 ⇒ 不开）。
    //   放在起载体之前：两条载体（stdio / 常驻监听口）一样要。起不来只出声、不拖垮后端
    //   —— 理由与形状住 `relay::listen::host` 的头注。中转线程随本进程生、随本进程死。
    // 〔升级那一跳，跨过 4.1.x 之后删〕常驻载体：先停掉还在跑的旧版（它占着中转口），再接中转。
    //   停它要等（至多宽限期），那一趟放进一条线程，不挡套接字开门；没有旧版 ⇒ 当场接中转。
    let legacy = listening
        .as_ref()
        .and_then(|_| control::resident::here())
        .filter(|dh| relay_route_core::legacy_listen_pid_for(dh).exists());
    let off_thread = legacy.is_some();
    let start_relay = move || {
        if let Some(said) = legacy.as_deref().and_then(control::resident::retire_legacy) {
            tracing::info!("{said}");
        }
        tracing::info!(
            "{}",
            accounts::upstream_select::host_relay(faces::rotation_face::library())
        );
    };
    if off_thread {
        std::thread::spawn(start_relay);
    } else {
        start_relay();
    }
    // 全文搜索的常驻索引起来就后台建（两条载体都要；一次性线程，建完就退）。
    observe::search_query::warm_in_background(agent_home.clone());
    // 历史清单的缓存同样起来就后台热（低优先级的一次性线程）：第一次打开历史页 / 按 sid 开查看窗不再现扫整台。
    observe::history_query::warm_listing_in_background(agent_home.clone());
    // 别名清单换了存法（配置文件 `profiles.toml`）：这台的配置文件还不在、旧形状的别名文件在 ⇒ 起来时一次性转过去（迁完就不再看旧文件）。
    assets::aliases::migrate_here();

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
    // 判定住 [`listen::mode_from`]（**纯函数**，所以「只写了一半」那两条错误支都测得到），在 [`claim_then_log`] 里定、抢口。

    // (b) Emit the Hello handshake FIRST, flushed, before anything else.
    let hello = build_hello(&agent_home);

    // 别的进程（命令行 · quota-warm · AI 照 skill 调）写了这台的轮换 ⇒ 这一路也推 `changed {rotation}`（广播通道是进程内的）。
    inbound::watch_rotation();
    match listening {
        None => run_over_stdio(hello, agent_home, wants).await,
        Some((listener, held)) => {
            // 这台账号库里各号共用的用户级 MCP：常驻那条载体上盯各号的配置文件，一有动静同步一趟（一次性的 stdio 那条不起）。
            inbound::watch_account_mcp();
            // 停机信号只挂**一次**（不在 accept 循环里每轮重装一个 SIGTERM 处理器）。
            // 收到之后监听口随 `serve_listening` 一起丢（不再接新连接）；已接上的那条流是独立任务，
            //   排空期间照常写应答，新来的阻塞命令回 `shutting_down`（`inbound::exit_after_drain`）。
            let stop = inbound::shutdown_listener();
            tokio::select! {
                () = serve_listening(
                    listener,
                    held,
                    hello,
                    agent_home,
                    wants,
                ) => {}
                _ = stop => {
                    tracing::info!("shutdown signal received; exiting");
                }
            }
            inbound::exit_after_drain("常驻后端收到停机信号", None::<std::future::Ready<()>>).await
        }
    }

    // ★ **必须显式 exit，不能让 runtime 自然 drop。**那一下今天住 `inbound::exit_after_drain`
    //   （三个退出口共用）；下面这段理由原样留着，它说的是那一下为什么必须是 `exit`。
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
    // 🪦原话「为什么 exit 是安全的：**流模式后端没有任何待落盘状态** —— 它只读；
    // 唯一的写盘入口 `control/fork_write.rs` 在一次性查询模式，那条路早就 exit 了」—— RW1 / FW5 / F9c 之后**已假**：
    // `files-*` 写面、`files-commit-*`、`exit-policy-set`、资产目录、`apikey-key-set`、`history-annotate` 全在流模式里跑，
    // 当场 `exit` 会把正在写的那一条连线程带走（E §E2）。⇒ 今天 exit 之前先排空停不下来的那一档（`inbound::exit_after_drain`）。
    // stdout 不欠 flush 那半仍成立：`writer_task` 每帧写完即 flush。
    // 两条载体都不返回（类型上是 `!`，各自在收场处 exit）⇒ 这里没有第二个 exit。
}

/// 造那一帧 hello。**抽出来是因为两条载体都要发它**，而它必须只有一份 ——
/// 两份 hello 会各自漂，而这一帧是仓外 aterm 按精确字节在读的东西。
fn build_hello(agent_home: &std::path::Path) -> Frame {
    Frame::Hello {
        v: PROTO_VERSION,
        build_id: BUILD_ID.to_string(),
        host_arch: std::env::consts::ARCH.to_string(),
        // ⚠ **左边的字段名与右边的变量名刻意不一致**，这不是笔误：
        // 左边 `claude_dir` 是 **wire 字段**，冻结兼容（真在线上、仓外 aterm 在读），
        // 登记在 `agent_boundary_guard::FROZEN_COMPAT`，带解锁条件，**不许改名**；
        // 右边 `agent_home` 是**仓内的参数名**，`S4b` 已把它从 `claude_dir` 改过来
        //（通用层不该在标识符里叫得出某个 agent 的名字 —— `D3` 的同一条道理往仓内推）。
        // ⇒ 这一行正是两条纪律的交界处：**字段名归契约，变量名归架构**。
        claude_dir: agent_home.to_string_lossy().into_owned(),
        // `S4`（`D3`）：wire 面已换成通用的 `homes`（`[{agent_kind, path}]`）。
        //
        // ★★**这一行是空表，但已经不是因为"做不到"了。**
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
        commands: inbound::command_names()
            .into_iter()
            .map(str::to_string)
            .collect(),
        // ★★〔NET2 真填〕握手帧第四条面「我做得到什么」：这台机器上接得下却做不到的命令（tmux · unix 权限位两维，
        // 表从 `inbound::REGISTRY` 的 `codes` 派生）。是**提示不是闸门**（读数是握手那一刻的，`wire.rs` 那个字段头注口径③）。
        // 仓外 aterm 不读这个字段（只读核过 `DaemonTransport.kt::parseHello`，未知字段忽略）；有 tmux 的 unix 机器上恒空 ⇒ 字节不变。
        // 钉它的：`main_fourth_face_tests::production_hello_fills_unavailable_from_this_machine`。
        unavailable: unavailable_here(),
        // 回显起我的宿主交来的那几格（名单 `wire::HOST_ECHO_ENVS`）；一格都没交 ⇒ 省略、线上字节不变。
        host_env: wire::host_env_from(|name| std::env::var(name).ok()),
        // 撤不动的那几条（阻塞档），从命令表派生。
        uncancellable: inbound::uncancellable(),
    }
}

/// 今天那条路：**stdin/stdout 一对管道**。宿主一退读端就断，backend 153ms 内自己走。
///
/// ⚠ 本函数体是 `K-P1` 之前 `main()` 的那一段**原样搬过来的**，一行行为都没改 ——
/// 常驻是**加一条载体**，不是把这条改掉。改这一段之前先问：另一条载体要不要跟着改？
async fn run_over_stdio(hello: Frame, agent_home: PathBuf, wants: StreamWants) -> ! {
    let mut stdout = BufWriter::new(tokio::io::stdout());
    // U6b-3：写 + flush 一步到位，**并拿到 `HelloFlushed` 见证**。
    // 那个见证是 `inbound::spawn` 的必填参数 ⇒「reader 抢在 Hello 之前起来」
    // 变成编译期不可表示（此前靠一条比较字节位置的机检，被普通函数抽取绕过）。
    let hello_flushed = match wire::write_and_flush_hello(&mut stdout, &hello).await {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("failed to write/flush hello frame: {e}");
            // 一条命令都还没收过 ⇒ 没有可排空的；走同一个收场口只为「exit 只有一处」。
            inbound::exit_after_drain("hello 写不出去", None::<std::future::Ready<()>>).await
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
    // 运行簿：这条连接的 watcher 写、tap 那一路的流归位读（一条连接一本）。
    let book = observe::runs::RunBook::shared();
    let tz = wants.tz.clone();
    let (rx, poke) = observe::watcher::spawn(agent_home, wants, book.clone());

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
    // `K-P1`：处理器认的不是句柄，是 watcher 的名单（`observe::watcher::poke_all`；另一条载体上 watcher 会换人）。
    let poke_task = spawn_sigusr1_task();

    // (d) Run the stdout writer until the channel closes or a signal fires.
    // 收信号那一支**不丢写者**：排空期间它照常把在飞命令的最终应答写回去（`inbound::exit_after_drain`）；
    //   入方向 reader 也留着 —— 新来的阻塞命令要有人回它一句 `shutting_down`。
    let stop = inbound::shutdown_listener();
    // 这条流连接的 tap 接收端（中转抄出来的 SSE 事件，最低优先、可丢）。
    let tap_rx = tap::attach(book, tz.clone());
    let writer = writer_task(stdout, rx, reply_rx, tap_rx, tz);
    tokio::pin!(writer);
    let signalled = tokio::select! {
        _ = &mut writer => {
            tracing::info!("writer task ended (channel closed)");
            false
        }
        _ = stop => {
            tracing::info!("shutdown signal received; exiting");
            true
        }
    };
    // P5：**显式告诉 reader 停** —— 删掉 8s ticker 之后，reader 那边的
    // `sink.is_closed()` 复查再没有定期醒来的机会，只会一直阻塞在 `recv()`。
    // 漏这一句不会红任何测试（进程退出时线程随之消亡），所以它与删 ticker 是同一步。
    poke_for_shutdown.shutdown();
    poke_task.abort();
    if signalled {
        inbound::exit_after_drain("收到停机信号", Some(writer)).await
    }
    // 写者断了 = 对端走了（SSH 断 / monitor 退）：入方向不会再有东西来，收掉它再排空。
    inbound_task.abort();
    drop(reply_tx);
    inbound::exit_after_drain("对端走了（写不出去）", None::<std::future::Ready<()>>).await
}

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
///
/// ★ **戳的是名单，不是句柄**：常驻那条载体上 watcher 会**换人**（每接上一个客户端换一份新的，
/// 理由见 [`serve_listening`]），而处理器活得比任何一个 watcher 都长。名单只有一个家：
/// `observe::watcher` 的在跑名单（`spawn` 登记、退出即摘；`resync` 也按它找人）。
fn spawn_sigusr1_task() -> tokio::task::JoinHandle<()> {
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
            observe::watcher::poke_all();
        }
    })
}

#[cfg(not(unix))]
fn spawn_sigusr1_task() -> tokio::task::JoinHandle<()> {
    tokio::spawn(async {})
}

/// 一条**已经认证通过**的连接，等着被接成流。
///
/// 三样一起交出去不是为了打包好看：`HelloFlushed` 是**类型级见证**，
/// 它只能由 `write_and_flush_hello` 产出，而握手那一步就发生在这条连接自己身上
/// ⇒ 「hello 先于 reader」这条时序在换了载体之后**逐字保留**，仍然编译期不可表示。
struct Attached {
    reader: tokio::io::BufReader<tokio::io::ReadHalf<own_chan::Stream>>,
    writer: BufWriter<tokio::io::WriteHalf<own_chan::Stream>>,
    hello_flushed: wire::HelloFlushed,
    /// 这条连接要的流模式旗标（attach 行里的 `flags`）；`None` = 用进程起参那一份。
    flags: Option<StreamWants>,
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
/// 多客户：attach 行形状对就接成流，不再有「谁拿到那一张牌」（原 `busy.swap` 那一格）；门在 accept 那一下（对端 uid）。
async fn handshake_one(
    sock: own_chan::Stream,
    hello: Frame,
    attached: tokio::sync::mpsc::Sender<Attached>,
) {
    let (r, w) = tokio::io::split(sock);
    let mut w = BufWriter::new(w);
    let hello_flushed = match wire::write_and_flush_hello(&mut w, &hello).await {
        Ok(x) => x,
        Err(e) => {
            tracing::warn!("往一条新连接写 hello 失败（{e}）；关掉它");
            return;
        }
    };
    let mut r = tokio::io::BufReader::new(r);
    // ⚠ **有上限地读** —— 对端完全可以一直发字节不发换行，
    // 而无界读就是无界堆分配（backend 侧为同一形栽过一次实测，见 `stream/inbound/` 头注）。
    let line = match listen::read_capped_line(&mut r, listen::ATTACH_LINE_CAP).await {
        Ok(listen::HandshakeLine::Line(l)) => l,
        Ok(listen::HandshakeLine::Eof) => {
            // 「只读 hello 就走」那一档：对端读完就关，是**正常**结局，不出声。
            return;
        }
        Ok(listen::HandshakeLine::TooLong(bytes)) => {
            tracing::warn!("一条 attach 请求 {bytes} 字节还没换行 ⇒ 整行丢弃并关连接");
            return;
        }
        Err(e) => {
            tracing::warn!("读 attach 请求失败（{e}）；关掉这条连接");
            return;
        }
    };
    // flags 写坏了与整行坏了同一格拒（`malformed-attach`）。
    let flags = listen::attach_flags(&line);
    let verdict = match (listen::attach_verdict(&line), &flags) {
        (listen::Verdict::Attach, Err(())) => listen::Verdict::Malformed,
        (v, _) => v,
    };
    match listen::admit(verdict) {
        listen::Admit::Refuse(reason) => {
            // **出声地拒**（照 `relay::serve` 那条 503 的形状：宁可拒绝，也不静默 FIN）。
            // 静默 FIN 会让对端只能靠「等了很久没动静」去猜，而那是猜不出原因的。
            let _ = listen::write_line(&mut w, &listen::refusal_line(reason)).await;
            tracing::warn!("拒绝一条 attach 请求：{reason}");
        }
        listen::Admit::Stream => {
            if listen::write_line(&mut w, listen::ATTACH_OK_LINE)
                .await
                .is_err()
            {
                return;
            }
            let _ = attached
                .send(Attached {
                    reader: r,
                    writer: w,
                    hello_flushed,
                    flags: flags.unwrap_or(None),
                })
                .await;
        }
    }
}

/// 选载体、抢门牌；抢到了（或这条载体不用抢）才接 stderr 落盘（`install`）。
///
/// 那两份日志属于在听的那一个：已有常驻后端时，后起的这一个若先接，就把在跑那一个的日志滚走（再来一个就删掉）。
/// ⇒ 配置不成立 / 沙箱跑却要占真家 / 抢不到锁都不接，话只落自己原来的 stderr，退出码交回 `main` 退（一条命令都还没收，没有可排空的）。
async fn claim_then_log(
    env: &dyn Fn(&str) -> Option<String>,
    account_home: Option<&std::path::Path>,
    install: impl FnOnce() -> stderr_log::Installed,
) -> Result<
    (
        Option<(own_chan::Listener, own_chan::Held)>,
        stderr_log::Installed,
    ),
    i32,
> {
    claim_then_log_as(
        control::resident::unsupported_here(),
        env,
        account_home,
        install,
    )
    .await
}

/// [`claim_then_log`] 的里子：「本平台有没有常驻」那一格是入参（`unsupported` = 没有时那一句），判据两形都量得到。
async fn claim_then_log_as(
    unsupported: Option<String>,
    env: &dyn Fn(&str) -> Option<String>,
    account_home: Option<&std::path::Path>,
    install: impl FnOnce() -> stderr_log::Installed,
) -> Result<
    (
        Option<(own_chan::Listener, own_chan::Held)>,
        stderr_log::Installed,
    ),
    i32,
> {
    let mode = match listen::mode_from(env) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("常驻配置不成立 ⇒ 拒绝起：{e}");
            return Err(listen::EXIT_BAD_LISTEN_CONFIG);
        }
    };
    if mode == listen::Mode::Stdio {
        return Ok((None, install()));
    }
    // 本平台没有常驻 ⇒ 先说「不支持」（与 `--resident-ensure` 同一句），家与门牌目录一个都不建。
    if let Some(why) = unsupported {
        tracing::error!("{why}");
        return Err(listen::EXIT_BAD_LISTEN_CONFIG);
    }
    let Some(dh) = control::resident::data_home_from(env) else {
        tracing::error!("找不到这台的家 ⇒ 拒绝起常驻");
        return Err(listen::EXIT_BAD_LISTEN_CONFIG);
    };
    // 〔台架防真家〕沙箱跑却要占本账号真家目录里的门牌 ⇒ 不起（10-08 一路台架漏清环境，改写过真家里的门牌）。
    if let Some(why) = control::resident::sandbox_refusal(env, &dh, account_home) {
        tracing::error!("{why}");
        return Err(listen::EXIT_BAD_LISTEN_CONFIG);
    }
    match control::resident::claim(&dh) {
        control::resident::Claim::Listening(l, held) => Ok((Some((l, held)), install())),
        control::resident::Claim::Held => {
            // ★★ **抢不到锁就退出，绝不换个地方再起一个**（换地方 = 每台机 N 个后端，中转口与全部 SSH 各 N 份）。
            tracing::error!(
                "这台的家里已有一个常驻后端在听 ⇒ 退出。宿主该**连上去读一行 hello 比对**，对不上就出声并拒绝。"
            );
            Err(listen::EXIT_ADDR_IN_USE)
        }
        control::resident::Claim::Failed(why) => {
            tracing::error!("{why}");
            Err(listen::EXIT_BAD_LISTEN_CONFIG)
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
    listener: own_chan::Listener,
    // 目录独占锁：随这个函数（＝ 常驻载体）活着。
    _held: own_chan::Held,
    hello: Frame,
    agent_home: PathBuf,
    defaults: StreamWants,
) {
    tracing::info!("常驻后端已在听这台家里的套接字（多条流 + 不限次「只读 hello 就走」）");

    // 「谁在听」由常驻后端自己记（本机远端同一个写者；起它的那一方不写）：抢到锁、开门之前那一刻记的（`control::resident::claim`）。

    let _poke_task = spawn_sigusr1_task();

    let (mut idle_rx, mut idle_poke) = {
        let (rx, poke) = observe::watcher::spawn(
            agent_home.clone(),
            defaults.clone(),
            std::sync::Arc::default(),
        );
        (Some(rx), Some(poke))
    };

    // 多客户：认证通过的连接排进这里；每条自己一份 watcher / inbound / writer。
    let (attached_tx, mut attached_rx) = tokio::sync::mpsc::channel::<Attached>(1);
    let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<u64>(1);
    // 此刻连着几条流（归零才按退出行为办）。
    let mut clients = listen::Clients::default();

    loop {
        tokio::select! {
            // ① 有连接进来。**每条连接一个 task** —— 握手不许挡住 accept，
            //    否则一条只想读 hello 的连接会把整条监听线堵住。
            accepted = listener.accept() => {
                match accepted {
                    Ok(own_chan::Accepted::Ours(sock)) => {
                        tokio::spawn(handshake_one(
                            sock,
                            hello.clone(),
                            attached_tx.clone(),
                        ));
                    }
                    // 门在这里：别的 uid 连进来 ⇒ 关掉、出声，hello 都不给。
                    Ok(own_chan::Accepted::Foreign(uid)) => {
                        tracing::warn!("一条连接不是本账号的（对端 uid {uid:?}）⇒ 关掉");
                    }
                    Err(e) => tracing::warn!("accept 失败（{e}）；继续听"),
                }
            }
            // ② 有人认证通过 ⇒ 第一条来时退掉空转那份 watcher；给他一份新的（拿到一次完整初扫），起流。
            Some(att) = attached_rx.recv() => {
                if clients.count() == 0 {
                    if let Some(p) = idle_poke.take() {
                        p.shutdown();
                    }
                    idle_rx = None;
                }
                let id = clients.join();
                let Attached { reader, writer, hello_flushed, flags } = att;
                let book = observe::runs::RunBook::shared();
                let wants = flags.unwrap_or_else(|| defaults.clone());
                let tz = wants.tz.clone();
                let (rx, poke) = observe::watcher::spawn(agent_home.clone(), wants, book.clone());
                // 应答走**独立通道**：出方向丢一条内容帧可恢复，丢一条应答会让客户端永远等下去。
                let (reply_tx, reply_rx) =
                    tokio::sync::mpsc::channel::<Frame>(inbound::REPLY_CHANNEL_CAPACITY);
                let mut inbound_task = inbound::spawn(reader, reply_tx.clone(), hello_flushed);
                // 这条流连接的 tap 接收端（hub 扇出：每条连接一条）。
                let tap_rx = tap::attach(book, tz.clone());
                let done = done_tx.clone();
                tracing::info!("一条流已接上（此刻 {} 条）", clients.count());
                tokio::spawn(async move {
                    // ★★ **两个事件都算「客户端走了」，缺一个就会把那一档永久占住。**
                    //
                    // ⚠ 这一格是 `K-P1` 的 e2e **实测**逼出来的：只等 `writer_task`（它靠**写**拿到错误才结束）时，
                    // 一个**空闲**的后端根本没有东西可写 ⇒ 客户走了也不知道 ⇒ 连接计数永远不归零。
                    // ⇒ 再认一个事件：**入方向读到 EOF**（客户端关了它的写半边 / 进程没了）。
                    tokio::select! {
                        _ = writer_task(writer, rx, reply_rx, tap_rx, tz) => {
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
                    let _ = done.send(id).await;
                });
            }
            // ③ 一条流结束 ⇒ 计数减一；归零才回到空转（或按退出行为退）。
            Some(id) = done_rx.recv() => {
                let left = clients.leave(id);
                if left > 0 {
                    tracing::info!("一条流结束 ⇒ 还连着 {left} 条");
                    continue;
                }
                // 〔条 66〕**最后一个客户走了 ⇒ 现读这台机器的值，照它办。**
                //   不是「起我的那个 monitor 退了」—— 谁起的它不重要，重要的是此刻还有没有人连着。
                //   ⚠ 这里**现读**（`exit_policy::last_client_left` 每次真去读盘）：用户可能刚在
                //   **另一台** monitor 上改过它，不需要任何推送 / 同步协议。
                //   ⚠ `§3.3b ⑦` 的 `lingerMs`（归零后等一下再决定）**本路没做**：那是一个会自己醒来的构件，
                //   后端零定时器铁律（P6）不放行，理由整段在 `control/exit_policy.rs` 头注，待定。
                if control::exit_policy::last_client_left() {
                    // 不再当场 `exit`：刚走的那个客户可能还有停不下来的写在跑 ⇒ 先排空再退。
                    //   排空期间 accept 循环停着（新连接排在 backlog 里，退了之后被 RST，monitor 那头按「连不上 ⇒ 起一个」走）。
                    inbound::exit_after_drain(
                        "流结束 ⇒ 这台机器的退出策略是「结束」",
                        None::<std::future::Ready<()>>,
                    )
                    .await
                }
                tracing::info!("流结束 ⇒ 回到空转：口仍在听，sessions/ 仍在看");
                let (rx, poke) = observe::watcher::spawn(
                    agent_home.clone(),
                    defaults.clone(),
                    std::sync::Arc::default(),
                );
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
///
/// `tz` ＝ 这条流看的那一台的时区（起流的 `--tz` / attach 行的 `tz`）：出方向的每一帧写出去之前按它写钟面（[`Frame::stamp`]）。
async fn writer_task<W: tokio::io::AsyncWrite + Unpin>(
    mut out: W,
    mut rx: tokio::sync::mpsc::Receiver<Frame>,
    mut reply_rx: tokio::sync::mpsc::Receiver<Frame>,
    mut tap_rx: impl tap::TapSource,
    tz: cc_monitor_backend::Tz,
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
        // tap 帧**排在最后**（`biased` 按书写顺序问）：只有出方向与应答此刻都没有东西时才轮到它。
        //   它可丢（SSE 只保快，jsonl 保对），而出方向里的 `line` 帧不许因为它晚到或被挤掉 —— tap 走自己那条
        //   有界通道（`tap::TAP_CAPACITY`），满了在中转那一侧当场丢、位置号原位说。
        //   tap 通道被换掉（又一条流连接接上了）⇒ `recv` 回 `None`，这一臂的模式不匹配、本轮不参与，不会空转。
        let frame = if burst < REPLY_BURST {
            tokio::select! {
                biased;
                Some(f) = reply_rx.recv() => { burst += 1; f }
                f = rx.recv() => match f {
                    Some(mut f) => { burst = 0; f.stamp(&tz); f }
                    None => return, // 出方向通道关了 = 寿终
                },
                Some(f) = tap_rx.next() => f,
            }
        } else {
            burst = 0;
            tokio::select! {
                biased;
                f = rx.recv() => match f {
                    Some(mut f) => { f.stamp(&tz); f }
                    None => return,
                },
                Some(f) = reply_rx.recv() => f,
                Some(f) = tap_rx.next() => f,
            }
        };
        if let Err(e) = write_frame(&mut out, &frame, &tz).await {
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
/// 复制详情里那一行时刻的空位按这条流看的那一台的时区填好（应答 · 推送都过这里；`stamp_detail_slots`）。
async fn write_frame<W: tokio::io::AsyncWrite + Unpin>(
    out: &mut W,
    frame: &Frame,
    tz: &cc_monitor_backend::Tz,
) -> std::io::Result<()> {
    let line =
        to_line(frame).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    out.write_all(cc_monitor_backend::stamp_detail_slots(&line, tz).as_bytes())
        .await
}

/// 解析会话数据根：后端盯着的那一家的家目录（注册表 `LocalFace::home_at`）。
///
/// 要的是**恒定**答得出的那个 home（流式 watcher 与所有一次性子命令都拿它当根），
/// 不是 `agents::visible_homes()`（那只报目录已存在的几家；新机器上 home 还没建出来时后端照样起、等第一个会话）。
/// 装 `tracing`。流模式：写 stderr（stdout 是线上的流；stderr 进 monitor 的滚动日志，脱离载体时进诊断文件）。
/// 一次性模式：stderr 是协议通道（成功一个字节都没有、失败正好一行信封）⇒ 一行都不写 stderr，
/// 整行追加进这台后端的诊断文件（`stderr_log::oneshot_writer`）。
fn init_tracing(oneshot: bool) {
    let writer = if oneshot {
        tracing_subscriber::fmt::writer::BoxMakeWriter::new(stderr_log::oneshot_writer)
    } else {
        // 仍是 stderr；写每一行之前看一眼要不要滚（没被交诊断文件路径时它就是 `std::io::stderr`）。
        tracing_subscriber::fmt::writer::BoxMakeWriter::new(stderr_log::stderr_writer)
    };
    // Log to stderr so it never corrupts the stdout wire stream.
    tracing_subscriber::fmt()
        .with_writer(writer)
        // 〔NT2 问 3 ＋ RT1 F3〕只在 stderr 是终端（人在手跑）时上色：进 monitor 日志的管子、
        //   进脱离载体那份 stderr 文件的，转义码原样落盘、而且让 monitor 那一侧认不出行首的级别字。
        //   一次性模式写的是诊断文件 ⇒ 不上色。
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()) && !oneshot)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
}

fn resolve_agent_home() -> PathBuf {
    agent_home(None, true)
}

/// 本机 agent 家目录：给了账号配置目录就是它；没给 ⇒ `from_env` 时照进程环境，否则默认家目录
/// （`ccm --base` resume 时问的那一处）。问注册表只此一处。
fn agent_home(config_dir: Option<&std::path::Path>, from_env: bool) -> PathBuf {
    agents::home_at(config_dir, from_env)
}

// 🪦这里原有 `shutdown_signal`（等一次 SIGTERM / SIGINT，别处 Ctrl-C）—— 下沉到 `platform/signal.rs::shutdown_listener`〔散文墓碑〕：
//   平台 cfg 只许住那一层，而流模式的收场（`inbound::exit_after_drain`）也要它（排空时再来一次 ⇒ 不等了）。

#[cfg(test)]
#[path = "../../tests/backend/main_claim_tests.rs"]
mod claim_tests;

#[cfg(test)]
#[path = "../../tests/backend/writer_task_tests.rs"]
mod writer_task_tests; // 写者的优先序：tap 灌满时内容帧一条不少、顺序不变
