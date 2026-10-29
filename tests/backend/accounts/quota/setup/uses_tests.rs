//! 配置编辑层那份账本：没有随包策略（新会话不轮换）· 新会话载入 `forNew` 那份的副本（读不懂 ⇒ 不轮换）。临时家目录，不碰真文件。

use super::*;
use crate::accounts::quota::rotation::RotationStore;
use crate::accounts::quota::setup::shelf;
use serde_json::json;
use std::path::PathBuf;

struct Home(PathBuf);

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Home {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ccm-rotuses-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(shelf::DIR_NAME)).expect("mkdir");
        Self(root)
    }
    fn own(&self) -> Own {
        Own::issue_data_at(&self.0)
    }
    fn file(&self) -> PathBuf {
        self.0.join(rotation::FILE_NAME)
    }
    fn sdir(&self) -> PathBuf {
        self.0.join(shelf::DIR_NAME)
    }
    fn store(&self) -> RotationStore {
        RotationStore::at(Some(self.file()))
    }
    fn setup(&self) -> Setup {
        Setup::of(Arc::new(self.store()))
    }
}

/// ★ 没有随包策略：策略目录空着、没有 `rotation-uses.json` ⇒ 新会话不轮换。
#[test]
fn no_strategy_ships_and_new_sessions_do_not_rotate() {
    let home = Home::new("none");
    assert!(
        shelf::all(Some(&home.sdir())).is_empty(),
        "策略目录里多了文件"
    );
    assert!(home.setup().for_new().is_none(), "新会话该不轮换");
}

const MINE: &str = "// 我的\n/*--- rotation\n{\"format\": 5, \"settings\": {\"n\": 1}}\n---*/\nfn decide(ctx) { #{ order: [] } }\n";

/// ★ 新会话载入 `forNew` 那份此刻的副本（之后文件再改 · `forNew` 再换都不跟）；那份读不懂 ⇒ 不给（不轮换）。
#[test]
fn a_new_session_loads_a_copy_of_for_new_and_keeps_it() {
    let home = Home::new("fornew");
    std::fs::write(home.sdir().join("mine.rhai"), MINE).expect("策略");
    let setup = home.setup();
    setup
        .change(&home.own(), |b| b.for_new = "mine".into())
        .expect("forNew");
    let mut b = Book::fresh();
    assert!(b.saw("s1", "claude-code", "a", 100, || setup.for_new()));
    let c = b.sessions["s1"].uses.clone().expect("载入了副本");
    assert_eq!((c.id.as_str(), &c.settings), ("mine", &json!({"n": 1})));
    assert_eq!(c.rev, shelf::rev_of(MINE));
    std::fs::write(
        home.sdir().join("mine.rhai"),
        MINE.replace("\"n\": 1", "\"n\": 2"),
    )
    .expect("改");
    b.saw("s1", "claude-code", "b", 200, || {
        panic!("已知的会话不该再问")
    });
    assert_eq!(
        b.sessions["s1"].uses.as_ref(),
        Some(&c),
        "换了开会话的号也该留着副本"
    );
    std::fs::write(home.sdir().join("mine.rhai"), "fn decide(").expect("坏");
    assert!(setup.for_new().is_none(), "读不懂的那份载入不了");
}
