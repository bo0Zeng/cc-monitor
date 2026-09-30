use std::fs;
use std::path::{Path, PathBuf};

/// POSIX 单引号逃逸序列的**源码**形态。两种写法都要认：
/// raw string `r"'\''"` 与普通串 `"'\\''"`。
const ESCAPE_RAW: &str = r#"r"'\''""#;
const ESCAPE_PLAIN: &str = r#""'\\''""#;

/// 〔audit-0805 08-06〕**同一件事的第二种标准写法**：`'"'"'`。
///
/// # 为什么它必须在人群里
///
/// POSIX shell 里「在单引号串中间嵌一个单引号」**恰好只有两种**做法：
/// 闭合后用反斜杠转义（`'\''`），或闭合后用双引号包一个单引号（`'"'"'`）。
/// 两种都正确、都常见。本守卫原来只认第一种 ——
/// 实测：往 `ssh_source.rs` 加一份
/// `format!("'{}'", s.replace('\'', "'\"'\"'"))`（**功能完整的第二份实现**），
/// 四条判据**全绿**；换成第一种写法则当场红。
///
/// ⇒ 「只许有一个实现」这条纪律，此前只对**一半的写法**成立。
/// 人群边界该由**域**给定（POSIX 就这两种），不是由写判据那天想起来的那一种给定。
///
/// ⚠ 走过一次弯路，记下来：我先想按「动作」派生人群（凡是替换单引号字符的都算）。
/// 量了之后否掉 —— 它**够不着唯一的家**（那里是逐 char `push_str`，根本没有 `replace`），
/// 却会**误伤当年 monitor 里那个 PowerShell `''` 转义**（另一门语言的正确写法；〔P5〕它已删，PowerShell 字面量只在后端写）。
/// **派生不是万能的：派生错了人群，比手写清单更糟，因为它看起来更有原则。**
const ESCAPE_ALT_RAW: &str = r#"'"'"'"#;
const ESCAPE_ALT_PLAIN: &str = r#"'\"'\"'"#;

/// 唯一允许持有这个实现的文件（相对仓根）。
const SOLE_HOME: &str = "src/common/shell-quote-core/src/lib.rs";

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

/// 扫 monitor + backend + 共享 crate 的**所有** `.rs`（`target/` 与 vendor 除外）。
fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for base in ["src/frontend/shell/src", "src/common", "src/backend"] {
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
        if src.contains(ESCAPE_RAW)
            || src.contains(ESCAPE_PLAIN)
            || src.contains(ESCAPE_ALT_RAW)
            || src.contains(ESCAPE_ALT_PLAIN)
        {
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

// 〔THIN〕「monitor 侧的入口逐字节对拍」那一条删了：最后一个入口（`ssh_source` 里那层转调壳）随它唯一的生产调用方
//   （monitor 侧 Gate 1 前检，THIN 第 3 件删）零调用、一起删 ⇒ monitor 侧零个 quote 入口，要 quote 直调内核。
//   更早摘掉的两个：`launch::posix_quote`〔散文墓碑〕（FIX4，远端 ssh 外壳随渲染进了本机后端）· `acct_iso_deploy::sq`（MIG-3a）。
//   「不逃逸的第二份实现」那一形照旧由上面的零命中守卫 ＋ 唯一的家那两条挡（当年只比一个入口时，把 `posix_quote`〔散文墓碑〕
//   换成不逃逸的写法照样全绿 —— 那条教训今天落在「monitor 侧零个入口」上）。

// ═══════ 〔P5〕PowerShell 那一门：monitor 里一个引号器都不许有 ═══════════════════

/// PowerShell 单引号引号器必有的记号：双写替换串 `"''"`（只转 ASCII 的那一形）· PowerShell 专有的四个引号字符
/// （认全的那一形必须点名它们；字面或 `\u{…}` 两种写法）。`(记号名, 源码里的写法)`。
const PS_QUOTER_MARKS: &[(&str, &[&str])] = &[
    ("\"''\"", &["\"''\""]),
    ("U+2018", &["\u{2018}", "\\u{2018}"]),
    ("U+2019", &["\u{2019}", "\\u{2019}"]),
    ("U+201A", &["\u{201a}", "\\u{201a}", "\\u{201A}"]),
    ("U+201B", &["\u{201b}", "\\u{201b}", "\\u{201B}"]),
];

/// 一份源码（原文）的生产段里各记号出现几处（只收非零）。
fn ps_quoter_census(src: &str) -> std::collections::BTreeMap<&'static str, usize> {
    let prod = guard_core::production_code(src);
    PS_QUOTER_MARKS
        .iter()
        .map(|(name, spellings)| {
            (
                *name,
                spellings.iter().map(|s| prod.matches(s).count()).sum(),
            )
        })
        .filter(|(_, k)| *k > 0)
        .collect()
}

/// ★ 住址：`4d-lanes` P5 ——「判据：monitor 生产段零 PowerShell 引号器（零命中）」
/// （主会话裁：V156 方言只住后端，PowerShell 字面量只走后端 `platform/shell/dialect.rs::ps_literal`）。
/// 人群：前端树 `src/frontend/**.rs` 的生产段（剥注释与测试段）。零命中；正控两条：同一把尺子量后端那唯一的出口恰好量出四个引号字符 ·
/// 往 `launch.rs` 副本塞回旧的只转 ASCII 那一形数得出。买不到：按码点现算出引号、不写任何字面量的等价实现这把尺子看不见。
#[test]
fn the_monitor_holds_no_powershell_quoter() {
    let root = repo_root();
    let files = guard_core::scan_tree_excluding(&root.join("src/frontend"), &["rs"], &[]);
    assert!(
        files
            .iter()
            .any(|(p, _)| p.ends_with("src/frontend/shell/src/launch.rs")),
        "前端树里没扫到 `launch.rs`（共 {} 份）—— 遍历坏了，本条会零命中地绿",
        files.len()
    );
    let hits: Vec<(String, std::collections::BTreeMap<&str, usize>)> = files
        .iter()
        .map(|(p, src)| {
            (
                p.strip_prefix(&root).unwrap_or(p).display().to_string(),
                ps_quoter_census(src),
            )
        })
        .filter(|(_, c)| !c.is_empty())
        .collect();
    assert_eq!(
        hits,
        vec![],
        "monitor 里又长出了 PowerShell 引号器 —— PowerShell 字面量只在后端写（`platform/shell/dialect.rs::ps_literal`，认全五个引号字符），\
         要渲 PowerShell 就问本机后端要成品"
    );
    let sole = include_str!("../../../src/backend/platform/shell/dialect.rs");
    assert_eq!(
        ps_quoter_census(sole),
        std::collections::BTreeMap::from([
            ("U+2018", 1),
            ("U+2019", 1),
            ("U+201A", 1),
            ("U+201B", 1)
        ]),
        "同一把尺子量不出后端那唯一的出口 —— 尺子坏了，上面那条零命中不可信"
    );
    let launch = include_str!("../../../src/frontend/shell/src/launch.rs");
    let planted = format!("{launch}\nfn q(s: &str) -> String {{\n    format!(\"'{{}}'\", s.replace('\\'', \"''\"))\n}}\n");
    assert_eq!(
        ps_quoter_census(&planted),
        std::collections::BTreeMap::from([("\"''\"", 1)])
    );
}
