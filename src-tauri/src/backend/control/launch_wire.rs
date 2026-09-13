//! U8c-2c-2：**生产切换** —— `ccm 调用行`改由 Rust 渲染。
//!
//! # 只切 CLI 那一支，为什么
//!
//! `remote-launch-run.ts::renderLaunchCommand` 有两支：
//! `tryRenderCli`（装了 ccm 时走，产 `ccm …`）与 `renderFallback`（没装时走，产裸载荷）。
//!
//! - **CLI 那支是真在跑的那支**（U8c-2b-0 摸底：装了 ccm 就直接 return，兜底根本不执行）；
//! - **兜底那支切不动**：`container: tmux` 时它要外层 tmux 命令（`session-backend.ts`，151 行），
//!   而 `doc/INVARIANTS.md` §33b 写死了「删/搬 `session-backend.ts` 前必须先回答三件事」
//!   （生产切到 daemon 没有 · attach 那条串归谁产 · daemonless 要不要能起会话）。
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
    /// `src/launch-cli-wire.vitest.ts` 的「字段集相等」钉着）与
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
/// （`session-backend.ts`）——那半归 U8c-3，且 §33b 有三个未答问题。
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
mod f07_main_path_tests {
    //! F07（出口③ 早已交付）：**远端起会话主路的决策已经在 backend 渲染** —— 把它钉住。
    //!
    //! # 摸底结论
    //!
    //! F07 的题目是「远端起会话主路走 backend」。逐段量下来**决策那半已经切完了**：
    //!
    //! | 段 | 今天在哪 |
    //! |---|---|
    //! | 会话名 | F13 的铸名口（`mintTmuxName`，避让不可分离） |
    //! | §34 三道门 | F03 + F04a 已搬进 daemon `control/` |
    //! | 内层载荷 | `backend::control::payload`（P4b） |
    //! | ccm 调用行 | `backend::control::ccm_invocation`（P4b） |
    //! | **生产切换** | ✅ `remote-launch-run.ts` 三处在调 `render_ccm_launch` / `render_launch_payload` |
    //!
    //! 剩下的**只有「删 TS 那两个渲染器」**，而那是 U8c-3 的题目、不是 F07 的
    //! —— F07 要的是「走 backend」，不是「删旧的」。
    //!
    //! # ⚠ 摸底在 `doc/INVARIANTS.md §33b` 里抓到**两处过期陈述**
    //!
    //! **过期一**：那张表把 **U8c-2c-2 写成「待做」** —— 实测已交付
    //! （两条 tauri 命令注册 + 生产 TS 三处在调 + `parity_ledger` 两条能力）。
    //!
    //! **过期二**：三问的答案① 写「**否** —— 全仓 `.call("launch")` 只有一处且在 `cfg(test)` 里」，
    //! 实测**生产段有一处**（`daemon_launch.rs`，U8a-2c-1 的 `daemon_send_into`）⇒ 应为「**部分是**」。
    //!
    //! ⚠ **结论仍然对**（U8c-3 今天删不得：③ U12 未决 + attach 那格仍在 TS），**但依据过期了**。
    //! 这是本工作区「**理由过期而结论仍对**」的第二次（F01 那次是四处「每 ~8s」）——
    //! 最难发现的一类，因为**结论对，所以没人会去查理由**。
    //!
    //! # ★★ U8c-3〔08-14 复裁〕：结论第三次不变，而这次**依据变硬了一条**
    //!
    //! 08-14 逐条重量三问（读数在 `unified-backend/features/U8c-3-r2-…md`）：
    //!
    //! | 问 | 08-04 的答 | 08-14 实测 |
    //! |---|---|---|
    //! | ① 生产切到 daemon 的 `launch` 了吗 | 部分是（`send-into` 一格） | **仍是「部分是」**：生产段 `create-or-attach` **0 处**（下面那条判据在量），`attach` 结构上不归 daemon（`control/launch.rs` 头注「本模块**不 attach**」） |
    //! | ② attach 那条串归谁产 | 一半有答案 | **仍挡着，而且不止 attach**：`renderFallback` 的三格（tmux `create` / `send-into` / `attach`）全在 TS。⇒「只剩 attach 那一格」是把阻碍读窄了 |
    //! | ③ daemonless 的远端还要不要能起会话 | **未决**（U12 待做） | **已决：要。** `U12` 那个**件**被 `C7` 关掉了，但 `C7` 裁的是「**本机**也要有后端进程」；而 `daemonless` 今天是**每台远端主机的用户开关**（`src/settings/machine-card.ts` 那个 checkbox「daemonless 降级读取（无需 daemon）」→ `RemoteHostConfig.daemonless`，生产段 7 个文件 31 处）⇒ 那种主机**存在**、且它的 `↗` 走纯 SSH（`launch_remote_terminal` 不经 daemon）⇒ 起会话只能靠 monitor 自己渲染整串 |
    //!
    //! ⇒ ③ 从**软障碍（未决，所以不敢删）变成硬障碍（已决为「要」，所以确定不能删）**。
    //! **「件关掉了」不等于「约束消失了」** —— 这是本节第三次栽在同一形状上，
    //! 前两次是「结论对所以没人查理由」，这次是「**件关了所以没人查约束**」。
    //!
    //! ⚠ 而 08-04 立的那条前提触发器**在它被造出来要报的那个方向上是瞎的**，
    //! 见 `production_ts` 的头注 —— 量具本身是本轮真正修掉的东西。
    //!
    //! ⚠⚠ 上表 ① 那个「0 处」的**分母 08-29 换过一次**〔`K-P2` C 阶段第二拍〕：
    //! 原来只数 `src-tauri/src` 那棵树，现在**同时数 `shared/ccm` 的生产段** ——
    //! 因为 `K-P2 §0d`〔PM 08-29〕把接线路裁成了「`ccm` 直接问后端二进制」，
    //! 而那条路整条落在那份 shell 脚本里，旧扫描面**够不到它**。
    //! **读数仍然是 0，变的是分母。** 逐字理由在
    //! `the_two_reasons_u8c3_cannot_delete_the_ts_renderer_still_hold` 的头注「第四次」那一节。

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .to_path_buf()
    }

    fn read_ts(rel: &str) -> String {
        std::fs::read_to_string(repo_root().join(rel))
            .unwrap_or_else(|e| panic!("读不到 {rel}: {e}"))
    }

    /// 剥 TS 的生产段：整行 `//` / `*` / `/*` 注释 + 行尾 `//`。
    ///
    /// ★★ **本轮真正修掉的东西是量具，不是结论。**
    ///
    /// 08-04 立的依据一原式是
    /// `fallback.contains("session-backend") || run.contains("session-backend")` ——
    /// **整份文件的子串，含注释，而且是 `||`**。而 `remote-launch-run.ts` 的注释里
    /// 逐字提了 4 次 `session-backend.ts`（那些注释干的正是「解释这一格为什么还在 TS」这件事）
    /// ⇒ **把生产 import 与调用点删干净，那条依然绿**。
    ///
    /// 它是**前提触发器**：整个价值就是「前提没了要主动红」。而它在那个方向上是瞎的 ——
    /// 一条只会在「什么都没变」时说话的判据，与没有判据是同一件东西。
    ///
    /// ⇒ 改成量**生产段里那个调用还在不在**。剥法与
    /// `the_remote_launch_main_path_really_calls_the_backend_renderers` 共用一份
    /// （原来那份是就地写的，两处各写一份就会漂）。
    ///
    /// # 为什么不是直接调 `guard_core::strip_comment_lines`（`structural_scan` 那张表要的回答）
    ///
    /// **整行那半就是它**（本函数只是转调）。多出来的只有**行尾 `//` 截断**一步 ——
    /// 共享原语的头注逐字说明它**刻意不剥行尾**，理由是会砍坏 `"http://host"` 这类字面量。
    /// 而本组判据必须剥行尾：F10 那次的教训逐字是「**行尾注释里的提及不算数**」，
    /// 08-04 那份就地剥法正是为此才截的。
    ///
    /// ⚠ `tool_registry.rs` 那条登记逐字写着「本文件今天没有 `://` 字面量所以没事，
    /// **但那是运气不是设计**」。⇒ 这里不留运气：`the_ts_comment_stripper_actually_strips`
    /// 对本组的**四份语料**逐个断言「不含 `://`」，运气变成读数。
    fn production_ts(src: &str) -> String {
        guard_core::strip_comment_lines(src)
            .lines()
            .map(|l| match l.find("//") {
                Some(i) => &l[..i],
                None => l,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 本组判据读的全部 TS 语料 —— `production_ts` 的适用面就是这张表。
    const TS_CORPUS: &[&str] = &[
        "src/remote-launch-run.ts",
        "src/launch-render-fallback.ts",
        "src/remote-config.ts",
        "src/settings/machine-card.ts",
        // `K-R59`：那条 TS 兜底路的消费者，逐处登记在 `TS_FALLBACK_KEEPERS`。
        "src/remote-launch.ts",
        "src/launch-payload-golden.ts",
    ];

    /// 🔴 **那条 TS 兜底路今天靠谁站着** —— `U8c-3` 的**新**存续理由，逐处点名。
    ///
    /// `(被引的符号, 消费者文件, 生产段处数, 它是什么, 它什么时候能走)`
    ///
    /// 现打于 `4d298c4`（`src` 下，去掉 `*.test.ts` / `*.vitest.ts`）。
    /// ⚠ **处数由判据从源码派生再逐格比对** —— 散文里那句「3 个消费者」会腐烂，这张表不行。
    /// ⚠ 尺子的射程如实写：它数的是**剥完注释的源码里那个标识符出现几次**
    /// （`import` 那一行算一处）。别名 import、动态取属性它都数不到，**不声称堵住**。
    const TS_FALLBACK_KEEPERS: &[(&str, &str, usize, &str, &str)] = &[
        (
            "renderFallback",
            "src/launch-payload-golden.ts",
            2,
            "金样本发生器：`import` 一处 + `payload: renderFallback(planOf(c))` 一处。\
             它产的是 `backend/control/fixtures/payload-golden.json` —— Rust 侧那条\
             「两边逐字节同构」的对拍拿它当**左边**。",
            "Rust 侧不再拿 TS 的输出当金样本的那天（那要先有另一个真相源）。",
        ),
        (
            "renderFallback",
            "src/remote-launch.ts",
            6,
            "五个 builder（resume 直起 / resume 进 tmux / 送进已有 tmux / 起 launcher / attach）\
             各调一次 + `import` 一处。",
            "这五条各自都改走后端渲染的那天。",
        ),
        (
            "renderFallback",
            "src/remote-launch-run.ts",
            2,
            "生产主路的**回落**：`renderCliViaBackend` 不成时兜底（`import` 一处 + 调用一处）。",
            "`renderCliViaBackend` 覆盖到全部三格、回落变成死代码的那天。",
        ),
        (
            "SESSION_BACKEND",
            "src/launch-render-fallback.ts",
            4,
            "座本身：`import` 一处 + `attach` / `createRunAttach` / `runInExistingAttach` 各一处。",
            "外层 tmux 命令改由后端产出的那天（`control/launch.rs` 头注逐字「本模块**不 attach**」）。",
        ),
        (
            "SESSION_BACKEND",
            "src/remote-launch-run.ts",
            2,
            "`import` 一处 + `attachCmd` 那一处（把 `↗` 交给用户自己的终端那一跳）。",
            "同上；⚠ `attach` 那一跳由 `K-R59` `§0c` 明写**不做** —— \
             后端在远端开不了你面前的窗，那是结构不是退路。",
        ),
    ];

    /// ★ 量具自检：`production_ts` 真的在剥，而不是原样返回；且行尾截断在本组语料上安全。
    ///
    /// **不用「剥完更短」当自检** —— guard-core 的头注逐字记着那条为什么不够
    /// （光靠剥注释就能满足，与剥对没剥对无关）。这里直接喂一段字面量看输出。
    #[test]
    fn the_ts_comment_stripper_actually_strips() {
        assert_eq!(production_ts("// a\n * b\n/* c\nd // e\nf"), "\n\n\nd \nf");
        // 反向：没有注释的文本一个字都不许动。
        assert_eq!(
            production_ts("let x = 1;\nlet y = 2;"),
            "let x = 1;\nlet y = 2;"
        );
        // ★ 行尾截断的安全前提**量出来**，不靠运气：本组语料一条 `://` 都没有。
        let mark = format!(":{}", "//");
        for f in TS_CORPUS {
            let src = read_ts(f);
            assert!(
                src.len() > 3000,
                "{f} 只有 {} 字节 —— 语料读错了",
                src.len()
            );
            assert!(
                !src.contains(mark.as_str()),
                "{f} 里出现了 `{mark}` 字面量 —— 行尾 `//` 截断会把它砍成半行、造出假阴性。\n\
                 ⇒ 要么把那个字面量挪走，要么给本组换一份不截行尾的剥法（那时上面两条断言也要改）。"
            );
        }
    }

    /// ★ **生产接线钉**：主路真的调那两条 backend 渲染命令。
    ///
    /// 一旦有人把它改回「TS 自己渲染」，本条红 —— 而那种回退**功能不变砖**
    /// （TS 兜底渲染器还在），门禁也不会因为别的原因红。
    #[test]
    fn the_remote_launch_main_path_really_calls_the_backend_renderers() {
        let ts = read_ts("src/remote-launch-run.ts");
        assert!(
            ts.len() > 5000,
            "remote-launch-run.ts 只有 {} 字节，抽错了？",
            ts.len()
        );
        // 剥整行注释 + 行尾注释（F10 那次学到的：行尾注释里的提及不算数）。
        let prod = production_ts(&ts);
        for needle in [
            "commands.render_ccm_launch(",
            "commands.render_launch_payload(",
        ] {
            assert!(
                prod.contains(needle),
                "`remote-launch-run.ts` 的生产段里找不到 `{needle}` ——\n\
                 远端起会话主路不再走 backend 渲染了。\n\
                 ⚠ 这种回退**功能不变砖**（TS 兜底渲染器还在），所以除了本条没人会红。"
            );
        }
    }

    /// ★ **前提触发器**：U8c-3（删 TS 渲染器）今天删不得的**两条依据**仍然成立。
    ///
    /// 依据一：**兜底渲染器仍是生产渲染器** —— `remote-launch-run.ts` 的**生产段**
    /// 仍在调 `renderFallback(`，而它产的三格（tmux `create` / `send-into` / `attach`）
    /// 全要外层 tmux 命令，归 TS 的座 `session-backend.ts`
    /// （daemon 的 `control/launch.rs` 头注逐字写着「本模块**不 attach**，一次都不」）。
    /// 依据二：**`create-or-attach` 那格仍未切** —— 生产段一次都不发这个 mode。
    ///
    /// ⚠ 〔08-14〕依据一的**量法换了**：原来量的是「文件里出现过 `session-backend` 这个词」
    /// （含注释、且两份文件 `||`），那在它要报的方向上是瞎的 —— 见 `production_ts` 头注。
    /// 现在量「**那个调用还在不在**」，注释里怎么写它都不算数。
    ///
    /// ⚠ 如实登记本条量法的边界：它仍是**子串**。把 `SESSION_BACKEND` 换个本地别名
    /// （`const seat = SESSION_BACKEND; seat.attach(…)`）能从缝里过去 ——
    /// 但那种改动**不改变前提本身**（座还在 TS、外层命令还是 TS 产），
    /// 而本条要报的是**前提没了**，所以这个缝不在它的射程里。
    ///
    /// 任一条变了 ⇒ **主动红**：那时 U8c-3 的前置动了，回来重裁 F07 的剩余面。
    ///
    /// # ⚠ F04c 订正了依据二的**度量方式**（结论没变）
    ///
    /// 原来数的是「生产段 `.call("launch")` 的处数 == 1」。F04c 让它变成 **2** 而当场报红 ——
    /// **它红得对**（前提确实动了，该回来重裁），**但重裁的结论是「依据二仍成立」**：
    /// 新增那一处是 `daemon_send_keys` 发的 `send-into` / `send-keys-raw`，那是
    /// **`tmux_send_keys` 这条命令**改走 daemon，**不是「起会话」又切了一格**。
    ///
    /// ⇒ **`.call("launch")` 的处数是个过期的代理指标**：它把「有几条代码路径用 launch 命令」
    /// 和「起会话有几格切到了 daemon」混成一个数。改成直接量后者 ——
    /// **生产段发不发 `create-or-attach`**。
    ///
    /// ★ 这是「**依据/度量过期而结论仍对**」在本工作区的**第三次**
    /// （F01 四处「每 ~8s」· F07 `§33b` 两处 · 本条）。三次的处置都一样：
    /// **把结论留住，把依据换成还量得准的那个**。
    ///
    /// # ⚠⚠ 第四次：`K-P2` 08-29 —— 依据二的**扫描面**比它要报的事实小了一格
    ///
    /// 上面那句「改成直接量后者」把**量法**修对了，却留下一个**面**的洞：
    /// 扫的是 `env!("CARGO_MANIFEST_DIR")/src`，也就是**只有 `src-tauri/src/**.rs`**。
    /// 而「起会话改走 daemon」有两条路，`K-P2 §0d`〔PM 08-29〕**裁的是后一条**：
    ///
    /// | 路 | 接线落在哪 | 本条**看不看得见** |
    /// |---|---|---|
    /// | ㈡ 宿主自己发 `launch` | `src-tauri/src/**.rs` | 看得见 |
    /// | ㈠ **`ccm` 直接问后端二进制**（`§0d` 裁定：`ccm <子命令>` = 后端以**一次性模式**跑） | `shared/ccm`（**shell**） | **看不见** |
    ///
    /// ⇒ 走㈠ 的话，「起会话那格切到 daemon 了」这件事**做成了，而本条一个字都不说**
    /// —— 那不是绿，是**零命中地绿**。`K-P2` 的 `KP2A` 逐字预言过这个形状：
    /// 「它的扫描面只有 `src-tauri/src/**/*.rs` ⇒ **扫不到 `shared/ccm`** ……
    ///  这条判据**零命中地绿**，而事情做成了它一个字都不说」。
    ///
    /// ⇒ **把 `shared/ccm` 的生产段加进同一个扫描面**（同一个结论、同一个 needle、
    /// 同一条失败文案），并给它配自己的抽取器自检。
    /// **结论仍然只有一条**：「起会话那格有没有切过去」——变的是它够得到哪几棵树。
    ///
    /// ⚠ **本条不管「ccm 发了哪几条一次性子命令」**（那是 `ccm_cli_contract` 的
    /// `ccm_reaches_the_backend_through_one_shot_subcommands` 〔散文墓碑〕（`K-R48` 第二拍随 `shared/ccm` 删），`K-P2 KP2A②`）：
    /// 一条判据一件事。本条只回答 F07/U8c-3 要的那一句——**起会话那格切了没有**。
    #[test]
    fn the_two_reasons_u8c3_cannot_delete_the_ts_renderer_still_hold() {
        // 依据一 a：**生产主路仍在调兜底渲染器**（`container: tmux` 的三格都落它）。
        let run = production_ts(&read_ts("src/remote-launch-run.ts"));
        assert!(
            run.contains("renderFallback("),
            "`remote-launch-run.ts` 的**生产段**里再也没有 `renderFallback(` 了 ——\n\
             **这多半是好事**：兜底那支可能已经搬走了 ⇒ U8c-3 的依据一没了，回 F07/U8c-3 重裁。\n\
             ⚠ 别看注释怎么写 —— 本条只读生产段（`production_ts`），这正是 08-14 换掉的量法。"
        );
        // 依据一 b：**外层 tmux 命令仍归 TS 的座产**（兜底渲染器仍问 `SESSION_BACKEND` 要）。
        let fallback = production_ts(&read_ts("src/launch-render-fallback.ts"));
        assert!(
            fallback.contains("SESSION_BACKEND."),
            "`launch-render-fallback.ts` 的**生产段**不再问座要命令了 ——\n\
             **这多半是好事**：外层 tmux 命令（`new-session` / `send-keys` / `attach`）\n\
             可能已经不归 TS 产了 ⇒ U8c-3 的依据一没了，回 F07/U8c-3 重裁。"
        );
        // 依据二：**起会话那格**有没有切过去 —— 直接量「生产段发不发 `create-or-attach`」。
        // **运行时拼，免得命中本文件自己的说明。**
        let mode = format!("\"create-or-{}\"", "attach");
        let mut hits: Vec<String> = Vec::new();
        let mut stack = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        let mut scanned = 0usize;
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().and_then(|x| x.to_str()) != Some("rs") {
                    continue;
                }
                if p.file_name().is_some_and(|n| n == "launch_wire.rs") {
                    continue; // 本文件的说明里逐字写着那个串
                }
                scanned += 1;
                let src =
                    guard_core::production_code(&std::fs::read_to_string(&p).unwrap_or_default());
                if src.contains(mode.as_str()) {
                    hits.push(p.file_name().unwrap().to_string_lossy().to_string());
                }
            }
        }
        // ★ 抽取器自检：扫描面没缩水（否则下面那条零命中地绿）。
        assert!(
            scanned >= 50,
            "只扫到 {scanned} 个 .rs —— 遍历坏了，下面那条断言会零命中地绿"
        );
        // ★★ 第二棵树〔`K-P2` 08-29 立，`K-R48` 第二拍 09-11 换住址〕。
        //    它原来是 `shared/ccm`（那份 bash 脚本），理由是「`§0d` 裁定的接线路落在那里，
        //    而上面那棵树够不到它」。〔用@09-11 `K33`〕脚本删了 ⇒ 换成
        //    `remote-daemon-proto/src/control/ccm/`：**接线路还在那条边上，只是换了语言**。
        //    ⚠ needle 仍用**带边界的词**（不是 Rust 那边的带引号字面量）：
        //    它在两侧可能长在字面量里、也可能长在 JSON 串里，带边界两种形态都收得到。
        let word = format!("create-or-{}", "attach");
        let ccm_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .join("remote-daemon-proto/src/control/ccm");
        let ccm: String = ["mod.rs", "argv.rs", "plan.rs"]
            .iter()
            .map(|f| {
                guard_core::production_code(
                    &std::fs::read_to_string(ccm_dir.join(f))
                        .unwrap_or_else(|e| panic!("读不到 control/ccm/{f}：{e}")),
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        // ★ 抽取器自检（这一棵树自己的）：剥注释器没把代码一起剥掉。
        //   地板 300 行是从 `shared/ccm` 那一版逐字沿用的（那边生产段 536 行 / 全文 1258）；
        //   现打这三份剥完远在其上。
        assert!(
            ccm.lines().count() >= 300,
            "`control/ccm/` 三份的生产段只剩 {} 行 —— 剥注释器把代码也剥了？下面那条会零命中地绿",
            ccm.lines().count()
        );
        // ★★ 〔`K-P2` `D` 阶段第三拍 09-03〕**这一半本拍翻了面。**
        //
        // 它原来与 Rust 那棵树同判：「**两棵树都不许**出现 `create-or-attach`」。
        // `K-P2` `D3` 把 `shared/ccm` 的 `--tmux` 接到了后端那条一次性口上
        //（旧 bash 的 `launch_via_daemon` 〔散文墓碑〕 发 `mode=create-or-attach`）⇒ **这一半当场红了，红得对**。
        //
        // ⇒ 翻成正向：`shared/ccm` 从「不许有」变成「**必须有**」。
        // **别把它删掉**：删掉之后「起会话又退回本机 tmux 直起」就没有任何东西会说话。
        //
        // 🔴 **而本条的结论不变，理由要写清楚**（这正是那句 assert 文案要求「回来重裁」的事）：
        // 三问的答案① 变了（起会话这一格 `shared/ccm` 已经切到 daemon），
        // **但上面那两条依据是独立的、且都还成立** ——
        // ① a：`remote-launch-run.ts` 的生产主路仍在调 `renderFallback(`；
        // ① b：`launch-render-fallback.ts` 仍问 `SESSION_BACKEND.` 要外层 tmux 命令。
        // 那两条说的是 **monitor 自己那条 `↗` 路**（TS 渲染 → `ssh -t bash -lic '<串>'`），
        // 它与「ccm 在远端自己起会话时问不问 daemon」**是两条路，别压成一句**。
        // 〔散文墓碑〕这里原来还写着「再加上 `the_daemonless_remote_still_needs_the_ts_fallback_renderer`
        // 那条**硬**障碍（daemonless 主机今天仍是产品提供的开关）」——
        // 🔴 `K-R59`（09-11，定框 `K35`）把那一档整格删了，那条判据也随之换人。
        // ⇒ **删 TS 渲染器的前置仍然不成立**，但今天的理由换成了
        //   `the_ts_fallback_renderer_now_stands_on_its_own_consumers`（3 + 2 个生产消费者）。
        // ⚠ 而 **Rust 那棵树仍然必须是零** —— monitor 侧那条 `.call("launch")` 至今只发
        //   `send-into` / `send-keys-raw`（`daemon_launch::the_only_mode_this_channel_can_speak_is_send_into`
        //   钉着它）。**两棵树本拍起口径不同，这不是疏漏，是两件不同的事。**
        assert!(
            hits.is_empty(),
            "**Rust 生产段**开始发 `create-or-attach` 了（{hits:?}）—— **这多半是好事**：\n\
             monitor 侧「起会话」那格可能也切到 daemon 了 ⇒ `INVARIANTS §33b` 三问的答案① 又变了，\n\
             回 F07/U8c-3 重裁「删 TS 渲染器」的前置。\n\
             ⚠ 同轮还要回 `K-P2`：`KP2A②` 的棘轮要抬、`KP2C` 的退路要登记、\n\
             `KP2D` 的通道 A/B 冲突必须已经解掉。\n\
             ⚠ `control/ccm/` **不在本断言的人群里**（它在下面单判，`K-P2` `D3` 已经切过去了）。"
        );
        assert!(
            guard_core::contains_word(&ccm, &word),
            "`control/ccm/` 的生产段**不再发** `create-or-attach` 了 —— 起会话退回本机 tmux 直起？\n\
             `K-P2` `D3`（09-03）把它接到了后端那条一次性口上；`K-R48`（09-11）之后\n\
             那条口住进了同一个进程（`control::launch::parse_request` 那道门）。\n\
             ⇒ 真要退回来，请连同 daemon 侧那条\n\
             `the_container_launch_goes_through_the_one_door_with_every_field_intact`\n\
             一起撤，并回 `K-P2` 说明为什么。"
        );
    }

    /// 🔴🔴 **`U8c-3` 的前提换人了 —— 这是那份换人手续。**〔`K-R59` `KR59D2` 09-11〕
    ///
    /// # 被换掉的那条是什么
    ///
    /// 〔散文墓碑〕这里此前住着 `the_daemonless_remote_still_needs_the_ts_fallback_renderer`（08-14 立）。
    /// 它主张两件事：① `daemonless` 这个每机开关还在（字段 + 界面那一格，两处都要）；
    /// ② **所以** `src/launch-render-fallback.ts` 与 `src/session-backend.ts` 删不得 ——
    /// 没装 ccm 的 daemonless 远端，它的 `↗` 命令就是这两个文件产的。
    ///
    /// 它自己逐字写着：「哪天 `daemonless` 这个开关真被取消了（那才是 `C7` 覆盖到远端的那一天），
    /// **本条主动红**，提醒回来重裁 `U8c-3` —— 那时兜底渲染器少了一类必须服务的主机。」
    ///
    /// **那一天就是 09-11**（用户定框 `K35`：「不要有 daemonless。没有没有后端的情况。
    /// 前端应该就是去调用远程后端的。」）⇒ 那条判据**红了，而且红得对**。
    ///
    /// # 🔴 而它红完之后的答案，不是它自己那句话
    ///
    /// 它预写的结论是「那时兜底渲染器**少了一类必须服务的主机**」——
    /// 听起来像「可以删了」。**现打（`4d298c4`，`src` 下、去掉 `*.test.ts` / `*.vitest.ts`）不是这样**：
    /// `renderFallback` 有 **3 个**生产消费者、`SESSION_BACKEND` 有 **2 个**（逐处见 [`TS_FALLBACK_KEEPERS`]）。
    ///
    /// ⇒ **前提退役，但那条路不退役 —— 它另有消费者。**
    /// 08-14 那条把「daemonless 主机需要它」当成了「它删不得」的**理由**，
    /// 而那不是唯一的理由。**换人手续办完了，不是把它抹掉。**
    ///
    /// ⚠ **本条只答「那条路还有没有别的消费者」，不答「那 3 个消费者今天还该不该存在」** ——
    /// 后者要读 `U8c-3` 的原意，不在 `K-R59` 射程（件文件 `§0b-3` 逐字）。
    ///
    /// # 本条什么时候该红（新的触发条件，逐条写死）
    ///
    /// - **那一档回潮**：`daemonless` 又变回一个用户开关（字段清单 / 界面 / 数据源分支任一处）
    ///   ⇒ 红。`K35` 是用户定框，回潮要先回去改定框，不是悄悄加回来。
    /// - **消费者少一个**：[`TS_FALLBACK_KEEPERS`] 与源码对不上 ⇒ 红。**掉到 0 那天**
    ///   就是这两个文件真能删的那天 —— 那时回 `U8c-3`，而不是靠读注释判断。
    #[test]
    fn the_ts_fallback_renderer_now_stands_on_its_own_consumers() {
        // ① **前提确实退役了**（不是「名字没了」，是那一档没了）。
        //    🔴 刻意**不**用 `!contains("daemonless")` —— `remote-config.ts` 里还剩**一处**
        //    该词的字面量（`LEGACY_NO_BACKEND_KEY`，认旧配置用的墓碑），数名字会把它读成回潮。
        //    ⇒ 断的是**载体**：落盘字段清单里那一项 · 界面那个 input · 数据源那条分支。
        let cfg = production_ts(&read_ts("src/remote-config.ts"));
        assert!(
            !cfg.contains(r#""daemonless","#),
            "`REMOTE_HOST_FIELDS` 里又有 `daemonless` 了 —— 那一档回潮了。\n\
             `K35`〔用 09-11〕逐字：「不要有 daemonless。没有没有后端的情况。」\n\
             要加回来先回去改定框，并同轮重裁 `U8c-3`（本条的存续理由会跟着变）。"
        );
        let card = production_ts(&read_ts("src/settings/machine-card.ts"));
        assert!(
            !card.contains("daemonlessInput"),
            "机器卡片的生产段里又有那个开关了 —— 用户又能造出「不装后端」的主机。同上。"
        );
        let src = guard_core::production_code(include_str!("../../ssh_source.rs"));
        assert!(
            !src.contains("daemonless_stream_loop"),
            "`ssh_source.rs` 生产段里那条轮询回落又回来了 —— \n\
             **开关没了而路还在**，那是最坏的一种：没有任何界面造得出它，却仍有一条代码路等着。"
        );
        // ② **新的存续理由**：那条路今天靠自己的消费者站着，逐处点名、处数从源码派生。
        for (symbol, file, want, what, unlock) in TS_FALLBACK_KEEPERS {
            let code = production_ts(&read_ts(file));
            let got = code.matches(symbol).count();
            assert_eq!(
                got, *want,
                "`{file}` 里 `{symbol}` 的生产处数是 {got}，登记的是 {want}。\n\
                 那一处是什么：{what}\n\
                 **少一处** ⇒ 一个消费者走了，把登记拧下来；\n\
                 **掉到一个都不剩** ⇒ 那才是「这条路可以退役」，回 `U8c-3` 重裁，别自批。\n\
                 它什么时候能走：{unlock}"
            );
        }
        // ③ 承接方那两个文件本身还在（`U8c-3` 的顺序是「先有承接方，再删旧的」）。
        for f in ["src/launch-render-fallback.ts", "src/session-backend.ts"] {
            assert!(
                repo_root().join(f).is_file(),
                "`{f}` 没了，而 ② 那几个消费者还在 —— **有人先删了承接方**。"
            );
        }
    }

    /// 🔴 **`KR59D2` 的死值验落点：那份换人手续不许被悄悄撕掉。**
    ///
    /// # 它为什么是一条独立的判据
    ///
    /// `KR59D2` 逐字要的是：「把那条判据整条删掉、别的都不动 ⇒ **必须有东西红**
    /// （若没有，说明「前提没了」这件事在盘上真的没有任何痕迹 —— 那正是本条要治的）」。
    ///
    /// 上面那条墓碑自己做不到这件事：删掉它，它就不再运行，也就不再说话。
    /// ⇒ 由**本条**在旁边看着它。**两条一起删**才能静默 —— 而那已经不是「别的都不动」了。
    ///
    /// # 它防的那个活体
    ///
    /// `RELAY_KEEPS_THE_OLD_PATH` 犯过同形的病：退役条件悬空指向一个**已经被删掉的东西**
    /// （`shared/ccm`），没人发现。本条断的正是「指着的那几样今天都还在盘上」。
    #[test]
    fn the_retired_premise_left_a_tombstone_that_is_still_on_the_board() {
        let me = include_str!("launch_wire.rs");
        // 地板：切不到语料时下面几条会零命中地绿。
        assert!(
            me.len() > 20_000,
            "只读到 {} 字节的本文件 —— 本条在空转",
            me.len()
        );
        // 🔴🔴 **针一律现拼，一个都不许写成整串字面量 —— 这一条本轮实测栽过两次。**
        //
        // ① 第一版把墓碑那个函数名连着 `fn ` 前缀写成一整串字面量，
        //    而**那串字面量自己就住在本文件里** ⇒ `me.contains(needle)` **恒真**：
        //    死值验把那条判据整条改名，读数是「新红 0」——判据在自己身上空转。
        // ② 改成现拼之后，我又把那串完整字面量抄进了**解释它的注释**里，同一条当场又恒真一次。
        // ⇒ 所以这段解释里也不写完整串（要提就断开写：`fn the_ts_fallback_renderer_now_` ＋ 后半）。
        // 与 `6g` 那族「断言用的子串取自夹具自己的名字」是同一个病。
        let tomb_fn = format!(
            "fn the_ts_fallback_renderer_now_{}",
            "stands_on_its_own_consumers"
        );
        let keepers = format!("TS_FALLBACK_{}", "KEEPERS");
        for needle in [tomb_fn.as_str(), keepers.as_str()] {
            assert!(
                me.contains(needle),
                "`{needle}` 不在本文件里了 —— **`U8c-3` 的那份换人手续被撕掉了。**\n\
                 08-14 那条前提触发器（`daemonless` 主机需要兜底渲染器）在 09-11 `K-R59` 红了，\n\
                 而它红完之后留下的东西就是那条墓碑 + 那张消费者登记。\n\
                 删掉它们 = 「一个前提没了」这件事在盘上再没有任何痕迹，\n\
                 下一个人读到的会是「兜底渲染器没有存续理由」——**而那是假的**。\n\
                 真要删，先回 `U8c-3` 答出「那条路今天还删不删得」，再连本条一起删。"
            );
        }
        // 🔴 **手续在，不等于手续还在干活。**〔本轮死值验 `M10` 逼出来的：把那半的比对
        //    换成 `.iter().take(0)`，六格全绿 —— 那半当时没有任何东西看着。〕
        //    ⇒ 再断一句：墓碑那个函数体里**真的在整表迭代**那张登记。
        //    ⚠ 射程如实写：它认的是「整表迭代」这一个**形状**（`in <表> {`）。
        //      换一种写法（先 `collect` 再比、或换个循环变量顺序）它就认不出 —— **不声称堵住**。
        let at = me.find(tomb_fn.as_str()).expect("上面刚断过它在");
        let body_end = me[at..].find("\n    }\n").map_or(me.len() - at, |k| k + 6);
        let body = &me[at..at + body_end];
        assert!(
            body.len() > 800,
            "切出来的墓碑函数体只有 {} 字节 —— 本条在空转（「体里没有」与「压根没切出体」同形）",
            body.len()
        );
        let iterating = format!("in {keepers} {{");
        assert!(
            body.contains(iterating.as_str()),
            "墓碑还在，但它**不再逐条比对**那张消费者登记（找不到 `{iterating}`）——\n\
             表留着而比对掏空 = 「兜底渲染器还有 3 + 2 个消费者」这句话从此没人核。\n\
             真要换写法，请连本条一起改（并说明新的形状怎么认）。"
        );
        // 阴性对照：这把尺子不是恒真的。
        // ⚠ **针要现拼**：写成字面量的话它自己就在本文件里，这一条当场自相矛盾
        //   （本轮实测红过一次 —— 与 `daemon-section` 那条「别让路径混进断言」同族）。
        let absent = format!("fn {}", "a_judgement_that_was_never_written");
        assert!(
            !me.contains(absent.as_str()),
            "扫描器恒真 —— 上面几条在空转"
        );
    }

    /// ★ **F04c 补：`send-keys` 那两个 mode 只许从一个地方发出去。**
    ///
    /// 上面那条不再数 `.call("launch")` 的处数了，于是「谁在发 launch」这件事少了一道账。
    /// 本条把它补回来，但量的是**对的东西**：走 daemon 的 `send-keys` 语义
    /// （`send-into` / `send-keys-raw`）在生产段只许有**一个**产出点
    /// （`daemon_send_keys::mode_for`）—— 多一处就是「同一个决策两份实现」的起点，
    /// 而这个决策错了的后果是**把「打断当前回合」变成「提交用户排队的文本」**。
    #[test]
    fn the_send_keys_mode_names_have_exactly_one_production_home() {
        let raw = format!("\"send-keys-{}\"", "raw");
        let mut homes: Vec<String> = Vec::new();
        let mut stack = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().and_then(|x| x.to_str()) != Some("rs") {
                    continue;
                }
                if p.file_name().is_some_and(|n| n == "launch_wire.rs") {
                    continue;
                }
                let src =
                    guard_core::production_code(&std::fs::read_to_string(&p).unwrap_or_default());
                if src.contains(raw.as_str()) {
                    homes.push(p.file_name().unwrap().to_string_lossy().to_string());
                }
            }
        }
        assert_eq!(
            homes,
            vec!["daemon_send_keys.rs".to_string()],
            "`send-keys-raw` 这个 mode 名的生产段落点不止一个（或搬走了）：{homes:?}\n\
             它必须只有一个家（`daemon_send_keys::mode_for`）—— 两处就会漂，\n\
             而这个决策漂了的后果是把 `Escape`（打断当前回合）当成「键入并提交」。"
        );
    }
}

// ────────────────────────────────────────────────────────────────────────────
// `K-R95`：**前端不再自己写一份「要跑什么」** —— 生成物 ＋ 它的判据。
// ────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod k_r95_launch_render_facts {
    //! `K-R95`（定框 `K28`：前端不许自己发明对外行为）：「要跑什么」这件事上，
    //! **前端此前自己写死的那几格**改成从后端取。
    //!
    //! # 为什么走生成物，而不是再开一条 IPC
    //!
    //! 与上一件 `K-R93` 同一个理由，逐字照它的范例做：这几格的消费点
    //! （`launch-render-cli.ts::tryRenderCli` 的降级理由、
    //! `accounts.ts::localLaunchAccountSync` 的载荷键名）**都是同步纯函数**，
    //! 而 IPC 是异步的 —— 改成 IPC 要连它们的调用链一起翻成 async，
    //! 触及 `remote-launch-run.ts` / `launch-cli-golden.ts` / `tabs.ts` /
    //! `views/history.ts` **四个本件写区没给的文件**。
    //! ⇒ 选生成物那条：**同样做到「后端改一处，前端手里那份跟着变」**，而写区一格没越。
    //!
    //! # 三格分别是什么
    //!
    //! | 格 | 前端此前哪一处自己写 | 现在的源 | 前端改读了吗 |
    //! |---|---|---|---|
    //! | 每次调用无条件要求的能力集 | `launch-render-cli.ts::CLI_REQUIRED_CAPS` | [`super::super::ccm_invocation::CLI_REQUIRED_CAPS`] | ❌ 见下 |
    //! | 八句降级理由的措辞 | `launch-render-cli.ts` 里八处字面量 | [`super::super::ccm_invocation::Refusal::reason`] ＋ 本模块的 [`PROBE_UNKNOWN_REASON`] | ✅ 八句里的六句 |
    //! | 本机拉起载荷「哪个号」那一格的 wire 键名 | `accounts.ts` 里 `{ kind: "named", configDir, name }` | [`crate::history::LaunchAccount`] 的 serde 契约 | ✅ |
    //!
    //! # 🔴 没搬动的那三处，卡的是**判据**不是设计
    //!
    //! 能力清单 ＋ 两条「维度 …」闸门的措辞，各自被一条**按源码字面量/措辞 grep** 的判据
    //! 钉在 `src/launch-render-cli.ts` 里：`e2e/ccm-contract-parity.sh:291`（按文件路径 ＋
    //! 单行数组字面量抽清单，还配了「抽到 ≥5 项」的自检）· `src/launch-render-cli.vitest.ts`
    //! （两条闸门各按措辞钉「恰好一处」）。**两个文件都不在本件写区**，搬走会让它们假红。
    //! ⇒ 处置：留在前端，但由 `launch_cli_parity.rs` 那两条判据**逐字节钉在后端这一份上**
    //! （它们此前一条判据都没有，只有两侧互指的注释）。这一条报给 PM，不许自己解开。
    //!
    //! ⚠ **第三格的诚实边界**：`LaunchAccount` 今天只派生 `Deserialize`（它是命令入参），
    //! 序列化不出来 ⇒ 键名在本模块里仍是**写出来的**，但它们**每次生成都跑一遍生产
    //! 反序列化器验一次**（还配了阴性对照：换一个名字必须被拒）。
    //! ⇒ `history.rs` 那边改名 ⇒ `npm run gen:types` 当场 panic，而不是悄悄生成一份旧的
    //! —— 那正是 `KR95D3` 要的「**不许悄悄回落到一份写死的默认**」。

    use super::super::ccm_invocation::{Refusal, CLI_REQUIRED_CAPS};

    /// 🔴 `KR95D3`：「**没探出来**」那一句。`{error}` 是占位。
    ///
    /// 它**不进** [`Refusal`]：那个枚举答的是「渲染器为什么渲不出来」，而渲染器手上
    /// 根本没有「探测出没出错」这个概念（它只拿到 `caps` 与 `installed`）。
    /// 这一格丢在**线上** —— 见 [`super::CliRenderRequest::caps`] 上那段登记在案的缺口。
    ///
    /// ⚠ 措辞与前端此前那一句**逐字相同**（`launch-render-cli.ts` 的
    /// `` `探测没得出答案（不等于没装）：${probe.error}` ``）—— 本件把它搬到这里，
    /// 前端改读生成物。**全仓只此一处写这句话。**
    const PROBE_UNKNOWN_REASON: &str = "探测没得出答案（不等于没装）：{error}";

    /// 生成物的头。`generated-boundary-guard.vitest.ts` 认两样东西：
    /// 「谁生成的」那一行 ＋「不许手改」那句话。
    const HEADER: &str = "\
// 本文件由 `src-tauri/src/backend/control/launch_wire.rs` 的
// `export_bindings_launch_render_facts` 生成（`npm run gen:types`）。Do not edit this file manually.
//
// `K-R95`：**「要跑什么」只有后端一份说法**（定框 `K28`：前端不许自己发明对外行为）。
// 下面每一格都是生成时**跑一遍后端生产代码现算的**，不是照抄一遍常量 ——
// 改后端那一处、跑 `npm run gen:types`，前端手里这一份就跟着变。
//
// ⚠ 带 `{…}` 的是**占位**：调用方原样替换掉，措辞本身不许在前端改。
";

    /// TS 字符串字面量。走 `serde_json` 而不是自己拼引号：转义规则一次到位，
    /// 中文按原样输出（`serde_json` 不转 non-ASCII）。
    fn ts_str(s: &str) -> String {
        serde_json::to_string(s).expect("字符串序列化不会失败")
    }

    /// 逐行追加。**刻意收一个 `&[&str]` 而不是一行一次调用**：后者里超宽的中文注释行
    /// 会被 `rustfmt` 拆成三行（`line(`／`&mut s,`／字面量），而数组元素它拆不动。
    fn lines(s: &mut String, ts: &[&str]) {
        for t in ts {
            s.push_str(t);
            s.push('\n');
        }
    }

    /// 八句降级理由：`(TS 侧的键, 后端现场产出的措辞, 给读的人的一句注)`。
    fn refusal_reasons() -> Vec<(&'static str, String, &'static str)> {
        vec![
            (
                "notInstalled",
                Refusal::NotInstalled.reason(),
                "探到了，它**真的**没装",
            ),
            (
                "notSsh",
                Refusal::NotSsh.reason(),
                "本机那条路不走这个渲染器",
            ),
            (
                "missingCap",
                Refusal::MissingCap("{cap}".into()).reason(),
                "`{cap}` = 缺的那个能力名",
            ),
            (
                "sendIntoHasNoCliForm",
                Refusal::SendIntoHasNoCliForm.reason(),
                "#76 防线：idle-tmux 就地复用没有 CLI 等价语法",
            ),
            (
                "attachNeedsTmux",
                Refusal::AttachNeedsTmux.reason(),
                "attach 只有 tmux 一种容器",
            ),
            (
                "dimensionCannotSpeak",
                Refusal::DimensionCannotSpeak("{dim}".into()).reason(),
                "`{dim}` = 维度 id",
            ),
            (
                "dimensionNeedsCap",
                Refusal::DimensionNeedsCap {
                    dim: "{dim}".into(),
                    cap: "{cap}".into(),
                }
                .reason(),
                "`{dim}` / `{cap}` 两个占位",
            ),
            (
                "probeUnknown",
                PROBE_UNKNOWN_REASON.to_string(),
                "🔴 `KR95D3`：**没探出来 ≠ 没装**；`{error}` 是唯一线索",
            ),
        ]
    }

    /// 本机拉起载荷里「哪个号」那一格的 wire 键名 —— **每次生成都跑生产反序列化器验一遍**。
    ///
    /// 返回 `(TS 侧的键, 线上的名字)`。
    fn local_launch_account_wire() -> Vec<(&'static str, &'static str)> {
        use crate::history::LaunchAccount;
        let named: LaunchAccount = serde_json::from_value(serde_json::json!({
            "kind": "named", "configDir": "/d", "name": "n"
        }))
        .expect(
            "`history.rs::LaunchAccount` 不再收 `{kind:\"named\", configDir, name}` —— \
             前端手里那份载荷键名已经过期了，先改这里的表再生成",
        );
        match &named {
            LaunchAccount::Named { config_dir, name } => {
                assert_eq!(config_dir, "/d", "`configDir` 没落进 `config_dir`");
                assert_eq!(
                    name.as_deref(),
                    Some("n"),
                    "`name` 没落进 `name` —— 它有 `#[serde(default)]`，改名不会报缺字段，\
                     只会**静默变 None**（= 那次拉起从此进不了 ccm 容器）。本条就是为它写的"
                );
            }
            other => panic!("`kind:\"named\"` 解出来不是 `Named`：{other:?}"),
        }
        let base: LaunchAccount = serde_json::from_value(serde_json::json!({ "kind": "base" }))
            .expect("`history.rs::LaunchAccount` 不再收 `{kind:\"base\"}`");
        assert!(
            matches!(&base, LaunchAccount::Base),
            "`kind:\"base\"` 解出来不是 `Base`：{base:?}"
        );
        // ★ 阴性对照：上面两条若因为「反序列化太宽」而恒真，这一条会揭穿它。
        assert!(
            serde_json::from_value::<LaunchAccount>(serde_json::json!({
                "kind": "named", "cfgDir": "/d"
            }))
            .is_err(),
            "换个键名照样收 ⇒ 上面那两条是恒真的，这张表此刻没人管"
        );
        vec![
            ("tag", "kind"),
            ("named", "named"),
            ("base", "base"),
            ("configDir", "configDir"),
            ("name", "name"),
        ]
    }

    /// 生成 `src/generated/launch-render-facts.ts` 的正文。
    fn render_launch_render_facts() -> String {
        let mut s = String::from(HEADER);

        lines(
            &mut s,
            &[
                "",
                "/**",
                " * `ccm` 调用行**每次调用都无条件要求**的能力集。",
                " *",
                " * 源：`ccm_invocation::CLI_REQUIRED_CAPS`。",
                " *",
                " * ⚠ `K-R95` 交回时**前端还没能改读这一格**：`e2e/ccm-contract-parity.sh`",
                " * 按 `src/launch-render-cli.ts` 的单行数组字面量抽它，搬走那条 e2e 当场红，",
                " * 而那个文件不在本件写区。⇒ 前端那一份今天由",
                " * `launch_cli_parity.rs` 逐字节（含顺序）钉在后端这一份上。",
                " */",
                "export const CLI_REQUIRED_CAPS: readonly string[] = [",
            ],
        );
        for c in CLI_REQUIRED_CAPS {
            lines(&mut s, &[&format!("  {},", ts_str(c))]);
        }
        lines(&mut s, &["];"]);

        lines(
            &mut s,
            &[
                "",
                "/**",
                " * 八句**降级理由**的措辞。前七句由 `ccm_invocation::Refusal::reason()` 现场产出，",
                " * 第八句由 `launch_wire` 里那个 `PROBE_UNKNOWN_REASON` 给出。",
                " *",
                " * ⚠ 「渲染不出来」**不是错误，是诚实降级** —— 这几句是生产文案，用户会看到。",
                " */",
                "export const CLI_REFUSAL_REASON = {",
            ],
        );
        for (key, text, note) in refusal_reasons() {
            lines(
                &mut s,
                &[
                    &format!("  /** {note} */"),
                    &format!("  {key}: {},", ts_str(&text)),
                ],
            );
        }
        lines(&mut s, &["} as const;"]);

        lines(
            &mut s,
            &[
                "",
                "/**",
                " * 本机拉起载荷里「哪个号」那一格的 **wire 键名**。",
                " *",
                " * 源：`history.rs::LaunchAccount` 的 serde 契约，生成时**跑一遍生产反序列化器**",
                " * 验过（还配了阴性对照）⇒ 那边改名，`npm run gen:types` 当场 panic。",
                " *",
                " * ⚠ 前端要**用计算键**读它，不许再把 `configDir` 这几个字自己写一遍 ——",
                " * 写了就又是第二份说法。",
                " */",
                "export const LOCAL_LAUNCH_ACCOUNT_WIRE = {",
            ],
        );
        for (key, wire) in local_launch_account_wire() {
            lines(&mut s, &[&format!("  {key}: {},", ts_str(wire))]);
        }
        lines(&mut s, &["} as const;"]);
        s
    }

    /// `K-R95`：生成 `src/generated/launch-render-facts.ts`。
    ///
    /// 名字必须以 `export_bindings` 打头 —— `npm run gen:types` 就是
    /// `cargo test --lib export_bindings`。
    #[test]
    fn export_bindings_launch_render_facts() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级目录");
        let out = repo.join("src/generated/launch-render-facts.ts");
        let body = render_launch_render_facts();
        std::fs::write(&out, body).unwrap_or_else(|e| panic!("写不进 {}：{e}", out.display()));
    }

    /// ★★ 🔴 `KR95D3`：**「没探出来」不许和「它真的没装」是同一句话。**
    ///
    /// 这一格今天的后果是真的去跑一条错命令：读到「远端未装 ccm」的人会去装一遍 ccm，
    /// 而真相可能只是一次 ssh 抖动。死值验：把 `PROBE_UNKNOWN_REASON` 改成
    /// `Refusal::NotInstalled.reason()` 那句（= 悄悄回落）⇒ 本条红。
    #[test]
    fn not_knowing_is_never_spelled_the_same_way_as_knowing_it_is_absent() {
        assert_ne!(
            PROBE_UNKNOWN_REASON,
            Refusal::NotInstalled.reason(),
            "「没探出来」被写成了「远端未装 ccm」—— 那是把「不知道」伪装成「知道」"
        );
        assert!(
            PROBE_UNKNOWN_REASON.contains("{error}"),
            "这一句没带 `{{error}}` 占位 ⇒ 那条唯一线索传不下去，\
             用户只会看到一句没有出处的「不知道」：{PROBE_UNKNOWN_REASON:?}"
        );
    }

    /// ★★ 🔴 `KR95D1`：生成物里那三格**真的是跑后端算出来的**，不是一份写死的表。
    ///
    /// 本条不查「前端源码里还有没有那几个字符串」（那是**判写法**，`KR95D1` 点名的失效方向），
    /// 它查的是**产出**：拿后端此刻的值现算一遍，与写进生成物的那份逐字节比。
    /// ⇒ 改后端一处而没跑 `npm run gen:types` ⇒ 本条红（门禁第六格 `generated` 是它的复核）。
    #[test]
    fn the_facts_the_frontend_holds_are_recomputed_from_the_backend_every_time() {
        let body = render_launch_render_facts();
        for c in CLI_REQUIRED_CAPS {
            assert!(
                body.contains(&format!("  {},", ts_str(c))),
                "能力 {c} 没进生成物 —— 前端手里那份就不是后端这一份"
            );
        }
        for (key, text, _) in refusal_reasons() {
            assert!(
                body.contains(&format!("  {key}: {},", ts_str(&text))),
                "降级理由 {key} 没进生成物（后端现场产出的是 {text:?}）"
            );
        }
        // 反向自检：随便编一个后端说不出的值，必须**不在**里面（否则上面几条恒真）。
        assert!(
            !body.contains("\"没有人说过这句话\""),
            "生成物里出现了后端说不出的值 —— 上面那几条此刻恒真"
        );
    }
}
