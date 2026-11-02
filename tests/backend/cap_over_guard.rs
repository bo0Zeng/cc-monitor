//! 读上限超了怎么办（`Cap::over`）的两条行为对拍：声明写的那一档，调用处真那么做了。
//!
//! 人群是生产段里类型为 `Cap` 的常量（`const NAME: Cap = Cap { bytes: …, over: Over::… }`），按 `over` 那一档取：
//! - [`a_hard_error_cap_is_not_swallowed_at_its_call_site`]：`Over::HardError` 的调用处把错传上去（`?`）。
//! - [`every_over_limit_arm_of_a_drop_and_report_cap_says_so`]：`Over::DropAndReport` 的每一处超限臂都说话（`warn!`），
//!   且至少一处把它抬到用户那一层（原位 `Event::Gap`）。
//!
//! 别的几档（截断＋说清 · 拒收＋回错 · 跳过＋说清 · 降级＋说清 · 索引截断 · 分块）各有各的正当处置形态，一条规则套不下，
//! 不在这里假装覆盖；那几处各靠站点自己的行为测试。
//!
//! 两条都先对一份写坏的合成语料跑（[`hard_error_scan_sees_a_swallowed_call_site`] ·
//! [`drop_and_report_scan_sees_a_silent_arm`]），认得出坏的，再对真树跑。

use std::path::PathBuf;

/// 扫哪几棵树（相对仓根）：后端 · 共享包 · 通信包 · 壳 · 文件窗口。
const TREES: &[&str] = &[
    "src/backend",
    "src/common",
    "src/comms",
    "src/frontend/shell/src",
    "src/frontend/filewin/src",
];

/// 从一处提及往下看几行（启发式）：臂特别长 ⇒ 假红；紧邻的别的语句里恰好有针 ⇒ 假绿。
const STATEMENT_WINDOW: usize = 10;
/// 超限臂体看几行（同上，启发式）。
const ARM_WINDOW: usize = 12;

/// 一份语料：`(相对仓根的路径, 生产段代码)`（注释与测试段已剥）。
type Corpus = Vec<(String, String)>;

fn real_tree() -> Corpus {
    let root: PathBuf = guard_core::addr::repo_root();
    let mut out = Vec::new();
    for sub in TREES {
        for (f, raw) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, guard_core::production_code(&raw)));
        }
    }
    out.sort();
    out
}

/// 一个 `Cap` 常量的声明：住哪份 · 名字 · `over` 那一档（变体名）。
#[derive(Debug, PartialEq, Eq)]
struct CapDecl {
    file: String,
    name: String,
    over: String,
}

/// 抠出语料里每一个 `Cap` 常量与它的 `over`。类型写 `Cap` 或带路径（`relay_route_core::Cap`）都认。
fn cap_decls(corpus: &Corpus) -> Vec<CapDecl> {
    let mut out = Vec::new();
    for (file, code) in corpus {
        let lines: Vec<&str> = code.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let t = l.trim();
            let Some(rest) = t
                .strip_prefix("pub(crate) const ")
                .or_else(|| t.strip_prefix("pub const "))
                .or_else(|| t.strip_prefix("const "))
            else {
                continue;
            };
            let Some((name, tail)) = rest.split_once(':') else {
                continue;
            };
            let Some((ty, _)) = tail.split_once('=') else {
                continue;
            };
            let ty = ty.trim();
            if ty != "Cap" && !ty.ends_with("::Cap") {
                continue;
            }
            // `over:` 在声明体里（到 `};` 为止）。
            let end = lines[i..]
                .iter()
                .position(|l| l.trim_end().ends_with("};"))
                .map_or(lines.len(), |p| i + p + 1);
            let over = lines[i..end]
                .iter()
                .find_map(|l| {
                    let at = l.find("Over::")?;
                    Some(
                        l[at + "Over::".len()..]
                            .trim_end()
                            .trim_end_matches([',', '}', ';', ' '])
                            .to_string(),
                    )
                })
                .unwrap_or_default();
            out.push(CapDecl {
                file: file.clone(),
                name: name.trim().to_string(),
                over,
            });
        }
    }
    out
}

/// 一行里提到这个常量，而且不只是在报错文字里引它的数（`NAME.bytes`）。
fn uses_cap(line: &str, name: &str) -> bool {
    if !guard_core::contains_word(line, name) {
        return false;
    }
    guard_core::contains_word(&line.replace(&format!("{name}.bytes"), ""), name)
}

/// 一条语句把错传上去了没有：得有正向的一样 —— `?` · `return Err` · 造一个 `Err(…)` 值（不是 `Err(…) =>` 那种模式）·
/// `.map_err(`（还是 `Result`，作尾表达式交回去）；而且不许有把错变没的那两种（`.ok()` · `unwrap_or…`）。
///
/// 只认吞掉的拼法不够（把 `Err(_)` 写成 `Err(e)` 就绕过去了），所以先要正向的一样；只认 `?` 又太窄：
/// 尾表达式交回 `Result`、`match` 臂里 `return Err(…)` 都是传上去。
fn passes_the_error_up(scope: &str) -> bool {
    let builds_err = scope.lines().any(|l| {
        l.match_indices("Err(")
            .any(|(at, _)| !l[at..].contains("=>"))
    });
    let up = scope.contains('?')
        || scope.contains("return Err")
        || builds_err
        || scope.contains(".map_err(");
    up && !scope.contains(".ok()") && !scope.contains("unwrap_or")
}

/// 从第 `i` 行起的这条语句收尾在哪（不含的那一行下标）：花括号与圆括号都回到不深于起点那一层、且该行以 `;` 收尾；
/// 或者起点所在的块先被一个 `}` 合上（尾表达式）。至多 [`STATEMENT_WINDOW`] 行。
/// 圆括号低于起点是常量写在一次调用的实参里（`fetch(…, CAP,\n)?`），语句还没完。
/// 只按行尾认 `;` 会停在 `match` 臂里的头一条语句上，看不见臂里交回的 `Err(…)`。
fn statement_end(lines: &[&str], i: usize) -> usize {
    let (mut brace, mut paren): (i64, i64) = (0, 0);
    let hi = (i + STATEMENT_WINDOW).min(lines.len());
    for (k, l) in lines.iter().enumerate().take(hi).skip(i) {
        for c in l.chars() {
            match c {
                '{' => brace += 1,
                '}' => brace -= 1,
                '(' | '[' => paren += 1,
                ')' | ']' => paren -= 1,
                _ => {}
            }
        }
        if brace < 0 || (brace <= 0 && paren <= 0 && l.trim_end().ends_with(';')) {
            return k + 1;
        }
    }
    hi
}

/// `Over::HardError` 的每一处调用：从提及那一行扫到这条语句收尾（[`statement_end`]），
/// 看它把错传上去了没有（[`passes_the_error_up`]）。回 `(看过的调用处数, 没传上去的那几处)`；
/// 一个常量在它住的那份里一处调用都没看见也算在后者里。
fn hard_error_swallows(corpus: &Corpus) -> (usize, Vec<String>) {
    let mut checked = 0usize;
    let mut bad = Vec::new();
    for d in cap_decls(corpus).iter().filter(|d| d.over == "HardError") {
        let Some((_, code)) = corpus.iter().find(|(f, _)| *f == d.file) else {
            continue;
        };
        let lines: Vec<&str> = code.lines().collect();
        let mut seen = 0usize;
        for (i, l) in lines.iter().enumerate() {
            if l.contains("const ") || !uses_cap(l, &d.name) {
                continue;
            }
            seen += 1;
            let scope = lines[i..statement_end(&lines, i)].join("\n");
            if !passes_the_error_up(&scope) {
                bad.push(format!(
                    "  {}:{} 用 {}（`Over::HardError`）却没把错传上去",
                    d.file,
                    i + 1,
                    d.name
                ));
            }
        }
        if seen == 0 {
            bad.push(format!(
                "  {}::{}（`Over::HardError`）在它住的这份里一处调用都没看见 —— 抽取器认不出调用处，本条对它是空转的",
                d.file, d.name
            ));
        }
        checked += seen;
    }
    (checked, bad)
}

/// `Over::DropAndReport` 的超限臂：用到这个常量的每一份里，每一处 `CappedLine::TooLong … =>` 臂。
/// 回 `(臂数, 什么都没说的臂, 抬到用户那一层的臂数)`。
///
/// 取样单位是处置臂本身，不是常量的每一处提及：上限在有界读处 · 报错文字 · 消费点都会被提到，报告只在处置臂里。
/// 同时要求 `=>`：`CappedLine::TooLong` 也出现在构造处（有界读里 `return Ok(CappedLine::TooLong(seen))`），那不是处置。
fn drop_and_report_arms(corpus: &Corpus) -> (usize, Vec<String>, usize) {
    let mut arms = 0usize;
    let mut silent = Vec::new();
    let mut reported = 0usize;
    for d in cap_decls(corpus)
        .iter()
        .filter(|d| d.over == "DropAndReport")
    {
        for (file, code) in corpus {
            let lines: Vec<&str> = code.lines().collect();
            if *file != d.file
                && !lines
                    .iter()
                    .any(|l| !l.contains("const ") && guard_core::contains_word(l, &d.name))
            {
                continue;
            }
            for (i, l) in lines.iter().enumerate() {
                if !l.contains("CappedLine::TooLong") || !l.contains("=>") {
                    continue;
                }
                arms += 1;
                let body = lines[i..(i + ARM_WINDOW).min(lines.len())].join("\n");
                if !body.contains("warn!") {
                    silent.push(format!("  {file}:{}（{}）", i + 1, d.name));
                }
                if guard_core::contains_word(&body, "Event::Gap") {
                    reported += 1;
                }
            }
        }
    }
    (arms, silent, reported)
}

fn corpus(files: &[(&str, &str)]) -> Corpus {
    files
        .iter()
        .map(|(f, s)| (f.to_string(), s.to_string()))
        .collect()
}

#[test]
fn cap_decls_reads_the_over_of_every_cap_const() {
    let c = corpus(&[(
        "a.rs",
        "pub(crate) const A_CAP: Cap = Cap {\n    bytes: 1 << 20,\n    over: Over::HardError,\n};\n\
         const B: relay_route_core::Cap = relay_route_core::Cap {\n    bytes: 4096,\n    over: relay_route_core::Over::DropAndReport,\n};\n\
         const N: usize = 4096;\n",
    )]);
    let got: Vec<(String, String)> = cap_decls(&c)
        .into_iter()
        .map(|d| (d.name, d.over))
        .collect();
    assert_eq!(
        got,
        vec![
            ("A_CAP".to_string(), "HardError".to_string()),
            ("B".to_string(), "DropAndReport".to_string()),
        ]
    );
}

#[test]
fn hard_error_scan_sees_a_swallowed_call_site() {
    let decl = "const A_CAP: Cap = Cap {\n    bytes: 1 << 20,\n    over: Over::HardError,\n};\n";
    let swallowed = corpus(&[(
        "a.rs",
        &format!("{decl}fn f(p: &Path) -> Option<Vec<u8>> {{\n    Ro::read_capped(p, A_CAP).ok()\n}}\n\
                  fn g(p: &Path) -> Vec<u8> {{\n    let b = Ro::read_capped(p, A_CAP)\n        .unwrap_or_default();\n    b\n}}\n"),
    )]);
    let (checked, bad) = hard_error_swallows(&swallowed);
    assert_eq!(checked, 2);
    assert_eq!(bad.len(), 2, "{bad:#?}");

    let matched_away = corpus(&[(
        "a.rs",
        &format!("{decl}fn f(p: &Path) {{\n    match Ro::read_capped(p, A_CAP) {{\n        Ok(b) => use_it(b),\n        Err(e) => {{}}\n    }}\n}}\n"),
    )]);
    assert_eq!(
        hard_error_swallows(&matched_away).1.len(),
        1,
        "`Err(e) => {{}}` 是吞"
    );

    let tail_and_return = corpus(&[(
        "a.rs",
        &format!("{decl}fn f(p: &Path) -> Result<String, String> {{\n    Ro::read_string_capped(p, A_CAP)\n        .map_err(|e| e.to_string())\n}}\n\
                  fn g(p: &Path) -> Result<(), String> {{\n    match Ro::read_capped(p, A_CAP) {{\n        Ok(b) => use_it(b),\n        Err(e) => return Err(e.to_string()),\n    }}\n    Ok(())\n}}\n\
                  fn h() -> io::Result<()> {{\n    let a = fetch(\n        url,\n        A_CAP.as_len(),\n    )\n    .context(\"x\")?;\n    Ok(())\n}}\n"),
    )]);
    let (checked, bad) = hard_error_swallows(&tail_and_return);
    assert_eq!(checked, 3, "实参里的 `A_CAP.as_len()` 也是一处调用");
    assert!(
        bad.is_empty(),
        "尾表达式交回 `Result` · 臂里 `return Err` 都是传上去：{bad:#?}"
    );

    let passed_up = corpus(&[(
        "a.rs",
        &format!("{decl}fn f(p: &Path) -> io::Result<Vec<u8>> {{\n    let b = Ro::read_capped(p, A_CAP)\n        .map_err(wrap)?;\n    \
                  Err(other(format!(\"over {{}}\", A_CAP.bytes)))\n}}\n"),
    )]);
    let (checked, bad) = hard_error_swallows(&passed_up);
    assert_eq!(checked, 1, "报错文字里引 `A_CAP.bytes` 不算一处调用");
    assert!(bad.is_empty(), "{bad:#?}");

    let unused = corpus(&[("a.rs", decl)]);
    assert_eq!(
        hard_error_swallows(&unused).1.len(),
        1,
        "一处调用都没看见要说出来"
    );
}

#[test]
fn drop_and_report_scan_sees_a_silent_arm() {
    let decl =
        "pub const L_CAP: Cap = Cap {\n    bytes: 64 << 20,\n    over: Over::DropAndReport,\n};\n";
    let c = corpus(&[
        ("line.rs", decl),
        (
            "feed.rs",
            "fn run() {\n    match read_capped_line(&mut r, &mut b, L_CAP).await {\n        \
             Ok(CappedLine::TooLong(n)) => {\n            tracing::warn!(\"{n}\");\n            (events)(Event::Gap).await;\n        }\n        \
             _ => {}\n    }\n}\n",
        ),
        (
            "host.rs",
            "fn run() {\n    match read_capped_line(&mut r, &mut b, L_CAP).await {\n        \
             Ok(CappedLine::TooLong(_)) => continue,\n        _ => {}\n    }\n}\n",
        ),
        (
            "other.rs",
            "fn x() { match y { Ok(CappedLine::TooLong(_)) => {} _ => {} } }\n",
        ),
    ]);
    let (arms, silent, reported) = drop_and_report_arms(&c);
    assert_eq!(arms, 2, "不用这个上限的那份（other.rs）不进人群");
    assert_eq!(silent.len(), 1, "{silent:#?}");
    assert!(silent[0].contains("host.rs"), "{silent:#?}");
    assert_eq!(reported, 1);

    let no_gap = corpus(&[
        ("line.rs", decl),
        (
            "feed.rs",
            "fn run() {\n    match read_capped_line(&mut r, &mut b, L_CAP).await {\n        \
             Ok(CappedLine::TooLong(n)) => {\n            tracing::warn!(\"{n}\");\n        }\n        _ => {}\n    }\n}\n",
        ),
    ]);
    assert_eq!(drop_and_report_arms(&no_gap).2, 0);
}

/// 登记 `Over::HardError` 的上限，调用处不许把错吞掉。
#[test]
fn a_hard_error_cap_is_not_swallowed_at_its_call_site() {
    let tree = real_tree();
    assert!(
        cap_decls(&tree).iter().any(|d| d.over == "HardError"),
        "真树里一个 `Over::HardError` 的 `Cap` 都没认出来 —— 抽取器坏了，本条此刻是空转的"
    );
    let (checked, bad) = hard_error_swallows(&tree);
    assert!(checked > 0, "一处调用都没看见 —— 抽取器坏了");
    assert!(
        bad.is_empty(),
        "声明 `over: Over::HardError` 的上限，调用处把错吞了：\n{}\n\n\
         两条路：把调用处改成真的传上去（`?`）；或者把 `over` 改成实话 —— 真实处置是跳过就写 `SkipAndSay` 并 `warn!` 说是哪一份，\
         整份降级就写 `DegradeAndSay` 并把错带进结果。",
        bad.join("\n")
    );
}

/// 登记 `Over::DropAndReport` 的上限：每一处超限臂都说话，至少一处说到用户那一层（原位 `Gap`）。
#[test]
fn every_over_limit_arm_of_a_drop_and_report_cap_says_so() {
    let tree = real_tree();
    let (arms, silent, reported) = drop_and_report_arms(&tree);
    assert!(
        arms > 0,
        "一处超限处置臂都没找到 —— 抽取器坏了，本条此刻是空转的"
    );
    assert!(
        silent.is_empty(),
        "这些超限处置臂什么都没说：\n{}\n\n丢一行不说就是静默失败：用户看到的是「这条会话少了一行」且无从得知为什么。",
        silent.join("\n")
    );
    assert!(
        reported >= 1,
        "没有任何一处超限把话说到用户那一层（原位 `Event::Gap`）。那是「丢弃＋带身份报告」与「跳过＋说清」的分界 —— \
         只写 `warn!` 的话 `over` 就该是 `SkipAndSay`。"
    );
}
