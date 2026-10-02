//! 那一行 `ccm …` 的渲染器（`control/launch_render/ccm_invocation.rs`）。期望串与能力清单全是手写的
//! （判据自带清单，不遍历被测常量 —— 遍历被测常量自己是恒真的）。

use super::*;

fn base_spec() -> CliSpec<'static> {
    CliSpec {
        action: Action::New,
        container: Container::None,
        cwd: None,
        account: CliAccount::Base,
        ccm_sid: None,
        model: None,
        launcher: "claude",
        default_launcher: "claude",
        args: &[],
        rbind_token: None,
        launch_id: None,
        ccm_path: "ccm",
        detach: false,
    }
}

/// 判据自带的无条件能力清单（不复用 `CLI_REQUIRED_CAPS`）。
const STATIC_CAPS_EXPECTED: &[&str] = &["new", "resume", "cwd", "launcher"];
/// 用到才要的那几条（容器 · 接回 · 维度各自声明的）。
const CONDITIONAL_CAPS_EXPECTED: &[&str] = &["tmux", "attach", "ccm-sid", "account", "model"];

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

const TOKEN: &str = "0123456789abcdef0123456789abcdef";

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
        send_into: false,
    };
    assert_eq!(
        render_ccm_invocation(&tmux, &no_tmux),
        Err(Refusal::MissingCap("tmux".into()))
    );
    let mut attach = base_spec();
    attach.action = Action::Attach { name: "cc-s1" };
    attach.container = Container::Tmux {
        name: "cc-s1",
        send_into: false,
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
            "这台机器上的 ccm 做不到这样起会话（缺 tmux）".into(),
        ),
        (Refusal::AttachNeedsTmux, "只能接入 tmux 里的会话".into()),
        (
            Refusal::DimensionCannotSpeak("account".into()),
            "这一项设置（account）写不成 ccm 参数".into(),
        ),
        (
            Refusal::DimensionNeedsCap {
                dim: "model".into(),
                cap: "model".into(),
            },
            "这台机器上的 ccm 不认 model 这一项设置（缺 model）".into(),
        ),
        (
            Refusal::FreeTextRefused {
                slot: FreeTextSlot::Cwd,
                value: q("rel/dir"),
            },
            "工作目录 \"rel/dir\" 用不了。要绝对路径，不含 .. 段、换行或 NUL".into(),
        ),
        (
            Refusal::IdentifierRefused {
                slot: IdentifierSlot::RbindToken,
                value: q("XYZ"),
            },
            "启动期令牌 \"XYZ\" 不合法（要 32 位小写十六进制）".into(),
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
    assert_eq!(variants.len(), 6, "抠到的变体：{variants:?}");
    for v in &variants {
        assert!(
            pairs.iter().any(|(r, _)| format!("{r:?}").starts_with(v)),
            "变体 `{v}` 不在逐字表里"
        );
    }
}

/// 账号维度恒表态：`--base` · `--account <名>` · `--account-dir <目录>` · 继承（不吐）· 名字目录都没有 ⇒ 整条放弃。
#[test]
fn the_account_dimension_has_its_five_shapes() {
    let with = |a: CliAccount<'static>| {
        let mut s = base_spec();
        s.account = a;
        render(&s)
    };
    assert_eq!(with(CliAccount::Base).unwrap(), "ccm -- new --base");
    assert_eq!(
        with(CliAccount::Named {
            name: Some("z"),
            config_dir: Some("/h/.claude-alt/z")
        })
        .unwrap(),
        "ccm -- new --account z"
    );
    assert_eq!(
        with(CliAccount::Named {
            name: None,
            config_dir: Some("/h/.claude-alt/z")
        })
        .unwrap(),
        "ccm -- new --account-dir /h/.claude-alt/z"
    );
    assert_eq!(with(CliAccount::Inherit).unwrap(), "ccm -- new");
    assert_eq!(
        with(CliAccount::Named {
            name: None,
            config_dir: None
        }),
        Err(Refusal::DimensionCannotSpeak("account".into()))
    );
}

/// 全触发：每一段按维度表的顺序出现、两半各归各位（逐字节手写）。
#[test]
fn a_fully_loaded_invocation_emits_every_part_in_registry_order() {
    let spec = CliSpec {
        action: Action::Resume { sid: "s1" },
        container: Container::Tmux {
            name: "cc-s1",
            send_into: false,
        },
        cwd: Some("/w d"),
        account: CliAccount::Named {
            name: Some("z"),
            config_dir: None,
        },
        ccm_sid: Some("s1"),
        model: Some("opus"),
        launcher: "ccr code",
        default_launcher: "claude",
        args: &["--verbose"],
        rbind_token: Some(TOKEN),
        launch_id: Some("s1"),
        ccm_path: "ccm",
        detach: false,
    };
    assert_eq!(
        render(&spec).unwrap(),
        format!(
            "ccm --resume s1 --model opus --verbose -- --ccm-tmux=cc-s1 --ccm-sid=s1 --account z \
             --ccm-rbind-token {TOKEN} --ccm-launch-id s1 --cwd '/w d' --launcher 'ccr code'"
        )
    );
}

/// 两个身份 token 只在起 agent 时带（接回不起进程，令牌没有读者）。
#[test]
fn identity_tokens_ride_only_when_an_agent_starts() {
    let mut s = base_spec();
    s.rbind_token = Some(TOKEN);
    s.launch_id = Some("id-1");
    assert_eq!(
        render(&s).unwrap(),
        format!("ccm -- new --base --ccm-rbind-token {TOKEN} --ccm-launch-id id-1")
    );
    s.action = Action::Attach { name: "cc-x" };
    s.container = Container::Tmux {
        name: "cc-x",
        send_into: false,
    };
    assert_eq!(render(&s).unwrap(), "ccm -- --attach cc-x");
}

/// 就地 resume：外层只包那一行直路 `ccm …`（键进已有的 pane，再接进去）。
#[test]
fn send_into_wraps_exactly_the_direct_line() {
    let mut s = base_spec();
    s.action = Action::Resume { sid: "s1" };
    s.rbind_token = Some(TOKEN);
    let direct = render(&s).unwrap();
    s.container = Container::Tmux {
        name: "cc x",
        send_into: true,
    };
    assert_eq!(
        render(&s).unwrap(),
        format!("tmux send-keys -t '=cc x:' '{direct}' Enter; tmux attach -t '=cc x:'")
    );
    assert!(direct.starts_with("ccm --resume s1 -- "), "{direct}");
}

/// 拼进命令之前每一格各过各的那一条：sid · 身份标记 · 模型 · 账号名 · 账号目录 · 令牌 · 身份 token · 会话名 · cwd · 透传参数 · 接回目标。
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
                account: CliAccount::Named {
                    name: Some("a b"),
                    config_dir: None,
                },
                ..base_spec()
            },
            "IdentifierRefused { slot: Account",
        ),
        (
            CliSpec {
                account: CliAccount::Named {
                    name: None,
                    config_dir: Some("rel/z"),
                },
                ..base_spec()
            },
            "FreeTextRefused { slot: AccountDir",
        ),
        (
            CliSpec {
                rbind_token: Some("XYZ"),
                ..base_spec()
            },
            "IdentifierRefused { slot: RbindToken",
        ),
        (
            CliSpec {
                launch_id: Some("a/b"),
                ..base_spec()
            },
            "IdentifierRefused { slot: LaunchId",
        ),
        (
            CliSpec {
                container: Container::Tmux {
                    name: "-x",
                    send_into: false,
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
                args: &["a\nb"],
                ..base_spec()
            },
            "FreeTextRefused { slot: AgentArg",
        ),
        (
            CliSpec {
                container: Container::Tmux {
                    name: "a\u{7}b",
                    send_into: true,
                },
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
