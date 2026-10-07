use std::fs;
use std::path::{Path, PathBuf};

/// `(相对仓根的路径, 类别, 说明 + 谁退役它)`，按文件两向相等（产名的本体只许一个文件，由下面那条钉）。
///
/// 类别：`producer-target`（S12 要留的两族）· `producer-duplicate`（要退役的副本，
/// **必须写退役归属**）· `consumer`（只判名字形状、不产名）。
const REGISTERED: &[(&str, &str, &str)] = &[
    // 前端那两格出表：`src/remote-launch.ts`（`deriveTmuxName`，原 `producer-target`）整份删、
    //   `src/frontend/ui/fork-launch.ts`（`forkTmuxName` 的 `-fork-cc`，原 `producer-duplicate` 退役归 U11）随派生 ＋ 避让一起搬进后端。
    //   ⇒ 前端零产名点（界面要名字就问那台后端的 `terminal-name-mint`）；后端那一份从「副本」翻成**唯一本体**。
    (
        // 🔴 住址从 `shared/ccm` 换到这里：〔用@09-11 `K33`〕那个 bash 脚本删了，`derive_tmux_name` 搬进了后端本体。
        "src/backend/control/ccm/plan.rs",
        "producer-target",
        "本体：`derive_tmux_name`（cwd 派生 `<项目名>-cc`，空 ⇒ `session-cc`）＋ `fork_tmux_base`（分叉 `<源名>-fork-cc`）。\
全仓唯一一份：`ccm` 起会话与帧命令 `terminal-name-mint`（界面问的那一口）都走它，\
             撞名避让走同文件的 `next_free_name`（`mint_tmux_name`，只收会话快照那张 `TakenNames`）。\
             ★★ `K-R96`（09-12）退役了 sid 派生那一族（`pickFreshTmuxName`，用户 `R55`「要是可读的名字 / 不要id」）；\
             sid 骑在 `@ccm_sid` 上。",
    ),
    (
        "src/shared/cc-bus/scripts/cc-spawn",
        "producer-duplicate",
        "`<basename>_cc` —— ⚠ **下划线不是连字符**，全仓其余都用 `-cc` ⇒ \
             `is_ccm_tmux_name` 认不出它。是刻意隔开命名空间还是漂了，**待裁定**。\
             退役归 **U11 本体**。",
    ),
    (
        "src/backend/control/gate_rules.rs",
        "consumer",
        "`is_ccm_tmux_name` —— 只**判**名字形状（§34 Gate 2 的本地那半），**不产名**。\
             登记它是为了让上面那条「多一处就红」不会被消费点噪音淹掉。\
             ⚠ **F03 从 `src/frontend/shell/src/tmux.rs` 搬到这里**：判定收进共享 crate，\
             monitor 与后端共用同一份（定框 C1）。本条棘轮当场红了 —— \
             **它就该红**：被测对象搬家，判据要跟着走，而不是让它悄悄少扫一处。\
共享 crate 收回后端模块（monitor 那一侧的门删了）⇒ 住址跟着换、扫描面按 extra 点名它。",
    ),
];

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
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

/// 扫描面：`src/**/*.ts`（排除测试）+ monitor 与 `src/common` 的 `.rs` + 后端两份（`ccm/plan.rs` · `control/gate_rules.rs`）+ `cc-spawn`。
/// 扫描面本体 —— **单独抽出来，好让自检量的是「真正被扫的那一份」**。
///
/// 这一步不是重构洁癖：上一版自检自己又走了一遍遍历器，
/// 于是把 `scan()` 里的根路径改坏之后**自检照样绿**（实测 4 passed），
/// 而「未扫文件里的产名点」那个洞当场重新打开。
/// ⇒ **自检必须量被测者实际用的那个对象**，不能量一个「同样构造」的副本。
fn scan_files() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files: Vec<PathBuf> = Vec::new();
    collect_ts(&root.join("src"), &mut files);
    files.sort();
    // 壳那棵换成它的全部人群根（壳 `src/` ＋ manifest 明写的兄弟包，窗口包也在内）；有几个住 `src/common/`，去重。
    for r in crate::guard_support::crate_population_roots() {
        collect_rs(&r, &mut files);
    }
    collect_rs(&root.join("src/common"), &mut files);
    files.sort();
    files.dedup();
    // 🔴 `shared/ccm` 删了 ⇒ 换成后端那份原生实现。
    //    `collect_rs` 只扫 `src/frontend/shell/`，够不着 `src/backend/` ⇒ 仍按 extra 点名。
    for extra in [
        "src/backend/control/ccm/plan.rs",
        // 原 `src/common/gate-core`（上面 `collect_rs` 扫得到那一棵）收成后端模块 ⇒ 按 extra 点名。
        "src/backend/control/gate_rules.rs",
        "src/shared/cc-bus/scripts/cc-spawn",
    ] {
        files.push(root.join(extra));
    }
    files
}

fn scan() -> Vec<String> {
    let root = repo_root();
    let mut out = Vec::new();
    for f in scan_files() {
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
            out.push(rel);
        }
    }
    out
}

/// Rust 侧也递归遍历 —— 与 TS 侧同一口径（`target/` 除外）。
fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect_rs(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
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
    // 两侧遍历器的正控：前端入口 `main.ts` 与 Rust 那半的入口 `lib.rs` 必在人群里
    //（把遍历根指到一个不存在的目录，本条要红）。
    assert!(
        ts.iter()
            .any(|p| *p == root.join("src/frontend/ui/main.ts")),
        "前端 .ts 的遍历器没走到 `main.ts`（走到 {} 份）—— 遍历器坏了",
        ts.len()
    );
    let files = scan_files();
    assert!(
        files
            .iter()
            .any(|p| *p == root.join("src/frontend/shell/src/lib.rs")),
        "Rust 那半的遍历器没走到 `src/frontend/shell/src/lib.rs` —— 产名点会重新变成「只看两个文件」"
    );
    for f in [
        "src/backend/control/gate_rules.rs",
        "src/backend/control/ccm/plan.rs",
        "src/shared/cc-bus/scripts/cc-spawn",
    ] {
        assert!(
            fs::read_to_string(root.join(f)).is_ok_and(|s| !s.trim().is_empty()),
            "{f} 读不到或是空的 —— 路径变了？"
        );
    }
}

/// ★ 目录内容 == 登记表（按文件两向）：多一处产名的文件 ⇒ 红；退役了 ⇒ 也红。
#[test]
fn the_session_name_producers_match_the_registry_count_for_count() {
    let found = scan();
    let mut want: Vec<String> = REGISTERED.iter().map(|(f, ..)| f.to_string()).collect();
    want.sort();
    let mut got = found.clone();
    got.sort();
    assert_eq!(
        got, want,
        "\n会话名产出点与登记表对不上。\n\
             **多一处** = 又开了一个产出点（S12 要收敛到 2 个，别往回走）；\n\
             **少一处** = 退役了一份 —— 把登记表那条删掉。"
    );
}

/// ★ 每个 `producer-duplicate` 都必须写明**谁退役它** —— 那是它与 `producer-target` 的分界。
#[test]
fn every_duplicate_producer_names_its_retirement_owner() {
    let mut targets = 0usize;
    for (f, kind, why) in REGISTERED {
        assert!(
            matches!(*kind, "producer-target" | "producer-duplicate" | "consumer"),
            "{f} 的类别 {kind:?} 不在三类里"
        );
        match *kind {
            "producer-duplicate" => {
                assert!(why.contains("退役归"), "{f} 记成副本却没说谁退役它");
            }
            "producer-target" => targets += 1,
            _ => {}
        }
    }
    assert_eq!(
        targets, 1,
        "产名的本体只许一个文件 —— 今天是后端 `control/ccm/plan.rs`（前端那份 `remote-launch.ts` 删了）；\
sid 派生那一族随 `R55`「要是可读的名字 / 不要id」整条退役了。"
    );
}
