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
            "**只剩一处**：预信任对话框等待（6 × 0.5s，**§1.3 登记在案的例外** —— 那个对话框\
             没有内核事件源，只能看屏）。\
             ⚠ **`U-NP④`（2026-08-14）**：本条原来还有「② 1s 身份轮询（`sleep 1`）」，\
             那是本仓唯一一条**与会话同寿、每会话一条、跑在远端**的每秒循环。\
             用户裁定「不要轮询」＋「ccm 做到必须走 daemon」⇒ **整条删掉，没留轮询退路**。\
             接班的是 daemon 的 `control/identity_tag.rs`（由 `sessions/` 的 pidfile inotify \
             驱动，零新增节拍）。所以本文件今天**不再是两类**，是一类。\
             钉住「它真的没了」的是本模块的 `the_identity_poller_is_gone_for_good`。",
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
        //
        // ★〔K-R14 09-01〕上一版是 `strip_comment_lines(&ccm).len() * 2 > ccm.len()`。
        //   那条线量的**不是剥法坏没坏**，是「ccm 里以 `//` / `*` / `/*` 开头的行占多少」——
        //   一个随写作漂移的量。现打（量于 `018b134`）：抹掉 545 字节 / 9 行，余量 90209 字节。
        //   它今天离翻红很远（不像下面那条 `* 4` 只剩 97 字节），但**形状同族** ⇒ 一起换掉，
        //   免得下一轮谁把 ccm 的形态改了，同一个病在这里再演一遍。
        // ⚠ 诚实边界：这个剥法是 Rust/TS 那套（`//` / `*` / `/*`），对 shell 的 `#` 一个字
        //   都不剥 ⇒ 这里**不许**断言「它剥掉了东西」（那不是它对 shell 语料的契约）。
        //   今天被抹掉的 9 行全是 shell 的 `*)` case 分支 —— 那是 `scan()` 在 ccm 上的
        //   真实行为，是另一件事，不是本条该钉的性质。
        let ccm = fs::read_to_string(root.join("shared/ccm")).unwrap_or_default();
        assert!(
            !guard_core::strip_comment_lines(&ccm).trim().is_empty(),
            "剥注释后 shared/ccm 一个非空白字节都不剩（原文 {} 字节）。两个可能的真因：\n\
             ① 剥法太狠（`strip_comment_lines` 坏了）⇒ `scan()` 此刻在扫空字符串；\n\
             ② `shared/ccm` 整份都成了以 `//` / `*` / `/*` 开头的行 —— 对一个 shell 脚本\n\
                来说那基本不可能，但真发生了就说明文件换了个东西，本条要重新裁定。",
            ccm.len()
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
        // P2s（补审 A4）：**有退出条件的自链**，不是 data-poll。
        ("src/settings/daemon-section.ts", "setTimeout", 1,
         "起/停一台机之后轮询状态到落定。**上限 30 次 × 100ms**、由用户动作触发、\
          落定即停 ⇒ 有退出条件的自链，不进 `REGISTERED`。\
          ⚠ 不轮询的后果很具体：两个命令都是「发出去就返回」（`daemon_start` 只 spawn 了监护线程、\
          `daemon_stop` 只发 SIGKILL），命令一返回就画等于**每次操作后都显示操作前的状态**。"),
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

    /// ★ **身份 poller 不许回来** —— `U-NP④`（2026-08-14）之后 `shared/ccm` 里
    /// **一条与会话同寿的循环都不许有**。
    ///
    /// # 这条判据取代了谁
    ///
    /// 它的**上一版**叫 `the_per_second_identity_poller_spawns_nothing_per_tick`，
    /// 钉的是「那条每秒循环每次醒来不许起外部进程」（audit-0805 F14 第六刀：管道形态
    /// 每 tick 7 次 clone/execve，纯 builtin 0 次）。那一版的头注**自己写着**：
    /// 「本条钉的是**每次醒来的代价**，不是醒不醒；『别每秒醒』要 inotify，
    /// 得动 ccm 的进程模型 —— 如实登记为未做」。
    ///
    /// 用户 08-14 裁定「**可以动ccm. 不要轮询**」＋「**ccm做到必须走daemon**」
    /// ⇒ 那件「未做」被做掉了，做法不是给 ccm 上 inotify（破「纯 POSIX shell、零第三方」，
    /// 而 ccm 要经 `include_str!` 部署到任意远端），而是**把通道 B 整条搬去 daemon**
    ///（`remote-daemon-proto/src/control/identity_tag.rs`，由它已有的 pidfile inotify 驱动）。
    /// ⇒ 「醒来的代价」这个量**不再存在**，钉它的判据必须换成钉「它真的没了」。
    ///
    /// # 为什么不是零命中的空守卫
    ///
    /// 它有一个真实的反向锚点：同一个文件里**仍然有**一处 `sleep`（预信任等待，
    /// `REGISTERED` 里登记为 `wait-for-condition`）。所以「ccm 里没有轮询」这句话
    /// 是**假的**、也不该被钉；该钉的是**那一种形态**：与会话同寿的循环。
    /// 下面第二段断言正是靠它证明抽取器没有空转。
    #[test]
    fn the_identity_poller_is_gone_for_good() {
        let raw = fs::read_to_string(repo_root().join("shared/ccm"))
            .expect("shared/ccm 读不到 —— 路径变了就把这条一起改");
        // 剥 **shell** 整行注释：用共享原语 `strip_hash_comment_lines`，**不自己写第二份**
        // （`structural_scan::every_comment_stripping_transformer_is_registered` 当场逮过我
        // 一次：本条第一版内联了一个同款剥法）。⚠ 别拿 `strip_comment_lines` 代替 ——
        // 那个认的是 `//` / `*` / `/*`（Rust/TS 那套），对 `#` 一个字都不剥。
        //
        // ⚠〔K-R14 09-01 订正〕这里原文写着「要剥是因为本文件的散文里逐字写着下面那些
        //   形态（在解释它们为什么没了）」—— **那句话今天是假的**。现打（量于 `018b134`）：
        //   `shared/ccm` **全文含注释**里 `kill -0` 命中 0 次、`_ccm_sid_from_file` 命中 0 次。
        //   ⇒ 剥不剥都不改变下面任何一条的判定：实测把剥法换成「一个字都不剥」的 no-op，
        //   整条测试仍**全绿**。留着剥法是**防**那几句解释哪天写回来，不是它今天在起作用。
        //   （这条订正本身就是本仓那句「世界变了、文本没动」的活体。）
        let prod = guard_core::strip_hash_comment_lines(&raw);
        // ★ 反空真自检 —— 它钉的是**剥法本身的两个失效方向**，**不是** ccm 的注释占比。
        //
        //   〔K-R14〕上一版是一行 `prod.len() * 4 > raw.len()`，报文逐字
        //   「剥注释后只剩 {} / {} 字节 —— 剥法坏了，本条在空转」。三个毛病：
        //   ① **它量的是注释占比，而那是一个会随写作漂移的量**。现打（`018b134`）：
        //      `prod` 22849 字节 · `raw` 91299 字节 ⇒ 注释占 74.97%，离 3/4 那条线
        //      只剩 **97 字节**。往 ccm 的注释里加一行 32 个汉字就翻红 ——
        //      而 ccm 恰恰是一个散文很多、经常有人回来补说明的文件。
        //   ② **翻红时它说的是假话**。实证（在**副本**上做，没动 ccm 本身）：
        //      加 96 字节注释仍绿、加 97 字节翻红，而两趟的 `prod` **逐字节相同**
        //      （都是 22849）—— 剥法一个字节都没变，报文却说「剥法坏了」。
        //      踩到的人会照着报文去查剥法，而剥法好好的。
        //   ③ 它还**守不住它自称守的东西**：剥法退化成 no-op 时 `raw * 4 > raw` 恒真
        //      ⇒ 本条**绿**（实测）。它只在「剥法把文件吃光」那一侧会响，
        //      而那一侧下面的**反向锚点 ②** 也会响，且报的是真因 ⇒ 它的独占覆盖面是 0。
        //   ⇒ 换成下面两条：一条一个失效方向，**都不随注释占比漂移**。
        //      余量（量于 `018b134`，口径 = Rust `str::lines()`）：ccm 共 1258 行，
        //      其中 `#` 整行注释 723 行、剥完剩 535 行（非空白的 486 行）。
        //      ⇒ 第一条要**那 723 行注释一行不剩**才假红；第二条要**那 486 行非空白内容
        //      一行不剩**才假红。两个都不是「有人回来补一段说明」够得着的量。
        //
        // ★★ 收工自查逮到的（本件治的正是这个病，别在治它的代码里复发一遍）：
        //    这两条各有**两个**可能的真因，报文**必须把两个都说出来**。
        //    只写「剥法坏了」就又是一次「因果整个说错」—— 那是本件的原病。
        assert!(
            prod.lines().count() < raw.lines().count(),
            "剥法一行都没剥掉（进 {} 行、出 {} 行）。两个可能的真因，**别只查第一个**：\n\
             ① `strip_hash_comment_lines` 没生效 ⇒ 下面几条会被 ccm 的散文喂饱；\n\
             ② `shared/ccm` 里一行 `#` 整行注释都没有了（`018b134` 时有 723 行）\n\
                ⇒ 那剥这一步已经没有意义，该连着下面几条一起重新裁定。",
            raw.lines().count(),
            prod.lines().count()
        );
        assert!(
            !prod.trim().is_empty(),
            "剥完 shared/ccm 一个非空白字节都不剩（原文 {} 字节）。两个可能的真因：\n\
             ① 剥法把整份文件吃掉了 ⇒ 下面几条此刻在扫空字符串，会零命中地绿；\n\
             ② `shared/ccm` 整份都成了 `#` 注释与空行（`018b134` 时非空白正文有 486 行）\n\
                ⇒ 那这个文件已经不是原来那个东西，本组判据要重新裁定。",
            raw.len()
        );
        // ★ 与会话同寿的循环，唯一写得出的形态就是「盯着一个 PID 活不活」。
        for shape in ["while kill -0", "until kill -0", "while ! kill -0"] {
            assert!(
                !prod.contains(shape),
                "`shared/ccm` 生产段里又出现了 `{shape}` —— 那是一条**与会话同寿**的循环。\n\
                 `U-NP④` 把身份通道 B 整条搬去了 daemon（`control/identity_tag.rs`），\n\
                 用户裁定逐字：「不要轮询」「ccm 做到必须走 daemon」。\n\
                 ⚠ 别把它当成「加个 sleep 兜一下更稳」——那正是本件要根除的东西：\n\
                 每会话一条、跑在**远端**机器上、与会话同寿。\n\
                 真需要一个新的等待，先回答「它的内核事件源是什么、为什么 daemon 接不了」。"
            );
        }
        // 反向锚点 ①：那条 poller 用的解析器也一起没了（留着就是死代码）。
        assert!(
            !prod.contains("_ccm_sid_from_file"),
            "`_ccm_sid_from_file` 还在 —— 它只服务那条已删的 poller，留着就是死代码"
        );
        // 反向锚点 ②：**抽取器没有空转** —— 同文件里那处登记在案的 `sleep` 必须还看得见。
        assert!(
            prod.contains("sleep 0.5"),
            "连预信任那处 `sleep 0.5` 都扫不到 —— 剥法或路径坏了，上面那几条是零命中地绿"
        );
    }

    /// ★ **没有 daemon 就必须响亮地失败** —— `U-NP④` 的第二条硬要求。
    ///
    /// # 为什么这条要单独钉
    ///
    /// 删掉 poller 之后，「这台机器没装 daemon」的后果从「少一个锦上添花」变成
    /// 「用户开了会话、monitor 绑不上、点 ↗ 弹『未绑定窗口』**而没人知道为什么**」。
    /// 静默降级在这里是最坏的失败模式（本仓反复吃过这个亏：假成功比失败更贵）。
    ///
    /// ⚠ **本条只钉形状**（三件事在源码里存在且顺序对），行为那半由
    /// `e2e/ccm-cli.test.sh` 的「daemon 前置检查」一节真跑一遍 ccm 去验
    ///（rc=2 · 不 exec launcher · 逃生口放行）。**两条都要**：形状挡改写，行为挡「写了但不生效」。
    #[test]
    fn ccm_fails_loudly_when_no_daemon_can_be_found() {
        let raw = fs::read_to_string(repo_root().join("shared/ccm")).expect("读不到 shared/ccm");
        let prod = guard_core::strip_hash_comment_lines(&raw);
        // ① 身份那一段真的去查了 daemon（共用同一份查找配方，不是第二套规则）。
        let block = prod
            .split("agent_has_identity \"$agent\"")
            .nth(1)
            .expect("找不到身份分支 —— 它被改写或搬走了，本条会零命中地绿");
        assert!(
            block.contains("eval \"$DAEMON_BIN_RECIPE\""),
            "身份分支里没有 `eval \"$DAEMON_BIN_RECIPE\"` —— 前置检查没了，或者它另起了\n\
             第二套查找规则（那正是 P4e 花力气收成一份的东西）"
        );
        // ② 找不到就 `die`（exit≠0），不是打一行日志继续。
        let after = block
            .split("eval \"$DAEMON_BIN_RECIPE\"")
            .nth(1)
            .expect("刚断言过它在");
        assert!(
            after.contains("die \"找不到 daemon"),
            "找不到 daemon 之后没有 `die` —— 静默降级正是本条要挡的。\n\
             实得（前 400 字节）：{}",
            &after[..after.len().min(400)]
        );
        // ③ 逃生口存在**且会说话**：`CCM_NO_DAEMON=1` 放行，但往 stderr 说一句。
        assert!(
            after.contains("CCM_NO_DAEMON") && after.contains(">&2"),
            "逃生口不见了、或它闷声放行 —— 明示放弃身份也要说一句"
        );
        // ④ **头注第 4 条必须跟着改**（用户 08-14 的硬要求之一）。
        //   它原文逐字写着「身份注册用**运行时自适应**而非编译期门控：装了就用，
        //   **没装逐字节退化成裸 launcher**」——走档 2 之后那句话是**假的**（没装 = 响亮失败）。
        //   留着它就是本仓反复吃亏的「停滞式腐坏」：世界变了、文本没动，照它做的人会走错。
        //   ⚠ 这是零命中守卫，但**有反向锚点**（下一条断言新话在），不会零命中地绿。
        //
        // ★ 针为什么一个是 `const` + `contains`、一个是 `find_pinned`（两种写法不是随手挑的）：
        //   `needle_anchor_registry` 的递减棘轮治的是「**匹配单位比事实小** ⇒ 把事实撑大的改动
        //   会溜过去而判据照样绿」。那条推理只对**正向**断言成立 ——
        //   对**「这句话不许存在」**的断言，子串是**强的那一侧**（子串不在 ⇒ 整句必不在），
        //   撑大事实反而更容易被逮到。⇒ 负向这条用具名 `const`（针成了一条可复核的事实，
        //   不是行内字面量），正向那条用 `find_pinned`（恰好一处 + 两侧有边界，严格强于 `contains`）。
        const BANNED_OLD_CLAIM: &str = "逐字节退化成裸 launcher";
        assert!(
            !raw.contains(BANNED_OLD_CLAIM),
            "`shared/ccm` 头注里那句「（没装就）{BANNED_OLD_CLAIM}」回来了 —— 它今天是假的：\n\
             身份只由 daemon 打，没有 daemon 就 `die`。散文与代码说的必须是同一件事。\n\
             ⚠ 想在注释里**引用**那句旧话也会打红本条（本护栏连注释一起扫，fail-closed）——\n\
             处置是改措辞，别把护栏改成剥注释（同 `tmux_hook.rs` 头注那条纪律）。"
        );
        guard_core::find_pinned(&raw, "身份（`@ccm_sid`）必须走 daemon").unwrap_or_else(|e| {
            panic!(
                "头注第 4 条不再声明「身份必须走 daemon」（{e}）—— 要么被改回去了，要么措辞漂了。\n\
                 上一条（禁旧话）是零命中守卫，靠本条当反向锚点才不会零命中地绿。"
            )
        });
        // ★ **顺序**也是判据：前置检查必须排在任何 `tmux` 调用之前，
        //   否则「没有 daemon」这条路上还会去碰 tmux（e2e 正是在没有 tmux 的 PATH 下跑的）。
        let die_at = block.find("die \"找不到 daemon").expect("刚断言过它在");
        let tmux_at = block.find("tmux ").unwrap_or(usize::MAX);
        assert!(
            die_at < tmux_at,
            "前置检查排在了 tmux 调用之后 —— 失败路径上会先去碰 tmux。\n\
             这个顺序不是洁癖：`e2e` 那几条正是靠「PATH 里根本没有 tmux」来证明它没被碰的。"
        );
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
