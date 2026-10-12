//! 命令那一层里纯的几样：入参按线上形状严格收（多键 · 缺键 · 认不出的命令都是 `bad_args`）· 别名事件按前后账号表之差 ·
//! 导入凭据的路径只收家目录底下 · 登录那一行的拼法。落盘那一半在 `faces/accounts_face_tests.rs` 真跑。夹具全是合成的。
use super::*;
use serde_json::json;

fn code(r: Result<Request, Refusal>) -> String {
    r.err().map(|f| f.code).unwrap_or_default()
}

/// 每条命令的名字 ⇒ 对应那一种请求；空入参（`null`）当成空对象收。
#[test]
fn each_command_name_parses_to_its_request() {
    let named = json!({"name": "z"});
    let name_args = AccountNameArgs {
        name: "z".into(),
        dry_run: None,
    };
    assert!(
        matches!(parse("accounts-init", &named), Ok(Request::Init(a)) if a.name == "z" && a.dry_run.is_none())
    );
    assert!(
        matches!(parse("accounts-remove", &json!({"name": "z", "force": true})), Ok(Request::Remove(a)) if a.force == Some(true))
    );
    assert_eq!(
        parse("accounts-set-default", &named).unwrap(),
        Request::SetDefault(name_args.clone())
    );
    assert_eq!(
        parse("accounts-login-cmd", &named).unwrap(),
        Request::LoginCmd(name_args)
    );
    assert_eq!(
        parse("accounts-verify", &Value::Null).unwrap(),
        Request::Verify
    );
    assert_eq!(
        parse("accounts-verify", &json!({})).unwrap(),
        Request::Verify
    );
}

/// 形状不对一律 `bad_args`：多键 · 缺键 · 类型不对 · verify 带了参数 · 认不出的命令。
#[test]
fn a_wrong_shape_is_bad_args() {
    assert_eq!(
        code(parse("accounts-init", &json!({"name": "z", "extra": 1}))),
        "bad_args"
    );
    assert_eq!(
        code(parse("accounts-init", &Value::Null)),
        "bad_args",
        "缺 name"
    );
    assert_eq!(
        code(parse("accounts-remove", &json!({"name": 3}))),
        "bad_args"
    );
    assert_eq!(code(parse("accounts-verify", &json!({"x": 1}))), "bad_args");
    assert_eq!(code(parse("accounts-rename", &json!({}))), "bad_args");
}

/// MCP 那几条：是这一族 ⇒ `Some`（read 不许带参数）；别的命令 ⇒ `None`（交给别处，不是拒）。
#[test]
fn mcp_commands_are_recognised_and_others_are_passed_on() {
    assert_eq!(
        parse_mcp("accounts-mcp-read", &Value::Null)
            .unwrap()
            .unwrap(),
        McpRequest::Read
    );
    assert_eq!(
        parse_mcp("accounts-mcp-read", &json!({"x": 1}))
            .unwrap()
            .unwrap_err()
            .code,
        "bad_args"
    );
    assert!(matches!(
        parse_mcp("accounts-mcp-sync", &json!({"on": true})),
        Some(Ok(McpRequest::Sync(_)))
    ));
    assert!(matches!(
        parse_mcp("accounts-mcp-remove", &json!({})),
        Some(Err(_))
    ));
    assert!(parse_mcp("accounts-init", &json!({"name": "z"})).is_none());
}

/// 别名事件只跟着账号表之差：消失的按删号、新出现的按建号；没变 ⇒ 空。
#[test]
fn alias_events_follow_the_difference_of_the_two_tables() {
    let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(
        alias_events(&v(&["a", "b"]), &v(&["b", "c"])),
        [
            AliasEvent::Removed("a".into()),
            AliasEvent::Added("c".into())
        ]
    );
    assert!(alias_events(&v(&["a"]), &v(&["a"])).is_empty());
    assert_eq!(
        alias_events(&[], &v(&["x"])),
        [AliasEvent::Added("x".into())]
    );
}

/// 导入凭据的路径：`~/` 按家展开、尾部 `/` 去掉；家目录外 · 家目录本身 · 相对 · `..` 段 · 控制符一律 `refused`。
#[test]
fn a_credential_path_must_live_under_home() {
    assert_eq!(cred_path("/h", "~/c/cred.json").unwrap(), "/h/c/cred.json");
    assert_eq!(cred_path("/h", " /h/c/ ").unwrap(), "/h/c");
    // `~/` · `/h/` 是家目录本身（这一条补测试时逮到：尾部 `/` 去掉之前判「不是家」，放过去了）。
    for bad in [
        "/etc/cred",
        "/h",
        "/h/",
        "~/",
        "c/cred",
        "/h/../etc",
        "/h/a\nb",
        "/hx/cred",
    ] {
        assert_eq!(cred_path("/h", bad).unwrap_err().code, "refused", "{bad:?}");
    }
}

/// 登录那一行：`<家>/.cc-monitor/bin/ccm -- --account <名>`，名字过 quote（带空格 · 引号也不拆开）。
#[test]
fn the_login_line_quotes_the_name() {
    let q = shell_quote_core::posix_quote;
    assert_eq!(
        login_line("/h/", "z"),
        format!("{} -- --account {}", q("/h/.cc-monitor/bin/ccm"), q("z"))
    );
    let odd = login_line("/h", "a b'c");
    assert!(
        odd.ends_with(&format!(" --account {}", q("a b'c"))),
        "{odd}"
    );
    assert_ne!(shell_quote_core::posix_quote("a b'c"), "a b'c");
}
