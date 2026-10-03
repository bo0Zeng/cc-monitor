//! **起会话那一行 `ccm …` 的渲染器** —— 全仓唯一一份，也是 monitor 每一条起会话路径交给终端的唯一形状。
//!
//! 环境、中转地址、身份标记都由那台机器上的 `ccm` 自己做；这里只把「起什么」说成 `ccm` 的参数
//! （`ccm [交给 agent 的…] -- [ccm 自己的…]`）。外层容器只剩「往已有 tmux 会话里就地 resume」那一格要包一层，
//! 包的也只是这一行。
//!
//! 渲不出来带理由（[`Refusal`]）：某个维度说不成 ccm 参数 ⇒ 整条放弃，不渲一条丢了修饰的命令。
//! 独立说法：本文件的自测（判据自带清单）与入库夹具 `fixtures/cli-golden.json`（手写 `out`）。

use copy_core::copy_text;
use std::collections::BTreeSet;

/// 渲染不出 ccm 调用行时的**理由**。它是一等返回值，不是 `None`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    MissingCap(String),
    AttachNeedsTmux,
    /// 某个维度在当前上下文里说不出 CLI 语法 ⇒ 整条放弃。
    DimensionCannotSpeak(String),
    /// 已触发的维度要求的能力这台 ccm 没有（这个维度触发了才要）。
    DimensionNeedsCap {
        dim: String,
        cap: String,
    },
    /// 一个自由文本值过不了拼进命令之前的放行判定。`value` = 原值（`{:?}` 形）。
    FreeTextRefused {
        slot: FreeTextSlot,
        value: String,
    },
    /// 一个**标识符**值过不了拼进命令之前的放行判定（判定住共享 crate，全仓唯一一份）。`value` = 原值（`{:?}` 形）。
    IdentifierRefused {
        slot: IdentifierSlot,
        value: String,
    },
}

/// [`Refusal::IdentifierRefused`] 是哪一格（各有各的一句话，文案走表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierSlot {
    /// `--resume <sid>`（`shell_quote_core::session_id_ok`）。
    Sid,
    /// `--ccm-sid=`（身份标记；同一条 sid 规则）。
    CcmSid,
    /// `--model <名>`（`shell_quote_core::model_name_ok`）。
    Model,
    /// `--account <名>`（`shell_quote_core::account_name_ok`）。
    Account,
    /// `--ccm-tmux=<名>`：要**新建**的会话名（`crate::control::gate_rules::new_tmux_name_issue`）。
    TmuxName,
    /// `--ccm-launch-id <标识>`（`relay_route_core::segment_is_safe`）。
    LaunchId,
}

/// [`Refusal::FreeTextRefused`] 是哪一格（各有各的一句话，文案走表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreeTextSlot {
    Cwd,
    AgentArg,
    /// 一个**已有**会话的名字（`--attach <名>` · 就地 resume 的目标；`gate_rules::existing_tmux_name_issue`）。
    AttachTarget,
    /// `--account-dir <目录>`（`acct_core::config_dir_ok`）。
    AccountDir,
}

impl Refusal {
    /// 降级理由。入库夹具 `cli-golden.json` 的 refusal 用例逐字节比它（夹具那一侧是手写期望）。
    pub fn reason(&self) -> String {
        match self {
            Refusal::MissingCap(c) => copy_text(
                "rsCcmInvocation.refusal.missingCap",
                &[("c", &c.to_string())],
            ),
            Refusal::AttachNeedsTmux => copy_text("rsCcmInvocation.refusal.attachNeedsTmux", &[]),
            Refusal::DimensionCannotSpeak(id) => copy_text(
                "rsCcmInvocation.refusal.cannotSpeak",
                &[("id", &id.to_string())],
            ),
            Refusal::DimensionNeedsCap { dim, cap } => copy_text(
                "rsCcmInvocation.refusal.dimensionNeedsCap",
                &[("dim", &dim.to_string()), ("cap", &cap.to_string())],
            ),
            Refusal::FreeTextRefused { slot, value } => match slot {
                FreeTextSlot::Cwd => {
                    copy_text("rsCcmInvocation.refusal.freeTextCwd", &[("value", value)])
                }
                FreeTextSlot::AgentArg => {
                    copy_text("rsCcmInvocation.refusal.freeTextArg", &[("value", value)])
                }
                FreeTextSlot::AttachTarget => copy_text(
                    "rsCcmInvocation.refusal.freeTextAttach",
                    &[("value", value)],
                ),
                FreeTextSlot::AccountDir => copy_text(
                    "rsCcmInvocation.refusal.freeTextAccountDir",
                    &[("value", value)],
                ),
            },
            Refusal::IdentifierRefused { slot, value } => match slot {
                IdentifierSlot::Sid => {
                    copy_text("rsCcmInvocation.refusal.idSid", &[("value", value)])
                }
                IdentifierSlot::CcmSid => {
                    copy_text("rsCcmInvocation.refusal.idCcmSid", &[("value", value)])
                }
                IdentifierSlot::Model => {
                    copy_text("rsCcmInvocation.refusal.idModel", &[("value", value)])
                }
                IdentifierSlot::Account => {
                    copy_text("rsCcmInvocation.refusal.idAccount", &[("value", value)])
                }
                IdentifierSlot::LaunchId => {
                    copy_text("rsCcmInvocation.refusal.idLaunchId", &[("value", value)])
                }
                IdentifierSlot::TmuxName => copy_text(
                    "rsCcmInvocation.refusal.idTmuxName",
                    &[
                        ("value", value),
                        ("refused", crate::control::gate_rules::NEW_TMUX_NAME_REFUSED),
                        (
                            "max",
                            &crate::control::gate_rules::NEW_TMUX_NAME_MAX.to_string(),
                        ),
                    ],
                ),
            },
        }
    }
}

/// CLI 语法覆盖面 —— **每次调用都无条件要求**的能力（`tests/e2e/ccm-contract-parity.sh` 从本文件抽它去比真
/// `ccm --ccm-probe` 的 `capabilities=`）。只放与容器、平台无关的那几条；`tmux` / `attach` 只在用到时才要，
/// 维度触发了才要的由各自维度声明（`account` · `model` · `ccm-sid`）。
pub const CLI_REQUIRED_CAPS: &[&str] = &["new", "resume", "cwd", "launcher"];

/// 动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action<'a> {
    New,
    Resume { sid: &'a str },
    Attach { name: &'a str },
}

/// 容器。`send_into` = 往一个**已有**的空 tmux 会话里就地 resume（键入直路那一行，再接进去）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Container<'a> {
    None,
    Tmux { name: &'a str, send_into: bool },
}

/// 账号维度的三态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAccount<'a> {
    /// 具名账号：说得出名字 ⇒ `--account <名>`；只说得出目录 ⇒ `--account-dir <目录>`；都没有 ⇒ 整条放弃。
    Named {
        name: Option<&'a str>,
        config_dir: Option<&'a str>,
    },
    Base,
    /// 调用方没表态（继承）：一个旗标都不吐，省略在 ccm 上有确定语义（`plan.rs::resolve_account`）。
    /// 只有本机那条路构造它（远端那台的继承态不是 monitor 的环境）。
    Inherit,
}

/// 渲染 ccm 调用行所需的全部输入。
#[derive(Debug, Clone)]
pub struct CliSpec<'a> {
    pub action: Action<'a>,
    pub container: Container<'a>,
    pub cwd: Option<&'a str>,
    pub account: CliAccount<'a>,
    pub ccm_sid: Option<&'a str>,
    pub model: Option<&'a str>,
    /// 等于默认启动器时**不吐** `--launcher`。
    pub launcher: &'a str,
    pub default_launcher: &'a str,
    pub args: &'a [&'a str],
    /// 本机回填 sid 用的身份 token（`--ccm-launch-id`）。
    pub launch_id: Option<&'a str>,
    pub ccm_path: &'a str,
    /// 建进 tmux 之后不接进去（`--detach`，只对新建 tmux 容器那一形有意义）：后端自己替人起会话时用（tab 栏批量在 tmux 里起）。
    pub detach: bool,
}

/// argv token 的 quote：只在含 ccm 允许字符集之外的东西时才包单引号。
fn argv(token: &str) -> String {
    let safe = !token.is_empty()
        && token.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '_' | '@' | '%' | '+' | '=' | ':' | ',' | '.' | '/' | '-')
        });
    if safe {
        token.to_string()
    } else {
        shell_quote_core::posix_quote(token)
    }
}

/// 一个维度在 CLI 语境下要说的话。`None` = **说不出** ⇒ 整条放弃。
type Flags = Option<Vec<String>>;

/// 维度表的 `applies` + `cli_flags` + `caps`，**顺序即契约**（与命令里各旗标的先后同序）。
fn dimension_flags(
    spec: &CliSpec,
    caps: &BTreeSet<String>,
) -> Result<Vec<(&'static str, Vec<String>)>, Refusal> {
    let mut out = Vec::new();
    // 能力检查与 flags 逐维度交错：「缺能力」与「说不出」同时成立时理由取先到的那一维。
    for dim in DIMENSION_ORDER {
        if !(dim.applies)(spec) {
            continue;
        }
        for cap in dim.caps {
            if !caps.contains(*cap) {
                return Err(Refusal::DimensionNeedsCap {
                    dim: dim.id.to_string(),
                    cap: cap.to_string(),
                });
            }
        }
        match (dim.cli_flags)(spec) {
            None => return Err(Refusal::DimensionCannotSpeak(dim.id.to_string())),
            Some(f) => out.push((dim.id, f)),
        }
    }
    Ok(out)
}

/// 一个维度在 CLI 侧的三个钩子。`caps` 只向**已触发**的维度收集。
struct Dim {
    id: &'static str,
    applies: fn(&CliSpec) -> bool,
    cli_flags: fn(&CliSpec) -> Flags,
    caps: &'static [&'static str],
}

fn starts_agent(s: &CliSpec) -> bool {
    matches!(s.action, Action::New | Action::Resume { .. })
}

/// **顺序即契约**：`identity` < `account` < `model` < `launch-id`。
/// 由 `a_fully_loaded_invocation_emits_every_part_in_registry_order`（全触发、逐字节比整条命令）钉住。
const DIMENSION_ORDER: &[Dim] = &[
    Dim {
        id: "identity",
        applies: |s| s.ccm_sid.is_some(),
        cli_flags: |s| Some(vec![format!("--ccm-sid={}", s.ccm_sid.unwrap_or_default())]),
        caps: &["ccm-sid"],
    },
    Dim {
        id: "account",
        // **恒真** —— 账号维度必须永远显式表态（沉默 = 意外身份切换）；继承是「表了态的省略」。
        applies: |_| true,
        cli_flags: |s| match s.account {
            CliAccount::Base => Some(vec!["--base".into()]),
            CliAccount::Named { name: Some(n), .. } => {
                Some(vec!["--account".into(), n.to_string()])
            }
            CliAccount::Named {
                name: None,
                config_dir: Some(d),
            } => Some(vec!["--account-dir".into(), d.to_string()]),
            // 名字与目录都说不出 ⇒ 整条放弃。
            CliAccount::Named {
                name: None,
                config_dir: None,
            } => None,
            CliAccount::Inherit => Some(vec![]),
        },
        caps: &["account"],
    },
    Dim {
        id: "model",
        // 条件式：没配偏好时 agent 用它自己的默认模型 —— 那正是用户的期望。
        applies: |s| s.model.is_some(),
        cli_flags: |s| {
            Some(vec![
                "--model".into(),
                s.model.unwrap_or_default().to_string(),
            ])
        },
        caps: &["model"],
    },
    Dim {
        id: "launch-id",
        applies: |s| s.launch_id.is_some() && starts_agent(s),
        cli_flags: |s| {
            Some(vec![
                "--ccm-launch-id".into(),
                s.launch_id.unwrap_or_default().to_string(),
            ])
        },
        caps: &[],
    },
];

/// `spec → ccm 调用行`。`caps` = 这台 ccm 会哪些（渲染就在那台后端里，问的是它自己）。
pub fn render_ccm_invocation(spec: &CliSpec, caps: &BTreeSet<String>) -> Result<String, Refusal> {
    match render_parts(spec, caps)? {
        Parts::Argv(tokens) => Ok(tokens.iter().map(|t| argv(t)).collect::<Vec<_>>().join(" ")),
        Parts::Shell(line) => Ok(line),
    }
}

/// 同一行的 argv 形（不过 shell，`argv[0]` 是 `ccm_path`）：后端自己起这一趟时用它（[`render_ccm_invocation`] 就是把它逐个引号化连起来）。
/// 就地 resume 那一形本来就是一串 shell（`tmux send-keys …`），没有 argv 形 ⇒ 拒。
pub fn ccm_argv(spec: &CliSpec, caps: &BTreeSet<String>) -> Result<Vec<String>, Refusal> {
    match render_parts(spec, caps)? {
        Parts::Argv(tokens) => Ok(tokens),
        Parts::Shell(_) => Err(Refusal::AttachNeedsTmux),
    }
}

/// 渲染的两种成品：一条 ccm 的 argv · 一串 shell（就地 resume 那一形）。
enum Parts {
    Argv(Vec<String>),
    Shell(String),
}

fn render_parts(spec: &CliSpec, caps: &BTreeSet<String>) -> Result<Parts, Refusal> {
    for c in CLI_REQUIRED_CAPS {
        if !caps.contains(*c) {
            return Err(Refusal::MissingCap((*c).to_string()));
        }
    }
    let need = |c: &str| -> Result<(), Refusal> {
        if caps.contains(c) {
            Ok(())
        } else {
            Err(Refusal::MissingCap(c.to_string()))
        }
    };

    // `ccm [交给 agent 的…] -- [ccm 自己的…]`：两半分开收，最后按 [`join_halves`] 拼。
    let mut tokens: Vec<String> = vec![spec.ccm_path.to_string()];
    let mut ours: Vec<String> = Vec::new();

    // attach 不起 agent：`ccm -- --attach <名>` 不接受任何修饰 flag。
    if let Action::Attach { .. } = spec.action {
        let Container::Tmux { name, .. } = spec.container else {
            return Err(Refusal::AttachNeedsTmux);
        };
        need("attach")?;
        existing_name_ok(name)?;
        ours.push("--attach".into());
        ours.push(name.to_string());
        return Ok(Parts::Argv(join_halves(tokens, ours)));
    }

    // 就地 resume：键进那个 pane 的是**直路**那一行（pane 里已经有 shell，不再建容器），外层只包这一行。
    if let Container::Tmux {
        name,
        send_into: true,
    } = spec.container
    {
        need("tmux")?;
        existing_name_ok(name)?;
        let inner = render_ccm_invocation(
            &CliSpec {
                container: Container::None,
                ..spec.clone()
            },
            caps,
        )?;
        let t = shell_quote_core::posix_quote(&format!("={name}:"));
        return Ok(Parts::Shell(format!(
            "tmux send-keys -t {t} {} Enter; tmux attach -t {t}",
            shell_quote_core::posix_quote(&inner)
        )));
    }

    match spec.action {
        Action::Resume { sid } => {
            if !shell_quote_core::session_id_ok(sid) {
                return Err(Refusal::IdentifierRefused {
                    slot: IdentifierSlot::Sid,
                    value: format!("{sid:?}"),
                });
            }
            // `--resume <sid>` 是 agent 的旗标，ccm 原样交过去。
            tokens.push("--resume".into());
            tokens.push(sid.to_string());
        }
        // 起新会话是 ccm 自己的位置词 `new`，写在 `--` 右边第一个（`ccm -- new …`）。
        Action::New => ours.push("new".into()),
        Action::Attach { .. } => {}
    }
    identifiers_ok(spec)?;
    if let Container::Tmux { name, .. } = spec.container {
        need("tmux")?;
        if crate::control::gate_rules::new_tmux_name_issue(name).is_some() {
            return Err(Refusal::IdentifierRefused {
                slot: IdentifierSlot::TmuxName,
                value: format!("{name:?}"),
            });
        }
        ours.push(format!("--ccm-tmux={name}"));
        if spec.detach {
            ours.push("--detach".into());
        }
    }

    // 维度吐的旗标分两半：`--model` 是 agent 的，其余是 ccm 的。
    for (dim, flags) in dimension_flags(spec, caps)? {
        if dim == "model" {
            tokens.extend(flags);
        } else {
            ours.extend(flags);
        }
    }

    if let Some(cwd) = spec.cwd {
        if !shell_quote_core::posix_free_path_ok(cwd) {
            return Err(Refusal::FreeTextRefused {
                slot: FreeTextSlot::Cwd,
                value: format!("{cwd:?}"),
            });
        }
        ours.push("--cwd".into());
        ours.push(cwd.to_string());
    }
    if spec.launcher != spec.default_launcher {
        ours.push("--launcher".into());
        ours.push(spec.launcher.to_string());
    }
    if let Some(a) = spec
        .args
        .iter()
        .find(|a| !shell_quote_core::free_text_ok(a))
    {
        return Err(Refusal::FreeTextRefused {
            slot: FreeTextSlot::AgentArg,
            value: format!("{a:?}"),
        });
    }
    tokens.extend(spec.args.iter().map(|a| (*a).to_string()));
    Ok(Parts::Argv(join_halves(tokens, ours)))
}

/// 一个**已有**会话的名字：拒绝集（控制符 · 欺骗字符）＋ 非空（`control/gate_rules.rs`，全仓唯一一份）。
fn existing_name_ok(name: &str) -> Result<(), Refusal> {
    match crate::control::gate_rules::existing_tmux_name_issue(name) {
        None => Ok(()),
        Some(_) => Err(Refusal::FreeTextRefused {
            slot: FreeTextSlot::AttachTarget,
            value: format!("{name:?}"),
        }),
    }
}

/// 拼进命令之前，几个标识符与账号目录各过各的那一条（判定都住共享 crate）。
fn identifiers_ok(spec: &CliSpec) -> Result<(), Refusal> {
    let bad = |slot, v: &str| {
        Err(Refusal::IdentifierRefused {
            slot,
            value: format!("{v:?}"),
        })
    };
    if let Some(s) = spec.ccm_sid.filter(|s| !shell_quote_core::session_id_ok(s)) {
        return bad(IdentifierSlot::CcmSid, s);
    }
    match spec.account {
        CliAccount::Named { name: Some(n), .. } if !shell_quote_core::account_name_ok(n) => {
            return bad(IdentifierSlot::Account, n);
        }
        CliAccount::Named {
            name: None,
            config_dir: Some(d),
        } if !acct_core::config_dir_ok(d) => {
            return Err(Refusal::FreeTextRefused {
                slot: FreeTextSlot::AccountDir,
                value: format!("{d:?}"),
            });
        }
        _ => {}
    }
    if let Some(m) = spec.model.filter(|m| !shell_quote_core::model_name_ok(m)) {
        return bad(IdentifierSlot::Model, m);
    }
    if let Some(l) = spec
        .launch_id
        .filter(|l| !relay_route_core::segment_is_safe(l))
    {
        return bad(IdentifierSlot::LaunchId, l);
    }
    Ok(())
}

/// `<ccm> <交给 agent 的…> -- <ccm 自己的…>`：ccm 那一半空、而 agent 那一半里没有 `--` ⇒ 不写 `--`；
/// agent 那一半里有它自己的 `--` ⇒ 末尾照样补一个（按最后一个 `--` 切，空的 ccm 部分也得标出来）。
fn join_halves(mut agent: Vec<String>, ours: Vec<String>) -> Vec<String> {
    if !ours.is_empty() || agent.iter().skip(1).any(|a| a == "--") {
        agent.push("--".into());
        agent.extend(ours);
    }
    agent
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/launch_render/ccm_invocation_tests.rs"]
mod tests;
