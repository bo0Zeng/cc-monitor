//! 别名：**名字 ＋ 一组 ccm 参数**。一类，没有「账号别名」这一种。
//!
//! # 住进**那台机器的后端**
//!
//! 从前是 monitor 的 `account_aliases.rs`：规则与方言在 monitor 一份、事实经那台后端问。今天整族（本文件 ·
//! [`dialect`] · [`block`] · [`fence`] · [`form`]）在那台后端里：读、算、经它自己的文件管理面写，界面经通道直问（`aliases-*` 八条帧命令，
//! 下面 `answer_*`）。「本机」与「远端」对后端没有区别（本机＝不走 ssh 的远端），`origin` 这一维随之退役：
//! 方言那道闸改问这台自己（[`dialect_here`]）、`PATH` 撞名查这台自己的 `PATH`、围栏的符号链接那一步量这台自己的盘。
//! 已握手的终端数（`boundTerminals`）不是这台盘上的事实，住 monitor 进程里 ⇒ 不在这里的成品里（界面另问 monitor）。
//!
//! 〔用 2026-09-17〕逐字：「**不要有 account alias 这种东西。alias 应该独立吗？应该就是 ccm
//! 参数附加器。即生成别名，都是调用 ccm，ccm 本身就可以指定账号。**」
//!
//! # 来历（`K-R49`）
//!
//! 本模块起于 09-10 那一句「**添加账号后添加对应命令, 像是 alphacc, betacc 这种……还得手动去改**」：
//! 那时它按**账号表**整份重写一份文件，TS 侧的 `buildAliasLine`〔散文墓碑〕拼行、这里只管落盘
//! ＋ 一道「形状围栏」挡注入。两处都翻了：清单归**用户**（不再是账号表的投影），
//! shell 文本由**本模块**渲染（前端递的是结构，不是代码 —— 审计 S-1 那条
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
//! 文件名从 `account-aliases.sh` 改成 `aliases.sh`，
//! **不留兼容** —— 不留转发件、不读旧名；旧文件留在盘上，本模块不读、不写、不删。
//!
//! # 🔴 接上这份文件的那一行 source **只住别名块里**（「source 那一行只许一处装」）
//!
//! 两种方言的别名块都自带那一行（POSIX：`src/shared/ccm-aliases.sh` 最后一行；PowerShell：`src/shared/cc.ps1.tpl`
//! 结尾那一行，补上 —— 从前那一侧不带，两边不对称），文件不在就什么都不做 ⇒ 装了别名块的人什么都不用做。
//! 本模块**不再往 rc 里写那一行**：从前 [`install_in`] 会把它装进人指定的那份 rc（另一对围栏），
//! 与别名块那一行是同一件事的第二个写处（`AL1d.md §5` 第 4 条）。今天那一步**降级成检查**：选了 rc 就看它接没接上，
//! 没接上就说「装上别名块就接上了」并给出那一行、由人自己决定（提供检测，不代装）。
//! 盘上已有的旧围栏那一块不读、不删、不写清理代码（两处 source 同一份文件是幂等的，只多点一次）。
//! 别名文件的写**不在本进程落盘**：经本机后端的文件管理那一面（`user_files::edit` → `files-peek` / `files-put`），
//! 备份 · 原子替换 · 回读 · 回滚那一份规则住后端。

//!
//! # 本模块是**通用层**：只持有结构与规则，不持有任何一种 shell 的文本
//!
//! 「这个 shell 怎么写 / 文件落在哪 / 名字怎么认」全在 `shell_dialect.rs`（POSIX 与 PowerShell
//! 各一份实现）；这里留的是**判定的规则**（组合规则 · 能力闸 · 重名 · 有一条不合格整批不写）。
//! 三条命令各带一个 `shell`：同一份清单，POSIX 落 `~/.cc-monitor/aliases.sh`、PowerShell 落
//! `~/.cc-monitor/aliases.ps1`，各自由那个 shell 的别名块里那一行 source 接上。

use copy_core::copy_text;
use serde_json::{json, Value};
use std::path::Path;

pub(crate) mod block;
pub(crate) mod fence;
pub(crate) mod form;

use crate::assets::door::{self, Door};
use crate::control::ccm::argv::flag;
// 方言住后端 OS 适配层（原住本目录）。
use crate::platform::child::Deadline;
use crate::platform::shell::dialect::{self, Listed, RestTo, Shell};

/// `aliases-read` 整条命令总期限的上限（每一代 PowerShell 问一次执行策略，最多两发）。
pub(crate) const ALIASES_READ_CAP: Deadline = Deadline::secs(28);
/// `powershell-policy-set` 整条命令总期限的上限（设一发 ＋ 再问一发）。
pub(crate) const POLICY_SET_CAP: Deadline = Deadline::secs(28);

/// 交给方言的那条调用形状：`ccm` 那个词与 `--` 分界住 `control::ccm`，适配层不往上够 ⇒ 由这里交下去。
pub(crate) const CALL: dialect::Call<'static> = dialect::Call {
    word: crate::control::ccm::SUBCOMMAND_WORD,
    end: flag::END,
};

// 墓碑：这里从前有一对**写进用户 rc / `$PROFILE`** 的围栏常量（`# === cc-monitor aliases BEGIN v1 ===` 那一对），
//   包着 [`install_in`] 代装的那一行 source。那一步退役，这对围栏随之删 —— 盘上已有的那一块不读不删。

/// 生成文件自己的围栏（整份重写，所以它只是给人看的边界；两种方言共用，理由同上）。
/// v1 → v2：`71` 逐字「不要有 account alias 这种东西」—— 文件里只有**一类**别名。
/// 读回（[`read_in`]）不认这两行（注释行一律跳过），所以盘上那份 v1 照样读得回来。
const FILE_BEGIN: &str = "# === cc-monitor aliases BEGIN v2 ===";
const FILE_END: &str = "# === cc-monitor aliases END ===";

/// 一份候选启动文件（rc / `$PROFILE`）的状态。
///
/// 从前叫 `AccountAliasRc`，只答「接没接上别名文件」；别名块（`cc` / `cct` ·
/// `__ccm_bind`）装进的是**同一批**文件，却另有一条命令、另一份候选（`AL1d.md §1.2`）。
/// 今天一份候选、一次扫描，两件事都在这一格上（`block`）。
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StartupFile {
    /// 绝对路径。
    pub path: String,
    /// 这份文件今天已经把生成文件 `source` 进去了吗（装过 ccm 别名块的人这一格就是 true）。
    pub sourced: bool,
    /// 这份文件在不在盘上。POSIX 只列在的（恒 true）；PowerShell 的 `$PROFILE` 常常要装的时候才建。
    pub exists: bool,
    /// 这份文件里别名块的现状（不在盘上 ⇒ 全空）。
    pub block: block::BlockState,
    /// 在盘上、可那台后端读不了它（非 UTF-8 · 太大 · 解到 home 外 · I/O）—— 后端原话；`None` = 读得了或不在。
    /// 从前本机直读时这一形被吞成「没有别名块」。
    pub unreadable: Option<String>,
    /// 加载它的那一代 PowerShell 的执行策略（现问；块装在这里它会不会跑）。POSIX 与人另指的那一份 ⇒ `None`。
    pub policy: Option<crate::platform::shell::powershell::ExecPolicy>,
}

/// 生成文件的绝对路径。`home` 由调用方给 —— 测试拿临时目录当 home，**绝不碰真实家目录**。
///
/// **逐段**拼：`our_alias_file_rel` 是 `/` 分隔的（交给后端的 `rel` 就是它，那一侧不动），
/// 而这里拼的是**给人看、也写进 `$PROFILE` 那一行**的那台机器上的绝对路径。整串一次拼在 Windows 上
/// 会得到 `C:\Users\user\.cc-monitor/aliases.ps1`（真机读数，`RT1.md §8` F8）—— 两种分隔符混着。
/// `home` 是**那台机器**的后端答的字符串，拼法跟它自己的分隔符走（`user_files::join_under`）：
/// 从前 `Path::join` 用的是 monitor 这台的分隔符 —— Windows 上的 monitor 拼远端 `/home/user` 会得到 `/home/user\.cc-monitor\aliases.sh`。
pub(crate) fn alias_file_in(home: &str, shell: Shell) -> String {
    door::join_under(home, shell.dialect().our_alias_file_rel())
}

/// 整份生成文件的内容（**编码前**：BOM 那一层在落盘那一跳按方言加）。
/// **没有时间戳** —— 有了就永远比不出「内容没变」，每次都要写一遍。
pub(crate) fn render_file(shell: Shell, lines: &[String]) -> String {
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
// **别名只有一类 —— 名字 ＋ 一组 ccm 参数**
// ═══════════════════════════════════════════════════════════════════════════
//
// 用户 2026-09-17 逐字：「不要有 account alias 这种东西……就是 ccm 参数附加器」。
// 账号（`--account`）只是参数里的一个维度。清单由**用户**拥有，不跟着账号表自动增删。
//
// 命令面是两跳：
//   ① [`render`] —— **纯**：清单 → 代码（＋ 每条的问题 ＋ 撞名提示）。预览、复制都只调这一跳；
//   ② [`install_in`] —— **唯一的副作用**：同一份渲染落进我们自己那份别名文件（经那台机器的后端），
//      可选地往用户选的启动文件里装一行 `source`。它收的是**清单**不是代码 —— 写进 shell 的文本
//      只由 `shell_dialect` 产出（审计 S-1：绝不让前端注入可执行的 shell），而「写的就是预览的那一份」
//      由两跳调同一个 [`render`] 保证。
// 读回口：[`read_in`] 把盘上那份解析回清单（那张「没有的」表第一条）。

/// 一条别名。`args` 是原样的 ccm argv（`["--ccm-tmux", "--account", "z"]`），渲染时由方言逐个按需加引号。
/// `rest_to`：调用时跟的词交给谁（缺省 claude；交给 ccm 只许接回会话那一形，见 [`check_alias`]）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Alias {
    pub name: String,
    pub args: Vec<String>,
    pub rest_to: RestTo,
}

impl Alias {
    /// 调用时的词交给 claude 的那一形（绝大多数）。
    pub(crate) fn new(name: &str, args: Vec<String>) -> Alias {
        Alias {
            name: name.to_string(),
            args,
            rest_to: RestTo::Agent,
        }
    }
}

/// 一条别名的问题（进不了代码的那一条为什么进不了）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct AliasProblem {
    pub name: String,
    pub message: String,
}

/// ① 那一跳的产物。
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AliasRender {
    /// 整份文件（写入那一跳原样落盘的就是它 —— PowerShell 多一个 BOM；手贴的人复制它或 `lines`）。
    pub file_text: String,
    /// 每条合格别名的写法，按清单顺序（POSIX 一行；PowerShell 一个函数块，含换行）。
    pub lines: Vec<String>,
    /// 不合格的那几条。**非空时 [`install_in`] 一个字节都不写**（fail-closed）。
    pub problems: Vec<AliasProblem>,
    /// 名字撞了的提示，一条一句。**只出声、不拦**（`cc` 在多数机器上是 C 编译器，盖不盖由人定）。
    pub collisions: Vec<String>,
}

/// 读回清单里一条的归组：参数恰是「某号」或「某号 ＋ tmux」⇒ 账号那一组（`account` 是号名）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountShape {
    pub account: String,
    pub tmux: bool,
}

/// 账号表里一个号缺的那一条：点一下就把 `alias` 加进清单。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MissingAlias {
    pub account: String,
    pub tmux: bool,
    pub alias: Alias,
}

/// 读回口的产物。
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AliasListing {
    pub alias_path: String,
    /// 那份文件在不在。不在 ≠ 读失败（读失败是 `Err`）。不在时 `aliases` 是首建时会带上的那几条。
    pub exists: bool,
    pub aliases: Vec<Alias>,
    /// 与 `aliases` 逐条对应：账号那一形 ⇒ `{account, tmux}`，其余 ⇒ `null`。界面照这一格分组，不自己认。
    pub groups: Vec<Option<AccountShape>>,
    /// 这台的账号表（具名号，按账号库的顺序）。
    pub accounts: Vec<String>,
    /// 账号表里的号缺哪一条（没有 tmux 的目标只看 `<号>cc`）。
    pub missing: Vec<MissingAlias>,
    /// 盘上那份别名文件的指纹（不在 ⇒ `null`）。存的时候交回来：盘上被别处改过 ⇒ 不写。
    pub fingerprint: Option<String>,
    /// 解析不回清单的那几行（原文 ＋ 原因）。**不静默丢**：写回去之前人得知道它们会没。
    pub unparsed: Vec<String>,
    /// 这台机器上这种 shell 的启动文件候选（「那一行 source 加进哪份」·「别名块装进哪份」，同一批）。
    pub rc_candidates: Vec<StartupFile>,
    // 墓碑：这里从前有一格「已握手的终端数」（`BindRegistry`，住 monitor 进程里）—— 不是这台盘上的事实 ⇒ 不进后端成品，界面另问 monitor。
    /// 人另指的那一份（`read_in` 的 `extra_rc`）过了围栏之后的绝对路径 —— 界面拿它在候选里认出「刚指的是哪一份」。
    pub other_rc: Option<String>,
}

/// ② 那一跳的产物。
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AliasInstallReport {
    pub alias_path: String,
    pub wrote_alias_file: bool,
}

/// 别名文件第一次被建出来时带上的那几条（之后和别的别名一样可改可删、删了不回补）：
/// `cc`（当前目录起）· `cct`（tmux 里起）· `cca`（接回 tmux 会话，会话名敲的时候跟上）。没有 tmux 的目标只带 `cc`。
pub(crate) fn first_aliases(shell: Shell) -> Vec<Alias> {
    let mut v = vec![Alias::new("cc", Vec::new())];
    if Caps::of(shell).tmux {
        v.push(Alias::new(
            "cct",
            vec![flag::END.to_string(), flag::TMUX.to_string()],
        ));
        v.push(Alias {
            name: "cca".to_string(),
            args: vec![flag::END.to_string(), flag::ATTACH.to_string()],
            rest_to: RestTo::Ccm,
        });
    }
    v
}

/// 盘上那份（`None` = 不在）→ 清单的起点 ＋ 认不出的行。不在 ⇒ 首建那几条（[`first_aliases`]）。
fn starting_list(shell: Shell, text: Option<&str>) -> (Vec<Alias>, Vec<String>) {
    let Some(text) = text else {
        return (first_aliases(shell), Vec::new());
    };
    let dia = shell.dialect();
    let mut aliases = Vec::new();
    let mut unparsed = Vec::new();
    for got in dia.parse_file(CALL, dia.decode_from_disk(text)) {
        match got {
            Ok((name, args, rest_to)) => {
                let a = Alias {
                    name,
                    args,
                    rest_to,
                };
                match check_alias(&a, shell) {
                    Ok(()) => aliases.push(a),
                    Err(why) => unparsed.push(format!("{}（{why}）", render_line(&a, shell))),
                }
            }
            Err(raw) => unparsed.push(raw),
        }
    }
    (aliases, unparsed)
}

/// 别名文件的指纹（盘上原样的字节：长度 ＋ FNV-1a 64）。不在 ⇒ `None`。
/// 只用来认「读回之后盘上那份变没变」（写那一跳另有 CAS），不是摘要形 —— 不借文件管理面那一份 SHA-256（那是它的内部）。
fn fingerprint_of(text: Option<&str>) -> Option<String> {
    text.map(|t| {
        let h = t.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
        });
        format!("{}-{h:016x}", t.len())
    })
}

/// 清单里每条的归组（账号那一形住 `accounts::manage::aliases::shape_of`）。
fn groups_of(aliases: &[Alias]) -> Vec<Option<AccountShape>> {
    aliases
        .iter()
        .map(|a| {
            crate::accounts::manage::aliases::shape_of(&a.args, a.rest_to)
                .map(|(account, tmux)| AccountShape { account, tmux })
        })
        .collect()
}

/// 账号表里每个号缺哪一条（同一形的不论名字都算有）。
fn missing_of(aliases: &[Alias], accounts: &[String], shell: Shell) -> Vec<MissingAlias> {
    use crate::accounts::manage::aliases as acct;
    let have = groups_of(aliases);
    let kinds: &[bool] = if Caps::of(shell).tmux {
        &[false, true]
    } else {
        &[false]
    };
    let mut out = Vec::new();
    for account in accounts {
        for &tmux in kinds {
            let present = have
                .iter()
                .flatten()
                .any(|g| g.account == *account && g.tmux == tmux);
            if present {
                continue;
            }
            if let Some(name) = acct::alias_name(account, tmux) {
                out.push(MissingAlias {
                    account: account.clone(),
                    tmux,
                    alias: Alias::new(&name, acct::alias_args(account, tmux)),
                });
            }
        }
    }
    out
}

/// 这台的账号表（具名号）。没有账号库 / 读不了 ⇒ 空（账号页自己会说读不了）。
fn account_table(home: &str) -> Vec<String> {
    crate::accounts::manage::mcp_share_exec::accounts_in(home)
        .ok()
        .flatten()
        .map(|v| v.into_iter().map(|(name, _)| name).collect())
        .unwrap_or_default()
}

/// 能进别名的 ccm 词（`new` 与壳层选项）。其余 ccm 认得的词（`--ccm-*` 诊断口 · 起会话身份 · 账号目录）每次取值都不同，
/// 做成固定别名没意义 ⇒ 不收。一个词跟几个值、合不合法只问 ccm 自己的解析器（`control/ccm/argv.rs`），这里只列「许不许进别名」。
/// 它们只许写在最后一个 `--` 右边；左边的词（`--model` · `--resume` · `-p` …）是交给 claude 的，原样放行。
///
/// ⚠ 每一个旗标都得是后端 `ccm --help` 里真有的那个词 —— 判据
/// `aliases_tests.rs::every_alias_flag_is_a_real_ccm_flag` 去用法文本里对（异源）。
/// `--attach` 的会话名在别名里不写、调用时现给（只许「调用时的词交给 ccm」那一形，[`check_alias`]）。
pub(crate) const ALIAS_FLAGS: &[&str] = &[
    flag::NEW,
    flag::CWD,
    flag::CWD_IF,
    flag::ACCOUNT,
    flag::BASE,
    flag::TMUX,
    flag::TMUX_BASE,
    flag::AGENT,
    flag::LAUNCHER,
    flag::TMUX_SIZE,
    flag::DETACH,
    flag::BUS_REGISTER,
    flag::BUS_NOTE,
    flag::ATTACH,
];

/// **载体是 tmux 的那几个旗标**（`--ccm-tmux=<名>` 是 `--ccm-tmux` 的内联形，一并算）。
/// 这台机器没有 tmux ⇒ 它们一个都不许进别名（生成出来就是一条当场 `no_tmux` 的别名）。
///
/// 🔴 它**不是**本模块自己的判断：事实源是 `control/ccm/mod.rs::CCM_TMUX_CARRIED`（靠 tmux 活着的 ccm 能力）。
/// 判据 `aliases_tests.rs::the_tmux_gate_is_exactly_the_backends_tmux_carried_flags`
/// 读那张表的原文、按「本表 == 那张表 ∩ [`ALIAS_FLAGS`]」两向相等钉着。
pub(crate) const NEEDS_TMUX: &[&str] = &[
    flag::ATTACH,
    flag::TMUX,
    flag::TMUX_BASE,
    flag::TMUX_SIZE,
    flag::DETACH,
    flag::BUS_REGISTER,
];

/// 这台机器的**能力**—— 不是方言（`has_tmux()` 不进那一族）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Caps {
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

/// **这台后端说不说这种方言**（「显式拒绝，不许静默推一份跑不起来的东西」）。
///
/// 事实只有一个：**PowerShell ⇔ Windows**（与 [`Caps::of`] 同一条）；POSIX 在三种 OS 上都有人说（Windows 上是 Git Bash）。
/// ⇒ 这台后端不在 Windows 上 ⇒ PowerShell 形拒。从前（住 monitor）按 `origin` 判「远端 × PowerShell 拒」；
/// 规则进了那台后端之后它不知道自己是不是「远端」，按这台自己的平台判
/// （本机＝不走 ssh 的远端，原意即远端只承诺 Linux）⇒ **本机 Linux 选 `.ps1` 那一形由收变拒**。
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

/// 一条别名合不合格。一条别名就是一条 `ccm` argv（`<交给 claude 的…> -- <ccm 自己的…>`）：
/// 词怎么分组、组合合不合法只问 ccm 自己的解析器（`argv::word_at` / `apply_word` / `finish`），这里不另写一份 ccm 文法。
/// 别名自己的规则只有这几条：名字 · 控制字符与方言能不能原样传 · 能力闸（没有 tmux 的目标）· 许不许进别名 ·
/// 目录要钉死 · 调用时跟的词交给谁。
pub(crate) fn check_alias(a: &Alias, shell: Shell) -> Result<(), String> {
    use crate::control::ccm::argv::{self, Die};
    let d = shell.dialect();
    let caps = Caps::of(shell);
    if !d.name_is_valid(&a.name) {
        return Err(copy_text("rsAccountAliases.check.badName", &[]));
    }
    let (left, right) = match argv::last_end(&a.args) {
        Some(k) => (&a.args[..k], &a.args[k + 1..]),
        None => (&a.args[..], &a.args[..0]),
    };
    for w in left.iter().chain(right) {
        if w.chars().any(char::is_control) {
            return Err(copy_text("rsAccountAliases.check.controlChar", &[]));
        }
        d.arg_is_passable(w)?;
    }
    // 调用时的词交给 ccm 只有一形：接回会话 `-- --attach "$@"`（会话名现场给）；别的 ccm 选项与它同用会被 ccm 静默忽略 ⇒ 不许。
    let attach_only = left.is_empty() && right.len() == 1 && right[0] == flag::ATTACH;
    let attach_bad = match a.rest_to {
        RestTo::Ccm => !attach_only,
        RestTo::Agent => right.iter().any(|w| w == flag::ATTACH),
    };
    // ccm 看到的右边：调用时交给 ccm 的那一形，补上调用时跟的那个词。
    let mut seen: Vec<String> = right.to_vec();
    if a.rest_to == RestTo::Ccm {
        seen.push("x".to_string());
    }
    let mut o = argv::blank_opts(left);
    let mut i = 0;
    while i < seen.len() {
        let g = argv::word_at(&seen, i).map_err(|Die(m)| m)?;
        // 能力闸（「`cct` 在 Windows 上没有」从硬编码变成能力查询）。
        if !caps.tmux && NEEDS_TMUX.contains(&g.flag) {
            return Err(copy_text(
                "rsAccountAliases.check.noTmux",
                &[(
                    "flags",
                    &NEEDS_TMUX
                        .iter()
                        .map(|f| format!("`{f}`"))
                        .collect::<Vec<_>>()
                        .join(" / "),
                )],
            ));
        }
        if !ALIAS_FLAGS.contains(&g.flag) {
            return Err(copy_text(
                "rsAccountAliases.check.notAllowed",
                &[("word", &seen[i])],
            ));
        }
        // 别名要钉住一个目录：`~` 打头或绝对（ccm 运行时同样拒相对的 `--cwd`，`INVARIANTS §47`）。
        if g.flag == flag::CWD && !g.vals.iter().all(|v| pinned_dir_ok(v, shell)) {
            return Err(copy_text(
                "rsAccountAliases.check.cwdNotAbsolute",
                &[("value", &g.vals.join(" "))],
            ));
        }
        if let Some(v) = (g.flag == flag::CWD_IF)
            .then(|| g.vals.iter().find(|v| !pinned_dir_ok(v, shell)))
            .flatten()
        {
            return Err(copy_text(
                "rsAccountAliases.check.cwdIfNotPinned",
                &[("value", v)],
            ));
        }
        argv::apply_word(&mut o, &g, i).map_err(|Die(m)| m)?;
        i += g.len;
    }
    if attach_bad {
        return Err(copy_text("rsAccountAliases.check.attachAlone", &[]));
    }
    argv::finish(o).map(|_| ()).map_err(|Die(m)| m)
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

/// 一条（合格的）别名在这种 shell 里的写法（方言那一份的薄包装）。
pub(crate) fn render_line(a: &Alias, shell: Shell) -> String {
    shell
        .dialect()
        .render_alias(CALL, &a.name, &a.args, a.rest_to)
}

/// ① **纯**：清单 → 代码。一个字节都不写、一个文件都不读（撞名检查读的是自带片段 / 模板与 `PATH`）。
///
/// `origin` 退役：规则住那台后端，撞名那一格查的就是那台自己的 `PATH`（[`collision_note`]）。
pub(crate) fn render(aliases: &[Alias], shell: Shell) -> AliasRender {
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
        .filter_map(|a| collision_note(&a.name, shell))
        .collect();
    AliasRender {
        file_text: render_file(shell, &lines),
        lines,
        problems,
        collisions,
    }
}

/// **读回口**：盘上那份别名文件 → 清单 ＋ 启动文件候选（各带别名块的现状）。只读。
///
/// `extra_rc`：人在界面上指的「其它文件」（从前是终端集成那一块的「自定义路径」）。给了就过
/// `profile_installer::fence_on`（只许落在 home 之内）、并进候选一起扫；过不了围栏 ⇒ `Err`。
///
/// **事实全问这台的文件管理面**（「怎么读到这个事实 → 下沉」）：
/// home（`files-home`）· 别名文件（`files-peek`）· 候选各一次（`files-peek`，PS 7 那两份的目录 `files-stat`）。
/// 门就是本进程的 `files-*`（[`door`]），`origin` 退役。
pub(crate) fn read_via(
    d: &dyn Door,
    shell: Shell,
    extra_rc: Option<&str>,
) -> Result<AliasListing, String> {
    let home = door::home(d)?;
    let extra = extra_rc.map(|raw| block::fence(&home, raw)).transpose()?;
    let path = alias_file_in(&home, shell);
    let text = match look(d, &home, &path)? {
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
    let (aliases, unparsed) = starting_list(shell, text.as_deref());
    let accounts = account_table(&home);
    let names: Vec<String> = aliases.iter().map(|a| a.name.clone()).collect();
    Ok(AliasListing {
        alias_path: path,
        exists,
        groups: groups_of(&aliases),
        missing: missing_of(&aliases, &accounts, shell),
        accounts,
        fingerprint: fingerprint_of(text.as_deref()),
        aliases,
        unparsed,
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

/// 先 `files-peek`；它报错时再问一次 `files-stat`：那台答「这个路径读不到」⇒ 当不在（`files-peek` 在**父目录不在**时
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

/// 写别名文件没成：盘上那份在读回之后被别处改过（`Stale`，界面重读再让人存），或别的原因（原话）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InstallErr {
    Stale(String),
    Refused(String),
}

/// ② **唯一的副作用**：把 [`render`] 的产物整份写进别名文件。
/// 有一条不合格 ⇒ **整批不写**（写一半的别名文件是最坏的结局：它 source 得进去，少了的没人发现）。
/// `fingerprint` 是读回时那份的指纹（[`AliasListing::fingerprint`]）：盘上此刻不是那一份 ⇒ 一个字节不写、[`InstallErr::Stale`]。
///
/// 写经这台的文件管理那一面（[`door`]）；home 也问它。
pub(crate) fn install_in(
    d: &dyn Door,
    aliases: &[Alias],
    shell: Shell,
    fingerprint: Option<&str>,
) -> Result<AliasInstallReport, InstallErr> {
    let r = render(aliases, shell);
    if !r.problems.is_empty() {
        let why: Vec<String> = r
            .problems
            .iter()
            .map(|p| format!("{}：{}", p.name, p.message))
            .collect();
        return Err(InstallErr::Refused(copy_text(
            "rsAccountAliases.install.invalid",
            &[
                ("count", &(why.len()).to_string()),
                (
                    "list",
                    &(why.join(&copy_text("rsAccountAliases.install.listSep", &[]))).to_string(),
                ),
            ],
        )));
    }
    let home = door::home(d).map_err(InstallErr::Refused)?;
    let path = alias_file_in(&home, shell);
    let mut moved = false;
    let wrote = write_alias_file(d, &home, shell, |now| {
        if fingerprint_of(now).as_deref() != fingerprint {
            moved = true;
            return Err(copy_text(
                "rsAccountAliases.install.changedElsewhere",
                &[("path", &path)],
            ));
        }
        Ok(Some(r.file_text.clone()))
    });
    match wrote {
        Ok(wrote_alias_file) => Ok(AliasInstallReport {
            alias_path: path,
            wrote_alias_file,
        }),
        Err(e) if moved => Err(InstallErr::Stale(e)),
        Err(e) => Err(InstallErr::Refused(e)),
    }
}

/// 建号 / 删号那一刻改清单没成。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AmendErr {
    /// 文件里有这么多行认不出来 ⇒ 不动它（重写会把那几行丢掉）。
    Unparsed {
        path: String,
        n: usize,
    },
    Failed(String),
}

/// 改清单的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Amended {
    pub path: String,
    pub wrote: bool,
}

/// 建号 / 删号那一刻：读盘上那份（不在 ⇒ 首建那几条）→ `f` 改 → 写回，一次读改写（`stale` 重读重算）。
/// `f` 可能被叫不止一次（重读那一趟），它记的结果以最后一次为准。
pub(crate) fn amend_via(
    d: &dyn Door,
    shell: Shell,
    f: &mut dyn FnMut(Vec<Alias>) -> Vec<Alias>,
) -> Result<Amended, AmendErr> {
    let home = door::home(d).map_err(AmendErr::Failed)?;
    let path = alias_file_in(&home, shell);
    let mut unparsed = 0usize;
    let wrote = write_alias_file(d, &home, shell, |now| {
        let (list, bad) = starting_list(shell, now);
        unparsed = bad.len();
        if unparsed > 0 {
            return Err(String::new());
        }
        let r = render(&f(list), shell);
        if let Some(p) = r.problems.first() {
            return Err(format!("{}：{}", p.name, p.message));
        }
        Ok(Some(r.file_text))
    });
    match wrote {
        Ok(wrote) => Ok(Amended { path, wrote }),
        Err(_) if unparsed > 0 => Err(AmendErr::Unparsed { path, n: unparsed }),
        Err(e) => Err(AmendErr::Failed(e)),
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
/// 🔴 **`$PROFILE` 在哪，全仓只有 `ShellDialect::startup_candidates` 答**（`AL1d.md §2.3`）。
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
            path: c.path,
        });
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
    let done = door::edit(d, home, dia.our_alias_file_rel(), false, true, |now| {
        Ok(plan(now)?.map(|t| dia.encode_for_disk(&t)))
    })?;
    Ok(matches!(done, door::Edited::Written(_)))
}

// ═══════════════════════════════════════════════════════════════════════════
// 线上那八条：`aliases-render` · `-read` · `-install` · `-block-render` · `-block-install` · `-block-remove`
// ＋ 表单两向 `aliases-to-form` · `aliases-from-form`
// ═══════════════════════════════════════════════════════════════════════════

type Answer = Result<Value, (&'static str, String)>;

fn bad(msg: &str) -> (&'static str, String) {
    ("bad_args", crate::common::contract::malformed(msg))
}

fn shell_arg(args: &Value) -> Result<Shell, (&'static str, String)> {
    serde_json::from_value(args.get("shell").cloned().unwrap_or(Value::Null))
        .map_err(|_| bad("`shell` must be \"posix\" or \"powershell\""))
}

fn aliases_arg(args: &Value) -> Result<Vec<Alias>, (&'static str, String)> {
    serde_json::from_value(args.get("aliases").cloned().unwrap_or(Value::Null))
        .map_err(|_| bad("`aliases` must be [{name, args[]}]"))
}

fn str_arg<'a>(args: &'a Value, k: &str) -> Result<&'a str, (&'static str, String)> {
    args.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| bad(&format!("missing `{k}`")))
}

fn opt_str<'a>(args: &'a Value, k: &str) -> Result<Option<&'a str>, (&'static str, String)> {
    match args.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .map(Some)
            .ok_or_else(|| bad(&format!("`{k}` must be a string or null"))),
    }
}

fn refused(said: String) -> (&'static str, String) {
    ("refused", said)
}

fn to_value<T: serde::Serialize>(v: &T) -> Answer {
    serde_json::to_value(v).map_err(|e| ("io_failed", e.to_string()))
}

/// `aliases-render {aliases, shell}` → [`AliasRender`]（纯：一个字节都不写）。
pub(crate) fn answer_render(args: &Value) -> Answer {
    let shell = shell_arg(args)?;
    dialect_here(shell).map_err(refused)?;
    to_value(&render(&aliases_arg(args)?, shell))
}

/// `aliases-read {shell, rcPath?}` → [`AliasListing`]。
pub(crate) fn answer_read(d: &dyn Door, args: &Value) -> Answer {
    let shell = shell_arg(args)?;
    dialect_here(shell).map_err(refused)?;
    to_value(&read_via(d, shell, opt_str(args, "rcPath")?).map_err(refused)?)
}

/// `aliases-install {aliases, shell, fingerprint}` → [`AliasInstallReport`]。盘上被别处改过 ⇒ `stale`。
pub(crate) fn answer_install(d: &dyn Door, args: &Value) -> Answer {
    let shell = shell_arg(args)?;
    dialect_here(shell).map_err(refused)?;
    let list = aliases_arg(args)?;
    if !args
        .get("fingerprint")
        .is_some_and(|v| v.is_null() || v.is_string())
    {
        return Err(bad("missing `fingerprint` (string or null)"));
    }
    let fp = opt_str(args, "fingerprint")?;
    match install_in(d, &list, shell, fp) {
        Ok(r) => to_value(&r),
        Err(InstallErr::Stale(said)) => Err(("stale", said)),
        Err(InstallErr::Refused(said)) => Err(refused(said)),
    }
}

/// `aliases-to-form {alias}` → `{form}`（纯）：一条别名摊成表单那几格（[`form::to_form`]）。
pub(crate) fn answer_to_form(args: &Value) -> Answer {
    let a: Alias = serde_json::from_value(args.get("alias").cloned().unwrap_or(Value::Null))
        .map_err(|_| bad("`alias` must be {name, args[], restTo}"))?;
    Ok(json!({ "form": form::to_form(&a) }))
}

/// `aliases-from-form {form, orig}` → `{alias}`（纯）：表单拼回一条别名；`orig` 必给（正在改的那一条，新增 ⇒ `null`），
/// 没动过的那几组照它原样留（[`form::from_form`]）。一格文本的引号没配对 ⇒ `refused`。
pub(crate) fn answer_from_form(args: &Value) -> Answer {
    let f: form::AliasForm =
        serde_json::from_value(args.get("form").cloned().unwrap_or(Value::Null))
            .map_err(|_| bad("`form` must be the alias form"))?;
    let orig: Option<Alias> = match args.get("orig") {
        None => return Err(bad("missing `orig` (alias or null)")),
        Some(v) => serde_json::from_value(v.clone())
            .map_err(|_| bad("`orig` must be {name, args[], restTo} or null"))?,
    };
    let a = form::from_form(&f, orig.as_ref()).map_err(refused)?;
    Ok(json!({ "alias": a }))
}

/// 别名块那三条：`rcPath` 那份文件过围栏、方言按它的扩展名定、再过方言闸。
fn block_target(d: &dyn Door, args: &Value) -> Result<(String, String), (&'static str, String)> {
    let raw = str_arg(args, "rcPath")?;
    let home = door::home(d).map_err(refused)?;
    let p = block::fence(&home, raw).map_err(refused)?;
    dialect_here(Shell::of_target(Path::new(&p))).map_err(refused)?;
    Ok((home, p))
}

/// `aliases-block-render {rcPath}` → `{text}`（纯：往一份空文件里装一次会写成什么；方言由 `rcPath` 的扩展名定）。
pub(crate) fn answer_block_render(d: &dyn Door, args: &Value) -> Answer {
    let raw = str_arg(args, "rcPath")?;
    let shell = Shell::of_target(Path::new(raw));
    dialect_here(shell).map_err(refused)?;
    let home = door::home(d).map_err(refused)?;
    let code = block::render_block(shell, &home).map_err(refused)?;
    Ok(json!({ "text": code }))
}

/// `aliases-block-install {rcPath}` → `{}`（幂等，整块替换；经这台的 `files-put`）。
pub(crate) fn answer_block_install(d: &dyn Door, args: &Value) -> Answer {
    let (_, p) = block_target(d, args)?;
    block::install_to_profile(d, Path::new(&p)).map_err(refused)?;
    Ok(json!({}))
}

/// `aliases-block-remove {rcPath}` → `{}`（整块删，块外一个字节不动；围栏损坏 ⇒ 中止）。
pub(crate) fn answer_block_remove(d: &dyn Door, args: &Value) -> Answer {
    let (_, p) = block_target(d, args)?;
    block::uninstall_from_profile(d, Path::new(&p)).map_err(refused)?;
    Ok(json!({}))
}

/// `powershell-policy-set {host}` → `{policy, setError}`：那一代 PowerShell 当前用户那一档设成
/// `RemoteSigned`，再现问一次。只做这一件固定的事（不收策略值）；界面只在用户点了、确认了之后发。这台不说 PowerShell ⇒ 拒。
pub(crate) fn answer_policy_set(args: &Value) -> Answer {
    dialect_here(Shell::PowerShell).map_err(refused)?;
    let host: crate::platform::shell::PsHost =
        serde_json::from_value(args.get("host").cloned().unwrap_or(Value::Null))
            .map_err(|_| bad("`host` must be \"powershell\" or \"pwsh\""))?;
    let (policy, set_error) = crate::platform::shell::powershell::allow_local_scripts(host);
    Ok(json!({ "policy": policy, "setError": set_error }))
}

#[cfg(test)]
#[path = "../../../../tests/backend/assets/aliases/aliases_tests.rs"]
pub(crate) mod tests;
