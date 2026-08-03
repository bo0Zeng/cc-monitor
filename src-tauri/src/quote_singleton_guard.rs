//! U8c-2b-0（账本 S5 收口）：**POSIX 单引号 quote 在 Rust 侧只许有一个实现**。
//!
//! # 病史
//!
//! 收口前全仓有**五份逐字节相同**的实现：`launch.rs::posix_quote` ·
//! `ssh_source.rs::shell_quote` · daemon `tmux_hook.rs::sq` ·
//! **`shell-quote-core::posix_quote`（U8c-1 自己新加的第四份；那时 crate 叫 `launch-core`）** ·
//! **`acct_iso_deploy.rs::sq`（第五份 —— 我摸底只数出四份、账本 S5 记的也是四份，
//! 是这条守卫第一次跑就当场抓出来的）**。
//!
//! 账本 S5 原本记着「`common/` 收不了 quote」，而 U8c-1 造出了共享 crate 这个载体、
//! 却在里面又加了一份 —— **拆分制造新副本、账本没跟**的典型形状。S5 当时就写下了
//! 「要么收进那个共享 crate 让三处调它，要么开一条对拍。今天两样都没有」。本模块是那条收口。
//!
//! # 为什么值得一条守卫而不只是「改完就算」
//!
//! 五份是**逐字节相同**的 —— 也就是说它们从来没有红过，是靠巧合保持一致的。
//! **连「到底有几份」这件事，人数出来的和机器数出来的都不一样**（我数四份，它数五份）。
//! 下一个要 quote 的人复制一份出来同样不会红。**这条纪律不做成机检就等于没有。**
//!
//! # 它查什么、查不了什么
//!
//! 查的是「`'\''` 这个 POSIX 逃逸序列出现在几个**生产**文件里」。
//! `shell-quote-core` 是唯一允许的那个；其余文件出现即红。
//!
//! ⚠ **查不了「换个写法的等价实现」**（比如手写 char 循环而不用 `replace`）——
//! 那属于「换个名字继续错」，与本仓其它约定型守卫同一档。**比没有强，别读成证明。**

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// POSIX 单引号逃逸序列的**源码**形态。两种写法都要认：
    /// raw string `r"'\''"` 与普通串 `"'\\''"`。
    const ESCAPE_RAW: &str = r#"r"'\''""#;
    const ESCAPE_PLAIN: &str = r#""'\\''""#;

    /// 唯一允许持有这个实现的文件（相对仓根）。
    const SOLE_HOME: &str = "src-tauri/crates/shell-quote-core/src/lib.rs";

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .to_path_buf()
    }

    /// 扫 monitor + daemon + 共享 crate 的**所有** `.rs`（`target/` 与 vendor 除外）。
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

    /// ★ 抽取器自检：扫不到文件时，下面那条会零命中零失败地绿。
    #[test]
    fn the_source_scan_actually_finds_rust_files() {
        let n = rust_sources(&repo_root()).len();
        // 地板从 40 棘到 90（实测 99）：40 意味着**丢掉六成扫描面也不会红**。
        // 留 9 个余量是给正常增删文件的，不是给坏掉的遍历器的。
        assert!(
            n >= 90,
            "只扫到 {n} 个 .rs（实测应约 99）—— 扫描器坏了，下面那条「只有一个实现」会空转变绿"
        );
    }

    /// ★ 本模块的正题：`'\''` 只许出现在 `shell-quote-core` 的**生产段**里。
    ///
    /// 测试段与注释不算 —— 用 `guard_core::production_code` 剥掉（它就是为这件事存在的：
    /// 便宜的 `split("\n#[cfg(test)]")` 近似在大文件上会把扫描面砍掉三分之二）。
    #[test]
    fn posix_single_quote_escaping_has_exactly_one_home() {
        let root = repo_root();
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
            if src.contains(ESCAPE_RAW) || src.contains(ESCAPE_PLAIN) {
                offenders.push(rel);
            }
        }
        assert!(
            offenders.is_empty(),
            "又出现了第二份 POSIX 单引号 quote 实现（收口前有**五份**、逐字节相同、从来没红过）。\n\
             唯一的家是 `{SOLE_HOME}`，请调 `shell_quote_core::posix_quote`。\n\
             命中：{offenders:?}"
        );
    }

    /// 反向自检：唯一的那个家里**确实**有这个实现 —— 否则上面那条是在断言「哪里都没有」。
    ///
    /// # 复盘审计说它是仪式性的，实测判定：**不是，留**
    ///
    /// 分两种情形量过（2026-08-03）：
    /// - **把实现挖空**（换成不逃逸的实现）⇒ 全仓红 **27 条**（那个 crate 自己 7 条 +
    ///   monitor 20 条）。这一情形本条确实是重复的。
    /// - **行为等价、但源码里不再出现那个字面量**（改成逐 char push）⇒ 那个 crate 当时 36 条
    ///   全绿、monitor 737 条全绿，**只有本条红**。
    ///
    /// 第二种才是本条真正的岗位：那时零命中守卫会**零命中地绿**，从此对「下一个人再复制一份
    /// 同样写法的实现」也不再有效 —— 也就是守卫悄悄失去了锚点。**所以它不是仪式，是那条
    /// 零命中守卫的唯一锚点。**（同 `the_source_scan_actually_finds_rust_files` 一族。）
    #[test]
    fn the_sole_home_really_holds_the_implementation() {
        let src = fs::read_to_string(repo_root().join(SOLE_HOME)).expect("shell-quote-core 读不到");
        let prod = guard_core::production_code(&src);
        assert!(
            prod.contains(ESCAPE_RAW) || prod.contains(ESCAPE_PLAIN),
            "`{SOLE_HOME}` 的生产段里找不到 POSIX 逃逸实现 —— \
             那上面那条「只有一个家」就退化成「一个都没有」。\n\
             ⚠ 这里**只认那两种字符串字面量形态**：初版多写了一个宽松的第三备选（裸逃逸子串），\
             而 `out.push` 那个**字符**字面量也含同样的字节 ⇒「把实现挖空」的变异照样绿。\
             自己的变异检查抓到的。"
        );
    }

    /// ★ monitor 侧三个入口对同一输入产出**逐字节相同** —— 收口的行为判据。
    ///
    /// ⚠ 初版只比了 `ssh_source::shell_quote` **一个** ⇒「把 `launch.rs::posix_quote` 换成
    /// 不逃逸的实现」这个变异照样全绿（自己的变异检查抓到的）。零命中守卫也挡不住它 ——
    /// 换成 `format!` 不带逃逸序列时根本不含那个子串。⇒ 三个**逐个**对拍。
    ///
    /// daemon 的 `tmux_hook::sq` 跨 crate 够不着，由它自己那侧的测试覆盖。
    #[test]
    fn every_monitor_entry_point_agrees_byte_for_byte() {
        for s in [
            "",
            "/p",
            "a'b",
            "it's",
            "'''",
            "/home/用户/带 空格",
            "a\nb",
            "$(id)",
        ] {
            let core = shell_quote_core::posix_quote(s);
            assert_eq!(
                crate::ssh_source::shell_quote(s),
                core,
                "ssh_source::shell_quote 与内核不一致：{s:?}"
            );
            assert_eq!(
                crate::launch::posix_quote(s),
                core,
                "launch::posix_quote 与内核不一致：{s:?}"
            );
            assert_eq!(
                crate::acct_iso_deploy::sq(s),
                core,
                "acct_iso_deploy::sq 与内核不一致：{s:?}"
            );
        }
    }
}
