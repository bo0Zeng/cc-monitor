//! `24e` 第九刀：**改一份远端文本** —— `sftp_read_text_for_edit`〔散文墓碑〕 ＋ `sftp_write_text`。
//!
//! 🔴〔F7a · 第三波 2026-09-24〕**读写两半都经通道问后端了**：写那一半 F2 已换成
//! `files-write-text`；读那一半这一拍换成 `files-read-text`（[`read_text`]）。标题里那两条
//! 池子命令是第九刀当时的住址，下面几节讲「上限」「超了怎么办」的推理照旧成立 ——
//! 变的只是「最终护栏」住哪：从池子那边的解码函数换成后端那条命令（它自己判两次大小）。
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
//! `sftp_read_text_for_edit`〔散文墓碑〕 回的 `Option<String>` 把**三件事压成一件**：
//! 太大 / 含 NUL / 非 UTF-8 全是 `None` ⇒ **谁拿到 `None` 都说不出为什么**。
//! 老面板的做法是把那一行灰置 —— 而「灰置」与「这个功能坏了」在屏幕上同形。
//!
//! 🔴 **而那三件里最常见的那一件，窗口自己就判得出来**：列目录回来的每一行
//! 都带着 `size` ⇒ [`why_not_editable`] 按 [`MAX_EDIT_BYTES`]
//! 在**本地**判「太大」，**连那趟往返都不发**，而且把那个数说给用户听
//! （「这份 1.2 M 超过 256 K 的编辑上限」）。
//!
//! ⇒ 于是 `None` 只剩下两种可能（含 NUL / 非 UTF-8），而那两种本来就是同一句话
//! （「这不是一份文本文件」）⇒ **三件事分成了两句人话，零签名改动、零额外往返。**
//!
//! ⚠ 如实登记它**买不到**什么：远端那个文件在「我们读 `size`」与「我们真去读它」
//! 之间被换掉（变大 / 变成二进制）⇒ 本地预判会放它过去，而〔F7a〕后端 `files-read-text`
//! 仍然是最终护栏（它在那台机器上再判两次大小，正是为这个竞态；第九刀时这一格住池子那边）。
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
//! `调研/设计/60 §9`；落地的形状、两个阈值的推算、判据与买不到的，住 `设计/60 §9b`
//! 与 [`super::bigfile`] 头注。
//!
//! ⚠ 第十四刀留在这里的那一组内核（行索引 · 窗口 · 写回 · 「开窗买不到」那把尺子）
//! **随落地一起删了**：它们建模的是「窗口化 `TextEdit`」那条路，而落地走的是另一条
//! （自己画、全文坐标、横向也只排可见段）—— 那条路的撤销栈会静默改坏文件
//! （`§9 §四.2` 第 3 条），一行特别长时也买不到东西（第 10 条），留着只会让人以为它还是候选。
//! 行结构改由 [`super::bigfile::Lines`] 增量维护，判据对着 `str::split('\n')` 钉。

use crate::copy_table::copy_text;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// 编辑上限 ＝ **存得回、也读得回来的量**（8 MiB）。**超上限拒编而非截断**（截断过的文本当编辑源会写坏文件）。
///
/// 🔴〔F9c · 第四波 · 2026-09-24〕**1 MiB → 8 MiB**。上一版（F9 续）这个数被钉成后端入方向**一行**的上限，
/// 因为存盘把整份内容装在一条请求行里；现在装不进一行的那几份**分块走暂存区**（[`write_text`]），
/// 一行的上限不再管「能存多大」⇒ 能存多大改由后端提交那一条的天花板定：`files-commit-text` 只收
/// `src/backend/files/mod.rs::READ_TEXT_MAX_BYTES` 以内（存得回的要读得回来），而读那一趟的天花板也是它。
/// ⇒ 本常量**就是**那个数，一个数三处用：打开前按大小拒（[`why_not_editable`]）· 读那一趟的 `max_bytes` ·
///   存之前按字节数拒（[`Pane::over_cap`]，与后端 `files-commit-text` 拒 `bytes` 的那一关逐字节同一个界）。
/// ⚠ 两个 crate 之间引不到对方 ⇒ 由 `byte_cap_registry::the_cross_crate_twins_are_machine_checked_not_hand_copied`
///   读两侧源码**对拍相等**（对 E）。
/// ⚠ 界面那一侧 8 MiB 打得开、打得动字：`设计/60 §9c.1` 的读数（release，经窗口生产路径，大文件模式
///   打开约 20 ms、每键 ≤ 3.03 ms）。
pub const MAX_EDIT_BYTES: usize = 8 * 1024 * 1024;

/// 🔴〔F9c〕**一条请求行最多多长** ＝ 后端入方向一行的上限（`src/backend/inbound.rs::MAX_LINE_BYTES`，
/// 审计加的防 OOM 线，所有命令共用；多一个字节整行丢弃）。
///
/// 存盘用它分两支：整份装得进一行 ⇒ 一条 `files-write-text`；装不进 ⇒ 按它切块（[`plan_chunks`]），
/// 每块那一行都装得进。与后端那一处由 `byte_cap_registry` 对 F 读两侧源码钉相等：
/// 窗口多给一个字节 ⇒ 后端整行丢弃；少给 ⇒ 只是多切几块。
pub const SAVE_LINE_CAP: usize = 1 << 20;

/// 存盘那条线上命令的名字（装得进一行的那一支）。量的与发的必须是**同一条**命令同一份参数（[`save_args`]）。
pub const CMD_WRITE_TEXT: &str = "files-write-text";

/// 〔F9c〕装不进一行的那一支：逐块进暂存区（后端 `control/files_commit.rs`）。
pub const CMD_STAGE_CHUNK: &str = "files-stage-chunk";

/// 〔F9c〕装不进一行的那一支：读回拼起来、原地覆盖（与 `files-write-text` 同一个原语）。
pub const CMD_COMMIT_TEXT: &str = "files-commit-text";

/// 请求行里 `id` 最长能占多少字节 —— monitor 那一侧 `inbound_client` 发号的形状是
/// `m{毫秒:x}.{连接号}-{序号}`：`m` ＋ u128 十六进制最多 32 ＋ `.` ＋ u64 十进制最多 20 ＋ `-` ＋ 20 ＝ 75。
/// 窗口量的时候按这个最长的 id 算 ⇒ **只会比真发出去的那一行长、不会短**（最多保守 75 字节）。
pub const REQUEST_ID_ROOM: usize = 1 + 32 + 1 + 20 + 1 + 20;

/// 存盘那一趟的参数（路径切成 `(root, rel)` 与写面其余四条同形）。
///
/// 〔FW1 · 第四波 4D · D-c〕`expect` = 「我打开时那一份」的摘要（`files-read-text` 交的、或上一次存成时应答交的），
/// 后端比盘上此刻那一份、对不上就 `stale`、一个字节不写。窗口只把它当不透明令牌原样交回（算法住后端一处）。
pub fn save_args(path: &str, content: &str, expect_sha256: &str) -> serde_json::Value {
    serde_json::json!({
        "root": super::source::parent_dir(path),
        "rel": super::source::remote_basename(path),
        "content": content,
        "expect": { "sha256": expect_sha256 },
    })
}

/// 〔F9c〕送一块的参数。`key` 是这一次存盘现造的 32 位小写十六进制（后端只收这个形状）。
pub fn stage_args(key: &str, seq: u64, chunk: &str) -> serde_json::Value {
    serde_json::json!({ "key": key, "seq": seq, "content": chunk })
}

/// 〔F9c〕提交那一趟的参数：块数与总字节数**显式**给，后端读回来必须对得上。
pub fn commit_args(
    path: &str,
    key: &str,
    chunks: usize,
    bytes: usize,
    expect_sha256: &str,
) -> serde_json::Value {
    serde_json::json!({
        "root": super::source::parent_dir(path),
        "rel": super::source::remote_basename(path),
        "key": key,
        "chunks": chunks,
        "bytes": bytes,
        "expect": { "sha256": expect_sha256 },
    })
}

/// 请求行的形状（字段顺序与 monitor 那一侧 `inbound_client::encode_request` 的 `RequestLine` 相同；
/// 两者逐字节相等由判据对拍）。
#[derive(serde::Serialize)]
struct RequestLine<'a> {
    id: &'a str,
    cmd: &'a str,
    args: &'a serde_json::Value,
}

/// 🔴 **真序列化一次**：这条命令发到后端时那一行有多少字节（**不含**行尾 `\n` ——
/// 后端的上限数的就是换行之前那一段）。`id` 按最长的算（[`REQUEST_ID_ROOM`]）。
pub fn request_line_len(cmd: &str, args: &serde_json::Value) -> usize {
    let id = "0".repeat(REQUEST_ID_ROOM);
    serde_json::to_vec(&RequestLine { id: &id, cmd, args }).map_or(usize::MAX, |v| v.len())
}

/// 〔F9c〕整份装得进**一条** `files-write-text` 吗（真序列化量，按最长 id）。
/// 〔FW1〕`expect` 那一格定长（摘要恒 [`SHA256_HEX_LEN`] 位），按一份同长的占位量 —— 与真发的逐字节同长。
pub fn fits_one_line(path: &str, content: &str) -> bool {
    let room = "0".repeat(SHA256_HEX_LEN);
    request_line_len(CMD_WRITE_TEXT, &save_args(path, content, &room)) <= SAVE_LINE_CAP
}

/// 〔FW1〕后端 CAS 摘要（SHA-256）的十六进制长度。窗口不算摘要，只认形状：读回来的那一格不是这个形状 ⇒ 不打开
/// （存不回去的编辑面不该立起来）。与后端 `files::SHA256_HEX_LEN` 同一个数（SHA-256 的定义，不是可调的量）。
pub const SHA256_HEX_LEN: usize = 64;

/// 〔FW1〕这一格像不像后端交的摘要（64 位小写十六进制）。
pub fn is_sha256_hex(s: &str) -> bool {
    s.len() == SHA256_HEX_LEN
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// 〔FW1〕存盘那一问的「盘上那份在你打开之后被改过了」那句（`stale` 那一档；后端原话接在后面）。
pub fn stale_notice(why: &str) -> String {
    copy_text("rsFilewinEditor.stale.notice", &[("why", why)])
}

/// 〔F9c〕一个字放进 JSON 字符串之后占几个字节 —— 与 `serde_json` 的转义**逐码位**相同
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

/// 〔F9c〕每块内容（转义之后）最多占多少字节 ＝ 一行上限 − 空内容那一块的信封（块号按最大的算）。
pub fn chunk_budget() -> usize {
    let key = "0".repeat(32);
    SAVE_LINE_CAP - request_line_len(CMD_STAGE_CHUNK, &stage_args(&key, u64::MAX, ""))
}

/// 〔F9c〕把全文按字切成几块：每块转义之后 ≤ `budget`，**贪心取满**（下一块的第一个字放不进上一块）。
///
/// 按字符边界切 ⇒ 每块都是合法 UTF-8、能原样作为 JSON 字符串发出去；拼回来逐字节等于原文。
/// `budget` 至少要放得下一个最长的字（6 字节），否则一块都切不出来 —— 那是调用方的错，这里 `assert`。
pub fn plan_chunks(content: &str, budget: usize) -> Vec<&str> {
    assert!(budget >= 6, "plan_chunks: budget {budget} < 6");
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

/// 敲超上限、存的时候本地拒那句话（一个字节都没发）。
pub fn over_cap_notice(len: usize) -> String {
    copy_text(
        "rsFilewinEditor.overCap.notice",
        &[
            ("len", &len.to_string()),
            ("limit", &MAX_EDIT_BYTES.to_string()),
            ("over", &(len.saturating_sub(MAX_EDIT_BYTES)).to_string()),
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

use super::source::Row;

/// 行上那颗按钮。
pub static EDIT_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinEditor.label.edit", &[]));
/// 〔FW1 · D-c〕存盘撞上 stale 之后那两颗按钮（住这里：判据按名字点它们，不在画的地方另写一遍）。
pub static OVERWRITE_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinEditor.stale.overwrite", &[]));
/// 〔FW1 · D-c〕同上，另一颗。
pub static REOPEN_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinEditor.stale.reopen", &[]));

/// 这一行**为什么**改不了（`None` = 改得了）。
///
/// 🔴 回 `Option<String>` 而不是 `bool`：那句话就是这一刀买到的东西
///（见头注「超了怎么办」那一节）。`bool` 只够画一个灰按钮。
pub fn why_not_editable(r: &Row) -> Option<String> {
    if r.is_dir {
        return Some(copy_text("rsFilewinEditor.notEditable.dir", &[]).into());
    }
    if r.lossy_name {
        // 与 `is_copyable` / `is_writable` 同一条：名字不是合法 UTF-8 ⇒ 寻址不到真字节。
        return Some(copy_text("rsFilewinEditor.notEditable.badName", &[]).into());
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
        return Some(copy_text(
            "rsFilewinEditor.notEditable.tooBig",
            &[
                ("size", &(super::rows::human_size(r.size)).to_string()),
                ("bytes", &r.size.to_string()),
                (
                    "limit",
                    &(super::rows::human_size(MAX_EDIT_BYTES as u64)).to_string(),
                ),
                ("over", &over.to_string()),
            ],
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

/// 后端说「不可编辑」之后那句话（第九刀时是池子回了 `None`）。
///
/// 🔴 **「太大」不在这里** —— 那一档由 [`why_not_editable`] 在发往返**之前**挡掉。
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
    /// 〔FW1 · D-c〕**盘上那一份的摘要**（打开时后端交的；每次存成换成应答交的新摘要）—— 存盘 CAS 的 `expect`。
    base_sha256: String,
    /// 上一次存盘的结局（`None` = 还没存过）。
    pub last_save: Option<Result<(), String>>,
    /// 〔FW1 · D-c〕上一次存盘撞上了「盘上那份在你打开之后被改过了」（`stale`）⇒ 编辑面摆两颗按钮让人选
    /// （仍然覆盖 · 丢掉我的改动重新打开）。存成 / 重开之后清掉。
    pub stale: bool,
    /// 〔F9〕大文件模式那一格（`None` 在里面 ＝ 普通路径）。逐条住 [`super::bigfile`] 头注。
    pub(crate) big: super::bigfile::BigSlot,
}

impl Pane {
    pub fn opened(path: &str, name: &str, text: String, sha256: String) -> Self {
        Self {
            path: path.to_string(),
            name: name.to_string(),
            original: text.clone(),
            text,
            base_sha256: sha256,
            last_save: None,
            stale: false,
            big: Default::default(),
        }
    }

    /// 〔FW1〕存盘那一趟该交的 `expect`（盘上那一份的摘要）。
    pub fn expect_sha256(&self) -> &str {
        &self.base_sha256
    }

    /// 改过了吗。**相等断言**，不是一个「用户敲过键」的标志位 ——
    /// 敲进去又改回来**不算改过**，而一个标志位会把那一形报成「有未保存改动」。
    pub fn dirty(&self) -> bool {
        self.text != self.original
    }

    /// 存成功了 ⇒ 基准线跟上（从此 [`Self::dirty`] 回 `false`）；〔FW1〕摘要换成后端应答交的那份（写进去那份的）
    /// ⇒ 连存两次不自撞。
    ///
    /// ⚠ 基准线是**发出去的那一份**（`sent`），不是此刻的 `text`：存在路上时用户又敲了字，那几个字没存过，
    /// 该算「改过了」。
    pub fn mark_saved(&mut self, sent: &str, sha256: String) {
        self.original = sent.to_string();
        self.base_sha256 = sha256;
        self.last_save = Some(Ok(()));
        self.stale = false;
    }

    /// 〔FW1〕存盘撞上 `stale` ⇒ 同 [`Self::mark_failed`]（字一个不动、基准不动），外加摆出那两颗按钮。
    pub fn mark_stale(&mut self, why: String) {
        self.last_save = Some(Err(stale_notice(&why)));
        self.stale = true;
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

    /// 🔴 **敲超上限了吗。** 超上限的内容存不回去（〔F9c〕后端 `files-commit-text` 拒 `bytes` 超过
    /// 同一个数的那一趟；装得进一行的那一支写面不拦大小，但存回去之后下次就读不回来编辑了）
    /// —— 所以要在屏幕上先说，并且不发那一趟。
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
        /// 〔FW1〕后端对那份字节算的摘要（存盘 CAS 的 `expect`）。
        sha256: String,
    },
    /// 后端说它不可编辑（那句话由 [`not_text_notice`] 给）。
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
    /// 存的结局（UI 线程取走）。〔FW1〕成 ⇒ 发出去的那一份 ＋ 后端交的新摘要。
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

/// 读一份文本那一趟的往返上限（调用方给的期限，`05 §3.3.2`）。
pub const READ_BUDGET: std::time::Duration = std::time::Duration::from_secs(20);

/// 读一份远端文本 —— 〔F7a · 第三波 2026-09-24〕经通道问后端 `files-read-text`。
///
/// 回值：`Ok(Some(文本))` / `Ok(None)` = 后端说它不可编辑（那句话由
/// [`not_text_notice`] 给）/ `Err` = 那句原话（没走通 / 读不到 …）。
///
/// ⚠ 上一版这里调的是池子那条读文本命令（SFTP 把字节整份搬过来）；那是窗口进程里
/// 「跨机传输」那一类欠账的一条。现在字节**在那台机器上**读、只把文本经通道交回来。
/// ⚠ 有逻辑的那一段（哪几个码算「不可编辑」）住 [`text_from_reply`]。
pub async fn read_text(
    line: &super::source::Line,
    origin: &super::source::Origin,
    path: &str,
) -> Result<Option<Opened>, String> {
    let args = serde_json::json!({ "path": path, "max_bytes": MAX_EDIT_BYTES });
    opened_from_reply(
        super::source::ask_coded(line, origin, CMD_READ_TEXT, &args, READ_BUDGET).await,
    )
}

/// 〔FW1〕打开那一趟交回来的：全文 ＋ 后端对那份字节算的摘要。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opened {
    pub text: String,
    pub sha256: String,
}

/// 〔FW1〕[`text_from_reply`] ＋ 摘要那一格。**纯函数**。
///
/// 有文本却没有摘要（或形状不对）⇒ `Err`：没有它就存不回去（存盘必带「我打开时那一份」，后端不收不带的），
/// 立起一个存不回去的编辑面比不打开更糟 —— 那句话说清是后端太旧。
pub fn opened_from_reply(
    r: Result<serde_json::Value, super::source::Failed>,
) -> Result<Option<Opened>, String> {
    let sha = r
        .as_ref()
        .ok()
        .and_then(|d| d.get("sha256"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let Some(text) = text_from_reply(r)? else {
        return Ok(None);
    };
    let sha256 = sha.filter(|s| is_sha256_hex(s)).ok_or_else(|| {
        copy_text("rsFilewinEditor.reply.noDigest", &[])
    })?;
    Ok(Some(Opened { text, sha256 }))
}

/// 后端那一趟的结局 → 编辑器那三形。**纯函数**（判得动）。
///
/// - 回了 `text` ⇒ `Some(文本)`；回了却没有 `text` ⇒ 契约不符，报错（**不当成空文本**：
///   空编辑框存回去就是把那份文件清空）。
/// - 🔴 对端拒、码是 `too_large` / `not_text` ⇒ `None`：那是「这份不是一份能编辑的文本」，
///   不是「这一趟没走通」—— 两者分开说（[`not_text_notice`] 那句话里连竞态那一形都说了）。
/// - 其余一律 `Err`（那句话原样）。
pub fn text_from_reply(
    r: Result<serde_json::Value, super::source::Failed>,
) -> Result<Option<String>, String> {
    match r {
        Ok(d) => d
            .get("text")
            .and_then(serde_json::Value::as_str)
            .map(|t| Some(t.to_string()))
            .ok_or_else(|| copy_text("rsFilewinEditor.reply.noText", &[])),
        Err(f) if matches!(f.code.as_deref(), Some("too_large" | "not_text")) => Ok(None),
        Err(f) => Err(f.said),
    }
}

/// 存回去 —— 〔F2 · 2026-09-24〕经通道说后端；〔F9c · 第四波〕装不进一行的分块走暂存区。
///
/// 🔴 **路径解析在后端那一层** ⇒ 本模块不自己判一遍（判定只有一个家）。〔FN1 · V119〕从前这里说「写面那道会话数据围栏」，那道拿掉了。
/// 踩线时那句拒绝原样落进 [`Pane::last_save`]。两支落在盘上是**同一个结果**（后端同一个原地覆盖原语）：
///
/// | 整份装得进一行（[`fits_one_line`]） | 一条 `files-write-text` |
/// |---|---|
/// | 装不进 | 现造一个键 ⇒ [`plan_chunks`] 切块 ⇒ 逐块 `files-stage-chunk`（顺序发）⇒ `files-commit-text` |
///
/// ⚠ 某一块没送成 ⇒ **不发提交**、当场回错（第几段、共几段、原话）；已经送进暂存区的块由后端孤儿扫收，
///   远端那份一个字节没动。编辑框里的字一个不丢（「存失败不清编辑框」照旧，[`Pane::mark_failed`]）。
/// ⚠ 超上限 ⇒ 本地拒、一个字节不发（[`over_cap_notice`]）。
pub async fn write_text(
    line: &super::source::Line,
    origin: &super::source::Origin,
    path: &str,
    content: &str,
    expect_sha256: &str,
) -> Result<Saved, SaveError> {
    if content.len() > MAX_EDIT_BYTES {
        return Err(SaveError::Failed(over_cap_notice(content.len())));
    }
    let budget = super::writeops::WRITE_BUDGET;
    let r = if fits_one_line(path, content) {
        let args = save_args(path, content, expect_sha256);
        super::source::ask_coded(line, origin, CMD_WRITE_TEXT, &args, budget).await
    } else {
        let key = uuid::Uuid::new_v4().simple().to_string();
        let chunks = plan_chunks(content, chunk_budget());
        for (seq, chunk) in chunks.iter().enumerate() {
            let args = stage_args(&key, seq as u64, chunk);
            super::source::ask(line, origin, CMD_STAGE_CHUNK, &args, budget)
                .await
                .map_err(|why| SaveError::Failed(chunk_failed_notice(seq, chunks.len(), &why)))?;
        }
        let args = commit_args(path, &key, chunks.len(), content.len(), expect_sha256);
        super::source::ask_coded(line, origin, CMD_COMMIT_TEXT, &args, budget).await
    };
    saved_from_reply(content, r)
}

/// 〔FW1〕存成了：发出去的那一份 ＋ 后端交的新摘要（下一次存的 `expect`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub sent: String,
    pub sha256: String,
}

/// 〔FW1〕存没成的两形 —— 下一步完全不同，不压成一句话：
/// `Stale` = 盘上那份在你打开之后被改过了（让人选：仍然覆盖 / 丢掉重开）；`Failed` = 别的（原话照画）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    Stale(String),
    Failed(String),
}

/// 〔FW1〕存那一趟的结局 → [`Saved`] / [`SaveError`]。**纯函数**（判得动）。
///
/// 对端回 `stale` ⇒ `Stale`；成了却没交新摘要（或形状不对）⇒ `Failed`：后端已经写了，但下一次存交不出
/// 「盘上那份」，那句话照实说（关掉重开就好）。
pub fn saved_from_reply(
    sent: &str,
    r: Result<serde_json::Value, super::source::Failed>,
) -> Result<Saved, SaveError> {
    match r {
        Ok(d) => d
            .get("sha256")
            .and_then(serde_json::Value::as_str)
            .filter(|s| is_sha256_hex(s))
            .map(|s| Saved {
                sent: sent.to_string(),
                sha256: s.to_string(),
            })
            .ok_or_else(|| {
                SaveError::Failed(copy_text("rsFilewinEditor.saved.noDigest", &[]))
            }),
        Err(f) if f.code.as_deref() == Some("stale") => Err(SaveError::Stale(f.said)),
        Err(f) => Err(SaveError::Failed(f.said)),
    }
}

/// 〔FW1 · D-c〕**仍然覆盖**：先问一趟盘上此刻那一份的摘要（重读），拿它当 `expect` 再存。
///
/// CAS 仍在：重读与再存之间又被人改了 ⇒ 照样 `stale`（那时再让人选一次）。
/// 盘上那份此刻不是能编辑的文本（被换成二进制 / 超上限 / 不在了）⇒ 拿不到摘要 ⇒ 不写，说清为什么。
pub async fn overwrite_anyway(
    line: &super::source::Line,
    origin: &super::source::Origin,
    path: &str,
    content: &str,
) -> Result<Saved, SaveError> {
    let now = match read_text(line, origin, path).await {
        Ok(Some(o)) => o.sha256,
        Ok(None) => {
            return Err(SaveError::Failed(copy_text(
                "rsFilewinEditor.overwrite.notText",
                &[("why", &not_text_notice(path))],
            )))
        }
        Err(why) => {
            return Err(SaveError::Failed(copy_text(
                "rsFilewinEditor.overwrite.readFailed",
                &[("why", &why)],
            )))
        }
    };
    write_text(line, origin, path, content, &now).await
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/editor_tests.rs"]
mod tests;
