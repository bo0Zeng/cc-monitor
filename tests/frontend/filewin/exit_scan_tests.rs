//! **出口零判定零格式化**的扫描判据，Rust 那一半（壳 · 文件窗口）：与 `tests/frontend/ui/exit-rules.ts` 同样的两句——
//! - `key`：出口取的文案键（`copy_text("…", …)`）只许骨架那几档（文案表 `kind` 的 title · control · action · aria · body；
//!   error 是结局、由后端交成品），且那一条不带插值（没登 `args`）；键不是字面量、表里没有这把键，也算一处。
//! - `branch`：条件读了线上值的分支里不许取文。「读了线上值」＝ 条件拿一个值去比字面的码：`x == "dup"` 一类的比较、
//!   `matches!(x, "a" | …)`、`CODES.contains(…)`（`CODES` 是本文件里一组字面码的 `const` / `static`），
//!   或者 `match` 的臂里有字面码（那个 `match` 的每一臂都算）。管到的是 `if` / `else` 的块与 `match` 的臂；进到 `fn` 里就不算，
//!   闭包跟着外面的分支算（同 TS 的箭头函数、Kotlin 的 lambda）。
//!
//! 扫哪几棵：欠账名单 `tests/frontend/ui/exit-debt.json` 里 `lang: "rs"` 的 `roots`；生产段照 `guard_core::production_code` 剥。
//! 名单是三门语言共用的一份：这里只核 Rust 那几格，名单自身与合计由 `exit-judgment-scan.vitest.ts` 核。
//!
//! 买不到：比的码不是字面量就认不出；`&&` / `||` 右侧取文不算分支；
//! 分词是手写的（字 · 原样字 · 字符字面量认得），不是语法树。
//! 什么时候删：名单里 Rust 那几格数到 0 ⇒ 主断言改成零命中、删只判名单的那几行。

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

const SKELETON: &[&str] = &["title", "control", "action", "aria", "body"];

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
struct Hits {
    key: usize,
    branch: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Punct(&'static str),
    Other,
}

const PUNCTS: &[&str] = &[
    "==", "!=", "=>", "&&", "||", "::", "{", "}", "(", ")", "[", "]", ";", ",", "|", "!", "=", "&",
    ".", ":",
];

fn lex(src: &str) -> Vec<Tok> {
    let b: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == 'r'
            && i + 1 < b.len()
            && (b[i + 1] == '"'
                || (b[i + 1] == '#' && b.get(i + 2).is_some_and(|&x| x == '"' || x == '#')))
        {
            // 原样字符串 r"…" / r#"…"#
            let mut j = i + 1;
            let mut hashes = 0;
            while b[j] == '#' {
                hashes += 1;
                j += 1;
            }
            j += 1;
            let start = j;
            loop {
                if j >= b.len() {
                    break;
                }
                if b[j] == '"' && (0..hashes).all(|h| b.get(j + 1 + h) == Some(&'#')) {
                    break;
                }
                j += 1;
            }
            out.push(Tok::Str(b[start..j.min(b.len())].iter().collect()));
            i = j + 1 + hashes;
        } else if c == '"' {
            let mut j = i + 1;
            let mut s = String::new();
            while j < b.len() && b[j] != '"' {
                if b[j] == '\\' && j + 1 < b.len() {
                    s.push(b[j]);
                    j += 1;
                }
                s.push(b[j]);
                j += 1;
            }
            out.push(Tok::Str(s));
            i = j + 1;
        } else if c == '\'' {
            // 字符字面量 'x' · '\n' · '\u{..}'；否则是生命周期 'a
            if b.get(i + 1) == Some(&'\\') {
                let mut j = i + 2;
                while j < b.len() && b[j] != '\'' {
                    j += 1;
                }
                i = j + 1;
            } else if b.get(i + 2) == Some(&'\'') {
                i += 3;
            } else {
                i += 1;
            }
            out.push(Tok::Other);
        } else if c.is_alphanumeric() || c == '_' {
            let mut j = i;
            while j < b.len() && (b[j].is_alphanumeric() || b[j] == '_') {
                j += 1;
            }
            out.push(Tok::Ident(b[i..j].iter().collect()));
            i = j;
        } else {
            let two: String = b[i..(i + 2).min(b.len())].iter().collect();
            if let Some(p) = PUNCTS.iter().find(|p| p.len() == 2 && **p == two) {
                out.push(Tok::Punct(p));
                i += 2;
            } else if let Some(p) = PUNCTS.iter().find(|p| p.len() == 1 && p.starts_with(c)) {
                out.push(Tok::Punct(p));
                i += 1;
            } else {
                out.push(Tok::Other);
                i += 1;
            }
        }
    }
    out
}

fn is_p(t: Option<&Tok>, p: &str) -> bool {
    matches!(t, Some(Tok::Punct(x)) if *x == p)
}

fn is_id(t: Option<&Tok>, s: &str) -> bool {
    matches!(t, Some(Tok::Ident(x)) if x == s)
}

/// 本文件里一组字面码的 `const` / `static`：`const CODES: &[&str] = &["a", "b"];`。
fn code_lists(t: &[Tok]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for i in 0..t.len() {
        if !(is_id(t.get(i), "const") || is_id(t.get(i), "static")) {
            continue;
        }
        let Some(Tok::Ident(name)) = t.get(i + 1) else {
            continue;
        };
        let Some(eq) = (i..t.len())
            .take_while(|&j| !is_p(t.get(j), ";"))
            .find(|&j| is_p(t.get(j), "="))
        else {
            continue;
        };
        let mut j = eq + 1;
        while is_p(t.get(j), "&") {
            j += 1;
        }
        if !is_p(t.get(j), "[") {
            continue;
        }
        let mut k = j + 1;
        let mut n = 0;
        let mut ok = true;
        while !is_p(t.get(k), "]") {
            match t.get(k) {
                Some(Tok::Str(_)) => n += 1,
                Some(Tok::Punct(",")) => {}
                _ => {
                    ok = false;
                    break;
                }
            }
            k += 1;
        }
        if ok && n > 0 {
            out.insert(name.clone());
        }
    }
    out
}

/// 一段条件（词）里拿一个值去比字面的码。
fn reads_code(c: &[Tok], lists: &BTreeSet<String>) -> bool {
    let s = |j: usize| matches!(c.get(j), Some(Tok::Str(_)));
    for j in 0..c.len() {
        if is_p(c.get(j), "==") || is_p(c.get(j), "!=") {
            let l = j > 0 && s(j - 1);
            let r = s(j + 1);
            if l != r {
                return true;
            }
        }
        if is_id(c.get(j), "matches")
            && is_p(c.get(j + 1), "!")
            && c[j..]
                .iter()
                .take_while(|t| !is_p(Some(t), ";"))
                .any(|t| matches!(t, Tok::Str(_)))
        {
            return true;
        }
        if is_id(c.get(j), "contains") && j >= 2 && is_p(c.get(j - 1), ".") {
            if let Some(Tok::Ident(n)) = c.get(j - 2) {
                if lists.contains(n) {
                    return true;
                }
            }
            if is_p(c.get(j - 2), "]") {
                return true;
            }
        }
    }
    false
}

#[derive(Clone, Copy, PartialEq)]
enum Frame {
    /// 函数体：往外不再找分支。
    Fn,
    /// 按码判的那一块（`if` / `else` 的块，或 `match` 的臂都在里面）。
    OnCode,
    Plain,
}

/// 从下标 `open`（一个 `{`）往回读它的头：到同一层的 `;` `{` `}` `=>` `,` `=` 为止（跳过成对的括号）。
fn header(t: &[Tok], open: usize) -> (usize, Vec<Tok>) {
    let mut j = open;
    let mut depth = 0i32;
    while j > 0 {
        let p = &t[j - 1];
        match p {
            Tok::Punct(")") | Tok::Punct("]") => depth += 1,
            Tok::Punct("(") | Tok::Punct("[") => {
                if depth == 0 {
                    break;
                }
                depth -= 1
            }
            Tok::Punct(";")
            | Tok::Punct("{")
            | Tok::Punct("}")
            | Tok::Punct("=>")
            | Tok::Punct(",")
            | Tok::Punct("=")
                if depth == 0 =>
            {
                break
            }
            _ => {}
        }
        j -= 1;
    }
    (j, t[j..open].to_vec())
}

/// `match` 的身体（`open` 是它的 `{`）里，同一层有没有字面码的臂。
fn match_on_code(t: &[Tok], open: usize) -> bool {
    let mut depth = 0i32;
    let mut k = open + 1;
    while k < t.len() {
        match &t[k] {
            Tok::Punct("{") | Tok::Punct("(") | Tok::Punct("[") => depth += 1,
            Tok::Punct("}") | Tok::Punct(")") | Tok::Punct("]") => {
                if depth == 0 {
                    return false;
                }
                depth -= 1
            }
            Tok::Str(_) if depth == 0 && (is_p(t.get(k + 1), "=>") || is_p(t.get(k + 1), "|")) => {
                return true
            }
            _ => {}
        }
        k += 1;
    }
    false
}

/// 一份 Rust 生产代码里两句各命中几处。
fn scan_rs(code: &str, table: &serde_json::Map<String, Value>) -> Hits {
    let t = lex(code);
    let lists = code_lists(&t);
    let mut hits = Hits::default();
    let mut stack: Vec<Frame> = Vec::new();
    // 一块 `if` 收尾的那个 `}` 的下标 ⇒ 那一串 `if` 有没有比码（后面的 `else` 照它判）。
    let mut if_close: BTreeMap<usize, bool> = BTreeMap::new();
    let mut open_kind: Vec<Option<bool>> = Vec::new(); // 每一层：是 if 链的块 ⇒ Some(这一串比没比码)
    for i in 0..t.len() {
        match &t[i] {
            Tok::Punct("{") => {
                let (start, h) = header(&t, i);
                let first = h.iter().position(|x| !matches!(x, Tok::Other));
                let head = |s: &str| first.is_some_and(|f| is_id(h.get(f), s));
                let (frame, chain) = if h.iter().any(|x| is_id(Some(x), "fn")) {
                    (Frame::Fn, None)
                } else if head("if") || (head("else") && h.iter().any(|x| is_id(Some(x), "if"))) {
                    let before = if head("else") {
                        start
                            .checked_sub(1)
                            .and_then(|p| if_close.get(&p).copied())
                            .unwrap_or(false)
                    } else {
                        false
                    };
                    let code = before || reads_code(&h, &lists);
                    (if code { Frame::OnCode } else { Frame::Plain }, Some(code))
                } else if head("else") {
                    let code = start
                        .checked_sub(1)
                        .and_then(|p| if_close.get(&p).copied())
                        .unwrap_or(false);
                    (if code { Frame::OnCode } else { Frame::Plain }, Some(code))
                } else if head("match") {
                    (
                        if match_on_code(&t, i) {
                            Frame::OnCode
                        } else {
                            Frame::Plain
                        },
                        None,
                    )
                } else {
                    (Frame::Plain, None)
                };
                stack.push(frame);
                open_kind.push(chain);
            }
            Tok::Punct("}") => {
                stack.pop();
                if let Some(Some(code)) = open_kind.pop() {
                    if_close.insert(i, code);
                }
            }
            Tok::Ident(n) if n == "copy_text" && is_p(t.get(i + 1), "(") => {
                let key = match (t.get(i + 2), t.get(i + 3)) {
                    (Some(Tok::Str(k)), Some(Tok::Punct(",")) | Some(Tok::Punct(")"))) => {
                        Some(k.as_str())
                    }
                    _ => None,
                };
                let skeleton = key.and_then(|k| table.get(k)).is_some_and(|e| {
                    SKELETON.contains(&e["kind"].as_str().unwrap_or(""))
                        && e.get("args")
                            .and_then(Value::as_array)
                            .is_none_or(|a| a.is_empty())
                });
                if !skeleton {
                    hits.key += 1;
                }
                if stack
                    .iter()
                    .rev()
                    .take_while(|f| **f != Frame::Fn)
                    .any(|f| *f == Frame::OnCode)
                {
                    hits.branch += 1;
                }
            }
            _ => {}
        }
    }
    hits
}

fn read_json(rel: &str) -> Value {
    let p = guard_core::addr::repo_root().join(rel);
    serde_json::from_str(
        &std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {p:?} 失败：{e}")),
    )
    .unwrap_or_else(|e| panic!("{rel} 不是 JSON：{e}"))
}

fn copy_table() -> serde_json::Map<String, Value> {
    read_json("src/shared/copy/table.json")["entries"]
        .as_object()
        .expect("文案表没有 entries")
        .clone()
}

fn sample_table() -> serde_json::Map<String, Value> {
    serde_json::from_str(r#"{"s.k.x":{"kind":"action"},"p.k.x":{"kind":"body"},"b.k.x":{"kind":"error"},"a.k.x":{"kind":"title","args":["n"]}}"#).unwrap()
}

#[test]
fn every_rust_exit_cell_equals_the_debt_ledger() {
    let debt = read_json("tests/frontend/ui/exit-debt.json");
    let table = copy_table();
    let roots: Vec<String> = debt["roots"]
        .as_array()
        .expect("roots")
        .iter()
        .filter(|r| r["lang"] == "rs")
        .map(|r| r["dir"].as_str().expect("dir").to_string())
        .collect();
    assert!(
        roots.len() >= 2,
        "名单里 Rust 的树只有 {roots:?} —— 壳与文件窗口都该在"
    );
    let mut got: BTreeMap<String, usize> = BTreeMap::new();
    for dir in &roots {
        let base = guard_core::addr::repo_root().join(dir);
        let files = guard_core::files_by_extension(&base, "rs");
        assert!(
            files.len() > 5,
            "{dir} 只扫到 {} 份 —— 遍历坏了，这一棵恒绿",
            files.len()
        );
        for f in files {
            let raw = std::fs::read_to_string(base.join(&f)).expect("读生产文件");
            let h = scan_rs(&guard_core::production_code(&raw), &table);
            let rel = format!("{dir}/{f}");
            if h.key > 0 {
                got.insert(format!("{rel} · key"), h.key);
            }
            if h.branch > 0 {
                got.insert(format!("{rel} · branch"), h.branch);
            }
        }
    }
    // 名单按 kind 分组，一格是 `[文件, 句, 命中数]`。
    let want: BTreeMap<String, usize> = debt["groups"]
        .as_array()
        .expect("groups")
        .iter()
        .flat_map(|g| g["cells"].as_array().expect("cells").iter())
        .filter_map(|c| {
            let (file, rule, n) = (c[0].as_str()?, c[1].as_str()?, c[2].as_u64()?);
            let ours = roots.iter().any(|d| file.starts_with(&format!("{d}/")));
            ours.then(|| (format!("{file} · {rule}"), n as usize))
        })
        .collect();
    let mut wrong = Vec::new();
    for (k, n) in &got {
        match want.get(k) {
            None => wrong.push(format!("{k}：命中 {n} 处，名单里没有 —— 出口新取了描述数据的字（交核心写好那一格，出口照摆）")),
            Some(w) if w != n => wrong.push(format!("{k}：命中 {n} 处，名单登 {w}")),
            _ => {}
        }
    }
    for (k, w) in &want {
        if !got.contains_key(k) {
            wrong.push(format!("{k}：一处都没有了，名单还登 {w} —— 删掉那一格"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn the_two_rules_hit_exactly_their_samples() {
    let table = sample_table();
    let cases: &[(&str, Hits)] = &[
        // 文件窗口取报错那一档的键：照写法正则扫时这一棵整个没扫。
        (
            r#"fn f() { let d = split_local(&l).ok_or_else(|| copy_text("b.k.x", &[]))?; }"#,
            Hits { key: 1, branch: 0 },
        ),
        (
            r#"fn f() { let t = copy_text("a.k.x", &[("n", &n)]); }"#,
            Hits { key: 1, branch: 0 },
        ),
        (
            r#"fn f() { let t = copy_text(k, &[]); }"#,
            Hits { key: 1, branch: 0 },
        ),
        (
            r#"fn f() { let t = copy_text("no.such.key", &[]); }"#,
            Hits { key: 1, branch: 0 },
        ),
        (
            r#"fn f(x: X) { match x.code.as_str() { "dup" => copy_text("s.k.x", &[]), _ => copy_text("s.k.x", &[]) } }"#,
            Hits { key: 0, branch: 2 },
        ),
        (
            r#"fn f(x: X) { if x.why == "ended" { copy_text("s.k.x", &[]) } else { copy_text("s.k.x", &[]) } }"#,
            Hits { key: 0, branch: 2 },
        ),
        (
            r#"const RETRYABLE: &[&str] = &["screen_changed", "ambiguous"]; fn f(x: X) { if RETRYABLE.contains(&x.why) { copy_text("s.k.x", &[]); } }"#,
            Hits { key: 0, branch: 1 },
        ),
        (
            r#"fn f(x: X) { if matches!(x.why.as_str(), "a" | "b") { copy_text("s.k.x", &[]); } }"#,
            Hits { key: 0, branch: 1 },
        ),
        (
            r#"fn f(x: X) { if x.why == "a" { v.iter().map(|i| { copy_text("s.k.x", &[]) }); } else if ok { copy_text("s.k.x", &[]); } }"#,
            Hits { key: 0, branch: 2 },
        ),
        // 阴性：骨架字照摆；条件不比码；match 没有字面码的臂；分支里再定义的函数不算；字符字面量 · 原样字符串不乱了分词。
        (
            r#"fn f() { let t = copy_text("p.k.x", &[]); }"#,
            Hits { key: 0, branch: 0 },
        ),
        (
            r#"fn f() { let t = copy_text("s.k.x", &[]); }"#,
            Hits { key: 0, branch: 0 },
        ),
        (
            r#"fn f(open: bool) { if open { copy_text("s.k.x", &[]); } else { copy_text("s.k.x", &[]); } }"#,
            Hits { key: 0, branch: 0 },
        ),
        (
            r#"fn f(x: K) { match x { K::A => copy_text("s.k.x", &[]), _ => copy_text("s.k.x", &[]) } }"#,
            Hits { key: 0, branch: 0 },
        ),
        (
            r#"fn f(x: X) { if x.why == "a" { fn g() -> String { copy_text("s.k.x", &[]) } } }"#,
            Hits { key: 0, branch: 0 },
        ),
        (
            r##"fn f(c: char) { let q = '"'; let r = r#"{ "x" == y"#; if c == '{' { copy_text("s.k.x", &[]); } }"##,
            Hits { key: 0, branch: 0 },
        ),
    ];
    let mut bad = Vec::new();
    for (code, want) in cases {
        let got = scan_rs(code, &table);
        if got != *want {
            bad.push(format!("「{code}」命中 {got:?}，该是 {want:?}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
