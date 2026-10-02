//! 拖入多个文件 —— **先把覆盖确认一次问完，再并行起传输。**
//!
//! # 🔴 这条需求是从旧面板转过来的，转过来的理由在设计里，别当成新功能
//!
//! 旧面板 `src/sftp/panel.ts::uploadDropped` 是
//!
//! ```text
//! for (const localPath of paths) {
//!     ... await commands.sftp_stat(...)          // 问
//!     if (exists && !window.confirm(...)) continue;
//!     await this.runTransfer(...)                 // 传
//! }
//! ```
//!
//! 两条毛病是**同一个** `for … await` 生出来的：
//!
//! 1. **传输一件件来** ⇒ `sftp_pool` 那几格传输通道永远只用到 1 格。
//! 2. **问答与传输交错** ⇒ 拖 10 个文件、第 7 个才冲突，用户已经等了 6 趟传输才被弹一次窗；
//!    而那 6 趟已经在跑，答「取消」也收不回来。
//!
//! 裁定**不在旧面板上修**（那块面板按 `§6.6 C` 要退役），
//! 转成原生窗口的一条需求。⇒ 本模块。
//!
//! # 🔴 三段的顺序**就是 [`run_drop`] 的结构**，不是一条注释
//!
//! ```text
//! ① probe_all   —— 「远端已经有了吗」，一件一问，**并发**（有界）
//! ② confirm     —— 把**全部**冲突项一次交给人，拿回「准传的那几件」
//! ③ upload_all  —— **并行**起传输（有界）
//! ```
//!
//! 判据钉的正是这个顺序与并行度，见 `tests/frontend/filewin/transfer_tests.rs`：
//! · `confirm` 被调用**恰好一次**，且它拿到的是**全部**冲突项（相等断言）
//! · 任何一次 `launch` 都排在那一次 `confirm` **之后**（时序号相比）
//! · 4 件在 `lanes = 4` 下**真的同时在飞**（`Barrier` 凑齐才放行；串行实现会超时）
//! · **阴性对照**：`lanes = 2` 下同一个 `Barrier` 必须凑不齐 ⇒ 那道闸是真的
//!
//! # ⚠ 买不到什么（逐条写明，别读宽）
//!
//! - **没有一趟真传输的读数。** 本仓红线不许起真连接
//!   （`tests/frontend/shell/sftp_tests.rs` 逐字「跑不了真路」）。[`probe_remote`] / [`upload_remote`]
//!   今天**经通道**说话（开单 · 订阅进度 · 后端提交），不自己开连接、不碰池子
//!   （判据 `transfer_tests::the_real_adapters_speak_only_through_the_channel` 判源码，
//!   `transfer_tests::an_upload_opens_watches_then_commits_with_the_humans_answer` 在真回环 ＋ 合成对端上判三步的顺序与载荷）。
//!   传输台那一侧的读数：传输本体（真 SFTP 会话上）搬进了本机常驻后端，由后端
//!   `tests/backend/control/transfer_tests.rs` 判；monitor 的中继由 `sftp_pool_tests` / `chan::host::transfer_stream_tests` 判。
//! - **`lanes` 不是真正的闸。** 真正的闸在本机常驻后端里、按连接记（传输车道 ＋ 通道闸，
//!   借不到就 `await`，那个 `await` 就是队列）。
//!   本层这个数只为「别把一万条订阅一起堆起来」（[`WINDOW_TRANSFER_LANES`]）。
//! 池里那道 4 条的车道闸随浏览离开 SFTP 退役了，本层这个数不再是它的副本 ——
//!   判据改钉「一个窗口的一趟拖入占不满池子的通道闸」。
//! - **不做断点续传的判断**：那在传输台那一侧（暂存件的尾块对拍），本模块看不见也不该看见。
//! - ✅**「上传按钮」做了**：工具栏「上传」⇒ 问一句本机路径（一行一个）⇒ 交给 [`run_drop`]
//!   （`upload.rs`；下面三件里 ① 拍了、② 仍然没有原生选文件框、③ 挂载只占 `shell.rs` 几行）。下面是当时停下的原话：
//! - 🔴〔波 5〕**「上传按钮」没做，停在这里 —— 做不动，不是漏了。**
//!   今天只能把文件**拖**进窗口（[`run_drop`] 那一条）。加一颗按钮卡在三件事上，逐条：
//!   ① **一道没拍的设计题**：跨机传输走后端帧面还是走 SFTP（与 `§8.5` 要一起裁）
//!      —— 按钮背后接哪一条，取决于它；本路不替它选。
//!   ② **原生选文件框**：本包没有它的直接依赖（`rfd` 只作为 `tauri-plugin-dialog` 的传递依赖在锁文件里），
//!      加依赖要动 `src/frontend/shell/Cargo.toml`；而且「能不能从 egui 那条线程弹出来」本机**验不了**
//!      （`XDG_SESSION_TYPE=tty`，没有图形会话）。
//!   ③ 按钮本身住 `shell.rs`，不在本路写区。
//!   ⇒ 选完文件之后要走的那一段（先问覆盖、一次问完、再并行起传输）**今天就是 [`run_drop`]**，
//!   按钮只需要把「选出来的那几条本机路径」喂给它 —— 那一跳等上面三件都有了再接。
//!
//! # 🔴取消那一条（单记的那一格）
//!
//! 那一节逐字：「`sftp_cancel_transfer`〔散文墓碑〕 那一条值得单记：池子里有取消登记，
//! **窗口上没有取消按钮** ⇒ 一趟传输起来了就只能等它自己完。」
//!
//! ## 病根不是「少画一颗按钮」，是**那个键窗口说不出来**
//!
//! 池子的取消登记表以 `transfer_id` 为键（`sftp_pool::sftp_cancel_transfer〔散文墓碑〕` 吃的就是它），
//! 而这一刀之前那个 id 是在 [`upload_remote`] 里 `uuid::Uuid::new_v4()` **现造的**
//! ⇒ 它从没离开过那个函数的栈 ⇒ 窗口**根本说不出要取消哪一趟**。
//! 画一颗按钮解决不了这个：按钮手上没有键。
//!
//! ⇒ 把造键这一步搬到 [`CancelDesk::mint`]（**登记表与键同一个落点**），
//! [`upload_remote`] 改成**吃**一个 id。
//!
//! ## 取消有两半，各自买到什么**分开记**
//!
//! | 半 | 落点 | 买到 | 买不到 |
//! |---|---|---|---|
//! | **还没起的那几件一件都不起** | [`launch_unless_cancelled`] | 行为：`N` 件里按下取消之后，`go` 再也不被调（相等断言 ＋ 阴性对照） | —— |
//! | **已经在飞的那一趟停下来** | [`CancelDesk::stop_token`] → 停订 → 传输台撤（从前那条 `forward_cancel`〔散文墓碑〕把 id 送进池子的取消命令，随复制走后端一起删了） | 行为：按取消 ⇒ 对端看见流被丢掉、不提交 | 🔴 **一趟真传输在真 sshd 上真的停了** —— 那要一趟真连接（本仓红线不许） |

//!
//! # 🔴窗口进程**一行 SFTP 都不碰**了
//!
//! 用户逐字「**保留SFTP. 思考怎么干净**」＋「**现在只允许后端的文件管理部分写文件**」。
//! 上面几节里「调池子那条既有命令」「同一个进程级连接池」的说法**是上一版的**，今天的形状是：
//!
//! ```text
//! 开单  source::ask(line, origin, "transfer-upload", {local_path})        → {id, key}
//! 起跑  source::watch(line, origin, "transfer/<id>", 撤的令牌, 进度回调)    → 传完的字节数
//! 提交  source::ask(line, origin, "files-commit-upload", {key, root, rel, overwrite})
//! ```
//!
//! - 传输台（SFTP 连接、池、车道闸、续传、暂存区）住 **monitor**；窗口只经通道说 `call` / `subscribe`。
//! - 上传**只写暂存区**；落进用户目录的那一下是**后端文件管理**那条提交命令（先过围栏）。
//! - 覆盖不覆盖由这一侧**显式**交给后端（[`Pending::overwrite`]：人在那一问里点了「覆盖」的才是 `true`）；
//!   人没被问过的那几件一律 `false` ⇒ 目标在两问之间冒出来了，后端拒，不静默盖掉。
//! - 取消：窗口里那颗按钮 ⇒ [`CancelDesk::request`] 拨下这一摞的撤单令牌 ⇒ 每一趟的订阅停掉 ⇒
//!   传输台那一侧「停订即撤」。从前复制那一腿的池子取消（`forward_cancel`〔散文墓碑〕）
//!   随复制走后端（F7a `files-copy`，不可取消）一起删了 ⇒ 窗口进程里一个 `sftp_pool` 符号都不剩。

use copy_core::copy_text;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// 一趟拖入同时起几件（同时挂着几条进度订阅 / 同时问几件「那儿有没有东西」）。
///
/// 🔴 **这个数不在本层裁定** —— 真正的闸在本机常驻后端里、**按连接**记
/// （`src/backend/dial/pool.rs` 的 `TRANSFER_LANE_CAP` / `SESSION_CHANNEL_CAP`）；这里多挂的那几条订阅只是在那道闸前面排队。
/// 本层限并发只为「别把一万条订阅一起堆起来」。
///
/// 它原先是池里那道 4 条车道闸的副本（两份 ＋ 相等断言）；车道闸随浏览离开 SFTP
/// 退役之后改钉「占不满池子的通道闸」；池子搬进后端之后改钉两条关系
/// （`transfer_tests::one_windows_burst_fits_the_transfer_lane_and_never_fills_the_connection`）。
/// 窗口进程照旧一个 `sftp_pool` 的符号都不碰（`boundary_tests::WINDOW_SIDE` 两向钉着）。
pub const WINDOW_TRANSFER_LANES: usize = 4;

/// 一趟拖入同时起几件 —— 窗口那一侧拿这个数的**唯一**落点。
pub fn lanes() -> usize {
    WINDOW_TRANSFER_LANES
}

/// 一件待传：本机哪个文件 → 远端哪条路径。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub local_path: String,
    pub remote_path: String,
    /// 显示名（＝ 远端那一侧的 basename）。
    pub name: String,
    /// 提交时交给后端的覆盖策略。**只有人在那一问里点了「覆盖」的那几件是 `true`**
    /// （由 [`run_drop`] 在「一次问完」之后标上）；造出来时一律 `false`。
    pub overwrite: bool,
    /// 〔非 UTF-8 目录〕目标目录的原始字节（目录名不是合法 UTF-8 时才有）：探在不在与提交都按字节寻址。
    pub remote_dir_raw: Option<Vec<u8>>,
}

impl Pending {
    /// 提交的 `root`：目标目录有字节 ⇒ 按字节发；否则照旧切字符串路径。
    pub fn root_wire(&self) -> serde_json::Value {
        match &self.remote_dir_raw {
            Some(b) => super::source::wire_bytes(b),
            None => serde_json::Value::String(super::source::parent_dir(&self.remote_path)),
        }
    }

    /// 探「在不在」的那条整路径（线上那一形）。
    pub fn path_wire(&self) -> serde_json::Value {
        match &self.remote_dir_raw {
            Some(b) => {
                let mut v = b.clone();
                v.push(b'/');
                v.extend_from_slice(self.name.as_bytes());
                super::source::wire_bytes(&v)
            }
            None => serde_json::Value::String(self.remote_path.clone()),
        }
    }

    /// 「把本机这个文件放到远端这个目录里」。
    ///
    /// ⚠ 远端路径**恒用 `/`** 拼（同 [`super::source::parent_dir`] 那条理由）。
    /// 拿不到 basename（比如 `/` 或空串）⇒ `None`，**不编一个名字出来**。
    pub fn into_remote_dir(local_path: &str, remote_dir: &str) -> Option<Self> {
        let name = std::path::Path::new(local_path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())?;
        let base = remote_dir.trim_end_matches('/');
        Some(Self {
            local_path: local_path.to_string(),
            remote_path: format!("{base}/{name}"),
            name,
            overwrite: false,
            remote_dir_raw: None,
        })
    }
}

/// 一趟拖入跑完之后的读数。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DropOutcome {
    /// 一次问完的时候，摆在人面前的冲突件数。
    pub asked: usize,
    /// 人答「不覆盖」而没传的件数。
    pub skipped: usize,
    /// 传成功的件数。
    pub ok: usize,
    /// 传失败的那几件（名字 ＋ 报错原文）。
    pub failed: Vec<(String, String)>,
    /// 提交时对不上整份摘要（暂存件中间有坏块）、**从头重传了一次**的那几件（名字）。
    /// 重传成了也算在 `ok` 里，但这一句要画出来（主会话裁 09-25「从 0 重传并出声」）。
    pub redone: Vec<String>,
}

/// 🔴 **正题**：拖入那一摞的全过程。三段的顺序就是这个函数的结构。
///
/// - `probe`：一件一问「远端已经有了吗」。回 `true` = 已经有了（会覆盖）。
/// - `confirm`：**一次**拿到全部冲突项，回「这几件准传」。⚠ 它是 `FnOnce` ——
///   类型上就不许被调第二次，「一次问完」这件事有一半是编译器在守。
/// - `launch`：真起一件传输。
///
/// ⚠ **不冲突的那几件也要等这一次问完**。那是刻意的：拖 10 个文件、2 个冲突，
/// 若把 8 个不冲突的先放出去，用户答「全都别覆盖」时已经有 8 件在写对面盘了 ——
/// 「先问完再动手」这句话就只剩一半。
pub async fn run_drop<P, PFut, C, CFut, L, LFut>(
    items: Vec<Pending>,
    lanes: usize,
    probe: P,
    confirm: C,
    launch: L,
) -> DropOutcome
where
    P: Fn(Pending) -> PFut,
    PFut: Future<Output = bool>,
    C: FnOnce(Vec<Pending>) -> CFut,
    CFut: Future<Output = Vec<Pending>>,
    L: Fn(Pending) -> LFut,
    LFut: Future<Output = Result<(), String>>,
{
    if items.is_empty() {
        return DropOutcome::default();
    }
    // ── ① 问「会不会覆盖」──────────────────────────────────────────────
    let flags = bounded(lanes, items.clone(), |p| probe(p)).await;
    let clashes: Vec<Pending> = items
        .iter()
        .zip(flags.iter())
        .filter(|(_, hit)| **hit)
        .map(|(p, _)| p.clone())
        .collect();
    let asked = clashes.len();

    // ── ② 一次问完 ───────────────────────────────────────────────────
    //    没有冲突就**不问** —— 弹一个空框是噪音，不是慎重。
    let allowed: Vec<Pending> = if clashes.is_empty() {
        Vec::new()
    } else {
        confirm(clashes).await
    };

    // 准传的那一摞 = 不冲突的全部 ＋ 人点了「覆盖」的那几件。
    // 后者**标上** `overwrite` —— 提交时后端据它决定「目标在就拒」还是「整份换掉」；
    //   前者一律不标：人没被问过的，目标若在两问之间冒出来，后端拒，不替人盖。
    let go: Vec<Pending> = items
        .iter()
        .zip(flags.iter())
        .filter(|(p, hit)| !**hit || allowed.contains(p))
        .map(|(p, hit)| Pending {
            overwrite: *hit,
            ..p.clone()
        })
        .collect();
    let skipped = items.len() - go.len();

    // ── ③ 并行起传输 ─────────────────────────────────────────────────
    let results = bounded(lanes, go.clone(), |p| launch(p)).await;
    let mut out = DropOutcome {
        asked,
        skipped,
        ..Default::default()
    };
    for (p, r) in go.into_iter().zip(results.into_iter()) {
        match r {
            Ok(()) => out.ok += 1,
            Err(e) => out.failed.push((p.name, e)),
        }
    }
    out
}

/// 有界并发地把 `f` 跑在 `items` 上，**回值按输入顺序**。
///
/// 🔴 顺序必须保住：上面靠 `items.zip(flags)` 把「第几件冲突」对回去，
/// 乱序回来的话那个 `zip` 会把答案配错人 —— 那是最难看的一类错（覆盖了不该覆盖的）。
/// ⇒ 用 `buffered`（保序）而**不是** `buffer_unordered`。
///
/// ⚠ 不用 `tokio::spawn` ⇒ future 不需要 `'static`/`Send`，
/// 调用方（含判据）可以随手闭包捕获借用。
async fn bounded<T, R, F, Fut>(lanes: usize, items: Vec<T>, f: F) -> Vec<R>
where
    F: Fn(T) -> Fut,
    Fut: Future<Output = R>,
{
    use futures::stream::StreamExt;
    let n = lanes.max(1);
    futures::stream::iter(items.into_iter().map(f))
        .buffered(n)
        .collect::<Vec<R>>()
        .await
}

// ═══════════════════════════════════════════════════════════════════════
// 🔴取消：**那个键 ＋ 那道闸**
// ═══════════════════════════════════════════════════════════════════════

/// 一件被取消掉之后交回来的那句话。
///
/// 🔴 **回 `Err` 而不是静默跳过**：静默跳过的话 `DropOutcome` 上
/// 「取消了 3 件」与「传完了 3 件」分不开（`ok` 与 `skipped` 都装不下它），
/// 而那正是这一格要买的读数。
pub static CANCELLED: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinTransfer.cancelled.message", &[]));

/// 界面上那颗取消按钮的字面。**唯一住址**（判据按同一个常量去找它画出来的字）。
///
/// ⚠ 刻意不叫「取消」两个字：那三个字在「复制为」那个框上已经有一颗
/// （`super::shell::FileWindow::copy_ui` 里那颗，意思是「别复制了」），
/// 而按内容找控件的判据分不开同名的两颗。
pub static CANCEL_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinTransfer.label.cancel", &[]));

/// **取消台**：一摞传输的 `transfer_id` 登记 ＋ 那面「用户按过取消了」的旗。
///
/// 🔴 为什么键要在这儿造、不在适配器里造：理由逐字住本模块头注那一节
/// （造在适配器里 ⇒ 窗口说不出要取消哪一趟）。
///
/// ⚠ 它**跨线程**（UI 线程按取消，tokio 那条起传输）⇒ `Arc` + `Mutex`，
/// 与两块看板同形。刻意做成一个独立的 `Clone` 件，好让上传与复制两条路
/// **共用同一份实现**（两份实现会在「取消之后还起不起」这一档上分岔）。
/// 复制那一路换到后端之后取消不掉，已不用它；今天只有上传与往外拖。
#[derive(Clone, Default)]
pub struct CancelDesk {
    /// 在飞的那几趟：`(显示名, transfer_id)`。
    in_flight: Arc<Mutex<Vec<(String, String)>>>,
    /// 用户按过取消了。**一旦按下就不复位** —— 复位归下一摞（`reset`）。
    requested: Arc<AtomicBool>,
    /// 一共造过几个键 —— 给判据一个可观测的数（键唯一性靠它对账）。
    minted: Arc<AtomicU64>,
    /// 这一摞的**撤单令牌**：经通道起的每一趟（上传 / 下载）都盯着它，
    /// 拨下 ⇒ 那一趟的订阅停掉 ⇒ 传输台那一侧「停订即撤」。下一摞换一枚新的（[`CancelDesk::reset`]）。
    stop: Arc<Mutex<chan_core::chan::wire::CancelToken>>,
}

impl CancelDesk {
    /// 造一个这一趟的 `transfer_id` 并登记进在飞表。
    ///
    /// ⚠ 每趟现造：它是池子取消登记表的键，两趟用同一个键会互相摘掉对方的登记
    /// （从前池子那张按 id 的取消登记表的注释逐字记着这一条；那张表随老 Tauri 传输一路删了，
    /// 这一趟今天是停订即撤，键仍要唯一 —— 它是本层在飞表的键）。
    pub fn mint(&self, name: &str) -> String {
        let id = format!("filewin-{}", uuid::Uuid::new_v4());
        self.in_flight
            .lock()
            .unwrap()
            .push((name.to_string(), id.clone()));
        self.minted.fetch_add(1, Ordering::SeqCst);
        id
    }

    /// 这一趟收场了，从在飞表里摘掉。
    pub fn done(&self, id: &str) {
        self.in_flight.lock().unwrap().retain(|(_, i)| i != id);
    }

    /// 还在飞的那几个键（按登记顺序）。
    pub fn in_flight_ids(&self) -> Vec<String> {
        self.in_flight
            .lock()
            .unwrap()
            .iter()
            .map(|(_, i)| i.clone())
            .collect()
    }

    /// 还在飞的那几趟叫什么（画在按钮旁边）。
    pub fn in_flight_names(&self) -> Vec<String> {
        self.in_flight
            .lock()
            .unwrap()
            .iter()
            .map(|(n, _)| n.clone())
            .collect()
    }

    /// 一共造过几个键。
    pub fn minted(&self) -> u64 {
        self.minted.load(Ordering::SeqCst)
    }

    pub fn is_cancelled(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }

    /// 这一摞此刻的撤单令牌（经通道起的每一趟拿它去盯）。
    pub fn stop_token(&self) -> chan_core::chan::wire::CancelToken {
        self.stop.lock().unwrap().clone()
    }

    /// 🔴 **按下取消**：立旗 ＋ 把在飞的那几个键**真的送进池子**。
    ///
    /// 回值 = 送出去的那几个键（判据按它做相等断言）。
    pub fn request(&self) -> Vec<String> {
        self.requested.store(true, Ordering::SeqCst);
        self.stop_token().cancel();
        self.in_flight_ids()
    }

    /// 下一摞开始：旗放下、在飞表清空。
    ///
    /// ⚠ **不清旗就起下一摞**的后果是具体的：上一摞按过取消 ⇒ 下一摞
    /// 一件都起不来，而界面上看起来是「拖进去没反应」。
    pub fn reset(&self) {
        self.requested.store(false, Ordering::SeqCst);
        self.in_flight.lock().unwrap().clear();
        *self.stop.lock().unwrap() = Default::default();
    }
}

/// 🔴 **那道闸**：取消按下之后，**还没起的那几件一件都不起**。
///
/// 它同时是**造键的唯一落点** —— 两件事合在一个函数里是刻意的：
/// 「这一趟该不该起」与「这一趟的键叫什么」分在两处的话，会长出
/// 「键造了但闸没看」（起了一趟取消不掉的）或「闸看了但键没登记」
/// （按了取消而这一趟不在表里）两种半成品，而两种在屏幕上都像「取消不管用」。
///
/// - 取消过 ⇒ `go` **一次都不调**，回 `Err(CANCELLED)`。
/// - 没取消过 ⇒ 造键、登记、调 `go`、收场时摘掉登记。
///
/// ⚠ 回值对 `T` 泛型：第五刀时复制那一路也走这道闸（`T` 是它那一层的裁决）；
/// 复制换到后端、取消不掉之后不再走这里，今天只有 `T = ()` 那一路。
pub async fn launch_unless_cancelled<T, F, Fut>(
    desk: &CancelDesk,
    name: &str,
    go: F,
) -> Result<T, String>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    if desk.is_cancelled() {
        return Err(CANCELLED.to_string());
    }
    let id = desk.mint(name);
    let r = go(id.clone()).await;
    desk.done(&id);
    r
}

// ═══════════════════════════════════════════════════════════════════════
// 生产适配器：**一行自己的传输代码都没有**
// ═══════════════════════════════════════════════════════════════════════

/// 「远端已经有这条路径了吗」—— 经通道问后端 `files-stat`。
///
/// ⚠ `stat` 失败一律按「不存在」读（同旧面板 `uploadDropped` 的口径：
/// 不可读与不存在在这一步分不开，而按「存在」处理会**无端多问一次**）。
/// ⚠ 上一版问的是池子那条 `stat`（SFTP）；这一问不搬字节 ⇒ 它归后端，不归传输。
pub async fn probe_remote(
    line: &super::source::Line,
    origin: &super::source::Origin,
    remote_path: &str,
) -> bool {
    probe_remote_at(
        line,
        origin,
        serde_json::Value::String(remote_path.to_string()),
    )
    .await
}

/// 〔有损名全寻址〕同 [`probe_remote`]（同一个口径：`stat` 失败算「不在」），路径由调用方给线上那一形（字符串或 `{"b16": …}`）。
pub async fn probe_remote_at(
    line: &super::source::Line,
    origin: &super::source::Origin,
    remote_path: serde_json::Value,
) -> bool {
    super::source::ask(
        line,
        origin,
        "files-stat",
        &serde_json::json!({ "path": remote_path }),
        PROBE_BUDGET,
    )
    .await
    .is_ok()
}

/// 一次「那儿有没有东西」的往返上限（调用方给的期限）。
pub const PROBE_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// 开单：上传（传输台那一侧 `sftp_pool::TRANSFER_UPLOAD`，判据钉两份相等）。
pub const OP_UPLOAD: &str = "transfer-upload";
/// 进度流的 `kind` 前缀（传输台那一侧 `sftp_pool::TRANSFER_KIND_PREFIX`，判据钉两份相等）。
pub const KIND_PREFIX: &str = "transfer/";
/// 提交：后端文件管理那一条（后端 `control/files_commit.rs::COMMIT_COMMANDS`）。
pub const CMD_COMMIT: &str = "files-commit-upload";
/// 开单那一趟往返的上限（调用方给的期限）。开单只登记、不搬字节。
pub const OPEN_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);
/// 提交那一趟往返的上限：后端那一侧是围栏的几次 `canonicalize` ＋ 一次同盘改名。
pub const COMMIT_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

/// 从开单的应答里取一个字符串字段（缺了 ⇒ 契约不符，照实说）。
pub(crate) fn field(v: &serde_json::Value, _cmd: &str, key: &str) -> Result<String, String> {
    v.get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            copy_text(
                "rsFilewinTransfer.reply.missingField",
                &[("key", &key.to_string())],
            )
        })
}

/// 真起一件上传 —— **开单 → 起跑并看 → 提交**，三步全经通道。
///
/// 🔴 这里一行 SFTP 都没有：字节由 monitor 里的传输台搬进暂存区，
/// 落进用户目录的那一下是后端文件管理的 `files-commit-upload`（先过围栏）。
/// ⚠ 传输失败 ⇒ 不提交、暂存件留着（重拖一次从尾块接上）；撤 ⇒ 不提交、暂存件已删。
pub async fn upload_remote(
    line: &super::source::Line,
    origin: &super::source::Origin,
    p: &Pending,
    board: &DropBoard,
) -> Result<(), String> {
    // 这一窗已经改走后端链路 ⇒ 不再问 SFTP。
    if board.via_backend().is_some() {
        return super::chunk_upload::upload_by_chunks(line, origin, p, board).await;
    }
    let home = board.backend_home(line, origin).await;
    let home = home.as_deref();
    let first = match upload_once(line, origin, p, board, home).await {
        // 提交时对不上整份摘要 ⇒ 远端已经删掉那份坏暂存件 ⇒ **从头重传一次**并出声。
        //   只重一次：第二次还对不上，多半不是一次偶发的洞（盘 / 网络在持续出错），照原话报失败，不原地打转。
        Err(Once::Stale(_)) => {
            board.note_redone(&p.name);
            upload_once(line, origin, p, board, home).await
        }
        other => other,
    };
    match first {
        // 连上时比出来 SFTP 起始目录不是后端的 home ⇒ 一个字节没传；这一件起改走后端链路分块写（出声一次）。
        Err(Once::Mismatch(why)) => {
            board.switch_to_backend(why);
            super::chunk_upload::upload_by_chunks(line, origin, p, board).await
        }
        other => other.map_err(Once::said),
    }
}

/// 一趟上传没成的两形：提交时摘要对不上（`stale`，远端已删掉坏暂存件 ⇒ 值得从头重传）· 别的（原话）。
enum Once {
    Stale(String),
    Failed(String),
    /// 传输台连上之后比出来 SFTP 起始目录不是后端的 home（`sftp_home_mismatch`），一个字节没传。
    Mismatch(String),
}

impl Once {
    fn said(self) -> String {
        match self {
            Once::Stale(s) | Once::Failed(s) | Once::Mismatch(s) => s,
        }
    }
}

/// 传输台收场码：SFTP 起始目录不是那台后端的 home（后端 `control/transfer.rs::SFTP_HOME_MISMATCH`，判据钉两份相等）。
pub const SFTP_HOME_MISMATCH: &str = "sftp_home_mismatch";

/// 开单 → 起跑并看 → 提交，一趟。提交带传输台交的整份摘要（`expect`），远端后端改名上位之前核它。
async fn upload_once(
    line: &super::source::Line,
    origin: &super::source::Origin,
    p: &Pending,
    board: &DropBoard,
    home: Option<&str>,
) -> Result<(), Once> {
    let stop = board.cancels().stop_token();
    // 问得到后端的 `$HOME` 就带上：传输台连上之后与 SFTP 起始目录比（不一致 ⇒ 一个字节不写）。
    let mut args = serde_json::json!({ "local_path": p.local_path });
    if let Some(h) = home {
        args["home"] = serde_json::Value::String(h.to_string());
    }
    let opened = super::source::ask(line, origin, OP_UPLOAD, &args, OPEN_BUDGET)
        .await
        .map_err(Once::Failed)?;
    let id = field(&opened, OP_UPLOAD, "id").map_err(Once::Failed)?;
    let key = field(&opened, OP_UPLOAD, "key").map_err(Once::Failed)?;
    // 记下暂存件的键：跨机复制半路失败时，由它去那台机器上把暂存件删掉。
    board.note_staged(&key);
    let name = p.name.clone();
    let sink = board.clone();
    let watched = super::source::watch_coded(
        line,
        origin,
        &format!("{KIND_PREFIX}{id}"),
        &stop,
        |got, total| sink.progress(&name, got, total),
    )
    .await
    .map_err(|(code, said)| match code.as_deref() {
        Some(SFTP_HOME_MISMATCH) => Once::Mismatch(said),
        _ => Once::Failed(said),
    })?;
    let sha256 = watched
        .sha256
        .ok_or_else(|| Once::Failed(copy_text("rsFilewinTransfer.upload.noDigest", &[])))?;
    super::source::ask_coded(
        line,
        origin,
        CMD_COMMIT,
        &serde_json::json!({
            "key": key,
            "root": p.root_wire(),
            "rel": super::source::remote_basename(&p.remote_path),
            "overwrite": p.overwrite,
            "expect": { "sha256": sha256 },
        }),
        COMMIT_BUDGET,
    )
    .await
    .map(|_| ())
    .map_err(|f| match f.code.as_deref() {
        Some("stale") => Once::Stale(f.said),
        _ => Once::Failed(f.said),
    })
}

// ═══════════════════════════════════════════════════════════════════════
// 窗口那一侧的状态：**问什么 · 传到哪儿了**
// ═══════════════════════════════════════════════════════════════════════

/// 一摞拖入在窗口上的样子。**跨线程共享**（UI 线程画，tokio 那条写）。
#[derive(Clone, Default)]
pub struct DropBoard {
    inner: Arc<Mutex<Board>>,
    /// 已经跑完的趟数 —— 给判据与诊断一个可观测的数。
    rounds: Arc<AtomicU64>,
    /// 🔴 **拿来敲窗口的那只手。**
    ///
    /// 进度是从 tokio 那条线程写进来的，而 egui **只在有事发生时才画下一帧** ——
    /// 不敲一下，进度条要等到用户下次动鼠标才跳一格（看起来就是「卡住了」）。
    /// ⚠ `Option`：判据里没有窗口，那时它就是 `None`，`progress` 照常记数。
    ctx: Arc<Mutex<Option<egui::Context>>>,
    /// 🔴这一摞的取消台（键 ＋ 旗）。理由住本模块头注那一节。
    desk: CancelDesk,
}

#[derive(Default)]
struct Board {
    /// 正摆在人面前等答复的那几件（空 = 没在问）。
    asking: Vec<Pending>,
    /// 人勾了哪几件（与 `asking` 同长）。
    ticks: Vec<bool>,
    /// 答复往哪儿送。
    answer: Option<tokio::sync::oneshot::Sender<Vec<Pending>>>,
    /// 在传的那几件：名字 → (已传, 总共)。
    progress: Vec<(String, u64, u64)>,
    /// 上一趟的结果（画在窗口上，不是 `println!`）。
    last: Option<DropOutcome>,
    /// 这一趟里从头重传过的那几件（[`DropBoard::note_redone`] 记、[`DropBoard::finish`] 并进结局）。
    redone: Vec<String>,
    /// 那台后端的 `$HOME`（问过一次就记着；开单时交给传输台比 SFTP 起始目录）。
    backend_home: Option<String>,
    /// 这一窗的上传改走后端链路分块写了 ⇒ 为什么（传输台原话；出声一次，之后不再问 SFTP）。
    via_backend: Option<String>,
    /// 开过单的暂存件键（[`DropBoard::note_staged`]）。
    staged: Vec<String>,
}

impl DropBoard {
    /// 摆出问题，并交出「答复送哪儿」那一头。
    pub fn ask(&self, items: Vec<Pending>) -> tokio::sync::oneshot::Receiver<Vec<Pending>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut b = self.inner.lock().unwrap();
            b.ticks = vec![true; items.len()]; // 缺省勾上：用户拖过来就是想传
            b.asking = items;
            b.answer = Some(tx);
        }
        self.poke(); // 问题要立刻画出来，别等下一次鼠标动
        rx
    }

    pub fn is_asking(&self) -> bool {
        !self.inner.lock().unwrap().asking.is_empty()
    }

    /// 这一摞的取消台。**同一份**（`CancelDesk` 内部全是 `Arc`）。
    pub fn cancels(&self) -> CancelDesk {
        self.desk.clone()
    }

    /// 把窗口交给它，好让它在进度动的时候敲一下。
    pub fn attach(&self, ctx: Option<egui::Context>) {
        *self.ctx.lock().unwrap() = ctx;
    }

    /// 敲一下窗口：「有新东西了，画下一帧」。没有窗口就什么都不做。
    pub fn poke(&self) {
        if let Some(c) = self.ctx.lock().unwrap().as_ref() {
            c.request_repaint();
        }
    }

    pub fn progress(&self, name: &str, got: u64, total: u64) {
        {
            let mut b = self.inner.lock().unwrap();
            match b.progress.iter_mut().find(|(n, ..)| n == name) {
                Some(slot) => {
                    slot.1 = got;
                    slot.2 = total;
                }
                None => b.progress.push((name.to_string(), got, total)),
            }
        }
        // ⚠ 锁放掉之后才敲 —— `request_repaint` 会走进 egui 自己的锁，
        //   两把锁嵌着拿是死锁的常规做法。
        self.poke();
    }

    pub fn finish(&self, mut outcome: DropOutcome) {
        let mut b = self.inner.lock().unwrap();
        b.progress.clear();
        outcome.redone.append(&mut b.redone);
        b.last = Some(outcome);
        drop(b);
        self.rounds.fetch_add(1, Ordering::SeqCst);
        self.poke();
    }

    pub fn rounds(&self) -> u64 {
        self.rounds.load(Ordering::SeqCst)
    }

    /// 这一窗的上传是不是已经改走后端链路（`Some(为什么)`）。
    pub fn via_backend(&self) -> Option<String> {
        self.inner.lock().unwrap().via_backend.clone()
    }

    /// 改走后端链路（记下原因，界面上画一行；只记第一次的原因）。
    pub fn switch_to_backend(&self, why: String) {
        self.inner.lock().unwrap().via_backend.get_or_insert(why);
        self.poke();
    }

    /// 这一件在传的进度（`(已传, 总共)`；不在传 ⇒ `None`）。
    pub fn seen(&self, name: &str) -> Option<(u64, u64)> {
        self.inner
            .lock()
            .unwrap()
            .progress
            .iter()
            .find(|(n, ..)| n == name)
            .map(|(_, g, t)| (*g, *t))
    }

    /// 记一份开过单的暂存件键（SFTP 那条路）。
    pub fn note_staged(&self, key: &str) {
        self.inner.lock().unwrap().staged.push(key.to_string());
    }

    /// 开过单的暂存件键（跨机复制半路失败时逐个删掉）；取走即清。
    pub fn take_staged(&self) -> Vec<String> {
        std::mem::take(&mut self.inner.lock().unwrap().staged)
    }

    /// 换一台目标机器之前：记着的 home / 改走后端链路那一句 / 暂存件键 / 进度都清掉（它们说的是上一台）。
    pub fn reset_target(&self) {
        let mut b = self.inner.lock().unwrap();
        b.backend_home = None;
        b.via_backend = None;
        b.staged.clear();
        b.progress.clear();
    }

    /// 那台后端的 `$HOME`：问过就用记着的；没问过 ⇒ 问一次（问不到 ⇒ `None`，这一趟不比，照旧走 SFTP）。
    pub async fn backend_home(
        &self,
        line: &super::source::Line,
        origin: &super::source::Origin,
    ) -> Option<String> {
        if let Some(h) = self.inner.lock().unwrap().backend_home.clone() {
            return Some(h);
        }
        let d = super::source::ask(
            line,
            origin,
            "files-home",
            &serde_json::json!({}),
            PROBE_BUDGET,
        )
        .await
        .ok()?;
        let h = super::source::home_from_reply(&d).ok()?;
        self.inner.lock().unwrap().backend_home = Some(h.clone());
        Some(h)
    }

    /// 记一件「提交时对不上整份摘要、已从头重传」（这一趟收场时并进结局，画出来）。
    pub fn note_redone(&self, name: &str) {
        self.inner.lock().unwrap().redone.push(name.to_string());
    }

    pub fn last(&self) -> Option<DropOutcome> {
        self.inner.lock().unwrap().last.clone()
    }

    /// 人点了「确认」/「全部不覆盖」—— 把答复送出去，问题收掉。
    ///
    /// 回值 = 真的送出去了（重复点第二下不会送第二次；`oneshot` 也只收一次）。
    pub fn settle(&self, overwrite: bool) -> bool {
        let mut b = self.inner.lock().unwrap();
        let Some(tx) = b.answer.take() else {
            return false;
        };
        let allowed: Vec<Pending> = if overwrite {
            b.asking
                .iter()
                .zip(b.ticks.iter())
                .filter(|(_, t)| **t)
                .map(|(p, _)| p.clone())
                .collect()
        } else {
            Vec::new()
        };
        b.asking.clear();
        b.ticks.clear();
        drop(b);
        tx.send(allowed).is_ok()
    }

    /// 画确认框与进度。**模态** —— 有问题在等的时候，列表那边不接受点击。
    pub fn ui(&self, ui: &mut egui::Ui) {
        let (asking, mut ticks, progress, last, via) = {
            let b = self.inner.lock().unwrap();
            (
                b.asking.clone(),
                b.ticks.clone(),
                b.progress.clone(),
                b.last.clone(),
                b.via_backend.clone(),
            )
        };
        // 改走后端链路那一句（一窗一次；之后的上传都走那条，不再逐件说）。
        if let Some(why) = via {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                copy_text("rsFilewinChunkUpload.route.switched", &[("why", &why)]),
            );
        }
        if !asking.is_empty() {
            let mut answer: Option<bool> = None;
            let mut changed = false;
            egui::Modal::new(egui::Id::new("filewin-overwrite")).show(ui.ctx(), |ui| {
                ui.heading(copy_text(
                    "rsFilewinTransfer.ui.askOverwrite",
                    &[("n", &(asking.len()).to_string())],
                ));
                ui.label(&copy_text("rsFilewinTransfer.ui.askOnce", &[]));
                for (i, p) in asking.iter().enumerate() {
                    let mut t = ticks[i];
                    if ui.checkbox(&mut t, &p.name).changed() {
                        ticks[i] = t;
                        changed = true;
                    }
                }
                ui.horizontal(|ui| {
                    if ui
                        .button(&copy_text("rsFilewinTransfer.ui.overwriteChecked", &[]))
                        .clicked()
                    {
                        answer = Some(true);
                    }
                    if ui
                        .button(&copy_text("rsFilewinTransfer.ui.overwriteNone", &[]))
                        .clicked()
                    {
                        answer = Some(false);
                    }
                });
            });
            if changed {
                self.inner.lock().unwrap().ticks = ticks;
            }
            if let Some(ok) = answer {
                self.settle(ok);
            }
        }
        for (name, got, total) in &progress {
            let frac = if *total > 0 {
                *got as f32 / *total as f32
            } else {
                0.0
            };
            ui.add(egui::ProgressBar::new(frac).text(format!("{name} {got}/{total}")));
        }
        // 🔴**那颗取消按钮**（单记的那一格）。
        //    有东西在飞才画 —— 一颗常驻的、按下去什么都不取消的按钮比没有更坏。
        if !self.desk.in_flight_ids().is_empty() {
            ui.horizontal(|ui| {
                if ui.button(CANCEL_LABEL.as_str()).clicked() {
                    self.desk.request();
                }
                if self.desk.is_cancelled() {
                    ui.label(&copy_text("rsFilewinTransfer.ui.cancelling", &[]));
                }
            });
        }
        if let Some(o) = last {
            // 从头重传过的那几件：成没成都要说（一次坏块 = 那一趟的续传本钱白花了，而且可能是网络 / 盘在出错）。
            if !o.redone.is_empty() {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    copy_text(
                        "rsFilewinTransfer.ui.redone",
                        &[(
                            "names",
                            &o.redone
                                .join(&copy_text("rsFilewinTransfer.ui.failedSep", &[])),
                        )],
                    ),
                );
            }
            if !o.failed.is_empty() {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    copy_text(
                        "rsFilewinTransfer.ui.failed",
                        &[
                            ("n", &(o.failed.len()).to_string()),
                            (
                                "failed",
                                &(o.failed
                                    .iter()
                                    .map(|(n, e)| format!("{n}（{e}）"))
                                    .collect::<Vec<_>>()
                                    .join(&copy_text("rsFilewinTransfer.ui.failedSep", &[])))
                                .to_string(),
                            ),
                        ],
                    ),
                );
            } else if o.ok > 0 || o.skipped > 0 {
                ui.label(copy_text(
                    "rsFilewinTransfer.ui.done",
                    &[
                        ("ok", &o.ok.to_string()),
                        ("skipped", &o.skipped.to_string()),
                    ],
                ));
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/transfer_tests.rs"]
pub(crate) mod tests;
