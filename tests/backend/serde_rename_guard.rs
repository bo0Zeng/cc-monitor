//! 线上名只由容器级 `rename_all` 出（R2：一个概念一个名字）：全仓 `src/` 生产段里，字段 / 变体级的 `serde(rename = "…")`
//! 只许用在线上名是 Rust 关键字、Rust 那边起不成同名的那几处（`type` · `self` · `as` · `for` · `use` …）。
//! 线上名与 Rust 名只差大小写 · 分隔写法，或换了一个词 ⇒ 都红：改 Rust 名去贴线上名，交给容器的 `rename_all`（线上值不变）。
//! 容器级的 `rename`（类型自己的线上名）不归这条管。
//!
//! 判法是读源码文本：每个 `#[serde(…)]`（可跨行）里的 `rename = "…"`，往后跳过别的属性，下一个条目不是 `struct` / `enum` ⇒ 字段 / 变体级。
//! 什么时候删：serde 出了「只许关键字改名」的原生检查，或全仓不再用 serde 派生。

#[cfg(test)]
mod tests {
    /// Rust 的关键字（严格 ＋ 保留）：线上名是其中一个，Rust 那边就起不成同名，只能字段级改名。
    const KEYWORDS: &[&str] = &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "match", "mod",
        "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
        "trait", "true", "try", "type", "unsafe", "use", "where", "while", "abstract", "become",
        "box", "do", "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield",
    ];

    /// 从 `at`（指着 `#[` 的 `#`）起一个配平的属性，回它结束后的位置（字符串里的括号不算）。
    fn attr_end(s: &[u8], at: usize) -> usize {
        let (mut depth, mut i, mut in_str) = (0i32, at + 1, false);
        while i < s.len() {
            let c = s[i];
            if in_str {
                if c == b'\\' {
                    i += 1;
                } else if c == b'"' {
                    in_str = false;
                }
            } else {
                match c {
                    b'"' => in_str = true,
                    b'[' | b'(' => depth += 1,
                    b']' | b')' => {
                        depth -= 1;
                        if depth == 0 {
                            return i + 1;
                        }
                    }
                    _ => {}
                }
            }
            i += 1;
        }
        s.len()
    }

    /// 属性后面挂的那一条（跳过空白与别的属性）的开头一截。
    fn item_after(src: &str, mut i: usize) -> &str {
        let s = src.as_bytes();
        loop {
            while i < s.len() && s[i].is_ascii_whitespace() {
                i += 1;
            }
            if s.get(i) == Some(&b'#') {
                i = attr_end(s, i);
                continue;
            }
            break;
        }
        let rest = guard_core::strip_visibility(&src[i..]);
        &rest[..rest.find(['{', '(', ',', ';', '\n']).unwrap_or(rest.len())]
    }

    /// 那一条（可见性已剥）是不是类型本身（`struct X` · `enum X`）。
    fn is_container(item: &str) -> bool {
        matches!(item.split_whitespace().next(), Some("struct" | "enum"))
    }

    /// 一份生产段里字段 / 变体级、线上名不是关键字的 `serde(rename)`：`(线上名, 挂在哪一条)`。
    fn misnamed(src: &str) -> Vec<(String, String)> {
        let s = src.as_bytes();
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(k) = src[from..].find("#[serde(") {
            let at = from + k;
            let end = attr_end(s, at);
            let attr = &src[at..end];
            from = end;
            let item = item_after(src, end).trim().to_string();
            if is_container(&item) {
                continue;
            }
            let mut rest = attr;
            while let Some(r) = rest.find("rename") {
                let tail = &rest[r + "rename".len()..];
                let before = rest[..r].chars().last().unwrap_or(' ');
                rest = tail;
                if before.is_alphanumeric() || before == '_' || tail.starts_with('_') {
                    continue;
                }
                let tail = tail.trim_start();
                let wire = match tail.strip_prefix('=') {
                    Some(t) => t
                        .trim_start()
                        .strip_prefix('"')
                        .and_then(|t| t.split('"').next()),
                    None => Some("（rename(…) 那一形）"),
                };
                if let Some(w) = wire.filter(|w| !KEYWORDS.contains(w)) {
                    out.push((w.to_string(), item.clone()));
                }
            }
        }
        out
    }

    /// 判法本身：只差大小写的 · 换了词的都认得出；关键字放行；容器级不管；跨行的属性照认。
    #[test]
    fn the_judge_tells_the_kinds_apart() {
        let src = r#"
#[derive(Serialize)]
#[serde(rename = "Whole")]
pub(crate) struct S {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default, rename = "atMs")]
    at_ms: u64,
    #[serde(
        default,
        rename = "jsonlPath",
    )]
    #[cfg_attr(test, ts(optional))]
    pub record_path: String,
    #[serde(rename_all = "camelCase")]
    x: u8,
}
enum E {
    #[serde(rename = "self")]
    Own,
    #[serde(rename = "answer")]
    Question,
}
"#;
        let got: Vec<(String, String)> = misnamed(src);
        assert_eq!(
            got,
            [
                ("atMs", "at_ms: u64"),
                ("jsonlPath", "record_path: String"),
                ("answer", "Question"),
            ]
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .to_vec()
        );
    }

    /// 全仓 `src/` 生产段：字段 / 变体级的 `serde(rename)` 只许关键字。
    #[test]
    fn field_and_variant_renames_are_keywords_only() {
        let root = guard_core::addr::repo_root();
        let files = guard_core::addr::tracked_under("src", "rs");
        assert!(
            files.len() > 500,
            "src/ 下只扫到 {} 份 `.rs` —— 采集面塌了",
            files.len()
        );
        let mut kept = 0usize;
        let mut bad = Vec::new();
        for rel in &files {
            let raw = std::fs::read_to_string(root.join(rel))
                .unwrap_or_else(|e| panic!("读不到 {rel}：{e}"));
            let src = guard_core::production_code(&raw);
            kept += src.matches("rename = \"").count();
            for (wire, item) in misnamed(&src) {
                bad.push(format!("{rel}：rename = \"{wire}\" 挂在 `{item}`"));
            }
        }
        assert!(kept > 0, "一处关键字改名都没扫到 —— 判法瞎了");
        assert!(
            bad.is_empty(),
            "字段 / 变体级的 serde(rename) 只许线上名是 Rust 关键字；这几处改 Rust 名贴线上名、交给容器的 rename_all：\n  {}",
            bad.join("\n  ")
        );
    }
}
