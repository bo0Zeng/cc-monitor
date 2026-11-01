//! 平台层那几份判据里引到上层的几条：平台层是独立的包、够不到上层，这几条挂在后端库根（上层拆成包时各自带走）。

/// 起子进程原语摘掉的自有环境 · 起常驻载体交下去的 · 会话号变量（上层：control · wire · stderr_log · agents）。
mod child {
    use crate::platform::child::{Child, Deadline};
    use crate::platform::child_env::OWN_ENVS;

    /// 远端 `--resident-ensure` 交给常驻载体的那几格 == 自有那一份 ＋ 中转口（中转口要往下传给窗格里的 ccm）。
    #[test]
    fn the_resident_carrier_gets_exactly_the_own_envs_and_the_relay_port() {
        let handed: std::collections::BTreeSet<String> = crate::control::resident::child_env(
            std::path::Path::new("/h"),
            &[
                (crate::wire::listen::RELAY_PORT_ENV, "8788".into()),
                (crate::stderr_log::ENV, "~/x.log".into()),
            ],
        )
        .into_iter()
        .map(|(k, _)| k)
        .collect();
        let mut want_handed: std::collections::BTreeSet<String> =
            OWN_ENVS.iter().map(|s| s.to_string()).collect();
        want_handed.insert(crate::wire::listen::RELAY_PORT_ENV.to_string());
        assert_eq!(handed, want_handed, "起常驻载体交的 ≠ 自有那一份 ＋ 中转口");
        assert!(
            !OWN_ENVS.contains(&crate::wire::listen::RELAY_PORT_ENV),
            "中转口不许清：窗格里的 ccm 靠它找同机中转，清掉就去连默认口"
        );
    }

    /// `CCM_LISTEN_PORT` → `Ccm_lIsTeN_…`：逐字母交替大小写（下划线不动），保证与全大写、全小写都不逐字相等。
    fn mixed_case(k: &str) -> String {
        k.chars()
            .enumerate()
            .map(|(i, c)| {
                if i % 2 == 0 {
                    c.to_ascii_uppercase()
                } else {
                    c.to_ascii_lowercase()
                }
            })
            .collect()
    }

    /// 〔环境里没有后端内部变量，也没有会话号变量〕会话环境里混进来的后端内部变量（monitor 给终端窗口导的后端二进制路径 · 旧版常驻后端留在长寿 tmux server
    /// 全局环境里的监听口与钥匙文件 · 常驻开关 · 诊断文件）—— 起出来的每个子进程（agent 会话就在里面）一个都看不见；
    /// 真要往下传的那几格（中转口 · 家）照旧到得了。外层带着这些变量把本测试二进制当子进程起、跑内层（进程环境不能在多线程测试里改）。
    #[test]
    fn a_child_never_inherits_the_backend_internal_families() {
        assert!(crate::platform::child_env::is_internal("CCM_LISTEN_PORT"));
        assert!(crate::platform::child_env::is_internal("CCM_BACKEND_BIN"));
        assert!(crate::platform::child_env::is_internal("CCM_RESIDENT"));
        assert!(!crate::platform::child_env::is_internal("CCM_RELAY_PORT"));
        assert!(!crate::platform::child_env::is_internal("CCM_DATA_DIR"));
        // Windows 上名字不分大小写：外层故意用大小写混写的那一形交（真机上漏下去的就是这一形），内层照样一个都不许看见。
        let spell = |k: &str| -> String {
            if cfg!(windows) {
                mixed_case(k)
            } else {
                k.to_string()
            }
        };
        let out = std::process::Command::new(std::env::current_exe().expect("测试二进制"))
            .args([
                "platform_cross_tests::child::internal_families_inner",
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(INTERNAL_MARK, "1")
            .env(spell("CCM_LISTEN_PORT"), "52917")
            .env(
                spell("CCM_LISTEN_TOKEN_FILE"),
                "/h/.cc-monitor/listen-token",
            )
            .env(
                spell("CCM_BACKEND_STDERR_LOG"),
                "/h/.cc-monitor/logs/backend/stderr.log",
            )
            .env(spell("CCM_BACKEND_BIN"), "/h/.cc-monitor/bin/ccm")
            .env(spell("CCM_RESIDENT"), "1")
            .env("CCM_RELAY_PORT", "8788")
            .env("CCM_DATA_DIR", "/iso/home")
            .envs(
                crate::agents::self_sid_envs()
                    .into_iter()
                    .map(|k| (spell(k), "parent-sid")),
            )
            .output()
            .expect("起内层");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(text.contains(INTERNAL_READING), "内层没跑到：{text}");
        assert!(out.status.success(), "内层红了：{text}");
    }

    const INTERNAL_MARK: &str = "CCM_CHILD_INTERNAL_INNER";
    const INTERNAL_READING: &str = "INTERNAL-FAMILIES-READ";

    #[test]
    #[ignore = "内层入口：只在被外层用 CCM_CHILD_INTERNAL_INNER 拉起时才做断言"]
    fn internal_families_inner() {
        if std::env::var(INTERNAL_MARK).is_err() {
            return;
        }
        // 与 `main.rs` 同一个入口：登记会话号变量。
        crate::agents::install_child_env_filter();
        let out = if cfg!(windows) {
            Child::new("cmd").args(["/C", "set"])
        } else {
            Child::new("env")
        }
        .run(Deadline::secs(10))
        .expect("起 env");
        let text = String::from_utf8_lossy(&out.stdout);
        let names: std::collections::BTreeMap<&str, &str> =
            text.lines().filter_map(|l| l.split_once('=')).collect();
        let leaked: Vec<&str> = names
            .keys()
            .copied()
            .filter(|k| crate::platform::child_env::is_internal(k))
            .collect();
        assert_eq!(leaked, Vec::<&str>::new(), "后端内部变量漏给了子进程");
        let sids = crate::agents::self_sid_envs();
        assert!(
            !sids.is_empty(),
            "适配层一个「我是哪个会话」的变量都没登记 —— 下面那一格空转"
        );
        let leaked_sid: Vec<&str> = names
            .keys()
            .copied()
            .filter(|k| {
                sids.iter()
                    .any(|s| crate::platform::child_env::same_name(s, k))
            })
            .collect();
        assert_eq!(
            leaked_sid,
            Vec::<&str>::new(),
            "会话号变量漏给了子进程（它起的会话会把那个会话错当成父）"
        );
        assert_eq!(
            names.get("CCM_RELAY_PORT"),
            Some(&"8788"),
            "正控：中转口没到"
        );
        assert_eq!(
            names.get("CCM_DATA_DIR"),
            Some(&"/iso/home"),
            "正控：家没到"
        );
        println!("{INTERNAL_READING}");
    }
}

/// 两种方言的别名读法里用到上层那条调用形状与别名块正文的几条（上层：assets 的别名那一族）。
mod dialect {
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

    /// 黄金串 · PowerShell：函数体是从前别名块里那个 `function cc` 的形状（那一形在 `K-R132` 真机上
    /// `parse-errors=0`；`cc` 今天就是清单里的一条），预置参数**每个都单引号**、四种引号字符都双写。
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
            .is_none_or(|n| !copy_core::copy_matches(
                "rsShellDialect.ps.nameTakenIntegration",
                &n
            )));
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
}

/// 自家状态落盘的编码那一半（上层：common 的 `own_state`）：一份 JSON 编成一行、带结尾换行。
mod own {
    #[test]
    fn a_json_body_is_one_line_ending_in_a_newline() {
        let p = std::path::Path::new("/h/.cc-monitor/x.json");
        assert_eq!(
            crate::common::own_state::json_body(p, &serde_json::json!({"a": 1})).expect("编码"),
            b"{\"a\":1}\n"
        );
    }
}
