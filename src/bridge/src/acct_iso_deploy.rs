//! F5：一键部署 vendored `cc-acct-iso`（bash skill）到远端 + 存在性检测。
//!
//! 对标 [`crate::sftp::deploy_remote_backend`]，但 cc-acct-iso 是一套 **bash 脚本**（非架构相关
//! 二进制），故直接 `include_bytes!` 内嵌 `src/bridge/vendor/cc-acct-iso/`，部署时经本机常驻后端（SFTP，〔SR1b〕只许落 `~/.cc-monitor/bin/` 底下）推文件。
//! 〔MIG-3a · 主会话 09-28 裁 2〕**落进用户目录那一步不在这里了**：从前随后经 ssh 跑 `cc-acct-iso-install.sh`（软链 `~/.local/bin` ＋ 抄配置）
//! 再 `command -v` 核一次 PATH；今天界面在这条之后问**那台后端** `acct-iso-install`（链接走写面的 `files-link`、装卸账记 skill 装记录），
//! 本文件一次 ssh exec 都不起。
//!
//! 版本身份 = vendored 脚本内容哈希（`.vendor_id`）→ 远端 marker `<dest>/.vendor_id`，复用
//! [`crate::sftp::deploy_decision`] 的 skip-if-current 语义。**只读铁律豁免**：这是用户显式触发的
//! 一键安装（同后端部署），且落点被 [`is_safe_remote_acct_iso_dir`] 守卫限制。

use crate::copy_table::copy_text;
use crate::dial_host::RemoteFs;
use crate::sftp::{deploy_decision, put_marker, read_marker, upload_verified, DeployAction};
use crate::ssh_source::RemoteConfig;

// ---- 内嵌 vendored 脚本（single source = src/bridge/vendor/cc-acct-iso/）----
const SCRIPT_MAIN: &[u8] = include_bytes!("../vendor/cc-acct-iso/scripts/cc-acct-iso");
const SCRIPT_LIB: &[u8] = include_bytes!("../vendor/cc-acct-iso/scripts/lib.sh");
const SCRIPT_INSTALL: &[u8] =
    include_bytes!("../vendor/cc-acct-iso/scripts/cc-acct-iso-install.sh");
const SCRIPT_TEST: &[u8] = include_bytes!("../vendor/cc-acct-iso/scripts/test/run-tests.sh");
const SKILL_MD: &[u8] = include_bytes!("../vendor/cc-acct-iso/SKILL.md");
const EXAMPLE_CONFIG: &[u8] = include_bytes!("../vendor/cc-acct-iso/examples/config");
/// 内嵌脚本内容指纹（build 期由 `.vendor_id` 决定），trim 掉尾换行。
const VENDOR_ID_RAW: &str = include_str!("../vendor/cc-acct-iso/.vendor_id");

pub(crate) fn vendor_id() -> &'static str {
    VENDOR_ID_RAW.trim()
}

/// 远端部署目录安全守卫（纯函数，可单测）：绝对路径、无 `..`、非根，且含约定标记词
/// （`cc-acct-iso` 或 `.cc-monitor`）——杜绝把部署误用成往任意远端目录写文件。
pub fn is_safe_remote_acct_iso_dir(path: &str) -> bool {
    // T04 审计⑤：与 `is_safe_remote_backend_path`〔散文墓碑〕5 个条件里 4 个逐字相同，已抽到
    // `sftp::is_safe_remote_managed_path`（2 个消费者，同 `find_pair` 那把 ≥2 尺子）。
    crate::sftp::is_safe_remote_managed_path(path, &["cc-acct-iso", ".cc-monitor"])
}

// 〔MIG-3a · 09-28 裁 2〕`exec_collect`（一次性 ssh exec ＋ 超时）与 `sq`（第五份 quote 的转调）随「跑安装脚本 · 核 PATH」两步退役〔散文墓碑〕。

// 〔MIG-3a · `99 §2.1 ⑬`〕`acct-iso.*` 两问（装没装 · rc 片段）的 Tauri 命令与判读（`classify_status` · 围栏校验〔散文墓碑〕）
//   退役：那台后端出成品（`accounts/iso.rs`：`acct-iso-status` · `acct-iso-shellinit` 自己校验围栏），界面经通道直问（`src/acct-iso-reads.ts`）。

/// 一键部署 / 更新 vendored cc-acct-iso 到远端 `dest_dir`（只推字节 ＋ 写标记；落进用户目录由界面随后问那台后端 `acct-iso-install`）。
/// 返回人读结果。逻辑对标 [`crate::sftp::deploy_remote_backend`]。
#[tauri::command]
pub async fn deploy_remote_acct_iso(cfg: RemoteConfig, dest_dir: String) -> Result<String, String> {
    let dest = dest_dir.trim().trim_end_matches('/').to_string();
    if dest.is_empty() {
        return Err(copy_text("rsAcctIsoDeploy.deploy.noDest", &[]).into());
    }
    if dest.contains('~') {
        return Err(copy_text("rsAcctIsoDeploy.deploy.tilde", &[]).into());
    }
    if !is_safe_remote_acct_iso_dir(&dest) {
        return Err(copy_text(
            "rsAcctIsoDeploy.deploy.unsafe",
            &[("dest", &dest.to_string())],
        ));
    }

    // 〔SR1b · 2026-09-24〕经本机常驻后端那条 `files` 链路（写只许 `~/.cc-monitor/bin/` 与暂存区 ⇒ `dest` 必须在
    //   `~/.cc-monitor/bin/` 下；前端默认推导同拍改成 `…/.cc-monitor/bin/cc-acct-iso`，不在就是后端围栏原话拒）。
    let fs = RemoteFs::open(&cfg).await?;
    let marker = format!("{dest}/.vendor_id");
    let remote_id = read_marker(&fs, &marker)
        .await?
        .map(|b| String::from_utf8_lossy(&b).trim().to_string());

    match deploy_decision(remote_id.as_deref(), vendor_id()) {
        DeployAction::Skip => Ok(copy_text(
            "rsAcctIsoDeploy.deploy.upToDate",
            &[
                ("version", &(vendor_id()).to_string()),
                ("dest", &dest.to_string()),
            ],
        )),
        // 〔HX2〕`deploy_decision` 不产这一格（只有后端身份那条 `identity_decision` 分新旧）；照它的话原样回。
        DeployAction::Keep { why, .. } => Ok(why),
        DeployAction::Deploy(reason) => {
            // 建目录树：<dest>/scripts/test、<dest>/examples。
            fs.mkdirs(&format!("{dest}/scripts/test")).await?;
            fs.mkdirs(&format!("{dest}/examples")).await?;

            // 上传脚本（可执行 0o755）与文档/示例（0o644）。
            let scripts_dir = format!("{dest}/scripts");
            upload_verified(
                &fs,
                &format!("{scripts_dir}/cc-acct-iso"),
                SCRIPT_MAIN,
                0o755,
            )
            .await?;
            upload_verified(&fs, &format!("{scripts_dir}/lib.sh"), SCRIPT_LIB, 0o755).await?;
            upload_verified(
                &fs,
                &format!("{scripts_dir}/cc-acct-iso-install.sh"),
                SCRIPT_INSTALL,
                0o755,
            )
            .await?;
            upload_verified(
                &fs,
                &format!("{scripts_dir}/test/run-tests.sh"),
                SCRIPT_TEST,
                0o755,
            )
            .await?;
            upload_verified(&fs, &format!("{dest}/SKILL.md"), SKILL_MD, 0o644).await?;
            upload_verified(
                &fs,
                &format!("{dest}/examples/config"),
                EXAMPLE_CONFIG,
                0o644,
            )
            .await?;

            // 字节齐了 → 写 marker（部署成功身份）。落进用户目录（链接 ＋ 配置）不在这一步：它每趟都由界面随后问那台后端，
            // 幂等（已在不动）⇒ 标记先写不会出现从前「标记写了、软链没建、下次 Skip 再不重跑」那种死锁（D 审计 I1 那一条的前提没了）。
            put_marker(&fs, &marker, vendor_id().as_bytes(), 0o644).await?;

            tracing::info!(
                "远端 [{}] 部署 cc-acct-iso 完成（{}）：{dest}",
                cfg.origin_label(),
                vendor_id()
            );
            Ok(copy_text(
                "rsAcctIsoDeploy.deploy.done",
                &[
                    ("version", &(vendor_id()).to_string()),
                    ("dest", &dest.to_string()),
                    ("reason", &reason.to_string()),
                ],
            ))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/acct_iso_deploy_tests.rs"]
mod tests;
