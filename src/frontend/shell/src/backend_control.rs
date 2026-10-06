//! P2s（定框 `C8`）：**每台机一个后端开关** —— 状态 / 起 / 停，本机与远端**同一个契约**。
//!
//! # 这一层认识什么、不认识什么
//!
//! 它只认识 **origin**（本机是 [`crate::inbound_client::LOCAL_ORIGIN`]，远端是用户配的 label）。
//! 它**不认识 ssh、不认识进程监护** —— 那两样分别住 `stream_source` 与 `local_backend_host`。
//! 远端怎么起，由 `lib.rs` 在启动时注册一个**重起闭包**（把 replay / app handle / tx 那几个
//! 克隆关进去），本层只按 origin 找把手。
//!
//! ⇒ 「本地要和远端一样，只是远端走 ssh、本地不走」（`C1`）在命令面上的落点就是这一层：
//! **三个口各只有一条命令**，差别全部塞进各自的实现里。
//!
//! # ⚠ 「停」在两侧不是同一个动作（诚实边界 11c）
//!
//! 本机：杀掉被监护的子进程。远端：**断掉那条 SSH 流** —— 远端后端随之因管道破裂退出
//! （与本机 153ms 自杀同一个机制，见 P2s §0a）。两者结果相同、路径不同，本层不为时序差异作保。

use crate::copy_table::copy_text;
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

/// P2s（`C8`①）：**开关面该列哪几台机** —— 由后端说了算。
///
/// # 为什么不能让前端自己算〔D 阶段补审 08-11，A5〕
///
/// 前端原来用 `hostKey(h) = h.label.trim() || h.host` 自己拼清单，与 Rust 侧分叉四处：
///
/// | # | 分叉 | 后果 |
/// |---|---|---|
/// | 1 | 前端 `trim()`，Rust 的 `origin_label()` **不 trim** | label 是 `"  "` 时两边分别得到 `host` 与 `"  "` |
/// | 2 | **Rust 侧对重复 label 做后缀化**（`"pi" → "pi (#2)"`）并按后缀化后的名字注册 | 重复 label 的第二台，起/停恒回「没有这台机的把手」 |
/// | 3 | 前端忽略 `cfg.enabled` | 远端总开关关着时 `load_remote_configs` 直接返回空 ⇒ 一个都没注册，而 UI 照样列出全部 |
/// | 4 | `register_remote` 只在 `setup()` 跑一次 | 启动之后新增的远端机永远不会被注册，却会出现在列表里 |
///
/// ⇒ **注册表本身就是源头**：本命令直接倒它。前端只负责画。
/// 本机（`<local>`）**永远在第一行** —— 它不是「另一种机器」，只是不走 ssh 的那一台（`C1`）。
#[tauri::command]
pub fn backend_machines() -> Result<Vec<String>, String> {
    let mut out = vec![LOCAL_ORIGIN.to_string()];
    let g = remotes()
        .lock()
        .map_err(|e| copy_text("rsBackendControl.lock.poisoned", &[("e", &e.to_string())]))?;
    let mut names: Vec<String> = g.keys().cloned().collect();
    names.sort();
    out.extend(names);
    Ok(out)
}

/// P2s（`C8`②）：**这台机的后端现在什么状态**。
///
/// # 为什么「通道在不在」是两侧共用的那个真相
///
/// `inbound_client` 的登记表按 origin 存活着的通道 —— 远端在 hello 之后登记，
/// 本机（P2 之后）也在 hello 之后登记，**同一张表、同一个时机**。
/// ⇒ 问「这台机的后端在不在」不需要两套实现，那正是 `C1`。
///
/// `pid` / `attempts` 只有本机有（远端的进程在别人机器上，我们手里只有一条流）——
/// 这**不是欠账，是天然不对称**，所以它们是 `null` 而不是「远端那边填 0」。
///
/// 〔`INVARIANTS §10`〕**`async`**：本机那一支要拿句柄表的锁；起 / 停本机后端那一趟虽然不在锁里等外面了
/// （只拿 `local_backend_host` 那把起停串行锁），这里仍进 `spawn_blocking` —— 读锁这一下不许落在 IPC 派发线程上。
#[tauri::command]
pub async fn backend_status(origin: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || backend_status_now(origin))
        .await
        .map_err(|e| e.to_string())?
}

/// [`backend_status`] 的本体。
fn backend_status_now(origin: String) -> Result<serde_json::Value, String> {
    check_origin(&origin)?;
    let channel = crate::inbound_client::client_for(&origin).is_some();
    // ★★ `K-P1 KPY5`：**`detached` 的源头只能是「起它的时候走没走那条路」。**
    //
    // `is_detached()` 读的是一条**只在真的走过脱离那条路时才会被写下**的记录
    // （`local_backend_host::DETACHED`）。
    // ⚠ **不许拿 `channel` 或 `pid` 反推** —— 那正是 `P2d §0a` 翻掉的 `SSH_CONNECTION` 那一形：
    // 假信号不会报错，它只是**一直说是**，而在只有正例的测试里永远绿。
    // 远端那侧是 `null` 而不是 `false`：那个进程在别人机器上，「它脱没脱离」这句话
    // 在远端这条路上**没有意义**（同 `pid`/`attempts` 的天然不对称，不是欠账）。
    //
    // ⚠ 三格一起算，是因为 `the_three_ports_are_one_command_each_and_all_take_origin`
    // 逐口只许有**一处** `is_local(&origin)` 分派 —— 那条判据钉的正是「本机那一支只有一个入口」。
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
    // ★★ `K-P3b KP3W4`：**死亡账的读数也从这一口出去。**
    //
    // ⚠ 它**不走 `is_local` 分派**，理由是硬的：那张账是按 origin 存的
    // （`backend_policy::health(origin)`），远端那一格今天恒是「四个 0」——
    // 而那对远端是**真话**：本件不接远端（退出状态在别人机器上拿不到），
    // 所以「没人在记」正是那台机的实情，那一格说「答不出来」（判定在 `backend_policy::health_face`）。
    // ⇒ 这里**不能**填 `null` 装作不对称：`pid`/`attempts` 是「那个进程在别人机器上」，
    // 而这一格是「我们这边一条都没记过」，两件事。
    //
    // ⚠ 再开一个 `is_local(&origin)` 分派会当场撞
    // `the_three_ports_are_one_command_each_and_all_take_origin`（逐口恰好一处）。
    let h = crate::backend_policy::health(&origin);
    Ok(serde_json::json!({
        "machine": machine,
        "origin": origin,
        "channel": channel,
        "pid": pid,
        "attempts": attempts,
        "detached": detached,
        // 〔条 66〕原来这里还有一格 `killOnExit`（读 monitor 进程内那张表）。值搬到后端那台机器上之后
        //   这里没有它了 —— 要它问那台机器（后端 `exit-policy-read`），不许在这儿留一份副本。
        // 「健康」那一格是**成品**（状态 ＋ 一句 ＋ ⓘ ＋ `[详情]`），判定只在 `health_face`；
        //   原来这里交出去的四个计数与短摘要是原料，界面拿它再判一遍三档 —— 那一份判定随原料一起不上线了。
        "health": crate::backend_policy::health_face(&h),
    }))
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

/// P2s（`C8`②）：**起这台机的 backend**。已经在跑就是 no-op（`C8`①：每台机只许一个）。
///
/// 〔`INVARIANTS §10`〕**`async`**：本机那一支（`start_local_backend`）一路会起进程、
/// 连本机后端口、读 hello、`sleep` 等它绑上口、`attach_stream` 里 `block_on` —— 同步命令跑在 IPC 派发线程上，
/// 那几秒里别的 IPC 全排队。今天那一支进 `spawn_blocking`（形状照 [`backend_stop`] 本机那一支）；
/// 远端那一支只是换一个流任务的把手，不等任何东西，照旧就地做。判据 `sync_command_registry_tests`（例外表今天是空的）。
#[tauri::command]
pub async fn backend_start(origin: String) -> Result<String, String> {
    check_origin(&origin)?;
    if is_local(&origin) {
        // A6：**失败要回 `Err`**。原来三种结局都走 `Ok(reason)`，前端一律 `console.info`，
        // 「没内嵌后端」「释放失败」这两种真失败**一个 toast 都不弹**。
        return tauri::async_runtime::spawn_blocking(|| {
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
        .map_err(|e| e.to_string())?;
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
    // ⚠ **A2**：`JoinHandle` 完成之后**不会变成 `None`**〔D 阶段补审 08-11 修〕。
    // 原来判据是 `slot.handle.is_some()` ⇒ `stream_source::run` 一旦返回（`lib.rs` 记 error 后
    // task 结束），此后每次点「起」都恒回「已经在跑」，而实际上**一条流都没有**；
    // 用户只能先点「停」（abort 一个已结束的 handle）再点「起」。
    // ⇒ 改问 tokio 句柄的 `is_finished()`：**跑着才算在跑**。
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

/// P2s（`C8`②）：**停这台机的 backend**。
///
/// 本机远端同一条：在**那台机器上**跑一次 `--resident-stop`（同机监督者：请它收尾 → 宽限期内等 → 到点强杀），
/// 这里只发一次、拿回结局 `{stopped: graceful | killed | not_running, pid}`，机器页按它说一句。
/// 远端先 `abort()` 那条流再发（流随被停的那一位断，不能请它自己经那条流停自己）。本机那一支会等到结局（≤ 宽限期 ＋ 强杀后那一小段），
/// 同步命令跑在主线程上 ⇒ 等的那一段进阻塞线程池。
#[tauri::command]
pub async fn backend_stop(origin: String) -> Result<crate::remote_resident::StopAnswer, String> {
    check_origin(&origin)?;
    if is_local(&origin) {
        return tauri::async_runtime::spawn_blocking(crate::local_backend_host::stop_local_backend)
            .await
            .map_err(|e| e.to_string())?;
    }
    {
        let mut g = remotes()
            .lock()
            .map_err(|e| copy_text("rsBackendControl.lock.poisoned", &[("e", &e.to_string())]))?;
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
    stop_remote_resident(&origin).await
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
