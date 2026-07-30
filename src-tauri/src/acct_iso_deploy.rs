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
    connect_sftp, deploy_decision, ensure_dir_all, read_optional, upload_atomic,
    upload_atomic_verified, DeployAction,
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
const SCRIPT_INSTALL: &[u8] =
    include_bytes!("../vendor/cc-acct-iso/scripts/cc-acct-iso-install.sh");
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
    // T04 审计⑤：与 `is_safe_remote_daemon_path` 5 个条件里 4 个逐字相同，已抽到
    // `sftp::is_safe_remote_managed_path`（2 个消费者，同 `find_pair` 那把 ≥2 尺子）。
    crate::sftp::is_safe_remote_managed_path(path, &["cc-acct-iso", ".cc-monitor"])
}

/// 远端 cc-acct-iso 状态（供前端决定：一键部署 / 走 init 向导 / 正常）。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
    let path = probe
        .lines()
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    Ok(AcctIsoStatus {
        installed: path.is_some(),
        path: path.map(str::to_string),
        vendor_id: vendor_id().to_string(),
    })
}

/// Z05：抓远端 `cc-acct-iso shellinit` 的输出，交给前端做「待贴文本」。
///
/// **为什么是抓远端而不是在 TS 里重新生成一份**：片段的形态（`export CLAUDE_CONFIG_DIR=<默认号>`
/// + 每账号一个 `<名>cc()` + Z01 的 `0cc()` 逃生口）是 `cc-acct-iso` 的知识。在 TS 里照抄一份
/// 就多一个**跨语言双写点** —— 那正是本工作区反复在治的病（`TMUX_LS_FMT` / `NATIVE_IDENTITY` /
/// `--base`）。抓输出则**单一来源留在 bash**，一处都不用同步。
///
/// **只读**：`cmd_shellinit` 全是 `printf`，不写任何文件（已逐行核过）。
/// 它更**不会**去动用户的 `~/.bashrc` —— 贴不贴由用户自己决定，这是明令的红线。
///
/// `2>/dev/null` 丢掉 warn（比如「manifest 里没有默认账号」）：那些混进 stdout 会让片段贴了就坏。
/// 真出问题由下面的围栏校验兜住，并把可执行的下一步写进错误文案。
#[tauri::command]
pub async fn remote_acct_iso_shellinit(cfg: RemoteConfig) -> Result<String, String> {
    let out = exec_collect(
        &cfg,
        "PATH=\"$HOME/.local/bin:$PATH\" cc-acct-iso shellinit 2>/dev/null || true",
    )
    .await?;
    validate_shellinit_output(out)
}

/// `remote_acct_iso_shellinit` 的**fail-closed 校验**，抽成纯函数好单测（SSH 那半测不了）。
///
/// `shellinit` 的输出恒被 BEGIN/END 围栏夹住。**两条都要在**：只查 BEGIN 的话，
/// 一次被截断的输出（SSH 中途断、超时）会带着半截片段过关，而**半截片段贴进 rc
/// 会让用户的登录 shell 直接报错**（未闭合的函数体）。这就是这条必须 fail-closed 的理由。
pub(crate) fn validate_shellinit_output(out: String) -> Result<String, String> {
    let has_begin = out.contains(SHELLINIT_FENCE_BEGIN);
    let has_end = out.contains(SHELLINIT_FENCE_END);
    if has_begin && has_end {
        return Ok(out);
    }
    Err(if has_begin {
        format!(
            "远端产出的 rc 片段**不完整**（有 {SHELLINIT_FENCE_BEGIN:?} 但没有 \
{SHELLINIT_FENCE_END:?}）——输出可能被截断了。**别贴**，半截片段会让登录 shell 报错。请重试。"
        )
    } else {
        format!(
            "远端没能产出 rc 片段（输出里没有 {SHELLINIT_FENCE_BEGIN:?}）。\
常见原因：cc-acct-iso 未安装（先在「维护」里部署）、或该远端还没跑过 `cc-acct-iso init`。"
        )
    })
}

/// `cc-acct-iso shellinit` 输出的围栏 —— **跨语言双写点**，由
/// `acct_iso_shellinit_fence_matches_vendored_script` 钉住（它读 vendored 脚本对拍）。
pub(crate) const SHELLINIT_FENCE_BEGIN: &str = "# ===== BEGIN cc-acct-iso =====";
pub(crate) const SHELLINIT_FENCE_END: &str = "# ===== END cc-acct-iso =====";

/// 一键部署 / 更新 vendored cc-acct-iso 到远端 `dest_dir`，随后跑 install 脚本建软链。
/// 返回人读结果。逻辑对标 [`crate::sftp::deploy_remote_daemon`]。
#[tauri::command]
pub async fn deploy_remote_acct_iso(cfg: RemoteConfig, dest_dir: String) -> Result<String, String> {
    let dest = dest_dir.trim().trim_end_matches('/').to_string();
    if dest.is_empty() {
        return Err("请先填部署目录（绝对路径，如 /home/<user>/.cc-monitor/cc-acct-iso）".into());
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
            upload_atomic_verified(
                sftp,
                &format!("{scripts_dir}/cc-acct-iso"),
                SCRIPT_MAIN,
                0o755,
            )
            .await?;
            upload_atomic_verified(sftp, &format!("{scripts_dir}/lib.sh"), SCRIPT_LIB, 0o755)
                .await?;
            upload_atomic_verified(
                sftp,
                &format!("{scripts_dir}/cc-acct-iso-install.sh"),
                SCRIPT_INSTALL,
                0o755,
            )
            .await?;
            upload_atomic_verified(
                sftp,
                &format!("{scripts_dir}/test/run-tests.sh"),
                SCRIPT_TEST,
                0o755,
            )
            .await?;
            upload_atomic_verified(sftp, &format!("{dest}/SKILL.md"), SKILL_MD, 0o644).await?;
            upload_atomic_verified(
                sftp,
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

    /// ★ Z05 跨语言双写点守卫：`SHELLINIT_FENCE_BEGIN` 必须与 vendored `cc-acct-iso`
    /// 里 `cmd_shellinit` 真正打印的那行**逐字一致**。
    ///
    /// **为什么需要它**：Rust 侧拿这个围栏当「片段产出成功」的判据（没有它就报错、
    /// 绝不把半截东西交给前端当待贴文本）。bash 那边哪天改了围栏措辞，表现是
    /// **功能整体失灵但错误文案听起来像用户的错**（「远端没能产出 rc 片段」）。
    ///
    /// 做法同 `tmux.rs::tmux_ls_fmt_double_write_point_stays_in_sync` 与
    /// `accounts_query.rs` 的 Z06 守卫：`include_str!` 读 **vendored** 副本 + 锚定那一行。
    /// **`cp -a` 保 mtime ⇒ 本地 re-vendor 后要 `touch` 本文件，否则判的是上次的结果**（Z06 实测）。
    #[test]
    fn acct_iso_shellinit_fence_matches_vendored_script() {
        let script = include_str!("../vendor/cc-acct-iso/scripts/cc-acct-iso");
        for fence in [SHELLINIT_FENCE_BEGIN, SHELLINIT_FENCE_END] {
            assert!(
                script.contains(&format!("printf '{fence}\\n'")),
                "Z05 双写点漂移：vendored cc-acct-iso 里找不到打印 {fence:?} 的那行。\n\
                 Rust 侧拿这两条围栏当「片段完整」的判据，两边必须一致。"
            );
        }
        // 反向自检：断言的是「源真读进来了」，不是「命中若干条」。
        assert!(
            script.len() > 1000,
            "include_str! 没读到 vendored 脚本，上面的断言是空转"
        );
    }

    /// fail-closed：**半截片段绝不放行**（贴进 rc 会让登录 shell 报错）。
    #[test]
    fn shellinit_validation_is_fail_closed() {
        let good = format!("{SHELLINIT_FENCE_BEGIN}\nzcc() {{ :; }}\n{SHELLINIT_FENCE_END}\n");
        assert_eq!(validate_shellinit_output(good.clone()).unwrap(), good);

        // 只有 BEGIN（输出被截断）⇒ 拒，且诊断要说「不完整」而不是「没产出」
        let truncated = format!("{SHELLINIT_FENCE_BEGIN}\nzcc() {{ :; }}\n");
        let e = validate_shellinit_output(truncated).unwrap_err();
        assert!(e.contains("不完整"), "诊断该说是截断，实得：{e}");
        assert!(e.contains("别贴"), "必须明确劝阻，实得：{e}");

        // 压根没跑成（没装 / PATH 不对）⇒ 拒，诊断给可执行的下一步
        let e = validate_shellinit_output(String::new()).unwrap_err();
        assert!(e.contains("未安装"), "诊断该给下一步，实得：{e}");

        // 只有 END（不可能但也是坏数据）⇒ 拒
        assert!(validate_shellinit_output(SHELLINIT_FENCE_END.to_string()).is_err());
    }

    #[test]
    fn safe_dir_accepts_conventional_paths() {
        assert!(is_safe_remote_acct_iso_dir(
            "/home/z/.cc-monitor/cc-acct-iso"
        ));
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
