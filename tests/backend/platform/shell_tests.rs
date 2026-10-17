//! # （平台原语只住 `platform/`）＋（本平台没有的那一格如实说没有）
//!
//! 核原文：「`platform/` —— 唯一允许出现平台原语与平台 `cfg` 的地方」—— `shell/mod.rs::posix_shell` 是「把命令串交给
//! POSIX shell」这一格在那里的住址，本族判它在 unix 上原样带出脚本；非 unix 回 `None`、不编一个 `cmd /C` 冒充，对 `D7`「失败要显式」。
//! ⚠ 主住址偏弱：原文讲的是「只许住在这里」，本族判的是原语的行为。〔JA1 点址 2026-09-24〕

/// ★ unix 上：真的备出了一条 `sh -c <脚本>`，而且脚本是**传进去的那一份**。
///
/// ⚠ 断的是 `get_args()` 里那两段（`-c` 与脚本本身），不是「程序名叫 sh」——
/// 后者在这个仓里是写死的字面量，断它等于断一句源码。
#[test]
#[cfg(unix)]
fn the_posix_shell_carries_the_script_it_was_given() {
    let c = super::posix_shell("echo 甲乙丙").expect("unix 上必须备得出来");
    let args: Vec<String> = c
        .built()
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args,
        vec!["-c".to_string(), "echo 甲乙丙".to_string()],
        "备出来的 argv 不是 `-c <传进去的那份脚本>` —— 脚本被改写或被丢了"
    );
    // 反空真：换一份脚本，argv 跟着变（否则上面那条可能断在一个常量上）。
    let other = super::posix_shell("true")
        .expect("unix 上必须备得出来")
        .built()
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_ne!(args, other, "换了脚本 argv 却没变 —— 脚本那一维是个常量");
}

/// ★ 非 unix 上：**说得出「本平台没有这一格」**，不是编一个 `cmd /C` 出来。
#[test]
#[cfg(not(unix))]
fn there_is_no_posix_shell_off_unix() {
    assert!(
        super::posix_shell("echo x").is_none(),
        "非 unix 上竟然备出了一条 shell 命令 —— 那只能是编出来的"
    );
}

use super::PsHost;

/// ★ 要求：「L 的做法：那台后端只读现问生效策略，块不会加载就明说」。
/// 执行策略的词 → 本地未签名的 profile（我们装的块）跑不跑：PowerShell 定义的七个词逐个（手写规格，照 `about_Execution_Policies`），
/// 不分大小写；`Undefined` / `Default` 的效果随 Windows 版本变 ⇒ 说不清；别的词也说不清。
#[test]
fn which_execution_policies_load_a_local_unsigned_profile() {
    use super::powershell::policy_loads_local_script as loads;
    let spec = [
        ("Restricted", Some(false)),
        ("AllSigned", Some(false)),
        ("RemoteSigned", Some(true)),
        ("Unrestricted", Some(true)),
        ("Bypass", Some(true)),
        ("Undefined", None),
        ("Default", None),
    ];
    for (w, want) in spec {
        assert_eq!(loads(w), want, "{w}");
        assert_eq!(loads(&w.to_uppercase()), want, "{w} 大写");
    }
    assert_eq!(loads("Signed"), None);
}

/// ★ 住址同上。三行（生效 · `MachinePolicy` · `UserPolicy`）读成现状：组策略任一档有值 ⇒ 钉着；行数不对 ⇒ 「内容无法解析」，原话不随回（记日志）。
#[test]
fn the_policy_listing_reads_the_effective_policy_and_group_policy() {
    use super::powershell::read_policy_listing as read;
    let a = read(PsHost::Desktop, "Restricted\r\nUndefined\r\nUndefined\r\n");
    assert_eq!(
        (a.effective.as_deref(), a.loads, a.group_policy, a.error),
        (Some("Restricted"), Some(false), false, None)
    );
    let b = read(PsHost::Core, "AllSigned\nUndefined\nAllSigned\n");
    assert_eq!(
        (b.host, b.loads, b.group_policy),
        (PsHost::Core, Some(false), true)
    );
    let c = read(PsHost::Desktop, "RemoteSigned\n");
    assert_eq!((c.effective, c.loads), (None, None));
    assert_eq!(
        c.error.as_deref(),
        Some(copy_core::copy_text("reason.content.unparsable", &[]).as_str()),
        "认不出却没说「内容无法解析」，或把原话带进了这一格"
    );
}

/// ★ 住址同上。起那一代 PowerShell：按代选程序、`-NoProfile -NonInteractive -Command <脚本>`，
/// 剥掉继承来的 `PSExecutionPolicyPreference`（父进程给的进程级策略，新开的窗口没有它 —— 不剥就会把它当成那台的策略答）。
#[test]
fn a_powershell_starts_without_profile_and_without_the_inherited_policy() {
    for (h, exe) in [
        (PsHost::Desktop, "powershell.exe"),
        (PsHost::Core, "pwsh.exe"),
    ] {
        let c = super::powershell_command(h, "Get-ExecutionPolicy").built();
        assert_eq!(c.get_program(), exe);
        let args: Vec<_> = c
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Get-ExecutionPolicy"
            ]
        );
        assert!(
            c.get_envs()
                .any(|(k, v)| k == "PSExecutionPolicyPreference" && v.is_none()),
            "没剥继承来的进程级策略"
        );
    }
}

/// ★ 起会话那个 shell 的 `PATH`：从输出里认标记那一段（rc 文件往标准输出打的杂话不算进去）；没有标记 / 空 ⇒ 问不出来。
#[test]
fn the_session_shell_path_is_read_between_its_markers() {
    use super::{parse_marked_path, SESSION_PATH_MARK as M};
    assert_eq!(
        parse_marked_path(&format!("欢迎\n{M}/a/bin:/b/bin{M}\n")),
        Some("/a/bin:/b/bin".to_string())
    );
    assert_eq!(
        parse_marked_path("欢迎\n/a/bin:/b/bin\n"),
        None,
        "没有标记却认出了一段"
    );
    assert_eq!(
        parse_marked_path(&format!("{M}{M}")),
        None,
        "空的 PATH 不是答案"
    );
}

/// 沙箱家目录（`sh` 代替用户的 shell；登录时读的那份文件由各条自己写）。
#[cfg(unix)]
fn login_box(tag: &str, profile: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let home = std::env::temp_dir().join(format!("ccm-session-path-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join(".profile"), profile).unwrap();
    let sh = home.join("sh");
    std::fs::write(&sh, "#!/bin/sh\nexec /bin/sh +m \"$@\"\n").unwrap();
    std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755)).unwrap();
    home
}

#[cfg(unix)]
fn sandboxed(home: &std::path::Path) -> crate::platform::child::Child {
    super::login_shell_asking_path(home.join("sh").to_str().unwrap())
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env_remove("ENV")
        .env_remove("BASH_ENV")
}

/// ★ 真问一个登录 shell：登录时加进 `PATH` 的目录问得出来（后端进程自己的 `PATH` 里没有它）；
/// 问的那一趟 `TERM=dumb`（rc 里看终端类型的那几样别当成真终端）。
#[test]
#[cfg(unix)]
fn the_session_shell_path_comes_from_a_login_shell() {
    let home = login_box(
        "login",
        "echo 登录时的杂话\nPATH=\"$HOME/user-bin:$HOME/term-$TERM:$PATH\"\n",
    );
    let got = super::ask_session_path(sandboxed(&home), super::SESSION_PATH_WITHIN)
        .expect("沙箱里的登录 shell 问不出 PATH");
    let dirs: Vec<String> = std::env::split_paths(&got)
        .map(|d| d.to_string_lossy().into_owned())
        .collect();
    for want in ["user-bin", "term-dumb"] {
        let want = format!("{}/{want}", home.display());
        assert!(dirs.contains(&want), "{want} 不在问出来的 PATH 里：{got}");
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ rc 卡住（等终端 / 自动接 tmux 之类）⇒ 到期限算问不出来（`None`），不挂住后端。
#[test]
#[cfg(unix)]
fn a_stuck_login_shell_is_unknown_not_hung() {
    let home = login_box(
        "stuck",
        &format!("sleep {}\n", 2 * guard_core::wall::CEILING_SECS),
    );
    let t0 = std::time::Instant::now();
    let got = super::ask_session_path(sandboxed(&home), crate::platform::child::Deadline::secs(1));
    assert_eq!(got, None, "卡住的登录 shell 竟然答出了 PATH");
    assert!(
        t0.elapsed() < guard_core::wall::CEILING,
        "到期限没收手：等了 {:?}",
        t0.elapsed()
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// 夹具：这几个路径是普通文件，别的都不在。
fn files_at(present: &[std::path::PathBuf]) -> impl Fn(&std::path::Path) -> bool + '_ {
    move |p| present.iter().any(|f| f == p)
}

/// 缺陷（10-10）：Windows 上 agent 命令行认不出 / 起不来。找法与起法同一处答：
/// `PATH` 逐目录、同一目录里 `PATHEXT` 的默认次序（`.bat` 先于 `.cmd`），这几种哪儿都没有才认 `.ps1`（经 `powershell.exe -NoProfile -File` 起）；
/// 不带后缀的那份不算；unix 只认原名；`PATH` 问不出 ⇒ 判不了。
#[test]
fn a_session_command_is_found_and_started_the_windows_way() {
    use super::{find_session_command as find, SessionCommand, StartBy};
    use std::path::{Path, PathBuf};
    let a = PathBuf::from("/w/a");
    let b = PathBuf::from("/w/b");
    let both = std::env::join_paths([&a, &b]).unwrap();
    let both = both.to_str().unwrap();
    let only_a = a.to_str().unwrap();
    let at = |f: &PathBuf| SessionCommand::Found {
        file: f.clone(),
        program: f.clone().into_os_string(),
        lead: vec![],
        by: StartBy::Itself,
    };

    // npm 三件同在：挑 .cmd（不是 sh 壳、也不是 .ps1）
    let npm = [a.join("claude"), a.join("claude.cmd"), a.join("claude.ps1")];
    assert_eq!(
        find("claude", Some(only_a), true, &files_at(&npm)),
        at(&npm[1])
    );
    // 同一目录 .bat 与 .cmd：PATHEXT 里 .bat 在前
    let two = [a.join("claude.cmd"), a.join("claude.bat")];
    assert_eq!(
        find("claude", Some(only_a), true, &files_at(&two)),
        at(&two[1])
    );
    // 目录次序优先：前一个目录的 .cmd 先于后一个目录的 .exe
    let split = [a.join("claude.cmd"), b.join("claude.exe")];
    assert_eq!(
        find("claude", Some(both), true, &files_at(&split)),
        at(&split[0])
    );
    // .ps1 只在哪儿都没有 PATHEXT 那几种时才认：前一目录只有 .ps1、后一目录有 .exe ⇒ .exe
    let ps_then_exe = [a.join("claude.ps1"), b.join("claude.exe")];
    assert_eq!(
        find("claude", Some(both), true, &files_at(&ps_then_exe)),
        at(&ps_then_exe[1])
    );
    // 只有 .ps1 ⇒ 经 PowerShell 起它
    let ps = [a.join("claude.ps1")];
    assert_eq!(
        find("claude", Some(only_a), true, &files_at(&ps)),
        SessionCommand::Found {
            file: ps[0].clone(),
            program: "powershell.exe".into(),
            lead: vec![
                "-NoProfile".into(),
                "-File".into(),
                ps[0].clone().into_os_string()
            ],
            by: StartBy::PowerShellFile,
        }
    );
    // 只剩 sh 壳 ⇒ 没有
    let sh = [a.join("claude")];
    assert_eq!(
        find("claude", Some(only_a), true, &files_at(&sh)),
        SessionCommand::Absent
    );
    // unix：原名才算
    assert_eq!(
        find("claude", Some(only_a), false, &files_at(&sh)),
        at(&sh[0])
    );
    assert_eq!(
        find(
            "claude",
            Some(only_a),
            false,
            &files_at(&[a.join("claude.exe")])
        ),
        SessionCommand::Absent
    );
    // PATH 问不出 / 空 ⇒ 判不了
    assert_eq!(
        find("claude", None, true, &|_: &Path| true),
        SessionCommand::Unknown
    );
    assert_eq!(
        find("claude", Some("  "), true, &|_: &Path| true),
        SessionCommand::Unknown
    );
}

/// 只有 `.ps1`、执行策略又不让跑本地脚本（`Restricted` · `AllSigned`）⇒ 照实拒起、带那一档的原词，
/// 不悄悄加 `-ExecutionPolicy Bypass`。别的形不问执行策略（问一趟要起一个 PowerShell）；问不到 ⇒ 照起。
#[test]
fn a_script_install_is_refused_only_when_the_policy_forbids_scripts() {
    use super::{find_session_command as find, script_refused_by_policy as refused};
    use std::cell::Cell;
    use std::path::PathBuf;
    let a = PathBuf::from("/w/a");
    let only_a = a.to_str().unwrap();
    let ps = find(
        "claude",
        Some(only_a),
        true,
        &files_at(&[a.join("claude.ps1")]),
    );
    let cmd = find(
        "claude",
        Some(only_a),
        true,
        &files_at(&[a.join("claude.cmd")]),
    );
    let asked = Cell::new(0);
    let says = |w: Option<&'static str>| {
        let asked = &asked;
        move || {
            asked.set(asked.get() + 1);
            w.map(str::to_string)
        }
    };
    assert_eq!(
        refused(&ps, &says(Some("Restricted"))),
        Some("Restricted".into())
    );
    assert_eq!(
        refused(&ps, &says(Some("AllSigned"))),
        Some("AllSigned".into())
    );
    assert_eq!(refused(&ps, &says(Some("RemoteSigned"))), None);
    assert_eq!(refused(&ps, &says(None)), None, "问不到 ⇒ 照起");
    assert_eq!(asked.get(), 4);
    assert_eq!(refused(&cmd, &says(Some("Restricted"))), None);
    assert_eq!(
        refused(&super::SessionCommand::Absent, &says(Some("Restricted"))),
        None
    );
    assert_eq!(asked.get(), 4, "不是脚本那一形就不该去问执行策略");
}

/// ★ 后端里按 `PATH` 一目录一目录找命令的只许 [`super::find_session_command`] 这一处（Windows 后缀规矩只写一遍：
/// 「认得出」就是「起得来」）。认法：生产代码里切 `PATH` 的那一下（`split_paths(`）只许住 `platform/shell/mod.rs`、恰好一处。
#[test]
fn only_find_session_command_walks_path() {
    let root = guard_core::addr::backend_src_root();
    let files = guard_core::scan_tree_excluding(&root, &["rs"], &[]);
    assert!(
        files.len() > 200,
        "只扫到 {} 个文件 —— 采集坏了",
        files.len()
    );
    const HOME: &str = "platform/shell/mod.rs";
    let mut hits: Vec<String> = Vec::new();
    let mut home_count = 0;
    for (p, text) in &files {
        let rel = p
            .strip_prefix(&root)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/");
        let code = guard_core::production_code(text);
        // 别处拿 `path_dirs` 只许数目录（找不到时那句话里说查过几个）：每一处后面紧跟 `.len()`。
        if rel != HOME {
            for (i, _) in code.match_indices("shell::path_dirs(") {
                let rest = &code[i..];
                let close = rest.find(')').unwrap_or(rest.len());
                if !rest[close..].starts_with(").len()") {
                    hits.push(format!("{rel}：拿 path_dirs 自己逐目录找"));
                }
            }
        }
        let n = code.matches("split_paths(").count();
        if n == 0 {
            continue;
        }
        if rel == HOME {
            home_count = n;
        } else {
            hits.push(format!("{rel} ×{n}"));
        }
    }
    assert!(
        hits.is_empty(),
        "按 PATH 逐目录找命令只许经 `platform::shell::find_session_command`：\n{}",
        hits.join("\n")
    );
    assert_eq!(
        home_count, 1,
        "{HOME} 里切 PATH 的应恰好一处（find_session_command 那一下）"
    );
}

/// Windows 上问新开终端的 `PATH` 那一句：标记成对夹住输出（读回只认 `parse_marked_path` 那一对）、标记按 PowerShell 字面量写。
#[test]
fn the_fresh_path_query_brackets_its_answer_with_the_mark() {
    let q = super::powershell::fresh_path_query(super::SESSION_PATH_MARK);
    let lit = super::dialect::ps_literal(super::SESSION_PATH_MARK);
    assert_eq!(q.matches(lit.as_str()).count(), 2, "{q}");
    assert!(q.contains("'Machine'") && q.contains("'User'"), "{q}");
}

/// ★ 进一个目录那一行：方言按这台（POSIX ⇒ `cd`，PowerShell ⇒ `Set-Location -LiteralPath`），目录照各自的引号器成一个词。
#[test]
fn entering_a_dir_is_written_in_each_dialect() {
    use super::dialect::Shell;
    assert_eq!(
        super::enter_dir_line_in(Shell::Posix, "/a b/it's"),
        r"cd '/a b/it'\''s'"
    );
    assert_eq!(
        super::enter_dir_line_in(Shell::PowerShell, r"C:\a b\it's"),
        r"Set-Location -LiteralPath 'C:\a b\it''s'"
    );
    let here = if cfg!(windows) {
        Shell::PowerShell
    } else {
        Shell::Posix
    };
    assert_eq!(
        super::enter_dir_line("/p"),
        super::enter_dir_line_in(here, "/p")
    );
}
