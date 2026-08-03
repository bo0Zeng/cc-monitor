//! U8c-2c-1：**`ccm …` 调用行的渲染器** —— TS `launch-render-cli.ts::tryRenderCli` 的 Rust 对侧。
//!
//! # 它为什么比载荷那一半更要紧
//!
//! 一条远端起会话命令有两种形态：**ccm 调用行**（装了 ccm 时走）与**裸载荷**（没装时走）。
//! U8c-1 搬的是后者 —— 而 U8c-2b-0 摸底实测：**装了 ccm 的机器走的是前者，
//! `renderFallback` 根本不执行**。所以这一半才是「真正在跑的那条路」。
//!
//! # 诚实降级不是可选项（§33 / §35）
//!
//! 渲染失败**必须带理由**：TS 那边 `{ok:false, reason}` 的 `reason` 是生产侧唯一的降级线索
//! （`remote-launch-run.ts` 用 `console.debug` 打它）。所以这里回 [`Refusal`] 而不是 `Option`
//! —— 丢掉理由等于把「诚实降级」降级成「静默降级」。
//!
//! 而 `cliFlags` 返回 `null`（Rust 里的 `None`）是 **§35 的安全网**：它表示
//! 「这个维度在当前上下文里说不出 CLI 语法」⇒ **整条放弃**，不是「跳过这个维度继续渲染」。
//! 后者会渲染出一条**丢了修饰**的命令，而丢的恰好是账号那类东西（R11/R08 的病灶）。
//!
//! # 与 TS 的一致性靠什么保住
//!
//! 入库夹具逐字节对拍（同 U8c-1）：TS 生成 → 入库 → 两侧各自与它比。
//! ⚠ **ok 与 refusal 两类都要覆盖** —— 只比 ok 的话，「该降级却渲染出来了」抓不到，
//! 而那正是 §33 铁律要防的形态。

use std::collections::BTreeSet;

/// 渲染不出 ccm 调用行时的**理由**。它是一等返回值，不是 `None`（见模块头注）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    NotInstalled,
    NotSsh,
    MissingCap(String),
    /// #76 防线：`send-into`（idle-tmux 就地复用）**没有 CLI 等价语法**。
    /// ccm 表达不了「就地复用、不新建」—— 硬渲染出去会变成「新建一个」，
    /// 那正是 issue #76 的失管会话。
    SendIntoHasNoCliForm,
    AttachNeedsTmux,
    /// §35 的 `null` 安全网：某个维度在当前上下文里说不出 CLI 语法 ⇒ 整条放弃。
    DimensionCannotSpeak(String),
    /// 已触发的维度要求的能力远端没有。**与上面的 `MissingCap` 不是一回事**：
    /// 那个是「每次调用都要的静态能力」，这个是「这个维度触发了才要」（§37）。
    DimensionNeedsCap {
        dim: String,
        cap: String,
    },
}

impl Refusal {
    /// 与 TS 侧 `reason` 字符串**逐字节相同**（夹具对拍的比较对象）。
    pub fn reason(&self) -> String {
        match self {
            Refusal::NotInstalled => "远端未装 ccm".into(),
            Refusal::NotSsh => "本地路径不走 CLI 渲染器".into(),
            Refusal::MissingCap(c) => format!("远端 ccm 缺能力 {c}"),
            Refusal::SendIntoHasNoCliForm => {
                "send-into（idle-tmux 就地复用）无 CLI 等价语法，诚实降级".into()
            }
            Refusal::AttachNeedsTmux => "attach 必须是 tmux 容器".into(),
            Refusal::DimensionCannotSpeak(id) => {
                format!("维度 {id} 无法用 CLI 语法表达（cliFlags 返回 null）")
            }
            Refusal::DimensionNeedsCap { dim, cap } => {
                format!("维度 {dim} 需要远端 ccm 能力 {cap}，但它不支持")
            }
        }
    }
}

/// CLI 语法覆盖面 —— **每次调用都无条件要求**的能力。
///
/// 与 TS `launch-render-cli.ts::CLI_REQUIRED_CAPS` 逐项同序。只放「与具体维度无关的
/// 动作/容器语法」；`account`/`model` 由各自维度用 `required_caps` 声明（§37）。
pub const CLI_REQUIRED_CAPS: &[&str] = &[
    "new", "resume", "attach", "tmux", "cwd", "launcher", "ccm-sid",
];

/// 动作。与 TS `LaunchAction` 同构。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action<'a> {
    New,
    Resume { sid: &'a str },
    Attach { name: &'a str },
}

/// 容器。`send_into` 单独一个变体是因为它是 **#76 防线**的判据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Container<'a> {
    None,
    Tmux { name: &'a str, send_into: bool },
}

/// 账号维度的两态（同 [`crate::Account`]，但 CLI 侧还需要**名字**才能说出 `--account`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAccount<'a> {
    /// 具名账号。`name: None` = 只有 configDir 没有名字 ⇒ **说不出 `--account`** ⇒ §35 短路。
    Named {
        name: Option<&'a str>,
    },
    Base,
}

/// 渲染 ccm 调用行所需的全部输入（TS `LaunchPlan` + `LaunchContext` 的交集）。
#[derive(Debug, Clone)]
pub struct CliSpec<'a> {
    pub is_ssh: bool,
    pub action: Action<'a>,
    pub container: Container<'a>,
    pub cwd: Option<&'a str>,
    pub account: CliAccount<'a>,
    pub ccm_sid: Option<&'a str>,
    pub model: Option<&'a str>,
    /// 已 sanitize 的 launcher；等于默认启动器时**不吐** `--launcher`（与 TS 同）。
    pub launcher: &'a str,
    pub default_launcher: &'a str,
    pub args: &'a [&'a str],
    pub ccm_path: &'a str,
}

/// argv token 的 quote：只在含 ccm 允许字符集之外的东西时才包单引号（与 TS `argv()` 同规则）。
fn argv(token: &str) -> String {
    let safe = !token.is_empty()
        && token.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '_' | '@' | '%' | '+' | '=' | ':' | ',' | '.' | '/' | '-')
        });
    if safe {
        token.to_string()
    } else {
        // ⚠ **别在这里再写一遍逃逸** —— U8c-2b-0 的 `quote_singleton_guard` 这一轮就是这么
        // 咬到我的：初版 `argv` 自己 `format!` 了一份，成了第六份副本。
        crate::posix_quote(token)
    }
}

/// 一个维度在 CLI 语境下要说的话。`None` = **说不出** ⇒ §35 短路。
type Flags = Option<Vec<String>>;

/// 五个维度的 `applies` + `cliFlags` + `requiredCaps`，**顺序即契约**
/// （`identity`(5) < `env-reset`(10) < `account`(20) < `model`(25) < `nested-env-reset`(30)）。
///
/// ⚠ 这里只实现 `cliFlags` 那一半 —— `apply`（产 `EnvOp`）那一半 U8c-1 已经在
/// [`crate::render_payload`] 里了。
fn dimension_flags(spec: &CliSpec, caps: &BTreeSet<String>) -> Result<Vec<String>, Refusal> {
    let mut out = Vec::new();
    // ⚠ **能力检查与 flags 是逐维度交错的**（与 TS 的 `for (const dim of …)` 同构）——
    // 初版我把能力检查整体提到循环外，那会在「缺能力」与「说不出」同时成立时给出**另一个**
    // 理由；而 reason 是生产侧唯一的降级线索，换一个就是换一条诊断。
    for dim in DIMENSION_ORDER {
        if !dim.applies(spec) {
            continue;
        }
        for cap in dim.required_caps(spec) {
            if !caps.contains(*cap) {
                return Err(Refusal::DimensionNeedsCap {
                    dim: dim.id.to_string(),
                    cap: cap.to_string(),
                });
            }
        }
        push(&mut out, dim.id, (dim.cli_flags)(spec))?;
    }
    Ok(out)
}

/// 一个维度在 CLI 侧的三个钩子。与 TS `LaunchDimension` 同构（`apply` 那一半不在这里 ——
/// 它产 `EnvOp`，U8c-1 已经搬进 `render_payload`）。
struct Dim {
    id: &'static str,
    applies: fn(&CliSpec) -> bool,
    cli_flags: fn(&CliSpec) -> Flags,
    /// 只向**已触发**的维度收集（§37 的结构保证）。
    caps: &'static [&'static str],
}

impl Dim {
    fn applies(&self, spec: &CliSpec) -> bool {
        (self.applies)(spec)
    }
    fn required_caps(&self, _spec: &CliSpec) -> &'static [&'static str] {
        self.caps
    }
}

/// **顺序即契约**：`identity`(5) < `env-reset`(10) < `account`(20) < `model`(25) < `nested-env-reset`(30)。
/// TS 侧用 `order` 字段 + 加载期断言钉住；这里用数组顺序 + 下面那条测试钉住。
const DIMENSION_ORDER: &[Dim] = &[
    Dim {
        id: "identity",
        applies: |s| s.ccm_sid.is_some(),
        cli_flags: |s| Some(vec![format!("--ccm-sid={}", s.ccm_sid.unwrap_or_default())]),
        caps: &[],
    },
    Dim {
        id: "env-reset",
        applies: |s| {
            matches!(
                s.container,
                Container::Tmux {
                    send_into: true,
                    ..
                }
            ) && !matches!(s.account, CliAccount::Named { .. })
        },
        // ccm 内部按 --base/无 --account 自行处理，无专属 flag。
        cli_flags: |_| Some(vec![]),
        caps: &[],
    },
    Dim {
        id: "account",
        // **恒真** —— 账号维度在 CLI 语境下必须永远显式表态（F05：沉默 = 意外身份切换）。
        applies: |_| true,
        cli_flags: |s| match s.account {
            CliAccount::Base => Some(vec!["--base".into()]),
            CliAccount::Named { name: Some(n) } => Some(vec!["--account".into(), n.to_string()]),
            // 只有 configDir 没有名字 ⇒ 老实说「我说不出 --account」⇒ 整条降级（§35）。
            CliAccount::Named { name: None } => None,
        },
        caps: &["account"],
    },
    Dim {
        id: "model",
        // **条件式**（§37）：没配偏好时远端 claude 用它自己的默认模型 —— 那正是用户的期望。
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
        id: "nested-env-reset",
        applies: |s| matches!(s.action, Action::New | Action::Resume { .. }),
        // ccm 内部恒清（agent_nested_env 按 agent 查表），无专属 flag。
        cli_flags: |_| Some(vec![]),
        caps: &[],
    },
];

fn push(out: &mut Vec<String>, id: &str, flags: Flags) -> Result<(), Refusal> {
    match flags {
        None => Err(Refusal::DimensionCannotSpeak(id.to_string())),
        Some(f) => {
            out.extend(f);
            Ok(())
        }
    }
}

/// `ctx → ccm 调用行`。与 TS `tryRenderCli` 同构、逐字节对拍。
pub fn render_ccm_invocation(
    spec: &CliSpec,
    caps: &BTreeSet<String>,
    installed: bool,
) -> Result<String, Refusal> {
    if !installed {
        return Err(Refusal::NotInstalled);
    }
    if !spec.is_ssh {
        return Err(Refusal::NotSsh);
    }
    for c in CLI_REQUIRED_CAPS {
        if !caps.contains(*c) {
            return Err(Refusal::MissingCap((*c).to_string()));
        }
    }
    if matches!(
        spec.container,
        Container::Tmux {
            send_into: true,
            ..
        }
    ) {
        return Err(Refusal::SendIntoHasNoCliForm);
    }

    let mut tokens: Vec<String> = vec![spec.ccm_path.to_string()];

    // attach 分支**在维度循环之前 return** —— `ccm attach <名>` 不接受任何修饰 flag，
    // 所以它也不收集维度的 requiredCaps（§33 里登记在案的刻意豁免，不是回退）。
    if let Action::Attach { name } = spec.action {
        let Container::Tmux { name: cname, .. } = spec.container else {
            return Err(Refusal::AttachNeedsTmux);
        };
        let _ = name;
        tokens.push("attach".into());
        tokens.push(cname.to_string());
        return Ok(tokens.iter().map(|t| argv(t)).collect::<Vec<_>>().join(" "));
    }

    match spec.action {
        Action::Resume { sid } => {
            tokens.push("resume".into());
            tokens.push(sid.to_string());
        }
        _ => tokens.push("new".into()),
    }
    if let Container::Tmux { name, .. } = spec.container {
        tokens.push(format!("--tmux={name}"));
    }

    tokens.extend(dimension_flags(spec, caps)?);

    if let Some(cwd) = spec.cwd {
        tokens.push("--cwd".into());
        tokens.push(cwd.to_string());
    }
    if spec.launcher != spec.default_launcher {
        tokens.push("--launcher".into());
        tokens.push(spec.launcher.to_string());
    }
    if !spec.args.is_empty() {
        tokens.push("--".into());
        tokens.extend(spec.args.iter().map(|a| (*a).to_string()));
    }
    Ok(tokens.iter().map(|t| argv(t)).collect::<Vec<_>>().join(" "))
}
