use super::*;
use crate::origin::Origin;

#[test]
fn each_machine_sees_exactly_its_own_tokens() {
    let mut book = Book::new();
    let devbox = Origin("devbox".into());
    record_in_book(&mut book, &devbox, "capabilities:x");
    record_in_book(&mut book, &devbox, "capabilities:x");
    record_in_book(&mut book, &devbox, "capabilities:a");
    record_in_book(&mut book, &Origin::local(), "capabilities:l");
    assert_eq!(
        snapshot_in_book(&book, &devbox),
        vec!["capabilities:a".to_string(), "capabilities:x".to_string()],
        "那台那一份不是恰好它见过的那几个（去重、升序）"
    );
    assert_eq!(
        snapshot_in_book(&book, &Origin::local()),
        vec!["capabilities:l".to_string()]
    );
    assert!(
        snapshot_in_book(&book, &Origin("laptop".into())).is_empty(),
        "没记过的那台不是空"
    );
}

/// 有界：每台触顶后新 token 并进 `<overflow>`；一台溢出不碰另一台。
#[test]
fn the_token_set_is_bounded_per_machine() {
    let mut book = Book::new();
    let devbox = Origin("devbox".into());
    for i in 0..(MAX_TOKENS + 30) {
        record_in_book(&mut book, &devbox, &format!("t{i}"));
    }
    let got = snapshot_in_book(&book, &devbox);
    assert_eq!(got.len(), MAX_TOKENS + 1, "应当是上限 ＋ 一个 <overflow>");
    assert!(got.iter().any(|t| t == OVERFLOW_KEY));
    record_in_book(&mut book, &Origin::local(), "fresh");
    assert_eq!(
        snapshot_in_book(&book, &Origin::local()),
        vec!["fresh".to_string()]
    );
}

/// 读口：答的是所问那台、回包带回那台；空白名（「没说」）拒收，不许被当成某一台。
#[test]
fn the_read_side_answers_the_asked_machine_and_echoes_it() {
    let probe = Origin("st3-read-probe".into());
    record(&probe, "st3-read-key");
    let r = tauri::async_runtime::block_on(drift_ledger_report(probe.clone()))
        .expect("读口拒了一台正常的机器");
    assert_eq!(r.origin, probe, "回包没带回所问那台");
    assert!(r.unknown_tokens.iter().any(|t| t == "st3-read-key"));
    let other =
        tauri::async_runtime::block_on(drift_ledger_report(Origin("st3-read-other".into())))
            .unwrap();
    assert!(
        other.unknown_tokens.is_empty(),
        "问另一台却答出了探针那台的账"
    );
    let err = tauri::async_runtime::block_on(drift_ledger_report(Origin("  ".into())))
        .expect_err("空白名被当成了某一台");
    assert!(
        err.contains("drift_ledger_report"),
        "拒收的话没点名是哪条命令：{err}"
    );
}

// ── J3：喂账调用点登记表 ──────────────────────────────────────────────
//
// 🔴 **它的人群**：`src/frontend/shell/src` 生产段里，调了「喂账入口」的函数（`文件, 外层 fn`）。
// 喂账入口 ＝ 直接写账的 `drift_ledger::record` ＋ 把 `origin` 一路交给它的那一个（`note_unknown_capabilities`）。
// 记录解析那几条（`parse_line` / `parse_for_kind`〔散文墓碑〕 / `batch_to_payloads` / `range_payloads`）出列：解析进了后端，账也跟着记在那台后端。
// 判法两条，都不是地板：
//   ① 人群 == `FEEDERS` 的键（两向）：新长一个喂账点 ⇒ 红，必须来这里说清它记在哪台名下；
//      删了一个 ⇒ 死条目 ⇒ 红。
//   ② `Local` 那几行体里**有** `Origin::local()`；`Given` 那几行体里 `Origin::local()` **零命中** ——
//      拦的是「远端那条路上写死本机」（改签名之后「没说」写不出来，剩下的就是这一形）。
// ⚠ 同波别的路新调一处 `record` ⇒ 合并那一拍 ① 红：按它记在哪台补一行。

/// 记在哪台名下、凭什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Whose {
    /// 写死本机：它只碰本机的东西（理由写在 `FEEDERS` 那一行）。
    Local,
    /// 随调用方给的 origin（参数 / `route` 过的 / 那台的配置名），体里不许出现写死的本机。
    Given,
}

/// `(文件, 外层 fn, 记在哪台, 理由)`。
const FEEDERS: &[(&str, &str, Whose, &str)] = &[
    // `("history.rs", "analyze_jsonl")` 那一行摘了：本机历史清单搬进本机常驻后端（会话行由后端摘要，
    //   与远端同一个函数），monitor 不再为了列清单逐行解析本机 jsonl。
    // 本机远端的冷读合成一条：记账那一跳挪进 `SessionPager::page`（`history.rs`），
    //   `stream_read_session_jsonl` 本身不再解析；远端那一支 `stream_read_remote_session`〔散文墓碑〕那一行随它删了。
    // `lib.rs::run`（`Local`，「本机 jsonl watcher 那一批」）那一行摘了：
    //   本机会话的行从此是本机后端的 `line` 帧，经 `stream_source/batch·rs::flush_lines`（`Given`，origin 是本机）进账。
    // `("search.rs", "build_one", Local)` 那一行摘了：本机搜索改问本机后端，monitor 内存索引删了。
    // 「未登记的会话 kind」那一笔进了那台后端（适配层判后台会话时记），本机那条流上的那一行摘了。
    (
        "stream_source/version.rs",
        "note_unknown_capabilities",
        Whose::Given,
        "参数 `origin`",
    ),
    (
        "stream_source/run.rs",
        "on_hello",
        Whose::Given,
        "hello 那一段：那台的 `host_label`",
    ),
];

/// 喂账入口（调用形）。
const FEED_ENTRIES: &[&str] = &[
    "drift_ledger::record(",
    // `parse_line(` · `parse_for_kind(` · `batch_to_payloads(` · `range_payloads(` 出列：记录解析进了后端，那几条路不再喂这本账。
    "note_unknown_capabilities(",
];

/// 与签名那一行同缩进的收尾 `}` 在哪一行（rustfmt 保证）。
fn fn_end(lines: &[&str], start: usize) -> usize {
    let indent: String = lines[start].chars().take_while(|c| *c == ' ').collect();
    let close = format!("{indent}}}");
    (start..lines.len())
        .find(|&k| lines[k] == close)
        .unwrap_or(lines.len() - 1)
}

/// 某一行**所在**的函数：往回找 `fn <名>`，且它的体要把这一行包住
/// （外层 fn 里先定义过一个嵌套 `fn drop` 之类、体已收尾的，不算 —— `stream_source/run·rs::stream_loop` 现打过）。
fn enclosing_fn_of(lines: &[&str], at: usize) -> Option<(String, usize)> {
    for i in (0..=at).rev() {
        let l = lines[i];
        let rest = l
            .split(" fn ")
            .nth(1)
            .or_else(|| l.trim_start().strip_prefix("fn "));
        if let Some(rest) = rest {
            let n: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !n.is_empty() && fn_end(lines, i) >= at {
                return Some((n, i));
            }
        }
    }
    None
}

/// 函数体：从签名那一行起，到与它同缩进的 `}` 那一行。
fn fn_body(lines: &[&str], start: usize) -> String {
    lines[start..=fn_end(lines, start)].join("\n")
}

/// 从一份生产段里摘 (外层 fn, 体)：调了喂账入口的那些（定义行与更长名字里的子串不算）。
fn feeders_in(prod: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = prod.lines().collect();
    let mut out: Vec<(String, String)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let hit = FEED_ENTRIES.iter().any(|needle| {
            l.match_indices(needle).any(|(k, _)| {
                let before = &l[..k];
                let prev = before.chars().next_back();
                !before.ends_with("fn ") && !prev.is_some_and(|c| c.is_alphanumeric() || c == '_')
            })
        });
        if !hit {
            continue;
        }
        let (name, at) = enclosing_fn_of(&lines, i).expect("喂账调用不在任何 fn 里 —— 抽取坏了");
        if !out.iter().any(|(n, _)| *n == name) {
            out.push((name, fn_body(&lines, at)));
        }
    }
    out
}

#[test]
fn every_ledger_feeder_is_registered_with_whose_book_it_writes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found: std::collections::BTreeMap<(String, String), String> = Default::default();
    for (path, raw) in guard_core::scan_tree!(&root, &["rs"]) {
        let prod = guard_core::production_code(&raw);
        // 按模块住址认：人群声明带进来的兄弟包（通信层 `comms-inward` 等）认作 `<包名>/…`。
        let file = guard_core::module_address(&root, &path);
        for (name, body) in feeders_in(&prod) {
            found.insert((file.clone(), name), body);
        }
    }
    let got: std::collections::BTreeSet<(String, String)> = found.keys().cloned().collect();
    let want: std::collections::BTreeSet<(String, String)> = FEEDERS
        .iter()
        .map(|(f, n, _, _)| (f.to_string(), n.to_string()))
        .collect();
    assert_eq!(
        got, want,
        "**喂漂移账的调用点与 `FEEDERS` 两向对不上。**\n\
         · 只在左边 ＝ 新长了一个喂账点：说清它记在哪台名下（`Local` 写死本机 / `Given` 随调用方），补一行；\n\
         · 只在右边 ＝ 那个点没了（或改名）：删掉那一行。"
    );
    let local_ctor = "Origin::local()";
    for (f, n, whose, why) in FEEDERS {
        let body = &found[&(f.to_string(), n.to_string())];
        match whose {
            Whose::Local => assert!(
                body.contains(local_ctor),
                "`{f}::{n}` 登记为写死本机（{why}），体里却没有 `{local_ctor}` —— 登记过期了"
            ),
            Whose::Given => assert!(
                !body.contains(local_ctor),
                "`{f}::{n}` 登记为随调用方的 origin（{why}），体里却写死了 `{local_ctor}` —— \
                 远端那条路上的行会被记成本机的"
            ),
        }
    }
}

/// 阳性对照：抽取器认得出「新喂账点」「写死本机」「定义行不算」「更长名字里的子串不算」。
#[test]
fn the_feeder_scanner_sees_what_it_claims_to_see() {
    let src = "fn a(o: &Origin) {\n    note_unknown_capabilities(o, x);\n}\n\
               fn b() {\n    crate::drift_ledger::record(&Origin::local(), k);\n}\n\
               pub fn note_unknown_capabilities(o: &Origin) {\n    todo!()\n}\n\
               fn c() {\n    let _ = my_note_unknown_capabilities(x);\n}\n";
    let got = feeders_in(src);
    let names: Vec<&str> = got.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["a", "b"], "定义行或子串被当成了调用：{names:?}");
    assert!(
        got[1].1.contains("Origin::local()") && !got[0].1.contains("Origin::local()"),
        "体切错了"
    );
}
