//! Claude 数据目录与 monitor 自己配置文件的路径解析。
//!
//! ## 三级回退（resolve_claude_dir）
//!
//! 1. 用户在设置面板里手动选的路径 —— 写在 monitor config.json 的 `claudeDir` 字段
//! 2. 环境变量 `CLAUDE_CONFIG_DIR`（Anthropic 官方约定）
//! 3. `~/.claude`（默认）
//!
//! ## 关于循环依赖
//!
//! monitor 自己的 config.json 始终保存在**默认位置** `~/.claude/work/config.json`，
//! **不**跟随 claudeDir 变化。这样：
//!   - 读 monitor 配置不需要先解析 claudeDir
//!   - 用户切换 Claude 数据目录后，monitor 的 theme/字体设置不会丢
//!   - `claudeDir` 字段只决定 monitor 去哪里找 `projects/` 和 `sessions/`
//!
//! 文档化为"monitor 设置永远在默认位置，'Claude 数据目录'只影响数据源指向"。
//!
//! ## 🔴 那句「永远在默认位置」有一个出口〔2026-09-22 `P17`〕
//!
//! `CCM_DATA_DIR`（[`DATA_DIR_ENV`]）**只为「把这个进程整体挪到别处跑」而存在** ——
//! 跑自动化测试、跑一次性复算。它**不是**给用户搬家用的设置面
//! （用户那一侧的「数据位置」是只读展示）。
//! 给了但不是绝对路径 ⇒ 回 `None`，**不退回用户真 profile**，
//! 逐条理由住 [`resolve_monitor_data_dir`]。

use std::path::PathBuf;

/// Claude 数据目录根（即 `.claude` 的实际位置）。所有 projects / sessions 读取都从这里派生。
pub fn resolve_claude_dir() -> Option<PathBuf> {
    if let Some(p) = read_user_override() {
        if p.exists() {
            tracing::info!("claude_dir from user config: {}", p.display());
            return Some(p);
        }
        tracing::warn!(
            "claude_dir from user config does not exist: {} — falling back",
            p.display()
        );
    }
    if let Ok(env_path) = std::env::var("CLAUDE_CONFIG_DIR") {
        let p = PathBuf::from(env_path);
        if p.exists() {
            tracing::info!("claude_dir from CLAUDE_CONFIG_DIR: {}", p.display());
            return Some(p);
        }
        tracing::warn!(
            "CLAUDE_CONFIG_DIR points to non-existent path: {} — falling back",
            p.display()
        );
    }
    let home = dirs::home_dir()?;
    let default_path = home.join(".claude");
    tracing::info!("claude_dir default: {}", default_path.display());
    Some(default_path)
}

/// 那个 env 出口的名字。
///
/// 🔴 它**只为「把这个进程整体挪到别处跑」而存在**（跑自动化测试、跑一次性复算），
/// **不是**给用户搬家用的设置面。用户那一侧的「数据位置」是只读展示
/// （`data_paths.rs` → 设置面板那一块）。
pub const DATA_DIR_ENV: &str = "CCM_DATA_DIR";

/// Monitor 自己的 user-data 目录。
///
/// 默认 `~/.claude/work/` —— **不跟随 `claudeDir` 变化**
/// （避免循环依赖、且保留用户设置在切换数据目录后仍存在）。
///
/// # 🔴 那个 env 出口为什么必须有（`设计/99 §4.9.7 P17`）
///
/// 这个目录是**用户手写的真相**的家：`config.json` · tab 集合名 · 固定了哪些 tab
/// · 凭据库 · 历史元数据 · 全景引擎。`设计/30 §B.4` 逐字的理由是
/// 「**集合名是用户手写的真相，不是能重算的缓存 ⇒ 它必须活过一次清缓存**」。
///
/// ⇒ 而在这之前它**没有任何出口** ⇒ 任何一趟「把 monitor 跑起来量点东西」
/// 都会**写进用户真 profile**。2026-09-21 在那台 Win11 虚拟机上跑 tier-2 时
/// 现打到这一形：`auto-launch.json` 从 87 字节被改成 133 字节，
/// 那一路只能靠**跑前备份、跑后还原**做到零残留 ——
/// 而「靠每次记得备份」不是一个机制，是一次运气。
///
/// # 🔴 给了但不合法 ⇒ 回 `None`，**不退回用户真 profile**
///
/// 这一条是本函数唯一有争议的地方，所以写清：
/// 退回真 profile 看起来「更稳」，实际是**这个出口存在的理由的反面** ——
/// 那一趟自动化会以为自己被隔离了，而它正在写用户的东西，**而且没有一句话**。
/// ⇒ 宁可让各个消费者**可见地降级**（凭据库拿不到、全景落回被动、
/// 设置面板那一块显示拿不到路径），也不要静默写对家。
///
/// ⚠ 只认**绝对路径**：相对路径会按进程 cwd 解，而这个进程的 cwd 不是它自己定的。
///
/// ⚠ 它**不建目录、不判存在** —— 首次启动时它本来就不存在（建目录是各消费者的事）。
pub fn resolve_monitor_data_dir() -> Option<PathBuf> {
    monitor_data_dir_from(
        std::env::var(DATA_DIR_ENV).ok().as_deref(),
        dirs::home_dir(),
    )
}

/// [`resolve_monitor_data_dir`] 里**有逻辑的那一段**，抽出来所以判得到。
///
/// 🔴 **抽出来的理由不是风格，是本仓踩过的一条**：`lib_env_scrub_tests` 那条判据
/// 头注逐字「**绝不能在测试里 set/remove 真实的 `CLAUDE_*` 变量
/// （会干扰并发测试与宿主环境）**」—— cargo test 多线程跑、进程级 env 共享，
/// 而 `resolve_monitor_data_dir` 有 8 处消费者。
/// ⇒ 判据**不许**去动那个 env；它把值当参数喂进来。
/// （同族先例：`filewin::download::judge_dest` 把「那儿有没有东西」注进来。）
pub fn monitor_data_dir_from(env_val: Option<&str>, home: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(raw) = env_val {
        let t = raw.trim();
        // 设成空串 == 没设（shell 里 `CCM_DATA_DIR=` 是最常见的「取消」写法）。
        if !t.is_empty() {
            let p = PathBuf::from(t);
            if p.is_absolute() {
                tracing::info!("monitor_data_dir from {}: {}", DATA_DIR_ENV, p.display());
                return Some(p);
            }
            // 🔴 **不退回真 profile** —— 逐条理由住上面那一节。
            tracing::warn!(
                "{} 不是绝对路径（{}）—— 拒绝使用，也**不**退回 ~/.claude/work：                 那会让一趟以为自己被隔离了的自动化去写用户的东西",
                DATA_DIR_ENV,
                t
            );
            return None;
        }
    }
    Some(home?.join(".claude").join("work"))
}

/// Monitor 配置文件：`<monitor_data_dir>/config.json`
pub fn resolve_config_path() -> Option<PathBuf> {
    Some(resolve_monitor_data_dir()?.join("config.json"))
}

/// 读取 monitor config.json 的 `claudeDir` 字段（用户在设置面板里写入）。
fn read_user_override() -> Option<PathBuf> {
    let cfg = resolve_config_path()?;
    if !cfg.exists() {
        return None;
    }
    // 〔W5-VIS · E 吞错普查点名〕文件在而读不动 / 不是 JSON ⇒ 设置里那条「Claude 目录」覆盖这一次**不生效**、回到默认目录。
    // 原先两个 `.ok()?` 把它折成「没设」—— 用户以为改了目录，实际读的是默认那一份，一句话都没有。
    // 这个函数调用得很勤（每次解析 Claude 目录都读一遍）⇒ 同一个进程里只说一次。
    let value: serde_json::Value = match std::fs::read_to_string(&cfg)
        .map_err(|e| e.to_string())
        .and_then(|raw| serde_json::from_str(&raw).map_err(|e| e.to_string()))
    {
        Ok(v) => v,
        Err(e) => {
            static SAID: std::sync::Once = std::sync::Once::new();
            SAID.call_once(|| {
                tracing::warn!(
                    "设置里的 Claude 目录覆盖这一次没生效：{} 读不动或不是 JSON（{e}）—— 用的是默认目录",
                    cfg.display()
                );
            });
            return None;
        }
    };
    let dir_str = value.get("claudeDir")?.as_str()?;
    if dir_str.trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(dir_str))
}

#[cfg(test)]
#[path = "../../../tests/bridge/paths_tests.rs"]
mod tests;
