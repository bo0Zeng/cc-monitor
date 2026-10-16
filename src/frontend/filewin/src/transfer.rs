//! 拖入（或「上传」选到）的一摞本机文件 —— **一条命令开一组，同名由后端在落点那台探，窗口只问一次、再并行传。**
//!
//! ```text
//! ① 开单    —— 整组一条 `transfer-upload`（带远端落点目录）；落点那台有同名而没答 ⇒ 回 `exists` 带同名清单
//!              ⇒ 冲突框问一次（[`super::batch`]，与复制 · 下载同一个框）⇒ 带 `decide` 同一组再发；照答，跳过的不开单
//! ② 并行传  —— 开了单的那几项**并行**起跑（有界）：订阅 `transfer/<id>` → 后端提交（名字与盖不盖照开单回包那一项说的）
//! ```
//!
//! 「先探 → 问 → 做」住后端一处（`control/transfer.rs` 的上传开单）；这一层不问 `files-stat`、不自己比同名。
//!
//! # 🔴 并行度**就是 [`run_drop`] 的结构**
//!
//! 判据钉的是：4 件在 `lanes = 4` 下**真的同时在飞**（`Barrier` 凑齐才放行；串行实现会超时）·
//! **阴性对照**：`lanes = 2` 下同一个 `Barrier` 必须凑不齐 ⇒ 那道闸是真的（`tests/frontend/filewin/transfer_tests.rs`）。
//!
//! # ⚠ 买不到什么（逐条写明，别读宽）
//!
//! - **没有一趟真传输的读数。** 本仓红线不许起真连接；[`upload_remote`] **经通道**说话（订阅进度 · 后端提交），
//!   判据在真回环 ＋ 合成对端上判几步的顺序与载荷。传输本体由后端 `tests/backend/control/transfer_tests.rs` 判。
//! - **`lanes` 不是真正的闸。** 真正的闸在本机常驻后端里、按连接记；本层这个数只为「别把一万条订阅一起堆起来」（[`WINDOW_TRANSFER_LANES`]）。
//! - **不做断点续传的判断**：那在传输台那一侧（暂存件的尾块对拍），本模块看不见也不该看见。
//!
//! # 🔴取消
//!
//! 取消要的不只是一颗按钮，还要**窗口说得出要取消哪一趟**：造键这一步在 [`CancelDesk::mint`]
//! （**登记表与键同一个落点**），[`launch_unless_cancelled`] 那道闸挡住还没起的。
//!
//! # 🔴窗口进程**一行 SFTP 都不碰**
//!
//! ```text
//! 开单  source::ask_batch(line, origin, "transfer-upload", {items, remote_dir, home?, decide?}) → TransferOpened{items}
//! 起跑  source::watch(line, origin, "transfer/<id>", 撤的令牌, 进度回调)                         → 传完的字节数
//! 提交  source::ask(line, origin, "files-commit-upload", {key, root, rel, on_exists?})
//! ```
//!
//! - 传输台（SFTP 连接、池、车道闸、续传、暂存区）住本机常驻后端；窗口只经通道说 `call` / `subscribe`。
//! - 上传**只写暂存区**；落进用户目录的那一下是**后端文件管理**那条提交命令（先过围栏）。
//! - 覆盖不覆盖照开单回包那一项（[`Pending::overwrite`]：人在冲突框上答了「覆盖」的才是 `true`）；
//!   其余提交不带 `on_exists`（＝ 问）⇒ 目标在开单与提交之间冒出来了，后端回 `exists`，不静默盖掉。

use copy_core::copy_text;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// 一趟拖入同时起几件（同时挂着几条进度订阅 / 同时问几件「那儿有没有东西」）。
///
/// 🔴 **这个数不在本层裁定** —— 真正的闸在本机常驻后端里、**按连接**记
/// （`src/backend/ssh-dial/pool.rs` 的 `TRANSFER_LANE_CAP` / `SESSION_CHANNEL_CAP`）；这里多挂的那几条订阅只是在那道闸前面排队。
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
    /// 提交时交给后端的覆盖策略。**只有人在冲突框上答了「覆盖」的那几件是 `true`**
    /// （开单回包那一项说的，[`Pending::landed`] 标上）；造出来时一律 `false`。
    pub overwrite: bool,
    /// 〔非 UTF-8 目录〕目标目录的原始字节（目录名不是合法 UTF-8 时才有）：探在不在与提交都按字节寻址。
    pub remote_dir_raw: Option<Vec<u8>>,
}

impl Pending {
    /// 提交的 `root`：目标目录有字节 ⇒ 按字节发；否则照旧切字符串路径。
    pub fn root_path(&self) -> cmd_wire::files::WirePath {
        match &self.remote_dir_raw {
            Some(b) => cmd_wire::files::WirePath(b.clone()),
            None => cmd_wire::files::WirePath::from(
                super::source::parent_dir(&self.remote_path).as_str(),
            ),
        }
    }

    /// 提交的那一份入参（`chunks` · `bytes` 只在块形给）。
    pub fn commit_req(
        &self,
        key: &str,
        sha256: String,
        chunked: Option<(u64, u64)>,
    ) -> cmd_wire::files::FilesCommitUploadReq {
        cmd_wire::files::FilesCommitUploadReq {
            root: self.root_path(),
            rel: cmd_wire::files::WirePath::from(self.name.as_str()),
            key: key.to_string(),
            on_exists: self
                .overwrite
                .then_some(cmd_wire::batch::OnExists::Overwrite),
            expect: cmd_wire::files::Sha256Expect {
                sha256: cmd_wire::files::Sha256Hex(sha256),
            },
            chunks: chunked.map(|c| c.0),
            bytes: chunked.map(|c| c.1),
        }
    }

    /// 探「在不在」的那条整路径（按字节）。
    pub fn path_wire(&self) -> cmd_wire::files::WirePath {
        match &self.remote_dir_raw {
            Some(b) => cmd_wire::files::WirePath(super::source::child_bytes(b, self.name.as_bytes())),
            None => cmd_wire::files::WirePath::from(self.remote_path.as_str()),
        }
    }

    /// 开单回包那一项说了怎么落：保留两份 ⇒ 换成那台另起的名字；答了覆盖 ⇒ 提交时盖。
    pub fn landed(self, it: &cmd_wire::transfer::OpenedItem) -> Pending {
        let mut p = self;
        if let Some(n) = &it.new_name {
            let name = String::from_utf8_lossy(n.bytes()).into_owned();
            p.remote_path =
                super::source::child_of(&super::source::parent_dir(&p.remote_path), &name);
            p.name = name;
        }
        p.overwrite = it.overwrite;
        p
    }

    /// 「把本机这个文件放到远端这个目录里」。
    ///
    /// 拿不到 basename（比如 `/` 或空串）⇒ `None`，**不编一个名字出来**。
    pub fn into_remote_dir(local_path: &str, remote_dir: &str) -> Option<Self> {
        let name = std::path::Path::new(local_path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())?;
        Some(Self {
            local_path: local_path.to_string(),
            remote_path: super::source::child_of(remote_dir, &name),
            name,
            overwrite: false,
            remote_dir_raw: None,
        })
    }
}

/// 一趟拖入跑完之后的读数。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DropOutcome {
    /// 同名照答跳过而没传的件数。
    pub skipped: usize,
    /// 传成功的件数。
    pub ok: usize,
    /// 传失败的那几件（名字 ＋ 那一句与复制详情）。
    pub failed: Vec<(String, super::source::Failed)>,
    /// 提交时对不上整份摘要（暂存件中间有坏块）、**从头重传了一次**的那几件（名字）。
    /// 重传成了也算在 `ok` 里，但这一句要画出来（从 0 重传并出声）。
    pub redone: Vec<String>,
}

/// 整组开单那一问的结局（[`run_drop`] 的 `open` 交回）。
#[derive(Clone, Debug, PartialEq)]
pub enum Opened {
    /// 后端逐项回了怎么落（开了单 · 照答跳过 · 没开成）。
    Items(Vec<cmd_wire::transfer::OpenedItem>),
    /// 冲突框上答了取消：一件都不传。
    Dropped,
    /// 整组没开成（那一句 ＋ 复制详情）：每一件都算没传成。
    Failed(super::source::Failed),
}

/// 一件开了单的待传：落名与盖不盖已照回包标好（[`Pending::landed`]）· 传输单 id · 暂存件的键。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ticketed {
    pub item: Pending,
    pub id: String,
    pub key: String,
}

/// 🔴 **正题**：拖入那一摞的全过程。
///
/// - `open`：整组一条开单（同名由后端在落点那台探；有同名就在这一步里问一次、带答再发）—— 恰好调一次（`FnOnce`）。
/// - `launch`：真起一件（订阅 → 提交）。开了单的那几件**并行**起（有界 `lanes`）。
pub async fn run_drop<O, OFut, L, LFut>(
    items: Vec<Pending>,
    lanes: usize,
    open: O,
    launch: L,
) -> DropOutcome
where
    O: FnOnce() -> OFut,
    OFut: Future<Output = Opened>,
    L: Fn(Ticketed) -> LFut,
    LFut: Future<Output = Result<(), super::source::Failed>>,
{
    if items.is_empty() {
        return DropOutcome::default();
    }
    let mut out = DropOutcome::default();
    let opened = match open().await {
        Opened::Items(v) => v,
        Opened::Dropped => return out,
        Opened::Failed(f) => {
            out.failed = items.into_iter().map(|p| (p.name, f.clone())).collect();
            return out;
        }
    };
    let mut go = Vec::new();
    for it in &opened {
        let Some(p) = it.key.parse::<usize>().ok().and_then(|i| items.get(i)) else {
            continue;
        };
        match (it.state, &it.id, &it.staged) {
            (cmd_wire::transfer::OpenState::Skipped, ..) => out.skipped += 1,
            (cmd_wire::transfer::OpenState::Opened, Some(id), Some(key)) => go.push(Ticketed {
                item: p.clone().landed(it),
                id: id.clone(),
                key: key.clone(),
            }),
            _ => out.failed.push((
                p.name.clone(),
                super::source::Failed {
                    code: it.code.clone(),
                    said: it.reason.clone().unwrap_or_default(),
                    detail: it.detail.clone().unwrap_or_default(),
                },
            )),
        }
    }
    let names: Vec<String> = go.iter().map(|t| t.item.name.clone()).collect();
    let results = bounded(lanes, go, |t| launch(t)).await;
    for (name, r) in names.into_iter().zip(results) {
        match r {
            Ok(()) => out.ok += 1,
            Err(e) => out.failed.push((name, e)),
        }
    }
    out
}

/// 整组开单那一问（[`run_drop`] 的 `open`）：经 [`super::batch::run`] 发 —— 有同名 ⇒ 冲突框问一次、带 `decide` 同一组再发；
/// 问得到那台后端的 `$HOME` 就带上（传输台连上之后与 SFTP 起始目录比）。
pub async fn open_group(
    line: &super::source::Line,
    origin: &super::source::Origin,
    ask: &super::batch::BatchBoard,
    board: &DropBoard,
) -> Opened {
    let call = |mut args: serde_json::Value| async move {
        if let Some(h) = board.backend_home(line, origin).await {
            args["home"] = serde_json::Value::String(h);
        }
        super::source::ask_batch(line, origin, CMD_UPLOAD, &args, OPEN_BUDGET).await
    };
    let got = super::batch::run(ask, call, || async {}).await;
    match (got, ask.phase()) {
        (Some(v), _) => match serde_json::from_value::<cmd_wire::transfer::TransferOpened>(v) {
            Ok(o) => Opened::Items(o.items),
            Err(e) => Opened::Failed(super::source::Failed {
                code: None,
                said: copy_text("rsFilewinSource.said.badReply", &[]),
                detail: e.to_string(),
            }),
        },
        (None, super::batch::Phase::Failed(f)) => Opened::Failed(f),
        (None, _) => Opened::Dropped,
    }
}

/// 有界并发地把 `f` 跑在 `items` 上，**回值按输入顺序**。
///
/// 顺序保住（`buffered`）：结局按名字配回去。
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
    stop: Arc<Mutex<comms_inward::chan::wire::CancelToken>>,
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
    pub fn stop_token(&self) -> comms_inward::chan::wire::CancelToken {
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
pub async fn launch_unless_cancelled<T, E, F, Fut>(
    desk: &CancelDesk,
    name: &str,
    go: F,
) -> Result<T, E>
where
    E: From<String>,
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    if desk.is_cancelled() {
        return Err(CANCELLED.to_string().into());
    }
    let id = desk.mint(name);
    let r = go(id.clone()).await;
    desk.done(&id);
    r
}

// ═══════════════════════════════════════════════════════════════════════
// 生产适配器：**一行自己的传输代码都没有**
// ═══════════════════════════════════════════════════════════════════════

/// 一次「那儿有没有东西」的往返上限（调用方给的期限）。
pub const PROBE_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// 开单那一条（窗口说给 monitor，monitor 补拨号请求转给本机后端的传输台）。
pub const CMD_UPLOAD: &str = <cmd_wire::transfer::TransferUploadAsk as cmd_wire::files::Call>::NAME;
/// 进度流的 `kind` 前缀（传输台那一侧 `sftp_pool::TRANSFER_KIND_PREFIX`，判据钉两份相等）。
pub const KIND_PREFIX: &str = "transfer/";
/// 提交：后端文件管理那一条（后端 `control/files_commit.rs::COMMIT_COMMANDS`）。
pub const CMD_COMMIT: &str = "files-commit-upload";
/// 开单那一趟往返的上限（调用方给的期限）：开单不搬字节，但整组的同名要在远端落点那台逐项探一遍。
pub const OPEN_BUDGET: std::time::Duration = std::time::Duration::from_secs(60);
/// 提交那一趟往返的上限：后端那一侧是围栏的几次 `canonicalize` ＋ 一次同盘改名。
pub const COMMIT_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

/// 整组开单的那一份入参（同名照 `on_exists`；缺 ⇒ 后端先探、有同名回 `exists`）。落点目录取第一件的（一摞都落在同一个目录）。
pub fn group_ask(
    items: &[Pending],
    home: Option<String>,
    on_exists: Option<cmd_wire::batch::OnExists>,
) -> cmd_wire::transfer::TransferUploadAsk {
    cmd_wire::transfer::TransferUploadAsk {
        items: items
            .iter()
            .map(|p| cmd_wire::transfer::UploadItem {
                local_path: p.local_path.clone(),
            })
            .collect(),
        remote_dir: items
            .first()
            .map(Pending::root_path)
            .unwrap_or_else(|| cmd_wire::files::WirePath::from("/")),
        home,
        on_exists,
        decide: None,
    }
}

/// 真起一件上传 —— **起跑并看 → 提交**，全经通道（单子已在整组开单那一步开好）。
///
/// 🔴 这里一行 SFTP 都没有：字节由本机后端的传输台搬进暂存区，
/// 落进用户目录的那一下是后端文件管理的 `files-commit-upload`（先过围栏）。
/// ⚠ 传输失败 ⇒ 不提交、暂存件留着（再传一次从尾块接上）；撤 ⇒ 不提交、暂存件已删。
pub async fn upload_remote(
    line: &super::source::Line,
    origin: &super::source::Origin,
    t: &Ticketed,
    board: &DropBoard,
) -> Result<(), super::source::Failed> {
    let p = &t.item;
    let first = match upload_once(line, origin, p, (&t.id, &t.key), board).await {
        // 提交时对不上整份摘要 ⇒ 远端已经删掉那份坏暂存件 ⇒ **从头重传一次**并出声（这一件另开一张单）。
        //   只重一次：第二次还对不上，多半不是一次偶发的洞（盘 / 网络在持续出错），照原话报失败，不原地打转。
        Err(Once::Stale(_)) => {
            board.note_redone(&p.name);
            match reopen(line, origin, p, board).await {
                Ok((id, key)) => upload_once(line, origin, p, (&id, &key), board).await,
                Err(f) => Err(Once::Failed(f)),
            }
        }
        other => other,
    };
    match first {
        // 连上时比出来 SFTP 起始目录不是后端的 home ⇒ 一个字节没传；这一件改走后端链路分块写（出声一次）。
        Err(Once::Mismatch(why)) => {
            board.switch_to_backend(why);
            super::chunk_upload::upload_by_chunks(line, origin, p, board).await
        }
        other => other.map_err(Once::failed),
    }
}

/// 一件另开一张单（从头重传那一支）：名字与盖不盖照这一件已定下的（同名已经答过，开单只为一张新票）。
async fn reopen(
    line: &super::source::Line,
    origin: &super::source::Origin,
    p: &Pending,
    board: &DropBoard,
) -> Result<(String, String), super::source::Failed> {
    let home = board.backend_home(line, origin).await;
    let ask = group_ask(
        std::slice::from_ref(p),
        home,
        Some(cmd_wire::batch::OnExists::Overwrite),
    );
    let opened = super::source::ask_typed(line, origin, &ask, OPEN_BUDGET).await?;
    match super::source::opened_one(opened)? {
        (id, Some(key)) => Ok((id, key)),
        (_, None) => Err(super::source::Failed::local(copy_text(
            "rsFilewinSource.said.badReply",
            &[],
        ))),
    }
}

/// 一趟上传没成的两形：提交时摘要对不上（`stale`，远端已删掉坏暂存件 ⇒ 值得从头重传）· 别的（原话）。
enum Once {
    Stale(super::source::Failed),
    Failed(super::source::Failed),
    /// 传输台连上之后比出来 SFTP 起始目录不是后端的 home（`sftp_home_mismatch`），一个字节没传。
    Mismatch(String),
}

impl Once {
    fn failed(self) -> super::source::Failed {
        match self {
            Once::Stale(f) | Once::Failed(f) => f,
            Once::Mismatch(s) => s.into(),
        }
    }
}

/// 传输台收场码：SFTP 起始目录不是那台后端的 home（后端 `control/transfer.rs::SFTP_HOME_MISMATCH`，判据钉两份相等）。
pub const SFTP_HOME_MISMATCH: &str = "sftp_home_mismatch";

/// 起跑并看 → 提交，一趟。提交带传输台交的整份摘要（`expect`），远端后端改名上位之前核它。
async fn upload_once(
    line: &super::source::Line,
    origin: &super::source::Origin,
    p: &Pending,
    (id, key): (&str, &str),
    board: &DropBoard,
) -> Result<(), Once> {
    let stop = board.cancels().stop_token();
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
    .map_err(|f| match f.code.as_deref() {
        Some(SFTP_HOME_MISMATCH) => Once::Mismatch(f.said),
        _ => Once::Failed(f),
    })?;
    let sha256 = watched
        .sha256
        .ok_or_else(|| Once::Failed(copy_text("rsFilewinTransfer.upload.noDigest", &[]).into()))?;
    super::source::ask_typed(
        line,
        origin,
        &p.commit_req(key, sha256, None),
        COMMIT_BUDGET,
    )
    .await
    .map(|_| ())
    .map_err(|f| match f.code.as_deref() {
        Some("stale") => Once::Stale(f),
        _ => Once::Failed(f),
    })
}

// ═══════════════════════════════════════════════════════════════════════
// 窗口那一侧的状态：**传到哪儿了**（同名那一问是 [`super::batch`] 的冲突框）
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
    /// 在传的那几件：名字 → (已传, 总共)。
    progress: Vec<(String, u64, u64)>,
    /// 上一趟的结果（画在窗口上，不是 `println!`）。
    last: Option<DropOutcome>,
    /// 这一趟里从头重传过的那几件（[`DropBoard::note_redone`] 记、[`DropBoard::finish`] 并进结局）。
    redone: Vec<String>,
    /// 那台后端的 `$HOME`（问过一次就记着；开单时交给传输台比 SFTP 起始目录）。
    backend_home: Option<String>,
    /// 这一窗的上传改走过后端链路分块写 ⇒ 为什么（传输台原话；跟着每一摞的那一行说一句）。
    via_backend: Option<String>,
}

impl DropBoard {
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

    /// 新开一趟用的一块新看板，只带上「这一窗」记着的两样（那台的 `$HOME` · 改走后端链路那一句），
    /// 进度、取消、结局都是新的（一趟一块：互不清掉）。
    pub fn fresh_like(&self) -> DropBoard {
        let next = DropBoard::default();
        {
            let b = self.inner.lock().unwrap();
            let mut n = next.inner.lock().unwrap();
            n.backend_home = b.backend_home.clone();
            n.via_backend = b.via_backend.clone();
        }
        *next.ctx.lock().unwrap() = self.ctx.lock().unwrap().clone();
        next
    }

    /// 这一摞合起来走了多少 `(已传, 共)`（起过的那几件；还没起的不在里面）。
    pub fn totals(&self) -> (u64, u64) {
        let b = self.inner.lock().unwrap();
        b.progress
            .iter()
            .fold((0, 0), |(g, t), (_, a, c)| (g + a, t + c))
    }

    /// 这一摞已经传完了几件（读数到顶的那几件）。
    pub fn done_count(&self) -> usize {
        let b = self.inner.lock().unwrap();
        b.progress
            .iter()
            .filter(|(_, a, c)| *c > 0 && a >= c)
            .count()
    }

    /// 这一窗的上传改走过后端链路（`Some(为什么)`）。
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

    /// 那台后端的 `$HOME`：问过就用记着的；没问过 ⇒ 问一次（问不到 ⇒ `None`，这一趟不比）。
    pub async fn backend_home(
        &self,
        line: &super::source::Line,
        origin: &super::source::Origin,
    ) -> Option<String> {
        if let Some(h) = self.inner.lock().unwrap().backend_home.clone() {
            return Some(h);
        }
        let h = super::source::home_of(line, origin, PROBE_BUDGET)
            .await
            .ok()?;
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
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/transfer_tests.rs"]
pub(crate) mod tests;
