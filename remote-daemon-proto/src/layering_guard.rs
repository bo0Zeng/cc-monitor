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
//! 允许跨层的那条边今天**恰好一个符号**：`watcher` 调 `control::tmux_hook::install_hooks`。
//! 它有一个具体的、说得清的理由（tmux hook 活在 server 内存里、每次 server 重起要重装，
//! 而「server 起来了」只有 observe 知道）。**「有一个正当例外」与「这条线随便穿」是两回事**，
//! 中间隔着的就是这个计数。多一个就红，逼下一个人把他的理由也写出来。
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
    const ALLOWED_OBSERVE_TO_CONTROL: &[&str] = &["crate::control::tmux_hook::install_hooks"];

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
        let mut hits = refs_by_needle(code, &format!("crate::{layer}::"));
        // `super::super::<layer>::` —— 从 `observe/x.rs` 看，`super::super` 就是 crate 根。
        for h in refs_by_needle(code, &format!("super::super::{layer}::")) {
            let norm = h.replacen(
                &format!("super::super::{layer}::"),
                &format!("crate::{layer}::"),
                1,
            );
            if !hits.contains(&norm) {
                hits.push(norm);
            }
        }
        // `use crate::<layer> as X;` —— 别名一旦建立，后面怎么用都扫不到，所以**禁掉别名本身**。
        for pat in [
            format!("use crate::{layer} as "),
            format!("use super::super::{layer} as "),
        ] {
            if code.contains(&pat) {
                hits.push(format!(
                    "{pat}…（**层别名**：建立之后所有用法都绕过本护栏 ⇒ 直接禁）"
                ));
            }
        }
        hits.sort();
        hits
    }

    /// 按一个完整前缀抽路径。
    fn refs_by_needle(code: &str, needle: &str) -> Vec<String> {
        let mut hits: Vec<String> = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = code[from..].find(needle) {
            let i = from + rel;
            let tail = &code[i..];
            let end = tail
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
                .unwrap_or(tail.len());
            let mut sym = tail[..end].to_string();
            while sym.ends_with(':') {
                sym.pop();
            }
            if !hits.contains(&sym) {
                hits.push(sym);
            }
            from = i + needle.len();
        }
        hits
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
    }
}
