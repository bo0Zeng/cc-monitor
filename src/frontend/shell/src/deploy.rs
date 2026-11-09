//! SS-D：远端**自部署**的 monitor 那一半（issue #29 自动部署 F08 · 手动安装 / 卸载 · `ccm` 入口）：把这一版带着的那几格字节落成槽。
//!
//! 装那一趟整趟在本机常驻后端（帧命令 `deploy-install`，编排住后端 `deploy` crate，手机同一份）：问那台要哪一格 · 落点那一份是谁 ·
//! 只升不降 · 取字节自检 · 放进那台的暂存区 ＋ 读回 · 那一份自己换进落点（`--place-self`）· 收拾残件。
//! 卸载同样整趟在后端（帧命令 `deploy-uninstall`：传一份暂存件、由它删落点那一份）。界面装 · 卸直接问 `<local>` 那两条（`backend-deploy.ts`），只交那台的名字。
//! 本模块只做一件事：这一版带着的那几格字节（全仓只有壳一个取字节口）落成本机后端落点旁边的槽，本机后端装 · 卸时照槽目录现读。
//!
//! ## 只读铁律豁免（INVARIANT §1 / 账本 SS-G）—— 穷举登记见 `src/doc/INVARIANTS.md §1`
//! cc-monitor 对远端的写入均**用户显式触发**，各自独立路径守卫、绝不混用：
//! - **F08**：自部署后端二进制到 `~/.cc-monitor/bin/`（非用户数据、幂等、版本门控）。今天整趟在本机常驻后端。
//! - **F89a**：用户**显式**增/改/删远端**项目** `.mcp.json`：那台后端自己算、自己写（帧命令 `mcp-sync-apply` / `ext-uninstall-apply`）。
//! - **F50**：追加公钥到远端 `~/.ssh/authorized_keys`：本机常驻后端的帧命令 `pubkey-push`（不在本模块，登记于此备查）。

use crate::detail::Shell as _;
use copy_core::copy_text;
use copy_core::said::Said;

/// 这一版带着的那几格落成本机文件（`~/.cc-monitor/bin/<SLOTS_DIR>/<Key::slot_name>`，与本机后端同一套落盘：盘上那份逐字节相等就不动）。
/// 本机后端照同一对名字认哪一格（`deploy_plan::carried_slots_in`），身份照字节自报。
fn place_carried() -> Result<(), Said> {
    let slots = slots_bin().ok_or_else(|| Said::of("rsFilewinProc.bin.noHome", &[]))?;
    for (key, _id, bytes) in crate::byte_table::carried_backends() {
        crate::local_backend::place_local_program(&slots, &key.slot_name(), bytes).map_err(
            |e| {
                Said::with_raw(
                    copy_text(
                        "deploy.bytes.unreadable",
                        &[("why", &copy_core::io_reason(e.kind()))],
                    ),
                    &e,
                )
            },
        )?;
    }
    Ok(())
}

/// 起来时落一次这一版带着的那几格（本机后端装 · 卸时照这个目录现读）。没成只记一行：装那一步会照实说读不到。
pub(crate) fn place_slots() {
    if let Err(said) = place_carried() {
        tracing::warn!(
            "这一版带着的后端字节没落成（远端装不了）：{said}: {}",
            said.raw()
        );
    }
}

/// 那几格落在哪：本机后端落点旁边（`~/.cc-monitor/bin/<SLOTS_DIR>`）的句柄。家目录问不到 ⇒ `None`。
fn slots_bin() -> Option<crate::platform::own_fs::OwnBin> {
    crate::platform::own_fs::OwnBin::issue()?
        .sub(deploy_contract::SLOTS_DIR)
        .ok()
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/deploy_tests.rs"]
mod tests;
