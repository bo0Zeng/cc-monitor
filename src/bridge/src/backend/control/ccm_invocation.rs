//! U8c-2c-1：**`ccm …` 调用行的渲染器** —— TS `launch-render-cli.ts::tryRenderCli` 的 Rust 对侧。
//!
//! ⚠ **P4b 搬家**：它原来是共享 crate（当时叫 `launch-core`）的 `cli` 模块 —— 而 **daemon 对它零引用**。
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
            // ⚠ **P3t 之后这句话比事实宽**（登记在案的诚实边界，不是没看见）：
            // Rust 侧现在只在 `!is_ssh && !local_posix` 时回它，也就是**Windows 本机**。
            // 不改它的理由有两条，都不是「懒」：
            // ① 它与 TS `launch-render-cli.ts:76` **逐字节对拍**（金串 `cli-golden.json` 也存了这一条），
            //    改 Rust 不改 TS 会当场红；而那个 TS 函数已降级为「只供夹具对拍」、**排期 U8c-3 删掉**。
            // ② 它今天**产不出来**：两个活着的 Rust 调用方一个恒 `is_ssh: true`
            //    （`launch_wire`，前端只在 ssh 时才调），一个恒 `local_posix: true`
            //    （`history.rs::render_local_ccm`，整个函数挂在 `cfg(not(windows))` 下）。
            // ⇒ 等 U8c-3 删掉 TS 那份时，这句连同它的金串用例一起改成「Windows 本机…」。
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
    /// 唯一构造点是 `history.rs::render_local_ccm_with`（整段 `#[cfg(not(windows))]`）。
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
    /// 与 `platform_fs::make_executable` 同款。缺省 `false` = **fail-closed**：
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

/// `ctx → ccm 调用行`。与 TS `tryRenderCli` 同构、逐字节对拍。
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
// 补之前先量：对本文件逐条造变异、只跑现有门禁（当时那个 crate 15 条 + monitor 侧
// `_parity` 9 条），**七个存活**（R1–R7）。对照组「`--base` 改字」「account/model 换序」
// 都当场红，所以那个量具不是恒绿的。下面每条测试都注明它杀的是哪个。
//
// ⚠ 两处**刻意的重复**（`STATIC_CAPS_EXPECTED` / `ARGV_BARE_CHARS`）：判据必须自带清单，
// **不许遍历被测常量自己** —— 那是恒真的。R5 存活的原因正是这个形状：把 `"cwd"` 从
// `CLI_REQUIRED_CAPS` 删掉，任何「遍历该常量逐项抽掉」的循环也就不再测 `"cwd"`，照样全绿。
#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/ccm_invocation_tests.rs"]
mod tests;
