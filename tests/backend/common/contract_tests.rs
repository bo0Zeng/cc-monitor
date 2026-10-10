use super::*;

/// 判据用：跑 `f`，回它的结果 ＋ 期间契约错记进日志的那几条诊断（句子里不带诊断，判据要分「哪一格错」就读这里）。
pub(crate) fn diag<T>(f: impl FnOnce() -> T) -> (T, String) {
    let mut out = None;
    let lines = crate::stream::run_route::tests::heard(|| out = Some(f()));
    let said = lines
        .iter()
        .filter(|l| l.contains("contract: malformed request:"))
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    (out.expect("f 跑完了"), said)
}

/// 契约错那一句只说「请求格式不对」：英文诊断不上句子（记一行 warn 日志），任何诊断都回同一句。
#[test]
fn the_contract_sentence_carries_no_diagnostic() {
    let (said, heard) = diag(|| malformed("missing `to` (string)"));
    assert_eq!(said, copy_core::copy_text("beContract.malformed.say", &[]));
    assert!(!said.contains("missing"), "诊断进了句子：{said}");
    assert!(
        heard.contains("missing `to` (string)"),
        "诊断没记进日志：{heard}"
    );
    assert_eq!(malformed("another diag"), said);
}

/// 〔mg39〕`malformed` 有副作用（记一行 warn）⇒ 它只许在真错了的那条路上被调。
/// 写成急求值的实参（`.ok_or((码, malformed(..)))` · `.unwrap_or(..)` · `.map_or(默认, ..)` · `.or(..)` · `.and(..)`）的话，
/// 成功的那一趟也会记一行「请求格式不对」—— 日志说假话，一次性模式里还会混进 stderr。要写 `ok_or_else(|| ..)` 这类惰性形。
/// 人群：后端生产段全部 `.rs`；对账：扫到的 `malformed(` 调用数要过一个下限（扫了个空集 ⇒ 红，不是绿）。
#[test]
fn malformed_is_never_an_eager_argument_guard() {
    const EAGER: &[&str] = &["ok_or", "unwrap_or", "map_or", "or", "and"];
    let mut seen = 0usize;
    let mut bad = Vec::new();
    for (path, src) in
        guard_core::scan_tree_excluding(&crate::guard_support::src_root(), &["rs"], &[])
    {
        let code = crate::guard_support::production_side_of(&path, &src);
        seen += code.matches("malformed(").count();
        let b = code.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] != b'.' {
                i += 1;
                continue;
            }
            let name_end = b[i + 1..]
                .iter()
                .position(|c| !(c.is_ascii_alphanumeric() || *c == b'_'))
                .map_or(b.len(), |p| i + 1 + p);
            let name = &code[i + 1..name_end];
            if !EAGER.contains(&name) || b.get(name_end) != Some(&b'(') {
                i += 1;
                continue;
            }
            for arg in top_level_args(&code[name_end + 1..]) {
                let a = arg.trim_start();
                let lazy = a.starts_with('|') || a.starts_with("move |") || a.starts_with("move||");
                if !lazy && a.contains("malformed(") {
                    let line = code[..i].matches('\n').count() + 1;
                    bad.push(format!("{}:{line} .{name}(…malformed(…)…)", path.display()));
                }
            }
            i = name_end;
        }
    }
    assert!(
        seen >= 100,
        "只扫到 {seen} 处 malformed( —— 人群不对（树搬了？）"
    );
    assert!(
        bad.is_empty(),
        "malformed( 写成了急求值的实参（成功那一趟也记一行日志），改 ok_or_else / unwrap_or_else / map_or_else 这类惰性形：\n{}",
        bad.join("\n")
    );
}

/// `s` 从左括号后一个字节起：回到配对的右括号为止、按顶层逗号切开的几个实参（跳过字符串 · 字符字面量里的括号）。
fn top_level_args(s: &str) -> Vec<&str> {
    let b = s.as_bytes();
    let (mut depth, mut start, mut i) = (0i32, 0usize, 0usize);
    let mut out = Vec::new();
    while i < b.len() {
        match b[i] {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'\'' if b.get(i + 2) == Some(&b'\'') => i += 2,
            b'\'' if b.get(i + 1) == Some(&b'\\') => {
                while i + 1 < b.len() && b[i + 1] != b'\'' {
                    i += 1;
                }
                i += 1;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' if depth == 0 => {
                out.push(&s[start..i]);
                return out;
            }
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out
}
