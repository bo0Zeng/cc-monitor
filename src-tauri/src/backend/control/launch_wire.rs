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
    /// `null` = 未装 ccm。
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
        std::fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("读不到 {rel}: {e}"))
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
            assert!(src.len() > 3000, "{f} 只有 {} 字节 —— 语料读错了", src.len());
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
    /// `ccm_reaches_the_backend_through_one_shot_subcommands`，`K-P2 KP2A②`）：
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
        // ★★ 第二棵树：`shared/ccm`〔`K-P2` 08-29〕。理由见本条头注「第四次」那一节 ——
        //    `§0d` 裁定的接线路（㈠）落在这份 shell 脚本里，而上面那棵树够不到它。
        //    ⚠ needle 也要换个形状：Rust 那边是字面量 `"create-or-attach"`（带引号），
        //    shell 里它会长在 JSON 串或 argv 里 ⇒ 用**带边界的词**匹配，两种形态都收得到。
        let word = format!("create-or-{}", "attach");
        let ccm = guard_core::strip_hash_comment_lines(crate::sftp::CCM_CLI_SCRIPT);
        // ★ 抽取器自检（这一棵树自己的）：剥注释器没把代码一起剥掉。
        //   建条当天 `shared/ccm` 生产段 536 行 / 全文 1258 行；地板取 300 与
        //   `ccm_cli_contract` 那几条同口径。
        assert!(
            ccm.lines().count() >= 300,
            "`shared/ccm` 的生产段只剩 {} 行 —— 剥注释器把代码也剥了？下面那条会零命中地绿",
            ccm.lines().count()
        );
        // ★★ 〔`K-P2` `D` 阶段第三拍 09-03〕**这一半本拍翻了面。**
        //
        // 它原来与 Rust 那棵树同判：「**两棵树都不许**出现 `create-or-attach`」。
        // `K-P2` `D3` 把 `shared/ccm` 的 `--tmux` 接到了后端那条一次性口上
        //（`launch_via_daemon` 发 `mode=create-or-attach`）⇒ **这一半当场红了，红得对**。
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
        // 再加上 `the_daemonless_remote_still_needs_the_ts_fallback_renderer` 那条**硬**障碍
        // （daemonless 主机今天仍是产品提供的开关）⇒ **删 TS 渲染器的前置仍然不成立。**
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
             ⚠ `shared/ccm` **不在本断言的人群里**（它在下面单判，`K-P2` `D3` 已经切过去了）。"
        );
        assert!(
            guard_core::contains_word(&ccm, &word),
            "`shared/ccm` 的生产段**不再发** `create-or-attach` 了 —— 起会话退回本机 tmux 直起？\n\
             `K-P2` `D3`（09-03）把它接到了后端那条一次性口上（`launch_via_daemon`）。\n\
             ⇒ 真要退回来，请连同 `ccm_cli_contract::BACKEND_BACKED_PATHS` 的登记、\n\
             那条 2→3 的棘轮、与 e2e `WIRE/launch` 一节一起撤，并回 `K-P2` 说明为什么。"
        );
    }

    /// ★★ **前提触发器（08-14 新立）：三问的答案③ 今天是「要」，而此前没有任何东西量它。**
    ///
    /// # 它补的是哪一个洞
    ///
    /// `U12`（daemonless 处置）这个**件**已被 `C7`〔用 08-03〕关掉，于是 08-14 的跨区对账
    /// 把三问的③ 读成了「已被裁掉 ⇒ 不再挡着」。**那是把「件关了」读成「约束消失了」**。
    ///
    /// `C7` 逐字是「**没有 daemonless —— 使用软件就要有后端** ⇒ **本机**也要有后端进程」，
    /// 它裁的是**本机**那一格（`local_backend.rs` 头注「F05a（定框 C7）」就是它的产物）。
    /// 而 `daemonless` 今天仍是**每台远端主机的用户开关**：
    /// `settings/machine-card.ts` 里那个 checkbox 逐字「daemonless 降级读取（无需 daemon）」，
    /// 落进 `RemoteHostConfig.daemonless`。
    ///
    /// ⇒ 那种主机**存在**，而它的 `↗` 走的是纯 SSH（`launch_remote_terminal` 不经 daemon），
    /// 没装 ccm 时命令只能由 monitor 自己渲染 ⇒ **`renderFallback` + `session-backend.ts`
    /// 就是那条路**。三问③ 因此从「未决」变成「**已决：要**」——
    /// 从软障碍变成硬障碍，比 08-04 更删不得。
    ///
    /// # 本条什么时候该红
    ///
    /// 哪天 `daemonless` 这个开关真被取消了（那才是 `C7` 覆盖到远端的那一天），本条主动红，
    /// 提醒回来重裁 U8c-3 —— 那时兜底渲染器少了一类必须服务的主机。
    ///
    /// ⚠ 它**不**主张「daemonless 主机今天真的在跑」（那要真远端，见 `ROADMAP §5`），
    /// 只主张「产品今天仍然提供这个开关」。两件事，别混。
    #[test]
    fn the_daemonless_remote_still_needs_the_ts_fallback_renderer() {
        // ① 开关还在：类型上的字段 + 界面上的那一格，两处都要 —— 只留字段的话，
        //    「字段还在但界面已经不给了」会被读成「开关还在」。
        let cfg = production_ts(&read_ts("src/remote-config.ts"));
        assert!(
            cfg.contains("daemonless"),
            "`RemoteHostConfig` 的生产段里没有 `daemonless` 了 —— **这多半是好事**：\n\
             `C7`（没有 daemonless）可能终于覆盖到远端主机了 ⇒ 三问③ 的答案从「要」变了，\n\
             回 U8c-3 重裁：兜底渲染器少了一类必须服务的主机。"
        );
        let card = production_ts(&read_ts("src/settings/machine-card.ts"));
        assert!(
            card.contains("daemonlessInput"),
            "机器卡片的生产段里没有 daemonless 那个开关了 —— 同上，回 U8c-3 重裁。\n\
             （字段还在而界面没了，也是「用户不再能造出 daemonless 主机」，前提照样动了。）"
        );
        // ② 而承接它的那条路仍在 TS：兜底渲染器 + 座，两个文件都得在。
        for f in ["src/launch-render-fallback.ts", "src/session-backend.ts"] {
            assert!(
                repo_root().join(f).is_file(),
                "`{f}` 没了，而 `daemonless` 开关还在 ——\n\
                 **这一条红说明有人先删了承接方**：没装 ccm 的 daemonless 远端\n\
                 今天的起会话命令就是这两个文件产的，删掉它们那类主机的 `↗` 直接哑掉。\n\
                 U8c-3 的顺序是「先有承接方，再删旧的」，不是反过来。"
            );
        }
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
