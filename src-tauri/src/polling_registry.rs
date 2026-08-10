//! **前端 + `shared/` 下所有 shell 的周期唤醒清账**（用户 2026-08-03：「现在又轮询吗?
//! 尽量不要轮询，既然都收编了那就尽量在内部进行事件驱动」）。
//!
//! ⚠ **08-10（devbench F07）扩面**：本行原写「前端 + `shared/ccm`」，而扫描面确实只
//! `push` 了 `shared/ccm` **一个写死的文件名** ⇒ `shared/cc-bus/` 那棵树整个在账外，
//! 其中 `cc-busd` 是个长驻 broker、每 0.5s 扫一次队列目录（它自己的注释承认「本实现恒轮询,
//! 未用 inotify」）。★ **这不是「另一个仓不该管」**：同仓的 `shell_lint_registry` 与
//! `session_name_registry` **都**已把那棵树算进人群，只有本表没跟上 —— 这是「判据的人群
//! 从已经有名字的那批派生」在本表身上的实例。
//! ⇒ 人群改成**遍历 `shared/` 下所有 shell**（`.sh` 后缀 **或**首行有 shebang ——
//! `cc-busd`/`cc-send` 这些没后缀），加一个脚本自动进人群，不用谁记得回来 push 一行。
//!
//! # 为什么是本模块，而不是把 daemon 那条护栏扩过来
//!
//! daemon 侧的 `no_timer_guard`（§41）是**零容忍**的：那边一个自己醒过来的构件都不许有。
//! 它的头注自己写着范围：「**只钉 daemon crate。** monitor 侧另有自己的轮询纪律
//! （那边有 UI 刷新、重连退避等**正当**周期行为），把本护栏扩过去会立刻变成噪音
//! ⇒ **要钉那半得单独论证**。」
//!
//! **那个论证到今天没人做过** —— 于是前端与 `shared/ccm` 这半**一条机检都没有**，
//! 而两个文件的头注里写着「本文件里不得出现 setInterval / setTimeout 轮询」这类**散文纪律**
//! （`account-usage.ts` · `settings/cc-bus-section.ts`）。散文纪律 = 没有纪律，
//! 这正是本工作区一直在治的病。
//!
//! # 论证：这一半为什么是**登记表**而不是禁令
//!
//! 前端确实有**正当**的周期行为（UI 重绘时钟、退避），一刀禁掉就是 daemon 那条护栏
//! 预言的噪音。所以本模块钉的不是「不许有」，而是「**每一处都得说清它是哪一类、
//! 谁来退役它**」：
//!
//! | 类别 | 含义 | 要求 |
//! |---|---|---|
//! | `ui-clock` | 只重绘已有状态、**不取数** | 说清它不取数 |
//! | `data-poll` | **真轮询** —— 周期性去取数据 | **必须写明事件源在哪 + 谁退役它** |
//! | `wait-for-condition` | 等一个一次性条件（有上限，不是节拍器） | 说清没有内核事件源可用 |
//! | `one-shot` | **压根不是周期唤醒**（键入节奏 / kill 宽限 / 启动让路） | 说清它为什么不在任何循环的每轮上 |
//!
//! ⚠ `one-shot` 是 08-10 扩面时加的第四类，理由是 shell 那条针**刻意保持宽**
//! （`contains("sleep ")` 会命中一次性 sleep）。收窄针会漏掉「新加一个 `sleep 1` 在循环里」
//! 那种真轮询 ⇒ **宁可宽松让人判断，也不要用严格的错误引入噪声**。代价是一次性的也要
//! 登记一行，而那正是「默认拒绝」想要的：新加一处就得回答它是哪一类。
//!
//! # 它查什么、查不了什么
//!
//! **两层**，分工不同：
//!
//! 1. `REGISTERED` + `is_periodic` —— 查 TS 生产段（注释剥掉）里的 `setInterval`、
//!    **`poll` 命名的递归 `setTimeout`**、以及 `shared/ccm` 里的 `sleep`，
//!    要求每一处**说清它是哪一类、谁退役它**。钉的是「认出来的有没有主人」。
//! 2. `SCHEDULING_SITES` —— 查**全部** `setInterval` / `setTimeout` /
//!    `requestAnimationFrame` / `requestIdleCallback` 调用点，要求每一处**被分类过**。
//!    钉的是「**有没有认漏**」。
//!
//! ⚠ 第 2 层是〔audit-0805 F14 第二刀〕补的，补之前第 1 层有一个**实测过的洞**：
//! 「没按 `poll` 命名的递归 `setTimeout`」在语法上与一次性延时无法区分 ⇒
//! `views/history.ts` 那条「索引构建中 → 1 秒后自动重试」的每台一条 SSH **零命中**，
//! `tabs.ts` 的 rIC 物化队列、`session-viewer.ts` 的 rAF 补料链等**五处自链全部逃逸**。
//! 第 2 层没有把这些形态认得更聪明，而是**换了失守方向**：
//! 从「认出来的才要登记」改成「每一处都必须被分类」，**逃逸变成失败即红**。
//!
//! ⚠ **第 2 层保证的是「没有未分类的调度点」，不是「分类都对」** ——
//! 分类是人写的，写错了机器看不出来。**比没有强，别读成证明。**
//!
//! # monitor 的 Rust 侧：**不在本模块范围内，但已经有人管了**
//!
//! 本模块只覆盖 **TS 与 `shared/` 下的 shell**。Rust 那半的家是 **`rust_timer_registry`**（F09 建），
//! 同一套分类词汇（`ticker` / `wait-for-condition` / `throttle` / `startup-delay`）。
//!
//! ⚠⚠ **这段话此前是一句假陈述**，而且是最难被怀疑的那一种 —— 它原文写着：
//! 「monitor 的 Rust 侧刻意不在范围内…**如实登记为未做，不假装覆盖了**」。
//! 那句话在写下时是真的；F09 把那半做掉之后它就烂了，而**没有任何东西会因此变红**。
//! ★ **一句自称诚实的话烂掉，比一个错数字更贵**：它读起来像是「这里有人想过了」，
//! 于是下一个人不会去查。（audit-0805 F14 §4 复核时才发现。）
//! ⇒ 现在由 `the_other_half_of_the_sweep_still_has_a_home` 钉着：
//! `rust_timer_registry` 一旦消失，这段指针立刻变红。
//!
//! ⚠ 那半一上岗就抓到**两个真节拍器**（`bind.rs::run_heartbeat` 10s ·
//! `ssh_source.rs` daemonless 2s），两个都**如实记为未排期** —— 别读成「已经清干净了」。
//!
//! ⚠⚠ **08-10 订正：那个数今天是 4，不是 2。** devbench F07 把 `recv_timeout` 收进那张表的针
//! 之后，又上账两条：`watcher.rs` 的 **100ms（10Hz，全仓最快的一处）** 与 `session_map.rs` 的 2s。
//! ★ **它们在那之前一直在跑** —— 只是那张表的针（当时只有 `sleep`/`interval`）看不见它们，
//! 而 `doc/INVARIANTS.md:1379` 与 daemon 侧 `no_timer_guard` 的扫描面**都早已点名 `recv_timeout`**
//! ⇒ 缺的不是认知，是针没跟上。这两条的退役各有归属（devbench F11 / F12）。
//!
//! ⚠⚠⚠ **上面那句「今天是 4」当天就过期了 —— 08-10 收官时是 3**〔G 审计逮到〕。
//! devbench **F11 真的把 `watcher.rs` 那条退役了**（两条通道合成一个 `WatchEvent` enum、
//! 主循环改无超时 `recv()`），于是 4 → 3。
//! ★ 这条陈账的形状值得记：**A 模块的头注在描述 B 模块的状态，而指针那一侧没有判据。**
//! F11 只改了 `rust_timer_registry.rs` + `watcher.rs` 两个文件，没人会回来改这里。
//! ⇒ **别在散文里抄那个数** —— 权威在 `rust_timer_registry::the_ticker_count_is_pinned`
//! 的 `assert_eq!`，它自己会说话（本文件下面那条 `the_other_half_of_the_sweep_still_has_a_home`
//! 已经在钉「指针目标存在」，但**没钉那个数**）。

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// 周期唤醒的**登记表**：`(相对仓根的路径, 类别, 为什么 + 谁退役它)`。
    ///
    /// 多一处没登记的 ⇒ 下面那条红。**登记表不是豁免清单**，是「这些我看过、
    /// 而且知道它归谁」的账。
    const REGISTERED: &[(&str, &str, &str)] = &[
        (
            "src/session-accounts-poll.ts",
            "data-poll",
            "10s 拉一次 `refreshSessionAccounts`（会话↔账号映射）。\
             ⚠ **F14 第三刀搬家**：原来住 `main.ts`（那里一个 export 都没有 ⇒ 三条性质一条都测不了）。\
             搬过来之后扇出有上限（4，保序）、有重入锁、窗口不可见时跳过、`stop()` 停得掉。\
             **周期本身没变，仍是 10s**，所以这条登记照旧成立。\
             ⚠ **F02 订正**：原文写「事件源已经存在」，实测**只有一半成立** —— \
             本 UI 自己切号确实有回调（`onDefaultChanged → refreshSessionAccounts`），\
             但「**别人**（另一个 monitor / 终端里的 ccm）改了账号」**没有事件源**：\
             daemon 的帧集合里**没有任何账号帧**（`Hello/Line/TmuxSessions/SessionAdded/…` 十四种，\
             逐个数过），`--session-accounts` 是**一次性子命令查询**、不是帧。\
             ⇒ 这 10s 补的正是那一块。退役归 **U7e**，而 U7e 的前提是**先有一种账号事件** \
             （新帧或文件事件），不是「已经有了」。",
        ),
        (
            "src/views/grid-monitor.ts",
            "ui-clock",
            "1s 重绘一次网格。**不取数** —— 只把已有状态（相对时间等）重画；\
             取数走事件（`events.ts` 的帧）。这一类是 daemon 那条护栏头注说的「正当周期行为」。",
        ),
        (
            "src/tabs.ts",
            "data-poll",
            "`awaitExitFor`：等 claude 退出时每 1s 拉一次 `list_remote_tmux`（有 timeout 上限）。\
             ⚠⚠ **F02 订正：原文那句「事件源已经存在」是错的。** 它等的**不是会话消失**，\
             是 `claudeExited` —— 「目标 sid 不再被精确命中」，也就是**前台命令从 claude 变回 shell**\
             （会话还在）。而 daemon 只装三条 hook：`session-created` / `session-closed` / \
             `session-renamed`（`control/tmux_hook.rs::HOOK_EVENTS`，逐个数过）——\
             **「pane 里的命令变了」一条都不覆盖**，而 P5 之后 daemon 零定时器 ⇒ \
             那种情形下帧**可能永不刷新**。改等帧会永远等到超时再降级 kill。\
             ⇒ 这 1s 轮询**正在补 hook 覆盖不到的那一块**，今天**不能退役**。\
             **退役归**「先造一个『pane 前台命令变化』的事件源」这件事本身 —— \
             tmux 没有这种 hook，能想到的路只有轮询 `capture-pane` 或让 claude 自己上报；\
             **今天无人认领，如实登记为未排期**（不写一个假的 owner 让它看起来有人管）。\
             ⚠ 这一处是普查**新发现**的：它用递归 `setTimeout` 而不是 `setInterval`，\
             此前任何地方都没登记过。",
        ),
        (
            "shared/ccm",
            "wait-for-condition",
            "两处：① 预信任对话框等待（6 × 0.5s，**§1.3 登记在案的例外** —— 那个对话框没有\
             内核事件源，只能看屏）；② 1s 身份轮询（`sleep 1`）。**②是真 data-poll**，\
             退役归 **U9b**（thin ccm 变零决策执行臂）。⚠ 一个文件两类，故按文件登记。",
        ),
        // ★★ **08-10（devbench F07）扩面后逮到的一族**：`shared/cc-bus/scripts/`。
        // 本表原来的人群是「`src/**/*.ts` + 写死的 `shared/ccm` 一个文件名」⇒ 这棵树整个在账外。
        // ⚠ 第四类 `one-shot` 是这次新加的：shell 那条针是宽的（`contains("sleep ")`），
        // 一次性 sleep 也会命中。**刻意不收窄针**（宁可宽松让人判断，也不要用严格的错误引入噪声）
        // —— 收窄会漏掉「新加一个 `sleep 1` 在循环里」那种真轮询。代价是一次性的也要登记一行，
        // 而那正是「默认拒绝」想要的：新加一处就得回答它是哪一类。
        (
            "shared/cc-bus/scripts/cc-busd",
            "data-poll",
            "★ **本表扩面当天逮到的唯一真轮询**：长驻 broker 进程，`while [ \"$running\" = 1 ]` \
             里每 0.5s 醒一次扫队列目录（`:101` `sleep \"$POLL\"`；`:99` 是「一整轮没进展」的退避）。\
             它**自己的注释就承认了**：`:20` 逐字「队列空时轮询间隔秒(**本实现恒轮询,未用 inotify**)」。\
             **事件源**：`$BUS/queue` 目录的 inotify —— 队列是文件系统目录、天然可 watch，\
             与 daemon 侧看 pidfile 的做法同构。\
             **退役归**未排期（cc-bus 增强属 issue #77/#78 那一族，用户 08-10 明确「后面再增强」）。\
             如实记未排期，不编一个假 owner 让它看起来有人管。\
             ⚠ 同文件 `:115` 另有 `for _i in $(seq 1 10); do sleep 0.3; daemon_running && break; done` \
             —— 那是 wait-for-condition（上限 ~3s，注释自陈「轮询确认最多 ~3s」），不是节拍器。",
        ),
        (
            "shared/cc-bus/scripts/cc-bus-lib.sh",
            "one-shot",
            "`:238` 的 `sleep 0.3` 夹在 `tmux send-keys <文本>` 与 `tmux send-keys Enter` 之间 —— \
             **键入节奏**，一次性。不是周期唤醒：它不在任何循环的每轮上，前后是一对 send-keys。",
        ),
        (
            "shared/cc-bus/scripts/cc-kill",
            "one-shot",
            "`:18` `kill $procs; sleep 0.3; kill -9 $procs` —— **优雅退出与强杀之间的宽限**，一次性。",
        ),
        (
            "shared/cc-bus/scripts/cc-spawn",
            "one-shot",
            "`:146` `sleep 1.5` —— **启动让路**（等被 spawn 的 agent 把自己登记上来）。一次性；\
             同文件 `:145` 注释逐字「显式保留而非默默删掉——原先 pretrusted 成功路径上就是 \
             sleep 1.5」，说明它是刻意留的既有行为。",
        ),
    ];

    /// ★ **前提触发器**：本模块头注说「Rust 那半的家是 `rust_timer_registry`」——
    /// 那句话只在**它真的还在**时成立。
    ///
    /// # 为什么这条值得单独存在
    ///
    /// 这段指针的**上一版**是「Rust 侧刻意不在范围内，如实登记为未做，不假装覆盖了」。
    /// 那句话在写下时是真的，F09 把那半做掉之后它就烂了 —— 而**没有任何东西会因此变红**，
    /// 直到 audit-0805 F14 §4 复核时才发现。
    ///
    /// ★ **一句自称诚实的话烂掉，比一个错数字更贵**：错数字会被人核对，
    /// 而「我如实记了未做」读起来像「这里有人想过了」，下一个人就不会去查。
    /// ⇒ 指针必须有判据看着，跟散文数字一样（定框 E12）。
    #[test]
    fn the_other_half_of_the_sweep_still_has_a_home() {
        let root = repo_root();
        let other = root.join("src-tauri/src/rust_timer_registry.rs");
        let body = fs::read_to_string(&other).unwrap_or_default();
        // ⚠ `contains_word` 不是 `contains`：变异实测把 `REGISTERED` 改名成 `REGISTERED_X`，
        // 裸 `contains` **照样绿**（前缀）。那正是 F24 那一族 —— 而它在这条**新写的**判据里
        // 又复发了一次，说明「知道有这个坑」不等于不踩。原语在手就别手写匹配。
        assert!(
            guard_core::contains_word(&body, "REGISTERED"),
            "`rust_timer_registry` 不见了（或不再是登记表）。\n\
             本模块头注逐字说着「Rust 那半的家是它」—— 那句话此刻是假的。\n\
             ★ 要么把那半的新家写进头注，要么把头注改回「未做」；\n\
             **不许留着一句指向空处的指针** —— 那比没有注释更坏（skill 铁律 14）。"
        );
        let me = fs::read_to_string(root.join("src-tauri/src/polling_registry.rs"))
            .expect("读不到本文件");
        // ⚠ **只看头注那半**（`production_source` 把 `#[cfg(test)]` 段剥掉）。
        // 变异实测：拿整份文件 `contains` 时，**本条自己的代码里就写着这个名字**
        // （上面那个路径 join、下面那个 `mod` 断言）⇒ 散文里的指针被删光了它照样绿。
        // 那是 F23 那一族「判据匹配到自己」—— 在这条**新写的**判据里又复发了一次。
        let head_note = guard_core::production_source(&me);
        // 反向：头注真的指过去了才算。只留判据不改散文，读的人还是被那句旧话骗。
        assert!(
            guard_core::contains_word(&head_note, "rust_timer_registry"),
            "本模块头注里找不到 `rust_timer_registry` —— 指针被删了而本条还绿着，\n\
             说明本条钉的是「那半存在」而不是「这里指着它」。两件都要。"
        );
        // 那半必须真的被编进来（`mod` 声明），否则它是一份没人跑的死代码。
        let lib = fs::read_to_string(root.join("src-tauri/src/lib.rs")).expect("读不到 lib.rs");
        // `find_pinned`：恰好一处 + 两侧有边界。裸 `contains` 会被
        // `mod rust_timer_registry_v2` 之类喂饱，而那时「那半有人管」已经不成立了。
        assert!(
            guard_core::find_pinned(&lib, "mod rust_timer_registry;").is_ok(),
            "`rust_timer_registry` 没有在 `lib.rs` 里声明 ⇒ 它根本不参与编译与测试，\n\
             「那半有人管」这句话就成了空头支票。"
        );
    }

    /// ★ **前提触发器**：上面两条「今天不能退役」的理由，前提是
    /// **daemon 只装那三条 hook**（`session-created` / `session-closed` / `session-renamed`）。
    ///
    /// hook 覆盖面一变（多一条、少一条、换名字）⇒ 本条**主动红**，逼人回来重新裁定
    /// 「哪些轮询现在可以退役了」。这是好事：多一条 hook 往往正好解锁一处轮询。
    ///
    /// ⚠ 它挡不住「hook 装上了但 tmux 那个事件本身覆盖面变了」（tmux 版本差异）——
    /// 那属于外部世界，本仓钉不了。**比没有强，别读成证明。**
    #[test]
    fn the_hook_coverage_that_these_reasons_rest_on_has_not_changed() {
        const DAEMON_HOOKS: &str =
            include_str!("../../remote-daemon-proto/src/control/tmux_hook.rs");
        let prod = guard_core::production_code(DAEMON_HOOKS);
        // 判据串运行时拼，免得命中本文件自己上面那两段说明。
        let want: Vec<String> = ["created", "closed", "renamed"]
            .iter()
            .map(|e| format!("session-{e}"))
            .collect();
        for w in &want {
            assert!(
                prod.contains(w.as_str()),
                "daemon 的 hook 里找不到 `{w}` —— 覆盖面缩小了。\n\
                 上面 `src/tabs.ts` / `src/main.ts` 两条「今天不能退役」的理由建立在\
                 「只有这三条 hook」之上，覆盖面一变就要重新裁定。"
            );
        }
        // 反向：**不许多**。多一条就可能解锁一处轮询 ⇒ 主动红提醒。
        let found = prod.matches("session-").count()
            + prod.matches("window-").count()
            + prod.matches("pane-").count();
        assert!(
            found > 0,
            "一条 hook 名都没扫到 —— 剥法或路径坏了，本条会零命中地绿"
        );
        let hook_events = prod
            .split("HOOK_EVENTS")
            .nth(1)
            .unwrap_or("")
            .split("];")
            .next()
            .unwrap_or("");
        assert!(
            !hook_events.is_empty(),
            "抽不到 `HOOK_EVENTS` 数组 —— 抽取器坏了"
        );
        let n = hook_events.matches("session-").count()
            + hook_events.matches("window-").count()
            + hook_events.matches("pane-").count()
            + hook_events.matches("client-").count();
        assert_eq!(
            n, 3,
            "daemon 装的 hook 从 3 条变成了 {n} 条 —— **这多半是好事**，\n\
             但它意味着上面两条「今天不能退役」的理由前提变了：\n\
             请回 F02 重新裁定哪些轮询可以改等帧了（多一条 hook 常常正好解锁一处）。\n\
             抽到的数组：{hook_events}"
        );
    }

    /// **明令不许有周期唤醒**的文件（把两处散文纪律变成机检）。
    const NO_PERIODIC_WAKE: &[(&str, &str)] = &[
        (
            "src/account-usage.ts",
            "头注写着「没有 `setInterval`，没有后台…」—— 用量探测是重操作（起隐藏会话+网络查询），\
             周期化会把它变成后台负载",
        ),
        (
            "src/settings/cc-bus-section.ts",
            "头注写着「本文件里不得出现 setInterval / setTimeout 轮询 / 后台定时任务」",
        ),
    ];

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .to_path_buf()
    }

    /// 剥掉整行注释（`//` / `*` / `/*`）。
    ///
    /// ⚠ 必须剥：`account-usage.ts` 与 `cc-bus-section.ts` 的头注里**就写着**
    /// `setInterval` 这个词（写的是「不许有」）。不剥的话它们会被自己的纪律说明命中 ——
    /// 与 `launch-cli-wire.vitest.ts` 那次「文档注释里就写着 `deny_unknown_fields`」同一个坑。
    /// 一行里有没有周期唤醒的形态。
    /// `shared/` 下的 shell 脚本全集：`.sh` 后缀 **或** 首行有 shebang（`cc-busd`/`cc-send`
    /// 这些没有后缀）。**按内容认，不按后缀认** —— 后缀是可选的，shebang 才是「它是脚本」的证据。
    fn collect_shell(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let Ok(rd) = fs::read_dir(dir) else {
            return out;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(collect_shell(&p));
                continue;
            }
            let is_sh = p.extension().and_then(|x| x.to_str()) == Some("sh");
            let has_shebang = fs::read_to_string(&p)
                .ok()
                .and_then(|t| t.lines().next().map(|l| l.starts_with("#!")))
                .unwrap_or(false);
            if is_sh || has_shebang {
                out.push(p);
            }
        }
        out
    }

    fn is_periodic(line: &str, is_shell: bool) -> bool {
        if is_shell {
            return line.contains("sleep ");
        }
        if line.contains("setInterval") {
            return true;
        }
        // `poll` 命名的递归 setTimeout —— 见模块头注的诚实边界。
        line.contains("setTimeout") && line.to_lowercase().contains("poll")
    }

    /// 扫描面：`src/**/*.ts`（排除测试）+ **`shared/` 下所有 shell 脚本**。
    ///
    /// ⚠ **08-10（devbench F07）扩面**：原来这里是 `files.push(root.join("shared/ccm"))`
    /// —— **一个写死的文件名**。头注当时写的范围「TS 与 `shared/ccm`」在写下时是对的，
    /// 而 `shared/cc-bus/` 进仓之后就成了一个没人管的角落：`shared/cc-bus/scripts/cc-busd`
    /// 是个**长驻 broker 进程**，每 0.5s 醒一次扫队列目录，它自己的注释逐字承认
    /// 「队列空时轮询间隔秒(**本实现恒轮询,未用 inotify**)」—— 而本表看不见它。
    ///
    /// ★ 这不是「另一个仓不该管」：同仓的 `shell_lint_registry`（扫描面含
    /// `shared/cc-bus/scripts/*`）与 `session_name_registry`（点名 `cc-spawn`）**都**已经
    /// 把那棵树算进人群了，**只有轮询这张表没跟上**。⇒ 人群改成**遍历**，
    /// 加一个脚本自动进人群，不用谁记得回来 push 一行。
    fn scan() -> Vec<(String, usize)> {
        let root = repo_root();
        let mut files: Vec<PathBuf> = Vec::new();
        collect_ts(&root.join("src"), &mut files);
        files.sort();
        let mut shells = collect_shell(&root.join("shared"));
        shells.sort();
        files.extend(shells);
        let mut out = Vec::new();
        for f in files {
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            let is_shell = !rel.ends_with(".ts");
            let src = guard_core::strip_comment_lines(&fs::read_to_string(&f).unwrap_or_default());
            let n = src.lines().filter(|l| is_periodic(l, is_shell)).count();
            if n > 0 {
                out.push((rel, n));
            }
        }
        out
    }

    fn collect_ts(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_ts(&p, out);
                continue;
            }
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if name.ends_with(".ts")
                && !name.contains(".vitest.")
                && !name.contains(".test.")
                && !name.ends_with(".d.ts")
            {
                out.push(p);
            }
        }
    }

    /// ★ 抽取器自检：扫不到东西时下面几条会零命中变绿。
    #[test]
    fn the_scan_actually_reads_the_frontend_and_ccm() {
        let root = repo_root();
        let mut ts = Vec::new();
        collect_ts(&root.join("src"), &mut ts);
        // 地板 170：08-05 实测 **190** 个
        // （`generated/` 73 · `src/` 本层 58 · `settings/` 23 · `views/` 14 · 其余 22）。
        // ⚠ 原文写「实测应约 90+」、地板 `>= 60` —— 数字腐了一倍多而判据照样绿
        // （地板式判据在「数字变大」这个方向上不会红，定框 E12 第二个陷阱的又一例）。
        //
        // **余量 20 意味着什么**：它**装不下 `settings/`（23）** ⇒ 少掉 `settings/` 或任何
        // 更大的子目录（`generated/` 73 · 本层 58）都会被这条抓住。
        // ⚠ **`views/`（14）单独消失这条抓不住** —— 那一层由 `SCHEDULING_SITES` 的反向检查
        // 兜着：分类账里有 5 个 `views/` 下的文件，它们一起消失那条会红。
        // 写清这一点是因为**「地板护住了整个扫描面」是一句很容易顺手写下的假话**。
        assert!(
            ts.len() >= 170,
            "只扫到 {} 个前端 .ts（08-05 实测 190）—— 遍历器坏了",
            ts.len()
        );
        assert!(
            fs::read_to_string(root.join("shared/ccm"))
                .map(|s| s.len())
                .unwrap_or(0)
                > 10_000,
            "shared/ccm 读不到或太短 —— 路径变了？"
        );
        // 剥注释不能把整份文件剥空。
        let ccm = fs::read_to_string(root.join("shared/ccm")).unwrap_or_default();
        assert!(
            guard_core::strip_comment_lines(&ccm).len() * 2 > ccm.len(),
            "剥注释后 shared/ccm 只剩不到一半 —— 剥法太狠"
        );
    }

    /// ★ 正题：**每一处周期唤醒都得在登记表里**。
    #[test]
    fn every_periodic_wake_is_registered_with_an_owner() {
        let found = scan();
        let registered: Vec<&str> = REGISTERED.iter().map(|(f, _, _)| *f).collect();
        let mut unregistered: Vec<String> = Vec::new();
        for (f, n) in &found {
            if !registered.contains(&f.as_str()) {
                unregistered.push(format!("  {f}（{n} 处）"));
            }
        }
        assert!(
            unregistered.is_empty(),
            "有周期唤醒没登记。**登记表不是豁免清单** —— 请写明它是哪一类\
             （ui-clock / data-poll / wait-for-condition），\
             `data-poll` 还要写明事件源在哪、谁退役它：\n{}",
            unregistered.join("\n")
        );
        // 反向：登记表里的文件必须**真的还有**周期唤醒（搬走/退役了就该删条目）。
        for (f, _, _) in REGISTERED {
            assert!(
                found.iter().any(|(g, _)| g == f),
                "登记表里的 `{f}` 已经没有周期唤醒了 —— 退役了就把这条删掉（别留成僵尸账）"
            );
        }
    }

    /// ★ `data-poll` 这一类**必须**写明事件源与退役去处 —— 那是它与「正当周期行为」的分界。
    #[test]
    fn every_data_poll_names_its_event_source_and_owner() {
        let mut polls = 0usize;
        for (f, kind, why) in REGISTERED {
            assert!(
                matches!(
                    *kind,
                    "ui-clock" | "data-poll" | "wait-for-condition" | "one-shot"
                ),
                "{f} 的类别 {kind:?} 不在四类里"
            );
            if *kind == "data-poll" || why.contains("data-poll") {
                polls += 1;
                assert!(
                    why.contains("事件源"),
                    "{f} 记成 data-poll 却没说事件源在哪"
                );
                assert!(why.contains("退役归"), "{f} 记成 data-poll 却没说谁退役它");
            }
        }
        assert!(polls >= 3, "只认出 {polls} 条 data-poll —— 分类抽取坏了");
    }

    /// **全部调度调用点的分类账**：`(相对仓根的路径, API, 处数, 这几处是什么)`。
    ///
    /// # 为什么要有这张表，而不是继续只认周期形态〔audit-0805 F14 第二刀〕
    ///
    /// `is_periodic` 认的是「一行里像不像周期唤醒」（`setInterval` / 名字带 `poll` 的
    /// `setTimeout` / `sleep `）。它的漏网在本模块头注里**早就自陈过**，本轮实测了后果：
    /// `views/history.ts` 那条「索引构建中 → 1 秒后自动重试」的递归 `setTimeout` **零命中**，
    /// `tabs.ts` 的 rIC 物化队列、`session-viewer.ts` 的 rAF 补料链等**五处自链全部逃逸**。
    /// ⇒ 「登记齐了」这句话建立在一个看不见它们的扫描面上。
    ///
    /// 修法不是把形态认得更聪明（自链在语法上确实与一次性延时难分），而是**换失守方向**：
    /// 从「认出来的才要登记」改成「**每一个调度调用点都必须被分类**」。
    /// 新增一处没登记的调度点 ⇒ 本条红。**逃逸变成失败即红。**
    ///
    /// ⚠ **不含行号，只含处数** —— 行号会腐：`00-核实台账` 记的 `views/history.ts:752`
    /// 在 F14 第一刀改过那个文件之后已经是 `:761`。处数变了才是该重新分类的时刻。
    const SCHEDULING_SITES: &[(&str, &str, usize, &str)] = &[
        ("src/account-chip.ts", "setTimeout", 1, "0ms 下一拍才挂 pointerdown/keydown 关闭监听（避免开菜单这次事件立刻把它关掉）。一次性。"),
        ("src/account-restart.ts", "setTimeout", 1, "`new Promise(r => setTimeout(r, ms))` —— sleep 助手。一次性。"),
        ("src/branch-fold.ts", "requestAnimationFrame", 1, "★ F15：live 模式主线重算的**帧末合批**（`scheduleLiveRecompute`）。排一次位（`liveScheduled`）⇒ **不是自链**：回调里不再排下一次，只有新记录到达才会再排。原来这里是逐条同步跑 `computeMainBranch`（扫全部 records 的 Kahn 拓扑）⇒ N 条记录 O(N²)。"),
        ("src/branch-fold.ts", "setTimeout", 1, "★ F15：上面那条的**无 rAF 兜底**（`typeof requestAnimationFrame !== \"function\"` 时）。0ms，一次性。"),
        ("src/branch-button.ts", "setTimeout", 1, "2s 后把按钮文字恢复成 `⑂`。一次性 UI 反馈。"),
        ("src/e2e-probe.ts", "requestAnimationFrame", 2, "★ **rAF 自链**：`sample` 每帧重排自己（起点 1 处 + 链内 1 处）。退出条件是 `stopReplayJitterProbe` 显式 `cancelAnimationFrame`。只在 e2e 探针里启用，不在正常路径上。"),
        ("src/error-toast.ts", "setTimeout", 1, "`durationMs` 后移除 toast。一次性。"),
        ("src/events.ts", "setTimeout", 3, "① `scheduleBatchEnd` 的 batch-end 哨兵（每次重排前 `clearTimeout`，且有 `BATCH_HOLD_MAX_MS` 5min 防呆上限）② ③ `setTimeout(drain, 0)` —— **队列 drain 自链**，退出条件是 `queue.length === 0`，由 `scheduled` 标志防重入。不是节拍器：没有队列就不会再排。"),
        ("src/session-accounts-poll.ts", "setInterval", 1, "10s `refreshSessionAccounts` —— **真 data-poll**，详见上面 `REGISTERED` 那条（事件源与退役去处都在那里）。⚠ F14 第三刀从 `main.ts` 搬来：唯一的一处，且句柄留着（`stop()`）。"),
        ("src/main.ts", "setTimeout", 3, "① 0ms 下一拍挂 sftp 主机选择器的关闭监听 ② ③ 1.2s 后把「已复制」还原成「复制」。全是一次性 UI 反馈。"),
        ("src/settings/cc_integration.ts", "setTimeout", 1, "500ms 后撤掉状态徽章的高亮描边。一次性。"),
        ("src/settings/config-surface-section.ts", "setTimeout", 1, "1.5s 后把「已复制」还原。一次性。"),
        ("src/settings/drift-ledger-section.ts", "setTimeout", 1, "1.5s 后把「已复制」还原。一次性。"),
        ("src/tabs.ts", "requestAnimationFrame", 3, "① `fillAbove` 批末复检（间接自链，有队列型守卫）② 切 Tab 后把面板整表 re-render 推到下一帧，入口处 `this.activeId !== sessionId` 早返。③ ★ F15：`scheduleTabBarRefresh` —— live 路上后台 tab 的 unread 徽标**帧末合批**（原来每来一行整刷一次 bar）。排一次位，不是自链。⚠ 只合批这一处，用户动作触发的十几个调用点仍是同步的（合批对它们无收益，反而把「点完立刻看到」变成「下一帧」）。"),
        ("src/tabs.ts", "requestIdleCallback", 1, "★ **空闲物化队列的自链**：`run` 处理一个后台 tab 后再排自己。退出条件是队列空。"),
        ("src/tabs.ts", "setTimeout", 10, "① `setTimeout(run, 200)` —— 上面那条 rIC 队列在 `requestIdleCallback` 缺失时的兜底，同一条自链 ② ③ 两处 `timeoutMs` 上限（`finish(false)` / `stop(false)`）④ ★ `pollTimer = setTimeout(() => void tick(), pollMs)` —— **真 data-poll**（`awaitExitFor`），见 `REGISTERED` 那条 ⑤ ⑥ hover 菜单的 150ms 开 / 250ms 关延时 ⑦ 0ms 下一拍挂右键菜单关闭监听 ⑧ ⑨ 两处 `bring_*_terminal_to_front` 的 invoke 超时拒绝。除 ④ 外都不是周期取数。 ⑩ ★ F15：`scheduleTabBarRefresh` 的**无 rAF 兜底**，0ms、一次性。"),
        ("src/views/grid-monitor.ts", "setInterval", 1, "1s 整表重绘 —— **ui-clock，不取数**，见 `REGISTERED` 那条。"),
        ("src/views/history.ts", "requestAnimationFrame", 1, "展开/收起项目后合并重画一次列表，`rafPending` 标志防重入。一次性。"),
        ("src/views/history.ts", "setTimeout", 3, "① `waitForIndexThenSearch` 的 1 秒等待 —— **wait-for-condition**（等本地索引就绪），上限 120 拍、超限有说人话的文案。⚠ **F14 第四刀改过**：它原来每秒重发 `search_history`，而那条路在 Rust 侧无条件 join 了 `search_remote_all` ⇒ **每台一条 SSH**；现在只问 `get_search_index_status`（零 SSH），就绪后补跑一次完整搜索。关视图由 F14 第一刀的 `ftSeq++` 掐断。② 0ms 下一拍挂条目右键菜单的关闭监听。③ ★ **F07 下半新增**：搜索框输入去抖（250ms，每次输入前 `clearTimeout`）—— **一次性延时不是周期唤醒**，加它正是为了**减少**下游那三个放大器被触发的次数。"),
        ("src/views/panorama.ts", "requestAnimationFrame", 3, "① `scheduleDraw` 合并重绘，`drawScheduled` 防重入 ② ③ 开/关侧栏后下一帧重算画布尺寸再画。都是一次性。"),
        ("src/views/panorama.ts", "setTimeout", 1, "250ms 搜索去抖。一次性（每次输入前 clear）。"),
        ("src/views/session-viewer.ts", "requestAnimationFrame", 5, "① ② 两处 `maybeFillAbove` —— **向上补料的 rAF 链**，五道守卫在 `:418-426`（世代 / 已到顶 / 在途 等）③ 渲染批前先让状态文绘一帧 ④ ⑤ 双 rAF 后重发 `scrollIntoView`（等 content-visibility 材料化）。"),
        ("src/views/session-viewer.ts", "setTimeout", 2, "① `setTimeout(r, 0)` 让出主线程 ② 2.2s 后移除搜索命中的闪烁 class。都是一次性。"),
        ("src/views/usage-view.ts", "requestAnimationFrame", 1, "合并重画用量列表，`rafPending` 防重入 + `seq` 世代守卫。一次性。"),
    ];

    /// 数一个调度 API 在源码里的**调用**次数（散文里提到名字不算）。
    ///
    /// # 〔audit-0805 08-06〕原来是 `matches("{api}(")`，而它旁边的注释写着「允许 `api  (`」
    ///
    /// **代码不允许，注释说允许** —— 两者对不上，而对不上的那一边正是漏洞：
    /// 把 `requestAnimationFrame (tick)`（**自链**，正是 E6 禁的连续唤醒）写进
    /// `views/usage-view.ts`，本条与 `every_periodic_wake_is_registered_with_an_owner`
    /// **两条都不响**（后者的 `is_periodic` 根本不看 rAF，只看 `setInterval` 与带 `poll` 的
    /// `setTimeout`）⇒ 一个空格就能把「全部调度调用点」这条枚举式白名单的人群缩小。
    ///
    /// ⚠ 对照：同一轮里 `setInterval (…)` **被抓住了**，但那是隔壁那条判据的裸 `contains`
    /// 顺手接住的，不是本条的功劳 —— **纵深防御会掩盖单条判据的洞**，
    /// 所以变异要看「是谁红的」，不能只看有没有红。
    ///
    /// 现在的口径：名字必须是**完整的一个词**（`myRequestAnimationFrame` 不算，
    /// `window.setInterval` 算），其后允许任意空白，然后必须是 `(`。
    fn count_calls(src: &str, api: &str) -> usize {
        let mut n = 0usize;
        let mut from = 0usize;
        while let Some(rel) = src[from..].find(api) {
            let i = from + rel;
            from = i + api.len();
            let starts_word = !src[..i]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
            if starts_word && src[from..].trim_start().starts_with('(') {
                n += 1;
            }
        }
        n
    }

    /// 扫描面：**全部**调度调用点（不只是「看起来像周期」的那些）。
    fn scan_all_scheduling_sites() -> Vec<(String, String, usize)> {
        const APIS: [&str; 4] = [
            "setInterval",
            "setTimeout",
            "requestAnimationFrame",
            "requestIdleCallback",
        ];
        let root = repo_root();
        let mut files: Vec<PathBuf> = Vec::new();
        collect_ts(&root.join("src"), &mut files);
        files.sort();
        let mut out: Vec<(String, String, usize)> = Vec::new();
        for f in files {
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            let src = guard_core::strip_comment_lines(&fs::read_to_string(&f).unwrap_or_default());
            for api in APIS {
                let n = count_calls(&src, api);
                if n > 0 {
                    out.push((rel.clone(), api.to_string(), n));
                }
            }
        }
        out.sort();
        out
    }

    /// ★ **本轮的正题**：每一个调度调用点都必须被分类过。
    ///
    /// 与 `every_periodic_wake_is_registered_with_an_owner` 的分工：
    /// 那一条钉「**认出来的**周期唤醒有没有主人」，本条钉「**有没有认漏**」。
    /// 两条都要 —— 只有前者时，一处新写的 rAF 自链可以一声不响地进仓。
    #[test]
    fn every_scheduling_call_site_is_classified() {
        // 匹配单位自检（〔audit-0805 08-06〕，两个方向都要钉）：
        // 放松的那一侧 —— 带空白的调用要数进来（本轮的洞就在这里）。
        assert_eq!(
            count_calls("requestAnimationFrame (tick);", "requestAnimationFrame"),
            1,
            "带空格的调用没被数进来 —— 一个空格就能把这条判据的人群缩小"
        );
        assert_eq!(count_calls("setInterval\n  (f, 9);", "setInterval"), 1);
        // 收紧的那一侧 —— 别把不是调用的东西也数进来，否则新口径会误红好代码。
        // ⚠ 这条第一版写的是 `mySetInterval(` —— 里面是大写 `S`，**根本不含 `setInterval`**，
        // 于是把词边界整段删掉它照样绿（变异实测）。**负向断言最容易写成恒绿的**：
        // 它要求你先造出「真的会被误命中」的输入，而那一步很容易糊弄过去。
        assert_eq!(
            count_calls("my_setInterval(f, 9);", "setInterval"),
            0,
            "`my_setInterval` 被当成了 `setInterval` —— 词边界没守住"
        );
        assert_eq!(
            count_calls("const h = setInterval; use(h);", "setInterval"),
            0,
            "只是提到名字、没有调用，不该计数"
        );
        assert_eq!(
            count_calls("window.setInterval(f, 9);", "setInterval"),
            1,
            "`window.setInterval(` 是调用，必须数"
        );
        let found = scan_all_scheduling_sites();
        // 抽取器自检：扫不到东西时下面的对拍会两边都空、静默变绿。
        let total: usize = found.iter().map(|(_, _, n)| *n).sum();
        assert!(
            total >= 30,
            "全仓只扫到 {total} 个调度调用点（实测应为 40+）—— 抽取器坏了，\
             下面的对拍会在两边都空的情况下变绿"
        );

        let mut missing: Vec<String> = Vec::new();
        let mut drifted: Vec<String> = Vec::new();
        for (f, api, n) in &found {
            match SCHEDULING_SITES
                .iter()
                .find(|(g, a, _, _)| g == f && a == api)
            {
                None => missing.push(format!("  {f}  [{api}]  {n} 处")),
                Some((_, _, want, _)) if want != n => {
                    drifted.push(format!("  {f}  [{api}]  表里 {want} 处，实测 {n} 处"))
                }
                Some(_) => {}
            }
        }
        assert!(
            missing.is_empty(),
            "有调度调用点没被分类过。**这张表不是豁免清单** —— 请写清这几处各是什么：\n{}\n\
             （一次性 UI 反馈 / 有退出条件的自链 / 真 data-poll；是 data-poll 的还要进 `REGISTERED`）",
            missing.join("\n")
        );
        assert!(
            drifted.is_empty(),
            "调度调用点的处数变了 —— **那正是该重新分类的时刻**，别只改数字：\n{}",
            drifted.join("\n")
        );
        // 反向：表里的条目必须**真的还在**（搬走/删掉了就该删条目，别留僵尸账）。
        for (f, api, want, _) in SCHEDULING_SITES {
            assert!(
                found.iter().any(|(g, a, _)| g == f && a == api),
                "分类账里的 `{f} [{api}]`（{want} 处）已经一处都不剩了 —— 删掉这条"
            );
        }
    }

    /// ★ **每秒醒一次的循环，每次醒来不许起外部进程**〔audit-0805 F14 第六刀〕。
    ///
    /// `shared/ccm` 的身份 poller 是本仓**唯一一条与会话同寿的每秒循环**，而且它
    /// **每会话一条、跑在远端机器上**。它醒来做什么，代价要乘以「会话数 × 会话时长」。
    ///
    /// 原来那一行是 `s="$(grep -o … | head -1 | cut -d'"' -f4)"`。
    /// 实测（`strace -f -c -e trace=execve,clone,clone3`，101 轮减 1 轮除以 100）：
    /// **每 tick 7 次** clone/execve（三个外部进程 + 命令替换的子 shell）；
    /// 换成纯 builtin 的 `_ccm_sid_from_file` 之后 **0 次**。
    /// 加上 `sleep 1` 固定的 2 次 ⇒ 每 tick 从 **9 次降到 2 次**。
    ///
    /// ⚠ 本条钉的是「**每次醒来的代价**」，不是「醒不醒」。
    /// 「别每秒醒」要 inotify，得动 ccm 的进程模型 —— 如实登记为未做，见 `F14 §17`。
    #[test]
    fn the_per_second_identity_poller_spawns_nothing_per_tick() {
        let ccm = fs::read_to_string(repo_root().join("shared/ccm"))
            .expect("shared/ccm 读不到 —— 路径变了就把这条一起改");
        let start = ccm
            .find("while kill -0 ")
            .expect("找不到身份 poller 的循环头 —— 它被改写或搬走了，本条会零命中地绿");
        let body_start = start + ccm[start..].find('\n').expect("循环头没换行");
        let end = ccm[body_start..]
            .find("\n    done")
            .expect("找不到循环尾 `done` —— 缩进变了？本条会把整份文件当循环体");
        let body = &ccm[body_start..body_start + end];
        // 抽取器自检：抽出来的必须像个循环体，不能是空的、也不能是整份文件。
        assert!(
            (3..40).contains(&body.lines().count()),
            "抽到 {} 行，不像那个循环体（抽取器坏了）：\n{body}",
            body.lines().count()
        );
        // ★ **整行钉**，不是子串钉〔F24〕：原来这里是 `body.contains("sleep 1")`，
        // 而 `sleep 10` **也含有** `sleep 1` ⇒ 有人把频率从每秒改成每 10 秒时，
        // 本条照样绿，而判据名与上面整段头注都写着「**每秒**」。
        // 那是「匹配单位（子串）比事实（整行 `sleep 1`）小」这一族的活样本 ——
        // audit-0805 已实测三次（F05 起流/起流程 · F16 前缀 · F19 同名参数），
        // 三次都只在造变异时才看得见。
        guard_core::pin_line(body, "sleep 1").unwrap_or_else(|e| {
            panic!(
                "身份 poller 的循环体里钉不住那一行 `sleep 1`：{e}\n\
                 ⚠ 若是**周期改了**（比如改成 `sleep 10`），那不是把本条放宽的理由 —— \n\
                 本条头注整段（每 tick 的代价 × 会话数 × 会话时长）都是按「每秒」算的，\n\
                 周期变了要连头注一起重写。若是**抽错了段**，先修抽取器。\n\
                 循环体逐字：\n{body}"
            )
        });

        // 剥掉整行注释再判（头注里就写着 `grep -o …` 那一行原文）。
        // ⚠ 再把**算术展开** `$((…))` 换掉：它长得像命令替换但是 builtin、不 fork。
        // 本条第一次跑就是被 `n=$((n+1))` 误伤的 —— 诊断把循环体原文打出来才看出来。
        let prod: String = body
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n")
            .replace("$((", "«arith»");
        for bad in ["$(", "grep ", "head ", "cut ", "sed ", "awk ", "cat "] {
            assert!(
                !prod.contains(bad),
                "身份 poller 的循环体里出现了 `{bad}` —— 它每秒跑一轮、与会话同寿、\n\
                 每会话一条且在远端机器上。实测一条 `grep|head|cut` 管道 = **每 tick 7 次** \n\
                 clone/execve；纯 builtin 是 0 次。解析请走 `_ccm_sid_from_file`（纯 builtin，\n\
                 结果写 `$_ccm_sid_out`，**不要用命令替换取回**——那本身就要 fork 一个子 shell）。\n\
                 循环体逐字：\n{prod}"
            );
        }
    }

    /// ★ 把两处**散文纪律**变成机检：这两个文件里一处周期唤醒都不许有。
    #[test]
    fn the_files_that_forbid_polling_really_have_none() {
        let root = repo_root();
        for (f, why) in NO_PERIODIC_WAKE {
            let raw = fs::read_to_string(root.join(f))
                .unwrap_or_else(|e| panic!("{f} 读不到：{e} —— 文件搬了就把这条一起改"));
            assert!(raw.len() > 500, "{f} 只有 {} 字节，像是抽错了", raw.len());
            let prod = guard_core::strip_comment_lines(&raw);
            let hits: Vec<&str> = prod
                .lines()
                .filter(|l| is_periodic(l, false))
                .map(|l| l.trim())
                .collect();
            assert!(
                hits.is_empty(),
                "`{f}` 的纪律是「不许有周期唤醒」（{why}），却出现了：{hits:?}"
            );
        }
    }
}
