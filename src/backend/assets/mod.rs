//! **后端代管的用户资产**那一域：别名 · MCP · skill 的计算与判定住这里，
//! 写经这台后端自己的文件管理面（[`door`]）。从前「monitor 算好、后端写」的 D 组按用户 09-27「一处后端」收进来。

pub(crate) mod aliases;
pub(crate) mod cc_bus_install;
pub(crate) mod door;
// 设置「扩展」页的后端：跨机器一张表 · 从一台卸（装那一件的枢纽住 `hub`）。
pub(crate) mod ext;
pub(crate) mod hub;
pub(crate) mod mcp_edit;
pub(crate) mod mcp_sync_flow;
// 公钥推进那台的 `authorized_keys`（`pubkey-push` 本机那一跳 · `authorized-keys-add` 被写那台）。
pub(crate) mod pubkey;
pub(crate) mod skill_flow;

// 从 crate 根归进来（纯搬家，资产那一行）。
pub mod asset_catalog; // 资产目录：帧面 `assets-catalog` / `assets-catalog-merge`（后端自有状态 `~/.cc-monitor/assets-catalog.json`，第四层；一个用户文件都不写）
pub mod asset_sync; // 资产目录的自动同步：帧面 `assets-sync`（本机常驻后端沿池里那条 SSH 拉 / 并 / 推；写口由 inbound 递进来）
pub mod mcp_sync; // MCP 资产同步的判定：帧面 `mcp-sync-plan`（差异 · 可疑项 · 写哪几条；只读，写经文件管理那一面）
pub mod skill_install; // skill「装到这台」：帧面 `skill-read`（来源那台）/ `skill-install-plan`（要被写的那一台；复用 AS1 的差异与闸）。只读
pub mod skill_ledger; // skill 装记录（后端自有状态 `~/.cc-monitor/skill-installs.json`，第四层）：帧面 `skill-install-record`（装完记 / 卸掉摘）。一个用户文件都不写
