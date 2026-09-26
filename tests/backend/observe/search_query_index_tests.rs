//! 〔SX1〕J2：一串变更之后，热索引（增量读）与新建索引（整份读）逐问逐行相等（两向）· J3：真的是增量 ——
//! 经帧面那一臂的入口（`search_into`）连问，读盘字节数 == 按变更独立算出的数（没变 ⇒ 0）。
//! 守的要求：题面「按文件 mtime / 长度增量」「结果与现扫逐条相等（两向）」；J1（金样）钉「整份读 == 起步树现扫」。

use super::golden_tests::{build_corpus, cases};
use super::*;
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
                    &parse_opts(&rest),
                    &mut buf,
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
        search_into(&home, "docker", &[], &mut out).expect("search ok");
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
    let witness = before.min(WITNESS_BYTES as u64);
    let grew = Refresh {
        full: 0,
        appended: 1,
        reused: n - 1,
        bytes: witness + add.len() as u64,
    };
    assert_eq!(last(), grew, "追加 n 字节 ⇒ 只读见证 ＋ n");
    std::fs::remove_dir_all(&home).ok();
}
