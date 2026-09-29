//! 要求住址：`4d-lanes.md` MIG-3b 第 1 条 —— 部署决策（该不该换 · 换成什么 · 身份判定）在本机常驻后端出计划。
//!
//! 替身对面：按命令 / 路径答预设的话（不起 SSH）。纯判定本身的逐格判据住 `deploy-core` 的 `lib_tests.rs`；
//! 这里钉的是**编排**：先问机器、再判表、再问落点、再问旧落点，每一步的失败落在哪一格码上。

use super::*;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// 替身：`exec` 按命令原文查表；`stat` / `read` 按路径查表。没登记的 ⇒ 链路错。
#[derive(Default)]
struct Fake {
    exec: BTreeMap<String, Result<crate::dial::Captured, String>>,
    stat: BTreeMap<String, (Option<Option<u64>>, Option<bool>)>,
    read: BTreeMap<String, Vec<u8>>,
    /// 〔WF2〕`list` 按目录查表；没登记 ⇒ 列不出。
    list: BTreeMap<String, Vec<(String, Option<u64>)>>,
    asked: Mutex<Vec<String>>,
}

/// 〔WF2〕判据里的「此刻」（秒）。
const NOW: u64 = 1_000_000;

fn said(exit: u32, stdout: &str) -> Result<crate::dial::Captured, String> {
    Ok(crate::dial::Captured {
        stdout: stdout.to_string(),
        stderr: String::new(),
        exit_status: Some(exit),
    })
}

/// 替身的 ack：第 `nth` 趟报的逐地址指纹（形状同后端 `DialAck` 那几格）。
fn fake_ack(nth: usize) -> Value {
    serde_json::json!({
        "ok": true,
        "fingerprints": { "10.0.0.2:22": format!("SHA256:trip{nth}") },
        "jump_fingerprints": {},
        "strict": false,
        "jump_strict": false,
    })
}

fn stamp(id: &str) -> String {
    format!(
        "{}{id}{}\n",
        crate::BUILD_STAMP_OPEN,
        crate::BUILD_STAMP_CLOSE
    )
}

fn landing_scan() -> String {
    deploy_core::stamp_scan_cmd(relay_route_core::BACKEND_LANDING_SHELL, MARKS)
}

fn legacy_scan() -> String {
    deploy_core::stamp_scan_cmd(deploy_core::LEGACY_BACKEND_WORD, MARKS)
}

impl Fake {
    /// 一台 Linux x86_64，落点那一份自报 `landing`（`None` = 不在），旧落点不在。
    fn linux(landing: Option<&str>) -> Fake {
        let mut f = Fake::default();
        f.exec
            .insert(deploy_core::UNAME_CMD.into(), said(0, "Linux x86_64\n"));
        match landing {
            Some(id) => {
                f.stat.insert(
                    relay_route_core::BACKEND_LANDING_REL.into(),
                    (Some(Some(9)), None),
                );
                f.exec.insert(landing_scan(), said(0, &stamp(id)));
            }
            None => {
                f.stat.insert(
                    relay_route_core::BACKEND_LANDING_REL.into(),
                    (None, Some(false)),
                );
            }
        }
        f.stat
            .insert(deploy_core::LEGACY_BACKEND_REL.into(), (None, Some(false)));
        f
    }
}

impl Facing for Fake {
    fn exec(&self, command: String) -> Fut<'_, Result<(crate::dial::Captured, Value), String>> {
        self.asked.lock().unwrap().push(command.clone());
        // 每一趟的 ack 带上它是第几趟（判据据此认「计划里交回的是问 `uname` 那一趟的」）。
        let nth = self.asked.lock().unwrap().len();
        let got = self
            .exec
            .get(&command)
            .cloned()
            .unwrap_or_else(|| Err(format!("替身没登记这条命令：{command}")))
            .map(|c| (c, fake_ack(nth)));
        Box::pin(async move { got })
    }

    fn stat<'a>(
        &'a self,
        rel: &'a str,
    ) -> Fut<'a, Result<(Option<Option<u64>>, Option<bool>), String>> {
        let got = self
            .stat
            .get(rel)
            .cloned()
            .ok_or_else(|| format!("替身没登记这个路径：{rel}"));
        Box::pin(async move { got })
    }

    fn read<'a>(&'a self, rel: &'a str, max: u64) -> Fut<'a, Option<Vec<u8>>> {
        let got = self
            .read
            .get(rel)
            .cloned()
            .filter(|b| b.len() as u64 <= max);
        Box::pin(async move { got })
    }

    fn list<'a>(&'a self, rel: &'a str) -> Fut<'a, Option<Vec<(String, Option<u64>)>>> {
        let got = self.list.get(rel).cloned();
        Box::pin(async move { got })
    }
}

fn carried(id: &str) -> Vec<(Key, String)> {
    vec![(
        deploy_core::key_of("Linux", "x86_64").unwrap(),
        id.to_string(),
    )]
}

#[tokio::test]
async fn the_same_build_at_the_landing_is_skipped_and_an_absent_legacy_says_nothing() {
    let f = Fake::linux(Some("p9a-mine"));
    let p = plan(&f, &carried("p9a-mine"), "box", NOW).await.unwrap();
    assert_eq!(p.action, DeployAction::Skip);
    assert_eq!(p.legacy, LegacyVerdict::Absent);
    assert_eq!(p.expected, "p9a-mine");
}

#[tokio::test]
async fn nothing_at_the_landing_is_deployed() {
    let f = Fake::linux(None);
    let p = plan(&f, &carried("p9a-mine"), "box", NOW).await.unwrap();
    assert!(
        matches!(p.action, DeployAction::Deploy(_)),
        "{:?}",
        p.action
    );
    // 不在就不必再扫它的身份戳：只问了机器与旧落点那一格（旧落点也不在 ⇒ 也不扫）。
    assert_eq!(
        *f.asked.lock().unwrap(),
        vec![deploy_core::UNAME_CMD.to_string()]
    );
}

#[tokio::test]
async fn an_older_build_is_replaced_and_a_newer_one_is_kept_with_its_own_identity() {
    let older = plan(&Fake::linux(Some("p9a-old")), &carried("p9b-mine"), "box", NOW)
        .await
        .unwrap();
    assert!(
        matches!(older.action, DeployAction::Deploy(_)),
        "{:?}",
        older.action
    );
    let newer = plan(&Fake::linux(Some("p9c-new")), &carried("p9b-mine"), "box", NOW)
        .await
        .unwrap();
    assert!(
        matches!(&newer.action, DeployAction::Keep { theirs, .. } if theirs == "p9c-new"),
        "{:?}",
        newer.action
    );
}

#[tokio::test]
async fn a_machine_this_build_does_not_carry_is_refused_before_the_landing_is_asked() {
    let f = Fake::linux(Some("p9a-mine"));
    let aarch = vec![(
        deploy_core::key_of("Linux", "aarch64").unwrap(),
        "p9a-mine".to_string(),
    )];
    let (code, msg) = plan(&f, &aarch, "box", NOW).await.unwrap_err();
    assert_eq!(code, "refused");
    assert!(msg.contains("box"), "拒绝那句要点名那台：{msg}");
    assert_eq!(f.asked.lock().unwrap().len(), 1, "拒绝点在问落点之前");
}

#[tokio::test]
async fn a_windows_remote_is_refused_as_not_promised() {
    let mut f = Fake::linux(Some("p9a-mine"));
    f.exec.insert(
        deploy_core::UNAME_CMD.into(),
        said(0, "MINGW64_NT-10.0-19045 x86_64\n"),
    );
    let (code, _) = plan(&f, &carried("p9a-mine"), "box", NOW).await.unwrap_err();
    assert_eq!(code, "refused");
}

#[tokio::test]
async fn a_link_that_cannot_ask_uname_is_unreachable_not_a_refusal() {
    let f = Fake::default();
    let (code, _) = plan(&f, &carried("p9a-mine"), "box", NOW).await.unwrap_err();
    assert_eq!(code, "unreachable");
}

#[tokio::test]
async fn an_unstamped_landing_is_undecidable_unless_it_is_our_old_three_line_entry() {
    let mut f = Fake::linux(Some("x"));
    f.exec.insert(landing_scan(), said(1, ""));
    let (code, _) = plan(&f, &carried("p9a-mine"), "box", NOW).await.unwrap_err();
    assert_eq!(code, "undecidable");

    f.read.insert(
        relay_route_core::BACKEND_LANDING_REL.into(),
        "#!/bin/sh\n# cc-monitor: ccm = 后端本体的一次性模式（K33：所有命令只许有一处）\nexec x\n"
            .as_bytes()
            .to_vec(),
    );
    let p = plan(&f, &carried("p9a-mine"), "box", NOW).await.unwrap();
    assert!(
        matches!(p.action, DeployAction::Deploy(_)),
        "{:?}",
        p.action
    );
}

#[tokio::test]
async fn a_stamped_legacy_backend_is_marked_for_removal_and_an_unasked_one_is_unknown() {
    let mut f = Fake::linux(Some("p9a-mine"));
    f.stat.insert(
        deploy_core::LEGACY_BACKEND_REL.into(),
        (Some(Some(9)), None),
    );
    f.exec.insert(legacy_scan(), said(0, &stamp("p1a-ancient")));
    let p = plan(&f, &carried("p9a-mine"), "box", NOW).await.unwrap();
    assert_eq!(p.legacy, LegacyVerdict::Remove);

    f.stat.remove(deploy_core::LEGACY_BACKEND_REL);
    let p = plan(&f, &carried("p9a-mine"), "box", NOW).await.unwrap();
    assert!(
        matches!(p.legacy, LegacyVerdict::Unknown(_)),
        "{:?}",
        p.legacy
    );
}

/// 〔WF2〕要求住址：`第四波记录/WIN3.md §2` 读数 B「部署失败留下半截 ~/.cc-monitor/bin/ccm.…tmp，之后连上也不清」· 题面 WF2 第 2 条「下次连上清旧的」。
/// 落点目录里陈旧的临时件 · 备份件 ⇒ 进计划；新的（另一个部署者正在写）· 恰在门槛上 · 修改时间缺 · 不是 `put_atomic` 那个形状（陈旧也不碰）
/// · 落点本身 ⇒ 不进（两向相等）。目录列不出 ⇒ 空、计划照出。
#[tokio::test]
async fn stale_put_leftovers_in_the_landing_dir_are_handed_over_and_nothing_else() {
    let old = NOW - LEFTOVER_STALE_SECS - 1;
    let mut f = Fake::linux(Some("p9a-mine"));
    let rows: Vec<(&str, Option<u64>)> = vec![
        ("ccm.2c0-18d9c8c1df0ec9e7-c.tmp", Some(old)),
        ("ccm.2c0-18d9c8c1df0ec9e7-d.bak", Some(old)),
        ("ccm.2c1-18d9c8c1df0ec9e8-0.tmp", Some(NOW - 60)),
        ("ccm.2c1-18d9c8c1df0ec9e8-1.tmp", Some(NOW - LEFTOVER_STALE_SECS)),
        ("ccm.2c2-1-2.tmp", None),
        ("ccm", Some(old)),
        ("notes.tmp", Some(old)),
        ("ccm.old.tmp", Some(old)),
        ("ccm.2C0-1-2.tmp", Some(old)),
        ("ccm.2c0-1.tmp", Some(old)),
        (".2c0-1-2.tmp", Some(old)),
    ];
    f.list.insert(
        ".cc-monitor/bin".into(),
        rows.iter().map(|(n, t)| (n.to_string(), *t)).collect(),
    );
    let p = plan(&f, &carried("p9a-mine"), "box", NOW).await.unwrap();
    assert_eq!(
        p.leftovers,
        vec![
            ".cc-monitor/bin/ccm.2c0-18d9c8c1df0ec9e7-c.tmp".to_string(),
            ".cc-monitor/bin/ccm.2c0-18d9c8c1df0ec9e7-d.bak".to_string(),
        ]
    );
    f.list.clear();
    let p = plan(&f, &carried("p9a-mine"), "box", NOW).await.unwrap();
    assert_eq!((p.leftovers, p.action), (Vec::<String>::new(), DeployAction::Skip));
}

/// 键集合 == 金样 `tests/__fixtures__/deploy-plan.golden.json` 的键（monitor 的解码器读同一份金样）。
#[test]
fn the_plan_frame_has_exactly_the_golden_keys() {
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/deploy-plan.golden.json")).unwrap();
    let keys = |v: &serde_json::Value| -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    let p = Plan {
        key: deploy_core::key_of("Linux", "x86_64").unwrap(),
        expected: "p9a-mine".into(),
        action: DeployAction::Keep {
            theirs: "p9b".into(),
            why: "w".into(),
        },
        legacy: LegacyVerdict::Unknown("e".into()),
        leftovers: vec![".cc-monitor/bin/ccm.1-2-3.tmp".into()],
        ack: fake_ack(1),
    };
    let got = plan_json(&p);
    assert_eq!(keys(&got), keys(&golden["product"]));
    // 帧面登记的 `fields` 与真产出同一组（`protocol_doc_guard` 另核文档）。
    let spec = crate::stream::inbound::REGISTRY
        .iter()
        .find(|s| s.name == "deploy-plan")
        .expect("deploy-plan 没登记");
    let mut fields: Vec<String> = spec.fields.iter().map(|s| s.to_string()).collect();
    fields.sort();
    assert_eq!(fields, keys(&got));
}

#[test]
fn carried_rows_outside_table_a_or_without_an_id_are_bad_args() {
    let ok = serde_json::json!({"carried": [{"os": "Linux", "arch": "x86_64", "id": "p9a-x"}]});
    assert_eq!(carried_of(&ok).unwrap().len(), 1);
    for bad in [
        serde_json::json!({}),
        serde_json::json!({"carried": [{"os": "Plan9", "arch": "x86_64", "id": "p9a-x"}]}),
        serde_json::json!({"carried": [{"os": "Linux", "arch": "x86_64"}]}),
    ] {
        assert_eq!(carried_of(&bad).unwrap_err().0, "bad_args", "{bad}");
    }
}

/// 〔MIG-3b〕原住 `tests/frontend/shell/sftp_tests.rs`（序键那时住 monitor）；序键搬进 `deploy-core` 之后放在后端这一侧：读的历史表与 `BUILD_ID` 都在这一半。
/// 🔴 B1b：**出过的每一个 `BUILD_ID` 都有序、历史表按表序严格爬升、现在这个不低于最后一行**（读后端源码，异源）。
/// 下一次 bump 写出一个解不出序的形状（或比历史低）⇒ 当场红 —— 那一版部署出去就永远不会被判「更新」而换上。
#[test]
fn hx2_every_build_id_ever_shipped_has_an_order_and_the_history_climbs() {
    let guard = include_str!("../build_id_guard.rs");
    let start = guard
        .find("const SUBCOMMAND_HISTORY")
        .expect("历史表不在了 —— 本条的对照物没了");
    let end = start + guard[start..].find("\n    ];").expect("历史表没有收尾");
    let ids: Vec<&str> = guard[start..end]
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix('"')?.split('"').next())
        .filter(|s| s.starts_with('p') && !s.contains('\n') && !s.starts_with("--"))
        .collect();
    assert!(
        ids.len() >= 30,
        "历史表只抠出 {} 个 id —— 抠法坏了：{ids:?}",
        ids.len()
    );
    let mut prev: Option<(u32, u8)> = None;
    for id in &ids {
        let o =
            deploy_core::build_order(id).unwrap_or_else(|| panic!("历史表里的 {id:?} 解不出序"));
        if let Some(p) = prev {
            assert!(o > p, "历史表没有按表序严格爬升：{id:?} 不高于上一行");
        }
        prev = Some(o);
    }
    let now = crate::BUILD_ID;
    let o = deploy_core::build_order(now).unwrap_or_else(|| {
        panic!("现在的 BUILD_ID {now:?} 解不出序 —— 照 `p<代号><小写字母>-<名>` 起名（D-b：部署按这个序只升不降）")
    });
    assert!(
        o >= prev.unwrap(),
        "现在的 BUILD_ID {now:?} 比历史表最后一行还低"
    );
}

/// 🔴〔MIG-3b 续 · VIS2〕本机后端里拨的号，逐地址指纹要交回 monitor 固化：计划里原样带着**问 `uname` 那一趟**的 ack
/// （那一趟就是第一次连上的那一趟；后面几趟走池里同一条连接）。帧面那一格也带着它。
#[tokio::test]
async fn the_plan_hands_back_the_ack_of_the_first_trip_for_pinning() {
    let f = Fake::linux(Some("p9a-mine"));
    let p = plan(&f, &carried("p9a-mine"), "box", NOW).await.unwrap();
    assert_eq!(p.ack, fake_ack(1), "交回的不是第一趟（问 uname）的 ack");
    assert_eq!(plan_json(&p)["ack"], fake_ack(1));
}
