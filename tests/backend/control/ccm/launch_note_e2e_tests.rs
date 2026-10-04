//! **经 `ccm` 用某个号起一次 ⇒ 那个号记到那条会话名下**（端到端，正反两向）。
//!
//! 隔离的家目录里一份账号库清单（号 `x`）、`PATH` 上只有一个假 `claude`（把自己的 pid 写进一个文件就退）。
//! 子进程走生产那一条：`launch-local` 的入参 → 本机那一行 `ccm …`（`local::plan_argv`）→ `ccm` 自己（exec 掉子进程）→ 假启动器。
//! 父进程拿假启动器报的 pid 当 pidfile 里的 pid，照观测侧那样认便条（`launch_account::adopt`）⇒ 读回那条会话的号。
//! 买不到：真 claude 写 pidfile 那一下（这里由父进程代为「看见」）· Windows 那一臂（起好子进程再写便条）。

use std::path::{Path, PathBuf};

const CHILD_MARK: &str = "LNE_CHILD";
const CHILD_TEST_NAME: &str = "control::ccm::launch_note_e2e::launch_note_child_entry_point";
const SID: &str = "0000a11c-0000-4000-8000-000000000003";

fn sandbox(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("lne-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let home = root.join("home");
    let accts = home.join(".cc-monitor/accounts");
    for n in ["x", "y"] {
        std::fs::create_dir_all(accts.join(n)).unwrap();
        std::fs::write(accts.join(n).join(".credentials.json"), "{}").unwrap();
    }
    std::fs::write(
        accts.join("accounts.json"),
        format!(
            "{{\"version\":1,\"accounts\":[{{\"name\":\"x\",\"configDir\":\"{}\",\"isDefault\":false}},\
             {{\"name\":\"y\",\"configDir\":\"{}\",\"isDefault\":true}}]}}",
            accts.join("x").display(),
            accts.join("y").display()
        ),
    )
    .unwrap();
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let p = bin.join("claude");
    std::fs::write(
        &p,
        "#!/bin/sh\necho \"$$ ${CLAUDE_CONFIG_DIR-}\" > \"$LNE_OUT\"\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    root
}

/// 子进程里起一次（`account` 是 `launch-local` 那一格的 JSON），回假启动器报的 pid 与它收到的账号目录。
fn launch_in_child(root: &Path, account: &str) -> (u32, String) {
    let out = root.join("pid");
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
        .env(CHILD_MARK, account)
        .env("HOME", root.join("home"))
        .env("PATH", root.join("bin"))
        .env("LNE_OUT", &out)
        .current_dir(root)
        .output()
        .expect("起子进程");
    std::fs::read_to_string(&out)
        .ok()
        .and_then(|s| {
            let (pid, dir) = s.trim().split_once(' ').unwrap_or((s.trim(), ""));
            Some((pid.parse().ok()?, dir.to_string()))
        })
        .unwrap_or_else(|| {
            panic!(
                "假启动器没被叫到（子进程 {:?}）：\n{}\n{}",
                run.status,
                String::from_utf8_lossy(&run.stdout),
                String::from_utf8_lossy(&run.stderr)
            )
        })
}

#[test]
fn launching_through_ccm_with_an_account_records_it_for_that_session() {
    use crate::control::launch_account::{adopt, last_of, NOTES_DIR};
    let root = sandbox("named");
    let data = root.join("home/.cc-monitor");
    let (pid, _) = launch_in_child(&root, "{\"kind\":\"named\",\"name\":\"x\"}");
    assert!(
        data.join(NOTES_DIR).join(format!("{pid}.json")).exists(),
        "ccm 没给那个进程留便条"
    );
    // 观测侧看见 pid 的会话 SID（进程已经退了：起始时刻说不出 ⇒ 便条作数）。
    assert!(adopt(&data, pid, SID, None, &|_| None).unwrap());
    assert_eq!(last_of(Some(&data), SID).as_deref(), Some("x"));
    // 再跟随着起一次：上次的号 x 压过这台的默认号 y。
    let (_, dir) = launch_in_child(&root, "{\"kind\":\"follow\"}");
    assert_eq!(
        dir,
        data.join("accounts/x").display().to_string(),
        "跟随没落到上次的号"
    );
    std::fs::remove_dir_all(&root).ok();

    // 反向：账号 0 ⇒ 不留便条、不记；没有记录时跟随落默认号 y。
    let root = sandbox("base");
    let data = root.join("home/.cc-monitor");
    let (pid, _) = launch_in_child(&root, "{\"kind\":\"base\"}");
    assert!(
        !data.join(NOTES_DIR).join(format!("{pid}.json")).exists(),
        "账号 0 也留了便条"
    );
    assert!(!adopt(&data, pid, SID, None, &|_| None).unwrap());
    assert_eq!(last_of(Some(&data), SID), None);
    let (_, dir) = launch_in_child(&root, "{\"kind\":\"follow\"}");
    assert_eq!(
        dir,
        data.join("accounts/y").display().to_string(),
        "没有记录时跟随没落到默认号"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
#[ignore = "子进程入口：只在被父判据用 LNE_CHILD 拉起时才跑"]
fn launch_note_child_entry_point() {
    let Ok(account) = std::env::var(CHILD_MARK) else {
        return;
    };
    let req: crate::control::launch_render::local::LocalLaunchRequest =
        serde_json::from_value(serde_json::json!({
            "agent": "claude",
            "action": { "kind": "resume", "sid": SID },
            "cwd": null,
            "launcher": null,
            "account": serde_json::from_str::<serde_json::Value>(&account).unwrap(),
            "tmuxName": null,
            "defaultLauncher": "claude",
        }))
        .expect("入参照线上形状收得下");
    let facts = crate::control::launch_render::local::Facts {
        windows: false,
        is_dir: |_| true,
    };
    // 判号走生产那一份事实（这台的账号库 · 这台家里的记录）。
    let account = crate::faces::launch_face::with_facts("claude", |f| {
        crate::control::launch_render::local::settle(&req, f)
    })
    .expect("号判得出来");
    let argv = crate::control::launch_render::local::plan_argv(&req, &account, &facts, false)
        .expect("渲得出那一行");
    let code = crate::control::ccm::run(&argv[1..], &argv[..1], |_| Vec::new());
    panic!("ccm 没有 exec 掉自己（退出码 {code}）");
}
