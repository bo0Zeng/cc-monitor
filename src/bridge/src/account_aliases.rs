//! 别名：**名字 ＋ 一组 ccm 参数**（`设计/71`）。一类，没有「账号别名」这一种。
//!
//! 〔用 2026-09-17〕逐字：「**不要有 account alias 这种东西。alias 应该独立吗？应该就是 ccm
//! 参数附加器。即生成别名，都是调用 ccm，ccm 本身就可以指定账号。**」
//!
//! # 来历（`K-R49`）
//!
//! 本模块起于 09-10 那一句「**添加账号后添加对应命令, 像是 zcc, bcc 这种……还得手动去改**」：
//! 那时它按**账号表**整份重写一份文件，TS 侧的 `buildAliasLine`〔散文墓碑〕拼行、这里只管落盘
//! ＋ 一道「形状围栏」挡注入。〔AL1 · 2026-09-24〕两处都翻了：清单归**用户**（不再是账号表的投影，
//! `71 §8`），shell 文本由**本模块**渲染（前端递的是结构，不是代码 —— 审计 S-1 那条
//! 「后端拥有那段文本」终于对别名也成立了），那道形状围栏随之退役。
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
//! 〔RW1 · 第四波 09-24〕文件名从 `account-aliases.sh` 改成 `aliases.sh`（`71 §9.2` 那道迁移题按主会话裁结案：
//! **不留兼容** —— 不留转发件、不读旧名）；旧文件留在盘上，本模块不读、不写、不删。
//!
//! # 🔴 接上这份文件的那一行 source **只住别名块里**（`设计/71 §6.1`「source 那一行只许一处装」）
//!
//! 两种方言的别名块都自带那一行（POSIX：`src/shared/ccm-aliases.sh` 最后一行；PowerShell：`scripts/cc.ps1.tpl`
//! 结尾那一行，〔TL1 · 4C〕补上 —— 从前那一侧不带，两边不对称），文件不在就什么都不做 ⇒ 装了别名块的人什么都不用做。
//! 〔TL1 · 4C〕本模块**不再往 rc 里写那一行**：从前 [`install_in`] 会把它装进人指定的那份 rc（另一对围栏），
//! 与别名块那一行是同一件事的第二个写处（`AL1d.md §5` 第 4 条）。今天那一步**降级成检查**：选了 rc 就看它接没接上，
//! 没接上就说「装上别名块就接上了」并给出那一行、由人自己决定（提供检测，不代装）。
//! 盘上已有的旧围栏那一块不读、不删、不写清理代码（两处 source 同一份文件是幂等的，只多点一次）。
//! 别名文件的写**不在本进程落盘**：经本机后端的文件管理那一面（`user_files::edit` → `files-peek` / `files-put`），
//! 备份 · 原子替换 · 回读 · 回滚那一份规则住后端。

//!
//! # 〔AL1c · 第四波 4B〕本模块是**通用层**：只持有结构与规则，不持有任何一种 shell 的文本
//!
//! 「这个 shell 怎么写 / 文件落在哪 / 名字怎么认」全在 `shell_dialect.rs`（`设计/71 §4.4`，POSIX 与 PowerShell
//! 各一份实现）；这里留的是**判定的规则**（V1–V5 · 能力闸 · 重名 · 有一条不合格整批不写）。
//! 三条命令各带一个 `shell`：同一份清单，POSIX 落 `~/.cc-monitor/aliases.sh`、PowerShell 落
//! `~/.cc-monitor/aliases.ps1`，各自由那个 shell 的别名块里那一行 source 接上。

use crate::copy_table::copy_text;
use std::path::Path;

use crate::origin::Origin;
use crate::shell_dialect::{Listed, Shell};
use crate::user_files::Door;

// 〔TL1 · 4C〕墓碑：这里从前有一对**写进用户 rc / `$PROFILE`** 的围栏常量（`# === cc-monitor aliases BEGIN v1 ===` 那一对），
//   包着 [`install_in`] 代装的那一行 source。那一步退役（`71 §6.1`），这对围栏随之删 —— 盘上已有的那一块不读不删。

/// 生成文件自己的围栏（整份重写，所以它只是给人看的边界；两种方言共用，理由同上）。
/// 〔AL1 · 2026-09-24〕v1 → v2：`71` 逐字「不要有 account alias 这种东西」—— 文件里只有**一类**别名。
/// 读回（[`read_in`]）不认这两行（注释行一律跳过），所以盘上那份 v1 照样读得回来。
const FILE_BEGIN: &str = "# === cc-monitor aliases BEGIN v2 ===";
const FILE_END: &str = "# === cc-monitor aliases END ===";

/// 一份候选启动文件（rc / `$PROFILE`）的状态。
///
/// 〔AL1d · 第四波 4B〕从前叫 `AccountAliasRc`，只答「接没接上别名文件」；别名块（`cc` / `cct` ·
/// `__ccm_bind`）装进的是**同一批**文件，却另有一条命令、另一份候选（`AL1d.md §1.2`）。
/// 今天一份候选、一次扫描，两件事都在这一格上（`block`）。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct StartupFile {
    /// 绝对路径。
    pub path: String,
    /// 这份文件今天已经把生成文件 `source` 进去了吗（装过 ccm 别名块的人这一格就是 true）。
    pub sourced: bool,
    /// 〔AL1c〕这份文件在不在盘上。POSIX 只列在的（恒 true）；PowerShell 的 `$PROFILE` 常常要装的时候才建。
    pub exists: bool,
    /// 〔AL1d〕这份文件里别名块的现状（不在盘上 ⇒ 全空）。
    pub block: crate::profile_installer::BlockState,
    /// 〔AL2〕在盘上、可那台后端读不了它（非 UTF-8 · 太大 · 解到 home 外 · I/O）—— 后端原话；`None` = 读得了或不在。
    /// 从前本机直读时这一形被吞成「没有别名块」。
    pub unreadable: Option<String>,
}

/// 生成文件的绝对路径。`home` 由调用方给 —— 测试拿临时目录当 home，**绝不碰真实家目录**。
///
/// 〔WIN1 · RT1 F8〕**逐段**拼：`our_alias_file_rel` 是 `/` 分隔的（交给后端的 `rel` 就是它，那一侧不动），
/// 而这里拼的是**给人看、也写进 `$PROFILE` 那一行**的那台机器上的绝对路径。整串一次拼在 Windows 上
/// 会得到 `C:\Users\zbl\.cc-monitor/aliases.ps1`（真机读数，`RT1.md §8` F8）—— 两种分隔符混着。
/// 〔AL2 · 第四波 4D〕`home` 是**那台机器**的后端答的字符串，拼法跟它自己的分隔符走（`user_files::join_under`）：
/// 从前 `Path::join` 用的是 monitor 这台的分隔符 —— Windows 上的 monitor 拼远端 `/home/zbl` 会得到 `/home/zbl\.cc-monitor\aliases.sh`。
pub fn alias_file_in(home: &str, shell: Shell) -> String {
    crate::user_files::join_under(home, shell.dialect().our_alias_file_rel())
}

/// 整份生成文件的内容（**编码前**：BOM 那一层在落盘那一跳按方言加）。
/// **没有时间戳** —— 有了就永远比不出「内容没变」，每次都要写一遍。
pub fn render_file(shell: Shell, lines: &[String]) -> String {
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
    out.push_str(FILE_END);
    out.push('\n');
    out
}

// ═══════════════════════════════════════════════════════════════════════════
// 〔AL1 · 2026-09-24〕`设计/71`：**别名只有一类 —— 名字 ＋ 一组 ccm 参数**
// ═══════════════════════════════════════════════════════════════════════════
//
// 用户 2026-09-17 逐字：「不要有 account alias 这种东西……就是 ccm 参数附加器」。
// 账号（`--account`）只是参数里的一个维度。清单由**用户**拥有，不跟着账号表自动增删（`71 §8`）。
//
// 命令面是两跳（`71 §12.6`）：
//   ① [`render`] —— **纯**：清单 → 代码（＋ 每条的问题 ＋ 撞名提示）。预览、复制都只调这一跳；
//   ② [`install_in`] —— **唯一的副作用**：同一份渲染落进我们自己那份别名文件（经那台机器的后端），
//      可选地往用户选的启动文件里装一行 `source`。它收的是**清单**不是代码 —— 写进 shell 的文本
//      只由 `shell_dialect` 产出（审计 S-1：绝不让前端注入可执行的 shell），而「写的就是预览的那一份」
//      由两跳调同一个 [`render`] 保证。
// 读回口：[`read_in`] 把盘上那份解析回清单（`70 §3.1` 那张「没有的」表第一条）。

/// 一条别名。`args` 是原样的 ccm argv（`["--ccm-tmux", "--account", "z"]`），渲染时由方言逐个按需加引号。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct Alias {
    pub name: String,
    pub args: Vec<String>,
}

/// 一条别名的问题（进不了代码的那一条为什么进不了）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct AliasProblem {
    pub name: String,
    pub message: String,
}

/// ① 那一跳的产物。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AliasRender {
    /// 整份文件（写入那一跳原样落盘的就是它 —— PowerShell 多一个 BOM；手贴的人复制它或 `lines`）。
    pub code: String,
    /// 每条合格别名的写法，按清单顺序（POSIX 一行；PowerShell 一个函数块，含换行）。
    pub lines: Vec<String>,
    /// 不合格的那几条。**非空时 [`install_in`] 一个字节都不写**（fail-closed）。
    pub problems: Vec<AliasProblem>,
    /// 名字撞了的提示，一条一句。**只出声、不拦**（`cc` 在多数机器上是 C 编译器，盖不盖由人定）。
    pub collisions: Vec<String>,
}

/// 读回口的产物。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AliasListing {
    pub alias_path: String,
    /// 那份文件在不在。不在 ≠ 读失败（读失败是 `Err`）。
    pub exists: bool,
    pub aliases: Vec<Alias>,
    /// 解析不回清单的那几行（原文 ＋ 原因）。**不静默丢**：写回去之前人得知道它们会没。
    pub unparsed: Vec<String>,
    /// 这台机器上这种 shell 的启动文件候选（「那一行 source 加进哪份」·「别名块装进哪份」，同一批）。
    pub rc_candidates: Vec<StartupFile>,
    /// 〔AL1d〕这台机器上已经跟 monitor 完成拉前握手的终端数（PowerShell 别名块里 `__ccm_bind` 的产物）。
    /// 它不是盘上的事实（住 monitor 进程里的 `BindRegistry`）⇒ 由调用方给，本模块不认它。
    pub bound_terminals: u32,
    /// 〔AL1d〕人另指的那一份（`read_in` 的 `extra_rc`）过了围栏之后的绝对路径 —— 界面拿它在候选里认出「刚指的是哪一份」。
    pub other_rc: Option<String>,
}

/// ② 那一跳的产物。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AliasInstallReport {
    pub alias_path: String,
    pub wrote_alias_file: bool,
    /// 给人看的补充说明，一条一句。
    pub notes: Vec<String>,
}

/// 能进别名的 ccm 壳层选项：`(旗标, 要不要跟一个值)`。`71 §2.1` 第一、二档；
/// 第三档（[`NOT_IN_ALIASES`]）每次取值都不同，做成固定别名没意义 ⇒ 不收。
/// `--ccm-tmux=<名>` 是 `--ccm-tmux` 的内联形，另判。〔V138〕其余的词（`--model` · `--resume` · `-p` …）是交给 claude 的，原样放行。
///
/// ⚠ 每一个旗标都得是后端 `ccm --help` 里真有的那个词 —— 判据
/// `account_aliases_tests.rs::every_alias_flag_is_a_real_ccm_flag` 去后端的用法文本里对（异源）。
pub(crate) const ALIAS_FLAGS: &[(&str, bool)] = &[
    ("--cwd", true),
    ("--account", true),
    ("--base", false),
    ("--ccm-tmux", false),
    ("--tmux-base", true),
    ("--ccm-agent", true),
    ("--launcher", true),
    ("--tmux-size", true),
    ("--detach", false),
    ("--bus-register", false),
    ("--bus-note", true),
];

/// 〔V138〕ccm 自己的、不进别名的那几个（`71 §2.1` 第三档改名后的样子）：接回会话 ＋ `--ccm-*` 诊断口。
pub(crate) const NOT_IN_ALIASES: &[&str] = &[
    "--attach",
    "--ccm-sid",
    "--ccm-print",
    "--ccm-probe",
    "--ccm-help",
    "--ccm-version",
];

/// 〔AL1c〕**载体是 tmux 的那几个旗标**（`--ccm-tmux=<名>` 是 `--ccm-tmux` 的内联形，一并算）。
/// 这台机器没有 tmux ⇒ 它们一个都不许进别名（生成出来就是一条当场 `no_tmux` 的别名）。
///
/// 🔴 它**不是**本模块自己的判断：事实源是后端 `control/ccm/mod.rs::CCM_TMUX_CARRIED`（靠 tmux 活着的 ccm 能力）。
/// 判据 `account_aliases_tests.rs::the_tmux_gate_is_exactly_the_backends_tmux_carried_flags`
/// 读后端原文、按「本表 == 那张表 ∩ [`ALIAS_FLAGS`]」两向相等钉着。
pub(crate) const NEEDS_TMUX: &[&str] = &[
    "--ccm-tmux",
    "--tmux-base",
    "--tmux-size",
    "--detach",
    "--bus-register",
];

/// 〔AL1c〕这台机器的**能力**（`设计/96`）—— 不是方言（`71 §4.4` 逐字：`has_tmux()` 不进那一族）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    /// 有没有 tmux（`--ccm-tmux` 那一族能不能用）。
    pub tmux: bool,
}

impl Caps {
    /// 一种 shell 的目标机器上有什么。今天只有一个事实：**PowerShell 目标 ⇔ Windows ⇔ 没有 tmux**
    /// （后端 `lib.rs::TARGET_GAPS` 里 `ccm-launcher` × Windows 那几行，用户 2026-09-21「windows 用 windows
    /// 自己的后台服务，后面再做」）。POSIX 目标按「有」算：没装 tmux 的 Linux 由 ccm 在运行时报 `no_tmux`（今天就是这样）。
    pub fn of(shell: Shell) -> Caps {
        Caps {
            tmux: shell == Shell::Posix,
        }
    }
}

/// 〔AL2 · 第四波 4D〕**这台机器今天承诺说这种方言吗**（`设计/01 §6.7b` 表 B ＋ D7「显式拒绝，不许静默推一份跑不起来的东西」）。
///
/// 事实只有一个住址：表 B 住 `byte_table::promised`（「这个 origin 今天承诺哪几种机器」），这里**只问它**，不抄一份
/// 「远端只有 Linux」。方言 → 机器那一格与 [`Caps::of`] 同一条事实：**PowerShell ⇔ Windows**；POSIX 在三种 OS 上都有人说
/// （Windows 上是 Git Bash）。⇒ 远端（表 B 只承诺 Linux）拿 PowerShell ⇒ 拒。
///
/// 为什么要这一道：清单三条的 `shell` 是界面给的（远端卡恒 `posix`，`第四波记录/W5-ALIAS.md §2.2`），
/// 而别名块三条的方言按**目标文件扩展名**定（`71 §4.4`）—— 远端「其它文件」填一个 `.ps1` 就会走 PowerShell 那一臂，
/// 把**本机** monitor 的数据目录填进模板写到远端（`profile_installer::plan_install`）。`71 §4.4` 与表 B 两句同时成立的解只有「显式拒」。
/// 哪天表 B 多了远端 Windows：改 `byte_table::promised` 一处，这里自动放行（`AL2.md §2.3`）。
pub fn dialect_promised(origin: &Origin, shell: Shell) -> Result<(), String> {
    use crate::byte_table::{promised, Arch, Key, Os, Route};
    let route = if origin.is_local() {
        Route::Local
    } else {
        Route::Remote
    };
    let speakers: &[Os] = match shell {
        Shell::PowerShell => &[Os::Windows],
        Shell::Posix => &[Os::Linux, Os::Mac, Os::Windows],
    };
    let ok = speakers.iter().any(|&os| {
        [Arch::X86_64, Arch::Aarch64]
            .into_iter()
            .any(|arch| promised(route, Key { os, arch }))
    });
    if ok {
        return Ok(());
    }
    Err(copy_text(
        "rsAccountAliases.dialect.notPromised",
        &[
            (
                "machine",
                &crate::backend::control::cc_bus::machine_label(origin.as_wire_str()),
            ),
            (
                "shell",
                &match shell {
                    Shell::Posix => "POSIX",
                    Shell::PowerShell => "PowerShell",
                }
                .to_string(),
            ),
        ],
    ))
}

/// 一条别名合不合格。**这些是「判定的规则」**（`71 §4.3` 第 6、7 格），与哪种 shell 无关；
/// 方言只回答两个读法问题：名字的字符集、一个值能不能原样传到 ccm。
pub fn check_alias(a: &Alias, shell: Shell) -> Result<(), String> {
    let d = shell.dialect();
    let caps = Caps::of(shell);
    if !d.name_is_valid(&a.name) {
        return Err(copy_text("rsAccountAliases.check.badName", &[]).into());
    }
    let (mut account, mut base, mut tmux, mut tmux_named, mut tmux_base) =
        (false, false, false, false, false);
    let (mut size, mut detach, mut bus, mut note) = (false, false, false, false);
    let mut it = a.args.iter();
    while let Some(w) = it.next() {
        if w.chars().any(char::is_control) {
            return Err(copy_text("rsAccountAliases.check.controlChar", &[]).into());
        }
        d.arg_is_passable(w)?;
        if w == "--" {
            for x in it.by_ref() {
                if x.chars().any(char::is_control) {
                    return Err(copy_text("rsAccountAliases.check.controlChar", &[]).into());
                }
                d.arg_is_passable(x)?;
            }
            break;
        }
        // 能力闸（`71 §4.6 ①`：「`cct` 在 Windows 上没有」从硬编码变成能力查询）。
        let head = w.split_once('=').map_or(w.as_str(), |(h, _)| h);
        if !caps.tmux && NEEDS_TMUX.contains(&head) {
            return Err(copy_text(
                "rsAccountAliases.check.noTmux",
                &[(
                    "flags",
                    &(NEEDS_TMUX
                        .iter()
                        .map(|f| format!("`{f}`"))
                        .collect::<Vec<_>>()
                        .join(" / "))
                    .to_string(),
                )],
            ));
        }
        if let Some(n) = w.strip_prefix("--ccm-tmux=") {
            if n.is_empty() {
                return Err(copy_text("rsAccountAliases.check.tmuxNoName", &[]).into());
            }
            tmux = true;
            tmux_named = true;
            continue;
        }
        if NOT_IN_ALIASES.contains(&head) {
            return Err(copy_text(
                "rsAccountAliases.check.notAllowed",
                &[("word", &w.to_string())],
            ));
        }
        // V138：不是 ccm 的壳层选项 ⇒ 交给 claude 的词，原样放行（控制字符与方言那一关上面已过）。
        let Some((flag, takes)) = ALIAS_FLAGS.iter().find(|(f, _)| f == w) else {
            continue;
        };
        if *takes {
            match it.next() {
                Some(v) if !v.is_empty() && !v.chars().any(char::is_control) => {
                    d.arg_is_passable(v)?;
                    // `71 §8 #11`：相对 / 带 `..` 的 `--cwd` ccm 运行时会拒（`INVARIANTS §47`）⇒ 生成前就拦。
                    if *flag == "--cwd" && !cwd_form_ok(v, shell) {
                        return Err(copy_text(
                            "rsAccountAliases.check.cwdNotAbsolute",
                            &[("value", &v.to_string())],
                        ));
                    }
                }
                _ => {
                    return Err(copy_text(
                        "rsAccountAliases.check.missingValue",
                        &[("flag", &flag.to_string())],
                    ))
                }
            }
        }
        match *flag {
            "--account" => account = true,
            "--base" => base = true,
            "--ccm-tmux" => tmux = true,
            "--tmux-base" => {
                tmux = true;
                tmux_base = true;
            }
            "--tmux-size" => size = true,
            "--detach" => detach = true,
            "--bus-register" => bus = true,
            "--bus-note" => note = true,
            _ => {}
        }
    }
    // V1–V4（`71 §5`，依据是 `ccm --help` 逐字）＋ 〔AL1c〕后端 `argv.rs` 那道「备注要有登记」的闸。
    if account && base {
        return Err(copy_text("rsAccountAliases.check.accountXorBase", &[]).into());
    }
    if tmux_named && tmux_base {
        return Err(copy_text("rsAccountAliases.check.tmuxXorBase", &[]).into());
    }
    if bus && !detach {
        return Err(copy_text("rsAccountAliases.check.busNeedsDetach", &[]).into());
    }
    if note && !bus {
        return Err(copy_text("rsAccountAliases.check.noteNeedsRegister", &[]).into());
    }
    if (size || detach) && !tmux {
        return Err(copy_text("rsAccountAliases.check.sizeNeedsTmux", &[]).into());
    }
    Ok(())
}

/// `--cwd` 的形式判定，与 ccm 运行时同一条（`plan.rs::free_text_gate`：绝对 · 无 `..` 段 · 无 NUL/CR/LF）。
/// POSIX 调共享那一份；PowerShell 目标是 Windows 路径（盘符根或 UNC），分隔符两种都认。
fn cwd_form_ok(v: &str, shell: Shell) -> bool {
    match shell {
        Shell::Posix => shell_quote_core::posix_free_path_ok(v),
        Shell::PowerShell => {
            let b = v.as_bytes();
            let drive = b.len() >= 3
                && b[0].is_ascii_alphabetic()
                && b[1] == b':'
                && matches!(b[2], b'\\' | b'/');
            (drive || v.starts_with("\\\\"))
                && !v.split(['/', '\\']).any(|seg| seg == "..")
                && shell_quote_core::free_text_ok(v)
        }
    }
}

/// 一条（合格的）别名在这种 shell 里的写法（方言那一份的薄包装）。
pub fn render_line(a: &Alias, shell: Shell) -> String {
    shell.dialect().render_alias(&a.name, &a.args)
}

/// ① **纯**：清单 → 代码。一个字节都不写、一个文件都不读（撞名检查读的是自带片段 / 模板与 `PATH`）。
///
/// 〔AL2 · 第四波 4D〕`origin` 只决定一件事：撞名那一格查不查 `PATH`（[`collision_note`]）。其余一个字不看它 ——
/// 本机与远端同一份规则、同一种方言（`71 §6`「`origin` 是本机还是远端，对这些命令没有区别」）。
pub fn render(aliases: &[Alias], shell: Shell, origin: &Origin) -> AliasRender {
    let d = shell.dialect();
    let mut lines = Vec::new();
    let mut problems = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for a in aliases {
        if seen.iter().any(|s| d.same_name(s, &a.name)) {
            problems.push(AliasProblem {
                name: a.name.clone(),
                message: copy_text("rsAccountAliases.render.duplicate", &[]).into(),
            });
            continue;
        }
        seen.push(&a.name);
        match check_alias(a, shell) {
            Ok(()) => lines.push(render_line(a, shell)),
            Err(message) => problems.push(AliasProblem {
                name: a.name.clone(),
                message,
            }),
        }
    }
    let collisions = aliases
        .iter()
        .filter(|a| d.name_is_valid(&a.name))
        .filter_map(|a| collision_note(&a.name, shell, origin))
        .collect();
    AliasRender {
        code: render_file(shell, &lines),
        lines,
        problems,
        collisions,
    }
}

/// **读回口**：盘上那份别名文件 → 清单 ＋ 启动文件候选（各带别名块的现状）。只读。
///
/// 〔AL1d〕`extra_rc`：人在界面上指的「其它文件」（从前是终端集成那一块的「自定义路径」）。给了就过
/// `profile_installer::fence_on`（只许落在 home 之内）、并进候选一起扫；过不了围栏 ⇒ `Err`。
///
/// 〔AL2 · 第四波 4D〕**事实全问那台机器的后端**（`71 §4.1`「怎么读到这个事实 → 下沉」· `第四波记录/W5-ALIAS.md §2.2`）：
/// home（`files-home`）· 别名文件（`files-peek`）· 候选各一次（`files-peek`，PS 7 那两份的目录 `files-stat`）。
/// 从前这里 `std::fs::read_to_string` 读 monitor 这台的盘 —— 只答得了本机；今天本机远端同一个函数，只差门的 `origin`。
/// `origin` 只进围栏（符号链接那一步只对本机）。`bound_terminals` 由调用方给（它住 monitor 进程里的 `BindRegistry`，远端恒 0）。
pub async fn read_via<D: Door>(
    door: &D,
    origin: &Origin,
    shell: Shell,
    extra_rc: Option<&str>,
    bound_terminals: u32,
) -> Result<AliasListing, String> {
    let home = door.home().await?;
    let extra = extra_rc
        .map(|raw| crate::profile_installer::fence_on(origin, &home, raw))
        .transpose()?;
    let path = alias_file_in(&home, shell);
    let text = match look(door, &home, &path).await? {
        Look::Text(t) => Some(t),
        Look::Absent => None,
        Look::Unreadable(e) => {
            return Err(copy_text(
                "rsAccountAliases.read.failed",
                &[("path", &path), ("e", &e)],
            ))
        }
    };
    let exists = text.is_some();
    let mut aliases = Vec::new();
    let mut unparsed = Vec::new();
    let d = shell.dialect();
    for got in d.parse_file(d.decode_from_disk(text.as_deref().unwrap_or(""))) {
        match got {
            Ok((name, args)) => {
                let a = Alias { name, args };
                match check_alias(&a, shell) {
                    Ok(()) => aliases.push(a),
                    Err(why) => unparsed.push(format!("{}（{why}）", render_line(&a, shell))),
                }
            }
            Err(raw) => unparsed.push(raw),
        }
    }
    Ok(AliasListing {
        alias_path: path,
        exists,
        aliases,
        unparsed,
        rc_candidates: rc_candidates_via(door, &home, shell, extra.as_deref()).await?,
        bound_terminals,
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

/// 先 `files-peek`；它报错时再问一次 `files-stat`：那台答「这个路径读不到」⇒ 当不在（`files-peek` 在**父目录不在**时
/// 也报错 —— 它先解父目录 —— 而那一形就是「不在」），答得出 ⇒ 在但读不了、带回 `peek` 的原话。
/// ⚠ `files-stat` 的「读不到」与「不存在」分不开（权限不够也是它）⇒ 权限不够的那一份会被说成「不在」—— 那一形今天从
/// peek 那一步就先报了原话，走不到这里的只有 `stat` 也读不到的那几种。
async fn look<D: Door>(door: &D, home: &str, abs: &str) -> Result<Look, String> {
    let rel = crate::user_files::rel_under(home, abs)?;
    match door.peek(home, &rel).await {
        Ok(p) => Ok(p.text.map_or(Look::Absent, Look::Text)),
        Err(said) => Ok(match door.stat_kind(abs).await? {
            None => Look::Absent,
            Some(_) => Look::Unreadable(said),
        }),
    }
}

/// ② **唯一的副作用**：把 [`render`] 的产物整份写进别名文件。给了 `rc` ⇒ **只查**它接没接上（`71 §6.1`：不代装）。
/// 有一条不合格 ⇒ **整批不写**（写一半的别名文件是最坏的结局：它 source 得进去，少了的没人发现）。
///
/// 〔RW1 · 第四波 09-24〕两处写都经 `door`（生产 = 本机后端的文件管理那一面）；home 也问它
/// （写落在后端认的那个 home 底下，两边的 home 不许各算各的）。
/// 〔AL2 · 第四波 4D〕门按 `origin` 取（本机 / 远端同一个函数）；`origin` 在这里只进 `rc` 那一道围栏（符号链接那步只对本机）。
pub async fn install_in<D: Door>(
    door: &D,
    origin: &Origin,
    aliases: &[Alias],
    rc: Option<&str>,
    shell: Shell,
) -> Result<AliasInstallReport, String> {
    let r = render(aliases, shell, origin);
    if !r.problems.is_empty() {
        let why: Vec<String> = r
            .problems
            .iter()
            .map(|p| format!("{}：{}", p.name, p.message))
            .collect();
        return Err(copy_text(
            "rsAccountAliases.install.invalid",
            &[
                ("count", &(why.len()).to_string()),
                (
                    "list",
                    &(why.join(&copy_text("rsAccountAliases.install.listSep", &[]))).to_string(),
                ),
            ],
        ));
    }
    let home = door.home().await?;
    let path = alias_file_in(&home, shell);
    // 先查 rc（只读）再写：rc 路径过不了围栏 ⇒ 整趟停下、一个字节不写（同「有一条不合格整批不写」）。
    let rc_note = match rc {
        None => None,
        Some(rc_raw) if rc_sources_our_file(door, origin, &home, rc_raw).await? => Some(copy_text(
            "rsAccountAliases.install.sourceExists",
            &[("rc", &rc_raw.to_string())],
        )),
        Some(rc_raw) => {
            let line = shell.dialect().source_line(&path);
            Some(copy_text(
                "rsAccountAliases.install.sourceMissing",
                &[("rc", &rc_raw.to_string()), ("line", &line.to_string())],
            ))
        }
    };
    let wrote_alias_file = write_alias_file(door, &home, shell, &r.code).await?;
    let mut notes = Vec::new();
    if !wrote_alias_file {
        notes.push(copy_text("rsAccountAliases.install.unchanged", &[]));
    }
    notes.extend(rc_note);
    notes.push(copy_text(
        "rsAccountAliases.install.nextStep",
        &[("path", &path)],
    ));
    Ok(AliasInstallReport {
        alias_path: path,
        wrote_alias_file,
        notes,
    })
}

/// 这个名字是不是已经被占了。**只出声、不拦** —— 见 `§0c 问三`。怎么查由方言答（POSIX 查自带片段与 `PATH`；
/// PowerShell 查终端集成模板与 `PATH` 上的 `.exe` / `.cmd` / …）。
///
/// 〔AL2 · 第四波 4D〕`PATH` 那一格查的是 **monitor 这个进程**的 `PATH`（`shell_dialect.rs::on_path` 头注自认会漏报）——
/// 拿去说远端是**错的**（会把本机的 `/usr/bin/foo` 报成远端撞名）⇒ 只在本机查；远端只查自带别名块，
/// 界面远端那一块多一句静态说明（`第四波记录/W5-ALIAS.md §2.2`，不为这一格加帧命令）。
pub fn collision_note(name: &str, shell: Shell, origin: &Origin) -> Option<String> {
    shell.dialect().name_taken(name, origin.is_local())
}

/// 候选启动文件的现状。列哪几份由方言答（POSIX 只列在的；PowerShell 的 `$PROFILE` 不在也列）——
/// 🔴 〔AL1d〕**`$PROFILE` 在哪，全仓只有 `ShellDialect::startup_candidates` 答**（`AL1d.md §2.3`）。
/// `extra` 是人另指的那一份（已过围栏；不在也列），与方言给的重了就不重复列。
///
/// 每份读**一次**：别名文件那一行接没接上（`sourced`）与别名块的现状（`block`）出自同一次读。
/// 〔AL2 · 第四波 4D〕那一次读问**那台机器的后端**（[`look`]）：方言只给路径与列法（`shell_dialect::Listed`），
/// 在不在、里面是什么、PS 7 的目录在不在（`files-stat`）都由门答。读不了的那一份照列、带后端原话（`unreadable`）。
pub async fn rc_candidates_via<D: Door>(
    door: &D,
    home: &str,
    shell: Shell,
    extra: Option<&str>,
) -> Result<Vec<StartupFile>, String> {
    let d = shell.dialect();
    let mut cands = d.startup_candidates(home);
    if let Some(x) = extra {
        if !cands.iter().any(|c| c.path == x) {
            cands.push(crate::shell_dialect::StartupCandidate {
                path: x.to_string(),
                listed: Listed::Always,
            });
        }
    }
    let mut out = Vec::new();
    for c in cands {
        if let Listed::IfDirExists(dir) = &c.listed {
            if door.stat_kind(dir).await?.as_deref() != Some("dir") {
                continue;
            }
        }
        let (text, exists, unreadable) = match look(door, home, &c.path).await? {
            Look::Text(t) => (Some(t), true, None),
            Look::Absent if c.listed == Listed::IfFileExists => continue,
            Look::Absent => (None, false, None),
            Look::Unreadable(e) => (None, true, Some(e)),
        };
        out.push(StartupFile {
            sourced: text.as_deref().is_some_and(|s| d.sources_our_file(s)),
            exists,
            block: text
                .as_deref()
                .map(|t| crate::profile_installer::block_state(Path::new(&c.path), t))
                .unwrap_or_default(),
            unreadable,
            path: c.path,
        });
    }
    Ok(out)
}

/// 把生成文件写下去。**内容一致就一个字节都不写。**
///
/// 〔RW1 · 第四波 09-24〕经 `door`（本机后端）写：生成文件是**我们自己**的东西 ⇒ 不留备份文件；
/// `~/.cc-monitor` 还不在就逐级补（`parents`）。回读 · 回滚那一份规则住后端。
/// 〔AL1c〕落盘那一份按方言编码（PowerShell 加 BOM）。
async fn write_alias_file<D: Door>(
    door: &D,
    home: &str,
    shell: Shell,
    content: &str,
) -> Result<bool, String> {
    let d = shell.dialect();
    let disk = d.encode_for_disk(content);
    let done = crate::user_files::edit(door, home, d.our_alias_file_rel(), false, true, |_| {
        Ok(Some(disk.clone()))
    })
    .await?;
    Ok(matches!(done, crate::user_files::Edited::Written(_)))
}

/// 用户指定的那份启动文件**接没接上**我们那份别名文件。**只读**（〔TL1 · 4C〕从前这里是代装那一行的 `ensure_…` 一跳，退役）。
///
/// 路径过 `profile_installer::fence_on`（只许落在 home 之内 —— 同一道围栏，不另立一份）；读经 `door`
/// （生产 = 那台机器后端的 `files-peek`）。这份文件是哪种 shell 由**它自己**（扩展名）定；「接上了」由那种方言认
/// （任何一种写法都认，别按整行比 —— POSIX 别名块那一行写的是 `$HOME/…`，没展开）。文件不在 ⇒ 没接上。
async fn rc_sources_our_file<D: Door>(
    door: &D,
    origin: &Origin,
    home: &str,
    rc_raw: &str,
) -> Result<bool, String> {
    let path = crate::profile_installer::fence_on(origin, home, rc_raw)?;
    let rel = crate::user_files::rel_under(home, &path)?;
    let d = Shell::of_target(Path::new(&path)).dialect();
    let got = door.peek(home, &rel).await?;
    Ok(got
        .text
        .as_deref()
        .is_some_and(|raw| d.sources_our_file(d.decode_from_disk(raw))))
}

#[cfg(test)]
#[path = "../../../tests/bridge/account_aliases_tests.rs"]
mod tests;
