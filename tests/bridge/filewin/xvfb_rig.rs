//! 台架：**本机那个图形会话** —— `Xvfb` ＋ `xdotool`。
//!
//! # 🔴 一、它存在的理由：三格「判不了」此前被框大了一格
//!
//! `真相源/99 §9.1`／`§9.4`／`§10.1` 里那几句「**判不了 —— 缺一台有图形会话的机器**」
//! 逐字写着，而 `设计/60 §4.8.3` 也逐字登记「主线程那侧是裸 tao、不是真 Tauri」。
//! 那几句话对**真显示器**这一半是对的，但它们把射程写宽了一格：
//! 本机装着 `Xvfb`（`真相源/99 §八` 那四趟读数本身就是在 Xvfb 上打的 ——
//! 17 945 帧/29 秒），**「有画面的机器」这件事一直在手上**。
//!
//! ⇒ 本台架把那三格里**Xvfb 买得到的那一片**变成判据；买不到的那一片
//! **原样留在「判不了」里**，逐格写在下面。
//!
//! # 🔴 二、Xvfb 买得到什么、买不到什么（逐格，别读宽）
//!
//! | 维 | Xvfb 买得到吗 | 为什么 |
//! |---|---|---|
//! | 窗口真的被 X 服务器映射出来了 | ✅ | 它是一台真 X 服务器，窗口真的进窗口树，`xdotool` 找得到 |
//! | 真事件到得了控件 | ✅ | XTEST 注进去的是**真 X 事件**，走 winit → egui 的整条真路 |
//! | 两个事件循环共存 | ✅ | 两个循环真的在同一个进程里跑 |
//! | 真 GPU | ❌ | 软渲染（无硬件 GL）。**帧时不许拿这里的数去替换 `§2`／`§8.4`** |
//! | 字体回落 | ❌ | 字体面由本机 fontconfig 决定，不由 Xvfb 决定；换机器就换读数 |
//! | DPI / 缩放 | ❌ | Xvfb 恒 96 dpi、无缩放。真机上的 DPI 那一形照旧判不了 |
//! | 合成器（窗口特效 / 透明 / vsync） | ❌ | Xvfb 下**没有窗口管理器、没有合成器** |
//! | Windows | ❌ | 这一族整条 `cfg(not(windows))`，Windows 上照旧一趟没跑过 |
//!
//! 🔴 **最要紧的一条边界**：`真相源/99` 里那些**帧时与内存**读数
//! （`§2`／`§8.4`／`§8.6`）**不许**用 Xvfb 这边的数去替换或「订正」——
//! 那边量的是 CPU 段／release 档／`Memory::data` 条数，这边是软渲染下的一趟实景，
//! **两个分母**。本台架一个帧时数都不产出，就是为了不诱惑人去相减。
//!
//! # 🔴 三、缺件时它**红**，不是「跳过」
//!
//! 本仓头号病形逐字：「**跳过**」与「**过了**」在终端上长得一样。
//! ⇒ [`require_toolbox`] 在缺 `Xvfb`／`xdotool` 时**panic**，
//! 红的那句话换成「这一格判不了，缺什么」，而**不是**悄悄回一个绿。
//! 打法与本仓既有的那条前提闸同形（`launch_tests` 里那三态 spawn：
//! 「上限到了**照样红**，只是红的那句话换成『前提不成立』」）。
//!
//! ⚠ 后果如实写明：**没装这两件的机器上，这几格会红**。那是刻意的 ——
//! 备选是「悄悄绿」，而那正是这条纪律在禁的事。要在断网沙箱里跑门禁，
//! 就得把 `xvfb` 与 `xdotool` 装进镜像（两个都在发行版仓库里，无需网络之外的东西）。
//!
//! # 🔴 四、为什么每一格都要**另起一个进程**（这一条是承重的，不是洁癖）
//!
//! winit 全进程只许建**一个**事件循环：它有一个进程级的「已经建过了」标志，
//! 建第二个直接回一个「事件循环不能重建」的错，而那个标志**只在 web 平台**
//! 会被清回去（现打核过 winit 0.30.13 那个构造器）。
//! 而 eframe 把建好的那一个缓存在**线程局部**里（现打核过 eframe 0.36.2 那个
//! 包一层的函数）—— 于是：
//!
//! - 同一条线程上第二趟开窗：命中线程局部缓存 ⇒ 成。
//! - **换一条线程**第二趟开窗：缓存是空的 ⇒ 去建 ⇒ 撞上那个进程级标志 ⇒ **必败**。
//!
//! 而 `open_detached_seeded` 每趟都 `std::thread::spawn` 一条**新线程**。
//! ⇒ 一个测试进程里只量得到**一趟**实景开窗。
//! ⇒ 本台架把每一格塞进**它自己的一个子进程**（[`run_scenario`] 重新拉起
//! 这个测试二进制自己，只跑那一格）。顺带还买到两样：`cargo test` 的并行
//! 与这一族彻底隔开 · 那一格真挂了也只挂它自己那个进程。
//!
//! ⚠ 这件事**本身就是一条读数**，不只是台架的实现细节 ——
//! 🔴〔第十三刀 2026-09-23〕它今天是**产品形态的依据**：正因为「一个进程只量得到
//! 一趟开窗」，「关掉就销毁」与「关掉之后还能再打开」在同进程形态下不可同时成立
//! ⇒ 开窗改成了一个窗口一个进程（住 `crate::filewin::proc`）。
//! ⇒ 于是本台架「一格一个子进程」这条实现细节，反过来成了实景量「第二趟、第三趟
//! 开窗都成功」的**唯一**量法：同一台 Xvfb 上把同一个工作面跑三趟
//! （`shell_tests` 的 `scenario_trips`）。
//! 〔这一段先前指名的是那条按「同进程第二趟不许静默成功」写的判据；
//!  它已按新形态重写，旧那条性质为什么退役逐字留在它自己的头注里。〕

use std::io::Read as _;
use std::process::{Child, Command, Stdio};

/// 这一族判据要的两件外部现物。**名字逐字**，缺件那句话直接引它。
pub const NEEDS: [&str; 2] = ["Xvfb", "xdotool"];

/// 子进程输出里那条读数的**哨兵前缀**。
///
/// 🔴 为什么要哨兵：子进程什么都没印、与印了而且对，在断言上**必须分得开**。
/// [`reading`] 抠不到就 panic，所以「子进程静默」不会变成一条空真的绿。
pub const MARK: &str = "XVFB| ";

/// 在 `PATH` 上找一个可执行文件。
///
/// 刻意**不调 `which`** —— 那是第三件外部依赖，而它缺的时候报出来的话会是
/// 「Xvfb 没装」（假归因）。这里只读 `PATH`，自己看文件在不在。
pub fn on_path(bin: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(bin))
        .find(|p| p.is_file())
}

/// 缺了哪几件（顺序照 [`NEEDS`]）。全齐就是空。
pub fn missing() -> Vec<&'static str> {
    NEEDS
        .iter()
        .copied()
        .filter(|b| on_path(b).is_none())
        .collect()
}

/// 🔴 前提不成立就**红**。`cell` 是那一格的人话名字，会进那句话里。
///
/// ⚠ 它**不返回 bool** —— 返回 bool 就会有人写 `if !ok { return; }`，
/// 而那正是「跳过与过了长得一样」那一形。
pub fn require_toolbox(cell: &str) {
    let miss = missing();
    assert!(
        miss.is_empty(),
        "这一格判不了，**不是过了**：{cell} 要一个真图形会话，而本机 `PATH` 上缺 {miss:?}\n\
         ⇒ 缺的是**环境**不是证据：装上 {:?} 这一格就量得到（发行版仓库里都有）。\n\
         ⚠ 刻意让它红而不是悄悄跳过 —— 「跳过」与「过了」在终端上长得一样。",
        NEEDS
    );
}

// ════════════════════════════════════════════════════════════════════════
// 一台 Xvfb
// ════════════════════════════════════════════════════════════════════════

/// 一台跑着的 `Xvfb`。**离开作用域就杀掉**（不留孤儿 X 服务器）。
pub struct Screen {
    child: Child,
    display: String,
}

impl Screen {
    /// 起一台 Xvfb，等到它**真的答得出屏幕尺寸**才回。
    ///
    /// 🔴 就绪判据不是「`spawn` 成功」也不是「睡 N 毫秒」——
    /// 是拿 `xdotool` 去**真问一次**屏幕几何。`spawn` 成功只证明 fork 成了；
    /// 那之后 X 服务器还要绑 socket，而「还没绑上」与「起不来」在下一步长得一样。
    pub fn start() -> Result<Self, String> {
        require_toolbox("Xvfb 台架自己");
        let mut last = String::from("一个候选号都没试到");
        // 🔴 **号段挑高位（避开真会话 `:0` 与别人的临时屏），而起点必须每格不同。**
        //
        // 〔2026-09-21 修〕上一版从固定的 `:90` 开始扫、靠 `/tmp/.X<n>-lock` 在不在来跳号
        // —— **那是 TOCTOU**：`cargo test` 把这些格**并行**跑在同一个进程里，
        // 两格同时看到 `:90` 没锁、同时 `spawn Xvfb :90`，一个赢、另一个的窗口
        // 当场变成别人屏上的野窗口 ⇒ winit 抛 `BadWindow`
        // （winit 的 `x11/util/geometry.rs` 抛的，逐字 `Failed to translate window coordinates`
        //   ＋ `X11Error { error_kind: Window, error_code: 3 }`。
        //   ⚠ **刻意不写行号** —— 那是**仓外**位置，行号随 winit 版本走；
        //   而点仓外符号名会被 `structural_scan` 判红（22b·B 那一拍现打踩过一次）
        //   ⇒ 照它给的出路：指文件 ＋ 逐字引那句话。）
        // ⇒ 子进程被信号打死、退出码 `None`。
        //
        // ⚠ **这一形只在完整门禁里出现**：单独跑任一格都是绿的（一次只有一台 Xvfb）。
        //   立本台架那一拍因为「叫停不许跑门禁」而没跑全量 ⇒ 撞车在那一拍看不见。
        //   **教训：并行安全的东西，单独跑绿不算验过。**
        //
        // ⇒ 起点由一个进程级原子计数器发，**每次 `start()` 拿到不同的基点**。
        //
        // 🔴 **〔2026-09-21 再修 —— 上一版那句「不再是正确性依赖」是假的〕**
        //
        // 上一版逐字写着「锁文件那一跳留着当便宜的预筛……**不再是正确性依赖**」。
        // **那句话错了，而且错得会让这两格永久红。** 现打出来的形状：
        //   · `Drop` 只 `kill` 掉 Xvfb，**没有删 `/tmp/.X<n>-lock`** ——
        //     SIGKILL 之下 X 服务器不会自己收尾 ⇒ **每跑一趟漏一个锁**。
        //   · 而这一跳看到锁就 `continue` ⇒ 30 个号全是陈旧锁时，
        //     30 次全 `continue`，`start()` 回的是初值「一个候选号都没试到」。
        //   ⇒ **号池只减不增，跑够 30 趟就永久红。** 那不是抖动，是棘轮。
        // 现打（2026-09-21）：`:90`–`:119` **30/30 全被锁**，而这些锁记的 PID **全死了**，
        // 当时机器上真在跑的 `Xvfb` 是 **0 个**。四趟里红两趟，正是池子在那一刻见底。
        // ⚠ 而那一版的注释还写着「`screen` 在这里析构 ⇒ 那台 Xvfb 被杀掉，**号也就还回去了**」——
        //   **也是假的**：进程死了，锁还在，号并没有还回去。
        //
        // ⇒ 两头都修：**这一跳改成看「锁里那个 PID 还活着没有」**（见 [`classify_lock`]），
        //   陈旧锁当场回收；**`Drop` 把自己那个锁删掉**（见 `impl Drop`）。
        static NEXT_BASE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let base = NEXT_BASE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        for step in 0..30u32 {
            let num = 90 + (base + step) % 30;
            match slot(num) {
                Slot::Live(pid) => {
                    last = format!("`:{num}` 上有个活着的 X 服务器（pid {pid}）");
                    continue;
                }
                Slot::Stale(pid) => {
                    // 🔴 **陈旧锁当场回收** —— 不回收就等于把这个号永久报废。
                    //    只在本台架自己的号段（90–119）里做，且只在 PID 已死时做。
                    let _ = std::fs::remove_file(lock_path(num));
                    let _ = std::fs::remove_file(format!("/tmp/.X11-unix/X{num}"));
                    eprintln!("  〔台架〕回收陈旧锁 :{num}（记的 pid {pid} 已不在）");
                }
                Slot::Free => {}
            }
            let display = format!(":{num}");
            let child = Command::new("Xvfb")
                .args([&display, "-screen", "0", "1600x1200x24", "-nolisten", "tcp"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            let child = match child {
                Ok(c) => c,
                Err(e) => {
                    last = format!("起 Xvfb {display} 失败：{e}");
                    continue;
                }
            };
            let mut screen = Screen { child, display };
            // 最多等 5 秒（30 × 167ms）—— 就绪由「答得出几何」说了算。
            let mut taken_by_someone_else = false;
            for _ in 0..30 {
                // 🔴🔴 **先看我们自己那台还活着没有** 〔2026-09-22 修，`P25`〕
                //
                // 上一版没有这一跳，而那正是残余那条 flake 的机制：
                //   · 上面 `slot(num)` 与 Xvfb 真建锁之间有一段 TOCTOU；
                //     `NEXT_BASE` 只错开**起点**，两格的扫描路径照样会在下一步汇到同一个号。
                //   · 两格同时 spawn `Xvfb :N` ⇒ **X 服务器自己的锁是仲裁者**，
                //     输的那台当场退出，stderr 逐字
                //     `(EE) Cannot establish any listening sockets - Make sure an X server isn't already running`
                //     ＋ `exit=1`（2026-09-22 现打，三个并发里两个是这一形）。
                //   · 🔴 **而下面那一跳会「成功」** —— `xdotool` 去问 `:N` 的几何时，
                //     答话的是**赢家那台服务器**。于是 `start()` 返回一个
                //     「子进程已死、屏是别人的」`Screen`。
                //   ⇒ 两格的窗口挤在同一台屏上：数窗口那条判据数出 2 个（期望 1 个），
                //     或者赢家 `Drop` 时把屏杀掉 ⇒ winit 抛 `BadWindow`。
                //     **那正是本模块上面那段注释描述的症状，只是病根还剩这一段。**
                //
                // ⚠ 顺带订正一条现打：`Xvfb :90 -displayfd <fd>` **不会**从 `:90` 起扫
                //   （给了显式号就只试那一个，被占即退）。`-displayfd` 只在**不给号**时
                //   才自己扫，而那会从 `:0` 起 —— 撞本台架「避开低位号」那条策略。
                //   ⇒ 不走 `-displayfd`，改成把**服务器自己的锁**当仲裁者：
                //     它活着 = 这个号是我们的；它退了 = 别人的，换下一个。
                let exited = screen.child.try_wait().ok().flatten();
                let geometry_ok = screen
                    .xdotool(&["getdisplaygeometry"])
                    .is_ok_and(|out| out.split_whitespace().count() == 2);
                // ⚠ 两个观测量都取到了才裁决 —— 裁决本身住 [`judge_claim`]（纯，可判）。
                match judge_claim(exited.is_some(), geometry_ok) {
                    Claim::TakenByAnother => {
                        last = format!(
                            "`:{num}` 被别人占了 —— 我们那台 Xvfb 当场退出（{:?}）。\
                             并行的另一格赢了这个号",
                            exited
                        );
                        taken_by_someone_else = true;
                        break;
                    }
                    Claim::Ours => return Ok(screen),
                    Claim::NotReadyYet => {}
                }
                std::thread::sleep(std::time::Duration::from_millis(167));
            }
            if !taken_by_someone_else {
                last = format!("Xvfb {} 起了但 5 秒内答不出屏幕几何", screen.display);
            }
            // `screen` 在这里析构 ⇒ 那台 Xvfb 被杀掉（已经死了就是 no-op），
            // 而锁**只在它记着我们自己这个 pid 时**才删 ⇒ 不会误删赢家的锁。
        }
        Err(last)
    }

    /// 这台屏的 `DISPLAY` 值（形如 `:90`）。
    pub fn display(&self) -> &str {
        &self.display
    }

    /// 在这台屏上跑一条 `xdotool`。回它的标准输出（已 `trim`）。
    pub fn xdotool(&self, args: &[&str]) -> Result<String, String> {
        xdotool_on(&self.display, args)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let pid = self.child.id();
        let _ = self.child.kill();
        let _ = self.child.wait();
        // 🔴 **把自己那个锁删掉。** SIGKILL 之下 X 服务器不会自己收尾 ⇒
        //    不删就是漏一个号，而号池只有 30 个（理由逐条住 `start()` 里那段）。
        //
        // ⚠ **只删记着我们自己这个 pid 的那一份** —— 不许见锁就删：
        //    别人（另一条并行的格、或机器上真的 X 会话）的锁不归我们管。
        if let Some(num) = self
            .display
            .strip_prefix(':')
            .and_then(|n| n.parse::<u32>().ok())
        {
            if lock_records_pid(num, pid) {
                let _ = std::fs::remove_file(lock_path(num));
                let _ = std::fs::remove_file(format!("/tmp/.X11-unix/X{num}"));
            }
        }
    }
}

/// 一个候选显示号的占用状况。
///
/// 🔴 **三态，不是「锁在不在」两态** —— 「有人在用」与「有人留了个死锁」
/// 必须分得开，否则一个死锁就把那个号永久报废（那正是 2026-09-21 逮到的病）。
#[derive(Debug, PartialEq, Eq)]
pub enum Slot {
    /// 没有锁文件。
    Free,
    /// 锁在，记的进程**还活着** ⇒ 真有人在用，别碰。
    Live(u32),
    /// 锁在，记的进程**已经不在** ⇒ 可回收。
    Stale(u32),
}

/// `/tmp/.X<n>-lock` 的住址。抽成函数是为了让判据能指着它说话。
fn lock_path(num: u32) -> String {
    format!("/tmp/.X{num}-lock")
}

/// **纯函数**：由锁文件的内容与一个「这个 pid 活着吗」的判定，算出占用状况。
///
/// 🔴 抽成纯函数的理由是**可判**：判据能拿合成输入把两个方向都打一遍
/// （活 pid ⇒ `Live`、死 pid ⇒ `Stale`），**不用去动 `/tmp`**（动它会砸掉并行跑的别的格）。
///
/// ⚠ **读不懂的内容一律判 `Live`（保守）** —— 宁可放弃一个号，
/// 也不要把别人正在用的屏当成垃圾回收掉。
pub fn classify_lock(contents: Option<&str>, alive: impl Fn(u32) -> bool) -> Slot {
    let Some(body) = contents else {
        return Slot::Free;
    };
    // X11 的约定：锁里是十进制 pid（左侧空格补到 10 位）＋ 换行。
    match body.trim().parse::<u32>() {
        Ok(pid) if pid > 0 => {
            if alive(pid) {
                Slot::Live(pid)
            } else {
                Slot::Stale(pid)
            }
        }
        // 解不出 pid ⇒ 不知道是谁的 ⇒ 当成有人在用。
        _ => Slot::Live(0),
    }
}

/// 这个 pid 还活着吗。
///
/// ⚠ 走 `/proc` —— **Linux 专有**。本台架本来就只在有 `Xvfb`／`xdotool` 的
/// Unix 上跑（整份文件 `#[cfg(not(windows))]`），而那两样在实践中就是 Linux。
/// 哪天要上别的 Unix，这一处要重判。
fn pid_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// 读盘那一层：把 `classify_lock` 接到真的 `/tmp` 上。
fn slot(num: u32) -> Slot {
    classify_lock(
        std::fs::read_to_string(lock_path(num)).ok().as_deref(),
        pid_alive,
    )
}

/// 🔴 **这一族的独占闸** —— 任何要起真 Xvfb 的格，先拿它。
///
/// # 为什么要它（2026-09-22，`P25`）
///
/// 这一族此前**事实上是串的**：只有 `scenario_a` 那一处起屏，而它用 `OnceLock`
/// 保证只跑一趟。⇒「并行」这件事在这一族里没有被真正行使过。
///
/// 而本拍新加的 `the_toolbox_hands_out_a_different_display_to_each_screen`
/// 要**同时**起两台屏 ⇒ 它与 `scenario_a` 之间第一次出现了真竞争，
/// 而这一族正是本仓已知最脆的那一族（`真相源`／`设计/99 P25` 记着约 6–10 趟咬 1 次）。
///
/// 🔴 **我没有证据说那条新判据就是病根**（现打 6 趟全绿，而这个频率下 6 趟证明不了
/// 任何一侧）。⇒ 处置按「**不给已知脆的地方加拥挤**」来，而不是按「它无罪」来：
/// 两处都经这一个闸 ⇒ 它们从不重叠，而那条并发判据**在它自己内部**照样真并发
/// （它一次起两台），买到的东西一点没少。
///
/// ⚠ 毒化容忍：一条格 panic 了不该让后面每一格都跟着 panic
/// （同 `index_testing::serial` 那条的理由）。
pub fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    match LOCK.lock() {
        Ok(g) => g,
        Err(p) => {
            eprintln!("  〔台架〕上一格在持有独占闸时 panic 了 —— 毒化容忍，继续");
            p.into_inner()
        }
    }
}

/// spawn 之后那一步的裁决：**这个号到底是不是我们的**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// 是我们的 —— 我们那台还活着，而且屏答得出几何。
    Ours,
    /// 🔴 **别人的** —— 我们那台当场退出了（X 服务器自己的锁把它挡回来了）。
    TakenByAnother,
    /// 还没就绪 —— 我们那台活着，但屏还没答得出几何。再等等。
    NotReadyYet,
}

/// **纯函数**：由两个观测量定夺（同 [`classify_lock`] 的理由 —— 判得到，
/// 而且不用真起两台 Xvfb 去撞号）。
///
/// # 🔴 承重的是第一行的**顺序**
///
/// 「我们那台死了没有」要排在「几何答不答得出」**前面**，因为两者可以**同时成立**：
/// 并行的另一格赢了这个号之后，`xdotool` 去问 `:N` 的几何，
/// **答话的是赢家那台服务器** ⇒ `geometry_ok == true`，而我们那台已经 `exit 1`。
///
/// 顺序反了（或者干脆不看 `child_exited`，那正是 2026-09-22 之前的样子）的后果：
/// `start()` 返回一个「子进程已死、屏是别人的」`Screen`
/// ⇒ 两格的窗口挤在同一台屏上 ⇒ 数窗口那条判据数出 2 个（期望 1 个），
/// 或者赢家 `Drop` 时把屏杀掉 ⇒ winit 抛 `BadWindow`。
pub fn judge_claim(child_exited: bool, geometry_ok: bool) -> Claim {
    if child_exited {
        // 🔴 **即使 `geometry_ok` 也走这一支** —— 那台答话的不是我们的。
        return Claim::TakenByAnother;
    }
    if geometry_ok {
        Claim::Ours
    } else {
        Claim::NotReadyYet
    }
}

/// 这个号的锁**记的是不是我们这个 pid**。
fn lock_records_pid(num: u32, pid: u32) -> bool {
    std::fs::read_to_string(lock_path(num))
        .ok()
        .and_then(|b| b.trim().parse::<u32>().ok())
        == Some(pid)
}

/// 在 `display` 那台屏上跑一条 `xdotool`。
pub fn xdotool_on(display: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new("xdotool")
        .args(args)
        .env("DISPLAY", display)
        .output()
        .map_err(|e| format!("起 xdotool 失败（{args:?}）：{e}"))?;
    if !out.status.success() {
        return Err(format!(
            "xdotool {args:?} 退出码 {:?}；stderr={}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// 等一个标题含 `needle` 的窗口出现。回**所有**命中的窗口 id。
///
/// ⚠ 回的是**全部**而不是第一个：数得出「恰好一个」才谈得上相等断言，
/// 而「找到了一个」在多出一个窗口时照样成立。
///
/// 🔴 带 `--onlyvisible`：只认**真的映射到屏幕上**的那一档。
/// 不带的话一个建了却没 map 的窗口照样数得到 ——
/// 而「建了」与「摆到屏幕上了」正是这一格要分开的两件事。
pub fn wait_for_windows(display: &str, needle: &str, budget_ms: u64) -> Vec<String> {
    let step = 100;
    let mut waited = 0;
    loop {
        if let Ok(out) = xdotool_on(display, &["search", "--onlyvisible", "--name", needle]) {
            let ids: Vec<String> = out
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            if !ids.is_empty() {
                return ids;
            }
        }
        if waited >= budget_ms {
            return Vec::new();
        }
        std::thread::sleep(std::time::Duration::from_millis(step));
        waited += step;
    }
}

/// 一个窗口在**根坐标**里的位置与尺寸（都是物理像素）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// 问一个窗口的几何。走 `--shell` 那一档（`KEY=值`，好解析）。
pub fn geometry(display: &str, id: &str) -> Result<Geometry, String> {
    let out = xdotool_on(display, &["getwindowgeometry", "--shell", id])?;
    let get = |k: &str| -> Result<i64, String> {
        out.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .ok_or_else(|| format!("几何里没有 {k}=：{out}"))?
            .trim()
            .parse::<i64>()
            .map_err(|e| format!("{k} 不是整数：{e}"))
    };
    Ok(Geometry {
        x: get("X")? as i32,
        y: get("Y")? as i32,
        w: get("WIDTH")? as u32,
        h: get("HEIGHT")? as u32,
    })
}

// ════════════════════════════════════════════════════════════════════════
// 一格 ＝ 一个子进程
// ════════════════════════════════════════════════════════════════════════

/// 一趟子进程跑完之后的全部现物。
pub struct ChildRun {
    pub ok: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl ChildRun {
    /// 子进程没跑成就红，并把它自己印的东西**整段带出来**
    /// （门禁那一侧只看得见这一段 —— 藏起来等于红了也说不出原因）。
    pub fn must_have_passed(&self, cell: &str) {
        assert!(
            self.ok,
            "{cell}：实景子进程退出码 {:?}\n───── 子进程 stdout ─────\n{}\n\
             ───── 子进程 stderr ─────\n{}",
            self.code,
            self.stdout.trim(),
            self.stderr.trim()
        );
    }

    /// 抠一条读数；抠不到就红。见 [`reading`]。
    pub fn reading(&self, key: &str) -> String {
        reading(&self.stdout, key)
    }
}

/// 从子进程的输出里抠 `key=` 那一条读数。
///
/// 🔴 **抠不到就 panic**：子进程什么都没印（那一格静默没跑）与
/// 印了而且对，在断言上必须分得开 —— 否则父进程那几条相等断言在空转。
/// ⚠ 同一个 key 出现多次就取**最后一条**（一格里同一个量可能量两趟）。
///
/// ⚠ 🔴 **哨兵在行内任意位置都算**，不是只认行首 —— 现打栽过一次：
/// libtest 在 `--nocapture` 下把测试自己印的第一行**接在**
/// `test 某某 ... ` 后面，于是第一条读数永远不在行首。
/// 只认行首的话，「一格真的量到了」会被读成「什么都没印」。
/// ⚠ 认的是 `哨兵 + key + =` 整串 ⇒ `n.reason` 不会误吃 `n.reason_len`。
pub fn reading(out: &str, key: &str) -> String {
    let want = format!("{MARK}{key}=");
    let got = out
        .lines()
        .filter_map(|l| l.find(&want).map(|i| &l[i + want.len()..]))
        .last();
    match got {
        Some(v) => v.trim().to_string(),
        None => panic!(
            "子进程没印 `{key}` 这条读数 —— 这一格**没量到**，不是过了。\n\
             ───── 子进程印了这些 ─────\n{}",
            out.lines()
                .filter(|l| l.contains(MARK))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}

/// 印一条读数（子进程那一侧用）。
pub fn emit(key: &str, value: impl std::fmt::Display) {
    println!("{MARK}{key}={value}");
}

/// 在**自己一个进程**里跑一格实景判据。
///
/// `test_path` 是那个工作面测试的**全路径**（`filewin::shell::tests::某某`）。
/// 那几个工作面都挂着 `#[ignore]`，所以：
/// - 平时 `cargo test` 里它们是 `ignored`（终端上看得见，不会冒充一条绿）；
/// - 这里带 `--ignored --exact` 把它们**逐个**点起来。
///
/// 为什么必须另起进程见本模块头注第四节（winit 一个进程只许一个事件循环）。
pub fn run_scenario(display: &str, test_path: &str) -> ChildRun {
    let exe = std::env::current_exe().expect("拿不到这个测试二进制自己的路径");
    let mut child = Command::new(&exe)
        .args([
            "--exact",
            test_path,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("DISPLAY", display)
        // 子进程再去起子进程会无穷递归 —— 用它挡住（工作面自己也查一遍）。
        .env("CCM_FILEWIN_XVFB_CHILD", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("拉不起实景子进程 {exe:?}：{e}"));

    // 两条管子各一条线程 —— 只读一条会在另一条写满管子时死锁。
    let mut so = child.stdout.take().expect("stdout 管子");
    let mut se = child.stderr.take().expect("stderr 管子");
    let t_out = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = so.read_to_string(&mut s);
        s
    });
    let t_err = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = se.read_to_string(&mut s);
        s
    });
    let status = child.wait().expect("等实景子进程");
    let stdout = t_out.join().unwrap_or_default();
    let stderr = t_err.join().unwrap_or_default();
    ChildRun {
        ok: status.success(),
        code: status.code(),
        stdout,
        stderr,
    }
}

/// 工作面那一侧：**父进程没给 `DISPLAY` 就红**。
///
/// ⚠ 它不许退化成「没有 DISPLAY 就当过了」—— 那一形是本族在防的那件事。
pub fn child_display() -> String {
    assert_eq!(
        std::env::var("CCM_FILEWIN_XVFB_CHILD").ok().as_deref(),
        Some("1"),
        "这是一个**工作面**，只许由它的父判据在自己的进程里点起来（见台架头注第四节）"
    );
    std::env::var("DISPLAY").expect("父进程没把 DISPLAY 传下来 —— 这一格判不了，不是过了")
}
