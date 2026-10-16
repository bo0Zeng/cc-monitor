//! 命令表 · 资产：`assets-*` · `ext-*` · `skill-*` · `mcp-*` · `hooks-*` · `footprint-report` · `data-report` · `cc-bus-*`。

use crate::wire::spec::CommandSpec;

pub(super) const SPECS: &[CommandSpec] = &[
    // 「足迹」由这台后端出整份成品（申报表 ＋ 判定都在 `footprint/`）；
    //   本机那一栏 `client` 带 monitor 自己进程独有的几条事实（家目录 · PATH），`HostScope::Client` 那一族按它们解、这台 stat。只读，阻塞档。
    crate::wire::typed::blocking::<crate::faces::footprint_report_face::FootprintCmd>(
        "「足迹」由这台后端出整份成品",
    ),
    // 「文件与数据」那一份成品：同一份足迹按「改过你的文件 · 要装 · 有没有 tmux · 进角标几件」重排（`footprint/data.rs`）。
    //   入参同 `footprint-report`（本机那一栏带 `client`）。只读，阻塞档；别名块的候选经文件管理面读。
    crate::wire::typed::blocking::<crate::faces::footprint_report_face::DataReportCmd>(
        "「文件与数据」那一份成品",
    ),
    // 首次运行「开始用 · 剩 N 步」那份数（`footprint/readiness.rs`）：三步各自由这台的事实打勾；机器表台数由问的那一方带上。只读，阻塞档。
    crate::wire::typed::blocking::<crate::faces::first_run_face::FirstRunCmd>(
        "首次运行「开始用」三步各自打没打勾",
    ),
    // 离线那台的上次值（`footprint/last_seen.rs`）：本机后端替界面记下每台最近一次读成的账号清单 · 「文件与数据」那一份，
    //   断开时界面照它画「上次的」，跨重启还在。写后端自己的 `~/.cc-monitor/last-seen.json`（锁里读—改—写、原子写）。阻塞档。
    crate::wire::typed::blocking::<crate::faces::footprint_face::LastSeenReadCmd>(
        "读离线那台的上次值",
    ),
    crate::wire::typed::blocking::<crate::faces::footprint_face::LastSeenWriteCmd>(
        "记下一台这一次读成的那一份（或清掉那台）",
    ),
    // 「待办」里界面记下的两样选择（不用了 / 还是要做 · 我自己贴 / 改回让 cc-monitor 接上）：写后端自己的 `~/.cc-monitor/chores.json`
    //   （`footprint/chores/marks.rs`，读—改—写在跨进程锁里、原子写）。一个用户文件都不写。阻塞档（同步文件 I/O）。
    crate::wire::typed::blocking::<crate::faces::footprint_face::ChoresMarkCmd>(
        "记下「待办」里的一个选择",
    ),
    // 设置里换 agent 家目录存之前那一问：在不在 · 是不是目录 · 有没有那一家的记录树，回一个码（`footprint/agent_home_check.rs`）。只读，阻塞档（stat）。
    crate::wire::typed::blocking::<crate::faces::footprint_face::AgentHomeCheckCmd>(
        "那个目录能不能当 agent 家目录",
    ),
    // **资产目录**：这台现扫一次 skill 与项目级 MCP、记进后端自有的
    //   `~/.cc-monitor/assets-catalog.json`（自家状态，经 `Own`，变了才写）、回整份目录 ＋「这台缺什么」的判定。
    //   `-merge` 那条再把另一台后端的整份并进来（同一台取 `gen` 大的整份）。一个用户文件都不写。阻塞档（扫盘）。
    crate::wire::typed::blocking::<crate::faces::catalog_face::AssetsCatalogCmd>(
        "这台现扫一次、记下，回整份资产目录 ＋「这台缺什么」",
    ),
    crate::wire::typed::blocking::<crate::faces::catalog_face::AssetsCatalogMergeCmd>(
        "把另一台后端的整份目录并进来",
    ),
    // **资产目录的自动同步**：本机常驻后端沿池里那条 SSH 连接（多开一个 exec 通道，零新连接）
    //   拉远端的目录、并进本机、把远端缺的推过去。写口（`answer_merge`）由这扇门递进去 —— `asset_sync.rs`
    //   自己不直呼它（写口只从本族进来：`Own` 由分派那一层递）。真异步（拨号 / 等远端）。
    crate::wire::typed::asynchronous::<crate::faces::assets_sync_face::AssetsSyncCmd>(
        "本机常驻后端沿池里那条 SSH 同步资产目录",
    ),
    // **skill「装到这台」**：`skill-read` 在来源那台读出这个 skill 的全部文件（原文 ＋ 执行位）；
    //   `skill-install-plan` 在要被写的那一台判 —— 差异四态与「不同的要显式说盖」那道闸原样用 AS1 的 `mcp_sync::{diff, plan}`，
    //   可疑项（可执行 · 二进制 · 绝对路径 · `#!` 要的命令）带那台的事实。两条都只读；写经 `Door::put`（CAS）。阻塞档（扫盘）。
    crate::wire::typed::blocking::<crate::faces::hub_inner_face::SkillReadCmd>(
        "读来源那台上的一个 skill",
    ),
    crate::wire::typed::blocking::<crate::faces::hub_inner_face::SkillPlanCmd>(
        "在要被写的那一台判 skill 装不装得过来",
    ),
    // 装记录：`skill-install-record` 是 `~/.cc-monitor/skill-installs.json` 的写口（自家状态，经 `Own`；
    //   skill 装完记 `add` · 卸掉的摘 `drop`；MCP 那一条 `mcp-add` / `mcp-drop`）。卸在 `ext-uninstall-*`（判 · 删 · 摘同一台）。阻塞档。
    crate::wire::typed::blocking::<crate::faces::catalog_face::SkillInstallRecordCmd>(
        "skill 装记录的写口",
    ),
    // MCP 列表成品：user / local / project 三段 ＋ 用过的项目目录 ＋ 读不出来的那几份。阻塞档（同步文件 I/O）。
    crate::wire::typed::blocking::<crate::faces::feature_face::McpReadCmd>(
        "这台机器的 MCP 列表成品",
    ),
    // cc-bus 装到这台（扩展页经枢纽问被写那台）：`assets/cc_bus_install.rs`；写经 [`LocalFiles`]，装记录写口由本门递进去（`Own` 由分派那一层递）。
    crate::wire::typed::blocking::<crate::faces::hub_inner_face::CcBusStateCmd>(
        "装 cc-bus 到这台之前看一眼",
    ),
    crate::wire::typed::blocking::<crate::faces::hub_inner_face::CcBusInstallCmd>(
        "把这台二进制带着的 cc-bus 装到这台",
    ),
    // 两台之间「装」那一件的枢纽（skill 与 MCP 同一对）：界面只问本机一次，本机后端向来源那台取、交被写那台判与写（`assets/hub.rs`）。
    //   依赖本进程的可达表 ⇒ 只在流面上（`stream::cli::STREAM_ONLY`）。
    crate::wire::typed::asynchronous::<crate::faces::ext_hub_face::ExtHubPreview>(
        "装到几台之前那一张卡：本机后端当枢纽，向各台问完并好",
    ),
    crate::wire::typed::asynchronous::<crate::faces::ext_hub_face::ExtHubApply>(
        "装到几台，本机后端当枢纽；各台各自结局",
    ),
    // 扩展页那张表：这台现扫一次、记下（资产目录的写口从这扇门递进去），各台目录合成「条目 × 机器」。
    //   读可达表认出每台的名字 ⇒ 只在流面上。
    crate::wire::typed::blocking::<crate::faces::ext_list_face::ExtListCmd>("设置「扩展」页那张表"),
    // 〔第二个前端 2026-10-08〕扩展页那张表的**本机那一半**：同一份 `assets::ext::answer_list`，只是目录先裁到这台自己那一格。
    //   凭据在 `ext::columns`：本机那一列**压根不问可达表**（`here` 为真 ⇒ `reachable` 恒真、`key` 恒 `null`、`name` 取目录里那台自报的 label），
    //   可达表只被**别的台**那几列用来认名字与连没连上。裁完一列不剩 ⇒ 可达表无从参与 ⇒ 这一条在一次性进程里与常驻那一个**答得一样真**，
    //   照派生规则（`stream::cli::cli_exposed`）上 CLI 面。
    //   跨机那一整张（别的台叫什么 · 连没连上 · 「装到…」从哪台拿哪一版）仍只在 `ext-list`（`stream::cli::STREAM_ONLY`，理由在 `stream::cli` 判据的 `NOT_ON_CLI`）。
    //   ⚠ 不是把 `ext-list` 加一个参数：上不上 CLI 面按**命令名**派生，同一条命令没法「带这个参数才上」。
    crate::wire::typed::blocking::<crate::faces::catalog_face::ExtListHereCmd>(
        "设置「扩展」页那张表的本机那一半（不问可达表）",
    ),
    // 用户写 / 改 / 清一个条目的备注：资产目录的写口从这扇门递进去（记进本机自己那一格，随目录同步）。
    crate::wire::typed::blocking::<crate::faces::catalog_face::ExtNoteSetCmd>(
        "写 / 改 / 清一个扩展的备注",
    ),
    // 从这台卸一个扩展：装记录里有 ⇒ 只撤装时写的；没有 ⇒ 先挪进 `~/.cc-monitor/backups/` 再删。判与写都在这台（写经 [`LocalFiles`]，
    //   装记录的写口从这扇门递进去）。
    crate::wire::typed::blocking::<crate::faces::ext_list_face::ExtUninstallPreviewCmd>(
        "从这台卸一个扩展之前那张卡",
    ),
    crate::wire::typed::blocking::<crate::faces::ext_list_face::ExtUninstallApplyCmd>(
        "从这台卸一个扩展",
    ),
    // 〔D 组〕MCP 推 / 拉：每一问只在一台上（`assets/mcp_sync_flow.rs`）；判定原样是 `mcp_sync::answer_with`，写经 [`LocalFiles`]。
    crate::wire::typed::blocking::<crate::faces::hub_inner_face::McpSourceCmd>(
        "装到别的机器时来源那一条",
    ),
    crate::wire::typed::blocking::<crate::faces::hub_inner_face::McpPreviewCmd>(
        "在要被写的那一台看装上之后那一条",
    ),
    crate::wire::typed::blocking::<crate::faces::hub_inner_face::McpApplyCmd>(
        "在要被写的那一台把那一条写进去",
    ),
    // 〔D 组〕skill 装 / 卸的写那一半（`assets/skill_flow.rs`）：判（`skill_install`）· 写（[`LocalFiles`]）·
    //   记（`skill_ledger::answer_record`，写口只从这扇门递进去）同一台。
    crate::wire::typed::blocking::<crate::faces::hub_inner_face::SkillApplyCmd>(
        "在要被写的那一台把勾的那几个 skill 文件写进去",
    ),
    // cc-bus 钩子诊断：这台自己的 `settings.json` ＋ stat ⇒ 诊断 ＋ 两种待贴片段（`observe/cc_bus_hooks.rs`）。阻塞档（同步文件 I/O）。
    crate::wire::typed::blocking::<crate::faces::feature_face::HooksDiagCmd>("cc-bus 钩子诊断"),
];
