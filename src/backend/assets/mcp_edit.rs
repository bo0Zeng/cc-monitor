//! 项目级 MCP 那一份的增 / 改 / 删那几样纯函数：装 · 推 · 拉（`mcp_sync_flow`）与卸（`ext`）都在被写那台后端自己算、自己写。
//! 那份文件是哪一份 · 什么格式都取自同一家（[`project_mcp_of`]：资产面的 `project_mcp_file` ＋ 账号面的 `McpCells`）；
//! 写面只 `<项目目录>/<那一份>`（`INVARIANTS §1` 例外 5），别的配置一个字节不碰。
//! 「原文怎么变成新原文」只有 [`edit_checked`] 一处。

use crate::agents::McpCells;
use crate::common::said::Said;
use copy_core::copy_text;
use serde_json::{Map, Value};

/// 一台机器的绝对路径写法。路径只由它所属的那台判（枢纽不替别台判）；判据拿它在 Linux 上换成 Windows 那一形。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathForm {
    /// `/…`
    Posix,
    /// `X:\…` · `X:/…` · `\\…`
    Windows,
}

impl PathForm {
    /// 本进程这一台的写法。
    pub(crate) const HERE: PathForm = if cfg!(windows) {
        PathForm::Windows
    } else {
        PathForm::Posix
    };

    pub(crate) fn is_absolute(self, d: &str) -> bool {
        let b = d.as_bytes();
        match self {
            PathForm::Posix => d.starts_with('/'),
            PathForm::Windows => {
                d.starts_with("\\\\")
                    || (b.len() >= 3
                        && b[0].is_ascii_alphabetic()
                        && b[1] == b':'
                        && (b[2] == b'\\' || b[2] == b'/'))
            }
        }
    }
}

/// 项目目录必须是这台机器上的绝对路径（相对路径在后端这个进程里指的是别处）。
pub(crate) fn project_root(project_dir: &str) -> Result<String, Said> {
    project_root_as(project_dir, PathForm::HERE)
}

/// [`project_root`] 的本体，「这台怎么写绝对路径」是参数。
pub(crate) fn project_root_as(project_dir: &str, form: PathForm) -> Result<String, Said> {
    let d = project_dir.trim();
    if d.is_empty() {
        return Err(Said::new(
            "bad_args",
            copy_text("beMcpEdit.path.emptyDir", &[]),
        ));
    }
    if !form.is_absolute(d) || d.split(['/', '\\']).any(|s| s == "..") {
        return Err(Said::new(
            "bad_path",
            copy_text("beMcpEdit.path.notAbsolute", &[("dir", d)]),
        ));
    }
    Ok(d.trim_end_matches(['/', '\\']).to_string())
}

/// 那一家的项目级 MCP：（项目目录下那份文件的相对段, 它的配置格式 `McpCells`）—— 装 · 推 · 拉 · 卸都只从这里取，
/// 文件名与格式出自同一家。家认不出 / 没有资产面 · 说不出是哪一家 · 那一家没声明项目级 MCP 文件或配置格式 ⇒ `bad_args`
/// （不退回默认那一家、不拿别家的格式去改）。
pub fn project_mcp_of(
    reg: &[crate::agents::Adapter],
    agent: Option<&str>,
) -> Result<(&'static str, McpCells), Said> {
    let no_format = || {
        let who = agent.map_or_else(
            || "the default agent".to_string(),
            |a| format!("`agent` {a:?}"),
        );
        crate::wire::spec::fail(
            "bad_args",
            crate::common::contract::malformed(&format!(
                "{who} has no project MCP config on this machine"
            )),
        )
    };
    let kind = super::skill_install::family_among(reg, agent)?.ok_or_else(no_format)?;
    let file = crate::agents::asset_face_among(reg, kind)
        .map(|f| f.project_mcp_file)
        .filter(|f| !f.is_empty())
        .ok_or_else(no_format)?;
    let cells = crate::agents::accounts_face_among(reg, kind)
        .and_then(|f| f.mcp)
        .ok_or_else(no_format)?;
    Ok((file, cells))
}

/// 项目级那一份原文 ⇒ 各服务器（照那一家的格式读）：不存在 ⇒ 空表；读不懂 ⇒ `bad_file`（带上是哪份 `what`）。
pub(crate) fn project_servers(
    cells: &McpCells,
    what: &str,
    text: Option<&str>,
) -> Result<Map<String, Value>, Said> {
    let Some(t) = text else {
        return Ok(Map::new());
    };
    (cells.read)(t).map_err(|why| {
        crate::wire::spec::fail(
            "bad_file",
            copy_text("reason.fs.readFailed", &[("path", what), ("why", &why)]),
        )
    })
}

/// 装 · 推 · 拉：把一条 server 写进项目级那一份（照那一家的格式只换那一个服务器，别的原样），交新原文（纯）。
/// 不存在（`existing = None`）⇒ 从空文件算起；名空 ⇒ 拒；已存在但那一家读不懂 ⇒ 拒绝覆盖。
pub(crate) fn upsert_mcp_server_value(
    cells: &McpCells,
    what: &str,
    existing: Option<&str>,
    name: &str,
    server: &Value,
) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err(copy_text("beMcpEdit.upsert.emptyName", &[]));
    }
    edit_checked(cells, what, existing.unwrap_or(""), name, Some(server))
}

/// 卸：从项目级那一份删一条 server（照那一家的格式只删那一节，别的原样），交新原文（纯）。读不懂 ⇒ 拒绝覆盖。
pub(crate) fn remove_mcp_server(
    cells: &McpCells,
    what: &str,
    existing: &str,
    name: &str,
) -> Result<String, String> {
    edit_checked(cells, what, existing, name, None)
}

/// 🔴 项目级 MCP 的原文怎么变成新原文只有这一处：先照那一家的读法核一遍整份读得懂（读不懂 ⇒ 拒，不从空文件算起去盖），
/// 再交那一家只改那一节。
fn edit_checked(
    cells: &McpCells,
    what: &str,
    text: &str,
    name: &str,
    to: Option<&Value>,
) -> Result<String, String> {
    let parse_failed = |e: String| {
        crate::common::said::IntoNote::into_note(Said::with_raw(
            copy_text("beMcpEdit.plan.parseFailed", &[("what", what)]),
            e,
        ))
    };
    if !text.trim().is_empty() {
        (cells.read)(text).map_err(parse_failed)?;
    }
    (cells.edit)(text, name, to).map_err(parse_failed)
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/mcp_edit_tests.rs"]
mod tests;
