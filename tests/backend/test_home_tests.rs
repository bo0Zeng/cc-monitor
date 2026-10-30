//! `src/common/test-home` 自己的行为：换不换家 · 摘哪几样会话变量 · 测试进程看不见外面的会话 · 临时目录跑完照旧。
//! 子进程那几条要一个真测试二进制把自己再拉起来，挂在后端的测试二进制里跑。

use std::path::Path;

/// 换不换家：`HOME` 是真家（或没设）⇒ 换；判据自己给的家照用；查不到真家 ⇒ 一律换。
#[test]
fn only_the_real_home_is_moved() {
    let real = Path::new("/home/someone");
    assert!(test_home::must_move(Some(real), Some(real)));
    assert!(test_home::must_move(None, Some(real)));
    assert!(!test_home::must_move(
        Some(Path::new("/tmp/x/home")),
        Some(real)
    ));
    assert!(test_home::must_move(Some(real), None));
}

/// 真家的另一种写法（指向它的符号链接 · 带 `..` 的那一形）也是真家：照样换家，表里的会话变量一律摘（连落在那个家底下的也摘）。
#[cfg(unix)]
#[test]
fn another_spelling_of_the_real_home_is_still_the_real_home() {
    let d = std::env::temp_dir().join(format!("home-spelling-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let real = d.join("real");
    std::fs::create_dir_all(&real).unwrap();
    let link = d.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    std::fs::create_dir_all(d.join("x")).unwrap();
    let dotted = d.join("x/../real");
    for home in [&link, &dotted] {
        assert!(
            test_home::must_move(Some(home), Some(&real)),
            "{} 就是真家",
            home.display()
        );
        let inside = home.join(".claude");
        assert!(
            test_home::strays(
                "CLAUDE_CONFIG_DIR",
                inside.as_os_str(),
                Some(home),
                Some(&real)
            ),
            "家是真家 ⇒ 表里的一律摘：{}",
            inside.display()
        );
    }
    let other = d.join("x");
    assert!(
        !test_home::must_move(Some(&other), Some(&real)),
        "别的家照用"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 外面那个会话漏进来、子测试进程里要看见「都不在」的那几样（名字 ＝ 值随便；真值是什么不要紧，要紧的是进不来）。
const LEAKS: &[&str] = &[
    "CLAUDE_CONFIG_DIR",
    "CLAUDE_CODE_PLUGIN_DIRS",
    "TMUX",
    "TMUX_PANE",
    "CCM_RELAY_PORT",
    "CCM_DATA_DIR",
    "CCM_RESIDENT",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_API_KEY",
    "CLAUDECODE",
    "AI_AGENT",
    "CODEX_HOME",
    "CC_BUS_ID",
    "HTTPS_PROXY",
];
/// 台架 / 人交给判据的输入：照留（反控：清单没把 `CCM_` 一族一刀切）。
const KEPT: &[&str] = &["CCM_E2E_WORK", "CCM_SHOTS_NOW_MS", "CCM_SANDBOX"];
const CHILD: &str = "CCM_E2E_TEST_HOME_CHILD";

/// 子进程那一半：只在下面那条判据带着一身外面会话的变量把本测试二进制再拉起来时跑。
#[test]
#[ignore = "子进程入口：只在 a_test_process_never_sees_the_outer_session 拉起时跑"]
fn child_sees_no_outer_session() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    // 像历史判据那样照环境解出记录树、往里写一条：写到哪儿，父判据在外面看。
    let tree = crate::agents::claudecode::paths::projects_root(
        &crate::agents::claudecode::paths::resolve_home(),
    );
    std::fs::create_dir_all(tree.join("-p")).expect("建记录树");
    std::fs::write(tree.join("-p/s.jsonl"), b"{}\n").expect("写一条记录");
    let left: Vec<&str> = LEAKS
        .iter()
        .copied()
        .filter(|k| std::env::var_os(k).is_some())
        .collect();
    assert!(left.is_empty(), "这几样从外面漏进了测试进程：{left:?}");
    for k in KEPT {
        assert!(std::env::var_os(k).is_some(), "{k} 是判据的输入，不该摘");
    }
}

/// ★ 测试进程里没有外面会话的状态：带着 `CLAUDE_CONFIG_DIR` · `TMUX` · `TMUX_PANE` · `CCM_RELAY_PORT` · `CCM_*` · `ANTHROPIC_*` · 代理 等
/// 把本测试二进制当一次新进程拉起（摘掉换家标记），子进程里一样都看不见；判据的输入照留。`HOME` 两形各起一回：给真家（会话里直接敲
/// `cargo test`）· 给一个别的家（跑的人只换了 `HOME`、没摘会话变量）。`CLAUDE_CONFIG_DIR` 指向本判据在自己临时目录里新建的假账号目录
/// （子进程的沙箱之外，从不指向真账号）；子进程照环境往记录树里写一条，那个假账号目录底下不许冒出记录树。
#[test]
fn a_test_process_never_sees_the_outer_session() {
    let exe = std::env::current_exe().expect("拿不到测试二进制自己的路径");
    let scratch = std::env::temp_dir().join(format!("outer-session-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let account = scratch.join("fake-account");
    let given = scratch.join("given-home");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::create_dir_all(&given).unwrap();
    // 真家查得到 ⇒ 给真家（子进程起来那一刻就换走）；查不到（非 unix）⇒ 不给（没设一样换）。
    for home in [test_home::account_home(), Some(given.clone())] {
        let mut cmd = std::process::Command::new(&exe);
        cmd.args([
            "--exact",
            "test_home_tests::child_sees_no_outer_session",
            "--ignored",
            "--test-threads=1",
        ])
        .env_remove(test_home::SANDBOX_MARK)
        .env(CHILD, "1");
        match &home {
            Some(h) => cmd.env("HOME", h),
            None => cmd.env_remove("HOME"),
        };
        // 外面会话的值都指向判据自建的那个假账号目录（给了别的家时，摘的是指向家以外的路径）。
        for k in LEAKS {
            cmd.env(k, &account);
        }
        for k in KEPT {
            cmd.env(k, "outer-session");
        }
        let out = cmd.output().expect("起子测试进程");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let wrote = account.join("projects").exists();
        let _ = std::fs::remove_dir_all(account.join("projects"));
        assert!(
            !wrote,
            "HOME={home:?}：子测试进程照外面的 CLAUDE_CONFIG_DIR 把记录写进了沙箱外的账号目录\n{text}"
        );
        assert!(
            out.status.success() && text.contains("1 passed"),
            "HOME={home:?}：子测试进程里看见了外面会话的变量：\n{text}"
        );
    }
    let _ = std::fs::remove_dir_all(&scratch);
}

/// 摘哪一格：真家（或没设家）⇒ 表里的都摘；给了别的家 ⇒ 只摘指向那个家以外的绝对路径，家里的路径与不是路径的值（父判据交给
/// `env_clear()` 子进程的 `CCM_HOST=lne` 之类）照留；表外的一律不碰。路径都是编的，不碰盘。
#[test]
fn a_given_home_keeps_inputs_but_not_paths_outside_it() {
    use std::ffi::OsStr;
    let real = Path::new("/nowhere/real-home");
    let given = Path::new("/nowhere/sandbox/home");
    let strays = |k: &str, v: &str, home: Option<&Path>| {
        test_home::strays(k, OsStr::new(v), home, Some(real))
    };
    let acct = "/nowhere/real-home/.cc-monitor/accounts/z";
    for home in [None, Some(real)] {
        assert!(strays("CLAUDE_CONFIG_DIR", acct, home));
        assert!(strays("CCM_HOST", "lne", home));
    }
    assert!(strays("CLAUDE_CONFIG_DIR", acct, Some(given)));
    assert!(strays("CCM_DATA_DIR", "/nowhere/elsewhere", Some(given)));
    assert!(strays("TMUX", "/tmp/tmux-1000/default,1,0", Some(given)));
    let inside = "/nowhere/sandbox/home/data";
    assert!(!strays("CCM_DATA_DIR", inside, Some(given)));
    assert!(!strays("CCM_HOST", "lne", Some(given)));
    assert!(!strays("CCM_RELAY_PORT", "4242", Some(given)));
    assert!(!strays("PATH", "/usr/bin", None));
    assert!(!strays("CCM_E2E_WORK", "/nowhere/elsewhere", Some(given)));
}

/// 清单本身：摘的与留的各几样（同一份 `leaked`）。
#[test]
fn the_scrub_list_takes_session_state_and_keeps_test_inputs() {
    for k in LEAKS {
        assert!(test_home::leaked(k), "{k} 该摘");
    }
    for k in KEPT
        .iter()
        .chain(&["CCM_FILEWIN_PROC_CHILD", "HOME", "PATH"])
    {
        assert!(!test_home::leaked(k), "{k} 不该摘");
    }
}

const LITTER: &str = "CCM_E2E_TEST_TMP_CHILD";

/// 子进程那一半：在临时目录里留一棵树（`fail` ⇒ 留完再红）。只在下面那条判据拉起时跑。
#[test]
#[ignore = "子进程入口：只在 a_test_process_leaves_the_temp_dir_as_it_found_it 拉起时跑"]
fn child_litters_the_temp_dir() {
    let Some(mode) = std::env::var_os(LITTER) else {
        return;
    };
    let d = std::env::temp_dir().join(format!("litter-{}", std::process::id()));
    std::fs::create_dir_all(d.join("deep")).unwrap();
    std::fs::write(d.join("deep/f"), b"x").unwrap();
    println!("littered {}", d.display());
    assert!(mode != "fail", "照令红");
}

/// ★ 测试进程跑完（绿或红）临时目录与跑之前一样：给子测试进程一个新建的空临时目录，它在里面留下的东西随它退出删掉。
/// 正控：子进程确实跑了、确实留了（stdout 里那一行）。
#[test]
fn a_test_process_leaves_the_temp_dir_as_it_found_it() {
    let exe = std::env::current_exe().expect("拿不到测试二进制自己的路径");
    for (mode, verdict) in [("pass", "1 passed"), ("fail", "1 failed")] {
        let empty = std::env::temp_dir().join(format!("tmp-before-{mode}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&empty);
        std::fs::create_dir_all(&empty).unwrap();
        let mut cmd = std::process::Command::new(&exe);
        cmd.args([
            "--exact",
            "test_home_tests::child_litters_the_temp_dir",
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .env_remove(test_home::SANDBOX_MARK)
        .env(LITTER, mode);
        for k in ["TMPDIR", "TMP", "TEMP"] {
            cmd.env(k, &empty);
        }
        let out = cmd.output().expect("起子测试进程");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        // 空目录才删得掉（`remove_dir` 不删非空的）。
        let left = std::fs::remove_dir(&empty).err();
        if left.is_some() {
            std::fs::remove_dir_all(&empty).expect("收拾判据自己的目录");
        }
        assert!(
            text.contains("littered ") && text.contains(verdict),
            "子测试进程没照令跑（{mode}）：\n{text}"
        );
        assert!(
            left.is_none(),
            "子测试进程（{mode}）退出后临时目录里留下了东西：{left:?}\n{text}"
        );
    }
}
