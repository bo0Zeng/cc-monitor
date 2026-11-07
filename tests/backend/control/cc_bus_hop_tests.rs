//! 跨台发信（A13-8-4）：收件人在别台 ⇒ 本机只核信封，经跨台那一跳（`dial::remote_ask::hop`）交给那台的常驻，
//! 那台照它自己的组名单判、收进它自己的收件箱；够不到 ⇒ `offline`，两边一个字节不写。
//!
//! 两台各一个临时家目录；对面是替身，照生产那一半（[`send_here`]）在它自己的家里现答。真 SSH 与真远端常驻买不到。

use super::*;
use crate::common::said::Said;
use crate::control::cc_bus::store;
use crate::dial::remote_ask::Remote;
use cmd_wire::groups::{
    GroupKnown, GroupMember, GroupRead, GroupsMachine, GroupsRead, MemberFacts,
};
use cmd_wire::listing::HistoryMachineRead as Read;
use cmd_wire::Machine;
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

const A: &str = "0f3c2a1e-1111-2222-3333-444455556666";
const B: &str = "9a8b7c6d-aaaa-bbbb-cccc-ddddeeeeffff";
const C: &str = "1c1c1c1c-cccc-cccc-cccc-cccccccccccc";

fn home(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-bus-hop-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn member(sid: &str) -> GroupMember {
    GroupMember {
        origin: Machine::local(),
        sid: sid.into(),
        pinned: false,
        face: <crate::wire::SessionFace as crate::wire::SessionFaceExt>::of(
            crate::wire::Life::Unclear,
        ),
        facts: MemberFacts {
            title: "t".into(),
            cwd: None,
            record: None,
            account: None,
            bg: false,
            at: None,
        },
    }
}

/// 那台的组名单：`sids` 同在一组（空 ⇒ 没有组）。
fn roster(sids: &[&str]) -> GroupsRead {
    let groups = if sids.is_empty() {
        vec![]
    } else {
        vec![GroupRead {
            id: "g1".into(),
            name: Some("g1".into()),
            title: "g1".into(),
            made: None,
            named: None,
            known: GroupKnown::Now,
            members: sids.iter().map(|s| member(s)).collect(),
            counts: vec![],
        }]
    };
    GroupsRead {
        groups,
        pinned: vec![],
        machines: vec![GroupsMachine {
            origin: Machine::local(),
            read: Read::Ok { notice: None },
        }],
    }
}

/// 那台：自己的家目录 ＋ 自己的组名单；记下收到的每一问。`down` ⇒ 常驻不在。
struct Owner {
    home: std::path::PathBuf,
    roster: GroupsRead,
    down: bool,
    asked: Mutex<Vec<(String, Value)>>,
}

impl Owner {
    fn at(home: &std::path::Path, roster: GroupsRead, down: bool) -> Self {
        Self {
            home: home.to_path_buf(),
            roster,
            down,
            asked: Mutex::new(Vec::new()),
        }
    }
    fn inbox(&self, sid: &str) -> Vec<String> {
        store::tail(&self.home.join(store::DIR), sid, 100).unwrap()
    }
}

impl Remote for Owner {
    fn run<'a>(
        &'a self,
        _dial: &'a Value,
        _command: String,
        _stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        unreachable!("跨台那一跳走 run_coded")
    }

    fn run_coded<'a>(
        &'a self,
        _dial: &'a Value,
        command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, Said>> + Send + 'a>> {
        Box::pin(async move {
            let line: Value = serde_json::from_str(stdin.as_deref().unwrap_or("")).unwrap();
            self.asked.lock().unwrap().push((command, line.clone()));
            if self.down {
                return Err(Said::new("resident_absent", "down".into()));
            }
            assert_eq!(line["cmd"], json!("bus-send"));
            let req: BusEnvelopeReq = serde_json::from_value(line["args"].clone()).unwrap();
            let own = crate::platform::fs::Own::issue_roots(&self.home, &self.home);
            match send_here(&own, &req, Ok(&self.roster), 1_000) {
                Ok(sent) => Ok(serde_json::to_string(&sent).unwrap()),
                Err(s) => Err(s),
            }
        })
    }
}

fn letter(machine: Option<&str>, title: &str) -> BusEnvelopeReq {
    let mut v =
        json!({"from": A, "to": B, "tier": 1, "act": "inform", "title": title, "text": "占位正文"});
    if let Some(m) = machine {
        v["machine"] = json!(m);
    }
    serde_json::from_value(v).unwrap()
}

async fn send_from(
    here: &std::path::Path,
    req: &BusEnvelopeReq,
    owner: &Owner,
) -> Result<BusSent, BusSendErr> {
    let own = crate::platform::fs::Own::issue_data_at(here);
    let table = crate::dial::remote_ask::test_peers::named(&["far"]);
    send(
        &own,
        req,
        &crate::common::time::Tz::default(),
        None,
        &table,
        owner,
        || async { unreachable!("别台的信不读这台的名单") },
    )
    .await
}

#[test]
fn the_machine_defaults_to_this_one() {
    assert!(
        letter(None, "t").machine.is_local(),
        "缺 `machine` ⇒ 本机（cc-bus send 不带 --machine 照今天）"
    );
}

#[tokio::test]
async fn a_letter_for_another_machine_lands_in_that_machines_inbox_once() {
    let here = home("here");
    let there = home("there");
    let owner = Owner::at(&there, roster(&[]), false);
    let sent = send_from(&here, &letter(Some("far"), "占位标题"), &owner)
        .await
        .unwrap();
    let inbox = owner.inbox(B);
    assert_eq!(inbox.len(), 1, "那台收件箱多一封、只一封");
    let landed: Value = serde_json::from_str(&inbox[0]).unwrap();
    assert_eq!(
        serde_json::to_value(&sent).unwrap(),
        json!({ "id": landed["id"] }),
        "回包只有收进去那一封的 id"
    );
    let asked = owner.asked.lock().unwrap();
    assert_eq!(asked.len(), 1, "只问一次");
    assert!(asked[0].0.contains("--resident-call"), "问的是那台的常驻");
    assert_eq!(
        asked[0].1["args"]["machine"],
        json!(cmd_wire::LOCAL),
        "交给那台的是同一条、machine 换成那台眼里的本机"
    );
    assert!(!here.join(store::DIR).exists(), "本机一个字节不写");
}

#[tokio::test]
async fn a_tier_one_letter_for_another_machine_knocks_in_that_machines_input_queue() {
    let here = home("knock-here");
    let there = home("knock-there");
    let owner = Owner::at(&there, roster(&[]), false);
    send_from(&here, &letter(Some("far"), "占位标题"), &owner)
        .await
        .unwrap();
    let queue = |h: &std::path::Path| {
        crate::history::session_queue::read(&crate::platform::fs::Own::issue_roots(h, h), B, 2_000)
            .unwrap()
            .items
    };
    let q = queue(&there);
    assert_eq!(q.len(), 1, "那台那条会话的输入队多一条（敲门在收件那台敲）");
    assert_eq!(q[0].end, "cc-bus");
    assert!(queue(&here).is_empty(), "本机的输入队不动");
}

#[tokio::test]
async fn an_offline_machine_is_refused_and_nothing_is_kept() {
    let here = home("off-here");
    let there = home("off-there");
    let owner = Owner::at(&there, roster(&[]), true);
    let e = send_from(&here, &letter(Some("far"), "占位标题"), &owner)
        .await
        .unwrap_err();
    assert_eq!(e.into_said().code.as_deref(), Some("offline"));
    // 不在机器表里的那一台：同样 `offline`，一问都不发。
    let e = send_from(&here, &letter(Some("nowhere"), "占位标题"), &owner)
        .await
        .unwrap_err();
    assert_eq!(e.into_said().code.as_deref(), Some("offline"));
    assert_eq!(owner.asked.lock().unwrap().len(), 1);
    assert!(!here.join(store::DIR).exists(), "不在发件端攒");
    assert!(!there.join(store::DIR).exists());
}

#[tokio::test]
async fn a_bad_envelope_is_refused_here_before_it_leaves() {
    let here = home("bad-here");
    let there = home("bad-there");
    let owner = Owner::at(&there, roster(&[]), false);
    let e = send_from(&here, &letter(Some("far"), ""), &owner)
        .await
        .unwrap_err();
    assert_eq!(e.into_said().code.as_deref(), Some("bad_title"));
    assert!(owner.asked.lock().unwrap().is_empty(), "坏信封不出门");
}

#[tokio::test]
async fn the_far_machines_own_group_roster_decides_and_its_code_comes_back() {
    let here = home("scope-here");
    let there = home("scope-there");
    // 那台的名单：发件人与 c 同组，收件人 b 不在 ⇒ 那台拒成 `not_same_group`。
    let owner = Owner::at(&there, roster(&[A, C]), false);
    let e = send_from(&here, &letter(Some("far"), "占位标题"), &owner)
        .await
        .unwrap_err();
    assert_eq!(e.into_said().code.as_deref(), Some("not_same_group"));
    assert!(owner.inbox(B).is_empty());
}
