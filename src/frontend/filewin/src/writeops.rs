//! `24e` 第五刀：**新建目录 · 删除 · 改名 · 改权限**接到原生窗口上
//! （前半，用户原话「我能连 ssh 对机器文件进行什么操作，
//! 后端就应该能进行什么操作」）。
//!
//! # 🔴那四条现在走**后端写面**，经通道说 `call`
//!
//! 下面第一节讲的是「经池子那四条 SFTP 命令调下去」—— **那是上一版**。窗口成了独立进程、
//! 后端写面（`files-mkdir` / `files-delete` / `files-rename` / `files-chmod`，F1 落的）上线之后，
//! [`apply_remote`] 改成经 [`super::source::ask`] 说那四条命令：`root` ＝ 那一行所在的目录，
//! `rel` ＝ 名字（后端路径解析按段判 `rel`）。
//! ⚠ 下面讲池子那几节的每一句都当成**历史**读（`guard_write`、车道预算、池里那条连接）。
//!
//! # 🔴〔用户〕**本层那道本地预判删了**
//!
//! 用户原话「**文件管理器全部都可以改. 不需要任何围栏**」⇒ 会话文件（`projects/<proj>/<sid>.jsonl` ·
//! `sessions/<x>.json`）、项目目录、subagent、tasks 在窗口上都能改名 / 删 / 改权限。
//! 从前的第一段（`fenced_path` 判一遍、踩线的不问不做、窗口上画一句「挡住了」）与
//! 后端写面那道会话文件围栏**一起**拿掉了；[`run_writes`] 今天只剩两段：**一次问完 → 才动手**。
//! 删除 / 改权限照旧问一次 —— 那一问是「不可撤销 / 不显眼」（§四那张表），**不是围栏**。
//! 后端那一侧仍会拒的只有路径解析那几形（上跳 · 父目录不在 · 解完链接跑出根），拒的那句原话照旧
//! 经 [`WriteOutcome::failed`] 画到窗口上。下面 §一 · §二讲「两道围栏」的段落是**历史**。
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
//! | 一格通道预算（`6 − 4 = 2` 格永远留给浏览） | 那条不变量被拆成两份互不知情的预算 |
//! | 池里那条连接 | 同一台远端被拨第二条 SSH（`super` 头注那一节） |
//!
//! # 二、〔**整节是历史**〕围栏这一刀是**两道，刻意重复** —— 但判定只有一份
//!
//! 两道今天都没有了：本层那道（`fenced_path`）与后端写面那道一起删。下面是原话。
//!
//! - **第一道在本层**（`fenced_path`）：发往返**之前**就把踩线的挑出去，
//!   并且在窗口上出声。它买到的是两件事：① 用户看得见为什么没做；
//!   ② 「被挡」这件事在**一台没有连接的机器上判得动**（本仓红线不许起真连接）。
//! - **第二道在池子入口**（那四条命令各自的 `guard_write`）。它是承重的那一道：
//!   本层被绕过（有人直接调 [`apply_remote`]、或者路径在这两步之间才变成受保护的），
//!   它照旧挡。它的原话会经 [`WriteOutcome::failed`] **原样**画到窗口上。
//!
//! 🔴 **两道问的是同一个函数** `claude_data_fence::is_protected_claude_data_path`
//! ⇒ 判定不会漂。漂得动的只有文案，而文案两处是因为**两层各自要说话**。
//!
//! ⚠ **本模块一个字节都不改那道围栏** —— 「要不要把它拆成独立一族」未定。
//! 这里只**用**它。
//!
//! # 🔴 三、两段（原来三段）的顺序**就是 [`run_writes`] 的结构** —— 照 [`super::transfer::run_drop`] 办
//!
//! ```text
//! ① confirm  —— 要动的那几件**一次**交给人（`FnOnce` ⇒ 一半由编译器守）
//! ② apply    —— 才动手
//! ```
//!
//! 原来排在最前面的「① fence —— 围栏（本地、不过网）」那一段删了（理由住本模块头注 FN1 那一节）。
//!
//! ⚠ `confirm` 回来的那一摞由 `ops.iter().filter(|o| allowed.contains(o))` 过一遍
//! （同 `run_drop` 那一行）⇒ **它没法凭空塞进一件调用方没交给它的操作**：
//! 回值只起「准不准」的作用，不起「做什么」的作用。
//!
//! ## 为什么 ③ 是**串行**的（与 `run_drop` 刻意不同形）
//!
//! `run_drop` 并行是因为传输走的是那 **4 条车道**，串行只用得到 1 条。
//! 而这四条命令走的是 `with_sftp`，吃的是**通道预算**（`SESSION_CHANNEL_CAP`）——
//! 一次并发起 N 件小写操作，会把那句「2 格永远留给浏览」吃掉，
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
//!   （把多选排在本节**之后**），一次手势只点得中一行。
//!   ⇒ 「N 个只问一次」这条性质由 [`run_writes`] 自己的判据按 `N > 1` 喂，
//!   而**生产路径上今天喂不出 N > 1**。如实记成一条边界：本刀买的是
//!   「多选长出来那天，它自动落在一次问完这条路上」，不是「今天已经多选了」。
//! - ✅~~目录递归删除没做~~：删**目录**现在带 `recursive: true` 走后端
//!   `files-delete`（连同里面全部内容；后端逐条目过路径解析 —— 树里的会话文件照删）。
//!   原话「`sftp_delete` 的 `is_dir` 走的是 `remove_dir`，递归要一条新的池命令」是 SFTP 那一版的事。
//! - ✅~~改权限没有「当前是多少」可显示~~：后端 `files-stat` 从此送 `mode`（unix 权限位低 12 位，
//!   非 unix 缺席）⇒ 框一摆出来就逐项问一次（[`ModeProbe`] · [`probe_modes`]），答回来之后框上说现值
//!   （[`mode_readout`]），只有一项或各项相同时**预填**那个值（它是读回来的，不是猜的；用户直接点确认 = 原样不变）。
//!   问不到 ⇒ 照旧空着开、说「读不到现在的权限」—— 不猜。〔墓碑 —— FW5 那一版这里写着「那个框**空着开**，刻意不预填一个
//!   猜出来的值」：那句的前提是「没有读口」，读口有了。〕
//! - ✅~~多选只有删除~~：多选也给「权限」（一个框、一个八进制数、出 N 件、一次问完）。
//! - ✅~~有损名一律不许写~~：带着原始字节（`Listed::raw_name`）的有损名能改名 · 删除 · 改权限（相对段发 b16）；
//! 进有损名目录 · 复制 / 编辑 / 算大小有损名也做了（窗口发路径改走 `source::RemotePath`，
//!   写面的根在有损目录里发字节 —— [`apply_remote_in`]）；**下载有损名**仍然做不到（SFTP 库按 UTF-8 有损解码文件名）。
//! - **往外拖（`sftp_download`）与文本编辑（`sftp_read_text_for_edit`〔散文墓碑〕 /
//!   `sftp_write_text`）不在本刀射程里**，登记在此：前者是另一个交互题（选目标目录），
//!   后者要一个编辑器面，而（大文件编辑改流式）至今没做、形状没定。
//! - 🔴〔波 5〕**新建空文件：后端那一半有了，窗口这一半刻意没接。**
//!   后端文件管理写面今天有 `files-create`（`O_EXCL` 新建，不给 `content` 就是空文件，
//!   过路径解析；`src/doc/IPC-PROTOCOL.md` §10 那一节），另有
//!   `files-mkdir` / `-rename` / `-delete` / `-chmod` / `-write-text` 五条。
//!   **本模块没有加第五种 [`WriteOp`]**，理由两条：① 窗口这一波**够不着后端**
//!   （独立进程里客户端登记表是空的；接通道归 F3、接窗口归 F2）；
//!   ② 若在这里用 `sftp_write_text` 兜一个「新建」，那是往**与目标相反**的方向走
//!   （SFTP 要缩成只做传输），而且会动
//!   `remote_write_registry_tests` 那条「窗口用了池子哪几条命令」的相等断言。
//!   ⇒ 窗口上那颗按钮（住 `rows.rs` / `shell.rs`，不在本路写区）等 F2 接后端时一起落。

use copy_core::copy_text;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::source::{parent_dir, remote_basename, Line, Origin};

use super::source::Listed;

/// 行上／工具栏上那几颗按钮的字面。**唯一住址** —— 判据按同一个常量去找它画出来的字。
pub static MKDIR_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinWriteops.label.mkdir", &[]));
/// 行上那颗「改名」。
pub static RENAME_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinWriteops.label.rename", &[]));
/// 行上那颗「删除」。
pub static DELETE_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinWriteops.label.delete", &[]));
/// 行上那颗「权限」。
pub static CHMOD_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinWriteops.label.chmod", &[]));

/// 这一行能不能被**写**（改名 / 删除 / 改权限）。**唯一住址** ——
/// 列表画不画那三颗按钮（[`super::rows`]）与状态机接不接那一跳
/// （[`super::shell::FileWindow::begin_rename`] 那一族），问的都是这一个函数。
///
/// 一档不能：**有损名而且手上没有它的原始字节** —— 非 UTF-8 文件名经库有损解码之后
/// 那个字符串**寻址不到真字节**（同旧面板 `panel.ts::mkRowBtn` 的 `disabled = e.lossyName`）。
/// 拿一个含 U+FFFD 的名字去删，删中的是**另一个**文件，或者什么都删不中。
///
/// 🔴**有损名但带着原始字节**（[`Listed::raw_name`]，后端 `files-ls` 送的就是字节）
/// ⇒ **能写**：发给后端的相对段走 `{"b16": …}`（[`rel_json`]），寻址的是那几个真字节，不是显示串。
/// 此前这一档一律灰置 —— 乱码名的文件在窗口上改不了名、删不掉，而「改成一个读得出的名字」正是它最常要的那一下。
///
/// ⚠ 与 [`super::copy::is_copyable`] **刻意不是同一个函数**：目录**能**改名 /
/// 删除 / 改权限，但**不能**零流量复制（`copy-data` 吃的是文件句柄）。
/// 合成一个就得让目录那一档在四个按钮上做不同的事，而那正是「一个函数两种语义」。
pub fn is_writable(r: &Listed) -> bool {
    !r.lossy_name || r.raw_name.is_some()
}

/// 一个名字发给后端写面时的形状：有原始字节 ⇒ `{"b16": …}`；否则就是那个字符串。
///
/// 🔴 **有损名只许走字节那一支** —— 把显示串（含 U+FFFD）发过去就是对另一个名字动手；
/// 这一条由 [`is_writable`] 在上游挡（有损而没有字节 ⇒ 不许写），这里不再兜第二份判定。
pub fn rel_json(shown: &str, raw: Option<&[u8]>) -> serde_json::Value {
    match raw {
        Some(b) => {
            serde_json::json!({ super::find::HEX_KEY: b.iter().map(|x| format!("{x:02x}")).collect::<String>() })
        }
        None => serde_json::Value::String(shown.to_string()),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 一件写操作
// ═══════════════════════════════════════════════════════════════════════

/// 一件待做的写操作。**刻意是封闭枚举** —— 多出第五种就得回来论证
/// （而且要同拍进那张「窗口用了池子哪几条命令」的登记，见
/// `remote_write_registry_tests.rs::the_file_window_uses_exactly_the_pool_commands_it_registers`）。
///
/// `raw` = 那一项**名字**（`path` / `from` 的尾段）的原始字节，只在有损名时是 `Some`。
/// 它是操作的一部分（进 `PartialEq`）：两个不同字节的有损名可能解成**同一个**显示串，
/// 少了它，两件操作在「一次问完」那一步里分不开（`allowed.contains` 会认错）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteOp {
    /// 在当前目录里新建一个目录。
    Mkdir { path: String },
    /// 删一项。`is_dir` ⇒ **连同里面全部内容**（后端 `files-delete` 带 `recursive: true`，
    /// 逐条目过路径解析；树里的会话文件照删）；文件 / 链接 ⇒ 只删它自己。
    Delete {
        path: String,
        is_dir: bool,
        raw: Option<Vec<u8>>,
    },
    /// 同一个目录里改名。`raw` 是 `from` 的尾段字节（`to` 是框里敲的，恒 UTF-8）。
    Rename {
        from: String,
        to: String,
        raw: Option<Vec<u8>>,
    },
    /// 改权限位。`mode` 是 unix mode 的低 12 位。
    Chmod {
        path: String,
        mode: u32,
        raw: Option<Vec<u8>>,
    },
}

// 这里原来有 `WriteOp::paths`（「这一件碰到的每一条路径」，改名回两条），
//   它唯一的读者是本地那道围栏预判 —— 预判删了，它也一起删了（没有读者的公开函数是死值）。

impl WriteOp {
    /// 给人看的一句话（确认框 · 结果行都用它）。
    pub fn label(&self) -> String {
        match self {
            WriteOp::Mkdir { path } => {
                copy_text("rsFilewinWriteops.op.mkdir", &[("path", &path.to_string())])
            }
            WriteOp::Delete {
                path, is_dir: true, ..
            } => copy_text("rsFilewinWriteops.op.rmdir", &[("path", &path.to_string())]),
            WriteOp::Delete { path, .. } => {
                copy_text("rsFilewinWriteops.op.rm", &[("path", &path.to_string())])
            }
            WriteOp::Rename { from, to, .. } => copy_text(
                "rsFilewinWriteops.op.rename",
                &[("from", &from.to_string()), ("to", &to.to_string())],
            ),
            WriteOp::Chmod { path, mode, .. } => copy_text(
                "rsFilewinWriteops.op.chmod",
                &[
                    ("path", &path.to_string()),
                    ("mode", &format!("{:o}", mode)),
                ],
            ),
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

// 这里原来有本地那道围栏预判的三样：被挡那句话的前缀常量 · 「这一件踩到 Claude 数据围栏的
//   那一条路径」· 被挡那一句话（窗口上画成红字，「是 Claude 的会话数据，动它会弄坏正在跑的那场会话」）。
//   用户「文件管理器全部都可以改. 不需要任何围栏」⇒ 三样一起删；那句话随之从文案台账退役。

/// 一摞写操作跑完之后的读数。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WriteOutcome {
    /// 一次问完的时候，摆在人面前的件数。
    pub asked: usize,
    /// 人答「不做」而没做的件数。
    pub skipped: usize,
    /// 做成了的件数。
    pub ok: usize,
    /// 失败的那几件（说明 ＋ 池子给的报错原文）。
    pub failed: Vec<(String, String)>,
}

/// 🔴 **正题**：一摞写操作的全过程。两段的顺序就是这个函数的结构（原来是三段，围栏那一段删了）。
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
    // ── ① 一次问完 ────────────────────────────────────────────────────
    //    没有要问的就**不问** —— 弹一个空框是噪音，不是慎重（同 `run_drop`）。
    // 这一段之前原来还有「围栏（本地、不过网）」一段，删了。
    let asking: Vec<WriteOp> = ops.iter().filter(|o| o.needs_confirm()).cloned().collect();
    out.asked = asking.len();
    let allowed: Vec<WriteOp> = if asking.is_empty() {
        Vec::new()
    } else {
        confirm(asking).await
    };

    // 准做的那一摞 = 不用问的全部 ＋ 人点了「做」的那几件。
    //
    // 🔴 `allowed.contains(o)` 这一行是**承重的**（同 `run_drop` 那一行）：
    //    回值只起「准不准」的作用 ⇒ `confirm` 没法凭空塞进一件调用方**没交给它**的操作。
    let go: Vec<WriteOp> = ops
        .into_iter()
        .filter(|o| !o.needs_confirm() || allowed.contains(o))
        .collect();
    out.skipped = out.asked - go.iter().filter(|o| o.needs_confirm()).count();

    // ── ② 才动手。**串行**，理由住本模块头注 §三那一段。 ──────────────────
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

/// 真做一件 —— 经通道说后端写面那四条命令（`files-mkdir` / `-delete` / `-rename` / `-chmod`）。
///
/// 🔴 **路径切成 `(root, rel)`**：后端写面按「一个根 ＋ 一段相对名」收参（路径解析逐段判 `rel`，
/// 拒 `..` / 盘符 / 空段）。窗口上的写操作全都发生在**当前目录里**（改名只许同目录，
/// 由 `clean_name` 那道闸先挡：名字里不许带 `/`），所以切法恒是「上一级 ＋ 尾段」，走
/// [`parent_dir`] / [`remote_basename`] 那一对（不另写一份切法）。
/// ⚠ 改名两端**不在同一个目录** ⇒ 这里当场拒（后端那一格只有一个 `root`），不猜。
///
/// ⚠ 回来的 `Err` **原样**带出去（后端路径解析那句拒绝经 [`super::source::said`] 翻成人话），
/// 由 [`WriteBoard::ui`] 画到窗口上 —— 这一层不改写、不摘要。
pub async fn apply_remote(line: &Line, origin: &Origin, op: &WriteOp) -> Result<(), String> {
    apply_remote_in(line, origin, op, None).await
}

/// 〔「有损名…整条寻址链要换成字节」〕同 [`apply_remote`]，但**根**由调用方给字节：
/// 窗口的当前目录不是合法 UTF-8 时（`root_raw = Some`），`root` 发 `{"b16": …}`（写操作的对象恒是当前目录的直接子项，
/// 根就是当前目录）；`None` ⇒ 与 [`apply_remote`] 逐字节同。
pub async fn apply_remote_in(
    line: &Line,
    origin: &Origin,
    op: &WriteOp,
    root_raw: Option<&[u8]>,
) -> Result<(), String> {
    let (cmd, mut args) = match op {
        WriteOp::Mkdir { path } => (
            "files-mkdir",
            serde_json::json!({ "root": parent_dir(path), "rel": remote_basename(path) }),
        ),
        // 目录 ⇒ `recursive: true`（连同里面全部内容，后端逐条目过路径解析）；文件 ⇒ 不带（射程同此前）。
        WriteOp::Delete { path, is_dir, raw } => {
            let mut a = serde_json::json!({
                "root": parent_dir(path),
                "rel": rel_json(remote_basename(path), raw.as_deref()),
            });
            if *is_dir {
                a["recursive"] = serde_json::Value::Bool(true);
            }
            ("files-delete", a)
        }
        WriteOp::Rename { from, to, raw } => {
            let root = parent_dir(from);
            if parent_dir(to) != root {
                return Err(copy_text(
                    "rsFilewinWriteops.remote.renameSameDir",
                    &[("op", &(op.label()).to_string())],
                ));
            }
            (
                "files-rename",
                serde_json::json!({
                    "root": root,
                    "from": rel_json(remote_basename(from), raw.as_deref()),
                    "to": remote_basename(to),
                }),
            )
        }
        WriteOp::Chmod { path, mode, raw } => (
            "files-chmod",
            serde_json::json!({
                "root": parent_dir(path),
                "rel": rel_json(remote_basename(path), raw.as_deref()),
                "mode": mode,
            }),
        ),
    };
    if let Some(r) = root_raw {
        args["root"] = super::source::wire_bytes(r);
    }
    super::source::ask(line, origin, cmd, &args, budget_for(op))
        .await
        .map(|_| ())
}

/// 这一件的往返上限：删目录（整棵树）放宽到 [`TREE_BUDGET`]，其余照旧 [`WRITE_BUDGET`]。
pub fn budget_for(op: &WriteOp) -> std::time::Duration {
    match op {
        WriteOp::Delete { is_dir: true, .. } => TREE_BUDGET,
        _ => WRITE_BUDGET,
    }
}

/// 删一整棵树的往返上限。后端那一趟在阻塞档（开跑之后打不断），上限十万条、
/// 每条两次路径解析 ⇒ 给它比单件写宽一档的等待；**这个数只管「窗口等多久」**，不管后端跑多久。
pub const TREE_BUDGET: std::time::Duration = std::time::Duration::from_secs(120);

/// 删这一行的那一件（有损名带着原始字节）。窗口上「删除」那一跳（单删 · 批量删）只经这一处拼。
pub fn delete_op(r: &Listed) -> WriteOp {
    WriteOp::Delete {
        path: r.path.clone(),
        is_dir: r.is_dir,
        raw: r.raw_name.clone(),
    }
}

/// 写面一件的往返上限（调用方给的期限，：说法归调用方）。
///
/// ⚠ 与后端那一侧无关：后端那几条都在阻塞档、开跑之后打不断；这个数只管「窗口等多久」。
pub const WRITE_BUDGET: std::time::Duration = std::time::Duration::from_secs(20);

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
    /// 把 `from` 改成同一个目录里的另一个名字（`raw` = `from` 尾段的原始字节，有损名才有）。
    Rename { from: String, raw: Option<Vec<u8>> },
    /// 把**这几项**的权限改成框里那个八进制数（一项 = 单改；N 项 = 批量改，一次问完）。
    Chmod { targets: Vec<ChmodTarget> },
}

/// 改权限那个框里的一项：路径 ＋ 名字的原始字节（有损名才有）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChmodTarget {
    pub path: String,
    pub raw: Option<Vec<u8>>,
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
    /// 改权限那个框：读回来的现值**已经处理过一次**了（预填过，或决定不预填）。
    /// 只预填一次 —— 用户清空那一栏之后不再被塞回去。
    pub prefilled: bool,
}

impl WritePrompt {
    pub fn for_mkdir(dir: &str) -> Self {
        Self {
            kind: PromptKind::Mkdir,
            dir: dir.to_string(),
            src_name: String::new(),
            text: String::new(),
            prefilled: false,
        }
    }

    /// 改名那个框 —— 缺省填**原名**（同旧面板 `panel.ts::rename` 的 `prompt(…, e.name)`）。
    ///
    /// 有损名缺省填的是**显示串**（含 U+FFFD）：用户要的正是把它改成一个读得出的名字；
    /// 寻址旧名走 `raw` 那几个真字节，不走这个显示串。
    pub fn for_rename(dir: &str, r: &Listed) -> Self {
        Self {
            kind: PromptKind::Rename {
                from: r.path.clone(),
                raw: r.raw_name.clone(),
            },
            dir: dir.to_string(),
            src_name: r.name.clone(),
            text: r.name.clone(),
            prefilled: false,
        }
    }

    /// 改权限那个框 —— 摆出来时**空着**；现值由 [`ModeProbe`] 逐项问回来之后再说、再预填（[`mode_readout`]）。
    /// 后端 `files-stat` 送 `mode` 了。〔墓碑 —— FW5 那一版这里写着「显示现值**今天做不到**：
    /// 后端 `files-ls` / `files-stat` 两条读口都不送权限位」。〕
    pub fn for_chmod(dir: &str, r: &Listed) -> Self {
        Self::for_chmod_many(dir, &[r])
    }

    /// **批量改权限**那个框：N 项共用一个八进制数，出 N 件，走「一次问完」。
    ///
    /// ⚠ 空摞 ⇒ 一个 `targets` 为空的框（[`Self::to_ops`] 会拒它，不会出零件却说「做完了」）。
    pub fn for_chmod_many(dir: &str, rows: &[&Listed]) -> Self {
        Self {
            kind: PromptKind::Chmod {
                targets: rows
                    .iter()
                    .map(|r| ChmodTarget {
                        path: r.path.clone(),
                        raw: r.raw_name.clone(),
                    })
                    .collect(),
            },
            dir: dir.to_string(),
            // 多项时框上那一行按件数说（[`Self::heading`]），不拼名字。
            src_name: match rows {
                [one] => one.name.clone(),
                _ => String::new(),
            },
            text: String::new(),
            prefilled: false,
        }
    }

    /// 框上那一行提示。
    pub fn heading(&self) -> String {
        match &self.kind {
            PromptKind::Mkdir => copy_text("rsFilewinWriteops.heading.mkdir", &[]),
            PromptKind::Rename { .. } => copy_text(
                "rsFilewinWriteops.heading.rename",
                &[("name", &self.src_name.to_string())],
            ),
            PromptKind::Chmod { targets } if targets.len() > 1 => copy_text(
                "rsFilewinWriteops.heading.chmodMany",
                &[("n", &(targets.len()).to_string())],
            ),
            PromptKind::Chmod { .. } => copy_text(
                "rsFilewinWriteops.heading.chmodOne",
                &[("name", &self.src_name.to_string())],
            ),
        }
    }

    /// 框里那几个字变成**恰好一件**真操作（批量改权限那一形请走 [`Self::to_ops`]）。
    ///
    /// # Errors
    ///
    /// 回的是**一句给用户的话**（调用方据此出声，且把框留着）——
    /// 静默收掉的话，用户点了确认什么都没发生，与成功长得一模一样。
    pub fn to_op(&self) -> Result<WriteOp, String> {
        let mut ops = self.to_ops()?;
        match ops.len() {
            1 => Ok(ops.remove(0)),
            n => Err(copy_text(
                "rsFilewinWriteops.toOp.notOne",
                &[("n", &n.to_string())],
            )),
        }
    }

    /// 框里那几个字变成**这一摞**真操作：新建目录 / 改名恒一件；改权限 = 框里那几项各一件。
    ///
    /// # Errors
    ///
    /// 同 [`Self::to_op`]：一句给用户的话，框留着。
    pub fn to_ops(&self) -> Result<Vec<WriteOp>, String> {
        let t = self.text.trim();
        match &self.kind {
            PromptKind::Mkdir => {
                let name = clean_name(t)?;
                Ok(vec![WriteOp::Mkdir {
                    path: join_remote(&self.dir, &name),
                }])
            }
            PromptKind::Rename { from, raw } => {
                let name = clean_name(t)?;
                let to = join_remote(&self.dir, &name);
                // ⚠ 有损名：显示串相等不等于名字没变（旧名的真字节不是这几个字）⇒ 只对无损名判「就是原名」。
                if &to == from && raw.is_none() {
                    return Err(copy_text(
                        "rsFilewinWriteops.toOps.sameName",
                        &[("name", &name.to_string())],
                    ));
                }
                Ok(vec![WriteOp::Rename {
                    from: from.clone(),
                    to,
                    raw: raw.clone(),
                }])
            }
            PromptKind::Chmod { targets } => {
                let mode = parse_mode(t)?;
                if targets.is_empty() {
                    return Err(copy_text("rsFilewinWriteops.toOps.nothingToChmod", &[]));
                }
                Ok(targets
                    .iter()
                    .map(|c| WriteOp::Chmod {
                        path: c.path.clone(),
                        mode,
                        raw: c.raw.clone(),
                    })
                    .collect())
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
///
/// 「新建空文件」（[`super::create`]）问的也是这一个函数 —— 两颗并排的「新建」一套规矩。
pub(super) fn clean_name(t: &str) -> Result<String, String> {
    if t.is_empty() {
        return Err(copy_text("rsFilewinWriteops.name.empty", &[]));
    }
    if t.contains('/') {
        return Err(copy_text(
            "rsFilewinWriteops.name.hasSlash",
            &[("name", &t.to_string())],
        ));
    }
    if t == "." || t == ".." {
        return Err(copy_text(
            "rsFilewinWriteops.name.isDir",
            &[("name", &t.to_string())],
        ));
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
        return Err(copy_text("rsFilewinWriteops.mode.empty", &[]));
    }
    let mode = u32::from_str_radix(t, 8).map_err(|_| {
        copy_text(
            "rsFilewinWriteops.mode.notOctal",
            &[("text", &t.to_string())],
        )
    })?;
    if mode > 0o7777 {
        return Err(copy_text(
            "rsFilewinWriteops.mode.outOfRange",
            &[("text", &t.to_string())],
        ));
    }
    Ok(mode)
}

/// 远端路径**恒用 `/`** 拼（同 [`super::source::parent_dir`] 那条理由：
/// 拿 `std::path` 切远端路径，在 Windows 上会把 `\` 也当分隔符）。
pub(super) fn join_remote(dir: &str, name: &str) -> String {
    format!("{}/{}", dir.trim_end_matches('/'), name)
}

// ═══════════════════════════════════════════════════════════════════════
// 改权限那个框的**现值**
// ═══════════════════════════════════════════════════════════════════════

/// 框上那一行说的现值，以及要不要预填。**纯函数**（判据直接喂它）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModeReadout {
    /// 框上那一行（「现在是 644」/「这几项现在的权限不一样」/「读不到现在的权限」）。
    pub line: String,
    /// 预填进那一栏的八进制串；`None` = 不预填（各项不同 / 读不到）。
    pub prefill: Option<String>,
}

/// 逐项读回来的权限位（`None` = 那一项没读到）⇒ 框上怎么说。
///
/// - 全读到且都一样 ⇒ 「现在是 644」，预填 `644`（一项就是这一形）；
/// - 全读到但不一样 ⇒ 「这几项现在的权限不一样」，不预填（填哪一个都是替用户挑）；
/// - 有一项没读到 ⇒ 「读不到现在的权限」，不预填 —— **不猜**：预填一个猜出来的值而用户直接点确认 = 静默改坏权限；
/// - 空摞 ⇒ 同「读不到」（框本身会被 [`WritePrompt::to_ops`] 拒，不会走到这里）。
pub fn mode_readout(modes: &[Option<u32>]) -> ModeReadout {
    let unreadable = || ModeReadout {
        line: copy_text("rsFilewinWriteops.mode.unreadable", &[]),
        prefill: None,
    };
    let Some(all) = modes.iter().copied().collect::<Option<Vec<u32>>>() else {
        return unreadable();
    };
    let Some(&m) = all.first() else {
        return unreadable();
    };
    if all.iter().any(|x| *x != m) {
        return ModeReadout {
            line: copy_text("rsFilewinWriteops.mode.mixed", &[]),
            prefill: None,
        };
    }
    ModeReadout {
        line: copy_text("rsFilewinWriteops.mode.now", &[("mode", &format!("{m:o}"))]),
        prefill: Some(format!("{m:o}")),
    }
}

/// `files-stat` 一次应答里的 `mode`（缺席 / 不是非负整数 / 超出低 12 位 ⇒ `None`：当读不到，不猜）。
pub fn mode_of(reply: &serde_json::Value) -> Option<u32> {
    reply
        .get("mode")
        .and_then(serde_json::Value::as_u64)
        .and_then(|m| u32::try_from(m).ok())
        .filter(|m| *m <= 0o7777)
}

/// 逐项问一次 `files-stat`（同 [`super::transfer::probe_remote`] 那一问、同一个期限），按入参顺序交回。
pub async fn probe_modes(line: &Line, origin: &Origin, paths: &[String]) -> Vec<Option<u32>> {
    let mut out = Vec::with_capacity(paths.len());
    for p in paths {
        let got = super::source::ask(
            line,
            origin,
            "files-stat",
            &serde_json::json!({ "path": p }),
            super::transfer::PROBE_BUDGET,
        )
        .await;
        out.push(got.ok().as_ref().and_then(mode_of));
    }
    out
}

/// 现值那一趟的共享落点（UI 线程读，tokio 那条写）。带一个**代数**：框换了（又摆了一个），
/// 上一个框那一趟晚到的答案不许落到新框上。形状照 [`WriteBoard`]（含「敲窗口的手」）。
#[derive(Clone, Default)]
pub struct ModeProbe {
    inner: Arc<Mutex<ModeProbeInner>>,
}

#[derive(Default)]
struct ModeProbeInner {
    gen: u64,
    got: Option<Vec<Option<u32>>>,
    ctx: Option<egui::Context>,
}

impl ModeProbe {
    /// 新摆了一个框：代数 +1、清掉上一个框的答案，交回这一趟的代数。
    pub fn start(&self) -> u64 {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.gen += 1;
        g.got = None;
        g.gen
    }

    /// 答案到了。代数对不上（框已经换了）⇒ 丢掉。
    pub fn land(&self, gen: u64, modes: Vec<Option<u32>>) {
        let ctx = {
            let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            if g.gen != gen {
                return;
            }
            g.got = Some(modes);
            g.ctx.clone()
        };
        if let Some(c) = ctx {
            c.request_repaint();
        }
    }

    /// 把窗口交给它（答案到了要敲一下，不然等用户动鼠标才画出来）。
    pub fn attach(&self, ctx: egui::Context) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).ctx = Some(ctx);
    }

    /// 这一个框的现值（还没答回来 ⇒ `None`）。
    pub fn readout(&self) -> Option<ModeReadout> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .got
            .as_deref()
            .map(mode_readout)
    }
}

/// 现值答回来之后，框里那一栏预填**至多一次**（纯函数：判据直接喂它）。
///
/// 只在「还没处理过」且「那一栏是空的」时填；填与不填都记「处理过了」—— 用户后来清空那一栏，不再被塞回去。
pub fn apply_prefill(p: &mut WritePrompt, r: &ModeReadout) {
    if p.prefilled {
        return;
    }
    p.prefilled = true;
    if p.text.is_empty() {
        if let Some(v) = &r.prefill {
            p.text = v.clone();
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 窗口那一侧的状态：**问什么 · 上一摞做完了什么**
// ═══════════════════════════════════════════════════════════════════════

/// 一摞写操作在窗口上的样子。**跨线程共享**（UI 线程画，tokio 那条写）。
///
/// 形状照 [`super::transfer::DropBoard`] 办，含那只「敲窗口的手」——
/// egui 只在有事发生时才画下一帧，不敲一下结果要等用户动鼠标才出现
/// （「卡住了」与「真的没在跑」在屏幕上分不开）。
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
    /// 原来这里还先画「被围栏挡住的那几件」（红字，`blocked` 那一段）；围栏拿掉之后那一段删了。
    /// 失败那几件（含后端路径解析拒的）照旧必须在屏幕上出声 —— 「什么都没发生」与「做完了」在屏幕上分不开。
    pub fn ui(&self, ui: &mut egui::Ui) {
        let (asking, mut ticks, last) = {
            let b = self.inner.lock().unwrap();
            (b.asking.clone(), b.ticks.clone(), b.last.clone())
        };
        if !asking.is_empty() {
            let mut answer: Option<bool> = None;
            let mut changed = false;
            egui::Modal::new(egui::Id::new("filewin-write-confirm")).show(ui.ctx(), |ui| {
                ui.heading(copy_text(
                    "rsFilewinWriteops.confirm.ask",
                    &[("n", &(asking.len()).to_string())],
                ));
                ui.colored_label(
                    egui::Color32::from_rgb(0xE0, 0x9A, 0x20),
                    &copy_text("rsFilewinWriteops.confirm.warn", &[]),
                );
                for (i, o) in asking.iter().enumerate() {
                    let mut t = ticks[i];
                    if ui.checkbox(&mut t, o.label()).changed() {
                        ticks[i] = t;
                        changed = true;
                    }
                }
                ui.horizontal(|ui| {
                    if ui
                        .button(&copy_text("rsFilewinWriteops.confirm.doChecked", &[]))
                        .clicked()
                    {
                        answer = Some(true);
                    }
                    if ui
                        .button(&copy_text("rsFilewinWriteops.confirm.doNone", &[]))
                        .clicked()
                    {
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
            if !o.failed.is_empty() {
                ui.colored_label(
                    egui::Color32::RED,
                    copy_text(
                        "rsFilewinWriteops.result.failed",
                        &[
                            ("n", &(o.failed.len()).to_string()),
                            (
                                "failed",
                                &(o.failed
                                    .iter()
                                    .map(|(n, e)| format!("{n}（{e}）"))
                                    .collect::<Vec<_>>()
                                    .join(&copy_text("rsFilewinWriteops.result.failedSep", &[])))
                                .to_string(),
                            ),
                        ],
                    ),
                );
            } else if o.ok > 0 || o.skipped > 0 {
                ui.label(copy_text(
                    "rsFilewinWriteops.result.done",
                    &[
                        ("ok", &o.ok.to_string()),
                        ("skipped", &o.skipped.to_string()),
                    ],
                ));
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/writeops_tests.rs"]
mod tests;
