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
];

/// 〔SR1a〕拨号那条路从此只放行这两条（题面「`STILL_DIALED` 缩到只剩真该拨号的」—— 点一次换号才发一次）。
/// **异源**：抄自题面与 `STILL_DIALED` 那两行的理由，不从表派生。
const STILL_DIALED_WANT: &[&str] = &["--account-trust", "--account-trust-zero"];

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

/// 后端 `inbound.rs` 生产段里把活交给 `read_face::answer` 的帧命令名（**从后端源码数**）。
fn backend_read_face_commands() -> Vec<String> {
    let got: Vec<String> = backend_command_blocks()
        .into_iter()
        .filter(|(_, blk)| blk.contains("read_face::answer"))
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
    assert_eq!(
        sorted(
            MOVED
                .iter()
                .map(|(_, c)| c.to_string())
                .chain(BORN_ON_FRAME.iter().map(|c| c.to_string()))
        ),
        backend_read_face_commands(),
        "monitor 这边以为搬上去的帧命令，与后端真登记的对不上"
    );
}

/// ★ 拨号那条路只放行 [`STILL_DIALED`]：八条里**一条都过不去**（零命中），登记的那几条过得去（正控）。
#[test]
fn the_dial_path_refuses_every_moved_query() {
    let leaked: Vec<&str> = MOVED
        .iter()
        .map(|(f, _)| *f)
        .filter(|f| dial_allowed(f))
        .collect();
    assert!(
        leaked.is_empty(),
        "这几条已上帧面，拨号那条路却还放行：{leaked:?}"
    );
    for (f, why) in STILL_DIALED {
        assert!(dial_allowed(f), "`{f}` 登记为仍拨号，却被拒了");
        assert!(!why.trim().is_empty(), "`{f}` 没写为什么还在拨");
        assert!(!MOVED.iter().any(|(m, _)| m == f), "`{f}` 同时在两张表里");
    }
    assert!(!dial_allowed("--fork-session"), "没登记的子命令也不许拨");
    assert_eq!(
        sorted(STILL_DIALED.iter().map(|(f, _)| f.to_string())),
        sorted(STILL_DIALED_WANT.iter().map(|s| s.to_string())),
        "仍拨号的那张表不等于题面要的那两条"
    );
}

/// ★ 拨号那条路**真的**先问了 [`dial_allowed`]：`run_list_query` 生产段里有这一问，
/// 且在它的 `connect_and_exec_cmd(` 之前（锚串恰好一处）。
#[test]
fn run_list_query_asks_before_it_dials() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/remote_history.rs");
    let prod = guard_core::production_code(&std::fs::read_to_string(p).unwrap());
    let at = prod
        .find("async fn run_list_query(")
        .expect("找不到 run_list_query");
    let body_end = prod[at + 1..]
        .find("\nasync fn ")
        .map_or(prod.len(), |k| at + 1 + k);
    let body = &prod[at..body_end];
    // 恰好一处、两侧有边界（`find_pinned`）—— 裸 `find`/`matches` 在 needle 被撑大时照样绿。
    let ask = guard_core::find_pinned(body, "dial_allowed(")
        .unwrap_or_else(|e| panic!("run_list_query 没（恰好一次地）问 dial_allowed：{e}"));
    let dial = body
        .find("connect_and_exec_cmd(")
        .expect("run_list_query 里没有拨号 —— 本条的前提变了");
    assert!(ask < dial, "先拨号后问，问了等于没问");
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
         迁过去之后逐行解释只剩 `src/accounts.ts::parseSessionAccountLines` 一处",
    ),
    (
        "history-search",
        "远端那半迁：逐台 fan-out ＋ 补 origin ＋ 与本机索引合并三件事搬到 `src/views/history-search.ts`，\
         每件只有那一个家；本机索引仍是 monitor 进程内的（`search_history` 只剩本机）",
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
];

/// 〔C4b · 第四波 4B〕**帧面只读查询那一族之外**、同样改成「前端经通道直接问、后端出成品」的帧命令 ——
/// `(帧命令, 为什么迁、迁了之后解释住哪)`。它们不在 [`MOVED`] 里（不是 `C1` 那一族），但前端 `chan.call` 的
/// 操作名集合要把它们算进来：下面那条两向判据的「前端那一侧」== [`CHANNELED`] ⊔ 本表。
/// 每一条还要**真的**是后端登记的帧命令（从后端 `inbound.rs` 生产段数，异源）、monitor 生产段里**零**字面量。
const CHANNELED_ELSEWHERE: &[(&str, &str)] = &[(
    "plugins-marketplaces",
    "`feature_face`（RM1b）那一族：后端应答就是整份 survey（成品），monitor 那条命令（`list_plugin_marketplaces`）\
     只在核「恰一行 ＋ 严格形状」—— 核验搬到唯一的消费者 `settings/plugins-section.ts::decodeSurvey`，命令与 `plugins.rs` 删了",
)];

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
const HELD_BACK: &[(&str, &str)] = &[
    (
        "accounts-list",
        "行解析与降级说明挪得进后端，卡在**本机那一侧要并 apikey 表**：并的规则（`history::apikey_routed_subset`）\
         与「这次起会话的是哪一家 agent」住 monitor 起会话那一侧；挪进后端 = 规则搬进 `acct-core`、agent 随请求带过去、\
         本机后端读它自己那份凭据文件（V107：账号层住本机常驻后端）—— 是账号层的一次搬家，不是本拍能顺手做的",
    ),
    (
        "history-projects",
        "每一行要并**本机**的注解（星标 / 隐藏计数，`history-metadata.json` 住 monitor 数据目录）＋ 本机判活（`SessionMap`）\
         ＋ 本机合成的 codex 项目 —— 远端的行要并的是**本机**这一份：跨两台机器的 join，那台的后端出不了成品。\
         出路是注解本身搬进本机常驻后端、由它去问远端那台（设计题，交主会话）",
    ),
    (
        "history-sessions",
        "同 `history-projects`：每一行并本机注解（星标 / 改名 / 隐藏，`remote_session_entry`）＋ 判活 —— 跨机 join",
    ),
    // 〔C4c · 第四波 4B〕下面三行按主会话裁决重写：`history-read` / `history-subagents` **等后端二次拆包**，
    //   `history-tail` 归 CF2（`subscribe`）。三行都仍有 monitor 侧发送点（判据照旧要求它们真有）。
    (
        "history-read",
        "**等后端二次拆包**（主会话裁，4B 第六批）：应答要过记录解析（`parse_line`，ts-rs 类型的来源）与可计行号\
         （`LineNumberer`），两样住 monitor crate；出成品要先把 `messages.rs` / `parser.rs` 搬进两边共用的 crate\
         （`00 §1.5.4` 的 `backend-core`）—— 那一拆之前不迁，不在 TS 再写一份记录解析",
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
    let moved = sorted(MOVED.iter().map(|(_, c)| c.to_string()));
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
    for (op, why) in CHANNELED_ELSEWHERE {
        assert!(!why.trim().is_empty(), "`{op}` 没写理由");
        assert!(registered.contains(*op), "`{op}` 不是后端登记的帧命令");
        assert!(
            !MOVED.iter().any(|(_, c)| c == op),
            "`{op}` 是 `C1` 那一族的，该登记在 `CHANNELED`"
        );
        assert_eq!(
            monitor_literal_count(op),
            0,
            "`{op}` 已迁到通道，monitor 生产段却还有它的字面量（又长出了一个发送点）"
        );
    }
    let mut still_sent_by_monitor: Vec<String> = Vec::new();
    for (op, _) in MOVED.iter().map(|(_, c)| (*c, ())) {
        // `MOVED` 那一行自己就是一次字面量出现。
        if monitor_literal_count(op) > 1 {
            still_sent_by_monitor.push(op.to_string());
        }
    }
    still_sent_by_monitor.sort();
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

/// 〔U4b · 第四波 · G1〕`history-record` 的应答解释：两个字段齐 ⇒ 原样；缺一个 ⇒ `Err`（**绝不**读成「不在」）。
#[test]
fn a_record_reply_missing_a_field_is_an_error_not_absent() {
    use serde_json::json;
    assert_eq!(
        parse_record(
            &Origin("h".into()),
            &json!({"present": false, "root": "/r/projects"})
        )
        .unwrap(),
        RecordProbe {
            present: false,
            root: "/r/projects".into()
        }
    );
    assert_eq!(
        parse_record(
            &Origin("h".into()),
            &json!({"present": true, "root": "/r/projects"})
        )
        .unwrap()
        .present,
        true
    );
    for bad in [
        json!({"root": "/r"}),
        json!({"present": false}),
        json!({"present": "no", "root": "/r"}),
    ] {
        assert!(
            parse_record(&Origin("h".into()), &bad).is_err(),
            "{bad} 不许被读成一个答案"
        );
    }
}
