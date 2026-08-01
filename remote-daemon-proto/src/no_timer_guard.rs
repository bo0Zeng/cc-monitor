//! P6（zero-poll-liveness）：**零定时器护栏** —— daemon 的判活**全部**由内核事件驱动，
//! 生产代码里不许再有任何周期性唤醒。
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
    fn periodic_wake_patterns() -> Vec<String> {
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

    /// **已登记的非定时器 `Duration` 用途**（文件名, 片段, 为什么它不是定时器）。
    ///
    /// 这不是豁免清单 —— 下面的断言要求生产段里的 `Duration::from_` 调用**恰好**等于
    /// 本表的条数。多出一处就红，逼人回答「这处是不是又把轮询请回来了」。
    const REGISTERED_DURATION_USES: &[(&str, &str, &str)] = &[(
        "watcher.rs",
        "Duration::from_millis(DEBOUNCE_MS)",
        "notify-debouncer 的**事件合并窗口**：它不产生唤醒，只决定「同一批文件事件攒多久\
         再一起交付」。去掉它 inotify 照样推事件，只是更碎。不是定时器。",
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
    fn daemon_sources() -> Vec<(String, String)> {
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
    /// 实测基线（2026-08-01，Phase E 复测）：**15 个文件合计 121_131 字节**，余量约 34%。
    ///
    /// **别手抄这个数。** 上面那行原本写的是 119_454 —— Phase E 审计算出 121_131，复测证明
    /// **审计对、我抄错了**。本仓已有多起「写下之后没人回来核」的记录，所以复测办法写在这里：
    /// 把下面的常量临时改成一个大数跑 `cargo test no_timer`，失败信息里的「只扫到 N 字节」
    /// 就是当前实测值（那条断言恒打印实时值，不依赖本注释）。
    ///
    /// **要下调这个数之前先问：是真的删了那么多生产代码，还是扫描面又缩了？**
    ///
    /// ⚠ **它一条挡不住「单个文件被剥空/剥过头」** —— 最大的 `watcher.rs`（34_506 字节，占 28%）
    /// 整个消失，总量仍有 86_625 ≥ 80_000、照样绿。所以字节地板必须与
    /// 下面那条**数量相等**判据配着用（Phase D 审计 I1 指出的缺口）。
    const MIN_SCANNED_CODE_BYTES: usize = 80_000;

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
        for (file, snippet, _why) in REGISTERED_DURATION_USES {
            let hit = daemon_sources().into_iter().any(|(n, code)| {
                let same_file = n == *file || n.ends_with(&format!("/{file}"));
                same_file && code.contains(snippet)
            });
            assert!(
                hit,
                "登记表里的 {file} / `{snippet}` 已经不在生产代码里了，请清理登记"
            );
        }
    }

    /// 登记表每条都要有非空理由 —— 不写理由的登记等于无声豁免。
    #[test]
    fn registered_uses_all_have_reasons() {
        for (file, snippet, why) in REGISTERED_DURATION_USES {
            assert!(
                why.len() > 20,
                "{file} / `{snippet}` 的登记理由太短，说不清它为什么不是定时器"
            );
        }
    }
}
