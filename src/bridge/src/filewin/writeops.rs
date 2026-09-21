//! `24e` 第五刀：**新建目录 · 删除 · 改名 · 改权限**接到原生窗口上
//! （`设计/99 §4.6.4` 的前半，用户原话「我能连 ssh 对机器文件进行什么操作，
//! 后端就应该能进行什么操作」）。
//!
//! # 🔴 一、这一层**没有**一行写代码
//!
//! 那四条命令在池子里早就有了（`sftp_pool` 的 `sftp_mkdir` / `sftp_delete` /
//! `sftp_rename` / `sftp_chmod`），每一条的第一行都是 `guard_write`。
//! **这一刀只做一件事：让窗口上点得到它们，并且让「被挡住」这件事被人看见。**
//!
//! ## ⚠ 为什么经那四条命令调下去，而不是自己借一条会话
//!
//! 同 [`super::copy`] 那一节逐字的理由：`with_sftp` 与 `pool_for` 都是 `sftp_pool`
//! **模块私有**的，本模块够不着 —— **而这正好是对的**。那四条命令外面套着
//!
//! | 套着的 | 丢了会怎样 |
//! |---|---|
//! | `guard_write`（`sftp_rename` 是 `from` / `to` **各一次**） | 能把正被 Claude 打开的 `jsonl` 删掉 / 改走 / 改成不可读 |
//! | 一格通道预算（`设计/60 §5.4a` 的 `6 − 4 = 2` 格永远留给浏览） | 那条不变量被拆成两份互不知情的预算 |
//! | 池里那条连接 | 同一台远端被拨第二条 SSH（`super` 头注那一节） |
//!
//! # 🔴 二、围栏这一刀是**两道，刻意重复** —— 但判定只有一份
//!
//! - **第一道在本层**（[`fenced_path`]）：发往返**之前**就把踩线的挑出去，
//!   并且在窗口上出声。它买到的是两件事：① 用户看得见为什么没做；
//!   ② 「被挡」这件事在**一台没有连接的机器上判得动**（本仓红线不许起真连接）。
//! - **第二道在池子入口**（那四条命令各自的 `guard_write`）。它是承重的那一道：
//!   本层被绕过（有人直接调 [`apply_remote`]、或者路径在这两步之间才变成受保护的），
//!   它照旧挡。它的原话会经 [`WriteOutcome::failed`] **原样**画到窗口上。
//!
//! 🔴 **两道问的是同一个函数** `sftp_pool::is_protected_claude_data_path`
//! ⇒ 判定不会漂。漂得动的只有文案，而文案两处是因为**两层各自要说话**。
//!
//! ⚠ **本模块一个字节都不改那道围栏** —— 「要不要把它拆成独立一族」是
//! `设计/99 §2 Q2`，用户还没拍板。这一刀只**用**它。
//!
//! # 🔴 三、三段的顺序**就是 [`run_writes`] 的结构** —— 照 [`super::transfer::run_drop`] 办
//!
//! ```text
//! ① fence    —— 围栏（本地、不过网）。踩线的**一件都不交给 apply**，也不进问答
//! ② confirm  —— 要动的那几件**一次**交给人（`FnOnce` ⇒ 一半由编译器守）
//! ③ apply    —— 才动手
//! ```
//!
//! ⚠ `confirm` 回来的那一摞由 `ops.iter().filter(|o| allowed.contains(o))` 过一遍
//! （同 `run_drop` 那一行）⇒ **它没法凭空塞进一件没过围栏的操作**：
//! 回值只起「准不准」的作用，不起「做什么」的作用。
//!
//! ## 为什么 ③ 是**串行**的（与 `run_drop` 刻意不同形）
//!
//! `run_drop` 并行是因为传输走的是那 **4 条车道**，串行只用得到 1 条。
//! 而这四条命令走的是 `with_sftp`，吃的是**通道预算**（`SESSION_CHANNEL_CAP`）——
//! 一次并发起 N 件小写操作，会把 `设计/60 §5.4a` 那句「2 格永远留给浏览」吃掉，
//! 而换来的只是几个 packet 的延迟。⇒ **串行，并把理由写在这儿**。
//!
//! # 🔴 四、谁要「问一次」，谁不要 —— 逐条给理由，不是手感
//!
//! | 操作 | 问不问 | 理由 |
//! |---|---|---|
//! | 删除 | **问** | 不可撤销。同旧面板 `src/sftp/panel.ts` 那一处 `window.confirm` |
//! | 改权限 | **问** | 一样能弄坏一场正在跑的会话（把 jsonl 改成不可读），**而它不像删除那样显眼** |
//! | 新建目录 | 不问 | 同名由**服务端**报错 ⇒ 天然毁不了东西（旧面板那处注释逐字记着这一点） |
//! | 改名 | 不问 | 目标已存在时由服务端拒（SFTP 的 `RENAME` 不覆盖）；而且它**可逆** |
//!
//! ⚠ 「问过了」与「没问」在类型上分得开：`confirm` 是 `FnOnce`
//! ⇒ **不许被调第二次**，这一半由编译器守（同 `run_drop` 的先例）。
//!
//! # ⚠ 这一刀**买不到**什么（逐条写明，别读宽）
//!
//! - **一趟真操作的读数买不到。** 本仓红线不许起真连接 ⇒ [`apply_remote`] 本机跑不到。
//!   它买得到的是**委派**（调的是池那四条既有命令，而不是自己借会话）。
//! - 🔴 **今天生产那一侧递进来的恒是 `N = 1`。** 窗口还**没有多选**
//!   （`设计/99 §4.7.3 β1` 把多选排在本节**之后**），一次手势只点得中一行。
//!   ⇒ 「N 个只问一次」这条性质由 [`run_writes`] 自己的判据按 `N > 1` 喂，
//!   而**生产路径上今天喂不出 N > 1**。如实记成一条边界：本刀买的是
//!   「多选长出来那天，它自动落在一次问完这条路上」，不是「今天已经多选了」。
//! - **目录递归删除没做**：`sftp_delete` 的 `is_dir` 走的是 `remove_dir`
//!   （只删空目录，非空由服务端报错）。递归要一条新的池命令，不在本刀写区。
//! - **改权限没有「当前是多少」可显示**：[`super::source::Row`] 里没有 mode
//!   （`SftpEntry` 也没有）⇒ 那个框**空着开**，刻意不预填一个猜出来的值
//!   （预填错了而用户直接点确认 = 静默改坏权限）。
//! - **往外拖（`sftp_download`）与文本编辑（`sftp_read_text_for_edit` /
//!   `sftp_write_text`）不在本刀射程里**，登记在此：前者是另一个交互题（选目标目录），
//!   后者要一个编辑器面，而 `设计/60 §5.4b`（大文件编辑改流式）至今没做、形状没定。

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::ssh_source::RemoteConfig;

use super::source::Row;

/// 行上／工具栏上那几颗按钮的字面。**唯一住址** —— 判据按同一个常量去找它画出来的字。
pub const MKDIR_LABEL: &str = "新建目录";
/// 行上那颗「改名」。
pub const RENAME_LABEL: &str = "改名";
/// 行上那颗「删除」。
pub const DELETE_LABEL: &str = "删除";
/// 行上那颗「权限」。
pub const CHMOD_LABEL: &str = "权限";

/// 这一行能不能被**写**（改名 / 删除 / 改权限）。**唯一住址** ——
/// 列表画不画那三颗按钮（[`super::rows`]）与状态机接不接那一跳
/// （[`super::shell::FileWindow::begin_rename`] 那一族），问的都是这一个函数。
///
/// 一档不能：**有损名** —— 非 UTF-8 文件名经库有损解码之后**寻址不到真字节**
/// （同旧面板 `panel.ts::mkRowBtn` 的 `disabled = e.lossyName`）。
/// 拿一个含 U+FFFD 的名字去删，删中的是**另一个**文件，或者什么都删不中。
///
/// ⚠ 与 [`super::copy::is_copyable`] **刻意不是同一个函数**：目录**能**改名 /
/// 删除 / 改权限，但**不能**零流量复制（`copy-data` 吃的是文件句柄）。
/// 合成一个就得让目录那一档在四个按钮上做不同的事，而那正是「一个函数两种语义」。
pub fn is_writable(r: &Row) -> bool {
    !r.lossy_name
}

// ═══════════════════════════════════════════════════════════════════════
// 一件写操作
// ═══════════════════════════════════════════════════════════════════════

/// 一件待做的写操作。**刻意是封闭枚举** —— 多出第五种就得回来论证
/// （而且要同拍进那张「窗口用了池子哪几条命令」的登记，见
/// `remote_write_registry_tests.rs::the_file_window_uses_exactly_the_pool_commands_it_registers`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteOp {
    /// 在当前目录里新建一个目录。
    Mkdir { path: String },
    /// 删一项。`is_dir` 分 `rmdir` / `rm`（同 `sftp_delete` 的签名）。
    Delete { path: String, is_dir: bool },
    /// 同一个目录里改名。
    Rename { from: String, to: String },
    /// 改权限位。`mode` 是 unix mode 的低 12 位。
    Chmod { path: String, mode: u32 },
}

impl WriteOp {
    /// 这一件碰到的**每一条路径**。
    ///
    /// 🔴 **改名回两条，这是本函数存在的全部理由。** 围栏要「两个参数各过一遍」
    /// （`remote_write_registry::a_two_path_write_entry_fences_both_of_its_paths`
    /// 就是为这一形立的，而它当初立起来时逮到了 `sftp_rename` 自己少一道围栏）。
    /// 把围栏写成「看 `op` 的第一条路径」⇒ 能把任意文件**改名成**
    /// `<远端>/projects/<proj>/<sid>.jsonl`，盖掉那台机器上正被 Claude 打开的会话。
    pub fn paths(&self) -> Vec<&str> {
        match self {
            WriteOp::Mkdir { path } => vec![path.as_str()],
            WriteOp::Delete { path, .. } => vec![path.as_str()],
            WriteOp::Chmod { path, .. } => vec![path.as_str()],
            WriteOp::Rename { from, to } => vec![from.as_str(), to.as_str()],
        }
    }

    /// 给人看的一句话（确认框 · 结果行 · 被挡那句话都用它）。
    pub fn label(&self) -> String {
        match self {
            WriteOp::Mkdir { path } => format!("新建目录 {path}"),
            WriteOp::Delete { path, is_dir } => {
                let kind = if *is_dir { "目录" } else { "文件" };
                format!("删除{kind} {path}")
            }
            WriteOp::Rename { from, to } => format!("改名 {from} → {to}"),
            WriteOp::Chmod { path, mode } => format!("改权限 {path} → {mode:o}"),
        }
    }

    /// 这一件动手之前要不要问人。**逐条的理由住本模块头注 §四那张表。**
    pub fn needs_confirm(&self) -> bool {
        match self {
            WriteOp::Delete { .. } | WriteOp::Chmod { .. } => true,
            WriteOp::Mkdir { .. } | WriteOp::Rename { .. } => false,
        }
    }
}

/// 逐字出现在**被围栏挡住**那一句里的前缀 —— 判据按它去找那句话。
pub const FENCE_PREFIX: &str = "⚠ 挡住了：";

/// 这一件踩到 Claude 数据围栏的**那一条路径**（`None` = 一条都没踩）。
///
/// 🔴 它**逐条问** [`WriteOp::paths`]，而不是只问第一条 —— 理由逐字住那个函数。
/// 🔴 判定走的是 `sftp_pool::is_protected_claude_data_path`，**全仓那一个**
/// （本模块不许有第二份判定；池子入口那道 `guard_write` 问的也是它）。
pub fn fenced_path(op: &WriteOp) -> Option<&str> {
    op.paths()
        .into_iter()
        .find(|p| crate::sftp_pool::is_protected_claude_data_path(p))
}

/// 被挡住那一句话。**画在窗口上**，不是 `tracing`。
pub fn fence_notice(op: &WriteOp, path: &str) -> String {
    format!(
        "{FENCE_PREFIX}{} —— `{path}` 是 Claude 的会话数据，\
         动它会弄坏正在跑的那场会话。要管这些用历史浏览器。",
        op.label()
    )
}

/// 一摞写操作跑完之后的读数。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WriteOutcome {
    /// 一次问完的时候，摆在人面前的件数。
    pub asked: usize,
    /// 人答「不做」而没做的件数。
    pub skipped: usize,
    /// 被 Claude 数据围栏挡住的那几件（那句话原文）。**一个字节都没动过对面的盘。**
    pub blocked: Vec<String>,
    /// 做成了的件数。
    pub ok: usize,
    /// 失败的那几件（说明 ＋ 池子给的报错原文）。
    pub failed: Vec<(String, String)>,
}

/// 🔴 **正题**：一摞写操作的全过程。三段的顺序就是这个函数的结构。
///
/// - `confirm`：**一次**拿到「要问的那几件」，回「这几件准做」。⚠ 它是 `FnOnce`
///   —— 类型上就不许被调第二次，「一次问完」这件事有一半是编译器在守。
/// - `apply`：真做一件。
///
/// ⚠ **不用问的那几件也要等这一次问完**。那是刻意的，同 [`super::transfer::run_drop`]：
/// 若把它们先放出去，用户答「全都别做」时已经有几件写到对面盘上了 ——
/// 「先问完再动手」就只剩一半。
pub async fn run_writes<C, CFut, A, AFut>(ops: Vec<WriteOp>, confirm: C, apply: A) -> WriteOutcome
where
    C: FnOnce(Vec<WriteOp>) -> CFut,
    CFut: Future<Output = Vec<WriteOp>>,
    A: Fn(WriteOp) -> AFut,
    AFut: Future<Output = Result<(), String>>,
{
    let mut out = WriteOutcome::default();
    if ops.is_empty() {
        return out;
    }
    // ── ① 围栏（本地、不过网）───────────────────────────────────────────
    //    踩线的**一件都不往下走**：不进问答、更不进 `apply`。
    let mut clean: Vec<WriteOp> = Vec::new();
    for op in ops {
        match fenced_path(&op) {
            Some(p) => out.blocked.push(fence_notice(&op, p)),
            None => clean.push(op),
        }
    }
    if clean.is_empty() {
        return out;
    }

    // ── ② 一次问完 ────────────────────────────────────────────────────
    //    没有要问的就**不问** —— 弹一个空框是噪音，不是慎重（同 `run_drop`）。
    let asking: Vec<WriteOp> = clean
        .iter()
        .filter(|o| o.needs_confirm())
        .cloned()
        .collect();
    out.asked = asking.len();
    let allowed: Vec<WriteOp> = if asking.is_empty() {
        Vec::new()
    } else {
        confirm(asking).await
    };

    // 准做的那一摞 = 不用问的全部 ＋ 人点了「做」的那几件。
    //
    // 🔴 `allowed.contains(o)` 这一行是**承重的**（同 `run_drop` 那一行）：
    //    回值只起「准不准」的作用 ⇒ `confirm` 没法凭空塞进一件**没过围栏**的操作。
    let go: Vec<WriteOp> = clean
        .into_iter()
        .filter(|o| !o.needs_confirm() || allowed.contains(o))
        .collect();
    out.skipped = out.asked - go.iter().filter(|o| o.needs_confirm()).count();

    // ── ③ 才动手。**串行**，理由住本模块头注 §三那一段。 ──────────────────
    for op in go {
        match apply(op.clone()).await {
            Ok(()) => out.ok += 1,
            Err(e) => out.failed.push((op.label(), e)),
        }
    }
    out
}

// ═══════════════════════════════════════════════════════════════════════
// 生产适配器：**一行自己的写代码都没有**
// ═══════════════════════════════════════════════════════════════════════

/// 真做一件 —— 调池子那四条既有命令。
///
/// 🔴 **为什么可以直接调一个 `#[tauri::command]`**：同进程（`super` 头注），
/// 它同时就是一个普通 `pub async fn` ⇒ 这里不过 IPC、不过 serde，
/// 走的是**同一个进程级连接池** ⇒ 围栏、通道预算全部照旧生效。
///
/// ⚠ 回来的 `Err` **原样**带出去（含池子那道 `guard_write` 的原话），
/// 由 [`WriteBoard::ui`] 画到窗口上 —— 这一层不改写、不摘要。
pub async fn apply_remote(cfg: &RemoteConfig, op: &WriteOp) -> Result<(), String> {
    match op {
        WriteOp::Mkdir { path } => crate::sftp_pool::sftp_mkdir(cfg.clone(), path.clone()).await,
        WriteOp::Delete { path, is_dir } => {
            crate::sftp_pool::sftp_delete(cfg.clone(), path.clone(), *is_dir).await
        }
        WriteOp::Rename { from, to } => {
            crate::sftp_pool::sftp_rename(cfg.clone(), from.clone(), to.clone()).await
        }
        WriteOp::Chmod { path, mode } => {
            crate::sftp_pool::sftp_chmod(cfg.clone(), path.clone(), *mode).await
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 那个框：**要什么名字 / 改成什么权限**（UI 线程自己的状态）
// ═══════════════════════════════════════════════════════════════════════

/// 那个框问的是哪一件事。
///
/// ⚠ 删除**不在这里** —— 它不需要向用户要任何输入，它要的是一次确认，
/// 而确认那一步归 [`run_writes`] 的第二段（一次问完）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptKind {
    /// 在 `dir` 里新建一个目录，框里输的是名字。
    Mkdir,
    /// 把 `from` 改成同一个目录里的另一个名字。
    Rename { from: String },
    /// 把 `path` 的权限改成框里那个八进制数。
    Chmod { path: String },
}

/// 「要什么名字 / 改成什么权限」那个框 —— **UI 线程自己的状态**，
/// 刻意**不进** [`WriteBoard`]（同 [`super::copy::CopyPrompt`] 逐字的理由：
/// 正在输入的那个草稿从头到尾只有 UI 线程碰得到，塞进共享内存里是把一件
/// 单线程的事摆到两条线程之间）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WritePrompt {
    pub kind: PromptKind,
    /// 目标落在哪个目录（＝ 当前目录）。
    pub dir: String,
    /// 那一行在列表上的名字（只用来说话）。
    pub src_name: String,
    /// 正在编辑的那几个字。
    pub text: String,
}

impl WritePrompt {
    pub fn for_mkdir(dir: &str) -> Self {
        Self {
            kind: PromptKind::Mkdir,
            dir: dir.to_string(),
            src_name: String::new(),
            text: String::new(),
        }
    }

    /// 改名那个框 —— 缺省填**原名**（同旧面板 `panel.ts::rename` 的 `prompt(…, e.name)`）。
    pub fn for_rename(dir: &str, r: &Row) -> Self {
        Self {
            kind: PromptKind::Rename {
                from: r.path.clone(),
            },
            dir: dir.to_string(),
            src_name: r.name.clone(),
            text: r.name.clone(),
        }
    }

    /// 改权限那个框 —— **空着开**。理由住本模块头注最后一节
    /// （列表里没有 mode 可读，预填一个猜出来的值而用户直接点确认 = 静默改坏权限）。
    pub fn for_chmod(dir: &str, r: &Row) -> Self {
        Self {
            kind: PromptKind::Chmod {
                path: r.path.clone(),
            },
            dir: dir.to_string(),
            src_name: r.name.clone(),
            text: String::new(),
        }
    }

    /// 框上那一行提示。
    pub fn heading(&self) -> String {
        match &self.kind {
            PromptKind::Mkdir => "在这个目录里新建一个目录，叫：".to_string(),
            PromptKind::Rename { .. } => format!("把 {} 改名为：", self.src_name),
            PromptKind::Chmod { .. } => format!("把 {} 的权限改成（八进制）：", self.src_name),
        }
    }

    /// 框里那几个字变成一件真操作。
    ///
    /// # Errors
    ///
    /// 回的是**一句给用户的话**（调用方据此出声，且把框留着）——
    /// 静默收掉的话，用户点了确认什么都没发生，与成功长得一模一样。
    pub fn to_op(&self) -> Result<WriteOp, String> {
        let t = self.text.trim();
        match &self.kind {
            PromptKind::Mkdir => {
                let name = clean_name(t)?;
                Ok(WriteOp::Mkdir {
                    path: join_remote(&self.dir, &name),
                })
            }
            PromptKind::Rename { from } => {
                let name = clean_name(t)?;
                let to = join_remote(&self.dir, &name);
                if &to == from {
                    return Err(format!("「{name}」就是它现在的名字 —— 改名没有要改的东西"));
                }
                Ok(WriteOp::Rename {
                    from: from.clone(),
                    to,
                })
            }
            PromptKind::Chmod { path } => {
                let mode = parse_mode(t)?;
                Ok(WriteOp::Chmod {
                    path: path.clone(),
                    mode,
                })
            }
        }
    }
}

/// 一个能用的**名字**（不是路径）。
///
/// 🔴 不许带 `/` —— 那是「放到别的目录去」。让人在一个「改个名」的框里
/// 不小心写出一条别的路径，就是把文件放到他没在看的目录里（同
/// [`super::copy::CopyJob::beside`] 那一条逐字的理由）。
/// 🔴 不许是 `.` / `..` —— 它们不是名字，是**当前目录与上一级**。
/// 拿 `..` 去 `sftp_delete` 就是让服务端对着父目录动手。
fn clean_name(t: &str) -> Result<String, String> {
    if t.is_empty() {
        return Err("名字是空的".to_string());
    }
    if t.contains('/') {
        return Err(format!(
            "「{t}」里带 `/` —— 这个框只改名字，不许在这儿写出一条别的路径"
        ));
    }
    if t == "." || t == ".." {
        return Err(format!("「{t}」不是一个名字，它是目录本身"));
    }
    Ok(t.to_string())
}

/// 八进制权限位。
///
/// ⚠ 掩到 `0o7777` 那一步在池子里（`sftp_pool::sftp_chmod` 转给 `chmod_attrs`），
/// 这里**拒**超出范围的输入而不是悄悄掩掉：用户输了 `100644`（连文件类型位一起抄来的）
/// 而我们掩成 `644` ⇒ 他以为改的是别的东西。
fn parse_mode(t: &str) -> Result<u32, String> {
    if t.is_empty() {
        return Err("权限位是空的 —— 八进制，比如 644".to_string());
    }
    let mode = u32::from_str_radix(t, 8)
        .map_err(|_| format!("「{t}」不是一个八进制数 —— 比如 644 或 755"))?;
    if mode > 0o7777 {
        return Err(format!(
            "「{t}」超出权限位的范围（最大 7777）—— 文件类型位不许在这儿改"
        ));
    }
    Ok(mode)
}

/// 远端路径**恒用 `/`** 拼（同 [`super::source::parent_dir`] 那条理由：
/// 拿 `std::path` 切远端路径，在 Windows 上会把 `\` 也当分隔符）。
fn join_remote(dir: &str, name: &str) -> String {
    format!("{}/{}", dir.trim_end_matches('/'), name)
}

// ═══════════════════════════════════════════════════════════════════════
// 窗口那一侧的状态：**问什么 · 上一摞做完了什么**
// ═══════════════════════════════════════════════════════════════════════

/// 一摞写操作在窗口上的样子。**跨线程共享**（UI 线程画，tokio 那条写）。
///
/// 形状照 [`super::transfer::DropBoard`] 办，含那只「敲窗口的手」——
/// egui 只在有事发生时才画下一帧，不敲一下结果要等用户动鼠标才出现
/// （`真相源/99 §9.6`：「卡住了」与「真的没在跑」在屏幕上分不开）。
#[derive(Clone, Default)]
pub struct WriteBoard {
    inner: Arc<Mutex<Board>>,
    /// 已经跑完的趟数 —— 给判据与「跑完要重列目录」一个可观测的数。
    rounds: Arc<AtomicU64>,
    ctx: Arc<Mutex<Option<egui::Context>>>,
}

#[derive(Default)]
struct Board {
    /// 正摆在人面前等答复的那几件（空 = 没在问）。
    asking: Vec<WriteOp>,
    /// 人勾了哪几件（与 `asking` 同长）。
    ticks: Vec<bool>,
    /// 答复往哪儿送。
    answer: Option<tokio::sync::oneshot::Sender<Vec<WriteOp>>>,
    /// 上一趟的结果（**画在窗口上**，不是 `println!`）。
    last: Option<WriteOutcome>,
}

impl WriteBoard {
    /// 摆出问题，并交出「答复送哪儿」那一头。
    pub fn ask(&self, ops: Vec<WriteOp>) -> tokio::sync::oneshot::Receiver<Vec<WriteOp>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut b = self.inner.lock().unwrap();
            // 缺省勾上：用户点的就是那一行那颗按钮。
            // ⚠ 缺省不勾的话，确认那颗按钮点下去什么都不会发生 ——
            //   而「什么都没发生」与「做完了」在屏幕上分不开。
            b.ticks = vec![true; ops.len()];
            b.asking = ops;
            b.answer = Some(tx);
        }
        self.poke(); // 问题要立刻画出来，别等下一次鼠标动
        rx
    }

    pub fn is_asking(&self) -> bool {
        !self.inner.lock().unwrap().asking.is_empty()
    }

    /// 正摆着的那几件（判据用）。
    pub fn asking(&self) -> Vec<WriteOp> {
        self.inner.lock().unwrap().asking.clone()
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

    pub fn finish(&self, outcome: WriteOutcome) {
        {
            let mut b = self.inner.lock().unwrap();
            b.asking.clear();
            b.ticks.clear();
            b.last = Some(outcome);
        }
        self.rounds.fetch_add(1, Ordering::SeqCst);
        self.poke();
    }

    pub fn rounds(&self) -> u64 {
        self.rounds.load(Ordering::SeqCst)
    }

    pub fn last(&self) -> Option<WriteOutcome> {
        self.inner.lock().unwrap().last.clone()
    }

    /// 人点了「做勾上的」/「都别做」—— 把答复送出去，问题收掉。
    ///
    /// 回值 = 真的送出去了（重复点第二下不会送第二次；`oneshot` 也只收一次）。
    pub fn settle(&self, go: bool) -> bool {
        let mut b = self.inner.lock().unwrap();
        let Some(tx) = b.answer.take() else {
            return false;
        };
        let allowed: Vec<WriteOp> = if go {
            b.asking
                .iter()
                .zip(b.ticks.iter())
                .filter(|(_, t)| **t)
                .map(|(o, _)| o.clone())
                .collect()
        } else {
            Vec::new()
        };
        b.asking.clear();
        b.ticks.clear();
        drop(b);
        tx.send(allowed).is_ok()
    }

    /// 画确认框与上一趟的结果。**模态** —— 有问题在等的时候，列表那边不接受点击。
    ///
    /// 🔴 `blocked` 那一段是这一刀的承重墙：被围栏挡住的那几件**必须在屏幕上出声**。
    /// 删掉它 ⇒ 用户点了删除、什么都没发生、也没有任何一句话，
    /// 而那与「删掉了」在屏幕上分不开（`INVARIANTS §1` 的 F47 那一段逐字要求
    /// 每一次写都是一次看得见的用户手势）。
    pub fn ui(&self, ui: &mut egui::Ui) {
        let (asking, mut ticks, last) = {
            let b = self.inner.lock().unwrap();
            (b.asking.clone(), b.ticks.clone(), b.last.clone())
        };
        if !asking.is_empty() {
            let mut answer: Option<bool> = None;
            let mut changed = false;
            egui::Modal::new(egui::Id::new("filewin-write-confirm")).show(ui.ctx(), |ui| {
                ui.heading(format!("这 {} 件要做吗？", asking.len()));
                ui.colored_label(
                    egui::Color32::from_rgb(0xE0, 0x9A, 0x20),
                    "⚠ 这一问只出现一次：勾完点确认。删除不可撤销。",
                );
                for (i, o) in asking.iter().enumerate() {
                    let mut t = ticks[i];
                    if ui.checkbox(&mut t, o.label()).changed() {
                        ticks[i] = t;
                        changed = true;
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("做勾上的").clicked() {
                        answer = Some(true);
                    }
                    if ui.button("都别做").clicked() {
                        answer = Some(false);
                    }
                });
            });
            if changed {
                self.inner.lock().unwrap().ticks = ticks;
            }
            if let Some(go) = answer {
                self.settle(go);
            }
        }
        if let Some(o) = &last {
            for line in &o.blocked {
                ui.colored_label(egui::Color32::RED, line);
            }
            if !o.failed.is_empty() {
                ui.colored_label(
                    egui::Color32::RED,
                    format!(
                        "上一摞 {} 件没做成：{}",
                        o.failed.len(),
                        o.failed
                            .iter()
                            .map(|(n, e)| format!("{n}（{e}）"))
                            .collect::<Vec<_>>()
                            .join("；")
                    ),
                );
            } else if o.ok > 0 || o.skipped > 0 {
                ui.label(format!("上一摞 {} 件做完、{} 件跳过", o.ok, o.skipped));
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/writeops_tests.rs"]
mod tests;
