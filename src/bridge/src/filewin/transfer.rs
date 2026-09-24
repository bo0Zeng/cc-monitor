//! `设计/60 §5.4d`：拖入多个文件 —— **先把覆盖确认一次问完，再并行起传输。**
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
//! 1. **传输一件件来** ⇒ `sftp_pool` 那 4 条传输车道（`TRANSFER_LANE_CAP`）永远只用到 1 条。
//! 2. **问答与传输交错** ⇒ 拖 10 个文件、第 7 个才冲突，用户已经等了 6 趟传输才被弹一次窗；
//!    而那 6 趟已经在跑，答「取消」也收不回来。
//!
//! `设计/60 §5.4d` 逐字裁定**不在旧面板上修**（那块面板按 `§6.6 C` 要退役），
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
//! 判据钉的正是这个顺序与并行度，见 `tests/bridge/filewin/transfer_tests.rs`：
//! · `confirm` 被调用**恰好一次**，且它拿到的是**全部**冲突项（相等断言）
//! · 任何一次 `launch` 都排在那一次 `confirm` **之后**（时序号相比）
//! · 4 件在 `lanes = 4` 下**真的同时在飞**（`Barrier` 凑齐才放行；串行实现会超时）
//! · **阴性对照**：`lanes = 2` 下同一个 `Barrier` 必须凑不齐 ⇒ 那道闸是真的
//!
//! # ⚠ 买不到什么（逐条写明，别读宽）
//!
//! - **没有一趟真传输的读数。** 本仓红线不许起真连接
//!   （`tests/bridge/sftp_tests.rs` 逐字「跑不了真路」）⇒ [`probe_remote`] / [`upload_remote`]
//!   这两个适配器**本机跑不到**。它们买得到的是**委派**：它们调的是
//!   `sftp_pool` 那两条既有命令，不自己开连接、不自己分块写
//!   （判据 `transfer_tests::the_real_adapters_delegate_to_the_shared_pool`）。
//! - **`lanes` 不是真正的闸。** 真正的闸在池里（`lease_transfer` 的 4 条车道，
//!   借不到就 `await`，那个 `await` 就是队列 —— `设计/60 §5.4a`）。
//!   本模块这个数只为「别把一万个 future 一起堆起来」，所以它**取的就是池里那个常量**
//!   （[`crate::sftp_pool::TRANSFER_LANE_CAP`]），不另写一个字面量。
//! - **不做断点续传的判断**：那在 `sftp_upload` 里面（尾块逐字节对账），本模块看不见也不该看见。
//! - 🔴〔F1 · 波 5 · 2026-09-24〕**「上传按钮」没做，停在这里 —— 做不动，不是漏了。**
//!   今天只能把文件**拖**进窗口（[`run_drop`] 那一条）。加一颗按钮卡在三件事上，逐条：
//!   ① **一道没拍的设计题**：跨机传输走后端帧面还是走 SFTP（`设计/60 §8.4` 与 `§8.5` 要一起裁）
//!      —— 按钮背后接哪一条，取决于它；本路不替它选。
//!   ② **原生选文件框**：本包没有它的直接依赖（`rfd` 只作为 `tauri-plugin-dialog` 的传递依赖在锁文件里），
//!      加依赖要动 `src/bridge/Cargo.toml`；而且「能不能从 egui 那条线程弹出来」本机**验不了**
//!      （`XDG_SESSION_TYPE=tty`，没有图形会话）。
//!   ③ 按钮本身住 `shell.rs`，不在本路写区。
//!   ⇒ 选完文件之后要走的那一段（先问覆盖、一次问完、再并行起传输）**今天就是 [`run_drop`]**，
//!   按钮只需要把「选出来的那几条本机路径」喂给它 —— 那一跳等上面三件都有了再接。
//!
//! # 🔴〔第五刀 2026-09-21〕取消那一条（`设计/99 §4.6.4` 单记的那一格）
//!
//! 那一节逐字：「`sftp_cancel_transfer` 那一条值得单记：池子里有取消登记，
//! **窗口上没有取消按钮** ⇒ 一趟传输起来了就只能等它自己完。」
//!
//! ## 病根不是「少画一颗按钮」，是**那个键窗口说不出来**
//!
//! 池子的取消登记表以 `transfer_id` 为键（`sftp_pool::sftp_cancel_transfer` 吃的就是它），
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
//! | **已经在飞的那一趟停下来** | [`forward_cancel`] → `sftp_pool::sftp_cancel_transfer` | **委派**：那几个 id 真的被送进池子那条命令 | 🔴 **一趟真传输在池子里真的停了** —— 那要一趟真连接（本仓红线不许），而池子那一侧的取消旗由它自己的判据与秤 F4 钉着 |

use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// 一趟拖入同时起几件。
///
/// 🔴 **这个数不在本层裁定** —— 真正的闸在池里（`lease_transfer` 的车道信号量，
/// `设计/60 §5.4a` 的 `6 − 4 = 2`）。本层限并发只为「别把一万个 future 一起堆起来」，
/// 所以它**取的就是池里那个常量**，不另写一个字面量。
///
/// ⚠ 抽成一个函数是为了让「窗口那一侧用的是不是池里那个数」有一个**唯一**的落点
/// 判得动（`transfer_tests::our_concurrency_cap_is_the_pools_own_lane_count`）。
pub fn lanes() -> usize {
    crate::sftp_pool::TRANSFER_LANE_CAP
}

/// 一件待传：本机哪个文件 → 远端哪条路径。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub local_path: String,
    pub remote_path: String,
    /// 显示名（＝ 远端那一侧的 basename）。
    pub name: String,
}

impl Pending {
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
    let go: Vec<Pending> = items
        .iter()
        .zip(flags.iter())
        .filter(|(p, hit)| !**hit || allowed.contains(p))
        .map(|(p, _)| p.clone())
        .collect();
    let skipped = items.len() - go.len();

    // ── ③ 并行起传输 ─────────────────────────────────────────────────
    let results = bounded(lanes, go.clone(), |p| launch(p)).await;
    let mut out = DropOutcome {
        asked,
        skipped,
        ok: 0,
        failed: Vec::new(),
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
// 🔴〔第五刀〕取消：**那个键 ＋ 那道闸**
// ═══════════════════════════════════════════════════════════════════════

/// 一件被取消掉之后交回来的那句话。
///
/// 🔴 **回 `Err` 而不是静默跳过**：静默跳过的话 `DropOutcome` 上
/// 「取消了 3 件」与「传完了 3 件」分不开（`ok` 与 `skipped` 都装不下它），
/// 而那正是这一格要买的读数。
pub const CANCELLED: &str = "这一趟被取消了";

/// 界面上那颗取消按钮的字面。**唯一住址**（判据按同一个常量去找它画出来的字）。
///
/// ⚠ 刻意不叫「取消」两个字：那三个字在「复制为」那个框上已经有一颗
/// （`super::shell::FileWindow::copy_ui` 里那颗，意思是「别复制了」），
/// 而按内容找控件的判据分不开同名的两颗。
pub const CANCEL_LABEL: &str = "取消传输";

/// **取消台**：一摞传输的 `transfer_id` 登记 ＋ 那面「用户按过取消了」的旗。
///
/// 🔴 为什么键要在这儿造、不在适配器里造：理由逐字住本模块头注那一节
/// （造在适配器里 ⇒ 窗口说不出要取消哪一趟）。
///
/// ⚠ 它**跨线程**（UI 线程按取消，tokio 那条起传输）⇒ `Arc` + `Mutex`，
/// 与两块看板同形。刻意做成一个独立的 `Clone` 件，好让上传与复制两条路
/// **共用同一份实现**（两份实现会在「取消之后还起不起」这一档上分岔）。
#[derive(Clone, Default)]
pub struct CancelDesk {
    /// 在飞的那几趟：`(显示名, transfer_id)`。
    in_flight: Arc<Mutex<Vec<(String, String)>>>,
    /// 用户按过取消了。**一旦按下就不复位** —— 复位归下一摞（`reset`）。
    requested: Arc<AtomicBool>,
    /// 一共造过几个键 —— 给判据一个可观测的数（键唯一性靠它对账）。
    minted: Arc<AtomicU64>,
}

impl CancelDesk {
    /// 造一个这一趟的 `transfer_id` 并登记进在飞表。
    ///
    /// ⚠ 每趟现造：它是池子取消登记表的键，两趟用同一个键会互相摘掉对方的登记
    /// （`sftp_pool::register_cancel` 的注释逐字记着这一条）。
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

    /// 🔴 **按下取消**：立旗 ＋ 把在飞的那几个键**真的送进池子**。
    ///
    /// 回值 = 送出去的那几个键（判据按它做相等断言）。
    pub fn request(&self) -> Vec<String> {
        self.requested.store(true, Ordering::SeqCst);
        let ids = self.in_flight_ids();
        forward_cancel(&ids);
        ids
    }

    /// 下一摞开始：旗放下、在飞表清空。
    ///
    /// ⚠ **不清旗就起下一摞**的后果是具体的：上一摞按过取消 ⇒ 下一摞
    /// 一件都起不来，而界面上看起来是「拖进去没反应」。
    pub fn reset(&self) {
        self.requested.store(false, Ordering::SeqCst);
        self.in_flight.lock().unwrap().clear();
    }
}

/// 把取消**真的送到池子里** —— 调既有命令 `sftp_pool::sftp_cancel_transfer`。
///
/// 🔴 **为什么可以在 UI 线程上同步跑完一个 `async fn`**：那条命令的函数体里
/// **一个 `await` 都没有**（它只锁一次取消登记表、翻一个 `AtomicBool`）
/// ⇒ `block_on` 立刻返回，不阻塞画帧。
/// ⚠ 换成「往看板里塞一个 tokio `Handle`」的话，**画一帧就依赖一个运行时**，
/// 而窗口手上**不一定有**一个（`FileWindow::rt` 是 `Option`，理由住它自己那一格；
/// 从前那条理由是「本机那一侧压根没有运行时」，本机侧 2026-09-23 退役了）。
///
/// ⚠ 没注册过的 id 在池子那侧是 no-op（那条命令的注释逐字）⇒ 重复按取消无害。
pub fn forward_cancel(ids: &[String]) {
    for id in ids {
        futures::executor::block_on(crate::sftp_pool::sftp_cancel_transfer(id.clone()));
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
/// ⚠ 回值对 `T` 泛型是**刻意的**：上传那一路的 `T = ()`（成没成），
/// 复制那一路的 `T = CopyVerdict`（成没成 ＋ **走的是哪条路**）。
/// 写死成 `()` 的话，复制那一路就得另想办法把裁决带出来，
/// 而「静默退化成 2× 流量」是 `设计/60 §5` 第二段逐字禁止的那一形。
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

/// 「远端已经有这条路径了吗」—— 〔F2 · 2026-09-24〕经通道问后端 `files-stat`。
///
/// ⚠ `stat` 失败一律按「不存在」读（同旧面板 `uploadDropped` 的口径：
/// 不可读与不存在在这一步分不开，而按「存在」处理会**无端多问一次**）。
/// ⚠ 上一版问的是池子那条 `stat`（SFTP）；这一问不搬字节 ⇒ 它归后端，不归传输。
pub async fn probe_remote(
    line: &super::source::Line,
    origin: &super::source::Origin,
    remote_path: &str,
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

/// 一次「那儿有没有东西」的往返上限（调用方给的期限，`05 §3.3.2`）。
pub const PROBE_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// 真起一件上传 —— 调既有命令 `sftp_pool::sftp_upload`。
///
/// 🔴 **为什么可以直接调一个 `#[tauri::command]`**：同进程（`super` 头注），
/// 它同时就是一个普通 `pub async fn` ⇒ 这里不过 IPC、不过 serde，
/// 走的是**同一个进程级连接池**，于是 4 条传输车道、取消登记、断点续传
/// 全部照旧生效，本模块一行传输代码都不用写。
///
/// 🔴 **进度通道是在本进程里现造的**（`tauri::ipc::Channel::new`）：
/// 那个类型不只为 webview 服务，`new` 收一个普通回调。
/// ⇒ 进度直接落进 `board`，不绕一圈 webview。
///
/// 🔴〔第五刀〕`transfer_id` **由调用方给**，不在这儿造。
/// 它是池子取消登记表的键 —— 造在这儿的话它从没离开过这个栈，
/// 窗口就说不出要取消哪一趟（理由逐字住本模块头注那一节，
/// 唯一的造键落点是 [`CancelDesk::mint`]）。
pub async fn upload_remote(
    cfg: &crate::ssh_source::RemoteConfig,
    p: &Pending,
    board: &DropBoard,
    transfer_id: &str,
) -> Result<(), String> {
    let name = p.name.clone();
    let sink = board.clone();
    let chan = tauri::ipc::Channel::new(move |body| {
        // `InvokeResponseBody` 在同进程里就是那段 JSON 文本。
        if let tauri::ipc::InvokeResponseBody::Json(s) = &body {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                let got = v.get("transferred").and_then(|x| x.as_u64()).unwrap_or(0);
                let total = v.get("total").and_then(|x| x.as_u64()).unwrap_or(0);
                sink.progress(&name, got, total);
            }
        }
        Ok(())
    });
    crate::sftp_pool::sftp_upload(
        cfg.clone(),
        p.local_path.clone(),
        p.remote_path.clone(),
        transfer_id.to_string(),
        chan,
    )
    .await
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
    /// 🔴〔第五刀〕这一摞的取消台（键 ＋ 旗）。理由住本模块头注那一节。
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

    pub fn finish(&self, outcome: DropOutcome) {
        let mut b = self.inner.lock().unwrap();
        b.progress.clear();
        b.last = Some(outcome);
        drop(b);
        self.rounds.fetch_add(1, Ordering::SeqCst);
        self.poke();
    }

    pub fn rounds(&self) -> u64 {
        self.rounds.load(Ordering::SeqCst)
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
        let (asking, mut ticks, progress, last) = {
            let b = self.inner.lock().unwrap();
            (
                b.asking.clone(),
                b.ticks.clone(),
                b.progress.clone(),
                b.last.clone(),
            )
        };
        if !asking.is_empty() {
            let mut answer: Option<bool> = None;
            let mut changed = false;
            egui::Modal::new(egui::Id::new("filewin-overwrite")).show(ui.ctx(), |ui| {
                ui.heading(format!("远端已经有这 {} 个，要覆盖吗？", asking.len()));
                ui.label("⚠ 这一问只出现一次：勾完点确认，剩下的并行传。");
                for (i, p) in asking.iter().enumerate() {
                    let mut t = ticks[i];
                    if ui.checkbox(&mut t, &p.name).changed() {
                        ticks[i] = t;
                        changed = true;
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("覆盖勾上的").clicked() {
                        answer = Some(true);
                    }
                    if ui.button("全都不覆盖").clicked() {
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
        // 🔴〔第五刀〕**那颗取消按钮**（`设计/99 §4.6.4` 单记的那一格）。
        //    有东西在飞才画 —— 一颗常驻的、按下去什么都不取消的按钮比没有更坏。
        if !self.desk.in_flight_ids().is_empty() {
            ui.horizontal(|ui| {
                if ui.button(CANCEL_LABEL).clicked() {
                    self.desk.request();
                }
                if self.desk.is_cancelled() {
                    ui.label("已经按过取消了，还没起的那几件不会再起");
                }
            });
        }
        if let Some(o) = last {
            if !o.failed.is_empty() {
                ui.colored_label(
                    egui::Color32::RED,
                    format!(
                        "上一趟 {} 件失败：{}",
                        o.failed.len(),
                        o.failed
                            .iter()
                            .map(|(n, e)| format!("{n}（{e}）"))
                            .collect::<Vec<_>>()
                            .join("；")
                    ),
                );
            } else if o.ok > 0 || o.skipped > 0 {
                ui.label(format!("上一趟 {} 件传完、{} 件跳过", o.ok, o.skipped));
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/transfer_tests.rs"]
mod tests;
