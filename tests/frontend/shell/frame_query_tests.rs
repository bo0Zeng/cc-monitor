//! 只读查询走长连接的判据（monitor 侧）。

use super::*;

/// 那八条 —— 那一串逐字（`--accounts` 在盘上叫 `--list-accounts`）。
/// **异源**：这张表手写，不从 [`MOVED`] 派生。
const DESIGN_EIGHT: &[&str] = &[
    "--read-session",
    "--read-session-tail",
    "--session-accounts",
    "--list-accounts",
    "--search",
    // `--list-subagents` 摘了：按目录列子 agent 候选那一条随「按运行读」（`history-run`）删了，没有被替掉的拨号子命令。
    // 要求「`--list-user-inputs` 与骨架 `--read-session-from-offset --index` 上帧面」。
    "--list-user-inputs",
    "--read-session-from-offset",
    // 协调方加的：`--find-in-session` 一起搬。
    "--find-in-session",
    // 「仍在拨号的 `--account-trust` / `--account-trust-zero`」随账号域上帧面（异源：手抄的要求）。
    "--account-trust",
    "--account-trust-zero",
];

// 这里原先还有一张「仍拨号」的表（C4c 起零条）—— 逐次拨号那条路删了，表随之摘掉。

fn sorted(v: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut v: Vec<String> = v.into_iter().collect();
    v.sort();
    v
}

/// 后端命令表各族生产段里的每一块 `CommandSpec`：`(帧命令名, 那一块的原文)`（**从后端源码数**，逐族切）。
/// 抽成一处：下面两条判据（交给只读宿主的那几条 · 全部登记的帧命令）共用同一个切法。
fn backend_command_blocks() -> Vec<(String, String)> {
    let families = crate::guard_support::backend_registry_sources();
    let blocks: Vec<(String, String)> = families
        .iter()
        .flat_map(|(_, prod)| prod.split("CommandSpec {").skip(1))
        .filter_map(|blk| {
            let at = blk.find("name: \"")? + "name: \"".len();
            Some((blk[at..].split('"').next()?.to_string(), blk.to_string()))
        })
        .collect();
    blocks
}

/// 后端 `stream/inbound/` 生产段里把活交给 `read_face::answer` 的帧命令名（**从后端源码数**）。
fn backend_read_face_commands() -> Vec<String> {
    let got: Vec<String> = backend_command_blocks()
        .into_iter()
        .filter(|(_, blk)| blk.contains("read_face::answer"))
        .map(|(name, _)| name)
        .collect();
    assert!(!got.is_empty(), "从后端源码一条都没数到 —— 抽取坏了");
    sorted(got)
}

/// ★ 两向相等：[`MOVED`] 的左列 == 那八条 ＋ 两条；右列 == 后端真登记上帧面、交给只读宿主的那几条。
#[test]
fn the_moved_table_matches_the_design_list_and_the_backend_registry() {
    assert_eq!(
        sorted(MOVED.iter().map(|(f, _)| f.to_string())),
        sorted(DESIGN_EIGHT.iter().map(|s| s.to_string())),
        "搬上帧面的子命令与要求点名的那几条不相等"
    );
    // 右边还要并上「生在帧面上」的那几条（没有被替掉的拨号子命令，见 `BORN_ON_FRAME`）。
    // 右列按**集合**比：信任预检两形合进一条帧命令（`accounts-trust` 在右列出现两次）。
    let mut right = sorted(
        MOVED
            .iter()
            .map(|(_, c)| c.to_string())
            .chain(BORN_ON_FRAME.iter().map(|c| c.to_string())),
    );
    right.dedup();
    assert_eq!(
        right,
        backend_read_face_commands(),
        "monitor 这边以为搬上去的帧命令，与后端真登记的对不上"
    );
}

/// ★**逐次拨号那条路不存在了**（零命中 ＋ 正控）。
///
/// 守的要求：「`run_list_query`〔散文墓碑〕（逐次拨号那条路，
/// 今天零放行）删，同拍动 `subagent.rs` 的回落」。原先这里两条判据钉「那条路只放行登记表 ＋ 先问后拨」——
/// 那张表 C4c 起是空的，路本身删了 ⇒ 改钉「它在 monitor 生产段里一个标识符都不剩」：
/// 函数名 · 闸门名 · 放行表名，按**整词**、剥注释之后数（散文里的墓碑不算）。
/// 正控：同一识别器在同一份语料上认得出今天真在的出口 `run_routed`（识别器没瞎）。
#[test]
fn the_dial_per_query_path_is_gone() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut corpus = String::new();
    let mut files = 0usize;
    for (_, src) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        files += 1;
        let prod = guard_core::production_code(&src);
        corpus.push_str(&guard_core::strip_trailing_comments(
            &guard_core::strip_comment_lines(&prod),
        ));
        corpus.push('\n');
    }
    assert!(files > 100, "只扫到 {files} 份 monitor 源码 —— 遍历坏了");
    let dead = [
        ["run", "list", "query"].join("_"),
        ["dial", "allowed"].join("_"),
        ["STILL", "DIALED"].join("_"),
    ];
    for name in &dead {
        assert!(
            !guard_core::contains_word(&corpus, name),
            "`{name}` 又出现在 monitor 生产段里 —— 逐次拨号那条路（或它的闸门 / 放行表）回来了"
        );
    }
    assert!(
        guard_core::contains_word(&corpus, "read_page"),
        "正控失败：识别器在同一份语料上认不出帧面出口 `read_page` —— 上面的零命中不可信"
    );
}

// argv 分流（`route_argv` / `ArgvRoute`〔散文墓碑〕）随它的调用方（子 agent · 按偏移取一段）一起删了，那条判据随之退役。

// ════════════════════════════════════════════════════════════════════════════
// 八条里哪几条已经**只走通道**（主界面经 `src/comms/inward/chan.ts`）
// ════════════════════════════════════════════════════════════════════════════

/// 已迁到通道的帧命令：前端经 `chan.call(origin, "<op>", …)` 直接问那台后端，monitor 那一跳只搬字节。
/// 迁的判准只有一条：迁过去之后，**那条应答的解释只有一个家**。
const CHANNELED: &[(&str, &str)] = &[
    (
        "accounts-sessions",
        "本机与远端都迁：两条 Tauri 命令（远端帧面 · 本机一次性 exec）各在 Rust 里把同一种行解析一遍；\
         迁过去之后逐行解释只剩 `src/frontend/ui/account-reads.ts::parseSessionAccountLines` 一处",
    ),
    (
        "history-search",
        "远端那半迁：逐台 fan-out ＋ 补 origin ＋ 与本机索引合并三件事搬到 `src/frontend/ui/views/history-search.ts`，\
         每件只有那一个家。本机那半也迁了：本机也经通道问本机后端，monitor 内存索引删了",
    ),
    // 会话读面那三条：**解释挪进后端、直接出成品**（`read_face.rs`），monitor 那一份
    //   「核头尾、剥行、失败分档」删了；界面经 `src/frontend/ui/session-reads.ts` 问，本机与远端同一条路。
    (
        "history-index",
        "后端出成品 `{from, end, rows}`；行本身前端不解释（`SkeletonFacts`）；monitor 那份核头尾删了",
    ),
    (
        "history-user-inputs",
        "后端出成品 `{from, end, entries}`；「什么算一条用户输入」只住后端，失败分档只住 `session-reads.ts::failureOf`",
    ),
    (
        "history-find",
        "后端出成品 `{total, hits}`；命中口径只住后端（`search_query` ＋ `search_rules` · 适配层记录文本），monitor 那份核头尾删了",
    ),
    // 账号域那两条（账号域读自己那台的 apikey 表、规则搬进 `acct-core`、agent 随请求带）。
    (
        "accounts-list",
        "后端出成品 `{meta, accounts, notice}`，并上**那台机器自己**那份 apikey 表（`acct_core::apikey_routed_subset`）；\
         monitor 那份行解析 / 降级说明 / 本机并表删了，界面经 `src/frontend/ui/account-reads.ts::fetchAccounts` 问、按形状收，本机与远端同一条路",
    ),
    (
        "accounts-trust",
        "后端出成品 `{trusted, known}`（CLI 那一臂同一个函数）；替掉最后两条逐次拨号的 `--account-trust*`，\
         界面经 `src/frontend/ui/account-reads.ts::checkTrust` 问",
    ),
];

/// **帧面只读查询那一族之外**、同样改成「前端经通道直接问、后端出成品」的帧命令 ——
/// `(帧命令, 为什么迁、迁了之后解释住哪)`。它们不在 [`MOVED`] 里（不是 `C1` 那一族），但前端 `chan.call` 的
/// 操作名集合要把它们算进来：下面那条两向判据的「前端那一侧」== [`CHANNELED`] ⊔ 本表。
/// 每一条还要**真的**是后端登记的帧命令（从后端 `stream/inbound/` 生产段数，异源）、monitor 生产段里**零**字面量。
const CHANNELED_ELSEWHERE: &[(&str, &str)] = &[
    // 历史页的平铺清单（`src/frontend/ui/history-list-reads.ts`）：本机后端出成品（判活 · 每行能做什么 · 分组 · 搜标题），远端经它问那台。
    ("history-list", "新帧命令：一台的全部会话一次给（`history_list.rs`），界面问 `<local>`、远端带 `origin`；取代按项目逐个展开的那两问"),
    // 额度与轮换（`src/frontend/ui/quota-reads.ts`）：额度账 · 轮换都是那台后端账号域自己的状态，monitor 从没有过这几条命令。
    ("plan-list", "新帧命令：这台的 pb 工作区与片（找 pb · 跑 pb dump 在后端）；计划页只排版"),
    ("plan-read", "新帧命令：一个工作区的成品（接手 / 签收人对到会话 · 要你看四种 · 认可与退回的状态都在后端）；计划页只排版"),
    ("plan-cell-view", "新帧命令：一格的 agent 视角（pb 原样那一段）；计划页详情折着的那一段只排版"),
    ("plan-ack", "新帧命令：认可一条需手动（只记在 cc-monitor，键带条目版本，后端判）；计划页那一条只排版"),
    ("plan-unack", "新帧命令：撤掉认可（toast［撤销］那一下）；计划页只排版"),
    ("plan-return", "新帧命令：把人的话送给负责那一格的会话（拼行 · 判能不能送 · 送字 · 记已退回都在后端）；退回框只排版"),
    // 扩展页 MCP 小标与抽屉那一行（`src/frontend/ui/ext-reads.ts`）：按台问那台后端的 `mcp-read` 照抄。
    ("mcp-read", "帧命令：一台的 MCP 列表 ＋ 每条写好的字（需登录那一句 · 最近连接失败那一条 · 小标画哪一种），判定只在后端 `mcp-read` 一处；扩展页按台问、只排版"),
    ("plan-command", "新帧命令：以人的身份代敲 pb 的 continue · pause · view（退出码与 pb 那一句在后端认）；计划页开关与［整张图］只排版"),
    ("quota-read", "新帧命令：那台的额度账 ＋ 显示态（判在后端）；界面状态栏按钮 · 悬停卡 · 账号面板只排版"),
    ("rotation-rules-read", "新帧命令：那台的轮换规则表（默认指向哪条 · 每条谁在用 · 摘要与说明由后端写），面板来源下拉与设置里规则管理只排版"),
    ("rotation-plan", "新帧命令：一份轮换草稿逐格校验（封顶时段重叠 · 起止相同 · 时刻写错 · 越界），浮层只照它标红"),
    ("rotation-rule-save", "新帧命令：新建 / 整份改一条规则（逐格校验 · 版本冲突由后端判），存为规则浮层与规则编辑器只排版"),
    ("rotation-rule-rename", "新帧命令：规则改名（重名 · 空 · 超长 · 版本冲突由后端判），设置「轮换」分栏行内改名只排版"),
    ("rotation-rule-delete", "新帧命令：删规则（默认那条拒；在用的会话转为本会话或改跟随默认，由后端挪），删除框只排版"),
    ("rotation-default-set", "新帧命令：设为这台的默认规则（回跟随默认的活会话数），设置「轮换」分栏只排版"),
    ("rotation-session-read", "新帧命令：一批会话的轮换与「账号」格（能不能热切换 · 下一个 · 卡住都由后端给）"),
    ("rotation-session-set", "新帧命令：改会话的轮换（勾号 · 拖序 · 触发 · 无号可换两态），新勾的按量号由后端挪末尾"),
    ("rotation-switch", "新帧命令：现在就换（热切换钉号 · 重启切换逐个交 `session-restart`）"),
    // tab 栏多选的批量停 / 起（`src/frontend/ui/tab-batch-run.ts`）：一台一次、那台逐个判逐个答，monitor 从没有过这条命令。
    (
        "sessions-start",
        "新帧命令：每一个同单个那一条（记录在不在 · 在跑不另起 · 空 tmux 就地键入 · 铸名建会话 / 渲那一行），判在那台后端",
    ),
    (
        "sessions-where",
        "新帧命令：这几个会话各在哪个 tmux 会话里（单个菜单亮哪几项 · 重启切换 · 分叉都问它），判在那台后端",
    ),
    (
        "sessions-stop",
        "新帧命令：每一个同单个「杀死会话」（恰一个带着它的 tmux 会话才杀，门在那台后端），判在那台后端",
    ),
    // 设置「扩展」页（`src/frontend/ui/ext-reads.ts`）：表 · 装（本机后端当枢纽）· 卸（问被卸那台），判定全在后端。
    (
        "ext-note-set",
        "界面直问本机后端：用户写的备注记进它的资产目录（自己那一格），随目录同步到别的后端",
    ),
    (
        "ext-list",
        "界面直问本机后端：各台目录合成「条目 × 机器」，每格的态与唯一那个按钮都是后端判的，线上不带摘要",
    ),
    (
        "ext-hub-preview",
        "只问本机一次：本机后端向来源那台取、交被写那台判，拼成确认卡（带两头的记号）",
    ),
    (
        "ext-hub-apply",
        "只问本机一次：两头再看一次、记号对不上 ⇒ `stale` 零写；对得上才交被写那台写",
    ),
    (
        "ext-uninstall-preview",
        "问被卸的那一台：装记录里有 ⇒ 按记录撤；没有 ⇒ 卡上说先备份再删",
    ),
    (
        "ext-uninstall-apply",
        "问被卸的那一台：记号对不上 ⇒ `stale` 零写；判 · 删 · 摘记录都在那台",
    ),
    // 会话正文那几条里生在帧面上的（不是 `C1` 那一族的换壳）：界面经 `src/frontend/ui/record-reads.ts` 直问。
    (
        "history-page",
        "按字节分页读、出记录行（骨架按偏移取一段：主窗口 tab 与查看器）：编号 · 进不进界面 · `cwd` 都是后端给的，要哪几格由界面交的声明（`view`）定",
    ),
    (
        "history-lines",
        "按行号取回一段：后端出记录行（原先回原文、monitor 解析）",
    ),
    (
        "drift-report",
        "那台后端的漂移账（看不懂的记录类型）：看不懂的那一刻在场的是那台后端，成品 `{faces}`；monitor 只留它天生观测的两面",
    ),
    (
        "history-run",
        "一个子运行的记录，按运行读（父记录 ＋ 子运行 ‖ 派出它的工具调用）：子运行住哪、哪条属于谁都问那台后端的适配层，成品 `{run, path, rows, end, more}`",
    ),
    (
        "ssh-config-import",
        "批量导入预览：逐个 `ssh -G` ＋ 聚合都在后端（`dial/ssh_config.rs::aggregate_ssh_hosts`），成品 `{groups}`；前端按形状收",
    ),
    // 测试连接：monitor 那条 Tauri 命令与它手里那份探针退役，本机后端组请求、拨一次、回结局。
    (
        "remote-probe",
        "测试连接：界面交表单那一台（＋ 已保存的同名那一份 · 跳板），后端 `dial/probe.rs` 出结局；前端 `src/frontend/ui/remote-probe.ts` 按恰好的键集合收",
    ),
    // 终端名单：那台后端出成品（与宿主无关的句柄）；只有 tmux 名在手的那一方先按名字认出那一行。
    (
        "terminals-list",
        "那台的终端名单：后端 `control/terminals.rs` 出成品；前端 `src/frontend/ui/terminal-reads.ts` 收它要的那几格（只加不改的形状，多出来的格照收）",
    ),
    // 终端页送字送键：能不能送（身份门 · 画面指纹）全在那台后端判，前端只照回话说。
    (
        "terminal-input",
        "往那个终端送一段字 / 一颗键：后端 `control/terminals.rs::input_for_inbound` 过身份门、比画面指纹、回 `delivered` · `unsure` · `refused ＋ why`；\
         前端 `src/frontend/ui/terminal-reads.ts::sendToTerminal` 问、底部抽屉终端页 `terminal-page.ts` 按回话说一句",
    ),
    // 终端实时预览三条：订阅 · 回执 · 退订（画面帧走那条流连接的应答通道，壳转进 `terminal-screen/<票>`）。
    (
        "terminal-follow",
        "订那个终端的实时画面：后端 `control/terminal_follow.rs` 起 tmux 控制模式客户端、有变化推一整屏（一帧在途）；\
         前端 `src/frontend/ui/terminal-follow.ts` 先订流再发它，收一屏交终端页画",
    ),
    (
        "terminal-follow-ack",
        "第几帧画完了（两次至少隔 100 ms）：后端收到才推下一帧；前端 `terminal-follow.ts` 发",
    ),
    // `terminal-unfollow` 不在这里：界面不发它（撤流即可），由 monitor 替界面发 —— 见 [`SENT_BY_MONITOR_ONLY`]。
    // 各台搜索结果合成一份：合并排序进本机后端（`search_rules::sort_by_recency`），界面逐台扇出。
    (
        "history-search-merge",
        "各台 `history-search` 的会话行合一份：本机后端 `observe/search_query.rs::answer_merge`（`search_rules::sort_by_recency`）；\
         前端 `src/frontend/ui/views/history-search.ts` 问，monitor 这一侧零发送点",
    ),
    // 开终端那一行：ssh 外壳由本机后端渲（monitor 只开窗）。
    (
        "terminal-ssh",
        "开终端那一行（`ssh -t …` 外壳 ＋ PowerShell 载荷）：本机后端 `dial/terminal.rs` 渲（组请求走 `dial/machine.rs::resolve`）；\
         前端 `src/frontend/ui/terminal-open.ts` 与文件窗口 `filewin/shell.rs::open_terminal_here` 问，monitor 那一侧只开窗",
    ),
    // 远端 ↗ 的前两问：那台谁在显示这个会话 · 本机哪串进程开着那条连接。
    (
        "session-terminals",
        "此刻连着这个会话的终端：会话所在那台后端 `observe/session_terminals.rs` 答；前端 `src/frontend/ui/remote-terminal-front.ts` 问",
    ),
    (
        "terminal-processes",
        "那台报来的终端连接是这台电脑上哪串进程：本机后端 `dial/terminal_processes.rs` 答；前端 `src/frontend/ui/remote-terminal-front.ts` 问，\
         monitor 那一侧只沿进程链找窗口、拉前",
    ),
    // 起会话要的 tmux 名：派生 ＋ 避让只留后端（前端那份铸名口删了）。
    (
        "terminal-name-mint",
        "铸 tmux 名：那台后端派生基名、按它那张会话快照避让（`control/ccm/mod.rs::answer_terminal_name_mint`）；前端 `src/frontend/ui/terminal-name-mint.ts` 问，monitor 这一侧零发送点",
    ),
    // 端口转发三条：monitor 那三条 Tauri 命令与它手里的转发账退役，账住本机常驻后端。
    (
        "forward-list",
        "列转发：后端 `dial/forwards.rs::list_with` 出成品 `{forwards}`；前端 `src/frontend/ui/port-forward-reads.ts` 按恰好的键集合收",
    ),
    (
        "forward-start",
        "起一条转发：后端查自己的可达表、开 `use: forward` 链路、等 ack 才进账（`dial/forwards.rs::start_with`）；monitor 零转发账",
    ),
    (
        "forward-stop",
        "停一条转发：后端从账上摘掉 ⇒ 链路被收、本地口放掉（`dial/forwards.rs::stop_with`）",
    ),
    // 起会话的计划与渲染：原 monitor Tauri 命令（`render_ccm_launch` · `render_launch_payload` ·
    //   `relay_endpoint_for_launch` · `new_local_session` / `resume_history_session` / `render_local_attach`〔散文墓碑〕）。
    (
        "launch-render-cli",
        "起会话那一行 `ccm …`：后端出成品 `{cmd}`，渲不出来回码 `refused`（`control/launch_render/wire.rs`）；前端 `src/frontend/ui/launch-render.ts::renderCli`。\
         环境与中转地址由起 agent 那台的 `ccm` 在最终 exec 那一处定（载荷那一条与「这一发的中转地址」那一条随之删了）",
    ),
    (
        "launch-local",
        "本机起会话整条：本机后端出成品 `{cmd, account}`（`control/launch_render/local.rs`）；前端 `src/frontend/ui/launch-render.ts::planLocalLaunch`，\
         monitor 只剩开终端窗口（`open_local_terminal`）",
    ),
    // 删会话 · 分叉：两件改世界的事本来就在那台后端，monitor 只剩转交 ⇒ 转交删了，界面直接说。
    (
        "files-delete-session",
        "后端出成品 `{path}`（只收 sid，落点由那台后端按 sid 找）；前端 `src/frontend/ui/session-writes.ts::deleteSession` 问、按恰好的键集合收，\
         monitor 这一侧零发送点（门那一问 `Door::delete_session` 删了）",
    ),
    // 起新会话：全产品一个框一个请求（分叉随这一个请求写，界面不再单独问 `session-fork`）。
    (
        "session-new",
        "后端出成品 `{outcome, session, sid, cmd, account, agent, cwd}`（`control/session_new.rs`：判目录 · 号 · 终端名 · 分叉只在这一步写）；\
         前端 `src/frontend/ui/new-session-reads.ts::askNew` 问、`decodeNew` 按恰好的键集合收，monitor 这一侧零发送点",
    ),
    (
        "session-new-facts",
        "后端出成品 `{recent, tmux, agents, fork}`（`faces/session_new_face.rs`，只读）；前端 `new-session-reads.ts::askFacts` 问、`decodeFacts` 按恰好的键集合收，monitor 这一侧零发送点",
    ),
    (
        "session-new-dir",
        "后端出成品 `{exists, tmuxName}`（只读）；前端 `new-session-reads.ts::askDir` 问、`decodeDir` 按恰好的键集合收，monitor 这一侧零发送点",
    ),
    // 钩子诊断：本机远端两条 Tauri 命令合成一条帧命令，界面直接问那台。
    (
        "hooks-diag",
        "后端出成品 `{diagnosis, snippet, source}`（`observe/cc_bus_hooks.rs`，读那台自己的 `settings.json` ＋ stat，只读）；\
         前端 `src/frontend/ui/cc-bus-hooks-reads.ts::fetchHooksReport` 问（扩展页 cc-bus 那一行每台一问）、`decodeHooksReport` 按恰好的键集合收，monitor 这一侧零发送点",
    ),
    // 〔⑬「monitor 零 SSH」〕公钥推送：本机后端读 `.pub` · 组请求 · 经那台后端写或一次 exec，界面直接问本机。
    (
        "pubkey-push",
        "后端出成品 `{outcome, pubPath, via}`（`assets/pubkey.rs`）；前端 `src/frontend/ui/pubkey-push.ts::pushPublicKey` 问、`decodePush` 按恰好的键集合收\
         （金样 `pubkey-push.golden.json`），monitor 这一侧零发送点（那条 Tauri 命令与它的两条路删了）",
    ),
    // 文件与数据：同一份足迹重排的成品（`src/backend/footprint/data.rs`）。
    (
        "data-report",
        "后端出成品 `{home, changedFiles, todo, tmux, chores, own}`；前端 `src/frontend/ui/settings/data-reads.ts::readDataReport` 问、\
         `decodeDataReport` 按恰好的键集合收；monitor 这一侧零发送点，本机那一栏只答它自己那台那几行的事实（`footprint_client_facts`）",
    ),
    // 首次运行「开始用」那份数：本机后端判三步、剩几步；机器表台数由界面带上。
    (
        "first-run",
        "本机后端读启动文件与账号库清单判三步（`src/backend/footprint/readiness.rs`）；前端 `src/frontend/ui/settings/readiness-reads.ts::readReadiness` 问\
         （设置窗机器页「开始用」· 主窗口状态栏那一枚），monitor 这一侧零发送点",
    ),
    // 离线那台的上次值：本机后端读写它自己的 `~/.cc-monitor/last-seen.json`。
    (
        "last-seen-read",
        "本机后端读自己那份小文件（`src/backend/footprint/last_seen.rs`）；前端 `src/frontend/ui/last-seen.ts::recallSeen` 问（那台问不到时画上次的），\
         monitor 这一侧零发送点",
    ),
    (
        "last-seen-write",
        "本机后端读—改—写自己那份小文件；前端 `src/frontend/ui/last-seen.ts::rememberSeen` 交（账号清单 · 「文件与数据」读成了那一份），\
         monitor 这一侧零发送点",
    ),
    // 「待办」记下的一个选择：后端写它自己的 `~/.cc-monitor/chores.json`。
    (
        "chores-mark",
        "后端读—改—写自己那份小文件（`src/backend/footprint/chores/marks.rs`）；前端 `src/frontend/ui/settings/data-reads.ts::markChore` 问\
         （「待办」里［不用了］［还是要做］· 别名页［我自己贴］［改由 cc-monitor 接上］），monitor 这一侧零发送点",
    ),
    // 换 Claude 目录前那一问：后端 stat 两次回码。
    (
        "agent-home-check",
        "后端回码 `{state}`（`src/backend/footprint/agent_home_check.rs`）；前端 `src/frontend/ui/settings/claude-dir-check.ts::claudeDirProblem` 问、按码取那一句；\
         monitor 这一侧零发送点（界面不再自己问两次 `files-stat` 去判）",
    ),
    // `footprint-report` 那一行退役：界面那份读口（`footprint-reads.ts`）没人用、随死代码删了；帧命令本身还在后端。
    // 生在帧面上、界面直接问的一条（不是只读宿主那一族，故不进 `BORN_ON_FRAME`）。
    (
        "resync",
        "新帧命令（手动对齐）：后端出成品 `{added, removed, retagged, watchers}`（`resync_face.rs` → `observe/watcher.rs::resync`）；\
         前端 `src/frontend/ui/resync.ts::resync` 问、`decodeResynced` 按恰好的键集合收，monitor 这一侧零发送点",
    ),
    (
        "machine-interrupts",
        "`BORN_ON_FRAME` 那一条：后端出成品 `{relayedSessions, relayedMaybe, liveStreams, forwards}`（`accounts_query.rs::machine_product`）；\
         前端 `src/frontend/ui/settings/interrupts.ts::askInterrupts` 问那台与本机、按恰好的键集合收，monitor 这一侧零发送点",
    ),
    // 生在帧面上、界面直接问的那一条。
    (
        "backend-log",
        "`BORN_ON_FRAME` 那一条：后端出成品 `{path, size, text, truncated}`（`read_face.rs::log_tail`，读本进程被交的那份诊断文件）；\
         前端 `src/frontend/ui/settings/backend-section.ts::readBackendLog` 按形状收，monitor 这一侧零发送点",
    ),
    // `C4b.md §6.6` A 组第一批：后端已是成品、monitor 只在转的那一条。
    // 生在帧面上、界面直接问的第二条。
    (
        "history-facts",
        "`BORN_ON_FRAME` 那一条：后端出成品 `{end, forkedFrom, touchedFiles, agents, usage}`（`observe/facts_query.rs`），\
         续传令牌就是上一份成品原样；前端 `src/frontend/ui/session-reads.ts::readSessionFacts` 问、`decodeFacts` 按恰好的键集合收，\
         monitor 这一侧零发送点（它替掉的是前端 `onLine` 上四个旁路记账员，不是一条 monitor 命令）",
    ),
    (
        "history-turns",
        "`BORN_ON_FRAME` 那一条：后端出成品 `{from, end, turns}`（`observe/turns.rs`，一轮的摘要，与大纲同源）；\
         前端 `src/frontend/ui/session-reads.ts::readTurns` 问、`decodeTurns` 按恰好的键集合收，monitor 这一侧零发送点",
    ),
    (
        "history-record",
        "`BORN_ON_FRAME`（U4b）那一条：后端应答就是成品 `{present, root}`，monitor 那条命令（`probe_session_record`）\
         只在转、核两格 —— 核验搬到唯一的消费者 `src/frontend/ui/session-reads.ts::decodeRecord`（缺一格仍是契约坏了，不读成「不在」），\
         命令与发送端（`frame_query::record`）删了",
    ),
    // `C4b.md §6.6` A 组第二批：`app.backend-policy`（S5 进主线后前置满足）。
    (
        "exit-policy-read",
        "问那台机器「退出行为」那个值：monitor 那条命令（`backend_exit_policy`）只在拦空白名 ＋ 转 ＋ 原样交回，\
         解释本来就在界面（`settings/backend-section.ts::readExitAnswer`）⇒ 设置页经通道直接问，命令删了。\
         ⚠ monitor 自己**另有**一处问它（退出臂，见 [`ASKED_BY_MONITOR_ITSELF`]）—— 那不是替界面转",
    ),
    (
        "exit-policy-set",
        "交那台机器写那个值：同上一行（`set_backend_exit_policy` 删了），画的仍是后端写完读回来的那一份",
    ),
    (
        "accounts-add",
        "账号库那一族：生来就走通道（从没过 monitor）—— 那台后端做完回成品，界面 `account-ops.ts` 按生成的线上类型收",
    ),
    (
        "accounts-init",
        "账号库那一族：生来就走通道（从没过 monitor）—— 那台后端做完回成品，界面 `account-ops.ts` 按生成的线上类型收",
    ),
    (
        "accounts-login-cmd",
        "账号库那一族：生来就走通道（从没过 monitor）—— 那台后端做完回成品，界面 `account-ops.ts` 按生成的线上类型收",
    ),
    (
        "accounts-mcp-pick",
        "账号库那一族：各账号共用的 MCP，生来就走通道 —— 那台后端同步完回成品，界面 `account-ops.ts` 按生成的线上类型收",
    ),
    (
        "accounts-mcp-sync",
        "账号库那一族：各账号共用的 MCP，生来就走通道 —— 那台后端同步完回成品，界面 `account-ops.ts` 按生成的线上类型收",
    ),
    (
        "accounts-mcp-read",
        "账号库那一族：各账号共用的 MCP，生来就走通道 —— 那台后端同步完回成品，界面 `account-ops.ts` 按生成的线上类型收",
    ),
    (
        "accounts-mcp-remove",
        "账号库那一族：各账号共用的 MCP，生来就走通道 —— 那台后端同步完回成品，界面 `account-ops.ts` 按生成的线上类型收",
    ),
    (
        "accounts-remove",
        "账号库那一族：生来就走通道（从没过 monitor）—— 那台后端做完回成品，界面 `account-ops.ts` 按生成的线上类型收",
    ),
    (
        "accounts-repair",
        "账号库那一族：生来就走通道（从没过 monitor）—— 那台后端做完回成品，界面 `account-ops.ts` 按生成的线上类型收",
    ),
    (
        "accounts-rollback",
        "账号库那一族：生来就走通道（从没过 monitor）—— 那台后端做完回成品，界面 `account-ops.ts` 按生成的线上类型收",
    ),
    (
        "accounts-set-default",
        "设这台的默认号（起会话跟随时没有上次的号就落它）：设置账号页 · 状态栏账号选单 · 命令面板三处都经 `account-ops.ts::accountsSetDefault`；monitor 那份默认号删了",
    ),
    (
        "accounts-verify",
        "账号库那一族：生来就走通道（从没过 monitor）—— 那台后端做完回成品，界面 `account-ops.ts` 按生成的线上类型收",
    ),
    // 注解三条：读写者换成本机常驻后端（`history_annotations.rs`，文件原地不动）；monitor 那两条命令
    //   （`update_history_metadata` / `list_last_accounts`）与删会话时那一句清注解删了，界面经 `src/frontend/ui/history-reads.ts` 问 `<local>`。
    (
        "history-annotate",
        "改一条注解：后端严格读 → 只改那一条 → 原子写，回那一条（`history-reads.ts::decodeEntry` 按形状收）",
    ),
    (
        "history-branch",
        "主线外清单冷读：查看器与子运行窗口打开时问一次（`record-reads.ts::decodeBranch` 按形状收），之后跟实时帧 `branch`",
    ),
    (
        "history-forget",
        "删会话之后连带删那一条注解（界面删成功之后交；从前是 monitor 删完顺手清）",
    ),
    (
        "history-last-accounts",
        "sid → 上次用哪个号起（账号徽章回落 · 带账号 resume 前现读）；从前是 monitor 读那份文件",
    ),
    // `creds.apikey` 写：界面经 `src/frontend/ui/apikey-reads.ts::writeApikeyKey` 直接交那台后端（账号 id 由后端推）。
    (
        "apikey-key-set",
        "给一个号配 key：monitor 那条命令（`write_apikey_credentials_key`〔散文墓碑〕）推账号 id、本机那一臂先问 `apikey-read` 核路径再转这一条 ——         推 id 搬进后端写口（`acct_core` 那一份规则），核路径由常驻后端的身份（hello 的 `host_env`）答，命令与写臂删了",
    ),
    (
        "apikey-routing",
        "这几个号在那台的表里有没有行 · 那台的中转在不在：monitor 那条命令（`apikey_routing_for`）本机自己读凭据文件 ＋ 连回环口、         远端转 `apikey-read` 的 `rows` ＋ `relay-status`，再调 `acct-core` 那条规则 —— 人群与判准整个搬进后端（`accounts/upstream_select/endpoint.rs`），命令删了",
    ),
    // skill 卸的「看」那一半：生来就走通道（没有过 monitor 那一条）。
    // `C4c.md §5.6` A 组 `tmux.manage` 第一格：抓一屏（今天走终端管理那一条）。
    (
        "terminal-preview",
        "后端应答就是成品（逐行文本 ＋ 尺寸光标），按名单里的句柄 / 会话 ID 指；拒绝码说成人话那一份住唯一的消费者那一侧\
         `src/frontend/ui/terminal-reads.ts`（预览窗 `views/pane-preview.ts` · 起会话没报出来时抓原话 `launch-arrival.ts` 调它）",
    ),
    // `tmux.manage` 其余两格（杀会话 · 送键）＋ `launch.send-into`（就地 resume）：
    //   三条 Tauri 命令（`kill_remote_tmux` / `tmux_send_keys` / `backend_send_into`〔散文墓碑〕）同拍迁完 ——
    //   `launch` 这条帧命令当年有两个 monitor 发送端（送键 · 就地 resume），只迁一个的话本表就得为过渡态开一格。
    (
        "kill",
        "后端应答就是成品 `{session, killed}`（身份门 ＋ 窗口门在后端先过、对句柄下手）；monitor 那条命令只在拒空目标、\
         转、核 `killed`、按三态说人话 —— 那一份解释搬到 `src/frontend/ui/tmux-control.ts::killSession`，命令与发送端\
         （`backend_kill.rs`）删了",
    ),
    // `C4c.md §5.6` A 组 `cc-bus.cockpit`：五条 Tauri 命令（`check_cc_bus_agent_online` / `cc_bus_send` /
    //   `cc_bus_kill` / `cc_bus_spawn` / `cc_bus_broadcast`〔散文墓碑〕）同拍迁完。广播那一条先在后端长出 `bus-broadcast`
    //   （挑人 ＋ 逐个投递搬进后端），界面才只剩「说成人话」。解释的家：`src/frontend/ui/cc-bus-control.ts`（单一住址由
    //   `cc_bus_tests.rs::the_front_end_speaks_the_bus_ops_only_through_one_module` 判）。
    (
        "bus-list",
        "查一个 agent 在不在线：后端应答就是成品（名单 ＋ 每人 `live`），monitor 那条命令只在转 ＋ 按 id 挑一个 ——\
         挑人与「问不到 ≠ 离线」搬到 `src/frontend/ui/cc-bus-control.ts::agentOnline`",
    ),
    (
        "bus-send",
        "发一条消息：后端应答就是成品 `{to, sent, registered, live, from}`，monitor 那条命令只在核 id / 正文、转、按三态说人话 —— 搬到 `sendMessage`",
    ),
    (
        "bus-kill",
        "收掉一个 agent：后端身份门在前、应答就是成品，monitor 那条命令只在核 id、转、说人话 —— 搬到 `killAgent`",
    ),
    (
        "bus-spawn",
        "派生一个 agent：形状（账号 ⊕ `base`）先核、后端应答就是成品，monitor 那条命令只在核形状、转、说人话 —— 搬到 `spawnAgent`",
    ),
    (
        "bus-broadcast",
        "新帧命令：广播从前是 monitor 自己列名单、挑在线的、逐个 `cc-send`；挑人与逐个投递搬进后端\
         （`control/cc_bus.rs::broadcast_for_inbound`，应答是成品计数 ＋ 逐个失败），界面 `broadcast` 只说成人话",
    ),
    // `session.tasks`。
    (
        "tasks-list",
        "后端出成品 `{tasks}`（字段语义挪进后端 `tasks_query.rs::task_entry`），界面 `tasks-panel.ts::decodeTasks` 按形状收；\
         monitor 那条命令（`get_session_tasks`）与行解释（`parse_task_lines`）删了。\
推送那一路也不经 monitor 问了：后端 `changed {tasks}` 帧 ⇒ 通道 `changed/tasks`，界面收到自己重问",
    ),
    (
        "session-interrupts",
        "新帧命令（只读）：动一个会话之前会打断什么，后端按族出成品（`observe/interrupts_query.rs`），\
         界面 `interrupt-reads.ts::decodeInterrupts` 按形状收；monitor 这一侧从来没有过这条命令",
    ),
    // 驾驶舱读面两条：后端转调 cc-bus 新加的机器可读读命令、出成品，界面 `cc-bus-control.ts` 按形状收；
    //   monitor 那两条 Tauri 命令与整套 shell 读（本机 `bash -lc` ＋ 远端拨号链路）删了。
    (
        "bus-state",
        "后端出成品 `{agents, spawned, skipped}`（`cc-list --tsv` ＋ `cc-agents --tsv`：登记时间 · 派生时间 · 坏行数），\
         界面 `cc-bus-control.ts::decodeState` 按形状收（金样 `cc-bus-read.golden.json`）",
    ),
    (
        "bus-inbox",
        "新帧命令：后端转调只读的 `cc-log`（不推已读位置）、出成品 `{messages, skipped, truncated}`，\
         界面 `cc-bus-control.ts::decodeInbox` 按形状收",
    ),
    // D 组 MCP：读写与推拉的计算、读、写都进了那台后端（`assets/mcp_edit.rs` · `assets/mcp_sync_flow.rs`），
    //   monitor 那八条 Tauri 命令（`mcp.rs` · `mcp_sync.rs`〔散文墓碑〕）删了；界面经 `src/frontend/ui/mcp-reads.ts` / `src/frontend/ui/mcp-sync-reads.ts` 按形状收。
    // `mcp-sync-source` / `-preview` / `-apply` 三条界面不再直问（那是经前端中继）：
    //   界面只问本机那两条枢纽命令，枢纽向来源那台取、向被写那台写（内层三条只经枢纽）。
    // cc-bus 装到本机：monitor 那两条 Tauri 命令（`deploy_local_cc_bus` / `cc_bus_install_state`〔散文墓碑〕）删了。
    //   那两条帧命令界面也不再直问：cc-bus 是扩展页里的一行，装它走枢纽（`ext-hub-*`），枢纽交被写那台装（内层两条只经枢纽）。
    // D 组 skill 装 / 卸与资产目录同步：monitor 那四条 Tauri 命令（`skill_install_*` · `skill_uninstall_apply` · `assets_sync`〔散文墓碑〕）删了。
    (
        "assets-sync",
        "界面直问本机常驻后端（远端那一页只报 `origin`，够到那台用握手时 `remote-reach` 登记的那一行）；成品 `{self, synced, reach}`，\
         `assets-sync-reads.ts::decodeAssetsSynced` 按形状收",
    ),
    (
        "aliases-block-install",
        "新帧命令：别名那一族（规则 · 方言 · 围栏住那台后端 `assets/aliases/`），monitor 那六条 Tauri 命令删了",
    ),
    (
        "aliases-block-remove",
        "新帧命令：别名那一族（规则 · 方言 · 围栏住那台后端 `assets/aliases/`），monitor 那六条 Tauri 命令删了",
    ),
    (
        "aliases-block-render",
        "新帧命令：别名那一族（规则 · 方言 · 围栏住那台后端 `assets/aliases/`），monitor 那六条 Tauri 命令删了",
    ),
    (
        "profiles-read",
        "新帧命令：设置窗「别名与配置文件」那一页（配置文件按「基于」排成树，合并只在那台后端 `assets/aliases/page.rs`）；前端 `src/frontend/ui/profiles-reads.ts` 问",
    ),
    (
        "profiles-resolve",
        "新帧命令：设置窗「别名与配置文件」那一页（配置文件按「基于」排成树，合并只在那台后端 `assets/aliases/page.rs`）；前端 `src/frontend/ui/profiles-reads.ts` 问",
    ),
    (
        "profiles-impact",
        "新帧命令：设置窗「别名与配置文件」那一页（配置文件按「基于」排成树，合并只在那台后端 `assets/aliases/page.rs`）；前端 `src/frontend/ui/profiles-reads.ts` 问",
    ),
    (
        "profiles-bases",
        "新帧命令：设置窗「别名与配置文件」那一页（配置文件按「基于」排成树，合并只在那台后端 `assets/aliases/page.rs`）；前端 `src/frontend/ui/profiles-reads.ts` 问",
    ),
    (
        "profiles-write",
        "新帧命令：设置窗「别名与配置文件」那一页（配置文件按「基于」排成树，合并只在那台后端 `assets/aliases/page.rs`）；前端 `src/frontend/ui/profiles-reads.ts` 问",
    ),
    (
        "aliases-read",
        "新帧命令：别名那一族（规则 · 方言 · 围栏住那台后端 `assets/aliases/`），monitor 那六条 Tauri 命令删了",
    ),
    (
        "powershell-policy-set",
        "新帧命令：那一代 PowerShell 的执行策略设成当前用户 `RemoteSigned`（`assets/aliases/mod.rs::answer_policy_set`）；\
         前端 `src/frontend/ui/alias-reads.ts::allowLocalScripts` 问（用户点了、确认了之后），monitor 这一侧零发送点",
    ),
    // `skill-read` / `skill-install-plan` / `skill-install-apply` 界面不再直问：只经本机那两条枢纽命令。
    // `files-ls` 那一行退役：数据位置页 `$PROFILE` 备份那一格随它的读口一起删了（文件窗口自己列目录不经界面）。
    // `files-stat` 那一行退役：Claude 目录像不像改由后端 `agent-home-check` 一条命令判（见上），界面不再自己问。
];

/// monitor 生产段里**拼写与某条已迁帧命令相同、却不是发送点**的字面量 —— `(拼写, 处数, 为什么)`。
///
/// 下面那条判据按字面量 `"<op>"` 数 monitor 里还剩几个发送点；`kill` 这个词在 monitor 里另有一处正当的用法
/// （起系统的 `kill` 进程），它与帧命令 `kill` 同拼写、不同义。两向相等：多一处 = 又长出一个发送点（或又一处同拼写，
/// 要来这里表态）；少一处 = 那一处用法没了，这一行馊了。
// 原先唯一一行（`kill`：`local_backend_host.rs` 起系统的 `kill` 进程发 SIGTERM）随那条路一起删了 ——
//   本机「停」改走一次性 `--resident-stop`，monitor 生产段里 `kill` 这个拼写一处都不剩 ⇒ 表空着（两向相等照旧成立）。
const SAME_SPELLING_NOT_A_SEND: &[(&str, usize, &str)] = &[
    (
        "resync",
        1,
        "`inbound_client::RESYNC_OP`：认出 `resync` 的应答、把它交回的那台当下能力事实换进 `Offer` —— 不发任何东西",
    ),
];

/// **monitor 自己**（不是替界面转）也要问的帧命令 —— `(帧命令, 生产段里几处, 为什么)`。
///
/// 它们同时在 [`CHANNELED_ELSEWHERE`] 里（界面那一问已走通道）；下面那条判据里 monitor 生产段的字面量
/// 从此 == `BORN_ON_FRAME` 那一次 ＋ 本表登记的处数（两向相等：多一处 = 又长出一个替界面转的发送点；
/// 少一处 = monitor 那件自己的事不问了，这一行馊了）。
const ASKED_BY_MONITOR_ITSELF: &[(&str, usize, &str)] = &[
    (
        "exit-policy-read",
        1,
        "退出臂在决定那一刻现问一次（`backend_policy::kill_on_exit_now`）：\
         monitor 自己要不要跟着收那台后端 —— 它的答案不给界面",
    ),
    // `apikey-read` 那一行退役：写 key 之前核路径那一问（`apikey_remote::send_key`〔散文墓碑〕）随写臂删了。
    (
        "assets-sync",
        1,
        "流握手那一刻（`asset_sync.rs::on_remote_ready`）交「怎么够到那台」并顺手同步一趟 —— 宿主交事实，\
         应答只记日志、不给界面（界面那一问经通道直问本机后端）",
    ),
    // `skill-uninstall-plan` 那一行退役：卸那一趟的删与摘记录进了被卸那台后端（`skill-uninstall-apply`），monitor 零处问它。
    // `tasks-list` 那一行退役：本机任务 notify 删了（监视进后端，`changed {tasks}` 帧 ⇒ 通道 `changed/tasks`），
    //   monitor 零处再问它。
    // `files-ls` · `files-stat` 不在这里：界面不再问它们（见上表），文件窗口列目录 · 看属性
    //   （`filewin/source.rs::CMD_LS` · `filewin/props.rs::CMD_STAT` …）是另一个前端自己的事。
    // 文件窗口「在此打开终端」。窗口只交意图（经通道 `terminal-open`），
    //   monitor 接下来补机器事实、问本机后端渲那一行、开窗（`chan/host.rs::TERMINAL_SSH`）—— 与主界面开终端同一条路。
    (
        "terminal-ssh",
        1,
        "文件窗口「在此打开终端」（`chan/host.rs::TERMINAL_SSH`）：monitor 接下窗口交来的意图，问本机后端渲 ssh 那一行",
    ),
];

/// **只由 monitor 发、界面不发**的帧命令 —— `(帧命令, 生产段里几处, 为什么)`。两向相等：monitor 生产段的字面量 == 登记的处数，
/// 界面 `chan.call` 的操作名里没有它（界面又长出一处发送点 = 两条路并存）。
const SENT_BY_MONITOR_ONLY: &[(&str, usize, &str)] = &[(
    "terminal-unfollow",
    1,
    "〔退订挂在订阅上〕界面那条 `terminal-screen/<票>` 撤掉（撤单 · 被重订 · 页面重载 · 窗口没了）时，\
     壳替界面向那台退订（`terminal_screen_relay.rs::unfollow`）；重载时界面没人能发，所以只住壳这一处",
)];

/// 后端 `stream/inbound/` 生产段里登记的全部帧命令名（异源：从后端源码数，不读本文件的表）。
fn backend_registered_commands() -> std::collections::BTreeSet<String> {
    backend_command_blocks()
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

/// 还留在 monitor 侧发送的那几条 —— `(帧命令, 为什么今天不迁)`。**不是豁免清单**：
/// 下面那条判据要求它们**真的**还有 monitor 侧发送点（没了 ⇒ 这一行的理由已经馊了）。
///
/// 判准是「业务解释只有一个家」，正路是「解释挪进后端、直接出成品」；下面每行写清卡在哪。
const HELD_BACK: &[(&str, &str)] = &[
    // `accounts-list` 那一行挪进了 [`CHANNELED`]（账号域搬家做了：后端出成品、并它自己那份表）。
    // 下面三行：`history-read` / `history-subagents` **等后端二次拆包**，
    //   `history-tail` 归 CF2（`subscribe`）。三行都仍有 monitor 侧发送点（判据照旧要求它们真有）。
    // `history-subagents` 那一行摘了（命令换成出成品的 `history-subagent`，进了 [`CHANNELED`]）。
    (
        "history-read",
        "**monitor 旁路快照的流机器在用**（续点 · 见证 · 分段编号，`stream_source::fetch_snapshot`）：应答已是后端出的逐行成品\
         （`{rows: [{end, hash, message?, cwd?}]}`，记录解释住后端 `agents/claudecode/`），monitor 只编号、攒批、转交 —— \
         它是会话流那一路的输入，不是前端查询（同 `history-tail`）",
    ),
    (
        "history-tail",
        "**归 CF2（`subscribe`）**：它不是前端查询，只被实时 tab 的快照续点用（`stream_source` 的流机器）——\
         会话流收口成 `subscribe`时由 CF2 处置，不属于 `call`",
    ),
    // 〔C4a 与 SR1a 合并〕SR1a 同波搬上来的 `history-index` / `history-user-inputs` / `history-find` 三行
    // 挪进了 [`CHANNELED`]（后端出成品）。
];

/// monitor 生产段（`src/frontend/shell/src/**/*.rs`，剥注释与 `#[cfg(test)]`）里，一条帧命令的字面量出现几次。
fn monitor_literal_count(op: &str) -> usize {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let needle = format!("\"{op}\"");
    guard_core::scan_tree!(&dir, &["rs"])
        .into_iter()
        .map(|(_, src)| {
            guard_core::production_code(&src)
                .matches(needle.as_str())
                .count()
        })
        .sum()
}

/// 前端生产 TS（`src/**/*.ts`，剥注释；测试文件整棵住 `tests/`，不在这棵树里）里 `chan.call(` 调用点的操作名 ——
/// **必须是字面量**。
/// 回 `(操作名集合, 不是字面量的那几处)`。
fn frontend_chan_ops() -> (std::collections::BTreeSet<String>, Vec<String>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut ops = std::collections::BTreeSet::new();
    let mut not_literal = Vec::new();
    for (p, src) in guard_core::scan_tree!(&root.join("src"), &["ts"]) {
        let rel = p.to_string_lossy().replace('\\', "/");
        chan_ops_in(
            &guard_core::strip_comment_lines(&src),
            &rel,
            &mut ops,
            &mut not_literal,
        );
    }
    (ops, not_literal)
}

/// 一段（已剥注释的）TS 里 `chan.call(` 的操作名。前面紧挨标识符字符的不算（`mychan.call(`）。
fn chan_ops_in(
    code: &str,
    rel: &str,
    ops: &mut std::collections::BTreeSet<String>,
    not_literal: &mut Vec<String>,
) {
    for (at, _) in code.match_indices("chan.call(") {
        if code[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
            continue;
        }
        let args = &code[at + "chan.call(".len()..];
        // 第二个实参：跳过第一个逗号之前的 origin。
        let Some(comma) = args.find(',') else {
            not_literal.push(format!("{rel}: 实参表不完整"));
            continue;
        };
        let second = args[comma + 1..].trim_start();
        match second.strip_prefix('"').and_then(|r| r.split_once('"')) {
            Some((op, _)) => {
                ops.insert(op.to_string());
            }
            None => not_literal.push(format!(
                "{rel}: {}",
                second.chars().take(40).collect::<String>()
            )),
        }
    }
}

/// ★★ 帧面只读查询的分区：[`MOVED`] 右列 == [`CHANNELED`] ⊔ [`HELD_BACK`]（不相交、每行都写了理由）。
/// （C4a 立它时是 C1 那八条；SR1a 同波又搬上来三条 ⇒ 今天十一条。）
#[test]
fn the_eight_are_partitioned_into_channeled_and_held_back() {
    let mut moved = sorted(MOVED.iter().map(|(_, c)| c.to_string()));
    moved.dedup(); // 信任预检两形合进一条帧命令
    let mut both: Vec<String> = CHANNELED
        .iter()
        .chain(HELD_BACK)
        .map(|(c, _)| c.to_string())
        .collect();
    let n = both.len();
    both.sort();
    both.dedup();
    assert_eq!(
        both.len(),
        n,
        "同一条帧命令同时在「已迁」与「未迁」两张表里"
    );
    assert_eq!(both, moved, "已迁 ⊔ 未迁 != C1 的八条（`MOVED` 右列）");
    for (c, why) in CHANNELED.iter().chain(HELD_BACK) {
        assert!(!why.trim().is_empty(), "`{c}` 没写理由");
    }
}

/// ★★ **迁过去的只走通道（两向）**：
/// - 前端 `chan.call(` 的操作名集合 == [`CHANNELED`]（多一条 = 没登记就迁了；少一条 = 登记了却没迁）；
/// - monitor 生产段里 [`CHANNELED`] 每条的字面量**只剩 `MOVED` 那一行**（= 再没有 monitor 侧发送点）；
/// - [`HELD_BACK`] 每条**还有** monitor 侧发送点（字面量多于 `MOVED` 那一行）—— 没了就是理由馊了。
///
/// 两侧异源：一侧读 TS 语料，一侧读 Rust 生产段。
#[test]
fn the_channeled_ops_are_sent_only_through_the_channel() {
    let (ops, not_literal) = frontend_chan_ops();
    assert!(
        not_literal.is_empty(),
        "这几处 `chan.call(` 的操作名不是字面量 —— 本判据认不出它们说的是哪条：{not_literal:?}"
    );
    assert_eq!(
        ops.iter().cloned().collect::<Vec<_>>(),
        sorted(
            CHANNELED
                .iter()
                .chain(CHANNELED_ELSEWHERE)
                .map(|(c, _)| c.to_string())
        ),
        "前端经通道说的操作名 != 登记的「已迁」（`CHANNELED` ⊔ `CHANNELED_ELSEWHERE`）"
    );
    // 那一族之外的已迁：真是后端的帧命令（异源）· 不在 `MOVED` 里 · monitor 生产段零字面量。
    let registered = backend_registered_commands();
    assert!(
        registered.len() > 20,
        "从后端命令表只数到 {} 条帧命令 —— 抽取坏了",
        registered.len()
    );
    for (op, n, why) in SAME_SPELLING_NOT_A_SEND {
        assert!(!why.trim().is_empty(), "`{op}` 没写理由");
        assert!(*n > 0, "`{op}` 登记了 0 处同拼写 —— 那就不该在这张表里");
        assert!(
            CHANNELED_ELSEWHERE.iter().any(|(c, _)| c == op),
            "`{op}` 登记成「同拼写、不是发送点」，却不是已迁到通道的帧命令"
        );
    }
    for (op, _, why) in ASKED_BY_MONITOR_ITSELF {
        assert!(!why.trim().is_empty(), "`{op}` 没写理由");
        assert!(
            CHANNELED_ELSEWHERE.iter().any(|(c, _)| c == op),
            "`{op}` 登记成「monitor 自己也问」，却不在 `CHANNELED_ELSEWHERE` 里"
        );
    }
    for (op, why) in CHANNELED_ELSEWHERE {
        assert!(!why.trim().is_empty(), "`{op}` 没写理由");
        assert!(registered.contains(*op), "`{op}` 不是后端登记的帧命令");
        assert!(
            !MOVED.iter().any(|(_, c)| c == op),
            "`{op}` 是 `C1` 那一族的，该登记在 `CHANNELED`"
        );
        // 生在帧面上的那几条在 `BORN_ON_FRAME` 里各有一次字面量（判据的一侧，不是发送点）；
        //   monitor 自己也问的那几条另有登记的处数（`ASKED_BY_MONITOR_ITSELF`）。
        let on_frame = BORN_ON_FRAME.iter().filter(|c| **c == *op).count();
        let itself: usize = ASKED_BY_MONITOR_ITSELF
            .iter()
            .filter(|(c, _, _)| c == op)
            .map(|(_, n, _)| *n)
            .sum();
        let homonyms: usize = SAME_SPELLING_NOT_A_SEND
            .iter()
            .filter(|(c, _, _)| c == op)
            .map(|(_, n, _)| *n)
            .sum();
        assert_eq!(
            monitor_literal_count(op),
            on_frame + itself + homonyms,
            "`{op}` 已迁到通道，monitor 生产段却还有它的字面量（又长出了一个发送点）"
        );
    }
    for (op, n, why) in SENT_BY_MONITOR_ONLY {
        assert!(!why.trim().is_empty(), "`{op}` 没写理由");
        assert!(registered.contains(*op), "`{op}` 不是后端登记的帧命令");
        assert!(
            !ops.contains(*op),
            "`{op}` 登记成「只由 monitor 发」，界面却也经通道发它（两条路并存）"
        );
        assert!(
            !CHANNELED
                .iter()
                .chain(CHANNELED_ELSEWHERE)
                .any(|(c, _)| c == op),
            "`{op}` 同时登记在「界面经通道发」的表里"
        );
        assert_eq!(
            monitor_literal_count(op),
            *n,
            "`{op}` 在 monitor 生产段里的发送点处数 != 登记的 {n}"
        );
    }
    let mut still_sent_by_monitor: Vec<String> = Vec::new();
    for (op, _) in MOVED.iter().map(|(_, c)| (*c, ())) {
        // `MOVED` 那几行自己就是字面量出现（信任预检两形合进一条帧命令 ⇒ 那一条在表里出现两次）。
        let in_moved = MOVED.iter().filter(|(_, c)| *c == op).count();
        if monitor_literal_count(op) > in_moved {
            still_sent_by_monitor.push(op.to_string());
        }
    }
    still_sent_by_monitor.sort();
    still_sent_by_monitor.dedup();
    assert_eq!(
        still_sent_by_monitor,
        sorted(HELD_BACK.iter().map(|(c, _)| c.to_string())),
        "monitor 侧还在发的帧命令 != 登记的「未迁」。\n\
         多出来的：已迁的那条在 monitor 里又长出了发送点（两条路并存）；\n\
         少了的：未迁那一行的理由已经馊了（它其实没人发了）"
    );
    // 反空真 ①：monitor 那一侧的数法认得出一条真有发送点的（快照续点那一处）。
    assert!(
        monitor_literal_count("history-tail") > 1,
        "正控：`history-tail` 必有 monitor 侧发送点 —— 数法坏了"
    );
    // 反空真 ②：前端那一侧的识别器认得出字面量、认得出非字面量、不把 `mychan.call(` 当成它。
    let mut ops = std::collections::BTreeSet::new();
    let mut bad = Vec::new();
    chan_ops_in(
        "await chan.call(o, \"x-op\", p, Budget.within(1));\n\
         await chan.call(o, OP, p, budget);\n\
         mychan.call(o, \"nope\", p, b);\n",
        "probe.ts",
        &mut ops,
        &mut bad,
    );
    assert_eq!(
        ops.into_iter().collect::<Vec<_>>(),
        vec!["x-op".to_string()]
    );
    assert_eq!(bad.len(), 1, "非字面量那一处没被认出来：{bad:?}");
}

// `history-record` 应答解释那条判据（`parse_record`〔散文墓碑〕）随发送端删了；同一条口径
//   「缺一格是契约坏了，**绝不**读成『不在』」搬到 TS 那一侧 `session-reads.ts::decodeRecord`（`tests/frontend/ui/session-reads.vitest.ts`）。

// ═════════════════════════════════════════════════════════════════════════════
// 本机查询接的是**正在跑的那份**常驻后端（WIN1 撤回的 F2 由本件接住）
// ═════════════════════════════════════════════════════════════════════════════
//
// 要求：「本机那几问从『exec 一次性本机后端』改走 `<local>` 长连接」·
// 「后端是给定的、不留退路」。RT1 F2 读数：旧那条 `run_query` 只认 exe 旁边那一份文件、
// 不认自释放之后正在跑的那一份 ⇒ Windows 上本机那几问一直「后端不在」。
// 三格，异源各在一处：
// ① 发送：本机那几问只经 `inbound_client::client_for("<local>")`（行为判据在 `subagent_tests` / `remote_branch_tests` /〔散文墓碑〕
//    `acct_iso_deploy_tests`〔散文墓碑〕 的部署那一步：假后端那一侧真收到了帧命令）；
// ② 谁能登记在 `<local>` 上：生产段里 `register(LOCAL_ORIGIN, …)` 的文件集合 == 两个载体（常驻回环 · stdio 监护），
//    两处都是拿**已经回了 hello 的那条活连接**造客户端 ⇒ 登记在那里的就是正在跑的那一份；
// ③ 谁还在「找 exe 旁那份文件」：生产段里 `resolve_beside_this_exe(` 的调用点集合 == {起 / 自释放常驻后端那两处}
//    （都不是查询；一条查询路径都不许再用它）。两向相等，带正控。
// ④给终端窗口的 `CCM_BACKEND_BIN` 同病：两处都交 `local_backend_host::running_backend_bin()`。

/// 生产段里含 `needle` 的「文件::外层函数」集合（按行往回找最近的 `fn `）。
fn loc1a_sites(needle: &str) -> std::collections::BTreeSet<String> {
    let root = crate::guard_support::crate_src_root();
    let mut out = std::collections::BTreeSet::new();
    for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
        let prod = guard_core::production_code(&src);
        let lines: Vec<&str> = prod.lines().collect();
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        for (i, l) in lines.iter().enumerate() {
            if !l.contains(needle) || l.trim_start().starts_with("pub fn ") {
                continue;
            }
            let f = lines[..=i]
                .iter()
                .rev()
                .find_map(|x| {
                    x.split(" fn ")
                        .nth(1)
                        .or_else(|| x.strip_prefix("fn "))
                        .map(|r| {
                            r.chars()
                                .take_while(|c| c.is_alphanumeric() || *c == '_')
                                .collect::<String>()
                        })
                        .filter(|n| !n.is_empty())
                })
                .unwrap_or_default();
            out.insert(format!("{rel}::{f}"));
        }
    }
    out
}

#[test]
fn local_queries_reach_the_running_resident_backend_not_a_file_beside_the_exe() {
    // ② 谁能登记在 `<local>` 上（多行调用：`register(` 与 `LOCAL_ORIGIN` 分两行 ⇒ 按「函数里两样都有」认）。
    let registers = loc1a_sites("inbound_client::register(");
    let with_local: std::collections::BTreeSet<String> = registers
        .into_iter()
        .filter(|site| {
            let (file, _) = site.split_once("::").unwrap();
            let src =
                std::fs::read_to_string(crate::guard_support::crate_src_root().join(file)).unwrap();
            // 路径短了一截，rustfmt 可能把实参折回同一行 ⇒ 去掉空白再认。
            let flat: String = guard_core::production_code(&src)
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            // stdio 那条载体登记在它那一条路的名下（`StdioRoute`）；产品那一条（`StdioRoute::local`）就是 `<local>`。
            flat.contains("register(crate::inbound_client::LOCAL_ORIGIN")
                || (flat.contains("register(route.origin.as_wire_str()")
                    && flat.contains("origin:crate::origin::Origin::local()"))
        })
        .collect();
    let want: std::collections::BTreeSet<String> = [
        "local_backend.rs::local_stdio_consumer",
        "local_backend_host.rs::attach_stream",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let files = |s: &std::collections::BTreeSet<String>| -> std::collections::BTreeSet<String> {
        s.iter()
            .map(|x| x.split("::").next().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        files(&with_local),
        files(&want),
        "能登记在 `<local>` 上的生产文件变了 —— 只许是两个载体（拿活连接的 hello 造客户端的那两处）"
    );
    // ③ 谁还在找 exe 旁那份文件：只剩给终端窗口导环境那一处。
    let beside = loc1a_sites("resolve_beside_this_exe(");
    let beside: std::collections::BTreeSet<String> = beside
        .into_iter()
        .filter(|s| !s.ends_with("::resolve_beside_this_exe"))
        .collect();
    assert_eq!(
        beside,
        [
            // 起本机常驻后端 / 自释放内嵌那份之前先看旁边有没有（起它，不是问它）。
            "local_backend.rs::resolve_or_extract",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        "又有代码去找 exe 旁那份后端了 —— 本机查询只许问登记在 `<local>` 上的那条活连接（RT1 F2 那一形）"
    );
    // ④ 给终端窗口导 `CCM_BACKEND_BIN` 的那一格（WIN1 报备的同病）：两处调用都交**正在跑的那一份**。
    //    两向：`backend_bin_env_for_window(` 的生产调用点 == 实参是 `running_backend_bin()` 的那几处。
    // 开窗的两个平台臂（两处调用都在里面）搬进 `platform/terminal.rs`。
    let launch = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/platform/terminal.rs"
    ));
    let calls = launch.matches("backend_bin_env_for_window(").count();
    let running = launch
        .matches("backend_bin_env_for_window(\n        crate::local_backend_host::running_backend_bin(),")
        .count();
    assert!(
        calls >= 1,
        "尺子瞎了：platform/terminal.rs 里一处 `backend_bin_env_for_window(` 都没数到"
    );
    assert_eq!(
        calls, running,
        "给终端窗口的后端路径有一处不是「正在跑的那一份」（`running_backend_bin`）"
    );
    // 正控：同一把尺子数得到一处明摆着的调用。
    assert!(loc1a_sites("fn backend_bin_env_for_window(").len() == 1);
}

// ════════════════════════════════════════════════════════════════════════════
// 一件事一个总期限（分页读逐页收紧，不重新计时）
//
//  守的要求：「**一次调用一个绝对时刻**，不是每跳一个 `Duration`」·
//  「`Duration` 跨跳传递时每一跳都会重新开始计时 —— 那正是病 2 的机制。绝对时刻只能收紧、不能放宽」·
//  「**造**期限的那一手住调用方」。
// ════════════════════════════════════════════════════════════════════════════

use crate::inbound_client::{park, BackendHello, InboundClient};
use tokio::io::AsyncBufReadExt;

/// 一个真 `InboundClient` 接一根内存双工管子，登记在全局表里一个**只有本用例用**的 origin 名下；
/// 对端（后端）由用例扮演：从管子里读请求行、经 `route_reply` 把应答交回（同生产里读循环做的那一步）。
fn client_for_test(
    origin: &str,
    commands: &[&str],
) -> (
    std::sync::Arc<InboundClient>,
    tokio::io::BufReader<tokio::io::DuplexStream>,
) {
    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    let hello = BackendHello::from_hello_frame(&crate::stream_source::InboundFrame::Hello {
        v: 1,
        build_id: "test".into(),
        host_arch: "x86_64".into(),
        claude_dir: "/tmp".into(),
        homes: vec![],
        capabilities: vec![],
        commands: commands.iter().map(|s| s.to_string()).collect(),
        unavailable: vec![],
        uncancellable: vec![],
    })
    .expect("是 Hello 帧");
    let client = park(mine).into_client(hello);
    inbound_client::register(origin, client.clone());
    (client, tokio::io::BufReader::new(theirs))
}

/// 读对端收到的下一行请求；`within` 内没有 ⇒ `None`。
async fn next_request(
    peer: &mut tokio::io::BufReader<tokio::io::DuplexStream>,
    within: Duration,
) -> Option<Value> {
    let mut line = String::new();
    match tokio::time::timeout(within, peer.read_line(&mut line)).await {
        Ok(Ok(n)) if n > 0 => Some(serde_json::from_str(line.trim_end()).expect("请求行是 JSON")),
        _ => None,
    }
}

// D2（分页读逐行那一件一个总期限）随被测的 `read_lines`〔散文墓碑〕删了：那一件（子 agent 读整段）进了后端；
//   今天仍分页的那一件（旁路快照）由 D5b 钉「期限在翻页循环之外造」。

/// D3 ★ **期限已经过了 ⇒ 一个字节都不发**（同 `src/comms/inward/chan.ts`「已经过了 ⇒ 一个字节都不发」）；
/// 还没过 ⇒ 恰好发一行（正控：同一个对端、同一个读法认得出一行）。
#[tokio::test]
async fn an_expired_deadline_sends_nothing() {
    let origin = Origin("dl1-d3-expired".into());
    let (client, mut peer) = client_for_test(origin.as_wire_str(), &["history-tail"]);
    let gone = Deadline::within(Duration::ZERO);
    let err = tail(&origin, "/p/s.jsonl", 10, gone)
        .await
        .expect_err("期限已过却问成了");
    assert_eq!(
        err,
        copy_text(
            "rsFrameQuery.call.overdue",
            &[
                ("who", &who(&origin)),
                ("dur", &copy_core::format_elapsed(Duration::ZERO))
            ]
        )
    );
    assert!(
        next_request(&mut peer, Duration::from_millis(150))
            .await
            .is_none(),
        "期限已过还是发出去了"
    );

    // 正控：期限还远 ⇒ 恰好一行、就是这一问。
    let c = client.clone();
    let o = origin.clone();
    let asking = tokio::spawn(async move {
        tail(
            &o,
            "/p/s.jsonl",
            10,
            Deadline::within(Duration::from_secs(5)),
        )
        .await
    });
    let req = next_request(&mut peer, Duration::from_secs(5))
        .await
        .expect("期限还远却一行都没发");
    assert_eq!(req["cmd"], "history-tail");
    c.route_reply(
        req["id"].as_str().expect("id"),
        true,
        None,
        None,
        None,
        Some(json!({"total": 3, "tail_from": 0, "split_at": 0, "end": 30})),
    );
    assert!(asking.await.expect("task").is_ok());
    assert!(
        next_request(&mut peer, Duration::from_millis(150))
            .await
            .is_none(),
        "一问发了不止一行"
    );
    inbound_client::unregister(origin.as_wire_str(), &client);
}

/// D4 ★ **本模块只用、不造**：`Deadline` 的构造恰好一处（`within`），生产段零处另起截止时刻
/// （`Instant::now() +` 恰好一处且在 `within` 里）；出口函数的签名里零个 `Duration` 形参（期限一律是 `Deadline`）。
/// 正控：同一识别器在合成语料里认得出一个带 `Duration` 形参的出口、一处多出来的 `now() +`。
#[test]
fn this_module_uses_the_deadline_it_is_given_and_never_makes_one() {
    let prod = guard_core::strip_comment_lines(&guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/frame_query.rs"
    )));
    let adds = |code: &str| code.matches("Instant::now() +").count();
    // 字段是私有的 ⇒ 结构体字面量只可能写在本文件里；`until:` 恰好两处 = 字段定义一处 ＋ 构造一处。
    let builds = |code: &str| code.matches("until:").count();
    let duration_params = |code: &str| -> Vec<String> {
        code.split("pub(crate) async fn ")
            .skip(1)
            .filter_map(|f| {
                let sig = f.split('{').next()?;
                sig.contains(": Duration")
                    .then(|| f.split('(').next().unwrap_or("").to_string())
            })
            .collect()
    };
    assert_eq!(
        adds(&prod),
        1,
        "起算截止时刻的地方不是恰好一处（只许在 `Deadline::within` 里）"
    );
    assert_eq!(
        builds(&prod),
        2,
        "`Deadline` 的构造不是恰好一处（`until:` 应是字段定义 ＋ 构造各一处）"
    );
    let within = guard_core::find_pinned(&prod, "pub(crate) fn within(")
        .unwrap_or_else(|e| panic!("`Deadline::within` 不是恰好一处：{e}"));
    let body = &prod[within..within + prod[within..].find("\n    }\n").expect("within 的函数体")];
    assert_eq!(adds(body), 1, "那唯一一处起算不在 `within` 里");
    assert_eq!(
        duration_params(&prod),
        Vec::<String>::new(),
        "这几个出口又收 `Duration` 了 —— 期限得是发起方造好的 `Deadline`"
    );
    // 正控
    let synthetic = "pub(crate) async fn page(o: &Origin, budget: Duration) -> R {\n    let u = Instant::now() + budget;\n}\n";
    assert_eq!(
        duration_params(synthetic),
        vec!["page".to_string()],
        "识别器认不出 `Duration` 形参"
    );
    assert_eq!(adds(synthetic), 1, "识别器认不出 `now() +`");
}

/// D5 登记表：**造期限的那一手**（`Deadline::within(` 的调用点，按「所在函数」记）。每行写理由。
/// 多一处 = 又长出一个发起点（进表、写这件事是什么、值给多少）；少一处 = 那件事不再有期限了（或者搬了家没改表）。
const DEADLINE_MAKERS: &[(&str, &str, usize, &str)] = &[
    // `tasks.rs` 那一行摘了：monitor 那份任务 notify 删了，不再问 `tasks-list`。
    // `subagent·rs` 的 `query` · `history·rs` 的整份读 · `session_skeleton·rs` 的 `read_session_lines` 三行摘了〔散文墓碑〕：
    //   那三条命令退役（界面经通道直问那台后端，期限在界面那一手造）。
    (
        "stream_source/snapshot.rs",
        "fetch_snapshot",
        3,
        "快照是两件事：先问图（一问，`PAGE_BUDGET`）· 读正文（分页，问图之后按要读的字节数给 `read_budget`）；\
续传时多一件：读正文之前先读回续点那一行核见证（一问，`PAGE_BUDGET`）—— 2 → 3，多的就是这一处",
    ),
    // LOC1a / LOC1b 新长的四个发起点（各自带着自己的值，DL1 只把形状换成 `Deadline`）：
    // acct-iso 两问那两个发起点摘了：界面经通道直问（`src/frontend/ui/acct-iso-reads.ts`），期限在那边造。
    // 在那台分叉一条会话那一行摘了：monitor 不再发（界面经通道直说 `session-fork`，期限在界面那一手造）。
    // 远端钩子诊断那一行摘了：monitor 不再问（界面经通道直问 `hooks-diag`，期限在界面那一手造）。
    // MCP 列表那一行摘了：界面经通道直问（`src/frontend/ui/mcp-reads.ts`），期限在那边造。
    // 列远端 tmux 会话那一行（monitor 问那台后端 `tmux-list`）摘了：界面经通道直问（`src/frontend/ui/tmux-reads.ts`），期限在那边造。
];

/// 一份生产段里 `Deadline::within(` 的每一处，按「所在的最近一个 `fn` 名」记账（定义那一行不算）。
/// 形状照 `dial_host_tests::lives_long_sites`（同一种「按所在函数 × 处数」的登记表）。
fn deadline_makers(file: &str, prod: &str) -> Vec<(String, String)> {
    let code = guard_core::strip_comment_lines(prod);
    let mut out = Vec::new();
    let mut current_fn = String::new();
    for line in code.lines() {
        let t = line.trim_start();
        if let Some((head, rest)) = t.split_once("fn ") {
            if head
                .split_whitespace()
                .all(|w| matches!(w, "pub" | "pub(crate)" | "async" | "const" | "unsafe"))
            {
                current_fn = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
            }
        }
        for _ in 0..t.matches("Deadline::within(").count() {
            out.push((file.to_string(), current_fn.clone()));
        }
    }
    out
}

/// D5 ★ **造期限的调用点 == 登记表**（两向，按所在函数与处数）。
/// 正控：合成语料里多种一处必被认出、且认对所在函数。
#[test]
fn every_deadline_is_made_where_its_job_begins_and_only_there() {
    let root = crate::guard_support::crate_src_root();
    let mut got: Vec<(String, String)> = Vec::new();
    let mut scanned = 0usize;
    for (path, file_text) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        scanned += 1;
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        got.extend(deadline_makers(
            &rel,
            &guard_core::production_code(&file_text),
        ));
    }
    assert!(
        scanned > 100,
        "只扫到 {scanned} 份 monitor 源码 —— 遍历坏了"
    );
    let mut want: Vec<(String, String)> = DEADLINE_MAKERS
        .iter()
        .flat_map(|(f, func, n, why)| {
            assert!(
                why.chars().count() >= 15,
                "`{f}::{func}` 没写清这件事是什么"
            );
            std::iter::repeat_n((f.to_string(), func.to_string()), *n)
        })
        .collect();
    got.sort();
    want.sort();
    assert_eq!(
        got, want,
        "造 `Deadline` 的地方与登记的发起点对不上。\n\
         多出来的 ⇒ 新长了一个发起点：进 `DEADLINE_MAKERS`、写清这件事是什么、值给多少；\n\
         少了的 ⇒ 那件事不再在开头造期限了（分页读会退回每页重新计时）"
    );
    let synthetic = "pub(crate) async fn fetch_x() {\n    let d = frame_query::Deadline::within(PAGE_BUDGET);\n}\n";
    assert_eq!(
        deadline_makers("x.rs", synthetic),
        vec![("x.rs".to_string(), "fetch_x".to_string())],
        "识别器瞎了 —— 上面那条相等不可信"
    );
}

/// D5b ★ **分页读的那两件，期限在翻页循环之外造**（第一页之前一次）—— 造在循环里就是每页重新计时，
/// 而 D5 按「所在函数 × 处数」数不出这一形（搬进循环处数不变）。
/// 正控：同一个找法在一份把造期限挪进循环的合成函数上报出来。
#[test]
fn paged_jobs_make_their_deadline_before_the_first_page() {
    // `(源码, 函数头, 翻页循环的起头)`：循环起头取各自生产代码里那一行的原文。
    // 历史浏览器读整份那一件进了界面，后来查看器改成骨架 ＋ 按视口取（`src/frontend/ui/record-reads.ts::readRange`），整份读这件事没了。
    let snapshot_src = crate::guard_support::stream_source_file("snapshot.rs");
    let cases: [(&str, &str, &str); 1] =
        [(&snapshot_src, "async fn fetch_snapshot(", "'read: for ")];
    fn made_after_loop(prod: &str, head: &str, loop_head: &str) -> Result<bool, String> {
        let at = guard_core::find_pinned(prod, head)?;
        let rest = &prod[at..];
        let end = rest[1..]
            .find("\nasync fn ")
            .into_iter()
            .chain(rest[1..].find("\nfn "))
            .chain(rest[1..].find("\npub"))
            .min()
            .map_or(rest.len(), |e| e + 1);
        let body = guard_core::strip_comment_lines(&rest[..end]);
        let last_make = body
            .rfind("Deadline::within(")
            .ok_or_else(|| format!("`{head}` 里一处造期限都没有"))?;
        let first_loop = body
            .find(loop_head)
            .ok_or_else(|| format!("`{head}` 里找不到翻页循环 `{loop_head}`"))?;
        Ok(last_make > first_loop)
    }
    for (src, head, loop_head) in cases {
        let prod = guard_core::production_code(src);
        assert_eq!(
            made_after_loop(&prod, head, loop_head),
            Ok(false),
            "`{head}` 的期限造在翻页循环里（或找不到）—— 每页重新计时回来了"
        );
    }
    let synthetic = "async fn fetch_snapshot(x: u8) {\n    'read: for p in pages {\n        let d = Deadline::within(PAGE_BUDGET);\n    }\n}\n";
    assert_eq!(
        made_after_loop(synthetic, "async fn fetch_snapshot(", "'read: for "),
        Ok(true),
        "识别器瞎了 —— 上面那条不可信"
    );
}

/// D6 分页读的总时限：下界是一页的期限、随字节数单调、在两个字节上限处等于手算的秒数
/// （历史浏览器 256 MiB ⇒ 60 ＋ 512 = 572 s；快照 512 MiB ⇒ 60 ＋ 1024 = 1084 s）；（读整段那一件随子 agent 那条命令进了后端。）
#[test]
fn the_read_budget_grows_with_the_bytes_from_one_page_up() {
    assert_eq!(read_budget(0), PAGE_BUDGET);
    assert_eq!(read_budget(1), PAGE_BUDGET + Duration::from_secs(1));
    assert!(read_budget(10 << 20) < read_budget(11 << 20));
    assert_eq!(read_budget(256 << 20), Duration::from_secs(572));
    assert_eq!(read_budget(512 << 20), Duration::from_secs(1084));
}

/// ★跨语言金样的 monitor 那一侧：后端 `history-read` 真出的逐行成品
/// （`tests/__fixtures__/record-reads.golden.json`，后端 `read_face_tests.rs` 钉着它 == 帧面现打）
/// 经 [`row_of`] 读得懂：可计行三条、末端是原始字节、不进界面的那条没有成品、`cwd` 与成品原样转交。
#[test]
fn the_snapshot_rows_of_the_golden_decode_through_row_of() {
    let golden: Value =
        serde_json::from_str(include_str!("../../__fixtures__/record-reads.golden.json"))
            .expect("金样不是合法 JSON");
    let rows: Vec<Row> = golden["history-read"]["rows"]
        .as_array()
        .expect("金样里没有 rows")
        .iter()
        .map(|r| row_of(r).expect("一行成品认不出"))
        .collect();
    assert_eq!(
        rows.iter().map(|r| r.end).collect::<Vec<_>>(),
        vec![Some(54), Some(152), Some(273)]
    );
    assert!(rows[0].record.is_none(), "不进界面的元数据记录不该带成品");
    assert_eq!(rows[1].cwd.as_deref(), Some("/w"));
    let m: Value = serde_json::from_str(rows[1].record.as_ref().unwrap().0.get()).unwrap();
    assert_eq!(
        m, golden["history-read"]["rows"][1]["record"],
        "成品没原样转交"
    );
    // 反向：多一格类型不对的 ⇒ 认不出（不猜）。
    assert!(row_of(&serde_json::json!({"end": 1, "hash": 1, "record": "x"})).is_none());
}

/// ★ 旁路快照那一页（`read_page` → `history-read`）交「折起那一行」那份声明：就是 `src/shared/views/folded-record.json`
/// （界面 `record-reads.ts` 用同一份），放在请求信封的 `view` 那一格、不进 `args`。
#[tokio::test]
async fn the_snapshot_page_carries_the_folded_view_from_the_shared_file() {
    let origin = Origin("proj6-folded-view".into());
    let (client, mut peer) = client_for_test(origin.as_wire_str(), &["history-read"]);
    let o = origin.clone();
    let asking = tokio::spawn(async move {
        read_page(
            &o,
            "/p/s.jsonl",
            0,
            Some(90),
            Deadline::within(Duration::from_secs(5)),
        )
        .await
    });
    let req = next_request(&mut peer, Duration::from_secs(5))
        .await
        .expect("一行都没发");
    let shared: Value =
        serde_json::from_str(include_str!("../../../src/shared/views/folded-record.json"))
            .expect("那份声明是 JSON");
    assert_eq!(req["cmd"], "history-read");
    assert_eq!(req["view"], shared, "旁路快照没交那份声明：{req}");
    assert!(req["args"].get("view").is_none(), "声明塞进了 args");
    client.route_reply(
        req["id"].as_str().expect("id"),
        true,
        None,
        None,
        None,
        Some(json!({"rows": [], "next": 90, "eof": true})),
    );
    assert!(asking.await.expect("task").is_ok());
    inbound_client::unregister(origin.as_wire_str(), &client);
}
