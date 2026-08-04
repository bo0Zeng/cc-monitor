//! F03（定框 C1/C6）：**§34 Gate 2 的身份判定在 Rust 侧只许有一个实现**。
//!
//! # 为什么这条必须机检
//!
//! Gate 2 挡的是「往一个不是本工具管理的 tmux 会话里打字 / 把它杀掉」。
//! F03 之前它只活在 monitor 的 `tmux.rs`（一个私有 fn）；F03 把它收进
//! `gate-core`，让 daemon 的 `control/gate.rs` 调**同一份**。
//!
//! 这道门一旦有第二份实现，两边就会各自漂 —— 而**漂了不会红**：
//! 两侧各自的测试都通过，只是对同一个会话名给出不同答案。
//! 这正是 `quote_singleton_guard` 那条的病史（收口前**五份逐字节相同**、
//! 从来没红过、靠巧合保持一致，连「到底有几份」人数的和机器数的都不一样）。
//! 身份门比 quote 更值得这条：quote 漂了是转义 bug，**身份门漂了是安全洞**。
//!
//! # 它查什么、查不了什么
//!
//! 查两个**判定形状**的字面量：`cc-` 前缀判定与 `-cc-<N>` 撞名后缀判定。
//! 实测（F03 落地时）：全仓 Rust 侧各只有 **1 处**，就在 `gate-core`。
//!
//! ⚠ **查不了「换个写法的等价实现」**（比如用正则、或逐 char 手写）——
//! 与本仓其它约定型守卫同一档。**比没有强，别读成证明。**
//!
//! ⚠ **查不了 TS 侧**。实测前端今天**没有**同族实现（`startsWith("cc-")` 零命中），
//! 所以不是「漏了」而是「今天不存在」；真出现了，本条也照样不会红。
//! 这条登记在功能件 F03 的诚实边界里。

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// 唯一允许持有这个判定的文件（相对仓根）。
    const SOLE_HOME: &str = "src-tauri/crates/gate-core/src/lib.rs";

    /// 判定形状的源码指纹。**运行时拼**，免得本文件自己被扫到时命中。
    fn fingerprints() -> Vec<String> {
        let cc = "cc";
        vec![
            format!("starts_with(\"{cc}-\")"),
            format!("rsplit_once(\"-{cc}-\")"),
        ]
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .to_path_buf()
    }

    fn rust_sources(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for base in [
            "src-tauri/src",
            "src-tauri/crates",
            "remote-daemon-proto/src",
        ] {
            walk(&root.join(base), &mut out);
        }
        out
    }

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n == "target") {
                    continue;
                }
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    /// ★ 抽取器自检：扫不到文件时下面两条会零命中地绿。
    #[test]
    fn the_source_scan_actually_finds_rust_files() {
        let n = rust_sources(&repo_root()).len();
        assert!(
            n >= 90,
            "只扫到 {n} 个 .rs（实测应约 100）—— 扫描器坏了，下面两条会空转变绿"
        );
    }

    /// ★ 正题：身份判定只许有一个家。
    #[test]
    fn the_identity_decision_has_exactly_one_home() {
        let root = repo_root();
        let pats = fingerprints();
        let mut offenders = Vec::new();
        for f in rust_sources(&root) {
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            if rel == SOLE_HOME {
                continue;
            }
            let src = guard_core::production_code(&fs::read_to_string(&f).unwrap_or_default());
            for p in &pats {
                if src.contains(p.as_str()) {
                    offenders.push(format!("  {rel}: `{p}`"));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "又出现了第二份 §34 Gate 2 身份判定。\n\
             唯一的家是 `{SOLE_HOME}`，请调 `gate_core::is_ccm_tmux_name` / `gate_core::gate2`。\n\
             ⚠ 这道门漂了**不会红**：两侧各自的测试都通过，只是对同一个会话名给出不同答案 ——\n\
             而它挡的是「往别人的 tmux 里打字 / 杀掉它」。\n{}",
            offenders.join("\n")
        );
    }

    /// ★ 反向锚点：唯一的那个家里**确实**有这个判定。
    ///
    /// 没有它，上面那条就退化成「哪里都没有」—— `quote_singleton_guard` 的同名一条
    /// 实测过它不是仪式：把实现换成行为等价但字面量不同的写法时，**只有这条会红**。
    #[test]
    fn the_sole_home_really_holds_the_decision() {
        let src = fs::read_to_string(repo_root().join(SOLE_HOME)).expect("gate-core 读不到");
        let prod = guard_core::production_code(&src);
        for p in fingerprints() {
            assert!(
                prod.contains(p.as_str()),
                "`{SOLE_HOME}` 的生产段里找不到 `{p}` —— \
                 那上面那条「只有一个家」就退化成「一个都没有」了"
            );
        }
    }

    /// ★ monitor 的本地包装真的在**转调**，不是留了一份副本。
    ///
    /// 这条与「只有一个家」不重复：副本可以写成不含指纹的等价实现（那时零命中守卫全绿）。
    /// 这里直接比行为 —— 两者对同一批输入必须逐个一致。
    #[test]
    fn the_monitor_wrapper_really_delegates() {
        let prod = guard_core::production_code(include_str!("tmux.rs"));
        assert!(
            prod.contains("gate_core::is_ccm_tmux_name"),
            "`tmux.rs` 的生产段里没有转调 `gate_core::is_ccm_tmux_name` —— \
             它要么自己又实现了一遍，要么这道门在 monitor 侧断了"
        );
    }
}
