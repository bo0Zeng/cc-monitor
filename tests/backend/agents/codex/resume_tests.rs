//! Codex 起会话 · 恢复 · 会话中途那几格：起法 · 身份 · 权限档 · 模型 · 压缩与清空。
//! 期望值手写（形照 codex-rs rust-v0.167.0 的命令行与斜杠命令）；`codex debug models --bundled` 的样例是手写的小样，不是真机输出。

use super::*;
use crate::agents::codex::LAUNCH;
use crate::agents::ListedModel;

/// 起法别名的词干是 `cx`（每个号出 `<号>cx` · `<号>cxt` · `<号>cxh`），不跟 Claude 那一家的 `cc` 撞。
#[test]
fn the_launcher_alias_is_cx_not_the_claude_one() {
    assert_eq!(LAUNCH.launcher_alias, Some("cx"));
    assert_ne!(
        LAUNCH.launcher_alias,
        crate::agents::claudecode::LAUNCH.launcher_alias
    );
}

#[test]
fn launches_and_resumes_without_the_shared_daemon_with_the_first_line_as_a_positional() {
    assert_eq!(LAUNCH.launch_args, &["--no-daemon"]);
    assert_eq!(LAUNCH.resume_token, "resume");
    assert!(!LAUNCH.resume_token.starts_with("--"), "子命令形");
    assert!(LAUNCH.first_prompt_arg);
    assert_eq!(LAUNCH.default_launcher, "codex");
}

#[test]
fn identity_is_the_thread_id_and_the_four_nested_markers_are_cleared() {
    assert_eq!(LAUNCH.self_sid_env, Some("CODEX_THREAD_ID"));
    assert_eq!(
        LAUNCH.nested_env,
        &[
            "CODEX_THREAD_ID",
            "CODEX_SESSION_ID",
            "CODEX_TOOL_CALL_ID",
            "CODEX_VERSION"
        ]
    );
    assert_eq!(LAUNCH.preset_sid, None);
    assert!(LAUNCH.rewind.is_none());
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn permission() -> crate::agents::PermissionFace {
    LAUNCH.permission.expect("Codex 有权限档")
}

#[test]
fn a_selectable_mode_launches_with_the_approval_and_sandbox_pair() {
    let p = permission();
    let offered: Vec<&str> = p.modes.iter().map(|m| m.value).collect();
    assert_eq!(offered, [":read-only", ":workspace"]);
    assert_eq!(
        p.args_of(":read-only"),
        Some(strings(&["-a", "on-request", "-s", "read-only"]))
    );
    assert_eq!(
        p.args_of(":workspace"),
        Some(strings(&["-a", "on-request", "-s", "workspace-write"]))
    );
    // 越过批准的那一档只认不选：不出参数。不认的档同。
    let shown: Vec<&str> = p.shown.iter().map(|c| c.value).collect();
    assert_eq!(shown, [":danger-full-access"]);
    assert_eq!(p.args_of(":danger-full-access"), None);
    assert_eq!(p.args_of("workspace-write"), None);
    assert_eq!(p.args_of("auto"), None);
}

#[test]
fn switching_mode_mid_session_says_permissions_with_the_same_policy() {
    let p = permission();
    assert!(matches!(
        p.switch,
        Some(crate::agents::ModeSwitch::Command(_))
    ));
    assert_eq!(
        p.switch_line(":workspace").as_deref(),
        Some("/permissions :workspace --approval-policy=on-request")
    );
    assert_eq!(
        p.switch_line(":read-only").as_deref(),
        Some("/permissions :read-only --approval-policy=on-request")
    );
    assert_eq!(p.switch_line(":danger-full-access"), None);
    assert_eq!(p.switch_line("auto"), None);
}

#[test]
fn every_offered_mode_is_an_approval_and_sandbox_pair_and_its_switch_reuses_that_policy() {
    let p = permission();
    for m in p.modes {
        let ["-a", policy, "-s", sandbox] = m.args else {
            panic!("{} 的参数不是 `-a <策略> -s <沙箱>`：{:?}", m.value, m.args);
        };
        assert!(!policy.is_empty() && !sandbox.is_empty(), "{}", m.value);
        assert_eq!(
            p.switch_line(m.value),
            Some(format!(
                "/permissions {} --approval-policy={policy}",
                m.value
            )),
            "{} 换档那一句的策略与起会话那一对同出一张表",
            m.value
        );
    }
}

#[test]
fn every_mode_it_can_be_in_has_words_and_unknown_ones_have_none() {
    let p = permission();
    // 钉取字的键，不钉中文；键在文案表里真有（缺键时 `copy_text` 回〔键〕）。
    for (mode, key) in [
        (":read-only", "beSessionNew.mode.readOnly"),
        (":workspace", "beSessionNew.mode.workspace"),
        (":danger-full-access", "beSessionMode.mode.dangerFullAccess"),
    ] {
        let text = p.text_of(mode).unwrap_or_else(|| panic!("{mode} 没有字"));
        assert_eq!(
            text,
            copy_core::copy_text(key, &[]),
            "{mode} 的字取自 {key}"
        );
        assert_ne!(text, format!("〔{key}〕"), "文案表里没有 {key}");
    }
    assert_eq!(p.text_of("workspace-write"), None);
    assert_eq!(p.text_of("auto"), None);
}

#[test]
fn a_chosen_model_launches_with_dash_m() {
    let args = LAUNCH.model_args.expect("起会话选得了模型");
    assert_eq!(args("gpt-x", None), strings(&["-m", "gpt-x"]));
    // 选了推理档 ⇒ 经 `-c` 盖这一趟的 `model_reasoning_effort`（值不是合法 TOML ⇒ Codex 当字符串收）。
    assert_eq!(
        args("gpt-x", Some("high")),
        strings(&["-m", "gpt-x", "-c", "model_reasoning_effort=high"])
    );
}

/// 手写的小样：形照 `codex debug models --bundled` 打的 `ModelsResponse`（字段只留读的那几个与一两个不读的）；
/// 文件序与 `priority` 序相反，缺 `slug` · 缺 `display_name` · 不列的各一项。
const DEBUG_MODELS: &str = r#"{"models":[
 {"slug":"model-b","visibility":"list","priority":3,"supported_reasoning_levels":[],"supported_in_api":true},
 {"slug":"model-hidden","display_name":"Hidden","visibility":"hide","priority":2,"supported_reasoning_levels":[]},
 {"display_name":"No Slug","visibility":"list","priority":0,"supported_reasoning_levels":[]},
 {"slug":"model-gone","display_name":"Gone","visibility":"none","priority":0,"supported_reasoning_levels":[]},
 {"slug":"model-a","display_name":"Model A","visibility":"list","priority":1,
  "default_reasoning_level":"medium",
  "supported_reasoning_levels":[{"effort":"low","description":"d"},{"effort":"medium","description":"d"},{"effort":"xhigh","description":"d"}]}
]}
"#;

#[test]
fn the_bundled_catalog_lists_only_the_picker_models_in_priority_order() {
    let list = LAUNCH.model_list.expect("Codex 列得出模型");
    assert_eq!(list.args, &["debug", "models", "--bundled"]);
    assert_eq!(
        (list.read)(DEBUG_MODELS).expect("读得出"),
        vec![
            ListedModel {
                value: "model-a".into(),
                text: "Model A".into(),
                efforts: strings(&["low", "medium", "xhigh"]),
                default_effort: Some("medium".into()),
            },
            ListedModel {
                value: "model-b".into(),
                text: "model-b".into(),
                efforts: vec![],
                default_effort: None,
            },
        ]
    );
}

#[test]
fn a_model_without_a_priority_comes_after_those_with_one() {
    let read = LAUNCH.model_list.expect("Codex 列得出模型").read;
    let got = read(
        r#"{"models":[
 {"slug":"no-priority","display_name":"N","visibility":"list","supported_reasoning_levels":[]},
 {"slug":"late","display_name":"L","visibility":"list","priority":9,"supported_reasoning_levels":[]}
]}"#,
    )
    .expect("读得出");
    let order: Vec<&str> = got.iter().map(|m| m.value.as_str()).collect();
    assert_eq!(order, ["late", "no-priority"]);
}

#[test]
fn an_unreadable_catalog_says_nothing() {
    let read = LAUNCH.model_list.expect("Codex 列得出模型").read;
    assert_eq!(read(""), None);
    assert_eq!(read("not json"), None);
    assert_eq!(read(r#"{"data":[]}"#), None);
}

#[test]
fn switching_model_mid_session_says_model_with_an_optional_reasoning_and_reads_nothing_back() {
    let sw = LAUNCH.model_switch.expect("会话中途换得了模型");
    assert_eq!((sw.command)("model-a", None), "/model model-a");
    assert_eq!(
        (sw.command)("model-a", Some("high")),
        "/model model-a --reasoning=high"
    );
    assert!(sw.read.is_none());
    assert!(sw.saves.is_none() && sw.note.is_none() && sw.confirm.is_none());
}

#[test]
fn the_static_model_cells_are_honestly_absent() {
    assert!(LAUNCH.models.is_none());
    assert!(LAUNCH.model_default.is_none());
    assert!(LAUNCH.model_text.is_none());
    assert!(LAUNCH.model_pick.is_none());
}

#[test]
fn compact_and_clear_are_the_slash_commands() {
    assert_eq!(COMPACT_REQUEST, "/compact");
    assert_eq!(CLEAR_REQUEST, "/clear");
}
