//! P2s（定框 `C8`）：**每台机一个 daemon 开关** —— 状态 / 起 / 停，本机与远端**同一个契约**。
//!
//! # 这一层认识什么、不认识什么
//!
//! 它只认识 **origin**（本机是 [`crate::inbound_client::LOCAL_ORIGIN`]，远端是用户配的 label）。
//! 它**不认识 ssh、不认识进程监护** —— 那两样分别住 `ssh_source` 与 `local_daemon`。
//! 远端怎么起，由 `lib.rs` 在启动时注册一个**重起闭包**（把 replay / app handle / tx 那几个
//! 克隆关进去），本层只按 origin 找把手。
//!
//! ⇒ 「本地要和远端一样，只是远端走 ssh、本地不走」（`C1`）在命令面上的落点就是这一层：
//! **三个口各只有一条命令**，差别全部塞进各自的实现里。
//!
//! # ⚠ 「停」在两侧不是同一个动作（诚实边界 11c）
//!
//! 本机：杀掉被监护的子进程。远端：**断掉那条 SSH 流** —— 远端 daemon 随之因管道破裂退出
//! （与本机 153ms 自杀同一个机制，见 P2s §0a）。两者结果相同、路径不同，本层不为时序差异作保。

use std::collections::HashMap;
use std::sync::Mutex;

use crate::inbound_client::LOCAL_ORIGIN;

/// 一台远端的「怎么再起」+ 「现在这条流的把手」。
struct RemoteSlot {
    /// 重起闭包：`lib.rs` 注册时把该台机需要的全部上下文关进来。
    respawn: Box<dyn Fn() -> tauri::async_runtime::JoinHandle<()> + Send + Sync>,
    handle: Option<tauri::async_runtime::JoinHandle<()>>,
}

fn remotes() -> &'static Mutex<HashMap<String, RemoteSlot>> {
    static R: std::sync::OnceLock<Mutex<HashMap<String, RemoteSlot>>> = std::sync::OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `lib.rs` 启动时每台远端注册一次：给出**怎么起**，并把第一条流的把手交进来。
///
/// ⚠ 注册的是**闭包不是配置** —— 本层不认识 `RemoteConfig`，也不该认识。
pub fn register_remote(
    origin: String,
    respawn: Box<dyn Fn() -> tauri::async_runtime::JoinHandle<()> + Send + Sync>,
    first: tauri::async_runtime::JoinHandle<()>,
) {
    let mut g = remotes().lock().expect("远端把手表锁毒化");
    g.insert(
        origin,
        RemoteSlot {
            respawn,
            handle: Some(first),
        },
    );
}

fn is_local(origin: &str) -> bool {
    origin == LOCAL_ORIGIN
}

fn check_origin(origin: &str) -> Result<(), String> {
    if origin.trim().is_empty() {
        return Err("origin 不许为空 —— 开关是 per-host 的，没有「全局」这一档".into());
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
/// ⇒ **注册表本身就是真相源**：本命令直接倒它。前端只负责画。
/// 本机（`<local>`）**永远在第一行** —— 它不是「另一种机器」，只是不走 ssh 的那一台（`C1`）。
#[tauri::command]
pub fn daemon_machines() -> Result<Vec<String>, String> {
    let mut out = vec![LOCAL_ORIGIN.to_string()];
    let g = remotes().lock().map_err(|e| format!("锁毒化: {e}"))?;
    let mut names: Vec<String> = g.keys().cloned().collect();
    names.sort();
    out.extend(names);
    Ok(out)
}

/// P2s（`C8`②）：**这台机的 daemon 现在什么状态**。
///
/// # 为什么「通道在不在」是两侧共用的那个真相
///
/// `inbound_client` 的登记表按 origin 存活着的通道 —— 远端在 hello 之后登记，
/// 本机（P2 之后）也在 hello 之后登记，**同一张表、同一个时机**。
/// ⇒ 问「这台机的 daemon 在不在」不需要两套实现，那正是 `C1`。
///
/// `pid` / `attempts` 只有本机有（远端的进程在别人机器上，我们手里只有一条流）——
/// 这**不是欠账，是天然不对称**，所以它们是 `null` 而不是「远端那边填 0」。
#[tauri::command]
pub fn daemon_status(origin: String) -> Result<serde_json::Value, String> {
    check_origin(&origin)?;
    let channel = crate::inbound_client::client_for(&origin).is_some();
    // ★★ `K-P1 KPY5`：**`detached` 的真相源只能是「起它的时候走没走那条路」。**
    //
    // `is_detached()` 读的是一条**只在真的走过脱离那条路时才会被写下**的记录
    // （`local_daemon::DETACHED`）。
    // ⚠ **不许拿 `channel` 或 `pid` 反推** —— 那正是 `P2d §0a` 翻掉的 `SSH_CONNECTION` 那一形：
    // 假信号不会报错，它只是**一直说是**，而在只有正例的测试里永远绿。
    // 远端那侧是 `null` 而不是 `false`：那个进程在别人机器上，「它脱没脱离」这句话
    // 在远端这条路上**没有意义**（同 `pid`/`attempts` 的天然不对称，不是欠账）。
    //
    // ⚠ 三格一起算，是因为 `the_three_ports_are_one_command_each_and_all_take_origin`
    // 逐口只许有**一处** `is_local(&origin)` 分派 —— 那条判据钉的正是「本机那一支只有一个入口」。
    let (pid, attempts, detached) = if is_local(&origin) {
        let (p, a) = crate::local_daemon::local_pid_and_attempts()?;
        (p, a, serde_json::json!(crate::local_daemon::is_detached()))
    } else {
        (None, None, serde_json::Value::Null)
    };
    // ★★ `K-P3b KP3W4`：**死亡账的读数也从这一口出去。**
    //
    // ⚠ 它**不走 `is_local` 分派**，理由是硬的：那张账是按 origin 存的
    // （`daemon_policy::health(origin)`），远端那一格今天恒是「四个 0」——
    // 而那对远端是**真话**：本件不接远端（退出状态在别人机器上拿不到），
    // 所以「没人在记」正是那台机的实情，TS 那侧会照 `seen == 0` 说「答不出来」。
    // ⇒ 这里**不能**填 `null` 装作不对称：`pid`/`attempts` 是「那个进程在别人机器上」，
    // 而这一格是「我们这边一条都没记过」，两件事。
    //
    // ⚠ 再开一个 `is_local(&origin)` 分派会当场撞
    // `the_three_ports_are_one_command_each_and_all_take_origin`（逐口恰好一处）。
    let h = crate::daemon_policy::health(&origin);
    Ok(serde_json::json!({
        "origin": origin,
        "channel": channel,
        "pid": pid,
        "attempts": attempts,
        "detached": detached,
        "killOnExit": crate::daemon_policy::kill_on_exit(&origin),
        // 键名与 TS 的 `DaemonHealth` 接口逐格对齐（`src/daemon-policy.ts`）。
        "health": serde_json::json!({
            "crashed": h.crashed,
            "refused": h.refused,
            "neverStarted": h.never_started,
            "misread": h.misread,
            "last": h.last,
        }),
    }))
}

/// P2s（`C8`②）：**起这台机的 daemon**。已经在跑就是 no-op（`C8`①：每台机只许一个）。
#[tauri::command]
pub fn daemon_start(origin: String) -> Result<String, String> {
    check_origin(&origin)?;
    if is_local(&origin) {
        // A6：**失败要回 `Err`**。原来三种结局都走 `Ok(reason)`，前端一律 `console.info`，
        // 「没内嵌 daemon」「释放失败」这两种真失败**一个 toast 都不弹**。
        use crate::local_daemon::StartOutcome;
        return match crate::local_daemon::start_local_backend() {
            StartOutcome::Started(p) => Ok(format!("已起：{}", p.display())),
            StartOutcome::AlreadyRunning => Ok("本机后端已经在跑（C8①：每台机只许一个）".into()),
            StartOutcome::Failed { reason, looked_at } => {
                Err(format!("{reason}；找过 {looked_at:?}"))
            }
        };
    }
    let mut g = remotes().lock().map_err(|e| format!("锁毒化: {e}"))?;
    let slot = g
        .get_mut(&origin)
        .ok_or_else(|| format!("没有这台机的把手：{origin}（启动时没注册过？）"))?;
    // ⚠ **A2**：`JoinHandle` 完成之后**不会变成 `None`**〔D 阶段补审 08-11 修〕。
    // 原来判据是 `slot.handle.is_some()` ⇒ `ssh_source::run` 一旦返回（`lib.rs` 记 error 后
    // task 结束），此后每次点「起」都恒回「已经在跑」，而实际上**一条流都没有**；
    // 用户只能先点「停」（abort 一个已结束的 handle）再点「起」。
    // ⇒ 改问 tokio 句柄的 `is_finished()`：**跑着才算在跑**。
    if slot
        .handle
        .as_ref()
        .is_some_and(|h| !h.inner().is_finished())
    {
        return Ok(format!("{origin} 的流已经在跑（C8①：每台机只许一个）"));
    }
    slot.handle = Some((slot.respawn)());
    Ok(format!("{origin} 的流已重起"))
}

/// P2s（`C8`②）：**停这台机的 daemon**。
///
/// ⚠ 远端这一侧是 `abort()` 那条流 —— 远端 daemon 随之因管道破裂退出。
/// 本层**不等它退**（我们在这台机上看不见那个进程），所以返回的是「已断流」不是「已停进程」。
/// **文案不许把这两件事写成一件**（P2s-Y5）。
#[tauri::command]
pub fn daemon_stop(origin: String) -> Result<String, String> {
    check_origin(&origin)?;
    if is_local(&origin) {
        return crate::local_daemon::stop_local_backend();
    }
    let mut g = remotes().lock().map_err(|e| format!("锁毒化: {e}"))?;
    let slot = g
        .get_mut(&origin)
        .ok_or_else(|| format!("没有这台机的把手：{origin}（启动时没注册过？）"))?;
    match slot.handle.take() {
        Some(h) => {
            h.abort();
            Ok(format!(
                "{origin} 的流已断（远端 daemon 随管道破裂退出，本机看不见它）"
            ))
        }
        None => Ok(format!("{origin} 的流本来就没在跑")),
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/daemon_control_tests.rs"]
mod tests;
