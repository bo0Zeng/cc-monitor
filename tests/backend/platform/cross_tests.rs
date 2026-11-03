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
