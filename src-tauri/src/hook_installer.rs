//! 占位 —— 当前活跃检测改为读 `~/.claude/sessions/`（见 session_map.rs），
//! 不再需要 SessionStart hook。保留此模块以便未来如需写入额外字段（比如 terminal_pid）时再启用。

#[tauri::command]
pub fn install_hook() -> Result<(), String> {
    Err("not used: monitor reads ~/.claude/sessions/ directly (no hook needed)".into())
}
