use super::*;

fn ex(stdout: &str, stderr: &str, status: Option<u32>) -> RemoteExec {
    RemoteExec {
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
        exit_status: status,
    }
}

const OK_LINE: &str =
    r#"{"sessionId":"new-sid-1","jsonlPath":"/home/pi/.claude/projects/p/new-sid-1.jsonl"}"#;

#[test]
fn exit_zero_parses_result() {
    let r = interpret_fork_exec(&ex(OK_LINE, "", Some(0))).unwrap();
    assert_eq!(r.session_id, "new-sid-1");
    assert!(r.jsonl_path.ends_with("new-sid-1.jsonl"));
}

/// ★ 失败必须**可见**：backend 的 `{code,message}` 要变成人话，不能静默。
#[test]
fn exit_two_surfaces_backend_message() {
    let e = interpret_fork_exec(&ex(
        "",
        r#"{"code":"fork_failed","message":"refuse fork: message uuid not found"}"#,
        Some(2),
    ))
    .unwrap_err();
    assert!(e.contains("message uuid not found"), "got: {e}");
    assert!(e.contains("fork_failed"), "错误码也要带上: {e}");
}

/// 认不出信封时**原样带出**，而不是吞成一句泛泛的失败。
#[test]
fn unrecognized_stderr_is_passed_through() {
    let e = interpret_fork_exec(&ex("", "bash: line 1: ccmd: command not found", Some(127)))
        .unwrap_err();
    assert!(e.contains("command not found"), "got: {e}");
}

/// ★★ 没收到退出码**绝不当成 0**。真出现过的形态：连接被中途掐断，stdout 是空的，
/// 若把 `None` 当 0 就会走进「报成功却没有输出」那条，措辞会把锅甩给后端。
#[test]
fn missing_exit_status_is_failure_not_success() {
    let e = interpret_fork_exec(&ex("", "", None)).unwrap_err();
    assert!(e.contains("没收到退出码"), "got: {e}");
}

/// exit 0 但 stdout 解析不出来 —— 仍是失败，**绝不**返回空壳结果。
#[test]
fn exit_zero_with_garbage_stdout_fails() {
    let e = interpret_fork_exec(&ex("not json at all", "", Some(0))).unwrap_err();
    // Phase G 起措辞改成指向真正的怀疑对象（远端 shell 打了 banner），不再叫「解析失败」。
    assert!(e.contains("找不到结果 JSON"), "got: {e}");
    let e2 = interpret_fork_exec(&ex("", "", Some(0))).unwrap_err();
    assert!(e2.contains("没有输出结果"), "got: {e2}");
}

/// ★ 旧后端不认参数会进流模式、先吐 hello 帧。必须**先于**解析判掉，
/// 否则用户看到的是「结果解析失败」而不是「去重新部署后端」。
#[test]
fn old_backend_hello_is_detected_first() {
    let hello = r#"{"kind":"hello","v":1,"build_id":"old"}"#;
    let e = interpret_fork_exec(&ex(hello, "", Some(0))).unwrap_err();
    assert!(e.contains("版本过旧"), "got: {e}");
}

/// `HELLO_MARKER` 同时是 `connect_and_exec_capture` 的 abort marker——
/// 它必须能在**没有换行**的半行上命中，所以只能是子串判、不能是整行判。
#[test]
fn hello_marker_matches_partial_line() {
    let partial = r#"{"kind":"hello","v":1,"build_i"#; // 半行，无 \n
    assert!(partial.contains(HELLO_MARKER));
}

/// ★ Phase G 审计：远端登录 shell 打 banner 是常见配置（本仓的部署流程本来就动 `.bashrc`）。
/// 只看第一条非空行的话，**分叉已经成功、文件已落盘、exit 0**，monitor 却报「解析失败」，
/// 用户重试就在远端多留一份孤儿分支文件。
#[test]
fn banner_before_result_does_not_break_parsing() {
    let out = format!("Welcome to raspberrypi!\n *  System load: 0.0\n{OK_LINE}\n");
    let r = interpret_fork_exec(&ex(&out, "", Some(0))).unwrap();
    assert_eq!(r.session_id, "new-sid-1");
}

/// 同理，stderr 上的 `{code,message}` 信封也不该被前面任意一行噪声顶掉。
#[test]
fn banner_before_error_envelope_still_surfaces_the_reason() {
    let err = "bash: warning: setlocale: LC_ALL: cannot change locale\n\
                   {\"code\":\"fork_failed\",\"message\":\"refuse fork: message uuid not found\"}\n";
    let e = interpret_fork_exec(&ex("", err, Some(2))).unwrap_err();
    assert!(e.contains("message uuid not found"), "got: {e}");
}

/// exit 0 但**只有噪声没有结果** —— 措辞要指向真正的怀疑对象（远端 shell），
/// 而不是甩锅给后端说它「没有输出」。
#[test]
fn noise_only_stdout_says_where_to_look() {
    let e = interpret_fork_exec(&ex("some banner line\n", "", Some(0))).unwrap_err();
    assert!(e.contains("banner"), "got: {e}");
}

/// 带空格的 hello 写法也要认（原来只认无空格那条，注释却说与 `remote_history` 同判据）。
#[test]
fn spaced_hello_is_also_detected() {
    let e = interpret_fork_exec(&ex(r#"{"kind": "hello","v":1}"#, "", Some(0))).unwrap_err();
    assert!(e.contains("版本过旧"), "got: {e}");
}

#[test]
fn fork_ids_are_whitelisted() {
    assert!(validate_fork_id("sid", "0473c3a0-1111-2222-3333-444455556666").is_ok());
    for bad in ["", "../etc/passwd", "a b", "a;rm -rf /", "a'b", "a/b"] {
        assert!(validate_fork_id("sid", bad).is_err(), "该拒: {bad:?}");
    }
    assert!(
        validate_fork_id("sid", &"a".repeat(65)).is_err(),
        "上限该与后端的 64 对齐"
    );
    assert!(validate_fork_id("sid", &"a".repeat(64)).is_ok());
}

/// 命令形状 = backend 的 argv 契约。**位置参数顺序错了后端会拿 uuid 当 sid 去找文件**，
/// 于是报一句「找不到会话」，排查方向被带偏一整轮——所以钉死。
#[test]
fn fork_cmd_shape_is_pinned() {
    let c = build_fork_cmd("/home/pi/.cc-monitor/bin/p1q", "src-sid", "msg-uuid");
    assert_eq!(
        c,
        "'/home/pi/.cc-monitor/bin/p1q' --fork-session 'src-sid' 'msg-uuid'"
    );
}

/// 纵深防御：id 已过白名单，`shell_quote` 仍照上（白名单哪天被放宽也不至于直接漏）。
#[test]
fn backend_path_with_space_is_quoted() {
    let c = build_fork_cmd("/opt/my backends/p1q", "s", "u");
    assert!(
        c.starts_with("'/opt/my backends/p1q' --fork-session"),
        "{c}"
    );
}

// ── 〔RW1 · 第四波 09-24〕本机那一支：exec 本机后端 `--fork-session`，结果与远端同一份解释 ──

#[test]
fn the_local_fork_outcome_folds_into_the_same_shape_as_the_remote_one() {
    use crate::backend::observe::local_query::QueryOutcome;
    // 成功：stdout 一行结果 JSON ⇒ 与远端同一份解析。
    let ok = local_fork_exec(QueryOutcome::Ok(
        "{\"sessionId\":\"new\",\"jsonlPath\":\"/p/new.jsonl\"}\n".to_string(),
    ))
    .expect("三态折叠");
    let r = interpret_fork_exec(&ok).expect("成功那一形");
    assert_eq!(r.session_id, "new");
    // 失败：退出码 ＋ stderr 信封 ⇒ 带原因的人话，不许被读成成功。
    let bad = local_fork_exec(QueryOutcome::Failed {
        code: Some(2),
        stderr: "{\"code\":\"fork_failed\",\"message\":\"refuse branch: session ccc not found\"}\n"
            .to_string(),
    })
    .expect("三态折叠");
    let e = interpret_fork_exec(&bad).expect_err("失败那一形");
    assert!(e.contains("not found"), "{e}");
    // 没有退出码（被信号杀了）⇒ 不许当 0。
    let killed = local_fork_exec(QueryOutcome::Failed {
        code: None,
        stderr: String::new(),
    })
    .expect("三态折叠");
    assert!(interpret_fork_exec(&killed).is_err());
    // 本机后端不在 ⇒ 明确说，不回落到本进程写（D11）。
    let e = local_fork_exec(QueryOutcome::NoBackend("旁边没有".into())).expect_err("不在");
    assert!(
        e.contains("本机后端不在") && e.contains("一个字节都没动"),
        "{e}"
    );
}
