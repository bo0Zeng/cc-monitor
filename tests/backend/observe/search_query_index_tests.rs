//! J2：一串变更之后，热索引（增量读）与新建索引（整份读）逐问逐行相等（两向）· J3：真的是增量 ——
//! 经帧面那一臂的入口（`search_into`）连问，读盘字节数 == 按变更独立算出的数（没变 ⇒ 0）。
//! 守的要求：「按文件 mtime / 长度增量」「结果与现扫逐条相等（两向）」；J1（金样）钉「整份读 == 起步树现扫」。

use super::golden_tests::{build_corpus, cases};
use super::test_support::{lines_into, opts_from_words};
use super::*;
use crate::agents::claudecode::paths::projects_root;
use serde_json::json;

fn p(home: &Path, rel: &str) -> PathBuf {
    home.join("projects").join(rel)
}

fn touch(path: &Path, ms: i64) {
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms as u64);
    std::fs::File::options()
        .write(true)
        .open(path)
        .and_then(|f| f.set_times(std::fs::FileTimes::new().set_modified(t)))
        .expect("改 mtime");
}

fn append(path: &Path, bytes: &[u8]) {
    use std::io::Write as _;
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .and_then(|mut f| f.write_all(bytes))
        .expect("追加");
}

fn line(uuid: &str, text: &str) -> String {
    json!({"type":"user","uuid":uuid,"timestamp":"2026-04-01T00:00:00Z",
        "message":{"role":"user","content":text}})
    .to_string()
}

/// 在一个索引上把 [`cases`] 全问一遍：每问 `(名字, 读不动数, 输出行)`；另回这一遍里追加读了几次。
fn ask_all(
    index: &mut SearchIndex,
    home: &Path,
) -> (Vec<(&'static str, usize, Vec<String>)>, usize) {
    let fence = Fence::at(&projects_root(home)).expect("围栏");
    let mut appended = 0;
    let rows = cases()
        .into_iter()
        .filter(|(_, q, _)| !q.trim().is_empty())
        .map(|(name, q, rest)| {
            let mut buf = Vec::new();
            let n = index
                .search(
                    &fence,
                    &q.trim().to_lowercase(),
                    &opts_from_words(&rest),
                    &crate::history::live::live_session_ids(home),
                    &mut lines_into(&mut buf),
                )
                .expect("search ok");
            appended += index.last.appended;
            let rows = String::from_utf8(buf).expect("UTF-8");
            (name, n, rows.lines().map(str::to_string).collect())
        })
        .collect();
    (rows, appended)
}

/// ★ J2：每一步变更之后，热索引那一问 == 新建索引那一问（读不动数与行序列都相等；不等时报两向差集）。
#[test]
fn sx1_incremental_answers_equal_a_fresh_full_read_after_every_change() {
    let home = build_corpus("j2");
    let mut warm = SearchIndex::default();
    let mut clock = 1_800_000_000_000i64;
    let mut stamp = |path: &Path| {
        clock += 1_000;
        touch(path, clock);
    };
    type Step = (&'static str, Box<dyn Fn(&Path)>);
    let steps: Vec<Step> = vec![
        ("起点", Box::new(|_| {})),
        (
            "补完残尾 ＋ 追加完整行与新标题",
            Box::new(|h| {
                let mut s = format!("\n{}\n", line("a1-u10", "appended docker line"));
                s.push_str(&json!({"type":"ai-title","aiTitle":"追加的标题"}).to_string());
                s.push('\n');
                append(&p(h, "p-alpha/a1.jsonl"), s.as_bytes());
            }),
        ),
        (
            "追加没写完的一行（完整 JSON、缺 \\n）",
            Box::new(|h| {
                append(
                    &p(h, "p-beta/b2.jsonl"),
                    line("b2-u9", "partial docker kubernetes").as_bytes(),
                )
            }),
        ),
        (
            "补完那一行再追加一行",
            Box::new(|h| {
                let s = format!("\n{}\n", line("b2-u10", "docker after partial"));
                append(&p(h, "p-beta/b2.jsonl"), s.as_bytes());
            }),
        ),
        (
            "截短",
            Box::new(|h| {
                let f = p(h, "p-delta/crlf.jsonl");
                let first = std::fs::read(&f).expect("读");
                let k = first.iter().position(|&b| b == b'\n').expect("有一行") + 1;
                std::fs::write(&f, &first[..k]).expect("写");
            }),
        ),
        (
            "改写且变长（见证对不上）",
            Box::new(|h| {
                let s = format!(
                    "{}\n{}\n",
                    line("t-new1", "rewritten docker"),
                    line("t-new2", "longer docker")
                );
                std::fs::write(p(h, "top.jsonl"), s).expect("写");
            }),
        ),
        (
            "同长改写（只有 mtime 变）",
            Box::new(|h| {
                let f = p(h, "p-gamma/g2.jsonl");
                let s = std::fs::read_to_string(&f)
                    .expect("读")
                    .replace("Docker only", "DOCKER ONLY");
                std::fs::write(&f, s).expect("写");
            }),
        ),
        (
            "删一份",
            Box::new(|h| std::fs::remove_file(p(h, "p-beta/b1.jsonl")).expect("删")),
        ),
        (
            "新增一份",
            Box::new(|h| {
                let f = p(h, "p-new/n1.jsonl");
                std::fs::create_dir_all(f.parent().expect("父")).expect("建");
                std::fs::write(&f, format!("{}\n", line("n1-u1", "new docker file"))).expect("写");
            }),
        ),
        (
            "追加一行坏 UTF-8（完整行里 ⇒ 读不动）",
            Box::new(|h| {
                append(
                    &p(h, "top.jsonl"),
                    b"{\"type\":\"user\",\"x\":\"\xff\xfe docker\"}\n",
                )
            }),
        ),
        (
            "整份改回合法、变短",
            Box::new(|h| {
                std::fs::write(
                    p(h, "top.jsonl"),
                    format!("{}\n", line("t-ok", "fixed docker")),
                )
                .expect("写")
            }),
        ),
        (
            "残尾撕在多字节字符中间（读不动）",
            Box::new(|h| {
                let mut b =
                    br#"{"type":"user","uuid":"c-u9","message":{"content":"docker "#.to_vec();
                b.extend_from_slice(&"中".as_bytes()[..2]);
                append(&p(h, "p-delta/crlf.jsonl"), &b);
            }),
        ),
        (
            "补完那个字符与那一行",
            Box::new(|h| {
                let mut b = "中".as_bytes()[2..].to_vec();
                b.extend_from_slice(b"\"}}\n");
                append(&p(h, "p-delta/crlf.jsonl"), &b);
            }),
        ),
    ];
    let touched = [
        "",
        "p-alpha/a1.jsonl",
        "p-beta/b2.jsonl",
        "p-beta/b2.jsonl",
        "p-delta/crlf.jsonl",
        "top.jsonl",
        "p-gamma/g2.jsonl",
        "",
        "p-new/n1.jsonl",
        "top.jsonl",
        "top.jsonl",
        "p-delta/crlf.jsonl",
        "p-delta/crlf.jsonl",
    ];
    assert_eq!(steps.len(), touched.len());
    let mut appended_any = 0usize;
    for ((what, act), rel) in steps.iter().zip(touched) {
        act(&home);
        if !rel.is_empty() {
            stamp(&p(&home, rel));
        }
        let (got, appended) = ask_all(&mut warm, &home);
        appended_any += appended;
        let (want, _) = ask_all(&mut SearchIndex::default(), &home);
        for ((name, gn, gl), (_, wn, wl)) in got.iter().zip(want.iter()) {
            let only_warm: Vec<&String> = gl.iter().filter(|l| !wl.contains(l)).collect();
            let only_fresh: Vec<&String> = wl.iter().filter(|l| !gl.contains(l)).collect();
            assert!(
                gn == wn && gl == wl,
                "「{what}」之后问 `{name}`：热索引 ≠ 整份重读（读不动 {gn} vs {wn}）。\n只在热索引里：{only_warm:#?}\n只在整份重读里：{only_fresh:#?}"
            );
        }
    }
    // 反空真：这一串里真的走过追加读（否则上面的相等在「每问都整份重读」下也成立 —— 那一格归 J3 判）。
    assert!(
        appended_any > 0,
        "一步追加读都没走过 —— 本条只在比整份读对整份读"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// ★ J3：经帧面那一臂的入口连问，读盘字节数 == 独立算出的数：首问整份读全部 · 没变 ⇒ 0 · 追加 n ⇒ 见证 ＋ n。
#[test]
fn sx1_the_frame_arm_reads_only_what_changed() {
    let home = build_corpus("j3");
    let key = projects_root(&home).canonicalize().expect("规范化");
    let ask = || {
        let mut out = Vec::new();
        search_into(
            &home,
            "docker",
            &opts_from_words(&[]),
            &mut lines_into(&mut out),
        )
        .expect("search ok");
    };
    let last = || {
        RESIDENT
            .lock()
            .expect("锁")
            .get(&key)
            .map(|i| i.last)
            .expect("帧面那一问没落在常驻表上")
    };
    let population = [
        "p-alpha/a1.jsonl",
        "p-beta/b1.jsonl",
        "p-beta/b2.jsonl",
        "p-gamma/g1.jsonl",
        "p-gamma/g2.jsonl",
        "p-gamma/g3.jsonl",
        "top.jsonl",
        "p-delta/empty.jsonl",
        "p-delta/crlf.jsonl",
    ];
    let size = |rel: &str| std::fs::metadata(p(&home, rel)).expect("stat").len();
    let total: u64 = population.iter().map(|r| size(r)).sum();
    let n = population.len();

    ask();
    let first = Refresh {
        full: n,
        appended: 0,
        reused: 0,
        bytes: total,
    };
    assert_eq!(last(), first, "首问：整份读全部人群");
    ask();
    let still = Refresh {
        full: 0,
        appended: 0,
        reused: n,
        bytes: 0,
    };
    assert_eq!(last(), still, "没变 ⇒ 一个字节都不读");

    let b2 = p(&home, "p-beta/b2.jsonl");
    let before = size("p-beta/b2.jsonl");
    let add = format!("{}\n", line("b2-u20", "docker appended for j3"));
    append(&b2, add.as_bytes());
    touch(&b2, 1_900_000_000_000);
    ask();
    let witness = before.min(crate::common::append_read::WITNESS_BYTES as u64);
    let grew = Refresh {
        full: 0,
        appended: 1,
        reused: n - 1,
        bytes: witness + add.len() as u64,
    };
    assert_eq!(last(), grew, "追加 n 字节 ⇒ 只读见证 ＋ n");
    std::fs::remove_dir_all(&home).ok();
}

// ───────────────────────── ─────────────────────────
// 守的要求：「常驻内存有界 · 常驻表只为真搜过的人存在」。第一问来时建、建好留着（按最近优先留到上界，留不下的照搜不留）。

/// ★ 起来不建：往常驻表里放东西的只有两问的入口（全文搜索 · 会话内查找）；没人问，表就是空的、projects 一份都不为它读。
/// 文本判（如实登记：按行为量要起真二进制再从外面看索引）；表与它的锁是本模块私有的，别处够不着，只需看这一份。
#[test]
fn fix_only_a_question_fills_the_resident_index() {
    let src = guard_core::production_code(include_str!(
        "../../../src/backend/observe/search_query.rs"
    ));
    let fn_name = |line: &str| {
        let rest = guard_core::strip_visibility(line.trim_start())
            .trim_start()
            .strip_prefix("fn ")?;
        let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_'))?;
        Some(rest[..end].to_string())
    };
    let mut within = None;
    let mut who = std::collections::BTreeSet::new();
    for line in src.lines() {
        if let Some(name) = fn_name(line) {
            within = Some(name);
        } else if line.contains("resident()") {
            who.insert(within.clone().expect("拿表的那一行在某个函数里"));
        }
    }
    assert_eq!(
        who,
        ["find_indexed", "search_counting"].map(String::from).into(),
        "拿常驻表的不止两问的入口：没人问也会建"
    );
}

/// ★ 有界：上界只够留几份时，每一问的答案与不设上界逐问逐行相等；常驻的字节不超上界、留下的恰是按最近优先装得下的那几份。
#[test]
fn fix_a_bounded_index_answers_like_an_unbounded_one_and_keeps_the_most_recent() {
    let home = build_corpus("bound");
    let fence = Fence::at(&projects_root(&home)).expect("围栏");
    let mut free = SearchIndex::with_budget(usize::MAX);
    let (want, _) = ask_all(&mut free, &home);
    let order: Vec<(PathBuf, usize)> = session_files(&fence)
        .into_iter()
        .filter_map(|(p, _)| free.files.get(&p).map(|e| (p, e.weight)))
        .collect();
    // 上界 = 最近两份之和 ⇒ 至少丢一份（反空真），最近那份一定留。
    let budget = order[0].1 + order[1].1;
    let mut expect_kept = BTreeMap::new();
    let mut sum = 0usize;
    for (p, w) in &order {
        if sum + w <= budget {
            sum += w;
            expect_kept.insert(p.clone(), ());
        }
    }
    assert!(expect_kept.len() < order.len(), "上界没丢掉任何一份");
    let mut bounded = SearchIndex::with_budget(budget);
    for round in 0..2 {
        let (got, _) = ask_all(&mut bounded, &home);
        assert_eq!(got, want, "第 {round} 趟：有界索引的答案 ≠ 不设上界");
        assert!(
            bounded.kept() <= budget,
            "常驻 {} 超了上界 {budget}",
            bounded.kept()
        );
        let kept: Vec<&PathBuf> = bounded.files.keys().collect();
        assert_eq!(
            kept,
            expect_kept.keys().collect::<Vec<_>>(),
            "留下的不是按最近优先装得下的那几份"
        );
    }
    std::fs::remove_dir_all(&home).ok();
}

/// 〔perfC #5〕常驻索引存紧：一条既没有正文、也没有工具文本、也不开一轮的记录（不搜工具时的工具调用 / 工具结果）
/// 永远命中不了、也不进轮次 ⇒ 不进索引。一千条这样的记录不许撑大那一格的常驻字节；搜与会话内查找的答案照旧。
#[test]
fn records_that_can_never_hit_take_no_room_in_the_index() {
    let home = std::env::temp_dir().join(format!("ccm-sx-dead-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let f = p(&home, "-w-dead/dead.jsonl");
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    let mut text = line("u-0", "开头那一句 cursor") + "\n";
    for i in 0..1000 {
        text.push_str(
            &json!({"type":"user","uuid":format!("t-{i}"),"timestamp":"2026-04-01T00:00:00Z",
                "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":format!("x{i}"),"content":"ok"}]}})
            .to_string(),
        );
        text.push('\n');
    }
    text.push_str(&(line("u-1", "结尾那一句 cursor") + "\n"));
    std::fs::write(&f, &text).unwrap();
    let mut index = SearchIndex::default();
    let entry = bring_up(&f, None, false, &mut index.last).expect("读得了");
    assert!(
        entry.weight < 4096,
        "一千条命中不了的记录占了 {} 字节常驻",
        entry.weight
    );
    // 答案照旧：两条正文都命中、轮次照数（会话内查找）。
    let mut hits = Vec::new();
    let fence = crate::common::fence::Fence::at(&home.join("projects")).unwrap();
    let admitted = fence.admit(&f).unwrap();
    index.files.insert(admitted.clone(), entry);
    index
        .find(
            &admitted,
            "cursor",
            false,
            None,
            FindPage { skip: 0, limit: 10 },
            |v| {
                hits.push(v.uuid.clone());
                Ok(())
            },
        )
        .expect("归索引管")
        .expect("找得了");
    assert_eq!(hits, vec!["u-0".to_string(), "u-1".to_string()]);
    std::fs::remove_dir_all(&home).ok();
}

/// 〔perfC2〕正文与工具文本分两层留：上界装得下每一份的正文、装不下全部工具文本时，问过一次「含工具」之后，
/// 不含工具的那一问**一个字节都不读**（工具层先放掉，正文层不被挤出去）；两种问法的答案都与不设上界逐字相等，常驻不超上界。
#[test]
fn a_tools_question_does_not_push_plain_text_out_of_the_index() {
    let home = std::env::temp_dir().join(format!("ccm-sx-tiers-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let bulk = "npm test --watch ".repeat(240);
    for i in 0..6 {
        let f = p(&home, &format!("-w-tiers/s{i}.jsonl"));
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        let mut text = line(&format!("u-{i}"), &format!("第 {i} 份 cursor")) + "\n";
        text.push_str(
            &json!({"type":"assistant","uuid":format!("a-{i}"),"timestamp":"2026-04-01T00:00:01Z",
                "message":{"role":"assistant","content":[{"type":"tool_use","id":format!("t{i}"),"name":"Bash","input":{"command":bulk}}]}})
            .to_string(),
        );
        text.push('\n');
        std::fs::write(&f, &text).unwrap();
        touch(&f, 1_800_000_000_000 + i64::from(i) * 1_000);
    }
    let fence = Fence::at(&projects_root(&home)).expect("围栏");
    let live = crate::history::live::live_session_ids(&home);
    let ask = |index: &mut SearchIndex, q: &str, tools: bool| {
        let rest: Vec<String> = if tools {
            vec!["--include-tools".into()]
        } else {
            vec![]
        };
        let mut buf = Vec::new();
        index
            .search(
                &fence,
                q,
                &opts_from_words(&rest),
                &live,
                &mut lines_into(&mut buf),
            )
            .expect("search ok");
        String::from_utf8(buf).expect("UTF-8")
    };
    let mut free_plain = SearchIndex::with_budget(usize::MAX);
    let want_plain = ask(&mut free_plain, "cursor", false);
    let mut free_tools = SearchIndex::with_budget(usize::MAX);
    let want_tools = ask(&mut free_tools, "npm test", true);
    let (plain, all) = (free_plain.kept(), free_tools.kept());
    assert!(
        all > plain * 3,
        "工具文本不够大，判不出挤占（正文 {plain} · 全部 {all}）"
    );
    let budget = plain + (all - plain) / 3;
    let mut bounded = SearchIndex::with_budget(budget);
    for round in 0..2 {
        assert_eq!(
            ask(&mut bounded, "npm test", true),
            want_tools,
            "第 {round} 趟：含工具的答案变了"
        );
        assert!(
            bounded.kept() <= budget,
            "常驻 {} 超了上界 {budget}",
            bounded.kept()
        );
        assert_eq!(
            ask(&mut bounded, "cursor", false),
            want_plain,
            "第 {round} 趟：不含工具的答案变了"
        );
        assert_eq!(
            (bounded.last.full, bounded.last.appended, bounded.last.bytes),
            (0, 0, 0),
            "第 {round} 趟：问过含工具之后，不含工具的那一问还在读盘（正文层被工具层挤出去了）"
        );
    }
    std::fs::remove_dir_all(&home).ok();
}

/// 〔perfC2〕几批、几条线程带出来的答案，行序仍是最近优先（跨批也不乱）：七十份、每份一条命中、修改时刻各不同。
#[test]
fn many_batches_still_answer_most_recent_first() {
    let home = std::env::temp_dir().join(format!("ccm-sx-batches-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let n = SEARCH_BATCH * 2 + 6;
    for i in 0..n {
        let f = p(&home, &format!("-w-b{}/s{i:03}.jsonl", i % 7));
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(
            &f,
            line(&format!("u-{i}"), &format!("cursor 第 {i} 份")) + "\n",
        )
        .unwrap();
        // 名字越大越旧：最近优先的序与名字序、目录序都不同。
        touch(&f, 1_800_000_000_000 - (i as i64) * 1_000);
    }
    let fence = Fence::at(&projects_root(&home)).expect("围栏");
    let mut buf = Vec::new();
    SearchIndex::default()
        .search(
            &fence,
            "cursor",
            &opts_from_words(&[]),
            &crate::history::live::live_session_ids(&home),
            &mut lines_into(&mut buf),
        )
        .expect("search ok");
    let got: Vec<String> = String::from_utf8(buf)
        .unwrap()
        .lines()
        .map(|l| {
            let v = serde_json::from_str::<Value>(l).unwrap();
            format!(
                "{} {}",
                v["sessionId"].as_str().unwrap(),
                v["title"].as_str().unwrap()
            )
        })
        .collect();
    // 标题取自那一份自己的第一句 ⇒ 线程交回的序乱了、读出来的那一格与它的路径对不上就红。
    let want: Vec<String> = (0..n)
        .map(|i| format!("s{i:03} cursor 第 {i} 份"))
        .collect();
    assert_eq!(got, want);
    std::fs::remove_dir_all(&home).ok();
}

/// 〔perfC4〕几条线程 ＝ min(8, 这台机器可用的并行数)，至少一条；问不出可用数 ⇒ 一条（远端可能是一两核的小机器，不按 8 条起）。
#[test]
fn par_workers_is_min_of_eight_and_the_machine_and_at_least_one() {
    use crate::common::par::par_workers;
    let got: Vec<(Option<usize>, usize)> = [
        None,
        Some(0),
        Some(1),
        Some(2),
        Some(3),
        Some(8),
        Some(9),
        Some(64),
    ]
    .into_iter()
    .map(|a| (a, par_workers(a)))
    .collect();
    assert_eq!(
        got,
        vec![
            (None, 1),
            (Some(0), 1),
            (Some(1), 1),
            (Some(2), 2),
            (Some(3), 3),
            (Some(8), 8),
            (Some(9), 8),
            (Some(64), 8),
        ]
    );
}

/// 〔perfC4〕几条线程只在一处算：后端生产段里问机器并行数的只有 `common/par.rs` 那一处（[`crate::common::par::par_workers`]），
/// 别处要分线程走 `par_in_order`，不自己再问一遍、再定一个上限。
#[test]
fn the_worker_count_is_asked_in_one_place() {
    let root = guard_core::addr::backend_src_root();
    let files = guard_core::scan_tree_excluding(&root, &["rs"], &[]);
    assert!(
        files.len() >= 100,
        "只扫到 {} 份 `.rs` —— 走树坏了，本条在空转",
        files.len()
    );
    let hits: Vec<(String, usize)> = files
        .iter()
        .map(|(p, src)| {
            let n = guard_core::production_code(src)
                .matches("available_parallelism")
                .count();
            (
                p.strip_prefix(&root)
                    .unwrap_or(p)
                    .display()
                    .to_string()
                    .replace('\\', "/"),
                n,
            )
        })
        .filter(|(_, n)| *n > 0)
        .collect();
    assert_eq!(
        hits,
        vec![("common/par.rs".to_string(), 1)],
        "问机器并行数只许在 `common/par.rs::par_workers` 一处（别处分线程走 `par_in_order`）"
    );
}

/// 〔perfC2〕分给几条线程的那一套（历史清单与全文搜索共用）：结果按原序交回；某一件 panic 原样抛出来，不悄悄少一件。
#[test]
fn par_in_order_keeps_the_order_and_does_not_swallow_a_panic() {
    let items: Vec<usize> = (0..500).collect();
    let got = crate::common::par::par_in_order(items.clone(), |i| {
        if i % 37 == 0 {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        i * 3
    });
    assert_eq!(got, items.iter().map(|i| i * 3).collect::<Vec<_>>());
    let caught = std::panic::catch_unwind(|| {
        crate::common::par::par_in_order((0..64).collect::<Vec<usize>>(), |i| {
            assert!(i != 40, "第 40 件坏了");
            i
        })
    });
    assert!(caught.is_err(), "有一件 panic 却交回了结果");
}

/// 〔perfC2〕常驻上界钉在 128 MB：口径是「多常驻 ≤ 200 MB 换每问 ≤ 1 s」，按 `drive.mjs --only search` 在 680 MB 合成世界量
/// （进程 CPU 毫秒，不按墙钟）—— 正文整份装得下的最小一档；含工具那一形 128 / 256 都装不下、调大只多常驻不变快。要改它先重量这一组读数。
#[test]
fn the_resident_bound_is_128_mb() {
    assert_eq!(RESIDENT_MAX_BYTES, 128 << 20);
}

/// d：整份读按块 —— 第一次带一份 8 MiB 的记录（整份那一臂），读的时候本线程手里最多两块多一点，不是整份文件。
#[test]
fn a_whole_read_holds_about_a_block_not_the_file() {
    use crate::common::append_read::test_support::{big_record, BIG, HELD_BOUND};
    let dir = std::env::temp_dir().join(format!("ccm-sx-held-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let path = dir.join("big.jsonl");
    let users = big_record(&path, BIG, false);
    let base = crate::alloc_probe::reset_peak();
    let got = {
        let mut last = Refresh::default();
        let e = bring_up(&path, None, false, &mut last).expect("读得动");
        (e.done.records.len(), last.full, last.bytes)
    };
    let held = crate::alloc_probe::peak_since(base);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(got.0, users, "用户话该一句不少地进索引");
    assert_eq!(got.1, 1, "走的该是整份那一臂");
    assert!(got.2 >= BIG as u64, "该把整份都读了（读了 {} 字节）", got.2);
    assert!(
        held <= HELD_BOUND,
        "整份读一份 {BIG} 字节的记录，本线程手里最多 {held} 字节，上界 {HELD_BOUND}（两块 ＋ 一行）"
    );
}

/// d：追加读按块 —— 两问之间长了 8 MiB，读的时候本线程手里同样只有两块多一点。
#[test]
fn an_appended_read_holds_about_a_block_not_the_growth() {
    use crate::common::append_read::test_support::{big_record, BIG, HELD_BOUND};
    let dir = std::env::temp_dir().join(format!("ccm-sx-held-app-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let path = dir.join("grow.jsonl");
    std::fs::write(&path, format!("{}\n", line("a-0", "first"))).expect("写");
    let mut last = Refresh::default();
    let first = bring_up(&path, None, false, &mut last).expect("读得动");
    let grow = dir.join("more.jsonl");
    let users = big_record(&grow, BIG, false);
    append(&path, &std::fs::read(&grow).expect("读夹具"));
    touch(&path, 1_780_000_000_000);
    let base = crate::alloc_probe::reset_peak();
    let got = {
        let mut last = Refresh::default();
        let e = bring_up(&path, Some(first), false, &mut last).expect("读得动");
        (e.done.records.len(), last.appended)
    };
    let held = crate::alloc_probe::peak_since(base);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(got, (users + 1, 1), "该走追加那一臂、新长的用户话一句不少");
    assert!(
        held <= HELD_BOUND,
        "追加读 {BIG} 字节，本线程手里最多 {held} 字节，上界 {HELD_BOUND}"
    );
}

/// 一格索引的全部事实（判「按块读 == 一整块读」用）。
fn facts_of(e: &FileEntry) -> String {
    let f = |x: &Facts| {
        let recs: Vec<String> = x
            .records
            .iter()
            .map(|r| {
                format!(
                    "{}|{}|{:?}|{:?}|{:?}|{}|{}|{}",
                    r.rt.is_assistant,
                    r.rt.report,
                    r.rt.main,
                    r.rt.tool,
                    r.rt.lands_in,
                    r.ts_ms,
                    r.uuid,
                    r.opens_turn
                )
            })
            .collect();
        format!(
            "cwd={:?} bg={} title={:?} excerpt={:?} recs={recs:?}",
            x.cwd, x.bg, x.title, x.excerpt
        )
    };
    format!(
        "done[{}] tail[{}] bad={:?} tools={} weight={} plain={} consumed={} witness={}",
        f(&e.done),
        f(&e.tail),
        e.bad.as_ref().map(|(at, why)| (*at, why.clone())),
        e.tools,
        e.weight,
        e.plain_weight,
        e.read.consumed(),
        e.read.witness_len()
    )
}

/// d：同一份记录按 1 · 7 · 64 · 4096 字节一块读，与一整块读出来的那一格逐个相等（含 CRLF · BOM · 跨块的多字节字 ·
/// 标题在后半 · 坏 UTF-8 在中段 / 在残尾 · 残尾半行）；工具文本抽与不抽两样都比。
#[test]
fn reading_in_blocks_gives_the_same_entry_as_one_block() {
    let user = |uuid: &str, text: &str| line(uuid, text);
    let ai = |t: &str| json!({"type":"ai-title","aiTitle":t}).to_string();
    let tool = json!({"type":"assistant","uuid":"a-1","timestamp":"2026-04-01T00:00:01Z","cwd":"/sandbox/p",
        "message":{"role":"assistant","content":[{"type":"text","text":"答：中文跨块"},
        {"type":"tool_use","id":"x1","name":"Bash","input":{"command":"echo 中文"}}]}})
    .to_string();
    let bg = json!({"type":"user","uuid":"u-bg","sessionKind":"bg","message":{"role":"user","content":"later"}}).to_string();
    let good = format!(
        "{}\r\n\u{feff}{}\n\n{}\n{}\n{}\n{}",
        user("u-1", "第一句：中文"),
        tool,
        ai("标题甲"),
        bg,
        user("u-2", "第二句"),
        r#"{"type":"user","uuid":"u-3","message":{"role":"user","content":"半"#
    )
    .into_bytes();
    let mut bad_mid = good.clone();
    let at = bad_mid.iter().position(|&b| b == b'\n').unwrap() + 3;
    bad_mid.insert(at, 0xff);
    let mut bad_tail = good.clone();
    bad_tail.push(0xff);
    for (name, bytes) in [
        ("好", &good),
        ("坏在中段", &bad_mid),
        ("坏在残尾", &bad_tail),
    ] {
        for tools in [false, true] {
            let one = {
                let mut e = FileEntry::empty(tools);
                e.take(None, &mut Blocks::new(&bytes[..], 1 << 20))
                    .expect("读得动");
                facts_of(&e)
            };
            for block in [1usize, 7, 64, 4096] {
                let mut e = FileEntry::empty(tools);
                e.take(None, &mut Blocks::new(&bytes[..], block))
                    .expect("读得动");
                assert_eq!(
                    facts_of(&e),
                    one,
                    "[{name} · 工具 {tools} · 块 {block}] 与一整块读的不一样"
                );
            }
        }
    }
    // 正控：后半那一段里的 `bg` 并进来了（完整行里的才算数：今天的金样只有它在残尾里那一形）。
    let mut e = FileEntry::empty(false);
    e.take(None, &mut Blocks::new(&good[..], 7))
        .expect("读得动");
    assert!(e.done.bg, "`sessionKind:\"bg\"` 在一整行里，并段之后丢了");
    // 正控：坏的那一句的位置按整段算。
    let mut e = FileEntry::empty(false);
    e.take(None, &mut Blocks::new(&bad_mid[..], 7))
        .expect("读得动");
    assert_eq!(
        e.bad.map(|(_, why)| why),
        Some(
            std::str::from_utf8(&bad_mid[..bad_mid.iter().rposition(|&b| b == b'\n').unwrap() + 1])
                .unwrap_err()
                .to_string()
        ),
        "坏在中段那一句与整段一次判的不一样"
    );
}
