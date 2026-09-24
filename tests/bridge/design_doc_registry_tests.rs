use std::collections::BTreeSet;
use std::path::PathBuf;

/// 设计篇那一族住在**仓外**（`work/调研/`，而仓根是 `cc-monitor/`）。
///
/// 🔴 `调研/` 是**给定的**（理由与代价住生产侧模块头注）⇒ 找不到就 panic，**不跳过**。
fn design_dir() -> PathBuf {
    let d = repo_root().join("../调研/设计");
    assert!(
        d.is_dir(),
        "找不到设计篇那一族（期望在 {}）—— `调研/` 是给定的，\
         缺席不是一种要伺候的常态，它是坏了。正确的处置是把 `调研/` 带上，\
         不是给本条开豁免（理由住 `design_doc_registry` 的头注）。",
        d.display()
    );
    d
}

fn index_text() -> String {
    let p = repo_root().join("../调研/README.md");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不动那份索引（{}）：{e}", p.display()))
}

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`。
    crate::guard_support::repo_root()
}

/// 「整篇作废」这个标记在索引里长什么样。**运行期拼**，免得本文件自己成为一处命中。
fn dead_mark() -> String {
    format!("🚫 {}", "整篇作废")
}

/// 🔴 比之前先剥掉 markdown 的强调符。
///
/// **它不是宽容，是判对了维度**：本条判的是「标了没标作废」，而 `**加粗**` 是排版。
/// 现打的活样本：同一份索引里 `50` 那一行写的是「🚫 整篇作废」、`61` 那一行写的是
/// 「🚫 **整篇作废**」—— 两行是同一个意思，而不剥星号的话第二行会被判成「没标」。
/// ⚠ 那一红**是真的**（索引里两处拼法不一致），但它红在**排版**上、不在**语义**上
/// ⇒ 按「判对维度」修判据，不是去改索引迁就判据。
fn strip_emphasis(s: &str) -> String {
    s.replace('*', "")
}

/// 索引里那张表：`(篇名, 那一行的状态格)`。
///
/// 抠法用共享原语 [`guard_core::md_table_rows`]（通用 markdown 表格抠法，
/// 它替我们丢掉分隔行与首尾那对 `|` 切出来的空格子 —— 两个都是踩过的坑）。
fn index_rows() -> Vec<(String, String)> {
    let t = index_text();
    let mut out = Vec::new();
    for cells in guard_core::md_table_rows(&t) {
        if cells.len() < 3 {
            continue;
        }
        // 第一格形如 `` `设计/60-xxx.md` `` —— 只收这一形，别的表不会误入。
        let f = cells[0].trim_matches('`');
        if let Some(name) = f.strip_prefix("设计/") {
            if name.ends_with(".md") {
                out.push((name.to_string(), cells[1].clone()));
            }
        }
    }
    out
}

fn docs_on_disk() -> BTreeSet<String> {
    // 🔴 走共享遍历原语，不裸 `read_dir` —— `scanning_guard_registry` 那条棘轮逐字要求的，
    //    理由也是它写的：「判据在自己的登记表/注释/常量里找到自己 ⇒ **恒绿**，
    //    audit-0805 实测五次，五次都不是被判据变红发现的」⇒ 修法是**让它写不出来**。
    //
    // ⚠ 排除名单**刻意是空的**，而那不是偷懒：本判据住 `tests/bridge/`，被扫的树是
    //    仓外的 `调研/设计/` ⇒ **结构上不可能在语料里找到自己**。空名单在这个原语里
    //    是合法的（它自己的头注逐字：「名单为空时退化成『不排除』是合法的：调用方就是要整棵树」），
    //    而**空串**才是它拒绝的那一形（空串命中一切 ⇒ 静默空集）。
    guard_core::scan_tree_excluding(&design_dir(), &["md"], &[])
        .into_iter()
        .map(|(p, _)| {
            p.file_name()
                .and_then(|s| s.to_str())
                .expect("文件名不是 UTF-8")
                .to_string()
        })
        .collect()
}

/// 那一篇**自己**声明了整篇作废吗 —— 只看抬头那一段（前 12 行），
/// 因为篇中提到「某一节作废」不等于整篇作废。
///
/// 🔴 〔D0b 09-24〕抬头这一侧**也先剥强调符**，与索引那一侧（[`strip_emphasis`] 的头注）同一个理由：
/// 判的是「声明了没有」，`**加粗**` 是排版。现打逼出来的：第四波 D0 对账把 `50`／`61` 的抬头
/// 改写成「🚫 **整篇作废 —— 已结单。**」—— 两篇照旧自称整篇作废，而不剥星号时
/// `every_doc_marked_dead_in_the_index_really_declares_itself_dead` 判它们「抬头没有那句声明」。
/// 那一红在**排版**上、不在**语义**上 ⇒ 修判据的维度，不去改文档迁就判据。
fn declares_itself_dead(name: &str) -> bool {
    let p = design_dir().join(name);
    let s = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不动 {}：{e}", p.display()));
    let head: String = s.lines().take(12).collect::<Vec<_>>().join("\n");
    let head = strip_emphasis(&head);
    guard_core::contains_word(&head, "已作废") || head.contains(&dead_mark())
}

#[test]
fn the_extractor_really_sees_the_status_column() {
    let rows = index_rows();
    assert!(
        rows.len() >= 20,
        "只从索引里抠出 {} 行 —— **抽取器坏了**，下面那几条会拿两个几乎空的集合比出绿。\
         （索引那张表的形状变了？第一格不再是 `` `设计/NN-…md` `` 了？）",
        rows.len()
    );
    // 状态栏必须真的有内容，否则「标没标作废」这一维恒假。
    let with_status = rows.iter().filter(|(_, s)| !s.is_empty()).count();
    assert_eq!(
        with_status,
        rows.len(),
        "有 {} 行的状态格是空的 —— 那一维在这些行上**判不了**，不许当成「没标作废」",
        rows.len() - with_status
    );
    // 合成对照：识别器认得出一段**故意**标了作废的文本。
    let synthetic = format!("| `设计/00-x.md` | {} | 随便 |", dead_mark());
    let got = guard_core::md_table_rows(&synthetic);
    assert_eq!(got.len(), 1, "通用抠法认不出一行合成的表格行 —— 尺子没跑");
    assert!(
        strip_emphasis(&got[0][1]).contains(&dead_mark()),
        "抠出来的状态格里没有那个标记 —— 下面两条此刻在空转"
    );
}

#[test]
fn the_index_lists_exactly_the_design_docs_on_disk() {
    let listed: BTreeSet<String> = index_rows().into_iter().map(|(f, _)| f).collect();
    let disk = docs_on_disk();
    let missing: Vec<&String> = disk.difference(&listed).collect();
    let phantom: Vec<&String> = listed.difference(&disk).collect();
    assert!(
        missing.is_empty() && phantom.is_empty(),
        "索引与盘面对不上（两向）：\n  盘上有而索引没列：{missing:?}\n  索引列了而盘上没有：{phantom:?}\n\
         ⇒ 索引腐掉比正文腐掉更贵：正文腐了读者还能读出矛盾，**索引腐了他根本不会去读那一篇**。"
    );
}

#[test]
fn every_doc_marked_dead_in_the_index_really_declares_itself_dead() {
    let bad: Vec<String> = index_rows()
        .into_iter()
        .filter(|(_, s)| strip_emphasis(s).contains(&dead_mark()))
        .filter(|(f, _)| !declares_itself_dead(f))
        .map(|(f, _)| f)
        .collect();
    assert!(
        bad.is_empty(),
        "索引把这几篇标成整篇作废，而那几篇**自己的抬头没有那句声明**：{bad:?}\n\
         ⇒ 两条出路：① 索引标错了 ⇒ 改索引；② 那篇真作废了 ⇒ 在它抬头写下来。\
         **不许靠删掉索引那一行了事** —— 删掉的是线索。"
    );
}

#[test]
fn every_doc_that_declares_itself_dead_is_marked_dead_in_the_index() {
    let rows = index_rows();
    let bad: Vec<String> = docs_on_disk()
        .into_iter()
        .filter(|f| declares_itself_dead(f))
        .filter(|f| {
            !rows
                .iter()
                .any(|(n, s)| n == f && strip_emphasis(s).contains(&dead_mark()))
        })
        .collect();
    assert!(
        bad.is_empty(),
        "这几篇**自己的抬头就写着作废**，而索引没把它们标成整篇作废：{bad:?}\n\
         🔴 **这一条才是治那个病的那一半** —— 2026-09-23 现打的活样本：一篇死文档的正文\
         从第一行起就喊着「我作废了」，而索引把它标成「已定」，\
         于是任何按索引挑活的人都会去读它。"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 〔D0b · 第四波 2026-09-24〕**「⚠ 部分作废」那一档接上执行链**
//
// 设计与读数住 `调研/第四波记录/D0b.md`（主会话并入设计篇之前的唯一住址）。
// 下面分两半：**纯函数的判官**（吃文本、不碰盘，合成夹具在它上面验牙）与
// **对真 `调研/` 的三条**（其中恰好一条 `#[ignore]`，解锁条件写在它自己头上）。
// ─────────────────────────────────────────────────────────────────────────────

/// 索引状态栏的三档。**按状态格剥掉强调之后的首个符号**分档 —— 后缀
/// （「活 · 在建」「活 · 唯一口径」）是说明，不是档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tier {
    Live,
    Partial,
    Dead,
}

fn tier_of(status: &str) -> Option<Tier> {
    let s = strip_emphasis(status);
    let s = s.trim_start();
    if s.starts_with('✅') {
        Some(Tier::Live)
    } else if s.starts_with('⚠') {
        Some(Tier::Partial)
    } else if s.starts_with('🚫') {
        Some(Tier::Dead)
    } else {
        None
    }
}

/// 块首行上的**保留字**。在标题行与引用块首行里，它们只许用来标作废／订正块。
///
/// 🔴 「腐」是题面三词之外加的第四个 —— 现打逼出来的：`设计/97 §0` 那块真正的作废块
/// 首行写的是「……大面积腐」，三词一个都不含；只用三词时那篇是**碰巧**绿的
/// （数到的是后文一句**提到**订正块的话）。索引自己给 ⚠ 标理由也用这个词（「已腐」）。
///
/// 主会话若不要它，删这一格后 `97` 仍绿（碰巧的那一块还在），但那就是回到碰巧。
const VOID_WORDS: [&str; 4] = ["作废", "推翻", "订正", "腐"];

/// 去掉引用前缀之后是不是 ATX 标题。单独成一个返回 `bool` 的小函数：
/// 它认的是 markdown 的 `#`，不是注释（`structural_scan` 那张「剥注释转换器」登记表
/// 管的是返回 `String`／`Vec` 的剥法，这里不是那一族）。
fn is_heading(body: &str) -> bool {
    body.starts_with('#')
}

/// 剥掉引用前缀（`>`，可嵌套、可带缩进）。返回 `(是不是引用行, 剥完的正文)`。
fn unquote(line: &str) -> (bool, &str) {
    let mut s = line.trim_start();
    let quoted = s.starts_with('>');
    while let Some(rest) = s.strip_prefix('>') {
        s = rest.trim_start();
    }
    (quoted, s)
}

/// 行内代码剥掉：反引号里的词是在**提**它，不是在**用**它。
fn visible_text(body: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for c in body.chars() {
        if c == '`' {
            in_code = !in_code;
            continue;
        }
        if !in_code {
            out.push(c);
        }
    }
    out
}

/// 一篇里的**作废块**：`(行号从 1 起, 那一行)`。
///
/// 块首行 ＝ ATX 标题行（含 `> # …`）或一段引用的**第一行**；围栏代码块里的一律不算。
/// 可见文字（剥掉行内代码）里含 [`VOID_WORDS`] 之一 ⇒ 这一行是一块作废块的首行。
fn void_block_lines(doc: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut prev_quoted = false;
    for (i, line) in doc.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
            prev_quoted = false;
            continue;
        }
        if in_fence {
            continue;
        }
        let (quoted, body) = unquote(line);
        let opener = is_heading(body) || (quoted && !prev_quoted);
        prev_quoted = quoted;
        if !opener {
            continue;
        }
        let vis = visible_text(body);
        if VOID_WORDS.iter().any(|w| vis.contains(w)) {
            out.push((i + 1, line.to_string()));
        }
    }
    out
}

/// 判官的裁决：三个违例集。全空 ⇔ 三档与正文一致。
#[derive(Debug, Default, PartialEq, Eq)]
struct TierVerdict {
    /// 状态格落不进三档之一 ⇒ 那一行**判不了**，不许默认当成哪一档。
    unclassified: BTreeSet<String>,
    /// 索引标 ⚠ 而篇内一块作废块都没有。
    partial_without_block: BTreeSet<String>,
    /// 索引标 ✅ 而篇内有作废块。
    live_with_block: BTreeSet<String>,
}

/// 纯函数：吃索引行与「按篇名取正文」，吐裁决。🚫 那一档两向都不看
/// （归上面「整篇作废」那两条管）。
fn judge_tiers(rows: &[(String, String)], read: impl Fn(&str) -> String) -> TierVerdict {
    let mut v = TierVerdict::default();
    for (name, status) in rows {
        match tier_of(status) {
            None => {
                v.unclassified.insert(name.clone());
            }
            Some(Tier::Dead) => {}
            Some(Tier::Partial) => {
                if void_block_lines(&read(name)).is_empty() {
                    v.partial_without_block.insert(name.clone());
                }
            }
            Some(Tier::Live) => {
                if !void_block_lines(&read(name)).is_empty() {
                    v.live_with_block.insert(name.clone());
                }
            }
        }
    }
    v
}

fn read_design_doc(name: &str) -> String {
    let p = design_dir().join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不动 {}：{e}", p.display()))
}

fn set(xs: &[&str]) -> BTreeSet<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

/// 合成语料上的识别器：**块行号集两向相等**，五种「不算块」的形逐一在场。
#[test]
fn the_void_block_recognizer_sees_exactly_the_marked_openers() {
    let doc = [
        "# 设计 X",                       // 1 标题，无保留字
        "",                               // 2
        "> ⚠ **状态订正**：本篇部分作废", // 3 ✔ 引用首行
        "> 续行里再说一次作废，不算新块", // 4 ✘ 引用续行
        "",                               // 5
        "## 3. 〔订正 2026-09-18〕某节",  // 6 ✔ 标题
        "正文段落里写推翻不算块。",       // 7 ✘ 正文
        "### 提到 `作废` 这个词的标题",   // 8 ✘ 保留字只在行内代码里
        "```text",                        // 9 围栏开
        "# 作废（围栏里的标题）",         // 10 ✘ 围栏里
        "> 推翻（围栏里的引用）",         // 11 ✘ 围栏里
        "```",                            // 12 围栏关
        "> # 🚫 本节三句结论全部作废",    // 13 ✔ 引用里的标题 ＋ 引用首行
        "> # ⚠ 本节那条已被推翻",         // 14 ✔ 引用续行，但它是标题
        "",                               // 15
        "  > 缩进的引用首行：这个数腐了", // 16 ✔ 缩进引用首行 ＋ 第四个保留字
        "- 列表项里写订正不算块",         // 17 ✘ 列表
    ]
    .join("\n");
    let got: BTreeSet<usize> = void_block_lines(&doc).into_iter().map(|(n, _)| n).collect();
    let want: BTreeSet<usize> = [3, 6, 13, 14, 16].into_iter().collect();
    assert_eq!(
        got, want,
        "识别器认出的块首行 ≠ 夹具标定的那几行（两向）。\n\
         ✔ 该认：引用首行 · 标题 · 引用里的标题 · 缩进引用首行 · 第四个保留字；\n\
         ✘ 不该认：引用续行 · 正文 · 行内代码里的词 · 围栏里的标题与引用 · 列表项"
    );
}

/// 合成语料上的判官：五篇各占一格，**三个违例集逐个相等**。
#[test]
fn the_partial_tier_judge_has_teeth_on_a_synthetic_corpus() {
    let rows: Vec<(String, String)> = [
        ("a-live-clean.md", "✅ 活"),
        ("b-live-with-block.md", "✅ 活 · **在建**"),
        ("c-partial-with-block.md", "⚠ 部分作废"),
        ("d-partial-without-block.md", "⚠ **部分作废**"),
        ("e-dead.md", "🚫 **整篇作废**"),
        ("f-no-tier.md", "已定"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    let read = |name: &str| -> String {
        match name {
            "a-live-clean.md" => "# A\n\n正文里提一句推翻，不在块首行。\n".into(),
            "b-live-with-block.md" => {
                "# B\n\n> 🔴 **〔订正 2026-09-24〕** 上面那句原是全称\n".into()
            }
            "c-partial-with-block.md" => "# C\n\n### 4.8 🚫 本节三句结论全部作废\n".into(),
            "d-partial-without-block.md" => "# D\n\n> 只是一段普通引用\n".into(),
            // 🚫 那一档两向都不看：有块、没块都不许进任何违例集。
            "e-dead.md" => "# 〔已作废〕E\n\n> # 🚫 整篇作废\n".into(),
            "f-no-tier.md" => "# F\n".into(),
            other => panic!("夹具外的篇名：{other}"),
        }
    };
    let got = judge_tiers(&rows, read);
    let want = TierVerdict {
        unclassified: set(&["f-no-tier.md"]),
        partial_without_block: set(&["d-partial-without-block.md"]),
        live_with_block: set(&["b-live-with-block.md"]),
    };
    assert_eq!(
        got, want,
        "判官在合成夹具上的裁决 ≠ 标定（三个违例集逐个相等）—— 判官坏了，下面对真 `调研/` 的几条在空转"
    );
}

/// 真 `调研/`：索引每一行都落进三档之一。
#[test]
fn every_index_row_falls_into_exactly_one_tier() {
    let rows = index_rows();
    // 正控：真索引里三档**各自非空**。某一档空了 ⇒ 要么抠法坏了，要么那一档的判词在空转。
    for (t, what) in [
        (Tier::Live, "✅ 活"),
        (Tier::Partial, "⚠ 部分作废"),
        (Tier::Dead, "🚫 整篇作废"),
    ] {
        assert!(
            rows.iter().any(|(_, s)| tier_of(s) == Some(t)),
            "真索引里一行「{what}」都没认出 —— 分档器或抠法坏了（那一档的判词此刻在空转）"
        );
    }
    let v = judge_tiers(&rows, |_| String::new());
    assert!(
        v.unclassified.is_empty(),
        "这几行的状态格落不进 ✅／⚠／🚫 任一档：{:?}\n⇒ 那一行**判不了**。按首个符号分档，别换写法。",
        v.unclassified
    );
}

/// 真 `调研/`：三档与正文**两向**一致 —— 标 ⚠ 的篇篇内必须真有作废块，标 ✅ 的篇篇内不许有。
///
/// 🔴 **本仓唯一一条为「等别人做完」而 `#[ignore]` 的判据**（第四波 D0b 题面授权的那一条）。
///
/// **为什么现在不上链**：`调研/设计/` 此刻正被第四波 D0 那几路全面对账（改抬头、删订正块），
/// 两个方向都在它们手里：设计时（15:5x）现打 ✅ 那一向红 8 篇、⚠ 那一向零漏；
/// 一刻钟后（16:0x）再打，⚠ 那一向红 8 篇 —— 对账把那几篇抬头的「状态订正」块换成了新抬头，
/// 而索引还没跟着改。**两次都是对账半途的真不一致**，但此刻上链只会在它们改到一半时红。
/// 判官本身已在 `the_partial_tier_judge_has_teeth_on_a_synthetic_corpus` 上验过牙。
/// 读数与契约住 `调研/第四波记录/D0b.md`。
///
/// **解锁条件**：D0 对账（含索引状态栏）合进主线之后，主会话跑
/// `cargo test -p monitor --lib design_doc_registry -- --ignored`，红集为空 ⇒
/// 删掉下面那行 `#[ignore]`，并同拍删 `shared_crate_registry_tests.rs` 里 `MANUAL`
/// 登记的这一行（那张表的自检会逼着删：「已经不是 `#[ignore]` 测试了」）。
#[test]
#[ignore = "D0b：等第四波 D0 对账完 `调研/设计/` 由主会话摘；解锁条件见本条文档注释"]
fn the_real_index_tiers_agree_with_the_void_blocks_both_ways() {
    let rows = index_rows();
    let v = judge_tiers(&rows, read_design_doc);
    let detail: Vec<String> = v
        .live_with_block
        .iter()
        .map(|n| {
            let lines: Vec<String> = void_block_lines(&read_design_doc(n))
                .into_iter()
                .map(|(i, l)| format!("    第 {i} 行：{}", l.chars().take(80).collect::<String>()))
                .collect();
            format!("  {n}\n{}", lines.join("\n"))
        })
        .collect();
    assert!(
        v.partial_without_block.is_empty() && v.live_with_block.is_empty(),
        "索引的三档与正文对不上（两向）。\n\
         「作废块」＝ 标题行或引用块首行里出现「作废／推翻／订正／腐」之一\
         （契约与读数见 `调研/第四波记录/D0b.md`）。\n\n\
         ■ 标 ⚠ 而篇内一块作废块都没有：{:?}\n\
           ⇒ ① 那几节真死了 ⇒ 在那一节就地标一块（块首行用保留字）；② 其实没有哪一节死 ⇒ 索引改回 ✅。\n\n\
         ■ 标 ✅ 而篇内有作废块：\n{}\n\
           ⇒ ① 订正已并进正文 ⇒ 删掉那块，或把块首行改成不含保留字；② 那一节真死了 ⇒ 索引改标 ⚠。\n\n\
         ⚠ **不许给识别器加例外** —— 保留字契约就是「这四个词在块首行只用来标作废」。",
        v.partial_without_block,
        detail.join("\n")
    );
}
