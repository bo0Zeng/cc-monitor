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

/// 取第一行非空内容：前面的空白行跳过；全是空白就是空串。
#[test]
fn the_first_line_skips_blank_lines() {
    assert_eq!(first_line(b"\n  \nerr-1\nerr-2\n"), "err-1");
    assert_eq!(first_line(b"   \n"), "");
    // 两头空白去掉（`\r\n` 行尾的 `\r` 也不进诊断）。
    assert_eq!(first_line(b"\n  err-3 \r\n"), "err-3");
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
    let Err(NotRun::Failed(s)) = run_fed(&gone, &[], 5, &[], None, None) else {
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

/// 给了目录 ⇒ 子进程的当前目录就是它；不给 ⇒ 照宿主的。
#[cfg(unix)]
#[test]
fn the_child_runs_in_the_given_dir() {
    let d = std::env::temp_dir().join(format!("ccm-invoke-dir-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let out = run_fed(Path::new("pwd"), &["-P"], 5, &[], Some(&d), None)
        .unwrap_or_else(|_| panic!("起不来 pwd"));
    assert_eq!(
        std::path::PathBuf::from(first_line(&out.stdout)),
        std::fs::canonicalize(&d).unwrap()
    );
    let _ = std::fs::remove_dir_all(&d);
}
