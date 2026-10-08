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

/// ★ 住址同上。三行（生效 · `MachinePolicy` · `UserPolicy`）读成现状：组策略任一档有值 ⇒ 钉着；行数不对 ⇒ 说认不出、带原话。
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
    assert!(
        c.error.is_some_and(|e| e.contains("RemoteSigned")),
        "认不出却没带原话"
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
    let home = std::env::temp_dir().join(format!("ccm-session-path-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join(".profile"), profile).unwrap();
    home
}

#[cfg(unix)]
fn sandboxed(home: &std::path::Path) -> crate::platform::child::Child {
    super::login_shell_asking_path("sh")
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
    let home = login_box("stuck", "sleep 30\n");
    let t0 = std::time::Instant::now();
    let got = super::ask_session_path(sandboxed(&home), crate::platform::child::Deadline::secs(1));
    assert_eq!(got, None, "卡住的登录 shell 竟然答出了 PATH");
    assert!(
        t0.elapsed() < std::time::Duration::from_secs(10),
        "到期限没收手：等了 {:?}",
        t0.elapsed()
    );
    let _ = std::fs::remove_dir_all(&home);
}
