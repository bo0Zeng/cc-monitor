//! 命令表 · 资产：`assets-*` · `ext-*` · `skill-*` · `mcp-*` · `hooks-*` · `footprint-report` · `data-report` · `cc-bus-*`。

use crate::stream::inbound::doors::hub_here;
use crate::stream::inbound::spec::{arg, both, out, CommandSpec, Run};
use crate::stream::inbound::LocalFiles;

pub(super) const SPECS: &[CommandSpec] = &[
    // 「足迹」由这台后端出整份成品（申报表 ＋ 判定都在 `footprint/`）；
    //   本机那一栏 `client` 带 monitor 自己进程独有的几条事实（家目录 · agent 家 · PATH），`HostScope::Client` 那一族按它们解、这台 stat。只读，阻塞档。
    //   〔墓碑 —— RM1a 那一版这里是 `footprint-probe`：只交路径事实，判定住 monitor。〕
    CommandSpec {
        name: "footprint-report",
        summary: "「足迹」由这台后端出整份成品",
        codes: &["bad_args", "failed"],
        fields: &[arg("client", "可选")],
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
        fields: &[arg("client", "同 `footprint-report`（本机那一栏才带）")],
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
        summary: "MCP 资产同步的判定",
        codes: &["bad_args", "bad_file", "needs_consent"],
        fields: &[arg("overwrite", "可缺席"), out("rows", "每个条目名一行 `{name, state, suspects}`"), arg("source", "拷出来的那一份原文（字符串，必给）"), arg("take", "可缺席"), arg("target", "要写进去的那一份原文；那份文件不存在 ⇒ `null`（**必给**：缺席不当成不存在）"), out("write", "没给 `take` ⇒ `null`；给了 ⇒ 真要写的条目名（排序）：`same` 不写、`new` 写、`differs` 在 `overwrite` 里才写")],
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
        summary: "资产目录",
        codes: &["catalog_unreadable", "io_failed"],
        fields: &[out("changed", "这一趟目录有没有变（变了才写盘）"), out("machines", "各台一份快照 `{id, label, gen, seenAt, assets}`：`gen` 是那台**自己**的代数（它自己那份变了才 +1）；`seenAt` 是那一代的时刻（那台的钟，只给人看、不参与合并）"), out("path", "目录文件在这台上的路径"), out("problems", "这一趟扫描读不出来的那几份（一句话一份）—— 「这台没有」与「这台那份读不出来」分开说"), out("rows", "别的机器有的每个（`kind`, `name`）一行 `{kind, name, state, from}`：`state` 闭集 `missing`（这台一条同名的都没有）· `differs`（有同名的，摘要都不同）· `same`（有一条摘要相同）；`from` 是别处那几条 `{machine, project, digest, summary}`"), out("self", "这台机器的 id（第一次记目录时生成，之后不变）")],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::assets::asset_catalog::answer_catalog(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "assets-catalog-merge",
        summary: "把另一台后端的整份目录并进来",
        codes: &["bad_args", "catalog_unreadable", "io_failed"],
        fields: &[arg("catalog", "另一台后端的整份目录，形状就是 `assets-catalog` 的应答（只读它的 `machines`）"), out("changed", "这一次有没有改动"), both("machines", "各台的快照（同一台取 `gen` 大的那一份整份）"), out("path", "目录文件的路径"), out("problems", "读不出的那几处各一句"), out("rows", "并完之后的资产行"), both("self", "这台的机器 id")],
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
        summary: "本机常驻后端沿池里那条 SSH 同步资产目录",
        // `unreachable`：只给 `origin`（界面直问）而可达表里还没有那一台。
        codes: &["bad_args", "io_failed", "unreachable"],
        fields: &[arg("dial", "可缺席"), arg("origin", "可缺席"), out("reach", "可达表 `[{origin, machine}]`：`machine` 是那台目录的 id（还没拉成过 ⇒ `null`）—— 界面据它把目录里的机器 id 对回 origin"), out("self", "本机目录的 id（开头那一次现扫拿到的）—— 界面据它把目录里本机那一格对回 `<local>`"), out("synced", "每一趟一行 `{origin, peer, changed, pushed, error}`：`peer` 是那台目录的 `self`")],
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
        summary: "读来源那台上的一个 skill",
        codes: &["bad_args", "io_failed", "not_found", "too_large"],
        fields: &[out("dir", "这个 skill 的目录（绝对路径）"), out("files", "每个普通文件一条 `{path, text, bytes, exec, why}`：`path` 是 skill 里的相对路径（`/` 分段）"), arg("name", "skill 的目录名（一段：不许分隔符 / `..` / 点开头）"), arg("project", "可缺席：给了 ⇒ 那个项目里的 skill（项目目录是这台上的绝对路径）；缺 ⇒ 用户级"), out("root", "这台 skill 的根"), out("skipped", "没读的那几处（指向目录的链接 / 特殊文件）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::skill_install::answer_read(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "skill-install-plan",
        summary: "在要被写的那一台判 skill 装不装得过来",
        codes: &[
            "bad_args",
            "bad_file",
            "io_failed",
            "needs_consent",
            "too_large",
        ],
        fields: &[out("base", "写的时候 `files-put` 用的 `root` 与相对前缀：`rel` = `<prefix>/<path>`"), out("dir", "要写进去的目录"), arg("name", "skill 的目录名"), arg("overwrite", "可缺席，语义同 `mcp-sync-plan`（给了 `take` 才答 `write`；`differs` 的要在 `overwrite` 里点名）"), out("prefix", "写的时候 `files-put` 用的 `root` 与相对前缀：`rel` = `<prefix>/<path>`"), arg("project", "同 `skill-read`（装到哪一级）"), out("root", "这台 skill 的根"), out("rows", "每个路径一行 `{path, state, suspects, blocked}`：`state` 闭集同 `mcp-sync-plan`"), arg("source", "`skill-read` 读到的 `[{path, text, exec}]`（`text` 可为 `null` = 装不过去的那一个）"), arg("take", "可缺席，语义同 `mcp-sync-plan`（给了 `take` 才答 `write`；`differs` 的要在 `overwrite` 里点名）"), out("target", "这一趟拷的那几个路径在这台上现有的原文 `[{path, text}]` —— 写的时候当 CAS 期望"), out("write", "没给 `take` ⇒ `null`；给了 ⇒ 真要写的路径（排序）"), out("ledger", "没给 `take` ⇒ `null`")],
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
        summary: "skill 装记录的写口",
        codes: &[
            "bad_args",
            "io_failed",
            "ledger_unreadable",
            "not_found",
            "too_large",
        ],
        fields: &[arg("at", "`add` 可缺席：缺 ⇒ 目录按 skill 根算；`\"home\"` ⇒ 目录 = 记录所在那个家目录（装在家目录底下、不在 skill 根下的东西用）；其余值 ⇒ `bad_args`"), out("changed", "记录变没变（没变不写）"), arg("digest", "装进去的那一条的摘要（`mcp-add`；卸时对得上就不用问）"), both("dir", "`drop` 的入参：记录里那个 skill 目录；应答里是这一条记录的目录"), arg("file", "`mcp-add` / `mcp-drop`：配置文件的绝对路径（键）"), arg("files", "`add`：`{<相对路径>: {digest, created}}` —— `skill-install-plan` 答的 `ledger` 里真写成了的那几个"), arg("name", "`add`：skill 的目录名"), arg("op", "`add`（装完记）或 `drop`（卸掉 / 已经不在的摘掉）；MCP 那一条：`mcp-add` · `mcp-drop`"), arg("paths", "`drop`：要摘的相对路径（不在记录里 ⇒ `bad_args`，一个字节不动）；摘到零个 ⇒ 整条记录摘掉"), arg("project", "`add` 可缺席：给了 ⇒ 目录按那个项目里的 skill 根算"), out("remaining", "这一条还剩几个文件（MCP：那份配置里还记着几条）")],
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
        summary: "这台机器的 MCP 列表成品",
        codes: &["bad_args", "too_large"],
        fields: &[out("dirs", "agent 用户级配置里那张项目表的键（排序）：用过的项目目录"), out("entries", "`{scope, name, server, sourcePath}`：`scope` 闭集 `user` · `local` · `project`；`server` 原样（未知字段不丢）"), out("name", "server 名"), out("problems", "在而读不出 / 不是 JSON 的那几份各一句（「这台没有」与「那份坏了」不合成一句）；不在的静默"), arg("projectDir", "可缺席 / `null`"), out("scope", "`user` · `local` · `project`"), out("server", "那一条配置原样（未知字段不丢）"), out("sourcePath", "读自哪一份文件")],
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
        summary: "增 / 改这台一个项目 `.mcp.json` 里的一条",
        codes: &["bad_args", "bad_path", "refused"],
        fields: &[out("changed", "真写了吗（算出来与盘上逐字节相同 ⇒ `false`，一个字节不动）"), arg("name", "server 名（空 ⇒ 拒）"), out("path", "写到了哪（解过链接的那一份）"), arg("projectDir", "这台机器上的绝对路径（不含 `..`）；落点恒是 `<它>/.mcp.json`（写面只此一个，agent 自己的配置一个字节不碰）"), arg("server", "那一条的配置，原样写进 `mcpServers[name]`")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::mcp_edit::answer_put(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "mcp-server-remove",
        summary: "删这台一个项目 `.mcp.json` 里的一条",
        codes: &["bad_args", "bad_path", "refused"],
        fields: &[out("changed", "文件不在 / 那一条不在 ⇒ `false`（一个字节不写、不建文件）"), arg("name", "要删的那一条"), out("path", "那份文件"), arg("projectDir", "同 `mcp-server-put`")],
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
        summary: "装 cc-bus 到这台之前看一眼",
        codes: &["refused"],
        fields: &[out("dest", "落点（`<skills 根>/cc-bus`）"), out("existing", "落点上已经有东西（要写时先整个改名留作备份）"), out("version", "内嵌那一份的摘要（只答「相同 / 不同」；枢纽拿它当卡上的记号）"), out("writes", "与这台二进制带着的那一份逐文件比，内容会变的那几个（缺的 ＋ 不一样的；清单外的文件不算）；空 ⇒ 已是这一版")],
        takes_input: false,
        run: Run::Blocking(|_r| {
            crate::assets::cc_bus_install::answer_state()
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "cc-bus-install",
        summary: "把这台二进制带着的 cc-bus 装到这台",
        codes: &["bad_file", "refused"],
        fields: &[out("backup", "覆盖前整个目录改名成的那一份（`cc-bus.bak-<秒>`，`null` = 之前没装过）"), out("dest", "落点（`<skills 根>/cc-bus`，过独立 realpath 围栏）"), out("recordFailed", "装好了但没记进 skill 装记录时那一句（这一趟装的卸不掉）；装卸账复用 `skill-install-record` 那一份"), out("written", "写了的那几个相对路径（全一致 ⇒ 空：一个字节不写、不备份、不记）")],
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
        summary: "装到一台之前那张确认卡，本机后端当枢纽",
        codes: &[
            "bad_args",
            "bad_file",
            "missing",
            "refused",
            "unreachable",
            "io_failed",
        ],
        fields: &[out("config", "MCP：装上之后那一条（待填的值是 `null`）"), arg("from", "来源那台"), arg("kind", "`skill` / `mcp`"), arg("name", "名字"), out("path", "被写那台上的落点"), arg("scope", "`{from, to}`，各是 `{level:\"user\"}` 或 `{level:\"project\", dir}`（那台上的绝对路径）；`to` = 用户在确认卡上选的那一处"), out("slots", "每个空位 `{field, key, kept}`"), out("stop", "装不了的原因（非文本文件 · 这台那一份盖不了）；有它就不该确认"), out("suspects", "要留意的几件（说人话）"), arg("to", "被写那台：可达表的键，**`null` = 这台自己**"), out("tokens", "两头看过的那一份的记号 `{source, target}` —— 应用时原样交回"), out("unchanged", "装上之后和现在一样"), out("writes", "要写的那几个（skill：目录里的相对路径；MCP：那份配置文件）")],
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
        summary: "装到一台，本机后端当枢纽",
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
        fields: &[out("changed", "写了的那几个"), arg("fill", "MCP：用户填的值（同 `mcp-sync-apply`）；来源机上的值从不经过这里"), arg("from", "同 `ext-hub-preview`"), arg("kind", "同 `ext-hub-preview`"), arg("name", "同 `ext-hub-preview`"), out("note", "做成了但要知道的一件（执行位没改成 · 没记下来）"), out("path", "写到了哪"), arg("scope", "同 `ext-hub-preview`"), arg("to", "同 `ext-hub-preview`"), arg("tokens", "确认卡上那一份：枢纽两头都再看一次，任一头对不上 ⇒ `stale`、**一个字节不写**")],
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
        summary: "设置「扩展」页那张表",
        codes: &["bad_args", "catalog_unreadable", "io_failed"],
        fields: &[out("machines", "每台一列 `{key, here, reachable, name, projects}`：`key` = 枢纽认它的键（本机后端自己 = `null`"), out("problems", "这台扫的时候读不出来的那几份"), out("rows", "每个条目一行 `{kind, name, about, detail, new, builtin, note, cells}`，`cells` 与 `machines` 同序"), arg("visit", "可缺席：`true` = 这一问算「来看了一次」（扩展页每次变可见时的第一问）—— 「新见到」按上一次来看算")],
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
        summary: "写 / 改 / 清一个扩展的备注",
        codes: &["bad_args", "catalog_unreadable", "io_failed"],
        fields: &[arg("kind", "`skill` / `mcp`"), arg("name", "名字"), out("note", "现在生效的那一份（清掉了 ⇒ `null`）"), arg("text", "备注正文（首尾空白去掉；空串 = 清掉；最长 2000 字，超了拒、不截断）")],
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
        summary: "装到别的机器时来源那一条",
        codes: &[
            "bad_args",
            "bad_file",
            "bad_path",
            "io_failed",
            "missing",
            "refused",
        ],
        fields: &[arg("at", "在这台上哪一级：`{level:\"project\", dir}`（`<dir>/.mcp.json`）或 `{level:\"user\"}`（agent 自己那份用户级配置，只读）"), out("def", "那一条；**`env` / `headers` 的值在这台就换成 `null`**（值不出来源机，只交键名）"), out("field", "空位在哪一格：`env` · `headers`"), out("key", "空位的键名"), arg("name", "server 名"), out("path", "读的是哪一份"), out("slots", "换成空位的那几格 `[{field, key}]`"), out("token", "按原样那一条（含密钥值）算的记号：它变了（连只改了一个密钥值也算）⇒ 应用时判 `stale`")],
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
        summary: "在要被写的那一台看装上之后那一条",
        codes: &["bad_args", "bad_file", "bad_path", "refused"],
        fields: &[arg("at", "同 `mcp-sync-source`；`at` 只收项目那一级（用户级只读 ⇒ `refused`）"), arg("def", "来源那台交回的那一条，原样；`env` / `headers` 里夹着值 ⇒ `bad_args`"), out("field", "空位 / 可疑项在哪一格"), out("key", "空位的键名"), out("kept", "这台那一条原来就有这个键的值（不填就沿用）"), out("kind", "可疑项的种类（闭集同 `mcp-sync-plan`）"), arg("name", "同 `mcp-sync-source`；`at` 只收项目那一级（用户级只读 ⇒ `refused`）"), out("path", "这台那份的路径"), out("slots", "每个空位 `{field, key, kept}`：`kept` = 这台那一条原来就有这个键的值（不填就沿用）"), out("state", "`new`（这台没有这一条）· `same`（除空位外一样）· `differs`"), out("suspects", "可疑项，每条 `{kind, field, value, there}`，闭集同 `mcp-sync-plan`"), out("target", "这台那份的记号（不存在 ⇒ `null`）—— 写的时候原样交回"), out("there", "它在这台指向的东西在不在"), out("value", "可疑的那个值")],
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
        summary: "在要被写的那一台把那一条写进去",
        codes: &[
            "bad_args",
            "bad_file",
            "bad_path",
            "needs_input",
            "refused",
            "stale",
        ],
        fields: &[arg("at", "同 `mcp-sync-preview`"), arg("def", "同 `mcp-sync-preview`"), arg("fill", "用户在确认卡上填的值 `{env: {键: 值}, headers: {…}}`；没填的键沿用这台原有的值，两样都没有 ⇒ `needs_input`、一个字节不写"), arg("name", "同 `mcp-sync-preview`"), out("path", "写到了哪"), out("recordFailed", "装记录没记下来时那一句（`null` = 记下了）"), arg("target", "看卡时这台那份的记号：这台在那之后变了 ⇒ `stale`，**一个字节不写、不重读重算**"), out("written", "真写了吗（与原有那一条逐字相同 ⇒ `false`）")],
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
        summary: "在要被写的那一台把勾的那几个 skill 文件写进去",
        codes: &[
            "bad_args",
            "bad_file",
            "io_failed",
            "needs_consent",
            "stale",
        ],
        fields: &[out("chmodFailed", "写成了但执行位没置上的那几个"), out("dir", "装到了哪"), arg("name", "同 `skill-read`"), arg("overwrite", "同 `skill-install-plan`（`differs` 的必须在 `overwrite` 里点名，否则整趟拒 `needs_consent`）"), arg("project", "同 `skill-read`"), out("recordFailed", "装记录没记下来时那一句（`null` = 记下了）—— 记不下来 ⇒ 这一趟装的卸不掉"), arg("source", "来源那台 `skill-read` 的 `files`，原样（`path` · `text` · `exec`；`text` 为 `null` 的装不过去）"), arg("take", "同 `skill-install-plan`（`differs` 的必须在 `overwrite` 里点名，否则整趟拒 `needs_consent`）"), arg("target", "看差异时这台 `skill-install-plan` 回的 `target`（这台那几份原文），原样 —— 写时的 CAS 期望"), out("written", "真写成了的那几个（按写的顺序）")],
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
        summary: "cc-bus 钩子诊断",
        codes: &["failed", "too_large"],
        fields: &[out("command", "钩子原文（去首尾空白）"), out("diagnosis", "`session_start`（→ `cc-register`）· `stop`（→ `cc-bus-stop-hook`）各一态 ＋ `note`（读不到 / 坏 JSON / 顶层不是对象时说原因，否则空串）"), out("kind", "一态：`not-installed` · `installed-via-path` · `installed-at-path` · `path-missing` · `unknown`"), out("note", "读不到 / 坏 JSON / 顶层不是对象时说原因，否则空串"), out("path", "钩子点名的路径（原样，环境变量不展开）"), out("session_start", "`SessionStart` 钩子（→ `cc-register`）那一态"), out("snippet", "要合并进那份文件的内容：两条钩子直接指向这台 cc-bus 的两个脚本（家目录底下写成相对家目录的形，否则绝对路径），不依赖 `PATH`"), out("source", "读的是哪份文件：这台后端的 agent 配置根下的设置文件"), out("stop", "`Stop` 钩子（→ `cc-bus-stop-hook`）那一态"), out("supported", "这台跑得了 cc-bus（它要 tmux；这份后端编到的平台没有原生 tmux ⇒ `false`，`snippet` 恒 `null`，界面只说这台不支持自动收信）")],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::faces::feature_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
];
