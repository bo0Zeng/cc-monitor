//! **拿 agent 的 kind 跟某一家比，只在 `agents::adapter_among` 那一处**。
//!
//! 「wire 上的 kind ⇒ 注册表里那一家」只判一处：`agents/mod.rs` 的 `adapter_among(registry, kind)`。
//! 各面 · 通用层要那一行就问它，不自己拿 kind 去比 —— 用 `find` · `filter` · `position` · `any` 还是手写循环都一样，
//! 判据认的是「比」这件事本身，不认迭代器那一形。
//!
//! 判据认的比较：生产代码里
//! - `<x>.kind == <y>` · `<x>.kind != <y>`（`.kind` 在哪一边都认；`.kind` 后接 `.as_str()` · `.as_ref()` · `.as_deref()` 也认；折行的也认）；
//! - `matches!(<x>.kind, <模式>)`。
//!
//! 另一边是常量（带 `::` 的路径、全大写的常量名、字符串字面量、数、`None` · `true` · `false`，或 `Some(…)` 包着它们）的
//! 不是在比某一家 —— 是在比别的东西的种类（`Kind::Api` · `WhoKind::Terminal` · `ExtKind::Plugin` · `KIND_DIR` …），不数；
//! 例外：常量是某一家的名字（字符串等于注册表里一家的 kind，或常量名带 `AGENT`，如 `AGENT_KIND`）⇒ 是在比那一家，照数。
//! 两边都是运行时的值 ⇒ 数：判据分不出 `<x>` 是不是 agent 那一族（文本看不见类型），一律数，再在 [`tests::REASONED`]
//! 里逐处写明为什么可以留：要么不是 agent 那一族（资产 · 差异行 · 单元格 …的 kind），要么是 agent 那一族但不是在注册表里取那一行。
//!
//! 登记表 [`tests::REASONED`] 只许减：实得与表逐条对齐（按「文件 + 那一句比较」），多一处红、少一处也红（少了就把表改短）。
//! 新冒出来的一处比较 ⇒ 是在注册表里取那一行就改问 `adapter_among`；真不是，才带着理由登表。
//!
//! ⚠ 诚实边界：认的是 `.kind` 这一格的 `==` · `!=` · `matches!`。还认不出的写法：
//! - 不经 `.kind` 这一格：把 kind 先取进变量再比（`let k = a.kind; … k == kind`）、解构出来比（`|Adapter { kind: k, .. }| k == kind`）、
//!   在注册表派生的家名单里比（`asset_kinds_among(registry).into_iter().find(|k| *k == a)`），或家名放在别的格里比（`a.agent == family`）；
//! - 不用 `==` · `!=` · `matches!` 比：`.eq(…)` · `.ne(…)` · `match a.kind { k if k == … }` · 拿 kind 当 `HashMap` 的键去查；
//! - 方法而不是格：`.kind()`（IO 错误 · `own_state` 的行 · `deploy/machine` 那几样用它，都不是 agent 那一族）；
//! - 另一边是带 `::` 的路径、却不带 `AGENT` 的家名常量（今天各家的家名常量都叫 `AGENT_KIND` · `AGENT`）；
//!   带 `::` 的函数调用（最后一段小写打头、带括号，如 `crate::agents::account_kind_among(reg, None)`）认作运行时的值，照数。
//!
//! 切操作数靠括号配对：字符串里的 `(` `)` · 引号转义会切偏；切偏了在登记表里看得见（那一句比较的文本不像样），看见就改判据。
//! 往左切遇到同层的 `}` 就停（前面那一块已收口），`S { .. }.kind == …` 这种紧跟在结构体字面量后面取格的会切短、照样认作比较。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
mod tests {
    /// 留着的比较：`(相对 src/backend 的文件, 那一句比较（空白压成一个）, 为什么可以留)`。**只许减**。
    const REASONED: &[(&str, &str, &str)] = &[
        // ── agent 那一族，但不是在注册表里取那一行 ──
        ("agents/mod.rs", "a.kind == kind", "`adapter_among` 本身：唯一一处在注册表里按 kind 取那一行"),
        (
            "agents/record_face.rs",
            "h.kind == a.kind",
            "家清单（`AgentHome`）里挑出这一家的那几份家：比的是家清单的行，注册表那一行已经在手（`a`）",
        ),
        (
            "agents/history_face.rs",
            "*k == a.kind",
            "历史根表（`history_roots_among` 交的 `(kind, 根)`）里有没有这一家的根：比的是根表的行，注册表那一行已经在手",
        ),
        (
            "accounts/manage/library.rs",
            "h.kind == kind",
            "家清单（`agent_home`）里取写这一行的那一家的家：比的是家清单的行，不是注册表",
        ),
        (
            "accounts/manage/mcp_share_exec.rs",
            "f.kind == kind",
            "「住共享集合顶层的那一家是不是 `kind`」：比的是 `main_family` 挑出的那一家，问的是谁是主家，不是取那一行",
        ),
        (
            "accounts/manage/mcp_share_exec.rs",
            "f.kind == k",
            "在共享 MCP 的那几家（`families`，注册表的子集带着格位）里取 `k`：取的是 `Family`，不是注册表那一行；认不出 ⇒ 明说这一家不共享",
        ),
        (
            "accounts/manage/mcp_share_exec.rs",
            "*k == fam.kind",
            "这一趟已写好的结果 `(kind, 视图)` 里取这一家那一份：比的是结果表的行",
        ),
        // ── 不是 agent 那一族的 kind ──
        ("assets/asset_catalog.rs", "a.kind == kind", "资产的种类（skill · 插件 · MCP …），不是 agent 那一族；家另由 `a.agent` 比"),
        ("assets/ext.rs", "k.kind == kind", "扩展清单行的资产种类，不是 agent 那一族"),
        ("assets/ext.rs", "k.kind == *kind", "扩展清单行的资产种类，不是 agent 那一族"),
        ("assets/hub.rs", "r.kind == kind", "资产行的资产种类，不是 agent 那一族；家另由 `r.agent` 比"),
        ("faces/cells_catalog.rs", "cells[&w[0].1].kind == cells[&w[1].1].kind", "两格单元格的格种类相同才并组，不是 agent 那一族"),
        ("footprint/chores/mod.rs", "r.kind == k", "差异行的种类（增 · 删 · 不变）计数，不是 agent 那一族"),
        ("plan/product.rs", "owner.kind != pass", "权力范围负责人是会话还是子 agent（`session` · `subagent`），不是 agent 那一族"),
    ];

    /// 这个后端认得的那几家的名字（字符串常量等于它们之一 ⇒ 是在比那一家）。
    fn families() -> Vec<&'static str> {
        crate::agents::REGISTRY.iter().map(|a| a.kind).collect()
    }

    const SUFFIXES: &[&str] = &[".as_str()", ".as_ref()", ".as_deref()"];

    /// 这一边是不是 `<x>.kind`（容许前面的 `*` · `&`、后面的 `.as_str()` 那几样）。
    fn is_kind_field(side: &str) -> bool {
        let mut s = side.trim();
        loop {
            match SUFFIXES.iter().find(|x| s.ends_with(*x)) {
                Some(x) => s = s[..s.len() - x.len()].trim_end(),
                None => break,
            }
        }
        let s = s.trim_start_matches(['*', '&', ' ']);
        s.len() > ".kind".len() && s.ends_with(".kind")
    }

    /// 这一边是不是「不是某一家」的常量。
    fn is_other_const(side: &str, families: &[&str]) -> bool {
        let mut s = side.trim().trim_start_matches(['*', '&', ' ']).trim();
        if let Some(inner) = s.strip_prefix("Some(").and_then(|r| r.strip_suffix(')')) {
            s = inner.trim();
        }
        if s.is_empty() {
            return false;
        }
        if let Some(lit) = s.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
            return !families.contains(&lit);
        }
        if matches!(s, "None" | "true" | "false" | "_") || s.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
        let last = s.rsplit("::").next().unwrap_or(s);
        // `Kind::DaySegments` 那样的枚举值、`Kind::Access(_)` 那样带括号的模式，看最后一段的开头。
        let head: &str = last.split(['(', '{', ' ']).next().unwrap_or(last);
        if head.contains("AGENT") {
            return false;
        }
        // 带 `::` 的函数调用（最后一段小写打头、带括号）是运行时的值。
        if last.contains('(') && head.starts_with(|c: char| c.is_ascii_lowercase()) {
            return false;
        }
        let all_caps = !head.is_empty()
            && head
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
        s.contains("::") || all_caps
    }

    fn squash(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// 从 `from`（不含）往左切出一个操作数：遇到同层的分隔就停。
    fn left_operand(b: &[u8], from: usize) -> usize {
        let mut depth = 0i32;
        let mut i = from;
        let mut in_str = false;
        while i > 0 {
            let c = b[i - 1];
            if in_str {
                if c == b'"' && (i < 2 || b[i - 2] != b'\\') {
                    in_str = false;
                }
                i -= 1;
                continue;
            }
            match c {
                b'"' => in_str = true,
                b')' | b']' => depth += 1,
                b'(' | b'[' | b'{' => {
                    if depth == 0 {
                        return i;
                    }
                    depth -= 1;
                }
                // 同层的 `}`：前面那一块已收口，操作数从这里起。
                b'}' if depth == 0 => return i,
                b'}' => depth += 1,
                b',' | b';' | b'|' | b'=' | b'>' | b'<' if depth == 0 => return i,
                b'&' if depth == 0 && i >= 2 && b[i - 2] == b'&' => return i,
                b'!' if depth == 0 => return i,
                _ => {}
            }
            i -= 1;
        }
        0
    }

    /// 从 `from` 往右切出一个操作数：遇到同层的分隔就停。
    fn right_operand(b: &[u8], from: usize) -> usize {
        let mut depth = 0i32;
        let mut i = from;
        let mut in_str = false;
        while i < b.len() {
            let c = b[i];
            if in_str {
                if c == b'\\' {
                    i += 2;
                    continue;
                }
                if c == b'"' {
                    in_str = false;
                }
                i += 1;
                continue;
            }
            match c {
                b'"' => in_str = true,
                b'(' | b'[' => depth += 1,
                b'{' => {
                    if depth == 0 {
                        return i;
                    }
                    depth += 1;
                }
                b')' | b']' | b'}' => {
                    if depth == 0 {
                        return i;
                    }
                    depth -= 1;
                }
                b',' | b';' | b'|' | b'?' if depth == 0 => return i,
                b'&' if depth == 0 && b.get(i + 1) == Some(&b'&') => return i,
                b'=' if depth == 0 && b.get(i + 1) == Some(&b'>') => return i,
                _ => {}
            }
            i += 1;
        }
        b.len()
    }

    /// 左边那一截去掉打头的关键字（`if` · `return` · `while` · `let …`）。
    fn drop_keywords(s: &str) -> &str {
        let mut s = s.trim();
        for kw in ["if ", "return ", "while ", "else ", "match ", "let "] {
            if let Some(r) = s.strip_prefix(kw) {
                s = r.trim_start();
            }
        }
        s
    }

    /// 一份生产代码里「拿 kind 跟一个可能是某一家的值比」的那几句（空白压过）。
    fn kind_compares(prod: &str, families: &[&str]) -> Vec<String> {
        let b = prod.as_bytes();
        let mut out = Vec::new();
        // ① `==` · `!=`
        let mut i = 0;
        while i + 1 < b.len() {
            let op = &b[i..i + 2];
            let is_op = (op == b"==" || op == b"!=")
                && b.get(i + 2) != Some(&b'=')
                && (i == 0 || !matches!(b[i - 1], b'=' | b'<' | b'>' | b'!'));
            if !is_op {
                i += 1;
                continue;
            }
            let ls = left_operand(b, i);
            let re = right_operand(b, i + 2);
            let left = drop_keywords(&prod[ls..i]);
            let right = prod[i + 2..re].trim();
            let other = match (is_kind_field(left), is_kind_field(right)) {
                (true, _) => Some(right),
                (false, true) => Some(left),
                _ => None,
            };
            if let Some(other) = other {
                if !is_other_const(other, families) {
                    let op = &prod[i..i + 2];
                    out.push(squash(&format!("{left} {op} {right}")));
                }
            }
            i += 2;
        }
        // ② `matches!(<x>.kind, <模式>)`
        const M: &str = "matches!(";
        let mut at = 0;
        while let Some(k) = prod[at..].find(M) {
            let start = at + k + M.len();
            at = start;
            let comma = right_operand(b, start);
            if b.get(comma) != Some(&b',') {
                continue;
            }
            let subject = prod[start..comma].trim();
            if !is_kind_field(subject) {
                continue;
            }
            // 模式到 `matches!(` 收口为止；`if` 守卫之后的不算模式。
            let mut depth = 0i32;
            let mut end = b.len();
            for (j, &c) in b[comma + 1..].iter().enumerate() {
                match c {
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' => {
                        if depth == 0 {
                            end = comma + 1 + j;
                            break;
                        }
                        depth -= 1;
                    }
                    _ => {}
                }
            }
            let pats = &prod[comma + 1..end];
            let pats = pats.split(" if ").next().unwrap_or(pats);
            let all_other = pats
                .split('|')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .all(|p| is_other_const(p, families));
            if !all_other {
                out.push(squash(&format!("matches!({subject}, {pats})")));
            }
        }
        out
    }

    fn measured() -> Vec<(String, String)> {
        let root = guard_core::addr::backend_src_root();
        let fams = families();
        let mut got: Vec<(String, String)> = guard_core::scan_tree_excluding(&root, &["rs"], &[])
            .into_iter()
            .flat_map(|(p, src)| {
                let rel = p
                    .strip_prefix(&root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                kind_compares(&guard_core::production_code(&src), &fams)
                    .into_iter()
                    .map(move |c| (rel.clone(), c))
            })
            .collect();
        got.sort();
        got
    }

    /// 实得与登记表逐条对齐。
    #[test]
    fn a_family_is_compared_by_kind_only_inside_adapter_among() {
        let got = measured();
        let mut want: Vec<(String, String)> = REASONED
            .iter()
            .map(|(f, c, _)| ((*f).to_string(), (*c).to_string()))
            .collect();
        want.sort();
        let extra: Vec<_> = got.iter().filter(|g| !want.contains(g)).collect();
        let gone: Vec<_> = want.iter().filter(|w| !got.contains(w)).collect();
        assert_eq!(
            got, want,
            "\n拿 kind 跟一个可能是某一家的值比的地方与登记表对不上。\n\
             登记表外多出来的：{extra:#?}\n登记表里已经没有的：{gone:#?}\n\
             多了 ⇒ 是在注册表里取那一行就改问 `agents::adapter_among(registry, kind)`；真不是，带着理由登进 `REASONED`。\n\
             少了 ⇒ 把 `REASONED` 改短。"
        );
    }

    /// 登记表每一条都写了理由。
    #[test]
    fn every_kept_compare_says_why() {
        for (f, c, why) in REASONED {
            assert!(!why.trim().is_empty(), "{f} 的 `{c}` 没写为什么可以留");
        }
    }

    /// 抽取自检：跟哪种迭代器无关，比到某一家的都认；比别的东西的种类的不认。
    #[test]
    fn the_detector_sees_every_iterator_shape() {
        let fams = ["claude", "fake"];
        let n = |s: &str| kind_compares(s, &fams).len();
        // 取那一行的各种写法
        assert_eq!(
            n("REGISTRY.iter().find(|a| a.kind == kind).and_then(|a| a.asks)"),
            1
        );
        assert_eq!(
            n("registry\n    .iter()\n    .find(|a|\n        a.kind == k.as_str())\n"),
            1
        );
        assert_eq!(n("registry\n    .iter()\n    .filter(|a| a.kind == o.agent)\n    .filter_map(|a| a.local)\n"), 1);
        assert_eq!(
            n("let i = REGISTRY.iter().position(|ad| ad.kind == kind)?;"),
            1
        );
        assert_eq!(n("let has = registry.iter().any(|x| kind == x.kind);"), 1);
        assert_eq!(
            n("for a in registry { if a.kind != wanted { continue; } }"),
            1
        );
        assert_eq!(
            n("reg.iter().find(|a| a.kind == kind && a.name == name)"),
            1
        );
        assert_eq!(n("homes.iter().filter(|h| h.kind == a.kind.as_str())"), 1);
        // 跟家名常量比
        assert_eq!(n(r#"if a.kind == "fake" { }"#), 1);
        assert_eq!(n("if a.kind == crate::agents::fake::AGENT_KIND { }"), 1);
        assert_eq!(n(r#"matches!(a.kind, "claude" | "fake")"#), 1);
        // 别的东西的种类：不认
        assert_eq!(n("if show.kind == Kind::Api {}"), 0);
        assert_eq!(n("rows.iter().filter(|r| r.kind != LineDiffKind::Ctx)"), 0);
        assert_eq!(n("(self.kind == KIND_DIR) == *want"), 0);
        assert_eq!(n(r#"if s.kind == "binary" && stop.is_none() {}"#), 0);
        assert_eq!(n("matches!(ev.kind, notify::EventKind::Access(_))"), 0);
        assert_eq!(
            n("r.result = (r.kind == Some(ResultKind::Handback)).then(|| 1)"),
            0
        );
        assert_eq!(n("v.iter().find(|a| a.name == kind)"), 0);
        assert_eq!(n("kind: a.kind,"), 0);
        // 另一边是带路径的函数调用：运行时的值，照数
        assert_eq!(
            n("if a.kind == crate::agents::account_kind_among(reg, None) {}"),
            1
        );
        // 紧跟在前一块收口之后：切出来的只是这一句
        assert_eq!(
            kind_compares("let x = { 1 };\n}\nif a.kind == kind { }", &fams),
            vec!["a.kind == kind".to_string()]
        );
    }

    /// `adapter_among` 还住在 `agents/mod.rs`，登记表里留给它的那一句就是它自己。
    #[test]
    fn adapter_among_lives_in_agents_mod() {
        let root = guard_core::addr::backend_src_root();
        let src =
            std::fs::read_to_string(root.join("agents/mod.rs")).expect("agents/mod.rs 读得到");
        guard_core::find_pinned(&src, "pub fn adapter_among")
            .expect("adapter_among 恰好一处住在 agents/mod.rs");
    }
}
