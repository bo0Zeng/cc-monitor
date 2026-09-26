//! 〔`C1` · 2026-09-24〕只读查询走长连接的判据（monitor 侧）。

use super::*;

/// 题面那八条 —— `设计/15 §3.2` 那一串逐字（`--accounts` 在盘上叫 `--list-accounts`）。
/// **异源**：这张表抄自设计篇，不从 [`MOVED`] 派生。
const DESIGN_EIGHT: &[&str] = &[
    "--list-projects",
    "--list-sessions",
    "--read-session",
    "--read-session-tail",
    "--session-accounts",
    "--list-accounts",
    "--search",
    "--list-subagents",
    // 〔SR1a · 09-24〕题面逐字「`--list-user-inputs` 与骨架 `--read-session-from-offset --index` 上帧面」。
    "--list-user-inputs",
    "--read-session-from-offset",
    // 〔SR1a × SE2〕协调方加的：`--find-in-session` 一起搬。
    "--find-in-session",
    // 〔C4c · 第四波 4B〕主会话裁「仍在拨号的 `--account-trust` / `--account-trust-zero`」随账号域上帧面（异源：题面那一句）。
    "--account-trust",
    "--account-trust-zero",
];

// 〔C4d · 第四波 4B〕这里原先还有一张「仍拨号」的题面表（C4c 起零条）—— 逐次拨号那条路删了，表随之摘掉。

fn sorted(v: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut v: Vec<String> = v.into_iter().collect();
    v.sort();
    v
}

/// 后端 `inbound.rs` 生产段里的每一块 `CommandSpec`：`(帧命令名, 那一块的原文)`（**从后端源码数**）。
/// 〔C4b〕抽成一处：下面两条判据（交给只读宿主的那几条 · 全部登记的帧命令）共用同一个切法。
fn backend_command_blocks() -> Vec<(String, String)> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../backend/inbound.rs");
    let src = std::fs::read_to_string(&p).expect("读后端 inbound.rs");
    let prod = guard_core::production_code(&src);
    let blocks: Vec<(String, String)> = prod
        .split("CommandSpec {")
        .skip(1)
        .filter_map(|blk| {
            let at = blk.find("name: \"")? + "name: \"".len();
            Some((blk[at..].split('"').next()?.to_string(), blk.to_string()))
        })
        .collect();
    blocks
}

/// 后端 `inbound.rs` 生产段里把活交给 `read_face::answer`（〔C4d〕或 `history_join::answer_*`）的帧命令名（**从后端源码数**）。
fn backend_read_face_commands() -> Vec<String> {
    let got: Vec<String> = backend_command_blocks()
        .into_iter()
        // 〔C4d · 第四波 4B〕出成品的那两条（`history-projects` / `history-sessions`）交给了 `history_join`（历史跨机 join 的唯一的家）——
        //   同一族「搬上帧面的只读查询」，宿主从换壳那一层挪到了出成品那一层 ⇒ 两个宿主一起数。
        .filter(|(_, blk)| {
            blk.contains("read_face::answer") || blk.contains("history_join::answer_")
        })
        .map(|(name, _)| name)
        .collect();
    assert!(!got.is_empty(), "从后端源码一条都没数到 —— 抽取坏了");
    sorted(got)
}

/// ★ 两向相等：[`MOVED`] 的左列 == 设计篇那八条 ＋ SR1a 两条；右列 == 后端真登记上帧面、交给只读宿主的那几条。
#[test]
fn the_moved_table_matches_the_design_list_and_the_backend_registry() {
    assert_eq!(
        sorted(MOVED.iter().map(|(f, _)| f.to_string())),
        sorted(DESIGN_EIGHT.iter().map(|s| s.to_string())),
        "搬上帧面的子命令与题面那几条不相等"
    );
    // 〔U4b〕右边还要并上「生在帧面上」的那几条（没有被替掉的拨号子命令，见 `BORN_ON_FRAME`）。
    // 〔C4c〕右列按**集合**比：信任预检两形合进一条帧命令（`accounts-trust` 在右列出现两次）。
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

/// ★〔C4d · 第四波 4B〕**逐次拨号那条路不存在了**（零命中 ＋ 正控）。
///
/// 守的要求：主会话 09-25 裁（`调研/第四波记录/C4d.md`「主会话裁」第 4 条，逐字）「`run_list_query`〔散文墓碑〕（逐次拨号那条路，
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
        guard_core::contains_word(&corpus, "run_routed"),
        "正控失败：识别器在同一份语料上认不出帧面出口 `run_routed` —— 上面的零命中不可信"
    );
}

/// ★ argv 分流认得本仓今天真在发的形状：区间取正文走帧面（〔C4b〕索引 · 查找 · 大纲三形改走通道，不再认）。
#[test]
fn argv_routing_covers_the_shapes_the_repo_actually_sends() {
    let range = crate::session_skeleton::range_argv("/p/s.jsonl", 10, 99);
    let range: Vec<&str> = range.iter().map(String::as_str).collect();
    match route_argv(&range) {
        Some(ArgvRoute::Read { path, from, upto }) => {
            assert_eq!((path.as_str(), from, upto), ("/p/s.jsonl", 10, Some(99)))
        }
        _ => panic!("按区间取正文那一形没走帧面"),
    }
    // 〔C4b · 第四波 4B〕索引 · 查找 · 大纲三形随那三条命令改走通道删了 —— 它们的 argv 造器一起删了，
    //   这里改成反向：那三个子命令**不再被认**（认得就说明 monitor 里又长出了一条发它们的路）。
    for gone in [
        &["--read-session-from-offset", "--index", "/p/s.jsonl", "7"][..],
        &[
            "--find-in-session",
            "--limit",
            "500",
            "--query",
            "q",
            "/p/s.jsonl",
        ][..],
        &["--list-user-inputs", "--from", "42", "/p/s.jsonl"][..],
    ] {
        assert!(
            route_argv(gone).is_none(),
            "{gone:?} 又被分流认出来了 —— 那一条已经只走通道"
        );
    }
    assert!(matches!(
        route_argv(&["--list-subagents", "/p/s.jsonl"]),
        Some(ArgvRoute::Lines("history-subagents", _))
    ));
    assert!(matches!(
        route_argv(&["--read-session", "/p/a.jsonl"]),
        Some(ArgvRoute::Read {
            from: 0,
            upto: None,
            ..
        })
    ));
}

// ════════════════════════════════════════════════════════════════════════════
//  〔C4a · 第四波 · 2026-09-24〕八条里哪几条已经**只走通道**（主界面经 `src/ipc/chan.ts`）
// ════════════════════════════════════════════════════════════════════════════

/// 已迁到通道的帧命令：前端经 `chan.call(origin, "<op>", …)` 直接问那台后端，monitor 那一跳只搬字节。
/// 迁的判准只有一条（`调研/第四波记录/C4a.md §3.2`）：迁过去之后，**那条应答的解释只有一个家**。
const CHANNELED: &[(&str, &str)] = &[
    (
        "accounts-sessions",
        "本机与远端都迁：两条 Tauri 命令（远端帧面 · 本机一次性 exec）各在 Rust 里把同一种行解析一遍；\
         迁过去之后逐行解释只剩 `src/account-reads.ts::parseSessionAccountLines` 一处",
    ),
    (
        "history-search",
        "远端那半迁：逐台 fan-out ＋ 补 origin ＋ 与本机索引合并三件事搬到 `src/views/history-search.ts`，\
         每件只有那一个家。〔LOC1b · 4D〕本机那半也迁了：本机也经通道问本机后端，monitor 内存索引删了",
    ),
    // 〔C4b · 第四波 4B〕会话读面那三条：**解释挪进后端、直接出成品**（`read_face.rs`），monitor 那一份
    //   「核头尾、剥行、失败分档」删了；界面经 `src/session-reads.ts` 问，本机与远端同一条路。
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
        "后端出成品 `{total, hits}`；命中口径只住后端（`search_query` ＋ `search-core`），monitor 那份核头尾删了",
    ),
    // 〔C4c · 第四波 4B〕账号域那两条（主会话裁：账号域读自己那台的 apikey 表、规则搬进 `acct-core`、agent 随请求带）。
    (
        "accounts-list",
        "后端出成品 `{meta, accounts, notice}`，并上**那台机器自己**那份 apikey 表（`acct_core::apikey_routed_subset`）；\
         monitor 那份行解析 / 降级说明 / 本机并表删了，界面经 `src/account-reads.ts::fetchAccounts` 问、按形状收，本机与远端同一条路",
    ),
    (
        "accounts-trust",
        "后端出成品 `{trusted, known}`（CLI 那一臂同一个函数）；替掉最后两条逐次拨号的 `--account-trust*`，\
         界面经 `src/account-reads.ts::checkTrust` 问",
    ),
    // 〔C4d · 第四波 4B〕历史跨机 join 那两条（主会话 09-25 裁：注解读写者换成本机常驻后端，它经 `remote_ask` 问远端那台、
    //   并上注解、出成品）—— 从 [`HELD_BACK`] 挪过来：「远端那台的后端出不了成品」那条理由由「问**本机**后端、带 `origin`」解开了。
    (
        "history-projects",
        "本机常驻后端出成品 `{rows, notice}`（`history_join.rs`：记录树 ＋ Codex 合成 ＋ pidfile 判活 ＋ 远端经 `remote_ask`，\
         并上本机注解）；monitor 那份 join（`history_project_from_row` 一族）与两条 Tauri 命令删了，界面经 `src/history-reads.ts` 问 `<local>`",
    ),
    (
        "history-sessions",
        "同 `history-projects`：会话行口径收成后端一份（`analyze_session`，本机与远端同一个函数），monitor 那份 `analyze_jsonl` /\
         `remote_session_entry` 删了",
    ),
];

/// 〔C4b · 第四波 4B〕**帧面只读查询那一族之外**、同样改成「前端经通道直接问、后端出成品」的帧命令 ——
/// `(帧命令, 为什么迁、迁了之后解释住哪)`。它们不在 [`MOVED`] 里（不是 `C1` 那一族），但前端 `chan.call` 的
/// 操作名集合要把它们算进来：下面那条两向判据的「前端那一侧」== [`CHANNELED`] ⊔ 本表。
/// 每一条还要**真的**是后端登记的帧命令（从后端 `inbound.rs` 生产段数，异源）、monitor 生产段里**零**字面量。
const CHANNELED_ELSEWHERE: &[(&str, &str)] = &[
    (
        "plugins-marketplaces",
        "`feature_face`（RM1b）那一族：后端应答就是整份 survey（成品），monitor 那条命令（`list_plugin_marketplaces`）\
         只在核「恰一行 ＋ 严格形状」—— 核验搬到唯一的消费者 `settings/plugins-section.ts::decodeSurvey`，命令与 `plugins.rs` 删了",
    ),
    // 〔C4c · 第四波 4B〕`C4b.md §6.6` A 组第一批：后端已是成品、monitor 只在转的那一条。
    (
        "history-record",
        "`BORN_ON_FRAME`（U4b）那一条：后端应答就是成品 `{present, root}`，monitor 那条命令（`probe_session_record`）\
         只在转、核两格 —— 核验搬到唯一的消费者 `src/session-reads.ts::decodeRecord`（缺一格仍是契约坏了，不读成「不在」），\
         命令与发送端（`frame_query::record`）删了",
    ),
    // 〔C4c · 第四波 4B〕`C4b.md §6.6` A 组第二批：`app.backend-policy`（S5 进主线后前置满足）。
    (
        "exit-policy-read",
        "〔B2〕问那台机器「退出行为」那个值：monitor 那条命令（`backend_exit_policy`）只在拦空白名 ＋ 转 ＋ 原样交回，\
         解释本来就在界面（`settings/backend-section.ts::readExitAnswer`）⇒ 设置页经通道直接问，命令删了。\
         ⚠ monitor 自己**另有**一处问它（退出臂，见 [`ASKED_BY_MONITOR_ITSELF`]）—— 那不是替界面转",
    ),
    (
        "exit-policy-set",
        "〔B2〕交那台机器写那个值：同上一行（`set_backend_exit_policy` 删了），画的仍是后端写完读回来的那一份",
    ),
    (
        "assets-catalog",
        "〔AS2 · 4B · V113〕资产目录：生来就走通道（没有过 monitor 那一条）—— 后端应答就是成品（整份目录 ＋「这台缺什么」的判定），\
         界面 `settings/assets-section.ts::decodeCatalog` 按形状收",
    ),
    // 〔C4d · 第四波 4B〕注解三条：读写者换成本机常驻后端（`history_annotations.rs`，文件原地不动）；monitor 那两条命令
    //   （`update_history_metadata` / `list_last_accounts`）与删会话时那一句清注解删了，界面经 `src/history-reads.ts` 问 `<local>`。
    (
        "history-annotate",
        "改一条注解：后端严格读 → 只改那一条 → 原子写，回那一条（`history-reads.ts::decodeEntry` 按形状收）",
    ),
    (
        "history-forget",
        "删会话之后连带删那一条注解（界面删成功之后交；从前是 monitor 删完顺手清）",
    ),
    (
        "history-last-accounts",
        "sid → 上次用哪个号起（账号徽章回落 · 带账号 resume 前现读）；从前是 monitor 读那份文件",
    ),
    // 〔US1 · 第四波 4D〕`05 §14.3` B 组：API key 那两问（`creds.apikey` 读 · `apikey.routing`）—— 后端出成品，界面经 `src/apikey-reads.ts` 问。
    (
        "apikey-read",
        "那台机器上那份凭据文件的状态（只回掩码）：monitor 那条命令（`read_apikey_credentials_status`）本机自己读文件、远端转这一条          —— 本机那一份读者删了，界面按形状严格收（`apikey-reads.ts::decodeApikeyStatus`，跨语言金样 `apikey.golden.json`）。         ⚠ monitor 自己**另有**一处问它（写 key 之前核路径，见 [`ASKED_BY_MONITOR_ITSELF`]）—— 那不是替界面转",
    ),
    (
        "apikey-routing",
        "这几个号在那台的表里有没有行 · 那台的中转在不在：monitor 那条命令（`apikey_routing_for`）本机自己读凭据文件 ＋ 连回环口、         远端转 `apikey-read` 的 `rows` ＋ `relay-status`，再调 `acct-core` 那条规则 —— 人群与判准整个搬进后端（`accounts/upstream/endpoint.rs`），命令删了",
    ),
    // 〔SU1 · 第四波 4C · V116〕skill 卸的「看」那一半：生来就走通道（没有过 monitor 那一条）。
    (
        "skill-installs",
        "这台记着的、从别处装来的 skill：后端应答就是成品 `{installs}`，界面 `settings/assets-section.ts::decodeInstalls` 按形状收",
    ),
    (
        "skill-uninstall-plan",
        "卸之前看：逐文件的态与「要不要问」都是那台后端答的成品，界面 `settings/assets-section.ts::decodeUninstallPlan` 按形状收。\
         ⚠ monitor 自己**另有**一处问它（带 `take` 的那一问，见 [`ASKED_BY_MONITOR_ITSELF`]）—— 那不是替界面转",
    ),
    // 〔C4e · 第四波 4C〕`C4c.md §5.6` A 组 `tmux.manage` 第一格：抓一屏。
    (
        "capture-pane",
        "后端应答就是成品 `{name, screen}`，monitor 那条命令（`capture_remote_pane`）只在拒空目标、预问认不认、转、\
         把五个拒绝码说成人话 —— 那一份解释搬到唯一的消费者那一侧 `src/tmux-control.ts::capturePane`\
         （预览窗 `views/pane-preview.ts` 调它），命令与发送端（`capture_via_backend`〔散文墓碑〕）删了",
    ),
    // 〔C4e · 第四波 4C〕`tmux.manage` 其余两格（杀会话 · 送键）＋ `launch.send-into`（就地 resume）：
    //   三条 Tauri 命令（`kill_remote_tmux` / `tmux_send_keys` / `backend_send_into`〔散文墓碑〕）同拍迁完 ——
    //   `launch` 这条帧命令当年有两个 monitor 发送端（送键 · 就地 resume），只迁一个的话本表就得为过渡态开一格。
    (
        "kill",
        "后端应答就是成品 `{session, killed}`（身份门 ＋ 窗口门在后端先过、对句柄下手）；monitor 那条命令只在拒空目标、\
         转、核 `killed`、按三态说人话 —— 那一份解释搬到 `src/tmux-control.ts::killSession`，命令与发送端\
         （`backend_kill.rs`）删了",
    ),
    (
        "launch",
        "界面只说它的 `send-into` / `send-keys-raw` 两个 mode（送键 · 就地 resume；`create-or-attach` 归 ccm）；\
         后端应答就是成品 `{session, created, typed}`。monitor 那两个发送端（`backend_send_keys.rs` / `backend_launch.rs`）\
         只在拒空目标 / 空载荷、把 `enter` 翻成 mode 名、核 `typed`、按三态说人话、给就地 resume 判「能不能回落」—— \
         那一份搬到 `src/tmux-control.ts::sendKeys` / `sendInto`（F14 那条规则住 `ipc/chan-caller.ts::provablyNotSent`，\
         与 Rust `route_call_error` 跨语言金样对拍）",
    ),
    // 〔C4e · 第四波 4C〕`C4c.md §5.6` A 组 `cc-bus.cockpit`：五条 Tauri 命令（`check_cc_bus_agent_online` / `cc_bus_send` /
    //   `cc_bus_kill` / `cc_bus_spawn` / `cc_bus_broadcast`〔散文墓碑〕）同拍迁完。广播那一条先在后端长出 `bus-broadcast`
    //   （挑人 ＋ 逐个投递搬进后端），界面才只剩「说成人话」。解释的家：`src/cc-bus-control.ts`（单一住址由
    //   `cc_bus_tests.rs::the_front_end_speaks_the_bus_ops_only_through_one_module` 判）。
    (
        "bus-list",
        "查一个 agent 在不在线：后端应答就是成品（名单 ＋ 每人 `live`），monitor 那条命令只在转 ＋ 按 id 挑一个 ——\
         挑人与「问不到 ≠ 离线」搬到 `src/cc-bus-control.ts::agentOnline`",
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
        "〔C4e〕新帧命令：广播从前是 monitor 自己列名单、挑在线的、逐个 `cc-send`；挑人与逐个投递搬进后端\
         （`control/cc_bus.rs::broadcast_for_inbound`，应答是成品计数 ＋ 逐个失败），界面 `broadcast` 只说成人话",
    ),
    // 〔LOC1a · 第四波 4D · C4e 批 4〕`session.tasks`。
    (
        "tasks-list",
        "后端出成品 `{tasks}`（字段语义挪进后端 `tasks_query.rs::task_entry`），界面 `tasks-panel.ts::decodeTasks` 按形状收；\
         monitor 那条命令（`get_session_tasks`）与行解释（`parse_task_lines`）删了。\
         ⚠ monitor 自己**另有**一处问它（本机任务 watcher，见 [`ASKED_BY_MONITOR_ITSELF`]）—— 那是推送，不是替界面转",
    ),
];

/// 〔C4e · 第四波 4C〕monitor 生产段里**拼写与某条已迁帧命令相同、却不是发送点**的字面量 —— `(拼写, 处数, 为什么)`。
///
/// 下面那条判据按字面量 `"<op>"` 数 monitor 里还剩几个发送点；`kill` 这个词在 monitor 里另有一处正当的用法
/// （起系统的 `kill` 进程），它与帧命令 `kill` 同拼写、不同义。两向相等：多一处 = 又长出一个发送点（或又一处同拼写，
/// 要来这里表态）；少一处 = 那一处用法没了，这一行馊了。
const SAME_SPELLING_NOT_A_SEND: &[(&str, usize, &str)] = &[(
    "kill",
    1,
    "`local_backend_host.rs::signal_term` 起系统的 `kill` 进程（`kill -TERM <pid>`，Linux 上给一个进程发 SIGTERM）—— \
     与后端的帧命令 `kill`（结束 tmux 会话）同一个拼写，不是发送点",
)];

/// 〔C4c · 第四波 4B〕**monitor 自己**（不是替界面转）也要问的帧命令 —— `(帧命令, 生产段里几处, 为什么)`。
///
/// 它们同时在 [`CHANNELED_ELSEWHERE`] 里（界面那一问已走通道）；下面那条判据里 monitor 生产段的字面量
/// 从此 == `BORN_ON_FRAME` 那一次 ＋ 本表登记的处数（两向相等：多一处 = 又长出一个替界面转的发送点；
/// 少一处 = monitor 那件自己的事不问了，这一行馊了）。
const ASKED_BY_MONITOR_ITSELF: &[(&str, usize, &str)] = &[
    (
        "exit-policy-read",
        1,
        "退出臂在决定那一刻现问一次（`backend_policy::kill_on_exit_now`，`设计/01 §3.3b ④`）：\
         monitor 自己要不要跟着收那台后端 —— 它的答案不给界面",
    ),
    (
        "apikey-read",
        1,
        "〔GP1 · US1〕写 key 之前核「那台后端写的就是本 monitor 认的那一份」（`apikey_remote::send_key` 只读 `path`，\
         `CCM_DATA_DIR` 隔离跑时接错后端 ⇒ 拒写）—— 它的答案不给界面",
    ),
    (
        "skill-uninstall-plan",
        1,
        "〔SU1〕卸那一趟真要删之前，带 `take` / `confirm` 再问一次「真要删哪几个」（`skill_install.rs::uninstall_with`）：\
         它的 `delete` / `forget` 是 monitor 自己编排删与摘记录要的，不给界面（同装那一侧 `skill-install-plan` 带 `take` 那一问）",
    ),
    (
        "tasks-list",
        1,
        "〔LOC1a〕本机任务 watcher（`tasks.rs::fetch_session_tasks`）：notify 报「哪个 sid 变了」之后经 `<local>` 问成品、推 `task-update` \
         —— 推送那一路是 monitor 自己的事（`05 §14.3`「推送那一路」待定：是否换 `subscribe` 不在本件），它不解释字段",
    ),
];

/// 后端 `inbound.rs` 生产段里登记的全部帧命令名（异源：从后端源码数，不读本文件的表）。
fn backend_registered_commands() -> std::collections::BTreeSet<String> {
    backend_command_blocks()
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

/// 还留在 monitor 侧发送的那几条 —— `(帧命令, 为什么今天不迁)`。**不是豁免清单**：
/// 下面那条判据要求它们**真的**还有 monitor 侧发送点（没了 ⇒ 这一行的理由已经馊了）。
///
/// 〔C4b · 第四波 4B〕逐行重裁过（`调研/第四波记录/C4b.md §1`）：判准照旧是「业务解释只有一个家」，
/// 正路是「解释挪进后端、直接出成品」。九行里三行做到了（挪进了 [`CHANNELED`]）；下面六行**逐条写清卡在哪**。
/// 〔C4c · 第四波 4B〕主会话裁六行的去向：`accounts-list` 做了（挪进 [`CHANNELED`]）；`history-projects` /
/// `history-sessions` 的设计写在 `调研/第四波记录/C4c.md §3`（「本机后端问远端后端」那一跳今天不存在，报备中）；
/// `history-read` / `history-subagents` 等后端二次拆包；`history-tail` 归 CF2。⇒ 今天五行。
/// 〔C4d · 第四波 4B〕`history-projects` / `history-sessions` 做了（「本机后端问远端后端」那一跳由 `remote_ask` 造出来）⇒ 今天三行。
const HELD_BACK: &[(&str, &str)] = &[
    // 〔C4c · 第四波 4B〕`accounts-list` 那一行挪进了 [`CHANNELED`]（账号域搬家做了：后端出成品、并它自己那份表）。
    // 〔C4d · 第四波 4B〕`history-projects` / `history-sessions` 两行挪进了 [`CHANNELED`]（跨机 join 进了本机常驻后端）。
    // 〔C4c · 第四波 4B〕下面三行按主会话裁决重写：`history-read` / `history-subagents` **等后端二次拆包**，
    //   `history-tail` 归 CF2（`subscribe`）。三行都仍有 monitor 侧发送点（判据照旧要求它们真有）。
    (
        "history-read",
        "**等后端二次拆包**（主会话裁，4B 第六批；〔LOC1b · 4D〕现打重核、主会话认可）：本机远端今天都由 monitor 经那台后端\
         的 `history-read` 取原文页、自己解析（`history·rs::stream_read_session_jsonl`，本机远端同一条路）；前端直接拿成品不行 ——\
         后端**没有任何** `JsonlRecord` 解析（`agents/claudecode/records.rs` 只装文件名形态，`agents/codex/parse.rs` 只镜像\
         turn-end / usage），出成品就得把 `messages.rs` / `parser.rs` / `codex_record.rs`（ts-rs 类型的来源）搬进两边共用的 crate\
         —— 那就是二次拆包，而且要挪文件（V123：目录重排排在最后单独做）。那一拆之前不迁，不在 TS 再写一份记录解析",
    ),
    (
        "history-subagents",
        "**等后端二次拆包**：挑候选（`choose_subagent`）之后还要读那份文件、过记录解析（`parse_line`）⇒ 同 `history-read`",
    ),
    (
        "history-tail",
        "**归 CF2（`subscribe`）**：它不是前端查询，只被实时 tab 的快照续点用（`ssh_source` 的流机器）——\
         会话流收口成 `subscribe`（`设计/05 §8` 步 6）时由 CF2 处置，不属于 `call`",
    ),
    // 〔C4a 与 SR1a 合并〕SR1a 同波搬上来的 `history-index` / `history-user-inputs` / `history-find` 三行
    // 〔C4b · 第四波 4B〕挪进了 [`CHANNELED`]（后端出成品）。
];

/// monitor 生产段（`src/bridge/src/**/*.rs`，剥注释与 `#[cfg(test)]`）里，一条帧命令的字面量出现几次。
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
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
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
    moved.dedup(); // 〔C4c〕信任预检两形合进一条帧命令
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
    // 〔C4b〕那一族之外的已迁：真是后端的帧命令（异源）· 不在 `MOVED` 里 · monitor 生产段零字面量。
    let registered = backend_registered_commands();
    assert!(
        registered.len() > 20,
        "从后端 `inbound.rs` 只数到 {} 条帧命令 —— 抽取坏了",
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
        // 〔C4c〕生在帧面上的那几条在 `BORN_ON_FRAME` 里各有一次字面量（判据的一侧，不是发送点）；
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
    let mut still_sent_by_monitor: Vec<String> = Vec::new();
    for (op, _) in MOVED.iter().map(|(_, c)| (*c, ())) {
        // `MOVED` 那几行自己就是字面量出现（〔C4c〕信任预检两形合进一条帧命令 ⇒ 那一条在表里出现两次）。
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

// 〔C4c · 第四波 4B〕`history-record` 应答解释那条判据（`parse_record`〔散文墓碑〕）随发送端删了；同一条口径
//   「缺一格是契约坏了，**绝不**读成『不在』」搬到 TS 那一侧 `session-reads.ts::decodeRecord`（`tests/session-reads.vitest.ts`）。

// ═════════════════════════════════════════════════════════════════════════════
// 〔LOC1a · 第四波 4D〕本机查询接的是**正在跑的那份**常驻后端（WIN1 撤回的 F2 由本件接住）
// ═════════════════════════════════════════════════════════════════════════════
//
// 要求住址：`设计/05 §14.6` 逐字「本机那几问从『exec 一次性本机后端』改走 `<local>` 长连接」·
// `设计/01 §5 D11`「后端是给定的、不留退路」。RT1 F2 读数：旧那条 `run_query` 只认 exe 旁边那一份文件、
// 不认自释放之后正在跑的那一份 ⇒ Windows 上本机那几问一直「后端不在」。
// 三格，异源各在一处：
// ① 发送：本机那几问只经 `inbound_client::client_for("<local>")`（行为判据在 `subagent_tests` / `remote_branch_tests` /
//    `local_accounts_tests`：假后端那一侧真收到了帧命令）；
// ② 谁能登记在 `<local>` 上：生产段里 `register(LOCAL_ORIGIN, …)` 的文件集合 == 两个载体（常驻回环 · stdio 监护），
//    两处都是拿**已经回了 hello 的那条活连接**造客户端 ⇒ 登记在那里的就是正在跑的那一份；
// ③ 谁还在「找 exe 旁那份文件」：生产段里 `resolve_beside_this_exe(` 的调用点集合 == {起 / 自释放常驻后端那两处}
//    （都不是查询；一条查询路径都不许再用它）。两向相等，带正控。
// ④〔WIN1 报备〕给终端窗口的 `CCM_BACKEND_BIN` 同病：两处都交 `local_backend_host::running_backend_bin()`。

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
            guard_core::production_code(&src).contains(
                "register(\n        crate::backend::control::inbound_client::LOCAL_ORIGIN",
            ) || guard_core::production_code(&src).contains(
                "register(\n            crate::backend::control::inbound_client::LOCAL_ORIGIN",
            )
        })
        .collect();
    let want: std::collections::BTreeSet<String> = [
        "backend/control/local_backend.rs::local_stdio_consumer",
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
            "backend/control/local_backend.rs::resolve_or_extract",
            "backend/control/local_backend.rs::start_if_present",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        "又有代码去找 exe 旁那份后端了 —— 本机查询只许问登记在 `<local>` 上的那条活连接（RT1 F2 那一形）"
    );
    // ④ 给终端窗口导 `CCM_BACKEND_BIN` 的那一格（WIN1 报备的同病）：两处调用都交**正在跑的那一份**。
    //    两向：`backend_bin_env_for_window(` 的生产调用点 == 实参是 `running_backend_bin()` 的那几处。
    let launch = guard_core::production_code(include_str!("../../../../src/bridge/src/launch.rs"));
    let calls = launch.matches("backend_bin_env_for_window(").count();
    let running = launch
        .matches("backend_bin_env_for_window(\n        crate::local_backend_host::running_backend_bin(),")
        .count();
    assert!(
        calls >= 1,
        "尺子瞎了：launch.rs 里一处 `backend_bin_env_for_window(` 都没数到"
    );
    assert_eq!(
        calls, running,
        "给终端窗口的后端路径有一处不是「正在跑的那一份」（`running_backend_bin`）"
    );
    // 正控：同一把尺子数得到一处明摆着的调用。
    assert!(loc1a_sites("fn backend_bin_env_for_window(").len() == 1);
}
