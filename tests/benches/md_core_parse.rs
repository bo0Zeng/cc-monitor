//! md-core 的量具：一页 600 段造的助手正文，量两样 —— 整页解析（不带高亮），与一段高亮的冷 · 热
//! （冷：缓存不命中、语法正则已编好；热：同一页再拉一遍，全命中）。
//!
//! 正文是造的（不放真正文）：长度照真记录量过的分布（中位 172 · p90 1 401 · p99 3 664 字节），两成带一个代码块
//! （rust · typescript · bash · python 轮换），其余是中英夹杂的句子，带加粗 · 行内代码 · 链接 · 列表 · 标题。
//!
//! - `cargo bench --manifest-path src/common/md-core/Cargo.toml --bench md_core_parse` ⇒ 读数表（墙钟，不当判据）。
//! - `cargo test`（同一个目标，cargo 不传 `--bench`）⇒ 只造一小页跑一遍，证明量具能跑、每段都解析出了东西。

use md_core::{parse, Doc, Highlighter};
use std::time::{Duration, Instant};

/// 线性同余：造的语料每次一样。
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn unit(&mut self) -> f64 {
        self.next() as f64 / (1u64 << 31) as f64
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.next() as usize % xs.len()]
    }
}

/// 分位点（概率, 字节）之间按对数插值。
fn length(r: &mut Lcg) -> usize {
    const Q: [(f64, f64); 5] = [
        (0.0, 12.0),
        (0.5, 172.0),
        (0.9, 1401.0),
        (0.99, 3664.0),
        (1.0, 9000.0),
    ];
    let u = r.unit().min(0.999_999);
    let i = Q.windows(2).position(|w| u < w[1].0).unwrap();
    let (p0, l0) = Q[i];
    let (p1, l1) = Q[i + 1];
    let t = (u - p0) / (p1 - p0);
    (l0.ln() + t * (l1.ln() - l0.ln())).exp() as usize
}

const SENTENCES: &[&str] = &[
    "这一步先读配置，再起子进程。",
    "改动只落在 `src/lib.rs` 一处，别的文件不动。",
    "**结论**：缓存命中时不必再读盘。",
    "The parser returns early when the input is empty.",
    "详见 [说明](https://example.com/docs/a) 第二节。",
    "数组长度是 `n`，循环跑 *n − 1* 次。",
    "跑完测试全绿，耗时约 3 秒。",
    "Then we merge the two branches and rerun the check.",
    "注意 ~~旧的那个开关~~ 已经删了。",
    "参数 `--manifest-path` 指向那份清单。",
];
const BULLETS: &[&str] = &[
    "- 先看日志\n",
    "- 再看配置\n",
    "1. 编译\n",
    "2. 运行\n",
    "- [ ] 待补的一项\n",
];
const CODES: &[(&str, &str)] = &[
    ("rust", "fn add(a: i32, b: i32) -> i32 {\n    // 两数相加\n    a + b\n}\n"),
    ("ts", "export function pick<T>(xs: T[], i: number): T | undefined {\n  return xs[i]; // 越界给 undefined\n}\n"),
    ("sh", "for f in *.log; do\n  grep -c \"error\" \"$f\" # 数错误\ndone\n"),
    ("python", "def mean(xs):\n    \"\"\"平均数\"\"\"\n    return sum(xs) / len(xs)\n"),
];

fn reply(r: &mut Lcg, i: usize) -> String {
    let want = length(r);
    let mut out = String::new();
    let with_code = i.is_multiple_of(5);
    if with_code {
        let (lang, body) = CODES[r.next() as usize % CODES.len()];
        let mut code = String::new();
        while code.len() < want * 2 / 5 || code.is_empty() {
            code.push_str(body);
        }
        out.push_str(r.pick(SENTENCES));
        out.push_str("\n\n```");
        out.push_str(lang);
        out.push('\n');
        out.push_str(&code);
        out.push_str("```\n\n");
    }
    while out.len() < want {
        match r.next() % 10 {
            0 => {
                out.push_str("\n\n");
                out.push_str(r.pick(BULLETS));
                out.push_str(r.pick(BULLETS));
                out.push('\n');
            }
            1 => out.push_str("\n\n## 小结\n\n"),
            2 => out.push_str("\n\n"),
            _ => out.push_str(r.pick(SENTENCES)),
        }
    }
    out
}

fn page(n: usize) -> Vec<String> {
    let mut r = Lcg(0x6d64_636f_7265);
    (0..n).map(|i| reply(&mut r, i)).collect()
}

fn pct(xs: &[Duration], p: f64) -> Duration {
    let mut v = xs.to_vec();
    v.sort();
    v[((v.len() - 1) as f64 * p).round() as usize]
}

fn us(d: Duration) -> f64 {
    d.as_secs_f64() * 1e6
}
fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn print_dist(what: &str, xs: &[Duration]) {
    println!(
        "{what}：中位 {:.1} µs · p90 {:.1} µs · p99 {:.1} µs · 最大 {:.1} µs",
        us(pct(xs, 0.5)),
        us(pct(xs, 0.9)),
        us(pct(xs, 0.99)),
        us(pct(xs, 1.0))
    );
}

fn print_pages(what: &str, xs: &[Duration]) {
    println!(
        "{what}（{} 趟）：中位 {:.2} ms · 最快 {:.2} ms · 最慢 {:.2} ms",
        xs.len(),
        ms(pct(xs, 0.5)),
        ms(pct(xs, 0.0)),
        ms(pct(xs, 1.0))
    );
}

/// 一页的每一段都拉一遍：每段一个读数，连同整页合计。
fn pull_page(h: &mut Highlighter, docs: &[Doc], per_seg: &mut Vec<Duration>) -> Duration {
    let page_t = Instant::now();
    for d in docs {
        for i in 0..=d.segs.len() {
            let t = Instant::now();
            std::hint::black_box(h.seg(d, i));
            per_seg.push(t.elapsed());
        }
    }
    page_t.elapsed()
}

fn report() {
    let replies = page(600);
    let bytes: usize = replies.iter().map(String::len).sum();
    let lens: Vec<Duration> = replies
        .iter()
        .map(|s| Duration::from_nanos(s.len() as u64))
        .collect();
    println!(
        "语料：600 段 · {bytes} 字节 · 每段长度 中位 {} · p90 {} · p99 {} 字节 · 带代码 {} 段",
        pct(&lens, 0.5).as_nanos(),
        pct(&lens, 0.9).as_nanos(),
        pct(&lens, 0.99).as_nanos(),
        replies.iter().filter(|s| s.contains("```")).count()
    );

    // 整页解析（不带高亮）。
    let mut per_reply = Vec::new();
    let mut per_page = Vec::new();
    let mut docs = Vec::new();
    for _ in 0..7 {
        docs.clear();
        let page_t = Instant::now();
        for s in &replies {
            let t = Instant::now();
            docs.push(std::hint::black_box(parse(s)));
            per_reply.push(t.elapsed());
        }
        per_page.push(page_t.elapsed());
    }
    print_dist("解析 每段", &per_reply);
    print_pages("解析 每页", &per_page);

    // 一段高亮。头一趟：新的高亮器（载语法表 ＋ 现编正则）。
    let t = Instant::now();
    let mut first = Highlighter::new(0);
    println!("载语法表：{:.1} ms", ms(t.elapsed()));
    let mut seg_first = Vec::new();
    let page_first = pull_page(&mut first, &docs, &mut seg_first);
    println!(
        "高亮 头一趟（现编正则 · 全不命中）：每页 {:.2} ms",
        ms(page_first)
    );

    // 冷：上限 0 ⇒ 一条都不存，每次都算；正则已在头一趟编好。
    let mut seg_cold = Vec::new();
    let page_cold: Vec<Duration> = (0..7)
        .map(|_| pull_page(&mut first, &docs, &mut seg_cold))
        .collect();
    print_dist("高亮 冷 每段", &seg_cold);
    print_pages("高亮 冷 每页", &page_cold);

    // 热：上限够大，先拉一遍填满，再量。
    let mut hot = Highlighter::new(64 << 20);
    pull_page(&mut hot, &docs, &mut Vec::new());
    let filled = hot.computed();
    let mut seg_hot = Vec::new();
    let page_hot: Vec<Duration> = (0..7)
        .map(|_| pull_page(&mut hot, &docs, &mut seg_hot))
        .collect();
    assert_eq!(hot.computed(), filled, "热的那几趟又算了");
    print_dist("高亮 热 每段", &seg_hot);
    print_pages("高亮 热 每页", &page_hot);
    println!(
        "缓存：{} 条真算过 · 占 {:.1} KB",
        filled,
        hot.bytes() as f64 / 1024.0
    );
}

fn smoke() {
    let replies = page(30);
    assert!(
        replies.iter().any(|s| s.contains("```")),
        "造的语料里没有代码块"
    );
    let mut h = Highlighter::new(1 << 20);
    let mut lit = 0;
    for s in &replies {
        let doc = parse(s);
        assert!(!doc.blocks.is_empty(), "一段没解析出块：{s:?}");
        for i in 0..=doc.segs.len() {
            lit += h.seg(&doc, i).iter().filter(|hl| !hl.is_empty()).count();
        }
    }
    assert!(lit > 0, "带代码的段一块高亮都没出");
    println!("md_core_parse smoke：30 段都解析出了块，{lit} 块出了高亮");
}

/// 高亮各语言的快慢：每种一块约 8 KB 的代码，热过一遍再量 5 趟取中位（上限 0，每趟都真算）。
fn per_lang() {
    let mut h = Highlighter::new(0);
    for (lang, body) in CODES {
        let mut code = String::new();
        while code.len() < 8_000 {
            code.push_str(body);
        }
        let doc = parse(&format!("```{lang}\n{code}```\n"));
        std::hint::black_box(h.seg(&doc, 0));
        let mut ts: Vec<Duration> = (0..5)
            .map(|_| {
                let t = Instant::now();
                std::hint::black_box(h.seg(&doc, 0));
                t.elapsed()
            })
            .collect();
        ts.sort();
        let d = ts[2];
        println!(
            "高亮 {lang:<6} {} 字节：{:.2} ms（{:.2} MB/s）",
            code.len(),
            ms(d),
            code.len() as f64 / d.as_secs_f64() / 1e6
        );
    }
}

fn main() {
    if std::env::args().any(|a| a == "--bench") {
        report();
        per_lang();
    } else {
        smoke();
    }
}
