//! U8c-2c-1：**`ccm …` 调用行的渲染器** —— TS `launch-render-cli.ts::tryRenderCli` 的 Rust 对侧。
//!
//! ⚠ **P4b 搬家**：它原来是共享 crate `launch-core` 的 `cli` 模块 —— 而 **daemon 对它零引用**。
//! 架构审计点破「这就是决策内核，放在共享 crate 里的真实原因是 monitor 没处放」。
//! 现在住 `backend/control/`：§1.3 把最终 exec 钉在用户自己的终端进程里，
//! U8a-2b 把 daemon 的执行面定成 argv 直传、不过 shell ⇒ **渲染 shell 串永远属于开终端那一侧。**
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
        launch_core::posix_quote(token)
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
/// TS 侧用 `order` 字段 + 加载期断言钉住；这里用数组顺序 +
/// `a_fully_loaded_invocation_emits_every_part_in_registry_order`（把三个会吐 flag 的维度
/// 同时触发、逐字节比整条命令）钉住。
/// ⚠ 订正：这句原本写「下面那条测试」，而**当时下面一条测试都没有**（本文件到 2026-08-03
/// 才有自测）。指向不存在的判据比没有注释更坏 —— 它让人以为那一层有人守着。
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
        //
        // ⚠ **这一格在本渲染器里是不可达的、也是惰性的**：它只在 `send_into: true` 时触发，
        // 而那种形态在维度循环**之前**就被 #76 防线拒掉了。所以「改它的 `applies`」这类变异
        // 不可能被行为判据杀掉 —— 那不是判据缺口，是这格改不了任何输出。
        // 钉住的是不可达性与惰性本身：见 `env_reset_can_never_be_reached_in_the_cli_renderer`
        // 与 `the_two_inert_dimensions_contribute_no_flags_for_any_shape`。
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
        // ⚠ 同 `env-reset`：**惰性格**（可达但一个 flag 都不吐），所以它的 `applies` 改了
        // 也改不了输出。惰性由 `the_two_inert_dimensions_contribute_no_flags_for_any_shape` 钉住。
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

// ────────────────────────────────────────────────────────────────────────────
// P1（2026-08-03 三视角复盘）：**本文件此前一条自测都没有。**
//
// 为什么必须补，而不是靠跨语言夹具：今天挡「两侧一起错」的是 TS 的
// `launch-render-cli.test.ts`，而它**排期在 U8c-3 被删**。那天一到，`cli-golden.json`
// 就变成一个没有生成者的冻结文件 —— 跨语言对拍会退化成「Rust 没变」的快照。
// 于是「渲染器该做什么」这件事在本仓就再没有独立说法了。
//
// 补之前先量：对本文件逐条造变异、只跑现有门禁（launch-core 15 条 + monitor 侧
// `_parity` 9 条），**七个存活**（R1–R7）。对照组「`--base` 改字」「account/model 换序」
// 都当场红，所以那个量具不是恒绿的。下面每条测试都注明它杀的是哪个。
//
// ⚠ 两处**刻意的重复**（`STATIC_CAPS_EXPECTED` / `ARGV_BARE_CHARS`）：判据必须自带清单，
// **不许遍历被测常量自己** —— 那是恒真的。R5 存活的原因正是这个形状：把 `"cwd"` 从
// `CLI_REQUIRED_CAPS` 删掉，任何「遍历该常量逐项抽掉」的循环也就不再测 `"cwd"`，照样全绿。
#[cfg(test)]
mod tests {
    use super::*;

    fn base_spec() -> CliSpec<'static> {
        CliSpec {
            is_ssh: true,
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
        }
    }

    /// 判据自带的静态能力清单（见本模块头注：**不复用 `CLI_REQUIRED_CAPS`**）。
    const STATIC_CAPS_EXPECTED: &[&str] = &[
        "new", "resume", "attach", "tmux", "cwd", "launcher", "ccm-sid",
    ];
    /// 维度各自声明的能力（§37），不在静态清单里。
    const DIMENSION_CAPS_EXPECTED: &[&str] = &["account", "model"];

    fn caps_all() -> BTreeSet<String> {
        STATIC_CAPS_EXPECTED
            .iter()
            .chain(DIMENSION_CAPS_EXPECTED)
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
        render_ccm_invocation(spec, &caps_all(), true)
    }

    fn dim(id: &str) -> &'static Dim {
        DIMENSION_ORDER
            .iter()
            .find(|d| d.id == id)
            .unwrap_or_else(|| panic!("维度注册表里没有 {id}"))
    }

    /// 覆盖「会影响渲染结果」的各个方向的一个小矩阵。给那几条「对所有形态都成立」的
    /// 结构性判据当输入。
    fn spec_matrix() -> Vec<CliSpec<'static>> {
        let mut out = Vec::new();
        for action in [
            Action::New,
            Action::Resume { sid: "s1" },
            Action::Attach { name: "cc-x" },
        ] {
            for container in [
                Container::None,
                Container::Tmux {
                    name: "cc-x",
                    send_into: false,
                },
                Container::Tmux {
                    name: "cc-x",
                    send_into: true,
                },
            ] {
                for account in [
                    CliAccount::Base,
                    CliAccount::Named { name: Some("z") },
                    CliAccount::Named { name: None },
                ] {
                    for (ccm_sid, model) in [(None, None), (Some("sid-1"), Some("opus"))] {
                        let mut s = base_spec();
                        s.action = action.clone();
                        s.container = container.clone();
                        s.account = account.clone();
                        s.ccm_sid = ccm_sid;
                        s.model = model;
                        out.push(s);
                    }
                }
            }
        }
        out
    }

    #[test]
    fn the_spec_matrix_is_not_accidentally_empty() {
        // 下面几条「对矩阵里每个 spec 都成立」的判据，矩阵一空就零命中变绿。
        let m = spec_matrix();
        assert_eq!(m.len(), 3 * 3 * 3 * 2, "矩阵规模变了，请确认覆盖面还在");
        assert!(
            m.iter().any(|s| render(s).is_ok()),
            "矩阵里没有一个渲染得出来的 spec —— 那「applies ⇒ 必被拒」那条就恒真了"
        );
    }

    // ── 静态能力闸 ───────────────────────────────────────────────────────────

    /// ★ 杀 R5（`CLI_REQUIRED_CAPS` 少一项 `cwd` —— 此前没有任何东西红）。
    ///
    /// 两半都要有：**清单相等**这一半才是杀掉「悄悄删一项」的那半；
    /// **逐项抽掉**这一半证明每项是真被执行、不是只声明（写了不查也是白写）。
    #[test]
    fn every_static_capability_is_declared_and_individually_enforced() {
        assert_eq!(
            CLI_REQUIRED_CAPS, STATIC_CAPS_EXPECTED,
            "静态能力清单变了。改它是改「装了哪种 ccm 才肯走 CLI 形态」的门槛，\
             要同步 TS 的 CLI_REQUIRED_CAPS 与 shared/ccm 的 --ccm-probe capabilities="
        );
        for missing in STATIC_CAPS_EXPECTED {
            let got = render_ccm_invocation(&base_spec(), &caps_without(missing), true);
            assert_eq!(
                got,
                Err(Refusal::MissingCap((*missing).to_string())),
                "缺静态能力 {missing} 时没有诚实降级"
            );
        }
    }

    #[test]
    fn not_installed_and_not_ssh_are_checked_before_anything_else() {
        assert_eq!(
            render_ccm_invocation(&base_spec(), &caps_all(), false),
            Err(Refusal::NotInstalled)
        );
        let mut s = base_spec();
        s.is_ssh = false;
        assert_eq!(render(&s), Err(Refusal::NotSsh));
        // 两者同时成立时先报「没装」—— 这两条 reason 会进 console.debug，顺序即诊断。
        assert_eq!(
            render_ccm_invocation(&s, &BTreeSet::new(), false),
            Err(Refusal::NotInstalled)
        );
    }

    // ── 降级理由（生产侧唯一的降级线索）─────────────────────────────────────

    /// ★ 杀 R4（`AttachNeedsTmux` 的理由被改掉，此前无人红）。
    ///
    /// 这是**钉住**，不是推导 —— 这几句字符串是与 TS 侧的契约（夹具逐字节比的就是它们）。
    /// 其中六条另有跨语言夹具背书（下一条测试对着入库文件核）；
    /// **`AttachNeedsTmux` 是唯一没有夹具用例的那条**，所以 R4 才存活。
    #[test]
    fn every_refusal_reason_is_pinned_byte_for_byte() {
        let pairs: &[(Refusal, &str)] = &[
            (Refusal::NotInstalled, "远端未装 ccm"),
            (Refusal::NotSsh, "本地路径不走 CLI 渲染器"),
            (Refusal::MissingCap("tmux".into()), "远端 ccm 缺能力 tmux"),
            (
                Refusal::SendIntoHasNoCliForm,
                "send-into（idle-tmux 就地复用）无 CLI 等价语法，诚实降级",
            ),
            (Refusal::AttachNeedsTmux, "attach 必须是 tmux 容器"),
            (
                Refusal::DimensionCannotSpeak("account".into()),
                "维度 account 无法用 CLI 语法表达（cliFlags 返回 null）",
            ),
            (
                Refusal::DimensionNeedsCap {
                    dim: "model".into(),
                    cap: "model".into(),
                },
                "维度 model 需要远端 ccm 能力 model，但它不支持",
            ),
        ];
        assert_eq!(pairs.len(), 7, "Refusal 有七个变体，这张表要全覆盖");
        for (r, want) in pairs {
            assert_eq!(&r.reason(), want, "{r:?} 的降级理由变了");
        }
    }

    /// 上一条那七句里的六句，**在入库夹具里逐字出现**（夹具是 TS 生成的）——
    /// 所以那六句不是自说自话。带文件规模自检：夹具读空时 `contains` 会全假、方向是红，
    /// 但那时报的错会很难懂，所以先断言它有内容。
    #[test]
    fn the_reasons_the_fixture_covers_really_come_from_the_typescript_side() {
        // P4b：搬家后改用 `include_str!` —— 夹具被删/改名 ⇒ **编译失败**，
        // 而不是运行时才发现（同两条 parity 判据的纪律）。
        let fx = include_str!("fixtures/cli-golden.json");
        assert!(fx.len() > 1000, "夹具只有 {} 字节，像是坏了", fx.len());
        for want in [
            "远端未装 ccm",
            "本地路径不走 CLI 渲染器",
            "远端 ccm 缺能力 tmux",
            "send-into（idle-tmux 就地复用）无 CLI 等价语法，诚实降级",
            "维度 account 无法用 CLI 语法表达（cliFlags 返回 null）",
            "维度 model 需要远端 ccm 能力 model，但它不支持",
        ] {
            assert!(fx.contains(want), "夹具里找不到这句降级理由：{want}");
        }
    }

    // ── attach 分支 ─────────────────────────────────────────────────────────

    /// ★ 杀 R4 的行为那一半：`attach` 落在非 tmux 容器上必须被拒。
    /// 夹具里没有这个形态（它只有「attach + tmux」那条 ok 用例）。
    #[test]
    fn attaching_into_a_non_tmux_container_is_refused() {
        let mut s = base_spec();
        s.action = Action::Attach { name: "cc-x" };
        s.container = Container::None;
        assert_eq!(render(&s), Err(Refusal::AttachNeedsTmux));
    }

    /// `ccm attach <名>` 不接受任何修饰 —— 所以它在维度循环**之前**返回，
    /// 连维度要的能力都不收（§33 登记在案的刻意豁免）。
    #[test]
    fn attach_reads_the_container_name_and_no_modifiers_at_all() {
        let mut s = base_spec();
        s.action = Action::Attach {
            name: "被忽略的动作名",
        };
        s.container = Container::Tmux {
            name: "cc-x",
            send_into: false,
        };
        s.ccm_sid = Some("sid-1");
        s.model = Some("opus");
        s.cwd = Some("/w");
        s.launcher = "claude-dev";
        s.account = CliAccount::Named { name: Some("z") };
        assert_eq!(render(&s).as_deref(), Ok("ccm attach cc-x"));
        // 连账号/模型维度的能力都不要 —— 全都缺也照样渲染得出来。
        let only_static: BTreeSet<String> = STATIC_CAPS_EXPECTED
            .iter()
            .map(|c| (*c).to_string())
            .collect();
        assert_eq!(
            render_ccm_invocation(&s, &only_static, true).as_deref(),
            Ok("ccm attach cc-x")
        );
    }

    // ── 维度：逐个的 applies / cliFlags / requiredCaps ───────────────────────

    #[test]
    fn identity_dimension_speaks_only_when_there_is_a_ccm_sid() {
        let d = dim("identity");
        assert!(d.caps.is_empty(), "identity 不该要求任何能力");
        for s in spec_matrix() {
            assert_eq!(d.applies(&s), s.ccm_sid.is_some());
        }
        let mut s = base_spec();
        s.ccm_sid = Some("sid-1");
        assert_eq!(
            (d.cli_flags)(&s),
            Some(vec!["--ccm-sid=sid-1".to_string()]),
            "identity 是 `--ccm-sid=<值>` 一个 token，不是空格分隔的两个"
        );
    }

    #[test]
    fn account_dimension_always_speaks_up_and_has_three_shapes() {
        let d = dim("account");
        assert_eq!(d.caps, &["account"]);
        // **恒真**：F05 的教训 —— 账号维度沉默 = 远端拿到的是「上一次是谁」。
        for s in spec_matrix() {
            assert!(d.applies(&s), "账号维度必须永远表态");
        }
        let mut s = base_spec();
        assert_eq!((d.cli_flags)(&s), Some(vec!["--base".to_string()]));
        s.account = CliAccount::Named { name: Some("z") };
        assert_eq!(
            (d.cli_flags)(&s),
            Some(vec!["--account".to_string(), "z".to_string()])
        );
        s.account = CliAccount::Named { name: None };
        assert_eq!(
            (d.cli_flags)(&s),
            None,
            "只有 configDir 没有名字时必须老实说「说不出」（§35），不能悄悄降级成 --base"
        );
    }

    #[test]
    fn model_dimension_is_conditional_by_design() {
        let d = dim("model");
        assert_eq!(d.caps, &["model"]);
        // §37：没配偏好 ⇒ 不触发 ⇒ 远端 claude 用它自己的默认模型（那正是用户的期望），
        // 也因此**不要求** model 能力。
        for s in spec_matrix() {
            assert_eq!(d.applies(&s), s.model.is_some());
        }
        let mut s = base_spec();
        s.model = Some("opus");
        assert_eq!(
            (d.cli_flags)(&s),
            Some(vec!["--model".to_string(), "opus".to_string()])
        );
    }

    /// R1 / R2 / R3 的处置：**这两个维度在 CLI 渲染器里改不了任何输出**（`cli_flags`
    /// 恒返回空），所以「拓宽/收窄它们的 `applies`」这类变异**不可能**被行为判据杀掉 ——
    /// 那不是判据的缺口，是这两格本身是惰性的。
    ///
    /// 能钉、也值得钉的是**惰性本身**：哪天有人给它们加了真 flag，夹具未必抓得到
    /// （`env-reset` 那格在 CLI 路径上根本到不了，见下一条），这条会红。
    #[test]
    fn the_two_inert_dimensions_contribute_no_flags_for_any_shape() {
        for id in ["env-reset", "nested-env-reset"] {
            let d = dim(id);
            assert!(d.caps.is_empty(), "{id} 声明了能力，但它一个 flag 都不吐");
            for s in spec_matrix() {
                assert_eq!(
                    (d.cli_flags)(&s),
                    Some(Vec::<String>::new()),
                    "{id} 开始吐 flag 了 —— 那它的 applies 就成了活判据，\
                     请回来给夹具补用例（尤其 env-reset：它在 CLI 路径上到不了）"
                );
            }
        }
    }

    /// ★ 杀 R2（`env-reset` 的 `applies` 被改成恒真）。
    ///
    /// `env-reset` 只在 `send_into: true` 时触发，而那种形态**在维度循环之前**就被
    /// #76 防线拒掉了 —— 也就是说这一格在本渲染器里**不可达**。把这条不可达性钉成判据：
    /// 哪天 #76 防线被挪走，它会红，提醒「env-reset 变活了，去补夹具」。
    #[test]
    fn env_reset_can_never_be_reached_in_the_cli_renderer() {
        let d = dim("env-reset");
        let mut seen = 0;
        for s in spec_matrix() {
            if d.applies(&s) {
                seen += 1;
                assert_eq!(
                    render(&s),
                    Err(Refusal::SendIntoHasNoCliForm),
                    "env-reset 触发了、却渲染出了东西 —— 这一格从不可达变成了活的"
                );
            }
        }
        assert!(seen > 0, "矩阵里没有一个 spec 触发 env-reset —— 这条恒真了");
    }

    // ── 三条铁律：#76 防线 / §35 安全网 / 能力闸逐维度交错 ───────────────────

    #[test]
    fn send_into_is_refused_before_any_dimension_runs() {
        let mut s = base_spec();
        s.container = Container::Tmux {
            name: "cc-x",
            send_into: true,
        };
        assert_eq!(render(&s), Err(Refusal::SendIntoHasNoCliForm));
        // 「在维度之前」这半：连账号说不出（§35）都不该抢在它前面报。
        s.account = CliAccount::Named { name: None };
        assert_eq!(
            render(&s),
            Err(Refusal::SendIntoHasNoCliForm),
            "#76 防线必须先于维度循环 —— 否则同一个上下文会报出另一条诊断"
        );
    }

    #[test]
    fn a_dimension_that_cannot_speak_abandons_the_whole_line() {
        let mut s = base_spec();
        s.account = CliAccount::Named { name: None };
        s.cwd = Some("/w");
        assert_eq!(
            render(&s),
            Err(Refusal::DimensionCannotSpeak("account".into())),
            "§35：说不出就整条放弃，不是跳过这个维度继续渲染出一条丢了账号的命令"
        );
    }

    #[test]
    fn a_triggered_dimension_carries_its_own_capability_requirement() {
        let mut s = base_spec();
        s.model = Some("opus");
        assert_eq!(
            render_ccm_invocation(&s, &caps_without("model"), true),
            Err(Refusal::DimensionNeedsCap {
                dim: "model".into(),
                cap: "model".into()
            })
        );
        // 没触发就不收 —— 缺 model 能力但没配模型时照样渲染得出来（§37）。
        assert!(render_ccm_invocation(&base_spec(), &caps_without("model"), true).is_ok());
    }

    /// 能力检查与 flags 是**逐维度交错**的，不是「先把所有能力查完再渲染」。
    /// 两个方向各一条 —— 把两种「看起来等价」的实现区分开。
    #[test]
    fn the_capability_gate_is_interleaved_per_dimension_not_hoisted() {
        // 方向一：靠前的维度说不出、靠后的维度缺能力 ⇒ 必须报**靠前那条**。
        // 把能力检查整体提到循环外，这里会报 DimensionNeedsCap{model}。
        let mut s = base_spec();
        s.account = CliAccount::Named { name: None }; // account(20) 说不出
        s.model = Some("opus"); // model(25) 要 model 能力
        assert_eq!(
            render_ccm_invocation(&s, &caps_without("model"), true),
            Err(Refusal::DimensionCannotSpeak("account".into())),
            "能力检查被提到了维度循环外 —— reason 是生产侧唯一的降级线索，换一个就是换一条诊断"
        );

        // 方向二：**同一个**维度既缺能力又说不出 ⇒ 报缺能力（本维度内能力先于 flags）。
        let mut s = base_spec();
        s.account = CliAccount::Named { name: None };
        assert_eq!(
            render_ccm_invocation(&s, &caps_without("account"), true),
            Err(Refusal::DimensionNeedsCap {
                dim: "account".into(),
                cap: "account".into()
            })
        );
    }

    // ── 整条命令的形状 ──────────────────────────────────────────────────────

    /// 一条把五个维度里会吐 flag 的三个**同时**触发的命令 —— 顺序即契约，
    /// 而且它同时钉住 `--tmux=`/`--cwd`/`--launcher`/`--` 各自的位置。
    #[test]
    fn a_fully_loaded_invocation_emits_every_part_in_registry_order() {
        let mut s = base_spec();
        s.action = Action::Resume { sid: "s1" };
        s.container = Container::Tmux {
            name: "cc-x",
            send_into: false,
        };
        s.ccm_sid = Some("sid-1");
        s.account = CliAccount::Named { name: Some("z") };
        s.model = Some("opus");
        s.cwd = Some("/w");
        s.launcher = "claude-dev";
        s.args = &["-p"];
        assert_eq!(
            render(&s).as_deref(),
            Ok(concat!(
                "ccm resume s1 --tmux=cc-x --ccm-sid=sid-1 ",
                "--account z --model opus --cwd /w --launcher claude-dev -- -p"
            ))
        );
    }

    /// ★ 杀 R6（`args` 那一段整段失效 —— 此前无人红：夹具里没有带 args 的用例，
    /// 因为生产今天零 producer）。`--` 之后逐个 token 各自 quote，不是拼成一个串。
    #[test]
    fn args_go_after_a_bare_double_dash_and_are_quoted_one_by_one() {
        let mut s = base_spec();
        s.args = &["-p", "两个 词"];
        assert_eq!(render(&s).as_deref(), Ok("ccm new --base -- -p '两个 词'"));
        let mut s = base_spec();
        s.args = &[];
        assert_eq!(
            render(&s).as_deref(),
            Ok("ccm new --base"),
            "空 args 不该吐出一个孤零零的 --"
        );
    }

    #[test]
    fn launcher_is_only_named_when_it_differs_from_the_default() {
        let mut s = base_spec();
        s.launcher = "claude";
        assert_eq!(render(&s).as_deref(), Ok("ccm new --base"));
        s.launcher = "claude-dev";
        assert_eq!(
            render(&s).as_deref(),
            Ok("ccm new --base --launcher claude-dev")
        );
    }

    // ── argv 的 quote 边界 ─────────────────────────────────────────────────

    /// ★ 杀 R7（放行集里少一个字符 —— 此前无人红：夹具的 token 里恰好没有 `.`）。
    ///
    /// 判据自带放行集（见本模块头注：遍历被测代码自己是恒真的）。与 TS
    /// `argv()` 的 `/^[A-Za-z0-9_@%+=:,./-]+$/` 同一份规则。
    const ARGV_BARE_CHARS: &str = "_@%+=:,./-";

    #[test]
    fn argv_leaves_every_allowed_character_bare() {
        for c in ARGV_BARE_CHARS.chars() {
            let token = format!("a{c}b");
            assert_eq!(
                argv(&token),
                token,
                "{c:?} 在放行集里，却被 quote 了 —— 与 TS 的 argv() 分家了"
            );
        }
        for token in ["abc", "ABC0", "/usr/local/bin/claude", "v1.2.3-rc.1"] {
            assert_eq!(argv(token), token);
        }
    }

    #[test]
    fn argv_quotes_everything_else_including_the_empty_token() {
        assert_eq!(argv(""), "''", "空 token 不 quote 会在命令行里整个消失");
        for token in ["a b", "a;b", "a|b", "$(id)", "a\nb", "中文", "a*b", "a>b"] {
            let got = argv(token);
            assert_ne!(got, token, "{token:?} 该被 quote");
            assert!(
                got.starts_with('\'') && got.ends_with('\''),
                "{token:?} 的 quote 结果形状不对：{got}"
            );
        }
        // 单引号自身走内核那份逃逸（`quote_singleton_guard` 钉住只有一个家）。
        assert_eq!(argv("a'b"), launch_core::posix_quote("a'b"));
    }
}
