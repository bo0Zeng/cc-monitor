//! 每台机一个后端开关 —— 状态 / 起 / 停，本机与远端同一个契约（三个口各只有一条命令，差别塞进各自的实现里）。
//!
//! 它只认识 origin（本机是 [`crate::inbound_client::LOCAL_ORIGIN`]，远端是用户配的 label），不认识 ssh、不认识进程监护
//! （那两样住 `stream_source` 与 `local_backend_host`）。远端怎么起，由 `lib.rs` 在启动时注册一个重起闭包，本层只按 origin 找把手。

use crate::copy_table::copy_text;
use crate::detail::Said;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::inbound_client::LOCAL_ORIGIN;

/// 重起闭包：`lib.rs` 注册时把那台机需要的全部上下文关进来。
pub type Respawn = Box<dyn Fn() -> tauri::async_runtime::JoinHandle<()> + Send + Sync>;

/// 一台远端的「怎么再起」+「现在这条流的把手」+「按哪份连接参数起的」。
struct RemoteSlot {
    respawn: Respawn,
    handle: Option<tauri::async_runtime::JoinHandle<()>>,
    /// 连接参数的指纹（变了 ⇒ 热加载时按新参数重起）。
    cfg_key: String,
}

fn remotes() -> &'static Mutex<HashMap<String, RemoteSlot>> {
    static R: std::sync::OnceLock<Mutex<HashMap<String, RemoteSlot>>> = std::sync::OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `lib.rs` 启动时每台远端注册一次：给出**怎么起**，并把第一条流的把手交进来。
///
/// ⚠ 注册的是**闭包不是配置** —— 本层不认识 `RemoteConfig`，也不该认识。
/// 机器表里要连的一台：名字 · 连接参数指纹 · 怎么起。
pub struct WantedRemote {
    pub origin: String,
    pub cfg_key: String,
    pub respawn: Respawn,
}

/// 热加载那一趟做了什么（按名字排序）。
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Reconciled {
    pub started: Vec<String>,
    pub stopped: Vec<String>,
    pub restarted: Vec<String>,
}

/// 机器表热加载：注册表照 `wanted` 对齐 —— 表里没有了的停流摘掉；新来的注册并起；
/// 连接参数变了的停掉按新参数重起；其余一台不碰（在跑的流不断）。启动时与每次改机器表都走这一条。
pub fn reconcile_remotes(wanted: Vec<WantedRemote>) -> Reconciled {
    let mut out = Reconciled::default();
    let mut g = remotes().lock().expect("远端把手表锁毒化");
    let names: std::collections::HashSet<String> =
        wanted.iter().map(|w| w.origin.clone()).collect();
    let gone: Vec<String> = g.keys().filter(|k| !names.contains(*k)).cloned().collect();
    for origin in gone {
        if let Some(slot) = g.remove(&origin) {
            if let Some(h) = slot.handle {
                h.abort();
            }
        }
        out.stopped.push(origin);
    }
    for w in wanted {
        match g.get_mut(&w.origin) {
            Some(slot) if slot.cfg_key == w.cfg_key => {}
            Some(slot) => {
                if let Some(h) = slot.handle.take() {
                    h.abort();
                }
                slot.handle = Some((w.respawn)());
                slot.respawn = w.respawn;
                slot.cfg_key = w.cfg_key;
                out.restarted.push(w.origin);
            }
            None => {
                let first = (w.respawn)();
                g.insert(
                    w.origin.clone(),
                    RemoteSlot {
                        respawn: w.respawn,
                        handle: Some(first),
                        cfg_key: w.cfg_key,
                    },
                );
                out.started.push(w.origin);
            }
        }
    }
    out.started.sort();
    out.stopped.sort();
    out.restarted.sort();
    // 机器表换了 ⇒ 会话成品账按新表重算「各台都报完」（同 `backend_machines` 那一份：本机恒第一，远端按名字排）。
    let mut table = vec![LOCAL_ORIGIN.to_string()];
    let mut names: Vec<String> = g.keys().cloned().collect();
    drop(g);
    names.sort();
    table.extend(names);
    crate::session_book::machines_changed(table);
    out
}

fn is_local(origin: &str) -> bool {
    origin == LOCAL_ORIGIN
}

fn check_origin(origin: &str) -> Result<(), String> {
    if origin.trim().is_empty() {
        return Err(copy_text("rsBackendControl.origin.empty", &[]).into());
    }
    Ok(())
}

/// 开关面该列哪几台机 —— 由后端的注册表说了算，前端只负责画：前端自己拼清单会在 trim · 重复 label 的后缀化（`"pi" → "pi (#2)"`）·
/// 远端总开关 · 启动之后新增的机器上与注册表分叉。本机（`<local>`）永远在第一行 —— 它只是不走 ssh 的那一台。
#[tauri::command]
pub fn backend_machines() -> Result<Vec<String>, Said> {
    let r: Result<Vec<String>, Said> = (move || -> Result<Vec<String>, Said> {
        let mut out = vec![LOCAL_ORIGIN.to_string()];
        let g = remotes()
            .lock()
            .map_err(|e| copy_text("rsBackendControl.lock.poisoned", &[("e", &e.to_string())]))?;
        let mut names: Vec<String> = g.keys().cloned().collect();
        names.sort();
        out.extend(names);
        Ok(out)
    })();
    r.map_err(|s| s.named("backend_machines"))
}

/// 这台机的后端现在什么状态。「通道在不在」两侧共用一张表：`inbound_client` 的登记表按 origin 存活着的通道，本机与远端都在 hello 之后登记。
/// `pid` / `attempts` 只有本机有（远端的进程在别人机器上）—— 天然不对称，所以它们是 `null`。
/// `async`（`INVARIANTS §10`）：本机那一支要拿句柄表的锁，进 `spawn_blocking` —— 读锁这一下不许落在 IPC 派发线程上。
#[tauri::command]
pub async fn backend_status(origin: String) -> Result<serde_json::Value, Said> {
    let r: Result<serde_json::Value, Said> = async move {
        Ok(
            tauri::async_runtime::spawn_blocking(move || backend_status_now(origin))
                .await
                .map_err(|e| e.to_string())??,
        )
    }
    .await;
    r.map_err(|s| s.named("backend_status"))
}

/// [`backend_status`] 的本体。
fn backend_status_now(origin: String) -> Result<serde_json::Value, String> {
    check_origin(&origin)?;
    let channel = crate::inbound_client::client_for(&origin).is_some();
    // `detached` 的源头只能是「起它的时候走没走那条路」：`is_detached()` 读的是一条只在真的走过脱离那条路时才写下的记录（`local_backend_host::DETACHED`）。
    // 不许拿 `channel` 或 `pid` 反推（假信号不报错，它只是一直说是）。远端那侧是 `null`（「它脱没脱离」在远端这条路上没有意义）。
    // 三格一起算：`the_three_ports_are_one_command_each_and_all_take_origin` 逐口只许有一处 `is_local(&origin)` 分派。
    // 那台的状态成品（判定只在 `machine_state`）同一处分派：本机按通道与手上这一版；远端按那张表，「连接这台」关着 ⇒ 停用。
    let (pid, attempts, detached) = if is_local(&origin) {
        let (p, a) = crate::local_backend_host::local_pid_and_attempts()?;
        (
            p,
            a,
            serde_json::json!(crate::local_backend_host::is_detached()),
        )
    } else {
        (None, None, serde_json::Value::Null)
    };
    let machine = machine_product(&origin, channel);
    // 死亡账的读数也从这一口出去。它不走 `is_local` 分派：那张账按 origin 存（`backend_policy::health(origin)`），远端那一格恒是「四个 0」——
    // 远端退出状态在别人机器上拿不到，「没人在记」正是实情，那一格说「答不出来」（判定在 `backend_policy::health_face`）。
    // 不填 `null`：那与 `pid` / `attempts` 不是一件事。再开一个 `is_local(&origin)` 分派会撞那条「逐口恰好一处」的判据。
    let h = crate::backend_policy::health(&origin);
    Ok(serde_json::json!({
        "machine": machine,
        "origin": origin,
        "channel": channel,
        "pid": pid,
        "attempts": attempts,
        "detached": detached,
        // 「退出行为」那个值住后端那台机器上（后端 `exit-policy-read`），这里不留副本。「健康」那一格是成品（状态 ＋ 一句 ＋ ⓘ ＋ `[详情]`），判定只在 `health_face`。
        "health": crate::backend_policy::health_face(&h),
    }))
}

/// 那台此刻的状态成品（通道在不在现看）：`machine-state` 推送与诊断信息那一段读这一处。
pub(crate) fn machine_now(origin: &str) -> crate::machine_state::MachineState {
    machine_product(origin, crate::inbound_client::client_for(origin).is_some())
}

/// 那台的状态成品（`backend_status` 的 `machine` 一格与 `machine-state` 推送同一处）：本机按通道与手上这一版；
/// 远端按那张表，「连接这台」关着 ⇒ 停用。
pub(crate) fn machine_product(origin: &str, channel: bool) -> crate::machine_state::MachineState {
    if is_local(origin) {
        return crate::machine_state::local_product(channel, crate::byte_table::my_backend_id());
    }
    let enabled = crate::load_all_remote_configs()
        .into_iter()
        .find(|(c, _)| c.origin_label() == origin)
        .is_none_or(|(_, connect)| connect);
    crate::machine_state::product(origin, enabled)
}

/// 起这台机的 backend。已经在跑就是 no-op（每台机只许一个）。
/// `async`（`INVARIANTS §10`）：本机那一支（`start_local_backend`）一路会起进程、连本机后端口、读 hello、`sleep` 等它绑上口 ⇒ 进 `spawn_blocking`；
/// 远端那一支只是换一个流任务的把手，不等任何东西，就地做。判据 `sync_command_registry_tests`。
#[tauri::command]
pub async fn backend_start(origin: String) -> Result<String, Said> {
    let r: Result<String, Said> = async move {
        check_origin(&origin)?;
        if is_local(&origin) {
            // 失败要回 `Err`：「没内嵌后端」「释放失败」是真失败，要弹出来。
            return Ok(tauri::async_runtime::spawn_blocking(|| {
                use crate::local_backend_host::StartOutcome;
                match crate::local_backend_host::start_local_backend() {
                    StartOutcome::Started(p) => Ok(format!("已起：{}", p.display())),
                    StartOutcome::AlreadyRunning => {
                        Ok("本机后端已经在跑（C8①：每台机只许一个）".into())
                    }
                    StartOutcome::Failed { reason, looked_at } => Err(copy_text(
                        "rsBackendControl.start.notFound",
                        &[
                            ("reason", &reason.to_string()),
                            ("looked", &format!("{:?}", looked_at)),
                        ],
                    )),
                }
            })
            .await
            .map_err(|e| e.to_string())??);
        }
        let mut g = remotes()
            .lock()
            .map_err(|e| copy_text("rsBackendControl.lock.poisoned", &[("e", &e.to_string())]))?;
        let slot = g.get_mut(&origin).ok_or_else(|| {
            copy_text(
                "rsBackendControl.handle.missing",
                &[("origin", &origin.to_string())],
            )
        })?;
        // `JoinHandle` 完成之后不会变成 `None` ⇒ 问 tokio 句柄的 `is_finished()`：跑着才算在跑（否则流任务结束之后每次点「起」都恒回「已经在跑」）。
        if slot
            .handle
            .as_ref()
            .is_some_and(|h| !h.inner().is_finished())
        {
            // 在跑、但断着在退避里等 ⇒ 立刻重拨一次（主窗口那条 `{machine} 离线` 的［重新连接］走的就是这里）。
            crate::inbound_client::kick(&origin);
            return Ok(format!("{origin} 的流已经在跑（C8①：每台机只许一个）"));
        }
        slot.handle = Some((slot.respawn)());
        Ok(format!("{origin} 的流已重起"))
    }
    .await;
    r.map_err(|s| s.named("backend_start"))
}

/// 停这台机的 backend。本机远端同一条：在那台机器上跑一次 `--resident-stop`（同机监督者：请它收尾 → 宽限期内等 → 到点强杀），
/// 这里只发一次、拿回结局 `{stopped: graceful | killed | not_running, pid}`，机器页按它说一句。
/// 远端先 `abort()` 那条流再发（不能请它自己经那条流停自己）。本机那一支会等到结局 ⇒ 等的那一段进阻塞线程池。
#[tauri::command]
pub async fn backend_stop(origin: String) -> Result<crate::remote_resident::StopAnswer, Said> {
    let r: Result<crate::remote_resident::StopAnswer, Said> = async move {
        check_origin(&origin)?;
        if is_local(&origin) {
            use tauri::async_runtime::spawn_blocking;
            return spawn_blocking(crate::local_backend_host::stop_local_backend)
                .await
                .map_err(|e| e.to_string())?
                .map_err(Said::from);
        }
        {
            let mut g = remotes().lock().map_err(|e| {
                copy_text("rsBackendControl.lock.poisoned", &[("e", &e.to_string())])
            })?;
            let slot = g.get_mut(&origin).ok_or_else(|| {
                copy_text(
                    "rsBackendControl.handle.missing",
                    &[("origin", &origin.to_string())],
                )
            })?;
            if let Some(h) = slot.handle.take() {
                h.abort();
            }
        }
        Ok(stop_remote_resident(&origin).await?)
    }
    .await;
    r.map_err(|s| s.named("backend_stop"))
}

/// 停那台的常驻后端（`--resident-stop`，经链路在那台跑）。只由 [`backend_stop`] 在分过本机之后调。
async fn stop_remote_resident(origin: &str) -> Result<crate::remote_resident::StopAnswer, String> {
    let cfg = crate::load_remote_config_by_label(origin).ok_or_else(|| {
        copy_text(
            "rsBackendControl.handle.missing",
            &[("origin", &origin.to_string())],
        )
    })?;
    crate::remote_resident::stop(&cfg).await.map_err(|e| {
        copy_text(
            "rsBackendControl.remote.stopFailed",
            &[("origin", &origin.to_string()), ("e", &e)],
        )
    })
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/backend_control_tests.rs"]
mod tests;
