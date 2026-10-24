//! **远端常驻后端，monitor 这一侧剩下的那一件**：起 · 找 · 只升不降（接上 · ensure · 换装 · 退避）是 `resident-client` 那一条循环
//! （`resident_client::feed`，手机同一份），monitor 这一侧只给它接线（`stream_source/run.rs::DeskLink`）。这里剩：
//! - 停：`--resident-stop`（那台自己做「请它收尾 → 宽限期内等 → 到点强杀」，这里只发一次、拿回 `graceful | killed | not_running`）。
//!   一次性回话的读法与循环读 `--resident-ensure` 同一份（`resident_client::feed::read_one_shot`）；本机那一趟同读。

use crate::copy_table::copy_text;
use crate::detail::Said;
use crate::stream_source::RemoteConfig;

/// 不认这条子命令的老后端会把它当未知旗标、直接进流模式发 hello ⇒ 见到它就收工、当「太旧」（循环那一侧同一个记号）。
use cmd_wire::dial::OLD_BACKEND_MARKER;

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
/// 没成的那几形（老后端 · 没装 · 非 unix · 那台的失败信封）照循环那一份读（`resident_client::feed::read_one_shot`）。
/// `machine` 是那台给人看的称呼（老后端 ⇒「{machine} 后端要更新」）。
pub(crate) fn read_stop(
    exec: &crate::stream_source::RemoteExec,
    machine: &str,
) -> Result<StopAnswer, Said> {
    let got = resident_client::feed::Exec {
        exit_status: exec.exit_status,
        stdout: exec.stdout.clone(),
        stderr: exec.stderr.clone(),
    };
    let v = resident_client::feed::read_one_shot(&got, machine)
        .map_err(|f| crate::stream_source::said_of(&f.problem))?;
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
