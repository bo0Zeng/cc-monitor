//! P2s（定框 `C8`）：**每台机一份 daemon 策略**。
//!
//! 今天只有一条策略：**monitor 退出时要不要主动结束这台机的 daemon**，默认 **false（不主动结束）**。
//!
//! # 归属：为什么持久化不在这里
//!
//! `config.rs` 头注逐字「Rust 端**不解释配置内容**（schema 在前端定义）」。
//! 若这里也往 `config.json` 里读写，同一个文件就有了**两个写者** ——
//! 前端「读—改—写」整份的那一刻，会把 Rust 刚写进去的键按一份**陈旧副本**覆盖掉。
//! ⇒ 持久化归前端；本模块只持有**生效值**，由前端在改动时与启动时推进来。
//!
//! # ⚠ 「不主动结束」不等于「继续跑」
//!
//! 实测〔08-11，P2s §0a〕：daemon 是**纯 stdio 子进程**，monitor 一退读端就断，
//! 它在 **153 毫秒**内自己 broken-pipe 退出。所以本策略的真实语义是
//! **「立刻杀」与「让它自己死」之差**，不是「后台常驻」。
//! 真要常驻得先给 daemon 一个监听口 —— 那是 `P2d`，待决 `U7`。
//! **UI 文案不许写「daemon 继续运行」**（P2s-Y5）。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn table() -> &'static Mutex<HashMap<String, bool>> {
    static T: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 缺省：**不主动结束**（`C8`③ 的前半句 —— 那半是站得住的）。
pub const DEFAULT_KILL_ON_EXIT: bool = false;

/// 这台机的 daemon，monitor 退出时要不要主动结束。
pub fn kill_on_exit(origin: &str) -> bool {
    table()
        .lock()
        .ok()
        .and_then(|t| t.get(origin).copied())
        .unwrap_or(DEFAULT_KILL_ON_EXIT)
}

/// 前端推进来的生效值（改动时 + 启动时各推一次）。
#[tauri::command]
pub fn set_daemon_kill_on_exit(origin: String, kill: bool) -> Result<(), String> {
    if origin.trim().is_empty() {
        return Err("origin 不许为空 —— 策略是 per-host 的，没有「全局」这一档".into());
    }
    let mut t = table().lock().map_err(|e| format!("锁毒化: {e}"))?;
    tracing::info!("daemon 策略：origin={origin} kill_on_exit={kill}");
    t.insert(origin, kill);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_origin_defaults_to_not_killing() {
        assert!(
            !kill_on_exit("这台机从来没被推过策略"),
            "缺省必须是「不主动结束」——`C8`③ 的前半句。\n\
             缺省若是 true，用户什么都没设就会被杀 daemon，而开关默认关着。"
        );
    }

    #[test]
    fn the_policy_is_per_origin_not_global() {
        set_daemon_kill_on_exit("甲机".into(), true).expect("设甲机");
        assert!(kill_on_exit("甲机"), "甲机设了 true 却读不回来");
        assert!(
            !kill_on_exit("乙机"),
            "改甲机把乙机也改了 —— 那就不是 per-host 而是全局一份（`C8`① 明说粒度是每台机各一个）"
        );
    }

    #[test]
    fn an_empty_origin_is_refused() {
        assert!(
            set_daemon_kill_on_exit("  ".into(), true).is_err(),
            "空 origin 必须拒。放过它等于悄悄造出一档「全局策略」，\n\
             而 `kill_on_exit(真 origin)` 永远读不到它 —— 设了没反应，且不报错。"
        );
    }
}
