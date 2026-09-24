//! `24e` 第九刀：**改一份远端文本** —— `sftp_read_text_for_edit` ＋ `sftp_write_text`。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 🔴 一、`设计/60 §5.4b` 把两个问题**指名**留给了这一刀
//! ═══════════════════════════════════════════════════════════════════════
//!
//! 那一节的裁定逐字：「256 KiB 上限」与「改流式」都是**面板还是 webview 时**
//! 写下的；`§4 戊` 已选原生窗口、`§6.6 C` 已改判为退役那块面板
//! ⇒「在一个要被替换掉的命令上改签名做流式，是**给将死的东西做手术**。
//! 那个上限该是多少、超了怎么办，要在**原生窗口的文本控件**这个语境里答，
//! 连 `byte_cap_registry` 一起改。⇒ 归 `99 §4` 的 `24e`。」
//!
//! ## 问题二「**超了怎么办**」—— 本刀答了，而答案不用改任何签名
//!
//! `sftp_read_text_for_edit` 回的 `Option<String>` 把**三件事压成一件**：
//! 太大 / 含 NUL / 非 UTF-8 全是 `None` ⇒ **谁拿到 `None` 都说不出为什么**。
//! 老面板的做法是把那一行灰置 —— 而「灰置」与「这个功能坏了」在屏幕上同形。
//!
//! 🔴 **而那三件里最常见的那一件，窗口自己就判得出来**：列目录回来的每一行
//! 都带着 `size` ⇒ [`why_not_editable`] 按 [`crate::sftp_pool::MAX_EDIT_BYTES`]
//! 在**本地**判「太大」，**连那趟往返都不发**，而且把那个数说给用户听
//! （「这份 1.2 M 超过 256 K 的编辑上限」）。
//!
//! ⇒ 于是 `None` 只剩下两种可能（含 NUL / 非 UTF-8），而那两种本来就是同一句话
//! （「这不是一份文本文件」）⇒ **三件事分成了两句人话，零签名改动、零额外往返。**
//!
//! ⚠ 如实登记它**买不到**什么：远端那个文件在「我们读 `size`」与「我们真去读它」
//! 之间被换掉（变大 / 变成二进制）⇒ 本地预判会放它过去，而池子那边的
//! `decode_editable` 仍然是最终护栏（它冗余复核大小，正是为这个竞态）。
//! ⇒ **本地预判是一句话的来源，不是一道围栏。**
//!
//! ## 问题一「**这个量该多大**」—— 本刀**不动它**，并写清为什么
//!
//! 256 KiB 保持不变。三条理由，都不是「懒」：
//!
//! 1. **egui 的 `TextEdit` 每帧要把整段文字排一次版**（galley）。
//!    那一档的代价随字节数长，而它决定的是「打字卡不卡」——
//!    那正是「上限该多大」在**原生文本控件语境里**的真实约束。
//!
//!    🔴 **现打（2026-09-22，本机）**：`262 143` 字节的多行文本，
//!    在真 `egui::Context` 上 `TextEdit::multiline` 排**一帧 16.3 ms**
//!    （那一帧真的画出了 262 143 字节文字）。
//!    住 [`tests::a_full_cap_worth_of_text_still_lays_out_in_one_frame`]。
//!
//!    ⇒ **这个读数改变了结论的性质**：60 fps 的预算是 **16.67 ms**
//!    ⇒ 满上限时排版**几乎吃掉整帧**，一键一帧、没有余量。
//!    **256 KiB 是天花板，不是一个中值。**
//!    ⚠ 所以「改大」不只是「要先有形状」—— 现打说它**直接超预算**。
//!    ⚠ 而「改小」今天**没有依据**：没有任何读数说 256 KiB 排不动
//!    （16.3 ms 是「刚好排得动」，不是「排不动」）。
//!
//!    🔴🔴 **〔第十四刀 2026-09-23 订正 —— 上面这一段的结论是错的，逐条见 §四.0〕**
//!    上面那个 16.3 ms 是**两个错**叠在一起：
//!    ① 它是 **debug 档**的数（同一条判据在 **release** 档上现打 **2.19 ms**），
//!       而 16.67 ms 那个预算说的是**发出去的那个 release 二进制**；
//!    ② 它是**全新 `Context` 的第一帧**，里头大头是字体图谱与中文字形栅格化
//!       （同样首帧、只放 3 字节：debug 4.6–5.6 ms / release 0.30–0.47 ms），
//!       **不随文本长度长**；同一份文本的后续帧因为 galley 缓存命中只要
//!       debug 0.98–1.27 ms。
//!    ⇒ 「满上限时排版几乎吃掉整帧」这句话**在 release 上不成立**
//!      （满上限、行结构正常时打字帧 release **0.08–1.02 ms**，余量 16–200 倍）。
//!    ⇒ 本段**不删**〔散文墓碑〕，但从此**不许**再拿它论证「满上限就超预算」。
//!    ⇒ 而真正超预算的那一族**另有其人**，也在 §四 里：**一行特别长的文件**。
//! 2. **改小**会让今天能改的文件改不了（用户会当成退步），而现打没有任何
//!    读数说 256 KiB 排不动。
//! 3. **改大**要先有「大文件怎么编辑」的形状（流式 / 分段 / 只读预览），
//!    而那一件 `§5.4b` 自己就说形状没定 —— **并且现打说它超帧预算**（见第 1 条）。
//!    ⇒ 不在没有形状、又没有预算的时候动那个数。
//!
//! ⚠ 那个读数**买不到**什么（逐条）：它是**CPU 排版**那一段，
//! 不含 GPU 上屏、不含真输入法、不含用户那台机器（本机没有图形会话，
//! `XDG_SESSION_TYPE=tty`）。⇒ 「16.3 ms」是一个**下界**，真机上只会更贵。
//! ⚠ 那条判据**刻意不钉毫秒数**（钉了就是一条随机器快慢红的判据 ——
//! 本仓那条「金标准把开发机烤进去只有它永远绿」的反面）。它钉的是
//! 「跑完了 ＋ 真的排了那么多字」，而那个毫秒数只印出来当读数。
//!
//! 🔴 ⇒ 交给 `byte_cap_registry` 那条判据的答复是：**这个数经本刀复核过，
//! 保持 256 KiB**，理由是上面这三条（而不是「没人动它」）。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 🔴 二、改了没存就关掉 —— 那是这一刀的「不静默覆盖」
//! ═══════════════════════════════════════════════════════════════════════
//!
//! 第八刀那一问挡的是「盖掉本机一个文件」。这一刀对应的那一格是
//! **「用户敲了半天的东西被静默丢掉」**：编辑框一关，那些字就没了，
//! 而它与「我存过了」在屏幕上长得一样。
//!
//! ⇒ [`Pane::dirty`] 是一条**相等断言**（`text != original`），
//! 而关窗那条路要先问它（[`Close::NeedsConfirm`]）。
//!
//! ⚠ **存失败时不许清掉编辑框** —— 那句话是老面板注释里原有的
//!（「失败传播 Err(前端保留编辑框内容)」），本刀照同一条办，
//! 并且由判据钉住（用户敲的东西是他唯一的一份）。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 三、这一刀**没**处置的（逐条，别读宽）
//! ═══════════════════════════════════════════════════════════════════════
//!
//! 1. **大文件编辑（流式）没做** —— 形状没定（`§5.4b` 自陈）。本刀**不改签名**。
//! 2. **新建文件没做** —— `sftp_write_text` 写得了一条不存在的路径，
//!    但「在界面上从零造一份文本」是另一个交互题（要先问名字）。
//! 3. **本机那一侧不能编辑** —— 两条命令都是远端的。本机文本要另一条路。
//! 4. **没有语法高亮 / 行号 / 查找替换** —— 那是一个编辑器，不是这一刀。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 四、「只排视口内的行」—— **〔F9 2026-09-24〕落地了，住 [`super::bigfile`]**
//!
//! 第十四刀的设计（egui 那一侧的源码级读数 · 十条难题表 · 四个更便宜的等价物）住
//! `调研/设计/60 §9`；落地的形状、两个阈值的推算、判据与买不到的，住 `设计/60 §13`
//! 与 [`super::bigfile`] 头注。
//!
//! ⚠ 第十四刀留在这里的那一组内核（行索引 · 窗口 · 写回 · 「开窗买不到」那把尺子）
//! **随落地一起删了**：它们建模的是「窗口化 `TextEdit`」那条路，而落地走的是另一条
//! （自己画、全文坐标、横向也只排可见段）—— 那条路的撤销栈会静默改坏文件
//! （`§9 §四.2` 第 3 条），一行特别长时也买不到东西（第 10 条），留着只会让人以为它还是候选。
//! 行结构改由 [`super::bigfile::Lines`] 增量维护，判据对着 `str::split('\n')` 钉。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::sftp_pool::MAX_EDIT_BYTES;

use super::source::Row;

/// 行上那颗按钮。
pub const EDIT_LABEL: &str = "编辑";

/// 这一行**为什么**改不了（`None` = 改得了）。
///
/// 🔴 回 `Option<String>` 而不是 `bool`：那句话就是这一刀买到的东西
///（见头注「超了怎么办」那一节）。`bool` 只够画一个灰按钮。
pub fn why_not_editable(r: &Row) -> Option<String> {
    if r.is_dir {
        return Some("这是一个目录".into());
    }
    if r.lossy_name {
        // 与 `is_copyable` / `is_writable` 同一条：名字不是合法 UTF-8 ⇒ 寻址不到真字节。
        return Some("这个名字不是合法 UTF-8，寻址不到真字节".into());
    }
    if r.size > MAX_EDIT_BYTES as u64 {
        // 🔴 **报「多了多少」，不是报两个 `human_size`。**
        //
        // 第一版写的是「这份 {human(size)} 超过 {human(cap)} 的编辑上限」，
        // 而判据当场逮到它退化：`256 KiB + 1` 字节被 `human_size` 四舍成 `256.0 K`
        // ⇒ 那句话读出来是「这份 **256.0 K** 超过 **256.0 K** 的编辑上限」，
        // 两个数一模一样，**什么都没告诉用户**。
        // ⇒ 换成「多了 N 字节」：它在边界附近**永远不退化**，而且直接答
        //   「我该把文件弄小多少」这个用户真正要问的问题。
        let over = r.size - MAX_EDIT_BYTES as u64;
        return Some(format!(
            "这份 {}（{} 字节）超过编辑上限 {}，多了 {} 字节（超限**拒编而非截断** —— \
             截断过的文本存回去会写坏文件）",
            super::rows::human_size(r.size),
            r.size,
            super::rows::human_size(MAX_EDIT_BYTES as u64),
            over
        ));
    }
    None
}

/// 行上那颗「编辑」画不画。
///
/// ⚠ 它就是 [`why_not_editable`] 的 `is_none()` —— **刻意不另写一套条件**
///（那正是「按钮画了但点了没反应」那个静默态的来源；同 `download` 那一对
/// `return_label` / `go_remote` 的理由）。
pub fn is_editable(r: &Row) -> bool {
    why_not_editable(r).is_none()
}

/// 池子回了 `None` 之后那句话。
///
/// 🔴 **「太大」不在这里** —— 那一档由 [`why_not_editable`] 在发往返**之前**挡掉。
/// 走到这儿还是 `None`，剩下的可能只有两种，而它们是同一句人话。
///
/// ⚠ 它**也可能**是那个竞态（读 `size` 之后文件被换大了）⇒ 这句话里
/// 把那一形也说了，否则用户会对着一个刚变大的文件反复点。
pub fn not_text_notice(path: &str) -> String {
    format!(
        "{path} 不是一份可编辑的文本（含 NUL 字节、或不是 UTF-8）。\
         ⚠ 也可能是它刚刚被换成了更大 / 二进制的内容 —— 刷新看看。"
    )
}

// ═══════════════════════════════════════════════════════════════════════
// 编辑面那一格
// ═══════════════════════════════════════════════════════════════════════

/// 打开着的那一份。
///
/// ⚠〔F9〕不再派生 `PartialEq`：[`Self::big`] 是一份共享的界面状态，谈不上「相等」，
/// 而全仓没有一处比较两个 `Pane`。
#[derive(Clone, Debug)]
pub struct Pane {
    pub path: String,
    /// 行上那个名字（标题用）。
    pub name: String,
    /// 正在编辑的那些字。
    pub text: String,
    /// 🔴 **读回来时的那一份** —— [`Self::dirty`] 靠它，而那是这一刀
    /// 「不静默丢弃」的判据源。
    original: String,
    /// 上一次存盘的结局（`None` = 还没存过）。
    pub last_save: Option<Result<(), String>>,
    /// 〔F9〕大文件模式那一格（`None` 在里面 ＝ 普通路径）。逐条住 [`super::bigfile`] 头注。
    pub(crate) big: super::bigfile::BigSlot,
}

impl Pane {
    pub fn opened(path: &str, name: &str, text: String) -> Self {
        Self {
            path: path.to_string(),
            name: name.to_string(),
            original: text.clone(),
            text,
            last_save: None,
            big: Default::default(),
        }
    }

    /// 改过了吗。**相等断言**，不是一个「用户敲过键」的标志位 ——
    /// 敲进去又改回来**不算改过**，而一个标志位会把那一形报成「有未保存改动」。
    pub fn dirty(&self) -> bool {
        self.text != self.original
    }

    /// 存成功了 ⇒ 基准线跟上（从此 [`Self::dirty`] 回 `false`）。
    pub fn mark_saved(&mut self) {
        self.original = self.text.clone();
        self.last_save = Some(Ok(()));
    }

    /// 🔴 存失败了 ⇒ **基准线不动、`text` 一个字都不碰**。
    ///
    /// 用户敲的那些东西是他**唯一的一份**（远端那份还是旧的）。
    /// 老面板注释里原话是「失败传播 Err(前端保留编辑框内容)」，同一条。
    pub fn mark_failed(&mut self, why: String) {
        self.last_save = Some(Err(why));
    }

    /// 这一份还差多少到上限（给界面画一句「还能写 N」）。
    pub fn headroom(&self) -> i64 {
        MAX_EDIT_BYTES as i64 - self.text.len() as i64
    }

    /// 🔴 **敲超上限了吗。** 存回去会被池子拒（`decode_editable` 的最终护栏），
    /// 所以要在屏幕上先说，而不是等存的时候才失败。
    pub fn over_cap(&self) -> bool {
        self.text.len() > MAX_EDIT_BYTES
    }
}

/// 关掉编辑面这一下该怎么走。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Close {
    /// 没改过 ⇒ 直接关。
    Now,
    /// 🔴 改了没存 ⇒ **先问**（这一刀的「不静默丢弃」）。
    NeedsConfirm,
}

/// 判一遍关窗这一下。
pub fn judge_close(p: &Pane) -> Close {
    if p.dirty() {
        Close::NeedsConfirm
    } else {
        Close::Now
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 生产适配器：**一行自己的 SFTP 代码都没有**
// ═══════════════════════════════════════════════════════════════════════

/// 一趟「打开」的结局。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arrived {
    /// 读回来了，可以编辑。
    Text {
        path: String,
        name: String,
        text: String,
    },
    /// 池子说它不可编辑（那句话由 [`not_text_notice`] 给）。
    NotText { path: String },
    /// 下层那句原话（连不上 / 没权限 …）。
    Failed { path: String, why: String },
}

#[derive(Default)]
struct Desk {
    /// 正在打开哪一份（`None` = 没在打开）。
    opening: Option<String>,
    /// 到货的那一趟（UI 线程取走）。
    arrived: Option<Arrived>,
    /// 正在存（`None` = 没在存）。
    saving: Option<String>,
    /// 存的结局（UI 线程取走）。
    saved: Option<Result<(), String>>,
}

/// 跨线程共享那一格（UI 线程读，tokio 那条写）。同 `DownloadBoard` 的理由。
#[derive(Clone, Default)]
pub struct EditBoard {
    inner: Arc<Mutex<Desk>>,
    /// 打开过几趟、存过几趟 —— 给判据与诊断两个可观测的数。
    opens: Arc<AtomicU64>,
    saves: Arc<AtomicU64>,
    /// 🔴 敲窗口那只手（同 `DownloadBoard::ctx`：读/存都在 tokio 那条线程上落，
    /// 而 egui 只在有事发生时画下一帧）。
    ctx: Arc<Mutex<Option<egui::Context>>>,
}

impl EditBoard {
    pub fn attach(&self, ctx: Option<egui::Context>) {
        *self.ctx.lock().unwrap() = ctx;
    }

    pub fn poke(&self) {
        if let Some(c) = self.ctx.lock().unwrap().as_ref() {
            c.request_repaint();
        }
    }

    pub fn begin_open(&self, path: &str) {
        self.lock().opening = Some(path.to_string());
    }

    pub fn deliver(&self, a: Arrived) {
        {
            let mut d = self.lock();
            d.opening = None;
            d.arrived = Some(a);
        }
        // ⚠ 同 `DownloadBoard::finish`：先落货、再加数、最后才敲（锁已放掉）。
        self.opens.fetch_add(1, Ordering::SeqCst);
        self.poke();
    }

    /// UI 线程**取走**到货的那一趟（取走之后就没了 —— 它是一次性事件，
    /// 不是一个状态；留着会让下一帧又建一次编辑面）。
    pub fn take_arrived(&self) -> Option<Arrived> {
        self.lock().arrived.take()
    }

    pub fn opening(&self) -> Option<String> {
        self.lock().opening.clone()
    }

    pub fn begin_save(&self, path: &str) {
        self.lock().saving = Some(path.to_string());
    }

    pub fn deliver_save(&self, r: Result<(), String>) {
        {
            let mut d = self.lock();
            d.saving = None;
            d.saved = Some(r);
        }
        self.saves.fetch_add(1, Ordering::SeqCst);
        self.poke();
    }

    pub fn take_saved(&self) -> Option<Result<(), String>> {
        self.lock().saved.take()
    }

    pub fn saving(&self) -> Option<String> {
        self.lock().saving.clone()
    }

    pub fn opens(&self) -> u64 {
        self.opens.load(Ordering::SeqCst)
    }

    pub fn saves(&self) -> u64 {
        self.saves.load(Ordering::SeqCst)
    }

    /// 毒化容忍 —— 同 `DownloadBoard::lock`（这个窗口崩掉 = 用户丢掉整个文件管理器）。
    fn lock(&self) -> std::sync::MutexGuard<'_, Desk> {
        match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }
}

/// 读一份远端文本。
///
/// 回值：`Ok(Some(文本))` / `Ok(None)` = 池子说它不可编辑（那句话由
/// [`not_text_notice`] 给）/ `Err` = 下层那句原话。
pub async fn read_text(
    cfg: &crate::ssh_source::RemoteConfig,
    path: &str,
) -> Result<Option<String>, String> {
    crate::sftp_pool::sftp_read_text_for_edit(cfg.clone(), path.to_string()).await
}

/// 存回去 —— 〔F2 · 2026-09-24〕经通道说后端写面那条 `files-write-text`。
///
/// 🔴 **围栏在后端那一层**（写面那道会话数据围栏，与桥那一份函数体逐字节相同）⇒ 本模块
/// 不自己判一遍（判定只有一个家）。踩线时那句拒绝原样落进 [`Pane::last_save`]。
/// ⚠ 上一版这里调的是池子那条写文本命令（SFTP）；读那一半（[`read_text`]）**仍走 SFTP**：
/// 后端今天没有「读一份文本」的命令，而把字节从那台机器搬到这个窗口是一次**跨机传输**
/// （`设计/60 §8.4` 未拍）—— 登记在 `boundary_tests::Kind::Transfer`。
/// ⚠ 路径切成 `(root, rel)` 与写面其余四条同形（[`super::writeops::apply_remote`] 头注）。
pub async fn write_text(
    line: &super::source::Line,
    origin: &super::source::Origin,
    path: &str,
    content: &str,
) -> Result<(), String> {
    let args = serde_json::json!({
        "root": super::source::parent_dir(path),
        "rel": super::source::remote_basename(path),
        "content": content,
    });
    super::source::ask(
        line,
        origin,
        "files-write-text",
        &args,
        super::writeops::WRITE_BUDGET,
    )
    .await
    .map(|_| ())
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/editor_tests.rs"]
mod tests;
