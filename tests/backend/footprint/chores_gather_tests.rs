//! 收事实那几样里能离线判的：PATH 上的 ccm 是不是我们那一份 · 启动文件里确证失效的行。

use super::*;

#[test]
fn path_上先找到的是我们那一份_不报_别的那一份_报() {
    let h = test_home::scratch("ccm");
    let ours = h.join(".cc-monitor/bin");
    let other = h.join("other");
    std::fs::create_dir_all(&ours).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(ours.join("ccm"), "x").unwrap();
    std::fs::write(other.join("ccm"), "x").unwrap();
    let p = std::env::join_paths([&ours, &other]).unwrap();
    assert!(stale_ccm(&h, p.to_str(), false).is_none());
    let p = std::env::join_paths([&other, &ours]).unwrap();
    let s = stale_ccm(&h, p.to_str(), false).unwrap();
    assert_eq!(s.path, other.join("ccm").display().to_string());
    assert!(!s.needs_root, "家目录底下的不用 sudo");
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn 失效行_只报_source_的文件不在的_围栏里与注释与说不清的不报() {
    let h = test_home::scratch("dead");
    std::fs::write(h.join("here.sh"), "").unwrap();
    let text = "source ~/here.sh\nsource ~/gone.sh\n. $HOME/gone2\n# source ~/gone3\nsource $FOO/x\n# === cc-monitor BEGIN ===\nsource ~/gone4\n# === cc-monitor END ===\nsource \"/nope/abs\"\n";
    let got = crate::platform::shell::posix::dead_source_lines(text, Some(&h));
    let nums: Vec<usize> = got.iter().map(|(n, _)| *n).collect();
    assert_eq!(nums, vec![2, 3, 9]);
    let _ = std::fs::remove_dir_all(&h);
}

/// 被「文件在才读」守着的 source（`[ -f X ]` · `[ -r X ]` · `test -f X`；同一行 `&&`、或包在 `if … then … fi` 里）——
/// 文件不在是正常的，不是失效；守的是别的文件、或裸 source 一个不在的文件 ⇒ 照报。
#[test]
fn 失效行_文件在才读的不报_守着别的文件的与裸的照报() {
    let h = test_home::scratch("guarded");
    let text = [
        "if [ -f ~/.extra_aliases ]; then",                   // 1
        "    . ~/.extra_aliases",                             // 2 守着
        "fi",                                                 // 3
        "[ -f ~/gone-a ] && . ~/gone-a",                      // 4 守着
        "[ -r \"$HOME/gone-b\" ] && source \"$HOME/gone-b\"", // 5 守着
        "test -f ~/gone-c && . ~/gone-c",                     // 6 守着
        "if test -r ~/gone-d; then source ~/gone-d; fi",      // 7 守着
        "if [ -f ~/gone-e ]",                                 // 8
        "then",                                               // 9
        "  echo x",                                           // 10
        "  . $HOME/gone-e",                                   // 11 守着（同一个文件的另一种写法）
        "fi",                                                 // 12
        ". ~/gone-bare",                                      // 13 裸的 ⇒ 报
        "if [ -f ~/other ]; then",                            // 14
        "  . ~/gone-f",                                       // 15 守的是别的文件 ⇒ 报
        "fi",                                                 // 16
        "[ -f ~/other ] && . ~/gone-g",                       // 17 同上 ⇒ 报
        ". ~/gone-h",                                         // 18 if 已经关了 ⇒ 报
    ]
    .join("\n");
    let got = crate::platform::shell::posix::dead_source_lines(&text, Some(&h));
    let nums: Vec<usize> = got.iter().map(|(n, _)| *n).collect();
    assert_eq!(nums, vec![13, 15, 17, 18]);
    let _ = std::fs::remove_dir_all(&h);
}

/// 根钥匙派生出的只许直通那一把（照中转的派生法在这里现算：HMAC-SHA256(根, "ccm-relay/t") 的小写十六进制）。
fn pass_hex(root: &str) -> String {
    let k = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, root.as_bytes());
    ring::hmac::sign(&k, b"ccm-relay/t")
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 一段文本里所有恰好 64 位的小写十六进制串（钥匙的形状）。
fn hex64(s: &str) -> std::collections::BTreeSet<String> {
    s.split(|c: char| !matches!(c, '0'..='9' | 'a'..='f'))
        .filter(|w| w.len() == 64)
        .map(str::to_string)
        .collect()
}

/// ★ 中转钥匙不进界面：这台收事实 → 出「待办」→ 序列化成线上那一份，里面所有钥匙形的串 == {只许直通那一把}。
/// 两份设置文件：没写 · 照旧贴过全权那一把（根钥匙本身，贴过的那一行在 diff 里显示成 `…`、整份里换成新的那一条）。
/// 同一份文件上还有收信那一件（它的 diff 上下文与整份也从那份文件来）。正控：直通那一把确实出现了。
#[test]
fn 待办里的钥匙只有只许直通那一把() {
    let root = "0123456789abcdef".repeat(4);
    let old = format!(
        "{{\n  \"model\": \"x\",\n  \"env\": {{\n    \"ANTHROPIC_BASE_URL\": \"http://127.0.0.1:8788/{root}/t/claude-code/_\"\n  }}\n}}\n"
    );
    for settings in [None, Some(old)] {
        let h = test_home::scratch("relay-key");
        std::fs::create_dir_all(h.join(".cc-monitor")).unwrap();
        std::fs::create_dir_all(h.join(".claude")).unwrap();
        std::fs::write(h.join(".cc-monitor/relay-key"), &root).unwrap();
        let file = h.join(".claude/settings.json");
        if let Some(s) = &settings {
            std::fs::write(&file, s).unwrap();
        }
        let relay = relay_at(
            crate::agents::REGISTRY,
            &h,
            &Default::default(),
            &|_| true,
            &[],
        );
        assert!(!relay.is_empty(), "该有「直接敲的也走中转」那几件");
        let f = Facts {
            windows: false,
            needs_install: vec![],
            stale_ccm: None,
            self_paste: None,
            clashes: vec![],
            dead: vec![],
            relay,
            hooks: vec![Hooks {
                kind: "claude".into(),
                ccbus: true,
                installed: false,
                path: file.display().to_string(),
                text: settings.clone(),
                items: vec![(
                    vec!["hooks".into(), "Stop".into()],
                    serde_json::json!({"hooks": []}),
                )],
                trust: None,
            }],
            mcp_login: vec![],
            zero_trust: vec![],
            outside_library: vec![],
            sshd: None,
            declined: vec![],
        };
        let wire = serde_json::to_string(&super::super::chores(&f)).unwrap();
        assert_eq!(
            hex64(&wire),
            std::collections::BTreeSet::from([pass_hex(&root)]),
            "设置文件 {settings:?} ⇒ {wire}"
        );
        let _ = std::fs::remove_dir_all(&h);
    }
}

/// 假的那台（临时目录里的 sshd 配置 ＋ 一个 Include 目录）拒端口转发 ⇒ 收到「关了」那一行；没那份配置（没装 sshd）⇒ 不收。
#[test]
fn sshd_配置读真文件_include_目录照展开() {
    let h = test_home::scratch("sshd");
    std::fs::create_dir_all(h.join("sshd_config.d")).unwrap();
    let main = h.join("sshd_config");
    std::fs::write(&main, "Include sshd_config.d/*.conf\nPort 22\n").unwrap();
    std::fs::write(
        h.join("sshd_config.d/60-deny.conf"),
        "AllowTcpForwarding no\n",
    )
    .unwrap();
    let got = sshd_at(&main).expect("该收到");
    assert_eq!(
        got.path,
        h.join("sshd_config.d/60-deny.conf").display().to_string()
    );
    assert_eq!(got.line, 1);
    assert!(sshd_at(&h.join("gone")).is_none());
    let _ = std::fs::remove_dir_all(&h);
}

/// 贴好了的那一家：活着的会话所在的项目里有一份自己写了上游地址 ⇒ 记下那个项目与那份文件；别的项目没写 ⇒ 不记。
#[test]
fn 中转_项目里自己写了地址_记下盖住它的那一份() {
    let h = test_home::scratch("covered");
    let root = "0123456789abcdef".repeat(4);
    std::fs::create_dir_all(h.join(".cc-monitor")).unwrap();
    std::fs::write(h.join(".cc-monitor/relay-key"), &root).unwrap();
    let (plain, app) = (h.join("plain"), h.join("app"));
    std::fs::create_dir_all(plain.join(".claude")).unwrap();
    std::fs::create_dir_all(app.join(".claude")).unwrap();
    std::fs::write(
        app.join(".claude/settings.json"),
        r#"{"env":{"ANTHROPIC_BASE_URL":"https://p.example"}}"#,
    )
    .unwrap();
    let projects = [plain.display().to_string(), app.display().to_string()];
    let claude = |ps: &[String]| {
        relay_at(
            crate::agents::REGISTRY,
            &h,
            &Default::default(),
            &|_| true,
            ps,
        )
        .into_iter()
        .find(|r| r.agent == "claude-code")
        .expect("该有 Claude 那一件")
    };
    let c = claude(&projects).covered.expect("该认出被盖住");
    assert_eq!(c.project, app.display().to_string());
    assert_eq!(
        c.file,
        app.join(".claude/settings.json").display().to_string()
    );
    assert!(
        claude(&projects[..1]).covered.is_none(),
        "没写的项目不算盖住"
    );
    let _ = std::fs::remove_dir_all(&h);
}

/// 两家都声明了 MCP 面：待办里要登录的 MCP 逐家各问一遍，两家的都在（不因为多了一家就一家都不认）。替身家。
#[test]
fn 两家都声明了_mcp_面_待办里两家要登录的都在() {
    use crate::agents::fake;
    let reg = [
        fake::family_row("alpha", "CCM_A_DIR", "a", true, || None),
        fake::family_row("beta", "CCM_B_DIR", "b", false, || None),
    ];
    let got = mcp_login_among(&reg, |kind| {
        vec![crate::accounts::manage::mcp_share_exec::McpNeed {
            name: format!("{kind}-srv"),
            scope: "user",
            login_in: vec![Some(format!("{kind}-acct"))],
            project: None,
        }]
    });
    let names: Vec<(&str, Vec<String>)> = got
        .iter()
        .map(|m| (m.name.as_str(), m.who.clone()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("alpha-srv", vec!["alpha-acct".to_string()]),
            ("beta-srv", vec!["beta-acct".to_string()]),
        ]
    );
}

/// 待办复制那一行逐词照 [`crate::platform::shell::dialect::posix_word`]：判哪些字裸写的不另写一份。
#[test]
fn the_copied_todo_line_quotes_each_word_like_posix_word() {
    let argv: Vec<String> = ["claude", "mcp", "login", "my server", "it's", "", "a=b"]
        .map(String::from)
        .to_vec();
    let want: Vec<String> = argv
        .iter()
        .map(|w| crate::platform::shell::dialect::posix_word(w))
        .collect();
    assert_eq!(command_line(&argv), want.join(" "));
}

/// 只答家目录的门（账号库那一族经门拿家；别的问一律不该来）。
struct HomeOnly(std::path::PathBuf);

impl crate::common::door::Door for HomeOnly {
    fn home(
        &self,
        _: cmd_wire::files::HomeReq,
    ) -> Result<cmd_wire::files::FilesPathReply, crate::common::said::Said> {
        Ok(cmd_wire::files::FilesPathReply {
            path: self.0.display().to_string().as_str().into(),
        })
    }
    fn stat(
        &self,
        _: cmd_wire::files::StatReq,
    ) -> Result<cmd_wire::files::StatReply, crate::common::said::Said> {
        unreachable!("只问家目录")
    }
    fn ls(
        &self,
        _: cmd_wire::files::LsReq,
    ) -> Result<cmd_wire::files::LsReply, crate::common::said::Said> {
        unreachable!("只问家目录")
    }
    fn peek(
        &self,
        _: cmd_wire::files::FilesPeekReq,
    ) -> Result<cmd_wire::files::FilesPeekReply, crate::common::said::Said> {
        unreachable!("只问家目录")
    }
    fn put(
        &self,
        _: cmd_wire::files::FilesPutReq,
    ) -> Result<cmd_wire::files::FilesPutReply, crate::common::said::Said> {
        unreachable!("只问家目录")
    }
    fn delete(
        &self,
        _: cmd_wire::files::FilesDeleteReq,
    ) -> Result<cmd_wire::batch::BatchReply, crate::common::said::Said> {
        unreachable!("只问家目录")
    }
    fn copy(
        &self,
        _: cmd_wire::files::FilesCopyReq,
    ) -> Result<cmd_wire::batch::BatchReply, crate::common::said::Said> {
        unreachable!("只问家目录")
    }
    fn mkdir(
        &self,
        _: cmd_wire::files::FilesMkdirReq,
    ) -> Result<cmd_wire::files::FilesPathReply, crate::common::said::Said> {
        unreachable!("只问家目录")
    }
    fn link(
        &self,
        _: cmd_wire::files::FilesLinkReq,
    ) -> Result<cmd_wire::files::FilesPathReply, crate::common::said::Said> {
        unreachable!("只问家目录")
    }
    fn rename_to(
        &self,
        _: cmd_wire::files::FilesRenameReq,
    ) -> Result<cmd_wire::files::FilesPathReply, crate::common::said::Said> {
        unreachable!("只问家目录")
    }
    fn chmod(
        &self,
        _: cmd_wire::files::FilesChmodReq,
    ) -> Result<cmd_wire::batch::BatchReply, crate::common::said::Said> {
        unreachable!("只问家目录")
    }
}

/// 账号 0 缺的信任照注入的注册表取家与号：替身家的号信任过、账号 0 没记上 ⇒ 那一家出一件；照生产那一份取就一件不出。
#[test]
fn 账号_0_缺的信任照注入的注册表取() {
    let home = test_home::scratch("zero-gaps-reg").canonicalize().unwrap();
    let h = home.display().to_string();
    let write = |rel: &str, body: &str| {
        let p = home.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    };
    write(
        "shared/fake-config.json",
        "{\"trusted\": {\"/a0\": true}}\n",
    );
    write(
        ".cc-monitor/accounts/a1/fake-config.json",
        "{\"trusted\": {\"/a0\": true, \"/a1\": true}}\n",
    );
    write(
        &format!(
            "{}/{}",
            relay_route_core::ACCOUNTS_DIR_REL,
            relay_route_core::ACCOUNTS_MANIFEST_NAME
        ),
        &serde_json::json!({
            "version": 1,
            "accounts": [{
                "name": "a1",
                "email": "",
                "configDir": format!("{h}/.cc-monitor/accounts/a1"),
                "isDefault": false,
                "agent": "fa",
            }],
        })
        .to_string(),
    );
    let registry = [crate::agents::fake::family_row(
        "fa",
        "CCM_FA_DIR",
        "fa",
        true,
        || None,
    )];
    let got: Vec<&str> =
        crate::accounts::manage::trust_share_exec::zero_gaps(&HomeOnly(home.clone()), &registry)
            .iter()
            .map(|g| g.kind)
            .collect();
    assert_eq!(got, ["fa"], "没照注入的那一份注册表取家与号");
    let _ = std::fs::remove_dir_all(&home);
}

/// 号目录不在账号库里的号照注入的注册表取家：替身家的号住在库外 ⇒ 列出它；照生产那一份取就认不出这一家、一个不列。
#[test]
fn 库外的号照注入的注册表取() {
    let home = test_home::scratch("outside-reg").canonicalize().unwrap();
    let h = home.display().to_string();
    let elsewhere = home.join("elsewhere").join("a1");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let manifest = home
        .join(relay_route_core::ACCOUNTS_DIR_REL)
        .join(relay_route_core::ACCOUNTS_MANIFEST_NAME);
    std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    std::fs::write(
        &manifest,
        serde_json::json!({
            "version": 1,
            "accounts": [{
                "name": "a1",
                "email": "",
                "configDir": elsewhere.display().to_string(),
                "isDefault": false,
                "agent": "fa",
            }],
        })
        .to_string(),
    )
    .unwrap();
    let registry = [crate::agents::fake::family_row(
        "fa",
        "CCM_FA_DIR",
        "fa",
        true,
        || None,
    )];
    let got: Vec<String> = crate::accounts::manage::trust_share_exec::outside_library(
        &HomeOnly(home.clone()),
        &registry,
    )
    .into_iter()
    .map(|o| o.name)
    .collect();
    assert_eq!(got, ["a1"], "没照注入的那一份注册表取家（{h}）");
    let _ = std::fs::remove_dir_all(&home);
}

/// 「直接敲的也走中转」列哪几家照注入的注册表：替身家声明了那一格 ⇒ 只列它（叫法也照它那一行）。
#[test]
fn 中转那几件照注入的注册表列家() {
    use crate::agents::{fake, SettingsBaseUrl, SettingsEnvFace};
    let h = test_home::scratch("relay-reg");
    let up = crate::agents::DefaultUpstream {
        settings_env: Some(SettingsEnvFace {
            read: |home| (home.join("alpha-env.json"), SettingsBaseUrl::Unset),
            merge: |_, _| None,
            slot: "env.ALPHA_URL",
            covered_in: |_| None,
        }),
        ..fake::upstream::UPSTREAM
    };
    let registry = [crate::agents::Adapter {
        upstream: Some(up),
        ..fake::family_row("alpha", "CCM_A_DIR", "a", true, || None)
    }];
    let got: Vec<(String, String)> = relay_at(&registry, &h, &Default::default(), &|_| false, &[])
        .into_iter()
        .map(|r| (r.agent, r.name))
        .collect();
    assert_eq!(
        got,
        vec![(
            fake::upstream::UPSTREAM.route_id.to_string(),
            fake::said_of("alpha").to_string()
        )],
        "没照注入的那一份注册表列家"
    );
    let _ = std::fs::remove_dir_all(&h);
}

/// 收信钩子那几件逐家照注入的注册表：两家替身各一件，生产那几家一家不出。
/// 只在跑得了 cc-bus 的平台判（Windows 上这台跑不了 ⇒ 一件不出，判不出照哪一份）。
#[cfg(unix)]
#[test]
fn 收信钩子照注入的注册表列家() {
    let reg = crate::agents::fake::footprint::two_footprint_families();
    let kinds: Vec<String> = hooks(reg).into_iter().map(|k| k.kind).collect();
    assert_eq!(kinds, ["fake", "fake2"], "没照注入的那一份注册表列家");
}

/// 要加的那条钩子指向的 cc-bus 照注入的注册表里默认那一家的用户级 skill 根找：替身家的根里装着 cc-bus ⇒ 两家都给要加的那一段。
#[cfg(unix)]
#[test]
fn 收信钩子要加的那一段照注入的注册表那一家的_skill_根() {
    use crate::agents::fake;
    let skills = fake::USER_SKILLS
        .get_or_init(|| test_home::scratch("hooks-skills"))
        .clone();
    let scripts = skills.join("cc-bus").join("scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    std::fs::write(scripts.join(crate::assets::cc_bus_hooks::PROGRAM), b"").unwrap();
    let reg = [
        crate::agents::Adapter {
            assets: Some(fake::ASSETS_USER_SKILLS),
            footprint: Some(fake::footprint::FACE_A),
            ..fake::family_row("fake", "CCM_FAKE_A", "fakea", true, || None)
        },
        crate::agents::Adapter {
            footprint: Some(fake::footprint::FACE_B),
            ..fake::family_row("fake2", "CCM_FAKE_B", "fakeb", false, || None)
        },
    ];
    let got: Vec<(String, bool)> = hooks(&reg).into_iter().map(|k| (k.kind, k.ccbus)).collect();
    assert_eq!(
        got,
        [("fake".to_string(), true), ("fake2".to_string(), true)],
        "没照注入的那一份注册表找 cc-bus 装在哪"
    );
    let _ = std::fs::remove_dir_all(&skills);
}

/// 收信钩子信任那一件的整份由注入的注册表里那一家的 `merge` 并：替身甲家钩子装着、信任没对上 ⇒ 整份是照它的改法并好的那一份。
#[test]
fn 收信钩子信任的整份照注入的注册表那一家并() {
    use crate::agents::fake::footprint::{trust_key_a, two_footprint_families, TRUST_FILE_A};
    let home = test_home::scratch("hooks-merge");
    let skills = home.join("skills");
    let scripts = skills.join("cc-bus").join("scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    std::fs::write(scripts.join(crate::assets::cc_bus_hooks::PROGRAM), b"").unwrap();
    std::fs::create_dir_all(home.join("a")).unwrap();
    std::fs::create_dir_all(home.join("b")).unwrap();
    let hooks_a = home.join("a").join("fake-settings.json");
    let cmd = "\"$HOME/skills/cc-bus/scripts/cc-bus\" stop-hook";
    std::fs::write(
        &hooks_a,
        serde_json::json!({ "onStop": [{ "run": cmd }] }).to_string(),
    )
    .unwrap();
    std::fs::write(home.join("a").join(TRUST_FILE_A), "other = x\n").unwrap();
    let at = home.clone();
    let report = crate::assets::cc_bus_hooks::answer_among(
        two_footprint_families(),
        Some(&home),
        &move |k| at.join(if k == "fake" { "a" } else { "b" }),
        Some(&skills),
        true,
    );
    let got = hooks_from(two_footprint_families(), report);
    let a = got.iter().find(|k| k.kind == "fake").expect("甲家那一件");
    let trust = a.trust.as_ref().expect("甲家声明了信任");
    assert!(!trust.trusted);
    let whole = trust
        .whole
        .as_deref()
        .expect("整份没照注入的那一份注册表里甲家的改法并");
    assert!(whole.starts_with("other = x\n"), "{whole}");
    assert!(whole.contains(&trust_key_a(&hooks_a, 0)), "{whole}");
    let _ = std::fs::remove_dir_all(&home);
}

/// 要登录的 MCP 照注入的注册表：替身家此刻活着的会话（判活照它的本机布局）所在项目里有一条要登录的 ⇒ 待办出那一件；
/// 判活或读 MCP 任一处取生产那一份 ⇒ 一件不出。
#[test]
fn 要登录的_mcp_照注入的注册表选家与判活() {
    use crate::agents::fake;
    static HOME: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    let home = HOME
        .get_or_init(|| test_home::scratch("mcp-login-reg"))
        .clone();
    let project = home.join("proj");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join(".fake-mcp.json"),
        serde_json::json!({ "servers": { "srv": { "needsLogin": true } } }).to_string(),
    )
    .unwrap();
    // 这一条会话的进程就是跑判据的这一个：启动时刻照实记上，判活认得出是它写的。
    let me = std::process::id();
    let pidfiles = crate::agents::AgentHome {
        kind: fake::AGENT_KIND,
        home: home.clone(),
    }
    .pidfile_dir_among(&[fake::full_row(|| None)])
    .expect("替身家有本机布局");
    std::fs::create_dir_all(&pidfiles).unwrap();
    std::fs::write(
        pidfiles.join(format!("{me}.json")),
        serde_json::json!({
            "sid": "s1",
            "cwd": project.display().to_string(),
            "procStart": crate::platform::proc::proc_starttime(me),
        })
        .to_string(),
    )
    .unwrap();
    let registry = [fake::full_row(|| HOME.get().cloned())];
    let got: Vec<(&str, String, String)> = mcp_login(&registry)
        .into_iter()
        .map(|m| (m.kind, m.name, m.command))
        .collect();
    let argv = crate::agents::mcp_login_argv_among(&registry, fake::AGENT_KIND, "srv")
        .expect("替身家声明了登录那一格");
    assert_eq!(
        got,
        vec![(fake::AGENT_KIND, "srv".to_string(), command_line(&argv))],
        "没照注入的那一份注册表判活 · 读 MCP"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// 各家的家（中转那一件问活着的会话照这一串；MCP 登录那一件逐家取，同一个 `family_home`）取自注入的注册表：替身家的家跟生产那几家不同 ⇒ 照替身列，生产那几家一家不出。
#[test]
fn 各家的家照注入的注册表取() {
    use crate::agents::fake;
    let reg = [
        fake::family_row("alpha", "CCM_A_DIR", "a", true, || {
            Some(std::path::PathBuf::from("/x/alpha-home"))
        }),
        fake::family_row("beta", "CCM_B_DIR", "b", false, || None),
    ];
    let got: Vec<(&str, std::path::PathBuf)> = family_homes(&reg)
        .into_iter()
        .map(|h| (h.kind, h.home))
        .collect();
    assert_eq!(
        got,
        vec![("alpha", std::path::PathBuf::from("/x/alpha-home"))],
        "没照注入的那一份注册表取各家的家"
    );
}
