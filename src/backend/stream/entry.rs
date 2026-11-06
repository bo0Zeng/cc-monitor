//! 进程入口：`main.rs` 只留分配器与一行 [`run`]，起来那一下的装配 · 分流 · 两条载体都在这里。
//!
//! 远端那一半：解析 `~/.claude`，先发一帧 `Hello`，再跟读会话 JSONL，`line` / `session_added` / `session_left`
//! 帧一行一个 JSON 写进 stdout（对端是 monitor，经 SSH 管道或常驻监听口）。
//!
//! 运行目标是 Linux（inotify）；代码跨平台，Windows 上编得过、跑得起基本的文件监视（`notify` 可移植）。
//!
//! ## 两个任务（§5.4 慢消费者那道闸）
//!
//! - **读者**（[`observe::watcher::spawn`]）在阻塞线程上跑文件监视，用 `try_send` 往**有界**通道里推帧
//!   （inotify 回调从不阻塞）。
//! - **写者**（[`writer_task`]，`stream/writer.rs`）排空通道、把线上行写进 stdout。SSH 管道慢 ⇒ 通道背压，
//!   而有界那一格把背压挡在通道上，到不了 inotify 读者。

use crate::common::id::{BUILD_ID, VERSION};
use crate::faces::read_face;
use crate::stream::claim::claim_then_log;
use crate::stream::inbound::unavailable_here;
use crate::stream::inbound::views::StreamWants;
use crate::stream::writer::writer_task;
use crate::stream::{cli, inbound, oneshot, tap, topic_body};
use crate::wire::{self, drain, listen, topic_hook, Frame};
use crate::{agents, control, dial, faces, history, observe, stderr_log};
use comms_inward::line::{read_capped_line, CappedLine};
use copy_core::discard::{Discard, Why};

use std::path::PathBuf;
use tokio::io::BufWriter;

/// watcher 看见、控制面去做的那三件事（观测侧不改世界，只在那一刻报一声；两处起 watcher 都接这一份）。
const WATCH_REACTIONS: observe::watcher::Reactions = observe::watcher::Reactions {
    tmux_server_up: control::tmux_hook::install_for_this_process,
    session_seen: control::launch_account::adopt_seen,
    tag: control::identity_tag::tag_seen,
};

/// `ccm -- <词>` 里那个词归不归后端：一次性子命令（闸门 [`cli::is_subcommand`]）∪ 流模式旗标。交给 `control::ccm::route` 那一问就是它。
pub(crate) fn is_backend_word(w: &str) -> bool {
    cli::is_subcommand(w) || crate::wire::cli_words::STREAM_FLAGS.contains(&w)
}

/// 后端进程的整个身子：装配 → 分流（`ccm` · 一次性子命令 · 流模式）→ 起载体。每条路都在自己的收场处 `exit`。
pub async fn run() {
    // 远端 musl 版的分配器放回来的空页当场还给系统（不等下一次分配；别的版本什么都不做）。
    crate::platform::return_freed_pages_at_once();
    // 帧里的小成品（`changed.body`）照主题表现算：装进发端调的那一层（理由住 `stream/topic_body.rs` 头注）。
    topic_hook::install(topic_body::body_now);
    // 拨号核心（`ssh-dial`，手机同一份）装上本后端那一套：钥匙文件 · known_hosts · 本机 agent · 详情事实 · 部署那一处写根。
    dial::host();
    // 出网口装上这台的出网描述：照本后端起来那一刻的环境（`egress::PROXY_NAMES` · `NO_PROXY_NAMES` 那几个名字）。
    // 中转去上游 · 拨 SSH 都经它；每台用自己的，不跨机器传。
    if egress::install(egress::Egress::from_env(&|k| std::env::var(k).ok())).is_err() {
        tracing::error!("egress: 出网口那一份已经被别人装过了 —— 本后端起来那一刻的出网设置不在用");
    }
    // 拨号层要的「这一端的机器表怎么读」：起来那一下装一次（之后各台的常驻流 · 问那台 · 传输 · 部署都按名字查它）。
    faces::install_machines();
    // ★★ **当 `ccm` 用的那一趟，在这里就整条分出去。**
    //
    // 〔用户定〕「后端**只有一个**，**不要有什么 bash 脚本**，**不要有什么单独的 ccm**。」
    // ⇒ 终端里敲的 `ccm` 就是本二进制；被叫成一段配置的名字（`~/.cc-monitor/bin/<名>` 那条链接）⇒ 等同 `ccm @<名>`。
    //
    // 🔴 **三个「必须排在前面」，一个都不是排版**：
    //   ① 排在 `tracing_subscriber` 之前 —— 一次性模式的 stderr 是给人看的，
    //      混进后端的日志行就把「正常路径一个字都不说」这条契约破了；
    //   ② 排在 `split_stream_flags` 之前 —— 那一步会把 `--stream` / `--tz`
    //      从 argv **任意位置**剥掉，而 `ccm --tz x` 里那个要原样透传给 agent；
    //   ③ 排在 `resolve_agent_home()` 之前 —— 一次性模式不必去解析 agent 家目录。
    // 分流只经 `control::ccm::route`：当后端用时，后端认的 argv 是它交回来的那一串（去掉了打头的 `--`）。
    // argv 只在这里取一次：分流看 `[1..]`，ccm 那一趟要的「我被怎么叫的」由这里交（`ccm::run` 的 `process_argv`）。
    // 起第一个子进程之前：各家「我是哪个会话」的变量一律不往下传（常驻后端若在某个会话里起，它起的会话不许认错父）。
    agents::install_child_env_filter();
    #[expect(
        clippy::disallowed_methods,
        reason = "进程 argv 只在这里取一次，要它的模块由调用方交"
    )]
    let process_argv: Vec<String> = std::env::args().collect();
    #[expect(
        clippy::disallowed_methods,
        reason = "写后端自家状态（数据目录底下）的句柄：只在启动入口造一次，顺着各入口递下去"
    )]
    let own = crate::platform::Own::issue();
    // 握手认下的主机钥匙记进 cc-monitor 那份 known_hosts：哪条拨号路上都可能发生 ⇒ 这里装一次那只只会记钥匙的手。
    dial::install_known_hosts(&own);
    // 冷开扫描图（全进程一份）的骨架落在数据目录底下：这里装一次那个家（实时跟读起步也照它）。
    observe::skeleton::install(&own);
    // 历史清单缓存落在数据目录底下：这里装一次那个家（重启照它接着认，不整台重扫）。
    history::listing::install(&own);
    let backend_args: Vec<String> = {
        let rest: Vec<String> = process_argv.iter().skip(1).cloned().collect();
        match control::ccm::route(
            process_argv.first().map_or("", String::as_str),
            &rest,
            is_backend_word,
        ) {
            // resume 判「在别处跑着」用观测层那一份扫描、选号判「能不能用」用账号清单成品（control 不引用 observe ⇒ 都由入口注入）。
            control::ccm::Entry::Ccm(ccm_args) => std::process::exit(control::ccm::run(
                &ccm_args,
                &process_argv,
                &own,
                |dir| {
                    let home = agent_home(dir, false);
                    observe::watcher::running_sessions(&home)
                },
                faces::launch_account_face::list_at,
            )),
            control::ccm::Entry::Backend(a) => a,
        }
    };

    // issue #16：带参数 = 一次性历史查询模式，干完即退，不进流式协议。
    // 旧后端不认参数会照常发 hello 进流模式——monitor 以"首行是 hello 帧"
    // 识别旧版并提示升级（优雅降级，无协议版本协商负担）。
    let args: Vec<String> = backend_args;
    // 流模式 flag 集合，先剥离再判一次性查询模式
    // （否则误入 query 分支——INVARIANT §26）。纯函数化供单测（审计 D）。
    // 流的声明写坏了（`--stream --view <认不出的>`）：不起流 —— 当一次性那一形回失败信封、退出 2（下面那个 `match` 的头一臂）。
    let (args, wants, bad_view) = match cli::split_stream_flags(args) {
        Ok((a, w)) => (a, w, None),
        Err(why) => (Vec::new(), StreamWants::default(), Some(why)),
    };
    init_tracing(cli::is_query_mode(&args) || bad_view.is_some());

    if cli::is_query_mode(&args) || bad_view.is_some() {
        // 看的那一台的时区（`--tz`，剥流旗标那一步剥出来的）：一次性回包里的「几点」按它写。
        let z = &wants.tz;
        // 一次性查询模式：查手写的那张表（`stream::oneshot::ONESHOTS`）→ 从帧命令派生的 CLI 面 → 别的一律「不认识」
        // （含删掉的那几条带着裸参数敲：stderr 一行 `unknown_command` 失败信封、退出 2）。
        let first = args.first().map(String::as_str).unwrap_or_default();
        let code = if bad_view.is_some() {
            cli::refuse_stream_view(bad_view.as_deref(), z)
        } else if let Some(run) = oneshot::oneshot(first) {
            run(oneshot::Call {
                args: &args,
                tz: z,
                own: &own,
            })
            .await
        } else if cli::handles(first) {
            cli::run(&args, z, &own).await
        } else {
            cli::refuse_unknown(&args, z)
        };
        std::process::exit(code);
    }

    // 一次性查询已 exit；到此必是流模式。agent_home 日志放此（审计 correctness-重要①：
    // 一次性查询模式 stderr 只承载结构化错误/查询结果，不掺 info——兑现协议 v1 §3
    // 「错误 exit2 + stderr 纯 JSON 失败信封」，客户端可整段 JSON-parse stderr）。
    // fd 软上限抬到硬上限：排在起中转、各台常驻流、监听口之前（起后端的桌面会话缺省只给 1024）。一次性那几条不抬（用不了几个 fd）。
    tracing::info!("{}", crate::platform::raise_open_files_limit());
    // 这台设了的各家数据目录（本机 `prefs.json` 的 `agentHomes`）交成那一家的目录环境变量：排在解析家目录、起任何会话之前。
    control::apply_agent_homes();
    let agent_home = resolve_agent_home();
    tracing::info!("agent_home = {}", agent_home.display());

    // **脱离常驻那条载体的 stderr 落盘**：宿主交了路径才接（monitor 只在起脱离那条时交），先抢口、抢到了才接（[`claim_then_log`]）。
    //   放在一次性子命令全部 `exit` 之后：它们的 stderr 是给人 / 给 JSON 解析看的，不动。放在起中转之前：接上之后这一行起的
    //   每一句（中转那两句、host key 警告、panic）都落进那份文件。
    let env = |k: &str| std::env::var(k).ok();
    #[expect(
        clippy::disallowed_methods,
        reason = "stderr 落盘那只窄手只在启动入口造：抢到口之后装给脱离常驻那条载体"
    )]
    let issue_diag = crate::platform::DiagLog::issue;
    let (listening, installed) = match claim_then_log(
        &own,
        &env,
        control::resident::account_home().as_deref(),
        || stderr_log::install_from_env(issue_diag, &env),
    )
    .await
    {
        Ok(got) => got,
        Err(code) => std::process::exit(code),
    };
    // 常驻那条载体：口绑好、锁抢到 ⇒ 告诉起它的那一方起好了（`--resident-ensure` 读回信管道；本机宿主那一形 stdout 是空设备，写了也没人读）。
    //   之后连上来的在内核的队里等到收。
    if listening.is_some() {
        crate::platform::child::say_ready();
    }
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
        .filter(|dh| control::resident::legacy_listen_pid_in(dh).is_some());
    let off_thread = legacy.is_some();
    let relay_own = own.clone();
    let legacy_own = own.clone();
    // 占着常驻的那一个（常驻监听口在本进程）：账号库补号色只由它写。
    let resident = listening.is_some();
    let start_relay = move || {
        if let Some(said) = legacy
            .as_ref()
            .and_then(|_| control::resident::retire_legacy(&legacy_own))
        {
            tracing::info!("{said}");
        }
        tracing::info!(
            "{}",
            faces::rotation_face::host_relay(tap::port(), relay_own)
        );
        if resident {
            inbound::fill_account_colors();
            inbound::share_on_demand_dirs();
        }
    };
    if off_thread {
        std::thread::spawn(start_relay);
    } else {
        start_relay();
    }
    // 历史清单的缓存起来就后台热（低优先级的一次性线程）：第一次打开历史页 / 按 sid 开查看窗不再现扫整台。
    history::listing::warm_listing_in_background(agent_home.clone());
    // 照这台的配置文件 `profiles.toml` 把别名文件与链接对齐一次（写法变了的那一份当场换）。
    inbound::align_aliases();
    if resident {
        // 各台的常驻流归常驻这一位（机器表里要连的那几台各起一条，`control/feeds.rs`）；那台流上的事交每一份 watcher，
        //   投给连着这一位的每条流连接（控制面与观测面互不引用，由这里接上）。
        control::feeds::start(std::sync::Arc::new(observe::watcher::relay));
    }

    // ★★ **同一个流模式，两种载体**。
    //
    // 「脱离宿主」本身不难（`launch.rs` 里三份现成的范例）；难的是**脱离之后还怎么跟它对话**
    // —— 今天讲协议走的就是那对 stdio 管道，一脱离管道就没了。
    // ⇒ 换载体**不换协议**：`wire.rs` 头注那句「exactly one UTF-8 JSON object per line」
    // 在两条载体上逐字成立；`inbound::spawn<R: AsyncRead>` 与 `writer_task<W: AsyncWrite>`
    // 本来就是泛型的，喂 socket 的两半与喂 stdin/stdout **在类型上无差别**。
    //
    // ⚠ **由环境决定，不由 argv 决定**：子命令集合一动就要 bump `BUILD_ID`
    // 并进 `IPC-PROTOCOL.md` 的对拍面（`build_id_guard` / `protocol_doc_guard` 各钉一半），
    // 而这里**一条子命令都没加** —— 它换的是同一个流模式的载体。
    // 判定住 [`listen::mode_from`]（**纯函数**，所以「只写了一半」那两条错误支都测得到），在 [`claim_then_log`] 里定、抢口。

    // (b) Emit the Hello handshake FIRST, flushed, before anything else.
    let hello = build_hello();

    // 别的进程（命令行 · quota-warm · AI 照 skill 调）写了这台的轮换 ⇒ 这一路也推 `changed {rotation}`（广播通道是进程内的）。
    inbound::watch_rotation(&own);
    match listening {
        None => run_over_stdio(hello, agent_home, wants, own).await,
        Some((listener, held)) => {
            // 这台账号库里各号共用的用户级 MCP：常驻那条载体上盯各号的配置文件，一有动静同步一趟（一次性的 stdio 那条不起）。
            inbound::watch_account_mcp(&own);
            // 会话输入队的投递线程：只在占着常驻的这一个进程里起（一台只有一个投递的）。
            inbound::deliver_queues(&own);
            // 停机信号只挂**一次**（不在 accept 循环里每轮重装一个 SIGTERM 处理器）。
            // 收到之后监听口随 `serve_listening` 一起丢（不再接新连接）；已接上的那条流是独立任务，
            //   排空期间照常写应答，新来的阻塞命令回 `shutting_down`（`drain::exit_after_drain`）。
            let stop = drain::shutdown_listener();
            tokio::select! {
                () = serve_listening(
                    listener,
                    held,
                    hello,
                    agent_home,
                    own,
                ) => {}
                _ = stop => {
                    tracing::info!("shutdown signal received; exiting");
                }
            }
            drain::exit_after_drain("常驻后端收到停机信号", None::<std::future::Ready<()>>).await
        }
    }

    // ★ **必须显式 exit，不能让 runtime 自然 drop。**那一下今天住 `drain::exit_after_drain`
    //   （三个退出口共用）；下面这段理由原样留着，它说的是那一下为什么必须是 `exit`。
    //
    // `tokio::io::stdin()` 走的是**阻塞线程池**。`inbound_task.abort()` 只取消那个 async
    // task，**阻塞中的 `read(0)` 不受影响**；而 `#[tokio::main]` 展开出来的 runtime 在 drop
    // 时会等所有 blocking 任务结束 ⇒ 只要对端还开着 stdin，进程就永远停在这一行。
    //
    // 实测（有了入方向 reader 就会出的**回归**）：
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
    // 当场 `exit` 会把正在写的那一条连线程带走（E §E2）。⇒ 今天 exit 之前先排空停不下来的那一档（`drain::exit_after_drain`）。
    // stdout 不欠 flush 那半仍成立：`writer_task` 三路一排空就 flush（一直排着时每 `WRITE_FLUSH_BYTES` 一次）。
    // 两条载体都不返回（类型上是 `!`，各自在收场处 exit）⇒ 这里没有第二个 exit。
}

/// 造那一帧 hello。**抽出来是因为两条载体都要发它**，而它必须只有一份 ——
/// 两份 hello 会各自漂，而这一帧是手机端按精确字节在读的东西。
fn build_hello() -> Frame {
    Frame::Hello(wire::Hello {
        build_id: BUILD_ID.to_string(),
        host_arch: std::env::consts::ARCH.to_string(),
        // 这份二进制出自的那一版：连它的那一端（本机后端那条常驻流）照它写机器页那台的版本号。
        version: Some(VERSION.to_string()),
        commands: inbound::command_names()
            .into_iter()
            .map(str::to_string)
            .collect(),
        // ★★〔NET2 真填〕握手帧第四条面「我做得到什么」：这台机器上接得下却做不到的命令（tmux · unix 权限位两维，
        // 表从 `inbound::REGISTRY` 的 `codes` 派生）。是**提示不是闸门**（读数是握手那一刻的，`wire.rs` 那个字段头注口径③）。
        // 手机端不读这个字段（`src/mobile/core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/transport/DaemonTransport.kt::parseHello`，未知字段忽略）；有 tmux 的 unix 机器上恒空 ⇒ 字节不变。
        // 钉它的：`main_fourth_face_tests::production_hello_fills_unavailable_from_this_machine`。
        unavailable: unavailable_here(),
        // 回显起我的宿主交来的那几格（名单 `wire::HOST_ECHO_ENVS`）；一格都没交 ⇒ 省略、线上字节不变。
        host_env: wire::host_env_from(|name| std::env::var(name).ok()),
        // typed 命令的形状指纹：编进来的那张表原样交出去（客户端拿它自己那张比）。
        shapes: cmd_wire::shapes::SHAPES
            .iter()
            .map(|(c, h)| (c.to_string(), h.to_string()))
            .collect(),
        // 每种帧的形状指纹：同上，客户端按帧种比。
        frames: cmd_wire::frame_shapes::FRAMES
            .iter()
            .map(|(k, h)| (k.to_string(), h.to_string()))
            .collect(),
    })
}

/// 今天那条路：**stdin/stdout 一对管道**。宿主一退读端就断，backend 153ms 内自己走。
///
/// ⚠ 常驻是**另一条载体**，与这条并列、行为一致。改这一段之前先问：另一条载体要不要跟着改？
async fn run_over_stdio(
    hello: Frame,
    agent_home: PathBuf,
    wants: StreamWants,
    own: crate::platform::Own,
) -> ! {
    let mut stdout = BufWriter::new(tokio::io::stdout());
    // 写 + flush 一步到位，**并拿到 `HelloFlushed` 见证**。
    // 那个见证是 `inbound::spawn` 的必填参数 ⇒「reader 抢在 Hello 之前起来」
    // 编译期不可表示（比较字节位置的机检挡不住普通的函数抽取，类型挡得住）。
    let hello_flushed = match wire::write_and_flush_hello(&mut stdout, &hello).await {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("failed to write/flush hello frame: {e}");
            // 一条命令都还没收过 ⇒ 没有可排空的；走同一个收场口只为「exit 只有一处」。
            drain::exit_after_drain("hello 写不出去", None::<std::future::Ready<()>>).await
        }
    };

    // (b2) **入方向 reader。位置不可上移。**
    //
    // ★ 必须在上面那个 Hello 已经 flush **之后**才起 —— 客户端要先读到 Hello 才知道
    // 对面是什么版本、有什么能力；在那之前就收命令，等于在能力协商之前执行它。
    // 这是**时序约束**，两边单看都合理、合起来才错（同 PS 握手顺序那一族）。
    // 它**编译期不可表示**：`inbound::spawn` 要一个 `wire::HelloFlushed` 见证，
    // 而那个见证只能由 `write_and_flush_hello` 产出 —— 上一行拿到的就是它。
    //
    // 应答走**独立通道**：出方向丢一条**内容帧**可恢复（行还在远端 jsonl 里），
    // ⚠ 而**状态增量帧**丢了别处没有 —— 那半靠 `Overflow.lost` 带身份让客户端重同步，
    // 丢一条应答会让客户端永远等下去。混在一个通道里，实时行的洪峰会把应答挤掉。
    let (reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(inbound::REPLY_CHANNEL_CAPACITY);
    // (c) Start the watcher reader（这台一份）并接上这一条流：回这条连接的有界出方向通道。
    // 运行簿：整台一本，watcher 写、tap 那一路的流归位读。
    let (poke, book) = observe::watcher::spawn(agent_home, own.clone(), WATCH_REACTIONS);
    tap::adopt(&book);
    let (tz, view) = (wants.tz.clone(), wants.view.clone());
    // stdio 载体没有 attach 行 ⇒ 名单等 `stream-watch` 报上来（之前全发）。
    let (rx, attached) = poke.attach(&wants.tz, None);
    // 这条流连接最低优先那一路（活卡的发件箱 ＋ 几种推送）；名单与 watcher 那份同拍交给它。
    let tap_rx = tap::attach(tz.clone(), None);
    let mut inbound_task = inbound::spawn(
        tokio::io::stdin(),
        reply_tx.clone(),
        watch_desk(&attached, &tap_rx),
        own,
        hello_flushed,
    );

    // (c2) **P4：SIGUSR1 = 「tmux 那边有事，赶紧重探一次」。**
    //
    // ★ **这一步必须先于任何 hook 安装落地** —— `SIGUSR1` 的**默认处置是终止进程**。
    // 先装 hook 再装处理器，等于给一个会自杀的后端装了自杀触发器。
    // 后端这一侧刻意只做这一半：没有 hook 在发信号，它完全惰性。
    //
    // 为什么是信号而不是别的：原方案让 hook 追加事件日志、backend inotify 读增量，
    // **撞红线 I7「backend 只读」**（`readonly_guard` 当场拦下）。信号通路让后端的
    // 文件系统写归零，且会话名根本不经 shell ⇒ 那条引号/注入面直接消失。
    // 代价是信号无载荷且会合并 —— 靠「重探 + 与上一份快照差分」天然免疫。
    // P5：留一份给停机用（下面 select 结束后要显式通知 reader）。
    let poke_for_shutdown = poke.clone();
    // 处理器认的不是句柄，是 watcher 的名单（`observe::watcher::poke_all`；另一条载体上 watcher 会换人）。
    let poke_task = spawn_sigusr1_task();

    // (d) Run the stdout writer until the channel closes or a signal fires.
    // 收信号那一支**不丢写者**：排空期间它照常把在飞命令的最终应答写回去（`drain::exit_after_drain`）；
    //   入方向 reader 也留着 —— 新来的阻塞命令要有人回它一句 `shutting_down`。
    let stop = drain::shutdown_listener();
    let writer = writer_task(stdout, rx, reply_rx, tap_rx, tz, view);
    tokio::pin!(writer, stop);
    // 入方向读到 EOF（对端关了写半边）不退：只说一句（带 `STDIN_EOF_KEPT` 那个记号，判据等的就是它），接着等上面两件。
    let mut reading = true;
    let signalled = loop {
        tokio::select! {
            _ = &mut writer => {
                tracing::info!("writer task ended (channel closed)");
                break false;
            }
            _ = &mut stop => {
                tracing::info!("shutdown signal received; exiting");
                break true;
            }
            _ = &mut inbound_task, if reading => {
                reading = false;
                tracing::info!("{STDIN_EOF_KEPT}：入方向读到 EOF，流照开（只在写不出去或收到停机信号时退）");
            }
        }
    };
    // P5：**显式告诉 reader 停** —— 删掉 8s ticker 之后，reader 那边的
    // `sink.is_closed()` 复查再没有定期醒来的机会，只会一直阻塞在 `recv()`。
    // 漏这一句不会红任何测试（进程退出时线程随之消亡），所以它与删 ticker 是同一步。
    drop(attached);
    poke_for_shutdown.shutdown();
    poke_task.abort();
    if signalled {
        drain::exit_after_drain("收到停机信号", Some(writer)).await
    }
    // 写者断了 = 对端走了（SSH 断 / monitor 退）：入方向不会再有东西来，收掉它再排空。
    inbound_task.abort();
    drop(reply_tx);
    drain::exit_after_drain("对端走了（写不出去）", None::<std::future::Ready<()>>).await
}

/// stdio 流模式入方向读到 EOF、照开之后 stderr 那一行的记号（e2e 与起真后端的判据等它，再判「还活着」）。
const STDIN_EOF_KEPT: &str = "stdin-eof-kept";

/// 这条连接的 `stream-watch` 口接到它在这台 watcher 上的那一份（名单按连接）。
/// `stream-watch` 那一口：名单只有一份，同拍交给这条连接在 watcher 上的那一份（行 · 运行表 · 主线外清单）与活卡（`live` 帧）。
fn watch_desk(attached: &observe::watcher::Attachment, tap_rx: &tap::TapRx) -> inbound::WatchDesk {
    let h = attached.handle();
    let live = tap_rx.watch_handle();
    inbound::WatchDesk::new(move |ask| {
        live.watch(&ask.sids);
        h.watch(ask)
    })
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
/// ★ **戳的是名单，不是句柄**：处理器不认哪一份 watcher（今天一个进程只起一份，见 [`serve_listening`]；
/// 名单这一层留着，处理器与 `resync` 都不必知道它是谁起的）。名单只有一个家：
/// `observe::watcher` 的在跑名单（`spawn` 登记、退出即摘；`resync` 也按它找人）。
fn spawn_sigusr1_task() -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigusr1 = match signal(SignalKind::user_defined1()) {
            Ok(s) => s,
            Err(e) => {
                // ★★ 说出来，而且**说真话**：这里**没有定时探测的退路** —— 本进程零定时器，
                // 每一拍都靠事件（`no_timer_guard` 钉着它）。
                // 「不可用但有兜底」与「不可用而且没兜底」是两件完全不同的事。
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
    /// 回话那一行之后的写半边：客户端要了 deflate ⇒ 整条压（`listen::Down`）。
    writer: listen::Down<BufWriter<tokio::io::WriteHalf<own_chan::Stream>>>,
    hello_flushed: wire::HelloFlushed,
    /// 看的这一台的时区（attach 行里的 `tz`）。
    tz: crate::common::time::Tz,
    /// attach 行带的在看名单（没带 ⇒ 全发，等 `stream-watch`）。
    watch: Option<crate::wire::WatchAsk>,
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
    let mut buf = Vec::new();
    let line = match read_capped_line(&mut r, &mut buf, listen::ATTACH_LINE).await {
        Ok(CappedLine::Line) => String::from_utf8_lossy(&buf).into_owned(),
        Ok(CappedLine::Eof) => {
            // 「只读 hello 就走」那一档：对端读完就关，是**正常**结局，不出声。
            return;
        }
        Ok(CappedLine::TooLong(bytes)) => {
            tracing::warn!("一条 attach 请求 {bytes} 字节还没换行 ⇒ 整行丢弃并关连接");
            return;
        }
        Err(e) => {
            tracing::warn!("读 attach 请求失败（{e}）；关掉这条连接");
            return;
        }
    };
    match listen::admit(&line, BUILD_ID) {
        listen::Admit::Refuse(reason) => {
            // **出声地拒**（照 `relay::serve` 那条 503 的形状：宁可拒绝，也不静默 FIN）。
            // 静默 FIN 会让对端只能靠「等了很久没动静」去猜，而那是猜不出原因的。
            listen::write_line(&mut w, &listen::refusal_line(reason))
                .await
                .discard(Why::DeadLink);
            tracing::warn!("拒绝一条 attach 请求：{reason}");
        }
        listen::Admit::Stream { tz, reply, watch } => {
            if listen::write_line(&mut w, &reply.line()).await.is_err() {
                return;
            }
            let deflate = matches!(
                reply,
                cmd_wire::attach::AttachReply::Ok { deflate: true, .. }
            );
            let w = listen::Down::new(w, deflate);
            let tz = tz
                .as_deref()
                .and_then(crate::common::time::Tz::named)
                .unwrap_or_default();
            attached
                .send(Attached {
                    reader: r,
                    writer: w,
                    hello_flushed,
                    tz,
                    watch,
                })
                .await
                .discard(Why::PeerGone);
        }
    }
}

/// **常驻形态的接受循环** —— 一条流 + 不限次的「只读 hello 就走」。
///
/// # 空转期为什么还留着一个 watcher
///
/// 常驻真正买到的是两样东西，第二样就在这里：
/// ① `ccm` 那一问有答案了（连一次 + 读一行 hello）；
/// ② **`@ccm_sid` 打标那段时间窗关掉了** —— 写 `@ccm_sid` 的是**正在跑的** backend
///   （`observe/watcher.rs` inotify `sessions/` → `identity_tag::tag`），
///   而身份打标**没有轮询退路**。
///   monitor 没开着的时候若这里不看 `sessions/`，②就一格都没买到。
/// ⇒ 没人连着时 watcher 照样跑（帧算出来、发不出去），打标那一半照常发生。
///
/// # 接上客户端时：整台一份 watcher，各连接一份投影
///
/// 这台只起一份（`observe::watcher::spawn`）：每接一条换一份新的 watcher 会让每条连接都把全部活会话的记录
/// 整份预读一遍（桌面 ＋ 手机 ＋ 重连 ⇒ 同一份读 N 遍），换人那一瞬还有一个 inotify 重装窗口。接上来的连接经 `attach` 先收一份**此刻的样子**（每个活会话的宣告 · 读完那一帧 ·
/// 运行表 · 主线外清单 · 会话账本那一份，`watcher::join`），之后与别的连接一样收每一帧；记录整台只预读一次，也没有重装窗口。
///
/// # 它**不做**什么
///
/// **不重起自己**（自愈不在这一层）。宿主不在时**没有监护**，
/// 这是**如实登记的降级**，而且那句话要在 UI 上说出来（有判据钉它），不许只写在这条注释里。
async fn serve_listening(
    listener: own_chan::Listener,
    // 目录独占锁：随这个函数（＝ 常驻载体）活着。
    _held: crate::platform::OwnLock,
    hello: Frame,
    agent_home: PathBuf,
    own: crate::platform::Own,
) {
    tracing::info!("常驻后端已在听这台家里的套接字（多条流 + 不限次「只读 hello 就走」）");

    let _poke_task = spawn_sigusr1_task();

    // 这台一份 watcher（read8a：整台一份读者）：从起来到退一直这一份 —— 没人连着时它照样看 `sessions/`、打标；
    //   每接上一条流就 `attach` 一份投影（名单 · 续点 · 没送出去的帧按连接），记录整台只预读一次。
    let (watcher, book) = observe::watcher::spawn(agent_home.clone(), own.clone(), WATCH_REACTIONS);
    tap::adopt(&book);

    // 多客户：认证通过的连接排进这里；watcher 一个进程一份（各连接在它上面一份投影），inbound / writer 每条自己一份。
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
            // ② 有人认证通过 ⇒ 接上这台那份 watcher（先收一份此刻的样子），起流。
            Some(att) = attached_rx.recv() => {
                let id = clients.join();
                let Attached { reader, writer, hello_flushed, tz, watch } = att;
                // 名单在接上那一刻就定（watcher 那份与活卡同拍）：这条流从第一帧起只发名单里的。
                let (rx, poke) = watcher.attach(&tz, watch.clone());
                // 应答走**独立通道**：出方向丢一条内容帧可恢复，丢一条应答会让客户端永远等下去。
                let (reply_tx, reply_rx) =
                    tokio::sync::mpsc::channel::<Frame>(inbound::REPLY_CHANNEL_CAPACITY);
                // 此刻接着的各台那一条连接的 hello 先交这条流（它在 watcher 上接好之后才取 ⇒ 那几台往后的帧不会排到它前面：
                //   应答这一路先写，写的那一头还没起）。不然这条流收不到那几台这一条连接的开头。
                for f in control::feeds::attached_hellos() {
                    if reply_tx.try_send(f).is_err() {
                        tracing::warn!("新连上的流收不下此刻接着的那几台的 hello（应答通道满了）");
                        break;
                    }
                }
                // 这条流连接最低优先那一路（活卡的发件箱：整台一份活卡、每条连接按名单一个发件箱）。
                let tap_rx = tap::attach(tz.clone(), watch.as_ref().map(|w| &w.sids[..]));
                let mut inbound_task =
                    inbound::spawn(reader, reply_tx.clone(), watch_desk(&poke, &tap_rx), own.clone(), hello_flushed);
                let done = done_tx.clone();
                tracing::info!("一条流已接上（此刻 {} 条）", clients.count());
                tokio::spawn(async move {
                    // ★★ **两个事件都算「客户端走了」，缺一个就会把那一档永久占住。**
                    //
                    // ⚠ 这一格是 e2e **实测**逼出来的：只等 `writer_task`（它靠**写**拿到错误才结束）时，
                    // 一个**空闲**的后端根本没有东西可写 ⇒ 客户走了也不知道 ⇒ 连接计数永远不归零。
                    // ⇒ 再认一个事件：**入方向读到 EOF**（客户端关了它的写半边 / 进程没了）。
                    tokio::select! {
                        _ = writer_task(writer, rx, reply_rx, tap_rx, tz, None) => {
                            tracing::info!("流结束：写不出去了（客户端走了）");
                        }
                        _ = &mut inbound_task => {
                            tracing::info!("流结束：入方向读到 EOF（客户端关了它的写半边）");
                        }
                    }
                    // 这条连接的投影摘掉（watcher 照跑：这台一份）。
                    drop(poke);
                    inbound_task.abort();
                    drop(reply_tx);
                    done.send(id).await.discard(Why::Signal);
                });
            }
            // ③ 一条流结束 ⇒ 计数减一；归零才回到空转（或按退出行为退）。
            Some(id) = done_rx.recv() => {
                let left = clients.leave(id);
                if left > 0 {
                    tracing::info!("一条流结束 ⇒ 还连着 {left} 条");
                    continue;
                }
                // **最后一个客户走了 ⇒ 现读这台机器的值，照它办。**
                //   不是「起我的那个 monitor 退了」—— 谁起的它不重要，重要的是此刻还有没有人连着。
                //   ⚠ 这里**现读**（`exit_policy::last_client_left` 每次真去读盘）：用户可能刚在
                //   **另一台** monitor 上改过它，不需要任何推送 / 同步协议。
                //   ⚠ `§3.3b ⑦` 的 `lingerMs`（归零后等一下再决定）**没做**：那是一个会自己醒来的构件，
                //   后端零定时器铁律（P6）不放行，理由整段在 `control/exit_policy.rs` 头注，待定。
                if control::exit_policy::last_client_left() {
                    // 不再当场 `exit`：刚走的那个客户可能还有停不下来的写在跑 ⇒ 先排空再退。
                    //   排空期间 accept 循环停着（新连接排在 backlog 里，退了之后被 RST，monitor 那头按「断开 ⇒ 起一个」走）。
                    drain::exit_after_drain(
                        "流结束 ⇒ 这台机器的退出策略是「结束」",
                        None::<std::future::Ready<()>>,
                    )
                    .await
                }
                tracing::info!("流结束 ⇒ 回到空转：口仍在听，sessions/ 仍在看（同一份 watcher）");
            }
        }
    }
}

/// 装 `tracing`。流模式：写 stderr（stdout 是线上的流；stderr 进 monitor 的滚动日志，脱离载体时进诊断文件）。
/// 一次性模式：stderr 是协议通道（成功一个字节都没有、失败正好一行信封）⇒ 一行都不写 stderr，
/// 整行追加进这台后端的诊断文件（`stderr_log::oneshot_writer`）。
fn init_tracing(oneshot: bool) {
    let writer = if oneshot {
        #[expect(
            clippy::disallowed_methods,
            reason = "诊断文件那一份的窄句柄只在启动入口造，装给一次性模式的写者"
        )]
        stderr_log::keep_oneshot(crate::platform::DiagLog::issue);
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

/// 解析会话数据根：后端盯着的那一家的家目录（注册表 `LocalFace::home_at`）。
///
/// 要的是**恒定**答得出的那个 home（流式 watcher 与所有一次性子命令都拿它当根），
/// 不是 `agents::visible_homes()`（那只报目录已存在的几家；新机器上 home 还没建出来时后端照样起、等第一个会话）。
fn resolve_agent_home() -> PathBuf {
    agent_home(None, true)
}

/// 本机 agent 家目录：给了账号配置目录就是它；没给 ⇒ `from_env` 时照进程环境，否则默认家目录
/// （`ccm --base` resume 时问的那一处）。问注册表只此一处。
fn agent_home(config_dir: Option<&std::path::Path>, from_env: bool) -> PathBuf {
    agents::home_at(config_dir, from_env)
}

#[cfg(test)]
#[path = "../../../tests/backend/main_startup_tests.rs"]
mod startup_tests; // 起来那一下：机器表在分流之前装上 · 常驻起来那一段不调换形
