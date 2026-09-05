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
mod tests {
    use super::*;

    /// P2s-Y1（acceptor: 机检）：**三个口各只有一条命令，且都吃 `origin`**。
    ///
    /// 防的是「本机一套名字、远端另一套名字」—— 那样两侧就长出两套语义，
    /// 正是 `C1` 排除掉的做法（也是 #58 门⑤ 要防的形态）。
    #[test]
    fn the_three_ports_are_one_command_each_and_all_take_origin() {
        let src = guard_core::production_code(include_str!("daemon_control.rs"));
        const PORTS: &[&str] = &["daemon_status", "daemon_start", "daemon_stop"];
        for p in PORTS {
            let sig = format!("pub fn {p}(origin: String)");
            let at = guard_core::find_pinned(&src, &sig).unwrap_or_else(|e| {
                panic!(
                    "`{p}` 不是「恰好一处、且第一个参数是 origin」的形状（{e}）。\n\
                     ★ 三个口必须**各只有一条命令**且都按 origin 分派 —— 本机一套、远端一套\n\
                     就是两套语义（`C1` 明说排除这条路）。"
                )
            });
            // ★★ **逐口切体，不数全局**。
            //
            // 第一版写的是「`is_local(&origin)` 全局出现 >= 2 处」。变异实测：
            // 把 `daemon_stop` 里那一支整个摘掉，**判据照样绿** —— 因为另外两口还各有一处，
            // 计数仍然 >= 2。⇒ 计数型自检在「人群有多个成员」时天然逮不到「某一个成员塌了」。
            let body: String = src[at..]
                .lines()
                .skip(1)
                // ⚠ 收尾行**不写字面量右花括号** —— 本仓有判据用「花括号配平」剥测试段
                // （`ssh_source::strip_cfg_test`），源码里多一个孤立的右花括号会让它**提前闭合**（`b'…'` 的字符字面量也算，我第一次「修」时就还带着一个），
                // 测试段整段泄漏进「生产段」⇒ 别的判据当场误报（08-11 实测：单写者守卫红了）。
                .take_while(|l| *l != "\u{7d}")
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                body.len() > 30,
                "`{p}` 切出来的函数体只有 {} 字节 —— 切错了，本条在空转",
                body.len()
            );
            guard_core::find_pinned(&body, "is_local(&origin)").unwrap_or_else(|e| {
                panic!(
                    "`{p}` 里没有恰好一处 `is_local(&origin)` 分派（{e}）——\n\
                     ⇒ 这一口没接上本机那侧（或接了两次），`C1`「本地要和远端一样」在这口上落空。"
                )
            });
        }
    }

    /// ★★ **两条「说成功其实没成」的形态**〔D 阶段补审 08-11 新增，A2 + A6〕。
    ///
    /// | # | 原形态 | 后果 |
    /// |---|---|---|
    /// | A2 | 远端「起」的判据是 `slot.handle.is_some()` | `JoinHandle` 完成后**不会变 `None`** ⇒ 流早就结束了，每次「起」仍恒回「已经在跑」；用户只能先「停」再「起」 |
    /// | A6 | `daemon_start` 把 `Resolved::Missing{reason}` 当 `Ok(reason)` | 「没内嵌 daemon」「释放失败」两种**真失败**在前端只走 `console.info`，**一个 toast 都不弹** |
    ///
    /// 本条钉：① 远端那支必须问 `is_finished()`（不是 `is_some()`）；
    /// ② 本机那支必须有 `Failed` ⇒ `Err` 那一格（三态各有去处，不靠 `reason` 字符串猜）。
    #[test]
    fn starting_reports_failure_as_failure_and_finished_streams_as_not_running() {
        let src = guard_core::production_code(include_str!("daemon_control.rs"));
        let at = guard_core::find_pinned(&src, "pub fn daemon_start(origin: String)")
            .expect("起口不在了");
        let body: String = src[at..]
            .lines()
            .skip(1)
            .take_while(|l| *l != "\u{7d}")
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            body.len() > 200,
            "切出来的体只有 {} 字节 —— 切错了",
            body.len()
        );

        guard_core::find_pinned(&body, "is_finished()").unwrap_or_else(|e| {
            panic!(
                "远端「起」不再问 `is_finished()`（{e}）。\n\
                 若换回 `handle.is_some()`：`JoinHandle` 完成后不会变 `None`\n\
                 ⇒ 流早就结束了，「起」仍恒回「已经在跑」，用户只能先「停」再「起」。"
            )
        });
        guard_core::find_pinned(&body, "StartOutcome::Failed").unwrap_or_else(|e| {
            panic!(
                "本机「起」没有把 `Failed` 单独接出来（{e}）。\n\
                 三种结局（起了 / 已经在跑 / 起不来）必须各有去处 ——\n\
                 全塞进 `Ok(reason)` 的话，真失败在前端只走 `console.info`，一个 toast 都不弹。"
            )
        });
        assert!(
            body.contains("Err(format!"),
            "本机「起」的失败那一格不回 `Err` —— 那就是把失败说成了成功。"
        );
    }

    /// ★ **接线钉（远端那半）**：`lib.rs` 起每台远端时**真的**把把手注册进来。
    ///
    /// 不注册的后果**不是编译错，是运行时一句「没有把手」** —— 而本模块的单测全都照样绿
    /// （它们不需要真把手）。这正是 F03 那个坑：「模块存在 ≠ 模块被调用」。
    ///
    /// ⚠ 还要钉「把手不是被丢掉的」：原来那处逐字是 `tauri::async_runtime::spawn(async move {…});`
    /// —— **JoinHandle 直接丢**，于是远端流起了就再也停不下来。
    #[test]
    fn the_startup_path_really_registers_remote_handles() {
        let prod = guard_core::production_code(include_str!("lib.rs"));
        guard_core::find_pinned(&prod, "daemon_control::register_remote(").unwrap_or_else(|e| {
            panic!(
                "`lib.rs` 的生产段里没有恰好一处 `register_remote(`（{e}）。\n\
                 ⇒ 远端那侧的起/停在运行时只会回一句「没有这台机的把手」，\n\
                 而本模块的单测**全都照样绿**（它们不需要真把手）。"
            )
        });
    }

    #[test]
    fn an_empty_origin_is_refused_by_every_port() {
        for r in [
            daemon_status("  ".into()).map(|_| ()),
            daemon_start(" ".into()).map(|_| ()),
            daemon_stop("".into()).map(|_| ()),
        ] {
            assert!(
                r.is_err(),
                "空 origin 被放过了 —— 那会造出一档谁都读不到的「全局」，设了没反应且不报错"
            );
        }
    }

    #[test]
    fn an_unknown_remote_origin_says_so_instead_of_pretending() {
        let e = daemon_stop("从没注册过的机器".into()).unwrap_err();
        assert!(
            e.contains("没有这台机的把手"),
            "对不认识的 origin 应当明说没有把手，而不是返回一句像成功的话：{e}"
        );
    }
}
