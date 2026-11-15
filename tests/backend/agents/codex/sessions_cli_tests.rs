//! 删 Codex 会话那一条命令怎么拼（注册表 `SessionDelete::Command` 那一格）。经删会话那道门真起一回（替身 `codex` 记下 argv）
//! 要上层，挂在 `tests/backend/stream/inbound/files_session_face_tests.rs`。
use super::*;

const SID: &str = "0199aaaa-0000-7000-8000-0000000000e1";

/// 家里有这个 sid 的那一份 ⇒ `codex delete <uuid> --force`，认同一个家（`CODEX_HOME`），回包交那一份记录在哪。
#[test]
fn a_session_in_this_home_is_deleted_by_codex_delete_force() {
    let home = test_home::scratch("codex-delete");
    let day = home.join("sessions/2026/10/20");
    std::fs::create_dir_all(&day).unwrap();
    let file = day.join(format!("rollout-2026-10-20T00-00-00-{SID}.jsonl"));
    std::fs::write(&file, "{}\n").unwrap();

    let c = delete_command(&home, SID).expect("家里有这一份");
    assert_eq!(c.program, "codex");
    assert_eq!(c.args, ["delete", SID, "--force"]);
    assert_eq!(c.env, [("CODEX_HOME", home.as_os_str().to_os_string())]);
    assert_eq!(c.record, file);
    assert!(file.exists(), "拼命令那一下不碰记录");
    let _ = std::fs::remove_dir_all(&home);
}

/// 注册表 Codex 那一格声明的是「跑一条命令」那一形，指的正是这一条（不是删记录文件）。
#[test]
fn the_registry_deletes_codex_sessions_by_this_command() {
    match super::super::RECORDS.delete {
        Some(crate::agents::SessionDelete::Command(f)) => {
            assert!(
                f as *const () as usize == delete_command as *const () as usize,
                "指着别的命令"
            )
        }
        _ => panic!("Codex 那一格该是跑命令那一形"),
    }
}

/// 家里没有这个 sid（别家的会话 · 删过了）⇒ 不是这一家的，不出命令。
#[test]
fn a_sid_this_home_does_not_have_is_not_ours() {
    let home = test_home::scratch("codex-delete-none");
    std::fs::create_dir_all(home.join("sessions")).unwrap();
    assert!(delete_command(&home, SID).is_none());
    assert!(delete_command(&home, "not a sid").is_none());
    let _ = std::fs::remove_dir_all(&home);
}

/// 归档了的会话照样删得掉：那一份在归档根下，删它的还是同一条命令，回包交的是归档根下那一份。
#[test]
fn an_archived_session_is_deleted_too() {
    let home = test_home::scratch("codex-delete-archived");
    let shelf = home.join("archived_sessions");
    std::fs::create_dir_all(home.join("sessions")).unwrap();
    std::fs::create_dir_all(&shelf).unwrap();
    let parked = shelf.join(format!("rollout-2026-10-20T00-00-00-{SID}.jsonl"));
    std::fs::write(&parked, "{}\n").unwrap();
    let c = delete_command(&home, SID).expect("归档根下有这一份");
    assert_eq!(c.args, ["delete", SID, "--force"]);
    assert_eq!(c.record, parked);
    let _ = std::fs::remove_dir_all(&home);
}

/// 归档 · 取消归档交 Codex 自己的命令（codex-rs `rust-v0.167.0`：`codex archive <SESSION>` · `codex unarchive <SESSION>`，
/// Codex 把 rollout 在 `sessions/YYYY/MM/DD/` 与平铺的 `archived_sessions/` 之间挪、状态库跟着改）：
/// 没归档的那一份在 `sessions/` 里 ⇒ 出归档那一条；归档了的在 `archived_sessions/` 里 ⇒ 出取消归档那一条；两边都认同一个家。
#[test]
fn archive_and_unarchive_are_codex_subcommands_in_the_same_home() {
    let home = test_home::scratch("codex-archive");
    let day = home.join("sessions/2026/10/20");
    std::fs::create_dir_all(&day).unwrap();
    let live = day.join(format!("rollout-2026-10-20T00-00-00-{SID}.jsonl"));
    std::fs::write(&live, "{}\n").unwrap();

    let a = archive_command(&home, SID).expect("没归档的那一份");
    assert_eq!(
        (a.program.as_str(), a.args.clone()),
        ("codex", vec!["archive".to_string(), SID.to_string()])
    );
    assert_eq!(a.env, [("CODEX_HOME", home.as_os_str().to_os_string())]);
    assert!(
        unarchive_command(&home, SID).is_none(),
        "没归档 ⇒ 不出取消归档"
    );
    assert!(!is_archived(&live));

    let shelf = home.join("archived_sessions");
    std::fs::create_dir_all(&shelf).unwrap();
    let parked = shelf.join(live.file_name().unwrap());
    std::fs::rename(&live, &parked).unwrap();
    assert!(archive_command(&home, SID).is_none(), "归档了 ⇒ 不再出归档");
    let u = unarchive_command(&home, SID).expect("归档了的那一份");
    assert_eq!(u.args, ["unarchive", SID]);
    assert_eq!(u.env, [("CODEX_HOME", home.as_os_str().to_os_string())]);
    assert!(is_archived(&parked));
    assert!(parked.exists(), "拼命令那一下不碰记录");

    assert!(archive_command(&home, "not a sid").is_none());
    assert!(unarchive_command(&home, "not a sid").is_none());
    let _ = std::fs::remove_dir_all(&home);
}

/// 注册表 Codex 那一格声明了归档面，三手指的正是这几个。
#[test]
fn the_registry_archives_codex_sessions_by_these_commands() {
    let f = super::super::RECORDS.archive.expect("Codex 声明了归档面");
    let same = |a: usize, b: usize| a == b;
    assert!(same(
        f.archive as *const () as usize,
        archive_command as *const () as usize
    ));
    assert!(same(
        f.unarchive as *const () as usize,
        unarchive_command as *const () as usize
    ));
    assert!(same(
        f.archived as *const () as usize,
        is_archived as *const () as usize
    ));
}
