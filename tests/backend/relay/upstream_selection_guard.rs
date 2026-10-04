//! 「**中转层里不要有账号**」的机检 —— 用户 2026-09-24 逐字：
//! 「中转层不要有账号, 账号就账号中转就中转」。
//!
//! 设计里早就是这一句（「中转 ≠ 账号」· 那张两层小结 ·
//! `§6` 命名推论「不许用『中转』指 ②」）。先前没落地的是**物理位置**：上游选择住在
//! `src/backend/relay/accounts/`，就在中转的目录底下。2026-09-24 搬到了 `src/backend/accounts/`。
//! 本文件把「两层之间到底连着什么」钉成**四条**，不用地板：
//!
//! | # | 它问什么 | 形态 |
//! |---|---|---|
//! | ㈠ | 上游选择的根在哪、有哪几份文件 | 盘上**现推**（唯一一份 `impl Destinations for` 的那个模块）⇔ [`UPSTREAM_SELECTION_FILES`]，两向相等 |
//! | ㈡ | `relay/` 目录里**住着**几份上游选择的文件 | **零命中**（两个人群各自非空，交集为空）|
//! | ㈢ | 中转的生产段**点名**上游选择的地方 | **零命中**（词表从上游选择的声明**现推**，非空；中转的人群非空）|
//! | ㈣ | 上游选择用到中转的哪几样（**接口面有多窄**） | 盘上现解析的路径 ⇔ [`CONTRACT`]，两向相等 |
//!
//! # ㈡ ㈢ 为什么是零命中而不是登记表
//!
//! 搬家之前这两条是两张**残留表**（`relay/` 里住着 4 份上游选择文件 · 中转点名上游选择 3 处），
//! 头注写明「那是一份逐条账，不是放行名单」。搬家那天两张表清空，**表本身也删了**：
//! 留一张空表等于给「往里加一行」留了一个看起来合法的口子。
//! ⇒ 今天谁让中转认识上游选择一样东西、或往 `relay/` 下放回一份上游选择文件，本条当场红，
//!   而改法只有一条：**把那件事交给上游选择**（请求路径走 `Destinations`，启动路径走 `Startup`/`Ready`，
//!   进程装配在 `accounts::upstream_select::host_relay`）。
//!
//! # 买不到什么（照实写）
//!
//! - ㈢ 认的是**名字**（`accounts` ＋ 上游选择在顶层声明的类型/常量名）。经**别的模块转一道手**的
//!   重导出（`pub use` 成另一个名字）它看不见；挡那一形的是 `C2`（通信层成员不许依赖业务模块）
//!   与本仓的 `layering_guard`，不是本条。
//! - ㈣ 认的是 `use` 与 `crate::` / `super::` / `self::` 起头的**路径**。一个先 `use` 进来、
//!   再以**别名**用的项，它按 `use` 那一行记（记的是真名），不按别名记 —— 这是对的；
//!   但 `use super::*` 这种**通配**它展不开，会当成一项 `*` 记下来并让相等断言当场红
//!   （宁可红，不假装看懂了）。它按「`use` 这个**词**」找语句：字符串里恰好有一个独立的
//!   `use` 词，会把它到下一个 `;` 之间当成一条 `use` 去解析（多半解析不出路径、被丢掉），
//!   那一段里的行内路径因此漏扫。
//! - 四条都**不买**「上游选择做得对」：那张决策表答得对不对由 `table_tests` 与 `wire_golden` 负责。

#[cfg(test)]
pub(super) mod tests {
    use crate::guard_support::production_code;
    use std::collections::BTreeSet;

    /// 中转的目录（相对 `src/backend/`）。
    const RELAY_DIR: &str = "relay";

    /// ㈠ 上游选择那棵树里的文件（相对**上游选择的根**）。**相等，不是地板**。
    // 根从 `accounts/` 收窄到 `accounts/upstream_select/`（现推，不是写死）；
    // 账号库管理（`accounts/manage/`）**不是**上游选择，不进本表 —— 它登记在 [`ACCOUNT_DOMAIN_OTHER_FILES`]。
    // +`file_face.rs`：这台机器上那份凭据文件的帧面读写口（`apikey-key-set` / `apikey-read`）。
    // 它是上游选择自己的状态、同一份文件、同一套格式 ⇒ 住上游选择这棵树；它用到中转的只有 `segment_is_safe`
    // （已在 [`CONTRACT`] 里），接口面一项没变宽。
    // +`endpoint.rs`：起会话那一发走哪、注入什么（`launch-endpoint`）· 界面「表里有没有行」（`apikey-routing`）——
    // 那张表从 monitor 搬来；它用到中转的只多一样 `our_relay_listening`（见 [`CONTRACT`]）。
    const UPSTREAM_SELECTION_FILES: &[&str] = &[
        "creds.rs",
        "endpoint.rs",
        "file_face.rs",
        "mod.rs",
        "policy.rs",
        "table.rs",
    ];

    /// 账号**域**里、上游选择**之外**的那几份（相对账号域根）。
    ///
    /// 账号域根 = 上游选择根的上一级（现推）。域根自己那份 `mod.rs` 只声明两块、不放代码，
    /// 不算任何一块（由 [`the_two_halves_of_the_account_domain_do_not_reference_each_other`] 钉着）。
    const ACCOUNT_DOMAIN_OTHER_FILES: &[&str] = &[
        "manage/aliases.rs",
        "manage/exec.rs",
        // 各号共用的用户级 MCP：只换一个键 · 三方对照（纯）· 读改写 · 文件事件触发。账号库管理的一部分，与上游选择零引用。
        "manage/json_key.rs",
        "manage/layout.rs",
        "manage/mcp_share.rs",
        "manage/mcp_share_exec.rs",
        "manage/mcp_share_watch.rs",
        "manage/mod.rs",
        "manage/model.rs",
        "manage/scan.rs",
        "manage/verify.rs",
        "manage/wire.rs",
        // 订阅号令牌：读 · 续 · 加锁整份写回。上游选择换到别的订阅号时问它要令牌；它不许认识上游选择与账号库管理。
        "oauth/mod.rs",
        "oauth/store.rs",
        // 额度与轮换：额度账（用量的唯一住址）。上游选择可以问它（换号要看各号的额度），它不许认识上游选择与账号库管理。
        "quota/ledger.rs",
        "quota/mod.rs",
    ];

    /// 账号域里**给上游选择用**的那几块（相对账号域根的顶层名）：上游选择可以引它们；
    /// 它们自己不许引上游选择、也不许引账号库管理（否则上游选择经它们绕到了账号库管理）。
    const SERVICE_BLOCKS: &[&str] = &["oauth", "quota"];

    /// ㈣ 上游选择用到的中转的东西（相对 `crate::relay::` 的路径）—— **接口面就这么宽**。
    ///
    /// | 项 | 为什么上游选择要它 |
    /// |---|---|
    /// | `Destinations` · `Destination` · `AuthSwap` · `Mode` · `RouteKey` | 请求路径上那一问一答|
    /// | `Startup` · `Ready` | 启动路径上那两步（起监听前验配置 · 起监听后交出 `Destinations`）|
    /// | `host` | 常驻后端进程内起中转的装配（`accounts::upstream_select::host_relay` 把 `Boot` 递进中转的入口）。依赖方向只许上游选择 → 中转，所以装配住这一侧 |
    /// | `Base` | 一行的上游是什么 —— 中转的**传输原语**，上游选择解析它、焊进行里、原样交回 |
    /// | `segment_is_safe` | 「这个账号 id 当得了路由段吗」与中转切键用的是**同一个谓词**（`route.rs` 头注逐字论证过为什么不许各写一份）|
    ///
    /// ⚠ 后两项经 `relay/mod.rs` 的 `pub(crate) use` 交出去（`upstream` / `route` 两个模块本身仍私有）
    ///   ⇒ 契约面上的每一样都住那一个文件。
    /// ⚠ 多一项 = 上游选择又伸手拿了中转一样东西（要来这里说清为什么）；少一项 = 表腐了。
    const CONTRACT: &[&str] = &[
        // 中转交给上游选择看的那一发请求（流标签 ＋ 请求体）：去处按会话换号要它。
        "Ask",
        "AuthSwap",
        "Base",
        "Destination",
        "Destinations",
        // `ccm` 在最终 exec 那一处问「这一发注入哪个口」：交了 `CCM_RELAY_PORT` 就是那个口（与常驻后端起中转读的同一个变量名）。
        "ENV_PORT",
        "Mode",
        "Ready",
        "RouteKey",
        // 回包头读完那一刻中转交回来的东西（`observe`：额度账按号记账）。
        "Heard",
        "Startup",
        // 常驻后端进程内起中转的入口；上游选择的 `host_relay` 把 `Boot` 递进去。
        "host",
        // 出「直接敲的也走中转」那一段（`relay-optin`）时把这台的钥匙插进地址（只交插好的地址）。
        "keyed_with_key_on_disk",
        // 出成品时问「这台机器上我们的中转在不在听」（`apikey-routing` 的 `running` · 别名预览那一面）。
        "our_relay_listening",
        // `run`（`--relay` 进程的中转入口）随那一形删了。
        "segment_is_safe",
    ];

    // ── 语料 ────────────────────────────────────────────────────────────────

    /// 整个后端 crate 的生产段：`(相对 src/backend 的路径, 生产段文本)`。
    fn crate_production() -> Vec<(String, String)> {
        let root = crate::guard_support::src_root();
        let files: Vec<(String, String)> = guard_core::scan_tree!(&root, &["rs"])
            .into_iter()
            .map(|(p, raw)| {
                // 面 B 成员住 `src/comms/outward/`、由 `lib.rs` 挂成 `relay` ⇒ 按模块住址认（`relay/…`）。
                let rel = guard_core::module_address(&root, &p);
                (rel, production_code(&raw))
            })
            .collect();
        assert!(files.len() >= 30, "只扫到 {} 份 —— 取法坏了", files.len());
        files
    }

    /// ㈠ 上游选择的根（相对 `src/backend/` 的目录，带尾 `/`）—— **从盘上现推**：
    /// 整个 crate 生产段里**唯一**一处 `impl Destinations for` 所在的那个模块。
    ///
    /// ★ 为什么不写死成一个常量：写死的话，「上游选择在哪」与「上游选择有哪几份」两侧同源，
    ///   搬了家而忘了改常量，本文件会继续去扫一个空目录 —— 恒绿。
    fn upstream_selection_root(files: &[(String, String)]) -> String {
        // ⚠ 用有边界的计数（`count_word`），不用裸子串：`impl Destinations for` 的两端都是标识符字符，
        //   裸子串会把 `impl DestinationsForX` 这种认成一处（`needle_anchor_registry` 那条递减棘轮治的正是这一族）。
        let hits: Vec<(&String, usize)> = files
            .iter()
            .map(|(rel, prod)| (rel, count_word(prod, "impl Destinations for")))
            .filter(|(_, n)| *n > 0)
            .collect();
        assert!(
            hits.len() == 1 && hits[0].1 == 1,
            "`impl Destinations for` 应当在生产段里**恰好一处**（上游选择唯一的决策点），实得 {hits:?}"
        );
        let f = hits[0].0;
        let dir = match f.strip_suffix("mod.rs") {
            Some(d) => d.to_string(),
            None => panic!(
                "上游选择的实现住在 `{f}` —— 它不是一个 `mod.rs`，本文件认不出它的「树」。\n\
                 上游选择是一棵树（表 · 凭据 · 热重载），不是一份文件；真改成单文件了就回来改这里。"
            ),
        };
        dir
    }

    /// 上游选择的模块路径（`crate::` 之后的段），由它的根目录现算。
    /// 上游选择的根（盘上现推）交给兄弟判据用 —— `creds_guard` 拿它核
    /// 「中转日志白名单圈的账号那棵，恰好是上游选择这棵子树」，不自己再写第二把推法。
    pub(in crate::relay) fn upstream_selection_root_from_disk() -> String {
        upstream_selection_root(&crate_production())
    }

    /// 上游选择根目录的最后一段（`accounts/upstream_select/` ⇒ `upstream`）：它自己的模块名，现推不写死。
    fn module_name_of_root(root: &str) -> String {
        module_of_dir(root)
            .pop()
            .unwrap_or_else(|| panic!("上游选择根 `{root}` 推不出模块名"))
    }

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

    /// ㈢ 的词表：`accounts` ＋ 上游选择在**顶层**声明的类型 / trait / 常量 / 静态量的名字（现推）。
    ///
    /// ⚠ 只收**类型级**的名字，不收函数名：函数名里有 `build` / `load` / `new` 这类通用词，
    ///   中转自己就有 `.load(SeqCst)` —— 收进来就是假阳，而假阳会训练人绕开判据。
    fn upstream_selection_vocabulary(selection: &[&(String, String)]) -> BTreeSet<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        out.insert("accounts".to_string());
        // 这里原先还硬插上游选择自己的模块名（`apikey`）。模块改名 `upstream` 之后它与中转自己的
        //   `relay::upstream`（传输原语 `Base`）同名 ⇒ 在 ㈢ 里当场假红，**不再插**。射程不变窄：中转要点名
        //   上游选择，路径必经 `accounts`（上一行）；「`use` 进来再以裸模块名用」那一写法的 `use` 行本身也带 `accounts`。
        //   账号域内部那条（账号库管理 ↔ 上游选择）仍要这个名字 ⇒ 由那条判据自己从根目录现推、单独加（[`module_name_of_root`]）。
        for (_, prod) in selection {
            for line in prod.lines() {
                let t = guard_core::strip_visibility(line.trim_start());
                for kw in ["struct ", "enum ", "trait ", "const ", "static ", "type "] {
                    if let Some(rest) = t.strip_prefix(kw) {
                        let name: String = rest.chars().take_while(|c| is_ident(*c)).collect();
                        // `const fn x` 那一形会推出 `fn` —— 关键字不是上游选择的名字。
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

    /// ㈣ 上游选择这批文件用到的中转的项（相对 `crate::relay::`）。
    fn contract_used(selection: &[&(String, String)], sel_mod: &[String]) -> BTreeSet<String> {
        let relay = vec![RELAY_DIR.to_string()];
        let mut out = BTreeSet::new();
        for (rel, prod) in selection {
            for abs in paths_in(rel, prod) {
                if abs.starts_with(&relay) && !abs.starts_with(sel_mod) && abs.len() > 1 {
                    out.insert(abs[1..].join("::"));
                }
            }
        }
        out
    }

    // ── 判据本体 ────────────────────────────────────────────────────────────

    /// ★★★ 「**中转层里不要有账号**」—— 四条两向集合相等（逐条见模块头注那张表）。
    #[test]
    fn the_relay_layer_holds_no_accounts() {
        let files = crate_production();
        let root = upstream_selection_root(&files);
        let sel_mod = module_of_dir(&root);
        let selection: Vec<&(String, String)> = files
            .iter()
            .filter(|(rel, _)| rel.starts_with(&root))
            .collect();

        // ㈠ 上游选择那棵树有哪几份：盘上现推 ⇔ 登记。
        let on_disk: BTreeSet<String> = selection
            .iter()
            .map(|(rel, _)| rel[root.len()..].to_string())
            .collect();
        let registered: BTreeSet<String> = UPSTREAM_SELECTION_FILES
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(
            on_disk, registered,
            "上游选择（根 `{root}`，由唯一那处 `impl Destinations for` 现推）的文件对不上登记。\n\
             加/减了文件就回来改 `UPSTREAM_SELECTION_FILES`，并重读下面三条 —— 它们的人群都从这里来。"
        );

        // ㈡ `relay/` 里住着几份上游选择：**零命中**。反空真：两个人群各自非空（交集为空不是因为扫不到）。
        let relay_files: Vec<&(String, String)> = files
            .iter()
            .filter(|(rel, _)| rel.starts_with(&format!("{RELAY_DIR}/")))
            .collect();
        assert!(
            relay_files.len() >= 7 && !selection.is_empty(),
            "人群取空了：`relay/` 扫到 {} 份、上游选择扫到 {} 份 —— 下面的「交集为空」此刻在空转",
            relay_files.len(),
            selection.len()
        );
        let resident: Vec<&String> = selection
            .iter()
            .map(|(rel, _)| rel)
            .filter(|rel| rel.starts_with(&format!("{RELAY_DIR}/")))
            .collect();
        assert!(
            resident.is_empty() && !root.starts_with(&format!("{RELAY_DIR}/")),
            "🔴 上游选择又住回了中转层底下（根 `{root}`）：{resident:?}\n\
             用户逐字：「中转层不要有账号, 账号就账号中转就中转」。把它挪回 `src/backend/accounts/`。"
        );

        // ㈢ 中转点名上游选择：**零命中**。反空真：词表非空、且同一把尺子在上游选择自己的文件里数得到那些词。
        let vocab = upstream_selection_vocabulary(&selection);
        assert!(
            vocab.len() >= 5 && vocab.contains("Accounts") && vocab.contains("RoutingTable"),
            "上游选择的词表只推出 {vocab:?} —— 推法坏了，下面那条在空转"
        );
        // 词表里除 `accounts`（模块自己的名字，它自己的代码里不必出现）之外，每个词都得在上游选择
        // 自己的生产段里被**同一把尺子**数到 —— 数不到就是尺子瞎了，下面的零命中是空真。
        let unseen: Vec<&String> = vocab
            .iter()
            .filter(|w| *w != "accounts")
            .filter(|w| !selection.iter().any(|(_, prod)| count_word(prod, w) > 0))
            .collect();
        assert!(
            unseen.is_empty(),
            "同一把尺子在上游选择自己的文件里数不到这些词：{unseen:?} —— 尺子是瞎的"
        );
        let mut named: Vec<String> = Vec::new();
        for (rel, prod) in &relay_files {
            for w in &vocab {
                let n = count_word(prod, w);
                if n > 0 {
                    named.push(format!("{rel}: `{w}` ×{n}"));
                }
            }
        }
        assert!(
            named.is_empty(),
            "🔴 中转层（`relay/` 的生产段）点名了上游选择：\n  {}\n\
             **别加例外** —— 把那件事交给上游选择：请求路径走 `Destinations`，启动路径走 `Startup`/`Ready`，\
             进程装配在 `accounts::upstream_select::host_relay`。",
            named.join("\n  ")
        );

        // ㈣ 上游选择用到中转的哪几样：盘上现解析 ⇔ 登记。
        let used = contract_used(&selection, &sel_mod);
        let contract: BTreeSet<String> = CONTRACT.iter().map(|s| (*s).to_string()).collect();
        assert_eq!(
            used, contract,
            "上游选择用到的中转的东西与登记的契约面对不上。\n\
             多出来的 = 上游选择又伸手拿了中转一样东西（来 `CONTRACT` 说清为什么，或者别拿）；\n\
             少了的 = 表腐了。"
        );
    }

    /// 反空真：三把尺子（数词 · 展 `use` · 解析相对路径）在**合成**文本上都认得出它们该认的，
    /// 也**不**把它们不该认的认进来。没有这一条，尺子坏成恒空之后上面那条照样可能绿
    /// （㈢ ㈣ 两侧都空的那一形）。
    #[test]
    fn the_rulers_of_the_upstream_selection_guard_are_not_blind() {
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

    // ── 账号域的两块互不引用 ─────────────────────────────────

    /// `from` 这批文件里，指向 `to_mod` 那棵模块树的路径 ＋ 点名 `to_vocab` 里那些词的地方。
    /// **纯函数**：正控喂合成文本，本体喂盘上的两块。
    fn cross_refs(
        from: &[&(String, String)],
        to_mods: &[Vec<String>],
        to_vocab: &BTreeSet<String>,
    ) -> Vec<String> {
        let mut out = Vec::new();
        for (rel, prod) in from {
            for abs in paths_in(rel, prod) {
                if to_mods.iter().any(|m| abs.starts_with(m)) {
                    out.push(format!("{rel}: 路径 `{}`", abs.join("::")));
                }
            }
            for w in to_vocab {
                let n = count_word(prod, w);
                if n > 0 {
                    out.push(format!("{rel}: `{w}` ×{n}"));
                }
            }
        }
        out
    }

    /// ★★ 「账号就账号, 中转就中转」在账号域**内部**的那一半：给中转当上游选择的 `upstream/`
    /// 与账号库管理（`manage`）**两向零引用**。
    ///
    /// | 格 | 断言 | 反空真 |
    /// |---|---|---|
    /// | 人群 | 域根 = 上游选择根的上一级（现推）；上游选择之外那几份 ⇔ [`ACCOUNT_DOMAIN_OTHER_FILES`]，两向相等 | 两块各自非空 |
    /// | 域根 `mod.rs` | 生产段每一行都是 `pub mod …;`，条数 = 两块的模块数 | —— |
    /// | 账号库管理 → 上游选择 | 零命中（路径 ＋ 上游选择词表） | 同一个 `cross_refs` 喂合成文本必须命中 |
    /// | 上游选择 → 账号库管理 | 零命中（路径 ＋ 账号库管理词表） | 同上 |
    ///
    /// ⚠ 买不到：经第三处（比如 `main.rs`）把两块的值接到一起 —— 那是装配，不是互相认识。
    #[test]
    fn the_two_halves_of_the_account_domain_do_not_reference_each_other() {
        let files = crate_production();
        let sel_root = upstream_selection_root(&files);
        let sel_mod = module_of_dir(&sel_root);
        let domain_root = {
            let mut segs = sel_mod.clone();
            segs.pop();
            assert!(
                !segs.is_empty(),
                "上游选择根 `{sel_root}` 没有上一级 —— 它不在任何账号域底下"
            );
            format!("{}/", segs.join("/"))
        };
        let domain_mod_rs = format!("{domain_root}mod.rs");
        let selection: Vec<&(String, String)> = files
            .iter()
            .filter(|(rel, _)| rel.starts_with(&sel_root))
            .collect();
        let others: Vec<&(String, String)> = files
            .iter()
            .filter(|(rel, _)| {
                rel.starts_with(&domain_root)
                    && !rel.starts_with(&sel_root)
                    && *rel != domain_mod_rs
            })
            .collect();

        // 人群：两向相等 ＋ 两块各自非空。
        let on_disk: BTreeSet<String> = others
            .iter()
            .map(|(rel, _)| rel[domain_root.len()..].to_string())
            .collect();
        let registered: BTreeSet<String> = ACCOUNT_DOMAIN_OTHER_FILES
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(
            on_disk, registered,
            "账号域（根 `{domain_root}`）里上游选择之外的文件对不上登记 —— 加/减了就回来改 \
             `ACCOUNT_DOMAIN_OTHER_FILES`，并想清楚它属于哪一块"
        );
        assert!(
            !selection.is_empty() && !others.is_empty(),
            "有一块是空的 —— 下面的零命中在空转"
        );

        // 域根 `mod.rs` 只声明两块、不放代码。
        let hub = files
            .iter()
            .find(|(rel, _)| *rel == domain_mod_rs)
            .unwrap_or_else(|| panic!("扫不到账号域根 `{domain_mod_rs}`"));
        let hub_lines: Vec<&str> = hub
            .1
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        // 块数 = 上游选择那一块 ＋ 其余文件的顶层名（`manage/…` 算一块）。
        let blocks: BTreeSet<String> = on_disk
            .iter()
            .map(|r| {
                r.split('/')
                    .next()
                    .unwrap_or(r)
                    .trim_end_matches(".rs")
                    .to_string()
            })
            .collect();
        let want_mods = 1 + blocks.len();
        assert!(
            hub_lines.len() == want_mods && hub_lines.iter().all(|l| l.starts_with("pub mod ") && l.ends_with(';')),
            "账号域根 `{domain_mod_rs}` 的生产段应当恰好是 {want_mods} 行 `pub mod …;`，实得 {hub_lines:?} —— \
             放了代码它就成了两块共用的第三处"
        );

        // 账号库管理那一块与「给上游选择用」的那几块分开看。
        let block_of = |rel: &str| {
            rel[domain_root.len()..]
                .split('/')
                .next()
                .unwrap_or("")
                .trim_end_matches(".rs")
                .to_string()
        };
        let services: Vec<&(String, String)> = others
            .iter()
            .copied()
            .filter(|(rel, _)| SERVICE_BLOCKS.contains(&block_of(rel).as_str()))
            .collect();
        let others: Vec<&(String, String)> = others
            .iter()
            .copied()
            .filter(|(rel, _)| !SERVICE_BLOCKS.contains(&block_of(rel).as_str()))
            .collect();
        assert!(
            !services.is_empty()
                && SERVICE_BLOCKS
                    .iter()
                    .all(|b| services.iter().any(|(rel, _)| block_of(rel) == *b)),
            "登记的服务块 {SERVICE_BLOCKS:?} 在盘上找不全 —— 表腐了"
        );

        // 两块的名字。`accounts` 是域名，两块都住在它底下 ⇒ 不作判据词（路径那一半已经分得开）。
        let mut sel_vocab = upstream_selection_vocabulary(&selection);
        sel_vocab.remove("accounts");
        // 上游选择自己的模块名（盘上现推）：账号库管理那一侧不许点它。中转那条（㈢）不收它 —— 见 `upstream_selection_vocabulary` 头注。
        let sel_mod_name = module_name_of_root(&sel_root);
        sel_vocab.insert(sel_mod_name.clone());
        let mut iso_vocab = upstream_selection_vocabulary(&others);
        iso_vocab.remove("accounts");
        let iso_mods: Vec<Vec<String>> =
            others.iter().map(|(rel, _)| module_of_file(rel)).collect();
        for (rel, _) in &others {
            if let Some(stem) = module_of_file(rel).last() {
                iso_vocab.insert(stem.clone());
            }
        }
        assert!(
            sel_vocab.contains(&sel_mod_name)
                && sel_vocab.contains("Accounts")
                && iso_vocab.contains("manage"),
            "词表推空了：上游选择 {sel_vocab:?} · 账号库管理 {iso_vocab:?}"
        );

        // 正控：同一个 `cross_refs` 对合成文本必须命中（两个方向各一刀）。
        let fake_iso = (
            "accounts/manage/wire.rs".to_string(),
            "use crate::accounts::upstream_select::Accounts;\n".to_string(),
        );
        let fake_l2 = (
            "accounts/upstream_select/mod.rs".to_string(),
            "fn f() { super::super::manage::wire::run_change(&[]); }\n".to_string(),
        );
        assert!(
            !cross_refs(&[&fake_iso], &[sel_mod.clone()], &sel_vocab).is_empty(),
            "正控：账号库管理引上游选择没被认出来"
        );
        assert!(
            !cross_refs(&[&fake_l2], &iso_mods, &iso_vocab).is_empty(),
            "正控：上游选择引账号库管理没被认出来"
        );

        let fwd = cross_refs(&others, &[sel_mod.clone()], &sel_vocab);
        assert!(
            fwd.is_empty(),
            "🔴 账号库管理那一块引用了中转的上游选择：\n  {}\n\
             用户逐字「账号就账号, 中转就中转」—— 账号域里给中转当上游选择的那一块，别的块不许认识它。",
            fwd.join("\n  ")
        );
        let back = cross_refs(&selection, &iso_mods, &iso_vocab);
        assert!(
            back.is_empty(),
            "🔴 中转的上游选择引用了账号库管理那一块：\n  {}",
            back.join("\n  ")
        );
        // 服务块：不许引上游选择（依赖方向只许上游选择 → 它们），也不许引账号库管理（不给上游选择留绕过去的路）。
        let up = cross_refs(&services, &[sel_mod.clone()], &sel_vocab);
        assert!(
            up.is_empty(),
            "🔴 账号域的服务块引用了上游选择：\n  {}",
            up.join("\n  ")
        );
        let side = cross_refs(&services, &iso_mods, &iso_vocab);
        assert!(
            side.is_empty(),
            "🔴 账号域的服务块引用了账号库管理：\n  {}",
            side.join("\n  ")
        );
    }
}
