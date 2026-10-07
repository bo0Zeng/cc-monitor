//! 后端代管的用户资产那一域：别名 · MCP · skill 的计算与判定住这里，写经这台后端自己的文件管理面（[`door`]）。

// `pub`：后端起来时那一次别名清单迁移由 `main.rs` 调（`aliases::migrate_here`）。
pub mod aliases;
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

/// 资产面那一家（读 / 装 skill · 用户级 MCP 文件）：注册表里**唯一**声明了资产面的那一家（今天只有 Claude）；两家都声明 ⇒ `None`、照实拒。
/// ⚠ Codex 的 skill 根来了（扩展位置带上程序这一维），改这里：由请求说是哪一家。
pub(crate) fn asset_kind() -> Option<&'static str> {
    crate::agents::sole_kind(|a| a.assets.is_some())
}

/// 资产面那一家（[`asset_kind`]）的资产面。
fn asset_face() -> Option<crate::agents::AssetFace> {
    crate::agents::asset_face(asset_kind()?)
}

/// 资产面那一家项目级 MCP 配置的文件名（`AssetFace.project_mcp_file`）。没有资产面那一家 ⇒ 空串（落点拼不出、照实拒）。
pub(crate) fn project_mcp_file() -> &'static str {
    asset_face().map_or("", |f| f.project_mcp_file)
}

/// 资产面那一家 MCP 配置里装 server 表的顶层键（`AssetFace.servers_key`）。没有资产面那一家 ⇒ 空串。
pub(crate) fn mcp_servers_key() -> &'static str {
    asset_face().map_or("", |f| f.servers_key)
}

pub mod asset_catalog; // 资产目录：帧面 `assets-catalog` / `assets-catalog-merge`（后端自有状态 `~/.cc-monitor/assets-catalog.json`，第四层；一个用户文件都不写）
pub mod asset_sync; // 资产目录的自动同步：帧面 `assets-sync`（本机常驻后端沿池里那条 SSH 拉 / 并 / 推；写口由 inbound 递进来）
pub mod mcp_sync; // MCP 资产同步的判定：帧面 `mcp-sync-plan`（差异 · 可疑项 · 写哪几条；只读，写经文件管理那一面）
pub mod skill_install; // skill「装到这台」：帧面 `skill-read`（来源那台）/ `skill-install-plan`（要被写的那一台；复用 AS1 的差异与闸）。只读
pub mod skill_ledger; // skill 装记录（后端自有状态 `~/.cc-monitor/skill-installs.json`，第四层）：帧面 `skill-install-record`（装完记 / 卸掉摘）。一个用户文件都不写
