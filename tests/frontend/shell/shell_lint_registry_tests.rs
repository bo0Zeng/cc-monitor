use std::path::{Path, PathBuf};

/// **刻意不进 shellcheck 的脚本**（路径, 为什么）。
///
/// 默认拒绝：不在这里、又不被门禁 shellcheck 人群覆盖的脚本，正题判据会点名。
const EXEMPT: &[(&str, &str)] = &[(
    "src/shared/ccm-aliases.sh",
    "供 source 的片段、无 shebang（SC2148 是构造性属性）；它会被写进用户 shell profile \
         并在 UI 面板里展示供手动复制 ⇒ 塞 `# shellcheck shell=bash` 等于往用户配置与界面文案里掺 lint 噪音",
)];

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

/// shellcheck 的人群：门禁 `tests/scripts/gate.sh` 里 `GATE_SHELLCHECK_GLOBS='…'` 那一行的各个 glob
/// （人群的唯一住址；CI 那个 job 调门禁的 `shellcheck` 格，不另记一份）。
fn shellcheck_patterns() -> Vec<String> {
    let gate = std::fs::read_to_string(repo_root().join("tests/scripts/gate.sh"))
        .expect("读不到 tests/scripts/gate.sh");
    let line = gate
        .lines()
        .filter_map(|l| l.strip_prefix("GATE_SHELLCHECK_GLOBS='"))
        .collect::<Vec<_>>();
    assert_eq!(
        line.len(),
        1,
        "`gate.sh` 里 `GATE_SHELLCHECK_GLOBS='…'` 不是恰好一行 —— 人群的写法变了，本模块要跟着改"
    );
    line[0]
        .trim_end_matches('\'')
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// bash 在**不开 globstar** 时的匹配语义：`*` 不跨 `/`。
///
/// ⚠ 这一点不是细节 —— `ci.yml` 那段注释逐字记着：写成 `.../scripts**` 时
/// `**` 等价于 `*`，会把目录喂给 shellcheck 而恒红。判据要和它**同一套语义**，
/// 否则我这边算出的「覆盖」和 CI 真扫的不是一回事。
fn matches(pattern: &str, path: &str) -> bool {
    let (ps, xs): (Vec<&str>, Vec<&str>) =
        (pattern.split('/').collect(), path.split('/').collect());
    if ps.len() != xs.len() {
        return false;
    }
    ps.iter()
        .zip(xs.iter())
        .all(|(p, x)| match p.split_once('*') {
            None => *p == *x,
            Some((pre, suf)) => {
                x.len() >= pre.len() + suf.len() && x.starts_with(pre) && x.ends_with(suf)
            }
        })
}

fn covered(patterns: &[String], path: &str) -> bool {
    patterns.iter().any(|p| matches(p, path))
}

/// **PowerShell 那一半：今天全仓零 lint，而没人盯着它别长大**。
///
/// `ci.yml` 逐字写着「两个 `.ps1` 仍**零 lint**（全仓无 PowerShell linter）」，
/// 也登记着同一件事（08-06 复核成立：`pwsh`/`powershell` 都不在 PATH，
/// 引一个 linter 属扩范围）。08-08 再复核：**仍是这两个、仍无 linter** —— 边界没漂。
///
/// ⇒ 本条不要求给它们上 lint（那是扩范围，且理由没变），只钉**这一族别悄悄长大**：
/// 多出第三个 `.ps1` 时，① 那个新脚本一行 lint 也没有；② `§5 1c` 那句「两个」当天过期。
/// 两件事都不会有人发现 —— 除非这里红一次。
const POWERSHELL_TODAY: &[(&str, &str)] = &[
    ("tests/scripts/run.ps1", "Windows 上的本地跑法入口"),
    (
        "tests/e2e/tier2/run-in-session1.ps1",
        "tier2 e2e：跳到已登录 session1 里跑（SSH 落 session0 没有桌面）",
    ),
    // RT1 的 Win11 虚拟机真机台架两份：只在台架里跑、不进产品，仍零 lint（`pwsh` 不在 PATH）。
    (
        "tests/evidence/RT1-lib.ps1",
        "RT1 真机台架：窗口枚举 / DPI / WM_CLOSE / 控制台事件",
    ),
    ("tests/evidence/RT1-winwatch.ps1", "RT1 真机台架：窗口哨兵"),
    // 手机端的两份：只在 Windows 开发机上装 / 激活 Android 工具链，不进产品、不进门禁，仍零 lint。
    (
        "src/mobile/scripts/bootstrap-android-env.ps1",
        "手机端：在 Windows 开发机上装一套隔离的 Android 工具链（JDK ＋ SDK）",
    ),
    (
        "src/mobile/scripts/env.ps1",
        "手机端：在当前 PowerShell 里激活那套工具链（env.sh 的 Windows 版）",
    ),
];

#[test]
fn the_powershell_family_has_not_grown() {
    let found = guard_core::files_by_extension(&repo_root(), "ps1");
    // ⚠ 两边都排序：登记表按「先重要后次要」写给人看，而 walker 按路径序返回。
    // 08-08 第一版直接比，红在**顺序**上 —— 那种红会让人以为集合变了。
    let mut known: Vec<String> = POWERSHELL_TODAY
        .iter()
        .map(|(p, _)| p.to_string())
        .collect();
    known.sort();
    assert_eq!(
        found, known,
        "全仓 `.ps1` 的集合变了。\n\
             ★ 多出来的那些**一行 lint 都没有**：全仓没有 PowerShell linter（`pwsh` 不在 PATH），\n\
             而 `ci.yml` 与都把「就这两个」当成已登记的诚实边界写着。\n\
             ⇒ 两条路：① 这一族真长大了 ⇒ 该重新问一次「要不要引 PSScriptAnalyzer」，\n\
             并把 `§5 1c` 那句「两个」改掉；② 只是挪了位置 ⇒ 更新这张表。\n\
             ⚠ 别把它当成登记表填一填就完 —— 本条存在的理由正是「零 lint 这件事不许悄悄变大」。"
    );
}

/// ★ 正题：**每个 shell 脚本要么被 shellcheck 扫到，要么登记豁免**。
#[test]
fn every_shell_script_is_either_linted_or_registered_as_exempt() {
    let scripts = guard_core::shell_scripts(&repo_root());
    let patterns = shellcheck_patterns();
    // 抽取器自检：任何一头空了，下面那条对拍都会零命中地绿（正控：门禁脚本自己在人群里）。
    assert!(
        scripts.iter().any(|s| s.replace('\\', "/") == "tests/scripts/gate.sh"),
        "shell 脚本的遍历里没有 `tests/scripts/gate.sh`（扫到 {} 份）—— 遍历口径坏了，本条会零命中地绿",
        scripts.len()
    );
    // 正控：门禁自己必须被人群盖住（抽取器坏了时这一条先红，而不是把所有脚本判成「没被扫」）。
    assert!(
        covered(&patterns, "tests/scripts/gate.sh"),
        "抽到的 pattern {patterns:?} 盖不住 `tests/scripts/gate.sh` —— 抽取器坏了"
    );

    let unlinted: Vec<&String> = scripts
        .iter()
        .filter(|s| !covered(&patterns, s))
        .filter(|s| !EXEMPT.iter().any(|(p, _)| *p == s.as_str()))
        .collect();
    assert!(
        unlinted.is_empty(),
        "这些 shell 脚本既不在门禁的 shellcheck 人群里、也没登记豁免：\n{}\n\n\
             ★ 它们**一次都没被 lint 过**。\n\
             两条路：① 补进 `tests/scripts/gate.sh` 的 `GATE_SHELLCHECK_GLOBS`，\n\
             或 ② 在本文件的 `EXEMPT` 里登记，**并写清为什么**。",
        unlinted
            .iter()
            .map(|s| format!("  {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// **豁免行不许变成死行，也不许是废话**（反向锚点）。
#[test]
fn every_exemption_still_points_at_a_real_unlinted_script() {
    let scripts = guard_core::shell_scripts(&repo_root());
    let patterns = shellcheck_patterns();
    for (path, why) in EXEMPT {
        assert!(
            scripts.contains(&path.to_string()),
            "`EXEMPT` 里登记着 `{path}`，而全仓扫不到这个 shell 脚本 —— \
                 它被删了或改名了 ⇒ 删掉这一行，别让豁免表长成一张没人看的旧账"
        );
        assert!(
            !covered(patterns.as_slice(), path),
            "`{path}` 登记着豁免，可门禁的人群**已经在扫它了** ⇒ 删掉这条豁免。\n\
                 留着的害处是具体的：下一个人会以为这个文件没被 lint，\
                 从而不敢改它 / 或者以为「反正没人扫」而放松它"
        );
        assert!(
            why.len() >= 20,
            "`{path}` 的豁免理由只有 {} 个字节 —— 太短的理由等于没有理由。\
                 写清楚「为什么这个文件不该被 lint」，不是「暂时不弄」",
            why.len()
        );
    }
}
