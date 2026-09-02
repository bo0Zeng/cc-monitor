//! P6（zero-poll-liveness）：**零定时器护栏**。
//!
//! # `K-G6` `KG62`：性质与人群，两行逐字（**各自只许有一句**，`readonly_guard::g6_scope_pins` 钉着）
//!
//! - **它守的性质是**：daemon **自己的生产代码**里不出现会让线程 / 任务自己醒来的构件 —— 判活由内核事件驱动（pidfile inotify + pidfd · socket 目录 inotify · tmux hook → SIGUSR1），不靠节拍反复问。
//! - **它扫的人群是**：本 crate `src/` 递归全部 `.rs`（`SKIPPED_BY_NAME` 跳过自身）剥掉测试段之后的**源码文本**；依赖 crate 的源码、以及被起进程的行为，**都不在里面**。
//!
//! ⚠ **这两行今天对得上**（三道护栏里唯一一道），而它买到的东西**比字面小一格** —— 如实登记：
//! 人群按**源码文本**画，性质说的是**这个 crate 的代码**；
//! 而「这个**进程**里有没有东西在自己醒来」是另一件事：今天就有一条依赖 crate 的线程在做带超时的等待，
//! 由生产段亲手拉起，而本护栏一个字都看不见。
//! ⇒ **它今天真能拦住的形状全表在 [`g6_reach`]，那条反例也在那里。**
//!
//! 〔`K-G6` 订正〕本头注原先第一句把性质写成了**进程级**的全称，而判据只兑现 crate 级的那句；
//! 收窄措辞的同时由 `readonly_guard::g6_scope_pins` 立了一条**反向棘轮**，钉住那句全称不许回来。
//!
//! # 它守的是什么性质
//!
//! P0-P5 把三类判活逐个换成了事件源：pidfile inotify + pidfd（进程死）· socket 目录
//! inotify（server 生死）· tmux hook → SIGUSR1（会话开关）。P5 删掉最后一个 8s ticker
//! 之后，**这个 crate 里已经没有任何东西会「自己醒过来」**。
//!
//! 没有护栏的话，这条性质会以最不起眼的方式退化：某天有人为了「稳一点」加一个
//! `thread::sleep(2s)` 的兜底循环，测试全绿、行为看起来更稳，而整轮工作的收益悄悄没了。
//!
//! # 判据落在「周期性唤醒」，不是落在「出现过 `Duration`」
//!
//! 这个区别是本护栏设计上最要紧的一点。`Duration` 有大量**非定时器**的正当用途
//! （超时上限、去抖窗口、时间戳算术），把它们一并禁掉会逼着后来人绕开护栏 —— 那时护栏
//! 就从「防退化」变成了「防不了但很吵」。
//!
//! 故：**禁的是会让线程/任务自己醒来的那些构件**（见 `PERIODIC_WAKE_PATTERNS`），
//! 而**已知的非定时器 `Duration` 用途逐条登记**（见 `REGISTERED_DURATION_USES`）——
//! 登记表不是豁免清单，它是「这些我看过、确认不是定时器」的账，**多一条就要红一次**、
//! 逼人把新的那处也想清楚。
//!
//! # 范围
//!
//! **只钉 daemon crate（`remote-daemon-proto/src/*.rs`）的生产段。** monitor 侧另有自己的
//! 轮询纪律（那边有 UI 刷新、重连退避等**正当**周期行为），把本护栏扩过去会立刻变成噪音
//! ⇒ 要钉那半得单独论证。**范围写清楚，别默认扩** —— 守卫范围必须等于它真正证明的性质。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空、零运行期开销、不改 daemon 行为。

#[cfg(test)]
mod f09_external_beat {
    //! F09（定框 C12 的那条 ⚠ 点名的活）：**「周期跑一次外部命令」也算自己醒过来。**
    //!
    //! C12 的 ⚠ 逐字写着：「护栏今天只钉『构件的源码形态』，**没钉住『周期跑一次外部命令』**
    //! —— 那是 F09 的活。」那个形态是：不用 `sleep`/`interval`，而是**产出一段带循环的
    //! shell 串**交给别人执行，靠外部进程提供节拍 —— daemon 侧的护栏一个字都看不见。
    //!
    //! # 实测：daemon 侧**今天零实例**
    //!
    //! F09 摸底逐个查过 daemon 的 shell 串产出点，**没有一处含循环关键字**。
    //! 所以本条今天是**预防性**的零命中守卫。
    //!
    //! ⚠ 那它会不会是个「谁都没写过」的空守卫？**不会** —— 它有一个真实的反向锚点：
    //! C14 登记的那个例外（预信任的「等信任框」，本质就是轮询、以 shell 串形态产出）
    //! **确实存在，但它住 `shared/ccm`，不在 daemon**。
    //! 也就是说「这种形态真实存在于本仓，只是刻意不在 daemon 侧」——
    //! 本条钉的正是那条边界。

    /// 循环关键字：shell 里提供节拍的三种写法。
    fn loop_words() -> Vec<String> {
        // 运行时拼，免得命中本模块自己的说明文字。
        [("whi", "le"), ("unti", "l"), ("fo", "r ")]
            .iter()
            .map(|(a, b)| format!("{a}{b}"))
            .collect()
    }

    /// daemon 生产段里**产出给别人执行的字符串字面量**。
    fn shell_string_literals() -> Vec<(String, String)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = Vec::new();
        let mut stack = vec![dir.clone()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().and_then(|x| x.to_str()) != Some("rs") {
                    continue;
                }
                let rel = p
                    .strip_prefix(&dir)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                // 本模块自己的说明里逐字写着那三个关键字 —— 排除掉（同族坑记过四次）。
                if rel == "no_timer_guard.rs" {
                    continue;
                }
                let prod =
                    guard_core::production_code(&std::fs::read_to_string(&p).unwrap_or_default());
                for line in prod.lines() {
                    if line.contains('"')
                        && (line.contains("sh -c")
                            || line.contains("run-shell")
                            || line.contains("format!"))
                    {
                        out.push((rel.clone(), line.trim().to_string()));
                    }
                }
            }
        }
        out
    }

    /// ★ 抽取器自检：抽不到候选行时下面那条会零命中地绿。
    #[test]
    fn the_shell_string_scan_finds_candidates() {
        let n = shell_string_literals().len();
        assert!(
            n >= 3,
            "只抽到 {n} 行可能的 shell 串 —— 抽取器坏了，下面那条会空转变绿"
        );
    }

    /// ★ 正题：daemon 产出的 shell 串里不许有循环 —— 那是「靠外部节拍反复跑」的形态。
    #[test]
    fn no_daemon_produced_shell_string_carries_its_own_loop() {
        let words = loop_words();
        let mut bad = Vec::new();
        for (f, line) in shell_string_literals() {
            for w in &words {
                // 只看引号内的部分（`while let` 这类 Rust 语法不算）。
                if let Some(q) = line.find('"') {
                    if line[q..].contains(w.as_str()) {
                        bad.push(format!("  {f}: {line}"));
                        break;
                    }
                }
            }
        }
        assert!(
            bad.is_empty(),
            "daemon 产出的 shell 串里出现了循环关键字 —— 那是 C12 的 ⚠ 点名的\n\
             「**周期跑一次外部命令**」形态：不用 sleep/interval，而是让别人的 shell 提供节拍，\n\
             于是本 crate 的零定时器护栏一个字都看不见。\n\
             ⚠ C14 登记的那个例外（预信任「等信任框」）**住 `shared/ccm`，不在 daemon** ——\n\
             真要在 daemon 侧开这种口子，先回定框把 C12/C14 的边界重新裁定。\n{}",
            bad.join("\n")
        );
    }
}

#[cfg(test)]
mod tests {
    /// 会让线程/任务**自己醒过来**的构件。命中即违规。
    ///
    /// 逐条说明为什么它在表里：
    /// - `thread::sleep` / `tokio::time::sleep`：睡到点自己醒 —— 轮询的标准形态
    /// - `recv_timeout`：等不到就自己醒，等价于给循环装了节拍
    /// - `tokio::time::interval`：字面意义的节拍器
    /// - `Instant::now`：本 crate 里它只会用来做「距上次多久了」的节流判断
    ///   （真要打时间戳有 `SystemTime`，且 daemon 的帧不带时间戳）
    /// - `Duration::from_secs`：秒级 `Duration` 在这个 crate 里只可能是节流常量
    ///   （去抖窗口是毫秒级，超时上限也不该出现在 reader 路径上）
    ///
    /// **不在表里的**：`Duration` 本身、`Duration::from_millis`（见 `REGISTERED_DURATION_USES`）。
    /// 行里有没有 `name(` 这个**调用**（`name` 要是完整的词）。
    ///
    /// 与 monitor 侧 `rust_timer_registry::is_call_of` 同形 —— 两侧各一份是刻意的：
    /// 两个 crate 之间没有共享测试工具的通道（跨 crate 够不着 `guard_core` 的测试模块）。
    pub(super) fn is_call_of(line: &str, name: &str) -> bool {
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

    /// ★ 调用匹配器的**负向**自检〔audit-0805 08-06〕。
    ///
    /// ⚠ 加它是因为一次实测：把 `is_call_of` 的词边界整段删掉，**六条判据照样绿** ——
    /// 也就是说这个匹配器的收紧那一侧当时**没有任何断言在行使**。
    /// 词边界松掉的后果是误红（`do_sleep(3)` 被当成周期唤醒），
    /// 而误红最省事的消法是把这条判据删掉。
    /// ⇒ 本区第四次踩「负向断言写不出来就等于没有」，这次在当轮就补上并验了。
    #[test]
    fn the_call_matcher_does_not_fire_on_lookalikes() {
        assert!(is_call_of("    sleep(d).await;", "sleep"), "真调用没认出来");
        assert!(
            is_call_of("    tokio::time::sleep(d);", "sleep"),
            "带路径的调用也要认"
        );
        assert!(
            !is_call_of("    let x = do_sleep(3);", "sleep"),
            "`do_sleep(` 被当成了 `sleep(` —— 词边界没守住"
        );
        assert!(
            !is_call_of("    self.my_interval(2);", "interval"),
            "`my_interval(` 被当成了 `interval(` —— 词边界没守住"
        );
        assert!(
            !is_call_of("    let sleep_ms = 5;", "sleep"),
            "只是同名变量、后面不是 `(`，不该算调用"
        );
    }

    pub(super) fn periodic_wake_patterns() -> Vec<String> {
        // 判据**运行时拼**：直接写字面量的话，本文件自己就会被下面的扫描命中
        //（同类自指陷阱在 P4 连踩七次，其中一次让守卫变成了安慰剂）。
        vec![
            format!("thread::{}", "sleep"),
            format!("time::{}", "sleep"),
            format!("recv{}", "_timeout"),
            format!("time::{}", "interval"),
            format!("{}::now", "Instant"),
            format!("Duration::{}", "from_secs"),
        ]
    }

    /// **已登记的非定时器 `Duration` 用途**。
    ///
    /// 这不是豁免清单 —— 下面的断言要求生产段里的 `Duration::from_` 调用**恰好**等于
    /// 本表的条数。多出一处就红，逼人回答「这处是不是又把轮询请回来了」。
    ///
    /// # 〔`K-G6` `KG64`〕三元组扩成**五元组**
    ///
    /// `(文件名, 片段, why——为什么它不是定时器, 格——`§0a` 四情形表里的哪一格, unlock——什么条件满足之后这一条就能删)`
    ///
    /// 第四栏由 `crate::readonly_guard::g6_doctrine::is_cell` 做**枚举比对**（不是子串）。
    /// 三条今天**全部**落在 `收窄人群`：本护栏的性质是「不许有会自己醒来的构件」，
    /// 而按 `Duration::from_*` 画的人群会把**非定时器**的正当用途一并扫进来 ——
    /// 这张表就是把人群收窄回性质上的那一刀，**它是收紧，不是豁免**。
    pub(super) const REGISTERED_DURATION_USES: &[(&str, &str, &str, &str, &str)] = &[
        (
        "watcher.rs",
        "Duration::from_millis(DEBOUNCE_MS)",
        "notify-debouncer 的**事件合并窗口**：它不产生唤醒，只决定「同一批文件事件攒多久\
         再一起交付」。去掉它 inotify 照样推事件，只是更碎。不是定时器。",
        "收窄人群",
        "去抖窗口不再由本 crate 传参（换成事件源自带合并、或整条 watch 路径重做）的那天。\
         ⚠ 这一条的**邻居**是本护栏最硬的那个反例：拉起这个去抖器的依赖 crate 内部有一条\
         带超时的等待线程，形状逐字落在禁用表上而人群够不着 —— 见 `g6_reach`。",
    ),
        (
        "server.rs",
        "Duration::from_millis(30_000)",
        "中转**下游** socket 的 `SO_RCVTIMEO`/`SO_SNDTIMEO`（`relay/server.rs::DOWNSTREAM_DEADLINE`）：\
         它说的是「**这一次**阻塞的读/写最多等多久」——有字节就立刻返回，没字节就**报错**返回。\
         它**不让任何线程自己醒来**、不产生节拍、不驱动任何循环：期限一到那条连接就被结掉，\
         `pump` 与 `http1` 的读循环只对 `Interrupted` 重试、其余一律 `return Err`。\
         没有它，一条半开连接会把一条线程永久钉在 `read_head` 上（D3 §2.3 实测 64 条 ⇒ 线程 4→68）。不是定时器。",
        "收窄人群",
        "中转那半整个搬走、或 socket 期限改由内核/别处设定的那天。\
         ⚠ 这条登记理由里「`pump` 与 `http1` 的读循环只对 `Interrupted` 重试」那句\
         **`K-G6` 摸底没有重打**（登记在件文件 `§0b-7②`），要摘这一条之前先把它核实。",
    ),
        (
        "upstream.rs",
        "Duration::from_millis(600_000)",
        "中转**上游** socket 的 `SO_RCVTIMEO`/`SO_SNDTIMEO`（`relay/upstream.rs::UPSTREAM_DEADLINE`）：\
         性质同上一条（一次阻塞的上限，不是唤醒），值不同是因为这一跳等的是**模型在想** ——\
         SSE 长流上游几十秒不发字节是正常形态，所以它必须比下游那条宽得多。\
         同样不驱动任何循环：到点即结连接，没有任何一层重试。不是定时器。",
        "收窄人群",
        "同上一条：中转那半搬走、或期限改由别处设定的那天一起摘。\
         ⚠ 「没有任何一层重试」这句同样是**登记理由里的话**，没被独立重打过。",
    )];

    use crate::guard_support::production_code;

    /// 遍历 `src/` 下**全部**（含子目录）`.rs`，返回 `(相对路径, 生产段)`。
    ///
    /// # 为什么必须递归（2026-08-01，U-1）
    ///
    /// 原来是单层 `read_dir` + `extension != "rs"` 跳过 —— **目录没有扩展名，于是被整个跳过**。
    /// `readonly_guard::scan` 在 Phase G 已经改成递归并留了警示注释，**这条没跟**。
    ///
    /// 失效形态实测（unified-backend 计划自审）：一旦生产代码搬进
    /// `src/<子目录>/`，扫到的文件只剩顶层那几个 mod 声明 + 本护栏 + `wire.rs`，
    /// 而当时的地板 `files.len() >= 5` **照样满足** ⇒ 护栏一行业务代码都没扫、全绿。
    /// 那正是本仓在「守卫范围 ≠ 性质范围」上栽过的第四次。
    pub(super) fn daemon_sources() -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                // 跳过本护栏自身：它的模式表与说明文字必然含这些子串。
                let base = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if SKIPPED_BY_NAME.contains(&base) {
                    continue;
                }
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let src = std::fs::read_to_string(&path).expect("read rs file");
                out.push((rel, production_code(&src)));
            }
        }
        out.sort();
        out
    }

    /// 采集时**按文件名主动跳过**的文件。
    ///
    /// 单独成表，是为了让「数量相等」那条判据算得出跳过了几个 —— 原来写死 `files.len() + 1`，
    /// 那个 `1` 与下面 `daemon_sources()` 里的跳过逻辑隔空耦合（Phase E 审计建议）。
    /// U1b 若要再跳过一个（如 `control/` 的窄写护栏自身），只改这张表，判据自动跟上。
    const SKIPPED_BY_NAME: &[&str] = &["no_timer_guard.rs"];

    /// 扫到的**真代码总量**下限。
    ///
    /// # 为什么是字节数而不是文件数
    ///
    /// 「文件数 >= N」挡不住本护栏真正的失效形态：**代码搬进子目录、顶层只剩壳**。
    /// 那时文件数照样够，而扫到的是一堆 `mod x;` 声明。字节数直接度量「扫到的是不是真代码」，
    /// 且**对拆分免疫**（把一个文件拆成三个，总字节不变）—— 而本工作区接下来做的正是拆分。
    ///
    /// # 这里刻意**不记**当前实测值
    ///
    /// 这段注释踩过两次同一个坑：U1a 时写「119_454」被审计算出实际 121_131（**我抄错了**）；
    /// 订正成 121_131 之后，U2 建出 `platform/` + `common/` 当场又过期，
    /// 被 Phase D 审计第二次逮到 —— 而那一版注释自己第一句写的就是「**别手抄这个数**」。
    ///
    /// **教训不是「下次抄仔细点」**：注释里但凡出现一个会随代码变的数字，
    /// 光写「别手抄」挡不住任何人。要么给复测办法，要么根本别记。这里两条都做：
    ///
    /// **复测办法** —— 把下面的常量临时改成一个大数跑 `cargo test no_timer`，
    /// 失败信息里的「只扫到 N 字节」就是当前实测值。**那条断言恒打印实时值、不依赖本注释**，
    /// 所以真值永远在你手上。
    ///
    /// **要下调这个数之前先问：是真的删了那么多生产代码，还是扫描面又缩了？**
    ///
    /// ⚠ **它一条挡不住「单个文件被剥空/剥过头」** —— 最大的那个文件（`watcher.rs`，约占三成）
    /// 整个消失，总量仍会高于本地板、照样绿。所以字节地板必须与下面那条**数量相等**判据
    /// 配着用（Phase D 审计 I1 指出的缺口）。
    const MIN_SCANNED_CODE_BYTES: usize = 80_000;

    /// 登记表的键（裸文件名）与采集到的相对路径是否指同一个文件。
    ///
    /// # 为什么单独抽成函数
    ///
    /// U-1 Phase E 把匹配从「路径全等」放宽成「全等 **或** 以 `/文件名` 结尾」，为的是
    /// U2/U3 把文件搬进子目录时不误红。但 Phase D 审计指出：**那条 `ends_with` 分支
    /// 今天被执行了却恒为 false** —— 登记表只有 `watcher.rs` 一条，而它还在顶层，
    /// 命中永远由左边的全等短路给出。⇒ **「返回 true」那一侧零覆盖**，
    /// U3 搬 `watcher.rs` 进 `observe/` 的那一刻它才第一次真生效。
    ///
    /// 而它一旦写错，表现出来是「登记表里的 watcher.rs 已经不在生产代码里了，请清理登记」
    /// —— 一条**指向完全错误方向**的诊断（表没腐烂，只是文件搬了家），而 §4.1 红线又盯着
    /// 这张表不许乱动。抽出来钉三行，比到时候排查便宜得多。
    ///
    /// ⇒ **U3 实测兑现**：`watcher.rs` 搬进 `observe/` 的那一刻这个分支第一次真生效。
    /// 变异（退回全等）当场产出上面那句预言过的误导性诊断。
    ///
    /// # 为什么这里可以裸文件名，而 `readonly_guard` 的白名单不行
    ///
    /// U3 刚花整段论证「按裸文件名钉写盘白名单 = 给写盘能力开第二个洞」，本函数却正是那种匹配。
    /// **两者方向相反，风险不对称**：
    ///
    /// | | `readonly_guard` 白名单 | 本登记表 |
    /// |---|---|---|
    /// | 匹配命中的后果 | **放行** —— 该文件获得写盘豁免 | **认账** —— 承认「这处 `Duration` 我看过、不是定时器」 |
    /// | 误配一个同名文件 | 那个文件获得它不该有的写盘权限，**静默开洞** | 少红一次；而**另一半判据 `code.contains(snippet)` 仍要求那份代码里真有那段片段** |
    /// | 失败方向 | fail-**open** | fail-closed 偏保守 |
    ///
    /// 换句话说：白名单是「谁可以为所欲为」，登记表是「我确认过这一处」。
    /// 前者误配等于授权，后者误配还要再过一道内容检查。**所以这里不路径化不是双标，
    /// 是判据性质不同** —— 但这条区别必须写下来，否则下一个人只会看到两条相反的做法。
    fn matches_registered(scanned_rel: &str, registered_file: &str) -> bool {
        scanned_rel == registered_file || scanned_rel.ends_with(&format!("/{registered_file}"))
    }

    /// 独立走一遍目录树，只数 `.rs` 个数 —— 用来与 `daemon_sources()` 的产出做**数量相等**核对。
    ///
    /// 刻意与 `daemon_sources` 分开写：那边还要读文件、剥生产段、跳过自身，
    /// 这边只做「树上有几个 `.rs`」这一件事，两者对不上就说明采集环节漏了东西。
    fn count_rs_in_tree() -> usize {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut n = 0usize;
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    n += 1;
                }
            }
        }
        n
    }

    /// ★ 生产段不许有任何**周期性唤醒**构件。
    #[test]
    fn daemon_production_code_has_no_periodic_wakeups() {
        let files = daemon_sources();
        // 反向自检：**扫到的必须是真代码**。断言的是「命中数 == 0」+「扫到了东西」，
        // 而不是「命中数 < N」—— 阈值绝不能挂在被优化的那个量上（rust-ts-boundary 的教训）。
        //
        // U-1：这里从「文件数 >= 5」换成**字节数**，理由见 `MIN_SCANNED_CODE_BYTES`。
        let bytes: usize = files.iter().map(|(_, c)| c.len()).sum();
        assert!(
            bytes >= MIN_SCANNED_CODE_BYTES,
            "只扫到 {bytes} 字节生产代码（下限 {MIN_SCANNED_CODE_BYTES}），\
             护栏多半没在扫该扫的东西。扫到的文件：{:?}",
            files.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>()
        );
        // ★ 反向自检之二（Phase D 审计 I1 补）：**数量相等**。
        // 字节地板挡不住「某个文件整体掉出扫描面」（实测 `watcher.rs` 占 29%，它整个消失
        // 总量仍 84_948 ≥ 80_000、照样绿）。这条用一遍独立的目录树计数来兜死那一类：
        // 采集到的 + 主动跳过的（本护栏自身）必须等于树上的 `.rs` 总数，一个都不许漏。
        let tree = count_rs_in_tree();
        assert_eq!(
            files.len() + SKIPPED_BY_NAME.len(),
            tree,
            "扫到 {} 个 .rs + 主动跳过 {} 个（{SKIPPED_BY_NAME:?}）≠ 树上的 {tree} 个 —— 采集漏了文件。扫到的：{:?}",
            files.len(),
            SKIPPED_BY_NAME.len(),
            files.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>()
        );
        // 反向自检之三：`src/` 下**只要存在含 `.rs` 的子目录**，扫到的集合里就必须有带 `/` 的项。
        // 这条专门钉住「有人把遍历改回非递归」——那正是 U-1 修的那个 bug 的形状。
        // **要求子目录里真有 `.rs`**（Phase D 审计 S1）：否则一个空目录 / `snapshots/` /
        // `testdata/` 就会把它打成误红，那种守卫最后会被人删掉。
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let has_rs_subdir = std::fs::read_dir(&root)
            .expect("read src dir")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .any(|e| {
                let mut stack = vec![e.path()];
                while let Some(d) = stack.pop() {
                    let Ok(rd) = std::fs::read_dir(&d) else {
                        continue;
                    };
                    for x in rd.flatten() {
                        let p = x.path();
                        if p.is_dir() {
                            stack.push(p);
                        } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
                            return true;
                        }
                    }
                }
                false
            });
        if has_rs_subdir {
            assert!(
                files.iter().any(|(n, _)| n.contains('/')),
                "src/ 下有含 .rs 的子目录，但扫到的全是顶层文件 —— 遍历退化成非递归了：{:?}",
                files.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>()
            );
        }
        // ★〔audit-0805 08-06〕**再按「调用」扫一遍** —— 上面那批针全是路径拼法。
        //
        // 实测：往 `inbound.rs` 写
        //   `use tokio::time::{self as _t, sleep};`
        //   `async fn probe() { loop { sleep(Duration::new(5, 0)).await; } }`
        // ——一个货真价实的无限周期唤醒，**六条判据全绿**：
        // `time::sleep` 对不上（导入写成了花括号组）、`Duration::from_` 也对不上
        //（用的是 `Duration::new`）⇒ 两道防线**同时**被同一种改写绕过去。
        //
        // 补的是**调用形态**（名字要是完整的词、后面紧跟 `(`），与怎么导入无关。
        // ⚠ 补之前量过误红面：这八个名字在 daemon 生产段今天**全为 0 处**。
        for (name, code) in &files {
            for call in [
                "sleep",
                "interval",
                "interval_at",
                "recv_timeout",
                "park_timeout",
                "wait_timeout",
                "timeout",
                "tick",
            ] {
                if let Some(line) = code.lines().find(|l| {
                    let t = l.trim_start();
                    !t.starts_with("//") && is_call_of(l, call)
                }) {
                    panic!(
                        "零定时器护栏违规（P6，**按调用形态**逮到）：生产代码 {name} 里有 `{call}(` 调用：\n  {}\n\
                         daemon 的判活全部由内核事件驱动（inotify / pidfd / tmux hook）。\n\
                         ⚠ 换个 import 写法或换个 `Duration` 构造器**绕不过这一条** —— 它只看调用。",
                        line.trim()
                    );
                }
            }
        }
        for (name, code) in &files {
            for pat in periodic_wake_patterns() {
                assert!(
                    !code.contains(&pat),
                    "零定时器护栏违规（P6）：生产代码 {name} 含周期性唤醒构件 `{pat}`。\n\
                     daemon 的判活全部由内核事件驱动（inotify / pidfd / tmux hook→SIGUSR1）；\n\
                     加回定时器等于把 P0-P5 的收益悄悄退掉。确有必要请先改本护栏的登记表并说明理由。"
                );
            }
        }
    }

    /// ★ `Duration::from_*` 的每一处都必须在登记表里 —— **多一处就红**。
    ///
    /// 这条与上一条互补：上一条禁「会自己醒的构件」，这条防「用没被列名的方式
    /// 把节拍偷渡回来」（比如 `from_millis(8000)`）。
    #[test]
    fn every_duration_use_is_registered_as_non_timer() {
        let needle = format!("Duration::{}", "from_");
        let mut found: Vec<(String, usize)> = Vec::new();
        for (name, code) in daemon_sources() {
            let n = code.matches(&needle).count();
            if n > 0 {
                found.push((name, n));
            }
        }
        let total: usize = found.iter().map(|(_, n)| n).sum();
        assert_eq!(
            total,
            REGISTERED_DURATION_USES.len(),
            "生产段 `Duration::from_*` 有 {total} 处，登记表里只有 {} 条：{found:?}\n\
             新增的那处若确实不是定时器，把它加进 REGISTERED_DURATION_USES 并写明理由；\n\
             若它是节流常量 —— 那就是本护栏要拦的东西。",
            REGISTERED_DURATION_USES.len()
        );
        // 登记表里的每条都必须**还在**（删了代码却留着登记 = 表在腐烂）。
        //
        // ★ 匹配按**文件名**不按完整相对路径（Phase E 审计 R4）：`daemon_sources()` 现在返回
        // `observe/watcher.rs` 这种相对路径，而本表的键是裸文件名。若这里用全等，U2/U3 把
        // `watcher.rs` 搬进 `observe/` 的那一刻，本断言就会以「登记表在腐烂」红掉 ——
        // 那是一条**误导性诊断**：表没腐烂，只是文件搬了家。而 §4.1 红线又盯着这张表不许乱动，
        // 于是下一个人只能在「改红线表」和「改护栏」之间二选一。⇒ 现在就让纯搬家不触碰它。
        // （真删掉那处代码仍会红 —— `code.contains(snippet)` 那半边管这个。）
        for (file, snippet, ..) in REGISTERED_DURATION_USES {
            let hit = daemon_sources()
                .into_iter()
                .any(|(n, code)| matches_registered(&n, file) && code.contains(snippet));
            assert!(
                hit,
                "登记表里的 {file} / `{snippet}` 已经不在生产代码里了，请清理登记"
            );
        }
    }

    /// `matches_registered` 的真值表 —— 尤其是**今天还没被真跑过**的 `ends_with` 那一侧。
    #[test]
    fn registered_file_matching_survives_a_move_into_a_subdir() {
        // 今天走的分支：文件还在顶层，全等命中。
        assert!(matches_registered("watcher.rs", "watcher.rs"));
        // U3 之后才会走的分支：搬进子目录仍要命中（否则「纯搬家」会被误报成「登记腐烂」）。
        assert!(matches_registered("observe/watcher.rs", "watcher.rs"));
        assert!(matches_registered("a/b/watcher.rs", "watcher.rs"));
        // 反向：**不许**把名字相近的当成同一个。这几条是这条判据真正的价值所在。
        assert!(!matches_registered("notwatcher.rs", "watcher.rs"));
        assert!(!matches_registered("observe/notwatcher.rs", "watcher.rs"));
        assert!(!matches_registered("watcher.rs.bak", "watcher.rs"));
        assert!(!matches_registered("watcher.rsx", "watcher.rs"));
        assert!(!matches_registered("", "watcher.rs"));
    }

    /// 登记表每条都要有非空理由 —— 不写理由的登记等于无声豁免。
    ///
    /// 〔`K-G6` `KG64`〕同轮加了两栏：**走的是哪一格**（闭集比对）与**解锁条件**（长度地板）。
    #[test]
    fn registered_uses_all_have_reasons() {
        for (file, snippet, why, cell, unlock) in REGISTERED_DURATION_USES {
            assert!(
                why.len() > 20,
                "{file} / `{snippet}` 的登记理由太短，说不清它为什么不是定时器"
            );
            assert!(
                crate::readonly_guard::is_cell(cell),
                "{file} / `{snippet}` 的第四栏是 `{cell}` —— 它不在 `§0a` 四情形表的闭集里（`{}`）。\n\
                 往这张表里加一条**必须说得出它走的是哪一格** —— \n\
                 ★★ 「加白名单」不是那四格里的任何一格：**放宽 ≠ 加白名单**。",
                crate::readonly_guard::cell_names().join(" / ")
            );
            assert!(
                unlock.trim().chars().count() >= 20,
                "{file} / `{snippet}` 没写**解锁条件**（实得 {} 字）—— 要写的是「什么条件满足之后\
                 这一条就能删」，不是「为什么现在不能删」",
                unlock.trim().chars().count()
            );
        }
    }
}

/// 〔`K-G6` `KG61`〕**本护栏今天真能拦住的形状全表 + 一个今天就通过了的反例。**
///
/// # 先列全表，再谈放宽（`§0c 裁五`）
///
/// 这一道判据本身的质量很高（三条反空真 + 调用形态 + 相等断言），**它不是「写得松」**。
/// 它买不到的东西全部落在**人群**上：人群按「本 crate 的源码文本」画，
/// 而「这个进程里有没有东西在自己醒来」是**进程**层面的事实。
/// ⇒ 下面的反例既不是漏洞、也不需要放宽 —— 它是「**它本来就拦不住**」那一侧的活体。
#[cfg(test)]
mod g6_reach {
    use super::tests::{daemon_sources, is_call_of, periodic_wake_patterns, REGISTERED_DURATION_USES};

    /// 按**调用形态**扫的那八个名字（与判据本体同一份清单，抄第二份必然漂开）。
    const CALL_FORMS: &[&str] = &[
        "sleep",
        "interval",
        "interval_at",
        "recv_timeout",
        "park_timeout",
        "wait_timeout",
        "timeout",
        "tick",
    ];

    /// ★ 全表①：**按源码文本子串**禁掉的六个构件，逐形一刀。
    ///
    /// ⚠ 射程如实登记：这一层是**子串**匹配、**没有词边界** ——
    /// 名字里含这几个字的东西也会命中（那一侧是「人群画大了」，靠人一看即知排除）。
    /// 带词边界的那半是下面的**调用形态**层，两层是互补的，不是重复的。
    #[test]
    fn every_banned_construct_is_still_matched_by_substring() {
        let pats = periodic_wake_patterns();
        assert_eq!(
            pats.len(),
            6,
            "禁用构件表从 6 条变成 {} 条了 —— **全表的分母变了**。\n\
             变少 = 有人从表里拿掉了一个构件，那是放宽：先摆出「它今天拦得住什么」再谈。",
            pats.len()
        );
        let mut uniq = pats.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), pats.len(), "禁用构件表里有重复项：{pats:?}");
        for pat in &pats {
            assert!(
                pat.chars().count() >= 6,
                "禁用构件 `{pat}` 短到会大面积误伤 —— 子串层没有词边界"
            );
            // 判据本体用的就是 `code.contains(pat)`：这一刀量的是同一个对象。
            assert!(
                format!("    let _ = {pat}(x);").contains(pat.as_str()),
                "子串层对 `{pat}` 这个形状不响了 —— 全表里这一格今天是空的"
            );
        }
    }

    /// ★ 全表②：**按调用形态**（词边界 + 后面紧跟括号）扫的那八个名字，
    /// 逐形**两刀**：真调用要认出来、形近的不许误认。
    ///
    /// 这一格是本护栏最有牙的一格 —— 它是实测换来的：
    /// 「换个 import 写法 + 换个 `Duration` 构造器」曾让六条判据同时全绿。
    #[test]
    fn every_call_form_fires_on_a_real_call_and_not_on_a_lookalike() {
        assert_eq!(
            CALL_FORMS.len(),
            8,
            "调用形态表从 8 条变成 {} 条了 —— 全表的分母变了",
            CALL_FORMS.len()
        );
        for name in CALL_FORMS {
            assert!(
                is_call_of(&format!("    {name}(d).await;"), name),
                "调用形态 `{name}(` 认不出来了 —— 全表里这一格今天是空的"
            );
            assert!(
                is_call_of(&format!("    tokio::time::{name}(d);"), name),
                "带路径的 `{name}(` 认不出来了"
            );
            assert!(
                !is_call_of(&format!("    let x = do_{name}(3);"), name),
                "`do_{name}(` 被当成了 `{name}(` —— 词边界没守住，误红会让人把这条判据删掉"
            );
            assert!(
                !is_call_of(&format!("    let {name}_ms = 5;"), name),
                "只是同名变量、后面不是括号，不该算调用"
            );
        }
    }

    /// ★ 全表③：`Duration::from_*` 那条是**恰好相等**，分母 = 登记表条数。
    ///
    /// ⚠ 下面刻意先把条数落到一个局部变量再断言：判据本体那条断言的实参行
    /// （`REGISTERED_DURATION_USES.len(),`）被 `ratchet_guard::PINS` 按
    /// 「整行相等 + **恰好 1 次**」钉着，在这里原样再写一遍会让它变成 2 次 ⇒ 棘轮当场红，
    /// 而红的原因与本条无关。**这一格是本轮现打出来的，不是想出来的。**
    #[test]
    fn the_duration_registry_is_an_equality_not_a_floor() {
        let registered = REGISTERED_DURATION_USES.len();
        assert_eq!(
            registered, 3,
            "登记表从 3 条变成 {registered} 条了 —— 这个数就是那条相等断言的分母，\
             改它等于改判据的射程"
        );
    }

    /// 〔`KG61` ②〕**今天在盘上、形状与本护栏声称要拦的相同、而它放过了的那一处。**
    ///
    /// `(住址, 生产段里的片段, 形状对得上的是禁用表里的哪几条, why, unlock)`
    const KNOWN_PASSING_COUNTEREXAMPLES: &[(&str, &str, &str, &str, &str)] = &[(
        "observe/watcher.rs",
        "new_debouncer(",
        "recv_timeout · Instant::now",
        "生产段亲手拉起一个**依赖 crate** 的去抖器，而那个 crate 内部起了一条具名线程，\
         循环里做**带超时的接收**、并用「现在几点」算下一次期限 —— \
         这两个构件逐字就是本护栏禁用表里的第 3 条与第 5 条，\
         护栏头注还专门解释过其中一条「等不到就自己醒，等价于给循环装了节拍」。\
         **形状对得上，而它一个字都看不见**：人群是本 crate `src/` 的源码文本，\
         依赖 crate 的源码不在里面。\
         ⚠ **诚实边界，别把它说过头**：那条线程**不是自由跑的节拍器** —— \
         有待合并事件时才带期限等，空闲时是无期限阻塞。\
         ⇒ 它**不构成**「daemon 在轮询」的证据；它构成的是\
         「护栏按名字禁掉的构件，此刻正在这个进程里运行」的证据。两件事别混。",
        "去抖改由本 crate 自己实现（那时它落回人群内、由判据管），\
         或人群从「本 crate 源码文本」扩到「进程里实际跑着什么」的那天 —— \
         后者今天做不到，所以这一条会躺很久，**而躺着正是它的岗位**：\
         它让「本护栏的射程」写在盘上、不许腐烂。",
    )];

    /// ★ 反例仍在盘上、仍然通过；而它撞的那两个构件仍然在禁用表里。
    ///
    /// ★★ 本件专属陷阱（件文件 `§3`）在这一格的答案：这条路**从来没红过**，
    /// 不需要放宽 —— 所以正确说法不是「放宽了没红」，是「**它对这条路本来就不响**」。
    #[test]
    fn the_counterexample_is_still_on_the_board_and_still_passes() {
        assert_eq!(KNOWN_PASSING_COUNTEREXAMPLES.len(), 1, "反例表的条数变了");
        let files = daemon_sources();
        let bytes: usize = files.iter().map(|(_, c)| c.len()).sum();
        // 本条自己的反空真地板。**刻意不复用判据本体那个常量**：
        // 那一行被 `ratchet_guard::PINS` 按「整行相等 + 恰好 1 次」钉着，
        // 在这里再写一遍会让它变成 2 次 ⇒ 棘轮当场红，而红的原因与本条无关。
        const FLOOR_FOR_THIS_CHECK: usize = 60_000;
        assert!(
            bytes >= FLOOR_FOR_THIS_CHECK,
            "只扫到 {bytes} 字节生产代码 —— 采集坏了，下面几条在空转"
        );
        for (rel, frag, shapes, why, unlock) in KNOWN_PASSING_COUNTEREXAMPLES {
            let owner = files
                .iter()
                .find(|(n, _)| n.as_str() == *rel)
                .unwrap_or_else(|| panic!("反例住址 `{rel}` 今天不在人群里了 —— 幽灵条目"));
            assert!(
                owner.1.contains(frag),
                "反例 `{rel}` 的生产段里找不到 `{frag}` 了 —— 改掉了就**同轮摘登记**，\
                 并回来重判本护栏的射程"
            );
            // 形状真的对得上：它撞的那两个构件今天**仍在**禁用表里。
            let pats = periodic_wake_patterns();
            for shape in shapes.split(" · ") {
                assert!(
                    pats.iter().any(|p| p.contains(shape)),
                    "反例声称撞的 `{shape}` 已经不在禁用表里了 —— \
                     那这条反例证不了「形状对得上」，回来重判"
                );
            }
            assert!(why.trim().chars().count() >= 20, "`{rel}` 的 why 太短");
            assert!(unlock.trim().chars().count() >= 20, "`{rel}` 没写解锁条件");
        }
        // ★ 它今天**真的通过** —— 全人群对那两个构件零命中。
        let pats = periodic_wake_patterns();
        let mut hits: Vec<String> = Vec::new();
        for (name, code) in &files {
            for pat in &pats {
                if code.contains(pat.as_str()) {
                    hits.push(format!("  {name}: {pat}"));
                }
            }
        }
        assert!(
            hits.is_empty(),
            "禁用构件在人群里出现了：\n{}\n\
             ⇒ 那是本护栏的正题在红，不是本条 —— 先修那个，再回来看反例表",
            hits.join("\n")
        );
    }

    /// ★ 一条**查过、但不算反例**的路，如实登记（免得下一轮有人再查一遍）。
    ///
    /// `plugin/invoke.rs` 用 `timeout` 当命令前缀交给子进程。
    /// `is_call_of` 认不出 `Command::new("timeout")`（前面是引号、后面不是括号）⇒ **确实通过**，
    /// 但它是**一次性期限**，不是周期唤醒 —— **形状不同，不算反例**。
    /// 这一条钉住那个判断今天仍然成立：那处仍然只有命令前缀这一种用法。
    #[test]
    fn the_timeout_command_prefix_is_checked_and_is_not_a_counterexample() {
        assert!(
            !is_call_of("    let mut cmd = Command::new(\"timeout\");", "timeout"),
            "`Command::new(\"timeout\")` 被当成了 `timeout(` 调用 —— \
             那会是一次误红，而误红最省事的消法是把判据删掉"
        );
        assert!(
            is_call_of("    let r = timeout(d, fut).await;", "timeout"),
            "真的 `timeout(` 调用必须认出来，否则上面那条靠「什么都认不出」恒真"
        );
    }
}
