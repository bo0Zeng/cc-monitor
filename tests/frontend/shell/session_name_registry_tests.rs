use std::fs;
use std::path::{Path, PathBuf};

/// `(相对仓根的路径, 类别, 命中数, 说明 + 谁退役它)`。
///
/// 类别：`producer-target`（S12 要留的两族）· `producer-duplicate`（要退役的副本，
/// **必须写退役归属**）· `consumer`（只判名字形状、不产名）。
const REGISTERED: &[(&str, &str, usize, &str)] = &[
    // 〔FIX4 · `设计/90 §3` J7〕前端那两格出表：`src/remote-launch.ts`（`deriveTmuxName`，原 `producer-target`）整份删、
    //   `src/frontend/ui/fork-launch.ts`（`forkTmuxName` 的 `-fork-cc`，原 `producer-duplicate` 退役归 U11）随派生 ＋ 避让一起搬进后端。
    //   ⇒ 前端零产名点（界面要名字就问那台后端的 `tmux-name-mint`）；后端那一份从「副本」翻成**唯一本体**。
    (
        // 🔴 〔`K-R48` 第二拍 09-11〕住址从 `shared/ccm` 换到这里：〔用@09-11 `K33`〕那个 bash 脚本删了，`derive_tmux_name` 搬进了后端本体。
        //    ⚠ 处数按**行**数：`derive_tmux_name` 里 `"session-cc"`（空名回落）与 `format!("{s}-cc")` 两行，
        //    〔FIX4〕＋ `fork_tmux_base` 两行：`format!("{seg}-fork-cc")`（产名）与 `strip_suffix("-cc")`（剥源名那个尾巴，
        //    尺子按「`-cc` 紧跟收尾引号」数、认不出它不产名）= 4。四行都在同一个 `name_segment` 净化器之上。
        "src/backend/control/ccm/plan.rs",
        "producer-target",
        4,
        "本体：`derive_tmux_name`（cwd 派生 `<项目名>-cc`，空 ⇒ `session-cc`）＋ `fork_tmux_base`（分叉 `<源名>-fork-cc`）。\
             〔FIX4 · J7〕全仓唯一一份：`ccm` 起会话与帧命令 `tmux-name-mint`（界面问的那一口）都走它，\
             撞名避让走同文件的 `next_free_name`（`mint_tmux_name`，只收会话快照那张 `TakenNames`）。\
             ★★ `K-R96`（09-12）退役了 sid 派生那一族（`pickFreshTmuxName`，用户 `R55`「要是可读的名字 / 不要id」）；\
             sid 骑在 `@ccm_sid` 上。",
    ),
    (
        "src/shared/cc-bus/scripts/cc-spawn",
        "producer-duplicate",
        1,
        "`<basename>_cc` —— ⚠ **下划线不是连字符**，全仓其余都用 `-cc` ⇒ \
             `is_ccm_tmux_name` 认不出它。是刻意隔开命名空间还是漂了，**待裁定**。\
             退役归 **U11 本体**。",
    ),
    (
        "src/backend/control/gate_rules.rs",
        "consumer",
        2,
        "`is_ccm_tmux_name` —— 只**判**名字形状（§34 Gate 2 的本地那半），**不产名**。\
             登记它是为了让上面那条「多一处就红」不会被消费点噪音淹掉。\
             ⚠ **F03 从 `src/frontend/shell/src/backend/control/tmux.rs` 搬到这里**：判定收进共享 crate，\
             monitor 与后端共用同一份（定框 C1）。本条棘轮当场红了 —— \
             **它就该红**：被测对象搬家，判据要跟着走，而不是让它悄悄少扫一处。\
             〔THIN〕共享 crate 收回后端模块（monitor 那一侧的门删了）⇒ 住址跟着换、扫描面按 extra 点名它。",
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
/// 〔audit-0805 08-06〕这一步不是重构洁癖：上一版自检自己又走了一遍遍历器，
/// 于是把 `scan()` 里的根路径改坏之后**自检照样绿**（实测 4 passed），
/// 而「未扫文件里的产名点」那个洞当场重新打开。
/// ⇒ **自检必须量被测者实际用的那个对象**，不能量一个「同样构造」的副本。
fn scan_files() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files: Vec<PathBuf> = Vec::new();
    collect_ts(&root.join("src"), &mut files);
    files.sort();
    collect_rs(&root.join("src/frontend/shell/src"), &mut files);
    collect_rs(&root.join("src/common"), &mut files);
    // 🔴 〔`K-R48` 第二拍 09-11〕`shared/ccm` 删了 ⇒ 换成后端那份原生实现。
    //    `collect_rs` 只扫 `src/frontend/shell/`，够不着 `src/backend/` ⇒ 仍按 extra 点名。
    for extra in [
        "src/backend/control/ccm/plan.rs",
        // 〔THIN〕原 `src/common/gate-core`（上面 `collect_rs` 扫得到那一棵）收成后端模块 ⇒ 按 extra 点名。
        "src/backend/control/gate_rules.rs",
        "src/shared/cc-bus/scripts/cc-spawn",
    ] {
        files.push(root.join(extra));
    }
    files
}

fn scan() -> Vec<(String, usize)> {
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
            out.push((rel, n));
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
    assert!(
        // ★ audit-0805 F16：60 → **190**（今日实测）。落后 130 意味着
        // 前端 `.ts` 少掉三分之二都不会红 —— 这条地板此前几乎不构成约束。
        ts.len() >= 190,
        "只扫到 {} 个前端 .ts —— 遍历器坏了",
        ts.len()
    );
    // 〔audit-0805 08-06〕Rust 侧改成递归之后，**它自己也要有地板** ——
    // 否则新加的覆盖面可以静默消失：实测把 `collect_rs` 的根指到一个不存在的目录，
    // 本条**照样绿**（4 passed），而「未扫文件里的产名点」那个洞当场重新打开。
    // ⇒ 扩了扫描面就要同步扩它的自检，这两步是一件事的两半。
    let rs: Vec<_> = scan_files()
        .into_iter()
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    assert!(
        rs.len() >= 90,
        "只扫到 {} 个 monitor 侧 .rs（08-06 实测 100+）—— Rust 那半的遍历器坏了，\
             产名点会重新变成「只看两个文件」",
        rs.len()
    );
    for f in [
        "src/backend/control/gate_rules.rs",
        "src/backend/control/ccm/plan.rs",
        "src/shared/cc-bus/scripts/cc-spawn",
    ] {
        let n = fs::read_to_string(root.join(f))
            .map(|s| s.len())
            .unwrap_or(0);
        assert!(n > 2000, "{f} 读不到或太短（{n} 字节）—— 路径变了？");
    }
}

/// ★ 递减棘轮：目录内容 == 登记表，**连每个文件的命中数一起钉**。
///
/// ⚠ **F13（2026-08-04）往下拧了一格：5 → 4 个文件。**
/// 消灭的是 `src/frontend/ui/launch-requests.ts` 那个 `<sid8>-cc` 默认值 —— 它与 `pickFreshTmuxName`
/// 的基名逐字相同却**不做撞名避让**，正是用户问的「为什么会撞名」的根因之一。
/// **是本条棘轮的「少一处也红」提醒我来拧的**（不是我记得）——
/// 只挡回潮的棘轮不会自己往下走，那半句的价值就在这里。
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
             （〔FIX4 · J7〕今天是 **2** 个产出点 + 1 个消费点 —— 前端两格（`remote-launch.ts` · `fork-launch.ts`）随派生 ＋ 避让\n\
             搬进后端出表，后端 `plan.rs` 成唯一本体；另一格是 `cc-spawn` 的 `_cc`，退役归 U11。）"
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
        "产名的本体只许一个文件 —— 〔FIX4 · J7〕今天是后端 `control/ccm/plan.rs`（前端那份 `remote-launch.ts` 删了）；\
             〔`K-R96` 09-12〕sid 派生那一族随 `R55`「要是可读的名字 / 不要id」整条退役了。"
    );
    // F13（2026-08-04）：4 → 3。退役的是 `src/frontend/ui/launch-requests.ts` 那个 `<sid8>-cc` 默认值
    // （与 `pickFreshTmuxName` 基名逐字相同却不做撞名避让 —— 用户问的「为什么会撞名」的根因之一）。
    // 剩下 3 份：`fork-launch.ts`（→ F13 已改调铸名口，仍自产基名）·`shared/ccm`（→ F06）·
    // `cc-spawn`（→ F13a）。⚠ **〔F13a 摸底订正 08-04〕它等的不是「本机后端」** ——
    // 实测：`cc-spawn` 已经**走 `ccm`**（`CCM_BIN --detach`，还带 `--ccm-probe` 能力协商），
    // 它走的是**命令行**、不是 monitor 的进程内通道 ⇒ 本机后端跑不跑起来与它**无关**。
    // 它真正等的是 **F06b**：`ccm` 去调后端的 `--resolve` 拿 argv/名字。
    // ⚠ 而后端侧 `--resolve` **早就做好了**（`control/resolve_query.rs`：
    // stdin `ResumeSpec` → stdout `CommandPlan`）—— 缺的是 **`ccm` 那一侧的调用**。
    // 〔FIX4 · J7〕3 → 1：`fork-launch.ts`（`-fork-cc`）随派生搬进后端退役；`plan.rs` 从副本翻成本体（`producer-target`）。
    //   剩 `cc-spawn` 一份（`_cc`，退役归 U11 本体）。
    assert_eq!(
        dups, 1,
        "副本数变了 —— 退役了就把棘轮往下拧，并更新 S12 的账"
    );
}
