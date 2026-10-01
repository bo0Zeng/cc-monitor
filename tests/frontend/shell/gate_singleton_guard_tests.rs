use std::fs;
use std::path::{Path, PathBuf};

/// 唯一允许持有这个判定的文件（相对仓根）。
// 共享 crate `gate-core` 收成后端模块（monitor 那一侧的门删了）。
const SOLE_HOME: &str = "src/backend/control/gate_rules.rs";

/// 判定形状的源码指纹。**运行时拼**，免得本文件自己被扫到时命中。
/// 判定的**源码指纹**。
///
/// # 补上现行命名那两个形态
///
/// `is_ccm_tmux_name` 认三种形状：`cc-<X>`（**旧**前缀）· `<X>-cc`（**现行**）·
/// `<X>-cc-<N>`（撞名避让）。而指纹原来只有前两者的字面量
/// （`starts_with("cc-")` 与 `rsplit_once("-cc-")`）——
/// **恰好漏掉了今天真正在用的那个主形态**（S4b-3b 把命名从 `cc-<X>` 反转成 `<X>-cc`）。
///
/// 后果实测：往 `ssh_source.rs` 加一份
/// `fn is_ours(n: &str) -> bool { n.ends_with("-cc") && n.len() > 3 }`
/// —— 一份**只实现现行形态**的第二判定 —— 四条判据**全绿**。
/// 这正是本模块头注自己写着「身份门漂了是安全洞」要防的东西。
///
/// ⚠ 补之前先量了误红面：`ends_with("-cc")` / `split("-cc")` 今天**只出现在 `SOLE_HOME`**，
/// 补进来零误红。（上一件的教训：派生/扩面前先量，别因为理由充分就跳过。）
fn fingerprints() -> Vec<String> {
    let cc = "cc";
    vec![
        format!("starts_with(\"{cc}-\")"),
        format!("rsplit_once(\"-{cc}-\")"),
        format!("ends_with(\"-{cc}\")"),
        format!("split(\"-{cc}\")"),
    ]
}

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    // monitor 那棵根换成它的全部人群根（`guard_support::crate_population_roots`：壳 `src/` ＋ manifest 明写的兄弟包，
    //   窗口包也在内）；兄弟包有几个住 `src/common/`，下面去重。
    for base in crate::guard_support::crate_population_roots()
        .into_iter()
        .chain(["src/common", "src/backend"].iter().map(|b| root.join(b)))
    {
        walk(&base, &mut out);
    }
    out.sort();
    out.dedup();
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
             唯一的家是 `{SOLE_HOME}`，请调 `gate_rules::gate2`。\n\
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
    let src = fs::read_to_string(repo_root().join(SOLE_HOME)).expect("gate_rules.rs 读不到");
    let prod = guard_core::production_code(&src);
    for p in fingerprints() {
        assert!(
            prod.contains(p.as_str()),
            "`{SOLE_HOME}` 的生产段里找不到 `{p}` —— \
                 那上面那条「只有一个家」就退化成「一个都没有」了"
        );
    }
}

/// ★ monitor 这一侧**一道门都没有**：生产段零处够 Gate 判定（`gate_rules` · 旧名 `gate_core`），清单里也没有 `gate-core`。
///
/// 替掉的是 `the_monitor_wrapper_really_delegates`〔散文墓碑〕（「monitor 的转调壳真在转调」）：那个壳只剩跨轨对拍锚点在用，
/// 随 monitor 侧的 Gate 残留删了 —— 「monitor 不许自己再实现一份」从「锚在壳上」换成「monitor 里零处」（上面那条管指纹，本条管调用）。
/// 正控：同一份语料在后端 `control/gate.rs` 上认得出 `gate_rules::gate2`。
#[test]
fn the_monitor_holds_no_gate_of_its_own() {
    let root = repo_root();
    let mut corpus = String::new();
    let mut files = 0usize;
    for (_, text) in
        guard_core::scan_tree_excluding(&root.join("src/frontend/shell/src"), &["rs"], &[])
    {
        files += 1;
        corpus.push_str(&guard_core::strip_comment_lines(
            &guard_core::production_code(&text),
        ));
        corpus.push('\n');
    }
    assert!(files > 100, "只扫到 {files} 份 monitor 源码 —— 遍历坏了");
    for word in ["gate_rules", "gate_core"] {
        assert!(
            !guard_core::contains_word(&corpus, word),
            "monitor 生产段里又够到了 `{word}` —— §34 的门只住后端（`control/gate.rs` ＋ `control/gate_rules.rs`）"
        );
    }
    let manifest =
        fs::read_to_string(root.join("src/frontend/shell/Cargo.toml")).expect("monitor 清单读不到");
    assert!(
        !manifest
            .lines()
            .any(|l| l.trim_start().starts_with("gate-core")),
        "monitor 清单里又挂上了 `gate-core`"
    );
    let gate_rs = guard_core::production_code(
        &fs::read_to_string(root.join("src/backend/control/gate.rs")).expect("gate.rs 读不到"),
    );
    assert!(
        guard_core::contains_word(&gate_rs, "gate_rules"),
        "正控失败：后端 `control/gate.rs` 的生产段里认不出 `gate_rules` —— 上面的零命中不可信"
    );
}
