//! 台架：**本机那个图形会话** —— `Xvfb` ＋ `xdotool`。
//!
//! # 🔴 一、它存在的理由：三格「判不了」此前被框大了一格
//!
//! ／`§9.4`／`§10.1` 里那几句「**判不了 —— 缺一台有图形会话的机器**」
//! 逐字写着，而也逐字登记「主线程那侧是裸 tao、不是真 Tauri」。
//! 那几句话对**真显示器**这一半是对的，但它们把射程写宽了一格：
//! 本机装着 `Xvfb`（那四趟读数本身就是在 Xvfb 上打的 ——
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
//! 🔴 **最要紧的一条边界**：里那些**帧时与内存**读数
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
//! 🔴它今天是**产品形态的依据**：正因为「一个进程只量得到
//! 一趟开窗」，「关掉就销毁」与「关掉之后还能再打开」在同进程形态下不可同时成立
//! ⇒ 开窗改成了一个窗口一个进程（住 `crate::proc`）。
//! ⇒ 于是本台架「一格一个子进程」这条实现细节，反过来成了实景量「第二趟、第三趟
//! 开窗都成功」的**唯一**量法：同一台 Xvfb 上把同一个工作面跑三趟
//! （`shell_tests` 的 `scenario_trips`）。
//! 〔这一段先前指名的是那条按「同进程第二趟不许静默成功」写的判据；
//!  它已按新形态重写，旧那条性质为什么退役逐字留在它自己的头注里。〕

use std::io::Read;
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

/// 起一台私有 Xvfb 的那一份起法（截图工具 `tests/shots/filewin.mjs` 同用它）：脚本挑空号、按 X 的老规矩建锁占住
/// （门禁的网络命名空间里外都认得），Xvfb 自己把号写在标准输出第一行（`-displayfd`）；脚本 `exec` 成 Xvfb 本身
/// ⇒ 子进程的 pid 就是那台屏。收场用它的 `release`。
pub const LAUNCHER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tests/scripts/xvfb-free.sh"
);

/// 一台跑着的 `Xvfb`。**离开作用域就杀掉**（不留孤儿 X 服务器）。
pub struct Screen {
    child: Child,
    display: String,
    /// 它报号的那一端（留着不关：关了它再写就是写一根断管）。
    _displayfd: std::process::ChildStdout,
}

impl Screen {
    /// 起一台 Xvfb（[`LAUNCHER`]），等到它**真的答得出屏幕尺寸**才回。
    ///
    /// - 号不由台架挑：挑号与占号在那份脚本里一把文件锁下做完（理由见脚本头注），
    ///   两格（或门禁沙箱里外、门禁与截图工具）同时起，也不会两台挤在同一个号上。
    /// - 就绪判据不是「`spawn` 成功」也不是「睡 N 毫秒」—— 是拿 `xdotool` 去**真问一次**屏幕几何。
    pub fn start() -> Result<Self, String> {
        require_toolbox("Xvfb 台架自己");
        let mut child = Command::new("bash")
            .arg(LAUNCHER)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("起 Xvfb 失败（{LAUNCHER}）：{e}"))?;
        let mut fd = child.stdout.take().ok_or("Xvfb 的标准输出没接上")?;
        let num = match read_display_number(&mut fd) {
            Ok(n) => n,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        let screen = Screen {
            child,
            display: format!(":{num}"),
            _displayfd: fd,
        };
        // 最多等 5 秒（30 × 167ms）—— 就绪由「答得出几何」说了算。
        for _ in 0..30 {
            if screen
                .xdotool(&["getdisplaygeometry"])
                .is_ok_and(|out| out.split_whitespace().count() == 2)
            {
                return Ok(screen);
            }
            std::thread::sleep(std::time::Duration::from_millis(167));
        }
        Err(format!(
            "Xvfb {} 起了但 5 秒内答不出屏幕几何",
            screen.display
        ))
    }

    /// 这台屏的 `DISPLAY` 值（形如 `:3`）。
    pub fn display(&self) -> &str {
        &self.display
    }

    /// 在这台屏上跑一条 `xdotool`。回它的标准输出（已 `trim`）。
    pub fn xdotool(&self, args: &[&str]) -> Result<String, String> {
        xdotool_on(&self.display, args)
    }
}

/// 读 Xvfb 报的号：一行十进制、换行收尾。它没报就退了（起不来）⇒ 读到文件尾 ⇒ `Err`。
pub fn read_display_number(fd: &mut impl Read) -> Result<u32, String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match fd.read(&mut byte) {
            Ok(0) => return Err("Xvfb 没报号就退了（起不来）".into()),
            Ok(_) if byte[0] == b'\n' => break,
            Ok(_) => line.push(byte[0]),
            Err(e) => return Err(format!("读 Xvfb 报的号失败：{e}")),
        }
    }
    let text = String::from_utf8_lossy(&line);
    text.trim()
        .parse::<u32>()
        .map_err(|_| format!("Xvfb 报的号读不懂：{text:?}"))
}

impl Drop for Screen {
    /// 先请它自己收场（SIGTERM）；2 秒不退再 SIGKILL。之后经那份脚本的 `release` 收掉占号的锁与套接字
    /// （锁记的是它这个 pid 才删 —— 别人的不碰）。
    fn drop(&mut self) {
        let pid = self.child.id().to_string();
        let _ = Command::new("kill")
            .args(["-TERM", &pid])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let gone = (0..20).any(|_| {
            let done = matches!(self.child.try_wait(), Ok(Some(_)));
            if !done {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            done
        });
        if !gone {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(num) = self.display.strip_prefix(':') {
            let _ = Command::new("bash")
                .args([LAUNCHER, "release", num, &pid])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

/// 一台屏的本地套接字文件。
pub fn socket_path(num: u32) -> String {
    format!("/tmp/.X11-unix/X{num}")
}

/// 一台屏占号的那个锁（X 的老规矩：里头是占着它的那个 pid）。
pub fn lock_path(num: u32) -> String {
    format!("/tmp/.X{num}-lock")
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
/// 而这一族正是本仓已知最脆的那一族（／记着约 6–10 趟咬 1 次）。
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

// ════════════════════════════════════════════════════════════════════════
// 🔴关窗：像窗口管理器那样**请**它关，而不是替它拆
// ════════════════════════════════════════════════════════════════════════
//
// # 为什么不用 `xdotool windowclose`
//
// 本机那版 xdotool（3.20160805）的 `windowclose` 是 **`XDestroyWindow`** —— 替窗口的主人
// 把窗口拆掉，**不管**窗口在 `WM_PROTOCOLS` 里列没列 `WM_DELETE_WINDOW`。
// 现打（`xev` 当靶子：它列了 `WM_DELETE_WINDOW`）：收到的是 `UnmapNotify` ＋ `DestroyNotify`，
// **一条 `ClientMessage` 都没有**，`xev` 自己也没退。
// ⇒ 一个 winit 窗口被外人拆掉之后，winit 下一次拿这个窗口去问 X 服务器
// （每帧 egui 都要问一次位置）就回 `BadWindow`，而 winit 在那几处是 `unwrap()`／`expect()`
// ⇒ **每一趟都 panic**（现打 160/160）。其中一小撮 panic 恰好落在 winit 拿着
// 自己那把状态锁的时候 ⇒ 锁被毒化 ⇒ 析构里再 panic ⇒ **abort**（现打约一成）——
// 那一成就是这一族「时好时坏」的全部来历。
//
// # 用户那一下在真桌面上是什么
//
// 用户点标题栏的 ×，窗口管理器发给窗口的是一条 `ClientMessage`
// （`WM_PROTOCOLS` / `WM_DELETE_WINDOW`，ICCCM §4.2.8.1）—— **请**窗口的主人自己关。
// winit 收到它回 `CloseRequested`，eframe 自己收场、自己拆窗口 ⇒ 没有「窗口已经没了、
// 别人还在问它」的那一段。Xvfb 下没有窗口管理器，于是本台架**替窗口管理器发这一条**。
//
// ⚠ 买不到：真窗口管理器会 reparent（窗口外面套一层框），`TranslateCoordinates`
// 那条路在真桌面上走的是另一种几何 ⇒ 真桌面上关窗干不干净，仍然判不了。
//
// ⚠ 为什么手写 X11 协议而不是再加一件工具／一个依赖：xdotool 这一版**发不了**
// 这条消息（它没有 `windowquit`），而加 crate 要动 `Cargo.toml`（不在这一路的写区）。
// 要发的只有三条请求（两次 `InternAtom` ＋ 一次 `SendEvent`）＋ 一次往返确认，
// 全是定长小包 ⇒ 字节布局住下面两个**纯函数**，由 `x11_wire_tests` 逐字节判。

/// X11 请求里的字符串要按 4 字节补齐。
fn pad4(n: usize) -> usize {
    (4 - n % 4) % 4
}

/// **纯函数**：`InternAtom` 请求（opcode 16，`only_if_exists = 0`），小端。
pub fn intern_atom_request(name: &str) -> Vec<u8> {
    let n = name.len();
    let words = 2 + (n + pad4(n)) / 4;
    let mut b = Vec::with_capacity(words * 4);
    b.push(16u8);
    b.push(0);
    b.extend_from_slice(&(words as u16).to_le_bytes());
    b.extend_from_slice(&(n as u16).to_le_bytes());
    b.extend_from_slice(&[0, 0]);
    b.extend_from_slice(name.as_bytes());
    b.extend(std::iter::repeat_n(0u8, pad4(n)));
    b
}

/// **纯函数**：`SendEvent`（opcode 25）装着一条 `ClientMessage`（事件码 33，format 32）：
/// `type = WM_PROTOCOLS`，`data[0] = WM_DELETE_WINDOW`，`data[1] = CurrentTime(0)`。
/// 投递目标就是那个窗口，`event_mask = 0`（ICCCM：发给窗口的主人本人）。
pub fn wm_delete_request(window: u32, wm_protocols: u32, wm_delete_window: u32) -> [u8; 44] {
    let mut b = [0u8; 44];
    b[0] = 25; // SendEvent
    b[1] = 0; // propagate = False
    b[2..4].copy_from_slice(&11u16.to_le_bytes()); // 44 字节 = 11 个字
    b[4..8].copy_from_slice(&window.to_le_bytes()); // destination
    b[8..12].copy_from_slice(&0u32.to_le_bytes()); // event_mask = 0
    let e = &mut b[12..44];
    e[0] = 33; // ClientMessage
    e[1] = 32; // format
    e[4..8].copy_from_slice(&window.to_le_bytes());
    e[8..12].copy_from_slice(&wm_protocols.to_le_bytes());
    e[12..16].copy_from_slice(&wm_delete_window.to_le_bytes());
    // e[16..20] = CurrentTime = 0，其余三格 0。
    b
}

/// 在 `display`（形如 `:3`）那台屏上，对窗口 `id`（`xdotool` 给的十进制）
/// 发一条 **`WM_DELETE_WINDOW`** —— 窗口管理器关窗时发的那一条。
///
/// 回 `Ok(())` = 服务器**收下并处理完**了那条 `SendEvent`（后面跟一次 `GetInputFocus`
/// 往返：X 按序处理请求，往返回来之前若有错误包，它一定先到）。
/// ⚠ 它**不**等窗口真的消失 —— 那是窗口主人自己的事，由调用方去量
/// （`run_native` 的裁决、退出码）。
pub fn close_like_a_wm(display: &str, id: &str) -> Result<(), String> {
    use std::io::Write as _;
    let num: u32 = display
        .strip_prefix(':')
        .and_then(|n| n.split('.').next())
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| format!("看不懂的 DISPLAY：{display:?}"))?;
    let window: u32 = id
        .trim()
        .parse()
        .map_err(|e| format!("窗口 id {id:?} 不是十进制整数：{e}"))?;
    let path = socket_path(num);
    let mut s = std::os::unix::net::UnixStream::connect(&path)
        .map_err(|e| format!("连不上 {path}：{e}"))?;
    s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let io = |e: std::io::Error| format!("X11 连接读写失败：{e}");

    // 握手：小端 'l'、协议 11.0、无认证（台架起 Xvfb 不带 `-auth`）。
    s.write_all(&[b'l', 0, 11, 0, 0, 0, 0, 0, 0, 0, 0, 0])
        .map_err(io)?;
    let mut head = [0u8; 8];
    s.read_exact(&mut head).map_err(io)?;
    let extra = u16::from_le_bytes([head[6], head[7]]) as usize * 4;
    let mut rest = vec![0u8; extra];
    s.read_exact(&mut rest).map_err(io)?;
    if head[0] != 1 {
        let n = (head[1] as usize).min(rest.len());
        return Err(format!(
            "X 服务器拒绝了握手（状态 {}）：{}",
            head[0],
            String::from_utf8_lossy(&rest[..n])
        ));
    }

    let mut intern = |name: &str| -> Result<u32, String> {
        s.write_all(&intern_atom_request(name)).map_err(io)?;
        let mut r = [0u8; 32];
        s.read_exact(&mut r).map_err(io)?;
        if r[0] != 1 {
            return Err(format!("InternAtom({name}) 回了错误码 {}", r[1]));
        }
        Ok(u32::from_le_bytes([r[8], r[9], r[10], r[11]]))
    };
    let protocols = intern("WM_PROTOCOLS")?;
    let delete = intern("WM_DELETE_WINDOW")?;

    s.write_all(&wm_delete_request(window, protocols, delete))
        .map_err(io)?;
    // GetInputFocus（opcode 43）当往返栅栏：它的回复到了 ⇒ 前面那条 SendEvent 已处理完。
    s.write_all(&[43, 0, 1, 0]).map_err(io)?;
    let mut r = [0u8; 32];
    s.read_exact(&mut r).map_err(io)?;
    match r[0] {
        1 => Ok(()),
        0 => Err(format!(
            "SendEvent 被 X 服务器拒了：错误码 {}（3 = BadWindow ⇒ 窗口 {window} 已经不在）",
            r[1]
        )),
        k => Err(format!("往返栅栏回来的不是回复而是第 {k} 类包")),
    }
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
/// `test_path` 是那个工作面测试的**全路径**（`shell::tests::某某`；窗口独立成包之后路径从本包根算）。
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

#[cfg(test)]
#[path = "xvfb_rig_tests.rs"]
mod same_launcher_tests;

/// 🔴那两个纯函数的字节布局 —— **逐字节相等**，不是「长度对」。
///
/// 期望值不是拿被测函数算出来的（那是两侧同源恒真），是照 X11 协议规范逐格手写的：
/// `InternAtom` = opcode 16 · 长度字 · 名字长 · 名字补到 4 的倍数；
/// `SendEvent` = opcode 25 · 长度 11 · 目标 · 掩码 0 · 32 字节的 `ClientMessage`（码 33、format 32）。
mod x11_wire_tests {
    use super::*;

    #[test]
    fn intern_atom_request_is_byte_exact() {
        // "WM_PROTOCOLS" 12 字节，恰好对齐 ⇒ 不补；长度 = 2 + 3 = 5 个字。
        let mut want = vec![16u8, 0, 5, 0, 12, 0, 0, 0];
        want.extend_from_slice(b"WM_PROTOCOLS");
        assert_eq!(intern_atom_request("WM_PROTOCOLS"), want);
        // "WM_DELETE_WINDOW" 16 字节；再拿一个**要补**的名字打补齐那一支（5 字节 ⇒ 补 3）。
        assert_eq!(
            intern_atom_request("ABCDE"),
            vec![16u8, 0, 4, 0, 5, 0, 0, 0, b'A', b'B', b'C', b'D', b'E', 0, 0, 0]
        );
    }

    #[test]
    fn wm_delete_request_is_byte_exact() {
        let got = wm_delete_request(0x0040_0004, 0x0000_01A3, 0x0000_01A4);
        let want: [u8; 44] = [
            25, 0, 11, 0, // SendEvent · propagate=0 · 11 个字
            0x04, 0x00, 0x40, 0x00, // destination
            0, 0, 0, 0, // event_mask = 0
            33, 32, 0, 0, // ClientMessage · format 32 · sequence
            0x04, 0x00, 0x40, 0x00, // window
            0xA3, 0x01, 0, 0, // type = WM_PROTOCOLS
            0xA4, 0x01, 0, 0, // data[0] = WM_DELETE_WINDOW
            0, 0, 0, 0, // data[1] = CurrentTime
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // data[2..5]
        ];
        assert_eq!(got, want);
    }
}
