//! **编辑一份远端文本**：读经后端 `files-read-text`、存经 `files-write-text`（装不进一行的分块走暂存区）。
//!
//! # 上限
//!
//! 打不打得开、许读多少由列表那一行上后端那道编辑门给（`gates.edit`；不许时那一句也是后端写好的），
//! 连那趟往返都不发；那个数记在打开的那一份上（[`Pane::cap`]），敲超了存之前本地拒、字一个不丢。
//! 读的那一趟后端再判一次大小（列 `size` 与真去读之间文件可能被换掉）。
//!
//! # 改了没存就关掉
//!
//! [`Pane::dirty`] 是一条相等断言（`text != original`），关窗那条路先问它（[`Close::NeedsConfirm`]）；
//! 存失败不清编辑框（用户敲的东西是他唯一的一份）。
//!
//! # 没做的
//!
//! 本机文本不能编辑（两条命令都是远端的）；没有语法高亮。长文件的排版住 [`super::bigfile`]。

use cmd_wire::files as w;
use copy_core::copy_text;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// 🔴**一条请求行最多多长** ＝ 后端入方向一行的上限（`src/backend/stream/inbound/mod.rs::MAX_LINE_BYTES`，
/// 审计加的防 OOM 线，所有命令共用；多一个字节整行丢弃）。
///
/// 存盘用它分两支：整份装得进一行 ⇒ 一条 `files-write-text`；装不进 ⇒ 按它切块（[`plan_chunks`]），
/// 每块那一行都装得进。与后端那一处由 `byte_cap_registry` 对 F 读两侧源码钉相等：
/// 窗口多给一个字节 ⇒ 后端整行丢弃；少给 ⇒ 只是多切几块。
pub const SAVE_LINE_CAP: usize = 1 << 20;

/// 存盘那条线上命令的名字（装得进一行的那一支）。量的与发的必须是**同一条**命令同一份参数（[`save_args`]）。
pub const CMD_WRITE_TEXT: &str = "files-write-text";

/// 装不进一行的那一支：逐块进暂存区（后端 `control/files_commit.rs`）。
pub const CMD_STAGE_CHUNK: &str = "files-stage-chunk";

/// 装不进一行的那一支：读回拼起来、原地覆盖（与 `files-write-text` 同一个原语）。
pub const CMD_COMMIT_TEXT: &str = "files-commit-text";

/// 请求行里 `id` 最长能占多少字节 —— monitor 那一侧 `inbound_client` 发号的形状是
/// `m{毫秒:x}.{连接号}-{序号}`：`m` ＋ u128 十六进制最多 32 ＋ `.` ＋ u64 十进制最多 20 ＋ `-` ＋ 20 ＝ 75。
/// 窗口量的时候按这个最长的 id 算 ⇒ **只会比真发出去的那一行长、不会短**（最多保守 75 字节）。
pub const REQUEST_ID_ROOM: usize = 1 + 32 + 1 + 20 + 1 + 20;

/// 存盘那一趟的参数（路径切成 `(root, rel)` 与写面其余四条同形）。
///
/// `expect` = 「我打开时那一份」的摘要（`files-read-text` 交的、或上一次存成时应答交的），
/// 后端比盘上此刻那一份、对不上就 `stale`、一个字节不写。窗口只把它当不透明令牌原样交回（算法住后端一处）。
pub fn save_args(path: &str, content: &str, expect_sha256: &str) -> w::FilesWriteTextReq {
    save_args_at(
        &super::source::RemotePath::plain(path),
        content,
        expect_sha256,
    )
}

/// 摘要形的 `expect`（窗口当不透明令牌原样交回）。
fn expect_of(sha256: &str) -> w::Sha256Expect {
    w::Sha256Expect {
        sha256: w::Sha256Hex(sha256.to_string()),
    }
}

/// 〔有损名全寻址〕同 [`save_args`]，路径可以带字节（`root` / `rel` 按字节切，有损那一段发 `{"b16": …}`）。
pub fn save_args_at(
    at: &super::source::RemotePath,
    content: &str,
    expect_sha256: &str,
) -> w::FilesWriteTextReq {
    w::FilesWriteTextReq {
        root: at.parent().path(),
        rel: at.tail(),
        content: w::WirePath::from(content),
        expect: expect_of(expect_sha256),
    }
}

/// 送一块的参数。`key` 是这一次存盘现造的 32 位小写十六进制（后端只收这个形状）。块按字切、按字符串发（[`plan_chunks`] 按转义之后的长度装满一行）。
pub fn stage_args(key: &str, seq: u64, chunk: &str) -> w::FilesStageChunkReq {
    w::FilesStageChunkReq {
        key: key.to_string(),
        seq,
        content: w::ChunkBytes::Text(chunk.to_string()),
    }
}

/// 提交那一趟的参数：块数与总字节数**显式**给，后端读回来必须对得上。
pub fn commit_args(
    path: &str,
    key: &str,
    chunks: usize,
    bytes: usize,
    expect_sha256: &str,
) -> w::FilesCommitTextReq {
    commit_args_at(
        &super::source::RemotePath::plain(path),
        key,
        chunks,
        bytes,
        expect_sha256,
    )
}

/// 同 [`commit_args`]，路径可以带字节。
pub fn commit_args_at(
    at: &super::source::RemotePath,
    key: &str,
    chunks: usize,
    bytes: usize,
    expect_sha256: &str,
) -> w::FilesCommitTextReq {
    w::FilesCommitTextReq {
        root: at.parent().path(),
        rel: at.tail(),
        key: key.to_string(),
        chunks: chunks as u64,
        bytes: bytes as u64,
        expect: expect_of(expect_sha256),
    }
}

/// 请求行的形状（字段顺序与 monitor 那一侧 `inbound_client::encode_request` 的 `RequestLine` 相同；
/// 两者逐字节相等由判据对拍）。
#[derive(serde::Serialize)]
struct RequestLine<'a, A: serde::Serialize> {
    id: &'a str,
    cmd: &'a str,
    args: &'a A,
    within_ms: u64,
    tz: &'a str,
}

/// 🔴 **真序列化一次**：这条命令发到后端时那一行有多少字节（**不含**行尾 `\n` ——
/// 后端的上限数的就是换行之前那一段）。`id` 按最长的算（[`REQUEST_ID_ROOM`]），发起方期限那一格按最长的数位算，
/// 时区那一格按最长的名算（[`host_core::TZ_ROOM`]；比它长的那一台不带这一格）。
pub fn request_line_len<A: serde::Serialize>(cmd: &str, args: &A) -> usize {
    let id = "0".repeat(REQUEST_ID_ROOM);
    serde_json::to_vec(&RequestLine {
        id: &id,
        cmd,
        args,
        within_ms: u64::MAX,
        tz: &"Z".repeat(host_core::TZ_ROOM),
    })
    .map_or(usize::MAX, |v| v.len())
}

/// 整份装得进**一条** `files-write-text` 吗（真序列化量，按最长 id）。
/// `expect` 那一格定长（摘要恒 [`SHA256_HEX_LEN`] 位），按一份同长的占位量 —— 与真发的逐字节同长。
pub fn fits_one_line(path: &str, content: &str) -> bool {
    let room = "0".repeat(SHA256_HEX_LEN);
    request_line_len(CMD_WRITE_TEXT, &save_args(path, content, &room)) <= SAVE_LINE_CAP
}

/// 后端 CAS 摘要（SHA-256）的十六进制长度。窗口不算摘要，只认形状：读回来的那一格不是这个形状 ⇒ 不打开
/// （存不回去的编辑面不该立起来）。与后端 `files::SHA256_HEX_LEN` 同一个数（SHA-256 的定义，不是可调的量）。
pub const SHA256_HEX_LEN: usize = 64;

/// 这一格像不像后端交的摘要（64 位小写十六进制）。
pub fn is_sha256_hex(s: &str) -> bool {
    s.len() == SHA256_HEX_LEN
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// 存盘那一问的「盘上那份在你打开之后被改过了」那句（`stale` 那一档；后端原话接在后面）。
pub fn stale_notice(why: &str) -> String {
    copy_text("rsFilewinEditor.stale.notice", &[("why", why)])
}

/// 一个字放进 JSON 字符串之后占几个字节 —— 与 `serde_json` 的转义**逐码位**相同
/// （判据对全部 Unicode 标量值对拍）：引号 · 反斜杠 · 五个有短写法的控制字符 ⇒ 2；
/// 其余 `U+0000..=U+001F` ⇒ 6（`\u00XX`）；别的字原样（UTF-8 字节数）。
pub fn escaped_len(c: char) -> usize {
    match c {
        // 引号与反斜杠按码位写（`\u{22}` / `\u{5c}`）：本仓按文本抠字符串字面量的量具
        // （`fonts_tests` 的探针人群）会把一个裸的引号字符字面量当成字符串的开头。
        '\u{22}' | '\u{5c}' | '\u{8}' | '\u{c}' | '\n' | '\r' | '\t' => 2,
        '\u{0}'..='\u{1f}' => 6,
        _ => c.len_utf8(),
    }
}

/// 每块内容（转义之后）最多占多少字节 ＝ 一行上限 − 空内容那一块的信封（块号按最大的算）。
pub fn chunk_budget() -> usize {
    let key = "0".repeat(32);
    SAVE_LINE_CAP.saturating_sub(request_line_len(
        CMD_STAGE_CHUNK,
        &stage_args(&key, u64::MAX, ""),
    ))
}

/// 把全文按字切成几块：每块转义之后 ≤ `budget`，**贪心取满**（下一块的第一个字放不进上一块）。
///
/// 按字符边界切 ⇒ 每块都是合法 UTF-8、能原样作为 JSON 字符串发出去；拼回来逐字节等于原文。
/// `budget` 要放得下一个最长的字（6 字节；[`chunk_budget`] 由常量算出，判据钉着它够）。放不下时照 6 切、不 panic：
/// 那一块超出一行的上限，后端拒，原话照常回到界面（发布档 panic = abort，一 panic 整个窗口就没了）。
pub fn plan_chunks(content: &str, budget: usize) -> Vec<&str> {
    let budget = budget.max(6);
    let mut out = Vec::new();
    let (mut start, mut used) = (0usize, 0usize);
    for (at, c) in content.char_indices() {
        let e = escaped_len(c);
        if used + e > budget {
            out.push(&content[start..at]);
            start = at;
            used = 0;
        }
        used += e;
    }
    if start < content.len() {
        out.push(&content[start..]);
    }
    out
}

/// 敲超上限（`cap` ＝ 打开时后端给的编辑上限）、存的时候本地拒那句话（一个字节都没发）。
pub fn over_cap_notice(len: usize, cap: u64) -> String {
    copy_text(
        "rsFilewinEditor.overCap.notice",
        &[
            ("len", &len.to_string()),
            ("limit", &cap.to_string()),
            ("over", &(len as u64).saturating_sub(cap).to_string()),
        ],
    )
}

/// 分块存到一半断了那句话（第几段从 1 数；原话接在后面）。提交那一趟没发 ⇒ 远端那份一个字节没动。
pub fn chunk_failed_notice(seq: usize, total: usize, why: &str) -> String {
    copy_text(
        "rsFilewinEditor.chunkFailed.notice",
        &[
            ("seq", &(seq + 1).to_string()),
            ("total", &total.to_string()),
            ("why", &why.to_string()),
        ],
    )
}


/// 行上那颗按钮。
pub static EDIT_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinEditor.label.edit", &[]));
/// 存盘撞上 stale 之后那两颗按钮（住这里：判据按名字点它们，不在画的地方另写一遍）。
pub static OVERWRITE_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinEditor.stale.overwrite", &[]));
/// 同上，另一颗。
pub static REOPEN_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinEditor.stale.reopen", &[]));

/// 这一行**为什么**改不了（`None` = 改得了）：目录 · 名字寻址不到 · 后端那道编辑门不许（那一句是后端写好的）。
/// 回那句话而不是 `bool`：灰着的按钮与「这个功能坏了」在屏幕上同形。
pub fn why_not_editable(l: &super::source::Listed) -> Option<String> {
    if l.is_dir {
        return Some(copy_text("rsFilewinEditor.notEditable.dir", &[]));
    }
    // 名字不是合法 UTF-8、又没带原始字节 ⇒ 寻址不到真字节。
    if l.lossy_name && l.raw_name.is_none() {
        return Some(copy_text("rsFilewinEditor.notEditable.badName", &[]));
    }
    match &l.gates {
        None => Some(copy_text("rsFilewinSource.gates.unlisted", &[])),
        Some(g) => g.edit.refused().map(str::to_string),
    }
}

/// [`why_not_editable`] 的 `is_none()`。
pub fn editable(l: &super::source::Listed) -> bool {
    why_not_editable(l).is_none()
}

/// 打开这一行许读多少（后端那道编辑门给的；不许 ⇒ `None`）。
pub fn edit_cap(l: &super::source::Listed) -> Option<u64> {
    l.gates.as_ref()?.edit.cap()
}

/// 后端说「不可编辑」之后那句话（第九刀时是池子回了 `None`）。
///
/// 「太大」不在这里 —— 那一档由后端那道编辑门在发往返**之前**挡掉。
/// 走到这儿还是 `None`，剩下的可能只有两种，而它们是同一句人话。
///
/// ⚠ 它**也可能**是那个竞态（读 `size` 之后文件被换大了）⇒ 这句话里
/// 把那一形也说了，否则用户会对着一个刚变大的文件反复点。
pub fn not_text_notice(path: &str) -> String {
    copy_text(
        "rsFilewinEditor.notText.notice",
        &[("path", &path.to_string())],
    )
}

// ═══════════════════════════════════════════════════════════════════════
// 编辑面那一格
// ═══════════════════════════════════════════════════════════════════════

/// 打开着的那一份。
///
/// ⚠不再派生 `PartialEq`：[`Self::big`] 是一份共享的界面状态，谈不上「相等」，
/// 而全仓没有一处比较两个 `Pane`。
#[derive(Clone, Debug)]
pub struct Pane {
    pub path: String,
    /// 编辑上限（打开时后端那道门给的）：敲超了不发那一趟存。
    pub cap: u64,
    /// 行上那个名字（标题用）。
    pub name: String,
    /// 正在编辑的那些字。
    pub text: String,
    /// 🔴 **读回来时的那一份** —— [`Self::dirty`] 靠它，而那是这一刀
    /// 「不静默丢弃」的判据源。
    original: String,
    /// **盘上那一份的摘要**（打开时后端交的；每次存成换成应答交的新摘要）—— 存盘 CAS 的 `expect`。
    base_sha256: String,
    /// 上一次存盘的结局（`None` = 还没存过）。
    pub last_save: Option<Result<(), String>>,
    /// 上一次存失败那一条的复制详情（首行之外；空 ⇒ 不出按钮）。随 [`Self::last_save`] 一起写。
    pub save_detail: String,
    /// 上一次存盘撞上了「盘上那份在你打开之后被改过了」（`stale`）⇒ 编辑面摆两颗按钮让人选
    /// （仍然覆盖 · 丢掉我的改动重新打开）。存成 / 重开之后清掉。
    pub stale: bool,
    /// 大文件模式那一格（`None` 在里面 ＝ 普通路径）。逐条住 [`super::bigfile`] 头注。
    pub(crate) big: super::bigfile::BigSlot,
    /// 〔有损名全寻址〕路径不是合法 UTF-8 时那份文件的整条原始字节（读 / 存都走它；`None` ⇒ `path` 就是真字节）。
    pub raw_path: Option<Vec<u8>>,
    /// 〔「查找替换」〕编辑面上那一截查找替换的状态（只在普通路径上画；大文件模式没有，`§5.5`）。
    pub find: FindBar,
    /// 刚打开：下一帧正文从头看起（滚动状态按 id 存在 egui 里、跨文件共用）。
    pub(crate) fresh: bool,
    /// 光标被外面挪了（查找 · 替换）：下一帧把它滚进视野。
    pub(crate) reveal: bool,
    /// 上一次存成的那一刻（UNIX 秒；编辑页头条「已保存 13:42」）。
    pub saved_at: Option<u64>,
    /// 存成了、却没拿到新摘要时挂的那一句（「重开后再改」）；下一次存成且拿到摘要就收起。
    pub save_note: Option<String>,
}

/// 编辑面那一截查找替换的状态。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FindBar {
    /// 查找框里的字。
    pub needle: String,
    /// 替换框里的字。
    pub with: String,
    /// 上一下做完时说的那句话（没找到 · 替换了几处）。
    pub notice: Option<String>,
}

/// 在 `text` 里从第 `from` 个**字**起找 `needle`（区分大小写、字面匹配，不做正则 —— 没人裁过）。
///
/// 往后找 ⇒ 从 `from` 起第一处，找不到从头绕回；往前找 ⇒ `from` 之前最后一处，找不到从尾绕回。
/// 回 `(起, 止)`，单位是**字**（egui 光标的单位）。空的查找串 ⇒ `None`。
pub fn find_from(text: &str, needle: &str, from: usize, backward: bool) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }
    let byte_at = |k: usize| -> usize { text.char_indices().nth(k).map_or(text.len(), |(b, _)| b) };
    let char_at = |b: usize| -> usize { text[..b].chars().count() };
    let at = byte_at(from);
    let hit = if backward {
        text[..at].rfind(needle).or_else(|| text.rfind(needle))
    } else {
        text[at..]
            .find(needle)
            .map(|b| b + at)
            .or_else(|| text.find(needle))
    }?;
    let start = char_at(hit);
    Some((start, start + needle.chars().count()))
}

/// 把 `text` 里所有的 `needle` 换成 `with`。回 `(新文本, 换了几处)`。空的查找串 ⇒ 原样、0 处。
pub fn replace_all(text: &str, needle: &str, with: &str) -> (String, usize) {
    if needle.is_empty() {
        return (text.to_string(), 0);
    }
    let n = text.matches(needle).count();
    (text.replace(needle, with), n)
}

/// 把第 `start..end` 个**字**换成 `with`。回新文本。
pub fn replace_chars(text: &str, start: usize, end: usize, with: &str) -> String {
    let byte_at = |k: usize| -> usize { text.char_indices().nth(k).map_or(text.len(), |(b, _)| b) };
    let (a, b) = (byte_at(start), byte_at(end));
    format!("{}{}{}", &text[..a], with, &text[b..])
}

impl Pane {
    pub fn opened(path: &str, name: &str, text: String, sha256: String, cap: u64) -> Self {
        Self {
            path: path.to_string(),
            cap,
            name: name.to_string(),
            original: text.clone(),
            text,
            base_sha256: sha256,
            last_save: None,
            save_detail: String::new(),
            stale: false,
            big: Default::default(),
            raw_path: None,
            find: FindBar::default(),
            fresh: true,
            reveal: false,
            saved_at: None,
            save_note: None,
        }
    }

    /// 存盘那一趟该交的 `expect`（盘上那一份的摘要）。
    pub fn expect_sha256(&self) -> &str {
        &self.base_sha256
    }

    /// 改过了吗。**相等断言**，不是一个「用户敲过键」的标志位 ——
    /// 敲进去又改回来**不算改过**，而一个标志位会把那一形报成「有未保存改动」。
    pub fn dirty(&self) -> bool {
        self.text != self.original
    }

    /// 存成功了 ⇒ 基准线跟上（从此 [`Self::dirty`] 回 `false`）；摘要换成后端应答交的那份（写进去那份的）
    /// ⇒ 连存两次不自撞。
    ///
    /// ⚠ 基准线是**发出去的那一份**（`sent`），不是此刻的 `text`：存在路上时用户又敲了字，那几个字没存过，
    /// 该算「改过了」。
    ///
    /// 写进去了却没拿到新摘要（`sha256 == None`）⇒ 照样算存成（字已在盘上），基准摘要**不动**：
    /// 下一次存交旧的那一份 ⇒ 撞 `stale`、让人选，而不是悄悄覆盖；另挂一句「重开后再改」。
    pub fn mark_saved(&mut self, sent: &str, sha256: Option<String>) {
        self.original = sent.to_string();
        self.save_note = match sha256 {
            Some(s) => {
                self.base_sha256 = s;
                None
            }
            None => Some(copy_text("rsFilewinEditor.saved.noDigest", &[])),
        };
        self.last_save = Some(Ok(()));
        self.save_detail.clear();
        self.stale = false;
        self.saved_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs());
    }

    /// 存盘撞上 `stale` ⇒ 同 [`Self::mark_failed`]（字一个不动、基准不动），外加摆出那两颗按钮。
    pub fn mark_stale(&mut self, why: String) {
        self.last_save = Some(Err(stale_notice(&why)));
        self.save_detail.clear();
        self.stale = true;
    }

    /// 🔴 存失败了 ⇒ **基准线不动、`text` 一个字都不碰**。
    ///
    /// 用户敲的那些东西是他**唯一的一份**（远端那份还是旧的）。
    /// 老面板注释里原话是「失败传播 Err(前端保留编辑框内容)」，同一条。
    pub fn mark_failed(&mut self, why: String) {
        self.mark_failed_with(why.into());
    }

    /// 同 [`Self::mark_failed`]，连复制详情一起记下（保存失败那一条的［复制详情］用）。
    pub fn mark_failed_with(&mut self, f: super::source::Failed) {
        self.last_save = Some(Err(f.said));
        self.save_detail = f.detail;
    }

    /// 这一份还差多少到上限（给界面画一句「还能写 N」）。
    pub fn headroom(&self) -> i64 {
        self.cap as i64 - self.text.len() as i64
    }

    /// 🔴 **敲超上限了吗。** 超上限的内容存不回去（后端 `files-commit-text` 拒 `bytes` 超过
    /// 同一个数的那一趟；装得进一行的那一支写面不拦大小，但存回去之后下次就读不回来编辑了）
    /// —— 所以要在屏幕上先说，并且不发那一趟。
    pub fn over_cap(&self) -> bool {
        self.text.len() as u64 > self.cap
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

/// 第 `at` 个**字**在第几行、第几列（都从 1 数；列按字数）。编辑页底条「行 12 · 列 5」。
pub fn line_col(text: &str, at: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for c in text.chars().take(at) {
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
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
        /// 后端对那份字节算的摘要（存盘 CAS 的 `expect`）。
        sha256: String,
        /// 编辑上限（读时给的那个数）。
        cap: u64,
    },
    /// 后端说它不可编辑（那句话由 [`not_text_notice`] 给）。
    NotText { path: String },
    /// 下层那句话（断开 / 没权限 …）与它的复制详情。
    Failed {
        path: String,
        why: super::source::Failed,
    },
}

#[derive(Default)]
struct Desk {
    /// 正在打开哪一份（`None` = 没在打开）。
    opening: Option<String>,
    /// 到货的那一趟（UI 线程取走）。
    arrived: Option<Arrived>,
    /// 正在存（`None` = 没在存）。
    saving: Option<String>,
    /// 存的结局（UI 线程取走）。成 ⇒ 发出去的那一份 ＋ 后端交的新摘要。
    saved: Option<Result<Saved, SaveError>>,
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

    /// 有一趟打开还没落进编辑面（在读，或读完了还没被取走）：回那一份的路径。
    pub fn open_pending(&self) -> Option<String> {
        let d = self.lock();
        d.opening.clone().or_else(|| {
            d.arrived.as_ref().map(|a| match a {
                Arrived::Text { path, .. } | Arrived::NotText { path } => path.clone(),
                Arrived::Failed { path, .. } => path.clone(),
            })
        })
    }

    /// 有一趟存还没落进编辑面（在存，或存完了还没被取走）。
    pub fn save_pending(&self) -> bool {
        let d = self.lock();
        d.saving.is_some() || d.saved.is_some()
    }

    pub fn begin_save(&self, path: &str) {
        self.lock().saving = Some(path.to_string());
    }

    pub fn deliver_save(&self, r: Result<Saved, SaveError>) {
        {
            let mut d = self.lock();
            d.saving = None;
            d.saved = Some(r);
        }
        self.saves.fetch_add(1, Ordering::SeqCst);
        self.poke();
    }

    pub fn take_saved(&self) -> Option<Result<Saved, SaveError>> {
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

/// 读文本那条线上命令的名字（后端 `files-read` 族第七条）。
pub const CMD_READ_TEXT: &str = "files-read-text";

/// 读一份文本那一趟的往返上限（调用方给的期限）。
pub const READ_BUDGET: std::time::Duration = std::time::Duration::from_secs(20);

/// 读一份远端文本 —— 经通道问后端 `files-read-text`。
///
/// 回值：`Ok(Some(文本))` / `Ok(None)` = 后端说它不可编辑（那句话由
/// [`not_text_notice`] 给）/ `Err` = 那句原话（没走通 / 读不到 …）。
///
/// ⚠ 上一版这里调的是池子那条读文本命令（SFTP 把字节整份搬过来）；那是窗口进程里
/// 「跨机传输」那一类欠账的一条。现在字节**在那台机器上**读、只把文本经通道交回来。
/// ⚠ 有逻辑的那一段（哪几个码算「不可编辑」）住 [`text_from_reply`]。
/// 路径可以带字节（`path` 发 `{"b16": …}`）；`cap` ＝ 列表那一行上后端那道编辑门给的上限。
pub async fn read_text_at(
    line: &super::source::Line,
    origin: &super::source::Origin,
    at: &super::source::RemotePath,
    cap: u64,
) -> Result<Option<Opened>, super::source::Failed> {
    let req = cmd_wire::files::ReadTextReq {
        path: at.path(),
        max_bytes: cap,
    };
    opened_from_reply(
        origin,
        super::source::ask_typed(line, origin, &req, READ_BUDGET).await,
    )
}

/// 打开那一趟交回来的：全文 ＋ 后端对那份字节算的摘要。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opened {
    pub text: String,
    pub sha256: String,
}

/// [`text_from_reply`] ＋ 摘要那一格。**纯函数**。
///
/// 有文本却没有摘要（或形状不对）⇒ `Err`：没有它就存不回去（存盘必带「我打开时那一份」，后端不收不带的），
/// 立起一个存不回去的编辑面比不打开更糟 —— 那句话说清是后端太旧。
pub fn opened_from_reply(
    origin: &super::source::Origin,
    r: Result<cmd_wire::files::ReadTextReply, super::source::Failed>,
) -> Result<Option<Opened>, super::source::Failed> {
    let Some(got) = reply_from(r)? else {
        return Ok(None);
    };
    if !is_sha256_hex(&got.sha256) {
        return Err(super::source::Failed::here(
            copy_core::backend_old(&origin.0),
            origin,
            CMD_READ_TEXT,
            None,
        ));
    }
    Ok(Some(Opened {
        text: got.text,
        sha256: got.sha256,
    }))
}

/// 后端那一趟的结局（已按回包类型解过）：拒的那两种（不是一份能编辑的文本）⇒ `None`。
fn reply_from(
    r: Result<cmd_wire::files::ReadTextReply, super::source::Failed>,
) -> Result<Option<cmd_wire::files::ReadTextReply>, super::source::Failed> {
    match r {
        Ok(d) => Ok(Some(d)),
        Err(f) if matches!(f.code.as_deref(), Some("too_large" | "not_text")) => Ok(None),
        Err(f) => Err(f),
    }
}

/// 后端那一趟的结局 → 编辑器那三形。**纯函数**（判得动）。
///
/// - 回了 `text` ⇒ `Some(文本)`；回了却没有 `text` ⇒ 契约不符，报错（**不当成空文本**：
///   空编辑框存回去就是把那份文件清空）。
/// - 🔴 对端拒、码是 `too_large` / `not_text` ⇒ `None`：那是「这份不是一份能编辑的文本」，
///   不是「这一趟没走通」—— 两者分开说（[`not_text_notice`] 那句话里连竞态那一形都说了）。
/// - 其余一律 `Err`（那句话原样）。
pub fn text_from_reply(
    r: Result<cmd_wire::files::ReadTextReply, super::source::Failed>,
) -> Result<Option<String>, super::source::Failed> {
    Ok(reply_from(r)?.map(|got| got.text))
}

/// 存回去 —— 经通道说后端；装不进一行的分块走暂存区。
///
/// 🔴 **路径解析在后端那一层** ⇒ 本模块不自己判一遍（判定只有一个家）。从前这里说「写面那道会话数据围栏」，那道拿掉了。
/// 踩线时那句拒绝原样落进 [`Pane::last_save`]。两支落在盘上是**同一个结果**（后端同一个原地覆盖原语）：
///
/// | 整份装得进一行（[`fits_one_line`]） | 一条 `files-write-text` |
/// |---|---|
/// | 装不进 | 现造一个键 ⇒ [`plan_chunks`] 切块 ⇒ 逐块 `files-stage-chunk`（顺序发）⇒ `files-commit-text` |
///
/// ⚠ 某一块没送成 ⇒ **不发提交**、当场回错（第几段、共几段、原话）；已经送进暂存区的块由后端孤儿扫收，
///   远端那份一个字节没动。编辑框里的字一个不丢（「存失败不清编辑框」照旧，[`Pane::mark_failed`]）。
/// ⚠ 超上限 ⇒ 本地拒、一个字节不发（[`over_cap_notice`]）。
/// 路径可以带字节；一行装不装得下按真要发的那一形量；`cap` ＝ 打开时后端给的编辑上限。
pub async fn write_text_at(
    line: &super::source::Line,
    origin: &super::source::Origin,
    at: &super::source::RemotePath,
    content: &str,
    expect_sha256: &str,
    cap: u64,
) -> Result<Saved, SaveError> {
    if content.len() as u64 > cap {
        return Err(SaveError::Failed(over_cap_notice(content.len(), cap).into()));
    }
    let budget = super::writeops::WRITE_BUDGET;
    let room = "0".repeat(SHA256_HEX_LEN);
    let r = if request_line_len(CMD_WRITE_TEXT, &save_args_at(at, content, &room)) <= SAVE_LINE_CAP
    {
        let req = save_args_at(at, content, expect_sha256);
        super::source::ask_typed(line, origin, &req, budget).await
    } else {
        let key = uuid::Uuid::new_v4().simple().to_string();
        let chunks = plan_chunks(content, chunk_budget());
        for (seq, chunk) in chunks.iter().enumerate() {
            let req = stage_args(&key, seq as u64, chunk);
            super::source::ask_typed(line, origin, &req, budget)
                .await
                .map_err(|f| {
                    SaveError::Failed(chunk_failed_notice(seq, chunks.len(), &f.said).into())
                })?;
        }
        let req = commit_args_at(at, &key, chunks.len(), content.len(), expect_sha256);
        super::source::ask_typed(line, origin, &req, budget).await
    };
    saved_from_reply(content, r)
}

/// 存成了：发出去的那一份 ＋ 后端交的新摘要（下一次存的 `expect`；后端写了却没交像样的摘要 ⇒ `None`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub sent: String,
    pub sha256: Option<String>,
}

/// 存没成的两形 —— 下一步完全不同，不压成一句话：
/// `Stale` = 盘上那份在你打开之后被改过了（让人选：仍然覆盖 / 丢掉重开）；`Failed` = 别的（原话照画）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    Stale(String),
    /// 别的失败：那一句 ＋ 复制详情（对端拒了 ⇒ 那台写的；通道没走通 ⇒ 窗口写的；这一侧拦下的 ⇒ 空）。
    Failed(super::source::Failed),
}

/// 存那一趟的结局 → [`Saved`] / [`SaveError`]。**纯函数**（判得动）。
///
/// 对端回 `stale` ⇒ `Stale`；别的失败 ⇒ `Failed`；成了却没交新摘要（或形状不对）⇒ 仍是存成
/// （后端已经写了），摘要那格 `None`，由 [`Pane::mark_saved`] 挂「重开后再改」。
pub fn saved_from_reply(
    sent: &str,
    r: Result<w::FilesWrittenReply, super::source::Failed>,
) -> Result<Saved, SaveError> {
    match r {
        Ok(d) => Ok(Saved {
            sent: sent.to_string(),
            sha256: Some(d.sha256.0).filter(|s| is_sha256_hex(s)),
        }),
        Err(f) if f.code.as_deref() == Some("stale") => Err(SaveError::Stale(f.said)),
        Err(f) => Err(SaveError::Failed(f)),
    }
}

/// **仍然覆盖**：先问一趟盘上此刻那一份的摘要（重读），拿它当 `expect` 再存。
///
/// CAS 仍在：重读与再存之间又被人改了 ⇒ 照样 `stale`（那时再让人选一次）。
/// 盘上那份此刻不是能编辑的文本（被换成二进制 / 超上限 / 不在了）⇒ 拿不到摘要 ⇒ 不写，说清为什么。
pub async fn overwrite_anyway_at(
    line: &super::source::Line,
    origin: &super::source::Origin,
    at: &super::source::RemotePath,
    content: &str,
    cap: u64,
) -> Result<Saved, SaveError> {
    let path = at.shown.as_str();
    let now = match read_text_at(line, origin, at, cap).await {
        Ok(Some(o)) => o.sha256,
        Ok(None) => {
            return Err(SaveError::Failed(
                copy_text(
                    "rsFilewinEditor.overwrite.notText",
                    &[("why", &not_text_notice(path))],
                )
                .into(),
            ))
        }
        Err(why) => {
            return Err(SaveError::Failed(
                copy_text(
                    "rsFilewinEditor.overwrite.readFailed",
                    &[("why", &why.said)],
                )
                .into(),
            ))
        }
    };
    write_text_at(line, origin, at, content, &now, cap).await
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/editor_tests.rs"]
mod tests;
