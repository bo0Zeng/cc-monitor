//! 换号时请求体里那一格账号身份：读哪个号的身份 · 只换那几个字节。

use super::{identity_file, identity_in, rewrite_identity};
use crate::agents::IdentityCell;

const A: &str = "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa";
const B: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";

fn body_with(uuid: &str) -> String {
    let user_id = format!(
        "{{\"device_id\":\"{}\",\"account_uuid\":\"{uuid}\",\"session_id\":\"s-1\"}}",
        "d".repeat(64)
    );
    let v = serde_json::json!({
        "model": "m",
        "messages": [{"role": "user", "content": "hi \u{4f60}\u{597d}"}],
        "metadata": {"user_id": user_id},
        "stream": true,
    });
    serde_json::to_string(&v).expect("json")
}

/// ★ 换成 B 的身份：整份请求体只差那一格（换回 A 逐字节等于原样），读回来那一格是 B。
#[test]
fn the_identity_cell_is_the_only_thing_that_changes() {
    let a = body_with(A);
    let IdentityCell::Rewritten(b) = rewrite_identity(a.as_bytes(), B) else {
        panic!("应换成");
    };
    assert_eq!(b, body_with(B).into_bytes());
    assert_eq!(b.len(), a.len());
    let IdentityCell::Rewritten(back) = rewrite_identity(&b, A) else {
        panic!("应换回");
    };
    assert_eq!(back, a.into_bytes());
}

/// 按量号给空串：那一格换成空串。
#[test]
fn an_empty_identity_is_written_as_an_empty_string() {
    let IdentityCell::Rewritten(b) = rewrite_identity(body_with(A).as_bytes(), "") else {
        panic!("应换成");
    };
    assert_eq!(b, body_with("").into_bytes());
}

/// 没有这一格 ⇒ 原样发；那几个字节不止一处（正文里也贴了一份）⇒ 认不准，不拿它换号；给的身份不像 id ⇒ 认不准。
#[test]
fn absent_ambiguous_and_odd_identities_are_told_apart() {
    assert_eq!(
        rewrite_identity(b"{\"model\":\"m\",\"metadata\":{}}", B),
        IdentityCell::Absent
    );
    assert_eq!(rewrite_identity(b"not json", B), IdentityCell::Absent);
    let twice = body_with(A).replace("hi ", &format!("\\\"account_uuid\\\":\\\"{A}\\\" "));
    assert_eq!(rewrite_identity(twice.as_bytes(), B), IdentityCell::Unsure);
    assert_eq!(
        rewrite_identity(body_with(A).as_bytes(), "x\"y"),
        IdentityCell::Unsure
    );
}

/// 身份读自那个号的 `.claude.json`：具名号在配置目录里，账号 0 在家目录下。
#[test]
fn the_identity_comes_from_that_accounts_own_config() {
    let root = std::env::temp_dir().join(format!("ccm-ident-{}", std::process::id()));
    let (home, dir) = (root.join("home"), root.join("acct"));
    std::fs::create_dir_all(&home).expect("mkdir");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let write = |d: &std::path::Path, u: &str| {
        std::fs::write(
            d.join(".claude.json"),
            format!("{{\"oauthAccount\":{{\"accountUuid\":\"{u}\",\"emailAddress\":\"x@y\"}}}}"),
        )
        .expect("write");
    };
    write(&dir, B);
    write(&home, A);
    let of =
        |d: &std::path::Path, base: Option<&std::path::Path>| identity_in(&identity_file(d, base));
    assert_eq!(of(&dir, None).as_deref(), Some(B));
    assert_eq!(of(&dir, Some(&home)).as_deref(), Some(A));
    assert_eq!(of(&root, None), None);
    let _ = std::fs::remove_dir_all(&root);
}
