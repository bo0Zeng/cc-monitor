use super::*;

/// argv 原样交给插件本身（不过 shell、不加前缀）；环境只剩白名单从宿主继承的 ∪ 显式交办的。
#[test]
fn the_plugin_gets_its_argv_verbatim_and_a_whitelisted_env() {
    let c = child_for(
        Path::new("/opt/p/tool"),
        &["--", "a b", "c"],
        &[("X_ONE", "1")],
    )
    .built();
    assert_eq!(c.get_program(), "/opt/p/tool");
    let argv: Vec<String> = c
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(argv, ["--", "a b", "c"]);
    let allowed: Vec<&str> = INHERITED_ENV_KEYS.iter().map(|(k, _)| *k).collect();
    for (k, v) in c.get_envs() {
        let k = k.to_string_lossy();
        if v.is_some() {
            assert!(
                allowed.contains(&k.as_ref()) || k == "X_ONE",
                "白名单外的键到了插件那里：{k}"
            );
        }
    }
}

/// 「被信号打断」与「退出码是几」是**两件事**，骨架必须分得开。
#[test]
fn a_signal_death_is_not_an_exit_code() {
    let killed = Done {
        code: None,
        stdout: Vec::new(),
        stderr: Vec::new(),
        waited_secs: None,
    };
    assert!(!killed.timed_out(), "码是 None 不该被算成超时");
    let expired = Done {
        code: Some(TIMED_OUT_CODE),
        stdout: Vec::new(),
        stderr: Vec::new(),
        waited_secs: None,
    };
    assert!(expired.timed_out());
    let ok = Done {
        code: Some(0),
        stdout: Vec::new(),
        stderr: Vec::new(),
        waited_secs: None,
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
        waited_secs: None,
    };
    assert_eq!(d.diagnosis(), "err-1", "stderr 里的第一行非空内容没被取到");
    let d2 = Done {
        code: Some(1),
        stdout: b"out-1\n".to_vec(),
        stderr: b"   \n".to_vec(),
        waited_secs: None,
    };
    assert_eq!(d2.diagnosis(), "out-1", "stderr 全空白时没退回 stdout");
    let d3 = Done {
        code: Some(1),
        stdout: Vec::new(),
        stderr: Vec::new(),
        waited_secs: None,
    };
    assert_eq!(d3.diagnosis(), "");
}

/// 非 UTF-8 的输出不许让取诊断这一步炸掉。
#[test]
fn invalid_utf8_output_still_yields_a_line() {
    assert_eq!(first_line(&[0xff, 0xfe, b'\n', b'x']), "\u{fffd}\u{fffd}");
}

/// 起不来：句子只带原因词（未装），系统原话另带、不上句子。
#[test]
fn a_program_that_is_not_there_is_said_with_a_reason_word() {
    let gone = std::env::temp_dir().join(format!("ccm-invoke-gone-{}", std::process::id()));
    let Err(NotRun::Failed(s)) = run(&gone, &[], 5, &[]) else {
        panic!("不存在的程序起来了");
    };
    assert_eq!(
        s.said,
        copy_text(
            "beInvoke.notRun.failed",
            &[
                ("bin", &gone.display().to_string()),
                (
                    "why",
                    &copy_core::spawn_reason(std::io::ErrorKind::NotFound)
                )
            ]
        )
    );
    assert!(s.raw.is_some(), "系统原话丢了");
}
