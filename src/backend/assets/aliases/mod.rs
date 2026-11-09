//! 别名：**名字 ＋ 一组 ccm 参数**。一类，没有「账号别名」这一种。
//!
//! # 住**那台机器的后端**
//!
//! 整族（本文件 · [`dialect`] · [`block`] · [`fence`] · [`form`]）在那台后端里：读、算、经它自己的文件管理面写，
//! 界面经通道直问（`aliases-*` 八条帧命令，下面 `answer_*`）。「本机」与「远端」对后端没有区别（本机＝不走 ssh 的远端），
//! 没有 `origin` 这一维：方言那道闸问这台自己（[`dialect_here`]）、`PATH` 撞名查这台自己的 `PATH`、围栏的符号链接那一步量这台自己的盘。
//!
//! 〔用 2026-09-17〕逐字：「**不要有 account alias 这种东西。alias 应该独立吗？应该就是 ccm
//! 参数附加器。即生成别名，都是调用 ccm，ccm 本身就可以指定账号。**」
//!
//! 别名清单归**用户**（不是账号表的投影）；shell 文本由**本模块**渲染（前端递的是结构，不是代码）。
//!
//! # 🔴 落点：**我们自己那份文件**，不是用户的 `~/.bashrc`（这一条没变）
//!
//! 往 rc 里追加的那条路有三条病：
//! 1. **重复追加**：没有任何东西负责去重；
//! 2. **删不掉**：删了一条，那一行还在；
//! 3. **弄坏的代价是「shell 起不来」** —— 而那正是用户用来救火的东西。
//!
//! ⇒ 落点是 [`alias_file_in`]（`~/.cc-monitor/aliases.sh`，**monitor 自己的目录**），
//! **整份重写**：幂等、删一条当场消失、删掉整份文件也只是少几个命令，shell 照常起得来。
//! 文件名从 `account-aliases.sh` 改成 `aliases.sh`，
//! **不留兼容** —— 不留转发件、不读旧名；旧文件留在盘上，本模块不读、不写、不删。
//!
//! # 🔴 接上这份文件的那一行 source **只住别名块里**（「source 那一行只许一处装」）
//!
//! 两种方言的别名块都自带那一行（POSIX：`src/shared/ccm-aliases.sh` 最后一行；PowerShell：`src/shared/cc.ps1.tpl`
//! 结尾那一行，两边对称），文件不在就什么都不做 ⇒ 装了别名块的人什么都不用做。
//! 本模块**不往 rc 里另写那一行**：另写一处（另一对围栏）就是同一件事的第二个写处。
//! 选了 rc 那一步**只是检查**：看它接没接上，
//! 没接上就说「装上别名块就接上了」并给出那一行、由人自己决定（提供检测，不代装）。
//! 盘上已有的旧围栏那一块不读、不删、不写清理代码（两处 source 同一份文件是幂等的，只多点一次）。
//! 别名文件的写**不在本进程落盘**：经本机后端的文件管理那一面（`user_files::edit` → `Door::peek` / `Door::put`），
//! 备份 · 原子替换 · 回读 · 回滚那一份规则住后端。

//!
//! # 本模块是**通用层**：只持有结构与规则，不持有任何一种 shell 的文本
//!
//! 「这个 shell 怎么写 / 文件落在哪 / 名字怎么认」全在 `shell_dialect.rs`（POSIX 与 PowerShell
//! 各一份实现）；这里留的是**判定的规则**（组合规则 · 能力闸 · 重名 · 有一条不合格整批不写）。
//! 哪种 shell 由这台后端自己答（[`here_shell`]；`aliases-read` 回包里带它，别名块那几条照目标文件的扩展名）：同一份清单，POSIX 落 `~/.cc-monitor/aliases.sh`、PowerShell 落
//! `~/.cc-monitor/aliases.ps1`，各自由那个 shell 的别名块里那一行 source 接上。

use copy_core::copy_text;
use serde_json::{json, Value};
use std::path::Path;

pub mod accounts;
pub mod block;
pub(crate) mod fence;
pub(crate) mod form;
pub mod links;
pub mod page;
pub mod profile;

use crate::ccm_line::argv::{flag, hosted_word, Parsed};
use crate::common::door::{self, Door};
use crate::common::said::Said;
// 方言住后端 OS 适配层。
use crate::platform::child::Deadline;
use crate::platform::shell::dialect::{self, Listed, RestTo, Shell};

/// `aliases-read` 整条命令总期限的上限（每一代 PowerShell 问一次执行策略，最多两发）。
pub const ALIASES_READ_CAP: Deadline = Deadline::secs(28);
/// `powershell-policy-set` 整条命令总期限的上限（设一发 ＋ 再问一发）。
pub const POLICY_SET_CAP: Deadline = Deadline::secs(28);

/// 交给方言的那条调用形状：`ccm` 那个词与 `--` 分界住 `control::ccm`、落点住 `relay_route_core`，适配层不往上够 ⇒ 由这里交下去。
pub(crate) const CALL: dialect::Call<'static> = dialect::Call {
    word: crate::ccm_line::SUBCOMMAND_WORD,
    end: flag::END,
    landing: relay_route_core::BACKEND_LANDING_REL,
};

/// 托管入口（被叫成一段写了 `--hosted` 的配置，如 `cch`）的补全交给方言的那几个词：住 `ccm_line::argv`，适配层不往上够 ⇒ 由这里交下去。
pub(crate) const HOSTED_WORDS: dialect::HostedWords<'static> = dialect::HostedWords {
    first: &[hosted_word::LS, hosted_word::ATTACH],
    takes_name: &[hosted_word::ATTACH, hosted_word::ATTACH_SHORT],
    list: &[flag::END, flag::HOSTS],
};

/// 合并后的清单里、叫起来是托管入口的那几条（合下来带 `--hosted`：内置的 `cch` · `<号>cch` 与用户基于它写的），照清单的先后。
pub(crate) fn hosted_names(book: &profile::Book) -> Vec<String> {
    book.profiles
        .iter()
        .filter(|p| is_hosted(book, &p.name))
        .map(|p| p.name.clone())
        .collect()
}

/// 叫这一段起来是不是托管入口（合下来带 `--hosted`）。合不下来的不算。
pub fn is_hosted(book: &profile::Book, name: &str) -> bool {
    matches!(
        profile::resolve(book, name, &[]).map(|r| r.parsed),
        Ok(Parsed::Opts(o)) if o.hosted
    )
}

/// 生成文件自己的围栏（整份重写，所以它只是给人看的边界；两种方言共用，理由同上）。
/// v1 → v2：`71` 逐字「不要有 account alias 这种东西」—— 文件里只有**一类**别名。
const FILE_BEGIN: &str = "# === cc-monitor aliases BEGIN v2 ===";
const FILE_END: &str = "# === cc-monitor aliases END ===";

/// 一份候选启动文件（rc / `$PROFILE`）的状态。
///
/// 「接没接上别名文件」与别名块（`cc` / `cct` · `__ccm_bind`）装进的是**同一批**文件
/// ⇒ 一份候选、一次扫描，两件事都在这一格上（`block`）。
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "AliasStartupFile",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupFile {
    /// 绝对路径。
    pub path: String,
    /// `path` 的显示形（这台按自己的家目录写好的 `~/…`，界面照抄）。
    pub shown: String,
    /// 这份文件今天已经把生成文件 `source` 进去了吗（装过 ccm 别名块的人这一格就是 true）。
    pub sourced: bool,
    /// 这份文件在不在盘上。POSIX 只列在的（恒 true）；PowerShell 的 `$PROFILE` 常常要装的时候才建。
    pub exists: bool,
    /// 这份文件里别名块的现状（不在盘上 ⇒ 全空）。
    pub block: block::BlockState,
    /// 在盘上、可那台后端读不了它（非 UTF-8 · 太大 · 解到 home 外 · I/O）—— 后端原话；`None` = 读得了或不在。
    /// 读不了不等于「没有别名块」，两形分开说。
    pub unreadable: Option<String>,
    /// 加载它的那一代 PowerShell 的执行策略（现问；块装在这里它会不会跑）。POSIX 与人另指的那一份 ⇒ `None`。
    pub policy: Option<crate::platform::shell::powershell::ExecPolicy>,
    /// 把别名块装进这一份会写几行（与装进去的那一块同源：[`block::render_block`]，方言按这份的扩展名；渲染不出 ⇒ 0）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub block_lines: usize,
}

/// 生成文件的绝对路径。`home` 由调用方给 —— 测试拿临时目录当 home，**绝不碰真实家目录**。
///
/// **逐段**拼：`our_alias_file_rel` 是 `/` 分隔的（交给后端的 `rel` 就是它，那一侧不动），
/// 而这里拼的是**给人看、也写进 `$PROFILE` 那一行**的那台机器上的绝对路径。整串一次拼在 Windows 上
/// 会得到 `C:\Users\user\.cc-monitor/aliases.ps1`（真机读数）—— 两种分隔符混着。
/// `home` 是**那台机器**的后端答的字符串，拼法跟它自己的分隔符走（`user_files::join_under`）：
/// 用 `Path::join` 的话分隔符是本进程这台的 —— Windows 上拼 `/home/user` 会得到 `/home/user\.cc-monitor\aliases.sh`。
pub(crate) fn alias_file_in(home: &str, shell: Shell) -> String {
    door::join_under(home, shell.dialect().our_alias_file_rel())
}

/// 整份生成文件的内容（**编码前**：BOM 那一层在落盘那一跳按方言加）。
/// **没有时间戳** —— 有了就永远比不出「内容没变」，每次都要写一遍。
/// `hosted`：要挂托管入口补全的那几条（[`hosted_names`]），补全那一段接在别名之后（方言出文本，[`dialect::ShellDialect::render_hosted_completion`]）。
pub(crate) fn render_file(shell: Shell, lines: &[String], hosted: &[String]) -> String {
    let mut out = String::new();
    out.push_str(FILE_BEGIN);
    out.push('\n');
    out.push_str(&shell.dialect().file_header());
    if lines.is_empty() {
        out.push_str(&copy_text("rsAccountAliases.file.empty", &[]));
    }
    for l in lines {
        out.push_str(l);
        out.push('\n');
    }
    if !hosted.is_empty() {
        out.push_str(
            &shell
                .dialect()
                .render_hosted_completion(CALL, hosted, HOSTED_WORDS),
        );
        out.push('\n');
    }
    out.push_str(FILE_END);
    out.push('\n');
    out
}

// ═══════════════════════════════════════════════════════════════════════════
// **别名只有一类 —— 名字 ＋ 一组 ccm 参数**
// ═══════════════════════════════════════════════════════════════════════════
//
// 「不要有 account alias 这种东西……就是 ccm 参数附加器」。
// 账号（`--account`）只是参数里的一个维度。清单由**用户**拥有，不跟着账号表自动增删。
//
// 命令面是两跳：
//   ① [`render`] —— **纯**：清单 → 代码（＋ 每条的问题 ＋ 撞名提示）。预览、复制都只调这一跳；
//   ② [`install_in`] —— **唯一的副作用**：同一份渲染落进我们自己那份别名文件（经那台机器的后端），
//      可选地往用户选的启动文件里装一行 `source`。它收的是**清单**不是代码 —— 写进 shell 的文本
//      只由 `shell_dialect` 产出（审计 S-1：绝不让前端注入可执行的 shell），而「写的就是预览的那一份」
//      由两跳调同一个 [`render`] 保证。

/// 读回口的产物：启动文件候选（接入那一格）。清单在配置文件那一页（`page.rs::read_with`）。
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AliasListing {
    /// 候选是哪种 shell 的（`aliases-read` 里就是这台后端说的那一种，[`here_shell`]）：界面照它画平台那几格。
    pub shell: Shell,
    /// 这台机器上这种 shell 的启动文件候选（「那一行 source 加进哪份」·「别名块装进哪份」，同一批）。
    pub rc_candidates: Vec<StartupFile>,
    /// 人另指的那一份（`read_via` 的 `extra_rc`）过了围栏之后的绝对路径 —— 界面拿它在候选里认出「刚指的是哪一份」。
    pub other_rc: Option<String>,
}

/// 别名文件写过之后给人的那一句：已开着的终端不会自己重读它（启动文件只在启动时读一次）。
pub fn reload_hint(path: &str) -> String {
    copy_text("rsAccountAliases.install.reload", &[("path", path)])
}

/// 一条别名在别名文件里交给 ccm 的那一个词：`@<名>`。
fn own_word(name: &str) -> String {
    format!("{}{name}", crate::ccm_line::PROFILE_SIGIL)
}

/// 别名文件的版本（盘上原样的字节：长度 ＋ FNV-1a 64）。不在 ⇒ `None`。
/// 只用来认「读回之后盘上那份变没变」（写那一跳另有 CAS），不是摘要形 —— 不借文件管理面那一份 SHA-256（那是它的内部）。
fn rev_of(text: Option<&str>) -> Option<String> {
    text.map(|t| format!("{}-{:016x}", t.len(), cmd_wire::fnv::fnv1a64(t.as_bytes())))
}

/// 这台的账号表（具名号）。没有账号库 / 读不了 ⇒ 空（账号页自己会说读不了）。
fn account_table(home: &str) -> Vec<String> {
    crate::accounts::manage::mcp_share_exec::manifest_in(home)
        .ok()
        .flatten()
        .map(|m| m.managed().map(|a| a.name.clone()).collect())
        .unwrap_or_default()
}

/// 这台后端自己说的那种 shell（Windows ⇒ PowerShell，其余 ⇒ POSIX）。
pub(crate) fn here_shell() -> Shell {
    if cfg!(windows) {
        Shell::PowerShell
    } else {
        Shell::Posix
    }
}

/// **合并后的清单**：用户那份（`text`；不在 ⇒ 一段都没有）＋ 内置那几段（照这台的账号表现算，`accounts::with_builtins`）。
/// 读清单的每一处（[`load`] · ccm 那一趟 `profile::read_at`）都经这里。
pub(crate) fn book_in(home: &str, text: Option<&str>) -> profile::Book {
    accounts::with_builtins(
        text.map(profile::parse_book).unwrap_or_default(),
        &account_table(home),
        builtin_tmux(),
        &same_name_here,
    )
}

/// 两个名字在这台说的那种 shell 里是不是同一个（PowerShell 不分大小写）：认「同名盖过内置」的每一处都问它。
pub fn same_name_here(a: &str, b: &str) -> bool {
    here_shell().dialect().same_name(a, b)
}

/// 内置那几段带不带 tmux 那一形（这台说的那种 shell 的能力）：合清单与建号删号回执说名字都问这一处。
pub fn builtin_tmux() -> bool {
    Caps::of(here_shell()).tmux
}

/// 这台机器的**能力**—— 不是方言（`has_tmux()` 不进那一族）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    /// 有没有 tmux（`--ccm-tmux` 那一族能不能用）。
    pub tmux: bool,
}

impl Caps {
    /// 一种 shell 的目标机器上有什么。今天只有一个事实：**PowerShell 目标 ⇔ Windows ⇔ 没有 tmux**
    /// （Windows 后台机制未定）。POSIX 目标按「有」算：没装 tmux 的 Linux 由 ccm 在运行时报 `no_tmux`（今天就是这样）。
    pub fn of(shell: Shell) -> Caps {
        Caps {
            tmux: shell == Shell::Posix,
        }
    }
}

/// **这台后端说不说这种方言**（「显式拒绝，不许静默推一份跑不起来的东西」）。
///
/// 事实只有一个：**PowerShell ⇔ Windows**（与 [`Caps::of`] 同一条）；POSIX 在三种 OS 上都有人说（Windows 上是 Git Bash）。
/// ⇒ 这台后端不在 Windows 上 ⇒ PowerShell 形拒。后端不知道自己是不是「远端」，按这台自己的平台判
/// （本机＝不走 ssh 的远端）⇒ **本机 Linux 选 `.ps1` 那一形也拒**。
/// 平台那一格住 `platform::shell::speaks_powershell`。
pub(crate) fn dialect_here(shell: Shell) -> Result<(), String> {
    if shell == Shell::Posix || crate::platform::shell::speaks_powershell() {
        return Ok(());
    }
    Err(copy_text(
        "rsAccountAliases.dialect.notHere",
        &[("machine", &crate::assets::asset_catalog::machine_label())],
    ))
}

/// `--cwd` 的形式判定，与 ccm 运行时同一条（`plan.rs::free_text_gate`：绝对 · 无 `..` 段 · 无 NUL/CR/LF）。
/// 两形都调共享那一份：POSIX 目标照 POSIX 认绝对，PowerShell 目标是 Windows 路径（盘符根或 UNC，分隔符两种都认）。
fn cwd_form_ok(v: &str, shell: Shell) -> bool {
    match shell {
        Shell::Posix => shell_quote_core::posix_free_path_ok(v),
        Shell::PowerShell => shell_quote_core::windows_free_path_ok(v),
    }
}

/// `--cwd` 与 `--cwd-if` 的一个目录：`~` / `~/…` 打头（ccm 起会话时换成那台的家目录；剩下那段不许有 `..`），
/// 或与 ccm 运行时同一条形式判定的绝对路径（[`cwd_form_ok`]）。别名要钉住一个目录 ⇒ 相对的不收。
fn pinned_dir_ok(v: &str, shell: Shell) -> bool {
    let rest = if v == "~" {
        Some("")
    } else {
        v.strip_prefix("~/").or_else(|| {
            (shell == Shell::PowerShell)
                .then(|| v.strip_prefix("~\\"))
                .flatten()
        })
    };
    match rest {
        Some(r) => {
            !r.split(['/', '\\']).any(|seg| seg == "..") && shell_quote_core::free_text_ok(v)
        }
        None => cwd_form_ok(v, shell),
    }
}

/// 配置文件里的目录：两种 shell 任一种认它钉住了就收（同一份配置两种 shell 共用；不对的那一种由 ccm 运行时说）。
pub(crate) fn pinned_anywhere(v: &str) -> bool {
    pinned_dir_ok(v, Shell::Posix) || pinned_dir_ok(v, Shell::PowerShell)
}

/// 一条别名在这种 shell 里的写法：只有名字（`名字() { ccm @名字 "$@"; }`），规则住配置文件。
pub(crate) fn render_line(name: &str, shell: Shell) -> String {
    shell
        .dialect()
        .render_alias(CALL, name, &[own_word(name)], RestTo::Agent)
}

/// 「在这台终端里起它」那一行：ccm 的绝对路径（`ccm`，家目录下的落点）＋ `@名`，按这台的 shell 引好 ——
/// 不靠 `PATH` 里有那个目录（别名块没装 · 终端开在装之前都照样起得来）。名字已过 [`profile::name_ok`]，不用引。
pub(crate) fn start_line(name: &str, shell: Shell, ccm: &str) -> String {
    shell.dialect().render_start(ccm, &own_word(name))
}

/// 这一条要不要写进别名文件（写成函数）：PowerShell 一律是函数；POSIX 只有与系统命令 / 自带片段撞名的那几条
/// （比如 `cc`：把链接放进 `PATH` 会让编译调到它），其余做成 `~/.cc-monitor/bin/<名>` 的链接（[`links`]）。
pub fn wants_function(name: &str, shell: Shell) -> bool {
    shell == Shell::PowerShell || collision_note(name, shell).is_some()
}

// ═══════════════════════════════════════════════════════════════════════════
// 配置文件这一侧：读 · 写 · 照它生成别名文件与链接
// ═══════════════════════════════════════════════════════════════════════════

/// 这台机器上配置文件此刻的样子（读的那一刻）。
pub struct Store {
    pub home: String,
    /// 盘上原文（不在 ⇒ `None`）。
    pub text: Option<String>,
    pub book: profile::Book,
}

/// 读配置文件，合上内置那几段（[`book_in`]；不在 ⇒ 只有内置的）。
pub fn load(d: &dyn Door) -> Result<Store, String> {
    let home = door::home(d)?;
    let path = profile::path_in(&home);
    let text = read_profiles(d, &home, &path)?;
    let book = book_in(&home, text.as_deref());
    Ok(Store { home, text, book })
}

fn read_profiles(d: &dyn Door, home: &str, path: &str) -> Result<Option<String>, String> {
    match look(d, home, path)? {
        Look::Text(t) => Ok(Some(t)),
        Look::Absent => Ok(None),
        // 那一句是 Door::peek 的成品句（带路径 ＋ 原因词），原样交出去，不再套一层「读取失败」。
        Look::Unreadable(said) => Err(said),
    }
}

/// 后端起来时那一次（经本进程自己的文件管理面，门由 `stream::inbound::align_aliases` 递）：照合并后的清单把别名文件 · 链接
/// 对齐一次（[`regenerate_via`]，内容一样一个字节不写）—— 写法变了的那一份不等人再改一次清单才换；配置文件不在也对齐
/// （老装的机器升级后起来就有内置那几条）。没做成只记日志，不挡起来。
pub fn align_here(d: &dyn Door) {
    match regenerate_via(d) {
        Ok(_) => {}
        Err(RegenErr::Unparsed { .. }) => {}
        Err(RegenErr::Failed(e)) => tracing::warn!("{e}"),
    }
}

/// 照合并后的清单重生成别名文件与链接没成。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegenErr {
    /// 配置文件整份写坏了（TOML 解不开）⇒ 读不出清单，不照「只有内置的」去删用户那几条的别名与链接（等人先改好）。
    Unparsed {
        path: String,
        n: usize,
    },
    Failed(String),
}

/// 重生成的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Regenerated {
    /// 配置文件的路径（这一趟不写它）。
    pub path: String,
    /// 别名文件或链接动了没有。
    pub changed: bool,
    /// 动了的别名文件（已开的终端要重读它）。
    pub rewrote: Vec<String>,
    /// 合并后的清单（建号 / 删号那一刻据它说哪几条是内置的、哪几条被用户那段盖着）。
    pub book: profile::Book,
}

/// **照合并后的清单重生成别名文件与链接**（建号 / 删号那一刻 · 后端起来那一次）：用户的配置文件一个字节不写。
pub fn regenerate_via(d: &dyn Door) -> Result<Regenerated, RegenErr> {
    let store = load(d).map_err(RegenErr::Failed)?;
    let path = profile::path_in(&store.home);
    if store.book.file_problem().is_some() {
        return Err(RegenErr::Unparsed {
            path,
            n: store.book.problems.len(),
        });
    }
    let out = sync_outputs(d, &store.home, &store.book, None);
    if let Some(p) = out.problems.first() {
        return Err(RegenErr::Failed(p.clone()));
    }
    Ok(Regenerated {
        path,
        changed: out.links_changed || !out.rewrote.is_empty(),
        rewrote: out.rewrote,
        book: store.book,
    })
}

/// 这台说哪几种 shell（POSIX 恒有；Windows 上另有 PowerShell）⇒ 生成哪几份别名文件。
fn machine_shells() -> Vec<Shell> {
    let mut v = vec![Shell::Posix];
    if crate::platform::shell::speaks_powershell() {
        v.push(Shell::PowerShell);
    }
    v
}

/// 写配置文件没成：盘上那份在读之后被别处改过（界面重读再让人存），或别的原因（原话）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WriteErr {
    Stale(String),
    Refused(String),
}

/// 把 `text` 写成配置文件：盘上此刻要恰是 `was`（读的那一刻的原文；`None` ＝ 不在），否则一个字节不写（[`WriteErr::Stale`]）。
/// 经这台的文件管理面：原子替换 ＋ 内容比对（同一时刻两处写，后到的那个被拒，不是互相盖）。内容一样就不写。回：真写了没有。
fn write_profiles(
    d: &dyn Door,
    home: &str,
    was: Option<&str>,
    text: &str,
) -> Result<bool, WriteErr> {
    let path = profile::path_in(home);
    let mut moved = false;
    let done = write_ours(d, home, relay_route_core::PROFILES_REL, &mut |now| {
        if now != was {
            moved = true;
            return Err(copy_text(
                "rsAccountAliases.install.changedElsewhere",
                &[("path", &path)],
            ));
        }
        Ok((now != Some(text)).then(|| text.to_string()))
    });
    match done {
        Ok(w) => {
            remember_written(d, home, text);
            Ok(w)
        }
        Err(e) if moved => Err(WriteErr::Stale(e)),
        Err(e) => Err(WriteErr::Refused(e)),
    }
}

/// 记下这一份是 cc-monitor 写出去的（[`relay_route_core::PROFILES_WRITTEN_REL`]：只存版本）。记不下只进日志 —— 代价只是之后少说一句「手改过」。
fn remember_written(d: &dyn Door, home: &str, text: &str) {
    let note = json!({ "rev": rev_of(Some(text)) }).to_string();
    if let Err(e) = write_ours(d, home, relay_route_core::PROFILES_WRITTEN_REL, &mut |_| {
        Ok(Some(note.clone()))
    }) {
        tracing::warn!("{e}");
    }
}

/// 配置文件上次经 cc-monitor 写出去之后**有人改过**：盘上那份的版本与记下的不一样。没记过（cc-monitor 没写过）· 文件不在 ⇒ `false`。
pub(crate) fn edited_since_written(d: &dyn Door, store: &Store) -> bool {
    let Some(text) = store.text.as_deref() else {
        return false;
    };
    let recorded = door::peek(d, &store.home, relay_route_core::PROFILES_WRITTEN_REL)
        .ok()
        .and_then(|p| p.text)
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("rev").and_then(Value::as_str).map(str::to_string));
    recorded.is_some_and(|r| Some(r) != rev_of(Some(text)))
}

/// 照配置文件生成的那几样动了什么：别名文件（要重读的那一种）动没动 · 链接动没动 · 没做成的几句。
#[derive(Debug, Default)]
pub(crate) struct Outputs {
    /// 动了的别名文件（已开的终端要重读它）。
    pub rewrote: Vec<String>,
    pub links_changed: bool,
    pub problems: Vec<String>,
}

/// **照配置文件生成别名**：POSIX 上撞名的那几条写进 `aliases.sh`（固定的 `名字() { ccm @名字 "$@"; }`），其余做成链接；
/// 这台说 PowerShell 的，`aliases.ps1` 里每条一个函数。两份末尾都接上托管入口（`cch` · `<号>cch`）的补全（[`hosted_names`]）。
/// 内容没变的一个字节不写。
/// `also`：这一趟另要照顾的那一种（界面按某一种 shell 存的那一下）。
pub(crate) fn sync_outputs(
    d: &dyn Door,
    home: &str,
    book: &profile::Book,
    also: Option<Shell>,
) -> Outputs {
    let mut out = Outputs::default();
    let names: Vec<String> = book.profiles.iter().map(|p| p.name.clone()).collect();
    let hosted = hosted_names(book);
    let mut shells = machine_shells();
    if let Some(sh) = also.filter(|sh| !shells.contains(sh)) {
        shells.push(sh);
    }
    for shell in shells {
        let lines: Vec<String> = names
            .iter()
            .filter(|n| wants_function(n, shell))
            .map(|n| render_line(n, shell))
            .collect();
        let text = render_file(shell, &lines, &hosted);
        match write_alias_file(d, home, shell, |_| Ok(Some(text.clone()))) {
            Ok(true) => out.rewrote.push(alias_file_in(home, shell)),
            Ok(false) => {}
            Err(e) => out.problems.push(e),
        }
    }
    if cfg!(unix) {
        let want: Vec<String> = names
            .iter()
            .filter(|n| !wants_function(n, Shell::Posix))
            .cloned()
            .collect();
        let s = links::sync(d, home, &want);
        out.links_changed = s.changed;
        out.problems.extend(s.problems);
    }
    out
}

/// **读回口**：盘上那份别名文件 → 清单 ＋ 启动文件候选（各带别名块的现状）。只读。
///
/// `extra_rc`：人在界面上指的「其它文件」。给了就过
/// `profile_installer::fence_on`（只许落在 home 之内）、并进候选一起扫；过不了围栏 ⇒ `Err`。
///
/// **事实全问这台的文件管理面**（「怎么读到这个事实 → 下沉」）：
/// home（`files-home`）· 别名文件（`Door::peek`）· 候选各一次（`Door::peek`，PS 7 那两份的目录 `files-stat`）。
/// 门就是本进程的 `files-*`（[`door`]），`origin` 退役。
pub fn read_via(
    d: &dyn Door,
    shell: Shell,
    extra_rc: Option<&str>,
) -> Result<AliasListing, String> {
    let store = load(d)?;
    let home = store.home.clone();
    let extra = extra_rc.map(|raw| block::fence(&home, raw)).transpose()?;
    let names: Vec<String> = store.book.profiles.iter().map(|p| p.name.clone()).collect();
    Ok(AliasListing {
        shell,
        rc_candidates: rc_candidates_via(d, &home, shell, extra.as_deref(), &names)?,
        other_rc: extra,
    })
}

/// 那台机器上一份**可能不在**的文件，问一次看到了什么。
enum Look {
    Text(String),
    /// 不在（父目录不在也算：那台后端答「这个路径读不到」）。
    Absent,
    /// 在，但读不了 —— 后端原话。
    Unreadable(String),
}

/// 先 `Door::peek`；它报错时再问一次 `files-stat`：那台答「这个路径读不到」⇒ 当不在（`Door::peek` 在**父目录不在**时
/// 也报错 —— 它先解父目录 —— 而那一形就是「不在」），答得出 ⇒ 在但读不了、带回 `peek` 的原话。
/// ⚠ `files-stat` 的「读不到」与「不存在」分不开（权限不够也是它）⇒ 权限不够的那一份会被说成「不在」—— 那一形今天从
/// peek 那一步就先报了原话，走不到这里的只有 `stat` 也读不到的那几种。
fn look(d: &dyn Door, home: &str, abs: &str) -> Result<Look, String> {
    let rel = door::rel_under(home, abs)?;
    match door::peek(d, home, &rel) {
        Ok(p) => Ok(p.text.map_or(Look::Absent, Look::Text)),
        Err(said) => Ok(match door::stat_kind(d, abs)? {
            None => Look::Absent,
            Some(_) => Look::Unreadable(said),
        }),
    }
}

/// 这个名字是不是已经被占了。**只出声、不拦** —— 见 `§0c 问三`。怎么查由方言答（POSIX 查自带片段与 `PATH`；
/// PowerShell 查终端集成模板与 `PATH` 上的 `.exe` / `.cmd` / …）。
///
/// `PATH` 那一格查的是**这台后端进程**的 `PATH`（`platform/shell/dialect.rs::on_path` 头注自认会漏报）—— 规则住在那台机器上，
/// 查的就是那台（住 monitor 时远端只能不查，那一格随之退役）。
pub(crate) fn collision_note(name: &str, shell: Shell) -> Option<String> {
    shell.dialect().name_taken(name, &block::own_block(shell))
}

/// 候选启动文件的现状。列哪几份由方言答（POSIX 只列在的；PowerShell 的 `$PROFILE` 不在也列）——
/// 🔴 **`$PROFILE` 在哪，全仓只有 `ShellDialect::startup_candidates` 答**。
/// `extra` 是人另指的那一份（已过围栏；不在也列），与方言给的重了就不重复列。
///
/// 每份读**一次**：别名文件那一行接没接上（`sourced`）与别名块的现状（`block`）出自同一次读。
/// 那一次读问**那台机器的后端**（[`look`]）：方言只给路径与列法（`shell_dialect::Listed`），
/// 在不在、里面是什么、PS 7 的目录在不在（`files-stat`）都由门答。读不了的那一份照列、带后端原话（`unreadable`）。
pub(crate) fn rc_candidates_via(
    d: &dyn Door,
    home: &str,
    shell: Shell,
    extra: Option<&str>,
    names: &[String],
) -> Result<Vec<StartupFile>, String> {
    rc_candidates_asking(
        d,
        home,
        shell,
        extra,
        names,
        &crate::platform::shell::powershell::execution_policy,
    )
}

/// 同上，执行策略由 `ask` 答（同一代一次读里只问一次；判据交替身）。
pub(crate) fn rc_candidates_asking(
    d: &dyn Door,
    home: &str,
    shell: Shell,
    extra: Option<&str>,
    names: &[String],
    ask: &dyn Fn(crate::platform::shell::PsHost) -> crate::platform::shell::powershell::ExecPolicy,
) -> Result<Vec<StartupFile>, String> {
    let dia = shell.dialect();
    let mut asked = std::collections::BTreeMap::new();
    let mut cands = dia.startup_candidates(home);
    if let Some(x) = extra {
        if !cands.iter().any(|c| c.path == x) {
            cands.push(dialect::StartupCandidate {
                path: x.to_string(),
                listed: Listed::Always,
                host: None,
            });
        }
    }
    let mut out = Vec::new();
    for c in cands {
        if let Listed::IfDirExists(dir) = &c.listed {
            if door::stat_kind(d, dir)?.as_deref() != Some("dir") {
                continue;
            }
        }
        let (text, exists, unreadable) = match look(d, home, &c.path)? {
            Look::Text(t) => (Some(t), true, None),
            Look::Absent if c.listed == Listed::IfFileExists => continue,
            Look::Absent => (None, false, None),
            Look::Unreadable(e) => (None, true, Some(e)),
        };
        out.push(StartupFile {
            sourced: text.as_deref().is_some_and(|s| dia.sources_our_file(s)),
            exists,
            block: text
                .as_deref()
                .map(|t| block::block_state(Path::new(&c.path), t, names))
                .unwrap_or_default(),
            unreadable,
            policy: c
                .host
                .map(|h| asked.entry(h).or_insert_with(|| ask(h)).clone()),
            block_lines: block::render_block(Shell::of_target(Path::new(&c.path)), home)
                .map_or(0, |t| t.lines().count()),
            shown: crate::platform::paths::home_short(&c.path, Some(Path::new(home))),
            path: c.path,
        });
    }
    let loaded = out.iter().any(|f| f.block.present || f.sourced);
    block::settle_wins(out.iter_mut().map(|f| &mut f.block), loaded);
    // POSIX 上不撞名的那几条是 PATH 里的链接：shell 函数恒盖过 PATH 上的程序 ⇒ 用户写的同名函数不论在接入行前后都生效。
    if shell == Shell::Posix {
        for c in out
            .iter_mut()
            .flat_map(|f| f.block.conflicting_functions.iter_mut())
            .filter(|c| !wants_function(&c.name, Shell::Posix))
        {
            c.wins = block::Wins::Yours;
        }
    }
    Ok(out)
}

/// 把生成文件写下去。**内容一致就一个字节都不写。**
///
/// 经 `door`（本机后端）写：生成文件是**我们自己**的东西 ⇒ 不留备份文件；
/// `~/.cc-monitor` 还不在就逐级补（`parents`）。回读 · 回滚那一份规则住后端。
/// 落盘那一份按方言编码（PowerShell 加 BOM）。
/// `plan` 拿到盘上此刻那一份（原样字节；`None` = 不在），回编码前的新全文。
fn write_alias_file(
    d: &dyn Door,
    home: &str,
    shell: Shell,
    mut plan: impl FnMut(Option<&str>) -> Result<Option<String>, String>,
) -> Result<bool, String> {
    let dia = shell.dialect();
    write_ours(d, home, dia.our_alias_file_rel(), &mut |now| {
        Ok(plan(now)?.map(|t| dia.encode_for_disk(&t)))
    })
}

/// 照盘上此刻那一份（`None` ＝ 不在）算要写的全文（`None` ＝ 不写）。
type PlanText<'a> = dyn FnMut(Option<&str>) -> Result<Option<String>, String> + 'a;

/// 本模块写盘的唯一一口：`~/.cc-monitor` 下我们自己的文件（别名文件 · 配置文件）经这台的文件管理面
/// （`Door::peek` / `Door::put`：原子替换 · 内容比对 · 回读 · 回滚）。`plan` 拿到盘上此刻那一份（`None` ＝ 不在），
/// 回要写的全文（`None` ＝ 不写）。不留备份文件（配置文件的历史由那份文件自己的写法管：按条目改、手写的留着）。
fn write_ours(
    d: &dyn Door,
    home: &str,
    rel: &str,
    plan: &mut PlanText<'_>,
) -> Result<bool, String> {
    let done = door::edit(d, home, rel, false, true, plan)?;
    Ok(matches!(done, door::Edited::Written(_)))
}

// ═══════════════════════════════════════════════════════════════════════════
// 线上那八条：`aliases-render` · `-read` · `-install` · `-block-render` · `-block-install` · `-block-remove`
// ＋ 表单两向 `aliases-to-form` · `aliases-from-form`
// ═══════════════════════════════════════════════════════════════════════════

fn refused(said: String) -> Said {
    Said::new("refused", said)
}

/// `aliases-read` 的入参：人另指的那一份启动文件（`null` = 不指；过围栏后并进候选）。
/// 哪种 shell 不由问的一方给：这台后端照自己的平台答（[`here_shell`]），回包里说是哪一种。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AliasesReadReq {
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub(crate) rc_path: Option<String>,
}

/// `aliases-read`：这台说的那种 shell 的启动文件候选（接入那一格）。
pub(crate) fn answer_read(d: &dyn Door, req: &AliasesReadReq) -> Result<AliasListing, Said> {
    read_via(d, here_shell(), req.rc_path.as_deref()).map_err(refused)
}

/// 别名块那三条的入参：目标文件（方言由它的扩展名定：`.ps1` ⇒ PowerShell）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BlockReq {
    pub(crate) rc_path: String,
}

/// 别名块那三条：`rcPath` 那份文件过围栏、方言按它的扩展名定、再过方言闸。
fn block_target(d: &dyn Door, req: &BlockReq) -> Result<(String, String), Said> {
    let home = door::home(d).map_err(refused)?;
    let p = block::fence(&home, &req.rc_path).map_err(refused)?;
    dialect_here(Shell::of_target(Path::new(&p))).map_err(refused)?;
    Ok((home, p))
}

/// `aliases-block-render` 的应答：往一份空文件里装一次会写成什么（与 `aliases-block-install` 调同一个 `plan_install`）。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct BlockText {
    pub(crate) text: String,
}

/// 装好 / 卸掉了（零出参）。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct BlockDone {}

/// `aliases-block-render {rcPath}` → `{text}`（纯：往一份空文件里装一次会写成什么；方言由 `rcPath` 的扩展名定）。
pub(crate) fn answer_block_render(d: &dyn Door, req: &BlockReq) -> Result<BlockText, Said> {
    let shell = Shell::of_target(Path::new(&req.rc_path));
    dialect_here(shell).map_err(refused)?;
    let home = door::home(d).map_err(refused)?;
    let text = block::render_block(shell, &home).map_err(refused)?;
    Ok(BlockText { text })
}

/// `aliases-block-install {rcPath}` → `{}`（幂等，整块替换；经这台的 `Door::put`）。
pub(crate) fn answer_block_install(d: &dyn Door, req: &BlockReq) -> Result<BlockDone, Said> {
    let (_, p) = block_target(d, req)?;
    block::install_to_profile(d, Path::new(&p)).map_err(refused)?;
    Ok(BlockDone {})
}

/// `aliases-block-remove {rcPath}` → `{}`（整块删，块外一个字节不动；围栏损坏 ⇒ 中止）。
pub(crate) fn answer_block_remove(d: &dyn Door, req: &BlockReq) -> Result<BlockDone, Said> {
    let (_, p) = block_target(d, req)?;
    block::uninstall_from_profile(d, Path::new(&p)).map_err(refused)?;
    Ok(BlockDone {})
}

/// `powershell-policy-set` 的入参：哪一代 PowerShell。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct PolicySetReq {
    /// `powershell`（5.1）· `pwsh`（7）。
    pub(crate) host: crate::platform::shell::PsHost,
}

/// `powershell-policy-set` 的应答：设完现问的那一份 ＋ 设的那一下没成的原因词。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct PolicySet {
    /// 形状同 `aliases-read` 候选里的 `policy`。
    pub(crate) policy: crate::platform::shell::powershell::ExecPolicy,
    /// 设的那一下没成的原因词（组策略压着时 PowerShell 会报；它的原话只记后端日志）；`null` = 没报。
    pub(crate) set_error: Option<String>,
}

/// `powershell-policy-set {host}` → `{policy, setError}`：那一代 PowerShell 当前用户那一档设成
/// `RemoteSigned`，再现问一次。只做这一件固定的事（不收策略值）；界面只在用户点了、确认了之后发。这台不说 PowerShell ⇒ 拒。
pub(crate) fn answer_policy_set(req: &PolicySetReq) -> Result<PolicySet, Said> {
    dialect_here(Shell::PowerShell).map_err(refused)?;
    let (policy, set_error) = crate::platform::shell::powershell::allow_local_scripts(req.host);
    Ok(PolicySet { policy, set_error })
}

crate::wire::typed::codes! {
    /// 别名那几条的码：这台的方言不认 · 过不了围栏 · 读写没成（那一句说清哪一样）。
    pub enum AliasErr {
        Refused = "refused",
    }
}

/// `aliases-read`：启动文件候选（接入那一格）（经这台的文件管理面读写 ⇒ 阻塞档，带文件面）。
pub struct AliasesRead;

impl crate::wire::typed::Command for AliasesRead {
    const NAME: &'static str = "aliases-read";
    type Req = AliasesReadReq;
    type Reply = AliasListing;
    type Err = AliasErr;
}

impl crate::wire::typed::Handle for AliasesRead {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        req: AliasesReadReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<AliasListing, AliasErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let files = needs;

            answer_read(files, &req).map_err(crate::wire::typed::of_fail_with(AliasErr::Refused))
        }
    }
}

/// `aliases-block-render`：别名块预览（经这台的文件管理面读写 ⇒ 阻塞档，带文件面）。
pub struct AliasesBlockRender;

impl crate::wire::typed::Command for AliasesBlockRender {
    const NAME: &'static str = "aliases-block-render";
    type Req = BlockReq;
    type Reply = BlockText;
    type Err = AliasErr;
}

impl crate::wire::typed::Handle for AliasesBlockRender {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        req: BlockReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BlockText, AliasErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let files = needs;

            answer_block_render(files, &req)
                .map_err(crate::wire::typed::of_fail_with(AliasErr::Refused))
        }
    }
}

/// `aliases-block-install`：别名块装进人选的那份启动文件（经这台的文件管理面读写 ⇒ 阻塞档，带文件面）。
pub struct AliasesBlockInstall;

impl crate::wire::typed::Command for AliasesBlockInstall {
    const NAME: &'static str = "aliases-block-install";
    type Req = BlockReq;
    type Reply = BlockDone;
    type Err = AliasErr;
}

impl crate::wire::typed::Handle for AliasesBlockInstall {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        req: BlockReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BlockDone, AliasErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let files = needs;

            answer_block_install(files, &req)
                .map_err(crate::wire::typed::of_fail_with(AliasErr::Refused))
        }
    }
}

/// `aliases-block-remove`：别名块卸掉（经这台的文件管理面读写 ⇒ 阻塞档，带文件面）。
pub struct AliasesBlockRemove;

impl crate::wire::typed::Command for AliasesBlockRemove {
    const NAME: &'static str = "aliases-block-remove";
    type Req = BlockReq;
    type Reply = BlockDone;
    type Err = AliasErr;
}

impl crate::wire::typed::Handle for AliasesBlockRemove {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        req: BlockReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<BlockDone, AliasErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let files = needs;

            answer_block_remove(files, &req)
                .map_err(crate::wire::typed::of_fail_with(AliasErr::Refused))
        }
    }
}

/// `powershell-policy-set`：那一代 PowerShell 的执行策略设成当前用户 `RemoteSigned`（起一次那一代 PowerShell ⇒ 阻塞档）。
pub struct PowershellPolicySet;

impl crate::wire::typed::Command for PowershellPolicySet {
    const NAME: &'static str = "powershell-policy-set";
    type Req = PolicySetReq;
    type Reply = PolicySet;
    type Err = AliasErr;
}

impl crate::wire::typed::Handle for PowershellPolicySet {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: PolicySetReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<PolicySet, AliasErr> {
        answer_policy_set(&req).map_err(crate::wire::typed::of_fail_with(AliasErr::Refused))
    }
}

// 判据用的真盘门（`HomeDoor` · `FilesHere`）：本包与上层判据经 `test-support` 共用。
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../../tests/backend/assets/aliases/aliases_support.rs"]
pub mod test_support;

#[cfg(test)]
#[path = "../../../../tests/backend/assets/aliases/aliases_tests.rs"]
mod tests;

// 两种方言的别名读法里用到这一族的调用形状与别名块正文的几条（方言住平台层，那个包够不到这一层）。
#[cfg(test)]
#[path = "../../../../tests/backend/assets/aliases/dialect_tests.rs"]
mod dialect_tests;
