//! 上游选择那一层（`Accounts`）自己的判据：锁的持有范围。

use super::*;
use crate::relay::{Ask, Mode, RouteKey};
use std::sync::mpsc;

fn ask(body: &[u8]) -> Ask<'_> {
    Ask {
        label: "",
        origin: None,
        body,
        heads: &[],
    }
}

/// 一发卡在 `act` 里（连上游连得慢 · 写请求体写不动，都在 `act` 里）时凭据文件被改了，
/// 下一发照常拿到去处、看到的是新表 —— 不排在卡住那一发后面。
///
/// 修之前：`resolve` 握着表的读锁跑 `act`；下一发先重载，要写锁 ⇒ 等到卡住那一发的 `act` 返回，
/// 而 std 的 `RwLock` 有写者在等时新读者也进不来 ⇒ 所有中转请求一起卡住。
#[test]
fn a_request_stuck_inside_act_does_not_hold_up_a_reload_or_the_next_request() {
    let dir = test_home::scratch("reload");
    let path = dir.join("creds.json");
    let upstreams = Upstreams::from_env(&|_| None).expect("内置默认必须解析得了");
    let (empty, _, _) = table::build(
        Vec::new(),
        CREDENTIALS_FILE_AGENT,
        upstreams.of_credentials_file(),
    );
    let accounts = std::sync::Arc::new(
        Accounts::new(empty, upstreams, crate::platform::fs::Own::for_tests())
            .reloading_from(Reload::new(path.clone(), None)),
    );
    let key = RouteKey {
        seg1: CREDENTIALS_FILE_AGENT.to_string(),
        seg2: "acct".to_string(),
    };

    // 第一发：进了 `act` 就停住，等第二发拿到去处才出来（兜底等到墙钟上限）。
    // ⇒ 不量快慢，只看先后：第二发是不是在第一发还卡在 `act` 里的时候就答到了。
    let (entered_tx, entered_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let first = {
        let accounts = std::sync::Arc::clone(&accounts);
        let key = key.clone();
        std::thread::spawn(move || {
            let mut answered_meanwhile = false;
            accounts.resolve(Mode::Substitute, &key, &ask(b"{}"), &mut |_| {
                let _ = entered_tx.send(());
                answered_meanwhile = done_rx.recv_timeout(guard_core::wall::CEILING).is_ok();
            });
            answered_meanwhile
        })
    };
    entered_rx
        .recv_timeout(guard_core::wall::CEILING)
        .expect("第一发没进到 act");

    // 改凭据文件（人手写一份 · 只给属主读写）。
    std::fs::write(&path, b"{\"api_key\":\"sk-ant-RELOAD-WHILE-STUCK\"}").expect("写夹具");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("改权限");
    }

    // 第二发：另一线程发。
    let second = {
        let accounts = std::sync::Arc::clone(&accounts);
        let key = key.clone();
        std::thread::spawn(move || {
            accounts.resolve(Mode::Substitute, &key, &ask(b"{}"), &mut |_| {
                let _ = done_tx.send(());
            });
        })
    };
    let answered_meanwhile = first.join().expect("第一发");
    second.join().expect("第二发");
    assert!(
        answered_meanwhile,
        "第一发卡在 act 里时，第二发拿不到去处：重载排在卡住那一发的读锁后面"
    );
    // 非空对照：第二发确实重载了（看到的是新表，不是靠没重载才快）。
    assert_eq!(accounts.table().len(), 1, "第二发没换上新表");
    let _ = std::fs::remove_dir_all(&dir);
}
