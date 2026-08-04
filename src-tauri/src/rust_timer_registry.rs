//! F09：**monitor 的 Rust 侧周期唤醒清账** —— 补上 `polling_registry` 明确留下的那半。
//!
//! # 为什么补这一半
//!
//! daemon 侧的 `no_timer_guard`（§41）是**零容忍**的。它的头注写着范围：
//! 「**只钉 daemon crate。** monitor 侧另有自己的轮询纪律…**要钉那半得单独论证**。」
//! 而 monitor 侧的 `polling_registry` 只覆盖了 **TS 与 `shared/ccm`**，它自己的头注逐字写着：
//! 「**monitor 的 Rust 侧刻意不在范围内**…逐条论证是另一件事。**如实登记为未做，不假装覆盖了。**」
//!
//! ⇒ 那「另一件事」就是本模块。F02 摸底时那条线索指向这里，F09 把它做掉。
//!
//! # 论证：为什么是**登记表**而不是禁令
//!
//! 实测（F09 摸底）monitor Rust 生产段的 43 处 `sleep`/`Duration::from_*` 里：
//!
//! - **`time::interval` 零处** —— 没有一个 tokio 节拍器；
//! - **23 处 `Duration::from_*` 是 timeout / debounce / 退避上限** ——
//!   那是「等待的**上界**」，不是「自己醒过来」。把它们混进禁令就是 daemon 那条护栏
//!   头注预言的**噪音**；
//! - **真正要清的是 `sleep` 那一族** —— 手工 grep 数出 8 处，
//!   而本表首跑（正确剥段后）数出 **13 处**（见下）。⚠ **以机器那个数为准。**
//!
//! ⇒ 禁令会误伤 23 处正当上界；登记表能把那 13 处逐个说清。**分类同 `polling_registry`**
//! （同一套词汇，别造第二套）：
//!
//! | 类别 | 含义 | 要求 |
//! |---|---|---|
//! | `ticker` | **真节拍器** —— 无限循环 + 周期唤醒，没有终止条件 | **必须写明事件源在哪 + 谁退役它** |
//! | `wait-for-condition` | 等一个一次性条件，**有次数/时间上限** | 说清等什么、上限是多少 |
//! | `throttle` | 分块/错开，**有明确的元素上界** | 说清上界从哪来 |
//! | `startup-delay` | 一次性启动延时 | 说清为什么要让路 |
//!
//! # ★ 它一上岗就抓到**两个**真节拍器，而且我摸底时都数漏了
//!
//! 1. `bind.rs::run_heartbeat` = `loop { sleep(10s); cleanup_dead(); }` —— 无限、周期、无上限。
//! 2. ★★ `ssh_source.rs` 的 **daemonless 数据轮询**（`DAEMONLESS_POLL_INTERVAL = 2s`）——
//!    **它与定框 C7（没有 daemonless）和 C8（不许轮询）直接冲突**，而且是本工作区的正题。
//!
//! **两个都此前完全没有被任何账本记过**：`polling_registry` 按设计不管 Rust 侧，
//! `no_timer_guard` 只管 daemon crate ⇒ 它们正落在「两个护栏各自划了范围、中间那块没人管」里。
//!
//! ⚠ 而且**我自己摸底时数漏了**：手工 grep 数出 8 处 `sleep`、`ssh_source` 只数到 1 处；
//! 本表首跑用 `guard_core::production_code` 正确剥段后数出 **13 处**、`ssh_source` **4 处**、
//! 还多出整个 `lib.rs` 的 3 处。**人数出来的和机器数出来的不一样** ——
//! 与 `quote_singleton_guard` 那次（我数四份、它数五份）完全同形。
//!
//! # 它查什么、查不了什么
//!
//! 查 monitor Rust **生产段**里的 `thread::sleep` / `tokio::time::sleep` / `time::interval`，
//! 逐处要求登记。
//!
//! ⚠ **查不了「不用 sleep 的忙等」**（`loop { if cond { break } }` 纯自旋）——
//! 那种形态在语法上与正常循环无法区分。**比没有强，别读成证明。**
//! ⚠ **也不查 `Duration::from_*` 本身**：实测 23 处里全是上界，查它只会得到噪音
//! （这是本模块选登记表而不选禁令的同一条理由）。

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// 周期唤醒的**登记表**：`(相对 src-tauri 的路径, 类别, 处数, 为什么 + 谁退役它)`。
    ///
    /// 多一处没登记的 ⇒ 下面那条红。**登记表不是豁免清单**，
    /// 是「这些我看过、而且知道它归谁」的账。
    const REGISTERED: &[(&str, &str, usize, &str)] = &[
        (
            "src/bind.rs",
            "ticker",
            1,
            "★ **本表一上岗就抓到的那个真节拍器**：`run_heartbeat` = \
             `loop { sleep(10s); cleanup_dead() }` —— 无限、周期、无上限。\
             它清的是「死 pid 的 HWND 绑定」。**事件源存在但没用**：pid 死亡本可由内核事件\
             （Windows job object / daemon 侧那套 pidfd）推回来，今天是靠 10s 扫一遍。\
             ⚠ **退役未排期** —— 它属 Windows HWND 绑定那一族，不在本工作区的五项范围内。\
             如实记未排期，**不编一个假 owner 让它看起来有人管**。",
        ),
        (
            "src/bind.rs",
            "wait-for-condition",
            2,
            "两处等窗口：`find_window_for_marker` 的 12×50ms（≤600ms，等 PowerShell 设标题传播）· \
             `try_bind_with_retry` 的 `for _ in 0..attempts`。**都有次数上限**，\
             不是节拍器。上限本身待真机实测调（那条 ⚠ 已在 `bind.rs` 头注里）。",
        ),
        (
            "src/search.rs",
            "startup-delay",
            1,
            "`build_blocking` 起头让路一次：避开首屏 replay 的磁盘/CPU 争用。\
             索引不在关键路径，晚几秒就绪没关系（UI 那之前显示「索引中」）。**一次性**。",
        ),
        (
            "src/port_forward.rs",
            "wait-for-condition",
            1,
            "`accept()` 拿到瞬时错误（ECONNABORTED / EMFILE）时 100ms backoff 再试 —— \
             **避免忙等**用的。本地已 bound 的 listener 没有「永久失败」态，故不 break。\
             它等的条件是「下一个连接」，由 `accept()` 本身阻塞驱动，sleep 只在错误分支。",
        ),
        (
            "src/ssh_source.rs",
            "ticker",
            1,
            "★★ **本表抓到的第二个真节拍器，而且是关键的那个**：\
             `DAEMONLESS_POLL_INTERVAL = 2s` 的 `loop { …; sleep(2s) }` —— \
             **daemonless 回落路径的数据轮询**（没有 daemon 时靠自己每 2s 读一遍）。\
             ⚠ 它与**定框 C7 直接冲突**（「没有 daemonless —— 使用软件就要有后端」，\
             回落路径是**过渡期**的、不是永久形态），也与 **C8**（不许轮询）冲突。\
             **事件源**：daemon 在场时那条路本来就是事件驱动的（帧）—— 这 2s 补的正是\
             「那台主机没有 daemon」那个过渡态。\
             ⚠⚠ **F10 摸底订正了 F09 在这里写的退役归属**（原写「归 F05a+F05b，F10 收尾」，\
             那是**错的**）：实测 `cfg.daemonless` 是 **每台远端主机的一个用户配置开关**\
             （`remote-config.ts` 的 `daemonless: boolean`，默认 false —— 用户主动勾「这台不装 daemon」），\
             与「本机后端进程」和「本机读面」**都无关**。\
             ⇒ 真正的退役条件是「**远端自动部署可靠到可以删掉这个开关**」\
             （`embedded_daemons` + SFTP 自动部署已存在），而那是一个**产品决策**\
             （有些主机可能装不上 daemon）。**今天无人认领，如实记未排期。**\
             ⚠ 记这一条的教训：F09 自己那条判据要求 `ticker` 必须写 owner，\
             而我写了一个**似是而非**的 —— 正是同一条登记里写着「不编假 owner」的反面。\
             ⚠ **它此前从未被任何账本记过** —— `polling_registry` 按设计不管 Rust 侧，\
             `no_timer_guard` 只管 daemon crate。两个护栏各自划了范围，它就落在中间那块。",
        ),
        (
            "src/ssh_source.rs",
            "throttle",
            1,
            "`RACE_STAGGER * i` —— 多端点竞速时按序号错开发起，避免同时打爆。\
             **上界是端点个数**，不是周期。",
        ),
        (
            "src/ssh_source.rs",
            "wait-for-condition",
            2,
            "两处：① 快照读失败时 `if attempt == 1 { sleep(1s) }` —— **只重试一次**；\
             ② 重连退避 `sleep(backoff)`，序列 2→4→8→16→**30 上限**。\
             ⚠ ② 的外层重连循环确实无限，但**它等的是「下次重连时机」，不是节拍** —— \
             连上之后由 `stream_loop` 阻塞驱动；断线才回到这里。上限 30s 是明写的常量。",
        ),
        (
            "src/lib.rs",
            "wait-for-condition",
            3,
            "三处都有终止条件：① resize 稳定检测 `loop { sleep(60ms); if now == last { break } }`；\
             ② `remote-bind-scan` 的 `for _ in 0..15`（每 ~0.6s、最多 ~9s，命中即停）；\
             ③ 等 watcher 首扫完成 `loop { …; if elapsed > WAIT_TIMEOUT { break }; sleep(10ms) }`\
             （10s 上限，超时就带部分历史 replay）。**三处都不是节拍器。**",
        ),
        (
            "src/event_replay.rs",
            "throttle",
            2,
            "两处 `CHUNK_PAUSE_MS`：分块 emit 之间让 UI 喘一口。\
             **上界是 `chunk_total`**（`if idx + 1 < chunk_total` 才 sleep），最后一块不停。",
        ),
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
        let mut stack = vec![src.clone()];
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
                        "src/{}",
                        p.strip_prefix(&src)
                            .unwrap_or(&p)
                            .to_string_lossy()
                            .replace('\\', "/")
                    );
                    out.push((rel, fs::read_to_string(&p).unwrap_or_default()));
                }
            }
        }
        out.sort();
        out
    }

    /// 一处「周期唤醒」的源码形态。**刻意不含 `Duration::from_`** —— 见模块头注。
    fn wake_hits(prod: &str) -> usize {
        // 判据串运行时拼，免得命中本文件自己的说明。
        let pats = [
            format!("thread::{}", "sleep"),
            format!("time::{}", "sleep"),
            format!("time::{}", "interval"),
        ];
        prod.lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("//") && pats.iter().any(|p| l.contains(p.as_str()))
            })
            .count()
    }

    /// ★ 抽取器自检：扫不到文件 / 剥太狠时，下面几条会零命中地绿。
    #[test]
    fn the_scan_actually_reads_the_monitor_rust_tree() {
        let files = rust_files();
        assert!(
            files.len() >= 60,
            "只扫到 {} 个 .rs —— 遍历器坏了（实测应约 90）",
            files.len()
        );
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
        let mut want: Vec<(String, usize)> = Vec::new();
        for (f, _, n, _) in REGISTERED {
            match want.iter_mut().find(|(k, _)| k == f) {
                Some((_, c)) => *c += n,
                None => want.push(((*f).to_string(), *n)),
            }
        }
        want.sort();
        let mut got: Vec<(String, usize)> = rust_files()
            .into_iter()
            .filter_map(|(rel, raw)| {
                let n = wake_hits(&production(&raw));
                (n > 0).then_some((rel, n))
            })
            .collect();
        got.sort();
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
        let mut tickers = 0;
        for (f, kind, _, why) in REGISTERED {
            assert!(
                matches!(
                    *kind,
                    "ticker" | "wait-for-condition" | "throttle" | "startup-delay"
                ),
                "{f} 的类别 `{kind}` 不在四类里 —— 新类别要先在模块头注那张表里定义"
            );
            if *kind == "ticker" {
                tickers += 1;
                assert!(why.contains("事件源"), "{f} 记成 ticker 却没说事件源在哪");
                assert!(
                    why.contains("退役"),
                    "{f} 记成 ticker 却没说谁退役它（没人认领也要写「未排期」，别留空）"
                );
            }
        }
        // 抽取器自检：一条 ticker 都没认出来时上面的断言全空转。
        assert_eq!(
            tickers, 2,
            "登记表里的 ticker 条数变了（实测 2 条：`bind.rs::run_heartbeat` 与 \
             `ssh_source.rs` 的 daemonless 2s 轮询）。\n\
             多一条 ⇒ 新增了真节拍器，必须单独论证；少一条 ⇒ 退役了，把账拧下来。"
        );
    }

    /// ★ 那个真节拍器的**形态**没变：还是「无限循环 + 周期 sleep」。
    ///
    /// 它一旦被改成事件驱动（或加上终止条件），本条会红 —— **那是好事**，
    /// 提醒把登记表那条从 `ticker` 降级到别的类。
    #[test]
    fn the_one_real_ticker_still_looks_like_a_ticker() {
        let src = production(include_str!("bind.rs"));
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
}
