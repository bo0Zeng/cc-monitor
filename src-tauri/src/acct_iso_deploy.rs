//! F5：一键部署 vendored `cc-acct-iso`（bash skill）到远端 + 存在性检测。
//!
//! 对标 [`crate::sftp::deploy_remote_daemon`]，但 cc-acct-iso 是一套 **bash 脚本**（非架构相关
//! 二进制），故直接 `include_bytes!` 内嵌 `src-tauri/vendor/cc-acct-iso/`，部署时 SFTP 推文件 +
//! 跑 `cc-acct-iso-install.sh`（**只软链 `~/.local/bin`、不碰 rc**，见脚本头注释）。
//!
//! 版本身份 = vendored 脚本内容哈希（`.vendor_id`）→ 远端 marker `<dest>/.vendor_id`，复用
//! [`crate::sftp::deploy_decision`] 的 skip-if-current 语义。**只读铁律豁免**：这是用户显式触发的
//! 一键安装（同 daemon 部署），且落点被 [`is_safe_remote_acct_iso_dir`] 守卫限制。

use crate::sftp::{
    connect_sftp, deploy_decision, ensure_dir_all, read_optional, upload_atomic, DeployAction,
};
use crate::ssh_source::{connect_and_exec_cmd, RemoteConfig};
use serde::Serialize;
use std::time::Duration;
use tokio::io::AsyncReadExt;

/// 一次性 exec 上限——install 建软链是本地文件操作、秒级完成；给足冗余同时防远端卡死
/// （D 审计 S1：`read_to_end` 无超时会令 IPC 永不返回、前端按钮永久停在「部署中…」）。
const EXEC_TIMEOUT: Duration = Duration::from_secs(45);

// ---- 内嵌 vendored 脚本（single source = src-tauri/vendor/cc-acct-iso/）----
const SCRIPT_MAIN: &[u8] = include_bytes!("../vendor/cc-acct-iso/scripts/cc-acct-iso");
const SCRIPT_LIB: &[u8] = include_bytes!("../vendor/cc-acct-iso/scripts/lib.sh");
const SCRIPT_INSTALL: &[u8] = include_bytes!("../vendor/cc-acct-iso/scripts/cc-acct-iso-install.sh");
const SCRIPT_TEST: &[u8] = include_bytes!("../vendor/cc-acct-iso/scripts/test/run-tests.sh");
const SKILL_MD: &[u8] = include_bytes!("../vendor/cc-acct-iso/SKILL.md");
const EXAMPLE_CONFIG: &[u8] = include_bytes!("../vendor/cc-acct-iso/examples/config");
/// 内嵌脚本内容指纹（build 期由 `.vendor_id` 决定），trim 掉尾换行。
const VENDOR_ID_RAW: &str = include_str!("../vendor/cc-acct-iso/.vendor_id");

fn vendor_id() -> &'static str {
    VENDOR_ID_RAW.trim()
}

/// 远端部署目录安全守卫（纯函数，可单测）：绝对路径、无 `..`、非根，且含约定标记词
/// （`cc-acct-iso` 或 `.cc-monitor`）——杜绝把部署误用成往任意远端目录写文件。
pub fn is_safe_remote_acct_iso_dir(path: &str) -> bool {
    let p = path.trim();
    !p.is_empty()
        && p.starts_with('/')
        && !p.contains("..")
        && p != "/"
        && (p.contains("cc-acct-iso") || p.contains(".cc-monitor"))
}

/// 远端 cc-acct-iso 状态（供前端决定：一键部署 / 走 init 向导 / 正常）。
#[derive(Debug, Serialize)]
pub struct AcctIsoStatus {
    /// 远端 PATH（含 ~/.local/bin）里能否找到 `cc-acct-iso`。
    pub installed: bool,
    /// `command -v cc-acct-iso` 命中的绝对路径（软链本身），未装为 None。
    pub path: Option<String>,
    /// 本 monitor 内嵌的 vendor 指纹——前端可比对提示「有更新」（当前 installed 判定用不到，
    /// 附带回传，避免以后要它时再加一趟往返）。
    pub vendor_id: String,
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
            .map_err(|e| format!("读远端输出失败: {e}"))?;
        Ok::<String, String>(String::from_utf8_lossy(&out).into_owned())
    };
    match tokio::time::timeout(EXEC_TIMEOUT, fut).await {
        Ok(r) => r,
        Err(_) => Err(format!("远端命令超时（>{}s）", EXEC_TIMEOUT.as_secs())),
    }
}

/// POSIX 单引号包裹（远端路径进 shell）。
fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 探测远端有没有装 `cc-acct-iso`（`command -v`）。**非交互 ssh 的 PATH 常不含 `~/.local/bin`**，
/// 故显式前置。只做一次 exec、不连 SFTP（D 审计 S2：读远端 marker 前端用不到、白握手一次）；
/// 不需要 dest_dir，故前端任何配置下都能探测（D 审计 S5：dest 推不出也不留 command-not-found 残角）。
#[tauri::command]
pub async fn check_remote_acct_iso(cfg: RemoteConfig) -> Result<AcctIsoStatus, String> {
    // command -v 命中即打印路径；未命中打印空行。PATH 显式含 ~/.local/bin（安装落点）。
    let probe = exec_collect(
        &cfg,
        "PATH=\"$HOME/.local/bin:$PATH\" command -v cc-acct-iso 2>/dev/null || true",
    )
    .await?;
    let path = probe.lines().next().map(str::trim).filter(|s| !s.is_empty());
    Ok(AcctIsoStatus {
        installed: path.is_some(),
        path: path.map(str::to_string),
        vendor_id: vendor_id().to_string(),
    })
}

/// 一键部署 / 更新 vendored cc-acct-iso 到远端 `dest_dir`，随后跑 install 脚本建软链。
/// 返回人读结果。逻辑对标 [`crate::sftp::deploy_remote_daemon`]。
#[tauri::command]
pub async fn deploy_remote_acct_iso(cfg: RemoteConfig, dest_dir: String) -> Result<String, String> {
    let dest = dest_dir.trim().trim_end_matches('/').to_string();
    if dest.is_empty() {
        return Err(
            "请先填部署目录（绝对路径，如 /home/<user>/.cc-monitor/cc-acct-iso）".into(),
        );
    }
    if dest.contains('~') {
        return Err("部署目录含 ~（SFTP 不展开 ~），请改用绝对路径".into());
    }
    if !is_safe_remote_acct_iso_dir(&dest) {
        return Err(format!(
            "部署目录不安全（须绝对、无 ..、非根、且含 cc-acct-iso 或 .cc-monitor）：{dest}"
        ));
    }

    let conn = connect_sftp(&cfg).await?;
    let sftp = &conn.sftp;
    let marker = format!("{dest}/.vendor_id");
    let remote_id = read_optional(sftp, &marker)
        .await
        .map(|b| String::from_utf8_lossy(&b).trim().to_string());

    match deploy_decision(remote_id.as_deref(), vendor_id()) {
        DeployAction::Skip => Ok(format!(
            "远端已是最新 cc-acct-iso（{}）：{dest}，无需重装。",
            vendor_id()
        )),
        DeployAction::Deploy(reason) => {
            // 建目录树：<dest>/scripts/test、<dest>/examples。
            ensure_dir_all(sftp, &format!("{dest}/scripts/test")).await;
            ensure_dir_all(sftp, &format!("{dest}/examples")).await;

            // 上传脚本（可执行 0o755）与文档/示例（0o644）。
            let scripts_dir = format!("{dest}/scripts");
            upload_atomic(sftp, &format!("{scripts_dir}/cc-acct-iso"), SCRIPT_MAIN, 0o755).await?;
            upload_atomic(sftp, &format!("{scripts_dir}/lib.sh"), SCRIPT_LIB, 0o755).await?;
            upload_atomic(
                sftp,
                &format!("{scripts_dir}/cc-acct-iso-install.sh"),
                SCRIPT_INSTALL,
                0o755,
            )
            .await?;
            upload_atomic(
                sftp,
                &format!("{scripts_dir}/test/run-tests.sh"),
                SCRIPT_TEST,
                0o755,
            )
            .await?;
            upload_atomic(sftp, &format!("{dest}/SKILL.md"), SKILL_MD, 0o644).await?;
            upload_atomic(sftp, &format!("{dest}/examples/config"), EXAMPLE_CONFIG, 0o644).await?;

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
                return Err(format!(
                    "脚本已上传到 {dest}，但 install（建软链）未成功完成——未标记为已装，可重试。远端输出：\n{}",
                    install_out.trim()
                ));
            }

            // install 成功 → 写 marker（部署成功身份）。
            upload_atomic(sftp, &marker, vendor_id().as_bytes(), 0o644).await?;

            // 复检 PATH 里是否可见。此时 install 已成功（软链已建），MISS 只可能是 ~/.local/bin
            // 不在**非交互 shell** 的 PATH 里 → 归因 PATH（不再误导成「脚本没就位」）。
            let visible = exec_collect(
                &cfg,
                "PATH=\"$HOME/.local/bin:$PATH\" command -v cc-acct-iso >/dev/null 2>&1 && echo OK || echo MISS",
            )
            .await
            .unwrap_or_default();
            let path_hint = if visible.trim() == "OK" {
                "已软链到 ~/.local/bin，命令可用。"
            } else {
                "已软链到 ~/.local/bin，但它可能不在你终端的 PATH——在终端里 `export PATH=\"$HOME/.local/bin:$PATH\"`。"
            };

            tracing::info!(
                "远端 [{}] 部署 cc-acct-iso 完成（{}）：{dest}",
                cfg.origin_label(),
                vendor_id()
            );
            Ok(format!(
                "已部署 cc-acct-iso（{}）到 {dest}（{reason}）。{path_hint}",
                vendor_id()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_dir_accepts_conventional_paths() {
        assert!(is_safe_remote_acct_iso_dir("/home/z/.cc-monitor/cc-acct-iso"));
        assert!(is_safe_remote_acct_iso_dir("/opt/cc-acct-iso"));
        assert!(is_safe_remote_acct_iso_dir("/home/z/.cc-monitor/x")); // 含 .cc-monitor
    }

    #[test]
    fn safe_dir_rejects_dangerous() {
        assert!(!is_safe_remote_acct_iso_dir("")); // 空
        assert!(!is_safe_remote_acct_iso_dir("/")); // 根
        assert!(!is_safe_remote_acct_iso_dir("relative/cc-acct-iso")); // 相对
        assert!(!is_safe_remote_acct_iso_dir("/home/../cc-acct-iso")); // ..
        assert!(!is_safe_remote_acct_iso_dir("/home/z/projects")); // 无标记词
        assert!(!is_safe_remote_acct_iso_dir("/tmp/evil")); // 无标记词
    }

    #[test]
    fn vendor_id_is_nonempty_trimmed() {
        let v = vendor_id();
        assert!(!v.is_empty());
        assert_eq!(v, v.trim());
        assert!(!v.contains('\n'));
    }
}
