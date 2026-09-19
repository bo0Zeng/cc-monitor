use super::*;

fn always(_: &str) -> bool {
    true
}
fn never(_: &str) -> bool {
    false
}

// ===== 该读哪个 settings.json（B04 登记项：尊重 CLAUDE_CONFIG_DIR）=====
#[test]
fn settings_path_honors_claude_config_dir() {
    let home = std::path::Path::new("/home/u");
    let yes = |_: &std::path::Path| true;
    let no = |_: &std::path::Path| false;

    // 未设 → 回落 ~/.claude
    assert_eq!(
        settings_path(None, home, &yes),
        std::path::PathBuf::from("/home/u/.claude/settings.json")
    );
    // 设了且是目录 → 用它。**这是本机实况**：CLAUDE_CONFIG_DIR=~/.claude-accts/z
    let acct = std::path::Path::new("/home/u/.claude-accts/z");
    assert_eq!(
        settings_path(Some(acct), home, &yes),
        std::path::PathBuf::from("/home/u/.claude-accts/z/settings.json")
    );
    // 设了但不是目录（已删 / 指向文件）→ 回落，而不是读一个不存在的路径
    assert_eq!(
        settings_path(Some(acct), home, &no),
        std::path::PathBuf::from("/home/u/.claude/settings.json")
    );
}

/// 旧写法恒读 `~/.claude/settings.json`。本机之所以**恰好**没出错，是因为
/// cc-acct-iso 把账号库的 settings.json 软链回了那里——**巧合不是保证**。
/// 这条断言的是"我们确实按 CLAUDE_CONFIG_DIR 走了"，而不是"结果碰巧一样"。
#[test]
fn config_dir_is_not_ignored_even_when_symlinked_to_the_same_place() {
    let home = std::path::Path::new("/home/u");
    let acct = std::path::Path::new("/home/u/.claude-accts/z");
    let got = settings_path(Some(acct), home, &|_| true);
    assert!(
        got.starts_with(acct),
        "必须读 CLAUDE_CONFIG_DIR 下那份（哪怕它软链到别处），实得 {got:?}"
    );
    assert_ne!(
        got,
        std::path::PathBuf::from("/home/u/.claude/settings.json")
    );
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
    let raw =
        "\u{feff}{\"hooks\":{\"Stop\":[{\"hooks\":[{\"command\":\"cc-bus-stop-hook\"}]}]}}";
    assert!(diagnose(Some(raw), &always).stop.is_working());
}

// ===== 生成的待贴文本必须是合法 JSON，且两种形态都对 =====

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

// ===== T03 收的 B04 登记项：片段必须看盘上实况 =====

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
    assert!(w.contains("path-missing"), "要说清后果：{w}");
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

/// **结构性守卫：PATH 切分必须用平台自己的切分器，不许写死分隔符。**
///
/// 为什么不是行为测试：`std::env::split_paths` **本身就是平台相关的**
/// （Linux 上按 `':'`、Windows 上按 `';'`），所以"喂一段 Windows 形态的 PATH，
/// 断言它不被按 `':'` 切"这个性质在 Linux 上**根本不成立**——我第一版就是这么写的，
/// 当场红在 `把盘符当目录了：C/cc-register | \Windows;C/cc-register | …`。
/// 那不是实现的错，是我把一个平台相关的行为断言成了平台无关的。
///
/// 真正想守的是**源码性质**：这里必须调 `std::env::split_paths`，
/// 不许出现写死的 `split(':')`。行为侧由上面那条 `join_paths` 测试覆盖——
/// 它在 CI 的 `windows-latest` job 上跑的就是 Windows 语义，那才是真覆盖。
#[test]
fn path_splitting_delegates_to_the_platform() {
    let src = include_str!("../../src/bridge/src/hooks_diag.rs");
    let body_start = src
        .find("pub fn resolves_on_path(")
        .expect("找不到 resolves_on_path——守卫失效了");
    let body_end = src[body_start..]
        .find("\n}\n")
        .map(|i| body_start + i)
        .expect("取不到函数体");
    let body: String = src[body_start..body_end]
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    // 反向自检：剥完还看得见真代码
    assert!(body.contains("path_env?"), "剥过头了，守卫在空转");
    assert!(
        body.contains("std::env::split_paths"),
        "PATH 切分必须交给平台，实得:\n{body}"
    );
    for bad in ["split(':')", "split(';')", "split(\":\")"] {
        assert!(
            !body.contains(bad),
            "不许写死分隔符 {bad}——生产平台是 Windows（ci.yml/release.yml 都是 windows-latest）"
        );
    }
}

/// **远端探测解析**（T03 审计阻塞 3）。此前这段藏在 `#[tauri::command] async fn` 里，
/// 要一条真 ssh 才走得到 = 不可测 = 没门禁；审计实测把远端 `home_path_exists` 改成
/// `= true`，24 项照样全绿。
#[test]
fn remote_probe_uses_each_kind_of_evidence_precisely() {
    // 两个程序都在 $HOME/.local/bin 下 `-x` 命中 + 都在 PATH 上
    let (lenient, p) = parse_remote_probe(
        "X\t/home/u/.local/bin/cc-register\nX\t/home/u/.local/bin/cc-bus-stop-hook\n\
             P\t/home/u/.local/bin/cc-register\nP\t/home/u/.local/bin/cc-bus-stop-hook\n",
    );
    assert_eq!(p.home_path_exists, Some(true));
    assert_eq!(p.on_path, Some(true));
    assert_eq!(lenient.len(), 4, "宽容清单要含全部命中（B04-7 的决定不变）");

    // **审计指出的真实假阴性**：只装在 /usr/local/bin 且在 PATH 上——
    // 旧代码按 basename 匹配会把它算成 `$HOME` 路径存在 → `$HOME` 形态不警示 →
    // 用户贴上去正是一个 path-missing 钩子。
    let (_, p) = parse_remote_probe(
        "P\t/usr/local/bin/cc-register\nP\t/usr/local/bin/cc-bus-stop-hook\n",
    );
    assert_eq!(
        p.home_path_exists,
        Some(false),
        "PATH 上有 ≠ $HOME/.local/bin 下有"
    );
    assert_eq!(p.on_path, Some(true));

    // 只有一个命中 → 不能说"都在"
    let (_, p) = parse_remote_probe("X\t/home/u/.local/bin/cc-register\n");
    assert_eq!(p.home_path_exists, Some(false));

    // 新协议但一个都没找到 → 确定地说"都不在"
    let (lenient, p) = parse_remote_probe("");
    assert_eq!(p.home_path_exists, Some(false));
    assert_eq!(p.on_path, Some(false));
    assert!(lenient.is_empty());

    // **旧协议（不打标记）→ 两项都说不知道**，不拿含混回报当精确证据
    let (lenient, p) = parse_remote_probe("/home/u/.local/bin/cc-register\n");
    assert_eq!(p.home_path_exists, None, "旧协议不许下结论");
    assert_eq!(p.on_path, None);
    assert_eq!(lenient.len(), 1, "但宽容清单仍要用它（诊断照旧宽容）");
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

// ===== B04 审计逼出来的补漏 =====

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

#[test]
fn unknown_is_not_working_but_is_flagged_unknown() {
    let st = classify_command("sh -c cc-register", "cc-register", &always).unwrap();
    assert!(!st.is_working(), "无法判断不能算能用");
    assert!(st.is_unknown(), "要能被 UI 识别成中性态而非问题");
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

/// **B04-5**：`REMOTE_HOOKS_CMD` 此前**一条守卫都没有**。
/// 对比 `cc_bus.rs` 的 `CC_BUS_CAT_CMD`：既有常量守卫又有调用点守卫。
/// 「绝不写远端任何文件」这句话在 B04 里曾经是纯口头的。
#[test]
fn remote_command_is_readonly_and_reaches_ssh_unmodified() {
    // 常量本身：零插值、无写动词
    assert!(!REMOTE_HOOKS_CMD.contains("{}"));
    assert!(!REMOTE_HOOKS_CMD.contains("$1"));
    for w in [
        "rm ", "mv ", ">>", "tee ", "truncate", "chmod", "kill", "ln -s",
    ] {
        assert!(!REMOTE_HOOKS_CMD.contains(w), "只读命令里不该有 {w:?}");
    }
    assert!(REMOTE_HOOKS_CMD.contains(HOOKS_SPLIT_MARKER));
    assert!(
        REMOTE_HOOKS_CMD.trim_end().ends_with("true"),
        "缺文件时仍须 rc=0"
    );
    // **调用点**：必须原样交给 SSH（包一层 format! 就不再是定值）
    let code = non_test_code();
    assert!(
        code.contains("connect_and_exec_cmd(&cfg, REMOTE_HOOKS_CMD)"),
        "定值命令必须原样交给 SSH"
    );
    assert_eq!(
        code.matches("REMOTE_HOOKS_CMD").count(),
        2,
        "常量只准出现两次：定义 + 唯一调用点"
    );
}

/// 取本文件的**非测试、非注释**代码。
/// 剥注释是必需的——B04 审计实测：朴素子串扫会把**注释里**写的 `File::create`
/// 当成真调用（假红）。
fn non_test_code() -> String {
    let src = include_str!("../../src/bridge/src/hooks_diag.rs");
    // **扫全文再去掉测试模块**，而不是"只取第一个 #[cfg(test)] 之前"——
    // 后者对写在测试模块**之后**的非测试代码是盲区（B04 审计指出的第二个洞）。
    let marker = concat!("#[cfg", "(test)]");
    let code = src.split(marker).next().unwrap_or(src);
    code.lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with('*') && !t.starts_with("/*")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn non_test_code_helper_is_sane() {
    let c = non_test_code();
    assert!(c.contains("pub fn diagnose"), "剥过头了");
    assert!(c.contains("pub fn snippet"), "剥过头了");
    assert!(!c.contains("剥注释是必需的"), "注释没剥干净");
    assert!(c.len() > 2000, "剩下的代码太少，守卫形同虚设");
}

/// **本模块绝不写盘。**用户定调不改 `~/.claude/settings.json`；`cc-bus-install.sh`
/// 第 3 行同样拒绝改它。这条守卫把红线变成门禁。
///
/// **第一版是黑名单，被审计当场绕过**：它只列了 6 个字面量
/// （`fs::write`/`File::create`/`OpenOptions`/`Command::new`/`std::process`/`remove_file`），
/// 审计往非测试段插了 `fs::rename` + `File::options` + `symlink` + `remove_dir_all`
/// 的真写盘代码，**15 项测试全绿**。黑名单永远漏，因为写盘的写法列不完。
///
/// 改成**白名单**：非测试代码里凡是 `std::fs::` 的用法，只准是 `read_to_string`。
/// 想新增任何文件操作都会当场红——包括我还没想到的那些写法。
#[test]
fn this_module_never_writes() {
    let code = non_test_code();

    // ★ **前提触发器**〔audit-0805 08-07〕：下面 ① 的白名单认的是 `fs::` **前缀**，
    // 而 `use std::fs as X;` 之后调用处根本不带这个前缀 ⇒ 本条会**静默瞎掉**。
    // 实测：往本文件放 `use std::fs as sysio;` + `sysio::write(p, s)`
    // （模块名逐字写着 never writes），全仓 974 条判据一条不红。
    // 那个前提由 `write_site_registry` 那条导入禁令保证 —— 它要是被删了，本条也就没了根。
    // ⚠⚠ **08-08 订正：光查名字挡不住「掏空」**。实测把那条禁令的函数体清空
    // （名字原样留着），本条**照样绿** —— 而别名导入从此又能让写盘调用整个隐形。
    // 「散文说有、实际没有」这一族的又一例，只是这次「说」的是一个函数名。
    // ⇒ 三条腿：名字在 · 它真在判导入 · 反例那一半还在。
    let ban = include_str!("../../src/bridge/src/write_site_registry.rs");
    for (needle, why) in [
        (
            "fn no_alias_or_item_import_can_hide_a_write_call",
            "那条禁令整个不见了",
        ),
        (
            "fs_import_verdict",
            "禁令还在，但它不再调那个判导入的函数 —— 大概率被掏空了",
        ),
        (
            "is_err()",
            "禁令里没有「坏样本必须被拒」那一半 —— 只剩正例的守卫是恒绿的",
        ),
    ] {
        assert!(
            ban.contains(needle),
            "`write_site_registry` 那条导入禁令：{why}（找 `{needle}`）。\n\
                 ★ 本条 `fs::` 前缀白名单的前提就压在它身上：`use std::fs as X;` 之后\n\
                 调用处根本不带这个前缀 ⇒ 本条会**静默瞎掉**（08-07 实测：往本文件放\n\
                 `use std::fs as sysio;` + `sysio::write(p, s)`，全仓 974 条判据一条不红）。\n\
                 要么把那条找回来/补全，要么本条改成不依赖前缀的写法，别让它绿着。"
        );
    }

    // ① std::fs:: 的白名单——只准读
    let fs_uses: Vec<&str> = code
        .match_indices("fs::")
        .map(|(i, _)| {
            let rest = &code[i + 4..];
            let end = rest
                .find(|c: char| !c.is_alphanumeric() && c != '_')
                .unwrap_or(rest.len());
            &rest[..end]
        })
        .collect();
    for u in &fs_uses {
        assert_eq!(
            *u, "read_to_string",
            "本模块只准 fs::read_to_string，发现 fs::{u}"
        );
    }
    // 反向自检：**确实扫到了**那唯一一处读（否则白名单在空转）
    assert!(
        fs_uses.contains(&"read_to_string"),
        "一处 fs 用法都没扫到，守卫在空转"
    );

    // ★★ **「把写交给别人」也算写**〔audit-0805 08-07〕。
    //
    // ① 扫的是 `fs::` 前缀，只认「自己写」。08-07 实测：本模块加一句
    // `crate::utils::atomic_write_json(p, v)`，**全仓 982 条判据一条不红**，
    // 而本条的名字逐字写着 never writes。与 F44 同族（动作类判据锚在写法上）。
    // 「谁在写」问唯一那张表：`write_site_registry::WRITE_SITES`（它自己由默认拒绝守着）。
    let delegated = crate::write_site_registry::writers::called_by(&code, "hooks_diag.rs");
    assert!(
        delegated.is_empty(),
        "本模块调了已登记的**写者**：{delegated:?}\n\
             ⚠ 前缀扫描看不见这种写法 —— 写发生在被调方，本文件里一个 `fs::` 都不出现。"
    );
    assert!(
        !crate::write_site_registry::writers::names().is_empty(),
        "`WRITE_SITES` 是空的 —— 上面那条在空转"
    );

    // ② 其它写/执行入口一律不准（这些不经 fs:: 前缀）
    for bad in [
        concat!("File::", "create"),
        concat!("File::", "options"),
        concat!("Open", "Options"),
        concat!("Command::", "new"),
        concat!("std::", "process"),
        "symlink",
        "set_permissions",
        "write_all",
        "create_dir",
        "remove_dir",
        ".write(",
    ] {
        assert!(!code.contains(bad), "只读模块里不该出现 {bad:?}");
    }
}
