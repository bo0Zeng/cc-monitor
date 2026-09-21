//! `24e` 第三刀：**把零流量复制接到原生窗口上**（`设计/60 §5` 第二段 ＋ `§5` 第三段第 7 步）。
//!
//! # 🔴 一、这一层**没有**复制逻辑，一行都没有
//!
//! 零流量复制那一路（`copy-data` 扩展 · 协商不到就退回中转 · 退路那句话带上实际过网
//! 字节数）**在步 23b 就落地了**，住 `sftp_pool::copy_remote_path`，秤 F3 两个方向钉着它
//! （`tests/bridge/sftp_copy_f3_tests.rs`，门禁 `f3-copy` 那一格 9 条）。
//! **这一刀只做一件事：让窗口上点得到它，并且让它的裁决被人看见。**
//!
//! ## ⚠ 为什么是经 `sftp_copy` 调下去，而不是直接调 `copy_remote_path`
//!
//! `copy_remote_path` 的第一个参数是一条**裸会话**（`crate::sftp::RawSftp`），
//! 而那条会话只能从池里借（`OriginPool::lease_raw`），拿它要先 `pool_for(origin)` ——
//! 那个函数是 `sftp_pool` **模块私有**的 ⇒ 本模块**够不着**。
//!
//! 🔴 **而这正好是对的，不是绕路。** `sftp_copy` 在 `copy_remote_path` 外面套了三样
//! 缺一不可的东西，自己拼一遍等于把三样一起丢掉：
//!
//! | 套着的 | 丢了会怎样 |
//! |---|---|
//! | `guard_write(&from)` ＋ `guard_write(&to)` **各一次** | Claude 数据源围栏没了 —— 能把正被 Claude 打开的 `jsonl` 复制走、或盖一份复制品上去 |
//! | `register_cancel(&transfer_id)` | 这一趟复制**取消不掉**（`copy_remote_path` 里那个 `cancel` 就永远是 false） |
//! | `pool.lease_raw(&cfg)`（一格车道 ＋ 一格通道预算） | `设计/60 §5.4a` 那条「6 − 4 = 2 格永远留给浏览」被拆成两份预算 |
//!
//! ⇒ **`sftp_copy` 就是那条池的入口**，而它的函数体里**直接点名** `copy_remote_path`
//! （`remote_write_registry` 那张一跳路由表逐字钉着 `("sftp_pool.rs", "sftp_copy",
//! "copy_remote_path")`，为了那条边它刻意没抽 `copy_inner`）。
//! ⇒ 本模块**禁止**出现 `copy_remote_path(` 的调用形状，判据钉着
//! （[`tests::the_real_adapter_delegates_to_the_pools_own_copy_command`]）。
//!
//! # 🔴 二、三段的顺序**就是 [`run_copy`] 的结构** —— 照 [`super::transfer::run_drop`] 办
//!
//! ```text
//! ① probe    —— 「远端已经有这个目标名了吗」
//! ② confirm  —— 要覆盖吗，**问一次**（`FnOnce` ⇒ 一半由编译器守）
//! ③ launch   —— 才动手，并把**裁决原样带回来**
//! ```
//!
//! ⚠ 与 `run_drop` **刻意不合成一个函数**，理由是**回值类型**不同，不是风格：
//! 上传那一路的 `launch` 回 `Result<(), String>`（成没成），复制这一路回
//! `Result<CopyVerdict, String>`（**成没成 ＋ 走的是哪条路**）。
//! 把它们并成一个泛型函数，要么让上传那一侧背一个永远是 `None` 的字段，
//! 要么把 `DropOutcome` 那四个字段整个换掉 —— 而 `DropOutcome` 上钉着
//! `transfer_tests` 五条相等断言。**顺序与「一次问完」这两件事照它的形状办，回值各归各。**
//!
//! # 🔴 三、退路必须在界面上出声 —— 这是**承重**的，不是锦上添花
//!
//! `设计/60 §5` 第二段逐字：「**不许静默退化成 2× 流量** —— 用户看得见『这一趟走的是慢路』」。
//! `copy_remote_path` 刻意**不回 `bool`** 而回 `CopyVerdict = Option<String>`，
//! 并把「为什么退 ＋ 实际过网了多少字节」拼进那一句话 ⇒ **静默退化在类型上就做不到**。
//!
//! 这一层于是只剩一件活：**把那句话原样摆到人眼前**，一个字都不改写、不摘要。
//! 落点是 [`outcome_notice`]（纯函数，判得动）＋ [`CopyBoard::ui`]（真画出来）。
//! ⚠ 旧面板那一侧是一条 12 秒的 info toast（`src/sftp/panel.ts::copyFile`）；
//! 窗口这一侧没有 toast，换成**画在窗口上、一直留到下一趟**的一行警告色字
//! —— 比 toast 更不容易错过，代价是它占一行。
//!
//! # ⚠ 这一刀**买不到**什么（逐条，别读宽）
//!
//! - **一趟真复制的端到端读数买不到。** 本仓红线不许起真连接
//!   ⇒ [`copy_remote`] 本机跑不到。它买得到的是**委派**（调的是池那条既有命令）。
//! - **「那句话在屏幕上没被裁掉」买不到。** 判据读的是 egui 这一帧真的交出去的
//!   galley 文本（`Galley::text()` 逐字「the full, non-elided text」）——
//!   它证明「这一帧真的把这句话拿去排版了」，**不证明**「那几个像素没被列宽切掉」。
//! - **真机上鼠标点那颗「复制」会不会触发买不到**（本机 `XDG_SESSION_TYPE=tty`，
//!   没有图形会话就没有真事件源）。判据喂的是合成事件 —— 同 `真相源/99 §9.1` 那条口径。
//! - **目录复制没做**（`copy-data` 吃的是文件句柄）· **多选复制没做**（还没有多选）
//!   · **复制到别的目录没做**（那个框只改名字，名字里不许带 `/`）。

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::sftp_pool::CopyVerdict;

use super::source::Row;

/// 行上那颗按钮的字面。**唯一住址** —— 判据按同一个常量去找它画出来的那几个字，
/// 不在判据里手抄第二份（抄一份就会漂）。
pub const COPY_LABEL: &str = "复制";

/// 这一行能不能复制。**唯一住址** —— 列表画不画那颗按钮（[`super::rows`]）
/// 与状态机接不接那一跳（[`super::shell::FileWindow::begin_copy`]），问的都是这一个函数。
///
/// 两档不能，理由各不相同：
///
/// - **目录** —— `copy-data` 吃的是**文件句柄**，目录递归不在这一层
///   （`sftp_pool::copy_remote_path` 头注逐条写着它不守什么；本机 `sftp` 客户端
///   现打同样拒：`Cannot copy non-regular file`）。
/// - **有损名** —— 非 UTF-8 文件名经库有损解码之后**寻址不到真字节**，
///   一切写操作灰置（同旧面板 `panel.ts::mkRowBtn` 的 `disabled = e.lossyName`）。
pub fn is_copyable(r: &Row) -> bool {
    !r.is_dir && !r.lossy_name
}

/// 一件待复制：**同一台远端、同一个目录**，`from` → `to`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyJob {
    pub from: String,
    pub to: String,
    /// 显示名（＝ 目标那一侧的 basename）。
    pub name: String,
}

impl CopyJob {
    /// 「把 `from` 复制成同一个目录 `dir` 里的 `new_name`」。
    ///
    /// ⚠ 远端路径**恒用 `/`** 拼（同 [`super::source::parent_dir`] 那条理由：
    /// 拿 `std::path` 切远端路径，在 Windows 上会把 `\` 也当分隔符）。
    ///
    /// 三档回 `None`，**一个都不许兜底编一个名字出来**：
    /// - 空名字；
    /// - 名字里带 `/` —— 那是「复制到别处」，本刀不做，更不许让人在一个
    ///   「改个名」的框里不小心写出一条别的路径（那会把文件放到他没在看的目录里）；
    /// - 目标算出来**就是源自己** —— `copy_remote_path` 会先写 `<to>.part`、
    ///   再删 `to`、再换名上位，`to == from` 就是「把源删了再换个名字回来」。
    pub fn beside(from: &str, dir: &str, new_name: &str) -> Option<Self> {
        let new_name = new_name.trim();
        if new_name.is_empty() || new_name.contains('/') {
            return None;
        }
        let base = dir.trim_end_matches('/');
        let to = format!("{base}/{new_name}");
        if to == from {
            return None;
        }
        Some(Self {
            from: from.to_string(),
            to,
            name: new_name.to_string(),
        })
    }

    /// 这一趟**会被盖掉**的是哪一条路径。
    ///
    /// 🔴 一行的函数，但它是[`probe_target`]唯一的取数处 —— 抽出来是为了让
    /// 「探的是目标还是源」这件事**判得动**。探成 `from` 的话：源一定存在
    /// ⇒ 每一趟都弹一次覆盖确认，而真正会被盖掉的那个目标**没人问过**。
    /// 那一形在没有真连接的机器上本来看不见（`probe_target` 跑不到），
    /// 现在它被压成一条相等断言。
    pub fn overwrite_target(&self) -> &str {
        &self.to
    }
}

/// 一趟复制跑完之后的读数。
///
/// 🔴 **`Done` 那一支把 [`CopyVerdict`] 原样背上来，不压成 `bool`。**
/// 压成 `bool` 就等于把「为什么退 ＋ 过了多少字节」在这一层丢掉，
/// 而那一句话只有 `sftp_pool` 那一层答得出（`copy_remote_path` 头注逐条写着理由）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CopyOutcome {
    /// 问过「要覆盖吗」，人答了「不覆盖」⇒ **一个字节都没动**。
    Skipped,
    /// 跑完了。
    Done {
        /// 这一趟问过人没有（＝ 目标本来就在）。
        asked: bool,
        /// `None` = 零流量走通了；`Some(说明)` = **退了路**，那句话已含实际过网字节数。
        verdict: CopyVerdict,
    },
    /// 起不来 / 半途失败，带原文。
    Failed(String),
}

/// 🔴 **正题**：一趟复制的全过程。三段的顺序就是这个函数的结构。
///
/// - `probe`：「远端已经有这个目标名了吗」。回 `true` = 已经有了（会覆盖）。
/// - `confirm`：**一次**把「要覆盖吗」交给人。⚠ 它是 `FnOnce` —— 类型上就不许被调第二次。
/// - `launch`：真起那一趟，回 `sftp_pool` 那一层的裁决。
///
/// ⚠ **不冲突就不问**（弹一个空框是噪音，不是慎重）—— 同 [`super::transfer::run_drop`]。
pub async fn run_copy<P, PFut, C, CFut, L, LFut>(
    job: CopyJob,
    probe: P,
    confirm: C,
    launch: L,
) -> CopyOutcome
where
    P: FnOnce(CopyJob) -> PFut,
    PFut: Future<Output = bool>,
    C: FnOnce(CopyJob) -> CFut,
    CFut: Future<Output = bool>,
    L: FnOnce(CopyJob) -> LFut,
    LFut: Future<Output = Result<CopyVerdict, String>>,
{
    // ── ① 问「会不会覆盖」────────────────────────────────────────────────
    let clash = probe(job.clone()).await;

    // ── ② 一次问完 ──────────────────────────────────────────────────────
    if clash && !confirm(job.clone()).await {
        return CopyOutcome::Skipped;
    }

    // ── ③ 才动手，并把裁决原样带回来 ──────────────────────────────────────
    match launch(job).await {
        Ok(verdict) => CopyOutcome::Done {
            asked: clash,
            verdict,
        },
        Err(e) => CopyOutcome::Failed(e),
    }
}

/// 一句要画在窗口上的话 ＋ 它的档位。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub text: String,
    /// 警告档（画成警告色）。退路与失败是 `true`。
    pub loud: bool,
}

/// 逐字出现在**退路**那一句里的前缀 —— 判据按它去找那句话。
///
/// ⚠ 后面跟的是 `copy_remote_path` 交上来的**原文**（含实际过网字节数），
/// 这一层一个字都不改写。
pub const SLOW_PATH_PREFIX: &str = "⚠ 这一趟走的是慢路：";

/// 🔴 **上一趟要摆到用户眼前的那句话。**
///
/// 抽成纯函数不是风格：`CopyBoard::ui` 里那几行 egui 调用判不动「说了什么」，
/// 而这一条**是**能按相等断言判的 —— 而且 [`CopyBoard::ui`] 真的走它
/// （判据一头喂这个函数、一头去 egui 这一帧画出来的文字里找同一句话）。
///
/// ⚠ **快路刻意不喊** —— 零流量是它该有的样子，不是成就（同旧面板 `copyFile` 那句注释）。
pub fn outcome_notice(o: &CopyOutcome) -> Notice {
    match o {
        CopyOutcome::Skipped => Notice {
            text: "上一趟：没覆盖，一个字节都没动".to_string(),
            loud: false,
        },
        CopyOutcome::Failed(e) => Notice {
            text: format!("复制失败：{e}"),
            loud: true,
        },
        CopyOutcome::Done {
            verdict: None,
            asked: _,
        } => Notice {
            text: "复制完成：服务端自己搬的字节，零流量".to_string(),
            loud: false,
        },
        // 🔴 承重的那一支：`why` **原样**带出来（它已经含了过网字节数）。
        CopyOutcome::Done {
            verdict: Some(why),
            asked: _,
        } => Notice {
            text: format!("{SLOW_PATH_PREFIX}{why}"),
            loud: true,
        },
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 生产适配器：**一行自己的传输代码都没有**
// ═══════════════════════════════════════════════════════════════════════

/// 「远端已经有这个目标名了吗」—— **借** [`super::transfer::probe_remote`]。
///
/// 🔴 借而不是再写一遍 `sftp_stat`，理由是**住址**：上传那一路与复制这一路问的是
/// 同一个问题（「这条远端路径上已经有东西了吗」），而那个问题有一档很容易漂 ——
/// **`stat` 失败算「不存在」还是「不可读」**。口径逐字写在 `probe_remote` 的头注里，
/// 两份实现早晚会在那一档上分岔，而分岔的后果是「无端多问一次」或
/// 「该问的没问就覆盖了」。⇒ 一份。
///
/// ⚠ 探的是[`CopyJob::overwrite_target`]（＝ `to`），不是 `from` —— 会被盖掉的是目标。
pub async fn probe_target(cfg: &crate::ssh_source::RemoteConfig, job: &CopyJob) -> bool {
    super::transfer::probe_remote(cfg, job.overwrite_target()).await
}

/// 真起一趟复制 —— 调既有命令 `sftp_pool::sftp_copy`。
///
/// 🔴 **为什么可以直接调一个 `#[tauri::command]`**：同进程（`super` 头注），
/// 它同时就是一个普通 `pub async fn` ⇒ 这里不过 IPC、不过 serde，
/// 走的是同一个进程级连接池 ⇒ 围栏、取消登记、车道/通道预算全部照旧生效
/// （逐条见本模块头注那张表）。
///
/// 🔴 **进度通道在本进程里现造**（`tauri::ipc::Channel::new` 收一个普通回调），
/// 同 [`super::transfer::upload_remote`] —— 进度直接落进 `board`，不绕一圈 webview。
///
/// 🔴〔第五刀〕`transfer_id` **由调用方给**，不在这儿造。它是取消登记表的键，
/// 造在这儿的话它从没离开过这个栈 ⇒ 窗口说不出要取消哪一趟
/// （逐字理由住 `super::transfer` 头注那一节；唯一的造键落点是
/// [`super::transfer::CancelDesk::mint`]，它同时保证「两趟不用同一个键」）。
pub async fn copy_remote(
    cfg: &crate::ssh_source::RemoteConfig,
    job: &CopyJob,
    board: &CopyBoard,
    transfer_id: &str,
) -> Result<CopyVerdict, String> {
    let name = job.name.clone();
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
    crate::sftp_pool::sftp_copy(
        cfg.clone(),
        job.from.clone(),
        job.to.clone(),
        transfer_id.to_string(),
        chan,
    )
    .await
}

// ═══════════════════════════════════════════════════════════════════════
// 窗口那一侧的状态：**问什么 · 传到哪儿了 · 上一趟走的是哪条路**
// ═══════════════════════════════════════════════════════════════════════

/// 「复制为」那个框 —— **UI 线程自己的状态**，刻意**不进** [`CopyBoard`]。
///
/// 理由：`CopyBoard` 是跨线程的（tokio 那条写、UI 线程画），而「正在输入的那个名字」
/// 从头到尾只有 UI 线程碰得到。塞进去就是把一件单线程的事摆进共享内存里
/// （同 `FileWindow::seen_rounds` 那条注释的口径）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyPrompt {
    /// 源的绝对路径（＝ 那一行的 `path`）。
    pub from: String,
    /// 源在列表上的名字（只用来说话）。
    pub src_name: String,
    /// 目标落在哪个目录（＝ 当前目录）。
    pub dir: String,
    /// 正在编辑的新名字。
    pub new_name: String,
}

impl CopyPrompt {
    /// 缺省的新名字 —— 同旧面板（`panel.ts::copyFile` 的 `${e.name}.copy`）。
    /// **别让两个面板两套手感。**
    pub fn suggest(name: &str) -> String {
        format!("{name}.copy")
    }

    pub fn for_row(dir: &str, r: &Row) -> Self {
        Self {
            from: r.path.clone(),
            src_name: r.name.clone(),
            dir: dir.to_string(),
            new_name: Self::suggest(&r.name),
        }
    }

    /// 框里那个名字变成一趟真复制。名字不合法 ⇒ `None`（调用方据此**出声**）。
    pub fn to_job(&self) -> Option<CopyJob> {
        CopyJob::beside(&self.from, &self.dir, &self.new_name)
    }
}

/// 一趟复制在窗口上的样子。**跨线程共享**（UI 线程画，tokio 那条写）。
///
/// 形状照 [`super::transfer::DropBoard`] 办，含那只「敲窗口的手」——
/// egui 只在有事发生时才画下一帧，不敲一下进度条要等用户动鼠标才跳一格
/// （`真相源/99 §9.6`：「卡住了」与「真的没在跑」在屏幕上分不开）。
#[derive(Clone, Default)]
pub struct CopyBoard {
    inner: Arc<Mutex<Board>>,
    /// 已经跑完的趟数 —— 给判据与「跑完要重列目录」一个可观测的数。
    rounds: Arc<AtomicU64>,
    ctx: Arc<Mutex<Option<egui::Context>>>,
    /// 🔴〔第五刀〕这一趟的取消台（键 ＋ 旗）。
    ///
    /// ⚠ **与上传那一摞共用同一个类型**（[`super::transfer::CancelDesk`]），不另写一份：
    /// 两份实现会在「取消之后还起不起」这一档上分岔，而那一档正是这一格的全部内容。
    desk: super::transfer::CancelDesk,
}

#[derive(Default)]
struct Board {
    /// 正摆在人面前等答复的那一件（`None` = 没在问）。
    asking: Option<CopyJob>,
    /// 答复往哪儿送。
    answer: Option<tokio::sync::oneshot::Sender<bool>>,
    /// 在跑的那一件：(名字, 已传, 总共)。
    progress: Option<(String, u64, u64)>,
    /// 上一趟的裁决（**画在窗口上**，不是 `println!`）。
    last: Option<CopyOutcome>,
}

impl CopyBoard {
    /// 摆出「要覆盖吗」，并交出「答复送哪儿」那一头。
    pub fn ask(&self, job: CopyJob) -> tokio::sync::oneshot::Receiver<bool> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut b = self.inner.lock().unwrap();
            b.asking = Some(job);
            b.answer = Some(tx);
        }
        self.poke(); // 问题要立刻画出来，别等下一次鼠标动
        rx
    }

    pub fn is_asking(&self) -> bool {
        self.inner.lock().unwrap().asking.is_some()
    }

    /// 这一趟的取消台。**同一份**（内部全是 `Arc`）。
    pub fn cancels(&self) -> super::transfer::CancelDesk {
        self.desk.clone()
    }

    /// 把窗口交给它，好让它在有事发生时敲一下。
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
        self.inner.lock().unwrap().progress = Some((name.to_string(), got, total));
        // ⚠ 锁放掉之后才敲 —— `request_repaint` 会走进 egui 自己的锁。
        self.poke();
    }

    pub fn finish(&self, outcome: CopyOutcome) {
        {
            let mut b = self.inner.lock().unwrap();
            b.progress = None;
            b.asking = None;
            b.last = Some(outcome);
        }
        self.rounds.fetch_add(1, Ordering::SeqCst);
        self.poke();
    }

    pub fn rounds(&self) -> u64 {
        self.rounds.load(Ordering::SeqCst)
    }

    pub fn last(&self) -> Option<CopyOutcome> {
        self.inner.lock().unwrap().last.clone()
    }

    /// 人点了「覆盖」/「别覆盖」—— 把答复送出去，问题收掉。
    ///
    /// 回值 = 真的送出去了（重复点第二下不会送第二次；`oneshot` 也只收一次）。
    pub fn settle(&self, overwrite: bool) -> bool {
        let mut b = self.inner.lock().unwrap();
        let Some(tx) = b.answer.take() else {
            return false;
        };
        b.asking = None;
        drop(b);
        tx.send(overwrite).is_ok()
    }

    /// 画覆盖确认框 · 进度 · **上一趟的裁决**。
    ///
    /// 🔴 最后那一段是这一刀的承重墙：退路那句话从 [`outcome_notice`] 出来，
    /// **原样**画上去。删掉它 ⇒ 复制在不支持 `copy-data` 的服务端上会静默花掉
    /// 2× 带宽，而界面上一切正常（`设计/60 §5` 第二段逐字禁止）。
    pub fn ui(&self, ui: &mut egui::Ui) {
        let (asking, progress, last) = {
            let b = self.inner.lock().unwrap();
            (b.asking.clone(), b.progress.clone(), b.last.clone())
        };
        if let Some(job) = asking {
            let mut answer: Option<bool> = None;
            egui::Modal::new(egui::Id::new("filewin-copy-overwrite")).show(ui.ctx(), |ui| {
                ui.heading(format!("远端已经有 {} 了，要覆盖吗？", job.name));
                ui.label(format!("{} → {}", job.from, job.to));
                ui.horizontal(|ui| {
                    if ui.button("覆盖").clicked() {
                        answer = Some(true);
                    }
                    if ui.button("别覆盖").clicked() {
                        answer = Some(false);
                    }
                });
            });
            if let Some(ok) = answer {
                self.settle(ok);
            }
        }
        if let Some((name, got, total)) = &progress {
            let frac = if *total > 0 {
                *got as f32 / *total as f32
            } else {
                0.0
            };
            ui.add(egui::ProgressBar::new(frac).text(format!("复制 {name} {got}/{total}")));
        }
        // 🔴〔第五刀〕取消那一颗 —— 有东西在飞才画（同 `DropBoard::ui` 那条理由）。
        if !self.desk.in_flight_ids().is_empty() {
            ui.horizontal(|ui| {
                if ui.button(super::transfer::CANCEL_LABEL).clicked() {
                    self.desk.request();
                }
                if self.desk.is_cancelled() {
                    ui.label("已经按过取消了");
                }
            });
        }
        if let Some(o) = &last {
            let n = outcome_notice(o);
            if n.loud {
                ui.colored_label(egui::Color32::from_rgb(0xE0, 0x9A, 0x20), n.text);
            } else {
                ui.label(n.text);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/copy_testing.rs"]
pub(crate) mod testing;

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/copy_tests.rs"]
mod tests;
