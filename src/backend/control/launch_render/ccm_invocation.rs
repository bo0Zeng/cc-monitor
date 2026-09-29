//! U8c-2c-1：**`ccm …` 调用行的渲染器** —— 全仓唯一一份（〔LR1 · U8c-3〕TS 那份对侧已删）。
//!
//! ⚠ **P4b 搬家**：它原来是共享 crate（当时叫 `launch-core`）的 `cli` 模块 —— 而 **backend 对它零引用**。
//! 架构审计点破「这就是决策内核，放在共享 crate 里的真实原因是 monitor 没处放」。
//! 现在住 `backend/control/`：§1.3 把最终 exec 钉在用户自己的终端进程里，
//! U8a-2b 把后端的执行面定成 argv 直传、不过 shell ⇒ **渲染 shell 串永远属于开终端那一侧。**
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
//! # 「该渲成什么」的独立说法住哪
//!
//! 〔LR1 · U8c-3〕TS 那份渲染器删了之后，不再有「另一种语言的实现」可对拍。独立说法是两份
//! **手写**的期望：本文件的自测（P1 那批，判据自带清单、不遍历被测常量，见文件尾）与入库夹具
//! `fixtures/cli-golden.json`（`src/frontend/ui/launch-cli-golden.ts` 用例表手写 `out`，`req` 由生产的
//! TS 请求构造现产 ⇒ 顺带钉住线与映射，`launch_cli_parity.rs`）。
//! ⚠ **ok 与 refusal 两类都要覆盖** —— 只比 ok 的话，「该降级却渲染出来了」抓不到，
//! 而那正是 §33 铁律要防的形态。

use copy_core::copy_text;
use std::collections::BTreeSet;

/// 渲染不出 ccm 调用行时的**理由**。它是一等返回值，不是 `None`（见模块头注）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    NotInstalled,
    // 〔MIG-2〕原先这里有 `ProbeUnknown`（界面带来的「没探出来」那一态）：渲染进了那台后端、能力问它自己，那一态产不出来了，删。
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
    /// 〔TL3 · `INVARIANTS §47` ②〕一个自由文本值过不了拼进命令之前的放行判定
    /// （工作目录：POSIX 绝对 · 无 `..` 段 · 不含 NUL / CR / LF；透传参数：不含 NUL / CR / LF）。`value` = 原值（`{:?}` 形）。
    FreeTextRefused {
        slot: FreeTextSlot,
        value: String,
    },
    /// 〔DUP1 · `INVARIANTS §47` ①〕一个**标识符**值过不了拼进命令之前的放行判定（闭集白名单 ＋ 不许 `-` 开头 ＋ 钉上界，
    /// 判定住 `shell-quote-core`，全仓唯一一份）。`value` = 原值（`{:?}` 形）。
    IdentifierRefused {
        slot: IdentifierSlot,
        value: String,
    },
}

/// [`Refusal::IdentifierRefused`] 是哪一格（各有各的一句话，文案走表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierSlot {
    /// `resume <sid>`（`shell_quote_core::session_id_ok`）。
    Sid,
    /// `--ccm-sid=`（身份标记；同一条 sid 规则）。
    CcmSid,
    /// `--model <名>`（`shell_quote_core::model_name_ok`）。
    Model,
    /// `--account <名>`（`shell_quote_core::account_name_ok`）。
    Account,
    /// 〔DUP2 · J6〕`--tmux=<名>`：本工具要**新建**的会话名（`gate_core::new_tmux_name_issue`）。
    TmuxName,
}

/// [`Refusal::FreeTextRefused`] 是哪一格（各有各的一句话，文案走表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreeTextSlot {
    Cwd,
    AgentArg,
    /// 〔DUP2 · J6〕`attach <名>`：一个**已有**会话的名字（V131 ②：`gate_core::existing_tmux_name_issue`，拒绝集 ＋ 非空）。
    AttachTarget,
}

impl Refusal {
    /// 降级理由。入库夹具 `cli-golden.json` 的 refusal 用例逐字节比它（夹具那一侧是手写期望）。
    pub fn reason(&self) -> String {
        match self {
            Refusal::NotInstalled => copy_text("rsCcmInvocation.refusal.notInstalled", &[]),
            // ⚠ **P3t 之后这句话比事实宽**（登记在案的诚实边界，不是没看见）：
            // Rust 侧现在只在 `!is_ssh && !local_posix` 时回它，也就是**Windows 本机**。
            // 它今天**产不出来**：两个活着的 Rust 调用方一个恒 `is_ssh: true`
            // （`launch_wire`，前端只在 ssh 时才调），一个恒 `local_posix: true`
            // （〔MIG-2〕今天是 `local.rs::render_ccm`，只在非 Windows 那一支走到）。
            // 〔LR1 · U8c-3〕原先挡着改字的那条（与 TS 渲染器逐字节对拍）随 TS 那份删了；
            // 〔CP2b〕进文案表那一拍按 CP1 裁词（改·§2.1）改成说「Windows 本机」，
            // `src/frontend/ui/launch-cli-golden.ts` 里「本地 transport」那条用例的期望同拍改。
            Refusal::NotSsh => copy_text("rsCcmInvocation.refusal.windowsLocal", &[]),
            Refusal::MissingCap(c) => copy_text(
                "rsCcmInvocation.refusal.missingCap",
                &[("c", &c.to_string())],
            ),
            Refusal::SendIntoHasNoCliForm => copy_text("rsCcmInvocation.refusal.sendInto", &[]),
            Refusal::AttachNeedsTmux => copy_text("rsCcmInvocation.refusal.attachNeedsTmux", &[]),
            Refusal::DimensionCannotSpeak(id) => copy_text(
                "rsCcmInvocation.refusal.cannotSpeak",
                &[("id", &id.to_string())],
            ),
            Refusal::DimensionNeedsCap { dim, cap } => copy_text(
                "rsCcmInvocation.refusal.dimensionNeedsCap",
                &[("dim", &dim.to_string()), ("cap", &cap.to_string())],
            ),
            Refusal::FreeTextRefused {
                slot: FreeTextSlot::Cwd,
                value,
            } => copy_text("rsCcmInvocation.refusal.freeTextCwd", &[("value", value)]),
            Refusal::FreeTextRefused {
                slot: FreeTextSlot::AgentArg,
                value,
            } => copy_text("rsCcmInvocation.refusal.freeTextArg", &[("value", value)]),
            Refusal::FreeTextRefused {
                slot: FreeTextSlot::AttachTarget,
                value,
            } => copy_text(
                "rsCcmInvocation.refusal.freeTextAttach",
                &[("value", value)],
            ),
            Refusal::IdentifierRefused {
                slot: IdentifierSlot::Sid,
                value,
            } => copy_text("rsCcmInvocation.refusal.idSid", &[("value", value)]),
            Refusal::IdentifierRefused {
                slot: IdentifierSlot::CcmSid,
                value,
            } => copy_text("rsCcmInvocation.refusal.idCcmSid", &[("value", value)]),
            Refusal::IdentifierRefused {
                slot: IdentifierSlot::Model,
                value,
            } => copy_text("rsCcmInvocation.refusal.idModel", &[("value", value)]),
            Refusal::IdentifierRefused {
                slot: IdentifierSlot::Account,
                value,
            } => copy_text("rsCcmInvocation.refusal.idAccount", &[("value", value)]),
            Refusal::IdentifierRefused {
                slot: IdentifierSlot::TmuxName,
                value,
            } => copy_text(
                "rsCcmInvocation.refusal.idTmuxName",
                &[
                    ("value", value),
                    ("refused", gate_core::NEW_TMUX_NAME_REFUSED),
                    ("max", &gate_core::NEW_TMUX_NAME_MAX.to_string()),
                ],
            ),
        }
    }
}

/// CLI 语法覆盖面 —— **每次调用都无条件要求**的能力。
///
/// 全仓唯一一份（〔LR1〕TS 那份随 TS 渲染器删了；`tests/e2e/ccm-contract-parity.sh` 从本文件
/// 抽它去比真 `ccm --ccm-probe` 的 `capabilities=`）。只放「与具体维度无关的
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

/// 账号维度的三态（同 [`crate::Account`] 的两态 ＋ 「调用方没表态」那一态；
/// CLI 侧还需要**名字**才能说出 `--account`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAccount<'a> {
    /// 具名账号。`name: None` = 只有 configDir 没有名字 ⇒ **说不出 `--account`** ⇒ §35 短路。
    Named {
        name: Option<&'a str>,
    },
    Base,
    /// 🔴 **`K-R89`（09-13）新加的第三态：调用方没表态（继承）。**
    ///
    /// # 它渲染成什么：**什么都不加**
    ///
    /// 既不发 `--account` 也不发 `--base`。这**不是**「这一维沉默了」（F05 禁的那个），
    /// 是「**省略在这条 CLI 上有确定语义**」—— 语义的唯一住址是
    /// `src/backend/control/ccm/plan.rs::resolve_account`，它把省略拆成两支：
    ///
    /// - `CLAUDE_CONFIG_DIR` **非空** ⇒ 保留不覆盖（`R08` 那道 `-z` 闸，
    ///   由一次真机复现过的静默换号逼出来）= **继承**；
    /// - 都没给（裸终端）⇒ 落 manifest 的 `isDefault`。
    ///
    /// **两支都是用户 09-12 亲裁要的行为**（`DECISIONS.md#R28` 逐字：
    /// 「把调用方选中的号静默换掉 / 不要这么做 / 不是有选默认账号吗? 就用那个」）。
    /// ⇒ 在 `R28` 之前这一态只能 §35 短路（`--base` 是「显式清空」≠「继承」，映过去就是 #75）；
    /// `R28` 之后它有了确定语义，于是**说得出话了**。
    ///
    /// # ⚠ 它今天只有**本机**那条路在用，远端不许照抄
    ///
    /// 唯一构造点是 `local.rs::render_ccm_with`（〔MIG-2〕只在非 Windows 那一支走到）。
    /// [`super::launch_wire::WireAccount`] **刻意没有对应变体** —— 远端是 ssh 过去，
    /// **那台机器上的继承态不是 monitor 的环境**（`R28` 裁定四逐字）⇒
    /// 「远端的继承怎么表达」是 `K-R90`，不是本变体。
    Inherit,
}

/// 渲染 ccm 调用行所需的全部输入（TS `LaunchPlan` + `LaunchContext` 的交集）。
#[derive(Debug, Clone)]
pub struct CliSpec<'a> {
    pub is_ssh: bool,
    /// P3t（`C12`）：**本机是不是 POSIX** —— 宿主告诉 backend 的，backend 自己不问平台。
    ///
    /// `backend/` 那一半不许有平台 cfg（`backend-split` 的 C10），所以这条是**注入的数据**，
    /// 与 `platform::fs::make_executable` 同款。缺省 `false` = **fail-closed**：
    /// 没人告诉过它就当不是 POSIX ⇒ 照旧拒 ⇒ 与本件之前的行为逐字相同。
    pub local_posix: bool,
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

/// argv token 的 quote：只在含 ccm 允许字符集之外的东西时才包单引号（〔LR1〕TS 那份同规则的 `argv()` 随 TS 渲染器删了）。
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
        shell_quote_core::posix_quote(token)
    }
}

/// 一个维度在 CLI 语境下要说的话。`None` = **说不出** ⇒ §35 短路。
type Flags = Option<Vec<String>>;

/// 五个维度的 `applies` + `cliFlags` + `requiredCaps`，**顺序即契约**
/// （`identity`(5) < `env-reset`(10) < `account`(20) < `model`(25) < `nested-env-reset`(30)）。
///
/// ⚠ 这里只实现 `cliFlags` 那一半 —— `apply`（产 `EnvOp`）那一半 U8c-1 已经在
/// [`crate::render_payload`] 里了。
fn dimension_flags(
    spec: &CliSpec,
    caps: &BTreeSet<String>,
) -> Result<Vec<(&'static str, Vec<String>)>, Refusal> {
    let mut out = Vec::new();
    // ⚠ **能力检查与 flags 是逐维度交错的**（与已删的 TS 渲染器那个维度循环同构）——
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
        let mut flags = Vec::new();
        push(&mut flags, dim.id, (dim.cli_flags)(spec))?;
        out.push((dim.id, flags));
    }
    Ok(out)
}

/// 一个维度在 CLI 侧的三个钩子。TS `LaunchDimension` 今天只剩 `applies` / `apply` 两格
/// （`apply` 产 `EnvOp`，渲染那一半 U8c-1 已经搬进 `render_payload`）；
/// 〔LR1 · U8c-3〕TS 维度上的 `cliFlags` / `requiredCaps` 随 TS 渲染器删了，本表是它们唯一的家。
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
        //
        // 🔴 `K-R89`（09-13）：**「表态」不等于「一定要吐一个 flag」**。
        // [`CliAccount::Inherit`] 是**表了态的省略** —— 省略在 `ccm` 上有确定语义
        //（`plan.rs::resolve_account` 的两支，用户 09-12 `R28` 亲裁）。
        // F05 禁的是「这一维没人管、于是身份被别的东西决定」，不是「这一维的答案恰好是空」。
        applies: |_| true,
        cli_flags: |s| match s.account {
            CliAccount::Base => Some(vec!["--base".into()]),
            CliAccount::Named { name: Some(n) } => Some(vec!["--account".into(), n.to_string()]),
            // 只有 configDir 没有名字 ⇒ 老实说「我说不出 --account」⇒ 整条降级（§35）。
            CliAccount::Named { name: None } => None,
            // 🔴 继承 ⇒ **一个 flag 都不吐**（`R28`）。⚠ `Some(vec![])` 与上一行的 `None`
            // 是两件完全不同的事：前者「我说得出，答案是省略」，后者「我说不出，整条降级」。
            CliAccount::Inherit => Some(vec![]),
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

/// `ctx → ccm 调用行`。生产入口是 `launch_wire::render_ccm_launch`（远端）与本机 POSIX 那一条。
pub fn render_ccm_invocation(
    spec: &CliSpec,
    caps: &BTreeSet<String>,
    installed: bool,
) -> Result<String, Refusal> {
    if !installed {
        return Err(Refusal::NotInstalled);
    }
    // ★★ **P3t（`C12`）：POSIX 本机放行，Windows 本机仍拒**。
    //
    // # 原来这里是「一律拒本机」，而那比 §36（只绑 Windows）说的宽
    //
    // `src/doc/INVARIANTS.md` 的 §36 逐字是「本地（**Windows**）路径不经 IR」，
    // 而 `launch_wire.rs` 的注释写成泛指的「本机」、代码按注释的宽度实现。三者不一致。
    // §36 那一行的「说明」列讲的全是 Windows 分支（`config_dir_prefix_ps` /
    // `validate_config_dir_ps`）与「`\` 与盘符」问题 —— **那些理由在 POSIX 上一条都不适用**。
    // ⇒ 本件采信「代码窄了」：放行 POSIX 是**在兑现 §36（只绑 Windows）的原意**，不是破例（P3t-Y4）。
    //
    // # 为什么必须放行（不是「为了对齐而对齐」）
    //
    // 不放行 ⇒ 本机走 `history.rs` 那条旧路 ⇒ 产出 `cc --resume <sid>` **不带 `--tmux`**
    // ⇒ ccm 走非容器分支 `exec`，加上 `launch_local_posix` 的 stdio 全 null
    // ⇒ **一个无 tty、无 tmux 的 claude 进程，用户敲进去的字会被脚本吃掉**
    //（`launch_local_posix` 头注与 `src/doc/IPC-PROTOCOL.md` 各记了一份）。
    // ⇒ 本件不是「加个容器求平价」，是**修一个今天就坏的东西**。
    if !spec.is_ssh && !spec.local_posix {
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

    // 〔V151 · 用户 09-27〕`ccm [交给 claude 的…] -- [ccm 自己的…]`：两半分开收，最后按 [`join_v151`] 拼。
    let mut tokens: Vec<String> = vec![spec.ccm_path.to_string()];
    let mut ours: Vec<String> = Vec::new();

    // attach 分支**在维度循环之前 return** —— `ccm --attach <名>` 不接受任何修饰 flag，
    // 所以它也不收集维度的 requiredCaps（§33 里登记在案的刻意豁免，不是回退）。
    if let Action::Attach { name } = spec.action {
        let Container::Tmux { name: cname, .. } = spec.container else {
            return Err(Refusal::AttachNeedsTmux);
        };
        let _ = name;
        // 〔DUP2 · J6 · `INVARIANTS §47` ②〕一个已有会话的名字：拒绝集（控制符 · 欺骗字符）＋ 非空，规则住 gate-core（全仓唯一一份）。
        //   ccm 那头按 `=<名>:` 精确寻址（`plan.rs` 的 attach 那一行），`*` `?` 不被当通配。界面那份删了之后这条路自己判。
        if gate_core::existing_tmux_name_issue(cname).is_some() {
            return Err(Refusal::FreeTextRefused {
                slot: FreeTextSlot::AttachTarget,
                value: format!("{cname:?}"),
            });
        }
        // V138：位置动作取消，接回用 ccm 的壳层选项 `--attach`。
        ours.push("--attach".into());
        ours.push(cname.to_string());
        return Ok(join_v151(tokens, ours));
    }

    match spec.action {
        Action::Resume { sid } => {
            // 〔DUP1 · §47 ①〕resume 的 sid 是标识符：共享那一份判（前端那份删了，`设计/90 §3` 判据 2）。
            if !shell_quote_core::session_id_ok(sid) {
                return Err(Refusal::IdentifierRefused {
                    slot: IdentifierSlot::Sid,
                    value: format!("{sid:?}"),
                });
            }
            // V138：`--resume <sid>` 是 claude 的旗标，ccm 原样交过去。
            tokens.push("--resume".into());
            tokens.push(sid.to_string());
        }
        // 〔V153 · 用户 09-27〕起新会话是 ccm 自己的位置词 `new`，写在 `--` 右边第一个（`ccm -- new …`）。
        Action::New => ours.push("new".into()),
        _ => {}
    }
    // 〔DUP1 · §47 ①〕身份标记同一条 sid 规则（`--ccm-sid=` 由下面的 identity 维度吐）。
    if let Some(s) = spec.ccm_sid.filter(|s| !shell_quote_core::session_id_ok(s)) {
        return Err(Refusal::IdentifierRefused {
            slot: IdentifierSlot::CcmSid,
            value: format!("{s:?}"),
        });
    }
    // 〔DUP1 · §47 ①〕账号名（`--account <名>` 由下面的 account 维度吐）：与建账号的那个工具逐字同的那一份判。
    if let CliAccount::Named { name: Some(n) } = spec.account {
        if !shell_quote_core::account_name_ok(n) {
            return Err(Refusal::IdentifierRefused {
                slot: IdentifierSlot::Account,
                value: format!("{n:?}"),
            });
        }
    }
    // 〔DUP1 · §47 ①〕模型名（`--model <名>` 由下面的 model 维度吐）：共享那一份判，前端那份删了。
    if let Some(m) = spec.model.filter(|m| !shell_quote_core::model_name_ok(m)) {
        return Err(Refusal::IdentifierRefused {
            slot: IdentifierSlot::Model,
            value: format!("{m:?}"),
        });
    }
    if let Container::Tmux { name, .. } = spec.container {
        // 〔DUP2 · J6 · `INVARIANTS §47` ①〕`--tmux=<名>` 是要**新建**的会话名：新建那一条（gate-core，全仓唯一一份）。
        //   今天之前这条路零判定、只靠界面那道 TS 谓词 —— 那份删了，这里接上（「对端会校验」不是理由）。
        if gate_core::new_tmux_name_issue(name).is_some() {
            return Err(Refusal::IdentifierRefused {
                slot: IdentifierSlot::TmuxName,
                value: format!("{name:?}"),
            });
        }
        ours.push(format!("--ccm-tmux={name}")); // 用户 09-26：ccm 的 tmux 旗标改名（claude 自己有 `--tmux`）
    }

    // 〔V151〕维度吐的旗标分两半：`--model` 是 claude 的（V138），其余（身份 · 账号）是 ccm 的。
    for (dim, flags) in dimension_flags(spec, caps)? {
        if dim == "model" {
            tokens.extend(flags);
        } else {
            ours.extend(flags);
        }
    }

    if let Some(cwd) = spec.cwd {
        // 〔TL3 · §47 ②〕工作目录是自由文本路径：形式 ＋ 拒绝集（与载荷那条路同一个判定 `shell_quote_core::posix_free_path_ok`）。
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
    // 〔TL3 · §47 ②〕透传给 agent 的参数是自由文本：拒绝集只收 NUL / CR / LF（元字符交给 `argv` 里那一处 quote）。
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
    Ok(join_v151(tokens, ours))
}

/// 〔V151〕`<ccm> <交给 claude 的…> -- <ccm 自己的…>`：ccm 那一半空、而 claude 那一半里没有 `--` ⇒ 不写 `--`；
/// claude 那一半里有它自己的 `--` ⇒ 末尾照样补一个（按最后一个 `--` 切，空的 ccm 部分也得标出来）。
fn join_v151(mut claude: Vec<String>, ours: Vec<String>) -> String {
    if !ours.is_empty() || claude.iter().skip(1).any(|a| a == "--") {
        claude.push("--".into());
        claude.extend(ours);
    }
    claude.iter().map(|t| argv(t)).collect::<Vec<_>>().join(" ")
}

// ────────────────────────────────────────────────────────────────────────────
// P1（2026-08-03 三视角复盘）：**本文件此前一条自测都没有。**
//
// 为什么必须补，而不是靠跨语言夹具：当时挡「两侧一起错」的是 TS 那份渲染器的自测，
// 而它**排期在 U8c-3 被删**（〔LR1〕已删）。那天一到，跨语言对拍就不再有另一种语言 ——
// 「渲染器该做什么」这件事的独立说法只剩这里这批手写期望（＋ 夹具里手写的 `out`）。
//
// 补之前先量：对本文件逐条造变异、只跑现有门禁（当时那个 crate 15 条 + monitor 侧
// `_parity` 9 条），**七个存活**（R1–R7）。对照组「`--base` 改字」「account/model 换序」
// 都当场红，所以那个量具不是恒绿的。下面每条测试都注明它杀的是哪个。
//
// ⚠ 两处**刻意的重复**（`STATIC_CAPS_EXPECTED` / `ARGV_BARE_CHARS`）：判据必须自带清单，
// **不许遍历被测常量自己** —— 那是恒真的。R5 存活的原因正是这个形状：把 `"cwd"` 从
// `CLI_REQUIRED_CAPS` 删掉，任何「遍历该常量逐项抽掉」的循环也就不再测 `"cwd"`，照样全绿。
#[cfg(test)]
#[path = "../../../../tests/backend/control/launch_render/ccm_invocation_tests.rs"]
mod tests;
