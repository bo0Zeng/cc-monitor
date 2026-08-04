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
    /// `false` = 本机路径。本机不走 CLI 渲染器（§36），Rust 侧也照样拒。
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

    /// ★ **生产接线钉**：主路真的调那两条 backend 渲染命令。
    ///
    /// 一旦有人把它改回「TS 自己渲染」，本条红 —— 而那种回退**功能不变砖**
    /// （TS 兜底渲染器还在），门禁也不会因为别的原因红。
    #[test]
    fn the_remote_launch_main_path_really_calls_the_backend_renderers() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级");
        let ts = std::fs::read_to_string(root.join("src/remote-launch-run.ts"))
            .expect("读不到 remote-launch-run.ts");
        assert!(
            ts.len() > 5000,
            "remote-launch-run.ts 只有 {} 字节，抽错了？",
            ts.len()
        );
        // 剥整行注释 + 行尾注释（F10 那次学到的：行尾注释里的提及不算数）。
        let prod: String = ts
            .lines()
            .filter(|l| !l.trim_start().starts_with("//") && !l.trim_start().starts_with('*'))
            .map(|l| match l.find("//") {
                Some(i) => &l[..i],
                None => l,
            })
            .collect::<Vec<_>>()
            .join("\n");
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
    /// 依据一：**attach 那格仍在 TS** —— `session-backend.ts` 仍被生产 import
    /// （daemon 的 `control/launch.rs` 头注逐字写着「本模块**不 attach**，一次都不」）。
    /// 依据二：**`create-or-attach` 那格仍未切** —— 生产段一次都不发这个 mode。
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
    #[test]
    fn the_two_reasons_u8c3_cannot_delete_the_ts_renderer_still_hold() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级");
        // 依据一：attach 那格
        let run = std::fs::read_to_string(root.join("src/remote-launch-run.ts")).expect("读不到");
        let fallback = std::fs::read_to_string(root.join("src/launch-render-fallback.ts"))
            .expect("读不到 launch-render-fallback.ts");
        assert!(
            fallback.contains("session-backend") || run.contains("session-backend"),
            "生产 TS 里再也找不到 `session-backend` —— **这多半是好事**：\n\
             attach 那格可能已经搬走了 ⇒ U8c-3 的依据一没了，回 F07/U8c-3 重裁。"
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
        assert!(
            hits.is_empty(),
            "生产段开始发 `create-or-attach` 了（{hits:?}）—— **这多半是好事**：\n\
             「起会话」那格可能切到 daemon 了 ⇒ `INVARIANTS §33b` 三问的答案① 又变了，\n\
             回 F07/U8c-3 重裁「删 TS 渲染器」的前置。"
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
