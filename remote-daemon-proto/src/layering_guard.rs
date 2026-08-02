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

    /// 抽出生产段里所有 `crate::<layer>::…` 形态的路径（去重、排序）。
    fn refs_to_layer(code: &str, layer: &str) -> Vec<String> {
        let needle = format!("crate::{layer}::");
        let mut hits: Vec<String> = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = code[from..].find(&needle) {
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
        hits.sort();
        hits
    }

    /// 采集面不许是空的 —— 否则下面两条断言全是空转。
    fn assert_non_empty(layer: &str, files: &[(String, String)]) {
        let bytes: usize = files.iter().map(|(_, c)| c.len()).sum();
        assert!(
            files.len() >= 3 && bytes > 10_000,
            "{layer}/ 只扫到 {} 个文件 / {bytes} 字节 —— 采集坏了，下面的断言是空转",
            files.len()
        );
    }

    /// ★ **反向零容忍**：`control/` 不许引用 `crate::observe`。
    #[test]
    fn control_layer_must_not_reference_observe() {
        let files = layer_sources("control");
        assert_non_empty("control", &files);
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
        assert_non_empty("observe", &files);
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
