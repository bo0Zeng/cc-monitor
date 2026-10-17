//! 会话清单那一族（`history/listing.rs`）：一行带什么 · 按名字给的目录挡穿越 · 清单缓存（没变不重扫、变长只扫尾巴）· 热缓存。

use super::*;
use crate::agents::claudecode::paths::projects_root;

fn fixture_project(root: &Path, dir_name: &str) -> PathBuf {
    let dir = root.join("projects").join(dir_name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_jsonl(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, lines.join("\n")).unwrap();
    p
}

#[test]
fn analyze_extracts_excerpt_title_cwd_count() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-test-{}", std::process::id()));
    let dir = fixture_project(&tmp, "proj-a");
    let p = write_jsonl(
        &dir,
        "s1.jsonl",
        &[
            r#"{"type":"user","cwd":"/home/pi/proj","isMeta":true,"message":{"role":"user","content":"skill 注入不算"}}"#,
            r#"{"type":"user","message":{"role":"user","content":"真正的首条用户输入，应该成为摘要"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"回复"}]}}"#,
            r#"{"type":"ai-title","aiTitle":"旧标题"}"#,
            r#"{"type":"ai-title","aiTitle":"最新标题"}"#,
        ],
    );
    let v = analyze_session(&p);
    assert_eq!(v["sessionId"], "s1");
    assert_eq!(v["messageCountApprox"], 5);
    assert_eq!(v["cwd"], "/home/pi/proj");
    assert_eq!(v["aiTitle"], "最新标题"); // 取最新
    assert!(v["firstUserExcerpt"]
        .as_str()
        .unwrap()
        .starts_with("真正的首条用户输入")); // isMeta 跳过
    std::fs::remove_dir_all(&tmp).ok();
}

/// Batch11-F32：sessionKind:"bg" 探测 → isBg。
#[test]
fn analyze_session_detects_bg_kind() {
    let tmp = std::env::temp_dir().join(format!("ccm-isbg-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let f = tmp.join("bg-sid.jsonl");
    std::fs::write(
        &f,
        concat!(
            "{\"type\":\"ai-title\",\"aiTitle\":\"迁移任务\",\"sessionKind\":\"bg\"}\n",
            "{\"type\":\"user\",\"uuid\":\"u1\",\"sessionKind\":\"bg\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n"
        ),
    )
    .unwrap();
    let v = analyze_session(&f);
    assert_eq!(v["isBg"], true);
    let g = tmp.join("normal.jsonl");
    std::fs::write(
        &g,
        "{\"type\":\"user\",\"uuid\":\"u1\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n",
    )
    .unwrap();
    assert_eq!(analyze_session(&g)["isBg"], false);
    std::fs::remove_dir_all(&tmp).ok();
}

// 〔审计 F 🔴-6〕「围栏只许有一处」（audit-0805 08-06，定框 E3）那一条搬去 `fence_tests.rs`、名字照旧：
//   它先前只数**本文件**里的 `canonicalize()`（== 2），而 `search_query.rs` 里还有一份内联的 ——
//   「守卫范围 ≠ 性质范围」。围栏收进 `common/fence.rs` 之后，人群换成 observe 全树。
// LOC1b 把那一条的锚从 `fn fence_under_projects` 换成了 `fn fence_under_root`（本体提成根是参数）；
//   那一形今天就是 `Fence::at` ＋ `admit`，锚跟着住 `fence_tests.rs`。

#[test]
fn list_sessions_rejects_path_traversal() {
    let tmp = std::env::temp_dir();
    assert!(list_sessions_for(&tmp, "../escape", None, &mut Vec::new()).is_err());
    assert!(list_sessions_for(&tmp, "a/b", None, &mut Vec::new()).is_err());
    assert!(list_sessions_for(&tmp, r"a\b", None, &mut Vec::new()).is_err());
}

#[test]
#[cfg(unix)]
fn list_sessions_rejects_symlink_escape() {
    use std::os::unix::fs::symlink;
    let tmp = std::env::temp_dir().join(format!("ccm-hq-sym-{}", std::process::id()));
    let projects = tmp.join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    // projects/ 外的目录，放一个 jsonl
    let outside = tmp.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("leak.jsonl"), r#"{"type":"user"}"#).unwrap();
    // projects/sneaky -> ../outside（名字合法、无分隔符、无 ..，旧 string 校验放行）
    symlink(&outside, projects.join("sneaky")).unwrap();
    // canonicalize 前缀校验解析 symlink 后落在 projects/ 外 → 拒绝
    assert!(list_sessions_for(&tmp, "sneaky", None, &mut Vec::new()).is_err());
    std::fs::remove_dir_all(&tmp).ok();
}

// ════════════════════════════════════════════════════════════════════════════════════════
// 会话清单那一行从此是本机与远端共用的唯一口径 —— 补上的三格 ＋ 摘录清洗
// ════════════════════════════════════════════════════════════════════════════════════════
//
// 要求：「本机后端 … 出成品 …（join 只一个家）」——
// 本机会话清单从 monitor 那份（经记录解析器）换到这一行，monitor 那份有的三格这里要有，否则本机用户换读者那一刻丢 fork 树与改名后的标题。

/// ★ fork 关系（首条带 `forkedFrom` 的 user / assistant）· `custom-title` 取最新 · 开始时刻取首条时间戳。
#[test]
fn c4d_the_row_carries_fork_parent_custom_title_and_first_timestamp() {
    let tmp = std::env::temp_dir().join(format!("ccm-c4d-row-{}", std::process::id()));
    let dir = fixture_project(&tmp, "proj-c4d");
    let p = write_jsonl(
        &dir,
        "child.jsonl",
        &[
            r#"{"type":"file-history-snapshot","messageId":"x"}"#,
            r#"{"type":"user","timestamp":"2026-09-25T01:02:03.456Z","cwd":"/w","forkedFrom":{"sessionId":"parent-sid","messageUuid":"m-1"},"message":{"role":"user","content":"<system-reminder>注入</system-reminder>占位问题\n第二行"}}"#,
            r#"{"type":"assistant","timestamp":"2026-09-25T01:02:04.000Z","forkedFrom":{"sessionId":"other","messageUuid":"m-2"},"message":{"role":"assistant","content":[]}}"#,
            r#"{"type":"ai-title","aiTitle":"旧标题"}"#,
            r#"{"type":"custom-title","customTitle":"改过的名字"}"#,
        ],
    );
    let v = analyze_session(&p);
    assert_eq!(v["forkedFromSessionId"], "parent-sid", "取首条那一个");
    assert_eq!(v["forkedFromMessageUuid"], "m-1");
    assert_eq!(v["aiTitle"], "改过的名字", "custom-title 在后 ⇒ 取它");
    assert_eq!(
        v["startedAtMs"],
        crate::common::time::parse_iso8601_ms("2026-09-25T01:02:03.456Z").unwrap()
    );
    assert_eq!(
        v["firstUserExcerpt"], "占位问题 第二行",
        "注入包装剥掉、换行折成空格"
    );
    // 没有 forkedFrom / 没有时间戳 ⇒ 两格 null、开始时刻退回文件时刻（不是 0）。
    let q = write_jsonl(
        &dir,
        "plain.jsonl",
        &[r#"{"type":"user","message":{"role":"user","content":"[Request interrupted by user]"}}"#],
    );
    let w = analyze_session(&q);
    assert_eq!(w["forkedFromSessionId"], serde_json::Value::Null);
    assert_eq!(w["forkedFromMessageUuid"], serde_json::Value::Null);
    assert!(
        w["startedAtMs"].as_i64().unwrap() > 0,
        "没有时间戳也不许报 0"
    );
    assert_eq!(w["firstUserExcerpt"], "", "纯中断标记不是用户说的话");
    // 摘录超长 ⇒ 120 个字符 ＋ `…`。
    let long = "字".repeat(130);
    let line = format!(r#"{{"type":"user","message":{{"role":"user","content":"{long}"}}}}"#);
    let r = write_jsonl(&dir, "long.jsonl", &[line.as_str()]);
    let x = analyze_session(&r);
    assert_eq!(x["firstUserExcerpt"], format!("{}…", "字".repeat(120)));
    std::fs::remove_dir_all(&tmp).ok();
}

/// ★ 这一行的键集合恒等（本机与远端两个出口读的是同一行；多一格 / 少一格 ⇒ 下游 join 那一层对不上）。
#[test]
fn c4d_the_row_has_exactly_these_keys() {
    let tmp = std::env::temp_dir().join(format!("ccm-c4d-keys-{}", std::process::id()));
    let dir = fixture_project(&tmp, "proj-keys");
    let p = write_jsonl(
        &dir,
        "k.jsonl",
        &[r#"{"type":"user","message":{"content":"x"}}"#],
    );
    let v = analyze_session(&p);
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "aiTitle",
            "cwd",
            "firstUserExcerpt",
            "forkedFromMessageUuid",
            "forkedFromSessionId",
            "isBg",
            "jsonlPath",
            "messageCountApprox",
            "sessionId",
            "startedAtMs",
            "updatedAtMs",
        ]
    );
    std::fs::remove_dir_all(&tmp).ok();
}

/// 要求：「本机没有 projects 目录 ⇒ 整页加载失败，原话 read_dir … (os error 3)」·
/// 记录树根不在 ⇒ 零个项目、`Ok`（不是失败）；根在但读不了 ⇒ 说人话（期望取自文案表那一条，不含 `read_dir` / `os error`）。
#[test]
fn a_machine_without_a_projects_dir_lists_nothing_and_an_unreadable_one_says_so_plainly() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-noproj-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let mut out = Vec::new();
    assert_eq!(list_projects_into(&tmp, &mut out), Ok(false));
    assert!(out.is_empty(), "没有记录树却列出了东西");
    // 平铺清单当零个项目（不失败）。
    assert_eq!(sessions_by_dir(&tmp), Ok(None));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let root = projects_root(&tmp);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o000)).unwrap();
        // root 跑测试时权限位拦不住读 —— 那一格判不了，如实跳过而不是假绿（同 `tasks_query_tests`）。
        let perms_bite = std::fs::File::open(&root).is_err();
        let got = list_projects_into(&tmp, &mut out);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        if perms_bite {
            let e = got.expect_err("读不了的记录树却列成功了");
            let path = root.display().to_string();
            assert_eq!(e, copy_text("beHistory.dir.denied", &[("path", &path)]));
            assert!(!e.contains("read_dir") && !e.contains("os error"), "{e}");
        }
    }
    std::fs::remove_dir_all(&tmp).ok();
}

/// 〔perfC #1/#2〕**扫清单那一行不复制用不上的大块**：一条记录里一段近 4 MiB 的工具输出，
/// 扫这一份时本线程未释放字节的高水位，不许比「只是逐行读一遍这份文件」高出一截 ——
/// 逐行解成完整 `Value` 的样子是行缓冲之外再复制一份那 4 MiB。
/// 第一行带 `cwd`：项目目录只读开头、读到第一个 `cwd` 就停（真记录第一行就有），不让它去读那一大行。
#[test]
fn listing_scan_does_not_copy_records_it_does_not_need() {
    use std::io::BufRead;
    let tmp = std::env::temp_dir().join(format!("ccm-hq-alloc-{}", std::process::id()));
    let dir = fixture_project(&tmp, "proj-alloc");
    let pad = "x".repeat(4 << 20);
    let big = format!(
        r#"{{"type":"user","timestamp":"2026-10-01T08:00:00Z","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t1","content":"{pad}"}}]}}}}"#
    );
    let lines = [
        r#"{"type":"user","cwd":"/w/alloc","timestamp":"2026-10-01T07:59:00Z","message":{"role":"user","content":"第一句"}}"#,
        big.as_str(),
        r#"{"type":"ai-title","aiTitle":"标题"}"#,
    ];
    let p = write_jsonl(&dir, "s-alloc.jsonl", &lines);
    // 地板：只逐行读一遍（行缓冲按倍数长，长多少由读法定，不由解析定）。
    let base = crate::alloc_probe::reset_peak();
    for l in std::io::BufReader::new(std::fs::File::open(&p).unwrap())
        .lines()
        .map_while(Result::ok)
    {
        std::hint::black_box(l);
    }
    let floor = crate::alloc_probe::peak_since(base);
    let base = crate::alloc_probe::reset_peak();
    let v = analyze_session(&p);
    let grew = crate::alloc_probe::peak_since(base);
    // 反空真：真扫到了这一份（三行都数到、标题与第一句都取到），地板真读到了那一大行。
    assert_eq!(v["messageCountApprox"], 3);
    assert_eq!(v["aiTitle"], "标题");
    assert_eq!(v["firstUserExcerpt"], "第一句");
    assert!(floor >= big.len(), "地板 {floor} 字节没读到那一大行");
    assert!(
        grew < floor + (256 << 10),
        "扫一份带 {} 字节记录的会话，高水位 {grew} 字节，比只逐行读一遍（{floor}）高出一截 —— 用不上的那一大段被复制了一份",
        big.len()
    );
    std::fs::remove_dir_all(&tmp).ok();
}

/// 〔perfC #1/#2〕扫清单的口径在怪形状上不变：每一格照「整行解成 JSON、按字段取串」那一套取。
/// 非串的 `type` / 标题 / 时刻当没有 · 同名键取最后一个 · 顶层不是对象只数行 · 坏行只数行 · 分叉来源两格都得是串。
#[test]
fn listing_scan_reads_odd_shapes_the_same_way() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-odd-{}", std::process::id()));
    let dir = fixture_project(&tmp, "proj-odd");
    let p = write_jsonl(
        &dir,
        "s-odd.jsonl",
        &[
            r#"[1,2,3]"#,
            r#""just a string""#,
            r#"{not json"#,
            r#"{"type":7,"aiTitle":"类型不是串，不算标题"}"#,
            r#"{"type":"ai-title","aiTitle":{"x":1}}"#,
            r#"{"type":"user","timestamp":12345,"message":{"role":"user","content":"时刻不是串"}}"#,
            r#"{"type":"assistant","forkedFrom":{"sessionId":"src","messageUuid":9},"timestamp":"2026-10-01T08:00:00Z"}"#,
            r#"{"type":"assistant","forkedFrom":"bad","sessionKind":["bg"]}"#,
            r#"{"type":"user","type":"ai-title","aiTitle":"同名键取最后一个"}"#,
            "\u{feff}{\"type\":\"user\",\"forkedFrom\":{\"sessionId\":\"src2\",\"messageUuid\":\"m2\"},\"sessionKind\":\"bg\"}",
        ],
    );
    let v = analyze_session(&p);
    assert_eq!(v["messageCountApprox"], 10);
    assert_eq!(v["aiTitle"], "同名键取最后一个");
    assert_eq!(v["firstUserExcerpt"], "时刻不是串");
    // 第一条 user/assistant 的时刻不是串 ⇒ 取下一条带串的那一条。
    assert_eq!(
        v["startedAtMs"],
        crate::common::time::parse_iso8601_ms("2026-10-01T08:00:00Z").unwrap()
    );
    assert_eq!(v["forkedFromSessionId"], "src2");
    assert_eq!(v["forkedFromMessageUuid"], "m2");
    assert_eq!(v["isBg"], true);
    std::fs::remove_dir_all(&tmp).ok();
}

/// 〔perfC #3〕**正在写的会话变长，清单那一行只扫新增的那一段**，答案与整份重扫逐字相同；
/// 被截短 · 同长被改写 · 变长但前面被改写（见证对不上）· 半行写完 ⇒ 照样与整份重扫相同（退回整份扫）。
#[test]
fn a_growing_session_is_rescanned_only_for_what_was_appended() {
    use std::io::Write as _;
    let tmp = std::env::temp_dir().join(format!("ccm-hq-grow-{}", std::process::id()));
    let dir = fixture_project(&tmp, "proj-grow");
    let p = dir.join("s-grow.jsonl");
    let rec = |i: usize| {
        format!(
            "{{\"type\":\"user\",\"cwd\":\"/w/grow\",\"timestamp\":\"2026-10-01T08:{:02}:00Z\",\"message\":{{\"role\":\"user\",\"content\":\"第 {i} 句 {}\"}}}}\n",
            i % 60,
            "填".repeat(2000)
        )
    };
    let body: String = (0..200).map(rec).collect();
    std::fs::write(&p, &body).unwrap();
    let bump = |secs: u64| {
        let f = std::fs::File::options().write(true).open(&p).unwrap();
        f.set_modified(
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_900_000_000 + secs),
        )
        .unwrap();
    };
    bump(0);
    let last_bytes = || {
        SESSION_META
            .lock()
            .unwrap()
            .get(&p)
            .map(|e| e.last_bytes)
            .expect("清单缓存里有这一份")
    };
    let same = |what: &str| {
        let got = analyze_session_cached(&p);
        assert_eq!(
            got,
            analyze_session(&p),
            "{what}：缓存那一行与整份重扫不一样"
        );
        got
    };
    same("第一次");
    assert!(last_bytes() >= body.len() as u64, "第一次就该整份扫");
    // 追加一条：只读尾巴（见证 ＋ 新的那一行）。
    let one = rec(200);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&p)
        .unwrap()
        .write_all(one.as_bytes())
        .unwrap();
    bump(1);
    let v = same("追加一条之后");
    assert_eq!(v["messageCountApprox"], 201);
    assert!(
        last_bytes() < (one.len() + 1024) as u64,
        "追加一条之后读了 {} 字节（整份 {}）—— 整份重扫了",
        last_bytes(),
        body.len() + one.len()
    );
    // 半行：先写一半（没有换行），再写完。
    let half = rec(201);
    let (a, b) = half.as_bytes().split_at(40); // 切在 ASCII 那一段：半行本身是合法 UTF-8（数作一行，与整份逐行读同）
    std::fs::OpenOptions::new()
        .append(true)
        .open(&p)
        .unwrap()
        .write_all(a)
        .unwrap();
    bump(2);
    assert_eq!(same("写了半行")["messageCountApprox"], 202);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&p)
        .unwrap()
        .write_all(b)
        .unwrap();
    bump(3);
    assert_eq!(same("半行写完")["messageCountApprox"], 202);
    // 截短：退回整份。
    std::fs::write(&p, (0..50).map(rec).collect::<String>()).unwrap();
    bump(4);
    assert_eq!(same("截短之后")["messageCountApprox"], 50);
    // 同长改写：标题换了。
    let mut text: String = (0..49).map(rec).collect();
    let bare = "{\"type\":\"ai-title\",\"aiTitle\":\"新标题\"}\n".len();
    let title = format!(
        "{{\"type\":\"ai-title\",\"aiTitle\":\"新标题\"{}}}\n",
        " ".repeat(rec(49).len() - bare)
    );
    assert_eq!(
        text.len() + title.len(),
        (0..50).map(rec).collect::<String>().len()
    );
    text.push_str(&title);
    std::fs::write(&p, &text).unwrap();
    bump(5);
    assert_eq!(same("同长改写之后")["aiTitle"], "新标题");
    // 变长、但前面被改写（见证对不上）：第一句换了。
    let mut text2 = rec(0).replace("第 0 句", "改过的第一句");
    text2.push_str(&(1..80).map(rec).collect::<String>());
    std::fs::write(&p, &text2).unwrap();
    bump(6);
    let v = same("变长但前面被改写");
    assert!(v["firstUserExcerpt"]
        .as_str()
        .unwrap()
        .starts_with("改过的第一句"));
    std::fs::remove_dir_all(&tmp).ok();
}

/// 〔perfC #4〕后台热缓存：热完之后整台清单一个字节都不再读（每份都是「没变」那一形），答案与现扫相同。
#[test]
fn warming_the_listing_leaves_nothing_to_rescan() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-warm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let mut paths = Vec::new();
    for d in 0..6 {
        let dir = fixture_project(&tmp, &format!("proj-warm-{d}"));
        for k in 0..4 {
            let line = format!(
                r#"{{"type":"user","cwd":"/w/warm{d}","message":{{"role":"user","content":"第 {k} 份"}}}}"#
            );
            paths.push(write_jsonl(
                &dir,
                &format!("s-warm-{d}-{k}.jsonl"),
                &[line.as_str()],
            ));
        }
    }
    let n = warm_listing(&tmp);
    assert!(n >= paths.len(), "热完缓存里只有 {n} 份");
    let tree = sessions_by_dir(&tmp).unwrap().unwrap();
    assert_eq!(
        tree.iter()
            .map(|(_, r)| r.as_ref().map_or(0, Vec::len))
            .sum::<usize>(),
        paths.len()
    );
    for p in &paths {
        let read = SESSION_META.lock().unwrap().get(p).map(|e| e.last_bytes);
        assert_eq!(read, Some(0), "热过之后再问清单，{} 又读了", p.display());
        assert_eq!(analyze_session_cached(p), analyze_session(p));
    }
    std::fs::remove_dir_all(&tmp).ok();
}

/// 〔perfC #4〕接线：`main.rs` 起来就热清单缓存，恰好一处（按行为量要起真二进制再从外面看缓存）。
#[test]
fn main_warms_the_listing_once() {
    let main = guard_core::production_code(include_str!("../../../src/backend/main.rs"));
    guard_core::find_pinned(
        &main,
        "history::listing::warm_listing_in_background(agent_home.clone());",
    )
    .unwrap_or_else(|e| panic!("`main.rs` 里热清单缓存不是恰好一处：{e}"));
}

/// 〔perfC #4〕后台热清单缓存的线程真降了优先级（Linux：这条线程的 nice 值），而且只降它自己。
#[cfg(target_os = "linux")]
#[test]
fn background_warmers_run_at_low_priority() {
    let nice_of_this_thread = || {
        let s = std::fs::read_to_string("/proc/thread-self/stat").unwrap();
        let rest: Vec<&str> = s[s.rfind(')').unwrap() + 2..].split(' ').collect();
        rest[16].parse::<i64>().unwrap()
    };
    let before = nice_of_this_thread();
    let (ok, after) = std::thread::spawn(move || {
        let ok = crate::platform::proc::lower_this_thread();
        (ok, nice_of_this_thread())
    })
    .join()
    .unwrap();
    assert!(ok, "降不了优先级");
    assert_eq!(
        after,
        i64::from(crate::platform::proc::BACKGROUND_NICE).max(before)
    );
    assert_eq!(nice_of_this_thread(), before, "降到了别的线程身上");
    let src = guard_core::production_code(include_str!("../../../src/backend/history/listing.rs"));
    assert!(
        src.contains("crate::platform::proc::lower_this_thread();"),
        "history/listing.rs 的后台热缓存线程没降优先级"
    );
}

/// d：清单缓存的追加读按块 —— 两问之间长了 8 MiB，读的时候本线程手里最多两块多一点（今天整截读进一个 `Vec`）；
/// 那一行与整份重扫的一样。
#[test]
fn listing_append_holds_about_a_block_not_the_growth() {
    use crate::common::append_read::test_support::{big_record, BIG, HELD_BOUND};
    use std::io::Write as _;
    let tmp = std::env::temp_dir().join(format!("ccm-hq-held-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let p = tmp.join("grow.jsonl");
    std::fs::write(
        &p,
        "{\"type\":\"user\",\"uuid\":\"u-a\",\"timestamp\":\"2026-04-01T00:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"first\"}}\n",
    )
    .unwrap();
    let mtime = |secs: u64| {
        std::fs::File::options()
            .write(true)
            .open(&p)
            .unwrap()
            .set_modified(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_900_000_000 + secs),
            )
            .unwrap();
    };
    mtime(0);
    let _ = analyze_session_cached(&p);
    let more = tmp.join("more.jsonl");
    big_record(&more, BIG, false);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&p)
        .unwrap()
        .write_all(&std::fs::read(&more).unwrap())
        .unwrap();
    mtime(1);
    let base = crate::alloc_probe::reset_peak();
    let row = analyze_session_cached(&p);
    let held = crate::alloc_probe::peak_since(base);
    let read = SESSION_META.lock().unwrap().get(&p).map(|e| e.last_bytes);
    let whole = analyze_session(&p);
    let _ = std::fs::remove_dir_all(&tmp);
    assert_eq!(row, whole, "追加读那一行与整份重扫不一样");
    assert!(
        read.is_some_and(|b| b < BIG as u64 + 4096),
        "该只读追加的那一截（读了 {read:?}）"
    );
    assert!(
        held <= HELD_BOUND,
        "追加读 {BIG} 字节，本线程手里最多 {held} 字节，上界 {HELD_BOUND}"
    );
}

/// ★ **历史查询那几条读路不许整读 jsonl**〔audit-0805 F07 / 报告 I-10 与 B-6 第 5 环〕。
///
/// `analyze_session` 此前是 `read_to_string`：`--list-sessions` 对该项目**每个** jsonl 调它一次；
///   43 个项目 / 2.4 GB 的机器上，一次列表就是把 2.4 GB 读进内存再逐行解析。
///
/// 扫描本身是必须的（要数行、要判 bg），**但不必先整份进内存**。
/// ⚠ 「慢/费内存」**不会让任何测试变红** ⇒ 只能靠源码形态钉（同 F04 那条）。
#[test]
fn the_history_readers_stream_instead_of_slurping() {
    let src = guard_core::production_code(include_str!("../../../src/backend/history/listing.rs"));
    // 项目目录那一格只读记录开头（适配层，`agents::first_in_head`），上界由那边的行为判据钉。
    // 〔perfC〕逐行扫那一段住清单缓存那一格（`ListingEntry::scan`，变长只扫尾巴也在那里）；`analyze_session` 只转调它。
    let begin = src
        .find("fn analyze_session(")
        .expect("找不到 analyze_session —— 抽取器坏了");
    let end = src[begin..]
        .find("\n}\n")
        .expect("找不到 analyze_session 的结尾");
    assert!(
        src[begin..begin + end].contains("ListingEntry::scan("),
        "analyze_session 不再转调 `ListingEntry::scan` —— 下面那条钉的就不是它读文件的那一段了"
    );
    for (name, sig) in [("ListingEntry", "impl ListingEntry {")] {
        let begin = src
            .find(sig)
            .unwrap_or_else(|| panic!("找不到 {name} —— 抽取器坏了，本条会零命中地绿"));
        let end = src[begin..]
            .find("\n}\n")
            .unwrap_or_else(|| panic!("找不到 {name} 的结尾 —— 抽取器坏了"));
        let body = &src[begin..begin + end];
        assert!(
            body.contains("BufReader"),
            "{name} 里没有 `BufReader` —— 要么切错范围（本条会零命中地绿），要么它被改回整读了"
        );
        assert!(
            !body.contains("read_to_string("),
            "{name} 又在整读 jsonl 了。\n\
                 `--list-projects` / `--list-sessions` 会对**每个**项目/会话文件调它一次，\n\
                 而 43 个项目 / 2.4 GB 的机器上那就是一次列表读 2.4 GB。\n\
                 慢不会让任何测试变红 —— 所以这条只能靠源码形态钉。"
        );
    }
}

/// 清单缓存落盘：内存那一份没了（重启）之后照盘上那一份接着认 —— 没变的一个字节都不读、变长的只读见证与追加的那一截；
/// 答案与整份重扫逐字相同。家在沙箱的数据目录里。
#[test]
fn a_restart_takes_the_listing_from_disk() {
    use std::io::Write as _;
    let tmp = std::env::temp_dir().join(format!("ccm-hq-disk-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("data")).unwrap();
    let (data, home) = (
        tmp.join("data").to_string_lossy().into_owned(),
        tmp.to_string_lossy().into_owned(),
    );
    let own = crate::platform::fs::Own::issue_with(&|k| match k {
        creds_core::store::DATA_DIR_ENV => Some(data.clone()),
        "HOME" => Some(home.clone()),
        _ => None,
    });
    let dir = fixture_project(&tmp, "proj-disk");
    let rec = |i: usize| {
        format!(
            "{{\"type\":\"user\",\"cwd\":\"/w/disk\",\"timestamp\":\"2026-10-01T08:{:02}:00Z\",\"message\":{{\"role\":\"user\",\"content\":\"第 {i} 句 {}\"}}}}\n",
            i % 60,
            "填".repeat(2000)
        )
    };
    let p = dir.join("s-disk.jsonl");
    std::fs::write(&p, (0..100).map(rec).collect::<String>()).unwrap();
    let title = dir.join("s-disk-title.jsonl");
    std::fs::write(
        &title,
        format!(
            "{}{{\"type\":\"ai-title\",\"aiTitle\":\"盘上的标题\"}}\n",
            rec(0)
        ),
    )
    .unwrap();
    let read = |p: &Path| SESSION_META.lock().unwrap().get(p).map(|e| e.last_bytes);
    let forget = || {
        let mut t = SESSION_META.lock().unwrap();
        t.remove(&p);
        t.remove(&title);
    };
    for q in [&p, &title] {
        analyze_session_cached(q);
    }
    save_to(&own);
    forget();
    load_from(&own);
    for q in [&p, &title] {
        assert_eq!(
            analyze_session_cached(q),
            analyze_session(q),
            "{}",
            q.display()
        );
        assert_eq!(read(q), Some(0), "重启之后没变的 {} 又读了", q.display());
    }
    // 重启之后变长：只读见证与追加的那一行。
    forget();
    load_from(&own);
    let one = rec(100);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&p)
        .unwrap()
        .write_all(one.as_bytes())
        .unwrap();
    let v = analyze_session_cached(&p);
    assert_eq!(v, analyze_session(&p));
    assert_eq!(v["messageCountApprox"], 101);
    let got = read(&p).unwrap();
    assert!(
        got < (one.len() + 1024) as u64,
        "重启之后追加一行读了 {got} 字节 —— 整份重扫了"
    );
    std::fs::remove_dir_all(&tmp).ok();
}
