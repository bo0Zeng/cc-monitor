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

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
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
// 生产适配器：**一行自己的传输代码都没有**
// ═══════════════════════════════════════════════════════════════════════

/// 「远端已经有这条路径了吗」—— 调既有命令 `sftp_pool::sftp_stat`。
///
/// ⚠ `stat` 失败一律按「不存在」读（同旧面板 `uploadDropped` 的口径：
/// 不可读与不存在在这一步分不开，而按「存在」处理会**无端多问一次**）。
pub async fn probe_remote(cfg: &crate::ssh_source::RemoteConfig, remote_path: &str) -> bool {
    crate::sftp_pool::sftp_stat(cfg.clone(), remote_path.to_string())
        .await
        .is_ok()
}

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
pub async fn upload_remote(
    cfg: &crate::ssh_source::RemoteConfig,
    p: &Pending,
    board: &DropBoard,
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
        format!("filewin-{}", uuid::Uuid::new_v4()),
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
        let mut b = self.inner.lock().unwrap();
        b.ticks = vec![true; items.len()]; // 缺省勾上：用户拖过来就是想传
        b.asking = items;
        b.answer = Some(tx);
        rx
    }

    pub fn is_asking(&self) -> bool {
        !self.inner.lock().unwrap().asking.is_empty()
    }

    pub fn progress(&self, name: &str, got: u64, total: u64) {
        let mut b = self.inner.lock().unwrap();
        match b.progress.iter_mut().find(|(n, ..)| n == name) {
            Some(slot) => {
                slot.1 = got;
                slot.2 = total;
            }
            None => b.progress.push((name.to_string(), got, total)),
        }
    }

    pub fn finish(&self, outcome: DropOutcome) {
        let mut b = self.inner.lock().unwrap();
        b.progress.clear();
        b.last = Some(outcome);
        drop(b);
        self.rounds.fetch_add(1, Ordering::SeqCst);
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
