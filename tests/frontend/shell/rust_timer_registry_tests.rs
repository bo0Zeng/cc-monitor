//! ═══════════════════════════════════════════════════════════════════
//! # 要求住址：**`src/doc/INVARIANTS.md` 条 43**
//! ═══════════════════════════════════════════════════════════════════
//!
//! 本表守的是「**monitor 侧**的周期性唤醒要逐条登记」。
//!
//! 🔴 **别把它读成 `§41.4` 的扩张** —— 那一条逐字划了范围：
//!
//! > 范围**只钉后端生产段**：monitor 侧有 UI 刷新、重连退避等正当周期行为，
//! > 扩过去会变成噪音。
//!
//! ⇒ **那一条明说不管 monitor，而条 43 管。** 两条不是同一件事：
//! `§41.4` 的正题是「**后端零定时器**」（禁那一族构件本身）；
//! 条 43 的正题是「**monitor 侧的节拍不许无声无息地长出来**」——
//! 它**不禁**周期行为，它要的是每一处都说得出自己是什么、谁退役它。
//!
//! ⚠ 条 43 是 2026-09-22 才有的（`P20` 现打之后升格）：
//! 在那之前本表**一直在管、却点不到任何要求**，而那天它**还在长**
//!（当天加了那个 10ms 轮询，今住 `filewin/proc.rs::early_failure`）⇒ 活着的缺口，不是历史遗留。
//! 逐条依据住。
//!
use std::fs;
use std::path::{Path, PathBuf};

/// 周期唤醒的**登记表**：`(相对 src/frontend/shell 的路径, 外层项, 类别, 为什么 + 谁退役它)`，按「文件 × 外层项」认。
///
/// 多一处没登记的 ⇒ 下面那条红。**登记表不是豁免清单**，
/// 是「这些我看过、而且知道它归谁」的账。
const REGISTERED: &[(&str, &str, &str, &str)] = &[
    (
        "src/bind.rs",
        "run_heartbeat",
        "ticker",
        "★ **本表一上岗就抓到的那个真节拍器**：`run_heartbeat` = \
             `loop { sleep(10s); cleanup_dead() }` —— 无限、周期、无上限。\
             它清的是「死 pid 的 HWND 绑定」。**事件源存在但没用**：pid 死亡本可由内核事件\
             （Windows job object / backend 侧那套 pidfd）推回来，今天是靠 10s 扫一遍。\
**节拍归 `bind.rs::BindRegistry`**（`spawn_heartbeat` 起，与 monitor 进程同寿）；远端 ↗ 点那一刻现查，不另起节拍器。\
             ⚠ **退役未排期** —— 换内核事件源要 Windows 真机才量得了，本机一格都买不到。\
             如实记未排期，**不编一个假 owner 让它看起来有人管**。",
    ),
    (
        "src/bind.rs",
        "process_await_file",
        "wait-for-condition",
        "一处等窗口：`find_window_for_marker` 的 12×50ms（≤600ms，等 PowerShell 设标题传播）。**有次数上限**，\
             不是节拍器。（**2 → 1**：远端那条点 ↗ 时的现扫重试随窗口标题那一套删了。）",
    ),
    (
        "src/ccm_probe.rs",
        "capture_full",
        "wait-for-condition",
        "P3t D 阶段补审：`probe_with` 的 10ms 轮询 `try_wait` —— 等**本机 ccm 探测那个子进程退出**，\
             上限是 `LOCAL_PROBE_TIMEOUT`（5s），到点 `kill` + `wait` 收尸。\
             **不是节拍器**：一次探测最多醒 500 次，探完就没了，且结果缓存 5 分钟。\
             ⚠ 为什么非轮询不可：std 不提供 `wait_timeout`，而这条**必须有上限** —— \
             它跑的是用户自己的交互式 rc（`bash -lic`），内容不在本仓控制之下，\
             没上限就等于「点一次恢复永远转圈」。退役归：std 有了 `wait_timeout` 之后（今天没有）。",
    ),
    (
        "src/remote_resident.rs",
        "retry_tunnel",
        "wait-for-condition",
        "`tunnel_when_bound`（编排住 `retry_tunnel`）的 30×200ms（≤6 s）：远端 `--resident-ensure` 起了一个脱离的常驻后端、\
             后端**不等它 bind**（后端零定时器）⇒ 这里等**那台回环口上有人在听**这个一次性条件（开隧道成功即止），\
             有次数上限，等不到就如实报「连不上」、交重连那一层；远端回拒码是「不许端口转发」⇒ 当场停、不等。\
             不是节拍器：只在接那台常驻后端的那一趟跑。",
    ),
    (
        "src/dial_host.rs",
        "local_backend_accepting",
        "wait-for-condition",
        "`local_channel` 的 60×50ms（≤3 s）：等**本机常驻后端那条流的 hello 到了**\
             （`<local>` 那条入方向客户端登记上）—— monitor 刚起、本机后端刚接上的那个窗口里，\
             远端链路要经它开。**一次性条件、有次数上限**，等到就走、等不到就如实报「本机后端不在」\
             （`D11`：不起代理进程、不进程内拨）。不是节拍器：没有链路要开时它根本不跑。",
    ),
    (
        "src/local_backend_host.rs",
        "adopt_with",
        "wait-for-condition",
        "〔`K-P1` 08-26；说法 08-27 补全，见下 ⚠⚠〕那一处 `sleep` 住 `adopt_with`，\
             而 `adopt_with` 有**两个**调用方，等的是**两件不同的事**，共用同一条上限：\
             ① `probe_and_attach_after_spawn`（`wait_for_bind = true`）等**刚脱离起来的那个 backend**\
             把回环口 bind 上；\
             ② `adopt_existing`（`wait_for_bind = false`）**根本没有 spawn** —— 它等的是\
             `stream-busy` 那张牌被还回来（上一个 monitor 刚退、对面还没把那条流放回去）。\
             不等它，用户会看到「换台电脑重开 monitor 就没有本机后端」。\
             上限两条路共用：`LISTEN_WAIT_TRIES × LISTEN_WAIT_INTERVAL_MS` = 50×20ms ≈ **1 秒**，\
             等到就走、等不到就如实报错（①那条还会把那个进程收掉）。**一次性条件，不是节拍器。**\
             ⚠⚠ 〔`K-P1-D1` `重-4`〕本行原先只写了①，而它盖着的是两件事。\
             **登记表的说法就是那条判据的诚实边界** —— 说法与代码不是一回事时，\
             判据在替一个不存在的性质背书。⇒ 同轮补了\
             `local_backend_host::the_timer_registry_names_every_caller_of_the_one_wait_here`\
             钉住「这一行必须点到**每一个**调用方的名字」。\
             ⚠ 为什么非等不可（这是**实测**出来的，不是推的）：`connect_timeout` 在**没人在听**的口上\
             拿到 `ECONNREFUSED` 时**内核立刻返回**，它压根不等 —— 第一版据此写了「让内核等」，\
             实测 0.00 秒就红了。⇒ 只有 `Probe::Nobody` 那一支重试；\
             「口上是别人」不会自己变好，重试它只是把一个确定的坏消息拖晚。\
             退役归：哪天改成「宿主自己 bind、把 fd 传给子进程」——那时竞态**根本不存在**\
             （而它也顺手把 `EADDRINUSE` 挪到宿主手里）。",
    ),
    (
        // `early_failure` 随「起进程那一侧」留在 monitor（从前住窗口的 `shell.rs`）。
        "src/filewin/proc.rs",
        "early_failure",
        "wait-for-condition",
        "🔴`early_failure` 里那一跳 10ms 轮询：\
             等「开窗那条线程是不是当场就死了」，**最多 300ms**（`EARLY_FAILURE_BUDGET`）。\
             它补的是一个**静默成功** —— 入口那条命令此前把开窗句柄 `let _ = …` 丢掉，\
             而同一进程里第二次开窗**必然失败**（winit 的进程级事件循环标志）\
             ⇒ 用户关掉窗口再点一次，屏幕上什么都没有而界面说「成功」。\
             ⚠ **不是 ticker**：它有硬上界、跑完就结束，不跟任何事件源绑。\
             ⚠ 为什么不用条件变量／`JoinHandle` 的阻塞等：阻塞等会把这条命令\
             挂在**窗口的整个寿命**上（`run_native` 要占着那条线程直到窗口关闭）——\
             那正是 `entry.rs` 头注②那条注释禁的事。⇒ 只能是「等一个短预算」。\
             退役归：哪天窗口改成**常驻一个事件循环**（那个进程形态\
             要重定，那条待裁），第二次开窗不再是失败 ⇒ 这一跳就没用了。",
    ),
    // `src/stop_grace.rs` 那一行（wait-for-condition 1 处：「停」之后每 100ms 看一眼它退了没有）随文件删了 ——
    //   「请它收尾 → 等 → 强杀」搬进那台机器上的一次性子命令 `--resident-stop`（等在 pidfd 上，不轮询），monitor 只拿回结局。
    // `src/search.rs` 那一行（`startup-delay` 1 处：`build_blocking` 起头让路 1.5 s）随本机内存索引删了 ——
    //   本机全文搜索改问本机后端（`history-search`），monitor 不再建索引。
    // `src/port_forward.rs` 那一行（accept 瞬时错误 100ms 退避）**删了**：accept 循环整个搬进了
    //   后端的拨号代理（`src/backend/dial/uses.rs::forward`），而后端不许睡 ⇒ 那一侧改成「accept 失败就收工并出声」。
    // ★★ 🔴 `K-R59`（09-11）：**这里原来是本表抓到的第二个真节拍器，那一条今天退役了。**
    //    它是 `src/stream_source/` 的 `BACKENDLESS_POLL_INTERVAL = 2s`（`loop { …; sleep(2s) }`），
    //    登记里逐字写着它「与**定框 C7 直接冲突**」「也与 **C8**（不许轮询）冲突」，
    //    而退役条件当时写的是「**远端自动部署可靠到可以删掉这个开关**……今天无人认领，
    //    如实记未排期」。
    //    ⇒ 用户原话（「不要有 daemonless。没有没有后端的情况。」）—— **认领人来了，
    //      而且不是走「自动部署可靠了」那条路，是直接取消那一档。**
    //    ★ 这一条与 `watcher.rs` 那条（F11）同形：**退役的验收证据就是本表先红在
    //      「少一处 = 退役了」上，删掉登记才绿。** 不是靠人说「我改好了」。
    // `src/inproc_dial.rs` 那一行（`RACE_STAGGER * i`：进程内多端点竞速的错开起拨，C2 从 `stream_source/`
    //   原样搬去的那一份）摘了 —— 那份文件整份删了（界面进程零 SSH）。竞速今天住本机后端 `dial/connect.rs`（同时起拨：后端零定时器）。
    (
        "src/stream_source/snapshot.rs",
        "snapshot_dispatcher",
        "wait-for-condition",
        "快照读失败时 `if attempt == 1 { sleep(1s) }` —— **只重试一次**。",
    ),
    (
        "src/stream_source/run.rs",
        "run",
        "wait-for-condition",
        "重连退避 `sleep(backoff)`，序列 2→4→8→16→**30 上限**。\
             ⚠ 外层重连循环确实无限，但**它等的是「下次重连时机」，不是节拍** —— \
             连上之后由 `stream_loop` 阻塞驱动；断线才回到这里。上限 30s 是明写的常量。",
    ),
    // `src/lib.rs` 那一行（`remote-bind-scan` 那条等标题的线程）随窗口标题那一套删了：lib.rs 从此零处。
    (
        "src/platform/window.rs",
        "desktop_fixes",
        "wait-for-condition",
        "原 `lib.rs` 那两处的 ①：resize 稳定检测 `loop { sleep(60ms); if now == last { break } }`\
             （WebView2 最大化 / 全屏后内容错位修复的去抖，Windows 那段随平台臂搬来）。有终止条件，**不是节拍器**。",
    ),
    (
        "src/event_replay.rs",
        "replay_lines",
        "throttle",
        "两处 `CHUNK_PAUSE_MS`：分块 emit 之间让 UI 喘一口。\
             **上界是 `chunk_total`**（`if idx + 1 < chunk_total` 才 sleep），最后一块不停。",
    ),
    (
        "src/event_replay.rs",
        "on_line_batch_awaited",
        "throttle",
        "两处 `CHUNK_PAUSE_MS`：分块 emit 之间让 UI 喘一口。\
             **上界是 `chunk_total`**（`if idx + 1 < chunk_total` 才 sleep），最后一块不停。",
    ),
    // `src/session_map.rs` 那条 `ticker`（`recv_timeout(2s)` 心跳，对每个本机会话跑 `is_process_alive`〔散文墓碑〕）
    //   **真退役**，按它自己写的出路：「事件源存在但住在别的 crate」—— 本机判活改由本机后端的帧来（后端 pidfd 看守
    //   ＋ Windows 的死亡事件，RT1 F9 真机读数：后端 1 ms 就醒），monitor 那份判活连同这条心跳一起删了。
];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// 生产段：用 `guard_core` 剥（连测试段一起剥）。
fn production(raw: &str) -> String {
    guard_core::production_code(raw)
}

/// 递归遍历 `src/`，**不是硬编码文件名单**（那一族本仓踩过多次）。
fn rust_files() -> Vec<(String, String)> {
    let src = root().join("src");
    let mut out = Vec::new();
    // 人群 ＝ 本 crate 的 `src/` ＋ manifest 明写的兄弟包（窗口包 · 通道 · 宿主原语 · 开窗契约，
    //   `guard_core::population_trees`）—— monitor 的代码搬进去了，人群不变；兄弟包的键带包名。
    let trees =
        std::iter::once(("src".to_string(), src.clone())).chain(guard_core::population_trees(&src));
    for (label, tree) in trees {
        let mut stack = vec![tree.clone()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p: PathBuf = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_some_and(|x| x == "rs") {
                    let rel = format!(
                        "{label}/{}",
                        p.strip_prefix(&tree)
                            .unwrap_or(&p)
                            .to_string_lossy()
                            .replace('\\', "/")
                    );
                    out.push((rel, fs::read_to_string(&p).unwrap_or_default()));
                }
            }
        }
    }
    out.sort();
    out
}

/// 行里有没有 `name(` 这个**调用**（`name` 要是完整的词）。
fn is_call_of(line: &str, name: &str) -> bool {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut from = 0usize;
    while let Some(i) = line[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        if line[..at].chars().next_back().is_some_and(ident) {
            continue;
        }
        if line[from..].trim_start().starts_with('(') {
            return true;
        }
    }
    false
}

/// 一处「周期唤醒」的源码形态 —— 按**调用**取样，不按路径拼法取样。
///
/// # 原来锚的是路径，于是锚点其实落在 `use` 那一行
///
/// 旧口径是三个带路径的串（`thread::sleep` / `time::sleep` / `time::interval`）。
/// 那意味着：只要**导入形态**变一下，整处唤醒就隐形。实测：
///
/// | 写法 | 旧口径 | 现在 |
/// |---|---|---|
/// | `use tokio::time::sleep;` + `sleep(d).await` | 抓到（`use` 行含 `time::sleep`） | 抓到 |
/// | **`use tokio::time::{self as _t, sleep};`** | **全绿** | 抓到 |
/// | **`use tokio::time::*;`** + `sleep(d)` | **全绿** | 抓到 |
///
/// 两个逃逸的都是**一个真的无限循环 + sleep**（`loop { sleep(5s).await }`），
/// 也就是定框 E6 逐字要挡的东西。
///
/// ⇒ 与本区反复记的那条同源：**判据锚在「怎么写」上，而不是锚在「做了什么」上**。
/// 改成按调用取样（`sleep(` / `interval(`，名字要是完整的词），
/// 与 `use` 怎么写、有没有别名、是不是 glob 导入统统无关。
///
/// ⚠ 换口径前量过误红面：新口径下**每个文件的命中数与旧口径完全一致**
/// （登记表一行没改），说明今天没有靠路径拼法躲着的、也没有新卷进来的。
/// ⚠ 仍**刻意不含** `Duration::from_`（那是取值不是唤醒）—— 见模块头注。
fn wake_fns(prod: &str) -> Vec<String> {
    // 判据串运行时拼，免得命中本文件自己的说明。
    let names = [
        format!("{}", "sleep"),
        format!("{}", "interval"),
        format!("{}", "recv_timeout"),
    ];
    let code = guard_core::strip_comment_lines(prod);
    let lines: Vec<&str> = code.lines().collect();
    (0..lines.len())
        .filter(|&i| names.iter().any(|n| is_call_of(lines[i], n)))
        .map(|i| crate::guard_support::enclosing_fn(&lines, i))
        .collect()
}

/// ★ 抽取器自检：扫不到文件 / 剥太狠时，下面几条会零命中地绿。
#[test]
fn the_scan_actually_reads_the_monitor_rust_tree() {
    // 匹配器自检（两个方向都钉）：
    for s in [
        "        sleep(Duration::from_secs(5)).await;",
        "    let mut t = interval(Duration::from_secs(1));",
        "    tokio::time::sleep(d).await;",
    ] {
        assert!(
            is_call_of(s, "sleep") || is_call_of(s, "interval"),
            "调用形态没被认出来：{s:?}"
        );
    }
    // ⚠ 注释过滤住在 `wake_hits` 而不是 `is_call_of` —— 自检要按**真契约**写：
    // 第一版把「注释里提到 sleep(d)」当成 `is_call_of` 的负例，当场红，
    // 而那是我搞错了分层，不是匹配器有问题。**自检写错契约与判据写错一样会误导人。**
    assert_eq!(
        wake_fns("    // sleep(d) 只是注释里提到\n").len(),
        0,
        "注释行被当成了周期唤醒"
    );
    // ⚠ 这一组的**第一版三条全是恒绿的**：`no_sleep_here` / `asleep` 后面都不是 `(`，
    // 所以它们为假与词边界无关 —— 把词边界整段删掉，三条照样过（变异实测）。
    // 真要触发边界，反例必须是「前面接着标识符字符**而且**后面就是 `(`」。
    // ★ 本区第二次踩同一个坑（上次是 `mySetInterval`）：
    // **负向断言最容易写成恒绿的**，因为构造「真会被误命中」的输入比想象中难。
    for s in [
        "    let x = do_sleep(3);",
        "    self.my_interval(2);",
        "    let no_sleep_here = 1;",
        "    struct Sleeper;",
        "    let asleep = true;",
    ] {
        assert!(
            !(is_call_of(s, "sleep") || is_call_of(s, "interval")),
            "把不是调用的东西当成了周期唤醒：{s:?}"
        );
    }

    let files = rust_files();
    // 剥法自检：本文件自己剥完应当只剩几行（它整体是 cfg(test)）。
    let me = files
        .iter()
        .find(|(n, _)| n == "src/rust_timer_registry.rs")
        .map(|(_, s)| s.as_str())
        .expect("扫不到本文件 —— 遍历器或路径坏了");
    assert!(
        production(me).len() < me.len() / 2,
        "本文件剥完还剩一半以上 —— 剥法没生效，下面几条会把说明文字当成命中"
    );
}

/// ★ 正题：每一处周期唤醒都必须在登记表里，**数目也要对上**。
///
/// 多一处 ⇒ 红（新加了没登记）；少一处 ⇒ **也红**（退役了要把账拧下来）。
/// 后半句是递减棘轮那半 —— 只挡回潮的账不会自己往下走。
#[test]
fn every_periodic_wake_in_the_rust_tree_is_registered() {
    let mut want: Vec<(String, String)> = REGISTERED
        .iter()
        .map(|(f, func, ..)| ((*f).to_string(), (*func).to_string()))
        .collect();
    want.sort();
    want.dedup();
    let mut got: Vec<(String, String)> = rust_files()
        .into_iter()
        .flat_map(|(rel, raw)| {
            wake_fns(&production(&raw))
                .into_iter()
                .map(move |f| (rel.clone(), f))
        })
        .collect();
    got.sort();
    got.dedup();
    assert_eq!(
        got, want,
        "\nRust 侧周期唤醒的实际分布与登记表对不上。\n\
             **多一处** = 新加了没登记 —— 先回答它属哪一类（ticker / wait-for-condition / \
             throttle / startup-delay），`ticker` 还要写明事件源与退役归属。\n\
             **少一处** = 退役了 —— 把登记表那条删掉，并把处数拧下来。\n\
             ⚠ 本表**刻意不查 `Duration::from_*`**：实测 23 处全是 timeout/debounce/退避上界，\
             查它只会得到噪音（见模块头注的论证）。"
    );
}

/// ★ `ticker` 这一类**必须**写明事件源与退役归属 —— 那是它与其余三类的分界。
///
/// 其余三类只要说清「等什么 / 上界从哪来」；只有真节拍器要回答「谁来杀掉它」。
#[test]
fn every_ticker_names_its_event_source_and_owner() {
    for (f, _, kind, why) in REGISTERED {
        assert!(
            matches!(
                *kind,
                "ticker" | "wait-for-condition" | "throttle" | "startup-delay"
            ),
            "{f} 的类别 `{kind}` 不在四类里 —— 新类别要先在模块头注那张表里定义"
        );
        if *kind == "ticker" {
            assert!(why.contains("事件源"), "{f} 记成 ticker 却没说事件源在哪");
            assert!(
                why.contains("退役"),
                "{f} 记成 ticker 却没说谁退役它（没人认领也要写「未排期」，别留空）"
            );
        }
    }
}

/// ★ **F12 的解锁闹钟**〔devbench F12 摸底，08-10〕。
///
/// # 它盯的是什么
///
/// `session_map.rs` 那条 2s 心跳的退役归 **F12**，而 F12 被 **U4b** 挡着：
/// backend 侧 `platform/pidwatch/fallback.rs` 在非 Linux 上是**一个诚实的空壳**
/// （头注原话），`on_dead` 永远不会被调用。而那条心跳治的 bug 恰恰是 Windows 场景
/// （关终端窗口 ⇒ `claude.exe` 被强杀 ⇒ pidfile 不会被删 ⇒ 死 Tab 永远 live）。
///
/// ⇒ **U4b 落地的那一刻就是 F12 能开工的那一刻**，而在本条之前
/// **没有任何东西会在那一刻说话** —— 只能靠人回来重读一遍计划。
///
/// # ★ 摸底顺带查出：U4b **只剩一半**
///
/// 「Windows 判活」在本仓**已经有一份在生产跑的实现** —— `session_map.rs` 的
/// `cfg(windows)` 那支：`OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` +
/// `GetExitCodeProcess`（`STILL_ACTIVE`）+ `GetProcessTimes` 与 `procStart` 比
/// （`PROC_START_TOLERANCE_TICKS`，防 PID 复用）。
///
/// 那是**轮询形态**（问一次「它还在吗」）；backend 要的是**事件形态**
/// （`watch_pid_until_exit`，阻塞等到它死）。⇒ 两者不能互相替代，
/// 但**身份校验那一半可以直接搬**，U4b 真正缺的只是 `WaitForSingleObject` 那一段。
/// ⚠ 这条订正很要紧：计划里 U4b 一直被当成「从零写 + 要真机验证」，
/// 而实际上它的一半已经在 Windows 用户机上跑了很久。
///
/// # 失效模式（如实登记）
///
/// ⚠ 第一版按「`fallback.rs` 里出现 `OpenProcess`/`WaitForSingleObject` 这两个名字」判，
/// **首跑就假红** —— 那两个词逐字写在它自己的 `tracing::error!` **文案**里
/// （「真实现见 U4b（OpenProcess + WaitForSingleObject）」），而文案是代码不是注释。
/// ⇒ 又一次「针打在字面量上」。改成按**行为性质**判：
/// 空壳的定义就是它头注承诺的那句 ——「`on_dead` **永远不会被调用**」。
///
/// 本条按「`on_dead` 有没有被调用」判。若 U4b 换个文件落地
/// （比如新开 `windows.rs` + 改 `mod.rs` 的 `cfg`），本条**看不见** ——
/// 那时 `mod.rs` 的 `cfg` 会变，而本条不读它。⇒ 它是个闹钟，不是围栏。
///
/// # 🔴 **闹钟响过了，而且正是上面那句预言的形状**
///
/// U4b 落在一份新文件里（`pidwatch/win32.rs` ＋ `mod.rs` 的 `cfg` 改成 Windows 走它），
/// `fallback.rs` 一个字的行为都没变 ⇒ 旧版本条会**继续绿**、一声不响 —— 头注自己写的那个盲区。
/// ⇒ 本条改成盯**新的事实**：Windows 那一格路由到 `win32.rs`，而那份文件真的会调 `on_dead`。
/// 它从「等 U4b 的闹钟」变成「U4b 别退回去的围栏」；**F12 从这一拍起可以开工**
/// （上面「要做的两件事」照旧：把 `pidwatch` 抽成两侧共用的 crate · `session_map.rs` 删 2s 心跳 ——
/// 那另做）。
/// ⚠ 别把「F12 可以开工」读成「Windows 上判活验过了」：`win32.rs` 只买到编得过 ＋ 与 `linux.rs`
/// 的源码对拍，真机零读数（WN1 不碰 Win11 虚拟机）。F12 删心跳之前，那一格值得先买一次真机读数。
#[test]
fn the_windows_pidwatch_has_landed_so_f12_is_unblocked() {
    let root = crate::guard_support::repo_root();
    let read = |rel: &str| {
        let p = root.join(rel);
        std::fs::read_to_string(&p).unwrap_or_else(|e| {
            panic!(
                "{} 读不到：{e} —— Windows 那一格的看守搬家了？回去读 F12。",
                p.display()
            )
        })
    };
    let router = guard_core::production_code(&read("src/backend/platform/pidwatch/mod.rs"));
    guard_core::pin_line(&router, "pub(crate) use win32::watch_pid_until_exit;").unwrap_or_else(|e| {
        panic!(
            "{e}\n⇒ Windows 那一格不再路由到 `pidwatch/win32.rs` —— U4b 退回去了（或者搬了家）。\n\
             F12 若已按「Windows 有死亡事件」删掉 `session_map.rs` 的 2s 心跳，那条心跳治的 bug 会回来：\n\
             关终端窗口 ⇒ `claude.exe` 被强杀 ⇒ pidfile 不会被删 ⇒ 死 Tab 永远 live。"
        )
    });
    let win32 = guard_core::production_code(&read("src/backend/platform/pidwatch/win32.rs"));
    // ★ 判的是**行为**不是名字（上一版的教训）：那条腿的定义是「`on_dead` 会被调用」。
    let called = format!("on_{}()", "dead");
    assert!(
        win32.contains(&called),
        "`pidwatch/win32.rs` 的生产段里一次 `on_dead` 都不调了 —— Windows 那条死亡事件的腿又没了。"
    );
    // 空壳那一份照旧是空壳（它今天只管既非 Linux 也非 Windows 的平台）。
    let fallback = guard_core::production_code(&read("src/backend/platform/pidwatch/fallback.rs"));
    assert!(
        !fallback.contains(&called),
        "`pidwatch/fallback.rs` 开始调 `on_dead` 了 —— 它是没有看守那几个平台的诚实空壳，\n\
         「立刻调 `on_dead`」是它头注列为最坏的那个选项（活进程被判死）。"
    );
}

/// ★ `bind.rs` 那个真节拍器的**形态**没变：还是「无限循环 + 周期 sleep」。
///
/// ⚠ **函数名里的 `the_one_real_ticker` 是历史措辞，别读成「全仓唯一」** —— 08-10 起
/// 登记表里有 **4 条** ticker（见上一条判据的诊断）。本条只查 `bind.rs` 这一个，
/// 因为它是唯一「事件源都还没有」的那个；另三条各自有事件源与退役归属。
/// 不改函数名是刻意的：改名会牵动引用它的地方，而误导用一句头注就消掉了。
///
/// 它一旦被改成事件驱动（或加上终止条件），本条会红 —— **那是好事**，
/// 提醒把登记表那条从 `ticker` 降级到别的类。
#[test]
fn the_one_real_ticker_still_looks_like_a_ticker() {
    let src = production(include_str!("../../../src/frontend/shell/src/bind.rs"));
    let at = src
        .find("fn run_heartbeat")
        .expect("`bind.rs` 里找不到 `run_heartbeat` —— 它被改名或删了，回 F09 更新登记表");
    let body = &src[at..src.len().min(at + 400)];
    let verb = format!("thread::{}", "sleep");
    assert!(
        body.contains("loop {") && body.contains(verb.as_str()),
        "`run_heartbeat` 不再是「无限循环 + 周期 sleep」了 —— **这多半是好事**，\n\
             但登记表里它还记成 `ticker`：请回 F09 把那条改成它现在真实的类别。\n\
             抽到的函数体：{body}"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// F17：**monitor Rust 自己拼出来的 shell 周期唤醒**（三张表此前都看不见的那一族）
// ═══════════════════════════════════════════════════════════════════════

/// shell 形态的周期唤醒。**只收「只可能是 shell」的写法**。
fn shell_wake_hits(prod: &str) -> usize {
    // 判据串运行时拼，免得命中本文件自己的说明。
    let pats = [
        format!("while {}", "[ "),
        format!("while {}", "true"),
        format!("until{}", " "),
        format!("sleep{}", " "),
        format!("us{}", "leep"),
        format!("$(s{}", "eq "),
    ];
    prod.lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && pats.iter().any(|p| l.contains(p.as_str()))
        })
        .count()
}

/// ★ 抽取器自检：模式面既没画小也没画大。
#[test]
fn the_shell_wake_scan_is_neither_too_narrow_nor_too_wide() {
    // ★ **画大了会怎样**：摸底时 `for i in` 误命中了 `find_ci` 里那个 Rust `for i in 0..n`。
    //   这条把那次教训钉住 —— 模式面不许收到 Rust 的循环写法。
    //
    // ⚠ **标的搬家了，本条跟着搬**：那个循环原先住 `src/search.rs`，
    //   收口后随 `find_ci` 搬进共享 crate `src/common/search-core/src/lib.rs`
    //   （`rust_files()` 只走 `src/`，够不着）。⇒ 直接按住址读，别让本条以
    //   「`search.rs` 里那个循环不见了」的形态红 —— **报的方向是错的**，
    //   它不见了不是因为有人删了它，是因为它搬走了。
    // 又搬一次：`search-core` 拆进后端，`find_ci` 随通用口径住 `src/backend/observe/search_rules.rs`。
    let anchor = crate::guard_support::repo_src_root().join("backend/observe/search_rules.rs");
    let search = production(&fs::read_to_string(&anchor).unwrap_or_default());
    assert!(
        search.contains(&format!("for i in 0{}n", "..")),
        "`src/backend/observe/search_rules.rs` 里那个 Rust `for i in 0..n`（`find_ci`）不见了 \
             —— 下面那条反向断言失去了标的"
    );
    assert_eq!(
        shell_wake_hits(&search),
        0,
        "模式面把 `search_rules.rs` 的 **Rust** 循环也收进来了 —— 那是摸底时踩过的那个错法\n\
             （画大了 ⇒ 这张表被噪音填满 ⇒ 与画小了一样失去意义）"
    );
}

/// ★★ **两个方向**：每一处 shell 周期唤醒都登记了；登记表里没有已经不存在的。
///
/// ⚠ 发现机制是**遍历**（`rust_files()`），不是手写清单 ——
/// 「一组同类东西都必须满足 X」的判据，清单只能用来**表态**（F12 在另一个守卫脚下逮到过我）。
#[test]
fn every_shell_shaped_periodic_wake_is_registered() {
    // ★ 反向自检先跑（`K-R104`）：表空了之后，「扫不到」必须与「扫描器坏了」分得开。
    //   合成一条 shell 轮询喂给同一把尺子，它必须数出 ≥1。
    let synthetic = format!(
        "let s = format!(\"while {} $i -lt 3 ]; do {} 0.5; done\");",
        "[", "sleep"
    );
    assert!(
        shell_wake_hits(&synthetic) >= 1,
        "尺子认不出一条摆在面前的 shell 轮询 —— 下面那格是零命中地绿"
    );
    let got: Vec<(String, usize)> = rust_files()
        .into_iter()
        .filter_map(|(rel, raw)| {
            let n = shell_wake_hits(&production(&raw));
            (n > 0).then_some((rel, n))
        })
        .collect();
    assert!(
        got.is_empty(),
        "\nmonitor Rust 又**拼出了 shell 周期唤醒**：{got:?}\n\
             远端那一侧的等待归后端（它有内核事件源）；先问能不能改问后端。"
    );
}
