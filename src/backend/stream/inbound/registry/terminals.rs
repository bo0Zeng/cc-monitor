//! 命令表 · 终端与会话：`terminal-*`（含实时预览三条）· `terminals-list` · `session-terminals` · `launch*` · `kill` · `sessions-*`。

use crate::stream::inbound::spec::{arg, both, out, CommandSpec, Run};
use crate::stream::inbound::LocalFiles;

pub(super) const SPECS: &[CommandSpec] = &[
    // 起会话那一行 `ccm …`（`control/launch_render/`）：交给终端的只有这一行，环境与中转地址归那台的 `ccm`。
    //   `launch-local`：本机那几形（阻塞档：核一次「新起」的目录在不在）；`launch-render-cli`：远端那几形（纯函数）。
    crate::stream::inbound::typed::with_files::<crate::faces::launch_face::LaunchLocal>(
        "本机起会话那一行 `ccm …`",
        &LocalFiles,
    ),
    crate::stream::inbound::typed::with_files::<crate::faces::launch_face::LaunchRenderCli>(
        "远端起会话那一行 `ccm …`",
        &LocalFiles,
    ),
    // 〔「待迁」最后一行〕**开终端那一串**：`{machine, saved?, jump?, prefer?, command}` ⇒ `{command}`（按本机终端方言的一行：
    //   Windows `& '<ssh 全路径>' -t[ -J …] -p … [-i …] user@host -- '<bash -lic …>'`，别处同一组参数的 POSIX shell 一行）。组请求走 `ssh-dial/machine.rs::resolve`，本体 `dial/terminal.rs`。
    //   查几个文件找 ssh 客户端（`platform/ssh_client.rs`）⇒ 阻塞档；不拨号、不起进程。
    CommandSpec {
        name: "terminal-ssh",
        summary: "给一台远端开终端要跑的那一串",
        codes: &["bad_args", "bad_jump", "refused", "no_ssh_client", "unobservable"],
        fields: &[both("command", "要在那台跑的命令；应答里是那一整行 PowerShell `& '<ssh 全路径>' -t … -- 'bash -lic …'`")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::dial::terminal::answer(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 那台报来的终端 ⇒ 这台电脑上开着那条连接的进程链：`{terminals}` ⇒ `{chain:[{pid, name, start}…], why?, addr?}`
    //   （`dial/terminal_processes.rs`）。阻塞档：读连接表 ＋ 进程表四格（Windows 直调系统接口、每个进程开一次句柄问启动时刻 · Linux 读 `/proc`）。只在被问时答。
    crate::stream::inbound::typed::blocking::<crate::dial::terminal_processes::TerminalProcesses>(
        "那台报来的终端连接是这台电脑上哪个进程开的",
    ),
    // 此刻是哪个终端在显示这个会话：`{sid}` ⇒ `{terminals:[{ssh, activity}…], why?}`（`observe/session_terminals.rs`）。
    //   阻塞档：读 `/proc` ＋ 在 tmux 里时起一次 `tmux list-clients`。零定时器，只在被问时答。
    crate::stream::inbound::typed::blocking::<crate::faces::feature_face::SessionTerminals>(
        "此刻是哪个终端在显示这个会话",
    ),
    // 终端管理 L1 三条（`control/terminals.rs`）：只认名单里的终端（句柄 / sid），不收 tmux 目标串；送字送键过身份门。
    //   阻塞档（起 tmux）；只抓一次、只送一次，轮询归调用方。
    crate::stream::inbound::typed::blocking::<crate::control::terminals_wire::TerminalsListCmd>(
        "这台的终端名单（形状与宿主无关；这一版宿主是 tmux）",
    ),
    crate::stream::inbound::typed::blocking::<crate::control::terminals_wire::TerminalPreviewCmd>(
        "抓一个终端的一屏（只抓一次，轮询归调用方）",
    ),
    crate::stream::inbound::typed::blocking::<crate::control::terminals_wire::TerminalInputCmd>(
        "往一个终端送字或送键（过身份门）",
    ),
    crate::stream::inbound::typed::blocking::<crate::faces::session_mode_face::SessionPermission>(
        "会话开着时切权限档（按那一家的循环键、等重画、读回到了为止，最多一圈；在等人不按）",
    ),
    crate::stream::inbound::typed::blocking::<crate::faces::session_say_face::SessionSay>(
        "往一条会话说一句（在等人 ⇒ 不送；不在等 ⇒ 照送字那一条送、补回车）",
    ),
    // 终端实时预览三条（`control/terminal_follow.rs`）：订阅名单里一个终端的画面，有变化就推一整屏（`terminal_screen` 帧），
    //   一帧在途、客户端回执之后才推下一帧；停了推 `terminal_follow_end`。都是 `Run::Builtin`：要碰本连接的票表与应答通道 ⇒ **只在帧面**。
    CommandSpec {
        name: "terminal-follow",
        summary: "订阅一个终端的画面（有变化推一整屏，一帧在途等回执；订着时那台 tmux 里多一个只读客户端，用户自己配的 client-attached / client-detached 钩子会被它触发）",
        codes: &[
            "bad_target",
            "bad_args",
            "not_known",
            "ambiguous",
            "no_tmux",
            "tmux_too_old",
            "too_many_follows",
            "unobservable",
            "child_timed_out",
        ],
        fields: &[out("live", "失败时：实时那一格落在哪 —— `snapshot_only`（这台只能快照：没装 tmux · tmux 低于 3.2）· `stopped`（别的）"), arg("sid", "目标：会话 id（与 `terminal` 恰给一个）"), arg("terminal", "目标：名单里的句柄（与 `sid` 恰给一个）"), arg("ticket", "订阅票（客户端铸的不透明串，至多 128 字节，只用一次）；之后的 `terminal_screen` / `terminal_follow_end` 帧带它")],
        takes_input: true,
        // 本体 `control/terminal_follow.rs::Desk::follow`（`dispatch` 的硬臂带着本连接的票表调它；起 tmux ⇒ 登记在 `target_parity_guard::BUILTIN_HANDLER_FILE`）。
        run: Run::Builtin,
    },
    CommandSpec {
        name: "terminal-follow-ack",
        summary: "订阅的第 `seq` 帧画完了（画面有变化就推下一帧）",
        codes: &["bad_args", "not_known"],
        fields: &[arg("seq", "画完的那一帧的序号"), arg("ticket", "订阅票")],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "terminal-unfollow",
        summary: "退订一个终端的画面（幂等，不发收尾帧）",
        codes: &["bad_args"],
        fields: &[arg("ticket", "订阅票；退不在册的也回 `ok`（记下它：订阅那一问若还在路上，到了也不起）")],
        takes_input: true,
        run: Run::Builtin,
    },
    // **起会话要一个终端名 —— 问这台**：`{cwd}`（`<项目名>-cc`）或 `{forkOf}`（`<…>-fork-cc`）⇒ `{name}`（按这台那张会话快照避让）。
    //   本体 `control/ccm/mod.rs::TerminalNameMint`（这一版宿主只有 tmux）；阻塞档（快照问一次就起一次 `tmux`）。
    crate::stream::inbound::typed::blocking::<crate::control::ccm::TerminalNameMint>(
        "起会话要的终端名",
    ),
    // F04a：**第一条破坏性命令。** 三道门在 `control/gate::admit_destructive`，
    // 对句柄下手不对名字。⚠ monitor 侧改走这条路是 **F04b**（定框 C6 的顺序）。
    crate::stream::inbound::typed::blocking::<crate::control::kill::Kill>("杀一个 tmux 会话"),
    // 一批会话的停 / 起：每一个同单个那一条（`kill` · `launch` · `launch-render-cli` · `launch-local`），逐个答结局。
    //   阻塞档（逐个起 tmux）；要动 tmux / 读记录的几样由帧面那层壳交进去（`faces/session_batch_face.rs`）。
    crate::stream::inbound::typed::blocking::<crate::faces::session_batch_face::SessionsStop>(
        "停一批会话（逐个答，一个不成不挡下一个）",
    ),
    crate::stream::inbound::typed::blocking::<crate::faces::session_batch_face::SessionsWhere>(
        "一批会话各在这台哪个终端里",
    ),
    crate::stream::inbound::typed::with_files::<crate::faces::session_batch_face::SessionsStart>(
        "起 / 接回一批会话（逐个答）",
        &LocalFiles,
    ),
    // **起新会话 —— 全产品一个请求**（`control/session_new.rs`，宿主 `faces/session_new_face.rs`）：这台判目录 · 号（选不了带替代号）·
    //   终端名 · 分叉（只在这一步写分支记录）· 起；回「起好了 / 要开窗跑那一行」，某一格不行 ⇒ 失败信封 `data.field` 说是哪一格。
    crate::stream::inbound::typed::with_own::<crate::faces::session_new_face::SessionNew>(
        "起一个新会话（全产品一个框、一个请求）",
        &LocalFiles,
    ),
    // 起新会话框打开时问一次：这台最近用过的目录 · 有没有 tmux · 能起哪几家 · 分叉源会话的三格（只读，不写分支记录）。
    crate::stream::inbound::typed::blocking::<crate::faces::session_new_face::SessionNewFacts>(
        "起新会话那个框要的事实：最近目录 · 有没有 tmux · 有哪几家 agent · 分叉时原会话的起法",
    ),
    // 工作目录那一格失焦时问：在不在 · 这台此刻会给它铸的终端名（没 tmux ⇒ `null`）。
    crate::stream::inbound::typed::blocking::<crate::faces::session_new_face::SessionNewDir>(
        "起新会话前核一个目录：在不在 · 会用哪个终端名",
    ),
    // 动一个会话之前会打断什么（`observe/interrupts_query.rs`）：按族答，界面只画。只读。
    crate::stream::inbound::typed::blocking::<crate::faces::feature_face::SessionInterrupts>(
        "动一个会话之前，会打断什么",
    ),
    // 换号重启（`control/session_restart.rs`）：查号 → 找终端 → 先压缩（可选，等摘要）→ 停旧 → 同一终端名用新号起 → 等报出，全在这台做完。
    //   可撤档：步与步之间撤单生效；停旧 ＋ 起新不可分（拿退出排空的票），开跑之后撤也做完。失败可带按码定形的 `data`。
    crate::stream::inbound::typed::async_with_files::<crate::faces::session_restart_face::SessionRestart>(
        "换号重启",
        &LocalFiles,
    ),
    crate::stream::inbound::typed::blocking::<crate::control::launch::Launch>(
        "建 tmux 会话并键入载荷，或键入一个已在的会话（远端执行面）",
    ),
];
