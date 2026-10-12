//! 命令表 · 终端与会话：`terminal-*`（含实时预览三条）· `terminals-list` · `session-terminals` · `launch*` · `kill` · `sessions-*`。

use crate::stream::inbound::spec::{arg, both, out, CommandSpec, Fail, Run};
use crate::stream::inbound::LocalFiles;

/// `session-new` 的失败（多一格下层原话：进应答的复制详情，不进那一句）。
fn failed_with_raw((code, message, data, raw): crate::control::session_new::Failed) -> Fail {
    Fail {
        code: code.to_string(),
        message,
        data,
        raw,
    }
}

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
    //   Windows `& '<ssh 全路径>' -t[ -J …] -p … [-i …] user@host -- '<bash -lic …>'`，别处同一组参数的 POSIX shell 一行）。组请求走 `dial/machine.rs::resolve`，本体 `dial/terminal.rs`。
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
    CommandSpec {
        name: "session-new",
        summary: "起一个新会话（全产品一个框、一个请求）",
        codes: &[
            "bad_args",
            "unknown_agent",
            "bad_command",
            "no_dir",
            "account_unavailable",
            "place_unavailable",
            "bad_tmux_name",
            "tmux_taken",
            "unobservable",
            "fork_failed",
            "refused",
            "start_failed",
            "child_timed_out",
            "launch_pending",
            "no_such_config",
        ],
        fields: &[arg("rotation", "可缺：打开哪份轮换配置。缺 ＝ 本机默认那份（不写）· `{config: id}` ＝ 起之前这台先定好 sid（那一家起新会话认的旗标，如 `--session-id`）、按它记成打开那一份，回包 `sid` 就是它；配置不在 ⇒ `no_such_config`；那一家不认先定 sid ⇒ `bad_args`"), both("account", "可缺 ＝ 跟随（分叉跟源会话上次的号；新起的 ⇒ 这台的默认号）· `{kind:\"base\"}` · `{kind:\"named\", name}`；应答里是实际用的号（账号 0 / 不指定 ⇒ `null`）"), both("agent", "哪一家（线上的 kind）；应答里是起的那一家"), both("cmd", "`open` 时界面要在终端里跑的那一行"), arg("command", "启动命令；空 / 缺 ⇒ 那一家的默认启动器"), out("configDir", "应答 `account` 里：那个号的配置目录"), both("cwd", "工作目录（开头的 `~` 按这台的家目录读）；应答里是展开之后的（认报到的会话按它）"), out("field", "失败时不行的那一格（`agent` · `command` · `cwd` · `account` · `place` · `tmuxName`；整体的 ⇒ `null`）"), arg("forkFrom", "可缺"), arg("kind", "`account` 的种类：`follow` · `base` · `named`（应答 `account` 是实际用的号，不带种类）"), arg("local", "发请求的界面就在这台上（开窗那一形本机与远端渲法不同）"), out("model", "应答 `account` 里：用的模型"), arg("model", "可缺：这一趟用的模型（压过 `models` 偏好表）；缺 ＝ 跟随"), arg("models", "可缺"), both("name", "`account` 为 `named` 时的号名；应答 `account` 里是实际用的号"), out("outcome", "`started`（tmux 里起好了，`session` 是会话名）· `open`（界面开一个终端跑 `cmd`）"), arg("permissionMode", "可缺：起在哪一个权限档（那一家声明的档之一，原值）；缺 ＝ 跟随那一家自己的默认；那一家没有 / 不给的档 ⇒ `bad_args`"), arg("place", "`tmux`（在这台 tmux 里后台起，关终端不断）· `window`（开一个新终端窗口直接跑）"), both("prompt", "入参：可缺，第一句（只跟 `place: tmux`；空 ⇒ `bad_args`）—— 作为启动参数交给那一家（交给它的那一半末尾、`--` 之后，可以跨行），那一家自己在启动画面 · 信任框过了之后交进会话；那一家声明不认起的时候带第一句 ⇒ `bad_args`。应答：交的结局 `{result, why, said}`（同 `terminal-input` 那三格；起好了 ⇒ `delivered`），没给第一句 ⇒ `null`"), out("session", "`started` 时的 tmux 会话名"), out("sid", "分叉出来的新会话 sid · 新起时这台先定好的 sid（那一家认先定 sid 的旗标）；那一家不认 ⇒ `null`（报到之前说不出）"), arg("ticket", "可缺：这一趟的票（界面每次点［新建］一张）；同一张票再问 ⇒ 起好了回原样那一份 · 还在起 ⇒ `launch_pending` · 没见过 / 没起成 ⇒ 照常起。票只记在这台常驻后端的进程里（最近 64 张），后端重启就忘了 ⇒ 照常起（已知边界）"), arg("tmuxName", "可缺 ⇒ 这台铸"), out("unavailable", "`account_unavailable` 时那一形，带替代号"), arg("uuid", "`forkFrom` 里：从哪条消息处分叉")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            crate::faces::session_new_face::answer(&r.args, &LocalFiles, own)
                .map(Some)
                .map_err(failed_with_raw)
        }),
    },
    // 起新会话框打开时问一次：这台最近用过的目录 · 有没有 tmux · 能起哪几家 · 分叉源会话的三格（只读，不写分支记录）。
    CommandSpec {
        name: "session-new-facts",
        summary: "起新会话那个框要的事实：最近目录 · 有没有 tmux · 有哪几家 agent · 分叉时原会话的起法",
        codes: &["bad_args", "fork_failed"],
        fields: &[arg("agent", "哪一家（线上的 kind）"), out("agents", "这台能起的几家（注册表里由我们起的、默认启动器在这台 `PATH` 上找得到的；注册表序）"), arg("at", "可缺（随 `forkOf`）"), arg("cwd", "工作目录（开头的 `~` 按这台的家目录读）"), out("fork", "没给 `forkOf` ⇒ `null`"), arg("forkOf", "可缺"), out("lastMs", "`recent` 一项：那个目录最近一次会话的修改时刻（毫秒）"), out("launch", "`fork` 里：起分叉会话要的三格（同 `session-fork` 的 `launch`）"), out("recent", "这台最近用过的工作目录（各家记录里的，新的在前、同一个目录一次、最多 8 个；`lastMs` 是那个目录最近一次会话的修改时刻）"), out("startText", "`fork` 里：那一轮你那句在看的那一台钟上的（请求的 `tz`）钟面 `HH:MM`（界面照抄；说不出 ⇒ `null`）"), out("tmux", "这台有没有 tmux（`false` ⇒ 只能开终端窗口）"), out("turn", "`fork` 里：`at` 那一条在第几轮")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::session_new_face::facts(&r.args, &r.tz)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
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
    CommandSpec {
        name: "session-restart",
        summary: "换号重启",
        codes: &[
            "bad_args",
            "unobservable",
            "account_unavailable",
            "not_in_terminal",
            "ambiguous",
            "session_already_live",
            "stop_failed",
            "start_failed",
        ],
        fields: &[both("account", "用户点名的号（只收名字）；应答里是实际用的号"), arg("agent", "同 `sessions-start` 的整批那几格"), out("alternative", "`account_unavailable` 失败的 `data`：给的显式替代号（这台的默认号；`null` ⇒ 只能「不指定号」）"), arg("arrive_within_ms", "等新进程报出的期限（≤ 3 600 000）"), arg("client", "自报的前端，同 `kill`"), out("compact", "先压缩那一步：`done` · `timed_out` · `skipped` · `unsupported` · `failed`"), arg("compact_first", "先请求压缩、等记录里出现压缩摘要再停"), arg("compact_within_ms", "等压缩的期限（≤ 3 600 000）"), out("configDir", "应答 `account` 里：配置目录"), arg("cwd", "工作目录"), arg("defaultLauncher", "同 `sessions-start`"), arg("launcher", "同 `sessions-start`"), out("listKnown", "`account_unavailable` 失败的 `data`：这台的账号清单读得出来"), arg("local", "同 `sessions-start`"), out("model", "应答 `account` 里：模型"), arg("models", "可缺：这台的模型偏好表原值"), out("name", "应答 `account` 里：实际用的号"), out("names", "`ambiguous` 失败的 `data`：在跑的那几个终端名"), out("pids", "`session_already_live` 失败的 `data`：那几个 pid"), out("pinned", "`account_unavailable` 失败的 `data`：要的号是这条会话上次用的（跟随），不是这次点名的"), out("requested", "`account_unavailable` 失败的 `data`：要的那个号"), arg("sid", "会话 id"), out("started", "`arrived`（新进程报出了）· `missed`"), out("stopped", "停完旧的之后才失败（`session_already_live` · `start_failed`）的 `data`：恒 `true`，旧会话已经不在了"), out("terminal", "所在的终端（会话名）"), out("why", "`stop_failed` / `start_failed` 失败的 `data`：那一步的码")],
        takes_input: true,
        run: Run::AsyncData(|r| {
            Box::pin(async move {
                crate::faces::session_restart_face::answer(r.args, r.until, &LocalFiles)
                    .await
                    .map(Some)
                    .map_err(|(code, message, data)| Fail {
                        code,
                        message,
                        data,
                        raw: None,
                    })
            })
        }),
    },
    crate::stream::inbound::typed::blocking::<crate::control::launch::Launch>(
        "建 tmux 会话并键入载荷，或键入一个已在的会话（远端执行面）",
    ),
];
