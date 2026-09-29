//! `24e` 第八刀：**往外拖** —— 把远端那一行拉到本机（`sftp_download`）。
//!
//! # 🔴 一、为什么不是一个原生「保存」对话框
//!
//! 老面板走的是 Tauri 的 `saveDialog`（`src/sftp/panel.ts::download`）。
//! 这个窗口是 **egui，不是 webview** ⇒ 那条路不能照搬。
//!
//! 底下那个 crate（`rfd 0.16.0`）其实**已经在锁文件里**（`tauri-plugin-dialog` 带进来的），
//! 所以「加个依赖」不是成本所在。**真正的障碍是量不到**：原生对话框能不能从 egui
//! 那条线程弹出来，要一个图形会话才验得了，而本机 `XDG_SESSION_TYPE=tty`
//! （`真相源/99 §一`）⇒ 在这台机器上**永远量不到**。
//!
//! ⇒ 走窗口自己那套「问一句」（同「复制为」「改名」那个形状）：全程可判，零新依赖。
//! ⚠ 这不是说原生对话框是错的答案 —— 它是一个**今天验不了**的答案。
//! 登记在 `设计/99`，等有真桌面那天再量。
//!
//! # 🔴〔F7c · 第三波 09-24〕窗口不碰 SFTP：下载经通道开单、订阅进度（[`pull_one`]）
//!
//! 下面第二、三节说的「池子那一层」今天住 **monitor 里的传输台**（`sftp_pool::transfer_call` 开单、本机常驻后端起跑）；
//! 窗口这一侧照旧只把那句拒绝原样带给用户。
//! 〔FN1 · V119〕本机落点那道 Claude 数据围栏（开单时一道、后端起跑时一道）**两道都删了**（用户「文件管理器全部都可以改.
//! 不需要任何围栏」）；后端那一侧只剩路径解析（绝对路径 · 有文件名 · 父目录在盘上）。第二节是历史。
//!
//! # 二、〔FN1 · 历史〕围栏在池子那一层，不在这儿
//!
//! 本机落点那道围栏 2026-09-21 补在了 `sftp_pool::sftp_download` 的第一行
//! （`guard_write(&local_path)`）—— 逐条来历住那段注释（三张账首尾相接推诿、
//! 链子末端一句假话）。⇒ **本模块不自己判一遍**：判定只有一个家。
//!
//! ⚠ 本模块**会**把那句拒绝原样带给用户（[`Outcome::Failed`]），
//! 但它不复制那个判定。
//!
//! # 🔴 三、不静默覆盖 —— 而这一格比上传那一侧更要紧
//!
//! `download_inner` 先写 `{local_path}.part` 再 `rename` 上位 ⇒ 落点原处那个文件
//! **被原子地盖掉**，没有备份、不可撤销。
//!
//! ⇒ 两问，不是一问（[`Ask`] 那个枚举就是这两步）：
//! ① **落点**（缺省填 `<本机 home>/<原名>`）；
//! ② 那条路径**已经有东西**了 ⇒ 再问一次「盖掉它？」。
//!
//! ⚠ 缺省值填不填，这一刀与 `writeops::WritePrompt::for_chmod` 的裁断**不同**，
//! 理由也不同：权限那一格空着开，是因为「列表里没有 mode 可读，预填一个猜出来的值
//! 而用户直接点确认 = 静默改坏权限」。这里预填的 `<home>/<原名>` **不是猜的**
//! （它是一条确定的路径），而且它**不可能静默毁东西** —— 存在性那一问在它后面。
//!
//! # 四、这一刀**没**处置的（逐条，别读宽）
//!
//! 1. **多选下载没做** —— 同第五刀那条：生产侧递进来的恒是一件。
//! 2. **目录递归下载没做** —— `sftp_download` 读的是一个文件；
//!    [`is_downloadable`] 因此把目录挡在外面（而不是让它失败在池子里）。
//! 3. **断点续传那一半不在这儿** —— 它在 `sftp_download` 里面（尾块逐字节对账），
//!    本模块看不见也不该看见（同 `transfer.rs` 头注那一条）。
//! 4. **真的「拖」出去没做** —— 这一刀是一颗按钮。窗口之间互拖、拖到别的应用里，
//!    那要平台的拖放协议，与本刀不是一件事。

use crate::copy_table::copy_text;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::source::Row;

/// 行上那颗按钮。
pub static DOWNLOAD_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinDownload.label.download", &[]));

/// 这一行**拉得下来**吗。
///
/// # 🔴 它与 [`super::copy::is_copyable`] 今天**逐行相同**，而它们刻意是两个函数
///
/// 两条规矩同源不是巧合：目录不行（`sftp_download` 读的是一个文件）、
/// 有损名不行（SFTP 那侧寻址不到真字节）。
/// **但它们答的是两个问题**：一个是「能不能在那台远端上零流量复制」，
/// 一个是「能不能拉到这台机器上来」。合成一个之后，改动任一侧的射程会
/// **静默改掉另一侧**（例：将来支持递归下载 ⇒ 目录这一档要放开，
/// 而零流量复制那一侧不该跟着放开）。
///
/// ⇒ 照本模块的先例（`source.rs` 那两份排序实现）：**两份各自存在，
/// 但「今天它们一致」是一条相等断言**（住
/// [`tests::the_two_row_gates_agree_today_and_say_so_when_they_stop`]），
/// 所以「它们悄悄漂开」这件事有判据。
pub fn is_downloadable(r: &Row) -> bool {
    !r.is_dir && !r.lossy_name
}

// ═══════════════════════════════════════════════════════════════════════
// 落点：一条纯函数
// ═══════════════════════════════════════════════════════════════════════

/// 用户在那个框里敲的东西 → 最终的本机落点。
///
/// # 规矩（逐条，都有判据）
///
/// - 空的 / 全是空白 ⇒ 报错（**不拿缺省值兜底**：框里被清空和没打开过是两件事，
///   而「悄悄用一个用户看不见的路径」正是本仓那条静默纪律要挡的）。
/// - 以分隔符结尾（`~/dl/`）⇒ 当成**目录**，把原名接上去。
///   那是用户最可能的意思，而且它不产生任何歧义。
/// - 其余原样用（用户给了完整路径，包括改了名）。
///
/// ⚠ **它不碰盘** —— 不判存在、不判可写、不建目录。
/// 那几件各自是一次 IO，而本函数要在没有任何盘的机器上判得动
/// （同从前那道 Claude 数据判定的纪律）。存在性那一问住 [`Ask::Overwrite`] 那一步。
pub fn plan_dest(typed: &str, src_name: &str) -> Result<String, String> {
    let t = typed.trim();
    if t.is_empty() {
        return Err(copy_text("rsFilewinDownload.plan.empty", &[]).into());
    }
    let ends_with_sep = t.ends_with('/') || (cfg!(windows) && t.ends_with('\\'));
    if ends_with_sep {
        if src_name.trim().is_empty() {
            return Err(copy_text(
                "rsFilewinDownload.plan.noName",
                &[("path", &t.to_string())],
            ));
        }
        return Ok(format!("{}/{}", t.trim_end_matches(['/', '\\']), src_name));
    }
    Ok(t.to_string())
}

/// 缺省落点：`<本机 home>/<原名>`。
///
/// ⚠ 它**问的是 `shell::local_home()`**，不自己调 `dirs::home_dir()` ——
/// `local_read_surface_registry::HOME_REACHES` 按「上一处 `fn` 名字」给每一处
/// `home_dir()` 归属，本模块再调一次就会在那张表上多出一格，
/// 而那一格说的是同一件事（同 `shell.rs::local_home` 头注那条登记要求）。
pub fn default_dest(src_name: &str) -> String {
    format!("{}/{}", super::shell::local_home(), src_name)
}

// ═══════════════════════════════════════════════════════════════════════
// 两问：那个状态机
// ═══════════════════════════════════════════════════════════════════════

/// 正在问用户什么。`None`（外层的 `Option`）= 没在问。
///
/// 🔴 **刻意是封闭枚举而不是一摞 `bool`**：两问有**顺序**（落点 → 覆盖），
/// 而一摞 `bool` 可以同时为真 —— 那一态在界面上是两个框叠在一起，
/// 在逻辑上是「还没问落点就问覆盖」。枚举让那一态**不可表示**。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// 第一问：存到哪儿。`text` 是框里正在编辑的那几个字。
    Dest {
        src_path: String,
        src_name: String,
        src_size: u64,
        text: String,
    },
    /// 第二问：那儿已经有东西了，盖掉？
    Overwrite {
        src_path: String,
        src_name: String,
        src_size: u64,
        dest: String,
    },
}

impl Ask {
    /// 摆出第一问（缺省填 `<home>/<原名>`）。
    pub fn for_row(r: &Row) -> Self {
        Ask::Dest {
            src_path: r.path.clone(),
            src_name: r.name.clone(),
            src_size: r.size,
            text: default_dest(&r.name),
        }
    }

    /// 那一行在列表上的名字（只用来说话）。
    pub fn src_name(&self) -> &str {
        match self {
            Ask::Dest { src_name, .. } | Ask::Overwrite { src_name, .. } => src_name,
        }
    }
}

/// 第一问的答复要往哪儿去。
///
/// 🔴 **三支，而「报错」与「要再问一次」刻意是两支**：
/// 合成一支（比如「回一个 `Result<Option<..>>`」）之后，调用方要靠嵌套
/// 去分辨「路径不合法」与「路径合法但要确认覆盖」，而那两件在界面上
/// 一个是红字、一个是第二个框。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DestVerdict {
    /// 路径不合法 —— 带着原话回去，框**留着**（不清空用户敲的东西）。
    Rejected(String),
    /// 那儿已经有东西 ⇒ 问第二问。
    NeedsOverwrite(Ask),
    /// 可以做了。
    Go { src_path: String, dest: String },
}

/// 判一遍第一问的答复。
///
/// `exists` 是**注进来的**（`fn(&str) -> bool`），不是直接 `Path::exists` ——
/// 那样这一整条判得动，而且判据不用在盘上摆真文件就能走两条分支。
/// ⚠ 生产那一侧传的就是 [`dest_exists`]，全树恰好一处。
pub fn judge_dest(ask: &Ask, exists: impl Fn(&str) -> bool) -> DestVerdict {
    let Ask::Dest {
        src_path,
        src_name,
        src_size,
        text,
    } = ask
    else {
        // 第二问那一态不该走到这儿来。回一句能读的，而不是 panic ——
        // 这个窗口崩掉等于用户丢掉整个文件管理器（release 是 `panic = "abort"`）。
        return DestVerdict::Rejected(copy_text("rsFilewinDownload.judge.wrongState", &[]).into());
    };
    let dest = match plan_dest(text, src_name) {
        Ok(d) => d,
        Err(e) => return DestVerdict::Rejected(e),
    };
    if exists(&dest) {
        return DestVerdict::NeedsOverwrite(Ask::Overwrite {
            src_path: src_path.clone(),
            src_name: src_name.clone(),
            src_size: *src_size,
            dest,
        });
    }
    DestVerdict::Go {
        src_path: src_path.clone(),
        dest,
    }
}

/// 生产那一侧的存在性判定。**全树恰好一处**（判据钉着）。
pub fn dest_exists(p: &str) -> bool {
    dest_exists_at(std::path::Path::new(p))
}

/// 〔FILES2 · Q4〕同 [`dest_exists`]，落点是一条 `Path`（有损名在 Linux 上落成原始字节，串装不下）。
/// 碰盘判存在只住这一个函数（[`dest_exists`] 转调它）。
pub fn dest_exists_at(p: &std::path::Path) -> bool {
    p.exists()
}

/// 〔FILES2 · Q4〕这一行拉不拉得下来（看 [`Listed`] 那一格原始字节）：名字不是 UTF-8 但带着字节 ⇒ 也拉得下来
/// （远端按字节就地拷进暂存区再下，`lossy_pull.rs`）。[`is_downloadable`] 照旧只看 [`Row`]（与复制那一道对拍的那条不动）。
///
/// [`Listed`]: super::source::Listed
pub fn is_downloadable_listed(r: &super::source::Listed) -> bool {
    is_downloadable(r) || (!r.is_dir && r.raw_name.is_some())
}

// ═══════════════════════════════════════════════════════════════════════
// 这一趟在窗口上的样子
// ═══════════════════════════════════════════════════════════════════════

/// 一趟下载的结局。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Done {
        dest: String,
        bytes: u64,
    },
    /// 带着下层那句**原话**（围栏的拒绝、连接失败、落地失败 …）。
    Failed {
        dest: String,
        why: String,
    },
}

#[derive(Default)]
struct Board {
    /// 正在拉哪一行（`None` = 没在拉）。
    in_flight: Option<String>,
    got: u64,
    total: u64,
    last: Option<Outcome>,
    /// 〔FILES2 · Q4〕上一趟结局旁边要说的那一句（Windows 上有损名改成了什么）。
    note: Option<String>,
}

/// 跨线程共享的那一格（UI 线程画，tokio 那条写）。同 `transfer::DropBoard` 的理由。
#[derive(Clone, Default)]
pub struct DownloadBoard {
    inner: Arc<Mutex<Board>>,
    /// 跑完的趟数 —— 给判据与诊断一个可观测的数（同 `DropBoard::rounds`）。
    rounds: Arc<AtomicU64>,
    /// 🔴 **拿来敲窗口的那只手。** 进度是从 tokio 那条线程写进来的，而 egui
    /// **只在有事发生时才画下一帧** ⇒ 没有它，进度行会冻在第一个读数上
    /// （`transfer::DropBoard` 那个字段的头注逐字记着同一形）。
    ctx: Arc<Mutex<Option<egui::Context>>>,
    /// 取消登记 —— **与上传/复制那两条路同一个类型**，因为那是池子取消登记表的
    /// 唯一造键落点（`transfer::CancelDesk::mint` 的头注：两趟同键会互相摘登记）。
    desk: super::transfer::CancelDesk,
}

impl DownloadBoard {
    /// 把窗口交给它，好让它在进度动的时候敲一下。
    pub fn attach(&self, ctx: Option<egui::Context>) {
        *self.ctx.lock().unwrap() = ctx;
    }

    /// 取消登记那一格（同上传/复制两条路，唯一造键落点在它里面）。
    pub fn cancels(&self) -> super::transfer::CancelDesk {
        self.desk.clone()
    }

    /// 敲一下窗口：「有新东西了，画下一帧」。没有窗口就什么都不做。
    pub fn poke(&self) {
        if let Some(c) = self.ctx.lock().unwrap().as_ref() {
            c.request_repaint();
        }
    }

    pub fn begin(&self, name: &str) {
        let mut b = self.lock();
        b.in_flight = Some(name.to_string());
        b.got = 0;
        b.total = 0;
    }

    pub fn progress(&self, got: u64, total: u64) {
        {
            let mut b = self.lock();
            b.got = got;
            b.total = total;
        }
        // ⚠ 锁放掉之后才敲 —— `request_repaint` 会走进 egui 自己的锁，
        //   两把锁嵌着拿是死锁的常规做法（同 `DropBoard::progress` 那一句）。
        self.poke();
    }

    pub fn finish(&self, o: Outcome) {
        {
            let mut b = self.lock();
            b.in_flight = None;
            b.last = Some(o);
        }
        // ⚠ 顺序要紧：先落 `last`、再加 `rounds` —— 反了的话判据看见
        // `rounds > 0` 之后去读 `last` 可能还是上一趟的（现打过一次这一形）。
        self.rounds.fetch_add(1, Ordering::SeqCst);
        self.poke();
    }

    pub fn in_flight(&self) -> Option<String> {
        self.lock().in_flight.clone()
    }

    /// 〔FILES2 · Q4〕这一趟结局旁边要说的那一句（`None` ＝ 不说）。每一趟开头都要设一次（上一趟的不留）。
    pub fn set_note(&self, note: Option<String>) {
        self.lock().note = note;
    }

    pub fn note(&self) -> Option<String> {
        self.lock().note.clone()
    }

    pub fn seen(&self) -> (u64, u64) {
        let b = self.lock();
        (b.got, b.total)
    }

    pub fn last(&self) -> Option<Outcome> {
        self.lock().last.clone()
    }

    pub fn rounds(&self) -> u64 {
        self.rounds.load(Ordering::SeqCst)
    }

    /// 毒化容忍（同 `index_testing::serial` 的理由：一次 panic 不该让
    /// 后面每一帧都跟着 panic —— 这个窗口崩掉 = 用户丢掉整个文件管理器）。
    fn lock(&self) -> std::sync::MutexGuard<'_, Board> {
        match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }
}

/// 开单：下载（传输台那一侧 `sftp_pool::TRANSFER_DOWNLOAD`，判据钉两份相等）。
pub const OP_DOWNLOAD: &str = "transfer-download";

/// 真起一件下载 —— **开单 → 起跑并看**，两步全经通道（`设计/60 §13.2 ⑤`）。
///
/// 🔴〔F7c · 第三波 09-24〕窗口进程**一行 SFTP 都不碰**了：字节由 monitor 里的传输台从远端读、
/// 落到用户在上面那一问里选的本机路径（远端只读、不经后端）。本机落点那道围栏、`.part` ＋ 改名上位、
/// 撤留 `.part` 续传，**全在传输台那一侧，一个字节没改**（`sftp_pool::download_inner`）。
/// ⚠ 窗口与 monitor 在同一台机器上 ⇒ 「本机路径」对两边是同一个东西。
pub async fn pull_one(
    line: &super::source::Line,
    origin: &super::source::Origin,
    remote_path: &str,
    dest: &str,
    board: &DownloadBoard,
) -> Result<(), String> {
    pull_one_at(
        line,
        origin,
        remote_path,
        serde_json::Value::String(dest.to_string()),
        board,
    )
    .await
}

/// 〔FILES2 · Q4〕同 [`pull_one`]，本机落点给线上那一形（字符串或 `{"b16": …}`：有损名在 Linux 上按原始字节落名）。
pub async fn pull_one_at(
    line: &super::source::Line,
    origin: &super::source::Origin,
    remote_path: &str,
    dest: serde_json::Value,
    board: &DownloadBoard,
) -> Result<(), String> {
    let stop = board.cancels().stop_token();
    let opened = super::source::ask(
        line,
        origin,
        OP_DOWNLOAD,
        &serde_json::json!({ "remote_path": remote_path, "local_path": dest }),
        super::transfer::OPEN_BUDGET,
    )
    .await?;
    let id = super::transfer::field(&opened, OP_DOWNLOAD, "id")?;
    let sink = board.clone();
    super::source::watch(
        line,
        origin,
        &format!("{}{id}", super::transfer::KIND_PREFIX),
        &stop,
        |got, total| sink.progress(got, total),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/filewin/download_tests.rs"]
mod tests;
