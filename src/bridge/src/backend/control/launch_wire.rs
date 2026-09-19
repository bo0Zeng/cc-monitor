//! U8c-2c-2：**生产切换** —— `ccm 调用行`改由 Rust 渲染。
//!
//! # 只切 CLI 那一支，为什么
//!
//! `remote-launch-run.ts::renderLaunchCommand` 有两支：
//! `tryRenderCli`（装了 ccm 时走，产 `ccm …`）与 `renderFallback`（没装时走，产裸载荷）。
//!
//! - **CLI 那支是真在跑的那支**（U8c-2b-0 摸底：装了 ccm 就直接 return，兜底根本不执行）；
//! - **兜底那支切不动**：`container: tmux` 时它要外层 tmux 命令（`session-backend.ts`），
//!   而 `src/doc/INVARIANTS.md` §33b 写死了「删/搬 `session-backend.ts` 前必须先回答三件事」。
//!   🔴 **那三问今天不是当年那三问了**（`K-R105` 09-13 第四次复裁）：第三问
//!   （daemonless 的远端要不要能起会话）**已随定框 `K35` / `K-R59` 退役**，
//!   第一问的答案也在 `K-P2 D3`（09-03）之后变过一次。**三问的今天版只有一个家**：
//!   `src/doc/INVARIANTS.md §33b` 那张表，由 `doc_claim_registry` 逐问与现场对拍
//!   —— 别在这里复述它们，复述就会漂（这一行原来就复述着一份，已撤）。
//!
//! ⇒ 本件切 CLI 支，兜底支原样留在 TS。**两支的判据都还在**（各自的黄金串夹具）。
//!
//! # 返回值为什么是 tagged 而不是 `Result`
//!
//! 「渲染不出来」**不是错误**，是**诚实降级**（§33）—— 调用方要拿着 `reason` 去走兜底。
//! 用 `Result` 的 `Err` 表达它，会和「IPC 真的失败了」混成一件事，
//! 而那两件事在前端要走**不同的分支**。

use super::ccm_invocation::{render_ccm_invocation, Action, CliAccount, CliSpec, Container};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// `ccm 调用行`的上线入参。字段与 TS `LaunchContext` + 探测结果一一对应。
///
/// ⚠ `deny_unknown_fields`：前端多送一个字段 ⇒ **拒**，不静默吞（同夹具那两份的纪律）。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CliRenderRequest {
    /// `false` = 本机路径。
    ///
    /// ★★ **P3t-Y4 订正**：这里原本写「本机不走 CLI 渲染器（§36 —— 而它只绑 Windows），
    /// Rust 侧也照样拒」——**两半都不准**。
    ///
    /// ① §36 逐字管的是别的事，而且它整节只绑 Windows。它的标题后半句就是它的全部内容：
    ///    「**嵌套 env 污染保护已在进程启动期做完，别在本地渲染器里重复实现**」，
    ///    铁律那段逐字禁的是「给本地渲染器补一段读 `plan.env`、把 `unset` 翻成 PowerShell
    ///    `Remove-Item Env:\X` 的代码」。它**从来不是**一条「本机不许用 CLI 渲染器」的禁令，
    ///    而且它整节讲的是 **Windows**（`config_dir_prefix_ps` / `validate_config_dir_ps`）。
    ///    ⇒ 拿它当「一律拒本机」的依据是**把一条窄铁律读宽了**。
    ///
    /// ② 「Rust 侧也照样拒」自 P3t（`C12`）起就不成立：`render_ccm_invocation` 对
    ///    **POSIX 本机放行**（`CliSpec::local_posix`），只有 Windows 本机仍拒。
    ///
    /// 那么**本条上线路今天为什么仍然只见 `is_ssh: true`**？不是因为 §36 禁了（它只绑 Windows），
    /// 是因为**前端只在 `transport.kind === "ssh"` 时才调这条 IPC** ——
    /// POSIX 本机那条路住在 Rust 里（`history.rs::render_local_ccm`），
    /// 不必绕一圈 IPC 问自己。⇒ 这是**路由事实**，不是禁令。
    pub is_ssh: bool,
    /// `null` = **拿不到能力集**。
    ///
    /// # 🔴 `K-R95` 登记的缺口：这条线只有两态，而**它上游有三态**
    ///
    /// 前端那一侧从 `K-R53` 起是**三态判别联合**（`ccm-probe.ts`：`installed` /
    /// `not-installed` / `unknown`，`unknown` 连缓存都不进，理由住那个文件的头注）。
    /// 而这里只有 `Some`/`None` ⇒ `unknown` **一过线就被压成「没装」**
    /// ⇒ 后端回 `Refusal::NotInstalled`（逐字「远端未装 ccm」）。
    /// **一次 ssh 抖动，用户被告知「那台机器上没有 ccm」** —— 正是 `K-R53` 治掉的那一形，
    /// 只不过它换到了线上：值那一侧分得开，线上又合回去了。
    ///
    /// ⚠ **本件没修它**，不是没看见：补第三态要给这个结构加一个字段，而那要同步改
    /// `src/launch-cli-wire.ts`（TS 那份**手写镜像**，由
    /// `tests/launch-cli-wire.vitest.ts` 的「字段集相等」钉着）与
    /// `remote-launch-run.ts::buildCliRenderRequest`（真正填它的地方）——
    /// **两个文件都不在 `K-R95` 的写区**。⇒ 交回里作 `〔R95b〕` 报给 PM。
    ///
    /// 那句话本身**已经只有一处**了：`src/generated/launch-render-facts.ts` 的
    /// `CLI_REFUSAL_REASON.probeUnknown`（源在本文件的生成器）。缺的是**线**，不是措辞。
    pub caps: Option<Vec<String>>,
    pub action: WireAction,
    pub container: WireContainer,
    pub cwd: Option<String>,
    pub account: WireAccount,
    pub ccm_sid: Option<String>,
    pub model: Option<String>,
    /// 已 sanitize 的 launcher（sanitize 仍在 TS，见 `super::payload` 头注）。
    pub launcher: String,
    pub default_launcher: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum WireAction {
    New,
    Resume { sid: String },
    Attach { name: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum WireContainer {
    None,
    Tmux { name: String, send_into: bool },
}

/// 🔴 **它比 [`CliAccount`] 少一态，那不是漏，是边界**〔`K-R89` 09-13〕。
///
/// `CliAccount` 09-13 起有第三态 `Inherit`（省略 `--account`，语义由
/// `DECISIONS.md#R28` 定、由 `src/backend/control/ccm/plan.rs::resolve_account`
/// 落地）。**本 wire 刻意没有对应变体** ——
/// 这条 IPC **只有远端会走**（前端的闸是 `ctx.transport.kind === "ssh"`），
/// 而远端是 ssh 过去，**那台机器上的继承态不是 monitor 的环境**（`R28` 裁定四逐字：
/// 「远端的继承怎么表达是另一回事，**不算已解**」）。
///
/// ⇒ 要给远端加这一态，落点是 `K-R90`，同拍要动的至少有三样：本枚举 ·
/// `src/launch-cli-wire.ts`（TS 那份手写镜像，由 `launch-cli-wire.vitest.ts` 的
/// 「字段集相等」钉着）· `remote-launch-run.ts::buildCliRenderRequest`（真正填它的地方）。
/// **别在这里顺手加一个变体就当远端也通了。**
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum WireAccount {
    Base,
    /// `name: None` = 只有 configDir 没有名字 ⇒ 说不出 `--account` ⇒ §35 短路。
    Account {
        name: Option<String>,
    },
}

/// 与 TS `CliRenderResult` 同构：`ok:true` 带命令，`ok:false` 带**降级理由**。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliRenderResponse {
    pub ok: bool,
    pub cmd: Option<String>,
    pub reason: Option<String>,
}

#[tauri::command]
pub fn render_ccm_launch(req: CliRenderRequest) -> CliRenderResponse {
    let caps: BTreeSet<String> = req.caps.clone().unwrap_or_default().into_iter().collect();
    let installed = req.caps.is_some();
    let action = match &req.action {
        WireAction::New => Action::New,
        WireAction::Resume { sid } => Action::Resume { sid },
        WireAction::Attach { name } => Action::Attach { name },
    };
    let container = match &req.container {
        WireContainer::None => Container::None,
        WireContainer::Tmux { name, send_into } => Container::Tmux {
            name,
            send_into: *send_into,
        },
    };
    let account = match &req.account {
        WireAccount::Base => CliAccount::Base,
        WireAccount::Account { name } => CliAccount::Named {
            name: name.as_deref(),
        },
    };
    let spec = CliSpec {
        is_ssh: req.is_ssh,
        // ★★ **P3t-Y3：这条上线路恒为 `false`，而这是路由事实，不是平台判断。**
        //
        // 本条 IPC **只有远端会走**：前端的闸是 `ctx.transport.kind === "ssh"`
        //（`remote-launch-run.ts::renderLaunchCommand`），POSIX 本机那条路住在 Rust 里
        //（`history.rs::render_local_ccm` 直接调渲染器），**不必绕一圈 IPC 问自己**。
        // ⇒ 这里没有「本机是什么平台」这个问题要答。
        //
        // Y1 原本在这里读一个进程内全局量（`host_facts::local_is_posix()`），**那是错的**：
        // 它让**夹具对拍变成环境依赖** —— 金串里那条 `isSsh:false` 的用例之所以绿，
        // 靠的是「单元测试进程从不跑 `lib.rs` 的启动段，所以全局量恰好是 `false`」。
        // 谁要是在同进程里先设了一次 `true`，那条对拍就翻，而翻的原因与被测的事毫无关系。
        // 本会话第三次撞上同族干扰（前两次：`<local>` 注册键、`host_facts` 自己）。
        //
        // 写死 `false` 是**fail-closed**：将来真要让本机走这条 IPC，它会先拒、
        // 而不是悄悄按某个没人设过的全局量放行。
        local_posix: false,
        action,
        container,
        cwd: req.cwd.as_deref(),
        account,
        ccm_sid: req.ccm_sid.as_deref(),
        model: req.model.as_deref(),
        launcher: &req.launcher,
        default_launcher: &req.default_launcher,
        args: &[],
        ccm_path: "ccm",
    };
    match render_ccm_invocation(&spec, &caps, installed) {
        Ok(cmd) => CliRenderResponse {
            ok: true,
            cmd: Some(cmd),
            reason: None,
        },
        Err(r) => CliRenderResponse {
            ok: false,
            cmd: None,
            reason: Some(r.reason()),
        },
    }
}

/// U8a-2c-pre / S28：**兜底那支的 `container:"none"` 形态**改由 Rust 渲染载荷。
///
/// # 只有 none 那一格
///
/// `renderFallback` 分两格：`container:"none"` 是 `env → cd → argv`（就是
/// [`super::payload::render_payload`]）；`container:"tmux"` 还要外层 tmux 命令
/// （`session-backend.ts`）——那半归 U8c-3。⚠ §33b 那三问**今天不是三问全未答**
/// （`K-R105` 09-13：③ 已退役、① 变过一次）；今天版逐问住 §33b 那张表，别在这儿复述。
///
/// ⇒ 本命令**只收 none 那一格**。容器形态由调用方判断后决定调不调它。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PayloadRenderRequest {
    /// 有序的 env 操作（与 TS `LaunchPlan.env` 同构）。
    pub env: Vec<WireEnvOp>,
    pub cwd: Option<String>,
    /// 已 sanitize 的 launcher。
    pub launcher: String,
    pub args: Vec<String>,
    /// 嵌套 env 键表（TS `AGENT_PROFILE.nestedEnvVars`）—— `unset-nested-env` 用。
    pub nested_env: Vec<String>,
    /// `( <prelude>; exec <inner> )` 包裹（§39 给 F04 rbind 留的槽）。
    ///
    /// ⚠ **这个字段是复盘补的。** 初版 wire 里根本没有它，`render_launch_payload` 硬写
    /// `wrap: &[]` ⇒ **静默丢**。两个审计各自独立点名（「内核为未来功能建好了，wire 却把它
    /// 挡在门外 —— 将来接上时不会有任何东西红」），而**新的生产命令对拍第一次跑就红了**：
    /// 夹具里那条 wrap 折叠用例的 TS 产物带包裹、Rust 产物没有。
    /// 今天 `plan.wrap` 恒空所以无生产影响；补上之后那条用例才真的在验生产路径。
    #[serde(default)]
    pub wrap: Vec<WireWrap>,
}

/// 与 TS `WrapSpec` 同构（`id` 只用于 TS 侧排错，不参与渲染）。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireWrap {
    pub order: i64,
    pub prelude: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum WireEnvOp {
    ExportConfigDir { value: String },
    ExportModel { value: String },
    UnsetConfigDir,
    UnsetNestedEnv,
}

#[tauri::command]
pub fn render_launch_payload(req: PayloadRenderRequest) -> Result<String, String> {
    let nested: Vec<&str> = req.nested_env.iter().map(String::as_str).collect();
    let env: Vec<super::payload::EnvOp> = req
        .env
        .iter()
        .map(|op| match op {
            WireEnvOp::ExportConfigDir { value } => {
                super::payload::EnvOp::ExportConfigDir { value }
            }
            WireEnvOp::ExportModel { value } => super::payload::EnvOp::ExportModel { value },
            WireEnvOp::UnsetConfigDir => super::payload::EnvOp::UnsetConfigDir,
            WireEnvOp::UnsetNestedEnv => super::payload::EnvOp::UnsetNestedEnv { keys: &nested },
        })
        .collect();
    let args: Vec<&str> = req.args.iter().map(String::as_str).collect();
    let wrap: Vec<super::payload::WrapSpec> = req
        .wrap
        .iter()
        .map(|w| super::payload::WrapSpec {
            order: w.order,
            prelude: &w.prelude,
        })
        .collect();
    super::payload::render_payload(&super::payload::PayloadSpec {
        env: &env,
        cwd: req.cwd.as_deref(),
        launcher: &req.launcher,
        args: &args,
        wrap: &wrap,
    })
}

#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/launch_wire_f07_main_path_tests.rs"]
mod f07_main_path_tests;

// ────────────────────────────────────────────────────────────────────────────
// `K-R95`：**前端不再自己写一份「要跑什么」** —— 生成物 ＋ 它的判据。
// ────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/launch_wire_k_r95_launch_render_facts.rs"]
mod k_r95_launch_render_facts;
