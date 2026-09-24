//! 「**中转层里不要有账号**」的机检 —— 用户 2026-09-24 逐字：
//! 「中转层不要有账号, 账号就账号中转就中转」。
//!
//! 设计里早就是这一句（`设计/01 §2.5`「中转 ≠ 账号」· `设计/20 §9` 那张两层小结 ·
//! `§6` 命名推论「不许用『中转』指 ②」）。没落地的是**物理位置**：层 2 住在
//! `src/backend/relay/accounts/`，就在层 1 的目录底下。本文件把「两层之间到底连着什么」
//! 钉成**四条两向集合相等**，不用地板。
//!
//! # 四条，各钉一件
//!
//! | # | 它问什么 | 两侧各是谁 |
//! |---|---|---|
//! | ㈠ | 层 2 的根在哪、有哪几份文件 | 盘上**现推**（唯一一份 `impl Destinations for` 的那个模块）⇔ [`ACCOUNT_LAYER_FILES`] |
//! | ㈡ | `relay/` 目录里**住着**几份层 2 的文件 | 盘上现扫的交集 ⇔ [`RESIDENT_IN_RELAY`] |
//! | ㈢ | 层 1 的生产段**点名**层 2 的地方 | 盘上现数（词表从层 2 的声明**现推**）⇔ [`LAYER_ONE_NAMES_LAYER_TWO`] |
//! | ㈣ | 层 2 用到层 1 的哪几样（**接口面有多窄**） | 盘上现解析的路径 ⇔ [`CONTRACT`] |
//!
//! # 🔴 ㈡ 与 ㈢ 今天**不是零**，而这是照实登记，不是豁免
//!
//! 搬家（层 2 离开 `relay/`）今天**落不了地**，挡它的是写区外的两处，逐条现打：
//!
//! 1. `tests/backend/readonly_guard.rs::BACKEND_CORE_MODULES` —— 它与 `lib.rs` 的模块声明
//!    **两向集合相等**。后端顶层新开一个模块，那条当场红（现打逐字：「`lib.rs` 有而登记没有：
//!    ["accounts"] ⇒ 🔴 新加的模块没人看」）。那份文件本波归另一路。
//! 2. `src/backend/main.rs` 里 `--relay` 那一臂调的是 `relay::run` —— 起进程时把两层接起来的
//!    那一处（今天是 [`LAYER_ONE_NAMES_LAYER_TWO`] 里 `listen.rs` 那两行）得搬到它那里。
//!
//! ⇒ 两处放行那天的改法是**机械的**：`git mv` 那棵树 ＋ 上面两处各一行 ＋ `lib.rs` 一行声明
//!   ＋ 删掉 `relay/mod.rs` 那行 `mod accounts;` ⇒ ㈡ ㈢ 两张表**清空**，本文件变成零命中形态。
//!   **那一天之前，这两张表是一份「中转层里还有多少账号」的逐条账，而不是一份放行名单**：
//!   往里多加一行 = 层 1 又多认识了层 2 一样东西 —— 那是**倒退**，不许。
//!
//! # 买不到什么（照实写）
//!
//! - ㈢ 认的是**名字**（`accounts` ＋ 层 2 在顶层声明的类型/常量名）。经**别的模块转一道手**的
//!   重导出（`pub use` 成另一个名字）它看不见；挡那一形的是 `C2`（通信层成员不许依赖业务模块）
//!   与本仓的 `layering_guard`，不是本条。
//! - ㈣ 认的是 `use` 与 `crate::` / `super::` / `self::` 起头的**路径**。一个先 `use` 进来、
//!   再以**别名**用的项，它按 `use` 那一行记（记的是真名），不按别名记 —— 这是对的；
//!   但 `use super::*` 这种**通配**它展不开，会当成一项 `*` 记下来并让相等断言当场红
//!   （宁可红，不假装看懂了）。
//! - 四条都**不买**「层 2 做得对」：那张决策表答得对不对由 `table_tests` 与 `wire_golden` 负责。

#[cfg(test)]
mod tests {
    use crate::guard_support::production_code;
    use std::collections::BTreeSet;

    /// 层 1 的目录（相对 `src/backend/`）。
    const RELAY_DIR: &str = "relay";

    /// ㈠ 层 2 那棵树里的文件（相对**层 2 的根**）。**相等，不是地板**。
    const ACCOUNT_LAYER_FILES: &[&str] = &["creds.rs", "mod.rs", "policy.rs", "table.rs"];

    /// ㈡ `relay/` 目录里**住着的**层 2 文件（相对 `src/backend/`）。
    ///
    /// 🔴 **搬家那天清空**（挡它的两处见模块头注）。今天这四行是「中转层里还有账号」的**物证**。
    const RESIDENT_IN_RELAY: &[&str] = &[
        "relay/accounts/creds.rs",
        "relay/accounts/mod.rs",
        "relay/accounts/policy.rs",
        "relay/accounts/table.rs",
    ];

    /// ㈢ 层 1 的生产段里**点名**层 2 的地方：`(文件, 名字, 处数)`。
    ///
    /// 🔴 **搬家那天清空**。今天剩的两件都不是「搬字节」的活，而是 Rust / 进程装配的机械产物：
    /// - `relay/mod.rs` 那行 `mod accounts;` —— 子模块只能由父模块声明（`comm_boundary_registry`
    ///   里 `relay/mod.rs` 那一行的裁词逐字记着这件事）；
    /// - `relay/listen.rs::run` 把 `accounts::Boot` 递进 `run_with` —— 起进程时把两层接起来。
    ///   `run_with` 以下，层 1 只见得到 `Startup` / `Ready` / `Destinations` 三个契约口。
    const LAYER_ONE_NAMES_LAYER_TWO: &[(&str, &str, usize)] = &[
        ("relay/listen.rs", "Boot", 1),
        ("relay/listen.rs", "accounts", 1),
        ("relay/mod.rs", "accounts", 1),
    ];

    /// ㈣ 层 2 用到的层 1 的东西（相对 `crate::relay::` 的路径）—— **接口面就这么宽**。
    ///
    /// | 项 | 为什么层 2 要它 |
    /// |---|---|
    /// | `Destinations` · `Destination` · `AuthSwap` · `Mode` · `RouteKey` | 请求路径上那一问一答（`设计/20 §2`）|
    /// | `Startup` · `Ready` | 启动路径上那两步（起监听前验配置 · 起监听后交出 `Destinations`）|
    /// | `upstream::Base` | 一行的上游是什么 —— 层 1 的**传输原语**，层 2 解析它、焊进行里、原样交回 |
    /// | `route::segment_is_safe` | 「这个账号 id 当得了路由段吗」与层 1 切键用的是**同一个谓词**（`route.rs` 头注逐字论证过为什么不许各写一份）|
    ///
    /// ⚠ 多一项 = 层 2 又伸手拿了层 1 一样东西（要来这里说清为什么）；少一项 = 表腐了。
    const CONTRACT: &[&str] = &[
        "AuthSwap",
        "Destination",
        "Destinations",
        "Mode",
        "Ready",
        "RouteKey",
        "Startup",
        "route::segment_is_safe",
        "upstream::Base",
    ];

    // ── 语料 ────────────────────────────────────────────────────────────────

    /// 整个后端 crate 的生产段：`(相对 src/backend 的路径, 生产段文本)`。
    fn crate_production() -> Vec<(String, String)> {
        let root = crate::guard_support::src_root();
        let files: Vec<(String, String)> = guard_core::scan_tree!(&root, &["rs"])
            .into_iter()
            .map(|(p, raw)| {
                let rel = p
                    .strip_prefix(&root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel, production_code(&raw))
            })
            .collect();
        assert!(files.len() >= 30, "只扫到 {} 份 —— 取法坏了", files.len());
        files
    }

    /// ㈠ 层 2 的根（相对 `src/backend/` 的目录，带尾 `/`）—— **从盘上现推**：
    /// 整个 crate 生产段里**唯一**一处 `impl Destinations for` 所在的那个模块。
    ///
    /// ★ 为什么不写死成一个常量：写死的话，「层 2 在哪」与「层 2 有哪几份」两侧同源，
    ///   搬了家而忘了改常量，本文件会继续去扫一个空目录 —— 恒绿。
    fn account_layer_root(files: &[(String, String)]) -> String {
        // ⚠ 用有边界的计数（`count_word`），不用裸子串：`impl Destinations for` 的两端都是标识符字符，
        //   裸子串会把 `impl DestinationsForX` 这种认成一处（`needle_anchor_registry` 那条递减棘轮治的正是这一族）。
        let hits: Vec<(&String, usize)> = files
            .iter()
            .map(|(rel, prod)| (rel, count_word(prod, "impl Destinations for")))
            .filter(|(_, n)| *n > 0)
            .collect();
        assert!(
            hits.len() == 1 && hits[0].1 == 1,
            "`impl Destinations for` 应当在生产段里**恰好一处**（层 2 唯一的决策点），实得 {hits:?}"
        );
        let f = hits[0].0;
        let dir = match f.strip_suffix("mod.rs") {
            Some(d) => d.to_string(),
            None => panic!(
                "层 2 的实现住在 `{f}` —— 它不是一个 `mod.rs`，本文件认不出它的「树」。\n\
                 层 2 是一棵树（表 · 凭据 · 热重载），不是一份文件；真改成单文件了就回来改这里。"
            ),
        };
        dir
    }

    /// 层 2 的模块路径（`crate::` 之后的段），由它的根目录现算。
    fn module_of_dir(dir: &str) -> Vec<String> {
        dir.trim_end_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// 一份文件自己的模块路径：`a/b/mod.rs` ⇒ `[a,b]`；`a/b.rs` ⇒ `[a,b]`。
    fn module_of_file(rel: &str) -> Vec<String> {
        let stem = rel.strip_suffix(".rs").unwrap_or(rel);
        let mut segs: Vec<String> = stem.split('/').map(str::to_string).collect();
        if segs.last().map(String::as_str) == Some("mod") {
            segs.pop();
        }
        segs
    }

    fn is_ident(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_'
    }

    /// `word` 在 `text` 里**第一次作为完整标识符**出现的位置（两侧都不是标识符字符）。
    ///
    /// ⚠ 本文件所有「在盘上文本里找一个词」都走它（或 [`count_word`]），不走裸子串：
    ///   匹配单位比事实小，事实被撑大了判据照样绿（`needle_anchor_registry` 那条递减棘轮治的那一族）。
    fn find_word(text: &str, word: &str) -> Option<usize> {
        let mut from = 0usize;
        while let Some(rel) = text[from..].find(word) {
            let at = from + rel;
            let end = at + word.len();
            let before = text[..at].chars().next_back();
            let after = text[end..].chars().next();
            if !before.is_some_and(is_ident) && !after.is_some_and(is_ident) {
                return Some(at);
            }
            from = end;
        }
        None
    }

    /// `word` 在 `text` 里作为**完整标识符**出现几次。
    fn count_word(text: &str, word: &str) -> usize {
        let mut n = 0usize;
        let mut rest = text;
        while let Some(at) = find_word(rest, word) {
            n += 1;
            rest = &rest[at + word.len()..];
        }
        n
    }

    /// ㈢ 的词表：`accounts` ＋ 层 2 在**顶层**声明的类型 / trait / 常量 / 静态量的名字（现推）。
    ///
    /// ⚠ 只收**类型级**的名字，不收函数名：函数名里有 `build` / `load` / `new` 这类通用词，
    ///   层 1 自己就有 `.load(SeqCst)` —— 收进来就是假阳，而假阳会训练人绕开判据。
    fn layer_two_vocabulary(layer_two: &[&(String, String)]) -> BTreeSet<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        out.insert("accounts".to_string());
        for (_, prod) in layer_two {
            for line in prod.lines() {
                let t = guard_core::strip_visibility(line.trim_start());
                for kw in ["struct ", "enum ", "trait ", "const ", "static ", "type "] {
                    if let Some(rest) = t.strip_prefix(kw) {
                        let name: String = rest.chars().take_while(|c| is_ident(*c)).collect();
                        // `const fn x` 那一形会推出 `fn` —— 关键字不是层 2 的名字。
                        if !name.is_empty() && !["fn", "mut", "unsafe"].contains(&name.as_str()) {
                            out.insert(name);
                        }
                    }
                }
            }
        }
        out
    }

    /// ㈣ 把一段 `use` 树展开成完整路径：`a::{b, c::{d, e}}` ⇒ `a::b` · `a::c::d` · `a::c::e`。
    fn expand_use(tree: &str) -> Vec<String> {
        let tree: String = tree.chars().filter(|c| !c.is_whitespace()).collect();
        fn go(prefix: &str, t: &str, out: &mut Vec<String>) {
            match t.find('{') {
                // ⚠ 别名（`as x`）必须由调用方在删空白**之前**剥掉：删完空白就认不出了。
                None => out.push(format!("{prefix}{t}")),
                Some(open) => {
                    let head = &t[..open];
                    let inner = &t[open + 1..t.len() - 1];
                    // 顶层逗号分割（跳过嵌套花括号里的逗号）。
                    let mut depth = 0i32;
                    let mut cur = String::new();
                    let mut parts: Vec<String> = Vec::new();
                    for c in inner.chars() {
                        match c {
                            '{' => depth += 1,
                            '}' => depth -= 1,
                            ',' if depth == 0 => {
                                parts.push(std::mem::take(&mut cur));
                                continue;
                            }
                            _ => {}
                        }
                        cur.push(c);
                    }
                    parts.push(cur);
                    for p in parts.into_iter().filter(|p| !p.is_empty()) {
                        go(&format!("{prefix}{head}"), &p, out);
                    }
                }
            }
        }
        let mut out = Vec::new();
        go("", &tree, &mut out);
        out
    }

    /// 把一条相对路径按「写在 `file_mod` 这个模块里」解析成绝对段（`crate::` 之后）。
    /// 认不出起头（不是 `crate` / `super` / `self`）⇒ `None`（外部 crate 或本地名）。
    fn resolve(file_mod: &[String], path: &str) -> Option<Vec<String>> {
        let segs: Vec<&str> = path.split("::").collect();
        let mut i = 0usize;
        let mut cur: Vec<String> = match segs.first().copied() {
            Some("crate") => {
                i = 1;
                Vec::new()
            }
            Some("self") => {
                i = 1;
                file_mod.to_vec()
            }
            Some("super") => file_mod.to_vec(),
            _ => return None,
        };
        while segs.get(i) == Some(&"super") {
            cur.pop()?;
            i += 1;
        }
        cur.extend(segs[i..].iter().map(|s| (*s).to_string()));
        Some(cur)
    }

    /// ㈣ 一份文件生产段里所有**指向模块树**的路径（`use` 展开 ＋ 行内 `crate::`/`super::`/`self::` 起头的），
    /// 解析成绝对段。
    fn paths_in(rel: &str, prod: &str) -> Vec<Vec<String>> {
        let file_mod = module_of_file(rel);
        let mut out: Vec<Vec<String>> = Vec::new();
        let mut rest_code = String::new();
        // ① `use …;`（可跨行）。别名 `as x` 在删空白之前剥掉。
        let mut s = prod;
        while let Some(at) = find_word(s, "use") {
            rest_code.push_str(&s[..at]);
            let Some(end) = s[at..].find(';') else { break };
            let body = &s[at + 3..at + end];
            let body = body
                .split(',')
                .map(|p| match p.find(" as ") {
                    Some(k) => {
                        p[..k].to_string()
                            + &p[k..].chars().filter(|c| *c == '}').collect::<String>()
                    }
                    None => p.to_string(),
                })
                .collect::<Vec<_>>()
                .join(",");
            for p in expand_use(&body) {
                if let Some(abs) = resolve(&file_mod, &p) {
                    out.push(abs);
                }
            }
            s = &s[at + end + 1..];
        }
        rest_code.push_str(s);
        // ② 行内路径：`crate::a::b` / `super::x` / `self::y`。
        let b = rest_code.as_str();
        for root in ["crate::", "super::", "self::"] {
            let mut from = 0usize;
            while let Some(r) = b[from..].find(root) {
                let at = from + r;
                from = at + root.len();
                if b[..at].chars().next_back().is_some_and(is_ident) {
                    continue;
                }
                let tail: String = b[at..]
                    .chars()
                    .take_while(|c| is_ident(*c) || *c == ':')
                    .collect();
                let tail = tail.trim_end_matches(':');
                if let Some(abs) = resolve(&file_mod, tail) {
                    out.push(abs);
                }
            }
        }
        out
    }

    /// ㈣ 层 2 这批文件用到的层 1 的项（相对 `crate::relay::`）。
    fn contract_used(layer_two: &[&(String, String)], l2_mod: &[String]) -> BTreeSet<String> {
        let relay = vec![RELAY_DIR.to_string()];
        let mut out = BTreeSet::new();
        for (rel, prod) in layer_two {
            for abs in paths_in(rel, prod) {
                if abs.starts_with(&relay) && !abs.starts_with(l2_mod) && abs.len() > 1 {
                    out.insert(abs[1..].join("::"));
                }
            }
        }
        out
    }

    // ── 判据本体 ────────────────────────────────────────────────────────────

    /// ★★★ 「**中转层里不要有账号**」—— 四条两向集合相等（逐条见模块头注那张表）。
    #[test]
    fn the_relay_layer_holds_no_accounts_beyond_the_itemised_residue() {
        let files = crate_production();
        let root = account_layer_root(&files);
        let l2_mod = module_of_dir(&root);
        let layer_two: Vec<&(String, String)> = files
            .iter()
            .filter(|(rel, _)| rel.starts_with(&root))
            .collect();

        // ㈠ 层 2 那棵树有哪几份：盘上现推 ⇔ 登记。
        let on_disk: BTreeSet<String> = layer_two
            .iter()
            .map(|(rel, _)| rel[root.len()..].to_string())
            .collect();
        let registered: BTreeSet<String> = ACCOUNT_LAYER_FILES
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(
            on_disk, registered,
            "层 2（根 `{root}`，由唯一那处 `impl Destinations for` 现推）的文件对不上登记。\n\
             加/减了文件就回来改 `ACCOUNT_LAYER_FILES`，并重读下面三条 —— 它们的人群都从这里来。"
        );

        // ㈡ `relay/` 里住着几份层 2：盘上的交集 ⇔ 登记。
        let resident: BTreeSet<String> = layer_two
            .iter()
            .map(|(rel, _)| rel.clone())
            .filter(|rel| rel.starts_with(&format!("{RELAY_DIR}/")))
            .collect();
        let resident_reg: BTreeSet<String> =
            RESIDENT_IN_RELAY.iter().map(|s| (*s).to_string()).collect();
        assert_eq!(
            resident, resident_reg,
            "`relay/` 目录里住着的层 2 文件与登记对不上。\n\
             🔴 搬家落地了 ⇒ 盘上这一侧变空 ⇒ 把 `RESIDENT_IN_RELAY` 清空（本条就成了零命中形态）。\n\
             🔴 盘上多出来一份 ⇒ 层 2 又往中转层底下放了东西 —— **那是倒退**，别往表里加行，把它挪出去。"
        );

        // ㈢ 层 1 点名层 2：盘上现数 ⇔ 登记。
        let vocab = layer_two_vocabulary(&layer_two);
        assert!(
            vocab.len() >= 5 && vocab.contains("Accounts") && vocab.contains("RoutingTable"),
            "层 2 的词表只推出 {vocab:?} —— 推法坏了，下面那条在空转"
        );
        let mut named: BTreeSet<(String, String, usize)> = BTreeSet::new();
        for (rel, prod) in &files {
            if !rel.starts_with(&format!("{RELAY_DIR}/")) || rel.starts_with(&root) {
                continue;
            }
            for w in &vocab {
                let n = count_word(prod, w);
                if n > 0 {
                    named.insert((rel.clone(), w.clone(), n));
                }
            }
        }
        let named_reg: BTreeSet<(String, String, usize)> = LAYER_ONE_NAMES_LAYER_TWO
            .iter()
            .map(|(f, w, n)| ((*f).to_string(), (*w).to_string(), *n))
            .collect();
        assert_eq!(
            named, named_reg,
            "层 1（`relay/` 里层 2 之外那几份）的生产段点名层 2 的地方与登记对不上。\n\
             🔴 盘上多出来的 = 中转层又认识了账号层一样东西 ⇒ **别往表里加行**，\n\
                把那件事交给层 2（请求路径走 `Destinations`，启动路径走 `Startup`/`Ready`）。\n\
             登记里有而盘上没有 = 表腐了（或者搬家落地了 ⇒ 清空这张表）。"
        );

        // ㈣ 层 2 用到层 1 的哪几样：盘上现解析 ⇔ 登记。
        let used = contract_used(&layer_two, &l2_mod);
        let contract: BTreeSet<String> = CONTRACT.iter().map(|s| (*s).to_string()).collect();
        assert_eq!(
            used, contract,
            "层 2 用到的层 1 的东西与登记的契约面对不上。\n\
             多出来的 = 层 2 又伸手拿了层 1 一样东西（来 `CONTRACT` 说清为什么，或者别拿）；\n\
             少了的 = 表腐了。"
        );
    }

    /// 反空真：三把尺子（数词 · 展 `use` · 解析相对路径）在**合成**文本上都认得出它们该认的，
    /// 也**不**把它们不该认的认进来。没有这一条，尺子坏成恒空之后上面那条照样可能绿
    /// （㈢ ㈣ 两侧都空的那一形）。
    #[test]
    fn the_rulers_of_the_account_layer_guard_are_not_blind() {
        // 数词：认完整标识符，不认子串。
        assert_eq!(
            count_word("use super::accounts; let x = accounts::Boot;", "accounts"),
            2
        );
        assert_eq!(count_word("let my_accounts = 1; accountsX", "accounts"), 0);
        // 展 `use`：嵌套花括号与别名。
        let mut got = expand_use("super::{a, b::{c, d}}");
        got.sort();
        assert_eq!(got, vec!["super::a", "super::b::c", "super::b::d"]);
        // 解析：同一句 `super::super::X` 在不同深度的文件里指向不同的模块。
        let deep = paths_in(
            "relay/accounts/x.rs",
            "use super::super::server::Relay;\nfn f() { super::table::build(); }\n",
        );
        assert!(deep.contains(&vec!["relay".into(), "server".into(), "Relay".into()]));
        assert!(deep.contains(&vec![
            "relay".into(),
            "accounts".into(),
            "table".into(),
            "build".into()
        ]));
        // 别名按真名记。
        let aliased = paths_in("relay/accounts/mod.rs", "use super::Mode as M;\n");
        assert_eq!(aliased, vec![vec!["relay".to_string(), "Mode".to_string()]]);
        // 搬家之后的写法（`crate::relay::…`）解析到同一处 —— ㈣ 那张表搬家不用改。
        let moved = paths_in(
            "accounts/mod.rs",
            "use crate::relay::{Mode, upstream::Base};\n",
        );
        assert!(moved.contains(&vec!["relay".to_string(), "Mode".to_string()]));
        assert!(moved.contains(&vec![
            "relay".to_string(),
            "upstream".to_string(),
            "Base".to_string()
        ]));
        // 外部 crate 的路径不算（不是 `crate`/`super`/`self` 起头）。
        assert!(paths_in(
            "relay/accounts/mod.rs",
            "use creds_core::store::AuthStyle;\n"
        )
        .is_empty());
    }
}
