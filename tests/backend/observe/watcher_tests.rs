/// ★ **事件分派不许有兜底臂**。
///
/// # 它钉的是一个「没人盯的前提」，不是一个缺陷
///
/// 本文件 87 条判据里，七个 `WatchEvent` 变体**每一个都被构造过** ——
/// 抽样时逐个数过（`Notify` 3 · `TmuxServerGone` 4 · `PidDied` 6 ·
/// `TmuxObserved` 4 · `TmuxProbeDue` 5 · `Poke` 5 · `Shutdown` 4）。
/// 而「加第八个变体时会不会被漏掉」靠的**不是**这些判据，
/// 是**编译器**：顶层分派是一条没有兜底臂的 `match`，少一个变体就编不过。
///
/// ⚠ 那是一个**前提**，而且今天没人盯着它：谁在那条 `match` 上加一条兜底臂，
/// 穷尽性当场消失，新变体从此被静默吞掉 —— 而**所有既有判据仍然全绿**
///（它们各测各的变体，没有一条会因为「多了一个没人处理的变体」而红）。
/// 本会话反复量到的正是这个形状：**一条纪律的成立依赖另一条，而那条依赖没人盯。**
///
/// ⇒ 本条只做一件事：钉住那条 `match` 里**没有兜底臂**。
#[test]
fn the_event_dispatch_has_no_catch_all_arm() {
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/observe/watcher.rs"
    ));
    let anchor = "WatchEvent::Notify(Ok(events)) =>";
    let at = prod
        .find(anchor)
        .expect("找不到事件分派的锚点臂 —— 分派改形了，本条要跟着改");
    // 从锚点臂往后取到分派块结束：按缩进找同级臂，遇到缩进更浅的行即出块。
    let indent = prod[..at].rfind('\n').map_or(0, |k| at - k - 1);
    let mut arms: Vec<String> = Vec::new();
    for line in prod[at - indent..].lines() {
        let cur = line.len() - line.trim_start().len();
        if line.trim().is_empty() {
            continue;
        }
        if cur < indent {
            break;
        }
        if cur == indent {
            let head: String = line.trim_start().chars().take(24).collect();
            if head.starts_with("WatchEvent::") || head.starts_with('_') {
                arms.push(head);
            }
        }
    }
    // 抽取器自检：抠不到臂 ⇒ 下面那条会零命中地绿。
    assert!(
        arms.len() >= 6,
        "只抠到 {} 条分派臂（08-06 实测 7）—— 抽取坏了，本条此刻是空转的：{arms:?}",
        arms.len()
    );
    assert!(
        !arms.iter().any(|a| a.starts_with('_')),
        "事件分派里出现了兜底臂：{arms:?}\n\
             ⚠ 它一加上，`match` 的穷尽性就没了 —— 新增的 `WatchEvent` 变体会被**静默吞掉**，\n\
             而本文件既有的判据**不会有一条因此变红**（它们各测各的变体）。\n\
             backend 的判活全靠这七路信号；被吞掉的那一路不会报错，只会「什么都不发生」。\n\
             ⇒ 要新增变体就在这里显式处理它；确实无事可做也请写成具名臂加一句注释。"
    );
}
use super::*;

// ---------- P2（zero-poll-liveness）：pidfd 判活 ----------

/// 起一个**假**进程当靶子。**绝不起真实已认证的 claude/codex**——这里只要一个
/// 「活着、能被杀、pid 可拿」的进程，`sleep` 足够。
fn spawn_target() -> std::process::Child {
    std::process::Command::new("sleep")
        .arg("30")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn sleep")
}

/// `pidfd_open` 的基本性质：自己开得开；不存在的 pid 开不开。
///
/// 不存在的 pid 取一个刚退出并已回收的子进程 pid——比硬编码一个大数可靠
/// （大数也可能恰好被占）。
// U4a：本测试直接调 `pidfd_open`，那是 Linux-only 原语。
#[cfg(target_os = "linux")]
#[test]
fn pidfd_open_works_for_self_and_fails_for_dead_pid() {
    assert!(
        pidfd_open(std::process::id()).is_ok(),
        "自己的 pid 必须开得开"
    );

    let mut child = spawn_target();
    let dead_pid = child.id();
    child.kill().expect("kill");
    child.wait().expect("reap"); // 回收，pid 彻底消失
    let err = pidfd_open(dead_pid).expect_err("已回收的 pid 不该开得开");
    assert_eq!(
        err.raw_os_error(),
        Some(libc::ESRCH),
        "应是 ESRCH，实得 {err:?}"
    );
}

/// ★ **双向验收（本功能 DoD 的硬项）**：杀 → 事件真的到；不杀 → 事件不到。
///
/// 只测"杀了会到"是不够的——一个恒发 `PidDied` 的实现也能让那半边绿。
/// 反方向那半边才是钉住"事件由**目标进程退出**驱动"的那条。
///
/// 测试里用 `recv_timeout` 是可以的：**要求零定时器的是生产循环**，不是测试。
#[test]
fn pidfd_watcher_fires_on_death_and_stays_silent_while_alive() {
    let key = PathBuf::from("/tmp/ccm-p2-fixture/1234.json");
    let mut child = spawn_target();
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
    // expected_start=None ⇒ 判据 2 退化成存在性（`is_same_live_process` 的 `_ => true` 臂）。
    spawn_pid_watcher(PidWatchTarget::Session { key: key.clone() }, pid, None, tx);

    // —— 反方向：目标还活着 ⇒ 不该有任何事件 ——
    match rx.recv_timeout(Duration::from_millis(400)) {
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        other => panic!("目标活着时不该有事件，实得 {:?}", other.is_ok()),
    }

    // —— 正方向：杀掉 ⇒ 内核唤醒 poll ⇒ 事件到 ——
    child.kill().expect("kill");
    let got = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("杀掉后必须收到 PidDied（超时 = pidfd 没被内核唤醒）");
    match got {
        WatchEvent::PidDied { key: k, pid: p } => {
            assert_eq!(k, key, "带回的 key 必须是挂看守时那个");
            assert_eq!(
                p, pid,
                "带回的 pid 必须是挂看守时那个（消费侧靠它挡陈旧唤醒）"
            );
        }
        _ => panic!("期望 PidDied"),
    }
    child.wait().expect("reap");
}

/// 判据 1：`pidfd_open` 失败（目标已不在）⇒ **立刻**发 `PidDied`，不静默丢。
#[test]
fn pidfd_watcher_reports_dead_when_open_fails() {
    let mut child = spawn_target();
    let pid = child.id();
    child.kill().expect("kill");
    child.wait().expect("reap");

    let key = PathBuf::from("/tmp/ccm-p2-fixture/dead.json");
    let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
    spawn_pid_watcher(PidWatchTarget::Session { key: key.clone() }, pid, None, tx);
    match rx
        .recv_timeout(Duration::from_secs(2))
        .expect("open 失败必须立刻报死")
    {
        WatchEvent::PidDied { key: k, pid: p } => {
            assert_eq!((k, p), (key, pid));
        }
        _ => panic!("期望 PidDied"),
    }
}

/// ★ 判据 2：**PID 复用**（open 之后身份复核不符）⇒ 当死。
///
/// 造法：给一个**活着**的进程配一个**对不上**的 procStart 基线。真实场景里这等价于
/// 「读 pidfile 拿到 (pid, start) → 那个进程死了 → 别人占了同一个 pid → 我们开到了冒名者」。
/// 这一格是原先那套 procStart 启发式的全部去处：从"每 2s 复查"降成"挂看守时校验一次"。
#[test]
fn pidfd_watcher_rejects_reused_pid_via_start_mismatch() {
    let mut child = spawn_target();
    let pid = child.id();
    let real = proc_starttime(pid);
    assert!(real.is_some(), "本机应能读到 /proc/<pid>/stat 的 starttime");
    // 刻意错开：真值 + 1 ⇒ `is_same_live_process(true, Some(a), Some(b))` 的 a != b 臂。
    let bogus = real.map(|t| t + 1);

    let key = PathBuf::from("/tmp/ccm-p2-fixture/reused.json");
    let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
    spawn_pid_watcher(PidWatchTarget::Session { key: key.clone() }, pid, bogus, tx);
    match rx
        .recv_timeout(Duration::from_secs(2))
        .expect("基线不符必须立刻报死（否则会把冒名者当成原会话一直判活）")
    {
        WatchEvent::PidDied { key: k, pid: p } => {
            assert_eq!((k, p), (key, pid));
        }
        _ => panic!("期望 PidDied"),
    }
    child.kill().expect("kill");
    child.wait().expect("reap");
}

/// `arm_pid_watcher` 幂等：同一 `(pidfile, pid)` 挂两次只起一条看守。
/// 按**对**而不是按路径存，所以同路径换了 pid 要能重新挂——两条都测。
#[test]
fn arm_pid_watcher_is_idempotent_per_pidfile_and_pid() {
    let mut st = ReaderState::new(PathBuf::from("/tmp/ccm-p2-proj"), false, false);
    let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
    st.events_tx = Some(tx);
    let key = PathBuf::from("/tmp/ccm-p2-fixture/idem.json");

    // 用一个已死的 pid：每次成功挂载都会立刻投一条 PidDied ⇒ 收到几条 = 挂了几次。
    let mut child = spawn_target();
    let dead = child.id();
    child.kill().expect("kill");
    child.wait().expect("reap");

    arm_pid_watcher(&key, dead, None, &mut st);
    arm_pid_watcher(&key, dead, None, &mut st); // 同对 ⇒ 应被跳过
    assert!(
        rx.recv_timeout(Duration::from_secs(2)).is_ok(),
        "第一次必须挂上"
    );
    match rx.recv_timeout(Duration::from_millis(400)) {
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        _ => panic!("同一 (pidfile, pid) 不该挂第二条看守"),
    }

    // 同路径换 pid ⇒ 必须重新挂（/clear 原地换 sid、PID 复用写同路径）
    let mut child2 = spawn_target();
    let dead2 = child2.id();
    child2.kill().expect("kill");
    child2.wait().expect("reap");
    arm_pid_watcher(&key, dead2, None, &mut st);
    assert!(
        rx.recv_timeout(Duration::from_secs(2)).is_ok(),
        "同路径换 pid 必须重新挂看守"
    );
}

/// ★★ **PID 复用：同路径、同 pid、不同进程实例，必须重新挂**〔audit-0805 F11 / 报告 I-7〕。
///
/// # 上面那条测的是「换 pid」，而这一格是「**没换 pid**」
///
/// `spawn_pid_watcher` 的头注声称要处理「PID 复用写同路径」，而去重键此前是
/// `(路径, pid)` 两元 —— PID 被复用时**这两元都没变**，于是落进去重、**不再挂看守**。
/// **头注声称能处理的那格，恰是它处理不了的那格。**
///
/// 区分进程实例的东西（`starttime`）本来就在参数里，只是没进键。F11 把它加进去了。
#[test]
fn a_recycled_pid_at_the_same_path_gets_a_fresh_watcher() {
    let mut st = ReaderState::new(PathBuf::from("/tmp/ccm-f11-proj"), false, false);
    let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
    st.events_tx = Some(tx);
    let key = PathBuf::from("/tmp/ccm-f11-fixture/reuse.json");

    let mut child = spawn_target();
    let dead = child.id();
    child.kill().expect("kill");
    child.wait().expect("reap");

    // 同一个 pid、同一个路径，但**两个不同的 starttime** = PID 被复用了。
    arm_pid_watcher(&key, dead, Some(1_000), &mut st);
    assert!(
        rx.recv_timeout(Duration::from_secs(2)).is_ok(),
        "抽取器自检：第一次都没挂上，本条后面的判定无意义"
    );
    arm_pid_watcher(&key, dead, Some(2_000), &mut st);
    assert!(
        rx.recv_timeout(Duration::from_secs(2)).is_ok(),
        "★ 同路径 + 同 pid + **不同 starttime** 没有重新挂看守。\n\
             那正是 PID 复用：pid 数字被回收给了另一个进程，而 (路径, pid) 两元键看不出区别\n\
             ⇒ 新进程死了没人报，会话永远停在「活着」。\n\
             `spawn_pid_watcher` 的头注声称处理这一格 —— 别让它继续说假话。"
    );

    // 反向：**同 starttime** 仍然要去重（防把上面写成「每次都挂」）。
    arm_pid_watcher(&key, dead, Some(2_000), &mut st);
    match rx.recv_timeout(Duration::from_millis(400)) {
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        _ => panic!("同 (路径, pid, starttime) 三元不该挂第二条看守 —— 去重被写坏了"),
    }
}

/// `events_tx` 为 `None`（单元测试默认）时 `arm_pid_watcher` 什么都不做——
/// 11 处 `ReaderState::new` 因此不必改签名。
#[test]
fn arm_pid_watcher_is_a_noop_without_sender() {
    let mut st = ReaderState::new(PathBuf::from("/tmp/ccm-p2-proj"), false, false);
    arm_pid_watcher(&PathBuf::from("/x/1.json"), 1, None, &mut st);
    assert!(
        st.pid_watched.is_empty(),
        "没有发送端时不该登记，也不该起线程"
    );
}

/// ★ 守卫：**事件 channel 必须建在 Phase 1 初始扫描之前**。
///
/// 为什么需要一条扫源码的守卫而不是一条行为测试：`watch_loop` 要真文件系统 + notify +
/// 多线程，单测碰不到；而这个顺序错了的后果**极其安静**——`process_session_added` 在
/// Phase 1 里被调用时 `events_tx` 还是 `None` ⇒ `arm_pid_watcher` 直接 return ⇒
/// **backend 启动时就活着的会话一个 pidfd 看守都没有**，永远判不出死。而 P2 之前那条
/// 2s 判活轮询是覆盖它们的 ⇒ 是回归。
///
/// **P2 初版真犯了这个错**（channel 建在 "Phase 2: live watch" 处），是被 clippy 的
/// 「field `start` is never read」间接暴露出来的——不是被任何测试抓到的。所以补这条。
/// ⚠⚠ **08-08 订正：这条原来比的是一行注释的位置。**
///
/// 原实现拿 `// --- Phase 1: synchronous initial scan. ---` 当「初始扫描」的锚点。
/// 实测：把**真扫描那一块**（`if sessions.is_dir()` 那段）搬到 `events_tx` 注入之前、
/// **注释原地不动**，本条照样绿 —— 而那正是它自陈要挡的那个静默回归。
/// ⇒ 「文本顺序 ≠ 执行顺序」这一族里还有更基础的一层：**判据得先比对代码，
/// 而不是比对描述代码的那句话**（本会话第四次撞上同一形状）。
///
/// 另一半也一起修：注释若被重排/改写，原实现会红在一个**与语义无关**的位置上
/// （把注入挪到注释之后、真扫描之前，语义完全正确却会红）。
/// 现在锚在 `WalkDir::new(&sessions)`（生产段唯一一处，扫描真正开始的地方）。
/// 扫描抽成 `initial_session_scan` 之后，锚点换成 `watch_loop` 里它的调用点；10-07 起再经 `arm_then_scan`（见函数体）。
#[test]
fn events_channel_is_created_before_the_initial_scan() {
    let src = crate::guard_support::production_code(include_str!(
        "../../../src/backend/observe/watcher.rs"
    ));
    let tx_at = src
        .find("state.events_tx = Some(events_tx.clone());")
        .expect("找不到 events_tx 注入点——守卫锚点漂了，先修锚点别改断言");
    // 扫描那一块抽成了 `initial_session_scan`（为了「清单报完了」那一帧的位置可验），
    //   它的**定义**住在文件后段 ⇒ 锚点从扫描本体（`WalkDir::new(&sessions)`）换到 `watch_loop` 里的**调用点**
    //   —— 那才是执行顺序上「扫描真正开始」的地方。
    // 10-07 起初扫经 `arm_then_scan`（先挂耳朵、再扫）⇒ `watch_loop` 里的调用点换成它。
    let scan_at = src.find("arm_then_scan(&sessions").expect(
        "找不到初始扫描的锚点（`arm_then_scan(&sessions` 调用点）——扫描改写了就把本条一起改",
    );
    // 锚点唯一性：两个都必须**恰好一处**，否则「谁在前」比的可能是别处那一份。
    assert_eq!(
        src.matches("arm_then_scan(&sessions").count(),
        1,
        "初始扫描的锚点在生产段里不止一处 —— 本条会比到别的那一份上去"
    );
    assert!(
        tx_at < scan_at,
        "events_tx 必须在 Phase 1 初始扫描**之前**注入，否则启动时已在跑的会话拿不到 \
             pidfd 看守（静默回归：那些会话永远判不出死）。实测 tx@{tx_at} scan@{scan_at}"
    );
    // 反向自检：断言的是"两个锚点都找到了 + 源码真读进来了"，
    // 不是"命中数 < N"——阈值不能挂在被检查的量上。
    assert!(
        src.len() > 1000,
        "include_str! 没读到源码，上面的断言是空转"
    );
}

/// **P5：ticker 已删，但「首轮立即发一拍」这条行为必须留着** ——
/// 这正是删 ticker 时差点顺手删掉的东西：monitor 一连上就该拿到 tmux 状态，
/// 否则空闲机器上要等到第一个 hook 触发才探（可能是**永远**）。
/// 本测试从「ticker 首拍」改判为「一次性初探真的发了一拍」，**性质没放松**。
#[test]
fn initial_probe_fires_once() {
    let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
    initial_tmux_probe(&tx);
    match rx.try_recv().expect("初探必须立刻发一拍") {
        WatchEvent::TmuxProbeDue => {}
        _ => panic!("期望 TmuxProbeDue"),
    }
    // 且**只发一拍** —— 它不是节拍器。
    assert!(
        rx.try_recv().is_err(),
        "初探不该发第二拍（那就又成定时器了）"
    );
}

/// P5：`WatcherPoke::shutdown()` 发的是 `Shutdown` 而不是别的。
/// 这条漏了不会红任何别的测试（进程退出时 reader 线程随之消亡），所以单独钉。
#[test]
fn poke_shutdown_sends_shutdown_variant() {
    let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
    WatcherPoke(tx).shutdown();
    assert!(matches!(rx.try_recv(), Ok(WatchEvent::Shutdown)));
}

// ---------- P4（zero-poll-liveness）：外部 poke ⇒ 立刻重探 ----------

/// P4：`Poke` 与 `TmuxProbeDue` 必须走**同一条**处理路径 —— 事件驱动的那一拍和
/// 定时器那一拍，除了来源不同，后续行为应当逐字相同。
///
/// 这条扫源码而不是跑循环：`watch_loop` 要真 spawn 线程 + 真跑 `tmux ls`，
/// 在单测里不可控。**范围窄于性质，如实记**：它钉的是「两个变体在同一个 match 臂上」，
/// 钉不住「那个臂里的代码是对的」——后者由 P4 的真机验收（hook 装上之后）覆盖。
#[test]
fn poke_shares_the_probe_arm_with_the_ticker() {
    // ★★ 判据**运行时拼**且**只扫生产段** —— 初版两样都没做，于是变异验收时
    // 「把 Poke 拆成独立臂」**没有变红**：断言那串字面量就在本测试自己的源码里，
    // `contains` 恒真 ⇒ 这条守卫是**安慰剂**。是变异（而不是审读）把它揪出来的。
    let me = include_str!("../../../src/backend/observe/watcher.rs");
    // 🔴 〔步 7c 剖分 2026-09-19〕
    //    **手写的那条切法删了，改走唯一住址 `production_code`。**
    //    原来是 `me.find("\n#[cfg(test)]\nmod tests")` 手工截断 —— 那是剥法的**第二份**，
    //    而剖分之后 `watcher.rs` 里那三行桩中间多了一行 `#[path = "…"]`
    //    ⇒ 那根针**恒不命中** ⇒ `prod` 退化成全文，下面数的就是全文。
    //    它自己的反空真（`prod.len() < me.len()`）当场红 —— 红得对。
    let prod = crate::guard_support::production_code(me);
    let prod = prod.as_str();
    let arm = format!("WatchEvent::TmuxProbeDue {} WatchEvent::Poke =>", "|");
    assert!(
        prod.contains(&arm),
        "P4：Poke 必须与 TmuxProbeDue 共用同一个 match 臂（否则两条路会各自漂）"
    );
    // 反空真（换了说法）：原来断的是「剥掉了东西」（`prod.len() < me.len()`），
    // 而剖分之后那份生产文件里**本来就没有测试段可剥** ⇒ 那句话恒假。
    // 换成两句今天成立、而且仍然会红的：① 语料没塌；② 生产段里一点测试代码都没有
    // （后者比原来那句强：原来只要求「剥掉了一些」，剥漏了一半照样过）。
    assert!(
        prod.len() > 1000,
        "只读到 {} 字节生产段 —— 语料塌了，这条断言在空转",
        prod.len()
    );
    crate::guard_support::assert_no_test_code("observe/watcher.rs", prod);
}

/// ★ S0：pidfile **绑的 sid 变了**才重探 tmux —— 不是「有 .json 事件就探」。
///
/// 为什么这条区分是必须的：CC 在**每次状态转换**时都会重写 `sessions/<PID>.json`
/// （远端红绿灯就靠它，见 `process_session_added` 里 F27 那段）。拿「有事件」当触发器，
/// 等于让 tmux 探测跟着对话节奏跑 —— 那是**变相轮询**，把 P5 好不容易拆掉的定时器
/// 用另一种形式装回来。
///
/// 同 `poke_shares_the_probe_arm_with_the_ticker`：扫**生产段**源码（`watch_loop` 要真
/// spawn 线程 + 真跑 `tmux ls`，单测里跑不动）。**范围窄于性质，如实记**：它钉的是
/// 「触发条件写的是 sid 前后比对」，钉不住「比对结果被正确用了」。
#[test]
fn tmux_reprobe_triggers_on_sid_drift_not_on_every_json_event() {
    let me = include_str!("../../../src/backend/observe/watcher.rs");
    // 🔴 〔步 7c 剖分 2026-09-19〕
    //    **手写的那条切法删了，改走唯一住址 `production_code`。**
    //    原来是 `me.find("\n#[cfg(test)]\nmod tests")` 手工截断 —— 那是剥法的**第二份**，
    //    而剖分之后 `watcher.rs` 里那三行桩中间多了一行 `#[path = "…"]`
    //    ⇒ 那根针**恒不命中** ⇒ `prod` 退化成全文，下面数的就是全文。
    //    它自己的反空真（`prod.len() < me.len()`）当场红 —— 红得对。
    let prod = crate::guard_support::production_code(me);
    let prod = prod.as_str();
    // 反空真（换了说法）：原来断的是「剥掉了东西」（`prod.len() < me.len()`），
    // 而剖分之后那份生产文件里**本来就没有测试段可剥** ⇒ 那句话恒假。
    // 换成两句今天成立、而且仍然会红的：① 语料没塌；② 生产段里一点测试代码都没有
    // （后者比原来那句强：原来只要求「剥掉了一些」，剥漏了一半照样过）。
    assert!(
        prod.len() > 1000,
        "只读到 {} 字节生产段 —— 语料塌了，这条断言在空转",
        prod.len()
    );
    crate::guard_support::assert_no_test_code("observe/watcher.rs", prod);
    // 触发器 = 处理前后拿同一个 key 的 sid 比一次。判据运行时拼，避免本测试自己的
    // 源码把 `contains` 喂成恒真（那正是 P4 那条守卫初版栽的跟头）。
    let before = format!(
        "let sid_before = state.sessions.get(&key){}",
        ".map(|e| e.sid.clone());"
    );
    assert!(
        prod.contains(&before),
        "S0：处理 session json 之前必须先记下该 key 当前绑的 sid"
    );
    let cmp = format!(
        "if state.sessions.get(&key).map(|e| e.sid.clone()) {} sid_before {{",
        "!="
    );
    assert!(
        prod.contains(&cmp),
        "S0：触发条件必须是「sid 前后不同」，不是「收到了 .json 事件」"
    );
}

/// ★ P4：`WatcherPoke` 是**窄**句柄 —— 外部只能催重探，不能伪造带载荷的内部事件。
///
/// 为什么要钉：`main` 的 SIGUSR1 流拿到的若是 `Sender<WatchEvent>` 本身，它就能发
/// `PidDied{key,pid}` / `TmuxObserved(..)` —— 那等于把「谁能宣布一个会话死了」这件事
/// 从 watcher 内部漏到了进程边界上（signal 处理器是最不该有这个权力的地方）。
#[test]
fn watcher_poke_is_a_narrow_handle() {
    let me = include_str!("../../../src/backend/observe/watcher.rs");
    // 结构性判据：poke 句柄只暴露一个无参方法，且它只发 Poke 这一个变体。
    assert!(
        me.contains("pub fn poke(&self)"),
        "WatcherPoke 应当只暴露 poke()"
    );
    assert!(
        me.contains("self.0.send(WatchEvent::Poke)"),
        "poke() 只许发 Poke 变体"
    );
    // 反向：句柄字段不许是 pub（否则外部直接拿 sender，窄类型就白设了）。
    //
    // ★ 判据**在运行时拼出来**，绝不把它当字面量写进源码 —— 否则这条 `!contains`
    // 会被**本测试自己的那行字面量**命中而恒红（初版就是这么栽的：源码里字段并不是
    // pub，测试却红了，因为 `me` 里找到了断言自己那串）。
    let forbidden = format!("pub struct WatcherPoke({} ", "pub");
    assert!(
        !me.contains(&forbidden),
        "WatcherPoke 的字段不许 pub —— 那就绕开了窄接口"
    );
    assert!(me.len() > 1000, "include_str! 没读到源码，上面的断言是空转");
}

/// P4：**channel 仍然只有一条**（账本第 1 行：不许再开第二条）。
/// `spawn` 把 channel 上提到自己这里造，是为了能在线程起来前交出 poke 句柄；
/// 上提**不等于**新增 —— `watch_loop` 收的是同一条的 receiver。
#[test]
fn still_exactly_one_event_channel() {
    // ★ 只数**生产代码**：测试自己造了 6 条同型 channel（各自的夹具），把它们算进来
    // 这条断言就恒红（初版实测 7 处）。做法与 `guard_core::production_source`
    // 同源，这里用够用的简化版：本文件只有**一个**测试模块，且在文件末尾。
    //
    // ★★ **锚点必须避开自指**，这个坑本功能连踩三次：
    //   ① 用 `rfind("#[cfg(test)]")` —— 找到的是**本测试源码里那行字面量**（在模块标记
    //      之后）⇒ 几乎什么都没剥掉，实测仍数出 6。
    //   ② 下面那条「字段不许 pub」的 `!contains` 判据写成字面量 —— 被自己命中而恒红。
    //      （连**解释这个坑的注释**逐字引用那串时也会再次触发，实测第四次才收干净。）
    // 现在锚 `\n#[cfg(test)]\nmod tests`：源码里这串是**转义写法**（反斜杠 + n 两个字符），
    // 与真正的换行不相等 ⇒ 不会匹配到本行自己。
    let me = include_str!("../../../src/backend/observe/watcher.rs");
    // 🔴 〔步 7c 剖分 2026-09-19〕
    //    **手写的那条切法删了，改走唯一住址 `production_code`。**
    //    原来是 `me.find("\n#[cfg(test)]\nmod tests")` 手工截断 —— 那是剥法的**第二份**，
    //    而剖分之后 `watcher.rs` 里那三行桩中间多了一行 `#[path = "…"]`
    //    ⇒ 那根针**恒不命中** ⇒ `prod` 退化成全文，下面数的就是全文。
    //    它自己的反空真（`prod.len() < me.len()`）当场红 —— 红得对。
    let prod = crate::guard_support::production_code(me);
    let prod = prod.as_str();
    let n = prod
        .matches("std::sync::mpsc::channel::<WatchEvent>()")
        .count();
    assert_eq!(
        n, 1,
        "生产代码里事件 channel 的创建点应当恰好 1 处（实得 {n}）——账本第 1 行不许开第二条"
    );
    // 反向自检：真剥掉了测试段（否则上面数的是全文）。
    // 反空真（换了说法）：原来断的是「剥掉了东西」（`prod.len() < me.len()`），
    // 而剖分之后那份生产文件里**本来就没有测试段可剥** ⇒ 那句话恒假。
    // 换成两句今天成立、而且仍然会红的：① 语料没塌；② 生产段里一点测试代码都没有
    // （后者比原来那句强：原来只要求「剥掉了一些」，剥漏了一半照样过）。
    assert!(
        prod.len() > 1000,
        "只读到 {} 字节生产段 —— 语料塌了，这条断言在空转",
        prod.len()
    );
    crate::guard_support::assert_no_test_code("observe/watcher.rs", prod);
}

// ---------- P3（zero-poll-liveness）：tmux server 生 / 死 / 复活 ----------

/// pidfd 看守的**目标**决定醒了发哪个事件——一份实现服务两种目标
/// （全 crate 只有一处 pidfd 的 unsafe）。
#[test]
fn pid_watch_target_maps_to_the_right_death_event() {
    let key = PathBuf::from("/x/7.json");
    match (PidWatchTarget::Session { key: key.clone() }).death_event(7) {
        WatchEvent::PidDied { key: k, pid } => assert_eq!((k, pid), (key, 7)),
        _ => panic!("Session 目标必须发 PidDied"),
    }
    match PidWatchTarget::TmuxServer.death_event(9) {
        WatchEvent::TmuxServerGone { pid } => assert_eq!(pid, 9),
        _ => panic!("TmuxServer 目标必须发 TmuxServerGone"),
    }
}

/// ★ pidfd 用在**真 tmux server 进程**上（不只是 `sleep`）：双向验收。
///
/// 隔离 socket（无 `-L` 一律不跑——本测试自己带 `-L`）。这一格 P2 没覆盖：
/// P2 只测了会话进程，P0-③ 只测了 cgroup 拓扑。
///
/// **无 tmux 就硬失败而不是静默跳过**——静默 SKIP 是 gate-integrity 在治的那个病。
/// 本 crate 的 CI job 跑在 ubuntu-latest，tmux 是标配；真缺了应当看见红。
// U4a：本测试清理 Linux 的 `/tmp/tmux-<uid>/` socket 目录（`libc::getuid`），
// 是 Linux-only 的夹具 —— 少这个门会让跨 target check 红。
#[cfg(target_os = "linux")]
#[test]
fn pidfd_watches_a_real_tmux_server_and_stays_silent_while_it_lives() {
    let sock = format!("ccmP3-{}", std::process::id());
    let tmux = |args: &[&str]| -> std::process::Output {
        std::process::Command::new("tmux")
            .args(["-L", &sock])
            .args(args)
            .output()
            .expect("tmux 不可执行——本测试要求环境有 tmux（刻意不静默跳过）")
    };
    // -f /dev/null：不读用户的 ~/.tmux.conf（隔离）
    let out = std::process::Command::new("tmux")
        .args([
            "-f",
            "/dev/null",
            "-L",
            &sock,
            "new-session",
            "-d",
            "-s",
            "p3",
            "sh",
        ])
        .output()
        .expect("tmux 不可执行——本测试要求环境有 tmux");
    assert!(
        out.status.success(),
        "隔离 socket 上建会话失败: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let pid_out = tmux(&["display-message", "-p", "#{pid}"]);
    let pid: u32 = String::from_utf8_lossy(&pid_out.stdout)
        .trim()
        .parse()
        .expect("拿 server pid");

    let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
    spawn_pid_watcher(PidWatchTarget::TmuxServer, pid, None, tx);

    // 反方向：server 还活着 ⇒ 不该有事件
    match rx.recv_timeout(Duration::from_millis(400)) {
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        other => panic!("server 活着时不该有事件（ok={}）", other.is_ok()),
    }

    // 正方向：杀掉**这个隔离 socket 上的** server ⇒ pidfd 醒
    let _ = tmux(&["kill-server"]);
    match rx
        .recv_timeout(Duration::from_secs(5))
        .expect("server 退出后必须收到 TmuxServerGone")
    {
        WatchEvent::TmuxServerGone { pid: p } => assert_eq!(p, pid),
        _ => panic!("期望 TmuxServerGone"),
    }
    let _ = std::fs::remove_file(format!("/tmp/tmux-{}/{sock}", unsafe { libc::getuid() }));
}

/// Build a byte buffer from JSONL lines joined with `\n` and a trailing one.
fn jsonl(lines: &[&str]) -> Vec<u8> {
    let mut s = String::new();
    for l in lines {
        s.push_str(l);
        s.push('\n');
    }
    s.into_bytes()
}

const KEY: &str = "/some/session.jsonl";

#[test]
fn appending_lines_advances_offset_and_seq_monotonically() {
    let mut seqs = SeqCounter::new();

    let first = jsonl(&[r#"{"a":1}"#, r#"{"a":2}"#]);
    let (out, cur) = read_new_lines(&first, ReadCursor::default(), KEY, &mut seqs);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].seq, 0);
    assert_eq!(out[1].seq, 1);
    assert_eq!(cur.consumed, first.len() as u64);
    assert_eq!(cur.seen_len, first.len() as u64);

    // Append two more lines (same prefix bytes, longer file).
    let mut second = first.clone();
    second.extend_from_slice(jsonl(&[r#"{"a":3}"#, r#"{"a":4}"#]).as_slice());
    let (out2, cur2) = read_new_lines(&second, cur, KEY, &mut seqs);
    assert_eq!(out2.len(), 2, "only the newly-appended lines come back");
    assert_eq!(out2[0].seq, 2);
    assert_eq!(out2[1].seq, 3);
    assert_eq!(cur2.consumed, second.len() as u64);
    assert_eq!(out2[0].raw, r#"{"a":3}"#);
}

#[test]
fn byte_offset_matches_aterm_lineframer() {
    // backend-01（gap#2）：Line.byte_offset **逐字节对齐 aterm `LineFramer.endOffset`**——计 CRLF 的 `\r`、
    // 含 `\n`、残行不计、在**原始字节**上算（非解码后串）。移植自 aterm LineFramerTest 的关键语料。
    let mut seqs = SeqCounter::new();
    // aterm feedFramedCountsCrlfAndMultibyteRawBytes: "你\r\nx\n" → endOffset [5,7]
    // 你=3B + \r + \n = 5；x + \n = 2 → 累计 7。raw 剥 \r/\n。
    let (out, cur) = read_new_lines(
        "你\r\nx\n".as_bytes(),
        ReadCursor::default(),
        KEY,
        &mut seqs,
    );
    assert_eq!(out.len(), 2);
    assert_eq!((out[0].raw.as_str(), out[0].byte_offset), ("你", 5));
    assert_eq!((out[1].raw.as_str(), out[1].byte_offset), ("x", 7));
    assert_eq!(cur.consumed, 7);

    // 无 CRLF 累计：jsonl(["ab","cde"]) = "ab\ncde\n" → [3, 7]。
    let mut s2 = SeqCounter::new();
    let (o2, _) = read_new_lines(&jsonl(&["ab", "cde"]), ReadCursor::default(), KEY, &mut s2);
    assert_eq!((o2[0].byte_offset, o2[1].byte_offset), (3, 7));

    // 增量续读用**绝对**文件 offset（start + line_end），非本次 slice 相对：
    let mut s3 = SeqCounter::new();
    let first = jsonl(&["x"]); // "x\n" = 2B
    let (_, cur3) = read_new_lines(&first, ReadCursor::default(), KEY, &mut s3);
    let mut second = first.clone();
    second.extend_from_slice(&jsonl(&["yy"])); // + "yy\n"
    let (o3, _) = read_new_lines(&second, cur3, KEY, &mut s3);
    assert_eq!(o3.len(), 1);
    assert_eq!(o3[0].byte_offset, 5, "绝对 offset = 2(x\\n) + 3(yy\\n)");

    // 残行（torn tail）不计入 byte_offset：
    let mut s4 = SeqCounter::new();
    let (o4, cur4) = read_new_lines(
        b"done\nhalf-no-newline",
        ReadCursor::default(),
        KEY,
        &mut s4,
    );
    assert_eq!(o4.len(), 1);
    assert_eq!((o4[0].byte_offset, cur4.consumed), (5, 5)); // done\n=5；残行不计

    // 空行跳过、不占 byte_offset 连续性（offset 仍按原始字节累计）：
    let mut s5 = SeqCounter::new();
    let (o5, _) = read_new_lines(b"a\n\nb\n", ReadCursor::default(), KEY, &mut s5);
    assert_eq!(o5.len(), 2); // 空行跳过
    assert_eq!((o5[0].byte_offset, o5[1].byte_offset), (2, 5)); // a\n=2；空\n 占 1B（→3，跳过）；b\n 到 5
}

#[test]
fn no_new_bytes_yields_nothing_and_does_not_bump_seq() {
    let mut seqs = SeqCounter::new();
    let buf = jsonl(&[r#"{"x":1}"#]);
    let (_, cur) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
    // Re-process identical bytes: consumed == len, start >= len, nothing new.
    let (again, cur2) = read_new_lines(&buf, cur, KEY, &mut seqs);
    assert!(again.is_empty());
    // A fresh read of the same key still hands out seq 1 only if a line was
    // produced; here nothing new, so the next live line would be seq 1.
    assert_eq!(seqs.next(KEY), 1, "seq must not have advanced past 1");
    assert_eq!(cur2.consumed, buf.len() as u64);
}

#[test]
fn truncation_resets_offset_but_seq_keeps_climbing() {
    let mut seqs = SeqCounter::new();

    let big = jsonl(&[r#"{"n":1}"#, r#"{"n":2}"#, r#"{"n":3}"#]);
    let (out, big_cur) = read_new_lines(&big, ReadCursor::default(), KEY, &mut seqs);
    assert_eq!(out.iter().map(|l| l.seq).collect::<Vec<_>>(), vec![0, 1, 2]);

    // Simulated truncation: file is now SHORTER than the recorded cursor.
    let small = jsonl(&[r#"{"n":99}"#]);
    assert!((small.len() as u64) < big_cur.seen_len, "test precondition");
    let (out2, small_cur) = read_new_lines(&small, big_cur, KEY, &mut seqs);

    // Cursor reset to 0 then re-advanced to the new (smaller) length.
    assert_eq!(small_cur.consumed, small.len() as u64);
    // The whole truncated file is re-read from byte 0 ...
    assert_eq!(out2.len(), 1);
    // ... but seq KEEPS CLIMBING (3, not back to 0): the climbing invariant.
    assert_eq!(out2[0].seq, 3, "seq must never reset on truncation");
}

/// F14 audit fix: a rewrite whose new length lands inside the pending
/// torn-tail window [consumed, seen_len) must still be detected as
/// truncation — no garbage line from a stale offset.
#[test]
fn rewrite_within_torn_window_detected_as_truncation() {
    let mut seqs = SeqCounter::new();
    // 19 bytes: complete line (8) + torn tail (11). consumed=8, seen_len=19.
    let torn = b"{\"a\":1}\n{\"a\":2,\"tor".to_vec();
    let (out, cur) = read_new_lines(&torn, ReadCursor::default(), KEY, &mut seqs);
    assert_eq!(out.len(), 1);
    assert_eq!(
        cur,
        ReadCursor {
            consumed: 8,
            seen_len: 19
        }
    );

    // Whole-file rewrite to 18 bytes: len >= consumed(8) but < seen_len(19).
    let rewritten = b"{\"b\":111}\n{\"b\":2}\n".to_vec();
    let (out2, cur2) = read_new_lines(&rewritten, cur, KEY, &mut seqs);
    assert_eq!(out2.len(), 2, "rewrite must be detected and re-read fully");
    assert_eq!(
        out2[0].raw, r#"{"b":111}"#,
        "no garbage from a stale offset"
    );
    assert_eq!(out2[0].seq, 1, "seq keeps climbing across truncation");
    assert_eq!(cur2.consumed, rewritten.len() as u64);
}

/// Truncate-to-empty must reset the cursor so a regrown file (even one
/// longer than the old consumed offset) is read from byte 0.
#[test]
fn truncate_to_empty_then_regrow_reads_from_zero() {
    let mut seqs = SeqCounter::new();
    let old = jsonl(&[r#"{"n":1}"#, r#"{"n":2}"#]); // 16 bytes
    let (_, cur) = read_new_lines(&old, ReadCursor::default(), KEY, &mut seqs);

    let (empty_out, cur2) = read_new_lines(&[], cur, KEY, &mut seqs);
    assert!(empty_out.is_empty());
    assert_eq!(
        cur2,
        ReadCursor {
            consumed: 0,
            seen_len: 0
        }
    );

    let regrown = jsonl(&[r#"{"m":1}"#, r#"{"m":2}"#, r#"{"m":3}"#]); // 24 > 16
    let (out, cur3) = read_new_lines(&regrown, cur2, KEY, &mut seqs);
    assert_eq!(out.len(), 3, "must re-read from byte 0, no lost prefix");
    assert_eq!(out[0].raw, r#"{"m":1}"#);
    assert_eq!(out[0].seq, 2, "seq never resets");
    assert_eq!(cur3.consumed, regrown.len() as u64);
}

/// \r\n endings: raw must match str::lines() semantics (strip \n plus one
/// adjacent \r); consumed advances by the byte count including \r\n.
#[test]
fn crlf_line_endings_are_stripped_like_lines() {
    let mut seqs = SeqCounter::new();
    let buf = b"{\"a\":1}\r\n{\"a\":2}\n".to_vec();
    let (out, cur) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].raw, r#"{"a":1}"#, "\\r must be stripped");
    assert_eq!(out[1].raw, r#"{"a":2}"#);
    assert_eq!(cur.consumed, 17);
}

/// Old-bug regression: invalid UTF-8 inside a COMPLETE line is lossy-decoded
/// for that line only — it must not abort the rest of the batch.
#[test]
fn invalid_utf8_in_complete_line_does_not_abort_batch() {
    let mut seqs = SeqCounter::new();
    let mut buf = b"{\"a\":1}\n".to_vec();
    buf.extend_from_slice(b"\xFF\xFEgarbage\n");
    buf.extend_from_slice(b"{\"a\":3}\n");
    let (out, _) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
    assert_eq!(out.len(), 3, "batch must not be silently aborted");
    assert_eq!(out[2].raw, r#"{"a":3}"#, "lines after the bad one survive");
    assert!(out[1].raw.contains('\u{FFFD}'), "bad line delivered lossy");
}

#[test]
fn leading_bom_is_stripped_for_the_empty_check_and_line_is_kept() {
    let mut seqs = SeqCounter::new();
    // A line that is ONLY a BOM + whitespace must be treated as empty.
    let only_bom = "\u{feff}   \n".as_bytes().to_vec();
    let (out, _) = read_new_lines(&only_bom, ReadCursor::default(), KEY, &mut seqs);
    assert!(out.is_empty(), "BOM-only/blank line is skipped");
    assert_eq!(seqs.next(KEY), 0, "skipped line must not consume a seq");

    // A BOM-prefixed real line is kept (and not double counted).
    let mut seqs2 = SeqCounter::new();
    let bom_line = "\u{feff}{\"k\":1}\n".as_bytes().to_vec();
    let (out2, _) = read_new_lines(&bom_line, ReadCursor::default(), KEY, &mut seqs2);
    assert_eq!(out2.len(), 1);
    assert_eq!(out2[0].seq, 0);
}

#[test]
fn empty_lines_are_skipped_and_do_not_consume_seq() {
    let mut seqs = SeqCounter::new();
    let buf = jsonl(&[r#"{"a":1}"#, "", "   ", r#"{"a":2}"#, ""]);
    let (out, _) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
    assert_eq!(out.len(), 2, "two blank/whitespace lines dropped");
    assert_eq!(out[0].seq, 0);
    assert_eq!(out[1].seq, 1);
    // Only two seqs were consumed; the next one is 2.
    assert_eq!(seqs.next(KEY), 2);
}

#[test]
fn is_jsonl_and_is_session_json_classify_correctly() {
    assert!(is_jsonl(Path::new("/x/abc.jsonl")));
    assert!(!is_jsonl(Path::new("/x/abc.json")));
    assert!(is_session_json(Path::new("/x/1234.json")));
    assert!(!is_session_json(Path::new("/x/1234.jsonl")));
}

#[test]
fn parse_session_id_extracts_the_field() {
    let blob = br#"{"sessionId":"abc-123","pid":4242}"#;
    assert_eq!(parse_session_id(blob), Some("abc-123".to_string()));
    // Missing field / wrong type / garbage → None.
    assert_eq!(parse_session_id(br#"{"pid":1}"#), None);
    assert_eq!(parse_session_id(br#"{"sessionId":5}"#), None);
    assert_eq!(parse_session_id(b"not json"), None);
}

#[test]
fn torn_line_without_trailing_newline_is_deferred() {
    let mut seqs = SeqCounter::new();
    // Complete line + torn tail (no trailing \n).
    let buf = b"{\"a\":1}\n{\"a\":2,\"tex".to_vec();
    let (out, cur) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
    assert_eq!(out.len(), 1, "torn tail must not be emitted");
    assert_eq!(out[0].raw, r#"{"a":1}"#);
    assert_eq!(
        cur.consumed, 8,
        "consumed stops after the complete line, not at EOF"
    );
    assert_eq!(cur.seen_len, buf.len() as u64, "seen_len covers the tail");

    // The tail completes (plus one more full line) — emitted exactly once,
    // seq continuous across the deferral.
    let mut healed = buf.clone();
    healed.extend_from_slice(b"t\":\"x\"}\n{\"a\":3}\n");
    let (out2, cur2) = read_new_lines(&healed, cur, KEY, &mut seqs);
    assert_eq!(out2.len(), 2);
    assert_eq!(out2[0].raw, r#"{"a":2,"text":"x"}"#);
    assert_eq!(out2[0].seq, 1);
    assert_eq!(out2[1].seq, 2);
    assert_eq!(cur2.consumed, healed.len() as u64);
}

#[test]
fn torn_multibyte_tail_does_not_decay_into_replacement_char() {
    let mut seqs = SeqCounter::new();
    let full = "{\"t\":\"文\"}\n".as_bytes(); // 文 = E6 96 87
    let torn = &full[..7]; // cut inside the multibyte sequence
    let (out, cur) = read_new_lines(torn, ReadCursor::default(), KEY, &mut seqs);
    assert!(out.is_empty(), "mid-multibyte torn tail must be deferred");
    assert_eq!(cur.consumed, 0);

    let (out2, cur2) = read_new_lines(full, cur, KEY, &mut seqs);
    assert_eq!(out2.len(), 1);
    assert_eq!(out2[0].raw, "{\"t\":\"文\"}", "no U+FFFD after healing");
    assert_eq!(cur2.consumed, full.len() as u64);
}

#[test]
fn fully_unterminated_single_line_is_deferred() {
    // A file whose only content is a line still being written: nothing is
    // complete yet, so nothing is emitted and the cursor stays put.
    let mut seqs = SeqCounter::new();
    let buf = br#"{"only":1}"#.to_vec();
    let (out, cur) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
    assert!(out.is_empty(), "unterminated line is deferred, not emitted");
    assert_eq!(cur.consumed, 0, "consumed must not advance past the tail");
    assert_eq!(cur.seen_len, buf.len() as u64);
    assert_eq!(seqs.next(KEY), 0, "deferral must not consume a seq");
}

// === Batch5-F20 add-time imposter check ===

#[test]
fn imposter_when_proc_started_after_pidfile() {
    // pidfile last written at t=1000, process started at t=2000 (> 1000+60).
    let v = add_time_verdict(None, None, Some(2000), Some(1000), None);
    assert_eq!(v, AddTimeVerdict::Imposter("started-after-pidfile"));
    // Reboot case is the same shape: old mtime, post-boot start.
    let v2 = add_time_verdict(
        None,
        None,
        Some(1_700_000_000),
        Some(1_600_000_000),
        Some("claude"),
    );
    assert_eq!(
        v2,
        AddTimeVerdict::Imposter("started-after-pidfile"),
        "time evidence must win even with a claude-looking cmdline (a NEW claude did not write the OLD pidfile)"
    );
}

#[test]
fn alive_within_tolerance() {
    // Started slightly after mtime but inside the 60s fuzz window.
    assert_eq!(
        add_time_verdict(None, None, Some(1030), Some(1000), None),
        AddTimeVerdict::Alive
    );
    // Started before mtime (the normal case: claude starts, then writes).
    assert_eq!(
        add_time_verdict(None, None, Some(900), Some(1000), None),
        AddTimeVerdict::Alive
    );
}

#[test]
fn imposter_by_cmdline() {
    assert_eq!(
        add_time_verdict(None, None, None, None, Some("tmux new-session -d")),
        AddTimeVerdict::Imposter("cmdline")
    );
    assert_eq!(
        add_time_verdict(None, None, Some(900), Some(1000), Some("-bash")),
        AddTimeVerdict::Imposter("cmdline"),
        "time check passing must not mask a non-claude cmdline"
    );
    // 并进来的样本（原 `agents/claudecode/liveness_tests.rs`，那一族退役）：
    // 编辑器 · 登录 shell · sshd 会话 —— PID 被复用时最常见的那几种「明显不像」。
    for foreign in ["/usr/bin/vim", "bash -l", "sshd: u@pts/0"] {
        assert_eq!(
            add_time_verdict(None, None, None, None, Some(foreign)),
            AddTimeVerdict::Imposter("cmdline"),
            "{foreign}"
        );
    }
}

#[test]
fn imposter_by_bg_spare_before_exact_identity() {
    // F74b(#43)：bg-spare 优先于 exact-identity——即便 procStart 自洽（recorded==current）
    // 也判 Imposter（否则守护池停泊备用进程恒绿）。
    assert_eq!(
        add_time_verdict(Some(555), Some(555), None, None, Some("claude bg-spare")),
        AddTimeVerdict::Imposter("bg-spare"),
        "bg-spare 必须在 exact-identity Alive 之前拦下"
    );
    assert_eq!(
        add_time_verdict(
            None,
            None,
            None,
            None,
            Some("/usr/bin/claude bg-spare --foo")
        ),
        AddTimeVerdict::Imposter("bg-spare")
    );
    // 普通 claude 会话不受影响（procStart 自洽仍 Alive）。
    assert_eq!(
        add_time_verdict(Some(555), Some(555), None, None, Some("claude --resume x")),
        AddTimeVerdict::Alive
    );
}

#[test]
fn claude_like_cmdlines_pass() {
    for cmd in [
        "claude --resume abc",
        "/usr/bin/node /home/u/.local/bin/claude",
        "NODE_OPTIONS=x node cli.js",
        // 只看得见解释器那一形（`liveness.rs` 头注：词表里有 `node` 就是为它）——
        // 解释器不在行首；从前这一格没人量（「`node` 只认行首」那一刀两族都放过）。
        "/usr/bin/node cli.js",
        "node index.js",
        "Claude", // case-insensitive
    ] {
        assert_eq!(
            add_time_verdict(None, None, Some(900), Some(1000), Some(cmd)),
            AddTimeVerdict::Alive,
            "{cmd}"
        );
    }
}

#[test]
fn missing_data_degrades_to_allow() {
    assert_eq!(
        add_time_verdict(None, None, None, None, None),
        AddTimeVerdict::Alive
    );
    assert_eq!(
        add_time_verdict(None, None, Some(2000), None, None),
        AddTimeVerdict::Alive
    );
    assert_eq!(
        add_time_verdict(None, None, None, Some(1000), None),
        AddTimeVerdict::Alive
    );
    // Empty cmdline (kernel threads read as empty) is not evidence.
    assert_eq!(
        add_time_verdict(None, None, None, None, Some("")),
        AddTimeVerdict::Alive
    );
    assert_eq!(
        add_time_verdict(None, None, None, None, Some("   ")),
        AddTimeVerdict::Alive
    );
}

// === Batch6-F22：远端会话生命周期 ===

/// ★★ `P0b-Y2`：**「盯着的目录被换掉」这条路必须有人重挂 + 重扫。**
///
/// # 这条判据够得到什么、够不到什么（先说清）
///
/// 够不到：**它真的听得见新 inode 吗** —— 那是 inotify 的运行期事实，
/// 只有真起后端、真删目录才验得出来（`tests/e2e/backend-sessions-rewatch.sh` 四组对照）。
/// 够得到：**那两件事还在不在代码里**。删掉任一件，e2e 会红 —— 但 e2e 不在 `cargo test` 里，
/// 有人只跑单测就会以为没事。⇒ 这条是给「改到这附近的人」的第一道提醒。
///
/// ⚠ 判据自己的失效方式：钉字符串会被注释喂绿 ⇒ 剥掉注释再钉（本仓第 N 次防它）。
#[test]
fn the_rewatch_path_still_exists_with_its_rescan() {
    let src = include_str!("../../../src/backend/observe/watcher.rs");
    let prod: String = src
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with("///")
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        prod.contains("fn rewatch_sessions("),
        "`rewatch_sessions` 没了 —— `sessions/` 被 rm+mkdir 之后后端会**活着、不吭声**\n             \
             （inotify 的 watch 绑在 inode 上）。那正是 `#60` 全链台架九拍拿不到读数的原因。"
    );
    // 调用点在（只有函数、没人调 = 死代码，e2e 会红但单测看不见）。
    assert!(
        prod.matches("rewatch_sessions(").count() >= 2,
        "`rewatch_sessions` 只有定义、没有调用点。"
    );
    // 重扫在：只重挂不重扫的话，「重建 → 挂上」之间写进去的文件永远捞不回来
    //（D 阶段变异 M23：只删这一步，前三组 e2e **全绿**，是第四组逼出来的）。
    let body_start = prod.find("fn rewatch_sessions(").expect("上面已确认存在");
    let body = &prod[body_start..(body_start + 2000).min(prod.len())];
    assert!(
        body.contains("WalkDir::new(sessions)"),
        "重挂之后没有重扫 —— 「重建 → 挂上」之间那段窗口期里写进去的 pidfile 会永远丢。"
    );
    // 父目录的耳朵在（听不见子目录出现/消失，重挂就永远不会被触发）。
    // 挂法从 `watch_loop` 里的一行搬进可重入的 `rewatch_agent_home`（针随之换）。
    assert!(
        prod.contains("watch(agent_home, RecursiveMode::NonRecursive)"),
        "没有监视 `agent_home` 本身 —— 那 `sessions/` 出现或被换掉时没有任何事件会来。"
    );
}

/// ★★★ **每一处 `.watch(` 都要回答「目录被换 inode 了怎么办」** —— 登记表。
///
/// # 为什么立这张表
///
/// 08-13 一天之内**同一个形状踩了三次**：`sessions/`（第十拍）· tmux socket 目录
///（第二十二拍）· `projects/`（第二十三拍）。前两次是**撞出来的**，第三次是
/// 「把挂点全列一遍、逐个对照有没有重挂路径」**查出来的**。
///
/// ⇒ 与其等第四次，不如把那次清点**固化**：本表锁住生产段 `.watch(` 的**处数与归属**。
/// 加一处就会红 —— 红了不是坏事，是让加的人**先回答那个问题**再往下写。
///
/// ⚠ 症状为什么值得这么防：inotify 的 watch 绑在 **inode** 上，目录被删掉重建之后
/// 那一路的事件**永远不来，且没有任何错误**。三次的表现分别是「永不宣告会话」
///「看不见新 tmux server」「会话还在但内容不动了」——**每一个都不报错**。
///
/// # 今天的 8 处（7 → 8：账号 manifest 所在目录；8 → 9：`agent_home` 那一处挪进可重入挂法、多一道它的上一层；
/// 9 → 8：账号目录那一处删了，改走 `rewatch_agent_home`（`AccountsEar`）—— 少的就是它；
/// 8 → 9：`AccountsEar::arm_subs` 挂各号目录（盯凭据文件））
///
/// | 处 | 归属 | 换 inode 怎么办 |
/// |---|---|---|
/// | `rewatch_dir` | **可重入挂法本体** | 就是它负责 |
/// | `watch_sock_dir_if_present` | socket 目录专用（多一条「目录没了翻记账」） | 同上 |
/// | `rewatch_sessions` | `sessions/` 专用（多一件事：挂上顺带重扫 pidfile） | 同上 |
/// | `rewatch_agent_home` 挂它本身那一道 | **父目录的耳朵**（`sessions/` · `projects/` 出现/消失的唯一信号源）；账号目录也走它；可重入 | 就是它负责（它自己后建 / 被换 ⇒ 由下一行那道上一层耳朵送事件来） |
/// | `rewatch_agent_home` 挂它上一层那一道 | `agent_home` 起来时不在 ⇒ 等它出现（与 socket 目录的父同形；挂上不摘） | 上一层被换掉 = 家目录那一级没了，**不在这一族** |
/// | `arm_ears` 里 socket 目录的**父**（从 `watch_loop` 挪进去，起步与「重新对齐」共用） | 等 socket 目录出现 | 同上 |
/// | `HomeEars::arm` 里 `sessions` 起步那次 | 起步挂一次，之后归 `rewatch_sessions` | 已有 |
/// | `watch_loop` 里 tmux socket **所在目录**（P3 复活探测） | 一次性触发器，socket 换 inode 由上面那条目录耳朵覆盖 | 已有 |
/// | `AccountsEar::arm_subs` 里各号目录 | 盯号目录里的凭据文件（登录完成那一刻推一帧） | 号目录被删重建 ⇒ 账号目录里那一格事件到 `on_path` ⇒ `arm_subs` 按盘上此刻重挂（不在了的先摘） |
#[test]
fn every_watch_site_answers_the_inode_swap_question() {
    let src = include_str!("../../../src/backend/observe/watcher.rs");
    // 生产段 = 测试模块之前（`guard_core::production_code` 在这里不能用：本条就住在测试模块里）。
    let cut = src
        .find("\n#[cfg(test)]")
        .map(|i| src[i..].find("\nmod ").map(|j| i + j).unwrap_or(i));
    let prod = match cut {
        Some(i) => &src[..i],
        None => src,
    };
    let sites = prod.matches(".watch(").count();
    assert_eq!(
        sites, 9,
        "生产段 `.watch(` 有 {sites} 处（登记表记着 9 处）。\n             \
             ⇒ **加了一处就来回答这个问题**：那个目录被删掉重建（换 inode）之后，\n             \
             它还收得到事件吗？收不到就走 `rewatch_dir`；确实不需要就把理由写进本条头注的表里。\n             \
             ⚠ 08-13 同一个形状踩了三次，三次的症状都是**不报任何错**：\n             \
             「永不宣告会话」「看不见新 tmux server」「会话还在但内容不动了」。"
    );
    // 三个可重入挂法必须都在（删掉任一个，上面的计数会跟着变，但报错要说得准）。
    for f in [
        "fn rewatch_dir(",
        "fn rewatch_sessions(",
        "fn watch_sock_dir_if_present(",
        "fn rewatch_agent_home(",
    ] {
        assert!(
            prod.contains(f),
            "可重入挂法 {f} 不见了 —— 那一路的重挂就没人做了"
        );
    }
}

/// ★★ socket 目录**被删掉再重建**时，watch 必须跟着换到新 inode。
///
/// # 为什么这条是结构判据而不是 e2e
///
/// 要真跑它得有一个**私有 socket 目录**（不然就得删用户真实的 `/tmp/tmux-<uid>`），
/// 而私有 socket 目录只能靠 `TMUX_TMPDIR` —— 那正是 `C7i` **零例外**禁止的东西
/// （`e2e_gate_registry::no_e2e_suite_isolates_with_tmux_tmpdir` 拦下了 e2e 那一版）。
/// ⇒ 红线与覆盖面冲突时本仓选红线，**换判据落在哪一层**。
///
/// ⚠ **如实记损失**：运行期行为 08-13 手工验过一次（删目录 → 起新 server ⇒
/// 修前新 server 一帧收不到、修后收得到），**没有进 CI**。这里钉的是那两处形状。
///
/// # 钉哪两处
///
/// ① 事件条件里**不许**再有 `&& !sock_dir_watched` —— 目录被换掉时那个标志仍是 true，
///    加上它等于把重挂整个短路（这正是首版的 bug）；
/// ② `watch_sock_dir_if_present` 必须**先 `unwatch` 再 `watch`** ——
///    分辨不出「同 inode 的普通事件」与「换了 inode」，重挂同一个无害、漏挂新的致命。
#[test]
fn the_socket_dir_watch_survives_an_inode_swap() {
    let src = include_str!("../../../src/backend/observe/watcher.rs");
    let prod: String = src
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with("///")
        })
        .collect::<Vec<_>>()
        .join("\n");
    // ⚠⚠ 针**运行时拼**：写成字面量的话，**本条自己的诊断文案**里那个串也会进语料
    //   ⇒ 判据恒红（08-13 实测栽了一次）。隔壁 `wire.rs` 的同族判据逐字记着这条：
    //   「判据扫自己所在的文件时，它写下的每一个例子都会变成语料」。
    //   ⚠ 而 `guard_core::production_code` 在这里**救不了**：它剥的是 `#[cfg(test)] mod`，
    //   而本条**就住在那个 mod 里** —— 剥完连要找的生产代码一起没了（首版又栽在这）。
    let short_circuit = format!("p == sock_dir.as_path() {} !sock_dir_watched", "&&");
    assert!(
        !prod.contains(short_circuit.as_str()),
        "事件条件里又出现了 `&& !sock_dir_watched` —— 目录被删掉再重建时那个标志仍是 true，\n             \
             这会把重挂整个短路，于是**新起的 tmux server 一帧都收不到**（08-13 实测过）。"
    );
    let at = prod
        .find("fn watch_sock_dir_if_present(")
        .expect("`watch_sock_dir_if_present` 不在了 —— 那是 socket 目录耳朵的唯一挂点");
    let body = &prod[at..(at + 1200).min(prod.len())];
    // ⚠ **数两处、不找第一处**：函数里有**两个** `unwatch(sock_dir)` ——
    //   一个在「目录没了」那支（翻记账），一个在重挂之前。首版用 `find` 取第一处，
    //   于是删掉重挂那处、变异**照样绿**（第一处顶了包）。这是本会话反复撞的
    //   「针在窗口内不唯一」那一族。
    let n_unwatch = body.matches("unwatch(sock_dir)").count();
    assert_eq!(
        n_unwatch, 2,
        "`watch_sock_dir_if_present` 里 `unwatch(sock_dir)` 应恰好 2 处\n             \
             （① 目录没了 ⇒ 翻记账；② 重挂之前 ⇒ 换 inode 时挂得上新的），实得 {n_unwatch}"
    );
    let watch_at = body
        .find("watch(sock_dir,")
        .expect("没有 `watch` —— 那它什么都没挂");
    let last_unwatch = body.rfind("unwatch(sock_dir)").expect("上面已确认有两处");
    assert!(
        last_unwatch < watch_at,
        "重挂前那次 `unwatch` 必须排在 `watch` **之前** —— 反了等于先挂再解，白挂一次"
    );
}

/// 同 pidfile 原地换 sid（/clear）：旧 sid 立即 Removed、新 sid Added，
/// active_sids 恰含新 sid（跨机审计实锤的假 live 泄漏回归测试）。
#[cfg(target_os = "linux")]
#[test]
fn sid_change_in_place_retires_old_sid() {
    let _iso = crate::control::identity_tag::door::isolate(); // §48.3：打标只落假 tmux
    let dir = std::env::temp_dir().join(format!("ccm-sidchange-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pid = std::process::id();
    let ticks = proc_starttime(pid).expect("own starttime");
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, false);
    let path = dir.join(format!("{pid}.json"));

    let write = |sid: &str| {
        std::fs::write(
            &path,
            format!(r#"{{"pid":{pid},"sessionId":"{sid}","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#),
        )
        .unwrap();
    };
    write("sid-1");
    process_session_added(&path, &mut state, &mut sink);
    assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { sid, .. }) if sid == "sid-1"));

    write("sid-2"); // /clear：同文件重写 sessionId
    process_session_added(&path, &mut state, &mut sink);
    assert!(
        // ★ S0：原地换 sid 的 removed 必须带 `Superseded`——monitor 靠它区分
        // 「死了（tmux 还在 ⇒ 灰点）」和「被顶替了（⇒ 直接归档）」。
        matches!(rx.try_recv(), Ok(Frame::SessionRemoved { sid, cause })
            if sid == "sid-1" && cause == RemovalCause::Superseded),
        "old sid must be retired BEFORE the new announcement"
    );
    assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { sid, .. }) if sid == "sid-2"));
    assert!(!state.active_sids.contains("sid-1"));
    assert!(state.active_sids.contains("sid-2"));
    assert_eq!(state.sessions.len(), 1);

    std::fs::remove_dir_all(&dir).ok();
}

/// pidfile 的工作目录是这台家里的 `autostart/` ⇒ 不宣告成会话（与历史页同一条判据）；同一个活进程换个目录就照常宣告。
/// 只把那个目录的字符串写进临时目录里的 pidfile，不碰那个目录本身。
#[cfg(target_os = "linux")]
#[test]
fn a_pidfile_in_the_hidden_dir_is_not_announced() {
    let _iso = crate::control::identity_tag::door::isolate();
    let hidden = crate::platform::paths::data_home()
        .expect("测试环境有家目录")
        .join("autostart");
    let dir = std::env::temp_dir().join(format!("ccm-hiddencwd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pid = std::process::id();
    let ticks = proc_starttime(pid).expect("own starttime");
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, false);
    let path = dir.join(format!("{pid}.json"));
    let write = |sid: &str, cwd: &str| {
        let body = serde_json::json!({
            "pid": pid, "sessionId": sid, "cwd": cwd, "kind": "interactive", "procStart": ticks.to_string(),
        });
        std::fs::write(&path, body.to_string()).unwrap();
    };

    write("sid-hidden", &hidden.to_string_lossy());
    assert!(!process_session_added(&path, &mut state, &mut sink));
    assert!(rx.try_recv().is_err(), "藏起来的那一趟不许出帧");
    assert!(state.active_sids.is_empty());

    write("sid-shown", "/x");
    process_session_added(&path, &mut state, &mut sink);
    assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { sid, .. }) if sid == "sid-shown"));

    std::fs::remove_dir_all(&dir).ok();
}

/// backend-09：`process_jsonl` 对 turn-end 记录发 **Line 后紧跟 TurnEnd**；非 turn-end 只发 Line；
/// **畸形行照发 Line、不 panic、无 TurnEnd**（§2.1 逐行转发 + turn-end 是 raw 之外额外边沿）。
#[test]
fn process_jsonl_emits_turn_end_after_line_raw_per_record() {
    let dir = std::env::temp_dir().join(format!("ccm-turnend-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, false);
    let path = dir.join("sess-1.jsonl");
    state.active_sids.insert("sess-1".to_string()); // process_jsonl 门控
                                                    // 三行：非 turn-end user / turn-end assistant / 畸形。
    let content = concat!(
        r#"{"type":"user","message":{}}"#,
        "\n",
        r#"{"type":"assistant","uuid":"u-2","message":{"stop_reason":"end_turn"}}"#,
        "\n",
        "not json at all",
        "\n",
    );
    std::fs::write(&path, content).unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    // 帧序：Line(user,seq0) / Line(end_turn,seq1) → TurnEnd(u-2) / Line(畸形,seq2)。
    assert!(
        matches!(rx.try_recv(), Ok(Frame::Line { seq: 0, .. })),
        "user 行 Line"
    );
    assert!(
        matches!(rx.try_recv(), Ok(Frame::Line { seq: 1, .. })),
        "end_turn 行 Line **先**发"
    );
    assert!(
        matches!(rx.try_recv(), Ok(Frame::TurnEnd { session_id, uuid }) if session_id == "sess-1" && uuid == "u-2"),
        "Line 后紧跟 TurnEnd(u-2)"
    );
    assert!(
        matches!(rx.try_recv(), Ok(Frame::Line { seq: 2, .. })),
        "畸形行照发 Line、不 panic"
    );
    assert!(
        rx.try_recv().is_err(),
        "无多余帧（畸形行不产 TurnEnd、user 行不产 TurnEnd）"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 同 sid 多 pidfile（resume 原进程未死）：删一个不发 Removed（引用计数），
/// 删第二个才 Removed 恰一次。
#[cfg(target_os = "linux")]
#[test]
fn same_sid_two_pidfiles_refcount() {
    let _iso = crate::control::identity_tag::door::isolate(); // §48.3：打标只落假 tmux
    let dir = std::env::temp_dir().join(format!("ccm-refcount-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pid = std::process::id();
    let ticks = proc_starttime(pid).expect("own starttime");
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, false);

    // 两个 pidfile 同 sid（借同一真实存活 pid；path key 不同即两个 entry）
    let p1 = dir.join(format!("{pid}.json"));
    // 第二个 pidfile 放子目录（path key 不同、file_stem 仍是 pid 数字）
    let sub = dir.join("dup");
    std::fs::create_dir_all(&sub).unwrap();
    let p2 = sub.join(format!("{pid}.json"));
    let body = format!(
        r#"{{"pid":{pid},"sessionId":"shared-sid","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#
    );
    std::fs::write(&p1, &body).unwrap();
    std::fs::write(&p2, &body).unwrap();
    process_session_added(&p1, &mut state, &mut sink);
    assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { sid, .. }) if sid == "shared-sid"));
    process_session_added(&p2, &mut state, &mut sink);
    // 第二个 pidfile：同一个会话已经宣告过 ⇒ 只记账，一帧都不发（也没有误发的 Removed）。
    assert!(
        rx.try_recv().is_err(),
        "second pidfile announces nothing (no second Added, no spurious Removed)"
    );
    assert_eq!(state.sessions.len(), 2);

    // 删第一个 → 仍被 p2 引用 → 不发 Removed
    process_session_removed(&p1, &mut state, &mut sink);
    assert!(
        rx.try_recv().is_err(),
        "no Removed while another pidfile holds the sid"
    );
    assert!(state.active_sids.contains("shared-sid"));

    // 删第二个 → 归零 → Removed 恰一次
    process_session_removed(&p2, &mut state, &mut sink);
    assert!(
        matches!(rx.try_recv(), Ok(Frame::SessionRemoved { sid, cause })
        if sid == "shared-sid" && cause == RemovalCause::Gone)
    );
    assert!(!state.active_sids.contains("shared-sid"));
    assert!(rx.try_recv().is_err());

    std::fs::remove_dir_all(&dir).ok();
}

/// 常规 added/removed 回归：单 pidfile 生命周期行为与 F22 前一致。
#[cfg(target_os = "linux")]
#[test]
fn plain_lifecycle_regression() {
    let _iso = crate::control::identity_tag::door::isolate(); // §48.3：打标只落假 tmux
    let dir = std::env::temp_dir().join(format!("ccm-plainlife-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pid = std::process::id();
    let ticks = proc_starttime(pid).expect("own starttime");
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, false);
    let path = dir.join(format!("{pid}.json"));
    std::fs::write(
        &path,
        format!(r#"{{"pid":{pid},"sessionId":"solo","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#),
    )
    .unwrap();
    process_session_added(&path, &mut state, &mut sink);
    assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { sid, .. }) if sid == "solo"));
    process_session_removed(&path, &mut state, &mut sink);
    assert!(
        matches!(rx.try_recv(), Ok(Frame::SessionRemoved { sid, cause })
        if sid == "solo" && cause == RemovalCause::Gone)
    );
    assert!(state.sessions.is_empty() && state.active_sids.is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

// === Batch9-F27：status 透传 ===

/// 宣告帧带初始 status；同 pidfile modify：status 变 → session_status 帧、
/// 不变 → 静默（幂等早退保留）。
#[cfg(target_os = "linux")]
#[test]
fn status_diff_emits_session_status_frame() {
    use crate::agents::SessionActivity;
    let _iso = crate::control::identity_tag::door::isolate(); // §48.3：打标只落假 tmux
    let dir = std::env::temp_dir().join(format!("ccm-status-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("projects")).unwrap();
    let pid = std::process::id();
    let ticks = proc_starttime(pid).expect("own starttime");
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, true);
    let pidfile = dir.join(format!("{pid}.json"));
    let write = |status: &str, waiting: Option<&str>| {
        let w = waiting
            .map(|x| format!(r#","waitingFor":"{x}""#))
            .unwrap_or_default();
        std::fs::write(
            &pidfile,
            format!(
                r#"{{"pid":{pid},"sessionId":"st-sid","cwd":"/p","procStart":"{ticks}","status":"{status}"{w}}}"#
            ),
        )
        .unwrap();
    };
    write("busy", None);
    process_session_added(&pidfile, &mut state, &mut sink);
    match rx.try_recv() {
        Ok(Frame::SessionAdded {
            sid,
            activity,
            background,
            ..
        }) => {
            assert_eq!(sid, "st-sid");
            assert_eq!(
                activity,
                Some(SessionActivity::Working),
                "适配层翻好的活动态"
            );
            assert!(!background, "不写 kind ⇒ 交互");
        }
        other => panic!("expected SessionAdded, got {other:?}"),
    }
    // 同内容 modify → 静默
    process_session_added(&pidfile, &mut state, &mut sink);
    assert!(rx.try_recv().is_err(), "status 未变不发帧");
    // status 变 → session_status 帧
    write("waiting", Some("permission prompt"));
    process_session_added(&pidfile, &mut state, &mut sink);
    match rx.try_recv() {
        Ok(Frame::SessionStatus {
            sid,
            activity,
            waiting_for,
            ..
        }) => {
            assert_eq!(sid, "st-sid");
            assert_eq!(activity, Some(SessionActivity::NeedsYou));
            assert_eq!(waiting_for.as_deref(), Some("permission prompt"));
        }
        other => panic!("expected SessionStatus, got {other:?}"),
    }
    // 再变回 → 再发
    write("idle", None);
    process_session_added(&pidfile, &mut state, &mut sink);
    assert!(matches!(
        rx.try_recv(),
        Ok(Frame::SessionStatus {
            activity: Some(SessionActivity::Idle),
            waiting_for: None,
            ..
        })
    ));
    std::fs::remove_dir_all(&dir).ok();
}

// === Batch8-F25：tail-only 模式 ===

/// tail-only 初扫：宣告帧带 path、零行帧；随后追加的新行 seq == 初扫时完整
/// 行数 L（行号语义）；末尾残行不计数（F14 torn-line 语义）。
#[cfg(target_os = "linux")]
#[test]
fn tail_only_primes_cursor_and_new_line_seq_is_line_number() {
    let _iso = crate::control::identity_tag::door::isolate(); // §48.3：打标只落假 tmux
    let dir = std::env::temp_dir().join(format!("ccm-tailonly-{}", std::process::id()));
    let proj = dir.join("projects").join("proj-x");
    std::fs::create_dir_all(&proj).unwrap();
    let pid = std::process::id();
    let ticks = proc_starttime(pid).expect("own starttime");
    // 既有历史：3 个完整行 + 1 个残行（残行不计数 → L=3）
    let jsonl = proj.join("tail-sid.jsonl");
    std::fs::write(&jsonl, b"{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n{\"torn").unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, true); // --tail-only
    let pidfile = dir.join(format!("{pid}.json"));
    std::fs::write(
        &pidfile,
        format!(r#"{{"pid":{pid},"sessionId":"tail-sid","cwd":"/p","procStart":"{ticks}"}}"#),
    )
    .unwrap();
    process_session_added(&pidfile, &mut state, &mut sink);
    // ① 宣告帧带 path
    match rx.try_recv() {
        Ok(Frame::SessionAdded {
            sid, path, lines, ..
        }) => {
            assert_eq!(sid, "tail-sid");
            assert_eq!(path.as_deref(), Some(jsonl.to_string_lossy().as_ref()));
            assert_eq!(
                lines,
                Some(3),
                "帧应带 prime 时的完整行数 L（快照完整性校验用）"
            );
        }
        other => panic!("expected SessionAdded, got {other:?}"),
    }
    // ② 零行帧（历史被 prime 吸收）
    assert!(rx.try_recv().is_err(), "tail-only 初扫不得发行帧");
    // ③ 补全残行 + 追加新行 → 唯一行帧 seq==3（残行补全后成为第 3 行，0-based）
    std::fs::write(
        &jsonl,
        b"{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n{\"torn\":true}\n{\"new\":1}\n",
    )
    .unwrap();
    process_jsonl(&jsonl, &mut state, &mut sink);
    match rx.try_recv() {
        Ok(Frame::Line {
            seq, byte_offset, ..
        }) => {
            assert_eq!(seq, 3, "残行补全行的 seq 应为初扫完整行数 L=3");
            // 帧上不再带原文 ⇒ 由末端字节认出它就是补全的那一行（`{"torn":true}\n` 收尾处）。
            assert_eq!(byte_offset, 38);
        }
        other => panic!("expected Line, got {other:?}"),
    }
    match rx.try_recv() {
        Ok(Frame::Line { seq, .. }) => assert_eq!(seq, 4),
        other => panic!("expected Line, got {other:?}"),
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// 默认（全量）模式行为不变：初扫把既有行全部推流（旧 monitor 兼容锚点）。
#[cfg(target_os = "linux")]
#[test]
fn full_replay_mode_still_streams_history() {
    let _iso = crate::control::identity_tag::door::isolate(); // §48.3：打标只落假 tmux
    let dir = std::env::temp_dir().join(format!("ccm-fullmode-{}", std::process::id()));
    let proj = dir.join("projects").join("proj-y");
    std::fs::create_dir_all(&proj).unwrap();
    let pid = std::process::id();
    let ticks = proc_starttime(pid).expect("own starttime");
    std::fs::write(proj.join("full-sid.jsonl"), b"{\"h\":1}\n{\"h\":2}\n").unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, false); // 默认全量
    let pidfile = dir.join(format!("{pid}.json"));
    std::fs::write(
        &pidfile,
        format!(r#"{{"pid":{pid},"sessionId":"full-sid","cwd":"/p","procStart":"{ticks}"}}"#),
    )
    .unwrap();
    process_session_added(&pidfile, &mut state, &mut sink);
    assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { .. })));
    assert!(matches!(rx.try_recv(), Ok(Frame::Line { seq: 0, .. })));
    assert!(matches!(rx.try_recv(), Ok(Frame::Line { seq: 1, .. })));
    std::fs::remove_dir_all(&dir).ok();
}

/// F25 DoD ④：(with_bg, tail_only) = (true, true) 组合——bg 会话放行且
/// tail-only 生效（宣告带元信息+path+lines，历史零行帧）。
#[cfg(target_os = "linux")]
#[test]
fn with_bg_and_tail_only_combined() {
    let _iso = crate::control::identity_tag::door::isolate(); // §48.3：打标只落假 tmux
    let dir = std::env::temp_dir().join(format!("ccm-combo-{}", std::process::id()));
    let proj = dir.join("projects").join("proj-c");
    std::fs::create_dir_all(&proj).unwrap();
    let pid = std::process::id();
    let ticks = proc_starttime(pid).expect("own starttime");
    std::fs::write(proj.join("combo-sid.jsonl"), b"{\"h\":1}\n{\"h\":2}\n").unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), true, true); // 双开
    let pidfile = dir.join(format!("{pid}.json"));
    std::fs::write(
        &pidfile,
        format!(r#"{{"pid":{pid},"sessionId":"combo-sid","cwd":"/p","kind":"bg","name":"任务","procStart":"{ticks}"}}"#),
    )
    .unwrap();
    process_session_added(&pidfile, &mut state, &mut sink);
    match rx.try_recv() {
        Ok(Frame::SessionAdded {
            sid, lines, path, ..
        }) => {
            assert_eq!(sid, "combo-sid");
            assert_eq!(lines, Some(2), "tail-only 带 L");
            assert!(path.is_some());
        }
        other => panic!("expected SessionAdded, got {other:?}"),
    }
    assert!(rx.try_recv().is_err(), "tail-only：历史零行帧");
    std::fs::remove_dir_all(&dir).ok();
}

// === Batch7-F24：--with-bg 放行 + 帧元信息 ===

#[cfg(target_os = "linux")]
#[test]
fn with_bg_announces_bg_with_metadata() {
    let _iso = crate::control::identity_tag::door::isolate(); // §48.3：打标只落假 tmux
    let dir = std::env::temp_dir().join(format!("ccm-withbg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pid = std::process::id();
    let ticks = proc_starttime(pid).expect("own starttime");
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), true, false); // --with-bg
    let path = dir.join(format!("{pid}.json"));
    std::fs::write(
        &path,
        format!(r#"{{"pid":{pid},"sessionId":"bg-sid","cwd":"/proj/x","kind":"bg","jobId":"j","name":"评估任务","procStart":"{ticks}"}}"#),
    )
    .unwrap();
    process_session_added(&path, &mut state, &mut sink);
    match rx.try_recv() {
        Ok(Frame::SessionAdded {
            sid,
            background,
            cwd,
            name,
            ..
        }) => {
            assert_eq!(sid, "bg-sid");
            assert!(background, "后台会话那一格由适配层判好");
            assert_eq!(cwd.as_deref(), Some("/proj/x"));
            assert_eq!(name.as_deref(), Some("评估任务"));
        }
        other => panic!("expected SessionAdded with metadata, got {other:?}"),
    }
    assert!(state.active_sids.contains("bg-sid"), "bg 行要能流出");
    std::fs::remove_dir_all(&dir).ok();
}

/// 宣告帧的项目目录读记录开头那一条（后面进了子目录也不漂，也不是 pidfile 那一格）；记录还没写出来 ⇒ pidfile 记的起会话目录。
#[cfg(target_os = "linux")]
#[test]
fn session_added_carries_the_project_dir_from_the_record_head() {
    let _iso = crate::control::identity_tag::door::isolate(); // §48.3：打标只落假 tmux
    let dir = std::env::temp_dir().join(format!("ccm-projdir-added-{}", std::process::id()));
    let proj = dir.join("projects").join("-a-proj");
    std::fs::create_dir_all(&proj).unwrap();
    let pid = std::process::id();
    let ticks = proc_starttime(pid).expect("own starttime");
    let pidfile = dir.join(format!("{pid}.json"));
    let announce = |sid: &str| {
        std::fs::write(
            &pidfile,
            format!(r#"{{"pid":{pid},"sessionId":"{sid}","cwd":"/launched/here","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, true);
        process_session_added(&pidfile, &mut state, &mut sink);
        match rx.try_recv() {
            Ok(Frame::SessionAdded {
                cwd, project_dir, ..
            }) => (cwd, project_dir),
            other => panic!("expected SessionAdded, got {other:?}"),
        }
    };
    let rec = |cwd: &str| format!("{{\"type\":\"user\",\"cwd\":\"{cwd}\"}}\n");
    std::fs::write(
        proj.join("pd-sid.jsonl"),
        rec("/a/proj") + &rec("/a/proj/sub") + &rec("/a/proj/sub"),
    )
    .unwrap();
    let (cwd, project_dir) = announce("pd-sid");
    assert_eq!(project_dir.as_deref(), Some("/a/proj"));
    assert_eq!(cwd.as_deref(), Some("/launched/here"), "pidfile 那一格原样");
    let (_, fresh) = announce("pd-fresh");
    assert_eq!(
        fresh.as_deref(),
        Some("/launched/here"),
        "还没有记录 ⇒ pidfile 记的起会话目录"
    );
    std::fs::remove_dir_all(&dir).ok();
}

// === Batch6-F21：kind 交互性门 ===

#[test]
fn background_reads_through_the_adapter() {
    assert!(
        is_background(br#"{"sessionId":"s","kind":"bg","jobId":"j"}"#),
        "真实 bg 样本形态"
    );
    assert!(!is_background(br#"{"sessionId":"s","kind":"interactive"}"#));
    assert!(!is_background(br#"{"sessionId":"s"}"#), "不写 kind ⇒ 交互");
    assert!(!is_background(b"not json"));
}

/// 集成：kind:"bg" 的 pidfile（真实存活进程 = 本进程，身份/时间证据全过）
/// 在 kind 门被拒——不发 SessionAdded、不进 sessions/active_sids。
/// 对照组：同进程 interactive pidfile 正常宣告。
#[cfg(target_os = "linux")]
#[test]
fn bg_pidfile_is_gated_even_when_author_is_alive() {
    let _iso = crate::control::identity_tag::door::isolate(); // §48.3：打标只落假 tmux
    let dir = std::env::temp_dir().join(format!("ccm-kind-gate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pid = std::process::id();
    let ticks = proc_starttime(pid).expect("own starttime");

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, false);

    // bg pidfile：作者活着、procStart 逐位相等——F20 证据全过，但 kind 门拒
    let bg_path = dir.join(format!("{pid}.json"));
    std::fs::write(
        &bg_path,
        format!(r#"{{"pid":{pid},"sessionId":"bg-sid","cwd":"/x","kind":"bg","jobId":"j","procStart":"{ticks}"}}"#),
    )
    .unwrap();
    process_session_added(&bg_path, &mut state, &mut sink);
    assert!(state.sessions.is_empty(), "bg must not be tracked");
    assert!(!state.active_sids.contains("bg-sid"));
    assert!(rx.try_recv().is_err(), "no SessionAdded frame for bg");

    // 对照：interactive 正常宣告
    std::fs::write(
        &bg_path,
        format!(r#"{{"pid":{pid},"sessionId":"int-sid","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#),
    )
    .unwrap();
    process_session_added(&bg_path, &mut state, &mut sink);
    assert!(state.active_sids.contains("int-sid"));
    match rx.try_recv() {
        Ok(Frame::SessionAdded { sid, .. }) => assert_eq!(sid, "int-sid"),
        other => panic!("expected SessionAdded, got {other:?}"),
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn procstart_identity_match_short_circuits_all_heuristics() {
    // Recorded ticks == current ticks → author confirmed, even when the
    // heuristics would individually scream imposter (stale mtime, bad
    // cmdline): identity evidence is strictly stronger.
    assert_eq!(
        add_time_verdict(
            Some(12285972),
            Some(12285972),
            Some(9_999_999),
            Some(1000),
            Some("tmux")
        ),
        AddTimeVerdict::Alive
    );
}

#[test]
fn procstart_mismatch_falls_through_to_heuristics() {
    // Mismatch + stale time evidence → imposter (the tmux reuse case).
    assert_eq!(
        add_time_verdict(Some(12285972), Some(99999999), Some(2000), Some(1000), None),
        AddTimeVerdict::Imposter("started-after-pidfile")
    );
    // Mismatch alone with fresh mtime and claude-like cmdline → allow
    // (defends against CC changing the procStart format: a hard reject
    // would black out every real session).
    assert_eq!(
        add_time_verdict(
            Some(12285972),
            Some(99999999),
            Some(990),
            Some(1000),
            Some("claude")
        ),
        AddTimeVerdict::Alive
    );
}

#[test]
fn tolerance_exact_boundary() {
    // start == mtime + 60 → still inside tolerance (uses >, not >=).
    assert_eq!(
        add_time_verdict(None, None, Some(1060), Some(1000), None),
        AddTimeVerdict::Alive
    );
    // One second past → imposter.
    assert_eq!(
        add_time_verdict(None, None, Some(1061), Some(1000), None),
        AddTimeVerdict::Imposter("started-after-pidfile")
    );
}

#[test]
fn parse_procstart_ticks_variants() {
    assert_eq!(
        parse_procstart_ticks(br#"{"sessionId":"abc","procStart":"12285972"}"#),
        Some(12285972),
        "CC's real format: decimal string"
    );
    assert_eq!(
        parse_procstart_ticks(br#"{"procStart":12285972}"#),
        Some(12285972),
        "bare number tolerated"
    );
    assert_eq!(parse_procstart_ticks(br#"{"sessionId":"abc"}"#), None);
    assert_eq!(
        parse_procstart_ticks(br#"{"procStart":"133849906480000000"}"#),
        Some(133_849_906_480_000_000),
        "Windows FILETIME magnitude still parses (mismatch then falls to heuristics)"
    );
    assert_eq!(parse_procstart_ticks(b"not json"), None);
}

/// Integration sanity on the real /proc (Linux only): our own process's
/// start epoch must be between boot and now — catches a broken btime +
/// ticks/USER_HZ composition that pure-function tests cannot see.
#[cfg(target_os = "linux")]
#[test]
fn own_process_start_epoch_is_sane() {
    let ticks = proc_starttime(std::process::id());
    assert!(ticks.is_some(), "own starttime must be readable");
    let epoch = start_epoch_from_ticks(ticks).expect("own start epoch");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(
        epoch <= now + 2,
        "start {epoch} must not be in the future (now {now})"
    );
    assert!(
        now - epoch < 24 * 3600,
        "test process started within a day (got {})",
        now - epoch
    );
}

#[test]
fn parse_btime_from_realistic_proc_stat() {
    let stat = "cpu  123 0 456 789 0 0 0 0 0 0\n\
                    cpu0 61 0 228 394 0 0 0 0 0 0\n\
                    intr 12345 0 0\n\
                    ctxt 987654\n\
                    btime 1719900000\n\
                    processes 4321\n\
                    procs_running 2\n";
    assert_eq!(parse_btime(stat), Some(1_719_900_000));
    assert_eq!(parse_btime("cpu 1 2 3\n"), None, "no btime line");
    assert_eq!(parse_btime("btime notanumber\n"), None);
}

// === #34 procStart double-check (F04) ===

/// A normal `/proc/<pid>/stat` line: starttime is field 22. Sample is a real
/// kernel layout with a simple comm `(bash)`.
#[test]
fn parse_starttime_normal_line() {
    // pid=1234 comm=(bash) state=S ... field22(starttime)=9876543 ...
    let stat = "1234 (bash) S 1 1234 1234 0 -1 4194304 1 0 0 0 0 0 0 0 \
                    20 0 1 0 9876543 12345678 100 18446744073709551615 1 1 0 0";
    assert_eq!(parse_starttime_from_stat(stat), Some(9876543));
}

/// The comm gotcha: a process named with a space inside the parens must not
/// derail field counting (splitting the whole line would shift every field).
#[test]
fn parse_starttime_comm_with_space() {
    let stat = "4242 (my proc) R 1 4242 4242 0 -1 0 0 0 0 0 0 0 0 0 \
                    20 0 1 0 555000 0 0";
    assert_eq!(parse_starttime_from_stat(stat), Some(555000));
}

/// The hard comm gotcha: parentheses *inside* comm. We must key off the LAST
/// `')'`, not the first, or the offset is wrong.
#[test]
fn parse_starttime_comm_with_inner_parens() {
    let stat = "7 ((odd) name)) S 1 7 7 0 -1 0 0 0 0 0 0 0 0 0 \
                    20 0 1 0 424242 0 0";
    assert_eq!(parse_starttime_from_stat(stat), Some(424242));
}

/// Malformed / too-few-fields stat → None (never panics, no bad starttime).
#[test]
fn parse_starttime_malformed_returns_none() {
    assert_eq!(parse_starttime_from_stat(""), None); // no ')'
    assert_eq!(parse_starttime_from_stat("123 (x) S 1 2 3"), None); // < 22 fields
                                                                    // ')' present but starttime token is non-numeric.
    let bad = "1 (x) S 1 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 notanum 0";
    assert_eq!(parse_starttime_from_stat(bad), None);
}

/// `session_alive` truth table around the captured procStart.
///
/// The existence-dependent assertions only hold on Linux. The
/// reuse-detection logic — the whole point of #34 — is Linux-only, matching
/// the `/proc` runtime target.
///
/// ⚠ **U4a 起这段的前提变了，本测试因此在 Windows 上会 panic。** 原文写的是
/// 「on non-Linux `pid_alive` is a hardcoded `true` smoke stub」，并据此**刻意不加 cfg 门**
/// —— 而 U4a 把那个恒真 stub 换成了 `unimplemented!()`。今天在 Windows 上
/// `session_alive` → `pid_alive` 会直接 panic。
///
/// **本轮不加门**：`cargo check` 只编不跑，DoD ① 不受影响；而 U4b 一旦在真机上跑
/// `cargo test`，这会是第一个红 —— 那正是应该有人看一眼的时刻，加门会把它藏起来。
/// Phase D 审计的问题 9-3 已把它写进 U4b 的清单。
///
/// `pid_alive` 有了 Windows 臂（`platform/win_proc.rs`）⇒ 上面那句「在 Windows 上会 panic」
/// 不再成立；本条在真 Windows 上 `cargo test` 会走 `OpenProcess` 自己那个 pid。那一趟没人跑过（没有真机读数）。
#[test]
fn session_alive_self_is_alive_in_existence_only_mode() {
    // Cross-platform: the current process is alive, and with no captured
    // baseline (`None`) liveness degrades to existence — must read alive.
    let me = std::process::id();
    assert!(
        session_alive(me, None),
        "self is alive in existence-only mode"
    );
}

/// Full, portable truth table for the pure liveness decision — including the
/// transient-read-failure arm (`exists=true, expected=Some, current=None`)
/// that must NOT archive a still-existing PID (the regression #34 audit
/// flagged). No real `/proc` needed.
#[test]
fn is_same_live_process_truth_table() {
    // Process gone → dead regardless of start info.
    assert!(!is_same_live_process(false, Some(5), Some(5)));
    assert!(!is_same_live_process(false, None, None));

    // Exists + baseline + current readable: alive iff equal (reuse = differ).
    assert!(
        is_same_live_process(true, Some(5), Some(5)),
        "same start = alive"
    );
    assert!(
        !is_same_live_process(true, Some(5), Some(6)),
        "different read start = reused PID = dead"
    );

    // Exists but current start unreadable right now → DO NOT false-archive.
    assert!(
        is_same_live_process(true, Some(5), None),
        "transient /proc read failure on a live PID must stay alive"
    );

    // Exists, no baseline captured → existence-only degrade = alive.
    assert!(is_same_live_process(true, None, Some(9)));
    assert!(is_same_live_process(true, None, None));
}

// === #32 overflow signal (F05) ===

/// FrameSink: a full channel drops + counts; once the channel drains, the
/// next send emits a single `Overflow{dropped}` before the real frame and
/// resets the counter. tokio's `try_send`/`try_recv` are sync, so no runtime.
#[test]
fn frame_sink_counts_drops_then_signals_overflow_on_recovery() {
    let (tx, mut rx) = mpsc::channel::<Frame>(2);
    let mut sink = FrameSink::new(tx);

    // Fill both slots — these go through cleanly, no overflow owed.
    sink.send(Frame::SessionAdded {
        sid: "a".into(),
        agent_kind: None,
        liveness_confidence: None,
        background: false,
        attachable: None,
        cwd: None,
        project_dir: None,
        name: None,
        path: None,
        lines: None,
        activity: None,
        waiting_for: None,
        container: None,
        pid: None,
    });
    sink.send(Frame::SessionAdded {
        sid: "b".into(),
        agent_kind: None,
        liveness_confidence: None,
        background: false,
        attachable: None,
        cwd: None,
        project_dir: None,
        name: None,
        path: None,
        lines: None,
        activity: None,
        waiting_for: None,
        container: None,
        pid: None,
    });
    assert_eq!(
        sink.dropped, 0,
        "nothing dropped while the channel had room"
    );

    // Channel is full now: three sends are dropped and counted.
    sink.send(Frame::SessionAdded {
        sid: "c".into(),
        agent_kind: None,
        liveness_confidence: None,
        background: false,
        attachable: None,
        cwd: None,
        project_dir: None,
        name: None,
        path: None,
        lines: None,
        activity: None,
        waiting_for: None,
        container: None,
        pid: None,
    });
    sink.send(Frame::SessionAdded {
        sid: "d".into(),
        agent_kind: None,
        liveness_confidence: None,
        background: false,
        attachable: None,
        cwd: None,
        project_dir: None,
        name: None,
        path: None,
        lines: None,
        activity: None,
        waiting_for: None,
        container: None,
        pid: None,
    });
    sink.send(Frame::SessionAdded {
        sid: "e".into(),
        agent_kind: None,
        liveness_confidence: None,
        background: false,
        attachable: None,
        cwd: None,
        project_dir: None,
        name: None,
        path: None,
        lines: None,
        activity: None,
        waiting_for: None,
        container: None,
        pid: None,
    });
    assert_eq!(sink.dropped, 3);

    // Drain both queued frames (they are the first two, not the dropped ones).
    assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { .. })));
    assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { .. })));

    // Next send (channel now empty, cap 2): emits Overflow{3} into slot 1,
    // resets the counter, then the real frame into slot 2.
    sink.send(Frame::SessionRemoved {
        sid: "f".into(),
        cause: RemovalCause::Gone,
    });
    assert_eq!(sink.dropped, 0, "overflow signal flushed, counter reset");
    assert!(
        matches!(rx.try_recv(), Ok(Frame::Overflow { dropped: 3, .. })),
        "overflow signal carries the dropped count and arrives first"
    );
    assert!(
        matches!(rx.try_recv(), Ok(Frame::SessionRemoved { .. })),
        "the real frame follows the overflow signal"
    );

    // Steady state: no spurious Overflow once recovered.
    sink.send(Frame::SessionAdded {
        sid: "g".into(),
        agent_kind: None,
        liveness_confidence: None,
        background: false,
        attachable: None,
        cwd: None,
        project_dir: None,
        name: None,
        path: None,
        lines: None,
        activity: None,
        waiting_for: None,
        container: None,
        pid: None,
    });
    assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { .. })));
}

/// ★★ **「只数不建」必须与「建了再数」得出同一个数、同一个游标、同一批 seq**
/// 〔audit-0805 F04 第 3 步〕。
///
/// 最容易错的是 **seq**：不收集时很容易顺手把 `seqs.next(key)` 一起省掉，
/// 而它是 per-path 单调且**永不重置**的 —— 少推一次，后续所有行的 seq 就与整读那条路
/// **错开一位**，而**结果仍然像一份合理的输出**（行数对、内容对，只有编号悄悄偏了）。
#[test]
fn counting_only_agrees_with_collecting_on_count_cursor_and_seq() {
    let data = b"{\"a\":1}\n{\"b\":2}\r\n\n{\"c\":3}\n{\"torn\":";
    let mut s_collect = SeqCounter::default();
    let (lines, cur_collect) = read_new_lines_at(
        data,
        0,
        data.len() as u64,
        ReadCursor::default(),
        "k",
        &mut s_collect,
    );
    let mut s_count = SeqCounter::default();
    let (n, cur_count) = count_new_lines_at(
        data,
        0,
        data.len() as u64,
        ReadCursor::default(),
        "k",
        &mut s_count,
    );

    assert!(!lines.is_empty(), "夹具没产出行 —— 本条会零命中地绿");
    assert_eq!(
        n,
        lines.len(),
        "行数不一致：只数 {n} vs 建了再数 {}",
        lines.len()
    );
    assert_eq!(cur_collect, cur_count, "游标不一致");

    // ★ seq 推进必须一样。⚠ **必须用同一个 key** —— `SeqCounter` 是 per-path 的，
    //   用两个不同的 key 去比等于两边都从 0 开始，这条判据就恒真了。
    //   （第一版就是这么写错的：变异「不收集时省掉 seqs.next」照样绿。）
    let more = b"{\"d\":4}\n";
    let (l_after_collect, _) = read_new_lines_at(
        more,
        0,
        more.len() as u64,
        ReadCursor::default(),
        "k",
        &mut s_collect,
    );
    let (l_after_count, _) = read_new_lines_at(
        more,
        0,
        more.len() as u64,
        ReadCursor::default(),
        "k",
        &mut s_count,
    );
    assert_eq!(
        l_after_collect[0].seq, l_after_count[0].seq,
        "同一个 key 上两条路推进的 seq 不同步 —— 只数不建时把 `seqs.next` 省掉了。\n\
             它是 per-path 单调且永不重置的：少推一次，后续所有行的编号就与整读那条路错开一位，\n\
             而**结果仍然像一份合理的输出**（行数对、内容对，只有编号悄悄偏了）。"
    );
}

/// prime 那条路**不许再走收集入口**〔源码形态钉，防回退〕。
#[test]
fn prime_does_not_build_the_line_vector() {
    let src = guard_core::production_code(include_str!("../../../src/backend/observe/watcher.rs"));
    let begin = src
        .find("fn prime_file_cursor(")
        .expect("找不到 prime_file_cursor —— 抽取器坏了，本条会零命中地绿");
    let end = src[begin..]
        .find("\n}\n")
        .expect("找不到结尾 —— 抽取器坏了");
    let body = &src[begin..begin + end];
    assert!(
        body.contains("count_new_lines_at"),
        "prime_file_cursor 没走「只数不建」那条入口 —— 要么切错范围，要么改回去了"
    );
    assert!(
        !body.contains("read_new_lines_at"),
        "prime_file_cursor 又在走收集入口了。它只要行数（一条 debug 日志），\n\
             而首次 prime 时「新增那一段」就是整份文件 —— 257 MB 会被物化成另一份 String 堆用完即扔。"
    );
}

/// 冷接宣告（只读尾巴）时主记录不整份进内存：按块读，数行 / 推游标与整读同一个结果，峰值只到一块的量级。
/// 量具是本线程的分配高水位（同步 `#[test]`，被测代码与断言同线程）。
#[test]
fn priming_a_big_record_reads_it_in_blocks() {
    let dir = std::env::temp_dir().join(format!("ccm-w5-prime-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let proj = dir.join("projects").join("-p");
    std::fs::create_dir_all(&proj).unwrap();
    let sid = "prime-big";
    let path = proj.join(format!("{sid}.jsonl"));
    let line = format!("{{\"type\":\"user\",\"pad\":\"{}\"}}\n", "x".repeat(200));
    let n = 100_000usize; // ≈ 21 MiB
    let mut body = line.repeat(n).into_bytes();
    body.extend_from_slice(b"\n{\"type\":\"user\",\"torn\":");
    std::fs::write(&path, &body).unwrap();
    let complete = (body.len() - b"{\"type\":\"user\",\"torn\":".len()) as u64;

    let mut state = ReaderState::new(dir.join("projects"), false, true);
    state.active_sids.insert(sid.to_string());
    let base = crate::alloc_probe::reset_peak();
    let lines = prime_file_cursor(&path, &mut state);
    let grew = crate::alloc_probe::peak_since(base);
    let cursor = state.offsets.get(&path_key(&path)).copied();
    std::fs::remove_dir_all(&dir).ok();

    assert_eq!(lines, n as u64, "行数（空行不算、残行不算）");
    assert_eq!(
        cursor.map(|c| (c.consumed, c.seen_len)),
        Some((complete, body.len() as u64)),
        "游标停在最后一个整行之后，看到的长度是整份"
    );
    assert!(
        grew < 8 * 1024 * 1024,
        "冷接一份 {} 字节的主记录，本线程峰值涨了 {grew} 字节 —— 整份读进内存了",
        body.len()
    );
}

/// ★★ **两条 tail 读路不许再整读会话 jsonl**〔audit-0805 F04 第 2 步〕。
///
/// # 为什么不能笼统禁 `fs::read`
///
/// 同一个文件里 `process_session_added` **正当地**整读 pidfile
/// （`sessions/<PID>.json`，几百字节）。一条「本文件不许出现 `fs::read`」的守卫
/// 会把它一起禁掉，于是下一个人要么绕过守卫、要么把它加进豁免名单 ——
/// **两条路都会让这条判据失去意义**。⇒ 精确钉**那两个函数的函数体**。
///
/// # 它防的是什么
///
/// 「改回整读」是最容易发生的回退：整读的代码更短、也照样能跑（只是 257 MB 会话的
/// 每一次文件事件都要把整份读进内存）。**慢不会让任何测试变红** —— 所以只能靠源码形态钉。
#[test]
fn the_two_tail_readers_do_not_slurp_the_whole_session_file() {
    let src = guard_core::production_code(include_str!("../../../src/backend/observe/watcher.rs"));
    for (name, sig, reader) in [
        ("process_jsonl", "fn process_jsonl(", "read_tail_from"),
        ("prime_file_cursor", "fn prime_file_cursor(", "PrimeReader"),
    ] {
        let begin = src
            .find(sig)
            .unwrap_or_else(|| panic!("找不到 {name} —— 抽取器坏了，本条会零命中地绿"));
        let end = src[begin..]
            .find("\n}\n")
            .unwrap_or_else(|| panic!("找不到 {name} 的结尾 —— 抽取器坏了"));
        let body = &src[begin..begin + end];
        assert!(
            body.contains(reader),
            "{name} 里没有调 `{reader}` —— 要么切错范围（本条会零命中地绿），\n\
                 要么它被改回整读了。"
        );
        for slurp in ["fs::read(", "read_to_string("] {
            assert!(
                !body.contains(slurp),
                "{name} 里出现了 `{slurp}` —— 会话 jsonl 又被整读了。\n\
                     257 MB 的活跃会话每来一行就要整份读进内存，而**慢不会让任何测试变红**，\n\
                     所以这条只能靠源码形态钉。要读整份得说明白为什么（pidfile 那种小文件除外，\n\
                     它在 `process_session_added` 里、不在本条管辖内）。"
            );
        }
    }
}

/// ★★ **分块入口与整读入口必须逐字节同解**〔audit-0805 F04 第 1 步〕。
///
/// 这条判据存在的全部理由：`read_new_lines` 把 `bytes.len()` 当文件长度用，
/// 而「改成 seek 只读新字节」的**第一步**就是把长度摘出来。摘的过程里最容易错的
/// 是那个相对索引（`start - chunk_start`），而错了之后**结果仍然像一份合理的输出** ——
/// 行还在、seq 还在涨，只是内容错位。⇒ 用同一份数据两条路对拍，逐字段比。
#[test]
fn the_chunked_entry_agrees_with_the_whole_file_entry_byte_for_byte() {
    let data = b"{\"a\":1}\n{\"b\":2}\r\n\n{\"c\":3}\n{\"torn\":";
    // 先各自消费前一段，制造一个非零的 consumed。
    let mut seqs_a = SeqCounter::default();
    let (_, cur_a) = read_new_lines(&data[..8], ReadCursor::default(), "k", &mut seqs_a);
    let mut seqs_b = SeqCounter::default();
    let (_, cur_b) = read_new_lines(&data[..8], ReadCursor::default(), "k", &mut seqs_b);
    assert_eq!(cur_a, cur_b, "前置状态本身要一致");

    // 整读入口：喂全文件。
    let (lines_whole, cur_whole) = read_new_lines(data, cur_a, "k", &mut seqs_a);
    // 分块入口：只喂 [consumed, EOF) 那一段。
    let start = cur_b.consumed;
    let (lines_chunk, cur_chunk) = read_new_lines_at(
        &data[start as usize..],
        start,
        data.len() as u64,
        cur_b,
        "k",
        &mut seqs_b,
    );

    assert_eq!(cur_whole, cur_chunk, "游标必须一致（consumed / seen_len）");
    assert_eq!(
        lines_whole.len(),
        lines_chunk.len(),
        "行数必须一致：整读 {lines_whole:?} vs 分块 {lines_chunk:?}"
    );
    for (w, c) in lines_whole.iter().zip(lines_chunk.iter()) {
        assert_eq!(w.raw, c.raw, "内容错位了");
        assert_eq!(
            w.byte_offset, c.byte_offset,
            "byte_offset 错位了（这是相对索引算错时最典型的表现）：{w:?} vs {c:?}"
        );
        assert_eq!(w.seq, c.seq, "seq 不一致");
    }
    assert!(!lines_whole.is_empty(), "夹具没产出行 —— 本条会零命中地绿");
}

/// 分块没读到 EOF 时**必须炸**，不许静默〔定框 E4〕。
///
/// 少一截会让 torn-tail 判定把「还没读到的字节」误当成「写了一半」，
/// 游标停在半路且再也不前进 —— 这类错**无声且自洽**，所以是 `assert!` 不是 `debug_assert!`。
#[test]
#[should_panic(expected = "chunk 必须覆盖到 EOF")]
fn a_chunk_that_stops_short_of_eof_panics_instead_of_silently_wedging() {
    let data = b"{\"a\":1}\n{\"b\":2}\n";
    let mut seqs = SeqCounter::default();
    // 谎报 file_len：说文件更长，但只给了前半段。
    read_new_lines_at(
        &data[..8],
        0,
        data.len() as u64,
        ReadCursor::default(),
        "k",
        &mut seqs,
    );
}

/// ★★ **丢的是「别处没有」的帧时，`Overflow` 必须带上身份**〔audit-0805 F03，定框 E4〕。
///
/// 只有计数的 `Overflow` 对**内容帧**够用（行还在远端 jsonl 里），对**状态增量帧**不够：
/// 它是一次差分的结果、别处不存在，客户端拿着「丢了 N 条」**没法重同步**。
///
/// ⚠ 这条钉的是**行为**，不是「代码里有没有那个字段」——
/// 塞满通道、真丢一条 `SessionRemoved`，再看排空后那条 `Overflow` 认不认得它。
#[test]
fn dropping_an_unrecoverable_frame_puts_its_identity_in_the_overflow() {
    let (tx, mut rx) = mpsc::channel::<Frame>(1);
    let mut sink = FrameSink::new(tx);

    // 占满（cap 1）。
    sink.send(Frame::Line {
        session_id: "occupy".into(),
        path: "/p".into(),
        seq: 0,
        record: None,
        cwd: None,
        byte_offset: 0,
        rid: None,
        raw: None,
    });
    // 丢一条内容帧（可恢复 ⇒ 只计数、不留身份）与一条状态增量帧（不可恢复 ⇒ 留身份）。
    sink.send(Frame::Line {
        session_id: "content-lost".into(),
        path: "/p".into(),
        seq: 1,
        record: None,
        cwd: None,
        byte_offset: 1,
        rid: None,
        raw: None,
    });
    sink.send(Frame::SessionRemoved {
        sid: "sid-gone".into(),
        cause: RemovalCause::Gone,
    });
    assert_eq!(sink.dropped, 2, "两条都该计入 dropped");

    // 排空后下一次 send 会先补 Overflow。
    assert!(matches!(rx.try_recv(), Ok(Frame::Line { .. })));
    sink.send(Frame::TurnEnd {
        session_id: "x".into(),
        uuid: "u".into(),
    });
    match rx.try_recv() {
        Ok(Frame::Overflow {
            dropped,
            lost,
            lost_truncated,
        }) => {
            assert_eq!(dropped, 2);
            assert!(!lost_truncated, "才两条，远没到上限");
            assert_eq!(
                lost,
                vec![crate::stream::wire::LostFrame {
                    kind: "session_removed",
                    subject: Some("sid-gone".into()),
                }],
                "只有不可恢复的那条留身份；内容帧丢了别处还有，不该占位"
            );
        }
        other => panic!("期望带身份的 Overflow，实得 {other:?}"),
    }
}

/// 身份表**有界**，且超限**不是静默截断**〔定框 E5：上限与超限语义成对定义〕。
///
/// 没有这个界，就等于把 `CHANNEL_CAPACITY` 想防的内存增长从帧挪到了 `Overflow` 自己身上。
#[test]
fn the_identity_list_is_bounded_and_says_so_when_it_truncates() {
    let (tx, mut rx) = mpsc::channel::<Frame>(1);
    let mut sink = FrameSink::new(tx);
    sink.send(Frame::Line {
        session_id: "occupy".into(),
        path: "/p".into(),
        seq: 0,
        record: None,
        cwd: None,
        byte_offset: 0,
        rid: None,
        raw: None,
    });
    let over = LOST_IDENTITY_CAP + 5;
    for i in 0..over {
        sink.send(Frame::SessionRemoved {
            sid: format!("sid-{i}"),
            cause: RemovalCause::Gone,
        });
    }
    assert_eq!(sink.dropped, over as u64, "超出上限的仍然计入 dropped");
    assert_eq!(sink.lost.len(), LOST_IDENTITY_CAP, "身份表不许越界增长");
    assert!(sink.lost_truncated, "截断了就要说出来，不许静默");

    assert!(matches!(rx.try_recv(), Ok(Frame::Line { .. })));
    sink.send(Frame::TurnEnd {
        session_id: "x".into(),
        uuid: "u".into(),
    });
    match rx.try_recv() {
        Ok(Frame::Overflow {
            lost,
            lost_truncated,
            ..
        }) => {
            assert_eq!(lost.len(), LOST_IDENTITY_CAP);
            assert!(lost_truncated, "标志要真的上线，不能只留在 sink 里");
        }
        other => panic!("期望 Overflow，实得 {other:?}"),
    }
}

/// 分类表**不许用 `_ =>` 兜底**〔LEDGER S1 的钉法〕。
///
/// 穷尽 `match` 的全部价值就在于**新增帧种时编译期躲不掉**。
/// 有人图省事加一条 `_ => true`，编译照过、而新帧种就此默认「丢了没关系」——
/// 那正是 B-3 的原样复发。⇒ 用零命中守卫钉住源码形态。
#[test]
fn the_recoverability_table_has_no_catch_all_arm() {
    let src = guard_core::production_code(include_str!("../../../src/backend/stream/wire.rs"));
    let begin = src
        .find("pub fn loss_is_recoverable")
        .expect("找不到 loss_is_recoverable —— 抽取器坏了，本条会零命中地绿");
    let end = src[begin..]
        .find("\n    }\n")
        .expect("找不到函数结尾 —— 抽取器坏了");
    let body = &src[begin..begin + end];
    assert!(
        body.contains("Frame::Line"),
        "抽到的函数体里连 `Frame::Line` 都没有 —— 切错范围了，本条会零命中地绿"
    );
    assert!(
        !body.contains("_ =>"),
        "`loss_is_recoverable` 里出现了 `_ =>` 兜底臂。\n\
             穷尽 match 的全部价值就是**新增帧种时编译期躲不掉**；加了兜底 = 新帧种默认\n\
             「丢了没关系」，而那正是 audit-0805 B-3 的原样复发。逐个列出来，别偷懒。"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn session_alive_decision_table_linux() {
    // A PID that cannot be alive on any sane host → dead regardless of start.
    let dead_pid = u32::MAX;
    assert!(
        !session_alive(dead_pid, Some(123)),
        "absent PID is dead even with an expected start"
    );
    assert!(
        !session_alive(dead_pid, None),
        "absent PID is dead in existence-only mode too"
    );

    // The current process IS alive. Baseline == its real start → alive;
    // a wrong baseline → dead (the PID-reuse signal).
    let me = std::process::id();
    let real = proc_starttime(me).expect("self has a /proc starttime");
    assert!(
        session_alive(me, Some(real)),
        "self is alive when start matches"
    );
    assert!(
        !session_alive(me, Some(real.wrapping_add(1))),
        "a mismatched start means the PID was reused → dead"
    );
}

/// 真 inotify 那两条的夹具：起一个摘掉 tmux 环境的 `sleep`，等它的环境读得到（打标那一步要读）。
#[cfg(target_os = "linux")]
fn vis2_sleeper() -> std::process::Child {
    crate::control::identity_tag::tests::spawn_settled_sleep(|c| {
        c.env_remove("TMUX_PANE").env_remove("TMUX");
    })
}

/// 写一份活进程的 pidfile（形状同上面令牌那条的夹具：真 pid ＋ 真 procStart）。
#[cfg(target_os = "linux")]
fn vis2_pidfile(sessions: &Path, pid: u32, sid: &str) -> PathBuf {
    let ticks = proc_starttime(pid).expect("子进程的 starttime 读不到 —— 夹具坏了");
    let at = sessions.join(format!("{pid}.json"));
    std::fs::write(
        &at,
        format!(
            r#"{{"pid":{pid},"sessionId":"{sid}","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#
        ),
    )
    .unwrap();
    at
}

/// ★★ 〔「判据换成真 inotify 行为（先无后建 ⇒ 补发 `session_added`）」〕真 debouncer ＋ `HomeEars`：
/// `agent_home` 不在 ⇒ 挂上一层；一口气建出来并立刻写 pidfile ⇒ 恰好一帧 `session_added`（sid 手写）；再写一份 ⇒ 事件来得了。
#[cfg(target_os = "linux")]
#[test]
fn vis2_s3_an_agent_home_created_after_start_still_announces_its_session() {
    let _iso = crate::control::identity_tag::door::isolate(); // §48.3：打标只落假 tmux
    let root = std::env::temp_dir().join(format!("ccm-vis2-s3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let home = root.join("claude-home");
    let projects = crate::agents::claudecode::paths::projects_root(&home);
    let sessions = crate::agents::claudecode::paths::sessions_root(&home);
    assert!(!home.exists(), "夹具坏了：agent_home 一开始就在");

    let (etx, erx) = std::sync::mpsc::channel::<WatchEvent>();
    let mut debouncer = new_debouncer(Duration::from_millis(DEBOUNCE_MS), DebouncerSink(etx))
        .expect("debouncer 起不来");
    let mut ears = HomeEars::new(&home, &projects, &sessions);
    ears.arm(&mut debouncer);
    // ①
    assert_eq!(
        (ears.home_watched, ears.parent_watched),
        (false, true),
        "`agent_home` 不在时挂的应当是它的上一层（且不是它本身）"
    );

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(projects.clone(), false, false);

    // ② 先无后建，建完立刻写 pidfile。
    let mut kids = vec![vis2_sleeper(), vis2_sleeper()];
    std::fs::create_dir_all(&sessions).unwrap();
    vis2_pidfile(&sessions, kids[0].id(), "vis2-s3-first");
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut added: Vec<String> = Vec::new();
    while added.is_empty() && std::time::Instant::now() < deadline {
        if let Ok(WatchEvent::Notify(Ok(evs))) = erx.recv_timeout(Duration::from_millis(200)) {
            for ev in evs {
                ears.on_path(&mut debouncer, &ev.path, &mut state, &mut sink);
            }
        }
        while let Ok(f) = rx.try_recv() {
            if let Frame::SessionAdded { sid, .. } = f {
                added.push(sid);
            }
        }
    }
    let upgraded = (ears.home_watched, ears.sessions_watched);

    // ③ 第二份 pidfile 的事件真的来（`sessions/` 那道 watch 挂在新目录上）。
    let second = vis2_pidfile(&sessions, kids[1].id(), "vis2-s3-second");
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut heard_second = false;
    while !heard_second && std::time::Instant::now() < deadline {
        if let Ok(WatchEvent::Notify(Ok(evs))) = erx.recv_timeout(Duration::from_millis(200)) {
            heard_second = evs.iter().any(|ev| ev.path == second);
        }
    }

    for k in kids.iter_mut() {
        let _ = k.kill();
        let _ = k.wait();
    }
    drop(debouncer);
    std::fs::remove_dir_all(&root).ok();

    assert_eq!(
        added,
        vec!["vis2-s3-first".to_string()],
        "`agent_home` 后建出来之后没有（恰好一次）宣告写在里面的会话 —— 上一层那道耳朵或「刚出现就挂下面两个 ＋ 重扫」断了"
    );
    assert_eq!(
        upgraded,
        (true, true),
        "`agent_home` 出现之后应当已挂上它本身与 `sessions/`"
    );
    assert!(
        heard_second,
        "挂上之后再写的 pidfile 没有事件 —— `sessions/` 那道 watch 不在新目录上"
    );
}

/// 接线：`watch_loop` 经 `HomeEars` 恰好 `arm` 一处、`on_path` 一处（上一条才在执行链上）；旧那句话零命中。带正控。
#[test]
fn vis2_s3_the_watch_loop_goes_through_the_home_ears_exactly_once() {
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/observe/watcher.rs"
    ));
    let at = prod.find("fn watch_loop(").expect("`watch_loop` 不在了");
    let end = prod[at..]
        .find("\nstruct ReaderState")
        .map(|i| at + i)
        .expect("`watch_loop` 之后的 `ReaderState` 不在了 —— 切函数体的锚断了");
    let body = &prod[at..end];
    let count = |text: &str, needle: &str| text.matches(needle).count();
    // 挂法收进 `arm_ears`：起步一处 ＋ 「重新对齐」那一臂一处；`arm_ears` 里 `HomeEars::arm` 恰好一处。
    assert_eq!(
        count(body, "arm_ears("),
        2,
        "挂法不是「起步 ＋ 重新对齐」恰好两处"
    );
    let resync_arm = body
        .find("WatchEvent::Resync { only, done } =>")
        .expect("`Resync` 那一臂不在了");
    assert!(
        body[resync_arm..].contains("arm_ears("),
        "「重新对齐」那一臂没有重挂耳朵"
    );
    assert!(
        body[resync_arm..].contains("resync_sessions("),
        "「重新对齐」那一臂没走 `resync_sessions`（对表 ＋ 补读）"
    );
    let arm_fn = prod.find("fn arm_ears(").expect("`arm_ears` 不在了");
    assert_eq!(
        count(&prod[arm_fn..arm_fn + 400], "ears.arm(debouncer)"),
        1,
        "`arm_ears` 没经 `HomeEars::arm`"
    );
    assert_eq!(
        count(body, "ears.on_path("),
        1,
        "事件路径不是恰好一处交给 `HomeEars`"
    );
    let old = "本进程不会再重试挂它";
    assert_eq!(count(&prod, old), 0, "那句旧话又回到生产段了");
    // 正控：量具认得出。
    assert_eq!(
        count(&format!("{body}\nears.on_path(x)"), "ears.on_path("),
        2
    );
    assert_eq!(count(&format!("{prod}\n\"{old}\""), old), 1);
}

/// ★ A1 的判定那一半：一批文件事件里**有** manifest ⇒ 真；只有无关文件（同目录里的别的文件、
/// 写 manifest 时的临时文件）⇒ 假。事件循环里每批只调一次它、真就发**恰好一帧**（结构判据见下一条）。
/// 真进程读数（manifest 改一次 ⇒ 恰好一帧；同目录别的文件 ⇒ 零帧）见 `tests/evidence/SR1a-link-loopback.py` ⑪。
#[test]
fn a_batch_counts_as_an_accounts_change_only_when_the_manifest_is_in_it() {
    let m = PathBuf::from("/h/.claude-alt/accounts.json");
    let other = PathBuf::from("/h/.claude-alt/accounts.json.tmp");
    let far = PathBuf::from("/h/.claude/projects/p/s.jsonl");
    assert!(manifest_touched(
        [other.as_path(), m.as_path()].into_iter(),
        &m
    ));
    assert!(manifest_touched([m.as_path(), m.as_path()].into_iter(), &m));
    assert!(!manifest_touched(
        [other.as_path(), far.as_path()].into_iter(),
        &m
    ));
    assert!(!manifest_touched(std::iter::empty(), &m));
}

/// 接线：`Notify` 那一臂里**恰好一处**问 `manifest_touched`、真了发 `Frame::AccountsChanged`，
/// 而且它排在逐条处理事件的 `for` **之前**（逐条那一段里有 `continue`，放进去会被跳过）。
#[test]
fn the_notify_arm_asks_once_per_batch_before_the_per_event_loop() {
    let src = include_str!("../../../src/backend/observe/watcher.rs");
    let prod = crate::guard_support::production_code(src);
    let ask = guard_core::find_pinned(&prod, "if manifest_touched(")
        .expect("Notify 那一臂里不是恰好一处问 manifest_touched");
    // 「重新对齐」那一臂也发一帧（整机时）⇒ 全文两处；这里只认 Notify 那一臂里、问完之后的那一处。
    let emit = prod[ask..]
        .find("sink.send(Frame::AccountsChanged);")
        .map(|k| ask + k)
        .expect("问完之后没有发 accounts_changed");
    let per_event = prod[ask..]
        .find("for ev in events {")
        .map(|k| ask + k)
        .expect("问完之后没有逐条处理事件的 for —— 结构变了");
    assert!(ask < emit && emit < per_event, "问与发要排在逐条处理之前");
}

/// 一个 tmux 会话里两个窗格各跑一个 claude：各自的标签打在各自的窗格上，对账一轮就稳 —— 之后每一轮一次都不写。
/// 标签若还是会话级单值，两个 claude 每一轮都把对方改回来，而每次「真写了」都会再起一次探测（探测 ↔ 打标死循环）。
///
/// 假 tmux 照真 tmux 的取值规则：窗格上有自己的值就取它，没有就往上取会话那一级的。
#[cfg(target_os = "linux")]
#[test]
fn two_claudes_in_one_tmux_session_settle_after_one_retag_round() {
    let root = std::env::temp_dir().join(format!("ccm-w5-two-in-one-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let opts = root.join("opts");
    let sessions = root.join("sessions");
    std::fs::create_dir_all(&opts).unwrap();
    std::fs::create_dir_all(&sessions).unwrap();
    let script = root.join("tmux");
    std::fs::write(
        &script,
        format!(
            r#"#!/bin/sh
D='{}'
if [ "$2" = display-message ]; then
  v=$(cat "$D/p$5" 2>/dev/null || cat "$D/s" 2>/dev/null)
  printf '%s\n' "$6" | sed -e 's/#{{session_id}}/$0/g' -e "s/#{{@ccm_sid}}/$v/g" -e 's/#{{session_windows}}/1/g' -e 's/#{{@[a-z_]*}}//g'
  exit 0
fi
if [ "$1" = set-option ]; then
  if [ "$2" = -p ]; then printf '%s' "$6" > "$D/p$4"; else printf '%s' "$5" > "$D/s"; fi
  exit 0
fi
exit 1
"#,
            opts.display()
        ),
    )
    .unwrap();
    let _iso = crate::control::identity_tag::door::isolate_with(&script);
    let mut kids: Vec<std::process::Child> = ["%1", "%2"]
        .iter()
        .map(|pane| {
            crate::control::identity_tag::tests::spawn_settled_sleep(|c| {
                c.env("TMUX_PANE", pane).env_remove("TMUX");
            })
        })
        .collect();
    let files: Vec<PathBuf> = kids
        .iter()
        .zip(["two-a", "two-b"])
        .map(|(k, sid)| vis2_pidfile(&sessions, k.id(), sid))
        .collect();
    let (tx, _rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(root.join("projects"), false, false);
    let first: Vec<bool> = files
        .iter()
        .map(|f| process_session_added(f, &mut state, &mut sink))
        .collect();
    let rounds = [retag_tracked(&state, None), retag_tracked(&state, None)];
    for k in kids.iter_mut() {
        let _ = k.kill();
        let _ = k.wait();
    }
    std::fs::remove_dir_all(&root).ok();
    assert_eq!(
        first,
        vec![true, true],
        "两个窗格起初都没挂标签 ⇒ 各写一次（夹具自检）"
    );
    assert_eq!(
        rounds,
        [0, 0],
        "同一个 tmux 会话里的两个 claude 在互相改写标签 —— 每一轮对账都「真写了」⇒ 每份快照之后再探一次，不收敛"
    );
}

/// 同一个 sid 两份 pidfile（resume 时原进程还活着）：「会话出现」只报一次；先退的那一份不摘这个会话。
#[cfg(target_os = "linux")]
#[test]
fn one_sid_in_two_pidfiles_is_announced_once() {
    let _iso = crate::control::identity_tag::door::isolate(); // §48.3：打标只落假 tmux
    let dir = std::env::temp_dir().join(format!("ccm-w5-dup-sid-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let sessions = dir.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let mut kids = vec![vis2_sleeper(), vis2_sleeper()];
    let files: Vec<PathBuf> = kids
        .iter()
        .map(|k| vis2_pidfile(&sessions, k.id(), "dup-sid"))
        .collect();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, false);
    initial_session_scan(&sessions, &mut state, &mut sink);
    std::fs::remove_file(&files[0]).unwrap();
    process_session_removed(&files[0], &mut state, &mut sink);
    let mut got = Vec::new();
    while let Ok(f) = rx.try_recv() {
        got.push(f.loss_identity().kind.to_string());
    }
    for k in kids.iter_mut() {
        let _ = k.kill();
        let _ = k.wait();
    }
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(
        got,
        vec!["session_added", "sessions_replayed"],
        "同一个会话宣告了两次，或先退的那份 pidfile 把还活着的会话摘了"
    );
}

/// **「清单报完了」那一帧的位置**：Phase 1 的每一帧 `session_added` 之后、恰好一帧。
///
/// 两趟：
/// - 有两个活 pidfile（真起两个 `sleep`，合成 pidfile 带真 procStart —— 与令牌那条判据同一个夹具形）
///   ⇒ 帧 kind 序列 == `[session_added, session_added, sessions_replayed]`；
/// - `sessions/` 压根不在 ⇒ 序列 == `[sessions_replayed]`（空清单也是说完了）。
///
/// 期望是手写的序列（不从实现生成）。子进程摘掉 `TMUX_PANE`：打标那一步不会去碰真机 tmux。
#[test]
fn sessions_replayed_follows_every_initial_session_added_exactly_once() {
    let _iso = crate::control::identity_tag::door::isolate(); // §48.3：打标只落假 tmux
    fn sleeper() -> std::process::Child {
        crate::control::identity_tag::tests::spawn_settled_sleep(|c| {
            c.env_remove("TMUX_PANE").env_remove("TMUX");
        })
    }
    fn kinds(rx: &mut tokio::sync::mpsc::Receiver<Frame>) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(f) = rx.try_recv() {
            out.push(f.loss_identity().kind.to_string());
        }
        out
    }

    // ① 两个活会话。
    let dir = std::env::temp_dir().join(format!("ccm-u4b-replayed-{}", std::process::id()));
    let sessions = dir.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let mut kids = vec![sleeper(), sleeper()];
    for (i, kid) in kids.iter().enumerate() {
        let pid = kid.id();
        let ticks = proc_starttime(pid).expect("子进程的 starttime 读不到 —— 夹具坏了");
        std::fs::write(
            sessions.join(format!("{pid}.json")),
            format!(
                r#"{{"pid":{pid},"sessionId":"u4b-{i}","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#
            ),
        )
        .unwrap();
    }
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, false);
    initial_session_scan(&sessions, &mut state, &mut sink);
    let mut frames = Vec::new();
    while let Ok(f) = rx.try_recv() {
        frames.push(f);
    }
    // 顺带钉容器那一格的**生产接线**：两个 `sleep` 都摘了 `TMUX_PANE`、环境读得到
    //   ⇒ 帧上 `container` == `none`（不是缺席）。结局 → 容器那张表另有一条逐格判；这一格判的是
    //   `process_session_added` 真把打标的结局接到了帧上。
    let containers: Vec<Option<crate::stream::wire::SessionContainer>> = frames
        .iter()
        .filter_map(|f| match f {
            Frame::SessionAdded { container, .. } => Some(container.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        containers,
        vec![Some(crate::stream::wire::SessionContainer::None); 2],
        "没有 TMUX_PANE、环境读得到的会话，帧上要说 `none`"
    );
    let got: Vec<String> = frames
        .iter()
        .map(|f| f.loss_identity().kind.to_string())
        .collect();
    for k in kids.iter_mut() {
        let _ = k.kill();
        let _ = k.wait();
    }
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(
        got,
        vec!["session_added", "session_added", "sessions_replayed"],
        "「清单报完了」要恰好一帧、排在 Phase 1 每一帧 `session_added` 之后"
    );

    // ② `sessions/` 不在：空清单也要说完。
    let empty = std::env::temp_dir().join(format!("ccm-u4b-replayed-empty-{}", std::process::id()));
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(8);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(empty.join("projects"), false, false);
    initial_session_scan(&empty.join("sessions"), &mut state, &mut sink);
    assert_eq!(kinds(&mut rx), vec!["sessions_replayed"]);
}

/// 起步那一下「先挂耳朵、再初扫」（[`arm_then_scan`]）：挂耳朵那一刻之后、初扫之前落下的 pidfile 由初扫报出
/// （原先先扫后挂，这一形两头都看不见 —— e2e `resume-frames` 间歇红的根因）。
/// 扫的过程中排队的事件交给主循环时都不再出帧：已报过的再来一次「有动静」不重报；初扫之前就没了的那份，
/// 它的「删掉了」不冒出一条从没 added 过的 `session_removed`。
#[test]
fn a_pidfile_landing_once_the_ears_are_up_is_announced_by_the_initial_scan() {
    let _iso = crate::control::identity_tag::door::isolate(); // §48.3：打标只落假 tmux
    let dir = std::env::temp_dir().join(format!("ccm-arm-then-scan-{}", std::process::id()));
    let sessions = dir.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let mut kid = crate::control::identity_tag::tests::spawn_settled_sleep(|c| {
        c.env_remove("TMUX_PANE").env_remove("TMUX");
    });
    let pid = kid.id();
    let ticks = proc_starttime(pid).expect("子进程的 starttime 读不到 —— 夹具坏了");
    let live = sessions.join(format!("{pid}.json"));
    let gone = sessions.join("1.json");
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, false);
    // 「挂耳朵」那一步里：一份活会话的 pidfile 落下；另一份落下又被删掉（初扫之前就没了，事件照样排着队）。
    arm_then_scan(&sessions, &mut state, &mut sink, || {
        std::fs::write(
            &live,
            format!(
                r#"{{"pid":{pid},"sessionId":"arm-live","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#
            ),
        )
        .unwrap();
        std::fs::write(
            &gone,
            r#"{"pid":1,"sessionId":"arm-gone","cwd":"/x","kind":"interactive"}"#,
        )
        .unwrap();
        std::fs::remove_file(&gone).unwrap();
    });
    let mut frames = Vec::new();
    while let Ok(f) = rx.try_recv() {
        frames.push(f);
    }
    let added: Vec<String> = frames
        .iter()
        .filter_map(|f| match f {
            Frame::SessionAdded { sid, .. } => Some(sid.clone()),
            _ => None,
        })
        .collect();
    let kinds: Vec<String> = frames
        .iter()
        .map(|f| f.loss_identity().kind.to_string())
        .collect();
    // 排队的那两件事件交给主循环（同 `watch_loop` 里 `is_session_json` 那一支：在 ⇒ 加，不在 ⇒ 删）。
    let mut later = Vec::new();
    for p in [&live, &gone] {
        if p.exists() {
            process_session_added(p, &mut state, &mut sink);
        } else {
            process_session_removed(p, &mut state, &mut sink);
        }
    }
    while let Ok(f) = rx.try_recv() {
        later.push(f.loss_identity().kind.to_string());
    }
    let _ = kid.kill();
    let _ = kid.wait();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(added, vec!["arm-live"], "挂耳朵之后落下的那份没被初扫报出");
    assert_eq!(kinds, vec!["session_added", "sessions_replayed"]);
    assert!(
        later.is_empty(),
        "排队的事件又出了帧（重报 / 冒出从没 added 过的 removed）：{later:?}"
    );
}

/// `session_added.pid` 有一道闸：索要了（`--with-pid`）⇒ 帧上是那个进程的 pid；
/// 没索要 ⇒ 缺席（没索要的客户端 —— 包括仓外 aterm —— 收到的字节与本字段加进来之前一字不差）。
///
/// 要求住址：`INVARIANTS §40` 逐字「我的目的就是把本地当成不走 ssh 的远端」—— 本机判活改由本机后端的帧来之后，
/// 本机 ↗ 按 pid 绑窗口只能从这一格拿 pid（monitor 不再自己读 pidfile）。
/// 两组对照：闸开 ⇒ `Some(那个 pid)`（不是别的数）· 闸关 ⇒ `None`（把闸删掉只有这一组红）。
#[cfg(target_os = "linux")]
#[test]
fn loc1b_the_pid_rides_the_session_added_frame_only_when_the_client_asked() {
    let _iso = crate::control::identity_tag::door::isolate(); // §48.3：打标只落假 tmux
    fn probe(label: &str, asked: bool) -> (u32, Option<u32>) {
        let dir =
            std::env::temp_dir().join(format!("ccm-loc1b-pid-{}-{label}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut kid = std::process::Command::new("sleep")
            .arg("60")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("起不来 `sleep` —— 夹具坏了");
        let pid = kid.id();
        let ticks = proc_starttime(pid).expect("子进程的 starttime 读不到 —— 夹具坏了");
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, false);
        state.with_pid = asked;
        let path = dir.join(format!("{pid}.json"));
        std::fs::write(
            &path,
            format!(
                r#"{{"pid":{pid},"sessionId":"pid-{label}","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#
            ),
        )
        .unwrap();
        process_session_added(&path, &mut state, &mut sink);
        let got = match rx.try_recv() {
            Ok(Frame::SessionAdded { sid, pid, .. }) => {
                assert_eq!(sid, format!("pid-{label}"));
                pid
            }
            other => panic!("[{label}] 没收到 `session_added`（实得 {other:?}）"),
        };
        let _ = kid.kill();
        let _ = kid.wait();
        std::fs::remove_dir_all(&dir).ok();
        (pid, got)
    }
    let (pid, got) = probe("asked", true);
    assert_eq!(got, Some(pid), "索要了，帧上却不是那个进程的 pid");
    let (_, got) = probe("unasked", false);
    assert_eq!(
        got, None,
        "没索要却上了 wire —— 没索要的客户端（仓外 aterm）收到的字节变了"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════════════
// 活会话的 jsonl 被删 / 改名 / 截短 / 原地改写
// ═══════════════════════════════════════════════════════════════════════════════════════
//
// 要求：「活会话的 jsonl 被改 / 删 / 改名：观察侧当它是
// 『看的、不是管的』—— 删了 / 改名 ⇒ 出声（该 tab 说一句『记录文件不见了』），不崩、不误判结束；被截短 ⇒ 按截断重读」；
// 「原地整份改写且变长要堵：游标旁记末尾若干字节，每次续读前核，对不上 ⇒ 当被改写：从 0 重读并出声」。
// 语料是结构性的假行（`{"n":…}`），不含任何真会话正文。

/// 把这一趟收到的帧摊平成 `(kind, 细节)`：行帧给 `line:<行号>`（帧上不再带原文），出声帧给它的 kind（重读带 why）。
fn fw1_drain(rx: &mut tokio::sync::mpsc::Receiver<Frame>) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(f) = rx.try_recv() {
        out.push(match f {
            Frame::Line { seq, .. } => format!("line:{seq}"),
            Frame::SessionFileGone { session_id, .. } => format!("gone:{session_id}"),
            Frame::SessionFileReread {
                session_id, why, ..
            } => {
                format!("reread:{session_id}:{why:?}")
            }
            other => format!("other:{}", other.loss_identity().kind),
        });
    }
    out
}

fn fw1_rig(
    tag: &str,
) -> (
    std::path::PathBuf,
    std::path::PathBuf,
    ReaderState,
    FrameSink,
    tokio::sync::mpsc::Receiver<Frame>,
) {
    let dir = std::env::temp_dir().join(format!("ccm-fw1-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel::<Frame>(256);
    let sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, false);
    state.active_sids.insert("s-fw1".to_string());
    let path = dir.join("s-fw1.jsonl");
    (dir, path, state, sink, rx)
}

/// ★ **删了 ⇒ 出声一次、不崩、不误判结束；同名再出现从 0 读；再不见再说一次**。改名走了 ＝ 旧路径不见了，同一形。
#[test]
fn a_vanished_session_file_is_said_once_and_a_recreated_one_is_read_from_zero() {
    let (dir, path, mut state, mut sink, mut rx) = fw1_rig("gone");
    std::fs::write(&path, "{\"n\":1}\n{\"n\":2}\n").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(fw1_drain(&mut rx), vec!["line:0", "line:1"]);

    std::fs::remove_file(&path).unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    process_jsonl(&path, &mut state, &mut sink); // 同一个「不在」再来一个事件 ⇒ 不重复说
    assert_eq!(
        fw1_drain(&mut rx),
        vec!["gone:s-fw1"],
        "删了之后不是恰好说一次"
    );
    assert!(
        state.active_sids.contains("s-fw1"),
        "记录文件没了就把会话当成结束了"
    );

    // agent 按路径追加 ⇒ 同名文件重新长出来，只有新行（比读到过的短也好、长也好，都从 0 读）。
    // 长回来的是另一份文件 ⇒ 先说「已从头重读」（行号从 0 重数，下游据它作废旧号）。
    std::fs::write(&path, "{\"n\":3}\n").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(
        fw1_drain(&mut rx),
        vec!["reread:s-fw1:Rewritten", "line:0"],
        "重建之后不是「出声 ＋ 从 0 读」"
    );
    std::fs::write(&path, "{\"n\":3}\n{\"n\":4-a-longer-line-than-before}\n").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(fw1_drain(&mut rx), vec!["line:1"]);

    // 改名走了 ⇒ 旧路径不见了 ⇒ 再说一次（上一次「不在」已经被「又在了」清掉）。
    std::fs::rename(&path, dir.join("elsewhere.jsonl")).unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(fw1_drain(&mut rx), vec!["gone:s-fw1"]);
    std::fs::remove_dir_all(&dir).ok();
}

/// ★ **截短 ⇒ 先出声（why = 截短）再从头重读**；行号从 0 重数（`watcher_lines_tests` 钉号）。
#[test]
fn a_truncated_session_file_is_said_and_reread_from_zero() {
    let (dir, path, mut state, mut sink, mut rx) = fw1_rig("trunc");
    std::fs::write(&path, "{\"n\":1}\n{\"n\":2}\n{\"n\":3}\n").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    let _ = fw1_drain(&mut rx);
    std::fs::write(&path, "{\"n\":9}\n").unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(fw1_drain(&mut rx), vec!["reread:s-fw1:Truncated", "line:0"]);
    std::fs::remove_dir_all(&dir).ok();
}

/// ★★ **原地整份改写且变长**（编辑器整份覆盖多了几个字）：长度没变短，截断判不出 ——
/// 末尾指纹对不上 ⇒ 先出声（why = 改写）再从头重读；读到的是新文件的每一行，**没有**从旧偏移读出来的半行。
/// 阴性对照：纯追加 ⇒ 不出声、只来新的那一行。
#[test]
fn an_in_place_rewrite_that_grew_is_caught_by_the_tail_fingerprint() {
    let (dir, path, mut state, mut sink, mut rx) = fw1_rig("rewrite");
    let before = "{\"n\":1,\"uuid\":\"aaaaaaaa-0001\"}\n{\"n\":2,\"uuid\":\"aaaaaaaa-0002\"}\n";
    std::fs::write(&path, before).unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    let _ = fw1_drain(&mut rx);

    // 阴性：纯追加。
    let appended = format!("{before}{{\"n\":3,\"uuid\":\"aaaaaaaa-0003\"}}\n");
    std::fs::write(&path, &appended).unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    assert_eq!(fw1_drain(&mut rx), vec!["line:2"], "纯追加却被当成改写了");

    // 改写：第一行多了几个字（整份平移），总长 ≥ 读到过的最长。
    let rewritten = appended.replacen("\"n\":1,", "\"n\":1,\"edited\":true,", 1);
    assert!(rewritten.len() >= appended.len());
    std::fs::write(&path, &rewritten).unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    let want: Vec<String> = std::iter::once("reread:s-fw1:Rewritten".to_string())
        .chain((0..rewritten.lines().count()).map(|k| format!("line:{k}")))
        .collect();
    assert_eq!(fw1_drain(&mut rx), want, "改写之后不是「出声 ＋ 整份重读」");
    std::fs::remove_dir_all(&dir).ok();
}

/// 两个新帧的线上形状（逐字节；异源 = 手写期望）＋ 丢了不可恢复（按身份报）。
#[test]
fn the_two_session_file_frames_have_exactly_these_bytes() {
    use crate::stream::wire::{to_line, RereadWhy};
    let gone = Frame::SessionFileGone {
        session_id: "s".into(),
        path: "/p/s.jsonl".into(),
    };
    assert_eq!(
        to_line(&gone).unwrap(),
        "{\"kind\":\"session_file_gone\",\"session_id\":\"s\",\"path\":\"/p/s.jsonl\"}\n"
    );
    for (why, lit) in [
        (RereadWhy::Truncated, "truncated"),
        (RereadWhy::Rewritten, "rewritten"),
    ] {
        let f = Frame::SessionFileReread {
            session_id: "s".into(),
            path: "/p/s.jsonl".into(),
            why,
        };
        assert_eq!(
            to_line(&f).unwrap(),
            format!("{{\"kind\":\"session_file_reread\",\"session_id\":\"s\",\"path\":\"/p/s.jsonl\",\"why\":\"{lit}\"}}\n")
        );
        assert!(!f.loss_is_recoverable());
    }
    assert!(!gone.loss_is_recoverable());
}

/// ★★ 〔「账号目录起步不在或被删掉重建 ⇒ 这一路失聪」〕真 debouncer ＋ `AccountsEar`：
/// 起步不在 ⇒ 挂上一层；建出来 ⇒ 它自己那一格事件（`on_path` 回 true）且之后写 manifest 听得见；删掉重建 ⇒ 仍听得见。
#[cfg(target_os = "linux")]
#[test]
fn gap1_an_accounts_dir_created_or_rebuilt_after_start_is_still_heard() {
    let root = std::env::temp_dir().join(format!("ccm-gap1-accts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let dir = root.join("accts");
    let manifest = dir.join("accounts.json");
    let (etx, erx) = std::sync::mpsc::channel::<WatchEvent>();
    let mut debouncer = new_debouncer(Duration::from_millis(DEBOUNCE_MS), DebouncerSink(etx))
        .expect("debouncer 起不来");
    let mut ear = AccountsEar::new(&dir);
    ear.arm(&mut debouncer);
    let armed = (ear.watched, ear.parent_watched);
    // 建目录 ⇒ 等到它自己那一格事件；再写 manifest ⇒ 等到 manifest 那一格。回 (目录事件到没到, manifest 事件到没到)。
    let mut round = |ear: &mut AccountsEar, debouncer: &mut _| {
        std::fs::create_dir_all(&dir).unwrap();
        let (mut dir_seen, mut manifest_seen) = (false, false);
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while !manifest_seen && std::time::Instant::now() < deadline {
            if dir_seen {
                std::fs::write(&manifest, b"{}").unwrap();
            }
            if let Ok(WatchEvent::Notify(Ok(evs))) = erx.recv_timeout(Duration::from_millis(200)) {
                for ev in &evs {
                    dir_seen |= ear.on_path(debouncer, &ev.path);
                }
                manifest_seen |=
                    dir_seen && manifest_touched(evs.iter().map(|e| e.path.as_path()), &manifest);
            }
        }
        (dir_seen, manifest_seen)
    };
    let first = round(&mut ear, &mut debouncer);
    std::fs::remove_dir_all(&dir).unwrap();
    let second = round(&mut ear, &mut debouncer);
    drop(debouncer);
    std::fs::remove_dir_all(&root).ok();
    assert_eq!(
        armed,
        (false, true),
        "账号目录不在时挂的应当是它的上一层（且不是它本身）"
    );
    assert_eq!(
        first,
        (true, true),
        "账号目录后建出来之后听不见（目录那一格 / manifest 那一格）"
    );
    assert_eq!(
        second,
        (true, true),
        "账号目录删掉重建之后失聪（目录那一格 / manifest 那一格）"
    );
}

/// ★★ 〔登录完成那一刻界面自己变「已登录」〕真 debouncer ＋ `AccountsEar`：起步就有的号目录 · 起步之后才建的号目录，
/// 各自写进凭据文件 ⇒ `on_path` 回 true；号目录里别的文件 ⇒ 不回 true。
#[cfg(target_os = "linux")]
#[test]
fn a_credentials_file_appearing_in_an_account_dir_is_heard() {
    let root = std::env::temp_dir().join(format!("ccm-cred-ear-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("accts");
    let old = dir.join("acct-a");
    std::fs::create_dir_all(&old).unwrap();
    let (etx, erx) = std::sync::mpsc::channel::<WatchEvent>();
    let mut debouncer = new_debouncer(Duration::from_millis(DEBOUNCE_MS), DebouncerSink(etx))
        .expect("debouncer 起不来");
    let mut ear = AccountsEar::new(&dir);
    ear.arm(&mut debouncer);
    // 写一份文件，等到一批事件里 `on_path` 回过 true（或期限到）。回到没到。
    let heard = |ear: &mut AccountsEar, debouncer: &mut _, path: &Path| -> bool {
        std::fs::write(path, b"{}").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut got = false;
        while !got && std::time::Instant::now() < deadline {
            if let Ok(WatchEvent::Notify(Ok(evs))) = erx.recv_timeout(Duration::from_millis(200)) {
                for ev in &evs {
                    got |= ear.on_path(debouncer, &ev.path);
                }
            }
        }
        got
    };
    let other = heard(&mut ear, &mut debouncer, &old.join("settings.json"));
    let at_start = heard(&mut ear, &mut debouncer, &old.join(".credentials.json"));
    let fresh = dir.join("acct-b");
    std::fs::create_dir_all(&fresh).unwrap();
    // 号目录出现那一格先到（重挂各号目录），之后那份凭据才听得见。
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !ear.subs.contains(&fresh) && std::time::Instant::now() < deadline {
        if let Ok(WatchEvent::Notify(Ok(evs))) = erx.recv_timeout(Duration::from_millis(200)) {
            for ev in &evs {
                ear.on_path(&mut debouncer, &ev.path);
            }
        }
    }
    let later = heard(&mut ear, &mut debouncer, &fresh.join(".credentials.json"));
    drop(debouncer);
    std::fs::remove_dir_all(&root).ok();
    assert!(!other, "号目录里别的文件不该当成「清单可能变了」");
    assert!(at_start, "起步就有的号目录里出现凭据文件，没听见");
    assert!(later, "起步之后才建的号目录里出现凭据文件，没听见");
}

// ═══ 身份标签对账 ═══════════════════════════════════════════════
//
// 夹具：一个有状态的假 tmux（一个会话 `$7`，`@ccm_sid` 存在一份文件里；`display-message` 读它、`set-option` 写它）＋
// 一个带 `TMUX_PANE` 的 `sleep` 当 claude（pidfile 带它的真 procStart）。真 tmux 一次都不碰（§48.3）。

/// 回（假 tmux 脚本，标签文件）。
fn fake_tmux_world(dir: &Path) -> (PathBuf, PathBuf) {
    std::fs::create_dir_all(dir).unwrap();
    let label = dir.join("label");
    let script = dir.join("tmux");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nst='{}'\ncase \"$*\" in\n  *display-message*) printf '$7\\t%s\\t1\\t\\n' \"$(cat \"$st\" 2>/dev/null)\";;\n  *set-option*) eval \"v=\\${{$#}}\"; printf '%s' \"$v\" > \"$st\";;\nesac\n",
            label.display()
        ),
    )
    .unwrap();
    (script, label)
}

/// 起一个住在 `pane` 里的「claude」，并在 `sessions` 下写它的 pidfile（`sid` · `status`）。
fn claude_in_pane(sessions: &Path, pane: &str, sid: &str, status: &str) -> std::process::Child {
    let kid = crate::control::identity_tag::tests::spawn_settled_sleep(|c| {
        c.env("TMUX_PANE", pane);
    });
    write_pidfile(sessions, kid.id(), sid, status);
    kid
}

fn write_pidfile(sessions: &Path, pid: u32, sid: &str, status: &str) -> PathBuf {
    std::fs::create_dir_all(sessions).unwrap();
    let ticks = proc_starttime(pid).expect("子进程的 starttime 读不到");
    let p = sessions.join(format!("{pid}.json"));
    std::fs::write(
        &p,
        format!(
            r#"{{"pid":{pid},"sessionId":"{sid}","cwd":"/x","kind":"interactive","procStart":"{ticks}","status":"{status}"}}"#
        ),
    )
    .unwrap();
    p
}

/// **标签被外部改掉之后会被纠正**：pidfile 重写（sid 没变）那一支 ＋ 对在跟会话的整批对账（tmux 探测到达时调的那一个）。
/// 要求：「tmux 探测结果到达、pidfile 重写（sid 没变那一支）时顺手比一次标签」。
#[test]
fn an_externally_changed_identity_tag_is_put_back() {
    let dir = std::env::temp_dir().join(format!("ccm-resync-retag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (script, label) = fake_tmux_world(&dir.join("tmux"));
    let _iso = crate::control::identity_tag::door::isolate_with(&script);
    let sessions = dir.join("sessions");
    let mut kid = claude_in_pane(&sessions, "%5", "resync-a", "busy");
    let (tx, _rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, false);
    let pidfile = sessions.join(format!("{}.json", kid.id()));
    process_session_added(&pidfile, &mut state, &mut sink);
    let read = || std::fs::read_to_string(&label).unwrap_or_default();
    assert_eq!(read(), "resync-a", "首次宣告就该打上");

    // ① 外部改掉 ⇒ pidfile 重写（sid 没变，只换了状态）⇒ 纠正。
    std::fs::write(&label, "bg-sid").unwrap();
    write_pidfile(&sessions, kid.id(), "resync-a", "idle");
    process_session_added(&pidfile, &mut state, &mut sink);
    let after_rewrite = read();

    // ② 再改掉 ⇒ 整批对账纠正、回写了 1 个；再对一次 ⇒ 0 个（值一样不动）。
    std::fs::write(&label, "bg-sid").unwrap();
    let wrote = retag_tracked(&state, None);
    let after_batch = read();
    let wrote_again = retag_tracked(&state, None);
    let _ = kid.kill();
    let _ = kid.wait();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        after_rewrite, "resync-a",
        "pidfile 重写没有纠正被改掉的标签"
    );
    assert_eq!(
        (after_batch.as_str(), wrote, wrote_again),
        ("resync-a", 1, 0)
    );
}

/// **对齐 = 拿盘上现实对后端的表，只对差异发帧**（与起步初扫同一个 `reconcile_sessions`）。
/// 表里：A（pidfile 删了）· B（进程死了、pidfile 还在）· C（活着、标签被外部改掉）；盘上多一个没跟的 D。
/// 整机一趟 ⇒ 移除 A、B · 宣告 D · 重打 C；只对 C 的一趟 ⇒ 只碰 C。期望全是手写的。
/// 要求：「pidfile 目录逐个重验（多的补 `session_added`，少的补移除）· 每个在跟的 pid 重判活 · …… · 只对差异发帧」。
#[test]
fn resync_reconciles_the_table_against_the_disk_and_emits_only_the_difference() {
    let dir = std::env::temp_dir().join(format!("ccm-resync-recon-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (script, label) = fake_tmux_world(&dir.join("tmux"));
    let _iso = crate::control::identity_tag::door::isolate_with(&script);
    let sessions = dir.join("sessions");
    let mut a = claude_in_pane(&sessions, "%1", "sid-a", "idle");
    let mut b = claude_in_pane(&sessions, "%2", "sid-b", "idle");
    let mut c = claude_in_pane(&sessions, "%5", "sid-c", "idle");
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(256);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, false);
    for k in [&a, &b, &c] {
        process_session_added(
            &sessions.join(format!("{}.json", k.id())),
            &mut state,
            &mut sink,
        );
    }
    while rx.try_recv().is_ok() {}
    std::fs::remove_file(sessions.join(format!("{}.json", a.id()))).unwrap();
    let _ = b.kill();
    let _ = b.wait();
    std::fs::write(&label, "bg-sid").unwrap();
    let mut d = claude_in_pane(&sessions, "%9", "sid-d", "idle");
    let frames = |rx: &mut tokio::sync::mpsc::Receiver<Frame>| {
        let mut v: Vec<String> = Vec::new();
        while let Ok(f) = rx.try_recv() {
            v.push(match f {
                Frame::SessionAdded { sid, .. } => format!("added {sid}"),
                Frame::SessionRemoved { sid, .. } => format!("removed {sid}"),
                other => other.loss_identity().kind.to_string(),
            });
        }
        v.sort();
        v
    };

    // 只对 C：D 不宣告、A/B 不移除，只重打 C。
    std::fs::write(&label, "bg-sid").unwrap();
    let one = reconcile_sessions(&sessions, &mut state, &mut sink, Some("sid-c"));
    let one_frames = frames(&mut rx);
    let one_label = std::fs::read_to_string(&label).unwrap_or_default();

    // 整机：再改一次标签。
    std::fs::write(&label, "bg-sid").unwrap();
    let all = reconcile_sessions(&sessions, &mut state, &mut sink, None);
    let all_frames = frames(&mut rx);
    for k in [&mut a, &mut c, &mut d] {
        let _ = k.kill();
        let _ = k.wait();
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        (one, one_frames, one_label.as_str()),
        (
            Reconciled {
                added: 0,
                removed: 0,
                retagged: 1,
                caught_up: 0
            },
            Vec::<String>::new(),
            "sid-c"
        ),
        "只对一个会话的那一趟碰了别的会话"
    );
    // D 的首次宣告也写一次标签（它住 %9，同一个假会话）⇒ 写入 2 处：C 的纠正 ＋ D 的首打。
    assert_eq!(
        (all, all_frames),
        (
            Reconciled {
                added: 1,
                removed: 2,
                retagged: 2,
                caught_up: 0
            },
            vec![
                "added sid-d".to_string(),
                "removed sid-a".to_string(),
                "removed sid-b".to_string()
            ]
        ),
        "整机对齐的差异不对"
    );
}

/// `resync` 等**每一份**在跑的 watcher 做完；中途退掉的那份（丢了应答端）不会把它挂住。
#[test]
fn resync_waits_for_every_live_watcher_and_never_hangs_on_a_gone_one() {
    let (tx1, rx1) = std::sync::mpsc::channel::<WatchEvent>();
    let (tx2, rx2) = std::sync::mpsc::channel::<WatchEvent>();
    let answers = std::thread::spawn(move || {
        for ev in rx1 {
            if let WatchEvent::Resync { only, done } = ev {
                let n = usize::from(only.as_deref() == Some("s1"));
                let _ = done.send(Reconciled {
                    added: 1,
                    removed: n,
                    retagged: 2,
                    caught_up: 3,
                });
            }
        }
    });
    // 第二份收到就丢（等价于它正在退出）。
    let drops = std::thread::spawn(move || for _ev in rx2 {});
    let w1 = live_enter(tx1);
    let w2 = live_enter(tx2);
    let got = resync(Some("s1"));
    live_leave(w1);
    live_leave(w2);
    answers.join().unwrap();
    drops.join().unwrap();
    assert_eq!(
        got,
        (
            Reconciled {
                added: 1,
                removed: 1,
                retagged: 2,
                caught_up: 3
            },
            1
        )
    );
}

/// 帧面 `resync` 的成品 == 跨语言金样的形状（键集合相等、每格是计数）；`sid` 给了却不是非空字符串 ⇒ `bad_args`。
/// 界面那一侧 `tests/frontend/ui/resync.vitest.ts` 按同一份金样解。
#[test]
fn resync_face_reply_matches_the_cross_language_golden() {
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/resync.golden.json")).unwrap();
    let got = crate::faces::resync_face::answer(&golden["request"]).expect("金样那份请求该答得出");
    let keys = |v: &serde_json::Value| -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    assert_eq!(keys(&got), keys(&golden["reply"]), "成品的键与金样不一致");
    // 五格计数 ＋ 两格能力事实（与 hello 同形：`[{command, code}]` · `[op]`）。
    for k in ["added", "removed", "retagged", "caught_up", "watchers"] {
        assert!(got[k].as_u64().is_some(), "`{k}` 不是计数：{got}");
    }
    let facts = got["unavailable"]
        .as_array()
        .expect("`unavailable` 不是数组");
    assert!(
        facts
            .iter()
            .all(|e| e["command"].is_string() && e["code"].is_string()),
        "`unavailable` 的项不是 {{command, code}}：{got}"
    );
    let ops = got["uncancellable"]
        .as_array()
        .expect("`uncancellable` 不是数组");
    assert_eq!(
        ops.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>(),
        crate::stream::inbound::uncancellable(),
        "`uncancellable` 不是命令表派生的那一份"
    );
    for bad in [
        serde_json::json!({"sid": 5}),
        serde_json::json!({"sid": ""}),
    ] {
        assert_eq!(
            crate::faces::resync_face::answer(&bad).map_err(|(c, _)| c),
            Err("bad_args"),
            "{bad}"
        );
    }
}

/// 〔「每个 tab『重新读取』（从游标补读 jsonl）」〕文件事件丢了一拍（这里干脆不发）：
/// `resync{sid}` 那一趟从游标把漏的那一行补出来，别的会话不碰。
/// 应答的 `caught_up` == 这一趟补读出的行数：单 sid 那趟 1；随后整机那趟 a 又漏 1 行、b 漏 2 行 ⇒ 3（各会话相加）。
#[test]
fn resync_for_one_sid_catches_up_its_jsonl_from_the_cursor() {
    let dir = std::env::temp_dir().join(format!("ccm-resync-catchup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (script, _label) = fake_tmux_world(&dir.join("tmux"));
    let _iso = crate::control::identity_tag::door::isolate_with(&script);
    let sessions = dir.join("sessions");
    let proj = dir.join("projects").join("p");
    std::fs::create_dir_all(&proj).unwrap();
    let mut a = claude_in_pane(&sessions, "%1", "sid-a", "idle");
    let mut b = claude_in_pane(&sessions, "%2", "sid-b", "idle");
    for sid in ["sid-a", "sid-b"] {
        std::fs::write(proj.join(format!("{sid}.jsonl")), "{\"n\":0}\n").unwrap();
    }
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(256);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, false);
    for k in [&a, &b] {
        process_session_added(
            &sessions.join(format!("{}.json", k.id())),
            &mut state,
            &mut sink,
        );
    }
    while rx.try_recv().is_ok() {}
    for sid in ["sid-a", "sid-b"] {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(proj.join(format!("{sid}.jsonl")))
            .unwrap();
        writeln!(f, "{{\"n\":1}}").unwrap();
    }
    let one = resync_sessions(&sessions, &mut state, &mut sink, Some("sid-a")).caught_up;
    let mut lines: Vec<String> = Vec::new();
    while let Ok(f) = rx.try_recv() {
        if let Frame::Line { session_id, .. } = f {
            lines.push(session_id);
        }
    }
    for (sid, n) in [("sid-a", 2), ("sid-b", 2)] {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(proj.join(format!("{sid}.jsonl")))
            .unwrap();
        writeln!(f, "{{\"n\":{n}}}").unwrap();
    }
    let all = resync_sessions(&sessions, &mut state, &mut sink, None).caught_up;
    for k in [&mut a, &mut b] {
        let _ = k.kill();
        let _ = k.wait();
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(lines, vec!["sid-a".to_string()]);
    assert_eq!((one, all), (1, 3), "补读行数（单 sid · 整机）不对");
}

/// `ccm` resume 要问「此刻哪些会话在跑」：一次性扫描与起步初扫同一条判活（pid 在 · 不是后台任务 ·
/// add-time 冒名判定）。家目录放在非 ASCII 路径下（纪律 25：主树住 `~/文档/…`）。
#[cfg(target_os = "linux")]
#[test]
fn fix_the_one_shot_scan_sees_exactly_the_live_interactive_sessions() {
    let home = std::env::temp_dir().join(format!("ccm-fix-文档-{}", std::process::id()));
    let sessions = crate::agents::claudecode::paths::sessions_root(&home);
    std::fs::create_dir_all(&sessions).expect("建目录");
    let me = std::process::id();
    let ticks = crate::platform::proc::proc_starttime(me).expect("读得到自己的 starttime");
    let write = |pid: u32, body: serde_json::Value| {
        std::fs::write(sessions.join(format!("{pid}.json")), body.to_string()).expect("写 pidfile")
    };
    write(
        me,
        serde_json::json!({"sessionId": "live-1", "procStart": ticks.to_string(), "kind": "interactive"}),
    );
    // 死进程（pid 上限之外，必不在）· 后台任务 · 冒名（procStart 对不上且 cmdline 不像 agent）。
    write(4_194_300, serde_json::json!({"sessionId": "dead-1"}));
    let bg = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("起 sleep");
    write(
        bg.id(),
        serde_json::json!({"sessionId": "bg-1", "kind": "bg"}),
    );
    let mut imp = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("起 sleep");
    write(
        imp.id(),
        serde_json::json!({"sessionId": "imp-1", "procStart": "1"}),
    );
    let got = running_sessions(&home);
    let mut bg = bg;
    let _ = bg.kill();
    let _ = imp.kill();
    let _ = bg.wait();
    let _ = imp.wait();
    std::fs::remove_dir_all(&home).ok();
    assert_eq!(got, vec![("live-1".to_string(), me)]);
    // 接线：`ccm` 入口注入的就是这一份扫描（恰一处）。
    let main = crate::guard_support::production_code(include_str!("../../../src/backend/main.rs"));
    guard_core::find_pinned(&main, "observe::watcher::running_sessions(&home)")
        .unwrap_or_else(|e| panic!("`main.rs` 注入的不是观测层那一份扫描：{e}"));
}

/// 〔要求住址 「`session.tasks` 推送改 `chan.subscribe(origin, …)`，监视进后端，monitor 那份 notify 删」〕
/// 任务目录里的动静 ⇒ 按会话 sid 去重（第一段）；根目录自己 · 别处的路径不算。
#[test]
fn tasks_touched_names_each_session_once_per_batch() {
    let t = std::path::Path::new("/h/.claude/tasks");
    let paths = [
        t.join("s1").join("1.json"),
        t.join("s1").join(".lock"),
        t.join("s2"),
        t.to_path_buf(),
        std::path::PathBuf::from("/h/.claude/projects/p/s3.jsonl"),
    ];
    assert_eq!(
        tasks_touched(paths.iter().map(|p| p.as_path()), t),
        vec!["s1".to_string(), "s2".to_string()]
    );
    assert!(tasks_touched(std::iter::empty(), t).is_empty());
}

/// 真 inotify：`tasks/` 起步不在、后来才建 ⇒ `agent_home` 那道耳朵把它挂上；之后往某会话目录写一份任务 ⇒
/// 那一批里认得出那个 sid（`tasks_touched` 与 `watch_loop` 同一个函数）。
#[cfg(target_os = "linux")]
#[test]
fn a_task_written_after_start_is_heard_for_its_session() {
    let root = std::env::temp_dir().join(format!("ccm-mig3b-tasks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("claude-home");
    std::fs::create_dir_all(&home).unwrap();
    let projects = crate::agents::claudecode::paths::projects_root(&home);
    let sessions = crate::agents::claudecode::paths::sessions_root(&home);
    let tasks = crate::observe::tasks_query::tasks_root(&home);
    let (etx, erx) = std::sync::mpsc::channel::<WatchEvent>();
    let mut debouncer = new_debouncer(Duration::from_millis(DEBOUNCE_MS), DebouncerSink(etx))
        .expect("debouncer 起不来");
    let mut ears = HomeEars::new(&home, &projects, &sessions);
    ears.arm(&mut debouncer);
    assert!(!ears.tasks_watched, "夹具坏了：tasks/ 一开始就在");
    let (tx, _rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(projects.clone(), false, false);
    std::fs::create_dir_all(&tasks).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !ears.tasks_watched && std::time::Instant::now() < deadline {
        if let Ok(WatchEvent::Notify(Ok(evs))) = erx.recv_timeout(Duration::from_millis(200)) {
            for ev in evs {
                ears.on_path(&mut debouncer, &ev.path, &mut state, &mut sink);
            }
        }
    }
    std::fs::create_dir_all(tasks.join("sid-t1")).unwrap();
    std::fs::write(tasks.join("sid-t1").join("1.json"), b"{}").unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut heard: Vec<String> = Vec::new();
    while heard.is_empty() && std::time::Instant::now() < deadline {
        if let Ok(WatchEvent::Notify(Ok(evs))) = erx.recv_timeout(Duration::from_millis(200)) {
            heard = tasks_touched(evs.iter().map(|ev| ev.path.as_path()), &ears.tasks);
        }
    }
    let watched = ears.tasks_watched;
    drop(debouncer);
    std::fs::remove_dir_all(&root).ok();
    assert!(watched, "`tasks/` 后建出来之后没挂上");
    assert_eq!(
        heard,
        vec!["sid-t1".to_string()],
        "写进会话任务目录的那一份没被听见 / 没认出 sid"
    );
}

/// 接线：`Notify` 那一臂里恰好一处问 `tasks_touched`、逐个发 `Frame::TasksChanged`，排在逐条处理事件的 `for` 之前。
#[test]
fn the_notify_arm_reports_task_changes_before_the_per_event_loop() {
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/observe/watcher.rs"
    ));
    let ask = guard_core::find_pinned(&prod, "tasks_touched(events.iter()")
        .expect("Notify 那一臂里不是恰好一处问 tasks_touched");
    let emit = prod[ask..]
        .find("sink.send(Frame::TasksChanged { sid });")
        .map(|k| ask + k)
        .expect("问完之后没有发 tasks_changed");
    let per_event = prod[ask..]
        .find("for ev in events {")
        .map(|k| ask + k)
        .expect("问完之后没有逐条处理事件的 for");
    assert!(
        ask < emit && emit < per_event,
        "问与发要排在逐条处理之前（逐条那一段里有 continue）"
    );
}

/// 子运行那一条（适配层 `run_of` 答得出）一轮收尾 ≠ 主运行一轮结束：主运行那条发 `TurnEnd`，子运行那条不发；
/// 两条的 `line` 都带上对账键（`RecordFace::response_id`）。守的要求：子 agent 的事归它自己的运行，不进主运行。
#[test]
fn a_sub_runs_turn_end_is_not_the_main_runs() {
    let (tx, mut rx) = mpsc::channel::<Frame>(16);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(PathBuf::from("/nonexistent-projects"), false, false);
    let main = r#"{"type":"assistant","uuid":"u-main","message":{"id":"m1","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"x"}]}}"#;
    let sub = r#"{"type":"assistant","uuid":"u-sub","isSidechain":true,"agentId":"a1","message":{"id":"m2","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"x"}]}}"#;
    for (i, raw) in [main, sub].into_iter().enumerate() {
        let line = ReadLine {
            seq: i as u64,
            raw: raw.to_string(),
            byte_offset: 0,
            start: 0,
        };
        send_line("s", "/p/s.jsonl", line, &mut state, &mut sink);
    }
    let mut got = Vec::new();
    while let Ok(f) = rx.try_recv() {
        got.push(match f {
            Frame::Line { rid, .. } => format!("line {}", rid.unwrap_or_default()),
            Frame::TurnEnd { uuid, .. } => format!("turn_end {uuid}"),
            other => format!("{other:?}"),
        });
    }
    assert_eq!(got, vec!["line m1", "turn_end u-main", "line m2"]);
}

/// 实时流：排队那一句的 `record.at` 是打字时刻（打字那一行早先在同一份记录里流过）；打字那一行自己不出成品；
/// 没有自己身份的那一条 `id` 按行的起点偏移合成。打字时刻表按会话记录分：别的会话里同一句话配不上。
#[test]
fn a_queued_line_on_the_live_stream_carries_the_moment_it_was_typed() {
    let (tx, mut rx) = mpsc::channel::<Frame>(16);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(PathBuf::from("/nonexistent-projects"), false, false);
    let enq = r#"{"type":"queue-operation","operation":"enqueue","timestamp":"2026-01-02T03:00:00.000Z","content":"also this"}"#;
    let rem = r#"{"type":"queue-operation","operation":"remove","timestamp":"2026-01-02T03:02:00.000Z","content":"also this"}"#;
    for (i, (path, raw, start)) in [
        ("/p/a.jsonl", enq, 0),
        ("/p/a.jsonl", rem, 300),
        ("/p/b.jsonl", rem, 0),
    ]
    .into_iter()
    .enumerate()
    {
        let line = ReadLine {
            seq: i as u64,
            raw: raw.to_string(),
            byte_offset: 0,
            start,
        };
        send_line("s", path, line, &mut state, &mut sink);
    }
    let mut got = Vec::new();
    while let Ok(f) = rx.try_recv() {
        if let Frame::Line { record, .. } = f {
            got.push(record.map(|r| (r.id, r.at.unwrap_or_default())));
        }
    }
    assert_eq!(
        got,
        vec![
            None,
            Some(("@300".to_string(), "2026-01-02T03:00:00.000Z".to_string())),
            Some(("@0".to_string(), "2026-01-02T03:02:00.000Z".to_string())),
        ]
    );
}

/// `--with-raw`：这条流索要了 ⇒ 每一行 `line` 带那一行原文（解析不出的行也带）；没索要 ⇒ 一格都不带。
#[test]
fn line_frames_carry_the_raw_text_only_when_the_stream_asked() {
    let rows = [
        r#"{"type":"user","uuid":"u1","message":{"role":"user","content":"x"}}"#,
        "not json at all",
    ];
    for asked in [false, true] {
        let (tx, mut rx) = mpsc::channel::<Frame>(16);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(PathBuf::from("/nonexistent-projects"), false, false);
        state.with_raw = asked;
        for (i, raw) in rows.iter().enumerate() {
            let line = ReadLine {
                seq: i as u64,
                raw: raw.to_string(),
                byte_offset: 0,
                start: 0,
            };
            send_line("s", "/p/s.jsonl", line, &mut state, &mut sink);
        }
        let mut got = Vec::new();
        while let Ok(f) = rx.try_recv() {
            if let Frame::Line { raw, .. } = f {
                got.push(raw);
            }
        }
        let want: Vec<Option<String>> = rows.iter().map(|r| asked.then(|| r.to_string())).collect();
        assert_eq!(got, want, "索要了 raw = {asked}");
    }
}

/// 冻结格 `line.raw`（两个前端的契约面）：真从文件读出来的那一行，`raw` 逐字节等于记录里那一行去掉行尾（`\n` / `\r\n`）——
/// 不重排键、不改转义、不动空白与非 ASCII；成品 `record` 换形不碰它。
#[test]
fn line_raw_is_the_record_line_byte_for_byte() {
    let dir = std::env::temp_dir().join(format!("ccm-rawbytes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, false);
    state.with_raw = true;
    let path = dir.join("sess-raw.jsonl");
    state.active_sids.insert("sess-raw".to_string());
    let rows = [
        r#"{"type":"user","uuid":"u1","message":{"role":"user","content":"a \"q\" \u00e9 文  x"},"z":1,  "a":2}"#,
        r#"{ "type" : "assistant" ,"uuid":"a1","message":{"content":[{"type":"text","text":"tab\there"}]}}"#,
        "not json at all",
    ];
    let content = format!("{}\n{}\r\n{}\n", rows[0], rows[1], rows[2]);
    std::fs::write(&path, &content).unwrap();
    process_jsonl(&path, &mut state, &mut sink);
    let mut got = Vec::new();
    while let Ok(f) = rx.try_recv() {
        if let Frame::Line { raw, .. } = f {
            got.push(raw.expect("索要了 raw 却没带"));
        }
    }
    assert_eq!(got, rows.iter().map(|r| r.to_string()).collect::<Vec<_>>());
    std::fs::remove_dir_all(&dir).ok();
}

/// 宣告会话找它的记录文件：先查「sid → 记录文件」那张表（起步一遍、之后跟着记录文件的事件改），查不到才整棵走一遍。
/// 表与整棵走得出的同一份（同 sid 两份 ⇒ 都在、按修改时刻新的在前）；删掉的不交；新长出来的经事件进表。
#[test]
fn announcing_a_session_finds_its_records_from_the_table_not_a_full_walk() {
    let base = std::env::temp_dir().join(format!("ccm-c4-sidfiles-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    let projects = base.join("projects");
    let sid = "0000aaaa-0000-4000-8000-00000000c4c4";
    let a = projects.join("-p-old").join(format!("{sid}.jsonl"));
    let b = projects.join("-p-new").join(format!("{sid}.jsonl"));
    for p in [&a, &b] {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    }
    std::fs::write(&a, "{}\n").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&b, "{}\n").unwrap();
    let mut st = ReaderState::new(projects.clone(), false, false);
    assert!(st.sid_files.is_none(), "表应在第一次宣告时才建");
    assert_eq!(sid_jsonls(&mut st, sid), find_sid_jsonls(&projects, sid));
    assert_eq!(sid_jsonls(&mut st, sid), vec![b.clone(), a.clone()]);
    assert!(st.sid_files.is_some(), "第一次宣告之后表该在了");
    // 表在之后：整棵树换成一个走不到的根，表照样答得出（证明答案来自表，不是又走了一遍）。
    st.projects = base.join("nowhere");
    assert_eq!(sid_jsonls(&mut st, sid), vec![b.clone(), a.clone()]);
    // 删一份 ＋ 它的事件 ⇒ 不交它。
    std::fs::remove_file(&a).unwrap();
    note_jsonl(&mut st, &a);
    assert_eq!(sid_jsonls(&mut st, sid), vec![b.clone()]);
    // 新长一份（另一条会话）＋ 它的事件 ⇒ 进表。
    let sid2 = "0000aaaa-0000-4000-8000-00000000c4c5";
    let c = projects.join("-p-new").join(format!("{sid2}.jsonl"));
    std::fs::write(&c, "{}\n").unwrap();
    note_jsonl(&mut st, &c);
    assert_eq!(sid_jsonls(&mut st, sid2), vec![c.clone()]);
    // 表里没有的 sid ⇒ 整棵走一遍（根换回来才走得到）。
    st.projects = projects.clone();
    let sid3 = "0000aaaa-0000-4000-8000-00000000c4c6";
    let d = projects.join("-p-old").join(format!("{sid3}.jsonl"));
    std::fs::write(&d, "{}\n").unwrap();
    assert_eq!(
        sid_jsonls(&mut st, sid3),
        vec![d.clone()],
        "表里没有就该整棵走一遍"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 配置文件（`profiles.toml`）那道耳朵，真 debouncer：直接写 · 写旁名再换名上位（设置窗与 `files-put` 的写法）都听得见；
/// 同目录别的文件不算；目录起步不在、后建出来也听得见。
#[cfg(target_os = "linux")]
#[test]
fn the_profiles_file_is_heard_written_or_renamed_into_place_and_nothing_else_counts() {
    let root = std::env::temp_dir().join(format!("ccm-profiles-ear-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let dir = root.join(".cc-monitor");
    let file = dir.join("profiles.toml");
    let (etx, erx) = std::sync::mpsc::channel::<WatchEvent>();
    let mut debouncer = new_debouncer(Duration::from_millis(DEBOUNCE_MS), DebouncerSink(etx))
        .expect("debouncer 起不来");
    let mut ear = ProfilesEar::new(file.clone()).expect("有上一层");
    ear.arm(&mut debouncer);
    // 做一件事，等到那一批（最多 20 秒）：回这一批里配置文件算不算动了。
    let mut heard = |ear: &mut ProfilesEar, debouncer: &mut _, act: &dyn Fn()| -> bool {
        act();
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let mut got = false;
        while std::time::Instant::now() < deadline {
            match erx.recv_timeout(Duration::from_millis(300)) {
                Ok(WatchEvent::Notify(Ok(evs))) => {
                    got |= ear.touched(debouncer, evs.iter().map(|e| e.path.as_path()));
                    if got {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) if got => break,
                Err(_) => {}
            }
        }
        got
    };
    let made = heard(&mut ear, &mut debouncer, &|| {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&file, "[cc]\n").unwrap();
    });
    let other = heard(&mut ear, &mut debouncer, &|| {
        std::fs::write(dir.join("aliases.sh"), "x").unwrap()
    });
    let renamed = heard(&mut ear, &mut debouncer, &|| {
        std::fs::write(dir.join(".profiles.toml.tmp"), "[cct]\n").unwrap();
        std::fs::rename(dir.join(".profiles.toml.tmp"), &file).unwrap();
    });
    drop(debouncer);
    std::fs::remove_dir_all(&root).ok();
    assert!(made, "目录后建出来、写进配置文件 ⇒ 要听得见");
    assert!(!other, "同目录别的文件不算配置文件变了");
    assert!(renamed, "写旁名再换名上位 ⇒ 要听得见");
}

/// 主线外清单实时那一路：冷接（只推游标）时历史里已经有回退 ⇒ 宣告之后发一帧整份；之后回退重发 ⇒ 再发一帧；
/// 接在主线末梢上的新行 ⇒ 清单没变、不发。
#[cfg(target_os = "linux")]
#[test]
fn the_off_main_list_goes_out_whole_and_only_when_it_changes() {
    let _iso = crate::control::identity_tag::door::isolate();
    let dir = std::env::temp_dir().join(format!("ccm-branch-{}", std::process::id()));
    let proj = dir.join("projects").join("proj-b");
    std::fs::create_dir_all(&proj).unwrap();
    let pid = std::process::id();
    let ticks = proc_starttime(pid).expect("own starttime");
    let jsonl = proj.join("br-sid.jsonl");
    let line = |id: &str, parent: &str, at: &str, ty: &str| {
        format!(
            r#"{{"type":"{ty}","uuid":"{id}","parentUuid":{p},"timestamp":"{at}","message":{{"role":"{ty}","content":"w"}}}}"#,
            p = if parent.is_empty() {
                "null".to_string()
            } else {
                format!("\"{parent}\"")
            }
        ) + "\n"
    };
    let mut body = line("u1", "", "t1", "user")
        + &line("a1", "u1", "t2", "assistant")
        + &line("u2", "a1", "t3", "user")
        + &line("u3", "a1", "t4", "user");
    std::fs::write(&jsonl, &body).unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    let mut state = ReaderState::new(dir.join("projects"), false, true);
    let pidfile = dir.join(format!("{pid}.json"));
    std::fs::write(
        &pidfile,
        format!(r#"{{"pid":{pid},"sessionId":"br-sid","cwd":"/p","procStart":"{ticks}"}}"#),
    )
    .unwrap();
    process_session_added(&pidfile, &mut state, &mut sink);
    let mut branches = Vec::new();
    while let Ok(f) = rx.try_recv() {
        if let Frame::SessionBranch { sid, path, off } = f {
            assert_eq!(sid, "br-sid");
            assert_eq!(path, jsonl.to_string_lossy());
            branches.push(off);
        }
    }
    assert_eq!(branches, [vec!["u2".to_string()]]);
    // 接在主线末梢上 ⇒ 不发。
    body += &line("a3", "u3", "t5", "assistant");
    std::fs::write(&jsonl, &body).unwrap();
    process_jsonl(&jsonl, &mut state, &mut sink);
    let mut got = Vec::new();
    while let Ok(f) = rx.try_recv() {
        if let Frame::SessionBranch { off, .. } = f {
            got.push(off);
        }
    }
    assert!(got.is_empty(), "清单没变不该发：{got:?}");
    // 在 a1 之后再回退重发 ⇒ 整份再发一次。
    body += &line("u4", "a1", "t6", "user");
    std::fs::write(&jsonl, &body).unwrap();
    process_jsonl(&jsonl, &mut state, &mut sink);
    while let Ok(f) = rx.try_recv() {
        if let Frame::SessionBranch { off, .. } = f {
            got.push(off);
        }
    }
    assert_eq!(
        got,
        [vec!["u2".to_string(), "u3".to_string(), "a3".to_string()]]
    );
    std::fs::remove_dir_all(&dir).ok();
}
