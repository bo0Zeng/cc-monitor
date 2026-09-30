use super::*;

/// **一律用局部账本**（见 `record_into` 头注）—— 全局账本会被任何走过喂账点
/// 的测试污染，在它上面断言整表形状必然 flaky（实测 6 次全量跑红 4 次）。
fn fresh() -> Ledger {
    Ledger::new()
}

#[test]
fn counts_and_keeps_only_the_first_sample() {
    let mut led = fresh();
    record_into(
        &mut led,
        DriftFace::UnknownBackendToken,
        "mode",
        Some("{\"type\":\"mode\",\"a\":1}"),
    );
    record_into(
        &mut led,
        DriftFace::UnknownBackendToken,
        "mode",
        Some("{\"type\":\"mode\",\"b\":2}"),
    );
    record_into(&mut led, DriftFace::UnknownBackendToken, "pr-link", None);
    let snap = snapshot_of(&led);
    assert_eq!(snap.len(), 1);
    let f = &snap[0];
    assert_eq!(f.face, DriftFace::UnknownBackendToken);
    assert!(!f.overflowed);
    assert_eq!(f.entries[0].key, "mode"); // count 降序
    assert_eq!(f.entries[0].count, 2);
    assert_eq!(
        f.entries[0].first_sample.as_deref(),
        Some("{\"type\":\"mode\",\"a\":1}"),
        "**首见**样例应当被留住，后来的不许覆盖"
    );
    assert_eq!(f.entries[1].key, "pr-link");
    assert!(f.entries[1].first_sample.is_none());
}

/// ★ **有界**：键数触顶后新键并进 `<overflow>`，内存不随 CC 的想象力增长。
#[test]
fn the_key_count_is_bounded() {
    let mut led = fresh();
    for i in 0..(MAX_KEYS + 30) {
        record_into(
            &mut led,
            DriftFace::UnknownBackendToken,
            &format!("t{i}"),
            None,
        );
    }
    let f = snapshot_of(&led).remove(0);
    assert!(f.overflowed, "溢出了却没标记");
    assert_eq!(
        f.entries.len(),
        MAX_KEYS + 1,
        "键数应当是上限 + 一个 <overflow>，实得 {}",
        f.entries.len()
    );
    let ov = f
        .entries
        .iter()
        .find(|e| e.key == OVERFLOW_KEY)
        .expect("有溢出键");
    assert_eq!(ov.count, 30, "溢出的 30 个应当全部并进 <overflow>");
    // 已经在表里的老键仍然正常累加（溢出不影响它们）。
    record_into(&mut led, DriftFace::UnknownBackendToken, "t0", None);
    let f2 = snapshot_of(&led).remove(0);
    assert_eq!(f2.entries.iter().find(|e| e.key == "t0").unwrap().count, 2);
}

/// 样例按**字符边界**截断 —— 多字节字符不许被切成半个（那会让诊断面显示成乱码）。
#[test]
fn samples_are_truncated_on_a_char_boundary() {
    let mut led = fresh();
    let long = "中".repeat(MAX_SAMPLE_BYTES);
    record_into(&mut led, DriftFace::UnknownSessionKind, "user", Some(&long));
    let s = snapshot_of(&led)[0].entries[0]
        .first_sample
        .clone()
        .expect("有样例");
    assert!(s.len() <= MAX_SAMPLE_BYTES + 4, "没截断：{}", s.len());
    assert!(s.ends_with('…'), "截断标记没了");
    assert!(
        s.trim_end_matches('…').chars().all(|c| c == '中'),
        "切出了半个字符 —— 诊断面会显示成乱码"
    );
}

/// 空键不许变成空行。
#[test]
fn an_empty_key_is_labelled() {
    let mut led = fresh();
    record_into(&mut led, DriftFace::UnknownSessionKind, "", None);
    assert_eq!(snapshot_of(&led)[0].entries[0].key, "<empty>");
}

/// 全局那两个入口只是纯函数的薄壳 —— 至少走一次，别让它们成为未覆盖的分叉。
#[test]
fn the_global_entry_points_delegate_to_the_pure_ones() {
    // 用一个**本测试专属**的键与机器名：全局账本会被别的测试写，只断言「我这条在」。
    let key = "u-cc1-delegation-probe";
    let probe = Origin("st3-delegation-probe".into());
    record(&probe, DriftFace::UnknownBackendToken, key, Some("s"));
    let found = snapshot(&probe)
        .into_iter()
        .find(|f| f.face == DriftFace::UnknownBackendToken)
        .and_then(|f| f.entries.into_iter().find(|e| e.key == key));
    let e = found.expect("全局 record/snapshot 没接上纯函数");
    assert!(e.count >= 1);
    assert_eq!(e.first_sample.as_deref(), Some("s"));
}

// ── 〔ST3〕按机器分 ─────────────────────────────────────────────────────────

use crate::origin::Origin;

fn key_set(v: &[DriftFaceReport]) -> std::collections::BTreeSet<(DriftFace, String)> {
    v.iter()
        .flat_map(|f| f.entries.iter().map(move |e| (f.face, e.key.clone())))
        .collect()
}

/// ★ J1：两台各记一笔 ⇒ 各自的快照**恰好**是自己那一笔（两向集合相等），没记过的那台 ⇒ 空。
#[test]
fn each_machine_sees_exactly_its_own_book() {
    let mut book = Book::new();
    let local = Origin::local();
    let devbox = Origin("devbox".into());
    record_in_book(
        &mut book,
        &local,
        DriftFace::UnknownSessionKind,
        "workflow",
        None,
    );
    record_in_book(
        &mut book,
        &devbox,
        DriftFace::UnknownBackendToken,
        "mode",
        Some("{}"),
    );
    record_in_book(
        &mut book,
        &devbox,
        DriftFace::UnknownBackendToken,
        "capabilities:x",
        None,
    );
    let want = |xs: &[(DriftFace, &str)]| -> std::collections::BTreeSet<(DriftFace, String)> {
        xs.iter().map(|(f, k)| (*f, k.to_string())).collect()
    };
    assert_eq!(
        key_set(&snapshot_in_book(&book, &local)),
        want(&[(DriftFace::UnknownSessionKind, "workflow")]),
        "本机那一本不是恰好本机那一笔"
    );
    assert_eq!(
        key_set(&snapshot_in_book(&book, &devbox)),
        want(&[
            (DriftFace::UnknownBackendToken, "mode"),
            (DriftFace::UnknownBackendToken, "capabilities:x"),
        ]),
        "devbox 那一本不是恰好 devbox 那两笔"
    );
    assert!(
        snapshot_in_book(&book, &Origin("laptop".into())).is_empty(),
        "没记过的那台答出了东西 —— 拿别台的账冒充它"
    );
}

/// 同一个键在两台上**各数各的**（计数与首见样例都不串台）。
#[test]
fn the_same_key_counts_separately_per_machine() {
    let mut book = Book::new();
    let local = Origin::local();
    let devbox = Origin("devbox".into());
    record_in_book(
        &mut book,
        &devbox,
        DriftFace::UnknownBackendToken,
        "mode",
        Some("devbox-1"),
    );
    record_in_book(
        &mut book,
        &devbox,
        DriftFace::UnknownBackendToken,
        "mode",
        Some("devbox-2"),
    );
    record_in_book(
        &mut book,
        &local,
        DriftFace::UnknownBackendToken,
        "mode",
        Some("local-1"),
    );
    let first = |o: &Origin| snapshot_in_book(&book, o)[0].entries[0].clone();
    assert_eq!(
        (first(&devbox).count, first(&devbox).first_sample.as_deref()),
        (2, Some("devbox-1"))
    );
    assert_eq!(
        (first(&local).count, first(&local).first_sample.as_deref()),
        (1, Some("local-1"))
    );
}

/// 有界是**每台每面**：一台触顶不挤占另一台。
#[test]
fn one_machine_overflowing_does_not_touch_another() {
    let mut book = Book::new();
    let devbox = Origin("devbox".into());
    for i in 0..(MAX_KEYS + 3) {
        record_in_book(
            &mut book,
            &devbox,
            DriftFace::UnknownBackendToken,
            &format!("t{i}"),
            None,
        );
    }
    record_in_book(
        &mut book,
        &Origin::local(),
        DriftFace::UnknownBackendToken,
        "fresh",
        None,
    );
    assert!(snapshot_in_book(&book, &devbox)[0].overflowed);
    let local = snapshot_in_book(&book, &Origin::local());
    assert!(!local[0].overflowed, "devbox 触顶连带本机那一本也标了溢出");
    assert_eq!(local[0].entries[0].key, "fresh");
}

/// ★ 每个面都必须说清楚「看不懂时会发生什么」—— 诊断面直接显示这句话。
#[test]
fn every_face_states_its_consequence() {
    let faces = [
        DriftFace::UnknownSessionKind,
        DriftFace::UnknownBackendToken,
    ];
    for f in faces {
        assert!(f.consequence().len() > 10, "{f:?} 的后果说明太短");
    }
    // 计数自检：枚举加了新面而这里没跟 ⇒ 红。
    let src = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/drift_ledger.rs"
    ));
    let at = src
        .find("pub enum DriftFace")
        .expect("找不到枚举 —— 抽取坏了");
    let end = src[at..].find("\n}").map(|k| at + k).expect("枚举没收尾");
    let variants = src[at..end]
        .lines()
        .filter(|l| {
            let t = l.trim();
            t.ends_with(',') && t.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        })
        .count();
    assert_eq!(
        variants,
        faces.len(),
        "`DriftFace` 有 {variants} 个变体，本条只覆盖了 {} 个 —— 新增面必须来这里写后果",
        faces.len()
    );
}

/// 〔ST3〕读口：答的是所问那台、回包带回那台；空白名（「没说」）拒收，不许被当成某一台。
#[test]
fn the_read_side_answers_the_asked_machine_and_echoes_it() {
    let probe = Origin("st3-read-probe".into());
    record(&probe, DriftFace::UnknownBackendToken, "st3-read-key", None);
    let r = tauri::async_runtime::block_on(drift_ledger_report(probe.clone()))
        .expect("读口拒了一台正常的机器");
    assert_eq!(r.origin, probe, "回包没带回所问那台");
    assert!(
        key_set(&r.faces).contains(&(DriftFace::UnknownBackendToken, "st3-read-key".to_string()))
    );
    let other =
        tauri::async_runtime::block_on(drift_ledger_report(Origin("st3-read-other".into())))
            .unwrap();
    assert!(other.faces.is_empty(), "问另一台却答出了探针那台的账");
    let err = tauri::async_runtime::block_on(drift_ledger_report(Origin("  ".into())))
        .expect_err("空白名被当成了某一台");
    assert!(
        err.contains("drift_ledger_report"),
        "拒收的话没点名是哪条命令：{err}"
    );
}

// ── 〔ST3〕J3：喂账调用点登记表 ──────────────────────────────────────────────
//
// 🔴 **它的人群**：`src/frontend/shell/src` 生产段里，调了「喂账入口」的函数（`文件, 外层 fn`）。
// 喂账入口 ＝ 直接写账的 `drift_ledger::record` ＋ 把 `origin` 一路交给它的那一个（`note_unknown_capabilities`）。
// 〔MOD〕记录解析那几条（`parse_line` / `parse_for_kind`〔散文墓碑〕 / `batch_to_payloads` / `range_payloads`）出列：解析进了后端，账也跟着记在那台后端。
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
    // 〔C4d · 第四波 4B〕`("history.rs", "analyze_jsonl")` 那一行摘了：本机历史清单搬进本机常驻后端（会话行由后端摘要，
    //   与远端同一个函数），monitor 不再为了列清单逐行解析本机 jsonl。
    // 〔LOC1b · 第四波 4D〕本机远端的冷读合成一条：记账那一跳挪进 `SessionPager::page`（`history.rs`），
    //   `stream_read_session_jsonl` 本身不再解析；远端那一支 `stream_read_remote_session`〔散文墓碑〕那一行随它删了。
    // 〔CF1 · 第四波 09-24〕`lib.rs::run`（`Local`，「本机 jsonl watcher 那一批」）那一行摘了：
    //   本机会话的行从此是本机后端的 `line` 帧，经 `ssh_source·rs::flush_lines`（`Given`，origin 是本机）进账。
    // 〔LOC1b · 第四波 4D〕`("search.rs", "build_one", Local)` 那一行摘了：本机搜索改问本机后端，monitor 内存索引删了。
    // 〔LOC1b · 第四波 4D〕`("session_map.rs", "is_interactive", Local)` 换成下面这一行：本机判活改由本机后端的帧来之后，
    //   「未登记的会话 kind」那一笔在本机那条流上记（monitor 不再自己扫 pidfile）。
    (
        "ssh_source.rs",
        "book_unknown_local_kind",
        Whose::Local,
        "本机那条流的 `session_added.session_kind`：只看本机（记在本机名下）",
    ),
    (
        "ssh_source.rs",
        "note_unknown_capabilities",
        Whose::Given,
        "参数 `origin`",
    ),
    (
        "ssh_source.rs",
        "stream_loop",
        Whose::Given,
        "hello 那一段：那台的 `host_label`",
    ),
];

/// 喂账入口（调用形）。
const FEED_ENTRIES: &[&str] = &[
    "drift_ledger::record(",
    // 〔MOD〕`parse_line(` · `parse_for_kind(` · `batch_to_payloads(` · `range_payloads(` 出列：记录解析进了后端，那几条路不再喂这本账。
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
/// （外层 fn 里先定义过一个嵌套 `fn drop` 之类、体已收尾的，不算 —— `ssh_source·rs::stream_loop` 现打过）。
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
        // 〔RE〕按模块住址认：通信层成员住 `src/comms/inward/`、经 `#[path]` 挂进本 crate（`guard_core` 顺着收）。
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
               fn b() {\n    crate::drift_ledger::record(&Origin::local(), f, k, None);\n}\n\
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
