//! 两种方言的别名读法里用到别名那一族的调用形状与别名块正文的几条（方言住平台层，那一层够不到资产域 ⇒ 挂在别名这一族底下）。

use crate::platform::shell::dialect::*;

/// 通用层交给方言的那条调用形状（`ccm` · `--`）。
const C: Call = crate::assets::aliases::CALL;

/// 通用层交给方言的「我们自己那块别名块」正文。
fn own(sh: Shell) -> String {
    crate::assets::aliases::block::own_block(sh)
}

fn sv(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

/// 同一份清单（带空格、单引号、弯引号、中文、`--ccm-tmux=`、`--` 透传、调用时的词交给 ccm 的那一形），两个方言各钉逐字一份。
fn sample() -> Vec<(&'static str, Vec<String>, RestTo)> {
    let c = RestTo::Agent;
    vec![
        ("workcc", sv(&["--account", "work"]), c),
        (
            "convz",
            sv(&[
                "--ccm-tmux=w.1",
                "--account",
                "work",
                "--cwd",
                "/home/u/文档/c c",
            ]),
            c,
        ),
        ("mo", sv(&["--model", "it's", "--", "--verbose"]), c),
        ("curly", sv(&["--model", "a\u{2019}b"]), c),
        ("bare", vec![], c),
        ("cca", sv(&["--", "--attach"]), RestTo::Ccm),
    ]
}

/// 黄金串 · POSIX：与 AL1 那一版逐字一样（只搬了住址，一个字节都没改）。
#[test]
fn posix_golden() {
    let got: Vec<String> = sample()
        .iter()
        .map(|(n, a, r)| Posix.render_alias(C, n, a, *r))
        .collect();
    assert_eq!(
        got,
        vec![
            r#"workcc() { ccm --account work "$@"; }"#.to_string(),
            r#"convz() { ccm --ccm-tmux=w.1 --account work --cwd '/home/u/文档/c c' "$@"; }"#
                .to_string(),
            r#"mo() { ccm --model 'it'\''s' "$@" -- --verbose; }"#.to_string(),
            "curly() { ccm --model 'a\u{2019}b' \"$@\"; }".to_string(),
            r#"bare() { ccm "$@"; }"#.to_string(),
            r#"cca() { ccm -- --attach "$@"; }"#.to_string(),
        ]
    );
}

/// 黄金串 · PowerShell：函数体同别名块里 `function cc` 的形状（真机上 PowerShell 解析过、
/// 零解析错误；`cc` 今天就是清单里的一条），预置参数**每个都单引号**、四种引号字符都双写。
#[test]
fn powershell_golden() {
    let got: Vec<String> = sample()
        .iter()
        .map(|(n, a, r)| PowerShell.render_alias(C, n, a, *r))
        .collect();
    let body = |call: &str, name: &str| {
        format!(
            "function {name} {{\n    [CmdletBinding()] param(\n        [Parameter(ValueFromRemainingArguments = $true)] $RemainingArgs\n    )\n    if (Get-Command __ccm_bind -CommandType Function -ErrorAction SilentlyContinue) {{ __ccm_bind }}\n    & \"$HOME\\.cc-monitor\\bin\\ccm.exe\"{call} $RemainingArgs\n}}"
        )
    };
    assert!(
        got[2].ends_with(concat!(r#"    & "$HOME\.cc-monitor\bin\ccm.exe" '--model' 'it''s' $RemainingArgs '--' '--verbose'"#, "\n}")),
        "{}",
        got[2]
    );
    assert_eq!(
        got,
        vec![
            body(" '--account' 'work'", "workcc"),
            body(
                " '--ccm-tmux=w.1' '--account' 'work' '--cwd' '/home/u/文档/c c'",
                "convz"
            ),
            // `$RemainingArgs`（敲别名时跟的）交 claude，别名的 ccm 选项在单引号的 `'--'` 右边。
            got[2].clone(),
            body(" '--model' 'a\u{2019}\u{2019}b'", "curly"),
            body("", "bare"),
            // 交给 ccm：`$RemainingArgs` 接在 `'--'` 右边的最后。
            body(" '--' '--attach'", "cca"),
        ]
    );
}

/// PowerShell 的别名按**落点的显式路径**叫 ccm（`$HOME` 那一台上展开）：别名块不往会话 `PATH` 里加东西，
/// 新机器上 `PATH` 里没有 ccm ⇒ 裸写 `& ccm` 的别名敲下去就是「无法将 ccm 项识别」。
#[test]
fn the_powershell_alias_names_ccm_by_its_landing() {
    let a = sv(&["--account", "work"]);
    let now = PowerShell.render_alias(C, "workcc", &a, RestTo::Agent);
    let calls: Vec<&str> = now
        .lines()
        .filter(|l| l.trim_start().starts_with('&'))
        .collect();
    assert_eq!(
        calls,
        [r#"    & "$HOME\.cc-monitor\bin\ccm.exe" '--account' 'work' $RemainingArgs"#],
        "{now}"
    );
}

#[test]
fn powershell_knows_the_names_its_own_block_defines() {
    assert!(PowerShell
        .name_taken("CC", &own(Shell::PowerShell))
        .is_none_or(|n| !copy_core::copy_matches("rsShellDialect.ps.nameTakenIntegration", &n)));
    for n in ["__ccm_bind", "__CCM_BIND"] {
        let note = PowerShell
            .name_taken(n, &own(Shell::PowerShell))
            .unwrap_or_else(|| panic!("`{n}` 在终端集成模板里就有，却一声不吭"));
        assert!(
            copy_core::copy_matches("rsShellDialect.ps.nameTakenIntegration", &note),
            "{note}"
        );
    }
    assert!(PowerShell
        .name_taken("zzz_no_such_command_anywhere", &own(Shell::PowerShell))
        .is_none());
    // 不查 `PATH`（远端）时，模板里的名字照样认得出 —— 那一格不是本机才答得了的事实。
    assert!(PowerShell
        .name_taken("__ccm_bind", &own(Shell::PowerShell))
        .is_some());
}
