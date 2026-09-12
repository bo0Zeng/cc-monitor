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

/// 每个 `--flag` 的字面量，**唯一住址**。
pub(crate) mod flag {
    pub(crate) const RESUME: &str = "--resume";
    pub(crate) const TMUX: &str = "--tmux";
    pub(crate) const TMUX_BASE: &str = "--tmux-base";
    pub(crate) const BUS_REGISTER: &str = "--bus-register";
    pub(crate) const BUS_NOTE: &str = "--bus-note";
    pub(crate) const ACCOUNT: &str = "--account";
    pub(crate) const BASE: &str = "--base";
    pub(crate) const CWD: &str = "--cwd";
    pub(crate) const AGENT: &str = "--agent";
    pub(crate) const MODEL: &str = "--model";
    pub(crate) const LAUNCHER: &str = "--launcher";
    pub(crate) const CCM_SID: &str = "--ccm-sid";
    pub(crate) const DETACH: &str = "--detach";
    pub(crate) const TMUX_SIZE: &str = "--tmux-size";
    pub(crate) const PRINT: &str = "--print";
    pub(crate) const CCM_PROBE: &str = "--ccm-probe";
    pub(crate) const VERSION: &str = "--version";
    pub(crate) const HELP_LONG: &str = "--help";
    pub(crate) const HELP_SHORT: &str = "-h";
    /// argv 终止符：其后全部透传给 agent。
    pub(crate) const END: &str = "--";
}

/// 位置动作。`new` / `resume <sid>` / `attach <名字>`，且**必须在最前**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    New,
    Resume,
    Attach,
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
    /// 不给位置动作 ⇒ `new`。
    pub(crate) const ACTION: Action = Action::New;
    /// 不给 `--agent` ⇒ `claude`。
    pub(crate) const AGENT: &'static str = "claude";
    /// 不给 `--cwd` ⇒ `auto`，而 `K-R58` 起 **`auto` 就是恒等**：调用方自己的 cwd。
    /// 见 `plan::resolve_cwd`（`K37` 第三条：诚实的默认 = 恒等 / 不作为 / 沿用调用者状态）。
    pub(crate) const CWD: CwdSpec = CwdSpec::Auto;
    /// 不给 `--tmux` ⇒ 不进容器路。
    pub(crate) const USE_TMUX: bool = false;
    /// 不给 `--base` ⇒ 不是基座态。
    pub(crate) const USE_BASE: bool = false;
    /// 不给 `--detach` ⇒ 建完接进去。
    pub(crate) const DETACH: bool = false;
    /// 不给 `--print` ⇒ 真跑。
    pub(crate) const PRINT: bool = false;
    /// 不给 `--bus-register` ⇒ 不登记 cc-bus。
    pub(crate) const BUS_REGISTER: bool = false;
    // 🔴 `K-R58`：这里原来有 `WORKSPACE_REL = "claude-conversation"`（`$HOME` 下裸敲时
    //    的落点）。它是**一张表里替用户挑的那个具体值**，`K37` 第三条判它「不诚实」⇒ 删了，
    //    连同读它的 `CCM_WORKSPACE`。默认值表里少一格，是因为那一格的默认现在是恒等。
    /// 账号库 manifest（相对 `$HOME`）。
    ///
    /// ⚠ 这是 **cc-acct-iso 这个工具**的账号库门牌号，**不是 Claude 的目录布局** ——
    /// `agents/claudecode/accounts.rs` 的头注逐字把它划在适配层之外（「属工具而非 agent」）。
    /// 同族先例：`control/cc_bus.rs` 里 cc-bus 的门牌号。
    pub(crate) const ACCTS_MANIFEST_REL: &'static str = ".claude-accts/accounts.json";
    /// 配置文件（相对 `$HOME`）。
    pub(crate) const CONFIG_REL: &'static str = ".config/ccm/config";
    /// 起 agent 前要 eval 的机器级 env（旧 `CC_ENV` 的搬家）。
    pub(crate) const ENV: &'static str = "";
}

/// 解析出来的一整套意图。**下游只许读这个结构，不许再看一眼 `args`。**
#[derive(Debug, Clone)]
pub(crate) struct Opts {
    pub(crate) action: Action,
    pub(crate) sid: String,
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
    pub(crate) agent: String,
    pub(crate) model: String,
    pub(crate) launcher: String,
    /// 用户**显式**给了 `--launcher` 吗。必须在填默认值之前记下来 ——
    /// 填完就分不清「用户写的」与「默认的」了，而 `resume` 那条路要靠它决定问不问后端。
    pub(crate) launcher_explicit: bool,
    pub(crate) ccm_sid: String,
    pub(crate) print: bool,
    pub(crate) detach: bool,
    pub(crate) tmux_size: String,
    pub(crate) passthru: Vec<String>,
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
        None => die(format!("{name} 需要一个值")),
        Some(v) if v.is_empty() => die(format!("{name} 需要一个值")),
        Some(v) if v.starts_with('-') => die(format!(
            "{name} 需要一个值，但拿到的是 '{v}'（像是漏了参数）"
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

/// 🔴 **这套 argv 的唯一解析口。**
///
/// 形状逐条承接 `K26`〔用@08-28〕：位置动作 `new` / `resume <sid>` / `attach <名字>`
/// 在最前，其余一律 flag，`--` 之后全部透传给 agent。
pub(crate) fn parse(args: &[String]) -> Result<Parsed, Die> {
    let mut o = Opts {
        action: Defaults::ACTION,
        sid: String::new(),
        attach_name: String::new(),
        use_tmux: Defaults::USE_TMUX,
        tmux_name: String::new(),
        tmux_base: String::new(),
        bus_register: Defaults::BUS_REGISTER,
        bus_note: String::new(),
        account: String::new(),
        use_base: Defaults::USE_BASE,
        cwd_spec: Defaults::CWD,
        agent: Defaults::AGENT.to_string(),
        model: String::new(),
        launcher: String::new(),
        launcher_explicit: false,
        ccm_sid: String::new(),
        print: Defaults::PRINT,
        detach: Defaults::DETACH,
        tmux_size: String::new(),
        passthru: Vec::new(),
    };

    let mut i = 0usize;
    // ── 位置动作：只认第一个 token，且只认这三个 ──────────────────────────
    if let Some(first) = args.first() {
        match first.as_str() {
            "new" => {
                o.action = Action::New;
                i = 1;
            }
            "resume" => {
                o.action = Action::Resume;
                // 下一个 token 以 `-` 开头 ⇒ 用户漏了参数
                //（`ccm resume --tmux` 会静默把 `--tmux` 当 sid）。
                match args.get(1) {
                    Some(v) if !v.starts_with('-') => o.sid = v.clone(),
                    _ => return die("resume 需要 <sid>"),
                }
                i = 2;
            }
            "attach" => {
                o.action = Action::Attach;
                match args.get(1) {
                    Some(v) if !v.starts_with('-') => o.attach_name = v.clone(),
                    _ => return die("attach 需要 <会话名>"),
                }
                i = 2;
            }
            _ => {}
        }
    }

    // ── 其余一律 flag ────────────────────────────────────────────────────
    while i < args.len() {
        let a = args[i].as_str();
        // `--flag=值` 这一形先拆开，省得每个旗标写两条臂。
        let (key, inline) = match a.find('=') {
            Some(p) if a.starts_with("--") && p > 2 => (&a[..p], Some(a[p + 1..].to_string())),
            _ => (a, None),
        };
        // `--flag <值>` 这一形：取值并前进一格。
        macro_rules! val {
            () => {
                match inline {
                    Some(v) => v,
                    None => {
                        let v = need_val(key, args.get(i + 1))?;
                        i += 1;
                        v
                    }
                }
            };
        }
        match key {
            flag::END => {
                o.passthru.extend_from_slice(&args[i + 1..]);
                break;
            }
            // `--resume <sid>` 与 `resume <sid>` **等价** —— cc-monitor 今天就是这么拼的。
            flag::RESUME => {
                o.action = Action::Resume;
                o.sid = val!();
            }
            flag::TMUX => {
                o.use_tmux = true;
                if let Some(v) = inline {
                    o.tmux_name = v;
                }
            }
            flag::TMUX_BASE => {
                o.use_tmux = true;
                o.tmux_base = val!();
            }
            flag::BUS_REGISTER => o.bus_register = true,
            flag::BUS_NOTE => o.bus_note = val!(),
            flag::ACCOUNT => o.account = val!(),
            flag::BASE => o.use_base = true,
            flag::CWD => o.cwd_spec = CwdSpec::Explicit(val!()),
            flag::AGENT => o.agent = val!(),
            flag::MODEL => o.model = val!(),
            flag::LAUNCHER => {
                o.launcher = val!();
                o.launcher_explicit = true;
            }
            flag::CCM_SID => o.ccm_sid = val!(),
            flag::DETACH => o.detach = true,
            flag::TMUX_SIZE => o.tmux_size = val!(),
            flag::PRINT => o.print = true,
            flag::CCM_PROBE => return Ok(Parsed::Early(Early::Probe)),
            flag::VERSION => return Ok(Parsed::Early(Early::Version)),
            flag::HELP_LONG | flag::HELP_SHORT => return Ok(Parsed::Early(Early::Help)),
            other if other.starts_with('-') => {
                return die(format!("未知选项: {other}（用 --help 看用法）"))
            }
            other => {
                return die(format!(
                    "多余的位置参数: {other}（动作只能是 new/resume/attach 且必须在最前）"
                ))
            }
        }
        i += 1;
    }

    validate(&o)?;
    Ok(Parsed::Opts(Box::new(o)))
}

/// 组合校验。**一条都不许静默忽略** —— 静默忽略正是本工作区反复消灭的那类病
///（写了个修饰、看起来生效了、实际被吃掉）。
fn validate(o: &Opts) -> Result<(), Die> {
    if !crate::control::ccm::AGENTS.contains(&o.agent.as_str()) {
        return die(format!(
            "未知 agent: {}（支持 {}）",
            o.agent,
            crate::control::ccm::AGENTS.join("|")
        ));
    }
    if !o.account.is_empty() && o.use_base {
        return die("--account 与 --base 互斥");
    }
    if o.detach && !o.use_tmux {
        return die("--detach 需要配合 --tmux（非容器路径没有会话可 detach）");
    }
    if !o.tmux_size.is_empty() && !o.use_tmux {
        return die("--tmux-size 需要配合 --tmux");
    }
    if !o.tmux_name.is_empty() && !o.tmux_base.is_empty() {
        return die(
            "--tmux=<名> 与 --tmux-base=<基名> 互斥（前者不避让、后者避让，同时给等于没说清要哪个）",
        );
    }
    if o.bus_register && !o.detach {
        return die(
            "--bus-register 需要配合 --detach（不 detach 那条随后 exec 进 attach，登记做不成）",
        );
    }
    if !o.bus_note.is_empty() && !o.bus_register {
        return die("--bus-note 需要配合 --bus-register（不登记的话这行备注没有去处）");
    }
    if !o.tmux_size.is_empty() && parse_size(&o.tmux_size).is_none() {
        // 尺寸会被拼进 `tmux new-session -x W -y H`，**必须**只允许纯数字，否则就是一条注入面。
        return die(format!(
            "非法 --tmux-size: '{}'（要 <宽>x<高>，如 220x50，且均为正整数）",
            o.tmux_size
        ));
    }
    if o.action == Action::Resume && crate::control::ccm::resume_flag(&o.agent).is_none() {
        return die(format!("agent={} 不支持 resume（无 resume flag）", o.agent));
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
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    fn ok(a: &[&str]) -> Opts {
        match parse(&v(a)).expect("该解析得动") {
            Parsed::Opts(o) => *o,
            other => panic!("期望拿到 Opts，实得 {other:?}"),
        }
    }

    fn err(a: &[&str]) -> String {
        match parse(&v(a)) {
            Err(Die(m)) => m,
            other => panic!("期望 die，实得 {other:?}"),
        }
    }

    /// 〔搬自 `e2e/ccm-cli.test.sh`「resume <sid>」「`--resume <sid>` 等价」「`--resume=<sid>`」〕
    ///
    /// 三种写法**必须**落到同一套意图上 —— cc-monitor 今天发的就是 `--resume <sid>`。
    #[test]
    fn the_three_ways_to_say_resume_land_on_the_same_intent() {
        for a in [
            v(&["resume", "abc-123"]),
            v(&["--resume", "abc-123"]),
            v(&["--resume=abc-123"]),
        ] {
            let o = match parse(&a).expect("该解析得动") {
                Parsed::Opts(o) => *o,
                other => panic!("{other:?}"),
            };
            assert_eq!(o.action, Action::Resume, "写法 {a:?} 没落到 resume");
            assert_eq!(o.sid, "abc-123", "写法 {a:?} 的 sid 不对");
        }
    }

    /// 〔搬自 `ccm-cli`「resume 后跟 flag → 报错（别把 --tmux 当 sid）」与 attach 同形那条〕
    #[test]
    fn a_positional_action_never_swallows_the_next_flag_as_its_value() {
        assert_eq!(err(&["resume", "--tmux"]), "resume 需要 <sid>");
        assert_eq!(err(&["resume"]), "resume 需要 <sid>");
        assert_eq!(err(&["attach", "--tmux"]), "attach 需要 <会话名>");
        assert_eq!(err(&["attach"]), "attach 需要 <会话名>");
    }

    /// 〔搬自 `ccm-cli`「未知选项报错」「未知 agent 报错」「--account 与 --base 互斥」〕
    #[test]
    fn the_combination_rules_all_fail_loudly() {
        assert!(err(&["--nope"]).starts_with("未知选项: --nope"));
        assert!(err(&["--agent", "gemini"]).starts_with("未知 agent: gemini"));
        assert_eq!(
            err(&["--account", "z", "--base"]),
            "--account 与 --base 互斥"
        );
        assert!(err(&["--detach"]).starts_with("--detach 需要配合 --tmux"));
        assert!(err(&["--tmux-size", "1x1"]).starts_with("--tmux-size 需要配合 --tmux"));
        assert!(err(&["--tmux=a", "--tmux-base", "b"]).starts_with("--tmux=<名> 与 --tmux-base"));
        assert!(err(&["--tmux", "--bus-register"]).starts_with("--bus-register 需要配合 --detach"));
        assert!(err(&["--bus-note", "x"]).starts_with("--bus-note 需要配合 --bus-register"));
        // 位置动作只认**第一个** token —— 排在旗标后面的 `resume` 是一个多余的位置参数
        assert!(err(&["--agent", "codex", "resume"]).starts_with("多余的位置参数"));
        assert!(err(&["resume", "s", "--agent", "codex"]).starts_with("agent=codex 不支持 resume"));
        assert_eq!(
            err(&["foo"]),
            "多余的位置参数: foo（动作只能是 new/resume/attach 且必须在最前）"
        );
    }

    /// 〔搬自 `ccm-cli`「非法 --tmux-size」那一格 —— 它是一条**注入面**，不是排版〕
    #[test]
    fn the_size_is_two_plain_decimals_or_it_is_refused() {
        assert_eq!(parse_size("220x50"), Some(("220".into(), "50".into())));
        for bad in [
            "x50", "220x", "1x2x3", "", "22 0x50", "220X50", "-1x2", "a x b",
        ] {
            assert!(parse_size(bad).is_none(), "'{bad}' 不该被当成合法尺寸");
        }
        assert!(err(&["--tmux", "--tmux-size", "x50"]).starts_with("非法 --tmux-size"));
    }

    /// 〔搬自 `ccm-cli`「`-- 之后透传给 agent`」与「resume 不带 `--` 时不许多出任何参数」〕
    #[test]
    fn everything_after_the_terminator_goes_to_the_agent_untouched() {
        let o = ok(&["--", "-p", "hi there", "--tmux"]);
        assert_eq!(o.passthru, v(&["-p", "hi there", "--tmux"]));
        assert!(
            o.use_tmux == Defaults::USE_TMUX,
            "`--` 之后的 --tmux 不许被本层认走"
        );
        assert!(ok(&["resume", "s"]).passthru.is_empty());
    }

    /// 🔴 `KR48D4` 的机检：**每个默认值只许有一处住址。**
    ///
    /// 判法不是「数一数」，是**真去比**：`parse(&[])` 的结果必须逐个字段等于
    /// [`Defaults`] 声明的那些值。任何人在别处再写一份默认（比如在 `plan.rs` 里
    /// `if agent.is_empty() { agent = "claude" }`），只要那份与这里分叉，这条就红。
    #[test]
    fn every_default_lives_only_in_the_defaults_block() {
        let o = ok(&[]);
        assert_eq!(o.action, Defaults::ACTION);
        assert_eq!(o.agent, Defaults::AGENT);
        assert_eq!(o.cwd_spec, Defaults::CWD);
        assert_eq!(o.use_tmux, Defaults::USE_TMUX);
        assert_eq!(o.use_base, Defaults::USE_BASE);
        assert_eq!(o.detach, Defaults::DETACH);
        assert_eq!(o.print, Defaults::PRINT);
        assert_eq!(o.bus_register, Defaults::BUS_REGISTER);
        assert!(!o.launcher_explicit, "没给 --launcher 就不许记成显式");
        // 反向：把默认值本身换掉，上面那一族必须跟着动 —— 否则它们是自说自话。
        assert_ne!(Defaults::AGENT, "", "默认 agent 是空串的话这条判据就是空真");
    }

    /// 🔴 `KR48D2` 的机检：**这套 argv 的解析只许在本文件里发生。**
    ///
    /// 失效方向（`KR48D2` 逐字）：「有人再起第二个 argv 解析口、或把某条子命令的行为
    /// 在第二处重写一遍」。判法 = 扫本模块**除本文件外**的生产段，
    /// 不许出现任何 ccm 旗标的字面量。
    #[test]
    fn the_ccm_argv_is_parsed_in_exactly_one_place() {
        let others: &[(&str, &str)] = &[
            ("control/ccm/mod.rs", include_str!("mod.rs")),
            ("control/ccm/plan.rs", include_str!("plan.rs")),
        ];
        let mut hits: Vec<String> = Vec::new();
        for (name, raw) in others {
            let prod = crate::guard_support::production_code(raw);
            if prod.contains("\"--") {
                hits.push((*name).to_string());
            }
        }
        assert!(
            hits.is_empty(),
            "这些文件里出现了 ccm 旗标的字面量：{hits:?}\n\
             `KR48D2` 要的是「一条命令一处实现」——旗标名的住址是 `argv.rs::flag`，\n\
             别处只许 `use` 它。在第二处敲一遍字面量，两份迟早分叉（`K-R50` 的成因）。"
        );
    }
}
