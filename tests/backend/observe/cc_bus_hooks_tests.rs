//! # 要求住址：`设计/95 §6` 逐字「本机的钩子诊断仍是 monitor 自己读 `settings.json` …… 本机远端两条路、两个命令，与「一个能力一条命令、带 origin」（`01 §6.8`）不齐」
//!
//! 〔MIG-3b〕判定本体从 monitor `hooks_diag.rs` 原样搬来（B04 那几条形态判据随之搬家、断言不变）；
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
          "SessionStart":[{"hooks":[{"type":"command","command":"\"$HOME/.local/bin/cc-register\" >/dev/null 2>&1 || true"}]}],
          "Stop":[{"hooks":[{"type":"command","command":"\"$HOME/.local/bin/cc-bus-stop-hook\""}]}]}}"#;
    let d = diagnose(Some(raw), &always);
    assert!(
        d.session_start.is_working(),
        "实测形态必须判为已装: {:?}",
        d.session_start
    );
    assert!(d.stop.is_working(), "{:?}", d.stop);
    // 且必须是"显式路径"那一态，不能滑成 PATH 态
    assert!(matches!(d.session_start, HookState::InstalledAtPath { .. }));
}

#[test]
fn canonical_snippet_form_is_also_installed() {
    // cc-bus-install.sh 的规范片段（裸命令）同样要认
    let raw = r#"{"hooks":{
          "SessionStart":[{"hooks":[{"type":"command","command":"cc-register >/dev/null 2>&1 || true"}]}],
          "Stop":[{"hooks":[{"type":"command","command":"cc-bus-stop-hook"}]}]}}"#;
    let d = diagnose(Some(raw), &never); // never：证明裸命令**不查路径存在性**
    assert!(matches!(
        d.session_start,
        HookState::InstalledViaPath { .. }
    ));
    assert!(matches!(d.stop, HookState::InstalledViaPath { .. }));
}

// ===== 真正的第三态：看着像装了，其实指不到东西 =====
#[test]
fn explicit_path_that_does_not_exist_is_the_third_state() {
    let raw = r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command",
          "command":"/opt/gone/cc-register"}]}]}}"#;
    let d = diagnose(Some(raw), &never);
    match &d.session_start {
        HookState::PathMissing { path, .. } => assert_eq!(path, "/opt/gone/cc-register"),
        other => panic!("应为 PathMissing，实得 {other:?}"),
    }
    assert!(!d.session_start.is_working(), "PathMissing 绝不能算能用");
}

#[test]
fn same_name_elsewhere_but_present_counts_as_installed() {
    // 装在别处但**确实存在** → 算已装（它能跑）。只有不存在才是问题。
    let raw = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command",
          "command":"/usr/local/bin/cc-bus-stop-hook"}]}]}}"#;
    let d = diagnose(Some(raw), &always);
    assert!(d.stop.is_working());
}

// ===== program_of：只做够用的事，看不懂就说不知道 =====
#[test]
fn program_of_handles_real_shapes() {
    assert_eq!(program_of("cc-register").unwrap().0, "cc-register");
    assert_eq!(
        program_of("\"$HOME/.local/bin/cc-register\" >/dev/null 2>&1 || true").unwrap(),
        ("cc-register".into(), "$HOME/.local/bin/cc-register".into())
    );
    // 前导环境赋值要剥掉
    assert_eq!(
        program_of("FOO=1 BAR=2 cc-register").unwrap().0,
        "cc-register"
    );
    // `=` 在首位不是赋值
    assert_eq!(program_of("=weird").unwrap().0, "=weird");
    assert_eq!(program_of("   "), None);
    assert_eq!(program_of(""), None);
    assert_eq!(program_of("''"), None);
}

#[test]
fn unrelated_hooks_do_not_false_positive() {
    let raw = r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command",
          "command":"some-other-tool --register"}]}]}}"#;
    let d = diagnose(Some(raw), &always);
    assert_eq!(
        d.session_start,
        HookState::NotInstalled,
        "别的工具的钩子不得算成 cc-bus 的"
    );
}

#[test]
fn coexisting_hooks_do_not_require_exclusivity() {
    // 同一事件挂多条：别的工具在前、cc-bus 在后，仍算已装
    let raw = r#"{"hooks":{"SessionStart":[
          {"hooks":[{"type":"command","command":"other-tool"}]},
          {"hooks":[{"type":"command","command":"cc-register"}]}]}}"#;
    assert!(diagnose(Some(raw), &always).session_start.is_working());
}

#[test]
fn working_entry_wins_over_path_missing() {
    // 一条坏的 + 一条好的 → 报好的（用户实际能用）
    let raw = r#"{"hooks":{"SessionStart":[
          {"hooks":[{"type":"command","command":"/gone/cc-register"}]},
          {"hooks":[{"type":"command","command":"cc-register"}]}]}}"#;
    let d = diagnose(Some(raw), &never);
    assert!(matches!(
        d.session_start,
        HookState::InstalledViaPath { .. }
    ));
}

// ===== 脏输入逐层容忍，不抛、不让坏条目吃掉整份诊断 =====
#[test]
fn malformed_input_degrades_with_a_reason() {
    assert!(diagnose(None, &always).note.contains("没读到"));
    assert!(diagnose(Some("{not json"), &always)
        .note
        .contains("不是合法 JSON"));
    assert!(diagnose(Some("[1,2]"), &always)
        .note
        .contains("顶层不是对象"));
    // 结构不对的各层：一律降级成未装，且不 panic
    for raw in [
        r#"{}"#,
        r#"{"hooks":null}"#,
        r#"{"hooks":{"SessionStart":"notarray"}}"#,
        r#"{"hooks":{"SessionStart":[{"nohooks":1}]}}"#,
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command"}]}]}}"#,
        r#"{"hooks":{"SessionStart":[{"hooks":[{"command":"   "}]}]}}"#,
    ] {
        let d = diagnose(Some(raw), &always);
        assert_eq!(d.session_start, HookState::NotInstalled, "raw={raw}");
        assert!(d.note.is_empty(), "结构问题不该报成读取失败: raw={raw}");
    }
}

#[test]
fn bom_is_tolerated() {
    let raw = "\u{feff}{\"hooks\":{\"Stop\":[{\"hooks\":[{\"command\":\"cc-bus-stop-hook\"}]}]}}";
    assert!(diagnose(Some(raw), &always).stop.is_working());
}

/// 一切正常的探测（两种形态都不该有 warning）。
fn ok_probe() -> SnippetProbe {
    SnippetProbe {
        home_path_exists: Some(true),
        on_path: Some(true),
    }
}

#[test]
fn snippet_is_valid_json_and_round_trips() {
    for home in [true, false] {
        let sn = snippet(home, &ok_probe());
        let v: serde_json::Value =
            serde_json::from_str(&sn.text).expect("生成的片段必须是合法 JSON");
        // 把自己生成的东西再喂给自己的诊断——闭环，防止生成一段自己都不认的文本
        let d = diagnose(Some(&sn.text), &always);
        assert!(
            d.session_start.is_working(),
            "home={home} 生成的片段自己都不认: {}",
            sn.text
        );
        assert!(d.stop.is_working(), "home={home}");
        assert!(v.get("hooks").is_some());
        assert!(sn.warning.is_none(), "探测一切正常时不该有警示");
    }
    assert!(snippet(true, &ok_probe())
        .text
        .contains("$HOME/.local/bin/"));
    assert!(!snippet(false, &ok_probe()).text.contains("$HOME"));
}

/// **上一版 `snippet(home: bool)` 只按布尔选形态，完全不看实况**：于是面板可以推荐
/// `$HOME/.local/bin/cc-register` 而那个文件根本不存在，贴上去就是一个 `path-missing`
/// 的钩子，而这一步没有任何测试能发现。这条测试就是那个缺口。
#[test]
fn home_form_warns_when_the_path_is_not_on_disk() {
    let probe = SnippetProbe {
        home_path_exists: Some(false),
        on_path: Some(true),
    };
    let sn = snippet(true, &probe);
    let w = sn.warning.expect("显式路径形态 + 路径不存在 → 必须警示");
    assert!(w.contains("$HOME/.local/bin/"), "要指名那个路径：{w}");
    // 〔CP2b〕原来靠诊断内部码 path-missing 说后果；CP1 台账裁掉内部码，后果改用人话「用不了」。
    assert!(w.contains("用不了"), "要说清后果：{w}");
    // **闭环验证后果是真的**：把这段喂回自己的诊断，`exists` 说不存在 → 真的 PathMissing
    let d = diagnose(Some(&sn.text), &|_| false);
    assert!(
        matches!(d.session_start, HookState::PathMissing { .. }),
        "警示说的后果得是真的，实得 {:?}",
        d.session_start
    );
    // 同一份探测下裸命令形态没问题 → 不该警示（否则两种形态都报警，用户无从选择）
    assert!(snippet(false, &probe).warning.is_none());
}

#[test]
fn bare_form_warns_when_not_on_path() {
    let probe = SnippetProbe {
        home_path_exists: Some(true),
        on_path: Some(false),
    };
    let w = snippet(false, &probe)
        .warning
        .expect("裸命令形态 + 不在 PATH → 必须警示");
    assert!(w.contains("PATH"), "{w}");
    assert!(snippet(true, &probe).warning.is_none());
}

/// **取不到就别猜**：`on_path: None` 时裸命令形态不许警示
/// （报一个我们并不知道的问题，和漏报一样是失信）。
#[test]
fn unknown_path_status_does_not_fabricate_a_warning() {
    let probe = SnippetProbe {
        home_path_exists: Some(true),
        on_path: None,
    };
    assert!(snippet(false, &probe).warning.is_none());
    assert!(snippet(true, &probe).warning.is_none());
}

/// **按平台构造 PATH**（T03 审计阻塞 1）。旧版这条测试硬编码 `"/usr/bin:/opt/bin"`，
/// 锁死的是 Unix 语义——于是 `split(':')` 这个在**生产平台 Windows 上算错**的实现
/// 被它钉成了绿的。现在用 `std::env::join_paths` 按当前平台拼，Linux 与 Windows 同一条测试。
#[test]
fn resolves_on_path_uses_the_injected_exists() {
    let sep_dir = if cfg!(windows) {
        "C:\\opt\\bin"
    } else {
        "/opt/bin"
    };
    let other = if cfg!(windows) {
        "C:\\Windows"
    } else {
        "/usr/bin"
    };
    let join = |dirs: &[&str]| -> String {
        std::env::join_paths(dirs.iter().map(std::path::Path::new))
            .unwrap()
            .to_string_lossy()
            .into_owned()
    };
    let want = format!("{}/cc-register", sep_dir.trim_end_matches(['/', '\\']));
    let ex = |s: &str| s == want;
    assert_eq!(
        resolves_on_path("cc-register", Some(&join(&[other, sep_dir])), &ex),
        Some(true),
        "PATH 必须按当前平台的分隔符切"
    );
    assert_eq!(
        resolves_on_path("cc-register", Some(&join(&[other])), &ex),
        Some(false)
    );
    // **取不到 PATH → None，不猜 false**
    assert_eq!(resolves_on_path("cc-register", None, &ex), None);
    assert_eq!(resolves_on_path("cc-register", Some("   "), &ex), None);
}

/// 两个字段现在**同一档口径**：`None` 一律不警示。
#[test]
fn unknown_home_path_does_not_fabricate_a_warning() {
    let probe = SnippetProbe {
        home_path_exists: None,
        on_path: None,
    };
    assert!(snippet(true, &probe).warning.is_none());
    assert!(snippet(false, &probe).warning.is_none());
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
        unquote_once("\"$HOME/.local/bin/cc-register\""),
        "$HOME/.local/bin/cc-register"
    );
    // 走到 program_of 上：不配对引号不该被当成正常路径
    let (base, full) = program_of("\"$HOME/.local/bin/cc-register").unwrap();
    assert_eq!(base, "cc-register");
    assert_eq!(
        full, "\"$HOME/.local/bin/cc-register",
        "不配对的引号要留着，让下游看到这条命令形状可疑"
    );
}

/// **B04-4**：包装器写法必须判「无法判断」，不得判「未装」。
/// 源码注释写着"看不懂就返回 None 而不是猜"，但 None 落到 NotInstalled → UI 渲染成
/// 确定性的"未装" → 用户去贴一份重复的钩子。**猜"未装"和猜"已装"一样是猜。**
#[test]
fn wrapper_forms_are_unknown_not_not_installed() {
    for cmd in [
        r#"sh -c "cc-register""#,
        "bash -lc cc-register",
        "exec cc-register",
        "command cc-register",
        "/usr/bin/env cc-register",
        "timeout 5 cc-register",
        "nohup cc-register &",
        "true && cc-register",
        r#""$HOME/.local/bin/cc-register"; echo hi"#,
    ] {
        let st = classify_command(cmd, "cc-register", &always);
        match st {
            Some(HookState::Unknown { .. }) => {}
            // 直接执行也可以（说明 program_of 认出来了，更好）
            Some(s) if s.is_working() => {}
            other => panic!("{cmd:?} 不该被判成 {other:?}——那会让用户以为没装"),
        }
    }
    // 真的没提到目标 → 仍然是 NotInstalled
    assert_eq!(
        classify_command("other-tool --register", "cc-register", &always),
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
        expanded == std::path::PathBuf::from("/home/u/.local/bin/cc-register")
    };
    for form in [
        "$HOME/.local/bin/cc-register",
        "${HOME}/.local/bin/cc-register",
        "~/.local/bin/cc-register",
    ] {
        let st = classify_command(form, "cc-register", &exists).unwrap();
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

/// ★ 成品两侧对拍：一台造好的机器（显式路径那一格在盘上 · 裸命令那一格 `PATH` 上没有）⇒ 整份成品 == 金样。
/// 金样同时是界面解码器（`src/frontend/ui/settings/cc-bus-hooks-section.ts::decodeHooksReport`）的输入。
#[test]
fn the_product_is_the_golden_the_ui_decodes() {
    let home = scratch("golden");
    let agent = home.join(".claude");
    let bin = home.join(".local").join("bin");
    std::fs::create_dir_all(&agent).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    for p in PROGRAMS {
        std::fs::write(bin.join(p), b"").unwrap();
    }
    std::fs::write(
        agent.join("settings.json"),
        r#"{"hooks":{
          "SessionStart":[{"hooks":[{"type":"command","command":"\"$HOME/.local/bin/cc-register\" >/dev/null 2>&1 || true"}]}],
          "Stop":[{"hooks":[{"type":"command","command":"sh -c cc-bus-stop-hook"}]}]}}"#,
    )
    .unwrap();
    let empty_path_dir = home.join("nothing-here");
    std::fs::create_dir_all(&empty_path_dir).unwrap();
    let path_env = std::env::join_paths([&empty_path_dir]).unwrap();
    let rep = answer_at(Some(&home), &agent, path_env.to_str());
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

/// 读不到那份文件 ⇒ `note` 说出来、两态「未装」，来源照报（不装作读过）。
#[test]
fn a_machine_without_settings_says_so() {
    let home = scratch("none");
    let agent = home.join(".claude");
    let rep = answer_at(Some(&home), &agent, None);
    assert!(rep.diagnosis.note.contains("没读到"), "{:?}", rep.diagnosis);
    assert_eq!(rep.diagnosis.session_start, HookState::NotInstalled);
    assert!(rep.source.ends_with("settings.json"));
    assert_eq!(rep.snippet_bare.warning, None, "PATH 取不到 ⇒ 不警示");
    let _ = std::fs::remove_dir_all(&home);
}
