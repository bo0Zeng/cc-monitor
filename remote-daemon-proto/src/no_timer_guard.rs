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

    /// 剥掉 `#[cfg(test)]` 块与行注释，只留生产代码。
    ///
    /// 剥注释是必需的：本 crate 的注释**大量**在解释「为什么这里没有定时器了」，
    /// 逐字提到那些模式名（P4 实测：不剥的话守卫会被解释它自己的那段散文打红）。
    fn production_code(src: &str) -> String {
        // 锚点用转义写法 ⇒ 与真正的换行不相等 ⇒ 不会匹配到本行自己。
        let marker = "\n#[cfg(test)]\nmod tests";
        let prod = match src.find(marker) {
            Some(i) => &src[..i],
            None => src,
        };
        prod.lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn daemon_sources() -> Vec<(String, String)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            // 跳过本护栏自身：它的模式表与说明文字必然含这些子串。
            if name == "no_timer_guard.rs" {
                continue;
            }
            let src = std::fs::read_to_string(&path).expect("read rs file");
            out.push((name, production_code(&src)));
        }
        out
    }

    /// ★ 生产段不许有任何**周期性唤醒**构件。
    #[test]
    fn daemon_production_code_has_no_periodic_wakeups() {
        let files = daemon_sources();
        // 反向自检：扫到的文件数 > 0。**断言的是「命中数 == 0」+「扫到了东西」**，
        // 而不是「命中数 < N」—— 阈值绝不能挂在被优化的那个量上（rust-ts-boundary 的教训）。
        assert!(
            files.len() >= 5,
            "只扫到 {} 个 daemon 源文件，护栏多半没在扫该扫的东西",
            files.len()
        );
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
        for (file, snippet, _why) in REGISTERED_DURATION_USES {
            let hit = daemon_sources()
                .into_iter()
                .any(|(n, code)| n == *file && code.contains(snippet));
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
