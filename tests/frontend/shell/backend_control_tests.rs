use super::*;

/// P2s-Y1（acceptor: 机检）：**三个口各只有一条命令，且都吃 `origin`**。
///
/// 防的是「本机一套名字、远端另一套名字」—— 那样两侧就长出两套语义，
/// 正是 `C1` 排除掉的做法（也是 #58 门⑤ 要防的形态）。
#[test]
fn the_three_ports_are_one_command_each_and_all_take_origin() {
    let src = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/backend_control.rs"
    ));
    const PORTS: &[&str] = &["backend_status", "backend_start", "backend_stop"];
    for p in PORTS {
        // `backend_stop` 本机那一支要等（SIGTERM → ≤ 约 10 秒 → 强杀）⇒ 它是 `async`（同步命令跑在主线程上）。
        // 〔INVARIANTS §10〕`backend_start` 本机那一支也要等（起进程 · 连口 · 读 hello · 等绑上口）⇒ 同样 `async`。
        // 〔INVARIANTS §10〕`backend_status` 本机那一支要拿句柄表的锁 ⇒ 也是 `async`，本体住 `backend_status_now`
        //   （命令本身只有一行 `spawn_blocking`）⇒ 那一口切本体那一份。
        guard_core::find_pinned(&src, &format!("pub async fn {p}(origin: String)"))
            .unwrap_or_else(|e| panic!("`{p}` 不是恰好一条 `async` 命令（{e}）"));
        let sig = if *p == "backend_status" {
            "fn backend_status_now(origin: String)".to_string()
        } else {
            format!("pub async fn {p}(origin: String)")
        };
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
        // 把 `backend_stop` 里那一支整个摘掉，**判据照样绿** —— 因为另外两口还各有一处，
        // 计数仍然 >= 2。⇒ 计数型自检在「人群有多个成员」时天然逮不到「某一个成员塌了」。
        let body: String = src[at..]
            .lines()
            .skip(1)
            // ⚠ 收尾行**不写字面量右花括号** —— 本仓有判据用「花括号配平」剥测试段
            // （剥法的唯一住址是 `guard_core::production_source`），源码里多一个孤立的右花括号会让它**提前闭合**（`b'…'` 的字符字面量也算，我第一次「修」时就还带着一个），
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
/// | A6 | `backend_start` 把 `Resolved::Missing{reason}` 当 `Ok(reason)` | 「没内嵌后端」「释放失败」两种**真失败**在前端只走 `console.info`，**一个 toast 都不弹** |
///
/// 本条钉：① 远端那支必须问 `is_finished()`（不是 `is_some()`）；
/// ② 本机那支必须有 `Failed` ⇒ `Err` 那一格（三态各有去处，不靠 `reason` 字符串猜）。
#[test]
fn starting_reports_failure_as_failure_and_finished_streams_as_not_running() {
    let src = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/backend_control.rs"
    ));
    let at = guard_core::find_pinned(&src, "pub async fn backend_start(origin: String)")
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
        // 那句失败的话进了文案表：`Err(format!(…))` 换成了 `Err(copy_text(…))`，都算「回 Err」。
        body.contains("Err(format!") || body.contains("Err(copy_text("),
        "本机「起」的失败那一格不回 `Err` —— 那就是把失败说成了成功。"
    );
}

/// ★ **接线钉（远端那半）**：`lib.rs` 起远端（启动时与热加载同一条路）**真的**把把手注册进来。
///
/// 不注册的后果**不是编译错，是运行时一句「没有把手」** —— 而本模块的单测全都照样绿
/// （它们不需要真把手）。这正是 F03 那个坑：「模块存在 ≠ 模块被调用」。
///
/// ⚠ 还要钉「把手不是被丢掉的」：原来那处逐字是 `tauri::async_runtime::spawn(async move {…});`
/// —— **JoinHandle 直接丢**，于是远端流起了就再也停不下来。
#[test]
fn the_startup_path_really_registers_remote_handles() {
    let prod = guard_core::production_code(include_str!("../../../src/frontend/shell/src/lib.rs"));
    guard_core::find_pinned(&prod, "backend_control::reconcile_remotes(").unwrap_or_else(|e| {
        panic!(
            "`lib.rs` 的生产段里没有恰好一处 `reconcile_remotes(`（{e}）。\n\
                 ⇒ 远端那侧的起/停在运行时只会回一句「没有这台机的把手」，\n\
                 而本模块的单测**全都照样绿**（它们不需要真把手）。"
        )
    });
}

#[test]
fn an_empty_origin_is_refused_by_every_port() {
    for r in [
        tauri::async_runtime::block_on(backend_status("  ".into())).map(|_| ()),
        // `backend_start` 也改成 `async` ⇒ 同样就地跑完它。
        tauri::async_runtime::block_on(backend_start(" ".into())).map(|_| ()),
        // `backend_stop` 改成 `async`（本机那一支要等）⇒ 就地跑完它。
        tauri::async_runtime::block_on(backend_stop("".into())).map(|_| ()),
    ] {
        assert!(
            r.is_err(),
            "空 origin 被放过了 —— 那会造出一档谁都读不到的「全局」，设了没反应且不报错"
        );
    }
}

#[test]
fn an_unknown_remote_origin_says_so_instead_of_pretending() {
    let e = tauri::async_runtime::block_on(backend_stop("从没注册过的机器".into())).unwrap_err();
    assert!(
        e.contains("没有这台机器的记录"),
        "对不认识的 origin 应当明说没有把手，而不是返回一句像成功的话：{e}"
    );
}

// ── P28：给这条源码扫描型守卫立**负对照** ──
//
// 判的不是产品性质，是「**剥法没把我要扫的那一段剥掉**」。
// 被扫的 `src/frontend/shell/src/lib.rs` 今天 2458 行，第一个 `#[cfg(test)]` 在 **59** 行
// ⇒ 便宜近似 `src.split("\n#[cfg(test)]").next()` 把扫描面砍到前 58 行，
// 而本文件要扫的东西在它**后面**（逐针行号写在下面）⇒ 扫描面静默缩水时本文件会**零命中地绿**。
//
// 原语与它买不到什么：`guard_core::assert_stripper_keeps` 的头注。
// 一句话：它不买「针还是那个针」—— 下面这张表必须从本文件真正用的针里抄。

/// ★ 扫描面自检：共享剥法留住了本文件要扫的那几段，而便宜近似留不住。
#[test]
fn the_shared_stripper_keeps_the_registration_this_guard_must_scan() {
    // ⚠ `lib.rs` 的第一个 `#[cfg(test)]` 是 **59** 行那句 `mod guard_support;` ——
    //    便宜近似只留 58 行 ⇒ 本文件那条 `find_pinned` 会报 0 处，
    //    而它的诊断说的是「远端那侧的起/停只会回一句没有这台机的把手」——
    //    **一句指向错地方的假诊断**。⇒ 逐针钉住扫描面本身。
    guard_core::assert_stripper_keeps(
        "backend_control_tests · lib.rs",
        include_str!("../../../src/frontend/shell/src/lib.rs"),
        &["backend_control::reconcile_remotes("],
    );
}

/// 机器表热加载：新来的那台起流、拿掉 / 停用的那台断流、连接参数变了的重起，其余一台不碰。
#[test]
fn reconcile_starts_new_stops_gone_restarts_changed_and_leaves_the_rest() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// 流被断（任务被 abort ⇒ future 被丢）时把旗子立起来。
    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let starts: Arc<Mutex<HashMap<String, usize>>> = Arc::new(Mutex::new(HashMap::new()));
    let dropped: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let want = |origin: &str, key: &str| {
        let (o, s, d) = (origin.to_string(), starts.clone(), dropped.clone());
        WantedRemote {
            origin: origin.to_string(),
            cfg_key: key.to_string(),
            respawn: Box::new(move || {
                *s.lock().unwrap().entry(o.clone()).or_insert(0) += 1;
                let flag = Arc::new(AtomicBool::new(false));
                d.lock().unwrap().insert(o.clone(), flag.clone());
                tauri::async_runtime::spawn(async move {
                    let _guard = Dropped(flag);
                    std::future::pending::<()>().await;
                })
            }),
        }
    };
    let settle = || {
        tauri::async_runtime::block_on(async {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await
        })
    };
    let count = |o: &str| starts.lock().unwrap().get(o).copied().unwrap_or(0);
    let was_dropped = |o: &str| dropped.lock().unwrap()[o].load(Ordering::SeqCst);

    let r = reconcile_remotes(vec![
        want("rc-a", "1"),
        want("rc-b", "1"),
        want("rc-c", "1"),
    ]);
    assert_eq!(r.started, ["rc-a", "rc-b", "rc-c"]);
    settle();

    // 停用 b（不在要连的里了）· c 改了连接参数 · a 原样。
    let r = reconcile_remotes(vec![want("rc-a", "1"), want("rc-c", "2")]);
    settle();
    assert_eq!(
        r,
        Reconciled {
            started: vec![],
            stopped: vec!["rc-b".into()],
            restarted: vec!["rc-c".into()]
        }
    );
    assert!(was_dropped("rc-b"), "停用的那台流没断");
    assert!(
        !backend_machines().unwrap().contains(&"rc-b".to_string()),
        "停用的那台还在注册表里"
    );
    assert_eq!(count("rc-c"), 2, "改了参数的那台没按新参数重起");
    assert_eq!(count("rc-a"), 1, "没动的那台被重起了");
    assert!(!was_dropped("rc-a"), "没动的那台流被断了");

    // 再开 b ⇒ 起一条新的；a · c 不动。
    let r = reconcile_remotes(vec![
        want("rc-a", "1"),
        want("rc-b", "1"),
        want("rc-c", "2"),
    ]);
    assert_eq!(r.started, ["rc-b"]);
    assert!(r.stopped.is_empty() && r.restarted.is_empty());
    assert_eq!((count("rc-a"), count("rc-b"), count("rc-c")), (1, 2, 2));
    let _ = reconcile_remotes(vec![]);
}
