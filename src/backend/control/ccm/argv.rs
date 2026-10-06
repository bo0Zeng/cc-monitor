//! `ccm` 这套 argv 的**唯一解析口**（`KR48D2`）与**唯一默认值住址**（`KR48D4`）。
//!
//! 〔用@09-11 `K33` 逐字〕「后端**只有一个**，**不要有什么 bash 脚本**，**不要有什么单独的 ccm**。
//! **所有命令只许有一处**，其他都是**根据传参来调用**。」
//!
//! # 本文件承的是哪两条 DoD
//!
//! - `KR48D2`「终端里敲的那个命令，实现只有一处」⇒ 这套 argv 的**解析**只许在这一个文件里。
//!   `plan.rs` / `mod.rs` 一律拿 [`Opts`]，**不许自己再看一眼 `args`**。
//!   由 `the_ccm_argv_is_parsed_in_exactly_one_place` 机检。
//! - `KR48D4`「一个参数的默认值只有一处住址」⇒ 每个参数的默认值只许写在 [`Defaults`] 里。
//!   由 `every_default_lives_only_in_the_defaults_block` 机检。
//!
//! # 旗标名也只有一处住址
//!
//! 每个 `--flag` 的**字面量**只住在 [`flag`] 里。`plan.rs` 拼内层载荷时要用同样的字面量
//! （`--account` / `--base` / …），在那边再敲一遍就是第二处住址 —— 那正是 `K-R50`
//! 那件事的成因（两份写法迟早分叉）。
//!
//! ⚠ **本文件是本 crate 里唯一允许持有 ccm 旗标字面量的地方**，
//! 由 `protocol_doc_guard::TERMINAL_SURFACE_FILES` 登记并机检（见那边的头注：
//! 它换掉了 `dispatch_registry_is_complete` 在本文件上的那一格，不是绕过它）。

use copy_core::copy_text;

/// 每个 `--flag` 的字面量，**唯一住址**。
///
/// ccm 是 claude 的壳：它只认下面这些（壳层选项 ＋ `--ccm-*` 诊断口 ＋ `--`），其余每个词原样交给 agent。
/// 用户 09-26：与 claude 同名的两个改名 `--ccm-tmux` / `--ccm-agent`（claude 2.1.283 自己有 `--tmux` / `--agent`）。
pub(crate) mod flag {
    pub(crate) const TMUX: &str = "--ccm-tmux";
    pub(crate) const TMUX_BASE: &str = "--tmux-base";
    pub(crate) const TMUX_SIZE: &str = "--tmux-size";
    pub(crate) const DETACH: &str = "--detach";
    pub(crate) const ACCOUNT: &str = "--account";
    pub(crate) const BASE: &str = "--base";
    pub(crate) const CWD: &str = "--cwd";
    /// `--cwd-if <在哪> <进哪>`：正好在「在哪」那个目录敲 ⇒ 进「进哪」（可写多次，按序第一条对上的算）。
    pub(crate) const CWD_IF: &str = "--cwd-if";
    pub(crate) const AGENT: &str = "--ccm-agent";
    pub(crate) const LAUNCHER: &str = "--launcher";
    pub(crate) const BUS_REGISTER: &str = "--bus-register";
    pub(crate) const BUS_NOTE: &str = "--bus-note";
    pub(crate) const ATTACH: &str = "--attach";
    pub(crate) const CCM_PRINT: &str = "--ccm-print";
    pub(crate) const CCM_HELP: &str = "--ccm-help";
    pub(crate) const CCM_VERSION: &str = "--ccm-version";
    pub(crate) const CCM_PROBE: &str = "--ccm-probe";
    pub(crate) const CCM_SID: &str = "--ccm-sid";
    /// 直接给账号配置目录（说不出账号名的那一形：分叉继承源会话的目录）。
    pub(crate) const ACCOUNT_DIR: &str = "--account-dir";
    /// 分隔符：最后一个 `--` 左边交 agent、右边归 ccm。
    pub(crate) const END: &str = "--";
    /// 位置动作「起新会话」：ccm 自己的词，只许是 `--` 右边的第一个词（`ccm [claude 的] -- new [ccm 选项]`）。
    /// 起新会话本来就是缺省，写出来是给想说清楚的人与渲染器用的；`ccm new`（没有 `--`）整行交 claude。
    pub(crate) const NEW: &str = "new";
}

/// 这个词是不是 ccm 自己认的（壳层选项 ＋ `--ccm-*` 诊断口）—— 问的就是真解析器（放在 `--` 右边喂它）。
/// 后端 CLI 面不许派生出这样的名字（`cli_control::cli_exposed`）。
pub(crate) fn is_ccm_word(word: &str) -> bool {
    // 放在 `--` 右边问：只要不是「认不得这个词」那两句，就是 ccm 的词（缺值 / 组合不对也算认得）。
    let unknown = [
        copy_text("beArgv.parse.unknownRight", &[("w", word)]),
        copy_text("beArgv.parse.backendWordAfterClaudeArgs", &[("w", word)]),
    ];
    !matches!(parse(&[flag::END.to_string(), word.to_string()]), Err(Die(m)) if unknown.contains(&m))
}

/// `--cwd` 的取值：`auto`（默认）或一个显式目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CwdSpec {
    Auto,
    Explicit(String),
}

/// 🔴 **每个参数的默认值，唯一住址**（`KR48D4`）。
///
/// 「可以省略参数，而且能省就尽量省」（`K33` 裁定五后半）的前提是**默认值不许在两处各写一份**：
/// 两份默认迟早分叉，而那正是 `K-R50` 那件事的成因（漏传不报错、悄悄换成另一个值）。
///
/// ✅ **`--account` 那一格已经裁了**（用户 09-12，住址 `DECISIONS.md#R28`）：
/// **不给 ⇒ 落 manifest 的默认账号，这就是要的行为**；用户逐字「不是有选默认账号吗? 就用那个」。
/// ⚠ 与它成对的另一半同样是裁定的一部分：**调用方已经选好的号不许被静默换掉**
/// （`plan::resolve_account` 的继承那一支，闸的来历是 `R08` 真机复现过的一次静默换号）。
/// ⇒ 这两句**别再读成「照搬 `shared/ccm` 的病灶、等人来裁」** —— 那是 09-11 的读法，已经过期。
pub(crate) struct Defaults;

impl Defaults {
    /// 不给 `--agent` ⇒ 注册表里声明默认的那一家（`LaunchFace::is_default`）。
    pub(crate) fn agent() -> &'static str {
        crate::agents::default_kind()
    }
    /// 不给 `--cwd` ⇒ `auto`，而 `K-R58` 起 **`auto` 就是恒等**：调用方自己的 cwd。
    /// 见 `plan::resolve_cwd`（`K37` 第三条：诚实的默认 = 恒等 / 不作为 / 沿用调用者状态）。
    pub(crate) const CWD: CwdSpec = CwdSpec::Auto;
    /// 不给 `--tmux` ⇒ 不进容器路。
    pub(crate) const USE_TMUX: bool = false;
    /// 不给 `--base` ⇒ 不是基座态。
    pub(crate) const USE_BASE: bool = false;
    /// 不给 `--detach` ⇒ 建完接进去。
    pub(crate) const DETACH: bool = false;
    /// 不给 `--ccm-print` ⇒ 真跑。
    pub(crate) const PRINT: bool = false;
    /// 不给 `--bus-register` ⇒ 不登记 cc-bus。
    pub(crate) const BUS_REGISTER: bool = false;
    // 🔴 `K-R58`：这里原来有 `WORKSPACE_REL = "projects/notes"`（`$HOME` 下裸敲时
    //    的落点）。它是**一张表里替用户挑的那个具体值**，`K37` 第三条判它「不诚实」⇒ 删了，
    //    连同读它的 `CCM_WORKSPACE`。默认值表里少一格，是因为那一格的默认现在是恒等。
    // 账号库 manifest 的门牌号不在这张默认值表里：它是后端的家那一族（`plan::accts_manifest_under`），
    // 不是一个可以换的默认值。
    /// 起 agent 前要 eval 的机器级 env（旧 `CC_ENV` 的搬家）。
    pub(crate) const ENV: &'static str = "";
}

/// 解析出来的一整套意图。**下游只许读这个结构，不许再看一眼 `args`。**
#[derive(Debug, Clone)]
pub(crate) struct Opts {
    /// `--attach <名>`；空 = 没给（起会话）。
    pub(crate) attach_name: String,
    pub(crate) use_tmux: bool,
    pub(crate) tmux_name: String,
    pub(crate) tmux_base: String,
    pub(crate) bus_register: bool,
    pub(crate) bus_note: String,
    /// 空串 = **没给** —— 它与「给了一个空值」在本 CLI 上同形（bash 那侧也是）。
    pub(crate) account: String,
    pub(crate) use_base: bool,
    pub(crate) cwd_spec: CwdSpec,
    /// `--cwd-if` 那几条 `(在哪, 进哪)`，按写的顺序；都没对上 ⇒ [`Opts::cwd_spec`]。
    pub(crate) cwd_if: Vec<(String, String)>,
    pub(crate) agent: String,
    pub(crate) launcher: String,
    pub(crate) ccm_sid: String,
    /// `--account-dir`；空 = 没给。
    pub(crate) account_dir: String,
    pub(crate) print: bool,
    pub(crate) detach: bool,
    pub(crate) tmux_size: String,
    /// 交给 agent 的那一串，按用户写的顺序（ccm 不认的词全在这里，含 `--resume` / `--model`）。
    pub(crate) passthru: Vec<String>,
    /// 透传里 resume 的那条会话（只看不吃：词仍原样在 `passthru` 里）；见 [`resume_sid`]。
    pub(crate) resumes: Option<String>,
}

/// 一次**立即结束**的请求：解析期就能答完、不必走计划面。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Early {
    Probe,
    Version,
    Help,
}

/// 解析失败 ⇒ 一句人话 + 退出码 2（用法错）。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Die(pub(crate) String);

fn die<T>(msg: impl Into<String>) -> Result<T, Die> {
    Err(Die(msg.into()))
}

/// `--flag <值>` 这一形：下一个 token 缺席、或它本身像个旗标 ⇒ 多半是漏了参数。
fn need_val(name: &str, next: Option<&String>) -> Result<String, Die> {
    match next {
        None => die(copy_text(
            "beArgv.needVal.missing",
            &[("name", &name.to_string())],
        )),
        Some(v) if v.is_empty() => die(copy_text(
            "beArgv.needVal.missing",
            &[("name", &name.to_string())],
        )),
        Some(v) if v.starts_with('-') => die(copy_text(
            "beArgv.needVal.looksMissing",
            &[("name", &name.to_string()), ("v", &v.to_string())],
        )),
        Some(v) => Ok(v.clone()),
    }
}

/// 解析结果：要么立即答（[`Early`]），要么拿到一整套 [`Opts`]。
#[derive(Debug)]
pub(crate) enum Parsed {
    Early(Early),
    Opts(Box<Opts>),
}

/// `args` 里**最后一个** `--` 的位置（没有 ⇒ `None`）。左边交 claude、右边归 ccm —— 切法只住这一处。
pub(crate) fn last_end(args: &[String]) -> Option<usize> {
    args.iter().rposition(|a| a == flag::END)
}

/// 一组没给任何选项时的意图（默认值都取 [`Defaults`]）；`left` 是交给 agent 的那一串。
pub(crate) fn blank_opts(left: &[String]) -> Opts {
    Opts {
        attach_name: String::new(),
        use_tmux: Defaults::USE_TMUX,
        tmux_name: String::new(),
        tmux_base: String::new(),
        bus_register: Defaults::BUS_REGISTER,
        bus_note: String::new(),
        account: String::new(),
        use_base: Defaults::USE_BASE,
        cwd_spec: Defaults::CWD,
        cwd_if: Vec::new(),
        agent: Defaults::agent().to_string(),
        launcher: String::new(),
        ccm_sid: String::new(),
        account_dir: String::new(),
        print: Defaults::PRINT,
        detach: Defaults::DETACH,
        tmux_size: String::new(),
        passthru: left.to_vec(),
        resumes: None,
    }
}

/// `--` 右边认得的全部词（[`flag::END`] 不在其中：右边不会再有它）。
const RIGHT_WORDS: &[&str] = &[
    flag::NEW,
    flag::TMUX,
    flag::TMUX_BASE,
    flag::TMUX_SIZE,
    flag::DETACH,
    flag::ACCOUNT,
    flag::BASE,
    flag::CWD,
    flag::CWD_IF,
    flag::AGENT,
    flag::LAUNCHER,
    flag::BUS_REGISTER,
    flag::BUS_NOTE,
    flag::ATTACH,
    flag::CCM_PRINT,
    flag::CCM_HELP,
    flag::CCM_VERSION,
    flag::CCM_PROBE,
    flag::CCM_SID,
    flag::ACCOUNT_DIR,
];

/// 右边的一组词：一个认得的词连同它吃掉的值。几个词、值从哪来只住 [`word_at`]，含义只住 [`apply_word`] ——
/// [`parse`] 与别名表单（`assets/aliases/form.rs`）都按这一份走。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Word {
    /// 认得的那个词（[`flag`] 里的常量）。
    pub(crate) flag: &'static str,
    /// 它带的值：`--x <值>` / `--x=<值>` 一个，`--cwd-if` 两个，开关没有（`--ccm-tmux=<名>` 的名算一个）。
    pub(crate) vals: Vec<String>,
    /// 这一组占几个词（原样那几个是 `args[i..i + len]`）。
    pub(crate) len: usize,
}

/// `args[i]` 起的那一组词（`args` 是 `--` 右边那一串）。认不得 / 缺值 ⇒ 与 [`parse`] 同一句。
pub(crate) fn word_at(args: &[String], i: usize) -> Result<Word, Die> {
    let a = args[i].as_str();
    // `--flag=值` 这一形先拆开，省得每个旗标写两条臂。
    let (key, inline) = match a.find('=') {
        Some(p) if a.starts_with("--") && p > 2 => (&a[..p], Some(a[p + 1..].to_string())),
        _ => (a, None),
    };
    let Some(&known) = RIGHT_WORDS.iter().find(|w| **w == key) else {
        // 右边认不得 ⇒ 报错。是后端子命令 / 流词（它们只能紧跟打头的 `--`）⇒ 说清为什么。
        return die(if i == 0 && crate::control::ccm::is_backend_word(a) {
            copy_text(
                "beArgv.parse.backendWordAfterClaudeArgs",
                &[("w", &a.to_string())],
            )
        } else {
            copy_text("beArgv.parse.unknownRight", &[("w", &a.to_string())])
        });
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
        | flag::BUS_NOTE
        | flag::ACCOUNT
        | flag::CWD
        | flag::AGENT
        | flag::LAUNCHER
        | flag::ATTACH
        | flag::CCM_SID
        | flag::ACCOUNT_DIR
        | flag::TMUX_SIZE => {
            let (v, n) = one()?;
            (vec![v], n)
        }
        // 开关：`--x=值` 那个值不看（与从前一样）。
        _ => (Vec::new(), 1),
    };
    Ok(Word {
        flag: known,
        vals,
        len,
    })
}

/// 一组词落进意图。`at` 是它在右边的位置（`new` 只许打头）。立即结束的那几个 ⇒ `Some`、`o` 不动。
pub(crate) fn apply_word(o: &mut Opts, w: &Word, at: usize) -> Result<Option<Early>, Die> {
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
        flag::BUS_REGISTER => o.bus_register = true,
        flag::BUS_NOTE => o.bus_note = v(0),
        flag::ACCOUNT => o.account = v(0),
        flag::BASE => o.use_base = true,
        flag::CWD => o.cwd_spec = CwdSpec::Explicit(v(0)),
        flag::CWD_IF => o.cwd_if.push((v(0), v(1))),
        flag::AGENT => o.agent = v(0),
        flag::LAUNCHER => o.launcher = v(0),
        flag::ATTACH => o.attach_name = v(0),
        flag::CCM_SID => o.ccm_sid = v(0),
        flag::ACCOUNT_DIR => o.account_dir = v(0),
        flag::DETACH => o.detach = true,
        flag::TMUX_SIZE => o.tmux_size = v(0),
        flag::CCM_PRINT => o.print = true,
        flag::CCM_PROBE => return Ok(Some(Early::Probe)),
        flag::CCM_VERSION => return Ok(Some(Early::Version)),
        flag::CCM_HELP => return Ok(Some(Early::Help)),
        other => {
            return die(copy_text(
                "beArgv.parse.unknownRight",
                &[("w", &other.to_string())],
            ))
        }
    }
    Ok(None)
}

/// 🔴 **这套 argv 的唯一解析口。**
///
/// 〔用户 09-27〕格式 `ccm [交给 claude 的…] -- [ccm 自己的…]`：没有 `--` ⇒ 整行原样交 agent（[`Opts::passthru`]，
/// 一个词都不拦）；有 ⇒ 按**最后一个** `--` 切（[`last_end`]），左边原样交 agent（claude 自己的 `--` 照写，
/// 没有 ccm 部分时末尾补一个空 `--`），右边逐词只认 ccm 表（壳层选项 ＋ `--ccm-*` 诊断口），认不得就报错、不猜。
/// 逐词那一圈是 [`word_at`]（几个词）＋ [`apply_word`]（什么意思）。
/// 〔墓碑 —— 上一版：壳层选项在任何位置都认、首词 `new` 是 ccm 的位置动作、`--` 之后一律透传。〕
pub(crate) fn parse(args: &[String]) -> Result<Parsed, Die> {
    let (left, right): (&[String], &[String]) = match last_end(args) {
        Some(k) => (&args[..k], &args[k + 1..]),
        None => (args, &[]),
    };
    let mut o = blank_opts(left);
    let mut i = 0;
    while i < right.len() {
        let w = word_at(right, i)?;
        if let Some(e) = apply_word(&mut o, &w, i)? {
            return Ok(Parsed::Early(e));
        }
        i += w.len;
    }
    Ok(Parsed::Opts(Box::new(finish(o)?)))
}

/// 逐词落完之后的收尾：认是哪一家、按那一家的写法认 resume、组合校验。[`parse`] 与别名校验（`assets/aliases::check_alias`）共用。
pub(crate) fn finish(mut o: Opts) -> Result<Opts, Die> {
    // 哪一家：空 ⇒ 默认那一家；注册表里没有 ⇒ 报错、列出认得的几家（不落默认）。下游只见解析好的 kind。
    let (kind, face) = crate::agents::pick_kind(Some(&o.agent)).map_err(Die)?;
    o.agent = kind.to_string();
    o.resumes = resume_sid(&o.passthru, face.resume_token).map(str::to_string);
    validate(&o)?;
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
        return die(&copy_text("beArgv.validate.accountAndBase", &[]));
    }
    if !o.account_dir.is_empty() && (!o.account.is_empty() || o.use_base) {
        return die(&copy_text("beArgv.validate.accountDirAndAccount", &[]));
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
    if o.detach && !o.use_tmux {
        return die(&copy_text("beArgv.validate.detachNeedsTmux", &[]));
    }
    if !o.tmux_size.is_empty() && !o.use_tmux {
        return die(&copy_text("beArgv.validate.sizeNeedsTmux", &[]));
    }
    if !o.tmux_name.is_empty() && !o.tmux_base.is_empty() {
        return die(&copy_text("beArgv.validate.nameAndBase", &[]));
    }
    if o.bus_register && !o.detach {
        return die(&copy_text("beArgv.validate.registerNeedsDetach", &[]));
    }
    if !o.bus_note.is_empty() && !o.bus_register {
        return die(&copy_text("beArgv.validate.noteNeedsRegister", &[]));
    }
    if !o.tmux_size.is_empty() && parse_size(&o.tmux_size).is_none() {
        // 尺寸会被拼进 `tmux new-session -x W -y H`，**必须**只允许纯数字，否则就是一条注入面。
        return die(copy_text(
            "beArgv.validate.badSize",
            &[("size", &o.tmux_size.to_string())],
        ));
    }
    // 〔`INVARIANTS §47` ①〕标识符在拼进容器路那条 shell 串之前先过放行判定
    // （判定住 `shell-quote-core`，全仓唯一一份；quote 只管元字符，管不了 `-` 开头的选项注入）。
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
    if !o.account.is_empty() && !shell_quote_core::account_name_ok(&o.account) {
        return die(copy_text(
            "beArgv.validate.badAccount",
            &[("account", &format!("{:?}", o.account))],
        ));
    }
    Ok(())
}

/// `<宽>x<高>` ⇒ `(宽, 高)`。只认纯十进制、两侧都非空、**有且仅有一个 `x`**。
pub(crate) fn parse_size(s: &str) -> Option<(String, String)> {
    let (w, h) = s.split_once('x')?;
    if w.is_empty() || h.is_empty() {
        return None;
    }
    if !w.bytes().all(|c| c.is_ascii_digit()) || !h.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((w.to_string(), h.to_string()))
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/ccm/argv_tests.rs"]
pub(crate) mod tests; // `pub(crate)`：旧写法夹具的换排列 `tests::mixed_to_split` 给同族几份单测共用

#[cfg(test)]
#[path = "../../../../tests/backend/control/ccm/claude_flags_tests.rs"]
mod claude_flags_tests;
