//! 命令表 · 终端与会话：`terminal-*`（含实时预览三条）· `terminals-list` · `session-window` · `session-terminals` · `launch*` · `terminal-end` · `terminals-end` · `sessions-*` · 监控板那两条。

use crate::wire::spec::CommandSpec;

pub(super) const SPECS: &[CommandSpec] = &[
    // 〔「待迁」最后一行〕**开终端那一串**：`{machine, saved?, jump?, prefer?, command}` ⇒ `{command}`（按本机终端方言的一行：
    //   Windows `& '<ssh 全路径>' -t[ -J …] -p … [-i …] user@host -- '<bash -lic …>'`，别处同一组参数的 POSIX shell 一行）。只收那台的名字，组请求照本机后端的机器表（`dial/mod.rs::dial_named`），本体 `dial/terminal.rs`。
    //   查几个文件找 ssh 客户端（`platform/ssh_client.rs`）⇒ 阻塞档；不拨号、不起进程。
    super::typed::blocking::<crate::dial::terminal::TerminalSsh>(
        "给一台远端开终端要跑的那一串",
    ),
    // ↗ 那一问：这条会话在这台电脑上的哪儿：`{machine, sid}` ⇒ `{labels, chain:[{pid, name, start}…], why?, addr?}`
    //   （`faces/session_window_face.rs`）。异步档：远端会话经 `--resident-call` 问那台的 `session-terminals`；
    //   读 `/proc` · 起一次 `tmux list-clients` · 读连接表 ＋ 进程表四格放阻塞线程。只在被问时答。
    super::typed::asynchronous::<crate::faces::session_window_face::SessionWindowCmd>(
        "这条会话在这台电脑上的哪儿（窗口标签 · 进程链，或原因）",
    ),
    // 此刻是哪个终端在显示这个会话：`{sid}` ⇒ `{terminals:[{ssh, activity}…], why?}`（`observe/session_terminals.rs`）。
    //   问它的是别台的本机后端（`session-window` 的远端一半）。阻塞档：读 `/proc` ＋ 在 tmux 里时起一次 `tmux list-clients`。零定时器，只在被问时答。
    super::typed::blocking::<crate::faces::feature_face::SessionTerminals>(
        "此刻是哪个终端在显示这个会话",
    ),
    // 终端管理 L1 三条（`control/terminals.rs`）：只认名单里的终端（句柄 / sid），不收 tmux 目标串；送字送键过身份门。
    //   阻塞档（起 tmux）；只抓一次、只送一次，轮询归调用方。
    super::typed::blocking::<crate::control::terminals_wire::TerminalsListCmd>(
        "这台的终端名单（两种宿主同一形，每一行带宿主）",
    ),
    // 「终端」视图那张名单（`faces/terminals_board_face.rs`）：这台那几行写成成品（阻塞档：起 tmux · 读活会话表）；
    //   本机后端问到各台合成一份（异步档：别的台经常驻问 `terminals-board-here`，没答的进那一组的 `down`）。
    super::typed::blocking::<crate::faces::terminals_board_face::TerminalsBoardHereCmd>(
        "这台「终端」视图那几行（只列 cc-monitor 起的，排好、字写好）",
    ),
    super::typed::asynchronous::<crate::faces::terminals_board_face::TerminalsBoardCmd>(
        "「终端」视图那张名单（问到各台、合成一份：每台一组 · 筛选四格的数）",
    ),
    // 终端名单「结束所选」· 起会话占位［结束］那一批（`faces/terminals_end_face.rs`）：按台分组，本机那一组同进程逐项走 `terminal-end` 的本体，
    //   别的台经常驻问它自己那一组；逐条照原序、合好那一句（同结束一批会话那一句）。异步档。
    super::typed::asynchronous::<crate::faces::terminals_end_face::TerminalsEndCmd>(
        "结束一批终端跨几台（按台分组做、照原序合好那一句）",
    ),
    // 监控板（`faces/monitor_board_face.rs`）：这台那几条会话的序 · 活不活 · 在跑几个（只读进程里那两本）；
    //   本机后端问到各台合成一份（别的台经常驻问 `monitor-board-here`，没答的那一组带那台离线那一句）。
    super::typed::asynchronous::<crate::faces::monitor_board_face::MonitorBoardHereCmd>(
        "监控板这台那几条（序 · 活不活 · 在跑几个 agent；不认得的不答）",
    ),
    super::typed::asynchronous::<crate::faces::monitor_board_face::MonitorBoardCmd>(
        "监控板（问到各台、合成一份：分组 · 组内先后 · 组头 · 汇总那一句）",
    ),
    super::typed::blocking::<crate::control::terminals_wire::TerminalPreviewCmd>(
        "抓一个终端的一屏（只抓一次，轮询归调用方）",
    ),
    super::typed::blocking::<crate::control::terminals_wire::TerminalInputCmd>(
        "往一个终端送字或送键（过身份门）",
    ),
    // 托管终端只有一个写者：接管 · 交还（`control/terminals.rs::hold_on`；tmux 那一形回 `not_managed`）。
    super::typed::blocking::<crate::control::terminals_wire::TerminalTakeInputCmd>(
        "替自报的前端接管托管终端的输入",
    ),
    super::typed::blocking::<crate::control::terminals_wire::TerminalReleaseInputCmd>(
        "交还托管终端的输入",
    ),
    // 会话 ■：中断这一轮，不进输入队、不靠持有输入（`control/terminals.rs::interrupt_on`；托管那一形经托管口 `interrupt`）。
    super::typed::blocking::<crate::control::terminals_wire::SessionInterruptCmd>(
        "中断一条会话这一轮（不进输入队、不换持有者）",
    ),
    // 按名单里的句柄结束一个终端，两种宿主同一条（`control/kill.rs::end_terminal`）。
    super::typed::blocking::<crate::control::kill::TerminalEnd>("结束名单里的一个终端"),
    super::typed::blocking::<crate::faces::session_commands_face::SessionCommandsCmd>(
        "一条会话能用的斜杠命令与 Skill（内置 ＋ 本项目 ＋ 它用的号的配置目录）",
    ),
    super::typed::asynchronous::<crate::faces::session_model_face::SessionModel>(
        "换模型（开着 ⇒ 送那一家的那一行、从记录里读回换成了什么；在等人不送；终端没了 ⇒ 记下、下次恢复照它起）",
    ),
    super::typed::blocking::<crate::faces::session_mode_face::SessionPermission>(
        "切权限档（开着 ⇒ 按那一家的循环键、等重画、读回到了为止，最多一圈；在等人不按；终端没了 ⇒ 记下、下次恢复照它起）",
    ),
    // 终端实时预览三条（`control/terminal_follow.rs`）：订阅名单里一个终端的画面，有变化就推一整屏（`terminal_screen` 帧），
    //   一帧在途、客户端回执之后才推下一帧；停了推 `terminal_follow_end`。都是 `Run::Builtin`：要碰本连接的票表与应答通道 ⇒ **只在帧面**。
    super::typed::frame::<crate::control::terminal_follow::TerminalFollow>(
        "订阅一个终端的画面（有变化推一整屏，一帧在途等回执；tmux 那一形订着时那台多一个只读客户端，用户自己配的 client-attached / client-detached 钩子会被它触发）",
    ),
    super::typed::frame::<crate::control::terminal_follow::TerminalFollowAck>(
        "订阅的第 `seq` 帧画完了（画面有变化就推下一帧）",
    ),
    super::typed::frame::<crate::control::terminal_follow::TerminalUnfollow>(
        "退订一个终端的画面（幂等，不发收尾帧）",
    ),
    // 一批会话的停：每一个同单个那一条（`terminal-end`，按名单句柄），逐个答结局。
    //   阻塞档（逐个起 tmux）；要动 tmux / 读记录的几样由帧面那层壳交进去（`faces/session_batch_face.rs`）。
    super::typed::blocking::<crate::faces::session_batch_face::SessionsStop>(
        "停一批会话（逐个答，一个不成不挡下一个）",
    ),
    // 一批会话跨几台（`faces/sessions_batch_face.rs`）：停 · 起 · 隐藏 · 固定，本机那一组同进程做，别的台经常驻问它自己那一组；逐条照原序、合好那一句。异步档。
    super::typed::asynchronous::<crate::faces::sessions_batch_face::SessionsBatchCmd>(
        "一批会话跨几台（按台分组做、照原序合好那一句）",
    ),
    super::typed::blocking::<crate::faces::session_batch_face::SessionsWhere>(
        "一批会话各在这台哪个终端里",
    ),
    // **起会话 · 恢复 · 接回 —— 一个请求**（`control/session_open/`，宿主 `faces/session_open_face.rs`）：意图进去，这台判号 · 放在哪 ·
    //   记录在不在 · 已有进程在写 · 在跑 / 就地 / 新开，回「落在哪 ＋ 要开终端就跑这一行」；每个码的那一句后端写好。
    super::typed::blocking::<crate::faces::session_open_face::SessionOpen>("起会话 · 恢复 · 接回（一个请求：落在哪 ＋ 要开终端就跑的那一行）"),
    // 等起好的会话报到（`control/session_arrive.rs`）：`session-open` 回包的 `arrival` 原样交回，期限 ＝ 请求信封的期限；
    //   没见到 ⇒ 那一句（起在 tmux 里的带那一屏末几行）。
    super::typed::asynchronous::<crate::faces::session_arrive_face::SessionArrive>("等起好的会话报到"),
    // 起新会话框打开时问一次：这台最近用过的目录 · 能放在哪几种 · 能起哪几家 · 分叉源会话的三格（只读，不写分支记录）。
    super::typed::blocking::<crate::faces::session_open_face::SessionOpenFacts>(
        "起新会话那个框要的事实：最近目录 · 能放在哪几种 · 有哪几家 agent · 分叉时原会话的起法",
    ),
    // 工作目录那一格失焦时问：在不在 · 这台此刻会给它铸的终端名（没 tmux ⇒ `null`）。
    super::typed::blocking::<crate::faces::session_open_face::SessionOpenDir>(
        "起新会话前核一个目录：在不在 · 会用哪个终端名",
    ),
    // 换号重启（`control/session_restart.rs`）：查号 → 找终端 → 先压缩（可选，等摘要）→ 停旧 → 同一终端名用新号起 → 等报出，全在这台做完。
    //   可撤档：步与步之间撤单生效；停旧 ＋ 起新不可分（拿退出排空的票），开跑之后撤也做完。失败可带按码定形的 `data`。
    super::typed::asynchronous::<crate::faces::session_restart_face::SessionRestart>("换号重启"),
];
