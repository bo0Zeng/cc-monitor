//! U3（2026-08-01）：**分层护栏** —— §1.1 第二条解耦线的机器判据。
//!
//! # 它守的是什么
//!
//! `observe/`（读）与 `control/`（改变世界）**互不引用**。时机在观测侧、做的那一半在控制面的
//! （装 tmux hook · 认起会话便条 · 打身份标）watcher 只报一声，由 `main.rs` 接上控制面的函数（`observe::watcher::Reactions`）。
//!
//! 这条性质没有护栏的话会以最不起眼的方式退化：某天 `fork_write` 需要读一份账号信息，
//! 顺手 `use crate::observe::accounts_query::...` —— 编译通过、测试全绿，而两层从此互相咬死。
//! 处置不是开例外，是把被引用的那个通用工具搬到两层都能用的地方（如带上限的整读住 `platform::fs::Ro`）。
//!
//! 全后端顶层模块之间另有两条：定了方向的那几对只许单向（`ONE_WAY`）· 不成环（`the_top_modules_form_no_ring`）。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#![cfg(test)]

#[cfg(test)]
mod tests {
    use guard_core::production_code;

    /// 收集某一层下所有 `.rs` 的 `(相对路径, 生产段)`。
    fn layer_sources(layer: &str) -> Vec<(String, String)> {
        let root = guard_core::addr::backend_src_root().join(layer);
        layer_sources_at(&root, layer)
    }

    /// [`layer_sources`] 的**根可注入**版本。
    ///
    /// 根可注入，夹具就能让**真判据本身**（不是它的复刻）跑在一棵真的、盘上存在的小树上。
    /// 根写死在函数里的话，夹具只能另写一份扫描，而「另写一份」证明的是那一份、不是护栏。
    ///
    /// `label` 只进报错文本里的相对路径前缀，**不进任何断言**〔断言别取自夹具的名字〕。
    fn layer_sources_at(root: &std::path::Path, label: &str) -> Vec<(String, String)> {
        let layer = label;
        let root = root.to_path_buf();
        let mut out = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read layer dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let src = std::fs::read_to_string(&path).expect("read rs");
                out.push((format!("{layer}/{rel}"), production_code(&src)));
            }
        }
        out.sort();
        out
    }

    /// 抽出生产段里所有指向 `layer` 的引用（去重、排序）。
    ///
    /// # 判据必须同时认三种拼法 —— 少认一种就是安慰剂
    ///
    /// 初版只扫 `crate::<layer>::` 这一种，Phase D 审计当场用三个变异证伪：
    ///
    /// | 拼法 | 初版 | 现在 |
    /// |---|---|---|
    /// | `use crate::observe::accounts_query;` + 短名调用 | 抓到（`use` 行本身含完整路径） | 抓到 |
    /// | **`use crate::observe as ob;`** + `ob::accounts_query::run(..)` | **全绿** | 抓到 |
    /// | **`super::super::observe::accounts_query::run(..)`** | **全绿** | 抓到 |
    ///
    /// # 而那次修法只补了两根针，没有把人群圈出来 —— 于是漏的是**最朴素的一种**
    ///
    /// 接着上表往下变异，三条里两条**全绿**：
    ///
    /// | 拼法 | 上表那版 | 现在 |
    /// |---|---|---|
    /// | **`use crate::observe;`** + `observe::accounts_query::run(..)` | **全绿** | 抓到 |
    /// | **`use crate::{observe, wire};`** | **全绿** | 抓到 |
    /// | `use crate::observe as obs;` | 抓到 | 抓到 |
    ///
    /// 讽刺的地方在于：上一版**专门禁了层别名**，理由逐字写着「建立之后所有用法都绕过本护栏」——
    /// 而 `use crate::observe;` **性质完全一样**（后面全是裸 `observe::…`），只是不用起别名，
    /// 更常见、更省事，却不在针里。⇒ 手写「拼法清单」这件事本身就是漏洞来源：
    /// 补一条只是把清单变长，下一种写法照样在清单外。
    ///
    /// 改成了**从层名派生**：`crate::<layer>` / `super::super::<layer>` 每一处出现都要分类
    ///（后面是 `::` ⇒ 符号路径；否则 ⇒ 模块级引入，与别名同罪），外加成组导入单独一路。
    ///
    /// 中间那一栏不是理论风险：审计在**真放进一条反向边**（control 调 observe 的 `run`）的状态下
    /// 跑了全量 `cargo test`，**199 passed / RC=0**，没有一条门禁叫。
    ///
    /// **这与判 `readonly_guard` 裸文件名匹配有罪是同一类问题** —— 护栏对一种
    /// 完全合法、编译器认账的写法视而不见。自己刚批评过的形状不能自己再犯一遍。
    ///
    /// # 它仍然挡不住什么（如实登记，别再宣称「恰好」而不加限定）
    ///
    /// - **测试段不受管**（下面走 `production_code` 剥掉）。这是**有意**的：分层是生产架构的性质，
    ///   测试跨层构造夹具是正常的。但这个取舍此前一个字都没写 —— 审计变异 M6 证实测试段里
    ///   放一条真反向边全绿。
    /// - 更曲折的间接（把符号先 `pub use` 到第三个模块再引）扫不到。
    ///   真判据得上 `syn` 级解析，成本远超本仓需要；写在这里，别让人以为它是完备的。
    fn refs_to_layer(code: &str, layer: &str) -> Vec<String> {
        let mut hits: Vec<String> = Vec::new();
        // **不再一种拼法一根针，改成从层名派生**（理由见上面那张表末行）。
        //
        // 做法：找出每一处 `crate::<layer>` / `super::super::<layer>`，**按紧随其后的字符分类** ——
        // 后面是 `::` 就是符号路径（照旧抠符号），否则就是**模块级引入**（`;` / `,` / ` as ` / `}`）。
        // 这样「怎么把这一层弄进来」的写法是被**枚举**出来的，不靠人再想起第四种。
        for root in ["crate::", "super::super::"] {
            let anchor = format!("{root}{layer}");
            let mut from = 0usize;
            while let Some(rel) = code[from..].find(&anchor) {
                let i = from + rel;
                from = i + anchor.len();
                let tail = &code[from..];
                // `crate::observed` / `crate::observe_helpers` 不是本层，别误命中。
                if tail.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
                    continue;
                }
                if let Some(rest) = tail.strip_prefix("::") {
                    let end = rest
                        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
                        .unwrap_or(rest.len());
                    let mut sym = format!("crate::{layer}::{}", &rest[..end]);
                    while sym.ends_with(':') {
                        sym.pop();
                    }
                    if !hits.contains(&sym) {
                        hits.push(sym);
                    }
                } else {
                    let mark = format!(
                        "{anchor}（**模块级引入**：引进来之后用法都是裸 `{layer}::…`，\
                         本护栏再也看不见 ⇒ 与层别名同性质，一样禁）"
                    );
                    if !hits.contains(&mark) {
                        hits.push(mark);
                    }
                }
            }
        }
        // 成组导入 `use crate::{observe, wire};` —— 层名被包进花括号，上面的锚点一个都对不上。
        // 实测：这一行放进 `control/gate.rs`，三条判据全绿。
        for prefix in ["use crate::{", "use super::super::{"] {
            let mut from = 0usize;
            while let Some(rel) = code[from..].find(prefix) {
                let i = from + rel;
                from = i + prefix.len();
                let stmt = &code[i..];
                let end = stmt.find(';').map(|e| e + 1).unwrap_or(stmt.len());
                let group = &stmt[..end];
                if group_names_layer(group, layer) {
                    let mark = format!(
                        "{}（**成组导入**：层名在花括号里 ⇒ 按层拆成一行一个，别让它藏在组里）",
                        group.replace('\n', " ")
                    );
                    if !hits.contains(&mark) {
                        hits.push(mark);
                    }
                }
            }
        }
        hits.sort();
        hits
    }

    /// 成组导入里，`layer` 是不是**顶层的一个组员**（`{observe, wire}` 里的 `observe`）。
    ///
    /// 只认前面紧挨着 `{` / `,` / 空白的那种，免得 `crate::{common::observe_helpers}` 误命中。
    fn group_names_layer(group: &str, layer: &str) -> bool {
        let mut from = 0usize;
        while let Some(rel) = group[from..].find(layer) {
            let i = from + rel;
            from = i + layer.len();
            let before_ok = group[..i]
                .chars()
                .next_back()
                .is_some_and(|c| c == '{' || c == ',' || c.is_whitespace());
            let after_ok =
                !group[from..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_');
            if before_ok && after_ok {
                return true;
            }
        }
        false
    }

    /// 采集面自检 —— **照抄 `no_timer_guard` 的做法，不自己另发明一套弱的**。
    ///
    /// # 为什么不用「≥3 文件 / ≥10_000 字节」
    ///
    /// 初版就是那样写的，Phase D 审计两头都点了：
    ///
    /// - **`observe/` 侧太松**：实测 9 文件 / 90_618 字节，余量 9.1 倍 ⇒
    ///   `accounts_query.rs`（16.5KB）**整个掉出采集面，地板照样绿**，反向判据静默失效。
    ///   而同一个仓的 `no_timer_guard` 早就论证过这一点并给了正解，我没沿用。
    /// - **`control/` 侧太紧**：实测 4 文件 / 11_863 字节，余量只有 **1.19 倍**。
    ///   `resolve_query` 已撤 —— 按字节的地板碰上这种撤法，断言当场红，报的却是「**采集坏了，下面的断言是空转**」：
    ///   **一条指向完全错误方向的诊断**，正是在 `matches_registered` 那里批评过的形状。
    ///
    /// ⇒ 改成**数量相等**：独立走一遍目录树数 `.rs`，与采集到的条数比。
    /// 它对「文件增删」免疫（那是正常演进），只对「采集漏了」敏感 —— 而后者才是要防的。
    fn assert_collection_is_complete(layer: &str, files: &[(String, String)]) {
        let root = guard_core::addr::backend_src_root().join(layer);
        assert_collection_is_complete_at(&root, layer, files);
    }

    /// [`assert_collection_is_complete`] 的**根可注入**版本（理由同 [`layer_sources_at`]）。
    fn assert_collection_is_complete_at(
        root: &std::path::Path,
        layer: &str,
        files: &[(String, String)],
    ) {
        let root = root.to_path_buf();
        // 刻意与 `layer_sources` 分开写：那边还要读文件、剥生产段，这边只数个数。
        let mut tree = 0usize;
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read layer dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    tree += 1;
                }
            }
        }
        assert_eq!(
            files.len(),
            tree,
            "{layer}/ 采集到 {} 个 .rs，而树上有 {tree} 个 —— **采集漏了文件**，\
             下面的分层判据对漏掉的那些是瞎的。",
            files.len()
        );
        assert!(
            tree >= 2,
            "{layer}/ 只有 {tree} 个 .rs —— 这一层是不是已经名存实亡了？"
        );
        // 剥法自检：剥完不许残留测试属性。`no_timer_guard`/`build_id_guard`/`readonly_guard`
        // 都有这条，本护栏初版是唯一没有的（Phase D 审计指出）。
        // 它挡的是「剥少了 ⇒ 护栏开始扫测试代码 ⇒ 被夹具打红 ⇒ 有人顺手放宽护栏」。
        for (name, prod) in files {
            guard_core::assert_no_test_code(&format!("{layer}/{name}"), prod);
        }
    }

    /// ★ **零容忍**：`control/` 与 `observe/` 互不引用。
    #[test]
    fn control_and_observe_do_not_reference_each_other() {
        let mut bad: Vec<String> = Vec::new();
        for (layer, other) in [("control", "observe"), ("observe", "control")] {
            let files = layer_sources(layer);
            assert_collection_is_complete(layer, &files);
            for (name, code) in &files {
                for sym in refs_to_layer(code, other) {
                    bad.push(format!("{name} → {sym}"));
                }
            }
        }
        assert!(
            bad.is_empty(),
            "control/ 与 observe/ 之间有引用（§1.1-2 互不引用）：\n  {}\n\
             时机在观测侧、做的那一半在控制面的，watcher 只报一声，由 `main.rs` 接上（`observe::watcher::Reactions`）。\n\
             **先别急着加例外** —— 先问：被引用的那个东西，是不是只是个放错地方的通用工具？\
             是 ⇒ 搬进 common/ 或 platform/，边就没了。",
            bad.join("\n  ")
        );
    }

    /// 后端顶层模块名（`lib.rs` 生产段里的 `mod <名>;`，`#[path]` 挂进来的判据不算），从源码现取。
    fn top_modules() -> Vec<String> {
        let lib = std::fs::read_to_string(guard_core::addr::backend_src_root().join("lib.rs"))
            .expect("读 lib.rs");
        let prod = production_code(&lib);
        let lines: Vec<&str> = prod.lines().map(str::trim).collect();
        lines
            .iter()
            .enumerate()
            .filter(|(i, _)| *i == 0 || !lines[i - 1].starts_with("#[path"))
            .filter_map(|(_, l)| l.strip_prefix("mod "))
            .filter_map(|r| r.split(';').next())
            .map(|n| n.trim().to_string())
            .collect()
    }

    /// 反向自检：判据真的会抓人（喂字符串，不改真文件）。
    #[test]
    fn the_layer_scan_actually_bites() {
        assert_eq!(
            refs_to_layer("let x = crate::observe::watcher::foo();", "observe"),
            vec!["crate::observe::watcher::foo"]
        );
        // 同一个符号出现多次只记一次。
        assert_eq!(
            refs_to_layer("crate::control::a::b; crate::control::a::b;", "control"),
            vec!["crate::control::a::b"]
        );
        // 不该误命中别的层。
        assert!(refs_to_layer("crate::common::fs::read", "observe").is_empty());
        assert!(refs_to_layer("crate::platform::proc::x", "control").is_empty());
        // **模块级引入的三种写法都要认**（上面第二张表）。
        for form in [
            "use crate::observe;",
            "use crate::observe as ob;",
            "use super::super::observe;",
        ] {
            assert!(
                refs_to_layer(form, "observe")
                    .iter()
                    .any(|h| h.contains("模块级引入")),
                "`{form}` 没被判成模块级引入 —— 引进来之后用法都是裸 `observe::…`，扫不到"
            );
        }
        // 成组导入：层名在花括号里。
        assert!(
            refs_to_layer("use crate::{observe, wire};", "observe")
                .iter()
                .any(|h| h.contains("成组导入")),
            "`use crate::{{observe, wire}};` 没被抓到 —— 层名藏在组里，锚点对不上"
        );
        // 反向：名字**只是前缀相同**的模块不许误命中（否则新判据会把好代码判红）。
        assert!(refs_to_layer("use crate::observe_helpers;", "observe").is_empty());
        assert!(refs_to_layer("use crate::{common::observe_helpers};", "observe").is_empty());
    }

    // ═══ 功能之间只许单向（oneway1，10-13）═══════════════════════════════════════════════

    /// **定了方向的那几对**：`(谁, 不许引谁, 为什么是这个方向)`。反方向一处都不许（生产段，量法见 [`top_level_refs`]）。
    ///
    /// 加一行之前先挑方向：被引的那一侧要能不知道引它的那一侧照样成立。
    const ONE_WAY: &[(&str, &str, &str)] = &[
        (
            "accounts",
            "assets",
            "各号共用的 MCP 与账号库同一把锁、同一份清单、同一份共享集合 ⇒ 归账号域；资产那边调它",
        ),
        (
            "agents",
            "footprint",
            "适配层在功能之下：申报表用适配层自己的词（`agents::footprint_decl`），足迹那边读",
        ),
        (
            "accounts",
            "observe",
            "这台此刻活着的会话（`live_here`）住 history；观测侧读账号库出账号清单成品",
        ),
        (
            "agents",
            "control",
            "适配层在功能之下",
        ),
        (
            "assets",
            "footprint",
            "足迹读别名清单（`assets::aliases::read_via`）；「这台有没有 tmux」住控制面（`control::terminals::tmux_here`）",
        ),
        (
            "assets",
            "observe",
            "资产目录要的项目清单与 agent 家住 history（`history::listing`）",
        ),
        (
            "footprint",
            "faces",
            "faces 是帧面宿主，功能不往上引；看哪几个家（`mcp_look`）住账号域",
        ),
        (
            "dial",
            "control",
            "机器表的读法由 `faces::install_machines` 装进拨号层；控制面经 dial 拨号",
        ),
        (
            "history",
            "observe",
            "会话清单 · 叫会话什么 · 判活住 history；observe 的记录级读与 watcher 反过来调它",
        ),
        (
            "accounts",
            "control",
            "ccm 这一行的词与解析住 `ccm_line`（谁都往下引）；起会话那一侧读账号库",
        ),
        (
            "assets",
            "control",
            "别名 / 配置那一族只认 `ccm_line`；探 tmux · 渲 ccm 那一行由 `faces::profiles_face` 交进来",
        ),
        (
            "accounts",
            "faces",
            "faces 是账号库与上游选择接头的那一处：账号库由 faces 读好交进来（`BookAt`）",
        ),
        (
            "host",
            "control",
            "宿主口在两层之下：控制面经它对终端下手（起 tmux 只在 `host/tmux.rs`）",
        ),
        (
            "host",
            "observe",
            "宿主口在两层之下：观测面经它探 tmux（`tmux ls` · `display-message` 那两跳）",
        ),
        // 读一份记录的四层（`observe` 里头）：读行 → 认行 → 扫描图与骨架 → 查询，跟读在最上；只许往下引。
        (
            "observe::lines",
            "observe::recognize · record_scan · skeleton · query · watcher",
            "读行在最下：一行行读 · 算不算一行 · 一行怎么解析",
        ),
        (
            "observe::recognize",
            "observe::record_scan · skeleton · query · watcher",
            "认行只认给它的那一行，不碰文件、不问扫描图",
        ),
        (
            "observe::record_scan",
            "observe::query · watcher",
            "扫描图一份记录扫一趟，查询与跟读都来要它",
        ),
        (
            "observe::query",
            "observe::watcher",
            "查询只读扫描图；跟读起步也问扫描图",
        ),
    ];

    /// 帧面的协议零件（`wire`）只许引的那几个顶层模块：它们自己也只引 `platform` / `common`。
    const WIRE_MAY_USE: &[(&str, &str)] = &[
        ("platform", "平台原语"),
        ("common", "两边都要的纯工具"),
        (
            "agents",
            "帧里带的那几样适配层的词（记录 · 块的种类 · 活动）",
        ),
        ("relay", "中转口那一个环境变量名"),
    ];

    /// 一份文件的模块路径（相对源码根，`assets/aliases/mod.rs` ⇒ `[assets, aliases]`）。
    fn module_path_of(rel: &str) -> Vec<String> {
        let mut parts: Vec<String> = rel
            .trim_end_matches(".rs")
            .split('/')
            .map(str::to_string)
            .collect();
        if parts.last().is_some_and(|l| l == "mod") {
            parts.pop();
        }
        parts
    }

    fn is_ident_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_'
    }

    /// 一段生产段里**引到的顶层模块**（每出现一次记一次）：`crate::<模块>` · 走到根的 `super::…::<模块>` ·
    /// 成组导入 `use crate::{a, b::c}`（花括号第一层的每一员）。`me` 是这份文件的模块路径。
    fn top_level_refs(me: &[String], code: &str) -> Vec<String> {
        let mut out = Vec::new();
        let ident_at =
            |s: &str| -> String { s.chars().take_while(|c| is_ident_char(*c)).collect() };
        let before_ok = |i: usize| {
            code[..i]
                .chars()
                .next_back()
                .is_none_or(|c| !is_ident_char(c) && c != ':')
        };
        // 成组：花括号第一层每一员的第一段。
        let group = |rest: &str, out: &mut Vec<String>| {
            let mut depth = 0usize;
            let mut start = true;
            for (k, c) in rest.char_indices() {
                match c {
                    '{' => {
                        depth += 1;
                        start = false;
                    }
                    '}' if depth == 0 => break,
                    '}' => depth -= 1,
                    ',' if depth == 0 => start = true,
                    c if start && is_ident_char(c) => {
                        out.push(ident_at(&rest[k..]));
                        start = false;
                    }
                    c if !c.is_whitespace() => start = false,
                    _ => {}
                }
            }
        };
        let mut from = 0usize;
        while let Some(rel) = code[from..].find("crate::") {
            let i = from + rel;
            from = i + "crate::".len();
            if !before_ok(i) {
                continue;
            }
            let rest = &code[from..];
            if let Some(g) = rest.strip_prefix('{') {
                group(g, &mut out);
            } else {
                let name = ident_at(rest);
                if !name.is_empty() {
                    out.push(name);
                }
            }
        }
        let mut from = 0usize;
        while let Some(rel) = code[from..].find("super::") {
            let i = from + rel;
            if !before_ok(i) {
                from = i + 1;
                continue;
            }
            let mut ups = 0usize;
            let mut j = i;
            while code[j..].starts_with("super::") {
                ups += 1;
                j += "super::".len();
            }
            from = j;
            if ups < me.len() {
                continue;
            }
            let rest = &code[j..];
            if let Some(g) = rest.strip_prefix('{') {
                group(g, &mut out);
            } else {
                let name = ident_at(rest);
                if !name.is_empty() {
                    out.push(name);
                }
            }
        }
        out
    }

    /// 父模块是不是写着 `#[cfg(test)] mod <名>;`（整份只在测试档）。
    fn declared_test_only(parent_src: &str, name: &str) -> bool {
        let lines: Vec<&str> = parent_src.lines().collect();
        for (k, l) in lines.iter().enumerate() {
            let t = guard_core::strip_visibility(l.trim_start());
            if t.trim_end() != format!("mod {name};") {
                continue;
            }
            let mut b = k;
            while b > 0 {
                b -= 1;
                let p = lines[b].trim();
                if p == "#[cfg(test)]" {
                    return true;
                }
                if !(p.starts_with("#[") || p.starts_with("//")) {
                    break;
                }
            }
        }
        false
    }

    /// `lib.rs` 根上转出去的名字：`pub use <模块>::…::<名>;`（含 `pub(crate)` · `as 别名`）⇒ `名 → 模块`。
    /// `crate::<名>` 照它记到那个模块头上（不然经根转一手就躲过了量法）。
    /// ⚠ `lib.rs` 里**自己定义**、内含别的模块类型的这里看不见：那一形手理（const1 起根上不再有这样的，`StreamWants` 挪去 `stream::inbound::views`）。
    fn root_reexports(tops: &[String]) -> std::collections::BTreeMap<String, String> {
        let lib = std::fs::read_to_string(guard_core::addr::backend_src_root().join("lib.rs"))
            .expect("读 lib.rs");
        let mut out = std::collections::BTreeMap::new();
        for l in production_code(&lib).lines() {
            let t = guard_core::strip_visibility(l.trim_start());
            let Some(path) = t.strip_prefix("use ") else {
                continue;
            };
            let path = path.split(';').next().unwrap_or("").trim();
            if path.contains('{') {
                continue;
            }
            let (path, alias) = match path.split_once(" as ") {
                Some((p, a)) => (p.trim(), Some(a.trim())),
                None => (path, None),
            };
            let path = path.trim_start_matches("crate::");
            let first = path.split("::").next().unwrap_or("");
            if !tops.iter().any(|m| m == first) {
                continue;
            }
            let name = alias.unwrap_or_else(|| path.rsplit("::").next().unwrap_or(""));
            out.insert(name.to_string(), first.to_string());
        }
        out
    }

    /// 全后端两两引用的次数：`(谁, 引谁) → (处, 份)`。人群＝各顶层模块目录下全部 `.rs`，减去父模块写明只在测试档的。
    fn reference_matrix() -> std::collections::BTreeMap<(String, String), (usize, usize)> {
        let root = guard_core::addr::backend_src_root();
        let tops = top_modules();
        let via_root = root_reexports(&tops);
        let mut m: std::collections::BTreeMap<(String, String), (usize, usize)> =
            Default::default();
        let mut seen = 0usize;
        for top in &tops {
            let dir = root.join(top);
            if !dir.is_dir() {
                continue;
            }
            let mut stack = vec![dir];
            while let Some(d) = stack.pop() {
                for entry in std::fs::read_dir(&d).expect("read dir") {
                    let path = entry.expect("dir entry").path();
                    if path.is_dir() {
                        stack.push(path);
                        continue;
                    }
                    if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                        continue;
                    }
                    let rel = path
                        .strip_prefix(&root)
                        .expect("under src root")
                        .to_string_lossy()
                        .replace('\\', "/");
                    let me = module_path_of(&rel);
                    // 祖先里有一层是测试档 ⇒ 整份不算。
                    let test_only = (1..me.len()).any(|k| {
                        let parent = &me[..k];
                        let dir = root.join(parent.join("/"));
                        let p1 = dir.join("mod.rs");
                        let p2 = root.join(format!("{}.rs", parent.join("/")));
                        [p1, p2].iter().any(|p| {
                            std::fs::read_to_string(p).is_ok_and(|s| declared_test_only(&s, &me[k]))
                        })
                    });
                    if test_only {
                        continue;
                    }
                    seen += 1;
                    let src = std::fs::read_to_string(&path).expect("read rs");
                    let mut per: std::collections::BTreeMap<String, usize> = Default::default();
                    for t in top_level_refs(&me, &production_code(&src)) {
                        let t = via_root.get(&t).cloned().unwrap_or(t);
                        if t != me[0] && tops.contains(&t) {
                            *per.entry(t).or_default() += 1;
                        }
                    }
                    for (t, n) in per {
                        let e = m.entry((me[0].clone(), t)).or_default();
                        e.0 += n;
                        e.1 += 1;
                    }
                }
            }
        }
        assert!(
            seen > 300,
            "只采到 {seen} 份 .rs —— 采集坏了，下面的判定是空转"
        );
        m
    }

    /// 一个顶层模块里头的一层（`top/layer.rs` 或 `top/layer/` 底下）引了同一顶层里哪几个不许引的兄弟：
    /// `not` 写作 `top::a · b · c`。认 `crate::top::x` · `super::x`（顶层那一级的文件）· `super::super::x`（下一级目录里的文件）。
    fn inner_refs(top: &str, layer: &str, not: &str) -> Vec<(usize, String, String)> {
        let root = guard_core::addr::backend_src_root().join(top);
        let targets: Vec<&str> = not
            .strip_prefix(&format!("{top}::"))
            .expect("`not` 以同一顶层开头")
            .split(" · ")
            .collect();
        let mut files = vec![root.join(format!("{layer}.rs"))];
        let mut stack = vec![root.join(layer)];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    files.push(p);
                }
            }
        }
        files.retain(|p| p.is_file());
        assert!(
            !files.is_empty(),
            "`{top}::{layer}` 一份文件都没有 —— 这一行是空转"
        );
        let mut out = Vec::new();
        for p in files {
            let nested = p.parent() != Some(root.as_path());
            let up = if nested { "super::super::" } else { "super::" };
            let layer_src = production_code(&std::fs::read_to_string(&p).expect("read rs"));
            for t in &targets {
                let n = [format!("crate::{top}::{t}"), format!("{up}{t}")]
                    .iter()
                    .map(|needle| {
                        layer_src
                            .match_indices(needle.as_str())
                            .filter(|(i, _)| {
                                let after = layer_src[i + needle.len()..].chars().next();
                                let before = layer_src[..*i].chars().next_back();
                                !after.is_some_and(is_ident_char)
                                    && !before.is_some_and(|c| is_ident_char(c) || c == ':')
                            })
                            .count()
                    })
                    .sum::<usize>();
                if n > 0 {
                    let rel = p.strip_prefix(&root).unwrap_or(&p).display().to_string();
                    out.push((n, rel, t.to_string()));
                }
            }
        }
        out
    }

    /// ★ **功能之间只许单向**：定了方向的那几对反方向零处；任何两个顶层模块都不许互相引用。
    #[test]
    fn features_reference_each_other_one_way_only() {
        let m = reference_matrix();
        let count = |a: &str, b: &str| m.get(&(a.to_string(), b.to_string())).copied();
        let mut bad = Vec::new();
        for (who, not, why) in ONE_WAY {
            if let Some((top, layer)) = who.split_once("::") {
                for (n, file, to) in inner_refs(top, layer, not) {
                    bad.push(format!(
                        "{who} → {top}::{to}：{n} 处（{file}；{who} 在 {to} 之下：{why}）"
                    ));
                }
                continue;
            }
            if let Some((n, files)) = count(who, not) {
                bad.push(format!(
                    "{who} → {not}：{n} 处 / {files} 份（方向定的是 {not} → {who}：{why}）"
                ));
            }
        }
        assert!(
            bad.is_empty(),
            "后端模块之间的方向不对：\n  {}",
            bad.join("\n  ")
        );
    }

    /// 引用图上的环：互相走得到的顶层模块各成一组，只留多于一个的那几组（组内按名排，组按名排）。两个模块互相引用是两元的环，也在里面。
    fn rings(m: &std::collections::BTreeMap<(String, String), (usize, usize)>) -> Vec<Vec<String>> {
        use std::collections::{BTreeMap, BTreeSet};
        let mut out_of: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for (a, b) in m.keys() {
            out_of.entry(a).or_default().insert(b);
            out_of.entry(b).or_default();
        }
        let reach_of = |from: &str| -> BTreeSet<&str> {
            let mut seen = BTreeSet::new();
            let mut stack = vec![from];
            while let Some(n) = stack.pop() {
                for &t in &out_of[n] {
                    if seen.insert(t) {
                        stack.push(t);
                    }
                }
            }
            seen
        };
        let reach: BTreeMap<&str, BTreeSet<&str>> =
            out_of.keys().map(|&n| (n, reach_of(n))).collect();
        let mut groups: BTreeSet<Vec<String>> = BTreeSet::new();
        for (&a, from_a) in &reach {
            let mut g: Vec<String> = from_a
                .iter()
                .filter(|&&b| reach[b].contains(a))
                .map(|b| b.to_string())
                .collect();
            if !g.iter().any(|x| x == a) {
                g.push(a.to_string());
            }
            g.sort();
            if g.len() > 1 {
                groups.insert(g);
            }
        }
        groups.into_iter().collect()
    }

    /// ★ **顶层模块之间不成环**：两两单向还不够 —— a → b → c → a 每一对都只有一个方向，却绕回了自己。
    /// 环上的模块谁也离不开谁，拆不成各自的 crate。
    #[test]
    fn the_top_modules_form_no_ring() {
        let m = reference_matrix();
        let bad: Vec<String> = rings(&m)
            .iter()
            .map(|g| {
                let edges: Vec<String> = m
                    .iter()
                    .filter(|((a, b), _)| g.contains(a) && g.contains(b))
                    .map(|((a, b), (n, files))| format!("{a} → {b} {n} 处 / {files} 份"))
                    .collect();
                format!("{{{}}}：{}", g.join(", "), edges.join(" · "))
            })
            .collect();
        assert!(
            bad.is_empty(),
            "后端顶层模块成环（环上每一对都可能只有一个方向）：\n  {}\n  \
             拆法：在环上挑一条边，被引的那一头要的「读」挪到下面一层，「写」留在原处；挑定了方向再加一行到 `ONE_WAY` 钉住",
            bad.join("\n  ")
        );
    }

    /// ★ **命令表与接线在最上、协议零件在最下**：
    /// - `stream`（命令表 · 两面的分派 · 门 · 期限 · 出口声明 · tee 消费侧）谁都不许引 —— 只有 `main.rs` / `lib.rs` 接它；
    /// - `wire`（帧 · typed 命令 · 登记形状 · 复制详情 · 码 → 句 · 主题表 · 监听判定 · 收场闸）只许引 [`WIRE_MAY_USE`]。
    #[test]
    fn the_registry_sits_on_top_and_the_protocol_parts_at_the_bottom() {
        let m = reference_matrix();
        let tops = top_modules();
        assert!(
            tops.iter().any(|t| t == "stream") && tops.iter().any(|t| t == "wire"),
            "`lib.rs` 里找不到 `stream` / `wire` —— 下面两条是空转"
        );
        let mut bad = Vec::new();
        for ((who, to), (n, files)) in &m {
            if to == "stream" {
                bad.push(format!(
                    "{who} → stream：{n} 处 / {files} 份（命令表与接线在最上，功能不引它；要的是协议零件就去 `wire`）"
                ));
            }
            if who == "wire" && !WIRE_MAY_USE.iter().any(|(t, _)| t == to) {
                bad.push(format!(
                    "wire → {to}：{n} 处 / {files} 份（协议零件只许引 platform · common · agents · relay）"
                ));
            }
        }
        for (low, why) in WIRE_MAY_USE {
            for ((who, to), (n, _)) in &m {
                if who == low && !["platform", "common"].contains(&to.as_str()) {
                    bad.push(format!(
                        "{low} → {to}：{n} 处 —— {low} 不再只引 platform / common，`wire` 引它（{why}）就不在最下了"
                    ));
                }
            }
        }
        assert!(bad.is_empty(), "层次不对：\n  {}", bad.join("\n  "));
    }

    /// 反向自检：量法认得 `crate::` · 走到根的 `super::` · 成组导入，不认没走到根的 `super::` 与只是后缀相同的路径。
    #[test]
    fn the_reference_matrix_scan_actually_bites() {
        let me = |s: &str| module_path_of(s);
        assert_eq!(me("assets/aliases/mod.rs"), vec!["assets", "aliases"]);
        assert_eq!(
            top_level_refs(&me("history/x.rs"), "let a = crate::observe::q::f();"),
            vec!["observe"]
        );
        assert_eq!(
            top_level_refs(&me("history/x.rs"), "use super::super::observe::q;"),
            vec!["observe"]
        );
        assert_eq!(
            top_level_refs(&me("history/mod.rs"), "use super::observe::q;"),
            vec!["observe"]
        );
        // 没走到根：`history/x.rs` 里的 `super::q` 是 history 自己。
        assert!(top_level_refs(&me("history/x.rs"), "use super::listing::f;").is_empty());
        assert_eq!(
            top_level_refs(
                &me("history/x.rs"),
                "use crate::{observe::{a, b}, common::said, stream};"
            ),
            vec!["observe", "common", "stream"]
        );
        assert!(top_level_refs(&me("history/x.rs"), "use self::crate_ish::observe;").is_empty());
        assert!(declared_test_only(
            "#[cfg(test)]\npub(crate) mod fake;\n",
            "fake"
        ));
        assert!(!declared_test_only("pub(crate) mod fake;\n", "fake"));
        // 认环：三个首尾相接 ⇒ 一个三元环（挂在环外、只往里引的那一个不算）；断一条 ⇒ 没有；两个互相引用 ⇒ 两元环。
        let graph = |edges: &[(&str, &str)]| {
            edges
                .iter()
                .map(|(a, b)| ((a.to_string(), b.to_string()), (1usize, 1usize)))
                .collect::<std::collections::BTreeMap<_, _>>()
        };
        let three = [("a", "b"), ("b", "c"), ("c", "a"), ("d", "a")];
        assert_eq!(rings(&graph(&three)), vec![vec!["a", "b", "c"]]);
        assert!(rings(&graph(&three[1..])).is_empty());
        assert_eq!(
            rings(&graph(&[("a", "b"), ("b", "a"), ("c", "d")])),
            vec![vec!["a", "b"]]
        );
        // 根上转出：`crate::REPLY_PAGE_BYTES` 记到它住的那个模块头上（此刻 lib.rs 里就有这一行）。
        let via = root_reexports(&top_modules());
        assert_eq!(
            via.get("REPLY_PAGE_BYTES").map(String::as_str),
            Some("wire")
        );
    }
}
