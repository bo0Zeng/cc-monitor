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
//! 而它不选独立二进制的**唯一**理由是当时那个进程级 SFTP 池（`sftp_pool.rs` 里的一个 `OnceLock`）
//! （换进程 = 换一份池 = 同一台远端第二条 SSH 连接）。当时那条理由仍成立、只是决定不了这道题，记成了这条路的代价。
//! 〔TL3 · 审计 F 🔴-3 订正〕**那条理由今天不成立了**：SR1b / V89 之后窗口进程一行 SFTP 都不碰（`设计/60 §2.2`），
//! SFTP 住本机常驻后端、一台机器一条 SSH 连接（`60 §4.6`），那个池也删了 ⇒ 第四节原先记的「第二条 SSH 连接」那笔代价销账。
//! （`60 §4.8` · `§8.5` 这两个旧节号今天住 `60 §2.2` · `§2.3`，对照见 `60` 附 A。）
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 🔴 二、它买到什么（两条**已经量到**的缺陷 ＋ 一条内存账）
//! ═══════════════════════════════════════════════════════════════════════
//!
//! | 缺陷 | 读数住哪 | 独立进程之后 |
//! |---|---|---|
//! | **第二次开窗必然失败**（而且此前是静默的） | `super::shell` 那一摞实景判据 | 新进程新循环，天然没有那个进程级标志 |
//! | **那 165 MiB/窗** | `真相源/107 §1` | 进程退出 = 内核收走，**真还给系统**，不必「常驻隐藏」 |
//!
//! 🔴 第一条就是「一窗一进程」的承重理由：用户裁了「关掉就销毁」，而同进程形态下
//! 「销毁」与「还能再开」只能二选一（上面第一节）。
//!
//! ## 🪦〔反订正 · 2026-09-24 · 第二波 X1〕这张表**曾经**有第三行，它的理由不成立了
//!
//! 上一版第一行是「**关掉文件窗口有可能把整个 app 一起带走**」（引 `真相源/107 §2`：
//! 优雅关窗的拆卸里有竞态，winit 在析构里 panic ⇒ 非 unwind 的 abort），并说它是三条里最要紧的。
//! X1 现打推翻了那条读数的**来路**（出处：`真相源/107 §2` 的〔反订正 · 2026-09-24〕块 ·
//! `设计/60` 的「## Xvfb 抖动」一节）：
//!
//! - Xvfb 上那条抖动来自**台架**那一锤 —— `xdotool windowclose` 在本机那一版是 `XDestroyWindow`，
//!   也就是**替窗口的主人把窗口硬拆了**；winit 之后再问 X 服务器拿到 `BadWindow`，那几处 `unwrap`
//!   每一趟都 panic，约一成落在持锁处 ⇒ 锁被毒化 ⇒ 析构里再 panic ⇒ abort。
//! - **产品里没有这一锤**：用户点 × 时窗口管理器发的是 `WM_DELETE_WINDOW`，
//!   按这条路优雅关窗 240/240 全干净；本仓生产代码也没有任何地方替窗口调 `XDestroyWindow`。
//!
//! ⇒ 「关窗可能带走整个 app」**不是**选独立进程的理由；结论「一窗一进程」照样站得住，
//! 靠的是上表第一行。⚠ 仍买不到：真窗口管理器（reparent 之后几何不同）· 真 Windows ·
//! release 档（`panic = "abort"`）—— 上游那几处 `unwrap` 原样都在，本仓只是不去触发。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 三、形状：种子走 stdin，argv 上一个字都不带
//! ═══════════════════════════════════════════════════════════════════════
//!
//! ```text
//! open_file_window（monitor 进程）
//!   ├─ ① resolve_window_bin()      ← 环境变量 CCM_FILEWIN_BIN，或 exe 旁那份
//!   ├─ ② 起进程的唯一出口          ← `crate::spawn_managed` 那个五参数形态
//!   │                                  （Hidden · Detached · Inherit，逐格理由住 [`spawn_window`]）
//!   ├─ ③ 把种子写进它的 stdin 再关掉    ← 源 + cwd（缺 = 问那台 home）+ reveal + 交接件
//!   ├─ ④ 读它 stdout 上的**一行**      ← 子进程拨回通道、问 home、列第一屏之后说「列到 N 行」或「列不出来：原话」
//!   │                                  （列不出来 ⇒ 它不开窗就退，父进程带着原话回错 —— 那条纪律一个字没动）
//!   ├─ ⑤ 等一个很短的预算，看它是不是**当场就退了**（shell::early_failure，同一条轮询）
//!   └─ ⑥ 把句柄交给一条收尸线程        ← Detached 不改父子关系 ⇒ 不 wait 就留僵尸；stdout 由它读到 EOF
//! ```
//!
//! 🔴〔MIG-3a · 主会话 09-28 裁 3〕**第一屏挪进了窗口进程。** 上一版 ① 之前还有一步「monitor 先经宿主注入的句柄
//! 问那台后端 `files-home` / `files-ls`，列出来的那一屏放进种子」—— 那是 monitor 替窗口问后端（`99 §2.1 ⑬` 待迁那一行）。
//! 今天窗口进程拨回通道之后自己问（[`first_screen`]，与窗口里之后每一次列目录同一条 `source::ask`），
//! 再在 stdout 上说**一行**（[`Ready`]）；monitor 这一侧只起进程、读那一行。
//!
//! ## 🔴 为什么种子走 stdin 而**不是**环境变量或 argv
//!
//! 三条独立的理由，前两条是硬的：
//!
//! 1. **环境变量装不下。** Linux 的 `MAX_ARG_STRLEN` 是 32 页（一条 128 KiB），
//!    而一屏的上限是 [`super::source::LS_LIMIT`] 条 —— 那个量纲上的 JSON 是**兆字节级**
//!    ⇒ `execve` 直接 `E2BIG`。它不是「大目录慢一点」，是**大目录根本开不出窗**。
//! 2. **argv 是世界可读的**（`/proc/<pid>/cmdline`），而种子里带着 `RemoteConfig`
//!    （主机名 · 用户名 · 私钥**路径**）。同一条理由已经在 `dial_host.rs::open`
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
//! - ✅〔F2 · 2026-09-24〕**后端那条通道 —— 这一格关了。** 上一版这里登记着「独立进程今天
//!   够不着后端」（进程级登记表在新进程里是空的 · 本机常驻后端一次只服务一条流 · 远端后端
//!   住在 monitor 手里那条 SSH 流里），于是列目录靠一条申报过的 SFTP 退路。
//!   现在种子里多了交接件（`chan::host::Handoff`，**只走 stdin**），[`child_main`] 先拨回
//!   monitor 的通道口（`chan::dial::dial`），拨不通就**不开窗**（`D11`：没有退路）；
//!   之后读侧与写面每一条都经那条线说 `call`（`super::source::ask`）。
//!   ⚠ 仍买不到：**订阅**（生产上零条流，`chan/host.rs` 头注）· **断线重拨**（`chan/client.rs`
//!   头注：断了就断了，之后每一件都会出声说没走通）。
//! - ✅〔TL3 · 审计 F 🔴-3 订正〕**「同一台远端被拨第二条 SSH 连接」这一笔销账了。** 上一版这里记着两条：
//!   跨机传输（上传 · 往外拖 · 读文本进编辑器）与同机复制仍走窗口进程自己那份池（当时 `60 §8.4` 未拍、后端缺 `files-copy`），
//!   而进程级的池在窗口进程里是**另一份** ⇒ 通道 / 车道预算的份数跟着窗口数涨、跨进程那一维没人在数。
//!   今天：读文本走后端 `files-read-text`、同机复制走后端 `files-copy`（F7a）；上传 / 下载是窗口说
//!   `transfer-upload` / `transfer-download` ＋ 订阅 `transfer/<id>`，monitor 的 `sftp_pool.rs` 只剩中继，SFTP 住本机常驻后端
//!   （`src/backend/dial/sftp.rs`），与其它 SSH 复用那一台一条的连接、预算按连接记（`dial/pool.rs::Budget`；`设计/60 §4.2` · `§4.6`）；
//!   往 OS 拖出去不做（V122）。⇒ 窗口进程零 SFTP、零 SSH（`60 §2.2`），预算只有一份。
//! - **「窗口真的出现在屏幕上」在本机量不到**：`XDG_SESSION_TYPE=tty`
//!   （`真相源/99 §一`）。这一侧买得到的是「那个进程起来了、而且没有当场退」；
//!   「屏幕上真有一个窗口」由 Xvfb 台架那一摞买（它自己的头注写清了它买不到的四样）。
//! - 〔订正 · F8〕**发版包里本来就有这个二进制**：tauri bundler 把每个 `[[bin]]` 装到主程序旁
//!   （真 deb ＋ 生成的 NSIS 脚本核过），[`resolve_window_bin`] 那条「exe 旁边」的分支找得到它。
//!   两向相等判据住 `K-R124-ruler.py` ⑭；真 Windows 装机那一维仍零读数。
//! - **Windows 上一趟读数都没有**（手上没有 Windows 机器）。

use crate::copy_table::copy_text;
use std::path::{Path, PathBuf};

use super::source::{Listed, Source};
// 〔P4〕种子 · 就绪那一行 · 指到窗口二进制的环境变量：monitor 与窗口进程两边对上的形状住 `filewin-contract`。
pub use filewin_contract::{
    decode_ready, decode_request, encode_ready, encode_request, OpenRequest, Ready, BIN_ENV,
};

/// 窗口进程那份二进制的**文件名主干**。
///
/// ⚠ 与本包主二进制同住一个目录（`cargo` 的 `target/<档>/` 或安装包的 exe 旁）。
pub const BIN_STEM: &str = "cc-monitor-filewin";

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
///    产物在那棵树的 `<target-dir>/<档>/` 下 —— 本仓现打是 `.build/shell/`，
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
        Err(e) => {
            return Err(copy_text(
                "rsFilewinProc.selfPath.failed",
                &[("e", &e.to_string())],
            ))
        }
    }
    Err(copy_text(
        "rsFilewinProc.bin.notFound",
        &[
            ("binStem", &BIN_STEM.to_string()),
            ("looked", &format!("{:?}", looked)),
            ("binEnv", &BIN_ENV.to_string()),
        ],
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
///   `dial_host.rs::open` 逐字同形：接管它要再起一条泵，
///   而这个子进程的 stderr 上只有「窗口为什么没立起来」那一句话。
///
/// # Errors
///
/// 二进制找不到 · `spawn` 失败 · 种子写不进去（含子进程当场死掉那一形的 `EPIPE`）。
pub fn spawn_window(req: &OpenRequest) -> Result<crate::spawn_managed::ManagedChild, String> {
    use crate::spawn_managed::{ConsolePolicy, Lifetime, StderrSink};
    let bin = resolve_window_bin()?;
    let seed = encode_request(req)?;
    // ⚠ **argv 上一个字都没有**（理由住头注 §三）；接 stdin（种子）与 stdout（就绪那一行）两根。
    //   〔MIG-3a · 09-28 裁 3〕stdout 上一版刻意**不接**（接成管子而没人读，对面一写满就卡死）；今天有人读：
    //   [`open_in_new_process`] 读那一行，之后 [`reap_later`] 把它读到 EOF ⇒ 永远有人读。
    let mut cmd = std::process::Command::new(&bin);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    let mut child = crate::spawn_managed::spawn_managed_cmd(
        &mut cmd,
        ConsolePolicy::Hidden,
        Lifetime::Detached,
        StderrSink::Inherit,
    )
    .map_err(|e| {
        copy_text(
            "rsFilewinProc.spawn.failed",
            &[("bin", &(bin.display()).to_string()), ("e", &e.to_string())],
        )
    })?;
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
        .ok_or_else(|| copy_text("rsFilewinProc.spawn.noStdin", &[]))?;
    pipe.write_all(seed.as_bytes())
        .and_then(|()| pipe.flush())
        .map_err(|e| copy_text("rsFilewinProc.seed.sendFailed", &[("e", &e.to_string())]))
}

/// 开窗没成的两种（〔MIG-3a · 09-28 裁 3〕分开：前一种是**窗口进程列不出来时说的原话**，入口原样交出去；
/// 后一种是进程这一层的事 —— 起不来 · 当场退了 · 一句话都没说 —— 入口给它套上「文件窗口没起来」）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unopened {
    /// 窗口进程说的「列不出来」那一行（home 问不到 / 目录列不出来 / 拨不回通道 / 种子读不动）。
    Said(String),
    /// 进程这一层没成。
    Process(String),
}

/// **开一个窗口** —— 生产那条路的入口。回 `(那个进程的 pid, 第一屏列到的行数)`。
///
/// 🔴 `D11`（用户 2026-09-22 裁决，横切纪律）：**这里一条退路都没有。**
/// 起不了独立进程就是错，照实报，不许「退回同进程开一个」——
/// 那条路今天已知会在第二趟必然失败（winit 一个进程只许一个事件循环）。
///
/// # 〔MIG-3a · 09-28 裁 3〕先读就绪那一行
///
/// 窗口进程列第一屏（[`first_screen`]）之后在 stdout 上说一行（[`Ready`]）：
/// 列不出来 ⇒ 它**不开窗**就退，这里带着那句原话回 [`Unopened::Said`]；一句话都没说就 EOF ⇒ 进程这一层的错。
/// ⚠ 这一读是**阻塞**的，上界由窗口进程自己那几段期限定（拨回 [`DIAL_BUDGET`] ＋ 两问各 [`FIRST_SCREEN_BUDGET`]）
/// ⇒ 入口把整件放在 `spawn_blocking` 上（不占 async worker）。
///
/// # 早失败那一跳买到 / 买不到什么
///
/// 走的是 `super::shell::early_failure` **同一条轮询**（全仓这一族只许有一处，
/// 登记在 `rust_timer_registry` 里）。它分得开的是**「当场就退了」**与**「还在跑」**：
/// - 「起不来」那一形（二进制不对 · 没有图形会话）在说完就绪之后毫秒级就退；
/// - 「起来了」那一形要占着那个进程直到用户关窗 ⇒ 预算内必然「还在跑」。
///
/// ⚠ **买不到「窗口真的出现在屏幕上」**，也买不到「慢失败」（起了循环之后才炸的
/// 那一形，预算内看不见，只落在它自己的 stderr 上）。如实登记，别读宽。
///
/// # Errors
///
/// [`spawn_window`] 的任何一档 · 窗口进程说列不出来 · 一句话没说就退了 · 说了就绪却在开窗预算内就退了。
/// 〔WF2 · WIN3 读数 J〕「判成功」之后窗口进程**不体面地退了**（退出码非零 / 被信号杀）⇒ 收尸线程把这句话交给它、由调用方出声。
/// 早退检测从此不是竞速：开窗预算之内退的照旧当场回错；预算之外退的（真机 166 ms 那一形）由它补报。
pub type LateExit = Box<dyn FnOnce(String) + Send + 'static>;

pub fn open_in_new_process(req: &OpenRequest, late: LateExit) -> Result<(u32, usize), Unopened> {
    let mut child = spawn_window(req).map_err(Unopened::Process)?;
    let pid = child.id();
    let Some(out) = child.stdout.take() else {
        if let Err(e) = child.kill() {
            tracing::warn!(
                "收不掉那个窗口进程（{e}）—— 它说的话对不上约定，可能还会开出一个没人认的窗口"
            );
        }
        reap_later(child, None, None);
        return Err(Unopened::Process(copy_text(
            "rsFilewinProc.spawn.noStdout",
            &[],
        )));
    };
    let mut out = std::io::BufReader::new(out);
    let n = match read_ready(&mut out) {
        Ok(Some(Ready::Listed(n))) => n,
        Ok(Some(Ready::Failed(said))) => {
            reap_later(child, Some(out), None);
            return Err(Unopened::Said(said));
        }
        Ok(None) => {
            let st = child.wait_for_status();
            let why = match st {
                Ok(st) => copy_text("rsFilewinProc.open.exited", &[("st", &st.to_string())]),
                Err(e) => copy_text("rsFilewinProc.open.failedWhy", &[("e", &e.to_string())]),
            };
            return Err(Unopened::Process(copy_text(
                "rsFilewinProc.open.seeStderr",
                &[("why", &why)],
            )));
        }
        Err(garbled) => {
            // 说了一句不是约定形状的话 ⇒ 两端契约漂了；它接下来会不会开窗说不准 ⇒ 收掉它，不留一个没人认的窗口。
            if let Err(e) = child.kill() {
                tracing::warn!(
                    "收不掉那个窗口进程（{e}）—— 它说的话对不上约定，可能还会开出一个没人认的窗口"
                );
            }
            reap_later(child, Some(out), None);
            return Err(Unopened::Process(garbled));
        }
    };
    if super::shell::early_failure(
        || matches!(child.try_wait(), Ok(Some(_)) | Err(_)),
        super::shell::EARLY_FAILURE_BUDGET,
    ) {
        let why = match child.try_wait() {
            Ok(Some(st)) => copy_text("rsFilewinProc.open.exited", &[("st", &st.to_string())]),
            Ok(None) => copy_text("rsFilewinProc.open.failed", &[]),
            Err(e) => copy_text("rsFilewinProc.open.failedWhy", &[("e", &e.to_string())]),
        };
        reap_later(child, Some(out), None);
        return Err(Unopened::Process(copy_text(
            "rsFilewinProc.open.seeStderr",
            &[("why", &why.to_string())],
        )));
    }
    reap_later(child, Some(out), Some(late));
    Ok((pid, n))
}

/// 读窗口进程 stdout 上的**第一行**。`Ok(None)` = 一句没说就 EOF（它退了）；`Err` = 说了但不是约定形状。
///
/// # Errors
///
/// 读失败 · 那一行解不出 [`Ready`]。
pub fn read_ready(r: &mut impl std::io::BufRead) -> Result<Option<Ready>, String> {
    let mut line = String::new();
    let n = r
        .read_line(&mut line)
        .map_err(|e| copy_text("rsFilewinProc.ready.readFailed", &[("e", &e.to_string())]))?;
    if n == 0 {
        return Ok(None);
    }
    decode_ready(&line).map(Some)
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
///
/// 〔MIG-3a · 09-28 裁 3〕它也把那根 stdout 读到 EOF（`out`）：就绪那一行之后窗口进程不该再往 stdout 写，
/// 但万一有（某个库自己往 stdout 打），没人读的管子写满就会卡住那个窗口 —— 读掉扔了，零代价。
/// 〔WF2〕`late` 只在「判成功」那一支给（别的几支当场已经回过错，不再报第二遍）。
fn reap_later(
    child: crate::spawn_managed::ManagedChild,
    out: Option<std::io::BufReader<std::process::ChildStdout>>,
    late: Option<LateExit>,
) {
    std::thread::spawn(move || {
        if let Some(mut out) = out {
            if let Err(e) = std::io::copy(&mut out, &mut std::io::sink()) {
                tracing::debug!("窗口进程的 stdout 读到一半断了（{e}）—— 只影响排空，不影响收尸");
            }
        }
        match child.wait_for_status() {
            Ok(st) if !st.success() => {
                tracing::warn!("文件窗口开出来之后又退出了：{st}");
                if let Some(say) = late {
                    let why = copy_text("rsFilewinProc.late.exited", &[("st", &st.to_string())]);
                    say(copy_text("rsFilewinProc.open.seeStderr", &[("why", &why)]));
                }
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("收不掉那个窗口进程（{e}）—— 内核里会留一条僵尸记录"),
        }
    });
}

// ═══════════════════════════════════════════════════════════════════
// 子进程那一侧
// ═══════════════════════════════════════════════════════════════════

/// 窗口进程拨回 monitor 那个通道口、出示钥匙，换一条线。**拨不通就是错**（`D11`）。
///
/// 抽成具名函数是为了让它可判：[`child_main`] 要读 stdin、要开真窗口，判据跑不动它；
/// 而「拿着交接件拨不拨得通、拨不通说什么」这一段不需要窗口。
/// 期限是这里给的（`05 §3.3.2`：说法归调用方）。
///
/// # Errors
///
/// 连不上 / 钥匙不对 / 期限内没答 —— 带着通道那一层的分层原因。
pub async fn dial_back(h: &crate::chan::host::Handoff) -> Result<super::source::Line, String> {
    use crate::chan::wire::{Budget, CancelToken};
    let budget = Budget {
        until: std::time::Instant::now() + DIAL_BUDGET,
        cancel: CancelToken::new(),
    };
    crate::chan::dial::dial(h, budget)
        .await
        .map_err(|e| copy_text("rsFilewinProc.dialBack.failed", &[("e", &e.to_string())]))
}

/// 拨回 monitor 那个通道口（回环）的期限。回环上连一次 ＋ 一来一回的认证，给得很宽。
pub const DIAL_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

/// 开窗前那两问（home · 第一屏）各自的往返上限（调用方给的期限；〔MIG-3a · 09-28 裁 3〕随那两问从 `entry.rs` 搬来）。
pub const FIRST_SCREEN_BUDGET: std::time::Duration = std::time::Duration::from_secs(20);

/// 〔MIG-3a · 09-28 裁 3〕**开窗前那一屏，窗口进程自己问**：`cwd` 缺席 ⇒ 先问那台 `files-home`；再列那个目录。
/// 回 `(起点, 那一屏)`。与窗口里之后每一次列目录同一条路（[`super::source::list_dir`] → [`super::source::ask`]）。
///
/// # Errors
///
/// home 问不到 / 解不出起点 · 目录列不出来 —— 带那一跳的原话（[`super::source::said`] 翻过的）。
pub async fn first_screen(
    line: &super::source::Line,
    source: &Source,
    cwd: Option<String>,
) -> Result<(String, Vec<Listed>), String> {
    let cwd = match cwd {
        Some(d) => d,
        None => {
            let d = super::source::ask(
                line,
                &source.origin(),
                super::source::CMD_HOME,
                &serde_json::json!({}),
                FIRST_SCREEN_BUDGET,
            )
            .await?;
            super::source::home_from_reply(&d)?
        }
    };
    let (rows, _truncated) =
        super::source::list_dir(line, source, &cwd, super::source::SortBy::default()).await?;
    Ok((cwd, rows))
}

/// 在 stdout 上说那一行。父进程已经不在了（管子断了）⇒ 只在 stderr 上记一句，不当成窗口的错。
fn say(r: &Ready) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    if let Err(e) = out
        .write_all(encode_ready(r).as_bytes())
        .and_then(|()| out.flush())
    {
        eprintln!("就绪那一行没送出去（{e}）");
    }
}

/// 说「列不出来」并回那个退出码（窗口不开）。
fn refuse(said: String, code: i32) -> i32 {
    eprintln!("{said}");
    say(&Ready::Failed(said));
    code
}

/// 种子解不出来时的退出码。
pub const EXIT_BAD_SEED: i32 = 2;
/// 窗口没立起来时的退出码。
pub const EXIT_WINDOW_FAILED: i32 = 1;
/// 〔MIG-3a · 09-28 裁 3〕第一屏列不出来（home 问不到 / 目录列不出来）时的退出码 —— 窗口没开。
pub const EXIT_NOT_LISTED: i32 = 3;

/// **窗口进程的躯体。** `[[bin]]` 那个入口只有一行，调的就是它。
///
/// 回值 = 进程退出码。四档刻意分开（`D7`：失败要显式、归因要准确）：
/// `0` 窗口开过又关了 · [`EXIT_WINDOW_FAILED`] 窗口立不起来 ·
/// [`EXIT_BAD_SEED`] 种子读不动（那是 monitor 与它之间的契约漂了，不是显示问题）·
/// [`EXIT_NOT_LISTED`] 第一屏列不出来（窗口没开）。
/// 〔MIG-3a · 09-28 裁 3〕开窗之前的每一种失败都**也**在 stdout 上说一行 [`Ready::Failed`]（父进程据此带原话回错）。
///
/// ⚠ **它把原因印在 stderr 上**，而那根 stderr 是继承来的（见 [`spawn_window`]）
/// ⇒ 从终端里起的 monitor 上看得见。装机那份 GUI app 没有 stderr 控制台
/// ⇒ 那句话今天会丢。**如实登记**：把它接进 monitor 的滚动日志要 `StderrSink::ToLog`，
/// 而那一格要一条泵、而且会把「窗口的话」与「后端的话」灌进同一个文件 —— 没顺手做。
pub fn child_main() -> i32 {
    let mut raw = String::new();
    if let Err(e) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut raw) {
        return refuse(
            copy_text("rsFilewinProc.seed.readFailed", &[("e", &e.to_string())]),
            EXIT_BAD_SEED,
        );
    }
    let req = match decode_request(&raw) {
        Ok(r) => r,
        Err(e) => return refuse(e, EXIT_BAD_SEED),
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
            return refuse(
                copy_text("rsFilewinProc.child.noRuntime", &[("e", &e.to_string())]),
                EXIT_WINDOW_FAILED,
            )
        }
    };
    // 🔴〔F2 · 2026-09-24〕**先拨通道，拨不通就别开窗**（`D11`：没有退路 ——
    //    不许「连不上就退回 SFTP 自己列」）。
    let line = match rt.block_on(dial_back(&req.handoff)) {
        Ok(c) => c,
        Err(e) => return refuse(e, EXIT_WINDOW_FAILED),
    };
    // 🔴〔MIG-3a · 09-28 裁 3〕**列不出来就别开窗**（那条纪律从 monitor 那一侧搬到这里，一个字没动）：
    //    先列第一屏，说一行给父进程；列不出来 ⇒ 说原话、退，窗口一个都不开。
    let source = Source::remote(req.origin.clone());
    let (cwd, rows) = match rt.block_on(first_screen(&line, &source, req.cwd.clone())) {
        Ok(first) => first,
        Err(e) => return refuse(e, EXIT_NOT_LISTED),
    };
    say(&Ready::Listed(rows.len()));
    let h = super::shell::open_detached_seeded(
        source,
        cwd,
        Some(rt.handle().clone()),
        Some(line),
        rows,
        req.reveal,
        req.bookmarks,
        req.machines,
        req.work_area,
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
#[path = "../../../../../tests/frontend/shell/filewin/proc_tests.rs"]
mod tests;
