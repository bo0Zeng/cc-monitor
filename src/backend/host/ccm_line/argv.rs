//! `ccm` 这套 argv 的唯一解析口与唯一默认值住址。
//!
//! - 解析只许在这一个文件里：`plan.rs` / `mod.rs` 一律拿 [`Opts`]，不许自己再看一眼 `args`
//!   （`ccm_flag_literals_live_only_in_argv_rs` 机检）。
//! - 每个参数的默认值只许写在 [`Defaults`] 里（`every_default_lives_only_in_the_defaults_block` 机检）：两份默认迟早分叉。
//! - 每个 `--flag` 的字面量只住在 [`flag`] 里；`plan.rs` 拼内层载荷时也从这里取。本文件是后端里唯一允许持有
//!   ccm 旗标字面量的地方（`protocol_doc_guard::TERMINAL_SURFACE_FILES` 登记并机检）。

use copy_core::copy_text;

/// 每个 `--flag` 的字面量，唯一住址。
///
/// ccm 是 claude 的壳：它只认下面这些（壳层选项 ＋ `--ccm-*` 诊断口 ＋ `--`），其余每个词原样交给 agent。
/// `--ccm-tmux` / `--ccm-agent` 带前缀：claude 自己有 `--tmux` / `--agent`。
pub mod flag {
    pub const TMUX: &str = "--ccm-tmux";
    /// `--hosted`：放进 cc-monitor 托管的终端（`host/managed`）。只有写了它才托管（`plan::container_host`；`cch` 那一段带它）。
    pub const HOSTED: &str = "--hosted";
    pub const TMUX_BASE: &str = "--tmux-base";
    pub const TMUX_SIZE: &str = "--tmux-size";
    pub const DETACH: &str = "--detach";
    pub const ACCOUNT: &str = "--account";
    pub const BASE: &str = "--base";
    pub const CWD: &str = "--cwd";
    /// `--cwd-if <在哪> <进哪>`：正好在「在哪」那个目录敲 ⇒ 进「进哪」（可写多次，按序第一条对上的算）。
    pub const CWD_IF: &str = "--cwd-if";
    pub const AGENT: &str = "--ccm-agent";
    pub const LAUNCHER: &str = "--launcher";
    pub const ATTACH: &str = "--attach";
    /// `--hosts`：这台的托管终端一行一个（`cch ls` 就是它，[`super::hosted_entry`]）。
    pub const HOSTS: &str = "--hosts";
    pub const CCM_PRINT: &str = "--ccm-print";
    pub const CCM_HELP: &str = "--ccm-help";
    pub const CCM_VERSION: &str = "--ccm-version";
    pub const CCM_PROBE: &str = "--ccm-probe";
    pub const CCM_SID: &str = "--ccm-sid";
    /// `--tab <show|hide>`：新会话进不进标签页（不给 ⇒ 照这台偏好，判定住 `history/session_hidden.rs`）。claude 没有同名旗标。
    pub const TAB: &str = "--tab";
    /// `--tab-from <别名>`：`--tab` 是哪个别名写的（ccm 往 tmux 里那一趟交下去用，那一趟的命令行里已经没有别名了）。
    pub const TAB_FROM: &str = "--tab-from";
    /// `--ccm-alias <别名>`：这一发是经哪条别名起的（ccm 往 tmux 里那一趟交下去用，同 `--tab-from`；不进配置）。
    pub const CCM_ALIAS: &str = "--ccm-alias";
    /// `--tab-group <名>`：起出来的会话进哪个标签页组（一组会话那一行没写组名时，组名缺省取别名合下来的它）。claude 没有同名旗标。
    pub const TAB_GROUP: &str = "--tab-group";
    /// `--say <那句>`：开口那句 —— 这一发落在 tmux 里、起新会话时交给 agent 当第一句（`plan::build_among`）。claude 没有同名旗标。
    pub const SAY: &str = "--say";
    /// `ccm -- --host-run <句柄> [键=值…] -- <命令…>`：托管终端的托管进程（`host/managed/run.rs`）。只许紧跟打头的 `--`，
    /// 由 ccm / 后端起托管终端时自己拼，不进配置。
    pub const HOST_RUN: &str = "--host-run";
    /// 直接给账号配置目录（说不出账号名的那一形：分叉继承源会话的目录）。
    pub const ACCOUNT_DIR: &str = "--account-dir";
    /// 这一发不轮换，只用起它的那个号（同 `session-open` 的 `rotation: {strategy: null}`）：起之前按这一发定好的会话ID记成「不轮换」。
    pub const NO_ROTATION: &str = "--no-rotation";
    /// 分隔符：最后一个 `--` 左边交 agent、右边归 ccm。
    pub const END: &str = "--";
    /// 位置动作「起新会话」：ccm 自己的词，只许是 `--` 右边的第一个词（`ccm [claude 的] -- new [ccm 选项]`）。
    /// 起新会话本来就是缺省，写出来是给想说清楚的人与渲染器用的；`ccm new`（没有 `--`）整行交 claude。
    pub const NEW: &str = "new";
}

/// 这个词是不是 ccm 自己认的（壳层选项 ＋ `--ccm-*` 诊断口）：解析器认右边那个词的同一处（[`right_word`]）。
/// 后端 CLI 面不许派生出这样的名字（`stream::cli::cli_exposed`）：后端的子命令集合要问这里，这里不许再问回去。
pub fn is_ccm_word(word: &str) -> bool {
    right_word(word).is_some()
}

/// `--` 右边这个词（`--flag=值` 只看等号前面）是 [`RIGHT_WORDS`] 里的哪一个。
fn right_word(a: &str) -> Option<&'static str> {
    let key = match a.find('=') {
        Some(p) if a.starts_with("--") && p > 2 => &a[..p],
        _ => a,
    };
    RIGHT_WORDS.iter().find(|w| **w == key).copied()
}

/// `--cwd` 的取值：`auto`（默认）或一个显式目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CwdSpec {
    Auto,
    Explicit(String),
}

/// 每个参数的默认值，唯一住址。
///
/// `--account` 不给 ⇒ 落 manifest 的默认账号；成对的另一半：调用方已经选好的号不许被静默换掉
/// （`plan::account_for` 的继承那一支）。
pub struct Defaults;

impl Defaults {
    /// 不给 `--agent` ⇒ 注册表里声明默认的那一家（`LaunchFace::is_default`）。
    pub fn agent() -> &'static str {
        crate::agents::default_kind()
    }
    /// 不给 `--cwd` ⇒ `auto`，而 `auto` 就是恒等：调用方自己的 cwd（`plan::resolve_cwd`；诚实的默认 = 沿用调用者状态）。
    pub(crate) const CWD: CwdSpec = CwdSpec::Auto;
    /// 不给 `--tmux` ⇒ 不进容器路。
    pub(crate) const USE_TMUX: bool = false;
    /// 不给 `--hosted` ⇒ 照在不在 tmux 里 · 这台能不能托管定（`plan::container_host`）。
    pub(crate) const HOSTED: bool = false;
    /// 不给 `--base` ⇒ 不是基座态。
    pub(crate) const USE_BASE: bool = false;
    /// 不给 `--detach` ⇒ 建完接进去。
    pub(crate) const DETACH: bool = false;
    /// 不给 `--ccm-print` ⇒ 真跑。
    pub(crate) const PRINT: bool = false;
    /// 不给 `--no-rotation` ⇒ 照「新会话用」那一份轮换。
    pub(crate) const NO_ROTATION: bool = false;
    // 工作目录不在这张表里：它的默认是恒等（上面 `CWD`），不替用户挑一个具体目录。
    // 账号库 manifest 的门牌号也不在：它是后端的家那一族（`plan::accts_manifest_under`），不是一个可以换的默认值。
    /// 起 agent 前要 eval 的机器级 env。
    pub const ENV: &'static str = "";
}

/// 解析出来的一整套意图。**下游只许读这个结构，不许再看一眼 `args`。**
#[derive(Debug, Clone)]
pub struct Opts {
    /// `--attach <名>`；空 = 没给（起会话）。
    pub attach_name: String,
    pub use_tmux: bool,
    /// `--hosted`。
    pub hosted: bool,
    pub tmux_name: String,
    pub tmux_base: String,
    /// 空串 = **没给** —— 它与「给了一个空值」在本 CLI 上同形（bash 那侧也是）。
    pub account: String,
    pub use_base: bool,
    pub cwd_spec: CwdSpec,
    /// `--cwd-if` 那几条 `(在哪, 进哪)`，按写的顺序；都没对上 ⇒ [`Opts::cwd_spec`]。
    pub cwd_if: Vec<(String, String)>,
    pub agent: String,
    pub launcher: String,
    pub ccm_sid: String,
    /// `--tab`；没给 ⇒ `None`。
    pub tab: Option<cmd_wire::session::Tab>,
    /// `--tab` 是哪个别名写的（合并那一处按来处填 · tmux 里那一趟由 `--tab-from` 交来）；空 ＝ 命令行当场给的 / 没给。
    pub tab_from: String,
    /// 这一发经哪条别名起的（合并那一处取最末那一层 · tmux 里那一趟由 `--ccm-alias` 交来）；空 ＝ 不经别名。
    pub alias: String,
    /// `--tab-group`；空 ＝ 没给。
    pub tab_group: String,
    /// `--say`；空 ＝ 没给。
    pub say: String,
    /// `--account-dir`；空 = 没给。
    pub account_dir: String,
    /// `--no-rotation`：这一发不轮换。
    pub no_rotation: bool,
    pub print: bool,
    pub detach: bool,
    pub tmux_size: String,
    /// 交给 agent 的那一串，按用户写的顺序（ccm 不认的词全在这里，含 `--resume` / `--model`）。
    pub passthru: Vec<String>,
    /// 透传里 resume 的那条会话（只看不吃：词仍原样在 `passthru` 里）；见 [`resume_sid`]。
    pub resumes: Option<String>,
}

/// 一次**立即结束**的请求：解析期就能答完、不必走计划面。
#[derive(Debug, PartialEq, Eq)]
pub enum Early {
    Probe,
    Version,
    Help,
    /// `--hosts`：列这台的托管终端。
    Hosts,
}

/// 托管入口（被叫成一段写了 `--hosted` 的配置，如 `cch`）自己认的两个词：`ls` · `attach <名>`。
pub mod hosted_word {
    pub const LS: &str = "ls";
    pub const ATTACH: &str = "attach";
}

/// 托管入口那一串（名字后面那几个词）的第一个词是它的词 ⇒ 换成 ccm 的那一形（`ls` ⇒ `-- --hosts` ·
/// `attach <名>` ⇒ `-- --attach <名>`）；缺名 · 多出词 ⇒ 说清。第一个词不是它的 ⇒ `None`（整串照旧交 `ccm @<名> …`）。
pub fn hosted_entry(rest: &[String]) -> Option<Result<Vec<String>, Die>> {
    let first = rest.first()?.as_str();
    let extra = |n: usize| {
        Die(copy_text(
            "beArgv.hostedEntry.extra",
            &[("word", first), ("extra", &rest[n..].join(" "))],
        ))
    };
    match first {
        hosted_word::LS if rest.len() == 1 => Some(Ok(vec![flag::END.into(), flag::HOSTS.into()])),
        hosted_word::LS => Some(Err(extra(1))),
        hosted_word::ATTACH => Some(match need_val(first, rest.get(1)) {
            Err(e) => Err(e),
            Ok(_) if rest.len() > 2 => Err(extra(2)),
            Ok(name) => Ok(vec![flag::END.into(), flag::ATTACH.into(), name]),
        }),
        _ => None,
    }
}

/// 解析失败 ⇒ 一句人话 + 退出码 2（用法错）。
#[derive(Debug, PartialEq, Eq)]
pub struct Die(pub String);

fn die<T>(msg: impl Into<String>) -> Result<T, Die> {
    Err(Die(msg.into()))
}

/// `--flag <值>` 这一形：下一个 token 缺席、或它本身像个旗标 ⇒ 多半是漏了参数。
fn need_val(name: &str, next: Option<&String>) -> Result<String, Die> {
    match next {
        None => die(copy_text("beArgv.needVal.missing", &[("name", name)])),
        Some(v) if v.is_empty() => die(copy_text("beArgv.needVal.missing", &[("name", name)])),
        Some(v) if v.starts_with('-') => die(copy_text(
            "beArgv.needVal.looksMissing",
            &[("name", name), ("v", v.as_str())],
        )),
        Some(v) => Ok(v.clone()),
    }
}

/// 解析结果：要么立即答（[`Early`]），要么拿到一整套 [`Opts`]。
#[derive(Debug)]
pub enum Parsed {
    Early(Early),
    Opts(Box<Opts>),
}

/// `args` 里**最后一个** `--` 的位置（没有 ⇒ `None`）。左边交 claude、右边归 ccm —— 切法只住这一处。
pub fn last_end(args: &[String]) -> Option<usize> {
    args.iter().rposition(|a| a == flag::END)
}

/// 一组没给任何选项时的意图（默认值都取 [`Defaults`]）；`left` 是交给 agent 的那一串。
pub fn blank_opts(left: &[String]) -> Opts {
    Opts {
        attach_name: String::new(),
        use_tmux: Defaults::USE_TMUX,
        hosted: Defaults::HOSTED,
        tmux_name: String::new(),
        tmux_base: String::new(),
        account: String::new(),
        use_base: Defaults::USE_BASE,
        cwd_spec: Defaults::CWD,
        cwd_if: Vec::new(),
        agent: Defaults::agent().to_string(),
        launcher: String::new(),
        ccm_sid: String::new(),
        tab: None,
        tab_from: String::new(),
        alias: String::new(),
        tab_group: String::new(),
        say: String::new(),
        account_dir: String::new(),
        no_rotation: Defaults::NO_ROTATION,
        print: Defaults::PRINT,
        detach: Defaults::DETACH,
        tmux_size: String::new(),
        passthru: left.to_vec(),
        resumes: None,
    }
}

/// `--` 右边认得的全部词（[`flag::END`] 不在其中：右边不会再有它）。
pub const RIGHT_WORDS: &[&str] = &[
    flag::NEW,
    flag::TMUX,
    flag::HOSTED,
    flag::TMUX_BASE,
    flag::TMUX_SIZE,
    flag::DETACH,
    flag::ACCOUNT,
    flag::BASE,
    flag::CWD,
    flag::CWD_IF,
    flag::AGENT,
    flag::LAUNCHER,
    flag::ATTACH,
    flag::HOSTS,
    flag::CCM_PRINT,
    flag::CCM_HELP,
    flag::CCM_VERSION,
    flag::CCM_PROBE,
    flag::CCM_SID,
    flag::TAB,
    flag::TAB_FROM,
    flag::CCM_ALIAS,
    flag::TAB_GROUP,
    flag::SAY,
    flag::ACCOUNT_DIR,
    flag::NO_ROTATION,
];

/// 右边的一组词：一个认得的词连同它吃掉的值。几个词、值从哪来只住 [`word_at`]，含义只住 [`apply_word`] ——
/// [`parse_layered`] 与别名表单（`assets/aliases/form.rs`）都按这一份走。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    /// 认得的那个词（[`flag`] 里的常量）。
    pub flag: &'static str,
    /// 它带的值：`--x <值>` / `--x=<值>` 一个，`--cwd-if` 两个，开关没有（`--ccm-tmux=<名>` 的名算一个）。
    pub vals: Vec<String>,
    /// 这一组占几个词（原样那几个是 `args[i..i + len]`）。
    pub len: usize,
}

/// `args[i]` 起的那一组词（`args` 是 `--` 右边那一串）。认不得 / 缺值 ⇒ 与 [`parse_layered`] 同一句。
pub fn word_at(args: &[String], i: usize) -> Result<Word, Die> {
    let a = args[i].as_str();
    // `--flag=值` 这一形先拆开，省得每个旗标写两条臂。
    let (key, inline) = match a.find('=') {
        Some(p) if a.starts_with("--") && p > 2 => (&a[..p], Some(a[p + 1..].to_string())),
        _ => (a, None),
    };
    let Some(known) = right_word(a) else {
        // 右边认不得 ⇒ 报错。那一句同时说清后端子命令只能紧跟打头的 `--`：认不认它是不是子命令要问命令表，这一层不问。
        return die(copy_text("beArgv.parse.unknownRight", &[("w", a)]));
    };
    // `--flag <值>` / `--flag=<值>`：值与占几个词。
    let one = || -> Result<(String, usize), Die> {
        match &inline {
            Some(v) => Ok((v.clone(), 1)),
            None => Ok((need_val(key, args.get(i + 1))?, 2)),
        }
    };
    let (vals, len) = match known {
        flag::TMUX => (inline.clone().into_iter().collect(), 1),
        flag::CWD_IF => {
            let (at, n) = one()?;
            let to = need_val(key, args.get(i + n))?;
            (vec![at, to], n + 1)
        }
        // 写空（`--ccm-agent ""`）⇒ 默认那一家，不当成漏了参数（认法住 `agents::pick_kind`）。
        flag::AGENT if inline.is_none() && args.get(i + 1).is_some_and(|v| v.is_empty()) => {
            (vec![String::new()], 2)
        }
        flag::TMUX_BASE
        | flag::ACCOUNT
        | flag::CWD
        | flag::AGENT
        | flag::LAUNCHER
        | flag::ATTACH
        | flag::CCM_SID
        | flag::TAB
        | flag::TAB_FROM
        | flag::CCM_ALIAS
        | flag::TAB_GROUP
        | flag::SAY
        | flag::ACCOUNT_DIR
        | flag::TMUX_SIZE => {
            let (v, n) = one()?;
            (vec![v], n)
        }
        // 开关：`--x=值` 那个值不看。
        _ => (Vec::new(), 1),
    };
    Ok(Word {
        flag: known,
        vals,
        len,
    })
}

/// 一组词落进意图。`at` 是它在右边的位置（`new` 只许打头）。立即结束的那几个 ⇒ `Some`、`o` 不动。
pub fn apply_word(o: &mut Opts, w: &Word, at: usize) -> Result<Option<Early>, Die> {
    let v = |k: usize| w.vals.get(k).cloned().unwrap_or_default();
    match w.flag {
        flag::NEW if at == 0 => {}
        // `new` 只许打头 ⇒ 别处出现说清为什么，不落到「不是 ccm 的选项」那句。
        flag::NEW => return die(copy_text("beArgv.parse.newNotFirst", &[])),
        flag::TMUX => {
            o.use_tmux = true;
            if let Some(n) = w.vals.first() {
                o.tmux_name = n.clone();
            }
        }
        flag::TMUX_BASE => {
            o.use_tmux = true;
            o.tmux_base = v(0);
        }
        flag::ACCOUNT => o.account = v(0),
        flag::BASE => o.use_base = true,
        flag::CWD => o.cwd_spec = CwdSpec::Explicit(v(0)),
        flag::CWD_IF => o.cwd_if.push((v(0), v(1))),
        flag::AGENT => o.agent = v(0),
        flag::LAUNCHER => o.launcher = v(0),
        flag::ATTACH => o.attach_name = v(0),
        flag::CCM_SID => o.ccm_sid = v(0),
        flag::TAB => {
            o.tab = Some(tab_of(&v(0)).ok_or_else(|| {
                Die(copy_text(
                    "beArgv.validate.badTab",
                    &[("v", &format!("{:?}", v(0)))],
                ))
            })?)
        }
        flag::TAB_FROM => o.tab_from = v(0),
        flag::CCM_ALIAS => o.alias = v(0),
        flag::TAB_GROUP => o.tab_group = v(0),
        flag::SAY => o.say = v(0),
        flag::ACCOUNT_DIR => o.account_dir = v(0),
        flag::DETACH => o.detach = true,
        flag::HOSTED => o.hosted = true,
        flag::NO_ROTATION => o.no_rotation = true,
        flag::TMUX_SIZE => o.tmux_size = v(0),
        flag::CCM_PRINT => o.print = true,
        flag::CCM_PROBE => return Ok(Some(Early::Probe)),
        flag::CCM_VERSION => return Ok(Some(Early::Version)),
        flag::CCM_HELP => return Ok(Some(Early::Help)),
        flag::HOSTS => return Ok(Some(Early::Hosts)),
        other => return die(copy_text("beArgv.parse.unknownRight", &[("w", other)])),
    }
    Ok(None)
}

/// `--tab` 的值那个词（`show` · `hide`）：写进命令行与配置文件都是它。
pub fn tab_word(t: cmd_wire::session::Tab) -> &'static str {
    match t {
        cmd_wire::session::Tab::Show => "show",
        cmd_wire::session::Tab::Hide => "hide",
    }
}

/// [`tab_word`] 反过来：认不得 ⇒ `None`。
pub fn tab_of(word: &str) -> Option<cmd_wire::session::Tab> {
    [cmd_wire::session::Tab::Show, cmd_wire::session::Tab::Hide]
        .into_iter()
        .find(|t| tab_word(*t) == word)
}

/// 一层配置（配置文件里的一段）：名字 ＋ 交给 agent 的词 ＋ ccm 的词（`--` 右边那种写法）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    pub name: String,
    pub agent: Vec<String>,
    pub ccm: Vec<String>,
}

/// 合并结果里的一项来自哪一层：`layer` 是那一段的名字，空串 ＝ 命令行当场给的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub flag: &'static str,
    pub vals: Vec<String>,
    pub layer: String,
}

/// 同一格的几个词：后来的一层写了其中任何一个，前面几层写的这一格整格作废。
/// 号（`--account` / `--base`）是一格；tmux 会话名（`--ccm-tmux[=名]` / `--tmux-base`）是一格；其余每个词自成一格。
fn slot_of(f: &'static str) -> &'static str {
    match f {
        flag::BASE => flag::ACCOUNT,
        flag::TMUX_BASE => flag::TMUX,
        other => other,
    }
}

/// 这套 argv 的唯一解析口，也是**继承合并的唯一住址**。
///
/// 格式 `ccm [交给 claude 的…] -- [ccm 自己的…]`：没有 `--` ⇒ 整行原样交 agent（[`Opts::passthru`]，一个词都不拦）；
/// 有 ⇒ 按最后一个 `--` 切（[`last_end`]），左边原样交 agent（claude 自己的 `--` 照写，没有 ccm 部分时末尾补一个空 `--`），
/// 右边逐词只认 ccm 表（壳层选项 ＋ `--ccm-*` 诊断口），认不得就报错、不猜。
///
/// `layers` 按父 → 子排好（配置文件里「基于」那条链；没有配置层 ⇒ 空），`args` 是命令行当场给的，最后盖上去。合并规则：
/// - 交给 agent 的词：父在前、子在后，命令行 `--` 左边那串接在最后；
/// - `--cwd-if`：子那一层的规则排在父的前面（同一个「在哪」子的先对上），各层内照写的顺序；
/// - 其余每一格（[`slot_of`]）：后来的一层写了就整格盖掉前面几层的；开关只能打开（写了就是开）。
///
/// 合并完逐词落意图（[`word_at`] 几个词 ＋ [`apply_word`] 什么意思）＋ [`finish`] 收尾；每一项来自哪一层一并交回（设置窗的合并预览照画）。
pub fn parse_layered(layers: &[Layer], args: &[String]) -> Result<(Parsed, Vec<Origin>), Die> {
    let (left, right): (&[String], &[String]) = match last_end(args) {
        Some(k) => (&args[..k], &args[k + 1..]),
        None => (args, &[]),
    };
    let cmd = Layer {
        name: String::new(),
        agent: left.to_vec(),
        ccm: right.to_vec(),
    };
    let mut agent = Vec::new();
    // `(哪一层, 在那一层右边的位置, 那一组词)`
    let mut kept: Vec<(usize, usize, Word)> = Vec::new();
    let mut rules: Vec<(usize, usize, Word)> = Vec::new();
    for (li, l) in layers.iter().chain(std::iter::once(&cmd)).enumerate() {
        agent.extend(l.agent.iter().cloned());
        // 逐词先落一遍草稿：报错与立即结束的先后与只有命令行时逐字一样。
        let mut scratch = blank_opts(&[]);
        let mut mine = Vec::new();
        let mut i = 0;
        while i < l.ccm.len() {
            let w = word_at(&l.ccm, i)?;
            if let Some(e) = apply_word(&mut scratch, &w, i)? {
                return Ok((Parsed::Early(e), Vec::new()));
            }
            i += w.len;
            mine.push((li, i - w.len, w));
        }
        let (own_rules, own): (Vec<_>, Vec<_>) = mine
            .into_iter()
            .partition(|(_, _, w)| w.flag == flag::CWD_IF);
        for (_, _, w) in &own {
            let s = slot_of(w.flag);
            kept.retain(|(lj, _, k)| *lj == li || slot_of(k.flag) != s);
        }
        kept.extend(own);
        rules.splice(0..0, own_rules);
    }
    let name = |li: usize| layers.get(li).map(|l| l.name.clone()).unwrap_or_default();
    let mut o = blank_opts(&agent);
    let mut origins = Vec::new();
    for (li, at, w) in rules.iter().chain(&kept) {
        apply_word(&mut o, w, *at)?;
        // `--tab` 来自哪个别名：照合并下来的那一层记（命令行当场给的 ⇒ 空；`--tab-from` 明说了的不盖）。
        if w.flag == flag::TAB && o.tab_from.is_empty() {
            o.tab_from = name(*li);
        }
        origins.push(Origin {
            flag: w.flag,
            vals: w.vals.clone(),
            layer: name(*li),
        });
    }
    // 这一发叫的是基于链最末那一层（`@名`）；`--ccm-alias` 明说了的不盖。
    if o.alias.is_empty() {
        o.alias = layers.last().map(|l| l.name.clone()).unwrap_or_default();
    }
    Ok((Parsed::Opts(Box::new(finish(o)?)), origins))
}

/// 逐词落完之后的收尾：认是哪一家、按那一家的写法认 resume、组合校验。[`parse_layered`] 与别名校验（`assets/aliases::check_alias`）共用。
pub(crate) fn finish(mut o: Opts) -> Result<Opts, Die> {
    // 哪一家：空 ⇒ 默认那一家；注册表里没有 ⇒ 报错、列出认得的几家（不落默认）。下游只见解析好的 kind。
    let (kind, face) = crate::agents::pick_kind(Some(&o.agent)).map_err(Die)?;
    o.agent = kind.to_string();
    o.resumes = resume_sid(&o.passthru, face.resume_token).map(str::to_string);
    validate(&o)?;
    // 不轮换是起之前按定好的会话ID记的：只有起新会话、且那一家认先定会话ID才记得上。
    if o.no_rotation {
        if o.resumes.is_some() || !o.attach_name.is_empty() {
            return die(copy_text("beArgv.validate.noRotationNotNew", &[]));
        }
        if face.preset_sid.is_none() {
            return die(copy_text(
                "beArgv.validate.noRotationNoSid",
                &[("agent", &o.agent.to_string())],
            ));
        }
    }
    Ok(o)
}

/// 透传里 resume 的那条会话，按这一家的 resume 写法认（`token` ＝ `LaunchFace::resume_token`）：
/// - 旗标形（`--` 开头）：`<token> <值>` / `<token>=<值>` / `-r <值>`（最后一个算；agent 自己的 `--` 之后不看）；
/// - 子命令形：透传第一个词恰是 `<token>` ⇒ 下一个词（不以 `-` 开头才算）。
///
/// 值可能是搜索词 / 会话名而不是 sid —— 那只会对不上快照里任何 `@ccm_sid`，不误伤。
pub(crate) fn resume_sid<'a>(passthru: &'a [String], token: &str) -> Option<&'a str> {
    if !token.starts_with("--") {
        return match passthru {
            [first, v, ..] if first == token && !v.starts_with('-') => Some(v.as_str()),
            _ => None,
        };
    }
    let mut found = None;
    let mut i = 0;
    while let Some(w) = passthru.get(i) {
        match w.as_str() {
            flag::END => break,
            w if w == token || w == "-r" => {
                if let Some(v) = passthru.get(i + 1).filter(|v| !v.starts_with('-')) {
                    found = Some(v.as_str());
                    i += 1;
                }
            }
            w => {
                if let Some(v) = w
                    .strip_prefix(token)
                    .and_then(|r| r.strip_prefix('='))
                    .filter(|v| !v.is_empty())
                {
                    found = Some(v);
                }
            }
        }
        i += 1;
    }
    found
}

/// 组合校验。**一条都不许静默忽略** —— 静默忽略正是本工作区反复消灭的那类病
///（写了个修饰、看起来生效了、实际被吃掉）。
fn validate(o: &Opts) -> Result<(), Die> {
    if !o.account.is_empty() && o.use_base {
        return die(copy_text("beArgv.validate.accountAndBase", &[]));
    }
    if !o.account_dir.is_empty() && (!o.account.is_empty() || o.use_base) {
        return die(copy_text("beArgv.validate.accountDirAndAccount", &[]));
    }
    // 这一家没有账号这一维 ⇒ 选号说不出：明说，不导出一个它不读的变量（`--base` 照收，什么都不做）。
    if (!o.account.is_empty() || !o.account_dir.is_empty())
        && crate::agents::account_env_of(&o.agent).is_none()
    {
        return die(copy_text(
            "beArgv.validate.agentNoAccounts",
            &[("agent", &o.agent.to_string())],
        ));
    }
    if o.hosted && o.use_tmux {
        return die(copy_text("beArgv.validate.hostedAndTmux", &[]));
    }
    if o.detach && !o.use_tmux && !o.hosted {
        return die(copy_text("beArgv.validate.detachNeedsTmux", &[]));
    }
    if !o.tmux_size.is_empty() && !o.use_tmux && !o.hosted {
        return die(copy_text("beArgv.validate.sizeNeedsTmux", &[]));
    }
    if !o.tmux_name.is_empty() && !o.tmux_base.is_empty() {
        return die(copy_text("beArgv.validate.nameAndBase", &[]));
    }
    if !o.tab_from.is_empty() && o.tab.is_none() {
        return die(copy_text("beArgv.validate.tabFromNeedsTab", &[]));
    }
    check_values(o)
}

/// 每一格自己的值合不合法（与别的格怎么组合无关）：起会话时 [`validate`] 收尾要过，配置文件逐项读的时候也过这一道
/// （写错报到那一行，`assets/aliases/profile.rs`）。没给的格不看。
pub fn check_values(o: &Opts) -> Result<(), Die> {
    if !o.tmux_size.is_empty() && parse_size(&o.tmux_size).is_none() {
        // 尺寸会被拼进 `tmux new-session -x W -y H`，**必须**只允许纯数字，否则就是一条注入面。
        return die(copy_text(
            "beArgv.validate.badSize",
            &[("size", &o.tmux_size.to_string())],
        ));
    }
    // 标识符在拼进容器路那条 shell 串之前先过放行判定（`INVARIANTS §47` ①；判定住 `shell-quote-core`；
    // quote 只管元字符，管不了 `-` 开头的选项注入）。
    if !o.ccm_sid.is_empty() && !shell_quote_core::session_id_ok(&o.ccm_sid) {
        return die(copy_text(
            "beArgv.validate.badCcmSid",
            &[("sid", &format!("{:?}", o.ccm_sid))],
        ));
    }
    if !o.account_dir.is_empty() && !acct_core::config_dir_ok(&o.account_dir) {
        return die(copy_text(
            "beArgv.validate.badAccountDir",
            &[("dir", &format!("{:?}", o.account_dir))],
        ));
    }
    // 「在哪」要钉住一个目录：`~` 打头或绝对路径（相对的话「在哪敲」恒对上）。
    for (at, _) in &o.cwd_if {
        let pinned = at == "~"
            || at.starts_with("~/")
            || at.starts_with("~\\")
            || std::path::Path::new(at).is_absolute();
        if !pinned || !shell_quote_core::free_text_ok(at) {
            return die(copy_text(
                "beArgv.validate.cwdIfNotPinned",
                &[("at", &format!("{at:?}"))],
            ));
        }
    }
    if !o.tab_from.is_empty() && !shell_quote_core::account_name_ok(&o.tab_from) {
        return die(copy_text(
            "beArgv.validate.badTabFrom",
            &[("name", &format!("{:?}", o.tab_from))],
        ));
    }
    if !o.alias.is_empty() && !shell_quote_core::account_name_ok(&o.alias) {
        return die(copy_text(
            "beArgv.validate.badAlias",
            &[("name", &format!("{:?}", o.alias))],
        ));
    }
    if !o.tab_group.is_empty() && o.tab_group.trim().is_empty() {
        return die(copy_text("beArgv.validate.blankTabGroup", &[]));
    }
    // 开口那句拼进交给 agent 的那一串（`-- <那句>`），同透传里每个词过同一道。
    if !o.say.is_empty() && !shell_quote_core::arg_text_ok(&o.say) {
        return die(copy_text(
            "beArgv.validate.badSay",
            &[("say", &format!("{:?}", o.say))],
        ));
    }
    if !o.account.is_empty() && !shell_quote_core::account_name_ok(&o.account) {
        return die(copy_text(
            "beArgv.validate.badAccount",
            &[("account", &format!("{:?}", o.account))],
        ));
    }
    Ok(())
}

/// `<宽>x<高>` ⇒ `(宽, 高)`。只认纯十进制、两侧都非空、**有且仅有一个 `x`**。
pub fn parse_size(s: &str) -> Option<(String, String)> {
    let (w, h) = s.split_once('x')?;
    if w.is_empty() || h.is_empty() {
        return None;
    }
    if !w.bytes().all(|c| c.is_ascii_digit()) || !h.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((w.to_string(), h.to_string()))
}

/// 判据的共用夹具（没有配置层的解析 · 旧写法的换排列）：本包判据与上层判据（经 `test-support`）共用。
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../../tests/backend/ccm_line/argv_support.rs"]
pub mod test_support;

#[cfg(test)]
#[path = "../../../../tests/backend/ccm_line/argv_tests.rs"]
mod tests;
