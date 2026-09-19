use super::*;

/// ★ 有期限命令时：它当**前缀**，秒数紧随其后，插件与参数原样跟在后面。
#[test]
fn the_deadline_command_becomes_the_prefix() {
    let t = PathBuf::from("/usr/bin/timeout");
    let bin = PathBuf::from("/opt/p/tool");
    let (prog, argv) = argv_for(&bin, &["--", "a b", "c"], 7, Some(&t));
    assert_eq!(prog, t, "程序应当是那条期限命令");
    assert_eq!(
        argv,
        vec![
            "7".to_string(),
            "/opt/p/tool".to_string(),
            "--".to_string(),
            "a b".to_string(),
            "c".to_string()
        ],
        "秒数没排在插件前面，或参数被改了形状"
    );
}

/// ★★ **降级那条路**：找不到期限命令就裸跑 —— 这一格此前只有一句头注。
///
/// 钉两件事：① 起的就是插件自己；② 秒数**一个字都不许出现在 argv 里**
///（不然会变成插件自己的第一个参数，那是货真价实的错传）。
#[test]
fn without_a_deadline_command_it_runs_bare_and_says_so_in_the_argv() {
    let bin = PathBuf::from("/opt/p/tool");
    let (prog, argv) = argv_for(&bin, &["x"], 7, None);
    assert_eq!(prog, bin, "裸跑时起的应当是插件自己");
    assert_eq!(argv, vec!["x".to_string()], "裸跑时不该有任何前缀参数");
    assert!(
        !argv.contains(&"7".to_string()),
        "秒数漏进了插件的 argv —— 那会被它当成一个真参数：{argv:?}"
    );
}

/// 没有参数的调用，两种形态都不该多出空串。
#[test]
fn an_empty_arg_list_stays_empty() {
    let bin = PathBuf::from("/opt/p/tool");
    let (_, bare) = argv_for(&bin, &[], 3, None);
    assert!(bare.is_empty(), "{bare:?}");
    let t = PathBuf::from("/usr/bin/timeout");
    let (_, pre) = argv_for(&bin, &[], 3, Some(&t));
    assert_eq!(pre, vec!["3".to_string(), "/opt/p/tool".to_string()]);
}

/// 「被信号打断」与「退出码是几」是**两件事**，骨架必须分得开。
#[test]
fn a_signal_death_is_not_an_exit_code() {
    let killed = Done {
        code: None,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    assert!(!killed.timed_out(), "码是 None 不该被算成超时");
    let expired = Done {
        code: Some(TIMED_OUT_CODE),
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    assert!(expired.timed_out());
    let ok = Done {
        code: Some(0),
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    assert!(!ok.timed_out());
}

/// 诊断取法：**stderr 优先**，空了才退回 stdout；两条都空就是空串。
#[test]
fn the_diagnosis_prefers_stderr_and_falls_back_to_stdout() {
    let d = Done {
        code: Some(1),
        stdout: b"out-1\nout-2\n".to_vec(),
        stderr: b"\n  \nerr-1\nerr-2\n".to_vec(),
    };
    assert_eq!(d.diagnosis(), "err-1", "stderr 里的第一行非空内容没被取到");
    let d2 = Done {
        code: Some(1),
        stdout: b"out-1\n".to_vec(),
        stderr: b"   \n".to_vec(),
    };
    assert_eq!(d2.diagnosis(), "out-1", "stderr 全空白时没退回 stdout");
    let d3 = Done {
        code: Some(1),
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    assert_eq!(d3.diagnosis(), "");
}

/// 非 UTF-8 的输出不许让取诊断这一步炸掉。
#[test]
fn invalid_utf8_output_still_yields_a_line() {
    assert_eq!(first_line(&[0xff, 0xfe, b'\n', b'x']), "\u{fffd}\u{fffd}");
}
