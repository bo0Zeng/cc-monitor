//! **历史里点「恢复」→ 真起的是那个会话的那一家**（端到端，正反两向）。
//!
//! 一个隔离的家目录里放一份合成的 Claude 记录与一份合成的 Codex 记录（只有结构，文字是编的），`PATH` 上只有两个假启动器
//! （`claude` · `codex`：把自己叫什么、收到什么参数、账号变量在不在写进一个文件就退）。子进程里走生产那一整条：
//! 历史清单（`history_list::machine_listing`，那一行带 `agent`）→ 界面那一问的入参（`launch-local`，`agent` 取那一行的）→ 本机那一行 `ccm …`
//! （`local::plan_argv`）→ `ccm` 自己（`control::ccm::run`，exec 掉子进程）→ 假启动器。
//! Codex 那一行 ⇒ `codex --no-daemon resume <sid>`；Claude 那一行 ⇒ `claude --resume <sid>`。两行都不带账号变量。
//! 买不到：界面里点那一下（GUI）· 真 codex / 真 claude · tmux 那一形（直路那一形就是 pane 里最终跑的那一趟）。

use std::path::{Path, PathBuf};

const CHILD_MARK: &str = "HRE_CHILD";
const CHILD_TEST_NAME: &str =
    "control::launch_render::history_resume_e2e::history_resume_child_entry_point";
const SID_CLAUDE: &str = "0000c1a0-0000-4000-8000-000000000001";
const SID_CODEX: &str = "0000c0de-0000-4000-8000-000000000002";
const CWD: &str = "/w/proj";

/// 一个假启动器：第一个词是它被当作什么叫的，后面是收到的参数，最后是账号变量在不在。
fn fake_launcher(bin: &Path, name: &str) {
    let p = bin.join(name);
    std::fs::write(
        &p,
        format!(
            "#!/bin/sh\n{{ printf '%s' {name}; for a in \"$@\"; do printf ' %s' \"$a\"; done; \
             printf ' | CLAUDE_CONFIG_DIR=%s\\n' \"${{CLAUDE_CONFIG_DIR-<unset>}}\"; }} > \"$HRE_OUT\"\n"
        ),
    )
    .expect("写假启动器");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

/// 隔离的家目录：Claude 记录树里一份、Codex 日期树里一份（结构照各自的格式，文字是编的）。
fn sandbox(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("hre-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let home = root.join("home");
    let proj = home.join(".claude/projects/-w-proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(
        proj.join(format!("{SID_CLAUDE}.jsonl")),
        format!(
            "{{\"type\":\"user\",\"uuid\":\"u1\",\"sessionId\":\"{SID_CLAUDE}\",\"cwd\":\"{CWD}\",\
             \"timestamp\":\"2026-10-02T10:00:00Z\",\"message\":{{\"role\":\"user\",\"content\":\"占位一\"}}}}\n"
        ),
    )
    .unwrap();
    let day = home.join(".codex/sessions/2026/10/02");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::write(
        day.join(format!("rollout-2026-10-02T10-00-00-{SID_CODEX}.jsonl")),
        format!(
            "{{\"timestamp\":\"2026-10-02T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{SID_CODEX}\",\"cwd\":\"{CWD}\"}}}}\n\
             {{\"timestamp\":\"2026-10-02T10:00:01Z\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\
             \"content\":[{{\"type\":\"input_text\",\"text\":\"占位二\"}}]}}}}\n"
        ),
    )
    .unwrap();
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    fake_launcher(&bin, "claude");
    fake_launcher(&bin, "codex");
    std::fs::create_dir_all(root.join("tmux")).unwrap();
    root
}

/// 起一个子进程恢复 `sid`（环境全清：只有隔离的家目录与只装着两个假启动器的 `PATH`，没有 tmux 可连）。回假启动器写下的那一行。
fn resume_in_child(root: &Path, sid: &str) -> String {
    let out = root.join(format!("out-{sid}"));
    let exe = std::env::current_exe().expect("测试二进制自己的路径");
    let run = std::process::Command::new(exe)
        .args([
            CHILD_TEST_NAME,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env_clear()
        .env(CHILD_MARK, sid)
        .env("HOME", root.join("home"))
        .env("PATH", root.join("bin"))
        .env("TMUX_TMPDIR", root.join("tmux"))
        .env("HRE_OUT", &out)
        .current_dir(root)
        .output()
        .expect("起子进程");
    std::fs::read_to_string(&out).unwrap_or_else(|_| {
        panic!(
            "假启动器没被叫到（子进程 {:?}）：\n{}\n{}",
            run.status,
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        )
    })
}

/// ★ 正反两向：Codex 那一行起的是 codex（子命令形 ＋ 不连共享后台），Claude 那一行起的是 claude（旗标形），都不带账号变量。
#[test]
fn resuming_from_history_starts_the_agent_the_session_belongs_to() {
    let root = sandbox("both");
    assert_eq!(
        resume_in_child(&root, SID_CODEX),
        format!("codex --no-daemon resume {SID_CODEX} | CLAUDE_CONFIG_DIR=<unset>\n")
    );
    assert_eq!(
        resume_in_child(&root, SID_CLAUDE),
        format!("claude --resume {SID_CLAUDE} | CLAUDE_CONFIG_DIR=<unset>\n")
    );
    std::fs::remove_dir_all(&root).ok();
}

/// 子进程入口（不是判据 ⇒ `#[ignore]`）：从历史成品里找到那一行，照界面那样问本机那一行，交给 `ccm` 跑（exec 掉自己）。
#[test]
#[ignore = "子进程入口：只在被父判据用 HRE_CHILD 拉起时才跑"]
fn history_resume_child_entry_point() {
    let Ok(sid) = std::env::var(CHILD_MARK) else {
        return;
    };
    let listing = crate::history::history_list::machine_listing().expect("历史清单");
    let row = listing["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|r| r["sessionId"] == sid.as_str())
        .cloned()
        .unwrap_or_else(|| panic!("历史里没有 {sid}：{listing}"));
    let agent = row["agent"].as_str().expect("历史那一行说不出是哪一家");
    let face = crate::agents::pick_kind(Some(agent)).expect("认得这一家").1;
    // 界面那一问（`launch-local`）：哪一家取那一行的，启动器缺省是那一家的；没有上次的号 ⇒ 账号缺席；没有 tmux ⇒ 直路。
    let req: super::local::LocalLaunchRequest = serde_json::from_value(serde_json::json!({
        "agent": agent,
        "action": { "kind": "resume", "sid": sid },
        "cwd": row["projectPath"],
        "launcher": null,
        "tmuxName": null,
        "defaultLauncher": face.default_launcher,
    }))
    .expect("入参照线上形状收得下");
    let facts = super::local::Facts {
        windows: false,
        is_dir: |_| true,
        entry: || Some("ccm".into()),
    };
    let argv = super::local::plan_argv(
        &req,
        &crate::control::launch_account::Settled::Unsaid,
        &facts,
        false,
    )
    .expect("渲得出那一行");
    eprintln!("ccm 那一行：{argv:?}");
    let code = crate::control::ccm::run(&argv[1..], &argv[..1], |_| Vec::new());
    panic!("ccm 没有 exec 掉自己（退出码 {code}）");
}
