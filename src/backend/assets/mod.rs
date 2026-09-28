//! 〔MIG-3a · `设计/99 §2.1 ⑬`〕**后端代管的用户资产**那一域（`01 §3.4`）：别名 · MCP · skill 的计算与判定住这里，
//! 写经这台后端自己的文件管理面（[`door`]）。从前「monitor 算好、后端写」的 D 组（`05 §14.3`）按用户 09-27「一处后端」收进来。

pub(crate) mod door;
pub(crate) mod mcp_edit;
pub(crate) mod mcp_sync_flow;
pub(crate) mod skill_flow;
pub(crate) mod skill_inbox;
