//! **远端常驻后端，monitor 这一侧剩下的那几件**：起 · 找 · 只升不降（接上 · ensure · 换装 · 退避）是 `resident-client` 那一条循环
//! （`resident_client::feed`，手机同一份），monitor 这一侧只给它接线（`stream_source/run.rs::DeskLink`）。这里剩：
//! - 停：`--resident-stop`（那台自己做「请它收尾 → 宽限期内等 → 到点强杀」，这里只发一次、拿回 `graceful | killed | not_running`）；
//! - 读一次性回话（[`parse_answer`]：停 · 本机那一趟同读）。

use crate::copy_table::copy_text;
use crate::detail::Said;
use crate::stream_source::RemoteConfig;

/// 不认这条子命令的老后端会把它当未知旗标、直接进流模式发 hello ⇒ 见到它就收工、当「太旧」（循环那一侧同一个记号）。
use resident_client::feed::OLD_BACKEND_MARKER;

/// 为什么没接上那台的常驻后端（那一句 ＋ 复制详情）。比较只比那一句与码（详情里有时刻）。
#[derive(Debug, Clone)]
pub(crate) enum AttachErr {
    /// 那台不是 Unix（后端脱离不了）⇒ **永久不支持**：记在那台的连接状态里，
    /// 不再自动按退避重连；界面出声，用户点「起」（`backend_start`）才再试一次。
    Unsupported(Said),
    /// 别的失败 ⇒ 照常按退避重连。
    Failed(Said),
}

impl PartialEq for AttachErr {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (AttachErr::Unsupported(a), AttachErr::Unsupported(b)) => a.said == b.said,
            (AttachErr::Failed(a), AttachErr::Failed(b)) => a.said == b.said,
            _ => false,
        }
    }
}

impl From<Said> for AttachErr {
    fn from(s: Said) -> Self {
        AttachErr::Failed(s)
    }
}

impl AttachErr {
    /// 给人看的那句 ＋ 复制详情。
    pub(crate) fn said(self) -> Said {
        match self {
            AttachErr::Unsupported(s) | AttachErr::Failed(s) => s,
        }
    }
}

/// 读 `--resident-ensure` / `--resident-stop` 那一趟的结果（纯函数）：退出 0 ⇒ stdout 那一行；退出 2 ⇒ stderr 的 `{code, message, detail, data?}`。
/// `unsupported`（那台脱离不了，非 unix）明说「远端只支持 Unix」；老后端 ⇒ 「太旧」—— 都是失败，没有回落。
/// `machine` 是那台给人看的称呼（老后端 ⇒「{machine} 后端要更新」）。
pub(crate) fn parse_answer(
    exec: &crate::stream_source::RemoteExec,
    machine: &str,
) -> Result<serde_json::Value, AttachErr> {
    if exec.stdout.contains(OLD_BACKEND_MARKER) {
        return Err(Said::written(copy_core::backend_old(machine)).into());
    }
    match exec.exit_status {
        Some(0) => serde_json::from_str(exec.stdout.trim()).map_err(|e| {
            Said::with_raw(
                copy_text("rsRemoteResident.ensure.answerUnreadable", &[]),
                &e,
            )
            .into()
        }),
        _ => {
            let err: serde_json::Value =
                serde_json::from_str(exec.stderr.trim()).unwrap_or_default();
            let msg = err["message"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| {
                    // 退出码只进日志；给人看的只说没起来、没说原因。
                    tracing::warn!(
                        "resident ensure exited {:?} without a reason",
                        exec.exit_status
                    );
                    copy_text("rsRemoteResident.ensure.noReason", &[])
                });
            // 那台的失败信封只有一种（`{code, message, detail, data?}`）：`detail` 是那台写好的一整份复制详情（下层原话在里面），
            //   原样当一块转交、补「本机」一行；老后端没写 ⇒ 只有那一句。
            let wrote = err["detail"].as_str().filter(|d| !d.trim().is_empty());
            if err["code"] == "unsupported" {
                // 那台的 `message` 这一格就是它的原话（脱离不了的系统报错）：句子由这边说，原话进详情。
                let said = copy_text("rsRemoteResident.ensure.unsupported", &[]);
                Err(AttachErr::Unsupported(match wrote {
                    Some(d) => Said::relayed_from(said, d, Some(&msg)),
                    None => Said::with_raw(said, &msg),
                }))
            } else {
                Err(AttachErr::Failed(match wrote {
                    Some(d) => Said::relayed_from(msg, d, None),
                    None => Said::written(msg),
                }))
            }
        }
    }
}

/// 「停」的结局 —— 后端 `--resident-stop` 那一行原样的三个词（`control/resident.rs::Stopped`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopWord {
    /// 收到「请你收尾」之后在宽限期内自己退了。
    Graceful,
    /// 宽限期满还在 ⇒ 强杀了（在跑的写可能只做了一半）。
    Killed,
    /// 本来就没在跑。
    NotRunning,
}

/// `backend_stop` 交给机器页的结局：`{stopped, pid}`（本机远端同形）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StopAnswer {
    pub stopped: StopWord,
    pub pid: Option<u32>,
}

/// 读 `--resident-stop` 那一趟（纯函数）：三个词之外的一律是错，不猜（本机那一趟也经它，`local_backend_host::run_resident_stop`）。
pub(crate) fn read_stop(
    exec: &crate::stream_source::RemoteExec,
    machine: &str,
) -> Result<StopAnswer, Said> {
    let v = parse_answer(exec, machine).map_err(AttachErr::said)?;
    let stopped = match v["stopped"].as_str() {
        Some("graceful") => StopWord::Graceful,
        Some("killed") => StopWord::Killed,
        Some("not_running") => StopWord::NotRunning,
        _ => {
            return Err(Said::with_raw(
                copy_text("rsRemoteResident.stop.unknownAnswer", &[]),
                exec.stdout.trim(),
            ))
        }
    };
    let pid = v["pid"].as_u64().and_then(|p| u32::try_from(p).ok());
    Ok(StopAnswer { stopped, pid })
}

/// **停那台的常驻后端**（机器页「停」）：发**一次** `--resident-stop`，等与强杀由那台自己做（同机监督者），这里只拿回结局。
pub(crate) async fn stop(cfg: &RemoteConfig) -> Result<StopAnswer, Said> {
    // 落点固定、打头的 `--` 让那台的 `ccm` 当后端用。
    let cmd = format!(
        "{} {} --resident-stop",
        crate::stream_source::BACKEND_CMD,
        crate::local_backend::BACKEND_SEP
    );
    let exec =
        crate::stream_source::connect_and_exec_capture(cfg, &cmd, Some(OLD_BACKEND_MARKER)).await?;
    read_stop(&exec, &cfg.origin_label())
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/remote_resident_tests.rs"]
mod tests;
