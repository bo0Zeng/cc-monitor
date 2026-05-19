use serde_json::Value;

/// 读 ~/.claude/claudecode-frontend/config.json，缺失时返回内置默认。
/// M5 落地：完整 schema + 默认值 + 热更新 watcher。
#[tauri::command]
pub fn load_config() -> Result<Value, String> {
    Ok(default_config())
}

/// 写回 config.json，原子写。
/// M5 落地。
#[tauri::command]
pub fn save_config(_value: Value) -> Result<(), String> {
    Ok(())
}

fn default_config() -> Value {
    serde_json::json!({
        "appearance": {
            "font_family": "Inter",
            "code_font": "JetBrains Mono",
            "font_size": 14,
            "line_height": 1.7,
            "theme": "dark"
        },
        "rendering": {
            "math_engine": "katex",
            "default_fold_tool_calls": true,
            "default_fold_thinking": true,
            "default_fold_subagent": true,
            "syntax_theme": "github-dark",
            "max_cached_messages_per_tab": 1000
        },
        "behavior": {
            "follow_terminal_focus": true,
            "archive_after_idle_minutes": 5,
            "show_in_tray": true,
            "minimize_to_tray": false
        },
        "session": {
            "load_archived_days": 7,
            "watch_paths": ["~/.claude/projects"]
        }
    })
}
