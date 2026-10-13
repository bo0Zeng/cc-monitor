//! 命令表 · 资产：`assets-*` · `ext-*` · `skill-*` · `mcp-*` · `hooks-*` · `footprint-report` · `data-report` · `cc-bus-*`。

use crate::stream::inbound::doors::hub_here;
use crate::stream::inbound::spec::{arg, out, CommandSpec, Run};
use crate::stream::inbound::LocalFiles;

pub(super) const SPECS: &[CommandSpec] = &[
    // 「足迹」由这台后端出整份成品（申报表 ＋ 判定都在 `footprint/`）；
    //   本机那一栏 `client` 带 monitor 自己进程独有的几条事实（家目录 · agent 家 · PATH），`HostScope::Client` 那一族按它们解、这台 stat。只读，阻塞档。
    CommandSpec {
        name: "footprint-report",
        summary: "「足迹」由这台后端出整份成品",
        codes: &["bad_args", "failed"],
        fields: &[out("agent_home", "解析基准：这台那一家的家目录（展示用）"), arg("client", "可选"), out("home", "这台的家目录"), out("rows", "每个足迹一行（落在哪 · 是谁写的 · 怎么收）"), out("settings_scopes", "各层设置文件（用户 · 项目 · 本地）的读法与先后")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::footprint::answer(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 「文件与数据」那一份成品：同一份足迹按「改过你的文件 · 要装 · 有没有 tmux · 进角标几件」重排（`footprint/data.rs`）。
    //   入参同 `footprint-report`（本机那一栏带 `client`）。只读，阻塞档。
    CommandSpec {
        name: "data-report",
        summary: "「文件与数据」那一份成品",
        codes: &["bad_args", "failed"],
        fields: &[arg("client", "同 `footprint-report`（本机那一栏才带）"), out("changedFiles", "cc-monitor 写进你的文件的那几处 `{path, what, undo}`（`undo` ＝ 撤回在哪：设置窗的页 · 栏 · 锚点）"), out("chores", "「待办」里进角标的件数：要做 ＋ 要装 ＋ 待定，还没做完的"), out("home", "这台家目录（显示时 `~` 缩写按它）"), out("own", "cc-monitor 在这台自己家里放的每一样 `{id, what, path, dir, class, exists, size}`：`id` 闭集同 `~/.cc-monitor/` 下的契约常量；`what` ＝ 写好的那一句「是什么」（界面照抄）；`class` ＝ `truth`（删了会丢）· `cache`（能重建）；目录 `size` 为 `null`"), out("tmux", "这台有没有 tmux（查不动 ⇒ `null`）"), out("todo", "「待办」各件 `{id, kind, state, name, loc, said, why, steps, diff, copy, whole, wholeCovers, file, go, howUrl, mask, action}`：`kind` 闭集 `must` · `install` · `decide` · `installOptional` · `optional`；`state` 闭集 `todo` · `done` · `expired` · `blocked` · `declined`；`action` 闭集 `copyCommand` · `copySnippet` · `decide` · `locate` · `how` · `installFirst`；`diff` 每行 `{n, op, text}`（`op` ＝ `same` · `del` · `add`，加的那几行 `n` 为 `null`）；`mask` ＝ 显示时要遮住的那把钥匙")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::footprint::data_answer(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 首次运行「开始用 · 剩 N 步」那份数（`footprint/readiness.rs`）：三步各自由这台的事实打勾；机器表台数由问的那一方带上。只读，阻塞档。
    crate::stream::inbound::typed::with_files::<crate::faces::first_run_face::FirstRunCmd>("首次运行「开始用」三步各自打没打勾", &LocalFiles),
    // 离线那台的上次值（`footprint/last_seen.rs`）：本机后端替界面记下每台最近一次读成的账号清单 · 「文件与数据」那一份，
    //   断开时界面照它画「上次的」，跨重启还在。写后端自己的 `~/.cc-monitor/last-seen.json`（锁里读—改—写、原子写）。阻塞档。
    crate::stream::inbound::typed::blocking::<crate::faces::footprint_face::LastSeenReadCmd>("读离线那台的上次值"),
    crate::stream::inbound::typed::owning::<crate::faces::footprint_face::LastSeenWriteCmd>("记下一台这一次读成的那一份（或清掉那台）"),
    // 「待办」里界面记下的两样选择（不用了 / 还是要做 · 我自己贴 / 改回让 cc-monitor 接上）：写后端自己的 `~/.cc-monitor/chores.json`
    //   （`footprint/chores/marks.rs`，读—改—写在跨进程锁里、原子写）。一个用户文件都不写。阻塞档（同步文件 I/O）。
    crate::stream::inbound::typed::owning::<crate::faces::footprint_face::ChoresMarkCmd>("记下「待办」里的一个选择"),
    // 设置里换 agent 家目录存之前那一问：在不在 · 是不是目录 · 有没有那一家的记录树，回一个码（`footprint/agent_home_check.rs`）。只读，阻塞档（stat）。
    crate::stream::inbound::typed::blocking::<crate::faces::footprint_face::AgentHomeCheckCmd>("那个目录能不能当 agent 家目录"),
    // **资产目录**：这台现扫一次 skill 与项目级 MCP、记进后端自有的
    //   `~/.cc-monitor/assets-catalog.json`（自家状态，经 `Own`，变了才写）、回整份目录 ＋「这台缺什么」的判定。
    //   `-merge` 那条再把另一台后端的整份并进来（同一台取 `gen` 大的整份）。一个用户文件都不写。阻塞档（扫盘）。
    crate::stream::inbound::typed::owning::<crate::faces::catalog_face::AssetsCatalogCmd>("这台现扫一次、记下，回整份资产目录 ＋「这台缺什么」"),
    crate::stream::inbound::typed::owning::<crate::faces::catalog_face::AssetsCatalogMergeCmd>("把另一台后端的整份目录并进来"),
    // **资产目录的自动同步**：本机常驻后端沿池里那条 SSH 连接（多开一个 exec 通道，零新连接）
    //   拉远端的目录、并进本机、把远端缺的推过去。写口（`answer_merge`）由这扇门递进去 —— `asset_sync.rs`
    //   自己不直呼它（写口只从本族进来：`Own` 由分派那一层递）。真异步（拨号 / 等远端）。
    CommandSpec {
        name: "assets-sync",
        summary: "本机常驻后端沿池里那条 SSH 同步资产目录",
        // `unreachable`：只给 `origin`（界面直问）而可达表里还没有那一台。
        codes: &["bad_args", "io_failed", "unreachable"],
        fields: &[arg("dial", "可缺席"), arg("origin", "可缺席"), out("reach", "可达表 `[{origin, machine}]`：`machine` 是那台目录的 id（还没拉成过 ⇒ `null`）—— 界面据它把目录里的机器 id 对回 origin"), out("self", "本机目录的 id（开头那一次现扫拿到的）—— 界面据它把目录里本机那一格对回 `<local>`"), out("synced", "每一趟一行 `{origin, peer, changed, pushed, error}`：`peer` 是那台目录的 `self`")],
        takes_input: true,
        run: Run::AsyncOwn(|r, own| {
            Box::pin(async move {
                let fold: crate::assets::asset_sync::Fold = std::sync::Arc::new(move |a| {
                    let req = crate::stream::inbound::typed::take(a)?;
                    crate::assets::asset_catalog::answer_merge(req, &own)
                        .and_then(|r| crate::stream::inbound::spec::wire(&r))
                });
                crate::assets::asset_sync::answer(
                    &r.args,
                    fold,
                    &crate::dial::remote_ask::DialRemote,
                )
                .await
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    // **skill「装到这台」**：`skill-read` 在来源那台读出这个 skill 的全部文件（原文 ＋ 执行位）；
    //   `skill-install-plan` 在要被写的那一台判 —— 差异四态与「不同的要显式说盖」那道闸原样用 AS1 的 `mcp_sync::{diff, plan}`，
    //   可疑项（可执行 · 二进制 · 绝对路径 · `#!` 要的命令）带那台的事实。两条都只读；写经 `files-put`（CAS）。阻塞档（扫盘）。
    crate::stream::inbound::typed::blocking::<crate::faces::hub_inner_face::SkillReadCmd>("读来源那台上的一个 skill"),
    crate::stream::inbound::typed::blocking::<crate::faces::hub_inner_face::SkillPlanCmd>("在要被写的那一台判 skill 装不装得过来"),
    // 装记录：`skill-install-record` 是 `~/.cc-monitor/skill-installs.json` 的写口（自家状态，经 `Own`；
    //   skill 装完记 `add` · 卸掉的摘 `drop`；MCP 那一条 `mcp-add` / `mcp-drop`）。卸在 `ext-uninstall-*`（判 · 删 · 摘同一台）。阻塞档。
    crate::stream::inbound::typed::owning::<crate::faces::catalog_face::SkillInstallRecordCmd>("skill 装记录的写口"),
    // MCP 列表成品：user / local / project 三段 ＋ 用过的项目目录 ＋ 读不出来的那几份。阻塞档（同步文件 I/O）。
    CommandSpec {
        name: "mcp-read",
        summary: "这台机器的 MCP 列表成品",
        codes: &["bad_args", "too_large"],
        fields: &[out("dirs", "agent 用户级配置里那张项目表的键（排序）：用过的项目目录"), out("entries", "`{scope, name, server, sourcePath, status, loginIn, seenAt, mark, login, failed}`：`scope` 闭集 `user` · `local` · `project`；`server` 原样（未知字段不丢）"), out("failed", "这台此刻活着的会话里那一家说它断开 ⇒ 最近说它的那一条写好的字 `{said, tip, detail}`（`said` 抽屉那一句：会话名 · 看的那一台钟上的时刻；`tip` 悬停那一截；`detail` 那一家的原话，没写 ⇒ `null`）；没有活会话说 · 停用了 ⇒ `null`"), out("login", "`status` 是 `needsLogin` 时写好的字 `{said, tip, copy}`（`said` 抽屉那一句：号名 · 看的那一台钟上的时刻；`tip` 悬停那一截；`copy` 在那一家会话里登录要敲的那条命令）；别的 ⇒ `null`"), out("loginIn", "`status` 是 `needsLogin` 时：在哪几个号里要登录（账号库里的名字，排序；没设账号的那一份不出名字）；别的状态恒空"), out("mark", "小标画哪一种：`failed`（有 `failed`，压过需登录）· `needsLogin`（有 `login`）· `null`（都没有：只说装了，不说连上了）"), out("name", "server 名"), out("problems", "在而读不出 / 不是 JSON 的那几份各一句（「这台没有」与「那份坏了」不合成一句）；不在的静默"), arg("projectDir", "可缺席 / `null`"), out("scope", "`user` · `local` · `project`"), out("seenAt", "`status` 是 `needsLogin` 时：最近一次看到是何时（epoch ms）；别的状态 `null`"), out("server", "那一条配置原样（未知字段不丢）"), out("sourcePath", "读自哪一份文件"), out("status", "闭集：`needsLogin`（agent 最近一次连它时要登录、还在有效期内）· `disabled`（这个项目里停用了）· `unknown`（配置层判不出：不说连上了）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::feature_face::answer(&r.cmd, &r.args, &r.tz)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // cc-bus 装到这台（扩展页经枢纽问被写那台）：`assets/cc_bus_install.rs`；写经 [`LocalFiles`]，装记录写口由本门递进去（`Own` 由分派那一层递）。
    crate::stream::inbound::typed::blocking::<crate::faces::hub_inner_face::CcBusStateCmd>("装 cc-bus 到这台之前看一眼"),
    crate::stream::inbound::typed::with_own::<crate::faces::hub_inner_face::CcBusInstallCmd>("把这台二进制带着的 cc-bus 装到这台", &LocalFiles),
    // 两台之间「装」那一件的枢纽（skill 与 MCP 同一对）：界面只问本机一次，本机后端向来源那台取、交被写那台判与写（`assets/hub.rs`）。
    //   依赖本进程的可达表 ⇒ 只在流面上（`cli_control::STREAM_ONLY`）。
    crate::stream::inbound::typed::async_owning::<crate::faces::ext_hub_face::ExtHubPreview>("装到几台之前那一张卡：本机后端当枢纽，向各台问完并好", hub_here),
    crate::stream::inbound::typed::async_owning::<crate::faces::ext_hub_face::ExtHubApply>("装到几台，本机后端当枢纽；各台各自结局", hub_here),
    // 扩展页那张表：这台现扫一次、记下（资产目录的写口从这扇门递进去），各台目录合成「条目 × 机器」。
    //   读可达表认出每台的名字 ⇒ 只在流面上。
    CommandSpec {
        name: "ext-list",
        summary: "设置「扩展」页那张表",
        codes: &["bad_args", "catalog_unreadable", "io_failed"],
        fields: &[out("machines", "每台一列 `{key, here, reachable, name, projects}`：`key` = 枢纽认它的键（本机后端自己 = `null`"), out("problems", "这台扫的时候读不出来的那几份"), out("rows", "每个条目一行 `{kind, name, about, detail, new, builtin, note, cells}`，`cells` 与 `machines` 同序"), arg("visit", "可缺席：`true` = 这一问算「来看了一次」（扩展页每次变可见时的第一问）—— 「新见到」按上一次来看算")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            crate::assets::ext::answer_list(
                &r.args,
                &|visit| crate::assets::asset_catalog::answer_current(visit, own),
                &crate::assets::ext::reach_of(&crate::dial::remote_ask::REACH),
            )
            .map(Some)
        }),
    },
    // 〔第二个前端 2026-10-08〕扩展页那张表的**本机那一半**：同一份 `assets::ext::answer_list`，只是目录先裁到这台自己那一格。
    //   凭据在 `ext::columns`：本机那一列**压根不问可达表**（`here` 为真 ⇒ `reachable` 恒真、`key` 恒 `null`、`name` 取目录里那台自报的 label），
    //   可达表只被**别的台**那几列用来认名字与连没连上。裁完一列不剩 ⇒ 可达表无从参与 ⇒ 这一条在一次性进程里与常驻那一个**答得一样真**，
    //   照派生规则（`cli_control::cli_exposed`）上 CLI 面。
    //   跨机那一整张（别的台叫什么 · 连没连上 · 「装到…」从哪台拿哪一版）仍只在 `ext-list`（`cli_control::STREAM_ONLY`，理由在 `cli_control_tests::NOT_ON_CLI`）。
    //   ⚠ 不是把 `ext-list` 加一个参数：上不上 CLI 面按**命令名**派生，同一条命令没法「带这个参数才上」。
    CommandSpec {
        name: "ext-list-here",
        summary: "设置「扩展」页那张表的本机那一半（不问可达表）",
        codes: &["bad_args", "catalog_unreadable", "io_failed"],
        fields: &[out("machines", "恒一列（这台自己）`{key: null, here: true, reachable: true, name, projects}`：裁掉了别的台，于是没有一格要问可达表"), out("problems", "同 `ext-list`：这台扫的时候读不出来的那几份"), out("rows", "同 `ext-list` 每个条目一行，`cells` 与 `machines` 同序 ⇒ 恒一格"), arg("visit", "同 `ext-list`：`true` = 这一问算「来看了一次」")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            // 裁到这台那一格再交给同一个本体。`answer_current` 仍是那扇门（现扫 ＋ 记下整份目录，别的台那几格一个字节不动）。
            let here = |visit: bool| {
                let (mut cat, problems) = crate::assets::asset_catalog::answer_current(visit, own)?;
                let me = cat.self_id.clone();
                cat.machines.retain(|id, _| *id == me);
                Ok((cat, problems))
            };
            crate::assets::ext::answer_list(&r.args, &here, &[]).map(Some)
        }),
    },
    // 用户写 / 改 / 清一个条目的备注：资产目录的写口从这扇门递进去（记进本机自己那一格，随目录同步）。
    crate::stream::inbound::typed::owning::<crate::faces::catalog_face::ExtNoteSetCmd>("写 / 改 / 清一个扩展的备注"),
    // 从这台卸一个扩展：装记录里有 ⇒ 只撤装时写的；没有 ⇒ 先挪进 `~/.cc-monitor/backups/` 再删。判与写都在这台（写经 [`LocalFiles`]，
    //   装记录的写口从这扇门递进去）。
    CommandSpec {
        name: "ext-uninstall-preview",
        summary: "从这台卸一个扩展之前那张卡",
        codes: &[
            "bad_args",
            "bad_file",
            "bad_path",
            "io_failed",
            "ledger_unreadable",
            "not_found",
            "refused",
        ],
        fields: &[arg("at", "在这台哪一级（用户级 MCP：这台有账号库 ⇒ 从各账号共用的那一份里删、所有号一起撤；没有 ⇒ 只读、`refused`）"), out("backup", "不是 cc-monitor 装的：删之前先放到哪（`~/.cc-monitor/backups/`）；否则 `null`"), out("files", "要删的那几个（skill：相对路径；MCP：那一条）"), arg("kind", "种类"), arg("name", "名字"), out("path", "skill 目录 / MCP 配置文件"), out("recorded", "装记录里有（cc-monitor 装的）⇒ 只撤装时写进去的；没有 ⇒ 不是 cc-monitor 装的"), out("said", "这一趟会做什么（说人话：改过没有 · 删了回不回得去）"), out("token", "看到的那一份的记号 —— 卸的时候原样交回")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::assets::ext::answer_uninstall_preview(
                &LocalFiles,
                &crate::assets::ext::Env::here(),
                &r.args,
            )
            .map(Some)
        }),
    },
    CommandSpec {
        name: "ext-uninstall-apply",
        summary: "从这台卸一个扩展",
        codes: &[
            "bad_args",
            "bad_file",
            "bad_path",
            "io_failed",
            "ledger_unreadable",
            "needs_consent",
            "not_found",
            "refused",
            "stale",
        ],
        fields: &[arg("at", "同 `ext-uninstall-preview`"), out("changed", "删了的那几个"), arg("kind", "同 `ext-uninstall-preview`"), arg("name", "同 `ext-uninstall-preview`"), out("note", "要知道的一件（挪 / 抄到了哪 · 没从装记录里摘掉 · 空目录没收掉）"), out("path", "卸的是哪"), arg("token", "卡上那一份：现在对不上 ⇒ `stale`、一个字节不动")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            crate::assets::ext::answer_uninstall_apply(
                &LocalFiles,
                &crate::assets::ext::Env::here(),
                &|a| crate::assets::skill_ledger::answer_record(a, own),
                &r.args,
            )
            .map(Some)
        }),
    },
    // 〔D 组〕MCP 推 / 拉：每一问只在一台上（`assets/mcp_sync_flow.rs`）；判定原样是 `mcp_sync::answer_with`，写经 [`LocalFiles`]。
    crate::stream::inbound::typed::with_files::<crate::faces::hub_inner_face::McpSourceCmd>("装到别的机器时来源那一条", &LocalFiles),
    crate::stream::inbound::typed::with_files::<crate::faces::hub_inner_face::McpPreviewCmd>("在要被写的那一台看装上之后那一条", &LocalFiles),
    crate::stream::inbound::typed::with_own::<crate::faces::hub_inner_face::McpApplyCmd>("在要被写的那一台把那一条写进去", &LocalFiles),
    // 〔D 组〕skill 装 / 卸的写那一半（`assets/skill_flow.rs`）：判（`skill_install`）· 写（[`LocalFiles`]）·
    //   记（`skill_ledger::answer_record`，写口只从这扇门递进去）同一台。
    crate::stream::inbound::typed::with_own::<crate::faces::hub_inner_face::SkillApplyCmd>("在要被写的那一台把勾的那几个 skill 文件写进去", &LocalFiles),
    // cc-bus 钩子诊断：这台自己的 `settings.json` ＋ stat ⇒ 诊断 ＋ 两种待贴片段（`observe/cc_bus_hooks.rs`）。阻塞档（同步文件 I/O）。
    CommandSpec {
        name: "hooks-diag",
        summary: "cc-bus 钩子诊断",
        codes: &["failed", "too_large"],
        fields: &[out("command", "钩子原文（去首尾空白）"), out("diagnosis", "`session_start`（→ `cc-register`）· `stop`（→ `cc-bus-stop-hook`）各一态 ＋ `note`（读不到 / 坏 JSON / 顶层不是对象时说原因，否则空串）"), out("kind", "一态：`not-installed` · `installed-via-path` · `installed-at-path` · `path-missing` · `unknown`"), out("note", "读不到 / 坏 JSON / 顶层不是对象时说原因，否则空串"), out("path", "钩子点名的路径（原样，环境变量不展开）"), out("session_start", "`SessionStart` 钩子（→ `cc-register`）那一态"), out("snippet", "要合并进那份文件的内容：两条钩子直接指向这台 cc-bus 的两个脚本（家目录底下写成相对家目录的形，否则绝对路径），不依赖 `PATH`"), out("source", "读的是哪份文件：这台后端的 agent 配置根下的设置文件"), out("stop", "`Stop` 钩子（→ `cc-bus-stop-hook`）那一态"), out("supported", "这台跑得了 cc-bus（它要 tmux；这份后端编到的平台没有原生 tmux ⇒ `false`，`snippet` 恒 `null`，界面只说这台不支持自动收信）")],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::faces::feature_face::answer(&r.cmd, &r.args, &r.tz)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
];
