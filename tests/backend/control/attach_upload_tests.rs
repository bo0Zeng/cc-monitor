//! 桌面附件草稿那一圈（本机后端替界面编排）：同一组「不收 / 超时 / 断开 / 提交失败」输入，推出的结局与手机核心那份结局表
//! （`chunk_core::attach::settle`）同一张；暂存区外的路径不读；落好之后 take 交编号、暂存那一份删掉。
//! 替身那台在内存里：每条命令按名字排好答，答的都是当场就有的结果。

use super::*;
use chunk_core::attach::{settle, Ended};
use cmd_wire::attachment::{
    AttachCardPhase, AttachDraftCard, AttachDraftPutReq, AttachDraftTakeReq,
};
use cmd_wire::Machine;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

/// 替身那台：每条命令一列排好的答（排完了 ⇒ 收 · 成）；记下推出来的每一排。
#[derive(Default)]
struct Far {
    answers: Mutex<HashMap<&'static str, VecDeque<Result<Value, Said>>>>,
    asked: Mutex<Vec<&'static str>>,
    pushed: Mutex<Vec<AttachDraftShown>>,
}

impl Far {
    fn answer(&self, cmd: &'static str, got: Result<Value, Said>) {
        self.answers
            .lock()
            .unwrap()
            .entry(cmd)
            .or_default()
            .push_back(got);
    }

    fn last(&self) -> AttachDraftShown {
        self.pushed
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("推过一排")
    }
}

fn verdict(ok: bool, said: Option<&str>) -> Value {
    json!({"ok": ok, "class": "image", "class_text": "图样", "said": said})
}

fn landed(id: &str) -> Value {
    json!({"id": id, "name": "a.png", "path": "/w/a.png", "rel": "a.png", "bytes": 5, "sha256": "0".repeat(64), "class": "image"})
}

impl Host for Far {
    fn ask(
        &self,
        _machine: &Machine,
        cmd: &'static str,
        _args: Value,
    ) -> Fut<'static, Result<Value, Said>> {
        self.asked.lock().unwrap().push(cmd);
        let queued = self
            .answers
            .lock()
            .unwrap()
            .get_mut(cmd)
            .and_then(VecDeque::pop_front);
        let got = queued.unwrap_or_else(|| match cmd {
            "session-attachment-check" => Ok(json!({"items": [verdict(true, None)]})),
            "files-stage-chunk" => Ok(json!({"bytes": 5})),
            "session-attachment-commit" => Ok(landed("id-1")),
            _ => Ok(json!({"removed": true})),
        });
        Box::pin(async move { got })
    }

    fn push(&self, shown: AttachDraftShown) {
        self.pushed.lock().unwrap().push(shown);
    }
}

/// 沙箱家里的那一份写句柄（暂存区在它底下）。
fn sandbox_own(tag: &str) -> Own {
    let home = test_home::sandbox_root().join(format!("attach-{tag}"));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("建沙箱家");
    let h = home.to_string_lossy().into_owned();
    Own::issue_with(&move |k| (k == "HOME").then(|| h.clone()))
}

/// 壳那一下：字节放进暂存区，回那条路径。
fn staged(desk: &Desk, bytes: &[u8]) -> String {
    let dir = desk.staging().to_path_buf();
    std::fs::create_dir_all(&dir).expect("建暂存区");
    let at = dir.join(format!("{}.part", "ab".repeat(16)));
    std::fs::write(&at, bytes).expect("写暂存件");
    at.to_string_lossy().into_owned()
}

fn put(draft: &str, path: String) -> AttachDraftPutReq {
    AttachDraftPutReq {
        draft: draft.into(),
        machine: Machine::named("box"),
        sid: serde_json::from_str("\"00000000-0000-0000-0000-000000000000\"").unwrap(),
        cards: vec![AttachDraftCard {
            key: "k1".into(),
            name: "a.png".into(),
            path,
            bytes: 5,
            dir: false,
        }],
    }
}

/// 替身的答都是当场的：让那一圈跑完。
async fn drain() {
    for _ in 0..200 {
        tokio::task::yield_now().await;
    }
}

fn said(code: &str, s: &str) -> Said {
    Said::new(code, s.to_string())
}

/// 一组输入跑一圈，回那张卡最后落在哪 · 那一排断着没有。
async fn run_one(tag: &str, far: Far) -> (AttachCardPhase, bool) {
    let far = Arc::new(far);
    let desk = Desk::new(far.clone(), sandbox_own(tag));
    let path = staged(&desk, b"hello");
    desk.put(put("d", path));
    drain().await;
    let last = far.last();
    (last.cards[0].phase.clone(), last.offline)
}

#[tokio::test(flavor = "current_thread")]
async fn the_same_inputs_land_where_the_phone_core_table_says() {
    let refused = Far::default();
    refused.answer(
        "session-attachment-check",
        Ok(json!({"items": [verdict(false, Some("那台不收"))]})),
    );
    let timeout = Far::default();
    timeout.answer(
        "session-attachment-check",
        Err(said(cmd_wire::TIMEOUT, "迟了")),
    );
    let cut = Far::default();
    cut.answer(
        "files-stage-chunk",
        Err(said(cmd_wire::DISCONNECTED, "断了")),
    );
    let failed = Far::default();
    failed.answer(
        "session-attachment-commit",
        Err(said("io_failed", "盘里没地方")),
    );

    let cases = [
        (
            "refused",
            refused,
            Ended::Refused {
                said: "那台不收".into(),
            },
            false,
        ),
        ("timeout", timeout, Ended::Cut, true),
        ("cut", cut, Ended::Cut, true),
        (
            "failed",
            failed,
            Ended::Failed {
                said: "盘里没地方".into(),
            },
            false,
        ),
    ];
    for (tag, far, end, offline) in cases {
        let (phase, cut) = run_one(tag, far).await;
        assert_eq!(Some(phase), settle(&end).phase, "{tag}：落点照结局表");
        assert_eq!(cut, offline, "{tag}：断着没有照结局表");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_path_outside_the_staging_area_is_not_read() {
    let far = Arc::new(Far::default());
    let desk = Desk::new(far.clone(), sandbox_own("outside"));
    let elsewhere = test_home::sandbox_root()
        .join("attach-outside")
        .join("a.png");
    std::fs::write(&elsewhere, b"hello").expect("写暂存区外那一份");
    desk.put(put("d", elsewhere.to_string_lossy().into_owned()));
    drain().await;
    assert_eq!(far.last().cards[0].phase, AttachCardPhase::Lost);
    assert!(
        !far.asked.lock().unwrap().contains(&"files-stage-chunk"),
        "暂存区外的那一份一块都不送"
    );
    assert!(elsewhere.exists(), "暂存区外的那一份不删");
}

#[tokio::test(flavor = "current_thread")]
async fn take_hands_the_ids_once_all_landed_and_the_staged_copy_is_gone() {
    let far = Arc::new(Far::default());
    // 送块那一问先挂一下：头一次 take 时还没落好。
    far.answer(
        "files-stage-chunk",
        Err(said(cmd_wire::DISCONNECTED, "断了")),
    );
    let desk = Desk::new(far.clone(), sandbox_own("take"));
    let path = staged(&desk, b"hello");
    desk.put(put("d", path.clone()));
    drain().await;
    let first = desk.take(AttachDraftTakeReq { draft: "d".into() });
    assert_eq!(first.ids, None, "没落好 ⇒ null、那一排不动");
    assert!(
        std::path::Path::new(&path).exists(),
        "断着时暂存那一份留着（重传从头读）"
    );

    desk.up("box");
    drain().await;
    assert_eq!(far.last().cards[0].phase, AttachCardPhase::Landed);
    assert!(far.last().ready);
    assert!(
        !std::path::Path::new(&path).exists(),
        "落好了 ⇒ 暂存那一份删掉"
    );
    let ids = desk.take(AttachDraftTakeReq { draft: "d".into() });
    assert_eq!(ids.ids, Some(vec!["id-1".to_string()]));
    let again = desk.take(AttachDraftTakeReq { draft: "d".into() });
    assert_eq!(again.ids, Some(vec![]), "交过一次那一排就空了");
}

/// 五条的入参 · 回包与主题那一臂（带体）照类型写出再读回，一字不差。
#[test]
fn the_five_shapes_and_the_attach_arm_round_trip() {
    fn back<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(v: T) {
        let text = serde_json::to_string(&v).expect("写得出");
        let again: T = serde_json::from_str(&text).expect("读得回");
        assert_eq!(again, v, "{text}");
    }
    back(put("d", "/s/a.part".into()));
    back(cmd_wire::attachment::AttachDraftDropReq {
        draft: "d".into(),
        key: "k".into(),
    });
    back(cmd_wire::attachment::AttachDraftRetryReq { draft: "d".into() });
    back(AttachDraftTakeReq { draft: "d".into() });
    back(cmd_wire::attachment::AttachDraftViewReq { draft: "d".into() });
    back(cmd_wire::attachment::AttachDraftTaken { ids: None });
    back(cmd_wire::attachment::AttachDraftTaken {
        ids: Some(vec!["i".into()]),
    });
    let mut row = Desk::nothing("d");
    row.cards.push(cmd_wire::attachment::AttachDraftShownCard {
        key: "k".into(),
        name: "a.png".into(),
        phase: AttachCardPhase::Failed { said: "s".into() },
        done: 3,
        bytes: 5,
        id: None,
        line: Some("l".into()),
        remove: "x".into(),
        class: cmd_wire::files::FileClass::Image,
        class_text: "t".into(),
    });
    back(row.clone());
    back(cmd_wire::frame::Changed::Attach {
        key: "d".into(),
        body: Some(row),
    });
    back(cmd_wire::frame::Changed::Attach {
        key: "d".into(),
        body: None,
    });
}
