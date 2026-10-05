//! 自动起算发那一趟：交给 ccm 的那一行（ccm 认得、左边是那一家的不交互一句）· 结果怎么记。

use super::*;

#[test]
fn the_one_line_is_print_mode_cheapest_model_no_record() {
    let agent = crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT;
    let v = ccm_argv(
        agent,
        Path::new("/h/.cc-monitor/accounts/q"),
        Path::new("/h/.cc-monitor/autostart"),
    )
    .expect("这一家有这一形");
    let end = v.iter().rposition(|a| a == "--").expect("--");
    let (claude, ccm) = v.split_at(end);
    assert_eq!(claude[0], "-p");
    let has = |w: &str| claude.iter().any(|a| a == w);
    assert!(has("--no-session-persistence") && has("--strict-mcp-config"));
    let after = |w: &str| {
        claude
            .iter()
            .position(|a| a == w)
            .map(|i| claude[i + 1].as_str())
    };
    assert_eq!(after("--model"), Some("haiku"));
    assert_eq!(after("--tools"), Some(""));
    assert_eq!(after("--setting-sources"), Some("project"));
    // 右边那一段 ccm 认得出：用这个号的配置目录、在专门的目录里。
    let crate::control::ccm::argv::Parsed::Opts(o) =
        crate::control::ccm::argv::parse(&v).expect("ccm 认得")
    else {
        panic!("不该是诊断口");
    };
    assert_eq!(o.passthru, claude);
    assert_eq!(o.account_dir, "/h/.cc-monitor/accounts/q");
    assert_eq!(
        o.cwd_spec,
        crate::control::ccm::argv::CwdSpec::Explicit("/h/.cc-monitor/autostart".into())
    );
    assert_eq!(ccm.len(), 5);
    assert_eq!(
        ccm_argv("no-such-agent", Path::new("/a"), Path::new("/b")),
        None
    );
}

#[test]
fn verdicts() {
    let gone = || {
        Err(ChildFail::NotFound(std::io::Error::from(
            std::io::ErrorKind::NotFound,
        )))
    };
    assert_eq!(verdict(&Ok(1), true, true), Ok(()));
    assert_eq!(
        verdict(&Ok(CCM_COULD_NOT_EXEC), false, false),
        Err(AutostartFail::NoClaude)
    );
    assert_eq!(verdict(&gone(), false, false), Err(AutostartFail::NoClaude));
    assert_eq!(verdict(&Ok(1), false, true), Err(AutostartFail::NeedsLogin));
    assert_eq!(verdict(&Ok(0), false, false), Err(AutostartFail::NoReading));
    let late = Err(ChildFail::TimedOut {
        program: "ccm".into(),
        after: SEND_WITHIN,
    });
    assert_eq!(verdict(&late, false, false), Err(AutostartFail::TimedOut));
}
