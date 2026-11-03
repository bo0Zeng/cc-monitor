//! 后端代管的用户资产那一域：别名 · MCP · skill 的计算与判定住这里，写经这台后端自己的文件管理面（[`crate::common::door`]）。
//!
//! 独立的包 `backend-assets`：只引 `accounts` · `agents` · `host`（含 `ccm_line`）· `dial` · `history` · `wire` · `common` · `platform`，
//! 引了上层就编不过。上层只够得到这里的 `pub` 项。

// 测试段可以直接问系统钟（量具、夹具的时刻）；生产段只许纯工具那一层的 `time::now`（`clippy.toml`）。
// 测试段收尾时丢结果不写理由；生产段丢一个 `Result` 写 `.discard(Why::…)`。
#![cfg_attr(
    test,
    allow(
        clippy::allow_attributes,
        clippy::allow_attributes_without_reason,
        clippy::disallowed_methods,
        clippy::let_underscore_must_use,
        clippy::unused_result_ok,
        reason = "测试段放开禁用名单与吞结果那两条：夹具自己定期限、读写盘、起进程、收尾时丢结果；用不用得到随判据增减，写成 `expect` 会在用不到时报 ⇒ 整包唯一许写 `allow` 的一行"
    )
)]

// 包里照旧写 `crate::assets::…` · `crate::accounts::…` · `crate::agents::…` · `crate::ccm_line::…` · `crate::dial::…` ·
// `crate::history::…` · `crate::wire::…` · `crate::common::…` · `crate::platform::…`。
extern crate self as assets;
use backend_accounts as accounts;
use backend_agents as agents;
use backend_common as common;
use backend_dial as dial;
use backend_history as history;
use backend_host::ccm_line;
use backend_platform as platform;
use backend_wire as wire;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本包的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

// 别名 · 配置文件 · 别名块（后端起来时那一次照配置文件对齐别名文件与链接 `aliases::align_here` 由 `stream/inbound/doors.rs` 调）。
pub mod aliases;
pub mod cc_bus_install;
// 设置「扩展」页的后端：跨机器一张表 · 从一台卸（装那一件的枢纽住 `hub`）。
pub mod ext;
pub mod hub;
// 枢纽问各台的那 8 条内层命令（typed）：每条 `impl Command`（线上名只写这一处），本体在本包各处。
pub mod hub_inner;
// 枢纽问各台的那 8 条内层命令的线上形（两台后端之间）。
pub mod hub_wire;
pub mod mcp_edit;
pub(crate) mod mcp_sync_flow;
// 公钥推进那台的 `authorized_keys`（`pubkey-push` 本机那一跳 · `authorized-keys-add` 被写那台）。
pub mod pubkey;
pub(crate) mod skill_flow;

/// 资产面那一家（[`crate::agents::asset_kind`]）的资产面。
fn asset_face() -> Option<crate::agents::AssetFace> {
    crate::agents::asset_face(crate::agents::asset_kind()?)
}

/// 资产面那一家项目级 MCP 配置的文件名（`AssetFace.project_mcp_file`）。没有资产面那一家 ⇒ 空串（落点拼不出、照实拒）。
pub(crate) fn project_mcp_file() -> &'static str {
    asset_face().map_or("", |f| f.project_mcp_file)
}

/// 资产面那一家 MCP 配置里装 server 表的顶层键（`AssetFace.servers_key`）。没有资产面那一家 ⇒ 空串。
pub(crate) fn mcp_servers_key() -> &'static str {
    asset_face().map_or("", |f| f.servers_key)
}

pub mod asset_catalog; // 资产目录：帧面 `assets-catalog` / `assets-catalog-merge`（后端自有状态 `~/.cc-monitor/assets-catalog.json`，经 `Own`；一个用户文件都不写）
pub mod asset_sync; // 资产目录的自动同步：帧面 `assets-sync`（本机常驻后端沿池里那条 SSH 拉 / 并 / 推；写口由 inbound 递进来）
pub(crate) mod mcp_sync; // MCP 资产同步的判定：（差异 · 可疑项 · 写哪几条；只读，写经文件管理那一面）
pub(crate) mod skill_install; // skill「装到这台」：帧面 `skill-read`（来源那台）/ `skill-install-plan`（要被写的那一台；复用 AS1 的差异与闸）。只读
pub mod skill_ledger; // skill 装记录（后端自有状态 `~/.cc-monitor/skill-installs.json`，经 `Own`）：装完记 / 卸掉摘只在本进程里经写句柄记（`skill_flow` · `ext` · `mcp_sync_flow` · `cc_bus_install`）。一个用户文件都不写

// 资产目录那一族四条的线上字节（换形前钉、换形后比）。
#[cfg(test)]
#[path = "../../../tests/backend/assets/catalog_wire_tests.rs"]
mod catalog_wire_tests;

// agent 适配层判据里经资产目录的那几条（适配层是独立的包、够不到这一层）。
#[cfg(test)]
#[path = "../../../tests/backend/agents/on_assets_tests.rs"]
mod agents_tests;

// 纯工具那一层够用户文件的那扇门对文件管理面的每一问（经判据用的真盘门 · 别名的链接那几样；纯工具那个包够不到这一层）。
#[cfg(test)]
#[path = "../../../tests/backend/common/door_wire_tests.rs"]
mod common_door_wire_tests;

// 账号库判据里经判据用的真盘门（`aliases::test_support::HomeDoor`）真建库的那几条（账号域那个包够不到这一层）。
#[cfg(test)]
#[path = "../../../tests/backend/accounts/manage/library_any_os_tests.rs"]
mod accounts_library_any_os_tests;
