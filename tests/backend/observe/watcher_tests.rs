/// ★ **事件分派不许有兜底臂**〔audit-0805 08-06〕。
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
#[test]
fn events_channel_is_created_before_the_initial_scan() {
    let src = crate::guard_support::production_code(include_str!(
        "../../../src/backend/observe/watcher.rs"
    ));
    let tx_at = src
        .find("state.events_tx = Some(events_tx.clone());")
        .expect("找不到 events_tx 注入点——守卫锚点漂了，先修锚点别改断言");
    let scan_at = src
        .find("WalkDir::new(&sessions)")
        .expect("找不到初始扫描的锚点（`WalkDir::new(&sessions)`）——扫描改写了就把本条一起改");
    // 锚点唯一性：两个都必须**恰好一处**，否则「谁在前」比的可能是别处那一份。
    assert_eq!(
        src.matches("WalkDir::new(&sessions)").count(),
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

// ---------- P5（zero-poll-liveness）：快照差分 → 正向死亡帧 ----------

use std::collections::BTreeSet;

fn names(v: &[&str]) -> Option<BTreeSet<String>> {
    Some(v.iter().map(|s| s.to_string()).collect())
}

/// ★ `K-R96`：切给那张唯一会话快照的行 —— 名字取第 0 列、`@ccm_sid` 取**末**列。
///
/// 顺带钉住两条：段数不等于 `TMUX_LS_FMT_FIELDS` 的行**整行丢掉**（下溢是通道被改写、
/// 过溢是有人往 `@ccm_sid` 里塞了 TAB —— 两种都不许当好数据）；`NO_TMUX` 哨兵不是会话。
#[test]
fn session_rows_carry_the_name_and_the_ccm_sid_and_nothing_else() {
    use crate::common::session_snapshot::SessionRow;
    let raw = "s1\t/p\tclaude\t1\t2\tsid-a\ns2\t/q\tbash\t0\t1\t\n";
    assert_eq!(
        session_rows(raw),
        vec![
            SessionRow {
                name: "s1".into(),
                ccm_sid: "sid-a".into()
            },
            SessionRow {
                name: "s2".into(),
                ccm_sid: String::new()
            },
        ]
    );
    assert!(session_rows("NO_TMUX\n").is_empty(), "哨兵不是会话");
    assert!(session_rows("只有一段\n").is_empty(), "下溢的行不当好数据");
    assert!(
        session_rows("s\t/p\tc\t1\t2\tsid\t多出来一段\n").is_empty(),
        "过溢的行不当好数据（`last()` 那种写法会在这里取到半截）"
    );
}

/// ★★ `K-R96` 死值验（observe 这一侧）：**观测无效时快照一个字都不许动。**
///
/// 把 `NoTmux`/`Unobservable` 那一支改成 `publish(Vec::new())`（= 「都没了」），
/// 本条当场红 —— 那正是「观测失败被读成零会话，把活会话全部误 retire」的那一下，
/// 只不过这一回它会顺着快照传染到 control 侧的判活。
#[test]
fn an_invalid_observation_leaves_the_shared_snapshot_untouched() {
    let snap = crate::common::session_snapshot::SessionSnapshot::with_prober(|| {
        panic!("本条一次都不该去探 —— 它量的是 `publish` 那一侧")
    });
    let mut prev: Option<BTreeSet<String>> = None;
    // 先让快照里有点东西（走 `Sessions` 那一支发布）。
    let _ = diff_closed_into(
        &mut prev,
        &TmuxObservation::Sessions("keep-cc\t/p\tclaude\t1\t1\tsid-k\n".into()),
        &snap,
    );
    let warmed = snap.peek();
    assert!(
        warmed.iter().any(|r| r.name == "keep-cc"),
        "`Sessions` 那一支没往快照里发布（实得 {warmed:?}）—— 本条此刻在空转"
    );
    for obs in [TmuxObservation::NoTmux, TmuxObservation::Unobservable] {
        let mut p = prev.clone();
        assert!(diff_closed_into(&mut p, &obs, &snap).is_empty());
        assert_eq!(
            snap.peek(),
            warmed,
            "观测无效那一支动了共享快照 —— 「不知道」被写成了「都没了」"
        );
    }
    // 而「server 没了」是**有效观测**：那一支必须把表清空（不是「不知道」）。
    let mut p = prev.clone();
    let _ = diff_closed_into(&mut p, &TmuxObservation::NoServer, &snap);
    assert!(
        snap.peek().is_empty(),
        "server 没了却还在表里留着会话 —— 判活会把它们报成活的"
    );
}

#[test]
fn session_names_takes_first_column_only() {
    let raw = "s1\t/p\tclaude\t1\t2\tsid-a\ns2\t/q\tbash\t0\t1\t\n";
    assert_eq!(
        session_names(raw),
        ["s1", "s2"].iter().map(|s| s.to_string()).collect()
    );
}

#[test]
fn session_names_ignores_blank_lines_and_no_tmux_sentinel() {
    assert!(session_names("\n\n").is_empty());
    assert!(session_names("NO_TMUX\n").is_empty());
}

#[test]
fn diff_reports_only_the_disappeared_one() {
    let mut prev = names(&["a", "b", "c"]);
    let closed = diff_closed(
        &mut prev,
        &TmuxObservation::Sessions("a\t/p\tsh\t0\t1\t\nc\t/p\tsh\t0\t1\t\n".into()),
    );
    assert_eq!(closed, vec!["b".to_string()]);
    assert_eq!(prev, names(&["a", "c"]));
}

/// 信号会合并 ⇒ 一次差分要能报出**所有**消失的（逐事件必漏）。
#[test]
fn diff_reports_all_disappeared_at_once() {
    let mut prev = names(&["a", "b", "c", "d"]);
    let closed = diff_closed(
        &mut prev,
        &TmuxObservation::Sessions("b\t/p\tsh\t0\t1\t\n".into()),
    );
    assert_eq!(
        closed,
        vec!["a".to_string(), "c".to_string(), "d".to_string()]
    );
}

#[test]
fn server_gone_closes_everything() {
    let mut prev = names(&["a", "b"]);
    let closed = diff_closed(&mut prev, &TmuxObservation::NoServer);
    assert_eq!(closed, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(prev, Some(BTreeSet::new()));
}

/// ★ 最要紧的一条：**观测失败 ≠ 都没了**。
/// 报一堆死亡帧会把活着的会话全部误 retire —— 这正是 P1 当年那条
/// 「空 `raw` 同时意味着零会话和出错」的教训在死亡帧这条路上的复发点。
#[test]
fn unobservable_never_reports_deaths_and_keeps_snapshot() {
    for obs in [TmuxObservation::Unobservable, TmuxObservation::NoTmux] {
        let mut prev = names(&["a", "b"]);
        assert!(
            diff_closed(&mut prev, &obs).is_empty(),
            "{obs:?} 不该报死亡"
        );
        assert_eq!(prev, names(&["a", "b"]), "{obs:?} 不该动快照");
    }
}

/// 第一次观测没有「上一份」可比 ⇒ 不报任何死亡（否则后端一启动就诬告一批）。
#[test]
fn first_observation_reports_nothing() {
    let mut prev = None;
    let closed = diff_closed(
        &mut prev,
        &TmuxObservation::Sessions("a\t/p\tsh\t0\t1\t\n".into()),
    );
    assert!(closed.is_empty());
    assert_eq!(prev, names(&["a"]));
}

/// 幂等：同一份观测再来一次，不该重复报死亡。
#[test]
fn repeated_identical_observation_reports_nothing() {
    let mut prev = names(&["a"]);
    let obs = TmuxObservation::Sessions("a\t/p\tsh\t0\t1\t\n".into());
    assert!(diff_closed(&mut prev, &obs).is_empty());
    assert!(diff_closed(&mut prev, &obs).is_empty());
}

/// 新会话出现不该被当成死亡（差分方向别搞反）。
#[test]
fn new_session_is_not_a_death() {
    let mut prev = names(&["a"]);
    let closed = diff_closed(
        &mut prev,
        &TmuxObservation::Sessions("a\t/p\tsh\t0\t1\t\nb\t/p\tsh\t0\t1\t\n".into()),
    );
    assert!(closed.is_empty());
    assert_eq!(prev, names(&["a", "b"]));
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
    // 🔴 〔步 7c 剖分 2026-09-19 · `设计/16 §5.1`＋`§6.2` D 类〕
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
    // 🔴 〔步 7c 剖分 2026-09-19 · `设计/16 §5.1`＋`§6.2` D 类〕
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
    // 🔴 〔步 7c 剖分 2026-09-19 · `设计/16 §5.1`＋`§6.2` D 类〕
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

/// ★ P3 收紧判据：`tmux ls` rc=1 时，**只有在我们记着的 server pid 确实已经不在**
/// 才认"零会话"；pid 还在 = 真异常 ⇒ `Unobservable`（保守跳过，不误 retire）。
///
/// **刻意不依赖"pidfd 是否已经醒过"**——那会有个危险失效模式：pidfd 路万一没醒，
/// 状态永停 `Alive`，rc=1 被永久压成 `Unobservable` ⇒ 永不 retire。改成直接查 `/proc`。
#[test]
fn no_server_is_tightened_only_when_the_pid_is_really_still_alive() {
    // ① 记着的 server 是**自己**（铁定活着）+ rc=1 ⇒ 真异常 ⇒ Unobservable
    let me = std::process::id();
    assert_eq!(
        classify_with_server_state(TmuxObservation::NoServer, ServerState::Alive(me)),
        TmuxObservation::Unobservable,
        "server 明明活着而 tmux ls 连不上 = 真异常，不该当成零会话"
    );

    // ② 记着的 server 已死 + rc=1 ⇒ 原样通过（真的没 server）
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn");
    let dead = child.id();
    child.kill().expect("kill");
    child.wait().expect("reap");
    assert_eq!(
        classify_with_server_state(TmuxObservation::NoServer, ServerState::Alive(dead)),
        TmuxObservation::NoServer,
        "server 真没了就该照常判零会话"
    );

    // ③ Unknown / Gone 一律不收紧（还没探过、或已知没了）
    for st in [ServerState::Unknown, ServerState::Gone] {
        assert_eq!(
            classify_with_server_state(TmuxObservation::NoServer, st),
            TmuxObservation::NoServer,
            "{st:?} 下不该收紧"
        );
    }

    // ④ **收紧只作用于 NoServer**（守卫范围必须等于性质范围）：别的观测原样穿过，
    //    尤其 `ServerEmpty`（exit-empty off 下 server 活着 + 零会话，是合法观测）。
    for obs in [
        TmuxObservation::Sessions("x".into()),
        TmuxObservation::ServerEmpty,
        TmuxObservation::NoTmux,
        TmuxObservation::Unobservable,
    ] {
        assert_eq!(
            classify_with_server_state(obs.clone(), ServerState::Alive(me)),
            obs,
            "{obs:?} 不该被 server 状态改写"
        );
    }
}

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

/// `query_tmux_server`：**死 socket 上不该把 server 拉活**、且拿不到 pid/socket。
///
/// 这里不能直接调 `query_tmux_server()`（它走默认 socket = 用户实况），所以只钉住
/// 「探测脚本对没有 server 的情形返回全 None」这条语义——用 PATH 前置一个 rc=1 的假 tmux。
#[test]
fn tmux_server_query_yields_nothing_without_a_server() {
    let dir = std::env::temp_dir().join(format!("ccm-p3-q-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let fake = dir.join("tmux");
    std::fs::write(&fake, "#!/bin/sh\necho 'error connecting' >&2\nexit 1\n").expect("write");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&fake).expect("stat").permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&fake, perm).expect("chmod");
    }
    // 跑与生产同一段脚本，只把 PATH 指向假 tmux。
    let script = "if command -v tmux >/dev/null 2>&1; then exec tmux display-message -p '#{pid}\t#{socket_path}' 2>/dev/null; else exit 97; fi";
    let out = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .env("PATH", dir.display().to_string())
        .output()
        .expect("spawn");
    assert_ne!(out.status.code(), Some(0), "没有 server 时脚本不该 rc=0");
    assert!(
        String::from_utf8_lossy(&out.stdout).trim().is_empty(),
        "没有 server 时不该有 stdout"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------- P1（zero-poll-liveness）：tmux 观测四态 ----------

/// 纯分类：四态各自的判据。**P0 实测的状态空间**（见
/// `.claude/planned-build/zero-poll-liveness/features/P0-machine-facts.md` §3 ④）。
#[test]
fn tmux_probe_classifies_four_states() {
    // rc=0 + 非空 → 有会话
    assert_eq!(
        classify_tmux_probe(Some(0), "s1\t/p\tclaude\t1\t1\tsid-a\n"),
        TmuxObservation::Sessions("s1\t/p\tclaude\t1\t1\tsid-a\n".to_string())
    );
    // rc=0 + 空 → **server 活但零会话**（exit-empty off）。P3 起与 rc=1 分开。
    assert_eq!(
        classify_tmux_probe(Some(0), ""),
        TmuxObservation::ServerEmpty
    );
    assert_eq!(
        classify_tmux_probe(Some(0), "  \n"),
        TmuxObservation::ServerEmpty,
        "只有空白也算空"
    );
    // rc=1 → **server 不在**（两种 stderr 措辞都走这里，刻意不看 stderr）
    assert_eq!(classify_tmux_probe(Some(1), ""), TmuxObservation::NoServer);
    // 约定 rc → 无 tmux
    assert_eq!(
        classify_tmux_probe(Some(TMUX_PROBE_NO_TMUX_RC), ""),
        TmuxObservation::NoTmux
    );
    // 其他 rc / 被信号杀 → 观测无效（**绝不当零会话**）
    assert_eq!(
        classify_tmux_probe(Some(2), ""),
        TmuxObservation::Unobservable
    );
    assert_eq!(
        classify_tmux_probe(Some(127), ""),
        TmuxObservation::Unobservable
    );
    assert_eq!(classify_tmux_probe(None, ""), TmuxObservation::Unobservable);
}

/// ★★ **K-R12 `J1` 死值验：让段数真的下溢一次，它必须红。**
///
/// # 这一条钉的到底是什么
///
/// 不是「有没有那行 `if`」，是**下溢那一档的处置**。今天（改之前）rc=0 + 非空 ⇒ 一律
/// `Sessions(raw)`，于是脏输入会被**当成好数据往下游送**，而下游的伤害是最大的那一档：
/// [`session_names`] 只取第 1 段、永远取得到 ⇒ 它把**整行**当会话名
/// ⇒ [`diff_closed`] 下一轮把**所有真会话**算成「消失了」⇒ 一批活着的会话被 retire。
/// ⇒ 所以本条第二段量的是**那个后果**，不是那个分支。
///
/// # 死值从哪来（不是我编的）
///
/// `DIRTY` 是 09-04 在**零挂载容器**里对真 tmux 3.4 私有 socket 打出来的字节
/// （`tests/evidence/K-R12-deathvalue.md` ①/S5，`od -c` 逐字节复核）：POSIX 客户端下
/// 六个真 TAB 全变 `_`，连 `文档`（3 字节/字）都按**显示宽度**变成了 `____`。
/// `CLEAN` 是同一台 server、同一条命令、只加了本拍那条口径之后的输出（同文件 ②/S5）。
///
/// # 正对照不能省 —— 两个方向都要钉
///
/// 只钉「脏的被拒」的话，把 `classify` 焊死成「永远 Unobservable」也能绿。所以：
/// ① 干净的 6 段必须照常 `Sessions`；② **7 段（过溢）也必须照常 `Sessions`** ——
/// 那是**合法内容**（cwd 里带真 TAB 的会话，实测切出 7 段），
/// 判据写成 `!= 6` 就会在这里误伤。**这一格就是「下溢而不是不等于」那个选择的死值。**
#[test]
fn a_dirty_tmux_channel_is_unobservable_never_sessions() {
    // 真 tmux 3.4 + POSIX 客户端打出来的字节（见头注）。六列塌成 1 段。
    const DIRTY: &str = "kr12_/tmp/kr12dv/____/proj_bash_0_1_cc-deadval1\n";
    // 同一台 server、加了 `-u`/`LC_ALL` 之后的同一行。
    const CLEAN: &str = "kr12\t/tmp/kr12dv/文档/proj\tbash\t0\t1\tcc-deadval1\n";
    // 合法的**过溢**：cwd 里有一个真 TAB ⇒ 7 段。`!= 6` 会误伤它，`< 6` 不会。
    const OVERFLOW: &str = "kr12\t/tmp/a\tb\tbash\t0\t1\tcc-deadval1\n";

    assert_eq!(
        classify_tmux_probe(Some(0), DIRTY),
        TmuxObservation::Unobservable,
        "通道脏（段数下溢）必须判观测无效；判成 Sessions 就是把垃圾当好数据送下游"
    );
    assert!(
        matches!(classify_tmux_probe(Some(0), CLEAN), TmuxObservation::Sessions(ref s) if s == CLEAN),
        "正对照：干净的六段必须照常放行，否则买到的是「门坏了」而不是「门对了」"
    );
    assert!(
        matches!(
            classify_tmux_probe(Some(0), OVERFLOW),
            TmuxObservation::Sessions(_)
        ),
        "过溢是**合法内容**（cwd 里带真 TAB）⇒ 必须放行。这一格钉的是「下溢」而不是「不等于 6」"
    );

    // ── 第二段：量**后果**，不是量分支 ────────────────────────────────
    // 先用一份干净观测建立快照，再喂一份脏的，断言**一个会话都没被报死**。
    let mut prev = None;
    let first = "s1\t/p\tclaude\t1\t1\tsid-a\ns2\t/q\tbash\t0\t1\t\n";
    let closed = diff_closed(&mut prev, &classify_tmux_probe(Some(0), first));
    assert!(closed.is_empty(), "第一次观测不该报任何死亡");
    assert_eq!(
        prev.as_ref().map(|s| s.len()),
        Some(2),
        "快照该记住两个会话"
    );

    let dirty_two = "s1_/p_claude_1_1_sid-a\ns2_/q_bash_0_1_\n";
    let closed = diff_closed(&mut prev, &classify_tmux_probe(Some(0), dirty_two));
    assert!(
        closed.is_empty(),
        "🔴 通道一脏就把**全部活会话**报成消失 —— 这才是本件真正的伤害。实得：{closed:?}"
    );
    assert_eq!(
        prev.as_ref().map(|s| s.len()),
        Some(2),
        "观测无效时快照必须原样保留，等下一次成功观测"
    );
}

/// K-R12 `J1` 的判据本体：**下溢红、恰好绿、过溢绿**。
///
/// 与上一条分开写，是因为上一条量的是「处置对不对」，这一条量的是「那条不等号的方向」。
#[test]
fn the_underflow_predicate_only_fires_downward() {
    assert!(
        tab_underflow("一段而已", TMUX_LS_FMT_FIELDS),
        "1 < 6 ⇒ 下溢"
    );
    assert!(
        tab_underflow("a\tb\tc\td\te", TMUX_LS_FMT_FIELDS),
        "5 < 6 ⇒ 下溢"
    );
    assert!(
        !tab_underflow("a\tb\tc\td\te\tf", TMUX_LS_FMT_FIELDS),
        "恰好 6 ⇒ 不红"
    );
    assert!(
        !tab_underflow("a\tb\tc\td\te\tf\tg", TMUX_LS_FMT_FIELDS),
        "7 段是合法内容（路径里有真 TAB）⇒ **不许红**，否则就成了 `!= 6` 那个误伤"
    );
    assert!(
        tab_underflow("15_/tmp/x/sock", 2),
        "query_tmux_server 那条 N=2 的同理"
    );
    assert!(!tab_underflow("15\t/tmp/x/sock", 2));
}

/// ★★ **K-R12 下一拍（09-04）：本模块每一处起 `sh` 的地方都必须挂上 UTF-8 那个 env
/// —— 一处都不许漏。**
///
/// # 🔴 它补的是上一拍留下的一个真洞（现打）
///
/// 上一拍把 `.env(…)` 挂上去了，却**没有任何判据看得见它**：
/// 全仓（两棵树）搜 `TMUX_UTF8_ENV` / `LC_ALL` 的命中，除了本文件生产段那三行之外
/// **一条都不在测试段里**。⇒ 那时把其中一处的 `.env(…)` 删掉，
/// cargo 一条都不会红（两处都删掉才会有个 unused 的**告警**，而本仓不 deny warnings）
/// ⇒ **静默回到病态**，而这一件治的就是静默。
///
/// 上一拍在 `control/gate.rs` 那侧装了同职的一条（`both_tmux_call_sites_…`），
/// **这一侧漏了** —— 正是「只覆盖了那条病的一个动词」那一族。
///
/// # 它守的是「每一处」，不是「有没有」
///
/// 逐处查（不只比总数）：从每一个起 `sh` 的地方到它那句 `.output()` 之间
/// 必须出现那行 `.env(…)`。只比总数的话，「一处挂了两遍、另一处零」照样绿。
///
/// # 🔴 `K-R55`（09-11）：**锚点换了** —— 起 `sh` 这一跳搬进了适配层
///
/// 上一版的锚点是本模块里的裸 `Command::new("sh")`。那两处今天住在
/// [`crate::platform::shell::posix_shell`]（`K33` 裁定二：平台差异只许住适配层），
/// 本模块留下的是**调用点** ⇒ 锚点跟着换成 `posix_shell(`。
/// ⚠ **换锚点不是把红的那条删掉了事**（`guard_support` 那条纪律）：
/// 本条要守的性质一个字没变 —— 「本模块每一处起 `sh` 的地方都挂了那个 env」，
/// 而挂 env 的仍然是**这一侧**（适配层只负责备命令，不碰 env）。
/// 🔴 它因此**没有**跟着搬走：`posix_shell` 有第二个使用者的那天，
/// 那一处的 env 归那一处自己管，本条**看不见它** —— 如实登记，别读宽。
///
/// 🔴 **本条守的是「别漏」，不是「它真的生效了」**（「盘上有 ≠ 被走到」）。
/// 行为那一半的死值在 `tests/evidence/K-R12-deathvalue.md`：同样这两条脚本对真 tmux 3.4
/// 私有 socket 打过，改前段数 1、改后各回各的 N。
#[test]
fn every_sh_call_site_in_this_module_carries_the_utf8_env() {
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/observe/watcher.rs"
    ));
    crate::guard_support::assert_no_test_code("observe/watcher.rs", &prod);
    // 非空对照：剥过头 / 没读到 ⇒ 下面全是 0 == 0 的空真。
    // 地板 = 实测值的一半（09-04 现打 41_149 字节）。
    assert!(
        prod.len() > 20_000,
        "生产段只有 {} 字节 —— 没读到或剥过头，本条此刻在空转",
        prod.len()
    );
    /// 本模块起 `sh` 的处数 —— **登记值**。这张表不是豁免清单：
    /// 新增一处 ⇒ 它也要挂 env，并把这个数一起改。
    const SH_CALL_SITES: usize = 2;
    let starts = prod.matches("posix_shell(").count();
    assert_eq!(
        starts, SH_CALL_SITES,
        "本模块起 `sh` 的处数变了（实得 {starts}，登记 {SH_CALL_SITES}）—— \
             新增的那一处也要挂 UTF-8 那个 env（家在 `common::tmux_utf8`），\
             并把这条判据的数一起改。**这张表不是豁免清单。**"
    );
    let env_call = format!(".env({}.0, {}.1)", "UTF8_CLIENT_ENV", "UTF8_CLIENT_ENV");
    // 逐处查：每个调用点到它那句 `.output()` 之间必须有那行 `.env(…)`。
    let mut checked = 0usize;
    for seg in prod.split("posix_shell(").skip(1) {
        let head = seg.split(".output()").next().unwrap_or(seg);
        assert!(
            head.contains(&env_call),
            "第 {} 处起 `sh` 的地方没挂 `{env_call}` —— 那一处的 tmux 客户端会退回\
                 非 UTF-8，输出里的 TAB 与非 ASCII 全变 `_`，而 rc 仍是 0、\
                 段数下溢那条只会把整趟观测判成「观测无效」：**看起来像远端没事**。\n\
                 这一段是：{head:?}",
            checked + 1
        );
        checked += 1;
    }
    assert_eq!(
        checked, SH_CALL_SITES,
        "逐处查只走到 {checked} 处 —— 切法坏了，上面那条等号是空转的"
    );
    // 反向：env 那一行不许被换成「往脚本串里插旗」。那一改会让 `query_tmux_server`
    // 的脚本与测试里那份逐字复制漂开，而 `tmux_probe_script` 的两条 `exec` 分支
    // 也会退回「要改两处、漏一处永远看不见」。
    assert!(
        !prod.contains("tmux -u ") && !prod.contains(" -u ls "),
        "本模块的脚本串里出现了 argv 形的旗 —— 这一侧按调用点形态该用 env 形，\
             理由（两条 `exec` 分支 + 那份逐字复制）在文件上方那段注释里"
    );
}

/// **`raw` 载荷与 P1 之前逐字节一致**——旧 monitor 行为零变化的那条保证。
/// 有会话时 `observation` 必须**省略**（热路径不加字节）。
#[test]
fn observation_frame_keeps_raw_payload_backward_compatible() {
    match observation_to_frame(TmuxObservation::Sessions("s1\t/p\tclaude\t1\t1\tx".into())) {
        Frame::TmuxSessions { raw, observation } => {
            assert_eq!(raw, "s1\t/p\tclaude\t1\t1\tx");
            assert_eq!(observation, None, "有会话时必须省略，否则热路径白涨字节");
        }
        f => panic!("期望 TmuxSessions，实得 {f:?}"),
    }
    // 无 tmux：保留 NO_TMUX 哨兵（旧 monitor 那道门认它）
    match observation_to_frame(TmuxObservation::NoTmux) {
        Frame::TmuxSessions { raw, observation } => {
            assert_eq!(raw.trim(), "NO_TMUX");
            assert_eq!(observation.as_deref(), Some(OBS_NO_TMUX));
        }
        f => panic!("期望 TmuxSessions，实得 {f:?}"),
    }
    // 零会话 / 观测无效：raw 都是空串（旧 monitor 一律保守跳过 = 今天的行为），
    // 区别只在 observation ⇒ 只有新 monitor 分得开。
    // P3：`ServerEmpty` 与 `NoServer` 两个细分**必须映射到同一个 wire 取值**
    // ——这就是"P3 加细分不改帧契约"那条承诺的机器化。
    for (obs, token) in [
        (TmuxObservation::ServerEmpty, OBS_ZERO_SESSIONS),
        (TmuxObservation::NoServer, OBS_ZERO_SESSIONS),
        (TmuxObservation::Unobservable, OBS_UNOBSERVABLE),
    ] {
        match observation_to_frame(obs) {
            Frame::TmuxSessions { raw, observation } => {
                assert_eq!(raw, "", "旧 monitor 必须看到与今天相同的空 raw");
                assert_eq!(observation.as_deref(), Some(token));
            }
            f => panic!("期望 TmuxSessions，实得 {f:?}"),
        }
    }
}

/// ★ **真跑那段 shell 脚本**（拿假 tmux 喂各种 rc），不只做字符串断言。
///
/// 为什么必须这样测：P1 的关键改动是把 `tmux ls … || true` 换成 `exec tmux …` 让 rc
/// 透出。`|| true` 与 `exec` 的差别**在字符串断言里看不出来**——只有真执行才知道 rc
/// 有没有传出来。（同 `control/ccm/plan.rs::render_container` 那一族的教训：
/// 门禁只锁字符串形状不锁行为。⚠ 这句话点名的活体**换过两次**：`K-R72` 09-12 之前指
/// `tmux.rs` 那一份（随桌面侧 SSH 回落一起走了），之后指用量探针那份
/// （`K-R104` 09-13 随编排搬上帧面一起走了）—— **教训没变，每次换指今天真在的那个。**）
#[test]
fn probe_script_propagates_rc_with_fake_tmux() {
    let dir = std::env::temp_dir().join(format!("ccm-p1-probe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let fake = dir.join("tmux");

    let run = |path_value: &str| -> TmuxObservation {
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(tmux_probe_script())
            .env("PATH", path_value)
            .output()
            .expect("spawn /bin/sh");
        classify_tmux_probe(out.status.code(), &String::from_utf8_lossy(&out.stdout))
    };
    let write_fake = |body: &str| {
        std::fs::write(&fake, body).expect("write fake tmux");
        let mut perm = std::fs::metadata(&fake).expect("stat").permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perm.set_mode(0o755);
        }
        std::fs::set_permissions(&fake, perm).expect("chmod");
    };
    let path_with_fake = format!("{}:/usr/bin:/bin", dir.display());

    // ① 假 tmux 打印一行会话、rc=0 → Sessions
    write_fake("#!/bin/sh\nprintf 's1\\t/p\\tclaude\\t1\\t1\\tsid-a\\n'\nexit 0\n");
    assert!(matches!(
        run(&path_with_fake),
        TmuxObservation::Sessions(ref s) if s.contains("sid-a")
    ));

    // ② rc=0 但不输出 → ServerEmpty（exit-empty off 那格）
    write_fake("#!/bin/sh\nexit 0\n");
    assert_eq!(run(&path_with_fake), TmuxObservation::ServerEmpty);

    // ③ rc=1（真 tmux 在 server 不在时就是这个）→ ZeroSessions
    //    **这一格是 P1 的核心**：改回 `|| true` 会让它变成 rc=0+空 ⇒ 仍是 ZeroSessions，
    //    所以本格单独看不出回归；真正钉住 `exec` 的是 ④。
    write_fake("#!/bin/sh\necho 'no server running on /tmp/x' >&2\nexit 1\n");
    assert_eq!(run(&path_with_fake), TmuxObservation::NoServer);

    // ④ ★ rc=2（观测无效）→ 必须是 Unobservable，**绝不能被折成零会话**。
    //    这一格就是 `|| true` 的变异检测点：加回 `|| true` 会把 rc=2 吞成 rc=0+空
    //    ⇒ 误判成 ZeroSessions ⇒ 本断言红。
    write_fake("#!/bin/sh\necho boom >&2\nexit 2\n");
    assert_eq!(
        run(&path_with_fake),
        TmuxObservation::Unobservable,
        "观测失败被折成零会话会批量误灰——这里红说明 rc 没有真的透出来"
    );

    // ⑤ PATH 里没有 tmux → NoTmux（command -v 门控）。
    //    **必须用一个确实没有 tmux 的空目录**：初版这里写的是 `/usr/bin:/bin`，而真 tmux
    //    就在 `/usr/bin` ⇒ 测试真跑了**默认 socket** 上的 `tmux ls`（只读、无损，但违反
    //    "tmux 一律走隔离 socket"的纪律，且在别人机器上结果不可预测）。断言当场红是因为
    //    它列出了真实会话而不是 NoTmux —— 算这条测试自己抓到的第一个问题。
    let empty = dir.join("no-tmux-here");
    std::fs::create_dir_all(&empty).expect("mkdir empty");
    assert_eq!(
        run(&empty.display().to_string()),
        TmuxObservation::NoTmux,
        "PATH 里没有 tmux 时必须走 command -v 门控那支"
    );

    let _ = std::fs::remove_dir_all(&dir);
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
fn subagents_path_is_excluded() {
    // A path containing a `subagents` segment must be filtered.
    let p = Path::new("/home/u/.claude/projects/foo/subagents/bar.jsonl");
    assert!(is_subagent_path(p));
    // Case-insensitive, mirrors watcher.rs.
    let p2 = Path::new("/home/u/.claude/projects/foo/SubAgents/bar.jsonl");
    assert!(is_subagent_path(p2));
    // A normal session file is not excluded.
    let p3 = Path::new("/home/u/.claude/projects/foo/abc-123.jsonl");
    assert!(!is_subagent_path(p3));
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

/// ★★ `P0b-Y2`〔08-13〕：**「盯着的目录被换掉」这条路必须有人重挂 + 重扫。**
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
    assert!(
        prod.contains("watch(&agent_home, RecursiveMode::NonRecursive)"),
        "没有监视 `agent_home` 本身 —— 那 `sessions/` 出现或被换掉时没有任何事件会来。"
    );
}

/// ★★★ **每一处 `.watch(` 都要回答「目录被换 inode 了怎么办」** —— 登记表〔08-13〕。
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
/// # 今天的 8 处（〔SR1a · 09-24〕7 → 8：账号 manifest 所在目录）
///
/// | 处 | 归属 | 换 inode 怎么办 |
/// |---|---|---|
/// | `rewatch_dir` | **可重入挂法本体** | 就是它负责 |
/// | `watch_sock_dir_if_present` | socket 目录专用（多一条「目录没了翻记账」） | 同上 |
/// | `rewatch_sessions` | `sessions/` 专用（多一件事：挂上顺带重扫 pidfile） | 同上 |
/// | `watch_loop` 里 `agent_home` | **父目录的耳朵**（子目录出现/消失的唯一信号源） | 父目录被换掉 = 整个 agent_home 没了，那时没有任何路可走，**不在这一族** |
/// | `watch_loop` 里 socket 目录的**父** | 同上（等 socket 目录出现） | 同上 |
/// | `watch_loop` 里 `sessions` 起步那次 | 起步挂一次，之后归 `rewatch_sessions` | 已有 |
/// | `watch_loop` 里 tmux socket **所在目录**（P3 复活探测） | 一次性触发器，socket 换 inode 由上面那条目录耳朵覆盖 | 已有 |
/// | `watch_loop` 里账号 manifest **所在目录**（〔SR1a〕`accounts_changed`） | 起步挂一次（目录不在就不挂） | **不重挂，如实认下**：目录被删掉重建之后这一路失聪、直到后端重启 —— 代价只是「账号清单变了不推帧」，客户端退回既有的刷新时机（连上 / 会话起停）；不许为它去盯整个 `$HOME`（那是噪声最大的目录） |
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
        sites, 8,
        "生产段 `.watch(` 有 {sites} 处（登记表记着 8 处）。\n             \
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
    ] {
        assert!(
            prod.contains(f),
            "可重入挂法 {f} 不见了 —— 那一路的重挂就没人做了"
        );
    }
}

/// ★★ socket 目录**被删掉再重建**时，watch 必须跟着换到新 inode〔08-13〕。
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

/// ★ `P0b-Y2` 第十六拍：**socket 目录按 `TMUX_TMPDIR` 推，不许硬编码 `/tmp`。**
///
/// # 为什么这条是单测而不是 e2e
///
/// 它是个**纯函数**（env → 路径），单测才是对的 acceptor。
/// ⚠ 第一版把它写进 e2e 的第三格，逼得那个套件自己设 `TMUX_TMPDIR` ——
/// 而 `C7i` **零例外**禁止 e2e 靠它做隔离，`e2e_gate_registry` 那条守卫当场拦下。
/// **它报得对**：判据要钉的性质与套件要用的隔离手段撞在同一个变量上时，
/// 该换的是**判据落在哪一层**，不是给红线开例外。
///
/// ⚠ 硬编码 `/tmp` 的后果是**静默失效**：在设了 `TMUX_TMPDIR` 的机器上，
/// backend 会去监视一个永远不会有动静的目录 —— 与修之前一模一样，且没有任何错误。
#[test]
fn tmux_socket_dir_follows_tmux_tmpdir() {
    // ⚠ env 是进程全局的：设完必须还原，否则会污染同进程里别的测试。
    let saved = std::env::var_os("TMUX_TMPDIR");
    // SAFETY: 单线程内设/取环境变量；本测试跑完立即还原。
    unsafe { std::env::set_var("TMUX_TMPDIR", "/x/y") };
    let d = tmux_socket_dir();
    unsafe {
        match &saved {
            Some(v) => std::env::set_var("TMUX_TMPDIR", v),
            None => std::env::remove_var("TMUX_TMPDIR"),
        }
    }
    let s = d.to_string_lossy();
    assert!(
        s.starts_with("/x/y/tmux-"),
        "socket 目录没跟着 `TMUX_TMPDIR` 走（实得 {s}）—— \
             硬编码 `/tmp` 会让后端监视一个永远没动静的目录，且**没有任何错误**。"
    );
    assert!(
        !s.starts_with("/tmp/"),
        "socket 目录仍落在 `/tmp` 下（实得 {s}）"
    );
}

/// 同 pidfile 原地换 sid（/clear）：旧 sid 立即 Removed、新 sid Added，
/// active_sids 恰含新 sid（跨机审计实锤的假 live 泄漏回归测试）。
#[cfg(target_os = "linux")]
#[test]
fn sid_change_in_place_retires_old_sid() {
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
    // 第二个 pidfile：幂等检查是 per-key 的 → 恰好再发一条 Added（前端
    // ensureTab 幂等）。断言帧序（审计 S3：吞帧会掩盖"先 Removed 再 Added
    // 闪烁"类回归）。
    assert!(
        matches!(rx.try_recv(), Ok(Frame::SessionAdded { sid, .. }) if sid == "shared-sid"),
        "second pidfile re-announces exactly once"
    );
    assert!(
        rx.try_recv().is_err(),
        "and nothing else (no spurious Removed)"
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
        Ok(Frame::SessionAdded { sid, status, .. }) => {
            assert_eq!(sid, "st-sid");
            assert_eq!(status.as_deref(), Some("busy"), "宣告带初始 status");
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
            status,
            waiting_for,
            ..
        }) => {
            assert_eq!(sid, "st-sid");
            assert_eq!(status.as_deref(), Some("waiting"));
            assert_eq!(waiting_for.as_deref(), Some("permission prompt"));
        }
        other => panic!("expected SessionStatus, got {other:?}"),
    }
    // 再变回 → 再发
    write("idle", None);
    process_session_added(&pidfile, &mut state, &mut sink);
    assert!(matches!(
        rx.try_recv(),
        Ok(Frame::SessionStatus { status: Some(s), waiting_for: None, .. }) if s == "idle"
    ));
    std::fs::remove_dir_all(&dir).ok();
}

// === Batch8-F25：tail-only 模式 ===

/// tail-only 初扫：宣告帧带 path、零行帧；随后追加的新行 seq == 初扫时完整
/// 行数 L（行号语义）；末尾残行不计数（F14 torn-line 语义）。
#[cfg(target_os = "linux")]
#[test]
fn tail_only_primes_cursor_and_new_line_seq_is_line_number() {
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
        Ok(Frame::Line { seq, raw, .. }) => {
            assert_eq!(seq, 3, "残行补全行的 seq 应为初扫完整行数 L=3");
            assert_eq!(raw, r#"{"torn":true}"#);
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
            sid,
            session_kind,
            lines,
            path,
            ..
        }) => {
            assert_eq!(sid, "combo-sid");
            assert_eq!(session_kind.as_deref(), Some("bg"), "with_bg 放行");
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
            session_kind,
            cwd,
            name,
            ..
        }) => {
            assert_eq!(sid, "bg-sid");
            assert_eq!(session_kind.as_deref(), Some("bg"));
            assert_eq!(cwd.as_deref(), Some("/proj/x"));
            assert_eq!(name.as_deref(), Some("评估任务"));
        }
        other => panic!("expected SessionAdded with metadata, got {other:?}"),
    }
    assert!(state.active_sids.contains("bg-sid"), "bg 行要能流出");
    std::fs::remove_dir_all(&dir).ok();
}

// === Batch6-F21：kind 交互性门 ===

#[test]
fn parse_kind_variants() {
    assert_eq!(
        parse_kind(br#"{"sessionId":"s","kind":"bg","jobId":"j"}"#).as_deref(),
        Some("bg"),
        "真实 bg 样本形态"
    );
    assert_eq!(
        parse_kind(br#"{"sessionId":"s","kind":"interactive"}"#).as_deref(),
        Some("interactive")
    );
    assert_eq!(parse_kind(br#"{"sessionId":"s"}"#), None, "旧 CC 无 kind");
    assert_eq!(parse_kind(b"not json"), None);
}

/// 集成：kind:"bg" 的 pidfile（真实存活进程 = 本进程，身份/时间证据全过）
/// 在 kind 门被拒——不发 SessionAdded、不进 sessions/active_sids。
/// 对照组：同进程 interactive pidfile 正常宣告。
#[cfg(target_os = "linux")]
#[test]
fn bg_pidfile_is_gated_even_when_author_is_alive() {
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
/// 〔WN1 · 09-24〕`pid_alive` 有了 Windows 臂（`platform/win_proc.rs`）⇒ 上面那句「在 Windows 上会 panic」
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
        session_kind: None,
        attachable: None,
        cwd: None,
        name: None,
        path: None,
        lines: None,
        status: None,
        waiting_for: None,
        rbind_token: None,
    });
    sink.send(Frame::SessionAdded {
        sid: "b".into(),
        agent_kind: None,
        liveness_confidence: None,
        session_kind: None,
        attachable: None,
        cwd: None,
        name: None,
        path: None,
        lines: None,
        status: None,
        waiting_for: None,
        rbind_token: None,
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
        session_kind: None,
        attachable: None,
        cwd: None,
        name: None,
        path: None,
        lines: None,
        status: None,
        waiting_for: None,
        rbind_token: None,
    });
    sink.send(Frame::SessionAdded {
        sid: "d".into(),
        agent_kind: None,
        liveness_confidence: None,
        session_kind: None,
        attachable: None,
        cwd: None,
        name: None,
        path: None,
        lines: None,
        status: None,
        waiting_for: None,
        rbind_token: None,
    });
    sink.send(Frame::SessionAdded {
        sid: "e".into(),
        agent_kind: None,
        liveness_confidence: None,
        session_kind: None,
        attachable: None,
        cwd: None,
        name: None,
        path: None,
        lines: None,
        status: None,
        waiting_for: None,
        rbind_token: None,
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
        session_kind: None,
        attachable: None,
        cwd: None,
        name: None,
        path: None,
        lines: None,
        status: None,
        waiting_for: None,
        rbind_token: None,
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

/// ★★ **超时必须落成「观测无效」，绝不能落成「零会话」**〔audit-0805 F09〕。
///
/// 这是本件最要命的一格：`ServerEmpty`（rc=0 且 stdout 空）会让上层认为
/// **那台机器上一个会话都没有** ⇒ 活着的会话被 retire。
/// 而超时是「**我没看清**」，不是「**我看清了，是空的**」。
///
/// `timeout -s KILL` 杀掉子进程后 rc 是 137（128+9）；有些实现/路径下是 124；
/// 被信号直接杀时 `code` 是 `None`。三种都必须落 `Unobservable`。
#[test]
fn a_timed_out_probe_is_unobservable_never_zero_sessions() {
    for code in [Some(124), Some(137), None] {
        let got = classify_tmux_probe(code, "");
        assert!(
            matches!(got, TmuxObservation::Unobservable),
            "rc={code:?} 被判成了 {got:?} —— 超时是「我没看清」，不是「我看清了，是空的」。\n\
                 判成 ServerEmpty 会让上层认为那台机器零会话 ⇒ **活着的会话被 retire**。"
        );
    }
    // 对照：真正的「server 在、但零会话」仍然要判 ServerEmpty（防把上面写成恒真）。
    assert!(
        matches!(
            classify_tmux_probe(Some(0), ""),
            TmuxObservation::ServerEmpty
        ),
        "rc=0 且空 stdout 该是 ServerEmpty —— 上面那条不许把它一起吞了"
    );
}

/// 探测脚本必须**带上界**，且 `timeout` 缺席时诚实退回〔audit-0805 F09，承接 C7〕。
#[test]
fn the_tmux_probe_is_bounded_and_degrades_honestly() {
    let script = tmux_probe_script();
    assert!(
        script.contains("tmux ls"),
        "抽取器自检：脚本里连 `tmux ls` 都没有 —— 拿错东西了：{script}"
    );
    assert!(
        script.contains("timeout"),
        "★ 探测没有上界。`run_tmux_ls` 的 `output()` 无超时，而 `watch_loop` 的 `tmux_inflight`\n\
             只在收到 `TmuxObserved` 时清 —— 探测永不返回 ⇒ 标志永远为真 ⇒ **此后一次 tmux 探测\n\
             都不会再发起，且不发任何理由帧**（报告 I-2）。实得：{script}"
    );
    assert!(
        script.contains("command -v timeout"),
        "★ `timeout` 必须门控。硬用它会在没有 coreutils 的系统上让整条探测直接失败 ——\n\
             那是把一个「偶发卡死」换成「必然不可用」。要诚实降级（C7），不是赌它存在。实得：{script}"
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
    for (name, sig) in [
        ("process_jsonl", "fn process_jsonl("),
        ("prime_file_cursor", "fn prime_file_cursor("),
    ] {
        let begin = src
            .find(sig)
            .unwrap_or_else(|| panic!("找不到 {name} —— 抽取器坏了，本条会零命中地绿"));
        let end = src[begin..]
            .find("\n}\n")
            .unwrap_or_else(|| panic!("找不到 {name} 的结尾 —— 抽取器坏了"));
        let body = &src[begin..begin + end];
        assert!(
            body.contains("read_tail_from"),
            "{name} 里没有调 `read_tail_from` —— 要么切错范围（本条会零命中地绿），\n\
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
        raw: "{}".into(),
        byte_offset: 0,
    });
    // 丢一条内容帧（可恢复 ⇒ 只计数、不留身份）与一条状态增量帧（不可恢复 ⇒ 留身份）。
    sink.send(Frame::Line {
        session_id: "content-lost".into(),
        path: "/p".into(),
        seq: 1,
        raw: "{}".into(),
        byte_offset: 1,
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
                vec![crate::wire::LostFrame {
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
        raw: "{}".into(),
        byte_offset: 0,
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
    let src = guard_core::production_code(include_str!("../../../src/backend/wire.rs"));
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

/// `P0b`：**「目录不存在就永远不重试」这条缺陷的登记**〔08-13 实测复现〕。
///
/// 本条**不是**在断言那是对的 —— 它钉的是**那条已知缺陷的说明还在**，
/// 因为下一个读到那两个 `else` 分支的人，第一反应会是「打个 warn 挺合理」。
/// 而实测告诉我们：`<claude_dir>/sessions/` 是**用户第一次跑 claude 时才建的**，
/// backend 起得早一步，就**永远不宣告会话**。
///
/// 复现（帧的 `kind` 直方图，其余条件一模一样）：
/// · 有 `sessions/` ⇒ `hello · line · session_added · tmux_sessions`
/// · 无 `sessions/` ⇒ **只有** `hello · tmux_sessions`
///
/// ⚠ 修掉它之后**请连同这条判据一起改** —— 它守的是「缺陷说明在」，
/// 缺陷没了这条就该换成守新行为的那一条。
#[test]
fn the_missing_dir_branch_still_says_it_never_retries() {
    let src = include_str!("../../../src/backend/observe/watcher.rs");
    for needle in ["本进程不会再重试挂它", "永远不宣告会话"] {
        assert!(
            src.contains(needle),
            "那条缺陷说明被删了（少了「{needle}」）—— 删它之前请先修掉缺陷本身，\
                 否则下一个人会以为「打个 warn 就够了」"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// `设计/80 §8.7` 步 2：**启动期令牌上 wire** —— `session_added.rbind_token`
// ═══════════════════════════════════════════════════════════════════════════

/// ★★ **本刀的正题（帧那一半）：一个带着 `CCM_RBIND_TOKEN` 的真进程，
/// 它的令牌真的出现在 `session_added` 帧上。**
///
/// # 为什么这一条与 `identity_tag_tests` 那条不重复
///
/// 那边断的是「**读得出来**」（`rbind_token_of` 对一个真进程回对值）；
/// 本条断的是「**接上了**」—— `process_session_added` 真的去调它、
/// 真的把结果放进了那个字段。这两件事之间**有一整条接线**可以断掉而那边照常绿
/// （本仓逐字「判据不在执行链上就等于不存在」）。
///
/// # 三组对照，缺任何一组读数都不可信
///
/// | 组 | 客户端索要了吗 | 进程环境里有吗 | 期望 |
/// |---|---|---|---|
/// | 正题 | ✅ `--with-rbind-token` | ✅ | 帧上是那个令牌 |
/// | 阴性一（**默认路**） | ❌ | ✅ | 帧上**没有**（令牌默认不上 wire，`§8.6 ③`） |
/// | 阴性二（**归因那一格**） | ✅ | ❌ | 帧上**没有** = 「这条会话真的没有令牌」（`§8.5 ②`） |
///
/// ★ 阴性一不是陪跑：它是「默认关」那条承诺的**唯一**判据。把 `state.with_rbind_token`
/// 那个闸门删掉（改成无条件读），正题与阴性二都还绿，只有它红。
#[cfg(target_os = "linux")]
#[test]
fn the_launch_token_rides_the_session_added_frame_only_when_the_client_asked() {
    /// 起一个 `sleep`，可选地给它注一个 `CCM_RBIND_TOKEN`。
    fn spawn_sleeper(token: Option<&str>) -> std::process::Child {
        let mut cmd = std::process::Command::new("sleep");
        cmd.arg("60");
        // ★ 先 `env_remove`：本测试进程自己碰巧带着这个变量时（步 1 落地之后
        //   开发机上完全可能），阴性二会继承到它、当场变成一条假绿。
        cmd.env_remove("CCM_RBIND_TOKEN");
        if let Some(t) = token {
            cmd.env("CCM_RBIND_TOKEN", t);
        }
        let kid = cmd
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("起不来 `sleep` —— 本条的夹具坏了，读数一个字都不能信");
        // 🔴 等它走出 `execve` 窗口——不等就是一条真的间歇性假红。
        // 成因与现打读数写在
        // `identity_tag_tests::the_token_is_read_back_out_of_a_real_child_process_environ`
        // 里那个同名助手的头注里（`/proc/<pid>/environ` 刚 `execve` 时回 0 字节，
        // 本机 1/500）。门是「environ 非空」而不是「读到令牌」：阴性组本来就没令牌。
        for _ in 0..500 {
            match std::fs::read(format!("/proc/{}/environ", kid.id())) {
                Ok(b) if !b.is_empty() => break,
                _ => std::thread::yield_now(),
            }
        }
        kid
    }

    /// 跑一趟：起子进程 → 配一份合成 pidfile → 喂 `process_session_added` → 取帧上那个字段。
    ///
    /// 回 `Ok(帧上的 rbind_token)`。**不是真 claude**（`C7`：夹具不许起真 agent），
    /// 而这条路上「是不是 claude」由 procStart 逐位相等那条主证据放行，与 cmdline 无关。
    fn probe(label: &str, asked: bool, token: Option<&str>) -> Option<String> {
        let dir = std::env::temp_dir().join(format!("ccm-rbind-{}-{label}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut kid = spawn_sleeper(token);
        let pid = kid.id();
        let ticks = proc_starttime(pid).expect("子进程的 starttime 读不到 —— 夹具坏了");
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, false);
        state.with_rbind_token = asked;
        let path = dir.join(format!("{pid}.json"));
        std::fs::write(
            &path,
            format!(
                r#"{{"pid":{pid},"sessionId":"rb-{label}","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#
            ),
        )
        .unwrap();
        process_session_added(&path, &mut state, &mut sink);
        let got = match rx.try_recv() {
            Ok(Frame::SessionAdded {
                sid, rbind_token, ..
            }) => {
                assert_eq!(sid, format!("rb-{label}"), "宣告的是另一条会话？");
                rbind_token
            }
            other => panic!(
                "[{label}] 没收到 `session_added` —— 本趟的读数一个字都不能信（实得 {other:?}）"
            ),
        };
        let _ = kid.kill();
        let _ = kid.wait();
        std::fs::remove_dir_all(&dir).ok();
        got
    }

    const GOOD: &str = "0123456789abcdef0123456789abcdef";

    // ── 正题 ───────────────────────────────────────────────────────────────
    assert_eq!(
        probe("yes", true, Some(GOOD)).as_deref(),
        Some(GOOD),
        "索要了、环境里也有，帧上却没有那个令牌 —— \
         `设计/80 §8.2` 那条「后端从 `/proc/<pid>/environ` 读出来、经 wire 报回」**没接上**"
    );

    // ── 阴性一：没索要 ⇒ 默认不上 wire ────────────────────────────────────
    assert_eq!(
        probe("unasked", false, Some(GOOD)),
        None,
        "没发 `--with-rbind-token` 却把令牌放上了 wire —— \
         令牌是敏感数据（`§8.6 ③`），默认关那条承诺没兑现"
    );

    // ── 阴性二：索要了，但这条会话压根没有令牌 ⇒ 缺席 = 归因那一格 ────────
    assert_eq!(
        probe("bare", true, None),
        None,
        "环境里没有那个变量，帧上却凭空多出一个令牌 —— \
         那会让 `§8.5 ②` 那个布尔恒真（「有没有令牌」从此答不准）"
    );
}

/// 〔SR1a〕★ A1 的判定那一半：一批文件事件里**有** manifest ⇒ 真；只有无关文件（同目录里的别的文件、
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

/// 〔SR1a〕接线：`Notify` 那一臂里**恰好一处**问 `manifest_touched`、真了发 `Frame::AccountsChanged`，
/// 而且它排在逐条处理事件的 `for` **之前**（逐条那一段里有 `continue`，放进去会被跳过）。
#[test]
fn the_notify_arm_asks_once_per_batch_before_the_per_event_loop() {
    let src = include_str!("../../../src/backend/observe/watcher.rs");
    let prod = crate::guard_support::production_code(src);
    let ask = guard_core::find_pinned(&prod, "if manifest_touched(")
        .expect("Notify 那一臂里不是恰好一处问 manifest_touched");
    let emit = guard_core::find_pinned(&prod, "sink.send(Frame::AccountsChanged);")
        .expect("发 accounts_changed 的不是恰好一处");
    let per_event = prod[ask..]
        .find("for ev in events {")
        .map(|k| ask + k)
        .expect("问完之后没有逐条处理事件的 for —— 结构变了");
    assert!(ask < emit && emit < per_event, "问与发要排在逐条处理之前");
}
