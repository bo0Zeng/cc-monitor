//! 命令表 · 终端与会话：`terminal-*` · `terminals-list` · `session-terminals` · `launch*` · `kill` · `sessions-*`。

use crate::stream::inbound::spec::{arg, both, out, CommandSpec, Fail, Run};

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
        summary: "本机起会话那一行 `ccm …`",
        codes: &["bad_args", "refused", "account_unavailable"],
        fields: &[arg("account", "缺席（不表态：继承；接回那一形）· 同 `launch-render-cli`（`follow` 什么都没选上 ⇒ 也是不表态）"), arg("action", "`{\"kind\":\"new\"}` · `{\"kind\":\"resume\",\"sid\":…}` · `{\"kind\":\"attach\"}`（接回 `tmuxName` 那个会话，不起 agent）"), out("cmd", "那一行 `ccm …`"), out("configDir", "应答 `account` 里：那个号的配置目录"), arg("cwd", "只用来核「新起」那一格的目录在不在"), arg("defaultLauncher", "这一家 agent 的默认启动器（等于它就不吐 `--launcher`）"), both("kind", "`action` 的种类：`new` · `resume` · `attach`；`account` 的种类：`follow` · `base` · `named`"), arg("launcher", "自定义启动命令（空 = 没设）"), out("model", "应答 `account` 里：用的模型"), out("name", "`account` 为 `named` 时的号名；应答 `account` 里是实际用的号"), arg("tmuxName", "建进 tmux 时的会话名（界面铸名口铸的，这里不铸）；缺 ⇒ 直路")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::faces::launch_face::answer_local(&r.args)
                .map(Some)
                .map_err(failed)
        }),
    },
    CommandSpec {
        name: "launch-render-cli",
        summary: "远端起会话那一行 `ccm …`",
        codes: &["bad_args", "refused", "account_unavailable"],
        fields: &[arg("account", "`{\"kind\":\"follow\"}` · `{\"kind\":\"base\"}` · `{\"kind\":\"named\",\"name\":…}`（按名字判）—— 生成的类型 `AccountAsk`"), arg("action", "`{\"kind\":\"new\"}` · `{\"kind\":\"resume\",\"sid\":…}` · `{\"kind\":\"attach\",\"name\":…}`"), arg("ccmSid", "这次拉起的修饰（`deny_unknown_fields`：多送一格就拒）；`model` = 显式指定的模型（压过 `models`）"), out("cmd", "那一行 `ccm …`"), out("configDir", "应答 `account` 里：那个号的配置目录"), arg("container", "`{\"kind\":\"none\"}`（直路）· `{\"kind\":\"tmux\",\"name\":…,\"send_into\":bool}`（建会话 / 键进已有 pane）"), arg("cwd", "这次拉起的修饰（`deny_unknown_fields`：多送一格就拒）；`model` = 显式指定的模型（压过 `models`）"), arg("defaultLauncher", "这次拉起的修饰（`deny_unknown_fields`：多送一格就拒）；`model` = 显式指定的模型（压过 `models`）"), both("kind", "`action` / `account` 的种类（同 `launch-local`）"), arg("launcher", "这次拉起的修饰（`deny_unknown_fields`：多送一格就拒）；`model` = 显式指定的模型（压过 `models`）"), arg("model", "这次拉起的修饰（`deny_unknown_fields`：多送一格就拒）；`model` = 显式指定的模型（压过 `models`）"), arg("models", "可缺：这台的模型偏好表（`{号: 模型}`，用户设置的原值）；判出来的号在表里 ⇒ `--model` 用那一条"), both("name", "号名（同 `launch-local`）")],
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
        summary: "给一台远端开终端要跑的那一串",
        codes: &["invalid_args", "bad_jump", "refused"],
        fields: &[both("command", "要在那台跑的命令；应答里是那一整行 PowerShell `& ssh -t … -- 'bash -lic …'`")],
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
    //   （`dial/terminal_processes.rs`）。阻塞档：直调系统接口读连接表 ＋ 进程表四格（每个进程开一次句柄问启动时刻）。只在被问时答。
    CommandSpec {
        name: "terminal-processes",
        summary: "那台报来的终端连接是这台电脑上哪个进程开的",
        codes: &["bad_args"],
        fields: &[out("addr", "`elsewhere` 时对面那个地址"), out("chain", "这台上开着那条连接的进程链（自下而上），每格 `{pid, name, start}`"), out("name", "进程名"), out("pid", "进程号"), out("start", "启动时刻（系统原值；拿不到 ⇒ 0）"), out("why", "对不上时：`mismatch`（经跳板 / 端口转换）· `elsewhere`（不是这台开的）…")],
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
        summary: "此刻是哪个终端在显示这个会话",
        codes: &["bad_args", "no_such_session", "failed", "child_timed_out"],
        fields: &[out("activity", "最近动静（Unix 秒；不在 tmux 里 ⇒ `null`）"), out("clientAddr", "对面地址"), out("clientPort", "对面端口"), out("serverAddr", "本机地址"), out("serverPort", "本机端口"), out("ssh", "那个终端的 `SSH_CONNECTION` 四段；不是经 ssh 连的 ⇒ `null`"), out("terminals", "此刻显示它的终端，每格 `{ssh, activity}`，最近动静在前，最多 16 格"), out("why", "一个都没有时：`detached` · `no-terminal` · `unreadable`")],
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
        summary: "这台的终端名单（形状与宿主无关；这一版宿主是 tmux）",
        codes: &["invalid_args", "unobservable", "child_timed_out"],
        fields: &[out("agent", "`session` 里：哪一家（没标就缺）"), out("can", "这个调用方能做什么：`preview` · `input` · `end`，做不了的写成 `{no: 原因}`"), both("client", "请求里可选：自报的前端，决定每行的 `mine` / `can`；`started_by` 里：会话上的 `@ccm_client`（没声明 ⇒ `null`）"), out("clients", "此刻连着它的终端客户端，每项 `{kind, since, last_activity}`；空 ＝ 后台"), out("complete", "`false` ＝ 名单里有读不懂的行（画「部分」）"), out("cwd", "当前目录"), out("end", "能不能结束（`{no: \"not-yours\" | \"not-managed\" | \"other-windows\"}`）"), out("host", "终端宿主（这一版是 `tmux`）"), out("input", "输入方式：`shared`（tmux：各端都能打字）"), out("kind", "`clients` 一项：客户端种类（`terminal-window` …）"), out("last_activity", "最近动静（秒）"), out("mine", "这个调用方能不能送字 / 结束"), out("no", "做不了的原因"), out("preview", "能不能抓屏"), out("program", "前台程序名"), out("purpose", "`normal` …"), out("session", "里面跑着会话（`@ccm_sid`）时才有：`{sid, agent?}`"), out("sid", "`session` 里：会话 id"), out("since", "`clients` 一项：连上的时刻（秒）"), out("started_by", "谁起的：`{client, mine}`"), out("state", "`running` · `idle`（没会话、前台是 shell）· `program-exited`（有会话、前台是 shell）"), out("terminal", "名单里那一行的不透明句柄（前端不拼、不解析；送字 / 抓屏时交回）"), out("terminals", "终端名单（每行一个终端）"), out("title", "窗格标题"), out("tmux_name", "tmux 会话名")],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::terminals::list_for_inbound(&r.args).map(Some)),
    },
    CommandSpec {
        name: "terminal-preview",
        summary: "抓一个终端的一屏（只抓一次，轮询归调用方）",
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
        fields: &[out("capped", "要的比上限多、截到了上限"), out("captured_at", "抓屏时刻（秒）"), arg("color", "要不要颜色；缺省 `true`"), out("cols", "列数"), out("cursor", "光标 `{x, y, visible}`"), out("lines", "自上而下的行，每行 `{text, spans?}`（往回要的在最前）"), out("rows", "行数"), out("screen", "这一屏的指纹（16 位十六进制）：内容一变就变，送字时带回来"), arg("scrollback", "往回多要几行；缺省 0、上限 2000"), out("scrollback_lines", "实际往回给了几行"), arg("sid", "目标：会话 id（与 `terminal` 恰给一个）"), out("spans", "着色段 `{from, to, fg?, bg?, bold?, dim?, italic?, underline?, inverse?}`；`color:false` ⇒ 不给"), arg("terminal", "目标：名单里的句柄（与 `sid` 恰给一个）"), out("text", "`lines` 一项：那一行的文字")],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::terminals::preview_for_inbound(&r.args).map(Some)),
    },
    CommandSpec {
        name: "terminal-input",
        summary: "往一个终端送字或送键（过身份门）",
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
        fields: &[arg("client", "自报的前端（过「哪个前端的会话」那一维）"), arg("enter", "`text` 之后补一个回车；缺省 `true`"), arg("key", "送键：`esc` · `ctrl-c` · `ctrl-d` · `up` · `down` · `left` · `right` · `tab` · `shift-tab` · `enter` · `backspace` · `page-up` · `page-down`"), out("result", "`delivered` · `unsure`（不知道送没送到，别重发）· `refused`"), out("screen", "`screen-changed` 时带的新指纹"), arg("seen_screen", "送之前看到的那一屏的指纹；画面已经变了 ⇒ 不送、回 `refused` ＋ `screen-changed`"), arg("sid", "目标：会话 id（与 `terminal` 恰给一个）"), arg("take", "要不要先接管输入（tmux 上无所谓，各端都能打字）"), arg("terminal", "目标：名单里的不透明句柄（前端不拼、不解析）"), arg("text", "送字：字面字，原样送、不解释成键名；多行按粘贴送（与 `key` 恰给一个）"), out("why", "`refused` 的原因：`not-known` · `ambiguous` · `ended` · `not-yours` · `not-managed` · `screen-changed`")],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::terminals::input_for_inbound(&r.args).map(Some)),
    },
    // **起会话要一个终端名 —— 问这台**：`{cwd}`（`<项目名>-cc`）或 `{forkOf}`（`<…>-fork-cc`）⇒ `{name}`（按这台那张会话快照避让）。
    //   本体 `control/ccm/mod.rs::answer_terminal_name_mint`（这一版宿主只有 tmux）；阻塞档（快照问一次就起一次 `tmux`）。
    CommandSpec {
        name: "terminal-name-mint",
        summary: "起会话要的终端名",
        codes: &["invalid_args", "child_timed_out"],
        fields: &[out("name", "铸出来的终端名（这台避让过）")],
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
        summary: "杀一个 tmux 会话",
        codes: &[
            "invalid_args",
            "no_tmux",
            "no_such_session",
            "wrong_owner",
            "too_many_windows",
            "kill_failed",
            "child_timed_out",
        ],
        fields: &[out("bus", "顺手从 cc-bus 注销的结果 `{removed, failed, unread}`"), arg("client", "自报的前端（过「哪个前端的会话」那一维）"), out("killed", "杀成了"), arg("name", "要杀的 tmux 会话名"), out("session", "杀掉的 tmux 会话名"), arg("sid", "可带：只结束挂着这个会话（`@ccm_sid`）的窗格")],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::kill::kill_for_inbound(&r.args).map(Some)),
    },
    // 一批会话的停 / 起：每一个同单个那一条（`kill` · `launch` · `launch-render-cli` · `launch-local`），逐个答结局。
    //   阻塞档（逐个起 tmux）；要动 tmux / 读记录的几样由帧面那层壳交进去（`faces/session_batch_face.rs`）。
    CommandSpec {
        name: "sessions-stop",
        summary: "停一批会话（逐个答，一个不成不挡下一个）",
        codes: &["invalid_args", "unobservable"],
        fields: &[out("bus", "同 `kill`"), arg("client", "自报的前端，同 `kill`"), both("cmd", "只有开终端那一形有"), out("detail", "那一个的原话"), out("outcome", "`done` · `skipped` · `failed`"), out("results", "逐个结果，与入参同序"), out("session", "落在哪个 tmux 会话上"), both("sid", "会话 id"), arg("sids", "要停的会话（1–64 个，不重复）"), out("why", "`skipped` / `failed` 的码")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::session_batch_face::stop(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "sessions-where",
        summary: "一批会话各在这台哪个终端里",
        codes: &["invalid_args", "unobservable"],
        fields: &[arg("client", "自报的前端，同 `kill`"), out("host", "终端宿主"), out("names", "带着它的 tmux 会话名"), out("results", "逐个结果，与入参同序"), both("sid", "会话 id"), arg("sids", "要问的会话（1–64 个，不重复）"), out("standing", "`running` · `ambiguous` · `idle` · `none` · `no_tmux`"), out("terminal", "名单里那一行的句柄"), out("terminals", "与 `names` 同序同数，每项 `{host, terminal}`")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::session_batch_face::where_(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "sessions-start",
        summary: "起 / 接回一批会话（逐个答）",
        codes: &["invalid_args", "unobservable"],
        fields: &[both("account", "那一项用哪个号：缺 ＝ 跟随 · `{kind:\"base\"}` · `{kind:\"named\", name}`；应答里是实际用的号"), arg("client", "自报的前端，同 `kill`"), both("cmd", "开终端那一形要跑的那一行"), out("configDir", "`account` 里：那个号的配置目录"), arg("cwd", "那一项的工作目录"), arg("defaultLauncher", "整批一份：那一家的默认启动器"), arg("fork_of", "可缺：源会话 sid（只许与 `fresh_terminal: true` 一起）"), out("detail", "那一个的原话"), arg("fresh_terminal", "可缺：分叉出来的那一条 ⇒ 必铸新终端名"), arg("items", "要起的会话，每项 `{sid, cwd, account?, fresh_terminal?, fork_of?}`"), both("kind", "`account` 的种类"), arg("launcher", "整批一份：用户设置的 resume 命令原值"), arg("local", "这台是不是界面所在那台（开终端那一形按它选本机 / 远端那一行）"), arg("mode", "`tmux`（在 tmux 里后台起）· `window`（只渲那一行交回，窗口由界面开）"), out("model", "`account` 里：用的模型"), both("name", "`account` 为 `named` 时的号名"), out("outcome", "`done` · `skipped` · `failed`"), out("results", "逐个结果，与入参同序"), out("session", "落在哪个 tmux 会话上"), both("sid", "会话 id"), out("unavailable", "选不了号的那一项：`{requested, pinned, listKnown, alternative}`"), out("why", "`skipped` / `failed` 的码")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::session_batch_face::start(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
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
        ],
        fields: &[arg("account", "可缺 ＝ 跟随（分叉跟源会话上次的号；新起的 ⇒ 这台的默认号）· `{kind:\"base\"}` · `{kind:\"named\", name}`"), arg("agent", "哪一家（线上的 kind）"), both("cmd", "`open` 时界面要在终端里跑的那一行"), arg("command", "启动命令；空 / 缺 ⇒ 那一家的默认启动器"), out("configDir", "应答 `account` 里：那个号的配置目录"), arg("cwd", "工作目录（开头的 `~` 按这台的家目录读）"), out("field", "失败时不行的那一格（`agent` · `command` · `cwd` · `account` · `place` · `tmuxName`；整体的 ⇒ `null`）"), arg("forkFrom", "可缺"), both("kind", "`account` 的种类：`follow` · `base` · `named`"), arg("local", "发请求的界面就在这台上（开窗那一形本机与远端渲法不同）"), out("model", "应答 `account` 里：用的模型"), arg("models", "可缺"), both("name", "`account` 为 `named` 时的号名；应答 `account` 里是实际用的号"), out("outcome", "`started`（tmux 里起好了，`session` 是会话名）· `open`（界面开一个终端跑 `cmd`）"), arg("place", "`tmux`（在这台 tmux 里后台起，关终端不断）· `window`（开一个新终端窗口直接跑）"), out("session", "`started` 时的 tmux 会话名"), out("sid", "分叉出来的新会话 sid；新起的 ⇒ `null`（报到之前说不出）"), arg("tmuxName", "可缺 ⇒ 这台铸"), out("unavailable", "`account_unavailable` 时那一形，带替代号"), arg("uuid", "`forkFrom` 里：从哪条消息处分叉")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::faces::session_new_face::answer(&r.args)
                .map(Some)
                .map_err(failed)
        }),
    },
    // 起新会话框打开时问一次：这台最近用过的目录 · 有没有 tmux · 能起哪几家 · 分叉源会话的三格（只读，不写分支记录）。
    CommandSpec {
        name: "session-new-facts",
        summary: "起新会话那个框要的事实：最近目录 · 有没有 tmux · 有哪几家 agent · 分叉时原会话的起法",
        codes: &["bad_args", "fork_failed"],
        fields: &[arg("agent", "哪一家（线上的 kind）"), out("agents", "这台能起的几家（注册表里由我们起的、默认启动器在这台 `PATH` 上找得到的；注册表序）"), arg("at", "可缺（随 `forkOf`）"), arg("cwd", "工作目录（开头的 `~` 按这台的家目录读）"), out("fork", "没给 `forkOf` ⇒ `null`"), arg("forkOf", "可缺"), out("lastMs", "`recent` 一项：那个目录最近一次会话的修改时刻（毫秒）"), out("launch", "`fork` 里：起分叉会话要的三格（同 `session-fork` 的 `launch`）"), out("recent", "这台最近用过的工作目录（各家记录里的，新的在前、同一个目录一次、最多 8 个；`lastMs` 是那个目录最近一次会话的修改时刻）"), out("start", "`fork` 里：那一轮你那句的时刻"), out("tmux", "这台有没有 tmux（`false` ⇒ 只能开终端窗口）"), out("turn", "`fork` 里：`at` 那一条在第几轮")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::session_new_face::facts(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 工作目录那一格失焦时问：在不在 · 这台此刻会给它铸的终端名（没 tmux ⇒ `null`）。
    CommandSpec {
        name: "session-new-dir",
        summary: "起新会话前核一个目录：在不在 · 会用哪个终端名",
        codes: &["bad_args"],
        fields: &[arg("cwd", "工作目录（开头的 `~` 按这台的家目录读）"), out("exists", "这个目录在不在"), arg("forkOf", "可缺"), arg("tmuxName", "可缺 ⇒ 这台铸")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::session_new_face::dir(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 动一个会话之前会打断什么（`observe/interrupts_query.rs`）：按族答，界面只画。只读。
    CommandSpec {
        name: "session-interrupts",
        summary: "动一个会话之前，会打断什么",
        codes: &["invalid_args", "failed"],
        fields: &[out("families", "按族的清单，空族不出现；一族都没有 ⇒ `[]`（界面直接做，不问）"), out("family", "`turn`（那个会话有一轮在跑：活着的 pidfile 里 `status` 是 `busy`）· `agent`（它派出去还在跑的子运行）· `task`（它任务表里 `in_progress` 的）"), out("names", "显示名：子运行的标签（无标签用种类 / 运行号）· 任务主题；`turn` 那一族为空表"), arg("sid", "会话 id（空 / 缺 ⇒ `invalid_args`）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::feature_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 换号重启（`control/session_restart.rs`）：查号 → 找终端 → 先压缩（可选，等摘要）→ 停旧 → 同一终端名用新号起 → 等报出，全在这台做完。
    //   可撤档：步与步之间撤单生效；停旧 ＋ 起新不可分（拿退出排空的票），开跑之后撤也做完。失败可带按码定形的 `data`。
    CommandSpec {
        name: "session-restart",
        summary: "换号重启",
        codes: &[
            "invalid_args",
            "unobservable",
            "account_unavailable",
            "not_in_terminal",
            "ambiguous",
            "session_already_live",
            "stop_failed",
            "start_failed",
        ],
        fields: &[both("account", "用户点名的号（只收名字）；应答里是实际用的号"), arg("agent", "同 `sessions-start` 的整批那几格"), arg("arrive_within_ms", "等新进程报出的期限（≤ 3 600 000）"), arg("client", "自报的前端，同 `kill`"), out("compact", "先压缩那一步：`done` · `timed_out` · `skipped` · `unsupported` · `failed`"), arg("compact_first", "先请求压缩、等记录里出现压缩摘要再停"), arg("compact_within_ms", "等压缩的期限（≤ 3 600 000）"), out("configDir", "应答 `account` 里：配置目录"), arg("cwd", "工作目录"), arg("defaultLauncher", "同 `sessions-start`"), arg("launcher", "同 `sessions-start`"), arg("local", "同 `sessions-start`"), out("model", "应答 `account` 里：模型"), arg("models", "可缺：这台的模型偏好表原值"), out("name", "应答 `account` 里：实际用的号"), out("names", "`ambiguous` 失败的 `data`：在跑的那几个终端名"), out("pids", "`session_already_live` 失败的 `data`：那几个 pid"), arg("sid", "会话 id"), out("started", "`arrived`（新进程报出了）· `missed`"), out("terminal", "所在的终端（会话名）"), out("why", "`stop_failed` / `start_failed` 失败的 `data`：那一步的码")],
        takes_input: true,
        run: Run::AsyncData(|r| {
            Box::pin(async move {
                crate::faces::session_restart_face::answer(r.args, r.until)
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
        summary: "建 tmux 会话并键入载荷，或键入一个已在的会话（远端执行面）",
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
        fields: &[arg("agent", "可选，仅 create-or-attach：注册表里的一家（空 ⇒ 默认那一家）"), arg("ccm_sid", "可选：create-or-attach 写成意图键 `@ccm_sid_expect`；send-into 键入挂着它的那个窗格"), arg("client", "可选：自报的前端（send-into 过「哪个前端的会话」那一维；create-or-attach 写成会话的 `@ccm_client`）"), out("created", "这一次新建了会话"), arg("cwd", "可选，仅 create-or-attach"), arg("height", "同 `width`"), arg("mode", "`create-or-attach`（建或接）· `send-into`（往已在的会话里键入）"), arg("name", "tmux 会话名"), arg("payload", "要键入的那一行"), out("session", "落在哪个会话上"), out("typed", "`send-keys` 退出 0：只有 `send-keys` 的退出码那么强（pane 在 copy-mode 时照样退 0、键被吃掉）"), arg("width", "可选，仅 create-or-attach：1–4 位十进制字符串，与 `height` 同时给或都不给")],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::launch::launch_for_inbound(&r.args).map(Some)),
    },
];
