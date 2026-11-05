//! **远端常驻后端，monitor 这一侧剩下的那一件**：起 · 找 · 只升不降（接上 · ensure · 换装 · 退避）是本机后端那几条常驻流
//! （`control/feeds.rs`，循环是 `resident_client::feed`，手机同一份）。这里剩：
//! - 停：请本机后端 `machine-stop`（它先停自己那一条、再在那台跑一次 `--resident-stop`：那台自己做「请它收尾 → 宽限期内等 → 到点强杀」），
//!   拿回 `graceful | killed | not_running`。本机那一趟（`local_backend_host::run_resident_stop`）读 `--resident-stop` 那一行，读法同循环那一份。

use crate::detail::Shell as _;
use cmd_wire::machine::{MachineStopReq, MachineStopped};
use cmd_wire::Call;
use copy_core::copy_text;
use copy_core::said::Said;

/// 等它的上限：那台自己的宽限期 ＋ 强杀后再等的那一段 ＋ 拨号都得在这里头
/// （`remote_resident_tests::the_stop_budget_outlasts_the_remote_stop` 现抠后端两个毫秒字面量比）。
const STOP_BUDGET: std::time::Duration = std::time::Duration::from_secs(120);

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

/// 读 `--resident-stop` 那一趟（纯函数）：三个词之外的一律是错，不猜（本机那一趟经它，`local_backend_host::run_resident_stop`）。
/// 没成的那几形（老后端 · 没装 · 非 unix · 那台的失败信封）照循环那一份读（`resident_client::feed::read_one_shot`）。
/// `machine` 是那台给人看的称呼（老后端 ⇒「{machine} 后端要更新」）。
pub(crate) fn read_stop(
    exec: &crate::stream_source::RemoteExec,
    machine: &str,
) -> Result<StopAnswer, Said> {
    // 本机那一趟是本机进程：有码 ⇒ 正常退出；没码 ⇒ 没给码的收尾（不是链路断）。
    let got = cmd_wire::dial::Captured {
        ended: exec
            .exit_status
            .map_or(cmd_wire::dial::CaptureEnd::ClosedBare, |code| {
                cmd_wire::dial::CaptureEnd::Exited { code }
            }),
        stdout: exec.stdout.clone(),
        stderr: exec.stderr.clone(),
    };
    let v = resident_client::feed::read_one_shot(&got, machine)
        .map_err(|f| crate::stream_source::said_of(&f.problem))?;
    stop_answer_of(&v)
}

/// **停那台的常驻后端**（机器页「停」）：请本机后端停（`machine-stop`：先停它那一条，再在那台发**一次** `--resident-stop`），
/// 等与强杀由那台自己做（同机监督者），这里只拿回结局。本机后端答了失败 ⇒ 它那一份原样；
/// 壳自己这一侧没成（本机后端不在 · 回包读不懂）⇒ 那一句说哪台没停成，下层那一句进原话。
pub(crate) async fn stop(machine: &str) -> Result<StopAnswer, cmd_wire::failed::Failed> {
    use crate::backend_route::{route_call_error, Routed};
    const NAME: &str = MachineStopReq::NAME;
    let own = |s: Said| {
        s.retold(copy_text(
            "rsBackendControl.remote.stopFailed",
            &[("origin", machine)],
        ))
        .named(NAME)
    };
    let client = crate::inbound_client::local_backend_accepting(NAME)
        .await
        .map_err(|said| own(Said::from(said)))?;
    let req = MachineStopReq {
        machine: machine.to_string(),
    };
    let deadline = tokio::time::Instant::now() + STOP_BUDGET;
    let got = client.ask(None, &req, None, deadline).await.map_err(|e| {
        let said = match route_call_error(&e, &copy_core::local_machine(), |_code, message| {
            message.to_string()
        }) {
            Routed::NoChannel(s) | Routed::Refused(s) => s,
        };
        crate::detail::of_call(said, NAME, &e)
    })?;
    match got {
        Ok(r) => answer_of_stopped(&r).map_err(own),
        Err(e) => Err(own(unknown_answer(e.to_string()))),
    }
}

/// `machine-stop` 的回包 ⇒ 结局（三个词之外的一律是错，不猜）。
pub(crate) fn answer_of_stopped(r: &MachineStopped) -> Result<StopAnswer, Said> {
    let raw = || serde_json::to_string(r).unwrap_or_default();
    answer_of(Some(&r.stopped), r.pid, raw)
}

/// `--resident-stop` 那一行 `{stopped, pid}` ⇒ 结局（同上）。
pub(crate) fn stop_answer_of(v: &serde_json::Value) -> Result<StopAnswer, Said> {
    let pid = v["pid"].as_u64().and_then(|p| u32::try_from(p).ok());
    answer_of(v["stopped"].as_str(), pid, || v.to_string())
}

fn answer_of(
    stopped: Option<&str>,
    pid: Option<u32>,
    raw: impl FnOnce() -> String,
) -> Result<StopAnswer, Said> {
    let stopped = match stopped {
        Some("graceful") => StopWord::Graceful,
        Some("killed") => StopWord::Killed,
        Some("not_running") => StopWord::NotRunning,
        _ => return Err(unknown_answer(raw())),
    };
    Ok(StopAnswer { stopped, pid })
}

fn unknown_answer(raw: String) -> Said {
    Said::with_raw(copy_text("rsRemoteResident.stop.unknownAnswer", &[]), raw)
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/remote_resident_tests.rs"]
mod tests;
