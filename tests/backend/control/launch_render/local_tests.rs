//! 本机起会话那一行（`control/launch_render/local.rs`）：起会话只有 `ccm` 一处 —— 每一形都只是一行 `ccm …`。
//! 喂 [`Facts`] 驱动生产那一条纯函数 [`plan`]；期望串全是手写的。
//! 买不到：真起一个终端窗口（monitor `open_local_terminal` 的事）· 真 Windows 上 PowerShell 真跑。

use super::*;

const SID: &str = "01998f2a-1234-7abc-9def-0123456789ab";

fn req(action: LocalAction) -> LocalLaunchRequest {
    LocalLaunchRequest {
        agent: "claude".into(),
        action,
        cwd: None,
        launcher: None,
        account: None,
        tmux_name: None,
        default_launcher: "claude".into(),
    }
}

fn resume() -> LocalAction {
    LocalAction::Resume { sid: SID.into() }
}

fn named(name: &str) -> Option<AccountAsk> {
    Some(AccountAsk::Named { name: name.into() })
}

fn settled(r: &LocalLaunchRequest) -> Settled {
    r.account.as_ref().map_or(Settled::Unsaid, |a| {
        crate::control::launch_account::settled_as_asked(a, &Default::default())
    })
}

fn go(r: &LocalLaunchRequest, f: &Facts) -> Result<Planned, String> {
    plan(r, &settled(r), f)
}

const POSIX: Facts = Facts {
    windows: false,
    is_dir: |_| true,
    entry: || Some("ccm".into()),
};
/// Windows 那一行叫入口的写法：PowerShell 单引号字面量前面加 `&`（带引号的命令名要它才叫得起来）。
const WIN_ENTRY_LITERAL: &str = r"'C:\Users\u\.cc-monitor\bin\ccm.exe'";

const WINDOWS: Facts = Facts {
    windows: true,
    is_dir: |_| true,
    entry: || Some(r"C:\Users\u\.cc-monitor\bin\ccm.exe".into()),
};

/// 每一形的成品（期望手写）：都以 `ccm ` 打头，起 agent 的那几格带上交回调用方的那个身份 token。
#[test]
fn every_local_launch_shape_is_one_ccm_line() {
    // resume：身份 token 就是那个 sid。
    let mut r = req(resume());
    r.tmux_name = Some("p-cc".into());
    r.account = named("work");
    let out = go(&r, &POSIX).unwrap();
    assert_eq!(
        out.cmd,
        format!("ccm --resume {SID} -- --ccm-tmux=p-cc --ccm-sid={SID} --ccm-agent claude --account work --ccm-launch-id {SID}")
    );
    assert_eq!(out.launch_id.as_deref(), Some(SID));

    // 账号 0 ⇒ `--base`；缺席 ⇒ 继承（一个账号旗标都不吐）。
    r.account = Some(AccountAsk::Base);
    assert!(go(&r, &POSIX).unwrap().cmd.contains(" --base "));
    r.account = None;
    let inherit = go(&r, &POSIX).unwrap().cmd;
    assert!(
        !inherit.contains("--base") && !inherit.contains("--account"),
        "{inherit}"
    );

    // 新起：身份 token 是现铸的 nonce，命令里带的就是交回去的那一个。
    let mut n = req(LocalAction::New);
    n.tmux_name = Some("w-cc".into());
    let out = go(&n, &POSIX).unwrap();
    let tok = out.launch_id.clone().unwrap();
    assert_eq!(tok.len(), 36);
    assert_eq!(
        out.cmd,
        format!("ccm -- new --ccm-tmux=w-cc --ccm-agent claude --ccm-launch-id {tok}")
    );

    // 自定义启动命令 ⇒ `--launcher`。
    n.launcher = Some("ccr code".into());
    assert!(go(&n, &POSIX)
        .unwrap()
        .cmd
        .contains(" --launcher 'ccr code'"));

    // 没有会话名 ⇒ 直路（命令照样是那一行 `ccm …`，只是不建 tmux）。
    let mut d = req(resume());
    d.account = Some(AccountAsk::Base);
    assert_eq!(
        go(&d, &POSIX).unwrap().cmd,
        format!("ccm --resume {SID} -- --ccm-agent claude --base --ccm-launch-id {SID}")
    );

    // 接回：不起 agent ⇒ 不带身份 token。
    let mut a = req(LocalAction::Attach);
    a.tmux_name = Some("p-cc".into());
    let out = go(&a, &POSIX).unwrap();
    assert_eq!(out.cmd, "ccm -- --attach p-cc");
    assert_eq!(out.launch_id, None);
}

/// 身份 token 落进 agent 进程环境、并交回调用方：交回去的那一个 == 那一行 `ccm` 自己解析出来的 `--ccm-launch-id`
/// （`ccm` 在最终 exec 那一处把它放进 `CCM_LAUNCH_ID`，那一半由 `ccm/plan_tests.rs` 钉）。resume 用 sid，新起用现铸的 nonce。
#[test]
fn the_identity_token_is_planted_and_handed_back() {
    let parsed_id = |cmd: &str| -> String {
        let words: Vec<String> = cmd.split_whitespace().map(str::to_string).collect();
        // Windows 那一行是 `& '<入口>' …`。
        let skip = if words[0] == "&" { 2 } else { 1 };
        assert!(words[0] == "ccm" || words[1] == WIN_ENTRY_LITERAL, "{cmd}");
        match crate::control::ccm::argv::parse(&words[skip..]) {
            Ok(crate::control::ccm::argv::Parsed::Opts(o)) => o.launch_id,
            other => panic!("`ccm` 不认这一行：{cmd}（{:?}）", other.err()),
        }
    };
    let mut r = req(resume());
    r.tmux_name = Some("p-cc".into());
    let out = go(&r, &POSIX).unwrap();
    assert_eq!(out.launch_id.as_deref(), Some(SID));
    assert_eq!(parsed_id(&out.cmd), SID);
    let a = go(&req(LocalAction::New), &POSIX).unwrap();
    let b = go(&req(LocalAction::New), &POSIX).unwrap();
    let (ta, tb) = (a.launch_id.clone().unwrap(), b.launch_id.clone().unwrap());
    assert_ne!(ta, tb, "两次新起铸出了同一个 token");
    assert_eq!(parsed_id(&a.cmd), ta);
    assert_eq!(
        parsed_id(&go(&req(LocalAction::New), &WINDOWS).unwrap().cmd).len(),
        36
    );
}

/// Windows 本机：没有 tmux ⇒ 一律直路（`ccm` 在那个 PowerShell 窗口里起 agent）；接回说不出 ⇒ 拒。
#[test]
fn windows_launches_go_the_direct_way_and_attach_is_refused() {
    let mut r = req(resume());
    r.tmux_name = Some("p-cc".into());
    r.account = named("work");
    assert_eq!(
        go(&r, &WINDOWS).unwrap().cmd,
        format!("& {WIN_ENTRY_LITERAL} --resume {SID} -- --ccm-agent claude --account work --ccm-launch-id {SID}")
    );
    let mut a = req(LocalAction::Attach);
    a.tmux_name = Some("p-cc".into());
    assert_eq!(
        go(&a, &WINDOWS).unwrap_err(),
        copy_core::copy_text("rsHistory.attach.windows", &[])
    );
}

/// 拒的那几形：新起的目录不在 · 启动命令带注入字符 · sid 不合法 · 接回没名字。
#[test]
fn bad_inputs_are_refused_before_anything_is_rendered() {
    let gone = Facts {
        windows: false,
        is_dir: |_| false,
        entry: || Some("ccm".into()),
    };
    let mut n = req(LocalAction::New);
    n.cwd = Some("/nope".into());
    assert!(go(&n, &gone).unwrap_err().contains("/nope"));
    let mut l = req(LocalAction::New);
    l.launcher = Some("claude; rm -rf ~".into());
    assert!(go(&l, &POSIX).is_err());
    let bad = req(LocalAction::Resume {
        sid: "--dangerously-skip-permissions".into(),
    });
    assert!(go(&bad, &POSIX).is_err());
    assert!(go(&req(LocalAction::Attach), &POSIX).is_err());
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
    r.account = Some(AccountAsk::Base);
    r.tmux_name = Some(name);
    let cmd = go(&r, &POSIX).expect("渲染不出来 —— e2e 无对象可跑").cmd;
    println!("P3T_CMD<<<{cmd}>>>");
}
