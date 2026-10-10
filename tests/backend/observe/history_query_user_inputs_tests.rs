//! `--list-user-inputs` 的 argv 与文件那一层（纯核的判据在 `user_inputs_tests.rs`）。

use super::*;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn argv_accepts_the_option_on_either_side_and_rejects_anything_else() {
    let parse = |v: &[&str]| {
        let a = s(v);
        parse_user_inputs_args(&a).map(|(f, p)| (f, p.clone()))
    };
    assert_eq!(parse(&["/p.jsonl"]).unwrap(), (0, "/p.jsonl".to_string()));
    // monitor 的写法：选项在前
    assert_eq!(
        parse(&["--from", "42", "/p.jsonl"]).unwrap(),
        (42, "/p.jsonl".to_string())
    );
    assert_eq!(
        parse(&["/p.jsonl", "--from", "7"]).unwrap(),
        (7, "/p.jsonl".to_string())
    );
    // 写错的一律报错，不静默
    assert!(parse(&[]).is_err(), "缺路径");
    assert!(parse(&["/a.jsonl", "/b.jsonl"]).is_err(), "多余的位置参数");
    assert!(parse(&["--from"]).is_err(), "--from 缺值");
    assert!(
        parse(&["--from", "x", "/p.jsonl"]).is_err(),
        "--from 不是数"
    );
    assert!(parse(&["--index", "/p.jsonl"]).is_err(), "别的子命令的选项");
}

/// `--from` 超过文件长度 ⇒ 报错（文件被截断/重写），不回一份空清单假装「没有新的」。
/// 对照：恰好等于文件长度（「读到头了、没有新的」）是合法的。路径守卫照 `--read-session`。
#[test]
fn from_past_eof_is_an_error_not_an_empty_list() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-ui-{}", std::process::id()));
    let dir = tmp.join("projects").join("proj-ui");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    std::fs::write(&p, "{\"type\":\"system\"}\n").unwrap();
    let len = std::fs::metadata(&p).unwrap().len();
    let ps = p.to_string_lossy().into_owned();
    let err = list_user_inputs(&tmp, &ps, len + 1).expect_err("越过 EOF 必须报错");
    assert!(err.contains("past EOF"), "{err}");
    assert!(list_user_inputs(&tmp, &ps, len).is_ok());
    let outside = tmp.join("secret.jsonl");
    std::fs::write(&outside, "x\n").unwrap();
    assert!(list_user_inputs(&tmp, &outside.to_string_lossy(), 0).is_err());
    std::fs::remove_dir_all(&tmp).ok();
}

/// 分派：`run` 认得这条子命令（不是落进 `unknown argument`）。
#[test]
fn run_dispatches_the_subcommand() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-ui-run-{}", std::process::id()));
    let dir = tmp.join("projects").join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    std::fs::write(&p, "").unwrap();
    let ps = p.to_string_lossy().into_owned();
    assert_eq!(
        run(
            &tmp,
            &s(&["--list-user-inputs", "--from", "0", &ps]),
            &Default::default()
        ),
        0
    );
    // 对照：同一份参数换个错名 ⇒ 2
    assert_eq!(
        run(
            &tmp,
            &s(&["--list-user-inputz", "--from", "0", &ps]),
            &Default::default()
        ),
        2
    );
    std::fs::remove_dir_all(&tmp).ok();
}

// ── `--find-in-session` 的 argv 与文件那一层（内核的判据在 `search_query_find_tests.rs`）──

#[test]
fn find_argv_takes_the_query_as_an_option_value_and_rejects_anything_else() {
    let parse = |v: &[&str]| {
        let a = s(v);
        parse_find_args(&a).map(|f| (f.path.clone(), f.query.clone(), f.include_tools, f.limit))
    };
    let d = crate::observe::search_query::FIND_DEFAULT_LIMIT;
    // monitor 的写法：选项在前
    assert_eq!(
        parse(&[
            "--include-tools",
            "--limit",
            "7",
            "--query",
            "abc",
            "/p.jsonl"
        ])
        .unwrap(),
        ("/p.jsonl".into(), "abc".into(), true, 7)
    );
    assert_eq!(
        parse(&["/p.jsonl", "--query", "abc"]).unwrap(),
        ("/p.jsonl".into(), "abc".into(), false, d)
    );
    // 🔴 查询串本身以 `--` 起头：它是 `--query` 的值，不是一个写错的选项
    assert_eq!(
        parse(&["--query", "--force", "/p.jsonl"]).unwrap().1,
        "--force"
    );
    // 上限封顶
    assert_eq!(
        parse(&["--limit", "999999", "--query", "a", "/p.jsonl"])
            .unwrap()
            .3,
        crate::observe::search_query::FIND_MAX_LIMIT
    );
    assert!(parse(&["/p.jsonl"]).is_err(), "缺 --query");
    assert!(parse(&["--query"]).is_err(), "--query 缺值");
    assert!(parse(&["--query", "a"]).is_err(), "缺路径");
    assert!(
        parse(&["--query", "a", "/a.jsonl", "/b.jsonl"]).is_err(),
        "多余的位置参数"
    );
    assert!(
        parse(&["--limit", "x", "--query", "a", "/p.jsonl"]).is_err(),
        "--limit 不是数"
    );
    assert!(
        parse(&["--scope", "user", "--query", "a", "/p.jsonl"]).is_err(),
        "别的子命令的选项"
    );
}

/// 分派 ＋ 路径守卫：`run` 认得这条子命令；`projects/` 之外的文件拒。
#[test]
fn run_dispatches_find_and_keeps_the_path_fence() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-find-run-{}", std::process::id()));
    let dir = tmp.join("projects").join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    std::fs::write(
        &p,
        "{\"type\":\"user\",\"uuid\":\"a\",\"message\":{\"content\":\"x\"}}\n",
    )
    .unwrap();
    let ps = p.to_string_lossy().into_owned();
    assert_eq!(
        run(
            &tmp,
            &s(&["--find-in-session", "--query", "x", &ps]),
            &Default::default()
        ),
        0
    );
    // 对照：同一份参数换个错名 ⇒ 2
    assert_eq!(
        run(
            &tmp,
            &s(&["--find-in-sessionz", "--query", "x", &ps]),
            &Default::default()
        ),
        2
    );
    let outside = tmp.join("secret.jsonl");
    std::fs::write(&outside, "x\n").unwrap();
    let os = outside.to_string_lossy().into_owned();
    assert_eq!(
        run(
            &tmp,
            &s(&["--find-in-session", "--query", "x", &os]),
            &Default::default()
        ),
        2
    );
    std::fs::remove_dir_all(&tmp).ok();
}
