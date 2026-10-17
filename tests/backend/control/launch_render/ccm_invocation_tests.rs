//! 那一行 `ccm …` 的渲染器（`control/launch_render/ccm_invocation.rs`）。期望串与能力清单全是手写的
//! （判据自带清单，不遍历被测常量 —— 遍历被测常量自己是恒真的）。

use super::*;

fn base_spec() -> CliSpec<'static> {
    CliSpec {
        agent: "claude",
        action: Action::New,
        container: Container::None,
        cwd: None,
        account: CliAccount::Base,
        ccm_sid: None,
        model: None,
        launcher: "claude",
        default_launcher: "claude",
        args: &[],
        ccm_path: "ccm",
        detach: false,
    }
}

/// 判据自带的无条件能力清单（不复用 `CLI_REQUIRED_CAPS`）。
const STATIC_CAPS_EXPECTED: &[&str] = &["new", "resume", "cwd", "launcher"];
/// 用到才要的那几条（容器 · 接回 · 维度各自声明的）。
const CONDITIONAL_CAPS_EXPECTED: &[&str] =
    &["tmux", "attach", "ccm-sid", "agent", "account", "model"];

fn caps_all() -> BTreeSet<String> {
    STATIC_CAPS_EXPECTED
        .iter()
        .chain(CONDITIONAL_CAPS_EXPECTED)
        .map(|c| (*c).to_string())
        .collect()
}

fn caps_without(missing: &str) -> BTreeSet<String> {
    let mut c = caps_all();
    assert!(
        c.remove(missing),
        "{missing} 本来就不在全集里，这条用例是空转"
    );
    c
}

fn render(spec: &CliSpec) -> Result<String, Refusal> {
    render_ccm_invocation(spec, &caps_all())
}

/// 无条件那几条每一条都单独被要；用到才要的那几条只在用到时要（Windows 那台没有 tmux 也照样起得了直路）。
#[test]
fn unconditional_caps_are_always_required_and_conditional_ones_only_when_used() {
    for c in STATIC_CAPS_EXPECTED {
        assert_eq!(
            render_ccm_invocation(&base_spec(), &caps_without(c)),
            Err(Refusal::MissingCap((*c).to_string())),
            "缺 {c} 照样渲出来了"
        );
    }
    let no_tmux = caps_without("tmux");
    let mut direct = base_spec();
    direct.action = Action::Resume { sid: "s1" };
    assert!(
        render_ccm_invocation(&direct, &no_tmux).is_ok(),
        "没有 tmux 的那台连直路都起不了"
    );
    let mut tmux = direct.clone();
    tmux.container = Container::Tmux {
        name: "cc-s1",
    };
    assert_eq!(
        render_ccm_invocation(&tmux, &no_tmux),
        Err(Refusal::MissingCap("tmux".into()))
    );
    let mut attach = base_spec();
    attach.action = Action::Attach { name: "cc-s1" };
    attach.container = Container::Tmux {
        name: "cc-s1",
    };
    assert_eq!(
        render_ccm_invocation(&attach, &caps_without("attach")),
        Err(Refusal::MissingCap("attach".into()))
    );
    let mut ident = base_spec();
    ident.ccm_sid = Some("s1");
    assert_eq!(
        render_ccm_invocation(&ident, &caps_without("ccm-sid")),
        Err(Refusal::DimensionNeedsCap {
            dim: "identity".into(),
            cap: "ccm-sid".into()
        })
    );
}

/// 每一句理由逐字（手写），变体人群与表两向相等。
#[test]
fn every_refusal_reason_is_pinned_byte_for_byte() {
    let q = |s: &str| format!("{s:?}");
    let pairs: Vec<(Refusal, String)> = vec![
        (
            Refusal::MissingCap("tmux".into()),
            copy_core::copy_text("rsCcmInvocation.refusal.missingCap", &[("c", "tmux")]).into(),
        ),
        (
            Refusal::AttachNeedsTmux,
            copy_core::copy_static!("rsCcmInvocation.refusal.attachNeedsTmux").into(),
        ),
        (Refusal::UnknownAgent("那一句".into()), "那一句".into()),
        (
            Refusal::AgentNoAccounts {
                agent: "Codex".into(),
            },
            copy_core::copy_text(
                "rsCcmInvocation.refusal.agentNoAccounts",
                &[("agent", "Codex")],
            )
            .into(),
        ),
        (
            Refusal::DimensionCannotSpeak("account".into()),
            copy_core::copy_text("rsCcmInvocation.refusal.cannotSpeak", &[("id", "account")])
                .into(),
        ),
        (
            Refusal::DimensionNeedsCap {
                dim: "model".into(),
                cap: "model".into(),
            },
            copy_core::copy_text(
                "rsCcmInvocation.refusal.dimensionNeedsCap",
                &[("dim", "model"), ("cap", "model")],
            )
            .into(),
        ),
        (
            Refusal::FreeTextRefused {
                slot: FreeTextSlot::Cwd,
                value: q("rel/dir"),
            },
            copy_core::copy_text(
                "rsCcmInvocation.refusal.freeTextCwd",
                &[("value", "\"rel/dir\"")],
            )
            .into(),
        ),
        (
            Refusal::IdentifierRefused {
                slot: IdentifierSlot::Model,
                value: q("-m"),
            },
            copy_core::copy_text("rsCcmInvocation.refusal.idModel", &[("value", "\"-m\"")]).into(),
        ),
    ];
    for (r, want) in &pairs {
        assert_eq!(&r.reason(), want, "{r:?} 的理由变了");
    }
    // 变体人群从枚举现抠（不手写个数），每一个都在表里。
    let src = include_str!("../../../../src/backend/control/launch_render/ccm_invocation.rs");
    let body = src
        .split("pub enum Refusal {")
        .nth(1)
        .and_then(|s| s.split("\n}").next())
        .expect("抠不到 `Refusal`");
    let variants: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|l| l.chars().next().is_some_and(char::is_uppercase))
        .map(|l| l.split(['(', ' ', ',', '{']).next().unwrap())
        .collect();
    assert_eq!(variants.len(), 8, "抠到的变体：{variants:?}");
    for v in &variants {
        assert!(
            pairs.iter().any(|(r, _)| format!("{r:?}").starts_with(v)),
            "变体 `{v}` 不在逐字表里"
        );
    }
}

/// 账号维度恒表态：`--base` · `--account <名>` · 继承（不吐）。
#[test]
fn the_account_dimension_has_its_five_shapes() {
    let with = |a: CliAccount<'static>| {
        let mut s = base_spec();
        s.account = a;
        render(&s)
    };
    assert_eq!(
        with(CliAccount::Base).unwrap(),
        "ccm -- new --ccm-agent claude --base"
    );
    assert_eq!(
        with(CliAccount::Named { name: "z" }).unwrap(),
        "ccm -- new --ccm-agent claude --account z"
    );
    assert_eq!(
        with(CliAccount::Inherit).unwrap(),
        "ccm -- new --ccm-agent claude"
    );
}

/// 全触发：每一段按维度表的顺序出现、两半各归各位（逐字节手写）。
#[test]
fn a_fully_loaded_invocation_emits_every_part_in_registry_order() {
    let spec = CliSpec {
        agent: "claude",
        action: Action::Resume { sid: "s1" },
        container: Container::Tmux {
            name: "cc-s1",
        },
        cwd: Some("/w d"),
        account: CliAccount::Named { name: "z" },
        ccm_sid: Some("s1"),
        model: Some("opus"),
        launcher: "ccr code",
        default_launcher: "claude",
        args: &["--verbose"],
        ccm_path: "ccm",
        detach: false,
    };
    assert_eq!(
        render(&spec).unwrap(),
        format!(
            "ccm --resume s1 --model opus --verbose -- --ccm-tmux=cc-s1 --ccm-sid=s1 --ccm-agent claude --account z \
             --cwd '/w d' --launcher 'ccr code'"
        )
    );
}

/// 拼进命令之前每一格各过各的那一条：sid · 身份标记 · 模型 · 账号名 · 账号目录 · 身份 token · 会话名 · cwd · 透传参数 · 接回目标。
#[test]
fn every_value_is_judged_before_it_becomes_a_ccm_argument() {
    let cases: Vec<(CliSpec, &str)> = vec![
        (
            CliSpec {
                action: Action::Resume { sid: "--evil" },
                ..base_spec()
            },
            "IdentifierRefused { slot: Sid",
        ),
        (
            CliSpec {
                ccm_sid: Some("a b"),
                ..base_spec()
            },
            "IdentifierRefused { slot: CcmSid",
        ),
        (
            CliSpec {
                model: Some("-m"),
                ..base_spec()
            },
            "IdentifierRefused { slot: Model",
        ),
        (
            CliSpec {
                account: CliAccount::Named { name: "a b" },
                ..base_spec()
            },
            "IdentifierRefused { slot: Account",
        ),
        (
            CliSpec {
                container: Container::Tmux {
                    name: "-x",
                },
                ..base_spec()
            },
            "IdentifierRefused { slot: TmuxName",
        ),
        (
            CliSpec {
                cwd: Some("rel"),
                ..base_spec()
            },
            "FreeTextRefused { slot: Cwd",
        ),
        (
            CliSpec {
                // 交给 agent 的参数可以跨行（多行的第一句）；CR 键进终端就是回车键，照拒。
                args: &["a\rb"],
                ..base_spec()
            },
            "FreeTextRefused { slot: AgentArg",
        ),
        (
            CliSpec {
                action: Action::Attach { name: "a\u{7}b" },
                container: Container::Tmux { name: "a\u{7}b" },
                ..base_spec()
            },
            "FreeTextRefused { slot: AttachTarget",
        ),
    ];
    for (spec, want) in cases {
        let got = format!("{:?}", render(&spec));
        assert!(got.contains(want), "期望 {want}，实得 {got}");
    }
}

/// argv 的 quote：允许集里的字符裸写，其余（含空串）包单引号。
#[test]
fn argv_leaves_the_allowed_characters_bare_and_quotes_the_rest() {
    const ARGV_BARE_CHARS: &str = "_@%+=:,./-";
    for c in ARGV_BARE_CHARS.chars() {
        let t = format!("a{c}b");
        assert_eq!(argv(&t), t, "{c:?} 被引上了");
    }
    for t in ["", "a b", "a'b", "a$b", "文"] {
        assert_eq!(argv(t), shell_quote_core::posix_quote(t), "{t:?}");
    }
}

/// 按会话的那一家渲：resume 用那一家的写法（Codex 是子命令 `resume <sid>`），`--ccm-agent` 恒显式（Claude 也写）。
/// 旧形（不说是哪一家、一律 `--resume`）在这里不可能再渲出来。
#[test]
fn resume_is_written_the_way_that_agent_resumes_and_the_agent_is_always_said() {
    let mut s = base_spec();
    s.action = Action::Resume { sid: "s1" };
    s.cwd = Some("/w");
    s.agent = "codex";
    assert_eq!(
        render(&s).unwrap(),
        "ccm resume s1 -- --ccm-agent codex --base --cwd /w"
    );
    s.agent = "claude";
    assert_eq!(
        render(&s).unwrap(),
        "ccm --resume s1 -- --ccm-agent claude --base --cwd /w"
    );
    // 起新会话也说是哪一家；接回不起 agent ⇒ 不写。
    let mut n = base_spec();
    n.agent = "codex";
    assert_eq!(render(&n).unwrap(), "ccm -- new --ccm-agent codex --base");
    n.action = Action::Attach { name: "cc-x" };
    n.container = Container::Tmux {
        name: "cc-x",
    };
    assert_eq!(render(&n).unwrap(), "ccm -- --attach cc-x");
    // 说不出是哪一家（空 · 带空白 · 注册表里没有）⇒ 拒，那一句列出认得的几家；不落默认那一家。
    for bad in ["", " codex", "gemini"] {
        let mut b = base_spec();
        b.agent = bad;
        b.action = Action::Resume { sid: "s1" };
        match render(&b) {
            Err(Refusal::UnknownAgent(said)) => {
                assert!(said.contains("claude / codex"), "{bad:?}：{said}")
            }
            other => panic!("{bad:?} 没被拒：{other:?}"),
        }
    }
}

/// 没有账号这一维的那一家（Codex）：具名账号 ⇒ 拒、明说；`--base` / 继承照常渲。Claude 选号照旧。
#[test]
fn an_agent_without_accounts_refuses_a_named_account_and_says_so() {
    let mut s = base_spec();
    s.agent = "codex";
    s.action = Action::Resume { sid: "s1" };
    for named in [CliAccount::Named { name: "z" }] {
        s.account = named;
        let r = render(&s);
        assert_eq!(
            r,
            Err(Refusal::AgentNoAccounts {
                agent: "Codex".into()
            })
        );
        assert_eq!(
            r.unwrap_err().reason(),
            copy_core::copy_text(
                "rsCcmInvocation.refusal.agentNoAccounts",
                &[("agent", "Codex")]
            )
        );
    }
    s.account = CliAccount::Inherit;
    assert_eq!(render(&s).unwrap(), "ccm resume s1 -- --ccm-agent codex");
    s.agent = "claude";
    s.account = CliAccount::Named { name: "z" };
    assert_eq!(
        render(&s).unwrap(),
        "ccm --resume s1 -- --ccm-agent claude --account z"
    );
}

/// 那台 ccm 会的那几样（整份；拒的那几格各缺一样）。
const ALL_CAPS: &[&str] = &[
    "new", "resume", "attach", "tmux", "account", "model", "cwd", "agent", "launcher", "ccm-sid",
    "print", "detach", "tmux-size",
];

/// 起什么（那一家的启动命令照缺省的偏好）；各格只改要改的那几样。
fn launch(agent: &str, act: super::super::Act) -> super::super::Launch {
    let (launcher, default_launcher) =
        super::super::Launch::launchers(agent, &crate::control::machine_prefs::Prefs::default());
    super::super::Launch {
        agent: agent.into(),
        act,
        tmux: None,
        cwd: None,
        ccm_sid: None,
        model: None,
        launcher,
        default_launcher,
        preset_args: Vec::new(),
    }
}

/// 那台判好的号：具名那一形带这台模型偏好里那个号的那一条。
fn named(name: &str, model: Option<&str>) -> crate::control::launch_account::Settled {
    crate::control::launch_account::Settled::Account(crate::control::launch_account::LaunchedAccount {
        name: name.into(),
        config_dir: format!("/h/.cc/{name}"),
        model: model.map(str::to_string),
    })
}

/// 一格：哪一形 · 起什么 · 那台判好的号 · 那台 ccm 缺哪一样（`None` ＝ 整份）· 那一行或拒的那一句。
pub(crate) type Cell = (
    &'static str,
    super::super::Launch,
    crate::control::launch_account::Settled,
    Option<&'static str>,
    Result<&'static str, &'static str>,
);

/// **那一行 `ccm …` 的字节只在这一张表里钉**（远端那一形；别处要「渲得出来的每一行」也取这一张）。
pub(crate) fn cells() -> Vec<Cell> {
    use super::super::{Act, Launch};
    use crate::control::launch_account::Settled;
    let resume = |sid: &str| Act::Resume(sid.into());
    let tmux = |mut l: Launch, name: &str| {
        l.tmux = Some(name.into());
        l
    };
    let cwd = |mut l: Launch, c: &str| {
        l.cwd = Some(c.into());
        l
    };
    let attach = |name: &str| tmux(launch("claude", Act::Attach(name.into())), name);
    let claude = |act| launch("claude", act);
    let mut ident = claude(Act::New);
    ident.ccm_sid = Some("sid-1".into());
    let mut own_launcher = claude(Act::New);
    own_launcher.launcher = "mycc".into();
    let mut same_launcher = claude(Act::New);
    same_launcher.launcher = "claude".into();
    let mut probe = cwd(tmux(claude(Act::New), "cc-proj"), "/home/pi/my proj");
    probe.launcher = "CCMPROBE".into();
    let mut p1 = cwd(tmux(claude(resume("p1")), "cc-p1"), "/tmp");
    p1.ccm_sid = Some("p1".into());
    let mut p1_opus = p1.clone();
    p1_opus.model = Some("opus".into());
    vec![
        ("new + base", claude(Act::New), Settled::Base, None, Ok("ccm -- new --ccm-agent claude --base")),
        ("new + 具名", claude(Act::New), named("z", None), None, Ok("ccm -- new --ccm-agent claude --account z")),
        (
            "resume + tmux + 具名",
            tmux(claude(resume("abc-123")), "cc-abc123"),
            named("z", None),
            None,
            Ok("ccm --resume abc-123 -- --ccm-tmux=cc-abc123 --ccm-agent claude --account z"),
        ),
        (
            "resume + cwd + 号带来的模型",
            cwd(claude(resume("s1")), "/w"),
            named("z", Some("opus")),
            None,
            Ok("ccm --resume s1 --model opus -- --ccm-agent claude --account z --cwd /w"),
        ),
        ("身份标记", ident, Settled::Base, None, Ok("ccm -- new --ccm-sid=sid-1 --ccm-agent claude --base")),
        ("自定义启动命令", own_launcher, Settled::Base, None, Ok("ccm -- new --ccm-agent claude --base --launcher mycc")),
        ("启动命令等于默认 ⇒ 不写 --launcher", same_launcher, Settled::Base, None, Ok("ccm -- new --ccm-agent claude --base")),
        ("接回（不起 agent，不收修饰）", attach("cc-foo"), Settled::Base, None, Ok("ccm -- --attach cc-foo")),
        (
            "要引号的目录",
            cwd(claude(Act::New), "/home/用户/带 空格"),
            Settled::Base,
            None,
            Ok("ccm -- new --ccm-agent claude --base --cwd '/home/用户/带 空格'"),
        ),
        ("缺无条件能力 cwd", claude(Act::New), Settled::Base, Some("cwd"), Err("此机 ccm 不支持这样启动会话 · 缺 cwd")),
        (
            "建 tmux 会话而这台没有 tmux",
            tmux(claude(Act::New), "cc-x"),
            Settled::Base,
            Some("tmux"),
            Err("此机 ccm 不支持这样启动会话 · 缺 tmux"),
        ),
        (
            "用到的模型维度缺能力",
            claude(Act::New),
            named("z", Some("opus")),
            Some("model"),
            Err("此机 ccm 不认设置项 model · 缺 model"),
        ),
        ("用到的账号维度缺能力", claude(Act::New), Settled::Base, Some("account"), Err("此机 ccm 不认设置项 account · 缺 account")),
        (
            "坏 sid",
            claude(resume("-x")),
            Settled::Base,
            None,
            Err("会话 ID \"-x\" 不合法 · 1–64 位 · 只许 A-Z a-z 0-9 与 - · 不以 - 开头"),
        ),
        (
            "Codex 恢复（直路，跟随落账号 0）",
            cwd(launch("codex", resume("s1")), "/p"),
            Settled::Unsaid,
            None,
            Ok("ccm resume s1 -- --ccm-agent codex --base --cwd /p"),
        ),
        ("Codex 选了具名账号", cwd(launch("codex", resume("s1")), "/p"), named("z", None), None, Err("Codex 会话不可选账号")),
        (
            "直路恢复 ＋ 具名",
            cwd(claude(resume("s1")), "/p"),
            named("z", None),
            None,
            Ok("ccm --resume s1 -- --ccm-agent claude --account z --cwd /p"),
        ),
        ("接回 cc-s1", attach("cc-s1"), Settled::Base, None, Ok("ccm -- --attach cc-s1")),
        (
            "恢复进 tmux ＋ 身份标记",
            p1,
            Settled::Base,
            None,
            Ok("ccm --resume p1 -- --ccm-tmux=cc-p1 --ccm-sid=p1 --ccm-agent claude --base --cwd /tmp"),
        ),
        (
            "新起进 tmux ＋ 自定义启动命令",
            probe,
            Settled::Base,
            None,
            Ok("ccm -- new --ccm-tmux=cc-proj --ccm-agent claude --base --cwd '/home/pi/my proj' --launcher CCMPROBE"),
        ),
        ("接回 cc-p1", attach("cc-p1"), Settled::Base, None, Ok("ccm -- --attach cc-p1")),
        (
            "恢复进 tmux ＋ 点名模型",
            p1_opus,
            Settled::Base,
            None,
            Ok("ccm --resume p1 --model opus -- --ccm-tmux=cc-p1 --ccm-sid=p1 --ccm-agent claude --base --cwd /tmp"),
        ),
    ]
}

/// 那台 ccm 会的那几样，缺 `missing` 那一样。
pub(crate) fn caps_without_one(missing: Option<&str>) -> BTreeSet<String> {
    ALL_CAPS
        .iter()
        .filter(|c| Some(**c) != missing)
        .map(|c| (*c).to_string())
        .collect()
}

/// 那一格渲出来的那一行（远端那一形）。
pub(crate) fn render_cell(c: &Cell) -> Result<String, String> {
    super::super::line_of(&c.1, &c.2, super::super::Here::Remote, &caps_without_one(c.3), "ccm")
}

/// ★ 起什么 × 那台判好的号 × 那台 ccm 会的那几样 ⇒ 那一行，或拒的那一句，逐字节。
#[test]
fn every_launch_renders_byte_for_byte() {
    let mut bad = Vec::new();
    for c in cells() {
        let got = render_cell(&c);
        if got.as_deref().map_err(String::as_str) != c.4 {
            bad.push(format!("「{}」\n  期望 {:?}\n  实得 {got:?}", c.0, c.4));
        }
    }
    assert!(bad.is_empty(), "那一行与表里不一致：\n{}", bad.join("\n"));
}

/// 按 POSIX 单引号规则切一行（渲染器只用 `posix_quote` 的单引号形，`'\''` 那一形也认）。
pub(crate) fn shell_words(line: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted, mut any) = (Vec::new(), String::new(), false, false);
    for c in line.chars() {
        match c {
            '\'' => {
                quoted = !quoted;
                any = true;
            }
            ' ' if !quoted => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                any = false;
            }
            '\\' if !quoted => {}
            c => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// ★★ 起会话只有 ccm 一处，且与 `ccm` 那套 argv 的**唯一解析口**对得上（异源：渲染一侧 vs 解析一侧）：
/// 表里渲得出来的每一行都是一行 `ccm …`、都被 `ccm_line::argv::parse` 收下。
/// 它是 `protocol_doc_guard::TERMINAL_SURFACE_FILES` 里 `ccm_invocation.rs` 那一格的接盘判据。
#[test]
fn every_rendered_ccm_line_is_accepted_by_the_ccm_argv() {
    let mut checked = 0;
    for c in cells() {
        let Ok(line) = render_cell(&c) else { continue };
        let words = shell_words(&line);
        assert_eq!(words.first().map(String::as_str), Some("ccm"), "「{}」不是一行 ccm：{line}", c.0);
        let parsed = crate::ccm_line::argv::parse(&words[1..]);
        assert!(parsed.is_ok(), "渲染器吐的「{}」那一行 `ccm` 自己不认：{line}（{:?}）", c.0, parsed.err());
        checked += 1;
    }
    assert!(checked > 0, "表里一行都没渲出来 —— 上面那条空转着绿");
    assert_eq!(
        shell_words("ccm -- new --cwd '/home/用户/带 空格'"),
        ["ccm", "--", "new", "--cwd", "/home/用户/带 空格"],
        "切词器自己坏了"
    );
}

/// Windows 本机：没有 tmux ⇒ 说了建进 tmux 也走直路（不带会话名、不打身份标记），那一行是 PowerShell 那一形；接回说不出 ⇒ 拒。
#[test]
fn windows_launches_go_the_direct_way_and_attach_is_refused() {
    use super::super::{line_of, Act, Here};
    use crate::control::launch_account::Settled;
    let win = Here::Local { windows: true };
    let caps = caps_without_one(None);
    let mut l = launch("claude", Act::Resume("s1".into()));
    l.tmux = Some("cc-x".into());
    l.ccm_sid = Some("s1".into());
    let line = line_of(&l, &Settled::Base, win, &caps, "C:/cc/ccm.exe").unwrap();
    assert!(line.starts_with("& 'C:/cc/ccm.exe' --resume s1 -- "), "{line}");
    assert!(!line.contains("--ccm-tmux") && !line.contains("--ccm-sid"), "{line}");
    let mut attach = launch("claude", Act::Attach("cc-x".into()));
    attach.tmux = Some("cc-x".into());
    assert!(line_of(&attach, &Settled::Base, win, &caps, "C:/cc/ccm.exe").is_err());
}
