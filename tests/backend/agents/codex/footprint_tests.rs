//! Codex 足迹：各号碰的那几份住哪 · `hooks.json` 里 Stop 那几条 · `config.toml` 里那一条信任对没对上。
//! 夹具手写（形照 codex-rs rust-v0.167.0 的 `codex_config::hook_config::HooksFile` · `codex_hooks::engine::discovery`）。

use super::*;
use serde_json::json;
use std::path::Path;

const CMD: &str = "\"$HOME/.claude/skills/cc-bus/scripts/cc-bus\" stop-hook";

#[test]
fn what_we_touch_lives_under_that_account_codex_home() {
    let home = Path::new("/accts/work");
    let touched = crate::agents::account_files_among(crate::agents::REGISTRY, "codex", home);
    assert_eq!(touched.len(), 3);
    for p in &touched {
        assert!(p.starts_with(home), "{}", p.display());
    }
    let names: Vec<_> = touched
        .iter()
        .filter_map(|p| p.file_name()?.to_str())
        .collect();
    assert_eq!(names, ["auth.json", "auth.lock", "config.toml"]);
}

#[test]
fn stop_commands_are_read_from_hooks_json() {
    let v = json!({
        "description": "mine",
        "hooks": {
            "Stop": [
                {"hooks": [{"type": "command", "command": CMD}, {"type": "prompt", "prompt": "x"}]},
                {"matcher": "*", "hooks": [{"type": "command", "command": "other"}]}
            ],
            "PreToolUse": [{"hooks": [{"type": "command", "command": "pre"}]}]
        }
    });
    assert_eq!(
        stop_hook_commands(&v),
        vec![CMD.to_string(), "other".to_string()]
    );
    assert!(stop_hook_commands(&json!({})).is_empty());
    assert!(stop_hook_commands(&json!({"hooks": {"Stop": "bad"}})).is_empty());
}

#[test]
fn the_slot_to_add_is_one_stop_group_with_that_command() {
    let slots = stop_hook_slots(CMD);
    assert_eq!(
        slots,
        vec![(
            vec!["hooks", "Stop"],
            json!({"hooks": [{"type": "command", "command": CMD}]})
        )]
    );
    // 并进去之后读得回来。
    let v = json!({"hooks": {"Stop": [slots[0].1.clone()]}});
    assert_eq!(stop_hook_commands(&v), vec![CMD.to_string()]);
}

fn hooks() -> serde_json::Value {
    json!({"hooks": {"Stop": [
        {"hooks": [{"type": "command", "command": "other"}]},
        {"hooks": [{"type": "command", "command": CMD, "timeout": 30}]}
    ]}})
}

#[test]
fn trust_is_on_only_when_the_recorded_hash_matches() {
    let file = Path::new("/accts/work/hooks.json");
    let key = "/accts/work/hooks.json:stop:1:0";
    let hash = super::super::hook_trust::trusted_hash("stop", CMD, 30);
    let trusted = format!("[hooks.state.\"{key}\"]\ntrusted_hash = \"{hash}\"\n");
    let rows = stop_hook_trust(file, &hooks(), &trusted);
    assert_eq!(rows.len(), 2);
    let ours = &rows[1];
    assert_eq!((ours.command.as_str(), ours.key.as_str()), (CMD, key));
    assert_eq!(ours.hash, hash);
    assert!(ours.trusted);
    assert!(!rows[0].trusted, "别的那一条没记信任");
    // 缺 · 不等 ⇒ 没信任；给出的那一行里就是算出的那个哈希，贴进去之后就对上。
    for config in [String::new(), trusted.replace(&hash, "sha256:00")] {
        let rows = stop_hook_trust(file, &hooks(), &config);
        assert!(!rows[1].trusted);
        assert_eq!(rows[1].line, trusted);
    }
    let pasted = format!(
        "model = \"m\"\n\n{}",
        stop_hook_trust(file, &hooks(), "")[1].line
    );
    assert!(stop_hook_trust(file, &hooks(), &pasted)[1].trusted);
}

#[test]
fn an_untimed_hook_is_hashed_with_the_default_timeout() {
    let file = Path::new("/h/.codex/hooks.json");
    let v = json!({"hooks": {"Stop": [stop_hook_slots(CMD)[0].1.clone()]}});
    let rows = stop_hook_trust(file, &v, "");
    assert_eq!(
        rows[0].hash,
        super::super::hook_trust::trusted_hash("stop", CMD, 600)
    );
    assert_eq!(rows[0].key, "/h/.codex/hooks.json:stop:0:0");
}

#[test]
fn the_face_reads_the_codex_home_files() {
    let home = Path::new("/h/.codex");
    assert_eq!(
        (FACE.user_settings)(home),
        [home.join("hooks.json"), home.join("config.toml")]
    );
    // 核心那一格只认一个 agent 家（Claude 的）⇒ 这一家不认领 `~/.codex/…`，照家目录解析。
    assert_eq!((FACE.under_agent_home)(".codex/sessions"), None);
}

#[test]
fn an_empty_command_is_skipped_but_keeps_its_place() {
    // Codex 跳过空命令那一条，后面那条的条序照旧数。
    let v = json!({"hooks": {"Stop": [{"hooks": [
        {"type": "command", "command": "  "},
        {"type": "command", "command": CMD}
    ]}]}});
    assert_eq!(stop_hook_commands(&v), vec![CMD.to_string()]);
    let rows = stop_hook_trust(Path::new("/h/hooks.json"), &v, "");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key, "/h/hooks.json:stop:0:1");
}

/// 足迹面信任那一格：照 `stop_hook_trust` 算的键与哈希并进 `config.toml` 之后读回就是那个值；已有那一格只换值、别的不动。
#[test]
fn hook_trust_merges_and_reads_back() {
    let t = FACE.hook_trust.expect("Codex 声明了信任");
    let file = Path::new("/h/hooks.json");
    let rows = (t.expected)(file, &hooks());
    let (_, key, value) = rows[0].clone();
    for base in [
        "",
        "model = \"o\"\n",
        "[hooks.state.\"x\"]\ntrusted_hash = \"y\"\n",
    ] {
        let merged = (t.merge)(base, &key, &value).unwrap();
        assert_eq!(
            (t.recorded)(&merged, &key).as_deref(),
            Some(value.as_str()),
            "{merged}"
        );
        assert!(merged.starts_with(base.trim_end()), "{merged}");
    }
    let stale = (t.merge)("", &key, "sha256:old").unwrap();
    let fixed = (t.merge)(&stale, &key, &value).unwrap();
    assert_eq!((t.recorded)(&fixed, &key).as_deref(), Some(value.as_str()));
    assert_eq!(fixed.matches("trusted_hash").count(), 1, "{fixed}");
}
