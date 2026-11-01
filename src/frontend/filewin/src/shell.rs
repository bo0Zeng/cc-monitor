//! 目录视图：一个标签页就是一个 [`FileWindow`]（列哪个目录 · 怎么排 · 选中了什么 · 起了哪几趟 · 摆着哪一问）。
//!
//! 本文件只放它的本体：字段 · 新建 · 导航与历史 · 计划归属 · 在此打开终端 · 「关得了吗」那几问。
//! 别的按职责住 `shell/` 底下（都是 `impl FileWindow` 的一段，或只给它用的零件）：
//!
//! | 文件 | 管什么 |
//! |---|---|
//! | `listing` | 列目录的共享落点 · 落地那两个口 |
//! | [`window`] | 开窗：次线程上的事件循环 · 进程 DPI · 夹进工作区 · 窗口标题 |
//! | `frame` | 一帧怎么画：正文 · 列表 · 那几行提示 · 画之前先收货 |
//! | `search` | 查找 · 按内容搜接到窗口上（搜索框 · 命中那一摞） |
//! | `trips` | 起一趟、收一趟：拖入上传 · 复制为 · 一组 · 解压 · 往外拖 · 算大小 |
//! | `cross` | 复制到另一台那一问（机器 · 放到哪） |
//! | `writes` | 新建目录 · 改名 · 删除 · 改权限 · 就地那一格 · 撤销 |
//! | `edit` | 改一份远端文本（编辑页） |
//! | `keys` | 键盘 · 多选 · 右键菜单接到窗口上的那几跳 |
//!
//! # ⚠ 没做到的，写在这儿而不是藏着
//!
//! - **本机跑不了真窗口**（`XDG_SESSION_TYPE=tty`，无图形会话）⇒ [`window::open_detached_seeded`]
//!   这条路在本机**没有端到端读数**；有的是 scratchpad 那个 `tao ＋ eframe` 原型的
//!   三趟读数（见 `super` 头注）与编译期的三档 `cargo check`。
//!   ⚠ 第二刀在这条上多欠一句：**鼠标真的双击一下那一行会怎样，本机判不了**
//!   （没有图形会话就没有真事件源）。判据喂的是**合成事件**，
//!   它买的是「egui 收到这串事件之后认出来的是哪一行」——
//!   逐条与那次现打的读数写在 [`super::rows`] 的 `paint_one_row` 头注里。
//! - **Windows 上一次都没跑过。**
//! - **预览 · 双栏 · 标签页做了**，住 [`super::workspace`]（最外一层）与
//!   [`super::preview`]；本文件的 [`FileWindow`] 照旧是「一个目录视图」，一个标签页就是一个它。
//!   〔此前这一格写的是「预览 · 双栏仍然一个都没有」（ 代价第 2 条）。〕
//! **多选与右键菜单做了**（还有键盘）：纯的那一份（选中态 · 键位 ·
//!   「能做什么」那张表）住 [`super::select`]，接到窗口上的那几跳住本文件
//!   [`FileWindow::apply_keys`] / [`FileWindow::apply_pick_click`] /
//!   [`FileWindow::apply_menu_click`] / [`FileWindow::perform`]。
//!   🔴 **写操作一条新路都没长**：键盘与菜单做的每一件，都落回行上那几颗按钮已经在走的
//!   那几个 `begin_*`（以及删除那一摞的 `start_writes`）⇒ 一次问完 · 只经通道说 `call`
//!   这几道闸一道都没绕开。
//!   ⚠ **拖放从这一条里划出去了**：第二刀做了「拖入本机文件 → 上传到当前远端目录」
//!   那一半（见 [`FileWindow::start_drop`] 与 [`super::transfer`]）；
//!   往外拖（下载）做了（[`super::download`]）；窗口之间互拖没做。
//! - （大文件编辑改流式）**仍然没做**。
//! - （拖入多文件先一次问完再并行）做了：整组一条开单（同名由后端在落点那台探、冲突框问一次），
//!   开了单的并行传，住 [`super::transfer::run_drop`]；判据钉的是开单一次与并行度。

use comms_inward::chan::wire::Budget;
use copy_core::copy_text;
use copy_core::said::Said;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::copy::{CopyJob, CopyPrompt};
use super::find::{self, SearchBoard};
use super::fonts::{self, FontState};
use super::grep::{self, GrepBoard, GrepTally};
use super::rows::{show_hit_rows, HitTally, RenderTally};
use super::select::{self, Action, Intent, Item, ItemFace, Selection, TypeAhead};
use super::source::{Line, Listed, Sort, Source};
use super::transfer::{DropBoard, Pending};
use super::writeops::{is_writable, PromptKind, WriteBoard, WriteOp, WritePrompt};

mod cross;
mod edit;
mod frame;
mod keys;
pub mod listing;
mod search;
mod trips;
pub mod window;
mod writes;
use self::{listing::*, window::*};

/// 接上通道时问那台能力事实的期限（一问一答，同读侧那几问的量级）。
const OFFER_WITHIN: std::time::Duration = std::time::Duration::from_secs(10);
/// 「在此打开终端」那一问的期限（monitor 接下它：问本机后端渲那一行 ＋ 开窗，都在本机）：给足握手余量即可。
const TERMINAL_WITHIN: std::time::Duration = std::time::Duration::from_secs(10);

/// 就地那一格那一趟的落点（UI 线程读，tokio 那条写）。
type InlineDone = Arc<Mutex<Option<(u64, Result<String, super::source::Failed>)>>>;

/// **「存到哪儿」那一问的缺省落点** —— 用户的 home。
///
/// 🔴 它只有一个消费者 —— [`super::picker::downloads_dir`]（系统存盘框缺省落在哪个文件夹；
/// 窗口只有远端一侧，见 `source.rs` 头注「只有远端一侧」那一节）。
/// ⇒ 它**不是文件管理器的一部分**，是**往外传**那条路上的一格。
///
/// 家目录取法同全仓（`creds_core::store::home_dir`，monitor 与后端同一家）。本包够不着壳的 `data_paths`，
/// 家目录只在这一处取，本包别处要家目录都经它（`clippy.toml` 取家目录那一组在这里就地放行）。
///
/// ⚠ 它**只把 home 当一条缺省路径**，不去读 home 里的任何东西 —— 零次 `read_dir`。
/// 拿不到 home 就退到 `.`（当前工作目录），**不猜一个路径出来**。
#[expect(
    clippy::disallowed_methods,
    reason = "本包取主目录只在这一处（用不了 `data_paths`），只当存盘框起始的目录"
)]
pub fn local_home() -> String {
    creds_core::store::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string())
}

/// 没连上通道时，每一件要问后端的事说的那一句（`D11`：不退回 SFTP、不静默）。
pub static NO_LINE: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinShell.noLine.message", &[]));

/// 窗口的全部状态。
/// 🔴**「就是这个文件」** —— 一次 reveal 的两半。
///
/// ⚠ 刻意是一个结构而不是 `Option<String>` ＋ 一个 `bool`：那两个散着放的时候
/// 「滚过了但名字清空了」与「名字还在但忘了滚」两种半态都可表示，
/// 而它们在屏幕上一个是「没高亮」、一个是「高亮了但在视野外」。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reveal {
    /// 要高亮那一行的**名字**（不是下标 —— 下标会随目录内容移位）。
    pub name: String,
    /// 滚过去了没有。**只滚一次**（每帧都滚就把用户自己的滚动按住了）。
    pub scrolled: bool,
}

pub struct FileWindow {
    pub source: Source,
    pub cwd: String,
    /// 〔「有损名的进目录…整条寻址链要换成字节」〕当前目录不是合法 UTF-8 时它的原始字节
    /// （`cwd` 是它的有损显示串）。`None` ⇒ `cwd` 就是真字节。发出去的一律经 [`Self::cwd_path`] / [`Self::row_path`]。
    pub cwd_raw: Option<Vec<u8>>,
    pub listing: Listing,
    pub tally: RenderTally,
    /// 命中那一摞这一帧画了什么。**与 [`Self::tally`] 刻意是两个类型** ——
    /// [`HitTally`] 里没有「谁被点了」这个概念，逐条理由住那个类型的头注。
    pub hits_tally: HitTally,
    /// 远端列目录要在 tokio 上跑，而**判据里大量窗口拿不到运行时**，所以是 `Option`。
    ///
    /// 它是 `Option`：
    /// 起窗那条路（`super::proc`）与判据都可能手上没有运行时，而那时它要**出声**
    /// （[`Self::reload`] 里那一支），不许假装列了个空目录。
    pub rt: Option<tokio::runtime::Handle>,
    /// 🔴**通道**：窗口进程够后端的唯一一条路（`source::Line`）。
    ///
    /// 是 `Option` 的理由与 [`Self::rt`] 同：判据里大量窗口不连后端（只画行、只点按钮）。
    /// 没有它的时候，每一件要问后端的事都**出声**（[`NO_LINE`]），不静默、不退回 SFTP（`D11`）。
    pub line: Option<Line>,
    /// 这台机器的能力事实（接上通道时问一次，`chan::wire::Offer`）：做不到的那几件在菜单上置灰并说为什么。
    pub offer: std::sync::Arc<std::sync::Mutex<Option<comms_inward::chan::wire::Offer>>>,
    /// 拖入的样板：这一窗记着的两样（那台的 `$HOME` · 改走后端链路那一句）。每一摞从它 `fresh_like` 一块新看板。
    pub board: DropBoard,
    /// 🔴 **这个标签页起的、还没收场的那几趟**（「进度」表里那几行的号）。
    ///
    /// 每一趟的状态只住它那块看板里、结局只在 [`super::progress::Job::landing`] 判一次；这里只记号，
    /// 收场（回执 · 重列 · 清选中）由 [`Self::settle_trips`] 照那一个结局做。
    /// ⚠ 为什么不让那条 tokio 任务自己 `reload()`：它手上没有 `&mut self` ⇒ 任务只记「我跑完了」，
    /// **换目录这件事一直留在 UI 线程手上**。
    trips: Vec<u64>,
    /// 正在读的那一份的原始字节（到货时交给编辑面；`(显示路径, 字节)`）。
    edit_raw: Option<(String, Vec<u8>)>,
    /// 〔「原生选文件框」〕选择框（生产是操作系统自己的，判据换一个假的）＋ 结局落点 ＋ 下载那一问上「没选到」那句话。
    pub picker: std::sync::Arc<dyn super::picker::Picker>,
    pick_board: super::picker::PickBoard,
    /// 「复制为」那个框。`None` = 没在问名字。**UI 线程自己的**（理由见 [`CopyPrompt`]）。
    copy_prompt: Option<CopyPrompt>,
    /// 「复制到另一台」下拉里的机器（开窗种子带来；新标签页照抄）。
    pub machines: Vec<String>,
    /// 「复制到另一台」那一问（UI 线程自己的）。
    cross_prompt: Option<super::cross_copy::CrossPrompt>,
    /// 那一问里「放到」那一块小目录选择器（那台的文件夹；`cross_copy::DirPick`）。
    pub cross_pick: super::cross_copy::DirPick,
    /// 框选按下那一刻的选中（Ctrl 框加进它 · Esc 回到它）；没在框选 ⇒ `None`。
    band_base: Option<select::Selection>,
    /// 下载多项：选目录框开着时要下的那几行。
    pull_many: Vec<Listed>,
    /// 窗口此刻是两栏（「复制到另一栏」那一项只在两栏时有）。由窗口那一级每帧写。
    pub dual: bool,
    /// 菜单上点了「复制到另一栏」：窗口那一级接（两栏的事归它）。
    pub across_wanted: bool,
    /// `fonts.rs`：这个窗口的字体装没装上、复核没复核过。
    ///
    /// 🔴 默认是 `NotInstalled` 而**不是**「一切正常」——
    /// 判据直接 `FileWindow::new(...)` 建出来的窗口就是这一态，而它**会在界面上出声**。
    /// 「装字体那一步被谁摘了」因此不可能安静地过去。
    pub font: FontState,
    /// 搜索那一趟的共享落点（UI 线程读，tokio 那条写）。
    pub search: SearchBoard,
    /// 搜索框里那几个字。**UI 线程自己的**（同 [`Self::copy_prompt`] 的理由：
    /// 它是一个正在被编辑的草稿，不该出现在两条线程共享的那份状态里）。
    query: String,
    /// 搜索范围「整台机器」（UI 线程自己的；关 ⇒ 当前目录以下）。
    search_whole: bool,
    /// 结果表按哪一列排（排是后端排的，这里只记用户点了哪一列）。
    hit_sort: find::FindSort,
    /// 结果表上选中的那一条（下标只指命中这一摞）。
    hit_pick: Option<usize>,
    /// 按内容搜那一趟的共享落点（UI 线程读，tokio 那条写；`super::grep`）。
    pub grep: GrepBoard,
    /// 按内容搜那个框里的字（UI 线程自己的草稿，同 [`Self::query`]）。
    grep_query: String,
    /// 命中那一摞这一帧交出来的东西（被点了哪一条 —— **命中这一摞**的下标）。
    pub grep_tally: GrepTally,
    /// 🔴：那四条写操作的状态机（**一次问完 · 结果**）。
    pub write_board: WriteBoard,
    /// 「叫什么名字 / 改成什么权限」那个框。`None` = 没在问。
    /// **UI 线程自己的**（理由见 [`WritePrompt`]）。
    write_prompt: Option<WritePrompt>,
    /// 改权限那个框的**现值**那一趟（UI 线程读，tokio 那条写；`writeops::ModeProbe`）。
    pub mode_probe: super::writeops::ModeProbe,
    /// 就地那一格（改名 · 新建）答完发出去的那一趟落在这儿：`(第几趟, 成了 ⇒ 新名字 / 没成 ⇒ 码 ＋ 那句话)`。
    inline_done: InlineDone,
    /// 就地那一格发出去的趟数（晚到的上一趟不许落到这一格上）· 这一格在飞吗 · 下一帧要选中前几个字。
    inline_gen: u64,
    inline_busy: bool,
    inline_select: Option<usize>,
    /// 上一次就地那一趟没成时框里那串字：没改过就不让「点别处」再发一趟（回车照样重试）。
    inline_failed_text: Option<String>,
    /// 已经消化过几摞写操作（`write_board.rounds()` 走在它前面 ⇒ 该收场了）。
    seen_write_rounds: u64,
    /// 「存到哪儿 / 盖掉它吗」那两问。`None` = 没在问。
    /// **UI 线程自己的**（同 [`Self::write_prompt`] 的理由：它是一个正在被编辑的草稿）。
    /// 系统存盘框开着时要下的那一项：`(远端整条路径, 名字)`（选完落点就起下载；取消 ⇒ 什么都不做）。
    pull_want: Option<(String, String)>,
    /// 那一问摆着的那一行，若整条路径不是 UTF-8：`(整条路径的字节, 显示名, 名字的字节)`。
    pull_raw: Option<(Vec<u8>, String, Vec<u8>)>,
    /// 🔴编辑那一趟的共享落点（读到货 · 存结局）。
    pub edits: super::editor::EditBoard,
    /// 打开着的那一份文本。`None` = 没在编辑。**UI 线程自己的**。
    editing: Option<super::editor::Pane>,
    /// 关窗那一问正摆着吗（改了没存）。
    asking_discard: bool,
    /// **按哪一列排、正着还是反着**（点表头改）。缺省按名称。
    pub sort: Sort,
    /// 详情视图的列宽（拖表头上的分隔线改）。
    pub cols: super::rows::Columns,
    /// 后退 · 前进两摞（每个标签页自己一份；最近的在末尾）。
    pub(super) back: Vec<super::source::RemotePath>,
    pub(super) ahead: Vec<super::source::RemotePath>,
    /// 地址栏正在手输吗（`Some` ＝ 输入框里那一串）。
    pub(super) addr_edit: Option<String>,
    /// 搜索框在「内容」那一段（UI 线程自己的）。
    pub(super) search_content: bool,
    /// 打不开那一条上点了「回主目录」（主目录住窗口那一级：由 `Workspace` 接过去）。
    pub(super) want_home: bool,
    /// 这一趟列目录从哪一刻起在等（骨架行 300 ms 后才画）。
    loading_since: Option<f64>,
    /// 窗口底部那张「进度」表（一扇窗一份，所有标签页共用；`Workspace` 交下来）。逻辑住 [`super::progress`]。
    pub progress: super::progress::Progress,
    /// 此刻能不能写：那台连没连着（一扇窗一份，`Workspace` 交下来）。断着 ⇒ 列表留着上次的内容、写类按钮灰。
    pub gate: super::chrome::WriteGate,
    /// 一次性的回执（「路径已复制」…）：窗口那一级每帧收走、摆成右下角的回执。
    pub(super) receipt: Option<String>,
    /// 同 [`Self::receipt`] 那一句的［复制详情］整段（写操作没做成、带详情时才有；与那一句同一拍交走）。
    pub(super) receipt_copy: Option<String>,
    /// 带［撤销］的回执（改名 · 改权限做完）：那句话 ＋ 撤销要做的那几件。窗口那一级收走，点了撤销交回 [`Self::start_undo`]。
    pub(super) receipt_undo: Option<(String, Vec<WriteOp>)>,
    /// 列表上按了空格（「看一眼」开 / 收）：窗口那一级每帧收走。
    pub(super) want_peek: bool,
    /// 用户按过「重建索引」（换目录就清）：新鲜度那一行这时也摆出来，答他刚问的那一下。
    pub(super) status_wanted: bool,
    /// 「属性」那一问（`None` ＝ 没摆）。逻辑住 [`super::props`]。
    pub(super) props: Option<super::props::Props>,
    /// 🔴〔补齐五项〕「在此打开终端」那一下**说了什么**（`None` = 没点过 / 上一下没话说）。
    ///
    /// 那一下可能失败（那台远端的配置没存全 · 本机找不到终端 · 本机没有 ssh 客户端），而一次失败与一次成功
    /// 在屏幕上长得一样 ⇒ 结果落在这一格，界面上画出来，判据读同一个值。
    term_notice: Arc<Mutex<Option<String>>>,
    /// 「在计划里看」那一下没成说的话（同 [`Self::term_notice`] 那一形：摆在列表上面）。
    plan_notice: Arc<Mutex<Option<String>>>,
    /// 🔴**「就是这个文件」** —— 要高亮的那一行的名字 ＋ 滚过去了没有。
    ///
    /// 会话工具卡 → 文件跳转那一形（`src/frontend/ui/cards/index.ts::openRemoteFileInSftp`）靠它。
    ///
    /// ⚠ 两个字段刻意分开：**高亮要一直留着**（一帧的高亮在连续重绘的窗口上等于看不见），
    /// 而**滚只滚一次**（每帧都滚就把用户自己的滚动按住了）。
    reveal: Option<Reveal>,
    /// 🔴**选中态**（哪几行 · 键盘光标 · Shift 的锚）。**UI 线程自己的**。
    ///
    /// ⚠ 按名字记（理由住 [`super::select`] 头注 §二）。换目录清空；
    ///   一摞写操作跑完只清「选中」、留着光标（[`Self::settle_finished_writes`]）。
    selection: Selection,
    /// 打字跳转攒着的那几个字。
    type_ahead: TypeAhead,
    /// 键盘把光标挪到了第几行 ⇒ 这一帧画列表时要不要把它滚进视野。**只滚一次**。
    key_scroll: Option<usize>,
    /// 摆着的那个右键菜单（`None` = 没摆）。
    menu: Option<MenuAt>,
    /// 键盘 / 菜单那一下**做不了**时说的那句话（`None` = 没话说）。
    ///
    /// ⚠ 刻意不写进 `listing.error`：那一格只在下一趟列目录时才清，
    ///   而「打字跳转没找到」是一句**下一次按键就过时**的话 ⇒ 下一次按键 / 点击就清掉。
    key_notice: Option<String>,
    /// 书签（一个窗口一份，所有标签页 / 两栏共用；逻辑住 [`super::bookmarks`]）。
    /// `None` ＝ 没接上（判据里直接建的窗口）⇒ 书签栏不画。生产那条开窗路恒是 `Some`。
    pub shelf: Option<super::bookmarks::Shelf>,
    /// 这个目录视图此刻**是不是焦点那一个**（双栏 / 标签页时只有一个是）。
    ///
    /// `false` ⇒ 不接键盘、不接拖入（[`Self::keys_blocked`] / [`Self::take_drops`]）。缺省 `true`：
    /// 单独建出来的目录视图（判据里那几百个）就是唯一那一个。谁来改它：[`super::workspace`]。
    pub focused: bool,
    /// 〔「行拖到另一栏的手势」〕有一行（连同它所在的那一摞选中）被拖起、还没松手。
    ///   松在哪儿由 [`super::workspace::Workspace`] 看（它才知道另一栏在哪）；它收摊时清掉这一格。
    pub dragging: bool,
    /// 起过几摞写操作（与 `write_board.rounds()` 比 ⇒ 有没有还没回话的；关标签那一问用）。
    writes_started: u64,
    /// 框里那一问做不成的原因（画在框里；框外列表上方那一行被暗底盖着）。框收掉就清。
    pub(super) prompt_error: Arc<Mutex<Option<super::source::Failed>>>,
    /// 搜索命中那一摞画好的行（按那一问的号与落地趟数缓存：没变就不重建）。
    hit_rows: (u64, u64, Arc<Vec<super::rows::HitRow>>),
    /// 这个标签页是一份文本的**编辑页**（标签上铅笔 ＋ 文件名，正文是编辑面、不是列表）。
    pub edit_tab: bool,
    /// 这个目录视图挂在窗口里（`Workspace` 置）：打开一份文本 ⇒ 交给窗口开一个编辑页，不在这一页里开。
    pub(super) in_workspace: bool,
    /// 这个标签页是一张图的**查看页**（标签上图片图标 ＋ 文件名，正文是画面、不是列表）。逻辑住 [`super::imgview`]。
    pub view_tab: bool,
    /// 摆着的那一张（`None` ＝ 不是查看页）。**UI 线程自己的**。
    pub(super) viewing: Option<super::imgview::Viewer>,
    /// 要开的那一张（目录页交给窗口那一级：开一个查看页 / 切到正摆着它的那一页）。
    pub(super) want_view: Option<super::imgview::Viewer>,
    /// 这扇窗里所有解好的图（预览 · 「看一眼」· 查看页共用一份；`Workspace` 让每个标签拿同一份）。
    pub pictures: super::picture::Pictures,
    /// 要开的那一份（目录页交给窗口那一级：开一个编辑页 / 切到已开着的那一页）。
    pub(super) want_edit: Option<EditWant>,
    /// 编辑页头上点了面包屑那一段 / 「在列表里显示」：窗口那一级开一个目录页到那里（或切到已在那里的那一页）。
    pub(super) want_dir: Option<String>,
    /// 「在列表里显示」要高亮的那个名字（随 [`Self::want_dir`] 交给那个目录页）。
    pub(super) want_reveal: Option<String>,
    /// 编辑页要关掉自己（那一问答了「不保存」，或答了「保存」而存成了）。
    pub(super) want_close: bool,
    /// 那一问答了「保存」：存成就关。
    close_after_save: bool,
    /// 打不开 / 不能编辑那一句（编辑页中央画；`None` ＝ 没出问题）。
    pub(super) edit_refused: Option<String>,
    /// 编辑页的那一行（「下载到本机」「重试」要它）。
    edit_row: Option<Listed>,
    /// 编辑页的查找条摊开着吗（Ctrl+F 开 · Esc / × 收）。
    pub(super) find_open: bool,
}

/// 目录页交给窗口的「开这一份」：那一行 ＋ 它的整条路径（有损名按字节）＋ 本地预判做不了的原因。
#[derive(Clone, Debug)]
pub struct EditWant {
    pub row: Listed,
    pub at: super::source::RemotePath,
    pub refused: Option<String>,
}

/// 一个摆着的右键菜单：**在哪儿 · 列哪几项（分几组）· 对几项说话**。
///
/// 🔴 那几项是**开菜单那一刻**按菜单那张表（[`select::menu_groups`]）算好的快照 ——
/// 每帧重算要扫一遍整摞行找选中（64 万行那一档每帧一趟 O(n)）。
/// 快照不会过时：菜单摆着的时候键盘不接（[`FileWindow::apply_keys`] 的闸），
/// 点别处菜单先关；而点菜单上那一项时 [`FileWindow::perform`] **再问一次**那张表。
#[derive(Clone, Debug, PartialEq)]
pub struct MenuAt {
    pub at: egui::Pos2,
    /// 表里这一种选中的那几组；每一项带着「这一份做不了」那一句（[`select::blocked`]；做得了 ⇒ `None`）。
    pub groups: Vec<Vec<(Item, Option<String>)>>,
    /// 这一刻真做得了的那几件（[`select::actions_for`]；判据与键盘那一侧对账用）。
    pub actions: Vec<Action>,
    pub n: usize,
    /// 单选的那一份归某一格 ⇒ 菜单多一项「在计划里看 · {标题}」（那一项的字；不归 ⇒ `None`）。
    pub plan: Option<String>,
    /// 🔴 第几次开菜单。egui 的弹层按「上一帧有没有这个 id 的响应」判「刚打开」：
    ///   摆着一个菜单时在另一行上再右键一下，同一个 id 会被当成「开着时有人点了别处」
    ///   当场关掉 ⇒ 右键第二下只关不开。每次开菜单换一个 id 就没有这一形。
    pub serial: u64,
}

/// 菜单上一项都没有时摆的那一句（有损名那一档：什么都做不了，但要说出来）。
pub static MENU_EMPTY: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinShell.menuEmpty.message", &[]));

impl FileWindow {
    /// 接上通道（`proc::child_main` 拨通之后调；判据里接一台合成后端）。
    pub fn attach_line(&mut self, line: Line) {
        // 接上就问一次那台的能力事实（不等它：没回来之前菜单照常，点了由那台的回话兜）。
        if let Some(h) = &self.rt {
            let (l, origin, slot) = (line.clone(), self.source.origin(), self.offer.clone());
            h.spawn(async move {
                let budget =
                    Budget::new(std::time::Instant::now() + OFFER_WITHIN, Default::default());
                if let Ok(o) = l.offer(&origin, budget).await {
                    *slot.lock().unwrap() = o;
                }
            });
        }
        self.line = Some(line);
    }

    /// 这一件此刻做不到 ⇒ 为什么（这台不支持 · 离线 · 只读；菜单 · 命令栏悬停 · 键盘按了的回话同一句）：判法在 `pick-core`，手机同一份。
    pub fn refused_here(&self, a: Action) -> Option<String> {
        let here = pick_core::Here {
            offline: self.gate.cut(),
            unavailable: self
                .offer
                .lock()
                .unwrap()
                .as_ref()
                .map(|o| o.unavailable_ops())
                .unwrap_or_default(),
        };
        pick_core::refused_here(a, &here)
    }

    /// 断线时命令栏那几颗哪颗做不了：要碰那台机器的（新建 · 上传 · 终端 · 复制到另一栏）⇒ 「离线 · 只读」；
    /// 只改这一扇窗怎么看的（左栏 · 显示隐藏文件 · 双栏 · 预览）照常。选中项上的那几件问 [`Self::refused_here`]。
    pub fn offline_cmd(&self, c: super::chrome::Cmd) -> Option<String> {
        use super::chrome::Cmd;
        let needs_machine = match c {
            Cmd::Sidebar | Cmd::Hidden | Cmd::Split | Cmd::Preview => false,
            Cmd::Act(a) => return self.refused_here(a),
            Cmd::Mkdir | Cmd::NewFile | Cmd::Upload | Cmd::Term | Cmd::Across => true,
        };
        needs_machine.then(|| self.gate.write_refused()).flatten()
    }

    /// 带着**已经列好的那一屏**建窗 —— [`super::entry::open_file_window`] 走这条。
    ///
    /// 🔴 为什么要有它：那条命令为了能在 webview 那侧**出声**（目录列不出来就别开窗），
    /// 已经先列过一趟了。再让 `new()` 列第二趟，就是对同一个远端目录连打两次往返。
    pub fn seeded(
        source: Source,
        cwd: String,
        rt: Option<tokio::runtime::Handle>,
        rows: impl IntoIterator<Item = impl Into<Listed>>,
    ) -> Self {
        let listing = Listing::default();
        // 🔴〔补齐五项〕第一屏走的是 `Vec<Listed>`（〔09-28 裁 3〕窗口进程自己列的，`proc::first_screen`）
        //    ⇒ 链接与时间两格第一屏就有。入参收 `Into<Listed>` 是为了
        //    判据夹具照旧能喂 `Vec<Row>`（那一形 ＝ `Listed::plain`，两格都「没送」）。
        // 这一屏照给的先后摆（第一屏是后端按缺省那一档排好的）。
        *listing.rows.lock().unwrap() = rows.into_iter().map(Into::into).collect();
        Self {
            source,
            cwd,
            cwd_raw: None,
            listing,
            tally: RenderTally::default(),
            hits_tally: HitTally::default(),
            rt,
            line: None,
            offer: Default::default(),
            board: DropBoard::default(),
            trips: Vec::new(),
            edit_raw: None,
            picker: super::picker::native(),
            pick_board: super::picker::PickBoard::default(),
            copy_prompt: None,
            machines: Vec::new(),
            cross_prompt: None,
            cross_pick: Default::default(),
            band_base: None,
            pull_many: Vec::new(),
            dual: false,
            across_wanted: false,
            font: FontState::NotInstalled,
            search: SearchBoard::default(),
            query: String::new(),
            search_whole: false,
            hit_sort: find::FindSort::default(),
            hit_pick: None,
            grep: GrepBoard::default(),
            grep_query: String::new(),
            grep_tally: GrepTally::default(),
            write_board: WriteBoard::default(),
            write_prompt: None,
            mode_probe: super::writeops::ModeProbe::default(),
            inline_done: Default::default(),
            inline_gen: 0,
            inline_busy: false,
            inline_select: None,
            inline_failed_text: None,
            seen_write_rounds: 0,
            pull_want: None,
            pull_raw: None,
            edits: super::editor::EditBoard::default(),
            editing: None,
            asking_discard: false,
            sort: Sort::default(),
            cols: super::rows::Columns::default(),
            back: Vec::new(),
            ahead: Vec::new(),
            addr_edit: None,
            search_content: false,
            want_home: false,
            loading_since: None,
            progress: Default::default(),
            gate: Default::default(),
            receipt: None,
            receipt_copy: None,
            receipt_undo: None,
            want_peek: false,
            props: None,
            status_wanted: false,
            term_notice: Arc::new(Mutex::new(None)),
            plan_notice: Arc::new(Mutex::new(None)),
            reveal: None,
            selection: Selection::default(),
            type_ahead: TypeAhead::default(),
            key_scroll: None,
            menu: None,
            key_notice: None,
            shelf: None,
            focused: true,
            dragging: false,
            writes_started: 0,
            prompt_error: Arc::new(Mutex::new(None)),
            hit_rows: (u64::MAX, u64::MAX, Arc::new(Vec::new())),
            edit_tab: false,
            in_workspace: false,
            view_tab: false,
            viewing: None,
            want_view: None,
            pictures: Default::default(),
            want_edit: None,
            want_dir: None,
            want_reveal: None,
            want_close: false,
            close_after_save: false,
            edit_refused: None,
            edit_row: None,
            find_open: false,
        }
    }

    /// 带着手上那一份的记号重问当前目录（那台答没变 / 变了的那几行 / 整份）。**远端扔给 tokio，不堵住 UI 线程。**
    pub fn reload(&self) {
        // 和那台断着、手上摆着上一屏 ⇒ 不去问（问了只会把那一屏换成一句「断开」）：列表进过期态、照常摆着，
        //   连上之后窗口那一级让每个标签页带着记号重问一遍（`Workspace::settle_link`）。
        if self.gate.offline() && !self.listing.rows.lock().unwrap().is_empty() {
            return;
        }
        let l = self.listing.clone();
        // 带着手上那一份的记号问：那台答「没变 / 变了的那几行 / 整份」（重连那一下、外面动了那一下都走这里）。
        let (mine, held) = l.start_held();
        // 〔有损名全寻址〕发出去的是线上那一形（有损目录 ⇒ `{"b16": …}`）。
        let cwd = self.cwd_path().path();
        // 🔴**先问后端，问不到才退回旧路** ——
        //    用户指令的第 1 步，逐条理由住 `source.rs` 那一段头注。
        // 🔴「问不到才退回旧路」那半句**拿掉了**（`D11`）：只问后端。
        match &self.rt {
            // ── 有运行时 ⇒ 主路（`files-ls` on 这个 origin）───────────
            Some(h) => {
                // 🔴有运行时但没连上通道 ⇒ 出声（`D11`：不退回 SFTP）。
                let Some(line) = self.line.clone() else {
                    store_if_current(&l, mine, Err(NO_LINE.to_string().into()));
                    return;
                };
                let origin = self.source.origin();
                // 带这一刻选的那一档走；换档会重列一趟，号 +1，这一趟落地时就作废了。
                let by = self.sort;
                h.spawn(async move {
                    let r = super::source::list_via_backend_at(
                        &line,
                        &origin,
                        cwd,
                        super::source::LS_LIMIT,
                        by,
                        held,
                    )
                    .await;
                    // 打不开（后端说是哪一种）⇒ 记下那一种，列表顶上画那一条 ＋ 出路。
                    let fail = r.as_ref().err().and_then(|f| {
                        super::source::OpenFail::of_code(&f.code).map(|k| (k, f.said.clone()))
                    });
                    if store_listed_if_current(&l, mine, r) {
                        *l.open_fail.lock().unwrap() = fail;
                    }
                });
                self.reload_plan(h, mine);
            }
            // ── 没有运行时 ⇒ 问不了后端，也走不了 SFTP 那条退路 ⇒ **出声**。
            //
            // 🔴 窗口只有远端一侧（`source.rs` 头注「只有远端一侧」那一节）⇒ 只有「出声」这一种结局。
            // ⚠ **不许**把它改回「静默交一个空列表」：那与「这个目录真的是空的」
            //    在屏幕上分不开，而那正是本仓的头号病形。
            None => {
                store_if_current(
                    &l,
                    mine,
                    Err(copy_text("rsFilewinShell.reload.noRuntime", &[]).into()),
                );
            }
        }
    }

    /// 换到 `path` 并重列。**在飞的那几趟从此全部作废**（见 [`Listing::epoch`]）。
    pub fn navigate_to(&mut self, path: String) {
        self.navigate_to_at(super::source::RemotePath::plain(&path));
    }

    /// 〔有损名全寻址〕当前目录（显示串 ＋ 可能有的字节）。
    pub fn cwd_path(&self) -> super::source::RemotePath {
        super::source::RemotePath::of(&self.cwd, self.cwd_raw.as_deref())
    }

    /// 列表里那一行的整条路径 ＝ 当前目录的字节 ＋ `/` ＋ 名字的字节（有损名用后端送的那一段字节）。
    pub fn row_path(&self, r: &super::source::Listed) -> super::source::RemotePath {
        self.cwd_path().child(&name_bytes(r))
    }

    /// 〔有损名全寻址〕进一个目录（显示串 ＋ 可能有的字节）。与 [`Self::navigate_to`] 同一套收摊。
    pub fn navigate_to_at(&mut self, at: super::source::RemotePath) {
        self.go_at(at, Hist::Push);
    }

    /// 后退一步（回值 ＝ 真的换了目录）。
    pub fn go_back(&mut self) -> bool {
        match self.back.pop() {
            Some(p) => self.go_at(p, Hist::Back),
            None => false,
        }
    }

    /// 前进一步（回值 ＝ 真的换了目录）。
    pub fn go_forward(&mut self) -> bool {
        match self.ahead.pop() {
            Some(p) => self.go_at(p, Hist::Forward),
            None => false,
        }
    }

    /// 后退 / 前进还有没有可走的（工具条那两颗灰不灰）。
    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }
    pub fn can_go_forward(&self) -> bool {
        !self.ahead.is_empty()
    }

    /// 换目录的唯一一处：记历史（新走一步 ⇒ 前进那一摞清掉）、收摊、重列。
    fn go_at(&mut self, at: super::source::RemotePath, hist: Hist) -> bool {
        if at == self.cwd_path() {
            return false;
        }
        let here = self.cwd_path();
        match hist {
            Hist::Push => {
                self.back.push(here);
                self.ahead.clear();
            }
            Hist::Back => self.ahead.push(here),
            Hist::Forward => self.back.push(here),
        }
        self.addr_edit = None;
        self.props = None;
        self.status_wanted = false;
        let path = at.shown.clone();
        // 🔴换了目录，那一行就不在这儿了 ⇒ 高亮清掉。
        //    留着的话，新目录里**恰好同名**的另一个文件会被高亮 ——
        //    而用户会以为那就是他要找的那个。
        self.reveal = None;
        // 🔴选中态按名字记 ⇒ 新目录里**同名**的那几个不是同一样东西。
        //    留着的话按 Delete 删的是新目录里恰好同名的文件。
        self.selection.clear();
        self.type_ahead.clear();
        self.key_scroll = None;
        self.menu = None;
        self.key_notice = None;
        self.cwd = path;
        self.cwd_raw = at.raw;
        self.listing.invalidate();
        self.reload();
        // 换目录时现问一次书签：别的窗口 · 手机刚加的那几条从这里进来（一次问，不是每帧）。
        if let Some(s) = &self.shelf {
            s.refresh(&self.cwd);
        }
        true
    }

    /// 上一级。已经在顶上就什么都不做（[`parent_dir`] 到顶回原值）。
    pub fn navigate_up(&mut self) {
        // 按字节切（合法 UTF-8 时与 `parent_dir` 逐字节同）。
        let up = self.cwd_path().parent();
        self.navigate_to_at(up);
    }

    /// 换一种排序：记下这一档、按它重列一趟（先后由后端排）。回值 = 真的换了（同一档 ⇒ `false`）。
    /// 和那台断着、手上摆着上一屏 ⇒ 不换（列不了，表头不许指着一列、行却是另一种序）。
    pub fn set_sort(&mut self, sort: Sort) -> bool {
        if self.sort == sort
            || (self.gate.offline() && !self.listing.rows.lock().unwrap().is_empty())
        {
            return false;
        }
        self.sort = sort;
        self.reload();
        true
    }

    /// 点了表头那一列（同一列再点 ⇒ 反过来）。
    pub fn click_header(&mut self, by: super::source::SortBy) -> bool {
        self.set_sort(super::source::after_click(self.sort, by))
    }

    /// 隐藏文件显示着吗（缺省显示，淡一级）。
    pub fn shows_hidden(&self) -> bool {
        self.listing.show_hidden.load(Ordering::SeqCst)
    }

    /// 切隐藏文件显不显示：收起来的那几行挪到一边（不是删），选中里它们那几项一并去掉。
    pub fn set_show_hidden(&mut self, on: bool) -> bool {
        if self.shows_hidden() == on {
            return false;
        }
        self.listing.set_show_hidden(on);
        if !on {
            let rows = self.listing.rows.lock().unwrap();
            let keep: std::collections::BTreeSet<String> = rows
                .iter()
                .map(|r| select::pick_key(r).into_owned())
                .collect();
            self.selection.retain(&keep);
        }
        true
    }

    /// 「在此打开终端」那一下说了什么（`None` = 没话说）。判据与界面看同一个值。
    pub fn term_notice(&self) -> Option<String> {
        self.term_notice.lock().unwrap().clone()
    }

    /// 「在计划里看」那一下没成说的话（`None` = 没话说）。
    pub fn plan_notice(&self) -> Option<String> {
        self.plan_notice.lock().unwrap().clone()
    }

    /// 计划反查：问那台后端这个目录归哪一片、每份文件归哪一格（`plan-files`），跟着这一趟列目录（同一个号）落地。
    /// 有损目录不问（反查按显示串比路径，比不准就不比）；问不成 ⇒ 当不在任何一片里（这一列不出，不出声：列目录那一趟会说话）。
    fn reload_plan(&self, h: &tokio::runtime::Handle, mine: u64) {
        let Some(line) = self.line.clone() else {
            return;
        };
        if self.cwd_raw.is_some() {
            return;
        }
        let l = self.listing.clone();
        let origin = self.source.origin();
        let req = cmd_wire::plan::PlanFilesReq {
            dir: self.cwd.clone(),
        };
        h.spawn(async move {
            let got =
                super::source::ask_typed(&line, &origin, &req, std::time::Duration::from_secs(10))
                    .await
                    .ok()
                    .and_then(super::plan::PlanDir::of);
            if l.epoch.load(Ordering::SeqCst) == mine {
                *l.plan.lock().unwrap() = got;
            }
        });
    }

    /// 计划变了：只重问这个目录归哪一格（不重列）。落地按此刻这一趟列目录的号：之后又列了一趟 ⇒ 这一问作废、由那一趟顺带问。
    pub fn refresh_plan(&self) {
        if let Some(h) = &self.rt {
            self.reload_plan(h, self.listing.epoch.load(Ordering::SeqCst));
        }
    }

    /// 单选的那一份文件归的那一格（`(文件名, 那一格)`）；多选 · 文件夹 · 不归 ⇒ `None`。
    pub fn picked_owner(&self) -> Option<(String, super::plan::Owner)> {
        let plan = self.listing.plan.lock().unwrap();
        let plan = plan.as_ref()?;
        let rows = self.listing.rows.lock().unwrap();
        let idx = self.selection.picked_indices(&rows);
        let [i] = idx.as_slice() else { return None };
        let r = rows.get(*i)?;
        if r.opens_as_dir() {
            return None;
        }
        plan.owner(&r.name).map(|o| (r.name.clone(), o.clone()))
    }

    /// 「在计划里看」：经通道交给 monitor（[`filewin_contract::PLAN_OPEN_OP`]，寻址 ＝ 这台 · 参数 `{workspace, slice, id}`）——
    /// 主窗口拉到前面、切到计划页、选中那一格。窗口不认识主窗口；那一问不成 ⇒ 那句原话摆在列表上面。回值 = 真的发出去了。
    pub fn open_in_plan(&mut self, ctx: Option<egui::Context>) -> bool {
        let Some((_, owner)) = self.picked_owner() else {
            return false;
        };
        let Some(dir) = self.listing.plan.lock().unwrap().clone() else {
            return false;
        };
        let (Some(h), Some(line)) = (self.rt.clone(), self.line.clone()) else {
            *self.plan_notice.lock().unwrap() = Some(NO_LINE.to_string());
            return false;
        };
        let args = filewin_contract::plan_open_args(&dir.workspace, &dir.slice, &owner.id);
        let origin = self.source.origin();
        let slot = self.plan_notice.clone();
        *slot.lock().unwrap() = None;
        h.spawn(async move {
            let r = super::source::ask(
                &line,
                &origin,
                filewin_contract::PLAN_OPEN_OP,
                &args,
                TERMINAL_WITHIN,
            )
            .await;
            *slot.lock().unwrap() = r
                .err()
                .map(|why| copy_text("rsFilewinPlan.open.failed", &[("why", &why.to_string())]));
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
        true
    }

    /// 在**当前这个目录**里给用户开一个真终端。回值 = 真的发出去了。
    ///
    /// 窗口只交**意图**：经它那条通道 `call` 一条 monitor 自己接的
    /// [`filewin_contract::TERMINAL_OPEN_OP`]（寻址 ＝ 这台 · 参数 `{cwd}`）。之后三步都在 monitor 那一侧
    /// （`chan/host.rs::terminal_open`）：补这台的机器事实 → 问本机后端 `terminal-ssh` 渲那一行（`cd` 那一串也在那里拼，
    /// `src/backend/dial/terminal.rs::command_for_cwd`）→ `launch::open_terminal_window` 开窗 —— 与主界面开终端同一条路。
    /// 窗口不拼命令、不认识 monitor 的配置、不起进程；那一问不成 ⇒ 对端那句原话画在窗口上（`D11`，不退回自己拼）。
    ///
    /// 那一问不成 ⇒ 那句原话摆在工具栏下面（[`Self::term_notice`]），**不假装成功**。
    /// 窗口里没有剪贴板兜底（旧面板那颗按钮同样没有）。
    pub fn open_terminal_here(&mut self, ctx: Option<egui::Context>) -> bool {
        let Some(h) = self.rt.clone() else {
            *self.term_notice.lock().unwrap() =
                Some(copy_text("rsFilewinShell.terminal.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.term_notice.lock().unwrap() = Some(NO_LINE.to_string());
            return false;
        };
        // 有损目录：当前目录按字节交（线上形 `{"b16": …}`），`cd` 那一串在后端走唯一的 quote 的字节形。
        let args = filewin_contract::terminal_open_args(self.cwd_path().wire());
        let origin = self.source.origin();
        let slot = self.term_notice.clone();
        *slot.lock().unwrap() = None;
        h.spawn(async move {
            let opened = super::source::ask(
                &line,
                &origin,
                filewin_contract::TERMINAL_OPEN_OP,
                &args,
                TERMINAL_WITHIN,
            )
            .await;
            let said = opened.err().map(|why| {
                copy_text(
                    "rsFilewinShell.terminal.failed",
                    &[("why", &why.to_string())],
                )
            });
            *slot.lock().unwrap() = said;
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
        true
    }

    /// 点开第 `i` 行。**目录进去，文件不动。**
    ///
    /// 回值 = 真的换了目录。对文件是明确的「什么都不做」：文件那一侧各有各的口
    /// （编辑 · 下载在行上与菜单里；预览是右侧那块面板，跟着选中走，不靠双击）。
    pub fn activate(&mut self, i: usize) -> bool {
        let target = {
            let rows = self.listing.rows.lock().unwrap();
            match rows.get(i) {
                // 有损名目录：没带字节就进不去（寻址不到）；带了 ⇒ 按字节进。
                Some(r) if r.opens_as_dir() && (!r.lossy_name || r.raw_name.is_some()) => {
                    self.row_path(r)
                }
                _ => return false,
            }
        };
        let before = self.cwd_path();
        self.navigate_to_at(target);
        self.cwd_path() != before
    }

    /// 摆一次 reveal：「进这个目录，并且高亮 `name` 那一行」。
    ///
    /// ⚠ 它**不判那一行在不在** —— 那要等目录列回来（异步）。
    /// 「列回来了却没有那一行」这一形由 [`Self::take_reveal_offset`] 说出来。
    pub fn set_reveal(&mut self, name: &str) {
        self.reveal = Some(Reveal {
            name: name.to_string(),
            scrolled: false,
        });
    }

    /// 这一帧要不要滚、滚到哪儿。**只在第一次调用时给值**（之后回 `None`）。
    ///
    /// 🔴 回值里那个 `Err` 是**「那一行不在这一摞里」** —— 文件刚被删了 / 改名了。
    /// 调用方要把它说出来：静默什么都不做与「跳过去了」在屏幕上同形。
    /// ⚠ **它自己拿那把锁，而且只握到扫完为止** —— 第一版我让它吃一个 `&[Row]`，
    /// 于是调用方得先 `rows.lock().unwrap().clone()` 才借得出 `&mut self`（E0502）
    /// ⇒ **那会每帧克隆整摞行**。编得过，但在 64 万条量纲上正是
    /// 那条纪律的反面。⇒ 锁与扫描都收进来，一次克隆都没有。
    pub fn take_reveal_offset(&mut self, pitch: f32) -> Option<Result<f32, String>> {
        // ① 先取出要找的名字（只读借用，随即放掉）。
        let want = match self.reveal.as_ref() {
            Some(r) if !r.scrolled => r.name.clone(),
            _ => return None,
        };
        // ② 握锁扫一遍（**只扫一遍，只在这一次 reveal 里**，不是每帧）。
        let (mut found, hidden) = {
            let rows = self.listing.rows.lock().unwrap();
            let hidden = self.listing.hidden.lock().unwrap();
            // ⚠ 列表还没到货 ⇒ **这一帧不算「滚过了」**，下一帧再来
            //   （不然会在空列表上判成「那一行不在」）。
            if rows.is_empty() && hidden.is_empty() {
                return None;
            }
            let found = super::rows::reveal_index(&rows, &want);
            let hidden = found.is_none() && hidden.iter().any(|(_, r)| r.name == want);
            (found, hidden)
        };
        // 它是一个隐藏文件、而隐藏文件关着 ⇒ 打开显示隐藏文件、照样跳过去，说一句（不说「可能被删了」）。
        if hidden {
            self.set_show_hidden(true);
            found = super::rows::reveal_index(&self.listing.rows.lock().unwrap(), &want);
            self.key_notice = Some(copy_text(
                "rsFilewinShell.reveal.hiddenShown",
                &[("want", &want)],
            ));
        }
        // ③ 锁放掉了，这里才改自己。
        if let Some(r) = self.reveal.as_mut() {
            r.scrolled = true;
        }
        match found {
            // 🔴 像素那一步在这儿，用的是那**唯一住址**的步距（`rows::row_pitch`）——
            //    第一版这里乘的是行高，漏了行间距，滚偏 14%。
            Some(i) => Some(Ok(i as f32 * pitch)),
            None => {
                // 找不到就把高亮也撤掉 —— 留着等于在屏幕上标一个不存在的东西。
                self.reveal = None;
                Some(Err(copy_text(
                    "rsFilewinShell.reveal.gone",
                    &[("want", &want.to_string())],
                )))
            }
        }
    }

    /// 把这一帧列表带出来的那个点击**接上**状态机。
    ///
    /// 🔴 **抽成一个函数是为了让这条胶水本身可判。** 它只有一行，而那一行正是
    /// 「列表说有人点了第 i 行」与「窗口换目录」之间的唯一连接 ——
    /// 写在 `ui()` 里的话，`activate` 有判据、`show_file_rows_with_tail` 有判据，
    /// 中间这一跳**谁都没在看**。
    ///
    /// 回值 = 真的换了目录。
    pub fn apply_click(&mut self) -> bool {
        let Some(i) = self.tally.clicked else {
            return false;
        };
        let dir = self
            .listing
            .rows
            .lock()
            .unwrap()
            .get(i)
            .map(Listed::opens_as_dir);
        match dir {
            Some(true) => self.activate(i),
            // 文件：同回车（图片 ⇒ 查看页 · 其余 ⇒ 编辑页）；不换目录。
            Some(false) => {
                self.open_file(i, None);
                false
            }
            None => false,
        }
    }

    /// 打开第 `i` 行那份文件（回车 · 双击）：窗口解得开的图 ⇒ 查看页；其余 ⇒ 编辑页（做不了的那一页上说为什么）。
    pub fn open_file(&mut self, i: usize, ctx: Option<egui::Context>) -> bool {
        let image = self
            .listing
            .rows
            .lock()
            .unwrap()
            .get(i)
            .is_some_and(super::imgview::viewable);
        if image {
            self.begin_view(i)
        } else {
            self.begin_edit(i, ctx)
        }
    }
}

impl FileWindow {
    /// 选中态（判据与界面看同一个值）。
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// 选中的**恰好那一项**叫什么；`Err(n)` ＝ 选中了 `n` 项（`n ≠ 1`）。
    /// O(1)：只问选中态，不扫列表（预览每帧都问它）。
    pub fn picked_name(&self) -> Result<String, usize> {
        match self.selection.len() {
            1 => self.selection.names().pop().ok_or(0),
            n => Err(n),
        }
    }

    /// 选中的那一摞（按列表的显示序）。选中态按 `pick_key` 记，这里按同一把键取回行。
    pub fn picked_rows(&self) -> Vec<Listed> {
        let rows = self.listing.rows.lock().unwrap();
        self.selection
            .picked_indices(&rows)
            .into_iter()
            .filter_map(|i| rows.get(i).cloned())
            .collect()
    }

    /// 按名字找那一行（O(n)：只在「选中的那一项换了」时调，不是每帧）。
    pub fn row_named(&self, name: &str) -> Option<Listed> {
        self.listing
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.name == name)
            .cloned()
    }

    /// 这个标签页**关得了吗**（`None` ＝ 关得了）—— 关标签页 / 收右栏之前问它。
    ///
    /// 有一问摆着、有一份文本开着、在读写文本 ⇒ 说是哪一件。
    /// 在传 / 在复制 / 在删的那几趟**不算**：它们是「进度」表里的行，不随标签页走，关了照跑（目录标签页直接关）。
    pub fn busy_reason(&self) -> Option<String> {
        if self.editing.is_some() {
            return Some(copy_text("rsFilewinShell.busy.editorOpen", &[]).into());
        }
        self.held_reason()
    }

    /// 这个标签页手上攥着、关了就丢的那几件（一问摆着 · 在读写文本 · 一摞改名 / 新建还没回话）。
    fn held_reason(&self) -> Option<String> {
        if self.asking_up() {
            return Some(copy_text("rsFilewinShell.busy.pendingAsk", &[]).into());
        }
        if let Some(p) = self.edits.opening().or_else(|| self.edits.saving()) {
            return Some(copy_text(
                "rsFilewinShell.busy.io",
                &[("path", &p.to_string())],
            ));
        }
        if self.writes_started > self.write_board.rounds() {
            return Some(copy_text("rsFilewinShell.busy.writes", &[]).into());
        }
        None
    }

    /// 要人填字的那几个框（改名 · 新建 · 改权限 · 复制为 · 复制到另一台）有一个摆着吗。
    fn prompt_up(&self) -> bool {
        self.write_prompt.is_some() || self.copy_prompt.is_some() || self.cross_prompt.is_some()
    }

    /// 做不成的那一下说的话落在哪：有框摆着 ⇒ 框里（[`Self::prompt_failure`]）；否则 ⇒ 列表上方那一行。
    pub(super) fn say_slot(&self) -> std::sync::MutexGuard<'_, Option<super::source::Failed>> {
        if self.prompt_up() {
            self.prompt_error.lock().unwrap()
        } else {
            self.listing.error.lock().unwrap()
        }
    }

    /// 摆着的框里那一句「为什么没做成」，连复制详情一起（框里那一行与它的［复制详情］用）。
    pub fn prompt_failure(&self) -> Option<super::source::Failed> {
        self.prompt_error.lock().unwrap().clone()
    }

    /// 框里那一句（红字，紧跟在输入框下面）。
    fn prompt_error_ui(&self, ui: &mut egui::Ui) {
        if let Some(f) = self.prompt_failure() {
            let id = ui.id().with("prompt-fail-detail");
            super::kit::failure_line(ui, &f.said, &f.detail, id);
        }
    }

    /// 有一个模态框摆着吗（上传那一问 · 复制那两问 · 写操作那两问 · 新建空文件那个框 · 往外拖那两问 · 编辑面）。
    ///
    /// 🔴 **一处**：拖入那一口（[`Self::take_drops`]）与键盘那一口（[`Self::apply_keys`]）
    /// 问的是同一个函数 —— 分成两份的症状是「编辑面开着，按 Delete 删掉了列表里的文件」。
    fn modal_up(&self) -> bool {
        self.asking_up() || self.editing.is_some() || self.edit_tab || self.showing_picture()
    }

    /// 编辑面以外的那几个框有没有一个摆着。
    fn asking_up(&self) -> bool {
        // 这个标签页起的那几趟开头那一问（同名冲突 · 解压撞名）。
        self.my_trips().iter().any(|j| j.trip.is_asking())
            // 复制到另一台：填机器名那一问 · 盖不盖那一问。
            || self.cross_prompt.is_some()
            || self.copy_prompt.is_some()
            || self.write_board.is_asking()
            || self.write_prompt.is_some()

            || self.props.is_some()
    }
}

/// 工具条上那个搜索框的 egui id（Ctrl+F 把焦点给它）。
pub const SEARCH_BOX_ID: &str = "filewin-search-box";

/// 搜索框平时多宽（窗口窄时收窄，见 `chrome.rs` 工具条）。
pub const SEARCH_BOX_WIDTH: f32 = 260.0;

/// 换目录时历史怎么记。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hist {
    /// 新走一步（地址栏 · 面包屑 · 双击 · 上一级 · 书签）。
    Push,
    Back,
    Forward,
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/shell_tests.rs"]
pub(crate) mod tests;
// 键盘 · 多选 · 右键菜单接到窗口上的那一摞（每一条都真跑 `frame_body`）。
#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/shell_keys_tests.rs"]
mod keys_tests;

/// 一行里的一句话：放不下就截成「…」、悬停看全句；`reserve` ＝ 它后面那几颗控件要留的宽。
pub(crate) fn fit_label(ui: &mut egui::Ui, text: String, reserve: f32) {
    let room = (ui.available_width() - reserve).max(0.0);
    ui.scope(|ui| {
        ui.set_max_width(room);
        ui.add(egui::Label::new(text.as_str()).truncate())
            .on_hover_text(text);
    });
}

/// 〔有损名全寻址〕一行的名字的真字节：有损 ⇒ 后端送的那一段（`raw_name`），否则 ⇒ 名字的 UTF-8。
pub fn name_bytes(r: &super::source::Listed) -> Vec<u8> {
    r.raw_name
        .clone()
        .unwrap_or_else(|| r.name.as_bytes().to_vec())
}
