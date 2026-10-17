//! SS-D：远端**自部署**的 monitor 那一半（issue #29 自动部署 F08 · 手动安装 / 卸载 · `ccm` 入口）。
//!
//! 装那一趟整趟在本机常驻后端（帧命令 `deploy-install`，编排住后端 `deploy` crate，手机同一份）：问那台要哪一格 · 落点那一份是谁 ·
//! 只升不降 · 取字节自检 · 放进那台的暂存区 ＋ 读回 · 那一份自己换进落点（`--place-self`）· 收拾残件。
//! 本模块只交事实（怎么够到那台 · 这一版带着的那几格字节落在本机哪个文件）、把后端推来的每一步交机器状态（`deploy` 帧）、把结局交出去。
//!
//! ## 只读铁律豁免（INVARIANT §1 / 账本 SS-G）—— 穷举登记见 `src/doc/INVARIANTS.md §1`
//! cc-monitor 对远端的写入均**用户显式触发**，各自独立路径守卫、绝不混用：
//! - **F08**：自部署后端二进制到 `~/.cc-monitor/bin/`（非用户数据、幂等、版本门控）。今天整趟在本机常驻后端。
//! - **F89a**：用户**显式**增/改/删远端**项目** `.mcp.json`：那台后端自己算、自己写（帧命令 `mcp-sync-apply` / `ext-uninstall-apply`）。
//! - **F50**：追加公钥到远端 `~/.ssh/authorized_keys`：本机常驻后端的帧命令 `pubkey-push`（不在本模块，登记于此备查）。

use crate::copy_table::copy_text;
use crate::detail::Said;

use crate::dial_host::RemoteFs;
use crate::stream_source::RemoteConfig;

/// 本机常驻后端那条命令的名字（与 `src/backend/stream/inbound/mod.rs` 那条硬臂同名）。
pub(crate) const INSTALL_CMD: &str = "deploy-install";

/// 等那一趟的上限：问几次那台 ＋ 推一份 MB 级的字节 ＋ 读回（同 `files` 链路 `put` 的上限，`dial_host::FILES_PUT_DEADLINE`）。
const INSTALL_BUDGET: std::time::Duration = std::time::Duration::from_secs(600);

/// 后端的落点：SFTP 那一侧（家目录相对）· 给人看的。**本机与远端同一个**，常量住 `relay_route_core`。
pub(crate) const LANDING_REL: &str = relay_route_core::BACKEND_LANDING_REL;
const LANDING_SHOWN: &str = "~/.cc-monitor/bin/ccm";

/// 这一趟做了什么（应答的 `action`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Did {
    /// 已是这一版。
    Skip,
    /// 放上了这一版。
    Deploy,
    /// 那台不比这一版旧 ⇒ 一个字节没写。
    Keep,
}

/// `deploy-install` 的应答（线上形状 `tests/__fixtures__/deploy-install.golden.json`）。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Installed {
    pub(crate) did: Did,
    /// 后端写好的那一句。
    pub(crate) said: String,
    /// 落点上现在那一份自报的身份。
    pub(crate) build: String,
    pub(crate) os: String,
    /// 问 `uname` 那一趟拨号的 ack：逐地址指纹交 `dial_host::settle_host_key` 固化（与 monitor 自己开链路那几条同一个判定）。
    pub(crate) ack: crate::ssh_link::Ack,
}

/// `deploy-install` 的应答 → [`Installed`]（**严格收**：少一格、多一格、认不出的值都是错 —— 两侧漂了要当场说出来）。
pub(crate) fn decode_installed(v: &serde_json::Value) -> Result<Installed, String> {
    const KEYS: [&str; 5] = ["ack", "action", "build", "os", "said"];
    let bad = || copy_text("rsSftp.plan.badProduct", &[("v", &v.to_string())]);
    let obj = v.as_object().ok_or_else(bad)?;
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    if keys != KEYS {
        return Err(bad());
    }
    let text = |k: &str| {
        obj.get(k)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(bad)
    };
    let did = match text("action")?.as_str() {
        "skip" => Did::Skip,
        "deploy" => Did::Deploy,
        "keep" => Did::Keep,
        _ => return Err(bad()),
    };
    let ack: crate::ssh_link::Ack = obj
        .get("ack")
        .cloned()
        .and_then(|a| serde_json::from_value(a).ok())
        .ok_or_else(bad)?;
    Ok(Installed {
        did,
        said: text("said")?,
        build: Some(text("build")?)
            .filter(|b| !b.is_empty())
            .ok_or_else(bad)?,
        os: text("os")?,
        ack,
    })
}

/// 这一版带着的那几格落成本机文件（`~/.cc-monitor/bin/slots/<os>-<arch>`，与本机后端同一套落盘：盘上那份逐字节相等就不动）
/// ⇒ `deploy-install` 的 `slots`。每个进程只落一次（同一版的字节不会变）。
fn slot_rows() -> Result<Vec<serde_json::Value>, Said> {
    static ROWS: std::sync::OnceLock<Vec<serde_json::Value>> = std::sync::OnceLock::new();
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
        rows.push(serde_json::json!({
            "os": key.os.label(),
            "arch": key.arch.label(),
            "id": id,
            "path": path.display().to_string(),
        }));
    }
    Ok(ROWS.get_or_init(|| rows).clone())
}

/// 那几格落在哪：本机后端落点旁边（`~/.cc-monitor/bin/slots`）。家目录问不到 ⇒ `None`。
fn slots_dir() -> Option<std::path::PathBuf> {
    let (rel, _) = relay_route_core::BACKEND_LANDING_REL.rsplit_once('/')?;
    creds_core::store::home_dir()
        .map(|h| rel.split('/').fold(h, |p, seg| p.join(seg)).join("slots"))
}

/// 请本机常驻后端给那台装一趟：入参只有事实；每走一步记进那台的状态成品（后端推 `changed {machine}`）。
async fn install(cfg: &RemoteConfig) -> Result<Installed, Said> {
    install_for(cfg, slot_rows()?).await
}

/// [`install`] 的本体，「带着哪几格」由调用方交（生产 = 这一版的槽落成的文件；回环台架交它送去的那一份）。
async fn install_for(cfg: &RemoteConfig, slots: Vec<serde_json::Value>) -> Result<Installed, Said> {
    use crate::backend_route::{route_call_error, Routed};
    let command = |said: String| Said::new(said, INSTALL_CMD, None);
    let dial = crate::dial_host::transfer_dial(cfg);
    let args = serde_json::json!({
        "dial": dial,
        "slots": slots,
        "machine": cfg.origin_label(),
    });
    let client = crate::dial_host::local_backend_accepting(INSTALL_CMD)
        .await
        .map_err(command)?;
    // 那一趟在本机常驻后端里 ⇒ 它写的那份详情（那台答 `uname` 的原话也在里面）原样带上。
    let data = client
        .call(INSTALL_CMD, args, INSTALL_BUDGET)
        .await
        .map_err(|e| {
            let said = match route_call_error(&e, &copy_core::local_machine(), |_code, message| {
                message.to_string()
            }) {
                Routed::NoChannel(s) | Routed::Refused(s) => s,
            };
            Said::of_call(said, INSTALL_CMD, &crate::origin::Origin::local(), &e)
        })?;
    let got =
        decode_installed(&data.ok_or_else(|| command(copy_text("rsSftp.plan.internal", &[])))?)
            .map_err(command)?;
    // 第一次连一台没钉过指纹的机器就在这一跳 ⇒ 照 monitor 自己开链路那几条同一个判定固化。
    crate::dial_host::settle_host_key(cfg, &dial, &got.ack);
    Ok(got)
}

/// 连接前确保远端后端已（自动）部署到固定落点 `~/.cc-monitor/bin/ccm`（issue #29；那个文件就是后端本身）。
///
/// **不阻断**：调用方（stream_source::run）拿到 `Err` 仍接着试连已有后端（手动部署的后端照样能连），
/// 但那句话经远端健康通道（`kind = "deploy"`）发到界面上，不再只是一行日志。
/// 返回值：`Ok(build)` = 已**确认**落点上那一份自报的身份（装上了这一版 · 已是这一版 · 那台不比这一版旧时是那台的）。
/// 调用方据此决定是否传新版才认识的流模式参数 —— `Err` 一律降级不传。
pub async fn ensure_backend_deployed(cfg: &RemoteConfig) -> Result<String, Said> {
    let got = install(cfg).await?;
    tracing::info!("远端 [{}] 部署：{}", cfg.origin_label(), got.said);
    Ok(got.build)
}

// ============================================================================
// F08c：手动安装 / 卸载后端（设置面板两个按钮）。安装逻辑同自动部署、但返回人读结果；
// 卸载删落点那个文件（固定落点 `~/.cc-monitor/bin/ccm`，没有外来路径要守）。
// ============================================================================

/// 手动安装 / 更新远端后端（机器页 ①「部署后端」按钮）：与自动部署同一趟（[`install`]），回那一句。
/// 那台不比这一版旧 ⇒ 手动点也不降级：那一句后面接出路（先卸载再部署 = 明确授权覆盖）。
#[tauri::command]
pub async fn deploy_remote_backend(cfg: RemoteConfig) -> Result<String, Said> {
    install(&cfg)
        .await
        .map(|got| match got.did {
            Did::Keep => copy_text("rsSftp.deploy.keptNotOlder", &[("why", &got.said)]),
            Did::Skip | Did::Deploy => got.said,
        })
        .map_err(|s| s.named("deploy_remote_backend"))
}

/// 卸载远端后端（设置面板「卸载后端」按钮）：删落点那个文件（它就是 `ccm`，卸后端就是卸 `ccm`）。
/// 只读铁律豁免（SS-G）：用户显式触发的删。注意：若该机器仍启用，自动部署会在下次连接重新装回——提示见返回消息。
#[tauri::command]
pub async fn uninstall_remote_backend(cfg: RemoteConfig) -> Result<String, Said> {
    let r: Result<String, Said> = async move {
        // 经本机常驻后端那条 `files` 链路删（写只许 `~/.cc-monitor/bin/` 与暂存区 —— 围栏拒 ⇒ 原话带回）。
        let fs = RemoteFs::open(&cfg).await?;
        let removed = fs.remove(LANDING_REL).await?;
        tracing::info!(
            "远端 [{}] 卸载后端：{LANDING_SHOWN} {}",
            cfg.origin_label(),
            if removed { "已删" } else { "本来就不在" }
        );
        let path = LANDING_SHOWN.to_string();
        if removed {
            Ok(copy_text("rsSftp.uninstall.done", &[("path", &path)]))
        } else {
            Ok(copy_text("rsSftp.uninstall.absent", &[("path", &path)]))
        }
    }
    .await;
    r.map_err(|s| s.named("uninstall_remote_backend"))
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/deploy_tests.rs"]
mod tests;
