//! Claude Code 用户级设置文件（`settings.json`）里钩子那一格的形状：`hooks.<事件>[].hooks[].command`。
//! 读 · 拼都只在这里，经注册表足迹面那两格（`FootprintFace::stop_hook_commands` · `stop_hook_slots`）给通用层：
//! cc-bus 钩子诊断（`assets/cc_bus_hooks.rs`）按它认「装没装」、拼「要加的内容」，「待办」按它算 diff。

use serde_json::{json, Value};

/// 设置文件里装钩子的那一格（顶层一格；每个事件底下一组组又各装一格同名的）。
const HOOKS: &str = "hooks";
/// 会话停下时跑的那一种钩子。
const STOP_EVENT: &str = "Stop";

/// 登记在「停下时」那一种下的每条命令（照文件里的次序）。逐层容忍：不是对象 / 数组的那一层、没有 `command` 串的条目、
/// 空串都跳过（坏条目不连累别的）。
pub(crate) fn stop_hook_commands(root: &Value) -> Vec<String> {
    root.get(HOOKS)
        .and_then(|h| h.get(STOP_EVENT))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|group| group.get(HOOKS).and_then(Value::as_array))
        .flatten()
        .filter_map(|hk| hk.get("command").and_then(Value::as_str))
        .filter(|c| !c.trim().is_empty())
        .map(str::to_string)
        .collect()
}

/// 一条命令 ⇒ 要并进那份文件的（键路径, 那一项）：并法是往那条路径上的数组末尾加一项（钩子各层合并、不替换）。只算不写。
pub(crate) fn stop_hook_slots(command: &str) -> Vec<(Vec<&'static str>, Value)> {
    vec![(
        vec![HOOKS, STOP_EVENT],
        json!({ HOOKS: [ { "type": "command", "command": command } ] }),
    )]
}
