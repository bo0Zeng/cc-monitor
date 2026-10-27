//! # 要求：「本机的钩子诊断仍是 monitor 自己读 `settings.json` …… 本机远端两条路、两个命令，与「一个能力一条命令、带 origin」不齐」
//!
//! 判定本体从 monitor `hooks_diag.rs` 原样搬来（B04 那几条形态判据随之搬家、断言不变）；
//! 新的一条是成品的跨语言金样（后端产出 == `tests/__fixtures__/hooks-diag.golden.json`，界面解码器读同一份）。
//! 夹具只造结构（假 `settings.json` ＋ 空文件当程序），不采真盘内容。

use super::*;

fn always(_: &str) -> bool {
    true
}
fn never(_: &str) -> bool {
    false
}

// ===== 核心：用户盘上的**真实**形态不得被误判 =====
#[test]
fn real_user_settings_is_installed_not_misreported() {
    // 逐字取自 2026-07-28 的 ~/.claude/settings.json
    let raw = r#"{"hooks":{
          "Stop":[{"hooks":[{"type":"command","command":"\"$HOME/.local/bin/cc-bus\" stop-hook"}]}]}}"#;
    let d = diagnose(Some(raw), &always);
    assert!(d.stop.is_working(), "实测形态必须判为已装: {:?}", d.stop);
    // 且必须是"显式路径"那一态，不能滑成 PATH 态
    assert!(matches!(d.stop, HookState::InstalledAtPath { .. }));
}

#[test]
fn canonical_snippet_form_is_also_installed() {
    // cc-bus-install.sh 的规范片段（裸命令）同样要认
    let raw = r#"{"hooks":{
          "Stop":[{"hooks":[{"type":"command","command":"cc-bus stop-hook"}]}]}}"#;
    let d = diagnose(Some(raw), &never); // never：证明裸命令**不查路径存在性**
    assert!(matches!(d.stop, HookState::InstalledViaPath { .. }));
}

// ===== 真正的第三态：看着像装了，其实指不到东西 =====
#[test]
fn explicit_path_that_does_not_exist_is_the_third_state() {
    let raw = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command",
          "command":"/opt/gone/cc-bus stop-hook"}]}]}}"#;
    let d = diagnose(Some(raw), &never);
    match &d.stop {
        HookState::PathMissing { path, .. } => assert_eq!(path, "/opt/gone/cc-bus"),
        other => panic!("应为 PathMissing，实得 {other:?}"),
    }
    assert!(!d.stop.is_working(), "PathMissing 绝不能算能用");
}

#[test]
fn same_name_elsewhere_but_present_counts_as_installed() {
    // 装在别处但**确实存在** → 算已装（它能跑）。只有不存在才是问题。
    let raw = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command",
          "command":"/usr/local/bin/cc-bus stop-hook"}]}]}}"#;
    let d = diagnose(Some(raw), &always);
    assert!(d.stop.is_working());
}

/// ★ 一个入口：程序是 `cc-bus`、第一个参数是 `stop-hook` 才算；老版的独立脚本 `cc-bus-stop-hook`（老名字不留）·
/// 只有入口没有动词 · 别的动词都不算装了这条钩子。
#[test]
fn only_the_entry_with_the_stop_hook_verb_counts() {
    for cmd in [
        "cc-bus-stop-hook",
        "\"$HOME/.local/bin/cc-bus-stop-hook\"",
        "cc-bus",
        "cc-bus recv",
    ] {
        let raw = serde_json::json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": cmd}]}]}}).to_string();
        assert_eq!(
            diagnose(Some(&raw), &always).stop,
            HookState::NotInstalled,
            "{cmd:?}"
        );
    }
    let raw = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"cc-bus stop-hook"}]}]}}"#;
    assert!(diagnose(Some(raw), &always).stop.is_working());
}

// ===== program_of：只做够用的事，看不懂就说不知道 =====
#[test]
fn program_of_handles_real_shapes() {
    assert_eq!(program_of("cc-bus stop-hook").unwrap().0, "cc-bus");
    assert_eq!(
        program_of("\"$HOME/.local/bin/cc-bus\" stop-hook >/dev/null 2>&1 || true").unwrap(),
        ("cc-bus".into(), "$HOME/.local/bin/cc-bus".into())
    );
    assert_eq!(
        first_arg_of("\"$HOME/.local/bin/cc-bus\" stop-hook >/dev/null 2>&1 || true").as_deref(),
        Some("stop-hook")
    );
    // 前导环境赋值要剥掉
    assert_eq!(
        program_of("FOO=1 BAR=2 cc-bus stop-hook").unwrap().0,
        "cc-bus"
    );
    assert_eq!(
        first_arg_of("FOO=1 BAR=2 cc-bus stop-hook").as_deref(),
        Some("stop-hook")
    );
    assert_eq!(first_arg_of("cc-bus"), None);
    // `=` 在首位不是赋值
    assert_eq!(program_of("=weird").unwrap().0, "=weird");
    assert_eq!(program_of("   "), None);
    assert_eq!(program_of(""), None);
    assert_eq!(program_of("''"), None);
}

#[test]
fn unrelated_hooks_do_not_false_positive() {
    let raw = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command",
          "command":"some-other-tool --register"}]}]}}"#;
    let d = diagnose(Some(raw), &always);
    assert_eq!(
        d.stop,
        HookState::NotInstalled,
        "别的工具的钩子不得算成 cc-bus 的"
    );
}

#[test]
fn coexisting_hooks_do_not_require_exclusivity() {
    // 同一事件挂多条：别的工具在前、cc-bus 在后，仍算已装
    let raw = r#"{"hooks":{"Stop":[
          {"hooks":[{"type":"command","command":"other-tool"}]},
          {"hooks":[{"type":"command","command":"cc-bus stop-hook"}]}]}}"#;
    assert!(diagnose(Some(raw), &always).stop.is_working());
}

#[test]
fn working_entry_wins_over_path_missing() {
    // 一条坏的 + 一条好的 → 报好的（用户实际能用）
    let raw = r#"{"hooks":{"Stop":[
          {"hooks":[{"type":"command","command":"/gone/cc-bus stop-hook"}]},
          {"hooks":[{"type":"command","command":"cc-bus stop-hook"}]}]}}"#;
    let d = diagnose(Some(raw), &never);
    assert!(matches!(d.stop, HookState::InstalledViaPath { .. }));
}

// ===== 脏输入逐层容忍，不抛、不让坏条目吃掉整份诊断 =====
#[test]
fn malformed_input_degrades_with_a_reason() {
    assert!(diagnose(None, &always)
        .note
        .contains(copy_core::copy_static!("rsHooksDiag.diagnose.noSettings")));
    assert!(diagnose(Some("{not json"), &always)
        .note
        .contains(copy_core::copy_static!("rsHooksDiag.diagnose.badJson")));
    assert!(diagnose(Some("[1,2]"), &always)
        .note
        .contains(copy_core::copy_static!("rsHooksDiag.diagnose.notObject")));
    // 结构不对的各层：一律降级成未装，且不 panic
    for raw in [
        r#"{}"#,
        r#"{"hooks":null}"#,
        r#"{"hooks":{"Stop":"notarray"}}"#,
        r#"{"hooks":{"Stop":[{"nohooks":1}]}}"#,
        r#"{"hooks":{"Stop":[{"hooks":[{"type":"command"}]}]}}"#,
        r#"{"hooks":{"Stop":[{"hooks":[{"command":"   "}]}]}}"#,
    ] {
        let d = diagnose(Some(raw), &always);
        assert_eq!(d.stop, HookState::NotInstalled, "raw={raw}");
        assert!(d.note.is_empty(), "结构问题不该报成读取失败: raw={raw}");
    }
}

#[test]
fn bom_is_tolerated() {
    let raw = "\u{feff}{\"hooks\":{\"Stop\":[{\"hooks\":[{\"command\":\"cc-bus stop-hook\"}]}]}}";
    assert!(diagnose(Some(raw), &always).stop.is_working());
}

/// 在 `<skills>/cc-bus/scripts/` 放上入口（空文件，只造结构）。
fn put_scripts(skills: &std::path::Path) {
    let dir = skills.join("cc-bus").join("scripts");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(PROGRAM), b"").unwrap();
}

/// ★ 要加的内容只有一形：一条 Stop 钩子直接指向这台 skills 根下 cc-bus 里那个脚本（家目录底下 ⇒ `"$HOME/…"`，否则绝对路径），
/// 不依赖 `PATH`、也不依赖别处的链接；合法 JSON，喂回自己的诊断算「装了」（不生成一段自己都不认的文本）；
/// 那个脚本不在（cc-bus 没装）⇒ 不给。
#[test]
fn the_snippet_points_at_the_installed_scripts_and_reads_back_as_installed() {
    let home = scratch("snippet");
    let skills = home.join(".claude").join("skills");
    assert_eq!(snippet(&skills, Some(&home)), None, "cc-bus 没装 ⇒ 不给");
    put_scripts(&skills);
    let text = snippet(&skills, Some(&home)).expect("装着就该给");
    let v: serde_json::Value = serde_json::from_str(&text).expect("要加的内容必须是合法 JSON");
    assert_eq!(
        v["hooks"]
            .as_object()
            .map(|o| o.keys().cloned().collect::<Vec<_>>()),
        Some(vec!["Stop".to_string()]),
        "要加的只有 Stop 那一条（会话 id 由 cc-monitor 认，起来时不用登记）"
    );
    assert_eq!(
        v["hooks"]["Stop"][0]["hooks"][0]["command"],
        serde_json::json!("\"$HOME/.claude/skills/cc-bus/scripts/cc-bus\" stop-hook")
    );
    let exists = |s: &str| expand(s, Some(&home)).exists();
    let d = diagnose(Some(&text), &exists);
    assert!(
        matches!(d.stop, HookState::InstalledAtPath { .. }),
        "{:?}",
        d.stop
    );
    // skills 根不在家目录底下 ⇒ 绝对路径（POSIX 单引号）。
    let other = scratch("snippet-elsewhere");
    let text = snippet(&skills, Some(&other)).unwrap();
    let abs = skills.join("cc-bus/scripts/cc-bus");
    assert!(
        text.contains(&shell_quote_core::posix_quote(&abs.display().to_string())),
        "{text}"
    );
    // 脚本不在了就不给。
    std::fs::remove_file(skills.join("cc-bus/scripts/cc-bus")).unwrap();
    assert_eq!(snippet(&skills, Some(&home)), None);
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&other);
}

/// **B04 登记项②**：`trim_matches` 逐字符两端剥，会把不配对的也剥掉。
#[test]
fn unquote_only_strips_a_matched_pair() {
    assert_eq!(unquote_once("\"x\""), "x");
    assert_eq!(unquote_once("'x'"), "x");
    // 不配对 → 原样返回（旧的 trim_matches 会剥成 `a`）
    assert_eq!(unquote_once("\"a'"), "\"a'");
    assert_eq!(unquote_once("\"a"), "\"a");
    assert_eq!(unquote_once("a\""), "a\"");
    // 只剥**一层**（旧的会把 `''x''` 剥干净）
    assert_eq!(unquote_once("''x''"), "'x'");
    assert_eq!(unquote_once(""), "");
    assert_eq!(unquote_once("\""), "\"");
    // 真实形态照旧
    assert_eq!(
        unquote_once("\"$HOME/.local/bin/cc-bus\""),
        "$HOME/.local/bin/cc-bus"
    );
    // 走到 program_of 上：不配对引号不该被当成正常路径
    let (base, full) = program_of("\"$HOME/.local/bin/cc-bus stop-hook").unwrap();
    assert_eq!(base, "cc-bus");
    assert_eq!(
        full, "\"$HOME/.local/bin/cc-bus",
        "不配对的引号要留着，让下游看到这条命令形状可疑"
    );
}

/// **B04-4**：包装器写法必须判「无法判断」，不得判「未装」。
/// 源码注释写着"看不懂就返回 None 而不是猜"，但 None 落到 NotInstalled → UI 渲染成
/// 确定性的"未装" → 用户去贴一份重复的钩子。**猜"未装"和猜"已装"一样是猜。**
#[test]
fn wrapper_forms_are_unknown_not_not_installed() {
    for cmd in [
        r#"sh -c "cc-bus stop-hook""#,
        "bash -lc 'cc-bus stop-hook'",
        "exec cc-bus stop-hook",
        "command cc-bus stop-hook",
        "/usr/bin/env cc-bus stop-hook",
        "timeout 5 cc-bus stop-hook",
        "nohup cc-bus stop-hook &",
        "true && cc-bus stop-hook",
        r#""$HOME/.local/bin/cc-bus" stop-hook; echo hi"#,
    ] {
        let st = classify_command(cmd, (PROGRAM, VERB), &always);
        match st {
            Some(HookState::Unknown { .. }) => {}
            // 直接执行也可以（说明 program_of 认出来了，更好）
            Some(s) if s.is_working() => {}
            other => panic!("{cmd:?} 不该被判成 {other:?}——那会让用户以为没装"),
        }
    }
    // 真的没提到目标 → 仍然是 NotInstalled
    assert_eq!(
        classify_command("other-tool --register", (PROGRAM, VERB), &always),
        None
    );
}

/// **B04-3**：`${HOME}/` 花括号形态此前被判成「装了但路径不存在」——
/// 正是本模块文档头声称要避免的那件事，只是换了个花括号写法就重现了。
#[test]
fn brace_home_form_is_not_a_false_alarm() {
    // exists 闭包按真实实现的展开逻辑：认 $HOME/ ${HOME}/ ~/
    let home = std::path::PathBuf::from("/home/u");
    let exists = move |s: &str| -> bool {
        let expanded = if let Some(rest) = s
            .strip_prefix("$HOME/")
            .or_else(|| s.strip_prefix("${HOME}/"))
            .or_else(|| s.strip_prefix("~/"))
        {
            home.join(rest)
        } else {
            std::path::PathBuf::from(s)
        };
        // 模拟"这个路径存在"
        expanded == std::path::PathBuf::from("/home/u/.local/bin/cc-bus")
    };
    for form in [
        "$HOME/.local/bin/cc-bus stop-hook",
        "${HOME}/.local/bin/cc-bus stop-hook",
        "~/.local/bin/cc-bus stop-hook",
    ] {
        let st = classify_command(form, (PROGRAM, VERB), &exists).unwrap();
        assert!(
            matches!(st, HookState::InstalledAtPath { .. }),
            "{form} 应判为已装，实得 {st:?}"
        );
    }
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("mig3b-hooks-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 金样里那台机器的家目录占位（夹具建在临时目录，出金样前把它换成这个）。
const HOME_MARK: &str = "/HOME";

/// ★ 成品两侧对拍：一台造好的机器（cc-bus 装着 · Stop 钩子是显式路径且在盘上）⇒ 整份成品 == 金样。
/// 金样同时是界面解码器（`src/frontend/ui/cc-bus-hooks-reads.ts::decodeHooksReport`）的输入。
#[test]
fn the_product_is_the_golden_the_ui_decodes() {
    let home = scratch("golden");
    let agent = home.join(".claude");
    let bin = home.join(".local").join("bin");
    std::fs::create_dir_all(&agent).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join(PROGRAM), b"").unwrap();
    std::fs::write(
        agent.join("settings.json"),
        r#"{"hooks":{
          "Stop":[{"hooks":[{"type":"command","command":"\"$HOME/.local/bin/cc-bus\" stop-hook"}]}]}}"#,
    )
    .unwrap();
    let skills = agent.join("skills");
    put_scripts(&skills);
    let rep = answer_at(Some(&home), &agent, Some(&skills), true);
    let got = serde_json::to_string_pretty(&rep)
        .unwrap()
        .replace(&home.display().to_string(), HOME_MARK);
    let want = include_str!("../../__fixtures__/hooks-diag.golden.json");
    assert_eq!(
        got.trim(),
        want.trim(),
        "成品形状变了 —— 界面解码器读的是同一份金样，两边要一起改"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// 读不到那份文件 ⇒ `note` 说出来、「未装」，来源照报（不装作读过）。
#[test]
fn a_machine_without_settings_says_so() {
    let home = scratch("none");
    let agent = home.join(".claude");
    let rep = answer_at(Some(&home), &agent, Some(&agent.join("skills")), true);
    assert!(
        rep.diagnosis
            .note
            .contains(copy_core::copy_static!("rsHooksDiag.diagnose.noSettings")),
        "{:?}",
        rep.diagnosis
    );
    assert_eq!(rep.diagnosis.stop, HookState::NotInstalled);
    assert!(rep.source.ends_with("settings.json"));
    assert_eq!(rep.snippet, None, "cc-bus 没装 ⇒ 不给要加的内容");
    let _ = std::fs::remove_dir_all(&home);
}

/// 跑不了 cc-bus 的那台（没有原生 tmux）：成品说「不支持」、不给要加的内容 —— 哪怕 skill 目录里那个脚本在。
#[test]
fn a_machine_without_tmux_says_it_cannot_receive() {
    let home = scratch("no-tmux");
    let agent = home.join(".claude");
    let skills = agent.join("skills");
    put_scripts(&skills);
    let rep = answer_at(Some(&home), &agent, Some(&skills), false);
    assert!(!rep.supported);
    assert_eq!(rep.snippet, None, "跑不了还给了要加的内容");
    assert!(
        answer_at(Some(&home), &agent, Some(&skills), true)
            .snippet
            .is_some(),
        "正控：同一台换成跑得了就给"
    );
    let _ = std::fs::remove_dir_all(&home);
}
