//! 命令表 · 终端与会话：`terminal-*` · `terminals-list` · `session-terminals` · `launch*` · `kill` · `sessions-*`。

use crate::stream::inbound::spec::{CommandSpec, Fail, Run};

/// 起会话那几条的失败（码 ＋ 那一句 ＋ 按码定形的 `data`）⇒ 应答那一格。
fn failed((code, message, data): crate::control::launch_render::Failed) -> Fail {
    Fail {
        code: code.to_string(),
        message,
        data,
    }
}

pub(super) const SPECS: &[CommandSpec] = &[
    // 起会话那一行 `ccm …`（`control/launch_render/`）：交给终端的只有这一行，环境与中转地址归那台的 `ccm`。
    //   `launch-local`：本机那几形（阻塞档：核一次「新起」的目录在不在）；`launch-render-cli`：远端那几形（纯函数）。
    CommandSpec {
        name: "launch-local",
        doc_anchor: Some("#### `launch-local`"),
        codes: &["bad_args", "refused", "account_unavailable"],
        fields: &[
            "account",
            "action",
            "cmd",
            "configDir",
            "cwd",
            "defaultLauncher",
            "kind",
            "launchId",
            "launcher",
            "model",
            "name",
            "tmuxName",
        ],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::faces::launch_face::answer_local(&r.args)
                .map(Some)
                .map_err(failed)
        }),
    },
    CommandSpec {
        name: "launch-render-cli",
        doc_anchor: Some("#### `launch-render-cli`"),
        codes: &["bad_args", "refused", "account_unavailable"],
        fields: &[
            "account",
            "action",
            "ccmSid",
            "cmd",
            "configDir",
            "container",
            "cwd",
            "defaultLauncher",
            "kind",
            "launcher",
            "model",
            "models",
            "name",
        ],
        takes_input: true,
        // 阻塞档：判号要读这台的账号清单与那份记录（同步文件 I/O）。
        run: Run::BlockingData(|r| {
            crate::faces::launch_face::answer_cli(&r.args)
                .map(Some)
                .map_err(failed)
        }),
    },
    // 〔「待迁」最后一行〕**开终端那一串**：`{machine, saved?, jump?, prefer?, command}` ⇒ `{command}`（一行 PowerShell：
    //   `& ssh -t[ -J …] -p … [-i …] user@host -- '<bash -lic …>'`）。组请求走 `dial/machine.rs::resolve`，本体 `dial/terminal.rs`。
    //   纯函数：校验 ＋ quote，不拨号、不起进程、不碰盘 ⇒ 不进阻塞档（同 `ping` 那一形）。
    CommandSpec {
        name: "terminal-ssh",
        doc_anchor: Some("#### `terminal-ssh`"),
        codes: &["invalid_args", "bad_jump", "refused"],
        fields: &["command"],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::dial::terminal::answer(&r.args)
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    // 那台报来的终端 ⇒ 这台电脑上开着那条连接的进程链：`{terminals}` ⇒ `{chain:[{pid, name, start}…], why?, addr?}`
    //   （`dial/terminal_processes.rs`）。阻塞档：起一趟 PowerShell（连接表 ＋ 进程表四格）并等它退出。只在被问时答。
    CommandSpec {
        name: "terminal-processes",
        doc_anchor: Some("#### `terminal-processes`"),
        codes: &["bad_args"],
        fields: &["addr", "chain", "name", "pid", "start", "why"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::dial::terminal_processes::answer(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 此刻是哪个终端在显示这个会话：`{sid}` ⇒ `{terminals:[{ssh, activity}…], why?}`（`observe/session_terminals.rs`）。
    //   阻塞档：读 `/proc` ＋ 在 tmux 里时起一次 `tmux list-clients`。零定时器，只在被问时答。
    CommandSpec {
        name: "session-terminals",
        doc_anchor: Some("#### `session-terminals`"),
        codes: &["bad_args", "no_such_session", "failed", "child_timed_out"],
        fields: &[
            "activity",
            "clientAddr",
            "clientPort",
            "serverAddr",
            "serverPort",
            "ssh",
            "terminals",
            "why",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::feature_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 终端管理 L1 三条（`control/terminals.rs`）：只认名单里的终端（句柄 / sid），不收 tmux 目标串；送字送键过身份门。
    //   阻塞档（起 tmux）；只抓一次、只送一次，轮询归调用方。
    CommandSpec {
        name: "terminals-list",
        doc_anchor: Some("#### `terminals-list` / `terminal-preview` / `terminal-input`"),
        codes: &["invalid_args", "unobservable", "child_timed_out"],
        fields: &[
            "agent",
            "can",
            "client",
            "clients",
            "complete",
            "cwd",
            "end",
            "host",
            "input",
            "kind",
            "last_activity",
            "mine",
            "no",
            "preview",
            "program",
            "purpose",
            "session",
            "sid",
            "since",
            "started_by",
            "state",
            "terminal",
            "terminals",
            "title",
            "tmux_name",
        ],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::terminals::list_for_inbound(&r.args).map(Some)),
    },
    CommandSpec {
        name: "terminal-preview",
        doc_anchor: Some("#### `terminals-list` / `terminal-preview` / `terminal-input`"),
        codes: &[
            "bad_target",
            "invalid_args",
            "not_known",
            "ambiguous",
            "no_tmux",
            "no_server",
            "no_such_session",
            "capture_failed",
            "unobservable",
            "child_timed_out",
        ],
        fields: &[
            "capped",
            "captured_at",
            "color",
            "cols",
            "cursor",
            "lines",
            "rows",
            "screen",
            "scrollback",
            "scrollback_lines",
            "sid",
            "spans",
            "terminal",
            "text",
        ],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::terminals::preview_for_inbound(&r.args).map(Some)),
    },
    CommandSpec {
        name: "terminal-input",
        doc_anchor: Some("#### `terminals-list` / `terminal-preview` / `terminal-input`"),
        codes: &[
            "bad_target",
            "invalid_args",
            "no_tmux",
            "no_server",
            "no_such_session",
            "capture_failed",
            "unobservable",
            "child_timed_out",
        ],
        fields: &[
            "client",
            "enter",
            "key",
            "result",
            "screen",
            "seen_screen",
            "sid",
            "take",
            "terminal",
            "text",
            "why",
        ],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::terminals::input_for_inbound(&r.args).map(Some)),
    },
    // **起会话要一个终端名 —— 问这台**：`{cwd}`（`<项目名>-cc`）或 `{forkOf}`（`<…>-fork-cc`）⇒ `{name}`（按这台那张会话快照避让）。
    //   本体 `control/ccm/mod.rs::answer_terminal_name_mint`（这一版宿主只有 tmux）；阻塞档（快照问一次就起一次 `tmux`）。
    CommandSpec {
        name: "terminal-name-mint",
        doc_anchor: Some("#### `terminal-name-mint`"),
        codes: &["invalid_args", "child_timed_out"],
        fields: &["name"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::ccm::answer_terminal_name_mint(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // F04a：**第一条破坏性命令。** 三道门在 `control/gate::admit_destructive`，
    // 对句柄下手不对名字。⚠ monitor 侧改走这条路是 **F04b**（定框 C6 的顺序）。
    CommandSpec {
        name: "kill",
        doc_anchor: Some("#### `kill`"),
        codes: &[
            "invalid_args",
            "no_tmux",
            "no_such_session",
            "wrong_owner",
            "too_many_windows",
            "kill_failed",
            "child_timed_out",
        ],
        fields: &["bus", "client", "killed", "name", "session", "sid"],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::kill::kill_for_inbound(&r.args).map(Some)),
    },
    // 一批会话的停 / 起：每一个同单个那一条（`kill` · `launch` · `launch-render-cli` · `launch-local`），逐个答结局。
    //   阻塞档（逐个起 tmux）；要动 tmux / 读记录的几样由帧面那层壳交进去（`faces/session_batch_face.rs`）。
    CommandSpec {
        name: "sessions-stop",
        doc_anchor: Some("#### `sessions-where` / `sessions-stop` / `sessions-start`"),
        codes: &["invalid_args", "unobservable"],
        fields: &[
            "bus", "client", "cmd", "detail", "outcome", "results", "session", "sid", "sids", "why",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::session_batch_face::stop(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "sessions-where",
        doc_anchor: Some("#### `sessions-where` / `sessions-stop` / `sessions-start`"),
        codes: &["invalid_args", "unobservable"],
        fields: &[
            "client",
            "host",
            "names",
            "results",
            "sid",
            "sids",
            "standing",
            "terminal",
            "terminals",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::session_batch_face::where_(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "sessions-start",
        doc_anchor: Some("#### `sessions-where` / `sessions-stop` / `sessions-start`"),
        codes: &["invalid_args", "unobservable"],
        fields: &[
            "account",
            "client",
            "cmd",
            "configDir",
            "cwd",
            "defaultLauncher",
            "detail",
            "items",
            "kind",
            "launcher",
            "local",
            "mode",
            "model",
            "name",
            "outcome",
            "results",
            "session",
            "sid",
            "unavailable",
            "why",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::session_batch_face::start(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 换号重启（`control/session_restart.rs`）：查号 → 找终端 → 先压缩（可选，等摘要）→ 停旧 → 同一终端名用新号起 → 等报出，全在这台做完。
    //   可撤档：步与步之间撤单生效；停旧 ＋ 起新不可分（拿退出排空的票），开跑之后撤也做完。失败可带按码定形的 `data`。
    CommandSpec {
        name: "session-restart",
        doc_anchor: Some("#### `session-restart`"),
        codes: &[
            "invalid_args",
            "unobservable",
            "account_unavailable",
            "not_in_terminal",
            "ambiguous",
            "stop_failed",
            "start_failed",
        ],
        fields: &[
            "account",
            "agent",
            "arrive_within_ms",
            "client",
            "compact",
            "compact_first",
            "compact_within_ms",
            "configDir",
            "cwd",
            "defaultLauncher",
            "launcher",
            "local",
            "model",
            "models",
            "name",
            "names",
            "sid",
            "started",
            "terminal",
            "why",
        ],
        takes_input: true,
        run: Run::AsyncData(|r| {
            Box::pin(async move {
                crate::faces::session_restart_face::answer(r.args)
                    .await
                    .map(Some)
                    .map_err(|(code, message, data)| Fail {
                        code,
                        message,
                        data,
                    })
            })
        }),
    },
    CommandSpec {
        name: "launch",
        doc_anchor: Some("#### `launch`"),
        // 〔C4e 问 2〕+`wrong_owner`：`send-into` 过 `gate::admit`（§34 Gate 2），
        // 它真会回这个码，登记表原先漏了。由 `gate_tests.rs::every_command_that_passes_the_gate_lists_the_gates_codes` 从 gate.rs 源码派生钉住。
        codes: &[
            "invalid_args",
            "no_tmux",
            "no_such_session",
            "wrong_owner",
            "create_failed",
            "typed_unconfirmed",
            "child_timed_out",
        ],
        // 〔`K-P2` `D` 阶段第三拍 09-03〕8 → 11：`agent` / `width` / `height`。
        // 那三个是「ccm 的 `--tmux` 真的改走这条路」逼出来的 —— 本地那条编排里
        // `@ccm_agent` 与 `-x/-y` 一直都在，这一侧此前没有字段能表达它们
        // ⇒ 不补就是**静默丢修饰**。⚠ `avoid_collision` **不加**：撞名避让住在要搬的那一块
        // **之外**，而「撞了」这件事后端已经用 `created:false` 表达完了（`§15 裁五`）。
        fields: &[
            "agent", "ccm_sid", "client", "created", "cwd", "height", "mode", "name", "payload",
            "session", "typed", "width",
        ],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::launch::launch_for_inbound(&r.args).map(Some)),
    },
];
