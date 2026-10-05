//! 自动起算的帧命令：只收这台的订阅号、坏时段整份拒、改了开关忘掉试过哪一段；读出来的「下一次」按时段与本地钟算。临时家目录，不起进程。

use super::*;
use crate::accounts::quota::ledger::{self, Ledger};
use crate::accounts::quota::rotation::RotationStore;
use crate::accounts::upstream_select::rotate::{Hop, LibAccount, Library};
use std::sync::Arc;

const UTC: &dyn Fn(u64) -> i64 = &|_| 0;
/// 2026-10-05 00:00:00 UTC。
const DAY0: u64 = 1_791_158_400;

fn at(h: u64, m: u64) -> u64 {
    DAY0 + h * 3600 + m * 60
}

// ── 帧命令 ────────────────────────────────────────────────────────────────

struct Home {
    root: PathBuf,
}

impl Home {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ccm-autoface-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("accts").join("b")).expect("mkdir");
        Self { root }
    }

    /// 账号库：b（订阅号）· api（按量号）· 账号 0（没有自己的配置目录）。
    fn ctx(&self) -> Ctx {
        let accts = self.root.join("accts");
        let library: crate::accounts::upstream_select::rotate::LibraryRead =
            Arc::new(move |_| Library {
                enabled: true,
                accounts: vec![
                    LibAccount {
                        id: "0".into(),
                        dir: None,
                        api: false,
                    },
                    LibAccount {
                        id: "b".into(),
                        dir: Some(accts.join("b")),
                        api: false,
                    },
                    LibAccount {
                        id: "api".into(),
                        dir: Some(accts.join("api")),
                        api: true,
                    },
                ],
            });
        Ctx {
            hop: Hop::new(
                Arc::new(RotationStore::at(Some(self.root.join(rotation::FILE_NAME)))),
                Arc::new(Ledger::at(Some(self.root.join(ledger::FILE_NAME)))),
                Some(self.root.clone()),
                library,
                None,
            ),
            rows: Box::new(|_, a| (a == "api").then_some(true)),
            live: Box::new(Default::default),
        }
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn set_takes_only_this_machines_subscription_accounts_and_whole_windows() {
    let home = Home::new("set");
    let ctx = home.ctx();
    let mem = Memory::default();
    for args in [
        json!({"account": "api", "enabled": true}),
        json!({"account": "0", "enabled": true}),
        json!({"account": "zz", "enabled": true}),
        json!({"account": "b"}),
        json!({"account": "b", "enabled": "yes"}),
        json!({"account": "b", "window": {"from": "07:00", "to": "07:00"}}),
        json!({"account": "b", "enabled": true, "x": 1}),
    ] {
        let e = set_with(&ctx, &mem, &args).expect_err("should refuse");
        assert_eq!(e.0, "bad_args", "{args}");
    }
    assert!(
        !home.root.join(rotation::FILE_NAME).exists(),
        "拒了 ⇒ 一个字节不写"
    );
    mem.note_tried("b", 0);
    set_with(
        &ctx,
        &mem,
        &json!({"account": "b", "enabled": true, "window": {"from": "22:00", "to": "06:00"}}),
    )
    .expect("ok");
    assert_eq!(mem.tried("b"), None, "改了开关 ⇒ 忘掉试过哪一段");
    let v = read_with(&ctx, &mem, at(23, 0), UTC, true);
    assert_eq!(
        v["accounts"],
        json!([{"account": "b", "enabled": true, "window": {"from": "22:00", "to": "06:00"},
                "next": "now", "running": false, "inactive": true}])
    );
    let v = read_with(&ctx, &mem, at(12, 0), UTC, false);
    assert_eq!(
        v["accounts"][0]["next"],
        json!({"outsideWindow": {"at": at(22, 0)}})
    );
    set_with(&ctx, &mem, &json!({"account": "b", "window": null})).expect("ok");
    let v = read_with(&ctx, &mem, at(12, 0), UTC, false);
    assert_eq!(v["accounts"][0].get("window"), None);
    assert_eq!(v["accounts"][0]["next"], "now");
}
