//! 〔第十三刀 2026-09-23〕**进程形态：一个窗口一个进程。**
//!
//! 设计住 `调研/设计/60 §4.8`（**作废重写** —— 见下面第一节）与 `§8.5`。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 🔴 一、为什么改：两条要求在同进程形态下**不可同时成立**
//!
//! 〔完整的设计与**被它推翻的那三句**住 `设计/60 §4.8` 的作废块；这儿只留做这件事的理由。〕
//! ═══════════════════════════════════════════════════════════════════════
//!
//! 用户 2026-09-22 夜逐字裁：「**窗口生命周期就是销毁**」。
//! 而 `super::shell` 的实景台架量到的那条铁的事实是：**winit 一个进程只许一个
//! 事件循环**（那个「已经建过了」的进程级标志只在 web 平台会被清回去），
//! 而 eframe 把建好的那个缓存在**线程局部**里
//! ⇒ **同一个进程里第二次 `run_native` 必然失败**（现打：第二趟裁决 `err`、窗口数 0）。
//!
//! ⇒ 「关掉就销毁」与「关掉之后还能再打开」，在同进程形态下**只能二选一**。
//! 裁「销毁」之后就只剩一条路：**每开一个窗口，起一个独立进程。**
//!
//! ⚠ **`设计/60 §4.8` 因此作废重写**。它原话逐字是「egui 窗口与 Tauri 的 webview
//! 主窗**跑在同一个进程**……**不做独立二进制，因此也不新开一条 IPC**」，
//! 而它不选独立二进制的**唯一**理由是 `sftp_pool::pool` 那个进程级 `OnceLock`
//! （换进程 = 换一份池 = 同一台远端第二条 SSH 连接）。
//! 那条理由**今天仍然成立**，只是它不再能决定这道题 —— 它变成了这条路的**代价**，
//! 逐条记在下面第四节，别读成「那条理由被推翻了」。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 🔴 二、它买到什么（三条都是**已经量到**的缺陷，不是推论）
//! ═══════════════════════════════════════════════════════════════════════
//!
//! | 缺陷 | 读数住哪 | 独立进程之后 |
//! |---|---|---|
//! | **关掉文件窗口有可能把整个 app 一起带走** | `真相源/107 §2`：优雅关窗的拆卸过程里有竞态，winit 在**析构函数**里 panic ⇒ 「panic in a destructor during cleanup」⇒ **非 unwind 的 abort** ⇒ 整个进程死。Xvfb 上插一点延迟**每趟都红**；release 档 `panic = "abort"` 只会更狠 | 崩也只崩它自己那个进程，主界面一个字节都不知道 |
//! | **第二次开窗必然失败**（而且此前是静默的） | `super::shell` 那一摞实景判据 | 新进程新循环，天然没有那个进程级标志 |
//! | **那 165 MiB/窗** | `真相源/107 §1` | 进程退出 = 内核收走，**真还给系统**，不必「常驻隐藏」 |
//!
//! 🔴 第一条是这三条里最要紧的：它不是「这个功能不好用」，是「用这个功能会把用户
//! 正在看的会话窗口一起带走」。同进程形态下它**没有便宜的修法**（竞态在 winit 的
//! 析构里、abort 不给 unwind 的机会）；换进程之后它**结构上消失**。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 三、形状：种子走 stdin，argv 上一个字都不带
//! ═══════════════════════════════════════════════════════════════════════
//!
//! ```text
//! open_file_window（monitor 进程）
//!   ├─ ① 先真的列一趟目录（列不出来就别开窗 —— 那条纪律一个字没动，住 `super::entry`）
//!   ├─ ② resolve_window_bin()      ← 环境变量 CCM_FILEWIN_BIN，或 exe 旁那份
//!   ├─ ③ 起进程的唯一出口          ← `crate::spawn_managed` 那个五参数形态
//!   │                                  （Hidden · Detached · Inherit，逐格理由住 [`spawn_window`]）
//!   ├─ ④ 把种子写进它的 stdin 再关掉    ← 一屏行 + 源 + cwd + reveal
//!   ├─ ⑤ 等一个很短的预算，看它是不是**当场就退了**（shell::early_failure，同一条轮询）
//!   └─ ⑥ 把句柄交给一条收尸线程        ← Detached 不改父子关系 ⇒ 不 wait 就留僵尸
//! ```
//!
//! ## 🔴 为什么种子走 stdin 而**不是**环境变量或 argv
//!
//! 三条独立的理由，前两条是硬的：
//!
//! 1. **环境变量装不下。** Linux 的 `MAX_ARG_STRLEN` 是 32 页（一条 128 KiB），
//!    而一屏的上限是 [`super::source::LS_LIMIT`] 条 —— 那个量纲上的 JSON 是**兆字节级**
//!    ⇒ `execve` 直接 `E2BIG`。它不是「大目录慢一点」，是**大目录根本开不出窗**。
//! 2. **argv 是世界可读的**（`/proc/<pid>/cmdline`），而种子里带着 `RemoteConfig`
//!    （主机名 · 用户名 · 私钥**路径**）。同一条理由已经在 `ssh_source::spawn_dial_proxy`
//!    上用过一次（它把那几样从 argv 挪进了环境变量）。stdin 连 `/proc` 那一格都不给。
//! 3. 顺带：stdin 那一头**关掉就是 EOF**，「种子给完了」不需要第二种表示。
//!
//! ⚠ **代价如实记**：写种子那一下是**阻塞写**。管子只有一个内核缓冲区（典型 64 KiB），
//! 而子进程第一件事就是把 stdin 读到 EOF（[`child_main`]）⇒ 正常路径上它不堵。
//! 子进程当场死掉时这一下拿到 `EPIPE` ⇒ **那正是我们要的失败**（按 `D7` 原样报出来）。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 🔴 四、它**买不到**什么（逐条，别读宽）
//! ═══════════════════════════════════════════════════════════════════════
//!
//! - 🔴 **后端那条通道，独立进程今天够不着。这是本刀最大的一格，现打确认过：**
//!   - `inbound_client::client_for` 问的是一张**进程级**登记表，而那张表是由
//!     `ssh_source` 的 `stream_loop`（远端）与 `local_backend_host` 的接流那一段（本机）
//!     填的 ⇒ **新进程里它是空的**。
//!   - **本机那一侧连不上**，理由不是没写代码：常驻后端那个回环口**一次只服务一条流**，
//!     monitor 占着的时候第二个来客拿到的是 `stream-busy`（两侧同一个字面量，
//!     `local_backend_host` 与后端 `listen` 各持一份、有判据逐字对拍）。
//!   - **远端那一侧更远**：那台机器上的后端住在 monitor 手里那条 SSH 流里，
//!     换个进程就不是同一条流。
//!   ⇒ 于是窗口进程里 `files-ls` / `files-find` 那一族拿到的是「没有可用的控制通道」。
//!   **列目录**靠 `super::source::list_dir` 那条**已申报的退路**（旧那两条，
//!   退路不静默、界面上一行橙字）；**搜索**那一族没有退路（`设计/60 §2 档①`：
//!   SFTP 给不了搜索）⇒ 它在窗口里出声说不通，**不静默**。
//!   🔴 这一格**不是本刀能关的**：`设计/60 §8.5` 自己就登记着「独立进程要有一条
//!   自己的 IPC 才够得着那条长连接（或者由 monitor 代理转发）」，并裁「那一格与
//!   `§8.4` 的传输那一格**可以是同一条通道** ⇒ 两件事应当一起裁，别分两次」。
//!   ⇒ 照它办：**本刀不新开通道**，把这一格如实登记，等那次合裁。
//! - 🔴 **同一台远端会被拨第二条 SSH 连接** —— `§4.8.2` 拒付的那个代价，
//!   今天由这条裁决付掉了：`sftp_pool::pool` 是进程级 `OnceLock` ⇒ 窗口进程里是
//!   **另一份**池 ⇒ `设计/60 §5.4a` 那套「一条连接 · 6 通道闸 · 4 传输车道闸，
//!   `6 − 4 = 2` 格永远留给浏览」在**每个进程里各成立一次**，两份互不知情。
//!   ⚠ 它**不是**「预算变松了」（每份仍然是 6/4），是「预算的份数跟着窗口数涨」。
//!   秤 F4 钉的那个相等读数在**单个进程内**照旧成立，**跨进程那一维没人在数** ——
//!   如实登记，别读成「已经处理了」。
//! - **「窗口真的出现在屏幕上」在本机量不到**：`XDG_SESSION_TYPE=tty`
//!   （`真相源/99 §一`）。这一侧买得到的是「那个进程起来了、而且没有当场退」；
//!   「屏幕上真有一个窗口」由 Xvfb 台架那一摞买（它自己的头注写清了它买不到的四样）。
//! - **发版包里还没有这个二进制。** 它是本包的第二个 `[[bin]]`，`cargo` 编得出来，
//!   但 Tauri 的安装包只装主二进制与 `externalBin` 那几个 sidecar
//!   ⇒ 装机那份今天 [`resolve_window_bin`] 会**找不到它并出声**（`D11`：不留退路）。
//!   打包那一格要动 `tauri.sidecar.conf.json` 与发版流水线，**两处都不在本刀写区** ⇒ 报备。
//! - **Windows 上一趟读数都没有**（手上没有 Windows 机器）。

use std::path::{Path, PathBuf};

use super::source::{Row, Source};

/// 窗口进程那份二进制的**文件名主干**。
///
/// ⚠ 与本包主二进制同住一个目录（`cargo` 的 `target/<档>/` 或安装包的 exe 旁）。
pub const BIN_STEM: &str = "cc-monitor-filewin";

/// 覆盖「那份二进制在哪」的环境变量。
///
/// 🔴 它存在的理由与 `local_backend::BACKEND_BIN_ENV` / `CCM_DIAL_PROXY` 逐字同形：
/// **判据跑在 `target/<档>/deps/` 里**（`current_exe()` 给的是那条测试二进制），
/// 而 `[[bin]]` 的产物在它的上一级 ⇒ 判据必须说得出「用这一份」。
/// ⚠ 它**不是** fail-open 的开关：给了但那份文件不在，照旧是一条响亮的失败。
pub const BIN_ENV: &str = "CCM_FILEWIN_BIN";

/// 一次开窗的**全部**输入 —— 它整份过一次进程边界（走 stdin，见模块头注 §三）。
///
/// 🔴 字段与 `super::shell::FileWindow::seeded` ＋ `set_reveal` 的入参**一一对应**，
/// 刻意不多不少：多一个字段就是一处「窗口那侧能有、而开窗这条路给不了」的缝。
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct OpenRequest {
    /// 这个窗口看哪台机器。
    pub source: Source,
    /// 开在哪个目录。
    pub cwd: String,
    /// **已经列好的那一屏** —— 入口那条命令为了能出声已经列过一趟了，别打第二次往返。
    pub rows: Vec<Row>,
    /// 开窗就高亮这一行（`None` = 不高亮）。
    pub reveal: Option<String>,
}

/// 种子 → 字节。**纯函数**（判据两向对拍）。
///
/// # Errors
///
/// 序列化失败（今天只可能是 `Row`/`Source` 里出现了 serde 表达不了的东西）。
pub fn encode_request(r: &OpenRequest) -> Result<String, String> {
    serde_json::to_string(r).map_err(|e| format!("开窗种子序列化失败：{e}"))
}

/// 字节 → 种子。**纯函数**。
///
/// ⚠ 它**不许**对残缺的种子补默认值：补一个默认 cwd 出来，用户会看到一个
/// 开在别处的窗口而且没有一句话。解不出来就是错。
///
/// # Errors
///
/// 不是合法 JSON / 字段形状不对 / 空输入。
pub fn decode_request(raw: &str) -> Result<OpenRequest, String> {
    if raw.trim().is_empty() {
        return Err(format!(
            "开窗种子是空的 —— 这个二进制不是给人直接跑的，\
             它要 monitor 从 stdin 把那一屏交给它（或者由判据喂）。{BIN_ENV} 指的就是它"
        ));
    }
    serde_json::from_str(raw).map_err(|e| format!("开窗种子读不动：{e}"))
}

/// 窗口那份二进制在 `dir` 里的落点。**纯函数**（判据要在临时目录上喂它）。
pub fn window_bin_in(dir: &Path) -> PathBuf {
    dir.join(format!("{BIN_STEM}{}", std::env::consts::EXE_SUFFIX))
}

/// 那份二进制到底在哪 —— **环境变量优先，否则 exe 旁边**。
///
/// 🔴 **没有第三条路，也没有回落**（`设计/01 §5 D11`：不许留退路）。
/// 找不到就是一条响亮的失败，带上**看过哪几处**（`D7`：归因要准确 ——
/// 「窗口没起来」与「那份二进制根本不在这台机器上」是两件不同的事）。
///
/// # 🔴〔2026-09-23 订正〕**装机那份里有它，「exe 旁边」这一支就是为它准备的**
///
/// 本函数原来那条诊断（以及模块头注 §四最后一条、`Cargo.toml` 那个 `[[bin]]` 的头注）
/// 都写着「装机那份今天还没有它 —— Tauri 只装主二进制与 `externalBin` sidecar」。
/// **那句是假的**，同日现打两趟推翻（逐条读数与机制住 `tests/evidence/K-R124-ruler.py`
/// 的 ⑭ 头注，判据也住那里）：
///
/// - 真 `.deb`：`dpkg-deb -c` 现打 `usr/bin/` 三份 —— `monitor` · **`cc-monitor-filewin`**
///   · `cc-monitor-backend`，**同一个目录** ⇒ `current_exe().parent()` 就是它。
/// - NSIS 那份清单（生成出来的 `installer.nsi`）现打 `File /a "/oname=cc-monitor-filewin.exe"`，
///   与 `${MAINBINARYNAME}.exe` 同落 `$INSTDIR`。
///
/// 机制：打包器把 Cargo.toml 里**每一个** `[[bin]]` 都装上（`name == 包名` 的那个算主
/// 二进制、其余算「非主 bin」，三个打包器都把非主 bin 放在主二进制旁边）。⇒ 本函数
/// **不需要**第二种文件名形态：非主 bin 装进去用的就是 [`BIN_STEM`]，名字里**永远不带**
/// target triple（这一点与 `local_backend::local_backend_candidates` 不同 —— 那一族是
/// `externalBin` sidecar，构建期带 triple、打包时剥掉，所以那边两形都得找）。
///
/// ⚠ **仍然会走到「找不到」那一支的两种情形**（这也是下面那条诊断要说准的事）：
/// ① 用户手里是 Release 页上那个**裸 `monitor.exe` / 裸 `monitor`** —— 单文件，不带任何
///    随包二进制（同族的账住 `local_backend` 头注的 `K-R42` 那一段：后端那一份是靠
///    「自己内嵌一份再释放」补的，**窗口这一份没有那条路**）；
/// ② 开发树里第二个 bin 还没编过（`cargo build --bin cc-monitor-filewin`，
///    产物在那棵树的 `<target-dir>/<档>/` 下 —— 本仓现打是 `.build/bridge/`，
///    由仓根那份 `.cargo/config.toml` 定）。
///
/// # Errors
///
/// 两处都不是一个存在的文件。
pub fn resolve_window_bin() -> Result<PathBuf, String> {
    let mut looked: Vec<PathBuf> = Vec::new();
    if let Some(p) = std::env::var_os(BIN_ENV) {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
        looked.push(p);
    }
    match std::env::current_exe() {
        Ok(exe) => {
            let p = window_bin_in(exe.parent().unwrap_or(Path::new(".")));
            if p.is_file() {
                return Ok(p);
            }
            looked.push(p);
        }
        Err(e) => return Err(format!("问不到本进程的可执行文件路径：{e}")),
    }
    Err(format!(
        "找不到窗口那份二进制（`{BIN_STEM}`）。看过这几处：{looked:?}\n\
         ⚠ **安装包里有它**（装完与主程序同目录：Windows 是 `$INSTDIR`、deb 是 `/usr/bin`）\
         ⇒ 走到这一句多半是两种情形之一：① 用户手里是 Release 页上那个**裸单文件**\
         （它不带任何随包二进制）—— 出路是装一次安装包；② 开发树里第二个 bin 还没编\
         —— 出路是 `cargo build --bin {BIN_STEM}`（产物在那棵树的 `<target-dir>/<档>/` 下）。\
         要指一份别处的，设环境变量 {BIN_ENV}。"
    ))
}

/// 起一个窗口进程并把种子交给它。回**那个句柄**（还没做早失败判断）。
///
/// 🔴 **起进程走的是全仓那个唯一出口**（`crate::spawn_managed`），三条策略逐格的理由：
///
/// - `ConsolePolicy::Hidden` —— Windows 上不给它开控制台黑框。monitor 是
///   `windows_subsystem = "windows"` 的 GUI app，不带这个 flag 时系统会**替子进程
///   新开一个控制台窗口**，而那个框是**可关的**：用户一关，`CTRL_CLOSE_EVENT`
///   打到窗口进程 ⇒ 文件窗口被杀。egui 那个窗口自己会出来，它不需要控制台。
/// - `Lifetime::Detached` —— **绝不能是 `JobKillOnClose`**。理由与
///   `lib.rs::open_with_os` 那一处逐字同形：这条命令一返回，句柄就丢了
///   ⇒ Job 句柄一关，刚开出来的窗口当场被收掉。
///   ⚠ 顺带一格**刻意的**：用户关掉 monitor 时，已经开着的文件窗口**不跟着走** ——
///   那正是「像系统自己的文件管理器一样」那句裁决的样子。
/// - `StderrSink::Inherit` —— 诊断跟着界面进程的 stderr 走。理由与
///   `ssh_source::spawn_dial_proxy` 逐字同形：接管它要再起一条泵，
///   而这个子进程的 stderr 上只有「窗口为什么没立起来」那一句话。
///
/// # Errors
///
/// 二进制找不到 · `spawn` 失败 · 种子写不进去（含子进程当场死掉那一形的 `EPIPE`）。
pub fn spawn_window(req: &OpenRequest) -> Result<crate::spawn_managed::ManagedChild, String> {
    use crate::spawn_managed::{ConsolePolicy, Lifetime, StderrSink};
    let bin = resolve_window_bin()?;
    let seed = encode_request(req)?;
    // ⚠ **argv 上一个字都没有**（理由住头注 §三）；唯一要接的是 stdin 那一根。
    //   stdout 刻意**不接**：接成管子而没人读的那一刻，对面一写满就卡死。
    let mut cmd = std::process::Command::new(&bin);
    cmd.stdin(std::process::Stdio::piped());
    let mut child = crate::spawn_managed::spawn_managed_cmd(
        &mut cmd,
        ConsolePolicy::Hidden,
        Lifetime::Detached,
        StderrSink::Inherit,
    )
    .map_err(|e| format!("起不了窗口进程（{}）：{e}", bin.display()))?;
    write_seed(&mut child, &seed)?;
    Ok(child)
}

/// 把种子写进那条 stdin **并关掉它**（关掉 = EOF = 「给完了」）。
///
/// ⚠ 抽成具名函数是为了让 `EPIPE` 那一形判得到：子进程当场死掉时这一下失败，
/// 而那正是 `D7` 要求说准的那句话（是「种子送不进去」，不是「窗口画不出来」）。
fn write_seed(child: &mut crate::spawn_managed::ManagedChild, seed: &str) -> Result<(), String> {
    use std::io::Write;
    let mut pipe = child
        .stdin
        .take()
        .ok_or_else(|| "窗口进程没有 stdin 管子 —— 那是本模块自己设的，设丢了".to_string())?;
    pipe.write_all(seed.as_bytes())
        .and_then(|()| pipe.flush())
        .map_err(|e| {
            format!(
                "开窗种子送不进那个进程：{e}\n\
                 ⚠ 这一形多半是它**当场就退了**（管子的读端没了 ⇒ EPIPE），\
                 而不是窗口画不出来 —— 原因在它自己的 stderr 上"
            )
        })
}

/// **开一个窗口** —— 生产那条路的入口。回那个进程的 pid。
///
/// 🔴 `D11`（用户 2026-09-22 裁决，横切纪律）：**这里一条退路都没有。**
/// 起不了独立进程就是错，照实报，不许「退回同进程开一个」——
/// 那条路今天已知会在第二趟必然失败，而且关窗时可能把整个 app 带走。
///
/// # 早失败那一跳买到 / 买不到什么
///
/// 走的是 `super::shell::early_failure` **同一条轮询**（全仓这一族只许有一处，
/// 登记在 `rust_timer_registry` 里）。它分得开的是**「当场就退了」**与**「还在跑」**：
/// - 「起不来」那一形（二进制不对 · 没有图形会话 · 种子解不出）在毫秒级就退；
/// - 「起来了」那一形要占着那个进程直到用户关窗 ⇒ 预算内必然「还在跑」。
///
/// ⚠ **买不到「窗口真的出现在屏幕上」**，也买不到「慢失败」（起了循环之后才炸的
/// 那一形，预算内看不见，只落在它自己的 stderr 上）。如实登记，别读宽。
///
/// # Errors
///
/// [`spawn_window`] 的任何一档 · 那个进程在开窗预算内就退了（带上它的退出码）。
pub fn open_in_new_process(req: &OpenRequest) -> Result<u32, String> {
    let mut child = spawn_window(req)?;
    let pid = child.id();
    if super::shell::early_failure(
        || matches!(child.try_wait(), Ok(Some(_)) | Err(_)),
        super::shell::EARLY_FAILURE_BUDGET,
    ) {
        let why = match child.try_wait() {
            Ok(Some(st)) => format!("那个进程在开窗预算内就退了（{st}）"),
            Ok(None) => "那个进程既没退也没在跑 —— 这一形说明上面那一跳的判法坏了".to_string(),
            Err(e) => format!("问不到那个进程的死活：{e}"),
        };
        return Err(format!(
            "{why}。原因只落在它自己的 stderr 上（跟着界面进程的 stderr 走）"
        ));
    }
    reap_later(child);
    Ok(pid)
}

/// 收尸：`Lifetime::Detached` **不改变父子关系** ⇒ 不 `wait` 就留僵尸。
///
/// 🔴 为什么是一条线程而不是一个定时器：它要等的是**用户什么时候关那个窗口**，
/// 没有上界、也不该有 —— 一条阻塞在 `waitpid` 上的线程是零 CPU、零唤醒的等法，
/// 而任何「隔一会儿看一眼」都是一个新节拍（那要进 `rust_timer_registry` 并说清谁退役它）。
///
/// ⚠ 它**不持有窗口的任何东西**：窗口的寿命归那个进程自己，这条线程只负责
/// 在它死后把内核里那条记录收掉。monitor 先退出的话，那个进程被 init 接管、由 init 收 ——
/// 那一格不归我们（同 `local_backend_host` 的脱离那条逐字）。
fn reap_later(child: crate::spawn_managed::ManagedChild) {
    std::thread::spawn(move || {
        if let Err(e) = child.wait_for_status() {
            tracing::warn!("收不掉那个窗口进程（{e}）—— 内核里会留一条僵尸记录");
        }
    });
}

// ═══════════════════════════════════════════════════════════════════
// 子进程那一侧
// ═══════════════════════════════════════════════════════════════════

/// 种子解不出来时的退出码。
pub const EXIT_BAD_SEED: i32 = 2;
/// 窗口没立起来时的退出码。
pub const EXIT_WINDOW_FAILED: i32 = 1;

/// **窗口进程的躯体。** `[[bin]]` 那个入口只有一行，调的就是它。
///
/// 回值 = 进程退出码。三档刻意分开（`D7`：失败要显式、归因要准确）：
/// `0` 窗口开过又关了 · [`EXIT_WINDOW_FAILED`] 窗口立不起来 ·
/// [`EXIT_BAD_SEED`] 种子读不动（那是 monitor 与它之间的契约漂了，不是显示问题）。
///
/// ⚠ **它把原因印在 stderr 上**，而那根 stderr 是继承来的（见 [`spawn_window`]）
/// ⇒ 从终端里起的 monitor 上看得见。装机那份 GUI app 没有 stderr 控制台
/// ⇒ 那句话今天会丢。**如实登记**：把它接进 monitor 的滚动日志要 `StderrSink::ToLog`，
/// 而那一格要一条泵、而且会把「窗口的话」与「后端的话」灌进同一个文件 —— 没顺手做。
pub fn child_main() -> i32 {
    let mut raw = String::new();
    if let Err(e) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut raw) {
        eprintln!("读不到开窗种子：{e}");
        return EXIT_BAD_SEED;
    }
    let req = match decode_request(&raw) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return EXIT_BAD_SEED;
        }
    };
    // 🔴 这个进程里要有一个 tokio 运行时 —— 窗口那一侧的每一次列目录 / 传输 / 搜索
    //    都是 `h.spawn(async …)`。**不给它就等于开一个什么都做不了的窗口**
    //    （`FileWindow` 在 `rt: None` 时会把「这个窗口没拿到运行时」画出来，
    //     那一形是判据夹具的样子，不是生产的）。
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("起不了 tokio 运行时：{e}");
            return EXIT_WINDOW_FAILED;
        }
    };
    let h = super::shell::open_detached_seeded(
        req.source,
        req.cwd,
        Some(rt.handle().clone()),
        req.rows,
        req.reveal,
    );
    match h.join() {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => {
            eprintln!("{e}");
            EXIT_WINDOW_FAILED
        }
        Err(_) => {
            eprintln!("开窗那条线程炸了（panic）—— 原因在它自己上面那几行");
            EXIT_WINDOW_FAILED
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/proc_tests.rs"]
mod tests;
