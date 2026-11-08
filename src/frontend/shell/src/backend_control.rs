//! 每台机一个后端开关 —— 状态 / 起 / 停，本机与远端同一个契约（三个口各只有一条命令，差别塞进各自的实现里）。
//!
//! 它只认识 origin（本机是 [`cmd_wire::LOCAL`]，远端是用户配的 label），不认识 ssh、不认识进程监护
//! （本机那一份住 `local_backend_host`；各台的常驻流归本机后端，起停经它的 `machine-wake` · `machine-stop`）。

use crate::detail::Shell as _;
use cmd_wire::failed::Failed;
use copy_core::copy_text;
use copy_core::said::Said;

use cmd_wire::LOCAL;

fn is_local(origin: &str) -> bool {
    origin == LOCAL
}

fn check_origin(origin: &str) -> Result<(), Said> {
    if origin.trim().is_empty() {
        return Err(Said::of("rsBackendControl.origin.empty", &[]));
    }
    Ok(())
}

/// [`backend_status`] 的回包。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
pub struct BackendStatus {
    /// 本机后端起没起来；远端 ⇒ `null`（各台的状态成品在本机后端）。
    pub local: Option<crate::ui_contract::LocalBackend>,
    pub origin: String,
    /// 本机那条连接在不在；远端 ⇒ `null`（那台在不在只看本机后端推来的那台成品 `MachineState`，壳不另答一份）。
    pub channel: Option<bool>,
    pub pid: Option<u32>,
    pub attempts: Option<u32>,
    /// 走没走过脱离那条路；远端 ⇒ `null`。
    pub detached: Option<bool>,
    pub health: crate::backend_policy::HealthFace,
}

/// 这台机的后端现在什么状态。「通道在不在」只答本机（本机那条连接 hello 之后才有）；远端那台在不在的唯一源是本机后端推来的成品。
/// `pid` / `attempts` 只有本机有（远端的进程在别人机器上）—— 天然不对称，所以它们是 `null`。
/// `async`（`INVARIANTS §10`）：本机那一支要拿句柄表的锁，进 `spawn_blocking` —— 读锁这一下不许落在 IPC 派发线程上。
#[tauri::command]
pub async fn backend_status(origin: String) -> Result<BackendStatus, Failed> {
    let r: Result<BackendStatus, Said> = async move {
        Ok(tauri::async_runtime::spawn_blocking(move || {
            backend_status_now(origin, crate::backend_policy::local_ledger())
        })
        .await
        .map_err(Said::crashed)??)
    }
    .await;
    r.map_err(|s| s.named("backend_status"))
}

/// [`backend_status`] 的本体。死亡账注入（生产那一本是 `backend_policy::local_ledger`；判据各持一本）。
pub(crate) fn backend_status_now(
    origin: String,
    ledger: &crate::backend_policy::DeathLedger,
) -> Result<BackendStatus, Said> {
    check_origin(&origin)?;
    // `detached` 的源头只能是「起它的时候走没走那条路」：`is_detached()` 读的是一条只在真的走过脱离那条路时才写下的记录（`local_backend_host::DETACHED`）。
    // 不许拿 `channel` 或 `pid` 反推（假信号不报错，它只是一直说是）。远端那侧是 `null`（「它脱没脱离」在远端这条路上没有意义）。
    // 三格一起算：`the_three_ports_are_one_command_each_and_all_take_origin` 逐口只许有一处 `is_local(&origin)` 分派。
    // 本机后端起没起来同一处分派（按通道与手上这一版）；各台的状态成品在本机后端（`machine-read` · `changed {machine}`）⇒ 远端 `null`。
    // 死亡账是本机后端的账，同一处分派：远端那一格是空账 —— 远端退出状态在别人机器上拿不到，「没人在记」正是实情，
    //   那一格说「答不出来」（判定在 `backend_policy::health_face`）。不填 `null`：那与 `pid` / `attempts` 不是一件事。
    // 通道那一格同一处分派：只答本机；远端那台在不在只看本机后端推来的成品（界面照 `MachineState` 画），壳不另答。
    let (channel, pid, attempts, detached, local, h) = if is_local(&origin) {
        let channel = crate::inbound_client::local_client().is_some();
        let (p, a) = crate::local_backend_host::local_pid_and_attempts()?;
        (
            Some(channel),
            p,
            a,
            Some(crate::local_backend_host::is_detached()),
            Some(crate::local_backend_host::local_backend(
                channel,
                crate::byte_table::my_backend_id(),
            )),
            ledger.health(),
        )
    } else {
        (
            None,
            None,
            None,
            None,
            None,
            crate::backend_policy::Health::default(),
        )
    };
    Ok(BackendStatus {
        local,
        origin,
        channel,
        pid,
        attempts,
        detached,
        // 「退出行为」那个值住后端那台机器上（后端 `exit-policy-read`），这里不留副本。「健康」那一格是成品（状态 ＋ 一句 ＋ ⓘ ＋ `[详情]`），判定只在 `health_face`。
        health: crate::backend_policy::health_face(&h),
    })
}

/// 起这台机的 backend。已经在跑就是 no-op（每台机只许一个）。
/// `async`（`INVARIANTS §10`）：本机那一支（`start_local_backend`）一路会起进程、连本机后端口、读 hello、`sleep` 等它绑上口 ⇒ 进 `spawn_blocking`；
/// 远端那一支只是换一个流任务的把手，不等任何东西，就地做。判据 `sync_command_registry_tests`。
#[tauri::command]
pub async fn backend_start(origin: String) -> Result<(), Failed> {
    let own = |s: Said| s.named("backend_start");
    check_origin(&origin).map_err(own)?;
    if is_local(&origin) {
        // 失败要回 `Err`：「没内嵌后端」「释放失败」是真失败，要弹出来。
        return tauri::async_runtime::spawn_blocking(|| {
            use crate::local_backend_host::StartOutcome;
            match crate::local_backend_host::start_local_backend() {
                StartOutcome::Started(_) | StartOutcome::AlreadyRunning => Ok(()),
                StartOutcome::Failed { reason, looked_at } => Err(Said::restate(
                    copy_text(
                        "rsBackendControl.start.notFound",
                        &[
                            ("reason", &reason.said),
                            ("looked", &format!("{:?}", looked_at)),
                        ],
                    ),
                    reason,
                )),
            }
        })
        .await
        .map_err(Said::crashed)
        .and_then(|r| r)
        .map_err(own);
    }
    // 那台的常驻流（装 · 版本门 · 状态成品）住本机后端：叫醒它（停过的不再停着、断着的不等退避马上再接）。
    //   各处的［重新连接］走的就是这里；那台拒了（停用着 · 正在连 · 机器表里没有）· 本机后端不在 ⇒ 那一份失败原样回给界面。
    wake_backend_feed(&origin).await
}

/// 叫醒本机后端手上那台的常驻流（`machine-wake`）。拒了 / 没交上 ⇒ 本机后端交来的那一份原样，或本机没有通道那一句。
async fn wake_backend_feed(origin: &str) -> Result<(), Failed> {
    const WAKE_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);
    let req = cmd_wire::machine::MachineWakeReq {
        machine: origin.to_string(),
    };
    let deadline = crate::frame_query::Deadline::within(WAKE_BUDGET);
    crate::frame_query::ask(&crate::origin::Origin::local(), &req, None, deadline)
        .await
        .map(|_| ())
}

/// 停这台机的 backend。本机远端同一条：在那台机器上跑一次 `--resident-stop`（同机监督者：请它收尾 → 宽限期内等 → 到点强杀），
/// 这里只发一次、拿回结局 `{stopped: graceful | killed | not_running, pid}`，机器页按它说一句。
/// 远端那一条由本机后端先停它手上那台的常驻流再发（`machine-stop`）。本机那一支会等到结局 ⇒ 等的那一段进阻塞线程池。
#[tauri::command]
pub async fn backend_stop(origin: String) -> Result<crate::remote_resident::StopAnswer, Failed> {
    let own = |s: Said| s.named("backend_stop");
    check_origin(&origin).map_err(own)?;
    if is_local(&origin) {
        use tauri::async_runtime::spawn_blocking;
        return spawn_blocking(crate::local_backend_host::stop_local_backend)
            .await
            .map_err(Said::crashed)
            .and_then(|r| r)
            .map_err(own);
    }
    // 本机后端 `machine-stop`：先查表（表里没有 ⇒ `unknown_machine`）、停它那一条常驻流，再在那台跑 `--resident-stop`；没停成 ⇒ 它交来的那一份原样。
    crate::remote_resident::stop(&origin).await
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/backend_control_tests.rs"]
mod tests;
