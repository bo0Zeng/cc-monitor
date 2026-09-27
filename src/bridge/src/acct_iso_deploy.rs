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
use serde::Serialize;
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
    // T04 审计⑤：与 `is_safe_remote_backend_path` 5 个条件里 4 个逐字相同，已抽到
    // `sftp::is_safe_remote_managed_path`（2 个消费者，同 `find_pair` 那把 ≥2 尺子）。
    crate::sftp::is_safe_remote_managed_path(path, &["cc-acct-iso", ".cc-monitor"])
}

/// 远端 cc-acct-iso 状态（供前端决定：一键部署 / 走 init 向导 / 正常）。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
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

/// 〔LOC1a · 第四波 4D〕`acct-iso.*` 两问的期限（本机远端同一个）：`status` 只看文件在不在；
/// `shellinit` 起一次 `cc-acct-iso`（后端那侧自带 20 s 期限）。
pub(crate) const ACCT_ISO_BUDGET: Duration = Duration::from_secs(30);

/// `acct-iso-status` 的结局 → `AcctIsoStatus` —— **纯函数**，本机远端同一份。
///
/// 「没装」是 `Ok(installed:false)`（后端答了「没有」），不是 `Err`；
/// `Err` 只给「问不出来」的两档（够不着 / 对端说不行 · 应答缺格），且**不许**说成「没装」。
pub(crate) fn classify_status(
    who: &str,
    got: Result<serde_json::Value, String>,
) -> Result<AcctIsoStatus, String> {
    let v = got.map_err(|e| {
        copy_text(
            "rsAcctIsoDeploy.status.cannotAsk",
            &[("who", who), ("e", &e)],
        )
    })?;
    let installed = v
        .get("installed")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| copy_text("rsAcctIsoDeploy.status.noInstalled", &[("who", who)]))?;
    Ok(AcctIsoStatus {
        installed,
        path: v.get("path").and_then(|p| p.as_str()).map(str::to_string),
        vendor_id: vendor_id().to_string(),
    })
}

/// 问 `origin` 那台机器的后端：装没装 `cc-acct-iso`（帧命令 `acct-iso-status`）。**本机远端同一个函数。**
pub(crate) async fn status_on(origin: &crate::origin::Origin) -> Result<AcctIsoStatus, String> {
    let who = crate::backend::control::frame_query::who(origin);
    classify_status(
        &who,
        crate::backend::control::frame_query::call(
            origin,
            "acct-iso-status",
            serde_json::json!({}),
            crate::backend::control::frame_query::Deadline::within(ACCT_ISO_BUDGET),
        )
        .await,
    )
}

/// `acct-iso-shellinit` 的结局 → 片段原文（围栏还没判）—— **纯函数**，本机远端同一份。
pub(crate) fn snippet_of(
    who: &str,
    got: Result<serde_json::Value, String>,
) -> Result<String, String> {
    let v = got.map_err(|e| {
        copy_text(
            "rsAcctIsoDeploy.shellinit.failed",
            &[("who", who), ("message", &e)],
        )
    })?;
    v.get("snippet")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| copy_text("rsAcctIsoDeploy.shellinit.noSnippet", &[("who", who)]))
}

/// 问 `origin` 那台机器的后端：`cc-acct-iso shellinit` 的片段原文（帧命令 `acct-iso-shellinit`）。**本机远端同一个函数**；
/// 围栏由各自的调用方判（话不同：远端说「先在『维护』里部署」，本机今天没有那个口）。
pub(crate) async fn snippet_on(origin: &crate::origin::Origin) -> Result<String, String> {
    let who = crate::backend::control::frame_query::who(origin);
    snippet_of(
        &who,
        crate::backend::control::frame_query::call(
            origin,
            "acct-iso-shellinit",
            serde_json::json!({}),
            crate::backend::control::frame_query::Deadline::within(ACCT_ISO_BUDGET),
        )
        .await,
    )
}

/// 〔SH1 · `00 §2.5 ①`〕这台机器装没装 `cc-acct-iso` —— **一条命令带 origin**（原是本机 / 远端两条）。
/// 问那台机器的后端（帧命令 `acct-iso-status`，[`status_on`]）；那台长连接不在 ⇒ 说「没连上」（前端照旧落到向导那一支），不再单拨 SSH。
#[tauri::command]
pub async fn acct_iso_status(origin: crate::origin::Origin) -> Result<AcctIsoStatus, String> {
    origin.route("acct_iso_status")?;
    status_on(&origin).await
}

/// 〔SH1 · `00 §2.5 ①`〕这台机器 `cc-acct-iso shellinit` 的片段 —— **一条命令带 origin**（原是本机 / 远端两条）。
///
/// **为什么是抓那台的输出而不是在 TS 里重新生成一份**（Z05）：片段的形态是 `cc-acct-iso` 的知识，单一来源留在 bash。
/// 围栏判定只有一个（[`shellinit_fence_state`]），话按那台是本机还是远端各说各的（远端说「先在『维护』里部署」，本机没有那个口）。
#[tauri::command]
pub async fn acct_iso_shellinit(origin: crate::origin::Origin) -> Result<String, String> {
    let route = origin.route("acct_iso_shellinit")?;
    let out = snippet_on(&origin).await?;
    match route {
        crate::origin::Route::Local => crate::local_accounts::local_fence(out),
        crate::origin::Route::Remote(_) => validate_shellinit_output(out),
    }
}

/// 远端那一支（[`acct_iso_shellinit`]）的**fail-closed 校验**，抽成纯函数好单测。
///
/// `shellinit` 的输出恒被 BEGIN/END 围栏夹住。**两条都要在**：只查 BEGIN 的话，
/// 一次被截断的输出（SSH 中途断、超时）会带着半截片段过关，而**半截片段贴进 rc
/// 会让用户的登录 shell 直接报错**（未闭合的函数体）。这就是这条必须 fail-closed 的理由。
pub(crate) fn validate_shellinit_output(out: String) -> Result<String, String> {
    let state = shellinit_fence_state(&out);
    if state == FenceState::Complete {
        return Ok(out);
    }
    Err(if state == FenceState::Truncated {
        copy_text(
            "rsAcctIsoDeploy.shellinit.truncated",
            &[
                ("begin", &format!("{:?}", SHELLINIT_FENCE_BEGIN)),
                ("end", &format!("{:?}", SHELLINIT_FENCE_END)),
            ],
        )
    } else {
        copy_text(
            "rsAcctIsoDeploy.shellinit.missing",
            &[("begin", &format!("{:?}", SHELLINIT_FENCE_BEGIN))],
        )
    })
}

/// 片段的围栏齐不齐 —— 〔`A3` 第二波〕从 [`validate_shellinit_output`] 里抽出来的**判定**，
/// 远端那一支与本机那一支（`local_accounts::local_fence`）共用它；两边只是话不同
/// （远端说「先在『维护』里部署」，本机今天没有那个口）。**两条都要在**的理由见上面那个函数的头注。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FenceState {
    /// BEGIN 与 END 都在。
    Complete,
    /// 有 BEGIN 没 END —— 多半是被截断了。
    Truncated,
    /// 连 BEGIN 都没有 —— 没产出片段。
    Missing,
}

pub(crate) fn shellinit_fence_state(out: &str) -> FenceState {
    match (
        out.contains(SHELLINIT_FENCE_BEGIN),
        out.contains(SHELLINIT_FENCE_END),
    ) {
        (true, true) => FenceState::Complete,
        (true, false) => FenceState::Truncated,
        (false, _) => FenceState::Missing,
    }
}

/// `cc-acct-iso shellinit` 输出的围栏 —— **跨语言双写点**，由
/// `acct_iso_shellinit_fence_matches_vendored_script` 钉住（它读 vendored 脚本对拍）。
pub(crate) const SHELLINIT_FENCE_BEGIN: &str = "# ===== BEGIN cc-acct-iso =====";
pub(crate) const SHELLINIT_FENCE_END: &str = "# ===== END cc-acct-iso =====";

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
