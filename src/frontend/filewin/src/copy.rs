//! `24e` 第三刀：**把复制接到原生窗口上**（第二段 ＋ `§5` 第三段第 7 步）。
//!
//! # 🔴复制换走通道：**在那台机器上复制，字节不过网**
//!
//! 第三刀那一版走的是 SFTP 池子那条零流量复制命令（`copy-data` 扩展，协商不到就退回经本机转发、
//! 字节经过用户这台机器一去一回）。窗口成了只经通道说话的独立前端之后，那一条是
//! 「后端缺命令」那一类欠账。现在后端有了 `files-copy`
//! （写面第七条：三条路径各过路径解析〔会话数据围栏 V119 拿掉了〕、缺省 `O_EXCL` 不覆盖、显式 `overwrite` 才经暂存旁名原子顶掉）
//! ⇒ 本层只剩三件：**问一次 · 起一趟 · 把结局摆出来**，一行复制逻辑都没有。
//!
//! 🔴 **「退路必须在界面上出声」那一格因此不在了 —— 不是被删了，是那一形不存在了。**
//! 第三刀那条硬要求（第二段逐字「不许静默退化成 2× 流量」）防的是
//! 「协商不到零流量、悄悄改走经本机转发」。后端在那台机器上本地复制，**没有第二条路**可退：
//! 字节从来不过网 ⇒ 没有「慢路」可喊。那一行警告色字随它的起因一起走了。
//!
//! # 🔴 一、三段的顺序**就是 [`run_copy`] 的结构** —— 照 [`super::transfer::run_drop`] 办
//!
//! ```text
//! ① probe    —— 「那儿已经有这个目标名了吗」（借 `transfer::probe_remote`，问后端 `files-stat`）
//! ② confirm  —— 要覆盖吗，**问一次**（`FnOnce` ⇒ 一半由编译器守）
//! ③ launch   —— 才动手；**覆盖策略由这一问的答案决定**（问过且答了「覆盖」⇒ `overwrite: true`，
//!               没问过 ⇒ `false`，后端 `O_EXCL`：探完之后才冒出来的同名文件照样不会被盖掉）
//! ```
//!
//! ⚠ 与 `run_drop` **刻意不合成一个函数**，理由是**回值类型**不同，不是风格：
//! 上传那一路的 `launch` 回 `Result<(), String>`，复制这一路回 `Result<u64, String>`
//! （复制了几个字节 —— 结局那句话里要说）。
//!
//! # ⚠ 这一刀**买不到**什么（逐条，别读宽）
//!
//! - **一趟真远端上的复制买不到。** 判据里那台后端是合成的（真回环口、真钥匙、真 `dial`，
//!   答话照冻结契约）；后端那一侧的真复制在本机临时目录上真跑过（`files_write_tests`），
//!   两段都真，**连起来经一台真远端**没有读数。
//! - **取消没有了。** 后端那条命令在阻塞档上、开跑之后打不断（`cancel` 回 `not_cancellable`）
//!   ⇒ 窗口上**不画**取消那颗按钮（画了就是一颗按了没用的按钮）。大文件复制因此停不下来 —— 如实登记。
//! - **进度没有了。** 后端一趟做完才回话 ⇒ 窗口上只有「正在复制 …」一行，没有进度条。
//! - **真机上鼠标点那颗「复制」会不会触发买不到**（本机没有图形会话）。判据喂的是合成事件。
//! - **目录复制**（后端 `recursive: true`）与**一摞复制到另一栏**（[`run_copy_batch`]）做了；
//!   「复制为」那个框仍只改名字、仍单选（它要一个名字），名字里不许带 `/`。

use copy_core::copy_text;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::source::Row;

/// 行上那颗按钮的字面。**唯一住址** —— 判据按同一个常量去找它画出来的那几个字，
/// 不在判据里手抄第二份（抄一份就会漂）。
pub static COPY_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinCopy.label.copy", &[]));

/// 这一行能不能复制。**唯一住址** —— 列表画不画那颗按钮（[`super::rows`]）
/// 与状态机接不接那一跳（[`super::shell::FileWindow::begin_copy`]），问的都是这一个函数。
///
/// **目录能复制了**：后端 `files-copy` 带 `recursive: true` 复制整棵（逐条目过路径解析）。
/// 今天不能的只剩一档：
///
/// - **有损名** —— 非 UTF-8 文件名经库有损解码之后**寻址不到真字节**，
///   一切写操作灰置（同旧面板 `panel.ts::mkRowBtn` 的 `disabled = e.lossyName`）。
pub fn is_copyable(r: &Row) -> bool {
    !r.lossy_name
}

/// 〔「有损名的…复制」〕**窗口里一行**能不能复制：名字寻址得到，或者有损但带着原始字节
/// （后端 `files-ls` 送的，`Listed::raw_name`）—— 线上那一形走字节（[`super::source::RemotePath`]）。
/// 行上那颗按钮、右键菜单、「复制到另一栏」问的都是它；[`is_copyable`] 留着给只有那五格的地方。
pub fn copyable(l: &super::source::Listed) -> bool {
    !l.lossy_name || l.raw_name.is_some()
}

/// 一件待复制：**同一台远端、同一个目录**，`from` → `to`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyJob {
    pub from: String,
    pub to: String,
    /// 显示名（＝ 目标那一侧的 basename）。
    pub name: String,
    /// 源是目录 ⇒ 线上带 `recursive: true`（复制整棵）；目录不问覆盖（不合并）。
    pub is_dir: bool,
    /// 〔有损名全寻址〕`from` / `to` 不是合法 UTF-8 时的**整条路径**原始字节（`None` ⇒ 那个串就是真字节）。
    pub from_raw: Option<Vec<u8>>,
    pub to_raw: Option<Vec<u8>>,
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
    /// - 目标算出来**就是源自己** —— 覆盖那一形是「写旁名、再换名顶掉目标」，
    ///   `to == from` 就是拿一份复制品顶掉源自己（后端那一侧也拒，这里先不发）。
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
            is_dir: false,
            from_raw: None,
            to_raw: None,
        })
    }

    /// 源的整条路径（显示串 ＋ 可能有的字节）。
    pub fn from_path(&self) -> super::source::RemotePath {
        super::source::RemotePath::of(&self.from, self.from_raw.as_deref())
    }

    /// 目标的整条路径。
    pub fn to_path(&self) -> super::source::RemotePath {
        super::source::RemotePath::of(&self.to, self.to_raw.as_deref())
    }

    /// 同一件，标上「源是目录」。
    pub fn dir(mut self, is_dir: bool) -> Self {
        self.is_dir = is_dir;
        self
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
/// `Done` 那一支此前背着 SFTP 那一层的裁决（走没走上零流量、
/// 退路过了多少字节）；复制换到后端之后那一问不存在了（见模块头注），背的换成**复制了几个字节**。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CopyOutcome {
    /// 问过「要覆盖吗」，人答了「不覆盖」⇒ **一个字节都没动**。
    Skipped,
    /// 跑完了。
    Done {
        /// 这一趟问过人没有（＝ 目标本来就在，而且人答了「覆盖」）。
        asked: bool,
        /// 后端报的复制字节数。
        bytes: u64,
    },
    /// 起不来 / 半途失败，带原文。
    Failed(String),
    /// 一摞（复制到另一栏）跑完的逐件读数。
    Batch(BatchReport),
    /// 一摞里有**目录**撞了名 ⇒ 整摞一件都没做（目录不覆盖、不合并）。带点了名的那句话。
    Refused(String),
}

/// 一件复制成了：几个文件 · 几个目录 · 几个字节。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Copied {
    pub name: String,
    pub bytes: u64,
    pub files: u64,
    pub dirs: u64,
}

/// 一摞复制的逐件结局。**失败不中断后面的件**（同写操作那一摞「一次问完、逐件做」）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BatchReport {
    pub done: Vec<Copied>,
    /// 撞了名、人答了「都不覆盖」的那几件。
    pub skipped: Vec<String>,
    /// `(名字, 原话)`。
    pub failed: Vec<(String, String)>,
}

/// 🔴 **正题**：一趟复制的全过程。三段的顺序就是这个函数的结构。
///
/// - `probe`：「远端已经有这个目标名了吗」。回 `true` = 已经有了（会覆盖）。
/// - `confirm`：**一次**把「要覆盖吗」交给人。⚠ 它是 `FnOnce` —— 类型上就不许被调第二次。
/// - `launch`：真起那一趟。第二个参数是**覆盖策略**：问过且人答了「覆盖」⇒ `true`；
///   没问过 ⇒ `false`（后端 `O_EXCL`：探完之后才冒出来的同名文件照样不会被盖掉，那一趟回错）。
///   回复制了几个字节。
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
    L: FnOnce(CopyJob, bool) -> LFut,
    LFut: Future<Output = Result<u64, String>>,
{
    // ── ① 问「会不会覆盖」────────────────────────────────────────────────
    let clash = probe(job.clone()).await;

    // ── ② 一次问完 ──────────────────────────────────────────────────────
    if clash && !confirm(job.clone()).await {
        return CopyOutcome::Skipped;
    }

    // ── ③ 才动手：走到这里 `clash` 为真 ⇔ 人答了「覆盖」⇒ 它就是覆盖策略 ─────────
    match launch(job, clash).await {
        Ok(bytes) => CopyOutcome::Done {
            asked: clash,
            bytes,
        },
        Err(e) => CopyOutcome::Failed(e),
    }
}

/// **一摞复制**（复制到另一栏的多选 / 目录）。三段同 [`run_copy`]，只是「问」是**一次**问完整摞：
///
/// ```text
/// ① probe    每一件的目标在不在（逐件问）
/// ①'         撞名的里有**目录** ⇒ 整摞不做、点名（目录不覆盖、不合并）—— 一个字节都没动
/// ② confirm  撞名的**文件**一次交给人（逐件列出）：「覆盖」⇒ 那几件带 `overwrite: true`；「都不覆盖」⇒ 那几件跳过
/// ③ launch   逐件顺序发（后端阻塞档，一趟一件）；失败记下原话，不中断后面的件
/// ```
///
/// ⚠ `confirm` 是 `FnOnce` —— 「只问一次」有一半由编译器守（同 [`run_copy`]）。
pub async fn run_copy_batch<P, PFut, C, CFut, L, LFut>(
    jobs: Vec<CopyJob>,
    probe: P,
    confirm: C,
    launch: L,
) -> CopyOutcome
where
    P: Fn(CopyJob) -> PFut,
    PFut: Future<Output = bool>,
    C: FnOnce(Vec<CopyJob>) -> CFut,
    CFut: Future<Output = bool>,
    L: Fn(CopyJob, bool) -> LFut,
    LFut: Future<Output = Result<Copied, String>>,
{
    let mut clash: Vec<CopyJob> = Vec::new();
    for j in &jobs {
        if probe(j.clone()).await {
            clash.push(j.clone());
        }
    }
    let dirs: Vec<&str> = clash
        .iter()
        .filter(|j| j.is_dir)
        .map(|j| j.name.as_str())
        .collect();
    if !dirs.is_empty() {
        return CopyOutcome::Refused(copy_text(
            "rsFilewinCopy.batch.dirClash",
            &[(
                "names",
                &dirs.join(&copy_text("rsFilewinCopy.batch.listSep", &[])),
            )],
        ));
    }
    let overwrite = !clash.is_empty() && confirm(clash.clone()).await;
    let mut report = BatchReport::default();
    for j in jobs {
        let clashing = clash.contains(&j);
        if clashing && !overwrite {
            report.skipped.push(j.name);
            continue;
        }
        let name = j.name.clone();
        match launch(j, clashing).await {
            Ok(c) => report.done.push(c),
            Err(e) => report.failed.push((name, e)),
        }
    }
    CopyOutcome::Batch(report)
}

/// 一句要画在窗口上的话 ＋ 它的档位。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub text: String,
    /// 警告档（画成警告色）。退路与失败是 `true`。
    pub loud: bool,
}

/// 🔴 **上一趟要摆到用户眼前的那句话。**
///
/// 抽成纯函数不是风格：`CopyBoard::ui` 里那几行 egui 调用判不动「说了什么」，
/// 而这一条**是**能按相等断言判的 —— 而且 [`CopyBoard::ui`] 真的走它
/// （判据一头喂这个函数、一头去 egui 这一帧画出来的文字里找同一句话）。
///
/// 成功那一句说出**复制了几个字节**、**在哪儿复制的**（那台机器上，字节不过网）——
/// 此前那一句是「服务端自己搬的字节，零流量」，意思一样，只是那时还有一条会过网的退路要分开说。
pub fn outcome_notice(o: &CopyOutcome) -> Notice {
    match o {
        CopyOutcome::Skipped => Notice {
            text: copy_text("rsFilewinCopy.outcome.skipped", &[]),
            loud: false,
        },
        CopyOutcome::Failed(e) => Notice {
            text: copy_text("rsFilewinCopy.outcome.failed", &[("e", &e.to_string())]),
            loud: true,
        },
        CopyOutcome::Done { bytes, asked: _ } => Notice {
            text: copy_text(
                "rsFilewinCopy.outcome.done",
                &[("bytes", &bytes.to_string())],
            ),
            loud: false,
        },
        CopyOutcome::Refused(why) => Notice {
            text: copy_text("rsFilewinCopy.outcome.refused", &[("why", why)]),
            loud: true,
        },
        CopyOutcome::Batch(r) => {
            let (files, dirs, bytes) = r.done.iter().fold((0, 0, 0), |(f, d, b), c| {
                (f + c.files, d + c.dirs, b + c.bytes)
            });
            let mut text = copy_text(
                "rsFilewinCopy.outcome.batchDone",
                &[
                    ("n", &r.done.len().to_string()),
                    ("files", &files.to_string()),
                    ("dirs", &dirs.to_string()),
                    ("bytes", &bytes.to_string()),
                ],
            );
            if !r.skipped.is_empty() {
                text.push_str(&copy_text(
                    "rsFilewinCopy.outcome.batchSkipped",
                    &[(
                        "names",
                        &r.skipped
                            .join(&copy_text("rsFilewinCopy.batch.listSep", &[])),
                    )],
                ));
            }
            for (name, why) in &r.failed {
                text.push_str(&copy_text(
                    "rsFilewinCopy.outcome.batchFailed",
                    &[("name", name), ("why", why)],
                ));
            }
            Notice {
                text,
                loud: !r.failed.is_empty(),
            }
        }
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
pub async fn probe_target(
    line: &super::source::Line,
    origin: &super::source::Origin,
    job: &CopyJob,
) -> bool {
    // 目标有损 ⇒ 按字节问（`to_path().wire()`；合法 UTF-8 时就是 `overwrite_target()` 那个串）。
    super::transfer::probe_remote_at(line, origin, job.to_path().wire()).await
}

/// 复制那条线上命令的名字（后端写面第七条）。
pub const CMD_COPY: &str = "files-copy";

/// 一趟复制的往返上限（调用方给的期限）。
///
/// ⚠ 比写面其余几条（[`super::writeops::WRITE_BUDGET`]）宽：复制一份大文件在那台机器上
/// 要搬满它的字节，而这一趟**取消不掉**（后端阻塞档）——期限就是它唯一的上界。
pub const COPY_BUDGET: std::time::Duration = std::time::Duration::from_secs(600);

/// 一件复制 → `files-copy` 的参数。**纯函数**（判得动）。
///
/// 路径切成 `(root, rel)` 与写面其余几条同形（[`super::writeops::apply_remote`] 头注）：
/// `root` = 源的上一级，`from` / `to` = 两个尾段。
///
/// 🔴 **目标不在源的同一个目录里 ⇒ 报错，不发。** [`CopyJob::beside`] 造出来的都是同目录的；
/// 这一道防的是「当前目录」与「那一行的路径」写法不一致（尾斜杠之类）时，
/// 拼出一条落到别处的路径 —— 那是把文件放到了他没在看的目录里。
pub fn copy_args(job: &CopyJob, overwrite: bool) -> Result<serde_json::Value, String> {
    // 〔有损名全寻址〕按**字节**切（合法 UTF-8 时与按串切逐字节同）。
    let (from, to) = (job.from_path(), job.to_path());
    let root = from.parent();
    if to.parent() != root {
        return Err(copy_text(
            "rsFilewinCopy.args.notSameDir",
            &[("from", &job.from.to_string()), ("to", &job.to.to_string())],
        ));
    }
    let mut v = serde_json::json!({
        "root": root.wire(),
        "from": from.tail_wire(),
        "to": to.tail_wire(),
        "overwrite": overwrite,
    });
    mark_recursive(&mut v, job);
    Ok(v)
}

/// 源是目录 ⇒ 参数里加 `recursive: true`（后端复制整棵；与 `overwrite: true` 同给会被拒 ——
/// 目录那一件从来不问覆盖，所以这里的 `overwrite` 恒是 `false`）。文件那一形一个键都不多。
pub fn mark_recursive(v: &mut serde_json::Value, job: &CopyJob) {
    if job.is_dir {
        v["recursive"] = serde_json::Value::Bool(true);
    }
}

/// `files-copy` 的应答 → [`Copied`]。`bytes` 必须在；`files` / `dirs` 是这一波新加的键 ——
/// 文件那一件在旧后端上没有它们 ⇒ 按「一个文件」记（旧后端只会复制文件）；目录那一件必须有（旧后端根本不会复制目录）。
pub fn copied_from_reply(job: &CopyJob, d: &serde_json::Value) -> Result<Copied, String> {
    let bytes = d
        .get("bytes")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| copy_text("rsFilewinCopy.remote.noBytes", &[]))?;
    let count = |k: &str, old: u64| -> Result<u64, String> {
        match d.get(k).and_then(serde_json::Value::as_u64) {
            Some(n) => Ok(n),
            None if !job.is_dir => Ok(old),
            None => Err(copy_text("rsFilewinCopy.remote.noCount", &[("field", k)])),
        }
    };
    Ok(Copied {
        name: job.name.clone(),
        bytes,
        files: count("files", 1)?,
        dirs: count("dirs", 0)?,
    })
}

/// 真起一趟复制 —— 经通道问后端 `files-copy`。
///
/// 回复制了几个字节。**路径解析在后端**（三条路径各过一次；会话数据围栏 V119 拿掉了），本层不自己判一遍
/// （判定只有一个家）；踩线时那句拒绝原样变成 [`CopyOutcome::Failed`]。
///
/// ⚠ 上一版这里调的是 SFTP 池子那条零流量复制命令（同进程直调一个 Tauri 命令，
/// 带进度通道与取消键）；那两样随它一起没了（后端这一趟没有进度、取消不掉，见模块头注）。
pub async fn copy_remote(
    line: &super::source::Line,
    origin: &super::source::Origin,
    job: &CopyJob,
    overwrite: bool,
) -> Result<u64, String> {
    let args = copy_args(job, overwrite)?;
    let d = super::source::ask(line, origin, CMD_COPY, &args, COPY_BUDGET).await?;
    d.get("bytes")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| copy_text("rsFilewinCopy.remote.noBytes", &[]))
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
    /// 源是目录（复制整棵）。
    pub is_dir: bool,
    /// 〔有损名全寻址〕源 / 当前目录不是合法 UTF-8 时的原始字节（窗口按行与当前目录填）。
    pub from_raw: Option<Vec<u8>>,
    pub dir_raw: Option<Vec<u8>>,
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
            is_dir: r.is_dir,
            from_raw: None,
            dir_raw: None,
        }
    }

    /// 框里那个名字变成一趟真复制。名字不合法 ⇒ `None`（调用方据此**出声**）。
    pub fn to_job(&self) -> Option<CopyJob> {
        let mut job = CopyJob::beside(&self.from, &self.dir, &self.new_name)?.dir(self.is_dir);
        // 有损那一侧：源的字节原样带上；目标 ＝ 当前目录的字节 ＋ `/` ＋ 新名字（新名字恒是框里敲的 UTF-8）。
        job.from_raw = self.from_raw.clone();
        if let Some(d) = &self.dir_raw {
            let mut t = d.clone();
            if t.last() != Some(&b'/') {
                t.push(b'/');
            }
            t.extend_from_slice(self.new_name.trim().as_bytes());
            job.to_raw = Some(t);
        }
        Some(job)
    }
}

/// 一趟复制在窗口上的样子。**跨线程共享**（UI 线程画，tokio 那条写）。
///
/// 形状照 [`super::transfer::DropBoard`] 办，含那只「敲窗口的手」——
/// egui 只在有事发生时才画下一帧，不敲一下进度条要等用户动鼠标才跳一格
/// （「卡住了」与「真的没在跑」在屏幕上分不开）。
#[derive(Clone, Default)]
pub struct CopyBoard {
    inner: Arc<Mutex<Board>>,
    /// 已经跑完的趟数 —— 给判据与「跑完要重列目录」一个可观测的数。
    rounds: Arc<AtomicU64>,
    ctx: Arc<Mutex<Option<egui::Context>>>,
    // 这里原来还有一张取消台（第五刀）：复制换到后端之后那一趟
    //   **取消不掉**（阻塞档），留着它就是一颗按了没用的按钮 ⇒ 随 SFTP 那条路一起摘了。
}

#[derive(Default)]
struct Board {
    /// 正摆在人面前等答复的那几件（空 = 没在问）。一摞复制一次问完，逐件列出。
    asking: Vec<CopyJob>,
    /// 答复往哪儿送。
    answer: Option<tokio::sync::oneshot::Sender<bool>>,
    /// 在跑的那一件（名字）。后端一趟做完才回话 ⇒ 没有进度，只有「在跑」。
    running: Option<String>,
    /// 上一趟的裁决（**画在窗口上**，不是 `println!`）。
    last: Option<CopyOutcome>,
}

impl CopyBoard {
    /// 摆出「要覆盖吗」，并交出「答复送哪儿」那一头。
    pub fn ask(&self, job: CopyJob) -> tokio::sync::oneshot::Receiver<bool> {
        self.ask_many(vec![job])
    }

    /// 一次摆出「这几件要覆盖吗」。
    pub fn ask_many(&self, jobs: Vec<CopyJob>) -> tokio::sync::oneshot::Receiver<bool> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut b = self.inner.lock().unwrap();
            b.asking = jobs;
            b.answer = Some(tx);
        }
        self.poke(); // 问题要立刻画出来，别等下一次鼠标动
        rx
    }

    pub fn is_asking(&self) -> bool {
        !self.inner.lock().unwrap().asking.is_empty()
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

    /// 这一趟真起了（问完了、要发了）。
    pub fn begin(&self, name: &str) {
        self.inner.lock().unwrap().running = Some(name.to_string());
        // ⚠ 锁放掉之后才敲 —— `request_repaint` 会走进 egui 自己的锁。
        self.poke();
    }

    /// 在跑的那一件（`None` = 没在跑）。
    pub fn running(&self) -> Option<String> {
        self.inner.lock().unwrap().running.clone()
    }

    pub fn finish(&self, outcome: CopyOutcome) {
        {
            let mut b = self.inner.lock().unwrap();
            b.running = None;
            b.asking.clear();
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
        b.asking.clear();
        drop(b);
        tx.send(overwrite).is_ok()
    }

    /// 画覆盖确认框 · 「正在复制」· **上一趟的结局**（从 [`outcome_notice`] 出来，原样画上去）。
    pub fn ui(&self, ui: &mut egui::Ui) {
        let (asking, running, last) = {
            let b = self.inner.lock().unwrap();
            (b.asking.clone(), b.running.clone(), b.last.clone())
        };
        if !asking.is_empty() {
            let mut answer: Option<bool> = None;
            egui::Modal::new(egui::Id::new("filewin-copy-overwrite")).show(ui.ctx(), |ui| {
                if let [job] = asking.as_slice() {
                    ui.heading(copy_text(
                        "rsFilewinCopy.ui.askOverwrite",
                        &[("name", &job.name.to_string())],
                    ));
                } else {
                    ui.heading(copy_text(
                        "rsFilewinCopy.ui.askOverwriteMany",
                        &[("n", &asking.len().to_string())],
                    ));
                }
                for job in &asking {
                    ui.label(format!("{} → {}", job.from, job.to));
                }
                ui.horizontal(|ui| {
                    if ui
                        .button(&copy_text("rsFilewinCopy.ui.overwrite", &[]))
                        .clicked()
                    {
                        answer = Some(true);
                    }
                    if ui
                        .button(&copy_text("rsFilewinCopy.ui.keep", &[]))
                        .clicked()
                    {
                        answer = Some(false);
                    }
                });
            });
            if let Some(ok) = answer {
                self.settle(ok);
            }
        }
        if let Some(name) = &running {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(copy_text(
                    "rsFilewinCopy.ui.copying",
                    &[("name", &name.to_string())],
                ));
            });
        }
        if let Some(o) = &last {
            let n = outcome_notice(o);
            if n.loud {
                ui.colored_label(ui.visuals().warn_fg_color, n.text);
            } else {
                ui.label(n.text);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/copy_testing.rs"]
pub(crate) mod testing;

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/copy_tests.rs"]
mod tests;
