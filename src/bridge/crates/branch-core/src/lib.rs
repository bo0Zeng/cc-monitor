//! **会话分叉的记录变换** —— monitor 与远端 daemon 共用的**唯一**一份实现。
//!
//! # 为什么要单独一个 crate（G1，branch-anywhere）
//!
//! 分叉这件事本地和远端都要做：本地由 monitor 直接算，远端由 daemon 在**远端本地**算
//! （几十 MB 的 jsonl 不该为了分叉拉过 ssh）。两边跑的必须是**同一段逻辑**。
//!
//! 备选过三条路，选这条的理由见 `.claude/planned-build/branch-anywhere/features/G1-*.md`：
//! 仓里已有三个「复制 + 漂移守卫」的双写点先例（`TMUX_LS_FMT` / `observation` 取值集 /
//! `RemovalCause` 字面量），但那三个都是**常量**；把一个 80 行的算法复制一份，
//! 守卫要么脆（改个变量名就假红），要么退化成整体字节比对。**共享 crate 让漂移在结构上不存在。**
//!
//! # 硬约束：不能把 daemon 拖进 monitor 的 workspace
//!
//! `src/backend` 是**独立 crate、刻意不在 workspace 里** —— 否则 Windows CI 的
//! `cargo test --all` 会去构建这个 Linux-only 的 daemon 并炸掉。
//! 本 crate 位于 monitor 的 workspace 内（`cargo test --all` 会跑它的测试），
//! 而 daemon 只是**单向 path 依赖**过来 —— 依赖不会反向制造 workspace 成员关系。
//!
//! # 落盘格式的判据
//!
//! **与 CC 原生 `/branch` 逐字段一致**，不是与 `claude-agent-sdk` 的 `fork_session` 一致。
//! 详见 `build_branch_records` 上方那段实证记录与 `branch_matches_native_fork_shape`。

/// **纯函数**（可注入直测）：从解析好的记录里，取分叉点 `message_uuid` 沿 parentUuid 回溯
/// 到根的线性前缀，逐条改写成原生分支格式。返回按「根 → 分叉点」顺序的输出记录。
///
/// - `message_uuid` 不在记录集中 → Err（前端传了不存在的 uuid）。
/// - 环防御：parentUuid 指回已访问节点即停（append-only jsonl 理论无环，防御性）。
pub fn build_branch_records(
    lines: &[serde_json::Value],
    message_uuid: &str,
    src_sid: &str,
    new_sid: &str,
) -> Result<Vec<serde_json::Value>, String> {
    use std::collections::{HashMap, HashSet};
    let mut by_uuid: HashMap<&str, &serde_json::Value> = HashMap::new();
    for v in lines {
        if let Some(u) = v.get("uuid").and_then(|x| x.as_str()) {
            by_uuid.entry(u).or_insert(v); // 保首见（幂等，容 at-least-once 重复行）
        }
    }
    if !by_uuid.contains_key(message_uuid) {
        return Err(format!(
            "refuse branch: message_uuid {message_uuid:?} not found in source session"
        ));
    }
    // G0：**子 agent 记录不是可分叉的会话**。
    //
    // `create_branch_session` 是 Tauri 命令，前端传什么 uuid 它就用什么；F77 只是在
    // 「子 agent 查看器」里不挂那个按钮，**后端从来没拦过**。真让一条 sidechain 记录
    // 进来，产出的是一段 subagent 转录冒充会话。
    //
    // **判据是 truthy，不是「字段存在」**：真机原生 fork 的复制段上 244/244 条都**带着**
    // `isSidechain` 这个键，值是 `false`。按「存在即拒」会把正常会话全拒光。
    if by_uuid[message_uuid]
        .get("isSidechain")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Err(format!(
            "refuse branch: message_uuid {message_uuid:?} is a sidechain (subagent) record"
        ));
    }
    // 沿 parentUuid 回溯到根
    let mut chain: Vec<&str> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut cur = Some(message_uuid);
    while let Some(u) = cur {
        if !by_uuid.contains_key(u) || seen.contains(u) {
            break;
        }
        seen.insert(u);
        chain.push(u);
        cur = by_uuid[u]
            .get("parentUuid")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty());
    }
    chain.reverse(); // 根 → 分叉点

    let mut out = Vec::with_capacity(chain.len());
    for u in chain {
        let mut rec = by_uuid[u].clone();
        let obj = rec
            .as_object_mut()
            .ok_or("refuse branch: record is not a JSON object")?;
        obj.insert(
            "sessionId".into(),
            serde_json::Value::String(new_sid.to_string()),
        );
        obj.insert(
            "forkedFrom".into(),
            serde_json::json!({ "sessionId": src_sid, "messageUuid": u }),
        );
        out.push(rec);
    }
    // 原生 root 恒 parentUuid=null。回溯若因链断（parentUuid 指向集合外 / 坏行被跳）
    // 而止，新 root(out[0]) 会残留悬空 parentUuid → 置 null，产出干净自洽的根。
    if let Some(first) = out.first_mut() {
        if let Some(obj) = first.as_object_mut() {
            if obj.get("parentUuid").is_some_and(|p| !p.is_null()) {
                obj.insert("parentUuid".into(), serde_json::Value::Null);
            }
        }
    }
    Ok(out)
}

// ═══════════════════════════════════════════════════════════════════════════
// `K-R88`：**「按 sid 找那份会话文件」也收成一份**
// ═══════════════════════════════════════════════════════════════════════════
//
// `G1` 收掉的是记录变换（上面那一半）。它前面还有一步 —— **把入参变成一条源文件路径**
// —— 收之前两侧各有一份，且**入参形状都不一样**：
// monitor 那条收**路径**，再 canonicalize 一遍验它落在记录树内；
// 后端那条收 **sid**，在记录树下按文件名找。
//
// 「同一件事两个入参形状」花的钱很具体：**「查不到」这件事两边可以各答各的** ——
// 一边报错、一边随手挑一个，而没有任何东西会因此变红。
// 收成一份之后入参形状也就只剩一个（sid），两侧各自把自己那棵记录树的根交进来。
//
// ⚠ **本段刻意没有单元测试住在这个 crate 里，这不是漏。**
// 后端那条常驻判据 `the_clean_verdict_is_re_measured_on_the_tree_every_run`
// 拿它自己那两张模式表**整棵树、不剥测试段、连注释一起**重扫本 crate，
// 而「在临时目录里造一棵树」要用的那几个动词正好都在它的针里
// ⇒ 在这里写 IO 夹具会把本 crate 的判档从「未见写面」撞掉。
// 驱动它的是**两侧各自的测试**（monitor 的 `history.rs` / 后端的 `control/fork_write.rs`），
// 而那恰好就是 `KR88D1` 第三刀要的形状：**改这里一处，两边一起红。**

use std::path::{Path, PathBuf};

/// 往记录树下面找几层。
///
/// `2` = 记录根自己那一层（`<根>/<sid>.jsonl`）＋ 项目目录那一层
/// （`<根>/<项目目录>/<sid>.jsonl`，真机上的常态）。**再深一层是子 agent 那一族**，
/// 而它们不是可分叉的会话 —— 同一件事的另一半是上面那条 sidechain 判据。
pub const SESSION_LOOKUP_DEPTH: usize = 2;

/// sid 的合法形状：非空、不超过 64、只许 `[A-Za-z0-9-]`。
///
/// 它挡掉 `..`、`/`、`\` 与任何能拼出别处路径的字符。理由不是「防手滑」：
/// 后端是被远程调起来的，**少一个可被构造的路径入参就少一条路径穿越面**
/// （`src/doc/INVARIANTS.md` §41.6 三条收窄里的第 3 条）。
pub fn is_plain_sid(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// **两侧唯一的一份「找文件」**：在记录树 `records_root` 下按 sid 找那份 `<sid>.jsonl`。
///
/// - sid 形状不合法 → `Err`，且**先于任何 IO**（见 [`is_plain_sid`]）。
/// - 找不到 → `Err`。🔴 **绝不静默退回「树上第一份」** —— `KR88D2` 第三刀验的就是这一格：
///   一边报错、一边随手挑一个，就是两边处置不一致。
/// - 符号链接**不算命中**：类型判定取自目录项本身（**不跟随**链接），
///   于是一条指向记录树之外的链接进不来。这半是围栏 ——
///   monitor 那条路原先靠「canonicalize 两边再比前缀」买同一样东西，
///   而收进 sid 之后连**表达**一个界外目标的办法都没有了。
pub fn find_session_file(records_root: &Path, sid: &str) -> Result<PathBuf, String> {
    if !is_plain_sid(sid) {
        return Err(format!("refuse branch: invalid session id {sid:?}"));
    }
    let want = format!("{sid}.jsonl");
    look_down(records_root, &want, SESSION_LOOKUP_DEPTH)
        .ok_or_else(|| format!("refuse branch: session {sid} not found under the session tree"))
}

/// 逐层往下找 `want` 这个文件名，最多 `depth` 层。
///
/// **同层先看文件、再下潜**，且子目录按名字排序后再走 —— 目录项的自然顺序由文件系统决定，
/// 而「同一棵树两次调用给两个答案」是那种只在真机上现形的分叉。
/// 读不动的目录（权限）**跳过而不是报错**：一棵记录树里有一个进不去的角落，
/// 不该让别处那份找得到的会话也分叉不了。
fn look_down(dir: &Path, want: &str, depth: usize) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    let listing = std::fs::read_dir(dir).ok()?;
    let mut subdirs: Vec<PathBuf> = Vec::new();
    for entry in listing.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_file() {
            if entry.file_name().to_str() == Some(want) {
                return Some(entry.path());
            }
        } else if kind.is_dir() {
            subdirs.push(entry.path());
        }
    }
    subdirs.sort();
    subdirs
        .into_iter()
        .find_map(|d| look_down(&d, want, depth - 1))
}

#[cfg(test)]
#[path = "../../../../../tests/bridge/crates/branch-core/lib_tests.rs"]
mod tests;
