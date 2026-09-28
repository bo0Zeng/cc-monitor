//! F5：一键部署 vendored `cc-acct-iso`（bash skill）到远端 + 存在性检测。
//!
//! 对标 [`crate::sftp::deploy_remote_backend`]，但 cc-acct-iso 是一套 **bash 脚本**（非架构相关
//! 二进制），故直接 `include_bytes!` 内嵌 `src/bridge/vendor/cc-acct-iso/`，部署时经本机常驻后端（SFTP，〔SR1b〕只许落 `~/.cc-monitor/bin/` 底下）推文件 +
//! 跑 `cc-acct-iso-install.sh`（**只软链 `~/.local/bin`、不碰 rc**，见脚本头注释）。
//!
//! 版本身份 = vendored 脚本内容哈希（`.vendor_id`）→ 远端 marker `<dest>/.vendor_id`，复用
//! [`crate::sftp::deploy_decision`] 的 skip-if-current 语义。**只读铁律豁免**：这是用户显式触发的
//! 一键安装（同后端部署），且落点被 [`is_safe_remote_acct_iso_dir`] 守卫限制。

use crate::copy_table::copy_text;
use crate::dial_host::RemoteFs;
use crate::sftp::{deploy_decision, put_marker, read_marker, upload_verified, DeployAction};
use crate::ssh_source::{connect_and_exec_cmd, RemoteConfig};
use std::time::Duration;
use tokio::io::AsyncReadExt;

/// 一次性 exec 上限——install 建软链是本地文件操作、秒级完成；给足冗余同时防远端卡死
/// （D 审计 S1：`read_to_end` 无超时会令 IPC 永不返回、前端按钮永久停在「部署中…」）。
const EXEC_TIMEOUT: Duration = Duration::from_secs(45);

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

/// 一次性远端 exec，收全 stdout（非交互 shell）。带超时（D 审计 S1）。
async fn exec_collect(cfg: &RemoteConfig, cmd: &str) -> Result<String, String> {
    let fut = async {
        let stream = connect_and_exec_cmd(cfg, cmd).await?;
        let mut reader = tokio::io::BufReader::new(stream);
        let mut out = Vec::new();
        reader
            .read_to_end(&mut out)
            .await
            .map_err(|e| copy_text("rsAcctIsoDeploy.exec.readFailed", &[("e", &e.to_string())]))?;
        Ok::<String, String>(String::from_utf8_lossy(&out).into_owned())
    };
    match tokio::time::timeout(EXEC_TIMEOUT, fut).await {
        Ok(r) => r,
        Err(_) => Err(copy_text(
            "rsAcctIsoDeploy.exec.timeout",
            &[("secs", &(EXEC_TIMEOUT.as_secs()).to_string())],
        )),
    }
}

/// POSIX 单引号包裹（远端路径进 shell）。
///
/// U8c-2b-0（账本 S5）：实现收进 `shell-quote-core`（P4c 前叫 `launch-core`）——
/// 这是收口时**守卫当场抓到的第五份**
/// （我摸底只数出四份，S5 记的也是四份）。五份**逐字节相同**、从来没红过，
/// 靠巧合保持一致 —— 这正是那条守卫要防的东西。
pub(crate) fn sq(s: &str) -> String {
    // `pub(crate)` 只为让 `quote_singleton_guard` 的行为对拍够得着它。
    shell_quote_core::posix_quote(s)
}

// 〔MIG-3a · `99 §2.1 ⑬`〕`acct-iso.*` 两问（装没装 · rc 片段）的 Tauri 命令与判读（`classify_status` · 围栏校验〔散文墓碑〕）
//   退役：那台后端出成品（`accounts/iso.rs`：`acct-iso-status` · `acct-iso-shellinit` 自己校验围栏），界面经通道直问（`src/acct-iso-reads.ts`）。

/// 一键部署 / 更新 vendored cc-acct-iso 到远端 `dest_dir`，随后跑 install 脚本建软链。
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

            // 跑 install 脚本建软链（BIN_DIR 默认 ~/.local/bin；脚本刻意不碰 rc）。install.sh
            // `set -euo pipefail`，成功才跑到我们追加的 sentinel。D 审计 I1：**不能** `|| true` 吞
            // 退出码——软链失败（~/.local 不可写等）若被吞，marker 照写 → 谎报成功 + 下次 deploy_decision
            // 判 Skip 再不重跑 → 死锁。故：无 sentinel = install 未成功 → 不写 marker、返回 Err（可重试）。
            const OK_SENTINEL: &str = "__CCM_ACCT_ISO_INSTALL_OK__";
            let install_cmd = format!(
                "bash {} 2>&1 && printf '\\n{OK_SENTINEL}\\n'",
                sq(&format!("{scripts_dir}/cc-acct-iso-install.sh"))
            );
            let install_out = exec_collect(&cfg, &install_cmd).await?;
            if !install_out.contains(OK_SENTINEL) {
                tracing::warn!(
                    "远端 [{}] cc-acct-iso install 未成功完成（未写 marker，可重试）：\n{}",
                    cfg.origin_label(),
                    install_out.trim()
                );
                return Err(copy_text(
                    "rsAcctIsoDeploy.deploy.installFailed",
                    &[
                        ("dest", &dest.to_string()),
                        ("output", &(install_out.trim()).to_string()),
                    ],
                ));
            }

            // install 成功 → 写 marker（部署成功身份）。
            put_marker(&fs, &marker, vendor_id().as_bytes(), 0o644).await?;

            // 复检 PATH 里是否可见。此时 install 已成功（软链已建），MISS 只可能是 ~/.local/bin
            // 不在**非交互 shell** 的 PATH 里 → 归因 PATH（不再误导成「脚本没就位」）。
            let visible = exec_collect(
                &cfg,
                "PATH=\"$HOME/.local/bin:$PATH\" command -v cc-acct-iso >/dev/null 2>&1 && echo OK || echo MISS",
            )
            .await
            .unwrap_or_default();
            let path_hint = if visible.trim() == "OK" {
                &copy_text("rsAcctIsoDeploy.deploy.linked", &[])
            } else {
                &copy_text("rsAcctIsoDeploy.deploy.linkedNotOnPath", &[])
            };

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
                    ("pathHint", &path_hint.to_string()),
                ],
            ))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/acct_iso_deploy_tests.rs"]
mod tests;
