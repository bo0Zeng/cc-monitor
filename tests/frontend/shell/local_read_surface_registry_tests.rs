use std::fs;
use std::path::{Path, PathBuf};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// 递归遍历 `src/`，不是硬编码文件名单。
fn rust_files() -> Vec<(String, String)> {
    let src = root().join("src");
    let mut out = Vec::new();
    // 人群 ＝ 本 crate 的 `src/` ＋ manifest 明写的兄弟包（`guard_core::population_trees`），兄弟包的键带包名。
    let trees =
        std::iter::once(("src".to_string(), src.clone())).chain(guard_core::population_trees(&src));
    for (label, tree) in trees {
        let mut stack = vec![tree.clone()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p: PathBuf = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_some_and(|x| x == "rs") {
                    let rel = format!(
                        "{label}/{}",
                        p.strip_prefix(&tree)
                            .unwrap_or(&p)
                            .to_string_lossy()
                            .replace('\\', "/")
                    );
                    out.push((rel, fs::read_to_string(&p).unwrap_or_default()));
                }
            }
        }
    }
    out.sort();
    out
}

/// ★★ **谁伸手进用户的 home —— 一张覆盖索引**：生产段里每一处 `home_dir()` 调用。
///
/// 人群按构造覆盖 claude 目录 / `.ssh` / profile / 数据目录 / 将来任何新目录。
/// 第四列写这一处归谁管；本表只回答「有没有人管」，具体守法在各自那张表里。
///
/// ⚠ 扫描面只有壳那棵树。后端的 `home_dir()` 说的是那台机器上的 home，由后端自己的 `readonly_guard` 管。
const HOME_REACHES: &[(&str, &str, &str, &str)] = &[
    (
        "data_paths.rs",
        "home",
        "壳取家目录的唯一住址（`clippy.toml` 取家目录那一组只在这里放行）",
        "经它的那几家，都**不是伸手拿用户的东西**：\
             ① `config::resolve_monitor_data_dir`（`~/.cc-monitor`，monitor 自己的数据目录）—— 写它的句柄只在 \
             `platform/own_fs.rs` 的 `OwnState::issue` 造，写侧登记在 `write_site_registry`；\
             ② `own_fs.rs` 的 `OwnBin::issue`（`~/.cc-monitor/bin`：自释放内嵌后端 · 文件窗口程序的落点，只有我们写、只有我们读，\
             `local_backend_host::start_local_backend` 经它拿；释放出来的文件按 build_id 命名 ⇒ 幂等、不覆盖别版）；\
             ③ `logging::log_file_info` 缩写日志路径的那一截（不读、不 stat 家里的文件）；\
             ④ filewin/proc 的 landing_dir · deploy 的 slots_dir · footprint_client 的 footprint_client_facts",
    ),
    (
        // 原生文件管理窗口「存到哪儿（本机）」那一问的缺省起点。
        "shell.rs",
        "local_home",
        "home 本身（当**起点路径**）",
        "它不是「伸手拿东西」，是给本机那个选择框一个开始的地方 —— \
             之后存到哪个目录**由用户在系统选择框里选**；窗口自己不列本机目录（只列远端，经后端 `files-ls`）。\
             ⚠ 与 profile 围栏（后端 `block.rs::fence`）**不是同一类**：\
             那一条拿 home 划界，这一条只是起点，**它不围任何东西** —— \
             「用户能在这个窗口里浏览到 home 之外」是设计如此，不是漏了围栏。\
             写侧归 `filewin::transfer`（上传经通道：monitor 的传输台只写远端暂存区，\
             落进用户目录那一下是后端 `files-commit-upload`，先过围栏）",
    ),
];

/// ★ 正题：**每一处 `home_dir()` 都要在表里，且表里不留死行**。
#[test]
fn every_reach_into_the_user_home_is_indexed() {
    let files = rust_files();
    let needle = format!("home_{}()", "dir");
    let mut found: Vec<(String, String)> = Vec::new();
    for (rel, src) in &files {
        let prod = guard_core::production_code(src);
        let lines: Vec<&str> = prod.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            if !l.contains(&needle) || l.trim_start().starts_with("fn ") {
                continue;
            }
            // 归属按「上一处 `fn 名字`」判（粗，但归错会红在名字对不上上）。
            let mut fname = "<找不到外层函数>".to_string();
            for prev in lines[..=i].iter().rev() {
                if let Some(rest) = prev
                    .split(" fn ")
                    .nth(1)
                    .or_else(|| prev.strip_prefix("fn "))
                {
                    fname = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    break;
                }
            }
            let stem = rel.rsplit('/').next().unwrap_or(rel).to_string();
            found.push((stem, fname));
        }
    }
    found.sort();
    found.dedup();
    let mut declared: Vec<(String, String)> = HOME_REACHES
        .iter()
        .map(|(f, n, _, _)| (f.to_string(), n.to_string()))
        .collect();
    declared.sort();
    assert_eq!(
        found, declared,
        "伸手进用户 home 的落点变了。\n\
             ★ 本表回答的是「**有没有人管**」，不是「怎么管」——多出来的那一处，\n\
             要在第四列写清它归哪张表（`.ssh` 读面 / profile 围栏 / 写点表 …）。\n\
             ⚠ 按 `home_dir()` 取人群而不是按目录名：按目录名取，下一个被伸手的目录仍然不在任何人群里。\n\
             ⚠ 少了的：那一处被删/改名了 ⇒ 删登记；若是**扫描面缩了**（`rust_files()` 少扫了），\n\
             先修扫描面，别改表。"
    );
}

/// ★ 抽取器自检。
#[test]
fn the_scan_actually_reads_the_monitor_tree() {
    let files = rust_files();
    // 遍历器正控：入口 `lib.rs` 必在人群里。
    for must in ["src/lib.rs"] {
        assert!(
            files.iter().any(|(n, _)| n == must),
            "遍历器没走到 `{must}`（走到 {} 份）—— 本条会零命中地绿",
            files.len()
        );
    }
    // 剥法自检：本模块的头注（住 `tests/`，不在被扫的生产树里）剥完要少一大半。
    let me: &str = include_str!("local_read_surface_registry.rs");
    assert!(
        guard_core::production_code(me).len() < me.len() / 2,
        "本文件剥完还剩一半以上 —— 剥法没生效，说明文字会被当成命中"
    );
}
