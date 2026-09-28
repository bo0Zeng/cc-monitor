//! 设计/01 §1.1（「一切判定都在后端：口径、命令串……」）· `99 §2.1 ⑬`：本机起会话的计划与渲染（`control/launch_render/local.rs`）。
//!
//! 〔MIG-2〕从 monitor `tests/bridge/history_tests.rs` 搬来的那一族（旧路逐字节形状 · 账号三态 · 注入闸 · `ccm` 容器路 ·
//! 中转前缀 · 身份 token）改成喂 [`Facts`] 驱动生产那一条纯函数 [`plan`]，不再靠线程局部的替身缝。
//! 买不到：真起一个终端窗口（那是 monitor `open_local_terminal` 的事）· 真 Windows 上 PowerShell 真跑。

use super::*;

const SID: &str = "01998f2a-1234-7abc-9def-0123456789ab";

fn agent() -> AgentFacts {
    AgentFacts {
        id: "claude-code".into(),
        default_launcher: "claude".into(),
        launcher_alias: Some("cc".into()),
        resume_flag: "--resume".into(),
    }
}

fn req(action: LocalAction) -> LocalLaunchRequest {
    LocalLaunchRequest {
        action,
        cwd: None,
        launcher: None,
        account: None,
        tmux_name: None,
        agent: agent(),
        all_sessions: false,
    }
}

fn resume() -> LocalAction {
    LocalAction::Resume { sid: SID.into() }
}

fn named(dir: &str, name: Option<&str>) -> Option<LaunchAccount> {
    Some(LaunchAccount::Named {
        config_dir: dir.into(),
        name: name.map(str::to_string),
    })
}

/// 这一版 `ccm --ccm-probe` 真实吐出的能力（与夹具 `cli-golden.json` 的能力齐全那一份同一串）。
fn current_ccm() -> CcmSeen {
    CcmSeen {
        installed: true,
        caps: [
            "new", "resume", "attach", "tmux", "account", "model", "cwd", "agent", "launcher",
            "ccm-sid", "print", "detach", "tmux-size",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    }
}
fn no_ccm() -> CcmSeen {
    CcmSeen {
        installed: false,
        caps: BTreeSet::new(),
    }
}
fn probe_panics() -> CcmSeen {
    panic!("这一格不该去探 ccm")
}
fn no_relay(_: &Value) -> Result<Option<String>, (&'static str, String)> {
    Ok(None)
}
fn relay_down(_: &Value) -> Result<Option<String>, (&'static str, String)> {
    Err(("relay_down", "中转没在听".into()))
}
const URL: &str = "http://127.0.0.1:8788/s/claude-code/acct-a";
thread_local! {
    static RELAY_ASKED: std::cell::RefCell<Vec<Value>> = const { std::cell::RefCell::new(Vec::new()) };
}
fn relay_url(args: &Value) -> Result<Option<String>, (&'static str, String)> {
    RELAY_ASKED.with(|a| a.borrow_mut().push(args.clone()));
    Ok(Some(URL.into()))
}
fn any_dir(_: &str) -> bool {
    true
}
fn no_dir(_: &str) -> bool {
    false
}

fn posix(probe: fn() -> CcmSeen) -> Facts {
    Facts {
        windows: false,
        probe_ccm: probe,
        relay: no_relay,
        is_dir: any_dir,
    }
}
fn ps() -> Facts {
    Facts {
        windows: true,
        probe_ccm: probe_panics,
        relay: no_relay,
        is_dir: any_dir,
    }
}

/// 命令串去掉身份那一段（`export CCM_LAUNCH_ID=…; ` / `$env:CCM_LAUNCH_ID='…'; `）后的本体。
fn body(cmd: &str) -> &str {
    let at = cmd.find(LAUNCH_ID_VAR).expect("命令串里没有身份那一段");
    let rest = &cmd[at..];
    &rest[rest.find("; ").expect("身份那一段没收尾") + 2..]
}

// ─── 旧路：逐字节形状（手写期望，原 `history_tests` 那几条） ───

#[test]
fn the_old_posix_path_prefers_the_wrapper_and_falls_back_byte_for_byte() {
    let got = plan(&req(resume()), &posix(probe_panics)).unwrap();
    assert_eq!(
        body(&got.cmd),
        format!("if command -v cc >/dev/null 2>&1; then cc --resume {SID}; else claude --resume {SID}; fi")
    );
    let got = plan(&req(LocalAction::New), &posix(probe_panics)).unwrap();
    assert_eq!(body(&got.cmd), "if command -v cc >/dev/null 2>&1; then cc; else claude; fi");
    let mut r = req(resume());
    r.launcher = Some(" cct ".into());
    assert_eq!(body(&plan(&r, &posix(probe_panics)).unwrap().cmd), format!("cct --resume {SID}"));
}

#[test]
fn the_powershell_path_is_byte_identical_and_never_grows_a_container() {
    let got = plan(&req(resume()), &ps()).unwrap();
    assert_eq!(
        body(&got.cmd),
        format!("if (Get-Command cc -ErrorAction SilentlyContinue) {{ cc --resume {SID} }} else {{ claude --resume {SID} }}")
    );
    assert_eq!(
        body(&plan(&req(LocalAction::New), &ps()).unwrap().cmd),
        "if (Get-Command cc -ErrorAction SilentlyContinue) { cc } else { claude }"
    );
    // 给了会话名也不进容器（C12「windows不要tmux」）：Windows 那一支连 ccm 都不探（探了就 panic）。
    for account in [None, Some(LaunchAccount::Base), named("C:\\Users\\z\\.claude-alt\\z", Some("z"))] {
        let mut r = req(resume());
        r.tmux_name = Some("s1-cc".into());
        r.account = account;
        let cmd = plan(&r, &ps()).unwrap().cmd;
        assert!(!cmd.contains("--ccm-tmux") && !cmd.contains(" cct"), "Windows 那一支长出了容器：{cmd}");
        assert!(cmd.starts_with(&format!("$env:{LAUNCH_ID_VAR}=")), "身份那一段不是 PowerShell 形：{cmd}");
    }
}

// ─── 账号三态 ＋ 注入闸 ───

#[test]
fn the_account_prefix_is_three_states_on_both_shells() {
    let shapes = |facts: &Facts| -> Vec<String> {
        [None, Some(LaunchAccount::Base), named("/home/u/.claude-alt/z", None)]
            .into_iter()
            .map(|a| {
                let mut r = req(LocalAction::New);
                r.launcher = Some("claude".into());
                r.account = a;
                body(&plan(&r, facts).unwrap().cmd).to_string()
            })
            .collect()
    };
    assert_eq!(
        shapes(&posix(probe_panics)),
        [
            "claude",
            "unset CLAUDE_CONFIG_DIR; claude",
            "export CLAUDE_CONFIG_DIR='/home/u/.claude-alt/z'; claude"
        ]
    );
    assert_eq!(
        shapes(&ps()),
        [
            "claude",
            "$env:CLAUDE_CONFIG_DIR=$null; claude",
            "$env:CLAUDE_CONFIG_DIR='/home/u/.claude-alt/z'; claude"
        ]
    );
    // Windows 的账号目录形（盘符 · 反斜杠）在 PowerShell 那一支放行。
    let mut r = req(LocalAction::New);
    r.account = named("C:\\Users\\z\\.claude-alt\\z", None);
    assert!(plan(&r, &ps()).is_ok());
}

#[test]
fn every_injection_shape_is_refused_not_sanitized() {
    for dir in ["", "rel/dir", "/", "/a/../b", "/a/..", "/a;rm", "/a$(id)", "/a`id`", "/a'b", "/a\nb", "/a\u{200b}b", "/a\u{3000}b"] {
        for facts in [posix(probe_panics), ps()] {
            let mut r = req(LocalAction::New);
            r.account = named(dir, None);
            let e = plan(&r, &facts).expect_err(&format!("{dir:?} 该拒"));
            assert!(e.starts_with(payload::REFUSE_TAG), "{dir:?} 的拒绝没打标：{e}");
        }
    }
    for sid in ["", "a; rm -rf /", "a b", "a$(id)", "a/../b", "-x"] {
        let r = req(LocalAction::Resume { sid: sid.into() });
        assert!(plan(&r, &posix(probe_panics)).is_err(), "非法 sid {sid:?} 没拒");
        assert!(plan(&r, &ps()).is_err(), "非法 sid {sid:?} 在 PowerShell 那一支没拒");
    }
    for (bad, c) in [("cc; calc", ';'), ("cc|id", '|'), ("cc$(id)", '$'), ("cc`id`", '`'), ("cc&&x", '&')] {
        let mut r = req(LocalAction::New);
        r.launcher = Some(bad.into());
        let e = plan(&r, &posix(probe_panics)).expect_err(bad);
        assert!(e.contains(&format!("{c:?}")), "{bad:?}：那一句没说出是哪个字符：{e}");
    }
    for good in ["/usr/local/bin/claude", "~/bin/claude --x", "ccr code"] {
        let mut r = req(LocalAction::New);
        r.launcher = Some(good.into());
        assert_eq!(body(&plan(&r, &posix(probe_panics)).unwrap().cmd), good);
    }
}

// ─── `ccm` 容器路 ───

#[test]
fn a_named_session_goes_into_the_ccm_container_when_ccm_is_there() {
    let mut r = req(resume());
    r.tmux_name = Some("s1-cc".into());
    r.account = named("/home/u/.claude-alt/z", Some("z"));
    let cmd = plan(&r, &posix(current_ccm)).unwrap().cmd;
    let b = body(&cmd);
    assert!(b.starts_with("ccm "), "没走 ccm 容器路：{cmd}");
    for w in ["--ccm-tmux=s1-cc", &format!("--resume {SID}"), "--account z", &format!("--ccm-sid={SID}")] {
        assert!(b.contains(w), "容器路少了 {w}：{cmd}");
    }
    // 缺席的账号 ⇒ 继承，绝不写成 `--base`。
    r.account = None;
    let b = plan(&r, &posix(current_ccm)).unwrap().cmd;
    assert!(!b.contains("--base") && !b.contains("--account"), "继承那一态被写成了显式账号：{b}");
}

#[test]
fn the_container_path_steps_aside_to_the_old_path_with_a_reason() {
    let old = format!("if command -v cc >/dev/null 2>&1; then cc --resume {SID}; else claude --resume {SID}; fi");
    // ① 说不出会话名 ⇒ 不去探（探了就 panic）、退旧路。
    assert_eq!(body(&plan(&req(resume()), &posix(probe_panics)).unwrap().cmd), old);
    // ② 探到没装 ⇒ 退旧路。
    let mut r = req(resume());
    r.tmux_name = Some("s1-cc".into());
    assert_eq!(body(&plan(&r, &posix(no_ccm)).unwrap().cmd), old);
    // ③ 只说得出目录（§35）⇒ 退旧路，前缀照旧 export。
    r.account = named("/home/u/.claude-alt/z", None);
    assert!(body(&plan(&r, &posix(current_ccm)).unwrap().cmd).starts_with("export CLAUDE_CONFIG_DIR="));
    // ④ 这一发要经中转 ⇒ 容器路今天说不出那一格 ⇒ 不探、退旧路，中转前缀在最前。
    r.account = None;
    let f = Facts {
        relay: relay_url,
        ..posix(probe_panics)
    };
    let cmd = plan(&r, &f).unwrap().cmd;
    assert!(cmd.starts_with(&payload::relay_env_prefix_posix(URL)), "中转前缀不在最前：{cmd}");
    assert_eq!(body(&cmd), old);
}

#[test]
fn attach_renders_only_the_join_line_and_mints_nothing() {
    let mut r = req(LocalAction::Attach);
    r.tmux_name = Some("s1-cc".into());
    let got = plan(&r, &posix(current_ccm)).unwrap();
    assert_eq!(got.cmd, "ccm -- --attach s1-cc");
    assert_eq!(got.launch_id, None, "接回不起 agent，不该铸身份 token");
    // 没名字 · 没装 ccm · Windows 本机 ⇒ 拒（旧路产不出接回，绝不拿「另起一条」糊过去）。
    let mut nameless = req(LocalAction::Attach);
    nameless.tmux_name = None;
    assert!(plan(&nameless, &posix(probe_panics)).is_err());
    assert!(plan(&r, &posix(no_ccm)).is_err());
    let e = plan(&r, &ps()).unwrap_err();
    assert!(e.contains(&copy_text("rsHistory.attach.windows", &[])), "{e}");
}

// ─── 中转 · 身份 · 目录 ───

#[test]
fn the_relay_is_asked_with_this_launch_and_its_refusal_stops_the_launch() {
    RELAY_ASKED.with(|a| a.borrow_mut().clear());
    let mut r = req(resume());
    r.account = named("/home/u/.claude-alt/acct-a", Some("a"));
    r.all_sessions = true;
    let f = Facts {
        relay: relay_url,
        ..posix(probe_panics)
    };
    let cmd = plan(&r, &f).unwrap().cmd;
    assert!(cmd.starts_with(&payload::relay_env_prefix_posix(URL)));
    let asked = RELAY_ASKED.with(|a| a.borrow().clone());
    assert_eq!(
        asked,
        [serde_json::json!({"agent":"claude-code","account":{"kind":"named","configDir":"/home/u/.claude-alt/acct-a","name":"a"},"allSessions":true})]
    );
    let wf = Facts { relay: relay_url, ..ps() };
    assert!(plan(&r, &wf).unwrap().cmd.starts_with(&payload::relay_env_prefix_ps(URL)));
    let down = Facts {
        relay: relay_down,
        ..posix(probe_panics)
    };
    assert_eq!(plan(&r, &down).unwrap_err(), payload::refuse("中转没在听"));
}

#[test]
fn the_identity_token_is_planted_and_handed_back() {
    let got = plan(&req(resume()), &posix(probe_panics)).unwrap();
    assert_eq!(got.launch_id.as_deref(), Some(SID), "resume 的身份 token 就是那个 sid");
    assert!(got.cmd.starts_with(&format!("export {LAUNCH_ID_VAR}='{SID}'; ")), "{}", got.cmd);
    let a = plan(&req(LocalAction::New), &posix(probe_panics)).unwrap();
    let b = plan(&req(LocalAction::New), &posix(probe_panics)).unwrap();
    let tok = a.launch_id.clone().unwrap();
    assert!(relay_route_core::segment_is_safe(&tok) && tok.len() == 36, "铸出来的不像 nonce：{tok}");
    assert_ne!(a.launch_id, b.launch_id, "两次新起铸出同一个 token");
    assert!(a.cmd.contains(&format!("{LAUNCH_ID_VAR}='{tok}'")), "交回的 token 不是渲进命令的那一个");
}

#[test]
fn a_new_session_in_a_missing_directory_is_refused() {
    let mut r = req(LocalAction::New);
    r.cwd = Some("/no/such".into());
    let f = Facts {
        is_dir: no_dir,
        ..posix(probe_panics)
    };
    assert!(plan(&r, &f).unwrap_err().contains("/no/such"));
    r.cwd = Some(String::new());
    assert!(plan(&r, &f).is_ok(), "空 cwd 是「没给」，不该当成目录不在");
    let mut r = req(resume());
    r.cwd = Some("/no/such".into());
    assert!(plan(&r, &f).is_ok(), "resume 不核目录（终端的工作目录由 monitor 给）");
}

// ─── 接线 ───

/// 生产那一份事实表插的就是生产那几个取值口（按函数地址比，不按文本）。
#[test]
fn the_production_facts_are_the_production_take_points() {
    let f = Facts::PRODUCTION;
    assert_eq!(f.windows, crate::platform::shell::LOCAL_TERMINAL_IS_POWERSHELL);
    assert_eq!(f.probe_ccm as usize, probe_local_ccm as fn() -> CcmSeen as usize);
    assert_eq!(
        f.relay as usize,
        crate::accounts::upstream::endpoint::launch_relay
            as fn(&Value) -> Result<Option<String>, (&'static str, String)> as usize
    );
    assert_eq!(f.is_dir as usize, dir_exists as fn(&str) -> bool as usize);
}

/// 谁绕开 [`Facts`] 直接调那几个取值口 ⇒ 红：生产段里它们的**调用形**只剩各自的定义行（原 monitor `D6 阻-4` 那道人群闸）。
#[test]
fn nobody_reads_the_launch_facts_around_the_facts_table() {
    let prod = crate::guard_support::production_code(include_str!(
        "../../../../src/backend/control/launch_render/local.rs"
    ));
    for f in ["probe_local_ccm(", "dir_exists(", "launch_relay("] {
        let n = prod.matches(f).count();
        let def = prod.matches(&format!("fn {f}")).count();
        assert_eq!(n - def, 0, "`{f}` 在事实表之外被直接调了（{} 处）", n - def);
    }
    assert!(prod.contains("probe_ccm: probe_local_ccm,"), "事实表里那一格不在了 —— 尺子瞎了");
}

/// 身份那一格的变量名两侧一个名字：写侧（本模块）· 读侧（`observe/accounts_query.rs` 从别人进程的环境里读）。
#[test]
fn the_launch_id_var_is_one_name_on_both_halves() {
    let reader = include_str!("../../../../src/backend/observe/accounts_query.rs");
    let key = "const LAUNCH_ID_ENV: &str = \"";
    let at = reader.find(key).expect("读侧那个常量不在了 —— 抽取坏了") + key.len();
    let name = &reader[at..at + reader[at..].find('"').unwrap()];
    assert_eq!(name, LAUNCH_ID_VAR);
    assert_eq!(LAUNCH_ID_VAR, "CCM_LAUNCH_ID", "手写锚：两侧同时改名也逃不过");
}

#[test]
fn the_probe_line_is_read_as_installed_with_capabilities() {
    let seen = parse_probe("name=ccm\nversion=2\nself=/x\ncapabilities=new,tmux,attach\nagents=claude\nbuild=p\n");
    assert!(seen.installed);
    assert_eq!(seen.caps, ["attach", "new", "tmux"].iter().map(|s| s.to_string()).collect());
    assert_eq!(parse_probe("NO_CCM\n"), no_ccm());
    assert_eq!(parse_probe(""), no_ccm());
}

/// 生产渲染器的真输出吐给 e2e（`tests/e2e/p3t-local-tmux.sh`）：串必须从这里出去，脚本里不手抄。
/// 跑法：`cargo test --lib emit_local_launch_command_for_e2e -- --ignored --nocapture`。
#[test]
#[ignore]
fn emit_local_launch_command_for_e2e() {
    let sid = std::env::var("P3T_E2E_SID").unwrap_or_else(|_| "s1abcdef".into());
    let name = std::env::var("P3T_E2E_TMUX").unwrap_or_else(|_| "s1abcdef-cc".into());
    let mut r = req(LocalAction::Resume { sid });
    r.launcher = std::env::var("P3T_E2E_LAUNCHER").ok();
    r.account = Some(LaunchAccount::Base);
    r.tmux_name = Some(name);
    let cmd = render_ccm_with(&r, &r.agent, &current_ccm()).expect("渲染不出来 —— e2e 无对象可跑");
    println!("P3T_CMD<<<{cmd}>>>");
}
