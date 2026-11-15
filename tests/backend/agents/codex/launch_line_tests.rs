//! Codex 这一行喂给核心，拼出来的那一串 codex 起得来：恢复 · 新起都带权限档那一对（`-a` · `-s`）与模型（`-m`）。
//!
//! 走生产那几步：起会话那一步交给渲染的 `Launch`（权限档 · 模型的参数由 `session_open::launch_args` 照 Codex 那一行拼，垫在 `preset_args` 里）→ 那一行 `ccm …` 的 argv 形（`launch_render::argv_of`）→ ccm 解析那一行（`ccm_line::argv::parse_layered`，不经别名）
//! → 计划（`plan::build_among`，注册表只放 `codex::ADAPTER`）→ 直路最终 exec 的 argv。
//!
//! 期望值手写，先后照 codex-rs rust-v0.167.0 的命令行：`MultitoolCli` 顶层摊平 `TuiCli`（`--no-daemon` 在子命令之前收），
//! `resume` 子命令是 `ResumeCommand { SESSION_ID 位置参数, .., SessionTuiCli(TuiCli) }`（sid 之后的 `-a` · `-s` · `-m`
//! 在 codex-rs 命令行那个 crate 的入口里并进起会话那一组）；`-a` ＝ `--ask-for-approval`（`on-request`）· `-s` ＝ `--sandbox`
//! （`read-only` / `workspace-write`）· `-m` ＝ `--model`（`SharedCliOptions`）。
//! 挂在核心起会话那个包（`backend-control`）自己的测试树：适配层那个包够不到这一层，这里够得着 Codex 那一行。

use crate::ccm::plan::{self, Env, Plan};
use crate::ccm_line::argv::Parsed;
use crate::launch_account::Settled;
use crate::launch_render::{self as lr, ccm_invocation::test_support, Act, Here};
use backend_agents::codex::{ADAPTER, AGENT_KIND};

const SID: &str = "0000c0de-0000-4000-8000-0000000000a8";
const MODEL: &str = "gpt-5-codex";
const CCM: &str = "/usr/local/bin/ccm";

fn owned(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// 选的档 · 模型 ⇒ 交给 Codex 的那几个参数：走核心起会话那一处的拼法（`session_open::launch_args`），喂的是 Codex 这一行。
fn picked(mode: &str, model: &str) -> Vec<String> {
    crate::session_open::launch_args(ADAPTER.launch.as_ref(), Some(mode), Some(model), None)
}

/// 那一行 `ccm …`（argv 形，本机直路、号不表态）喂给 ccm 解析、再算计划 ⇒ 直路最终 exec 的那一串。
fn exec_argv(act: Act, preset_args: Vec<String>) -> Vec<String> {
    let l = lr::Launch {
        cwd: Some("/w/proj".into()),
        preset_args,
        ..test_support::launch(AGENT_KIND, act)
    };
    let line = lr::argv_of(
        &l,
        &Settled::Unsaid,
        Here::Local { windows: false },
        false,
        &test_support::caps_without_one(None),
        CCM,
    )
    .expect("那一行渲得出");
    assert_eq!(line.first().map(String::as_str), Some(CCM), "{line:?}");
    let (parsed, _) =
        crate::ccm_line::argv::parse_layered(&[], &line[1..]).expect("ccm 解析得动那一行");
    let Parsed::Opts(o) = parsed else {
        panic!("那一行该是起会话：{parsed:?}");
    };
    let env = Env {
        home: "/home/pi".into(),
        pwd: "/w/proj".into(),
        accts_manifest: "/nonexistent/accounts.json".into(),
        self_argv: vec![CCM.into()],
        ..Default::default()
    };
    let registry = [ADAPTER];
    match plan::build_among(&registry, &o, &env, &plan::test_support::no_library, None)
        .expect("算得出计划")
    {
        Plan::Direct(d) => d.argv,
        other => panic!("直路那一形：{other:?}"),
    }
}

/// ★ 恢复 ＋ `:workspace` ＋ 一个模型 ⇒ `codex --no-daemon resume <sid> -a on-request -s workspace-write -m <模型>`：
/// `--no-daemon` 在子命令之前，sid 紧跟 `resume`，三对旗标跟在 sid 后面（`codex resume` 收同一组）。
#[test]
fn a_resume_carries_the_mode_pair_and_the_model_after_the_sid() {
    assert_eq!(
        exec_argv(Act::Resume(SID.into()), picked(":workspace", MODEL)),
        owned(&[
            "codex",
            "--no-daemon",
            "resume",
            SID,
            "-a",
            "on-request",
            "-s",
            "workspace-write",
            "-m",
            MODEL,
        ])
    );
}

/// ★ 新起 ＋ `:read-only` ＋ 一个模型 ⇒ `codex --no-daemon -a on-request -s read-only -m <模型>`（同一组旗标，没有子命令）。
#[test]
fn a_new_session_carries_the_same_mode_pair_and_model() {
    assert_eq!(
        exec_argv(Act::New, picked(":read-only", MODEL)),
        owned(&[
            "codex",
            "--no-daemon",
            "-a",
            "on-request",
            "-s",
            "read-only",
            "-m",
            MODEL,
        ])
    );
}

/// 只认不选的那一档（`:danger-full-access`）恢复时一个旗标不出，模型照带；什么都没选 ⇒ 只剩 `resume <sid>`。
#[test]
fn a_shown_only_mode_adds_nothing_and_nothing_picked_is_just_the_resume() {
    assert_eq!(
        exec_argv(
            Act::Resume(SID.into()),
            picked(":danger-full-access", MODEL)
        ),
        owned(&["codex", "--no-daemon", "resume", SID, "-m", MODEL])
    );
    assert_eq!(
        exec_argv(Act::Resume(SID.into()), Vec::new()),
        owned(&["codex", "--no-daemon", "resume", SID])
    );
}
