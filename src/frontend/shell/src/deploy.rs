//! SS-D：远端**自部署**的 monitor 那一半（issue #29 自动部署 F08 · 手动安装 / 卸载 · `ccm` 入口）。
//!
//! 装那一趟整趟在本机常驻后端（帧命令 `deploy-install`，编排住后端 `deploy` crate，手机同一份）：问那台要哪一格 · 落点那一份是谁 ·
//! 只升不降 · 取字节自检 · 放进那台的暂存区 ＋ 读回 · 那一份自己换进落点（`--place-self`）· 收拾残件。
//! 卸载同样整趟在后端（帧命令 `deploy-uninstall`：传一份暂存件、由它删落点那一份）。
//! 本模块只交事实（怎么够到那台 · 这一版带着的那几格字节落在本机哪个文件）、把后端推来的每一步交机器状态（`deploy` 帧）、把结局交出去。
//!
//! ## 只读铁律豁免（INVARIANT §1 / 账本 SS-G）—— 穷举登记见 `src/doc/INVARIANTS.md §1`
//! cc-monitor 对远端的写入均**用户显式触发**，各自独立路径守卫、绝不混用：
//! - **F08**：自部署后端二进制到 `~/.cc-monitor/bin/`（非用户数据、幂等、版本门控）。今天整趟在本机常驻后端。
//! - **F89a**：用户**显式**增/改/删远端**项目** `.mcp.json`：那台后端自己算、自己写（帧命令 `mcp-sync-apply` / `ext-uninstall-apply`）。
//! - **F50**：追加公钥到远端 `~/.ssh/authorized_keys`：本机常驻后端的帧命令 `pubkey-push`（不在本模块，登记于此备查）。

use crate::detail::Shell as _;
use cmd_wire::failed::Failed;
use copy_core::said::Said;
use cmd_wire::deploy::{ActionTag, DeployInstallReq, DeployUninstallReq, Installed, SlotRow};
use copy_core::copy_text;

/// 等那一趟的上限：问几次那台 ＋ 推一份 MB 级的字节 ＋ 读回。
const INSTALL_BUDGET: std::time::Duration = std::time::Duration::from_secs(600);

/// 这一版带着的那几格落成本机文件（`~/.cc-monitor/bin/slots/<os>-<arch>`，与本机后端同一套落盘：盘上那份逐字节相等就不动；
/// 名字两侧同读：本机后端照它认哪一格，`deploy_plan::carried_slots`）⇒ `deploy-install` 的 `slots`。每个进程只落一次（同一版的字节不会变）。
fn slot_rows() -> Result<Vec<SlotRow>, Said> {
    static ROWS: std::sync::OnceLock<Vec<SlotRow>> = std::sync::OnceLock::new();
    if let Some(r) = ROWS.get() {
        return Ok(r.clone());
    }
    let dir = slots_dir().ok_or_else(|| Said::of("rsFilewinProc.bin.noHome", &[]))?;
    let mut rows = Vec::new();
    for (key, id, bytes) in crate::byte_table::carried_backends() {
        let name = format!("{}-{}", key.os.label(), key.arch.label()).to_lowercase();
        let path = crate::local_backend::place_local_program(
            &dir,
            &name,
            bytes,
            &crate::platform::fs::make_executable,
            &crate::platform::fs::ensure_private_dir,
        )
        .map_err(|e| {
            Said::with_raw(
                copy_text(
                    "deploy.bytes.unreadable",
                    &[("why", &copy_core::io_reason(e.kind()))],
                ),
                &e,
            )
        })?;
        rows.push(SlotRow {
            os: key.os.label().to_string(),
            arch: key.arch.label().to_string(),
            id: id.to_string(),
            path: path.display().to_string(),
        });
    }
    Ok(ROWS.get_or_init(|| rows).clone())
}

/// 起来时落一次这一版带着的那几格（本机后端那几条常驻流装后端时照这个目录现读、身份照字节自报，`deploy_plan::carried_slots`）。
/// 没成只记一行：装那一步会照实说读不到。
pub(crate) fn place_slots() {
    if let Err(said) = slot_rows() {
        tracing::warn!(
            "这一版带着的后端字节没落成（远端装不了）：{said}: {}",
            said.raw()
        );
    }
}

/// 那几格落在哪：本机后端落点旁边（`~/.cc-monitor/bin/slots`）。家目录问不到 ⇒ `None`。
fn slots_dir() -> Option<std::path::PathBuf> {
    let (rel, _) = relay_route_core::BACKEND_LANDING_REL.rsplit_once('/')?;
    creds_core::store::home_dir()
        .map(|h| rel.split('/').fold(h, |p, seg| p.join(seg)).join("slots"))
}

/// 请本机常驻后端给那台装一趟：入参只有事实；每走一步记进那台的状态成品（后端推 `changed {machine}`）。
/// 只由 [`deploy_remote_backend`] 调：这一版的字节没落成是壳自己的错，投出去时带那条命令的名字。
async fn install(machine: &str) -> Result<Installed, Failed> {
    let slots = slot_rows().map_err(|s| s.named("deploy_remote_backend"))?;
    install_for(machine, slots).await
}

/// 问本机常驻后端一条装 / 卸的命令：入参只有事实（带着哪几格 · 那台叫什么；怎么够到它照本机后端的机器表），回包照那条命令的类型。
/// 后端答了失败 ⇒ 它那一份原样；壳这一侧问不成 · 回包读不懂 ⇒ 壳自己的错，命令就是问的那一条。
async fn ask_backend<C: cmd_wire::Call>(req: &C) -> Result<C::Reply, Failed> {
    use crate::backend_route::{route_call_error, Routed};
    let cmd = C::NAME;
    let client = crate::inbound_client::local_backend_accepting(cmd)
        .await
        .map_err(|said| Said::from(said).named(cmd))?;
    let deadline = tokio::time::Instant::now() + INSTALL_BUDGET;
    // 那一趟在本机常驻后端里 ⇒ 它交来的那一份失败（那台答 `uname` 的原话也在详情里）原样往外交。
    let got = client.ask(None, req, None, deadline).await.map_err(|e| {
        let said = match route_call_error(&e, &copy_core::local_machine(), |_code, message| {
            message.to_string()
        }) {
            Routed::NoChannel(s) | Routed::Refused(s) => s,
        };
        crate::detail::of_call(said, cmd, &e)
    })?;
    // 回包对不上类型：那一句不接原话，解码器那一句进复制详情。
    got.map_err(|e| Said::with_raw(copy_text("rsSftp.plan.badProduct", &[]), e).named(cmd))
}

/// [`install`] 的本体，「带着哪几格」由调用方交（生产 = 这一版的槽落成的文件；回环台架交它送去的那一份）。
async fn install_for(machine: &str, slots: Vec<SlotRow>) -> Result<Installed, Failed> {
    ask_backend(&DeployInstallReq {
        slots,
        machine: machine.to_string(),
    })
    .await
}

// ============================================================================
// F08c：手动安装 / 卸载后端（设置面板两个按钮）。自动装那一趟在本机后端那几条常驻流里（接不上 · 不是这一版时，`control/feeds.rs`）；
// 手动装同一条命令，卸载也整趟在后端，回那一句。
// ============================================================================

/// 手动安装 / 更新远端后端（机器页 ①「部署后端」按钮）：与自动部署同一趟（[`install`]），回那一句。
/// 那台不比这一版旧 ⇒ 手动点也不降级：那一句后面接出路（先卸载再部署 = 明确授权覆盖）。
#[tauri::command]
pub async fn deploy_remote_backend(origin: String) -> Result<String, Failed> {
    remote_in_table(&origin).map_err(|s| s.named("deploy_remote_backend"))?;
    let got = install(&origin).await?;
    Ok(match got.action {
        ActionTag::Keep => copy_text("rsSftp.deploy.keptNotOlder", &[("why", &got.said)]),
        ActionTag::Skip | ActionTag::Deploy => got.said,
    })
}

/// 装 · 卸只对机器表里的一台远端：本机不经这两条命令（本机那一份由宿主起来时自己放），表里没有那台 ⇒ 说是哪台、不去问后端。
fn remote_in_table(origin: &str) -> Result<(), Said> {
    if origin == crate::inbound_client::LOCAL_ORIGIN {
        return Err(Said::of("reason.code.bug", &[]));
    }
    if !crate::machine_table::has(origin) {
        return Err(Said::of(
            "rsBackendControl.handle.missing",
            &[("origin", &origin.to_string())],
        ));
    }
    Ok(())
}

/// 卸载远端后端（设置面板「卸载后端」按钮）：请本机常驻后端删那台落点那一份（它就是 `ccm`，卸后端就是卸 `ccm`），回后端写的那一句。
/// 只读铁律豁免（SS-G）：用户显式触发的删。注意：若该机器仍启用，自动部署会在下次连接重新装回——提示见返回消息。
#[tauri::command]
pub async fn uninstall_remote_backend(origin: String) -> Result<String, Failed> {
    let own = |s: Said| s.named("uninstall_remote_backend");
    remote_in_table(&origin).map_err(own)?;
    let slots = slot_rows().map_err(own)?;
    uninstall_for(&origin, slots).await
}

/// [`uninstall_remote_backend`] 的本体，「带着哪几格」由调用方交（同 [`install_for`]）。
async fn uninstall_for(machine: &str, slots: Vec<SlotRow>) -> Result<String, Failed> {
    let said = ask_backend(&DeployUninstallReq {
        slots,
        machine: machine.to_string(),
    })
    .await?
    .said;
    tracing::info!("远端 [{machine}] 卸载后端：{said}");
    Ok(said)
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/deploy_tests.rs"]
mod tests;
