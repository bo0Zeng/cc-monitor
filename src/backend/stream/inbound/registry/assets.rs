//! 命令表 · 资产：`assets-*` · `ext-*` · `skill-*` · `mcp-*` · `hooks-*` · `footprint-report` · `data-report` · `cc-bus-*`。

use crate::stream::inbound::doors::hub_here;
use crate::stream::inbound::spec::{CommandSpec, Run};
use crate::stream::inbound::LocalFiles;

pub(super) const SPECS: &[CommandSpec] = &[
    // 「足迹」由这台后端出整份成品（申报表 ＋ 判定都在 `footprint/`）；
    //   本机那一栏 `client` 带 monitor 自己进程独有的几条事实（家目录 · agent 家 · PATH），`HostScope::Client` 那一族按它们解、这台 stat。只读，阻塞档。
    //   〔墓碑 —— RM1a 那一版这里是 `footprint-probe`：只交路径事实，判定住 monitor。〕
    CommandSpec {
        name: "footprint-report",
        doc_anchor: Some("#### `footprint-report`"),
        codes: &["bad_args", "failed"],
        fields: &["client"],
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
        doc_anchor: Some("#### `data-report`"),
        codes: &["bad_args", "failed"],
        fields: &["client"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::footprint::data_answer(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // **MCP 资产同步的判定**（B，用户 09-24）：两份原文进、
    //   差异四态 ＋ 可疑项（带这台机器的事实）＋「写哪几条」出。由**要被写的那一台**跑（事实是那台的）。
    //   只读：原文由 monitor 经 `files-peek` 读来，写经 `files-put`（CAS）—— 本条一个字节都不落盘。阻塞档（`stat`）。
    CommandSpec {
        name: "mcp-sync-plan",
        doc_anchor: Some("#### `mcp-sync-plan`"),
        codes: &["bad_args", "bad_file", "needs_consent"],
        fields: &["overwrite", "rows", "source", "take", "target", "write"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::mcp_sync::answer(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // **资产目录**：这台现扫一次 skill 与项目级 MCP、记进后端自有的
    //   `~/.cc-monitor/assets-catalog.json`（第四层，变了才写）、回整份目录 ＋「这台缺什么」的判定。
    //   `-merge` 那条再把另一台后端的整份并进来（同一台取 `gen` 大的整份）。一个用户文件都不写。阻塞档（扫盘）。
    CommandSpec {
        name: "assets-catalog",
        doc_anchor: Some("#### `assets-catalog`"),
        codes: &["catalog_unreadable", "io_failed"],
        fields: &["changed", "machines", "path", "problems", "rows", "self"],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::assets::asset_catalog::answer_catalog(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "assets-catalog-merge",
        doc_anchor: Some("#### `assets-catalog-merge`"),
        codes: &["bad_args", "catalog_unreadable", "io_failed"],
        fields: &[
            "catalog", "changed", "machines", "path", "problems", "rows", "self",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::asset_catalog::answer_merge(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // **资产目录的自动同步**：本机常驻后端沿池里那条 SSH 连接（多开一个 exec 通道，零新连接）
    //   拉远端的目录、并进本机、把远端缺的推过去。写口（`answer_merge`）由这扇门递进去 —— `asset_sync.rs`
    //   自己不直呼它（`readonly_guard` 第四层 ④：写口只从本族进来）。真异步（拨号 / 等远端）。
    CommandSpec {
        name: "assets-sync",
        doc_anchor: Some("#### `assets-sync`"),
        // `unreachable`：只给 `origin`（界面直问）而可达表里还没有那一台。
        codes: &["bad_args", "io_failed", "unreachable"],
        fields: &["dial", "origin", "reach", "self", "synced"],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                let fold: crate::assets::asset_sync::Fold =
                    std::sync::Arc::new(crate::assets::asset_catalog::answer_merge);
                crate::assets::asset_sync::answer(
                    &r.args,
                    fold,
                    &crate::stream::remote_ask::DialRemote,
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
    CommandSpec {
        name: "skill-read",
        doc_anchor: Some("#### `skill-read`"),
        codes: &["bad_args", "io_failed", "not_found", "too_large"],
        fields: &["dir", "files", "name", "project", "root", "skipped"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::skill_install::answer_read(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "skill-install-plan",
        doc_anchor: Some("#### `skill-install-plan`"),
        codes: &[
            "bad_args",
            "bad_file",
            "io_failed",
            "needs_consent",
            "too_large",
        ],
        fields: &[
            "base",
            "dir",
            "name",
            "overwrite",
            "prefix",
            "project",
            "root",
            "rows",
            "source",
            "take",
            "target",
            "write",
            // 给了 `take` 才有：真要写的那几个的摘要 ＋ 装之前在不在（装完原样交回 `skill-install-record`）。
            "ledger",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::skill_install::answer_plan(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 装记录：`skill-install-record` 是 `~/.cc-monitor/skill-installs.json` 的写口（第四层；
    //   skill 装完记 `add` · 卸掉的摘 `drop`；MCP 那一条 `mcp-add` / `mcp-drop`）。卸在 `ext-uninstall-*`（判 · 删 · 摘同一台）。阻塞档。
    CommandSpec {
        name: "skill-install-record",
        doc_anchor: Some("#### `skill-install-record`"),
        codes: &[
            "bad_args",
            "io_failed",
            "ledger_unreadable",
            "not_found",
            "too_large",
        ],
        fields: &[
            "at",
            "changed",
            "digest",
            "dir",
            "file",
            "files",
            "name",
            "op",
            "paths",
            "project",
            "remaining",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::skill_ledger::answer_record(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // MCP 列表成品：user / local / project 三段 ＋ 用过的项目目录 ＋ 读不出来的那几份。阻塞档（同步文件 I/O）。
    CommandSpec {
        name: "mcp-read",
        doc_anchor: Some("#### `mcp-read`"),
        codes: &["bad_args", "too_large"],
        fields: &[
            "dirs",
            "entries",
            "name",
            "problems",
            "projectDir",
            "scope",
            "server",
            "sourcePath",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::feature_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔D 组〕项目 `.mcp.json` 增改 / 删：计算（`assets/mcp_edit.rs`）与写（本进程文件管理面 [`LocalFiles`]）在同一台。
    CommandSpec {
        name: "mcp-server-put",
        doc_anchor: Some("#### `mcp-server-put`"),
        codes: &["bad_args", "bad_path", "refused"],
        fields: &["changed", "name", "path", "projectDir", "server"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::mcp_edit::answer_put(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "mcp-server-remove",
        doc_anchor: Some("#### `mcp-server-remove`"),
        codes: &["bad_args", "bad_path", "refused"],
        fields: &["changed", "name", "path", "projectDir"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::mcp_edit::answer_remove(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // cc-bus 装到这台（扩展页经枢纽问被写那台）：`assets/cc_bus_install.rs`；写经 [`LocalFiles`]，装记录写口由本门递进去（第四层 ④）。
    CommandSpec {
        name: "cc-bus-install-state",
        doc_anchor: Some("#### `cc-bus-install-state`"),
        codes: &["refused"],
        fields: &["dest", "existing", "version", "writes"],
        takes_input: false,
        run: Run::Blocking(|_r| {
            crate::assets::cc_bus_install::answer_state()
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "cc-bus-install",
        doc_anchor: Some("#### `cc-bus-install`"),
        codes: &["bad_file", "refused"],
        fields: &["backup", "dest", "recordFailed", "written"],
        takes_input: false,
        run: Run::Blocking(|_r| {
            crate::assets::cc_bus_install::answer_install(
                &LocalFiles,
                &crate::assets::skill_ledger::answer_record,
            )
            .map(Some)
            .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 两台之间「装」那一件的枢纽（skill 与 MCP 同一对）：界面只问本机一次，本机后端向来源那台取、交被写那台判与写（`assets/hub.rs`）。
    //   依赖本进程的可达表 ⇒ 只在流面上（`cli_control::STREAM_ONLY`）。
    CommandSpec {
        name: "ext-hub-preview",
        doc_anchor: Some("#### `ext-hub-preview`"),
        codes: &[
            "bad_args",
            "bad_file",
            "missing",
            "refused",
            "unreachable",
            "io_failed",
        ],
        fields: &[
            "config",
            "from",
            "kind",
            "name",
            "path",
            "scope",
            "slots",
            "stop",
            "suspects",
            "to",
            "tokens",
            "unchanged",
            "writes",
        ],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::assets::hub::ext_preview(
                    &hub_here(),
                    &r.args,
                    &crate::stream::remote_ask::REACH,
                    &crate::stream::remote_ask::DialRemote,
                )
                .await
                .map(Some)
            })
        }),
    },
    CommandSpec {
        name: "ext-hub-apply",
        doc_anchor: Some("#### `ext-hub-apply`"),
        codes: &[
            "bad_args",
            "bad_file",
            "missing",
            "needs_input",
            "refused",
            "stale",
            "unreachable",
            "io_failed",
        ],
        fields: &[
            "changed", "fill", "from", "kind", "name", "note", "path", "scope", "to", "tokens",
        ],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::assets::hub::ext_apply(
                    &hub_here(),
                    &r.args,
                    &crate::stream::remote_ask::REACH,
                    &crate::stream::remote_ask::DialRemote,
                )
                .await
                .map(Some)
            })
        }),
    },
    // 扩展页那张表：这台现扫一次、记下（资产目录的写口从这扇门递进去），各台目录合成「条目 × 机器」。
    //   读可达表认出每台的名字 ⇒ 只在流面上。
    CommandSpec {
        name: "ext-list",
        doc_anchor: Some("#### `ext-list`"),
        codes: &["bad_args", "catalog_unreadable", "io_failed"],
        fields: &["machines", "problems", "rows", "visit"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::ext::answer_list(
                &r.args,
                &crate::assets::asset_catalog::answer_current,
                &crate::assets::ext::reach_of(&crate::stream::remote_ask::REACH),
            )
            .map(Some)
            .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 用户写 / 改 / 清一个条目的备注：资产目录的写口从这扇门递进去（记进本机自己那一格，随目录同步）。
    CommandSpec {
        name: "ext-note-set",
        doc_anchor: Some("#### `ext-note-set`"),
        codes: &["bad_args", "catalog_unreadable", "io_failed"],
        fields: &["kind", "name", "note", "text"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::ext::answer_note(&r.args, &crate::assets::asset_catalog::answer_note)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 从这台卸一个扩展：装记录里有 ⇒ 只撤装时写的；没有 ⇒ 先挪进 `~/.cc-monitor/backups/` 再删。判与写都在这台（写经 [`LocalFiles`]，
    //   装记录的写口从这扇门递进去）。
    CommandSpec {
        name: "ext-uninstall-preview",
        doc_anchor: Some("#### `ext-uninstall-preview`"),
        codes: &[
            "bad_args",
            "bad_file",
            "bad_path",
            "io_failed",
            "ledger_unreadable",
            "not_found",
            "refused",
        ],
        fields: &[
            "at", "backup", "files", "kind", "name", "path", "recorded", "said", "token",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::ext::answer_uninstall_preview(
                &LocalFiles,
                &crate::assets::ext::Env::here(),
                &r.args,
            )
            .map(Some)
            .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "ext-uninstall-apply",
        doc_anchor: Some("#### `ext-uninstall-apply`"),
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
        fields: &["at", "changed", "kind", "name", "note", "path", "token"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::ext::answer_uninstall_apply(
                &LocalFiles,
                &crate::assets::ext::Env::here(),
                &crate::assets::skill_ledger::answer_record,
                &r.args,
            )
            .map(Some)
            .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔D 组〕MCP 推 / 拉：每一问只在一台上（`assets/mcp_sync_flow.rs`）；判定原样是 `mcp_sync::answer_with`，写经 [`LocalFiles`]。
    CommandSpec {
        name: "mcp-sync-source",
        doc_anchor: Some("#### `mcp-sync-source`"),
        codes: &[
            "bad_args",
            "bad_file",
            "bad_path",
            "io_failed",
            "missing",
            "refused",
        ],
        fields: &[
            "at", "def", "field", "key", "name", "path", "slots", "token",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::mcp_sync_flow::answer_source(
                &LocalFiles,
                crate::assets::ext::Env::here().user_mcp.as_deref(),
                &r.args,
            )
            .map(Some)
            .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "mcp-sync-preview",
        doc_anchor: Some("#### `mcp-sync-preview`"),
        codes: &["bad_args", "bad_file", "bad_path", "refused"],
        fields: &[
            "at", "def", "field", "key", "kept", "kind", "name", "path", "slots", "state",
            "suspects", "target", "there", "value",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::mcp_sync_flow::answer_preview(
                &LocalFiles,
                &crate::assets::mcp_sync::Live::from_env(),
                &r.args,
            )
            .map(Some)
            .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "mcp-sync-apply",
        doc_anchor: Some("#### `mcp-sync-apply`"),
        codes: &[
            "bad_args",
            "bad_file",
            "bad_path",
            "needs_input",
            "refused",
            "stale",
        ],
        fields: &[
            "at",
            "def",
            "fill",
            "name",
            "path",
            "recordFailed",
            "target",
            "written",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::mcp_sync_flow::answer_apply(
                &LocalFiles,
                &crate::assets::skill_ledger::answer_record,
                &r.args,
            )
            .map(Some)
            .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔D 组〕skill 装 / 卸的写那一半（`assets/skill_flow.rs`）：判（`skill_install`）· 写（[`LocalFiles`]）·
    //   记（`skill_ledger::answer_record`，第四层写口只从这扇门递进去）同一台。
    CommandSpec {
        name: "skill-install-apply",
        doc_anchor: Some("#### `skill-install-apply`"),
        codes: &[
            "bad_args",
            "bad_file",
            "io_failed",
            "needs_consent",
            "stale",
        ],
        fields: &[
            "chmodFailed",
            "dir",
            "name",
            "overwrite",
            "project",
            "recordFailed",
            "source",
            "take",
            "target",
            "written",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::skill_flow::answer_install(
                &LocalFiles,
                &crate::assets::mcp_sync::Live::from_env(),
                None,
                &crate::assets::skill_ledger::answer_record,
                &r.args,
            )
            .map(Some)
            .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // cc-bus 钩子诊断：这台自己的 `settings.json` ＋ stat ⇒ 诊断 ＋ 两种待贴片段（`observe/cc_bus_hooks.rs`）。阻塞档（同步文件 I/O）。
    CommandSpec {
        name: "hooks-diag",
        doc_anchor: Some("#### `hooks-diag`"),
        codes: &["failed", "too_large"],
        fields: &[
            "command",
            "diagnosis",
            "kind",
            "note",
            "path",
            "session_start",
            "snippet",
            "source",
            "stop",
            "supported",
        ],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::faces::feature_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
];
