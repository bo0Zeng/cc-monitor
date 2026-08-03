//! **会话名产出点清账 + 递减棘轮**（账本 S12 的落地形态）。
//!
//! # S12 说的是「计数守卫 == 2」，而今天是 **5 个产出点**
//!
//! S12 的最终形态是「两族各一个函数 + 计数守卫 == 2」，并注明**守卫挂 U11 不是 U8**
//! （「ccm 到 U9、cc-spawn 到 U11 才收编，挂 U8 是**做完必红**的 DoD」）。
//!
//! U11 摸底实测：ccm 那份要等 **U9b**，而 **U9b 今天是 ⛔ 阻塞**的 ⇒ **`== 2` 今天钉不上**。
//! 硬钉就是 S12 自己警告的那个形状（做完必红）。
//!
//! ⇒ 本模块装的是**递减棘轮**：把今天的真实数目登记全、每处写明归谁退役；
//! **多一处 ⇒ 红**（防回潮）；**少一处 ⇒ 也红**（提醒把棘轮往下拧、并核对 S12 的账）。
//! 同 `polling_registry`（S29）的形状 —— 目标钉不上时，先把**当下**钉住。
//!
//! # 普查抓到的两件此前没登记的事
//!
//! ① **`launch-requests.ts` 有第二份 `<sid8>-cc`**，与 `pickFreshTmuxName` 的 base 逐字相同
//!    **但不做撞名检查** —— 而 `pickFreshTmuxName`（F74）存在的全部理由就是撞名
//!    （「被 `/branch` 漂移后仍占着原名的会话」）。**登记为待查，不在本轮盲改。**
//! ② **`cc-spawn` 用的是 `_cc`（下划线）**，全仓其余都是 `-cc`（连字符）⇒
//!    `is_ccm_tmux_name` **认不出它**。是刻意隔开命名空间、还是漂了，**登记为待裁定**。
//!
//! # 它查什么、查不了什么
//!
//! 查的是「`-cc` / `_cc` 紧跟收尾引号」这个**构造形态**（生产段，注释与测试段剥掉）。
//! ⚠ 裸查 `-cc` 会撞上 CSS 类名（`settings-cc-profile-status`）与命令 id（`open-cc-bus`）——
//! 普查时实测过，收紧到「紧跟引号」才干净。
//! ⚠ **查不了「不带 `cc` 后缀的会话名生成」**（真要另起一套命名它就看不见）。
//! **比没有强，别读成证明。**

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// `(相对仓根的路径, 类别, 命中数, 说明 + 谁退役它)`。
    ///
    /// 类别：`producer-target`（S12 要留的两族）· `producer-duplicate`（要退役的副本，
    /// **必须写退役归属**）· `consumer`（只判名字形状、不产名）。
    const REGISTERED: &[(&str, &str, usize, &str)] = &[
        (
            "src/remote-launch.ts",
            "producer-target",
            2,
            "两族本体：`pickFreshTmuxName`（sid 派生 + 撞名后缀，F74）· `deriveTmuxName`（cwd 派生）。\
             **S12 的目标就是最后只剩这两个。**",
        ),
        (
            "src/launch-requests.ts",
            "producer-duplicate",
            1,
            "`<sid8>-cc` 的**第二份** —— 与 `pickFreshTmuxName` 的 base 逐字相同，\
             **但不做撞名检查**。⚠ 本轮普查**新发现**，且它可能是真缺陷（F74 存在的理由就是撞名）\
             —— **登记为待查，不盲改**。退役归 **U11 本体**（改调 `pickFreshTmuxName`）。",
        ),
        (
            "src/fork-launch.ts",
            "producer-duplicate",
            1,
            "分叉会话名 `<base>-fork-cc`（G6 加的第三种形态）。退役归 **U11 本体**。",
        ),
        (
            "shared/ccm",
            "producer-duplicate",
            1,
            "`derive_tmux_name` 的 shell 副本（与 `deriveTmuxName` 同义）。\
             退役归 **U9b**（thin ccm 变零决策执行臂）—— 而 U9b 今天 ⛔ 阻塞，\
             **这就是 `== 2` 今天钉不上的直接原因**。",
        ),
        (
            "shared/cc-bus/scripts/cc-spawn",
            "producer-duplicate",
            1,
            "`<basename>_cc` —— ⚠ **下划线不是连字符**，全仓其余都用 `-cc` ⇒ \
             `is_ccm_tmux_name` 认不出它。是刻意隔开命名空间还是漂了，**待裁定**。\
             退役归 **U11 本体**。",
        ),
        (
            "src-tauri/src/tmux.rs",
            "consumer",
            2,
            "`is_ccm_tmux_name` —— 只**判**名字形状（§34 Gate 2 的本地那半），**不产名**。\
             登记它是为了让上面那条「多一处就红」不会被消费点噪音淹掉。",
        ),
    ];

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .to_path_buf()
    }

    /// 生产段：`.rs` 用 `guard_core`（连测试段一起剥）；`.ts` / shell 只剥整行注释。
    fn production(path: &str, raw: &str) -> String {
        if path.ends_with(".rs") {
            return guard_core::production_code(raw);
        }
        let shell = !path.ends_with(".ts");
        raw.lines()
            .filter(|l| {
                let t = l.trim_start();
                if shell {
                    !t.starts_with('#')
                } else {
                    !(t.starts_with("//") || t.starts_with('*') || t.starts_with("/*"))
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 「`-cc` / `_cc` 紧跟收尾引号」—— 见模块头注：裸查 `-cc` 会撞 CSS 类名。
    fn hits(src: &str) -> usize {
        src.lines()
            .filter(|l| {
                ["-cc\"", "-cc'", "-cc`", "_cc\"", "_cc'", "_cc`"]
                    .iter()
                    .any(|p| l.contains(p))
            })
            .count()
    }

    /// 扫描面：`src/**/*.ts`（排除测试）+ `src-tauri/src/tmux.rs` + 两个 shell 脚本。
    fn scan() -> Vec<(String, usize)> {
        let root = repo_root();
        let mut files: Vec<PathBuf> = Vec::new();
        collect_ts(&root.join("src"), &mut files);
        files.sort();
        for extra in [
            "src-tauri/src/tmux.rs",
            "shared/ccm",
            "shared/cc-bus/scripts/cc-spawn",
        ] {
            files.push(root.join(extra));
        }
        let mut out = Vec::new();
        for f in files {
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            let n = hits(&production(
                &rel,
                &fs::read_to_string(&f).unwrap_or_default(),
            ));
            if n > 0 {
                out.push((rel, n));
            }
        }
        out
    }

    fn collect_ts(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_ts(&p, out);
                continue;
            }
            let n = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if n.ends_with(".ts")
                && !n.contains(".vitest.")
                && !n.contains(".test.")
                && !n.ends_with(".d.ts")
            {
                out.push(p);
            }
        }
    }

    /// ★ 抽取器自检。
    #[test]
    fn the_scan_actually_reads_all_four_surfaces() {
        let root = repo_root();
        let mut ts = Vec::new();
        collect_ts(&root.join("src"), &mut ts);
        assert!(
            ts.len() >= 60,
            "只扫到 {} 个前端 .ts —— 遍历器坏了",
            ts.len()
        );
        for f in [
            "src-tauri/src/tmux.rs",
            "shared/ccm",
            "shared/cc-bus/scripts/cc-spawn",
        ] {
            let n = fs::read_to_string(root.join(f))
                .map(|s| s.len())
                .unwrap_or(0);
            assert!(n > 2000, "{f} 读不到或太短（{n} 字节）—— 路径变了？");
        }
    }

    /// ★ 递减棘轮：目录内容 == 登记表，**连每个文件的命中数一起钉**。
    ///
    /// 多一处 ⇒ 红（防回潮）；少一处 ⇒ 也红（提醒把棘轮往下拧 + 核对 S12 的账）。
    #[test]
    fn the_session_name_producers_match_the_registry_count_for_count() {
        let found = scan();
        let mut want: Vec<(String, usize)> = REGISTERED
            .iter()
            .map(|(f, _, n, _)| (f.to_string(), *n))
            .collect();
        want.sort();
        let mut got = found.clone();
        got.sort();
        assert_eq!(
            got, want,
            "\n会话名产出点与登记表对不上。\n\
             **多一处** = 又开了一个产出点（S12 要收敛到 2 个，别往回走）；\n\
             **少一处** = 退役了一份 —— 把登记表那条删掉，并回 S12 把「计数守卫 == 2」的进度更新。\n\
             （今天是 5 个产出点 + 1 个消费点；`== 2` 钉不上的直接原因是 ccm 那份要等 U9b，而 U9b ⛔。）"
        );
    }

    /// ★ 每个 `producer-duplicate` 都必须写明**谁退役它** —— 那是它与 `producer-target` 的分界。
    #[test]
    fn every_duplicate_producer_names_its_retirement_owner() {
        let mut dups = 0usize;
        let mut targets = 0usize;
        for (f, kind, _, why) in REGISTERED {
            assert!(
                matches!(*kind, "producer-target" | "producer-duplicate" | "consumer"),
                "{f} 的类别 {kind:?} 不在三类里"
            );
            match *kind {
                "producer-duplicate" => {
                    dups += 1;
                    assert!(why.contains("退役归"), "{f} 记成副本却没说谁退役它");
                }
                "producer-target" => targets += 1,
                _ => {}
            }
        }
        assert_eq!(
            targets, 1,
            "S12 的两族应当同住一个文件（`remote-launch.ts`）"
        );
        assert_eq!(
            dups, 4,
            "副本数变了 —— 退役了就把棘轮往下拧，并更新 S12 的账"
        );
    }
}
