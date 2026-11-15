//! 项目级 MCP 原文怎么变成新原文只照那一家声明的格式（`McpCells`）：增一条 · 改一条 · 删一条，别的行与写法一字不变；
//! 已存在但那一家读不懂 ⇒ 拒绝覆盖；落点必须是这台上的绝对项目目录。判据拿替身家的行式配置（不是 JSON）。
use super::*;
use serde_json::json;

/// 替身家的格式：一个服务器一行 `server <名> <一行 JSON>`，别的行原样。
const LINES: McpCells = crate::agents::fake::MCP_CELLS;

/// 一份带注释、别的设置、两个服务器的行式配置。
const BEFORE: &str =
    "# 我的设置\nserver a {\"command\":\"a\"}\nlevel 3\nserver b {\"command\":\"b\"}\n";

/// ★ 增一条 ⇒ 末尾多一行；改一条 ⇒ 只换那一行；删一条 ⇒ 只少那一行。别的行一字不变。
#[test]
fn a_project_mcp_edit_changes_only_that_servers_line_in_the_familys_format() {
    assert_eq!(
        upsert_mcp_server_value(&LINES, "p", Some(BEFORE), "c", &json!({ "command": "c" }))
            .expect("增不上"),
        "# 我的设置\nserver a {\"command\":\"a\"}\nlevel 3\nserver b {\"command\":\"b\"}\nserver c {\"command\":\"c\"}\n",
        "增一条动了别的行"
    );
    assert_eq!(
        upsert_mcp_server_value(&LINES, "p", Some(BEFORE), "a", &json!({ "command": "z" }))
            .expect("改不上"),
        "# 我的设置\nserver a {\"command\":\"z\"}\nlevel 3\nserver b {\"command\":\"b\"}\n",
        "改一条动了别的行"
    );
    assert_eq!(
        remove_mcp_server(&LINES, "p", BEFORE, "b").expect("删不掉"),
        "# 我的设置\nserver a {\"command\":\"a\"}\nlevel 3\n",
        "删一条动了别的行"
    );
}

/// 不存在 ⇒ 从空文件算起（只有那一行）；已存在但读不懂 ⇒ 增 · 删都拒（不覆盖）；名空 ⇒ 拒。
#[test]
fn a_broken_project_mcp_file_is_never_replaced() {
    assert_eq!(
        upsert_mcp_server_value(&LINES, "p", None, "x", &json!({}))
            .expect("不存在时没从空文件算起"),
        "server x {}\n"
    );
    let broken = "server a {oops\nlevel 3\n";
    assert!(
        upsert_mcp_server_value(&LINES, "p", Some(broken), "x", &json!({})).is_err(),
        "读不懂的也算出了新原文（增）"
    );
    assert!(
        remove_mcp_server(&LINES, "p", broken, "x").is_err(),
        "读不懂的也算出了新原文（删）"
    );
    assert!(
        upsert_mcp_server_value(&LINES, "p", Some(BEFORE), " ", &json!({})).is_err(),
        "名空也写了"
    );
}

/// 落点是相对目录 / 带 `..` ⇒ `bad_path`。
#[test]
fn relative_or_dotdot_project_dirs_are_refused() {
    for bad in ["rel/dir", "/a/../b"] {
        assert_eq!(
            project_root(bad).expect_err(bad).code.as_deref(),
            Some("bad_path"),
            "{bad}"
        );
    }
}
