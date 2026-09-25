//! 第三方 API key 那份文件在**本机**的**读侧掩码**（与「它在哪」那一个出处）。
//!
//! # 〔GP1 · 第四波〕写侧不在这里了
//!
//! 主会话 09-25 裁「每台机器上这份文件的程序写者恰好一个 ＝ **那台的后端**」⇒ 本机那一份也由本机常驻后端写
//! （`apikey_remote::write_key_on` → 帧面 `apikey-key-set` → `src/backend/accounts/apikey/file_face.rs`），
//! 与远端同一条路。monitor 这一侧**一个字节都不落**，`creds-core` 的写半边（`harden`）也不再开。
//! 〔墓碑 —— 从前本模块头注是「写侧（monitor 独占）与读侧掩码」，论证「写盘为什么留在 `src/bridge/src`」
//!  与 `K-H2a` 裁四「本机这一份只有这一侧写」；两段的前提（monitor 是本机那一份的写者）没了。〕
//! 本模块留下的：[`resolve_path`]（本 monitor 认的那一份在哪 —— 起本机后端时交给它的就是这个，写之前也拿它核
//! 后端写的是不是这一份）· [`read_status`]（界面上那几格：配没配 · 掩码 · 权限 · 读坏了没有）。
//!
//! # 这一档保什么、不保什么
//!
//! 整段逐字住 `creds_core` 的 crate 头注（保：同机器上别的用户读不到 · 顺手打开配置文件不会看见 ·
//! 前端每次读写整份配置时它不在里面；**不保**：已经能以你的身份运行程序的人）。
//! ⚠ 那里还记着两条别在这里重复、但**必须一起读**的：DPAPI 那条「拷走也解不开」的性质**今天没有**，
//! 以及**远端那一侧不许从 SFTP 的 mode 参数拿机密性**。

use creds_core::perm::{self, Verdict};
use creds_core::store;
use std::path::PathBuf;

/// 那份文件在本机的位置。
///
/// ★ **与后端那侧是同一个契约**：相对路径住 `creds_core::store`，两边各自 join 自己的家目录。
/// 由 `the_two_sides_resolve_the_same_file` 对拍 —— 两边各写一份字面量，
/// 漂开的那天没有任何东西会说，而症状是「界面上配好了，账号层说没配」这种查不出来的形状。
///
/// ⚠ 它**不跟随** `claudeDir` 覆盖：`config.rs` 头注逐字「monitor 自己的设置永远在默认
/// `~/.claude/claudecode-frontend/` 下，不跟随 `claudeDir` 字段变化」。
pub(crate) fn resolve_path() -> Option<PathBuf> {
    Some(crate::paths::resolve_monitor_data_dir()?.join(store::FILE_NAME))
}

/// 回给前端的东西。**永远只有掩码**（`KS6`）。
///
/// ⚠ 这个结构体**装不下明文**——不是「我们记得不填」，是**类型里没有那个字段**。
/// `KS6` 逐字：一旦回显，key 就从「只住在后端」变成「每次打开那个界面都往前端传一遍」
/// ⇒ 泄漏面从一次变成无数次，每一次都新增前端日志 / 崩溃报告 / 截图 / 录屏四个出口。
/// ⚠ **本类型是手写对拍的，不是 `ts-rs` 生成的** —— 照本仓 `skill_host::SkillView` 的先例
/// （`src/ipc/commands.ts` 头注逐字记着那条：手写、字段名与 Rust 侧必须手动同步、
/// 由 Rust 侧一条判据读那个文件的源码逐个字段对拍，漏一个就红）。
///
/// **为什么不走 `ts-rs`**（现打 08-27）：`#[ts(export)]` 会在 `src/generated/` 新增一个文件，
/// 而那个目录的**清单等号对拍**住 `tests/generated-boundary-guard.vitest.ts`
/// （`readdirSync` + 逐项比对，新增文件必然让它红一次）——**那个文件不在本轮写区**。
/// ⇒ 走手写 + 对拍，等价的牙由 `the_ts_status_type_matches_this_struct` 买。
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub struct ApikeyCredentialsStatus {
    /// 配了没配。
    pub configured: bool,
    /// 掩码形（前后各留几位；短到看不出前后缀的整条遮掉）。没配 = 空串。
    pub masked: String,
    /// 那份文件在哪 —— 给「我想自己拿编辑器改」的人看（`KS9` 的路径要能被找到）。
    pub path: String,
    /// 权限过宽 / 查不出来时的提醒（`KS11`：**在界面上显出来**）。没问题 = `None`。
    pub notice: Option<String>,
    /// 文件读坏了时的说法（人手编打错一个逗号）。`None` = 没问题。
    pub problem: Option<String>,
}

/// 读一次，**只回掩码**。
///
/// # ⚠⚠ `K-H2c` `KH2C3`：它读的是**顶层那一把**，也就是 `LEGACY_ACCOUNT_ID` 那一行
///
/// 那一格今天仍然**读得出来**（老用户手上那份文件、以及 `K-H2a` 期界面写下的那一把），
/// 而 `K-H2c` 之后它**不再是写进去的地方** —— 写侧（〔GP1〕那台的后端，`file_face.rs`）落的是 `accounts.<id>`。
/// **这两句话必须一起读**：
/// 只读前半会以为它被废了，只读后半会以为它被删了，而**两件事都没有发生**。
///
/// ⇒ 「某个**账号**配没配」不由本函数答，由 `history::apikey_rows_at` 那一族答
/// （那正是起会话那一侧用的同一个取值口，`KH2B7` 已经把它端给了界面）。
/// 本函数答的三样是**文件级**的：那份文件在哪 · 权限过不过宽 · 是不是被手编坏了。
pub(crate) fn read_status() -> Result<ApikeyCredentialsStatus, String> {
    read_status_at(&resolve_path().ok_or_else(|| "no home dir".to_string())?)
}

/// `read_status` 剥掉「路径从哪来」之后的那一半。
///
/// ★ 抽出来的理由与本仓 `relay::server::resolve_config` 那次逐字同一条：
/// 不抽的话，这段逻辑只能对着**真实 home 目录下那份文件**跑 —— 而判据不许碰用户的真东西，
/// 于是它会变成一格**永远没人量过**的代码。
pub(crate) fn read_status_at(path: &std::path::Path) -> Result<ApikeyCredentialsStatus, String> {
    let verdict = perm::judge(&perm::probe(path));
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Ok(ApikeyCredentialsStatus {
                configured: false,
                masked: String::new(),
                path: path.display().to_string(),
                notice: notice_of(&verdict),
                problem: Some(format!("读不动这份文件：{e}")),
            })
        }
    };
    // ⚠ **解析失败不许退化成「没配」** —— 那会让界面说「还没配」而文件里其实有东西，
    //   用户一按「保存」就把自己手编的内容盖掉了。
    let (configured, masked, problem) = match store::parse(&raw) {
        Ok(doc) => match store::read_key(&doc) {
            Some(k) => (true, k.masked(), None),
            None => (false, String::new(), None),
        },
        Err(e) => (false, String::new(), Some(e.to_string())),
    };
    Ok(ApikeyCredentialsStatus {
        configured,
        masked,
        path: path.display().to_string(),
        // 文件不存在时不报权限问题（`probe` 那时返回「查不出来」，那不是一条有用的提醒）。
        notice: if raw.is_empty() {
            None
        } else {
            notice_of(&verdict)
        },
        problem,
    })
}

/// 把判断变成一句给人看的话。`OwnerOnly` ⇒ `None`（不出声）。
fn notice_of(v: &Verdict) -> Option<String> {
    match v {
        Verdict::OwnerOnly => None,
        Verdict::TooWide { how, fix } => Some(format!("{how}。怎么修：{fix}")),
        Verdict::Undetermined { why } => Some(why.clone()),
    }
}

// 〔GP1 · 第四波〕**这里原来是本机那一份的写口**（`write_key`〔散文墓碑〕 / `write_key_at`〔散文墓碑〕 /
// `check_base_url`〔散文墓碑〕）。主会话 09-25 裁「每台机器一个写者 ＝ 那台的后端」⇒ 本机那一份也交本机常驻后端写
// （`apikey_remote::write_key_on` → 帧面 `apikey-key-set` → `src/backend/accounts/apikey/file_face.rs`），
// monitor 一个字节都不落。那几条写路性质（写的那一刻读盘 · 未知键一个不吃 · 顺序稳定 · 出生即只给本人 ·
// 说不出账号拒写 · Base URL 形状错整次不写）在后端那一份的判据里逐条都有（`tests/backend/accounts/apikey/file_face_tests.rs`，
// 本侧独有的三条随之搬了过去，名字带 `gp1_`）。

#[cfg(test)]
#[path = "../../../tests/bridge/creds_store_tests.rs"]
mod tests;
