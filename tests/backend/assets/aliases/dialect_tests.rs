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

// ═══════════════════════════════════════════════════════════════════════════
// 托管入口（`cch` · `<号>cch`）的补全：别名文件里一起出，两种方言各一份
// ═══════════════════════════════════════════════════════════════════════════

/// 合并后的清单里带 `--hosted` 的那几条（内置的 `cch` ＋ 每个号的 `<号>cch` ＋ 用户自己基于 `cch` 写的一段），别的不算。
#[test]
fn hosted_completion_covers_every_hosted_alias_and_only_those() {
    let book = crate::assets::aliases::accounts::with_builtins(
        crate::assets::aliases::profile::parse_book(
            "[mine]\nfrom = \"cch\"\n\n[plain]\nmodel = \"x\"\n",
        ),
        &["work".to_string()],
        true,
        &|a, b| a == b,
    );
    let got = super::hosted_names(&book);
    assert_eq!(got, ["cch", "workcch", "mine"], "{:?}", book.problems);
}

/// 一个沙箱 home：`bin/ccm` 是个替身，`-- --hosts` 时照 `cch ls` 那份的排版印两个托管终端（与「无托管终端」那一形）。
#[cfg(unix)]
fn fake_ccm_home(tag: &str, rows: &str) -> std::path::PathBuf {
    let home = test_home::scratch(tag);
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let ccm = bin.join(crate::ccm_line::SUBCOMMAND_WORD);
    std::fs::write(
        &ccm,
        format!(
            "#!/bin/sh\nif [ \"$1 $2\" = \"-- --hosts\" ]; then printf '%s' '{rows}'; else echo \"unexpected: $*\" >&2; exit 9; fi\n"
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&ccm, std::fs::Permissions::from_mode(0o755)).unwrap();
    home
}

/// 在沙箱 bash 里 source 生成的别名文件，照 bash 叫 `complete -C` 的样子（命令名 · 当前词 · 前一个词）喂一发，收它补出来的词。
#[cfg(unix)]
fn bash_complete(
    home: &std::path::Path,
    file: &str,
    cmd: &str,
    cur: &str,
    prev: &str,
) -> Vec<String> {
    let f = home.join("aliases.sh");
    std::fs::write(&f, file).unwrap();
    let script = format!(
        "set -u; . '{}'; spec=$(complete -p {cmd}) || exit 7; fn=${{spec#*-C }}; fn=${{fn%% *}}; fn=${{fn#\\'}}; fn=${{fn%\\'}}; \"$fn\" {cmd} '{cur}' '{prev}'",
        f.display()
    );
    let out = std::process::Command::new("bash")
        .args(["--norc", "--noprofile", "-c", &script])
        .env_clear()
        .env("HOME", home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .output()
        .expect("起 bash");
    assert!(
        out.status.success(),
        "bash 退出 {:?}：{}\n生成的文件：\n{file}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// 🔴 第一条会红：生成的 POSIX 别名文件里有 `cch` · `<号>cch` 的补全规约 —— 首词补 `ls` / `attach`，
/// `attach ` / `a ` 后补这台的托管终端名（名单取 `ccm -- --hosts`，即 `cch ls` 那份；「无托管终端」那一句不当名字）。
#[cfg(unix)]
#[test]
fn bash_completes_hosted_entry_words_and_terminal_names() {
    // 名单照 `cch ls` 真用的那条文案排（`host::managed::home::listing` 一行一个 `beHost.hosts.row`；这一族够不到 host，借同一条文案）：
    // 文案的排版改了，这里跟着变、补全那一段切不出名字就红。
    fn row(name: &str, cwd: &str, account: &str) -> String {
        let r: String = copy_core::copy_text(
            "beHost.hosts.row",
            &[
                ("name", name),
                ("cwd", cwd),
                ("account", account),
                ("at", "10:00"),
                ("attached", "-"),
            ],
        );
        format!("{r}\n")
    }
    let rows = format!(
        "{}{}",
        row("alpha", "/home/u/p", "缺省号"),
        row("beta-2", "/home/u/q", "work")
    );
    let home = fake_ccm_home("p151-bash", &rows);
    let names = vec!["cch".to_string(), "workcch".to_string()];
    let file = super::render_file(Shell::Posix, &[], &names);
    for cmd in ["cch", "workcch"] {
        assert_eq!(
            bash_complete(&home, &file, cmd, "", cmd),
            ["ls", "attach"],
            "{file}"
        );
        assert_eq!(bash_complete(&home, &file, cmd, "a", cmd), ["attach"]);
        for verb in ["attach", "a"] {
            assert_eq!(
                bash_complete(&home, &file, cmd, "", verb),
                ["alpha", "beta-2"],
                "`{cmd} {verb} ` 后补托管终端名\n{file}"
            );
        }
        assert_eq!(bash_complete(&home, &file, cmd, "b", "a"), ["beta-2"]);
        assert!(bash_complete(&home, &file, cmd, "", "ls").is_empty());
    }
    let none = fake_ccm_home(
        "p151-bash-none",
        &format!("{}\n", copy_core::copy_text("beHost.hosts.none", &[])),
    );
    assert!(bash_complete(&none, &file, "cch", "", "a").is_empty());
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&none);
}

/// 没有托管那一形的别名 ⇒ 文件里不出补全那一段（两种方言都是）。
#[test]
fn no_hosted_alias_no_completion() {
    for sh in [Shell::Posix, Shell::PowerShell] {
        let with = super::render_file(sh, &[], &["cch".to_string()]);
        let without = super::render_file(sh, &[], &[]);
        assert!(with.contains("cch"), "{sh:?}：有 cch 却没出补全\n{with}");
        assert!(!without.contains("cch"), "{without}");
    }
}

/// PowerShell 那一份：`Register-ArgumentCompleter` 挂在每条托管别名的 `RemainingArgs` 上，首词 `ls` / `attach`，
/// `attach` / `a` 后按落点显式叫 ccm 的 `'--' '--hosts'`（裸 `--` 会被 PowerShell 吃掉），每行第一段是名字。
/// ⚠ 这台没有 PowerShell：形状照 Win11 虚拟机上 5.1 `TabExpansion2` 实测过的那一份（`cch ` ⇒ ls,attach · `cch a ` ⇒ 名字）。
#[test]
fn powershell_registers_hosted_completion() {
    let names = vec!["cch".to_string(), "workcch".to_string()];
    let file = super::render_file(Shell::PowerShell, &[], &names);
    assert!(
        file.contains("Register-ArgumentCompleter -CommandName 'cch', 'workcch' -ParameterName RemainingArgs -ScriptBlock {"),
        "{file}"
    );
    assert!(file.contains("'ls', 'attach'"), "{file}");
    assert!(file.contains("-in 'attach', 'a'"), "{file}");
    assert!(
        file.contains(r#"& "$HOME\.cc-monitor\bin\ccm.exe" '--' '--hosts'"#),
        "{file}"
    );
    assert!(file.contains("-split '  '"), "{file}");
}
