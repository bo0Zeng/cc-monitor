//! 那一行 `ccm …` 的渲染器（`control/launch_render/ccm_invocation.rs`）。期望串与能力清单全是手写的
//! （判据自带清单，不遍历被测常量 —— 遍历被测常量自己是恒真的）。

use super::test_support::{caps_without_one, cells, launch, render_cell, shell_words};
use super::*;
use crate::control::launch_account::{LaunchedAccount, Settled};

/// 判好的号（夹具：渲染器只借用它，这里给一份活到进程结束的）。
fn picked(name: &str) -> &'static Settled {
    Box::leak(Box::new(Settled::Account(LaunchedAccount {
        name: name.into(),
        config_dir: format!("/h/.cc/{name}"),
        model: None,
    })))
}
fn base() -> &'static Settled {
    Box::leak(Box::new(Settled::Base))
}
fn inherit() -> &'static Settled {
    Box::leak(Box::new(Settled::Unsaid))
}

fn base_spec() -> CliSpec<'static> {
    CliSpec {
        agent: "claude",
        action: Action::New,
        container: Container::None,
        cwd: None,
        account: base(),
        model: None,
        launcher: "claude",
        default_launcher: "claude",
        args: &[],
        ccm_path: "ccm",
        alias: None,
        detach: false,
        windows: false,
    }
}

/// 判据自带的无条件能力清单（不复用 `CLI_REQUIRED_CAPS`）。
const STATIC_CAPS_EXPECTED: &[&str] = &["new", "resume", "cwd", "launcher"];
/// 用到才要的那几条（容器 · 接回 · 维度各自声明的）。
const CONDITIONAL_CAPS_EXPECTED: &[&str] = &["tmux", "attach", "agent", "account", "model"];

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
    tmux.container = Container::Tmux { name: "cc-s1" };
    assert_eq!(
        render_ccm_invocation(&tmux, &no_tmux),
        Err(Refusal::MissingCap("tmux".into()))
    );
    let mut attach = base_spec();
    attach.action = Action::Attach { name: "cc-s1" };
    attach.container = Container::Tmux { name: "cc-s1" };
    assert_eq!(
        render_ccm_invocation(&attach, &caps_without("attach")),
        Err(Refusal::MissingCap("attach".into()))
    );
}

/// 每一句理由逐字（手写），变体人群与表两向相等。
#[test]
fn every_refusal_reason_is_pinned_byte_for_byte() {
    let q = |s: &str| format!("{s:?}");
    let pairs: Vec<(Refusal, String)> = vec![
        (
            Refusal::MissingCap("tmux".into()),
            copy_core::copy_text("rsCcmInvocation.refusal.missingCap", &[("c", "tmux")]),
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
            ),
        ),
        (
            Refusal::DimensionCannotSpeak("account".into()),
            copy_core::copy_text("rsCcmInvocation.refusal.cannotSpeak", &[("id", "account")]),
        ),
        (
            Refusal::DimensionNeedsCap {
                dim: "model".into(),
                cap: "model".into(),
            },
            copy_core::copy_text(
                "rsCcmInvocation.refusal.dimensionNeedsCap",
                &[("dim", "model"), ("cap", "model")],
            ),
        ),
        (
            Refusal::FreeTextRefused {
                slot: FreeTextSlot::Cwd,
                value: q("rel/dir"),
            },
            copy_core::copy_text(
                "rsCcmInvocation.refusal.freeTextCwd",
                &[("value", "\"rel/dir\"")],
            ),
        ),
        (
            Refusal::IdentifierRefused {
                slot: IdentifierSlot::Model,
                value: q("-m"),
            },
            copy_core::copy_text("rsCcmInvocation.refusal.idModel", &[("value", "\"-m\"")]),
        ),
    ];
    for (r, want) in &pairs {
        assert_eq!(&r.reason(), want, "{r:?} 的理由变了");
    }
    // 变体人群从枚举现抠（不手写个数），每一个都在表里。
    let src = include_str!("../../../../src/backend/control/launch_render/ccm_invocation.rs");
    let body = src
        .split("pub(crate) enum Refusal {")
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
    let with = |a: &'static Settled| {
        let mut s = base_spec();
        s.account = a;
        render(&s)
    };
    assert_eq!(
        with(base()).unwrap(),
        "ccm -- new --ccm-agent claude --base"
    );
    assert_eq!(
        with(picked("z")).unwrap(),
        "ccm -- new --ccm-agent claude --account z"
    );
    assert_eq!(with(inherit()).unwrap(), "ccm -- new --ccm-agent claude");
}

/// 全触发：每一段按维度表的顺序出现、两半各归各位（逐字节手写）。
#[test]
fn a_fully_loaded_invocation_emits_every_part_in_registry_order() {
    let spec = CliSpec {
        agent: "claude",
        action: Action::Resume { sid: "s1" },
        container: Container::Tmux { name: "cc-s1" },
        cwd: Some("/w d"),
        account: picked("z"),
        model: Some("opus"),
        launcher: "ccr code",
        default_launcher: "claude",
        args: &["--verbose"],
        ccm_path: "ccm",
        alias: None,
        detach: false,
        windows: false,
    };
    assert_eq!(
        render(&spec).unwrap(),
        "ccm --resume s1 --model opus --verbose -- --ccm-tmux=cc-s1 --ccm-agent claude --account z \
             --cwd '/w d' --launcher 'ccr code'".to_string()
    );
}

/// ★ 模型那一维照那一家的 `model_args` 出（不认哪一家：注册表里由我们起的每一家都过一遍）：argv 里模型参数恰出现一次、
/// 恰是那一家给的那几个；那一家起会话选不了模型 ⇒ 整条拒（说不出那一维），不渲一条丢了模型的命令。
#[test]
fn the_model_dimension_speaks_each_agents_own_model_args_exactly_once() {
    let kinds = crate::agents::launchable_kinds();
    assert!(!kinds.is_empty());
    for kind in kinds {
        let (_, face) = crate::agents::pick_kind(Some(kind)).unwrap();
        let spec = CliSpec {
            agent: kind,
            action: Action::Resume { sid: "s1" },
            model: Some("m1"),
            launcher: face.default_launcher,
            default_launcher: face.default_launcher,
            ..base_spec()
        };
        let got = ccm_argv(&spec, &caps_all());
        let Some(args) = face.model_args else {
            assert_eq!(
                got,
                Err(Refusal::DimensionCannotSpeak("model".into())),
                "{kind}"
            );
            continue;
        };
        let want = args("m1", None);
        let argv = got.unwrap_or_else(|r| panic!("{kind}: {r:?}"));
        let hits = argv
            .windows(want.len())
            .filter(|w| *w == want.as_slice())
            .count();
        assert_eq!(hits, 1, "{kind}: {argv:?} 里 {want:?} 不是恰一次");
        assert_eq!(
            argv.iter().filter(|a| *a == "m1").count(),
            1,
            "{kind}: 模型出了不止一遍"
        );
    }
}

/// 拼进命令之前每一格各过各的那一条：sid · 模型 · 账号名 · 账号目录 · 身份 token · 会话名 · cwd · 透传参数 · 接回目标。
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
                model: Some("-m"),
                ..base_spec()
            },
            "IdentifierRefused { slot: Model",
        ),
        (
            CliSpec {
                account: picked("a b"),
                ..base_spec()
            },
            "IdentifierRefused { slot: Account",
        ),
        (
            CliSpec {
                container: Container::Tmux { name: "-x" },
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
        (
            // `:` 是 tmux 目标语法的分隔符、真会话名里不会有 ⇒ 当场拒，不交给 tmux 再报错。
            CliSpec {
                action: Action::Attach { name: "a:b" },
                container: Container::Tmux { name: "a:b" },
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
    n.container = Container::Tmux { name: "cc-x" };
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

/// 具名账号照「这一家有没有账号这一维」认，不写死哪一家：声明了账号面的每一家 ⇒ 那一行带 `--account <名>`；
/// 没声明的 ⇒ 拒、明说。继承（没说）每一家都照常渲、不带号。
#[test]
fn a_named_account_follows_whether_that_agent_has_accounts() {
    let mut with_accounts = 0;
    for kind in crate::agents::REGISTRY.iter().map(|a| a.kind) {
        let mut s = base_spec();
        s.agent = kind;
        s.account = picked("z");
        let r = render(&s);
        if crate::agents::account_env_of(kind).is_some() {
            with_accounts += 1;
            assert_eq!(
                r,
                Ok(format!("ccm -- new --ccm-agent {kind} --account z")),
                "{kind}"
            );
        } else {
            assert!(
                matches!(r, Err(Refusal::AgentNoAccounts { .. })),
                "{kind} 没有账号面，具名号该被拒：{r:?}"
            );
        }
        s.account = inherit();
        assert_eq!(
            render(&s),
            Ok(format!("ccm -- new --ccm-agent {kind}")),
            "{kind}"
        );
    }
    assert!(
        with_accounts > 0,
        "注册表里一家声明了账号面的都没有 —— 上面带号那一半空转着绿"
    );
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

/// ★★ 起会话只有 ccm 一处，且与 `ccm` 那套 argv 的**唯一解析口**对得上（异源：渲染一侧 vs 解析一侧）：
/// 表里渲得出来的每一行都是一行 `ccm …`、都被 `ccm_line::argv::parse_layered` 收下。
/// 它是 `protocol_doc_guard::TERMINAL_SURFACE_FILES` 里 `ccm_invocation.rs` 那一格的接盘判据。
#[test]
fn every_rendered_ccm_line_is_accepted_by_the_ccm_argv() {
    let mut checked = 0;
    for c in cells() {
        let Ok(line) = render_cell(&c) else { continue };
        let words = shell_words(&line);
        assert_eq!(
            words.first().map(String::as_str),
            Some("ccm"),
            "「{}」不是一行 ccm：{line}",
            c.0
        );
        let parsed = crate::ccm_line::argv::test_support::parse(&words[1..]);
        assert!(
            parsed.is_ok(),
            "渲染器吐的「{}」那一行 `ccm` 自己不认：{line}（{:?}）",
            c.0,
            parsed.err()
        );
        checked += 1;
    }
    assert!(checked > 0, "表里一行都没渲出来 —— 上面那条空转着绿");
    assert_eq!(
        shell_words("ccm -- new --cwd '/home/用户/带 空格'"),
        ["ccm", "--", "new", "--cwd", "/home/用户/带 空格"],
        "切词器自己坏了"
    );
}

/// Windows 本机：没有 tmux ⇒ 说了建进 tmux 也走直路（不带会话名），那一行是 PowerShell 那一形；
/// 接回只会是托管终端（A3-3-34）⇒ `ccm -- --attach <名>` 那一行，瘦客户端接得上。
#[test]
fn windows_launches_go_the_direct_way_and_attach_is_the_thin_client() {
    use super::super::{line_of, Act, Here};
    use crate::control::launch_account::Settled;
    let win = Here::Local { windows: true };
    let caps = caps_without_one(None);
    let mut l = launch("claude", Act::Resume("s1".into()));
    l.tmux = Some("cc-x".into());
    let line = line_of(&l, &Settled::Base, win, &caps, "C:/cc/ccm.exe").unwrap();
    assert!(
        line.starts_with("& 'C:/cc/ccm.exe' --resume s1 -- "),
        "{line}"
    );
    assert!(
        !line.contains("--ccm-tmux") && !line.contains("--ccm-sid"),
        "{line}"
    );
    let mut attach = launch("claude", Act::Attach("cc-x".into()));
    attach.tmux = Some("cc-x".into());
    let line = line_of(&attach, &Settled::Base, win, &caps, "C:/cc/ccm.exe").expect("接得回");
    assert!(
        line.starts_with("& 'C:/cc/ccm.exe' -- --attach ") && line.contains("cc-x"),
        "{line}"
    );
}

/// Windows 本机的托管终端：工作目录是 Windows 绝对路径（盘符根 · UNC），照那台的平台认；
/// 同一个值交给远端（POSIX）照拒；`..` 段 · 换行 · NUL · 相对路径两形都拒。
#[test]
fn a_windows_hosted_terminal_takes_a_windows_absolute_cwd() {
    use super::super::{hosted_argv_of, Act, Here};
    use crate::control::launch_account::Settled;
    let win = Here::Local { windows: true };
    let caps = caps_without_one(None);
    let hosted = |cwd: &str, here: Here| {
        let mut l = launch("claude", Act::New);
        l.cwd = Some(cwd.into());
        hosted_argv_of(&l, &Settled::Base, here, &caps, "C:/cc/ccm.exe")
    };
    for good in [
        r"C:\Users\x\proj",
        "C:/Users/x/照片 (2019)",
        r"\\nas\share\d",
        r"D:\",
    ] {
        let argv = hosted(good, win).unwrap_or_else(|e| panic!("{good:?} 被拒：{e}"));
        let at = argv.iter().position(|t| t == "--cwd").expect("带 --cwd");
        assert_eq!(argv.get(at + 1).map(String::as_str), Some(good), "{argv:?}");
    }
    let cwd_refused = copy_core::copy_text(
        "rsCcmInvocation.refusal.freeTextCwd",
        &[("value", &format!("{:?}", r"C:\Users\x\proj"))],
    );
    assert_eq!(
        hosted(r"C:\Users\x\proj", Here::Remote),
        Err(cwd_refused),
        "远端是 POSIX：盘符路径不是绝对路径"
    );
    for bad in [
        r"C:\a\..\b",
        "C:/a/../b",
        "C:\\a\nb",
        "C:\\a\0b",
        "C:\\a\rb",
        "rel",
        "C:rel",
        "/home/u",
    ] {
        assert!(hosted(bad, win).is_err(), "Windows 那一形放行了 {bad:?}");
    }
}

/// 经别名起：`@别名` 紧跟入口（ccm 只认第一个词是 `@名`）；PowerShell 那一行写成字面量（裸的 `@名` 在那里是摊开变量）。
#[test]
fn an_alias_goes_right_after_the_entry_and_is_a_literal_in_powershell() {
    let spec = CliSpec {
        alias: Some("dev"),
        ..base_spec()
    };
    let argv = ccm_argv(&spec, &caps_all()).unwrap();
    assert_eq!(argv.get(1).map(String::as_str), Some("@dev"), "{argv:?}");
    let ps = render_ccm_invocation_ps(&spec, &caps_all()).unwrap();
    assert!(ps.contains(" '@dev' "), "{ps}");
    let sh = render_ccm_invocation(&spec, &caps_all()).unwrap();
    assert!(sh.starts_with("ccm @dev "), "{sh}");
}

/// 起会话那一行的渲染也只问注册表那一格：喂一行没声明号载体的替身 ⇒ 具名号拒、明说；同一行声明了 ⇒ 带 `--account <名>`。
#[test]
fn a_stand_in_without_an_account_carrier_refuses_a_named_account_in_the_line() {
    let kind = crate::agents::fake::AGENT_KIND;
    let with = crate::agents::fake::full_row(|| None);
    let bare = crate::agents::Adapter {
        account_env: None,
        ..crate::agents::fake::full_row(|| None)
    };
    let mut s = base_spec();
    s.agent = kind;
    s.account = picked("z");
    assert!(
        matches!(
            ccm_argv_among(std::slice::from_ref(&bare), &s, &caps_all()),
            Err(Refusal::AgentNoAccounts { .. })
        ),
        "没有号载体的替身，具名号该被拒"
    );
    let line = ccm_argv_among(std::slice::from_ref(&with), &s, &caps_all()).expect("照收");
    assert!(
        line.windows(2).any(|w| w[0] == "--account" && w[1] == "z"),
        "{line:?}"
    );
}

/// 认是哪一家只问传进来的那份注册表：替身注册表里只有替身那一家时，说不出是哪一家（空 · 带空白 · 没有这一家）
/// ⇒ 拒的那句列的是替身那一家，不是生产注册表里的几家。
#[test]
fn an_unknown_agent_is_refused_listing_the_registry_that_was_passed_in() {
    let reg = [crate::agents::fake::full_row(|| None)];
    for bad in ["", " fake", "gemini"] {
        let mut s = base_spec();
        s.agent = bad;
        match ccm_argv_among(&reg, &s, &caps_all()) {
            Err(Refusal::UnknownAgent(said)) => {
                assert!(
                    said.contains(crate::agents::fake::AGENT_KIND),
                    "{bad:?}：{said}"
                );
                assert!(!said.contains("claude"), "{bad:?} 列了生产注册表：{said}");
            }
            other => panic!("{bad:?} 没被拒：{other:?}"),
        }
    }
}

/// 模型那一维照传进来那份注册表里那一家的 `model_args` 出：替身注册表里的替身那一家 ⇒ 那一行带替身给的模型参数。
#[test]
fn the_model_dimension_speaks_the_passed_registrys_model_args() {
    let row = crate::agents::fake::full_row(|| None);
    let face = row.launch.expect("替身由我们起");
    let want = (face.model_args.expect("替身选得了模型"))("m1", None);
    let spec = CliSpec {
        agent: crate::agents::fake::AGENT_KIND,
        model: Some("m1"),
        launcher: face.default_launcher,
        default_launcher: face.default_launcher,
        ..base_spec()
    };
    let argv = ccm_argv_among(std::slice::from_ref(&row), &spec, &caps_all())
        .unwrap_or_else(|r| panic!("替身那一家带模型该照收：{r:?}"));
    assert_eq!(
        argv.windows(want.len())
            .filter(|w| *w == want.as_slice())
            .count(),
        1,
        "{argv:?} 里 {want:?} 不是恰一次"
    );
}
