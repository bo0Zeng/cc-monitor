//! U3（2026-08-01）：**分层护栏** —— §1.1 第二条解耦线的机器判据。
//!
//! # 它守的是什么
//!
//! `observe/`（读）与 `control/`（改变世界）之间**只许一个方向**：
//! `observe → control`，且**接口面必须显式列举、条数被钉住**；反向一条都不许。
//!
//! 这条性质没有护栏的话会以最不起眼的方式退化：某天 `fork_write` 需要读一份账号信息，
//! 顺手 `use crate::observe::accounts_query::...` —— 编译通过、测试全绿，而两层从此互相咬死。
//! U3 摸底时**真的就有这么一条**（`fork_write` → `accounts_query::read_regular_capped`），
//! 处置不是开例外，是把那个函数搬进 `common/`（它本来就不是 observe 的域逻辑）。
//!
//! # 为什么正向要**钉条数**而不是「随便跨」
//!
//! 允许跨层的边今天**恰好两个符号**，都由 `watcher` 发起、都有具体说得清的理由：
//! `control::tmux_hook::install_hooks`（tmux hook 活在 server 内存里、每次 server 重起要重装，
//! 而「server 起来了」只有 observe 知道）与 `control::identity_tag::tag`（`U-NP④`：
//! `(pid, sid)` 只在 pidfile 事件那一刻同时在手，让 control 自己去发现只能靠轮询，
//! 而消灭轮询正是那件事的全部目的）。
//! **「有正当例外」与「这条线随便穿」是两回事**，中间隔着的就是这个计数。
//! 多一个就红，逼下一个人把他的理由也写出来。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#![cfg(test)]

#[cfg(test)]
mod tests {
    use crate::guard_support::production_code;

    /// 允许的 `observe → control` 跨层引用，逐条列举。
    ///
    /// **加一条之前先回答**：为什么这件事非得由观测侧发起？能不能反过来由 control 主动做？
    /// （`install_hooks` 的答案：不能 —— 触发时机是「tmux server 起来了」，
    /// 那是 socket 目录 inotify 观测到的事实，control 侧没有这个信号，
    /// 硬要它自己发现只能靠轮询，与 §41 零定时器铁律正面冲突。）
    ///
    /// `identity_tag::tag`（`U-NP④`，08-14）的答案**同型**：触发时机是「某个 pidfile 出现/
    /// 原地换了 sid」，那是 `sessions/` inotify 观测到的事实（`(pid, sid)` 也只有那一刻同时在手）；
    /// control 侧没有这个信号，硬要它自己发现只能靠轮询 —— 而本件的**全部目的**就是
    /// 把 `shared/ccm` 那条每秒轮询消掉（用户 08-14：「不要轮询」「ccm 做到必须走 daemon」）。
    /// 反过来做只是把轮询从 ccm 搬到 daemon。
    const ALLOWED_OBSERVE_TO_CONTROL: &[&str] = &[
        "crate::control::identity_tag::tag",
        "crate::control::tmux_hook::install_hooks",
    ];

    /// 收集某一层下所有 `.rs` 的 `(相对路径, 生产段)`。
    fn layer_sources(layer: &str) -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join(layer);
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
    /// 〔audit-0805 08-06〕接着上表往下变异，三条里两条**全绿**：
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
    /// 本轮改成**从层名派生**：`crate::<layer>` / `super::super::<layer>` 每一处出现都要分类
    ///（后面是 `::` ⇒ 符号路径；否则 ⇒ 模块级引入，与别名同罪），外加成组导入单独一路。
    ///
    /// 中间那一栏不是理论风险：审计在**真放进一条反向边**（control 调 observe 的 `run`）的状态下
    /// 跑了全量 `cargo test`，**199 passed / RC=0**，没有一条门禁叫。
    ///
    /// **这与本轮判 `readonly_guard` 裸文件名匹配有罪是同一类问题** —— 护栏对一种
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
        // 〔audit-0805 08-06〕**不再一种拼法一根针，改成从层名派生**（理由见上面那张表末行）。
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
    ///   而账本 S14 写明 `resolve_query` 要在 U6/U8 被吸收进计划面 —— 它一走 control 只剩
    ///   6_522 字节，断言当场红，报的却是「**采集坏了，下面的断言是空转**」：
    ///   **一条指向完全错误方向的诊断**，正是本轮在 `matches_registered` 那里刚批评过的形状。
    ///
    /// ⇒ 改成**数量相等**：独立走一遍目录树数 `.rs`，与采集到的条数比。
    /// 它对「文件增删」免疫（那是正常演进），只对「采集漏了」敏感 —— 而后者才是要防的。
    fn assert_collection_is_complete(layer: &str, files: &[(String, String)]) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join(layer);
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
            crate::guard_support::assert_no_test_code(&format!("{layer}/{name}"), prod);
        }
    }

    /// ★ **反向零容忍**：`control/` 不许引用 `crate::observe`。
    #[test]
    fn control_layer_must_not_reference_observe() {
        let files = layer_sources("control");
        assert_collection_is_complete("control", &files);
        let mut bad: Vec<String> = Vec::new();
        for (name, code) in &files {
            for sym in refs_to_layer(code, "observe") {
                bad.push(format!("{name} → {sym}"));
            }
        }
        assert!(
            bad.is_empty(),
            "control/ 引用了 observe（§1.1-2 反向不许）：\n  {}\n\
             **先别急着加例外** —— U3 摸底时那条反向边（fork_write → accounts_query::read_regular_capped）\
             的正解是「那个函数根本不属于 observe」，搬进 common/ 之后边就没了。\n\
             先问：被引用的那个东西，是不是也只是个放错地方的通用工具？",
            bad.join("\n  ")
        );
    }

    /// ★ **正向要显式列举且条数钉死**：`observe/` 只许用登记过的那几个 control 符号。
    #[test]
    fn observe_to_control_interface_is_exactly_the_registered_set() {
        let files = layer_sources("observe");
        assert_collection_is_complete("observe", &files);
        let mut found: Vec<String> = Vec::new();
        for (_, code) in &files {
            for sym in refs_to_layer(code, "control") {
                if !found.contains(&sym) {
                    found.push(sym);
                }
            }
        }
        found.sort();
        let mut want: Vec<String> = ALLOWED_OBSERVE_TO_CONTROL
            .iter()
            .map(|s| s.to_string())
            .collect();
        want.sort();
        // S1（Phase D 审计）：**登记项必须钉到函数级**。
        //
        // 审计的变异 M5 显示：`use crate::control::tmux_hook;`（模块级）会被记成一个新条目
        // `crate::control::tmux_hook`。下一个人「修红」最省事的办法就是把它加进表里 ——
        // 从此 `tmux_hook` 的**任意函数**都能被 observe 调，而计数仍是「2 条」、护栏再无信号。
        // ⇒ 直接禁掉模块级登记：`crate::<layer>::` 之后必须有 ≥2 段。
        for e in ALLOWED_OBSERVE_TO_CONTROL {
            let tail = e
                .strip_prefix("crate::control::")
                .unwrap_or_else(|| panic!("登记项必须以 `crate::control::` 开头：{e}"));
            assert!(
                tail.contains("::"),
                "登记项 `{e}` 只钉到**模块级** —— 那等于把整个模块的接口面都放开，\
                 而计数看不出区别。必须钉到函数：`crate::control::<模块>::<函数>`。"
            );
        }
        assert_eq!(
            found, want,
            "observe → control 的接口面与登记表对不上。\n\
             **多出来的**：加进 `ALLOWED_OBSERVE_TO_CONTROL` 之前先回答「为什么这件事非得由观测侧发起、\
             control 能不能自己做」——那张表的头注写着 `install_hooks` 的答案长什么样。\n\
             **少了的**：说明那条跨层调用没了，清理登记（别留着，登记表腐烂比没有登记更糟）。"
        );
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
        // 〔audit-0805 08-06〕**模块级引入的三种写法都要认**（上面第二张表）。
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
}
