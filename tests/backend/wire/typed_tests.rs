//! typed 命令的两份生成物由类型现算：形状指纹表（`src/common/cmd-wire/src/shapes.rs`，后端 hello 交出去、客户端拿来比）·
//! 界面那张命令表（`src/frontend/ui/generated/TypedCommands.ts`：命令名 ⇒ 入参 · 回包 · 码）。
//! 跟盘上那份不一样 ⇒ 重写盘上那份、并红（提交新的那一份）。
//! 读登记表（主 crate 的 `stream::inbound::REGISTRY`）⇒ 挂主 crate（`cross/wire.rs`）；形状那一套住 `wire::typed::test_support`。

use crate::stream::inbound::REGISTRY;
use crate::wire::typed::test_support::Shape;
use crate::wire::typed::Codes;

/// 走登记表跑一条命令（同分派那一层：写句柄是测试档那一份；异步档在当场起的运行时上等它）。各族的测试经它问。
pub(crate) fn ask(cmd: &str, args: serde_json::Value) -> crate::wire::spec::Outcome {
    let spec = REGISTRY
        .iter()
        .find(|s| s.name == cmd)
        .unwrap_or_else(|| panic!("`{cmd}` 不在登记表里"));
    let req = crate::wire::Request {
        id: "t".to_string(),
        cmd: cmd.to_string(),
        args,
        ..Default::default()
    };
    let own = crate::platform::fs::Own::for_tests();
    match crate::wire::spec::invoke(spec.run, &crate::stream::inbound::hands(Some(&own))) {
        Some(crate::wire::spec::Prepared::Blocking(f)) => f(req),
        Some(crate::wire::spec::Prepared::Async(f)) => {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("起运行时")
                .block_on(f(req))
        }
        None => panic!("`{cmd}` 是帧面硬臂，不经登记表跑"),
    }
}

/// 登记表里每条 typed 命令与它的形状，按命令名排。
fn typed() -> Vec<(&'static str, Shape)> {
    let mut v: Vec<(&'static str, Shape)> = REGISTRY
        .iter()
        .map(|s| {
            (
                s.name,
                Shape {
                    uncancellable: s.run.blocking(),
                    ..s.run.shape()
                },
            )
        })
        .collect();
    v.sort_by_key(|(n, _)| *n);
    v
}

/// 手机发出去的每条命令的入参（跨语言金样，手机那一侧 `SentRequestsTest` 照它每个拼入参的口写）：逐份照那条命令的入参类型解得开。
/// 金样里的命令都得是登记表里的 typed 命令。
#[test]
fn every_request_the_mobile_sends_decodes_by_that_commands_args_type() {
    let golden: std::collections::BTreeMap<String, Vec<serde_json::Value>> = serde_json::from_str(
        include_str!("../../__fixtures__/mobile-requests.golden.json"),
    )
    .expect("mobile-requests.golden.json 解不开");
    assert!(golden.len() > 20, "金样里没几条命令 —— 本条在空转");
    let rows = typed();
    let mut bad = Vec::new();
    for (cmd, sent) in &golden {
        let Some((_, shape)) = rows.iter().find(|(n, _)| n == cmd) else {
            bad.push(format!("{cmd}：登记表里没有这条 typed 命令"));
            continue;
        };
        for args in sent {
            if let Err(why) = (shape.decode)(args) {
                bad.push(format!("{cmd} {args}：{why}"));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "手机发的入参照后端入参类型解不开：\n{}",
        bad.join("\n")
    );
}

/// 现算的那一份与盘上的比；不一样 ⇒ 写成现算的那一份、红。
fn fresh(rel: &str, want: &str) {
    let path = guard_core::addr::repo_root().join(rel);
    let got = std::fs::read_to_string(&path).unwrap_or_default();
    if got != want {
        std::fs::write(&path, want).unwrap();
        panic!("`{rel}` 跟类型对不上，已按类型重写：提交新的那一份");
    }
}

#[test]
fn the_shapes_table_is_what_the_types_say() {
    let rows = typed();
    assert!(
        !rows.is_empty(),
        "登记表里一条 typed 命令都没有 —— 本条在空转"
    );
    let mut table = String::new();
    let mut stuck = String::new();
    for (name, shape) in &rows {
        table.push_str(&format!("    (\"{name}\", \"{}\"),\n", shape.fingerprint()));
        if shape.uncancellable {
            stuck.push_str(&format!("    \"{name}\",\n"));
        }
    }
    let want = format!(
        "//! 每条 typed 命令的形状指纹：入参 · 回包的 TS 声明（连同它们引到的每个类型）＋ 错误码，合起来的 FNV-1a 32 位，8 位十六进制。\n\
//! 🔴 由后端判据 `wire::typed::tests::the_shapes_table_is_what_the_types_say` 从类型现算写出（不一致就重写这份并红）：别手改。\n\
\n\
/// `(命令, 形状指纹)`，按命令名排。指纹里算进了撤不撤得动（见 [`UNCANCELLABLE`]）。\n\
pub const SHAPES: &[(&str, &str)] = &[\n{table}];\n\
\n\
/// typed 命令里开跑之后撤不动的那几条（阻塞档），按命令名排。客户端撤单时读本侧这一份：指纹对得上才会发，对上了两边必然一致。\n\
pub const UNCANCELLABLE: &[&str] = &[\n{stuck}];\n\
\n\
/// 这条命令在本份编进来的形状指纹；不是 typed 命令 ⇒ `None`。\n\
pub fn shape_of(cmd: &str) -> Option<&'static str> {{\n    SHAPES.iter().find(|(c, _)| *c == cmd).map(|(_, h)| *h)\n}}\n"
    );
    fresh("src/common/cmd-wire/src/shapes.rs", &want);
}

/// 一个 TS 类型名里要 import 的那几个具名类型（`null` · 内联的不用）：本身就是一份声明的；带类型参数的（`SyncPatch<A, B>`）⇒ 打头那个 ＋ 每个参数里具名的。
fn named_in(t: &str, decls: &[(String, String)]) -> Vec<String> {
    t.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|tok| !tok.is_empty())
        .filter(|tok| {
            decls
                .iter()
                .any(|(n, _)| n == tok || n.starts_with(&format!("{tok}<")))
        })
        .map(str::to_string)
        .collect()
}

#[test]
fn the_ui_command_map_is_what_the_types_say() {
    let rows = typed();
    let mut imports = std::collections::BTreeSet::new();
    let mut body = String::new();
    let mut data = String::new();
    for (name, shape) in &rows {
        for t in [&shape.req, &shape.reply] {
            imports.extend(named_in(t, &shape.decls));
        }
        let codes = shape
            .codes
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(" | ");
        body.push_str(&format!(
            "  \"{name}\": {{ req: {}; reply: {}; code: {codes} }};\n",
            shape.req, shape.reply
        ));
        if !shape.data.is_empty() {
            let per: Vec<String> = shape
                .data
                .iter()
                .map(|(code, ty)| {
                    imports.extend(named_in(ty, &shape.decls));
                    format!("\"{code}\": {ty}")
                })
                .collect();
            data.push_str(&format!("  \"{name}\": {{ {} }};\n", per.join("; ")));
        }
    }
    let imports: String = imports
        .iter()
        .map(|t| format!("import type {{ {t} }} from \"./{t}\";\n"))
        .collect();
    let want = format!(
        "// This file was generated by src/backend/wire/typed.rs（判据 `typed::tests::the_ui_command_map_is_what_the_types_say` 从类型现算写出）. Do not edit this file manually.\n\
{imports}\n\
/** typed 帧命令：命令名 ⇒ 入参 · 回包 · 它自己会回的码（`bad_args` 恒在：入参解不出来）。 */\n\
export interface TypedCommands {{\n{body}}}\n\
\n\
/** 拒绝信封里带 `data` 的码 ⇒ 那一份 `data` 的类型（随那条命令的形状指纹核过）；表里没有的码不带 `data`。 */\n\
export interface RefusalData {{\n{data}}}\n"
    );
    fresh("src/frontend/ui/generated/TypedCommands.ts", &want);
}

/// 外层命令那道边界：下层（经资产域那扇门交上来的）回了这一条码表里没有的码 ⇒ 落这一条自己的那一格失败（`fallback`），
/// 那一句照旧，原码与原话进原话（复制详情）；码表里有的照交。
#[test]
fn a_code_the_outer_table_lacks_lands_on_that_commands_own_failure() {
    crate::wire::typed::codes! {
        enum OuterErr {
            IoFailed = "io_failed",
        }
    }
    let kept = crate::wire::typed::of_fail(
        crate::wire::spec::fail("io_failed", "读不了"),
        OuterErr::IoFailed,
    )
    .into_said();
    assert_eq!(
        (kept.code.as_deref(), kept.said.as_str()),
        (Some("io_failed"), "读不了")
    );
    assert_eq!(kept.raw, None);

    let lower =
        crate::wire::spec::fail("not_dir", "那不是一个目录").and_raw(Some("ENOTDIR (os error 20)"));
    let f = crate::wire::typed::of_fail(lower, OuterErr::IoFailed).into_said();
    assert_eq!(
        f.code.as_deref(),
        Some("io_failed"),
        "码表里没有的码不上信封"
    );
    assert_eq!(f.said, "那不是一个目录");
    assert_eq!(
        f.raw.as_deref(),
        Some("not_dir: 那不是一个目录\nENOTDIR (os error 20)"),
        "原码与原话进原话"
    );
}

/// 契约错拒绝体不带出调用方给的原值（特别是哨兵串 / 秘密）。
#[test]
fn take_bad_args_omits_sentinel_value_from_rejection() {
    #[derive(serde::Deserialize, Debug)]
    struct DummyReq {
        #[expect(dead_code, reason = "只为反序列化那一下按类型拒，值没人读")]
        port: u16,
    }
    let sentinel = "SENTINEL_SECRET_VALUE_12345";
    let args = serde_json::json!({ "port": sentinel });
    let fail = crate::wire::typed::take::<DummyReq>(&args).expect_err("should fail");
    let rejection = format!(
        "code={} message={} raw={:?}",
        fail.code_or_fallback(),
        fail.said,
        fail.raw
    );
    assert!(
        !rejection.contains(sentinel),
        "拒绝体中泄露了哨兵串：{rejection}"
    );
}

/// 读者那几处（相对仓根）：界面 TS · 手机 Kotlin · 壳 / 文件窗口 / 手机核心 / 端核心的 Rust · 给 AI 读的说明与脚本。
/// 不算：生成物（`generated/` · `gen/`）与测试。
const READERS: &[(&str, &[&str])] = &[
    ("src/frontend/ui", &["ts", "tsx"]),
    ("src/mobile", &["kt"]),
    ("src/frontend/shell", &["rs"]),
    ("src/frontend/filewin", &["rs"]),
    ("src/mobile/core-rs", &["rs"]),
    ("src/common/face-core", &["rs"]),
    ("src/common/batch-core", &["rs"]),
    ("src/common/chunk-core", &["rs"]),
    ("src/common/pick-core", &["rs"]),
];
/// 给 AI 读的那一处：任何位置出现都算（说明里点名一格就是在叫 AI 读它）。
const PROSE: &str = "src/shared";
const SKIP_DIRS: &[&str] = &[
    "generated",
    "gen",
    "test",
    "tests",
    "androidTest",
    "testing",
    "node_modules",
    "build",
    "target",
    ".gradle",
];

/// 读者不在上面那几处的格：`(类型, 格, 谁读)`。只许三种读者：对端后端（后端问后端那几条命令的回包 / 入参）·
/// 原样往返（客户端拿到原样交回后端）· 拨号层（`src/comms/ssh-dial`）。
const ELSEWHERE: &[(&str, &str, &str)] = &[
    ("CcBusInstalled", "recordFailed", "对端后端：`cc-bus-install` 问的那台（assets/hub.rs）"),
    ("SkillApplied", "chmodFailed", "对端后端：`skill-install-apply` 问的那台（assets/hub.rs）"),
    ("SkillApplied", "recordFailed", "对端后端：`skill-install-apply` 问的那台（assets/hub.rs）"),
    ("McpApplied", "recordFailed", "对端后端：`mcp-sync-apply` 问的那台（assets/hub.rs）"),
    ("HistoryListingHead", "annotations", "对端后端：`history-listing` 问的那台照它出行（history/history_list.rs）"),
    ("HistoryListingHead", "annotationsNotice", "对端后端：`history-listing` 问的那台（history/history_list.rs）"),
    ("HistoryRawRow", "launchTab", "对端后端：`history-listing` 问的那台出启动 tab 那一格（history/history_list.rs）"),
    ("Entry", "resumeWith", "对端后端：`history-listing` 带的注解表，问的那台读（history/history_annotations.rs）"),
    ("CatalogMachine", "projectDirs", "对端后端：资产目录两台互拉互推（assets/asset_catalog.rs）"),
    ("CatalogMachine", "seenAt", "对端后端：资产目录两台互拉互推（assets/asset_catalog.rs）"),
    ("CatalogMachine", "sharedMcp", "对端后端：资产目录两台互拉互推（assets/asset_catalog.rs）"),
    ("AssetsCatalogMergeReq", "catalog", "对端后端：`assets-catalog-merge` 由本机后端推给那台（assets/asset_sync.rs）"),
    ("SshConnection", "clientAddr", "原样往返：`session-terminals` 的终端原样交回 `terminal-processes`（dial/terminal_processes.rs）"),
    ("SshConnection", "clientPort", "原样往返：`session-terminals` 的终端原样交回 `terminal-processes`（dial/terminal_processes.rs）"),
    ("SshConnection", "serverAddr", "原样往返：`session-terminals` 的终端原样交回 `terminal-processes`（dial/terminal_processes.rs）"),
    ("SshConnection", "serverPort", "原样往返：`session-terminals` 的终端原样交回 `terminal-processes`（dial/terminal_processes.rs）"),
    ("DialRequest", "agent_sock", "拨号层：ssh-dial 认 ssh-agent 套接字（comms/ssh-dial/connect.rs）"),
    ("DialRequest", "forward", "拨号层：ssh-dial 开转发（comms/ssh-dial/uses.rs）"),
    ("CaptureOpts", "abort_marker", "拨号层：ssh-dial 见到就收工（comms/ssh-dial/uses.rs）"),
    ("DeleteExpect", "empty_dir", "后端自己交：common/door.rs 删空目录走同一条入参，没有客户端交它"),
];

/// 没有读者、还没裁的格：`(类型, 格, 为什么裁不了)`。裁一格删一条；两向相等。
const UNCUT: &[(&str, &str, &str)] = &[
    ("ChoresMarks", "startSkipped", "盘上那一份原样作 `chores-mark` 回包：回包要另成一形"),
    ("QuotaWindow", "warnedAt", "额度账盘上那一形原样作 `quota-probe` 回包：回包要另成一形"),
    ("RotSawAcct", "used5h", "轮换记录本盘上那一形原样上线：回包要另成一形"),
    ("RotSawAcct", "resets5h", "轮换记录本盘上那一形原样上线：回包要另成一形"),
    ("RotSawAcct", "rate5h", "轮换记录本盘上那一形原样上线：回包要另成一形"),
    ("BrowseReply", "watch_failed", "该删；守它的 tests/backend/files/browse_watch_tests.rs 要连带改（另排）"),
    ("BrowseReply", "watch_error", "该删；守它的 tests/backend/files/browse_watch_tests.rs 要连带改（另排）"),
    ("ScannedAccount", "authReady", "后端自己用、该不出帧；界面那几份账号判据（accounts · account-chip · account-color · account-commands · judgment-single-home · account-availability-guard）的夹具与 eslint 守则都写着它，要连带改（另排）"),
    ("AccountRow", "authReady", "同 `ScannedAccount.authReady`"),
    ("AccountChange", "applied", "后端自己用、该不出帧；账号那几族判据（accounts_face · mcp_share_exec · trust_share_exec）经回包读它看行为，要先改成直接测本体"),
    ("VerifyReport", "checks", "同 `AccountChange.applied`：核对那几条判据经回包读逐项"),
    ("VerifyReport", "fails", "同 `VerifyReport.checks`"),
    ("DialAck", "winner", "写它的是 src/comms/ssh-dial，要连带改（另排）"),
    ("TerminalPreviewReq", "scrollback", "终端往回看整条没有调用方，牵到托管终端那一侧（host/managed 要连带改（另排））"),
    ("Preview", "capped", "同 `TerminalPreviewReq.scrollback`：往回看那一截的回包"),
    ("Preview", "scrollback_lines", "同 `TerminalPreviewReq.scrollback`：往回看那一截的回包"),
    ("SessionFacts", "forkedFrom", "两端只摆后端起好的 `forkedLine`，不读父会话号；后端起名要用它，该 `serde(skip)` 或删（后端那一路，另排）"),
    ("FactsReply", "forkedFrom", "同 `SessionFacts.forkedFrom`"),
    ("HistoryRow", "forkedFrom", "同 `SessionFacts.forkedFrom`：历史行只摆整行标题里的分叉记号"),
    ("MergedRow", "forkedFrom", "同 `HistoryRow.forkedFrom`"),
    ("SearchRow", "forkedFrom", "同 `HistoryRow.forkedFrom`"),
    ("SessionHits", "forkedFrom", "同 `HistoryRow.forkedFrom`"),
];

/// 去掉注释：`//` 到行尾 · `/* … */`；字符串原样留着（注释的记号在字符串里不算）。
fn strip_comments(s: &str) -> String {
    let b: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == '"' || c == '`' {
            let end = string_end(&b, i).unwrap_or(i);
            out.extend(&b[i..=end]);
            i = end + 1;
        } else if c == '/' && b.get(i + 1) == Some(&'/') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && b.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// 从 `at` 那个引号起，同一种引号收尾的位置（`"` `'` 不跨行，`` ` `` 跨行）；收不了尾 ⇒ `None`。
fn string_end(b: &[char], at: usize) -> Option<usize> {
    let q = b[at];
    let mut i = at + 1;
    while i < b.len() {
        match b[i] {
            '\\' => i += 1,
            '\n' if q != '`' => return None,
            c if c == q => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// 一份读者源码里以取格的写法出现的名字：`.名` · `::名` · `{名,` · `,名}` · `{名:` · `,名:` · `(名:` · `(名=` · `,名=` · 整串 `"名"`。
fn cell_reads(src: &str) -> std::collections::BTreeSet<String> {
    let b: Vec<char> = strip_comments(src).chars().collect();
    let mut code: Vec<char> = Vec::with_capacity(b.len());
    let mut got = std::collections::BTreeSet::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == '"' || c == '\'' {
            if let Some(end) = string_end(&b, i) {
                let inner: String = b[i + 1..end].iter().collect();
                if !inner.is_empty() && inner.chars().all(is_word) {
                    got.insert(inner);
                }
                code.extend(std::iter::repeat_n(' ', end + 1 - i));
                i = end + 1;
                continue;
            }
        }
        code.push(c);
        i += 1;
    }
    let prev = |at: usize| {
        code[..at]
            .iter()
            .rev()
            .find(|c| !c.is_whitespace())
            .copied()
    };
    let prev2 = |at: usize| {
        let mut it = code[..at].iter().rev().filter(|c| !c.is_whitespace());
        (it.next().copied(), it.next().copied())
    };
    let mut i = 0;
    while i < code.len() {
        if !is_word(code[i]) || (i > 0 && is_word(code[i - 1])) {
            i += 1;
            continue;
        }
        let start = i;
        while i < code.len() && is_word(code[i]) {
            i += 1;
        }
        let name: String = code[start..i].iter().collect();
        let rest: String = code[i..code.len().min(i + 8)]
            .iter()
            .filter(|c| !c.is_whitespace())
            .collect();
        let before = prev(start);
        let member = before == Some('.') || prev2(start) == (Some(':'), Some(':'));
        let keyed = matches!(before, Some('{' | ',' | '('))
            && (rest.starts_with(':') && !rest.starts_with("::") || rest.starts_with("?:"));
        let short =
            matches!(before, Some('{' | ',')) && (rest.starts_with(',') || rest.starts_with('}'));
        let named_arg = matches!(before, Some('(' | ','))
            && rest.starts_with('=')
            && !rest.starts_with("==")
            && !rest.starts_with("=>");
        if member || keyed || short || named_arg {
            got.insert(name);
        }
    }
    got
}

/// `dir` 底下扩展名在 `exts` 里的那几份（`exts` 为空 ⇒ 没有扩展名的那几份）的正文；生成物 · 测试不算。
fn walk(dir: &std::path::Path, exts: &[&str]) -> Vec<String> {
    guard_core::scan_tree!(dir, exts)
        .into_iter()
        .filter(|(p, _)| {
            let rel = p.strip_prefix(dir).unwrap_or(p);
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let wanted = if exts.is_empty() {
                !name.contains('.')
            } else {
                exts.iter().any(|x| name.ends_with(&format!(".{x}")))
            };
            let parts: Vec<&str> = name.split('.').collect();
            let test_file = parts.contains(&"test")
                || parts.first().and_then(|s| s.rsplit('_').next()) == Some("tests");
            wanted
                && !test_file
                && !rel
                    .components()
                    .any(|c| SKIP_DIRS.contains(&c.as_os_str().to_string_lossy().as_ref()))
        })
        .map(|(_, src)| src)
        .collect()
}

/// 生成的 TS 声明里一个类型的各格（对象键，连内联的）：`名:` · `名?:` · `"名":`。
fn cells_of_decl(src: &str) -> std::collections::BTreeSet<String> {
    let b: Vec<char> = strip_comments(src).chars().collect();
    let mut got = std::collections::BTreeSet::new();
    let mut i = 0;
    while i < b.len() {
        let (name, end) = if b[i] == '"' {
            match string_end(&b, i) {
                Some(e) => (b[i + 1..e].iter().collect::<String>(), e + 1),
                None => {
                    i += 1;
                    continue;
                }
            }
        } else if is_word(b[i]) && (i == 0 || !is_word(b[i - 1])) {
            let mut e = i;
            while e < b.len() && is_word(b[e]) {
                e += 1;
            }
            (b[i..e].iter().collect::<String>(), e)
        } else {
            i += 1;
            continue;
        };
        let mut j = end;
        if b.get(j) == Some(&'?') {
            j += 1;
        }
        if b.get(j) == Some(&':') && b.get(j + 1) != Some(&':') {
            got.insert(name);
        }
        i = end;
    }
    got
}

fn snake(n: &str) -> String {
    let mut s = String::new();
    for (k, c) in n.chars().enumerate() {
        if c.is_uppercase() && k > 0 {
            s.push('_');
        }
        s.extend(c.to_lowercase());
    }
    s
}

fn camel(n: &str) -> String {
    let mut s = String::new();
    let mut up = false;
    for c in n.chars() {
        if c == '_' {
            up = true;
        } else if up {
            s.extend(c.to_uppercase());
            up = false;
        } else {
            s.push(c);
        }
    }
    s
}

/// 线上每一格都有读者：生成的 TS 声明（`src/frontend/ui/generated/`，与线上 `serde_json` 同一套属性）里每个类型的每一格，
/// 在读者那几处（[`READERS`] · [`PROSE`]）以取格的写法出现过（名字按原样 · snake · camel 找），不然就得登在 [`ELSEWHERE`]（写谁读），
/// 或登在 [`UNCUT`]（没有读者、还裁不了，写为什么、要连带改什么）。后端自己还要用、两端不读的格留在类型里、不出帧（`serde(skip)`）。
/// 两向相等：表里的那一格得真在线上、真找不到读者。
/// 判法宽的一侧：同名的别的格也算读者 —— 零命中的一定没读者，有命中的不担保。
#[test]
fn cells_no_reader_reads_are_off_the_wire() {
    let root = guard_core::addr::repo_root();
    let mut reads = std::collections::BTreeSet::new();
    for (dir, exts) in READERS {
        let files = walk(&root.join(dir), exts);
        assert!(
            !files.is_empty(),
            "`{dir}` 底下一份读者都没有 —— 本条在空转"
        );
        for s in files {
            reads.extend(cell_reads(&s));
        }
    }
    let mut prose = walk(&root.join(PROSE), &["md", "rhai", "sh", "tpl"]);
    prose.extend(walk(&root.join(PROSE), &[]));
    assert!(
        !prose.is_empty(),
        "`{PROSE}` 底下一份说明都没有 —— 本条在空转"
    );
    for s in prose {
        reads.extend(
            s.split(|c: char| !is_word(c))
                .filter(|w| !w.is_empty())
                .map(str::to_string),
        );
    }
    let gen = root.join("src/frontend/ui/generated");
    let mut unread = std::collections::BTreeSet::new();
    let mut cells = 0;
    for (path, src) in guard_core::scan_tree!(&gen, &["ts"]) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let Some(ty) = name.strip_suffix(".ts") else {
            continue;
        };
        if ty == "TypedCommands" {
            continue;
        }
        for cell in cells_of_decl(&src) {
            cells += 1;
            if ![cell.clone(), snake(&cell), camel(&cell)]
                .iter()
                .any(|n| reads.contains(n))
            {
                unread.insert(format!("{ty}.{cell}"));
            }
        }
    }
    assert!(cells > 1000, "generated/ 里只认出 {cells} 格 —— 本条在空转");
    let listed: std::collections::BTreeSet<String> = ELSEWHERE
        .iter()
        .chain(UNCUT)
        .map(|(t, c, _)| format!("{t}.{c}"))
        .collect();
    let off: Vec<&String> = unread.difference(&listed).collect();
    let stale: Vec<&String> = listed.difference(&unread).collect();
    assert!(
        off.is_empty() && stale.is_empty(),
        "线上没有读者的格（删，或后端自己用的 `serde(skip)`；读者真在别处 ⇒ 登 ELSEWHERE 写谁读）：{off:?}\n\
         ELSEWHERE · UNCUT 里已不在线上、或在读者那几处读得到的：{stale:?}"
    );
}

/// 判法自己的正控：取格的写法认得出，注释 · 带别的字的字符串 · 声明里的名字不算。
#[test]
fn a_cell_read_is_told_from_a_mention() {
    let got = cell_reads(
        "const reading = t(\"kit.state.reading\"); // x.gone\n\
         a.seen; f(b?.deep); { short, other }; g({ key: 1 }); h(named = 2); m[\"quoted\"];",
    );
    let want: std::collections::BTreeSet<String> =
        ["seen", "deep", "short", "other", "key", "named", "quoted"]
            .iter()
            .map(|s| s.to_string())
            .collect();
    assert_eq!(got, want);
    let decl =
        cells_of_decl("export type X = { a: string, \"b\"?: number, c: { d: Y }, e: A::B };");
    assert_eq!(
        decl,
        ["a", "b", "c", "d", "e"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
}

/// 失败只一种值（`Said`）：经 `spec::fail` 带码，投出去那一下码缺 ⇒ 兜底码。
#[test]
fn a_failure_settles_with_its_code_or_the_fallback() {
    use crate::common::said::Said;
    use crate::wire::spec::Settle as _;
    assert_eq!(
        crate::wire::spec::fail("bad_args", "x".to_string())
            .settle("cmd")
            .code,
        "bad_args"
    );
    assert_eq!(Said::from("y").settle("cmd").code, "failed");
}
