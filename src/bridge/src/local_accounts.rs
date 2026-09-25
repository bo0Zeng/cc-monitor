//! 本机账号层的两问：`acct-iso.check` / `acct-iso.shellinit` 的本机对侧（文件末尾）。
//!
//! # 〔C4d · 第四波 4B〕本机账号清单的那份参照实现**删了**
//!
//! 这里原先还留着一份零生产调用方的本机 manifest 读者（`list_from_dir`〔散文墓碑〕一族：
//! 读上限 · 路径安全谓词 · 尾分隔符归一 · 账号库目录 · 原始 manifest 结构），只被判据驱动，理由是它身上挂着三个锚点。
//! 主会话 09-25 裁删，锚点逐个改指现存实现（后端 `observe/accounts_query.rs`，那一份才是真在答账号清单的）：
//! - 读上限的三种失败分得开 · 正好等于上限放行 ⇒ 后端 `common/fs.rs::read_regular_capped`；
//! - 欺骗字符逐组拒 ⇒ 后端 `accounts_query.rs::is_safe_config_dir`（「安全性质 / 平台形式」那条拆法后端早已照搬）；
//! - 账号库目录名是三方契约 ⇒ `acct-core` 的 `ACCTS_DIR_NAME` ＋ 后端缺省解析那一处；
//! - `authKind` / `authReady` 跨生产者对拍 ⇒ 生产者只剩后端一个，后端那一半（`auth_kind_parity_backend_side`）照旧对金样。
//! 判据在 `tests/backend/observe/accounts_query_tests.rs` 的 C4d 那一节。随之 monitor 的账号结构（`accounts.rs` 整份）
//! 与两个生成物（账号 · 鉴权方式）没了产出者、一起出列，TS 那一侧手写在 `src/accounts.ts`。
//!
//! 更早的来历（`N-F1c` 读口改问本机后端、C4c 本机清单与信任预检改走通道）见 git 历史与 `调研/第四波记录/C4c.md`。

// `N-F1c`：本机那两问的传输。**只做调用方**，一个字都不改它的语义。
use crate::backend::observe::local_query::{run_query, QueryOutcome};

// 〔C4c · 第四波 4B〕本机账号清单那条 Tauri 命令（`list_local_accounts`）与它的三档结局（`LocalAccountsOutcome`）·
//   行解析转交（`classify_local_accounts`）· 本机并表（`with_apikey_table`）〔散文墓碑〕一起退役：本机与远端同一条路 ——
//   前端经通道说 `accounts-list`，那台机器的后端出成品、并它自己那份 apikey 表（规则住 `acct-core`）。
//   「本机后端不在 ≠ 你没有账号」那一格由通道的失败层级接住（`ipc/chan-caller.ts::saidOf`：够不着 ≠ 答了空表）。

#[cfg(test)]
#[path = "../../../tests/bridge/local_accounts_tests.rs"]
mod tests;

// 〔C4c · 第四波 4B〕本机的「这个账号信任过这个目录吗」（`accounts.trust` 的本机对侧）退役〔散文墓碑〕：`local_trust_argv` /
//   `classify_local_trust` / `local_account_trust` 三个函数随之删了〔散文墓碑〕；信任预检上了帧面（`accounts-trust`），
//   本机与远端同一条路（前端 `accounts.ts::checkTrust` 经通道问）。

// ─────────────────────────────────────────────────────────────────────────────
// E79：本机的「某个 sid 现在跑在哪个账号下」
// ─────────────────────────────────────────────────────────────────────────────

// 〔F10b 第二批·下半〕`MAX_LOCAL_SESSION_FILES` / `MAX_LOCAL_SESSION_FILE_BYTES` **已删** ——
// 它们是后端侧同名上限的**第二份**（`accounts_query.rs::MAX_SESSION_FILES` 与
// `accounts_query.rs::MAX_SESSION_FILE_BYTES`，值逐字相同：
// 500 个文件 / 1 MiB）。唯一的用处随 E79 那条本机查询改走本机后端一起消失（〔C4a〕那条查询后来也退役了）
// ⇒ 留着就是「同一个数两处各写一份」（定框 §4）。上限现在只有一个家：backend 那边，
// 且由它自己的测试与 `read_regular_capped` 钉着。

// 〔F10b 第二批〕`proc_claude_config_dir` 与 `pid_alive` **已删** ——
// 它们是后端侧 `platform/proc.rs` 那两个（`:19` / `:80`）的**第二份实现**，
// 而本文件的头注原本就写着「判据与后端侧逐字同源」。
// 唯一的调用方（E79 那条本机查询，〔C4a〕已退役）当时已改走本机后端的 `--session-accounts`
// ⇒ 留着就是「同一件事两处各写一份」（定框 §4），且平台原语该住 `platform/`（C10）。
// ⚠ 不是「暂时没人用就删」（铁律 13 禁的那种）：它们没有判据、没有测试、
//   也不是任何东西的唯一锚点 —— 语义的家在后端那边，且由它自己的测试钉着。

// 〔C4a · 第四波〕E79 那条本机版「某会话跑在哪个账号下」的 Tauri 命令**退役**：
//   它每问一次 exec 一个本机后端 `--session-accounts`、再把行解析一遍 —— 与远端那条
//   （`accounts.rs` 里 A2 那条，同拍退役）是**同一套解析、两种传输**。
//   现在两侧收成一条路：前端经通道（`chan::webview::chan_call`）问那台机器的后端 `accounts-sessions`
//   （本机由 `<local>` 那条长连接答），逐行解释只剩 `src/accounts.ts::parseSessionAccountLines` 一处。
//   上一版头注里记着的边界（本机后端不在 ⇒ 说原因、不伪造空表；Windows 上后端明说观测不到）
//   换成通道的三层错误：没有控制通道 / 后端不认 / 对端说不行，前端一律按「这一次没问出来」。

// ─────────────────────────────────────────────────────────────────────────────
// 〔`A3` 第二波〕`acct-iso.check` / `acct-iso.shellinit` 的本机对侧
// ─────────────────────────────────────────────────────────────────────────────
//
// 远端那两条（`acct_iso_deploy.rs::check_remote_acct_iso` / `remote_acct_iso_shellinit`）
// 吃 `RemoteConfig`、经 SSH 跑一串 shell；本机这两条**问本机后端**
// （`--acct-iso-status` / `--acct-iso-shellinit`，住后端账号层 `accounts/iso.rs`），
// 与 `list_local_accounts` 同一种调用法 —— `NR2`「claude 真实跑在哪台机器，账号就归那台的后端管」。
// 出参类型与远端那条**逐字相同**（`AcctIsoStatus` / 片段文本），前端按同一个形状读。

/// 后端 stderr 那一行 `{code,message}` 里的 `message`；解析不了就原样带回（不猜）。
fn backend_message(stderr: &str) -> String {
    serde_json::from_str::<serde_json::Value>(stderr.trim())
        .ok()
        .and_then(|v| {
            v.get("message")
                .and_then(|m| m.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| stderr.trim().to_string())
}

/// `--acct-iso-status` 的结局 → `AcctIsoStatus` —— **纯函数**。
///
/// 「没装」是 `Ok(installed:false)`（后端 exit 0 的那一支），不是 `Err`；
/// `Err` 只给「问不出来」的两档（后端不在 / 查询失败 / 出参读不懂），且**不许**说成「没装」。
pub(crate) fn classify_local_acct_iso(
    outcome: QueryOutcome,
) -> Result<crate::acct_iso_deploy::AcctIsoStatus, String> {
    let stdout = match outcome {
        QueryOutcome::Ok(s) => s,
        QueryOutcome::NoBackend(reason) => {
            return Err(format!(
                "本机后端不在，查不出这台机器装没装 cc-acct-iso：{reason}"
            ))
        }
        QueryOutcome::Failed { code, stderr } => {
            return Err(format!(
                "本机后端查不出 cc-acct-iso 装没装（退出码 {code:?}）：{}",
                backend_message(&stderr)
            ))
        }
    };
    let v: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("本机后端回的 cc-acct-iso 状态读不懂：{e}"))?;
    let installed = v
        .get("installed")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "本机后端回的 cc-acct-iso 状态里没有 installed".to_string())?;
    Ok(crate::acct_iso_deploy::AcctIsoStatus {
        installed,
        path: v.get("path").and_then(|p| p.as_str()).map(str::to_string),
        vendor_id: crate::acct_iso_deploy::vendor_id().to_string(),
    })
}

/// `acct-iso.check` 的本机对侧：这台机器装没装 `cc-acct-iso`。
#[tauri::command]
pub async fn check_local_acct_iso() -> Result<crate::acct_iso_deploy::AcctIsoStatus, String> {
    tokio::task::spawn_blocking(|| {
        classify_local_acct_iso(run_query(
            env!("CCM_TARGET_TRIPLE"),
            &["--acct-iso-status"],
            &*crate::spawn_managed::local_backend_one_shot_query(),
        ))
    })
    .await
    .map_err(|e| format!("本机 cc-acct-iso 查询没能跑完：{e}"))?
}

/// `--acct-iso-shellinit` 的结局 → 片段 —— **纯函数**。
///
/// 围栏校验与远端那条**同一个判定**（`acct_iso_deploy::shellinit_fence_state`），
/// 只是话按本机说（远端那句「先在『维护』里部署」对本机是一条走不通的路 ——
/// 本机的安装口今天不存在，`LOCAL_ACCOUNTS_COPY.emptyNext` 逐字写着）。
pub(crate) fn classify_local_shellinit(outcome: QueryOutcome) -> Result<String, String> {
    use crate::acct_iso_deploy::{shellinit_fence_state, FenceState};
    use crate::acct_iso_deploy::{SHELLINIT_FENCE_BEGIN, SHELLINIT_FENCE_END};
    let out = match outcome {
        QueryOutcome::Ok(s) => s,
        QueryOutcome::NoBackend(reason) => {
            return Err(format!("本机后端不在，拿不到这台机器的 rc 片段：{reason}"))
        }
        QueryOutcome::Failed { stderr, .. } => {
            return Err(format!(
                "本机没能产出 rc 片段：{}",
                backend_message(&stderr)
            ))
        }
    };
    match shellinit_fence_state(&out) {
        FenceState::Complete => Ok(out),
        FenceState::Truncated => Err(format!(
            "本机产出的 rc 片段不完整（有 {SHELLINIT_FENCE_BEGIN:?} 但没有 {SHELLINIT_FENCE_END:?}），\
             可能被截断了。别贴：半截片段会让登录 shell 报错。请重试。"
        )),
        FenceState::Missing => Err(format!(
            "本机没能产出 rc 片段（输出里没有 {SHELLINIT_FENCE_BEGIN:?}）。\
             常见原因：这台机器还没跑过 `cc-acct-iso init`。"
        )),
    }
}

/// `acct-iso.shellinit` 的本机对侧：这台机器的 `cc-acct-iso shellinit` 片段（**只读**，不代写 rc）。
#[tauri::command]
pub async fn local_acct_iso_shellinit() -> Result<String, String> {
    tokio::task::spawn_blocking(|| {
        classify_local_shellinit(run_query(
            env!("CCM_TARGET_TRIPLE"),
            &["--acct-iso-shellinit"],
            &*crate::spawn_managed::local_backend_one_shot_query(),
        ))
    })
    .await
    .map_err(|e| format!("本机 rc 片段查询没能跑完：{e}"))?
}
