//! 项目 `.mcp.json` 原文怎么变成新原文只有 `plan_project_mcp` 一处：已存在但读不懂 ⇒ 拒绝覆盖；落点必须是这台上的绝对项目目录。
use super::*;

/// 已存在但读不懂 ⇒ 拒绝覆盖（不从骨架算起）；不存在 ⇒ 从骨架算起；没事可做 ⇒ `None`。
#[test]
fn a_broken_mcp_json_is_never_replaced_with_a_skeleton() {
    let mut add = |v: &mut Value| upsert_mcp_server_value(v, "x".into(), json!({})).map(|()| true);
    assert!(
        plan_project_mcp("p", Some("{ not json"), &mut add).is_err(),
        "读不懂的也算出了新原文"
    );
    let fresh = plan_project_mcp("p", None, &mut add)
        .unwrap()
        .expect("不存在时没从骨架算起");
    assert_eq!(
        serde_json::from_str::<Value>(&fresh).unwrap()[super::super::mcp_servers_key()]["x"],
        json!({})
    );
    let mut nothing = |_: &mut Value| Ok(false);
    assert_eq!(
        plan_project_mcp("p", Some("{}"), &mut nothing).unwrap(),
        None
    );
}

/// 落点是相对目录 / 带 `..` ⇒ `bad_path`。
#[test]
fn relative_or_dotdot_project_dirs_are_refused() {
    for bad in ["rel/dir", "/a/../b"] {
        assert_eq!(project_root(bad).expect_err(bad).0, "bad_path", "{bad}");
    }
}
