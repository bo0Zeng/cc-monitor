//! 要求：部署决策（该不该换 · 换成什么 · 身份判定）在本机常驻后端出计划。
//!
//! 替身对面：按命令 / 路径答预设的话（不起 SSH）。钉的是**编排**：先问机器、再判表、再问落点、再问旧落点，每一步的失败落在哪一格码上；
//! 文件末尾是随判定从共享 crate `deploy-core` 搬来的逐格判据（期望一字未改）与 `place-verdict` 的真值表。
//! 戳格式那几格（扫描命令 · 扫描回话 · 字节自报 · 序键）住契约 crate 的 `tests/common/deploy-contract/lib_tests.rs`。

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
    /// `list` 按目录查表；没登记 ⇒ 列不出。
    list: BTreeMap<String, Vec<(String, Option<u64>)>>,
    asked: Mutex<Vec<String>>,
}

/// 判据里的「此刻」（秒）。
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
        "fingerprints": { "192.0.2.2:22": format!("SHA256:trip{nth}") },
        "jump_fingerprints": {},
        "strict": false,
        "jump_strict": false,
    })
}

fn stamp(id: &str) -> String {
    format!(
        "{}{id}{}\n",
        deploy_contract::STAMP_OPEN,
        deploy_contract::STAMP_CLOSE
    )
}

fn landing_scan() -> String {
    deploy_contract::stamp_scan_cmd(relay_route_core::BACKEND_LANDING_SHELL, MARKS)
}

impl Fake {
    /// 一台 Linux x86_64，落点那一份自报 `landing`（`None` = 不在）。
    fn linux(landing: Option<&str>) -> Fake {
        let mut f = Fake::default();
        f.exec
            .insert(deploy_contract::UNAME_CMD.into(), said(0, "Linux x86_64\n"));
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
        deploy_contract::key_of("Linux", "x86_64").unwrap(),
        id.to_string(),
    )]
}

#[tokio::test]
async fn the_same_build_at_the_landing_is_skipped() {
    let f = Fake::linux(Some("p9a-mine"));
    let p = plan(&f, &carried("p9a-mine"), "box", NOW).await.unwrap();
    assert_eq!(p.action, DeployAction::Skip);
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
        vec![deploy_contract::UNAME_CMD.to_string()]
    );
}

#[tokio::test]
async fn an_older_build_is_replaced_and_a_newer_one_is_kept_with_its_own_identity() {
    let older = plan(
        &Fake::linux(Some("p9a-old")),
        &carried("p9b-mine"),
        "box",
        NOW,
    )
    .await
    .unwrap();
    assert!(
        matches!(older.action, DeployAction::Deploy(_)),
        "{:?}",
        older.action
    );
    let newer = plan(
        &Fake::linux(Some("p9c-new")),
        &carried("p9b-mine"),
        "box",
        NOW,
    )
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
        deploy_contract::key_of("Linux", "aarch64").unwrap(),
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
        deploy_contract::UNAME_CMD.into(),
        said(0, "MINGW64_NT-10.0-19045 x86_64\n"),
    );
    let (code, _) = plan(&f, &carried("p9a-mine"), "box", NOW)
        .await
        .unwrap_err();
    assert_eq!(code, "refused");
}

#[tokio::test]
async fn a_link_that_cannot_ask_uname_is_unreachable_not_a_refusal() {
    let f = Fake::default();
    let (code, _) = plan(&f, &carried("p9a-mine"), "box", NOW)
        .await
        .unwrap_err();
    assert_eq!(code, "unreachable");
}

#[tokio::test]
async fn an_unstamped_landing_is_undecidable_unless_it_is_our_old_three_line_entry() {
    let mut f = Fake::linux(Some("x"));
    f.exec.insert(landing_scan(), said(1, ""));
    let (code, _) = plan(&f, &carried("p9a-mine"), "box", NOW)
        .await
        .unwrap_err();
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

/// 要求：「部署失败留下半截 ~/.cc-monitor/bin/ccm.…tmp，之后连上也不清」⇒「下次连上清旧的」。
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
        (
            "ccm.2c1-18d9c8c1df0ec9e8-1.tmp",
            Some(NOW - LEFTOVER_STALE_SECS),
        ),
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
    assert_eq!(
        (p.leftovers, p.action),
        (Vec::<String>::new(), DeployAction::Skip)
    );
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
        key: deploy_contract::key_of("Linux", "x86_64").unwrap(),
        expected: "p9a-mine".into(),
        action: DeployAction::Keep {
            theirs: "p9b".into(),
            why: "w".into(),
        },
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
    let mut fields: Vec<String> = spec.field_names().map(str::to_string).collect();
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

/// 原住 `tests/frontend/shell/sftp_tests.rs`（序键那时住 monitor）；序键搬进 `deploy-core` 之后放在后端这一侧：读的历史表与 `BUILD_ID` 都在这一半。
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
        let o = deploy_contract::build_order(id)
            .unwrap_or_else(|| panic!("历史表里的 {id:?} 解不出序"));
        if let Some(p) = prev {
            assert!(o > p, "历史表没有按表序严格爬升：{id:?} 不高于上一行");
        }
        prev = Some(o);
    }
    let now = crate::BUILD_ID;
    let o = deploy_contract::build_order(now).unwrap_or_else(|| {
        panic!("现在的 BUILD_ID {now:?} 解不出序 —— 照 `p<代号><小写字母>-<名>` 起名（D-b：部署按这个序只升不降）")
    });
    assert!(
        o >= prev.unwrap(),
        "现在的 BUILD_ID {now:?} 比历史表最后一行还低"
    );
}

/// 🔴本机后端里拨的号，逐地址指纹要交回 monitor 固化：计划里原样带着**问 `uname` 那一趟**的 ack
/// （那一趟就是第一次连上的那一趟；后面几趟走池里同一条连接）。帧面那一格也带着它。
#[tokio::test]
async fn the_plan_hands_back_the_ack_of_the_first_trip_for_pinning() {
    let f = Fake::linux(Some("p9a-mine"));
    let p = plan(&f, &carried("p9a-mine"), "box", NOW).await.unwrap();
    assert_eq!(p.ack, fake_ack(1), "交回的不是第一趟（问 uname）的 ack");
    assert_eq!(plan_json(&p)["ack"], fake_ack(1));
}

// ═══ `resident-verdict`：远端常驻后端 hello 的新旧 ═══════════════════════════════════
// 要求：「共享 crate 只放契约，判定只在后端」（monitor 里的共享判定残留：远端常驻换不换）。
// 从 monitor `remote_resident_tests.rs` 那一条（`hello_decision` 的真值表）搬来：判定进了本机常驻后端，真值表跟着判定走。

/// 只升不降、只换一次：那台旧 ⇒ 换（换过一次就接）；同一版 · 更新 · 序解不出 ⇒ 接。`older` 与 `replaced` 无关。
#[test]
fn the_verdict_replaces_only_upward_and_only_once() {
    // (那台报的, 换过没有) ⇒ (换, 那台旧)
    for (theirs, replaced, want) in [
        ("p4a-x", false, (true, true)),
        ("p4a-x", true, (false, true)),
        ("p4j-y", false, (false, false)),
        ("p5a-z", false, (false, false)),
        ("dev", false, (false, false)),
        ("", false, (false, false)),
    ] {
        let v = resident_verdict("p4j-y", theirs, replaced);
        assert_eq!((v.replace, v.older), want, "{theirs} replaced={replaced}");
    }
}

/// 帧面那一格：`{action, older}` 恰这两个键；缺 `mine`（或空）· `theirs` · `replaced` ⇒ `bad_args`。
#[test]
fn the_verdict_frame_has_exactly_two_keys_and_refuses_missing_args() {
    let got =
        answer_resident_verdict(&json!({"mine": "p4j-y", "theirs": "p4a-x", "replaced": false}))
            .expect("答得出");
    assert_eq!(got, json!({"action": "replace", "older": true}));
    let got =
        answer_resident_verdict(&json!({"mine": "p4j-y", "theirs": "p4a-x", "replaced": true}))
            .expect("答得出");
    assert_eq!(got, json!({"action": "attach", "older": true}));
    for bad in [
        json!({"theirs": "p4a-x", "replaced": false}),
        json!({"mine": "", "theirs": "p4a-x", "replaced": false}),
        json!({"mine": "p4j-y", "replaced": false}),
        json!({"mine": "p4j-y", "theirs": "p4a-x"}),
    ] {
        assert_eq!(
            answer_resident_verdict(&bad).map_err(|(c, _)| c),
            Err("bad_args"),
            "{bad}"
        );
    }
}

// ═══ 随判定从共享 crate `deploy-core` 搬来的逐格判据（判定只在后端；期望一字未改）══════════════

/// I1：六形逐形（期望取自那张表 ＋ 0 字节那一格按「没装」）。
#[test]
fn identity_decision_answers_each_state_without_merging_them() {
    const EXPECT: &str = "p9b-sample";
    let d = |id: RemoteIdentity| identity_decision(&id, EXPECT, "devbox", "/h/.cc-monitor/bin/ccm");
    assert!(
        matches!(d(RemoteIdentity::Missing), Ok(DeployAction::Deploy(_))),
        "没装 ⇒ 装"
    );
    assert!(
        matches!(d(RemoteIdentity::Empty), Ok(DeployAction::Deploy(_))),
        "0 字节 ⇒ 装"
    );
    assert_eq!(
        d(RemoteIdentity::Stamp(EXPECT.into())),
        Ok(DeployAction::Skip),
        "同一版 ⇒ 复用"
    );
    let Ok(DeployAction::Deploy(why)) = d(RemoteIdentity::Stamp("p8z-older".into())) else {
        panic!("更旧的一版 ⇒ 该换");
    };
    assert!(
        why.contains("p8z-older") && why.contains(EXPECT),
        "换的理由没说清两边各是哪一版：{why}"
    );
    // 三种「判不清它是谁」：显式失败，而且三句话互不相同（下一步不同：一个没身份、一个身份不唯一、一个判不了）。
    let no = d(RemoteIdentity::NoStamp).unwrap_err();
    let many = d(RemoteIdentity::Ambiguous(vec!["a1".into(), "b2".into()])).unwrap_err();
    let cant = d(RemoteIdentity::Unreadable("Permission denied".into())).unwrap_err();
    for e in [&no, &many, &cant] {
        assert!(
            e.contains("devbox") && e.contains("/h/.cc-monitor/bin/ccm"),
            "没说哪台哪个文件：{e}"
        );
    }
    assert!(
        copy_core::copy_matches("rsSftp.identity.unstamped", &no),
        "{no}"
    );
    assert!(many.contains("a1") && many.contains("b2"), "{many}");
    assert!(
        copy_core::copy_matches("rsSftp.identity.undecidable", &cant)
            && cant.contains("Permission denied"),
        "{cant}"
    );
    assert!(no != many && many != cant && no != cant);
    // 出路是一个真存在的动作（机器页「卸载后端」），不是一句空话。
    assert!(
        no.contains(copy_core::copy_static!("rsSftp.identity.handsOff"))
            && many.contains(copy_core::copy_static!("rsSftp.identity.handsOff"))
    );
}

// ── K-W4b：取样层那四个状态的**映射规则**逐格各一条 ─────────────────────
// 上面那几格买的是「判定那一半」与「两条路真的去问了」；取样这一半（`metadata` /
// `try_exists` 的答案怎么变成 `TargetBinary`）09-06 之前一条判据都没有：
// 把那个取样壳的体换成恒答 `Present`，全量 cargo **0 红**（沙箱实测）。
// 下面五格逐格钉一条规则，第六格是反向自检（证明它们不是恒真）。

/// 映射规则①：`metadata` 说它在、且**有字节** ⇒ `Present`。
#[test]
fn probe_metadata_with_bytes_maps_to_present() {
    assert_eq!(
        interpret_target_probe(Some(Some(2_300_000)), None),
        TargetBinary::Present
    );
    assert_eq!(
        interpret_target_probe(Some(Some(1)), None),
        TargetBinary::Present,
        "1 字节也是「有字节」—— 只有恰好 0 才是 Empty 那一格"
    );
}

/// 映射规则②：`metadata` 说它在、size **恰好 0** ⇒ `Empty`，不是 `Present`。
/// 0 字节不是假想形态：`upload_atomic` 那条「绝不 set_metadata」注释记的就是
/// 真机 e2e 把后端截成 0 字节、不可 exec 的那次事故，而 `try_exists` 会把它算成「在」。
#[test]
fn probe_metadata_saying_zero_bytes_maps_to_empty() {
    assert_eq!(
        interpret_target_probe(Some(Some(0)), None),
        TargetBinary::Empty
    );
    assert_ne!(
        interpret_target_probe(Some(Some(0)), None),
        interpret_target_probe(Some(Some(1)), None),
        "0 字节与有字节判成了同一格 ⇒ 身份那一步的 0 字节那一格（按没装装）永远走不到"
    );
}

/// 映射规则③（本件的承重格）：`metadata` 成功而**服务器不给 size**（`Some(None)`）
/// ⇒ 仍是 `Present`。
/// `TargetBinary` 与取样壳的头注逐字：「服务器不给 size（size=None）≠ 0 字节」——
/// 把「没说」读成「空」，等于对着一台好机器每次连接都重传 2.3MB。
#[test]
fn probe_a_server_that_gives_no_size_is_not_the_empty_cell() {
    assert_eq!(
        interpret_target_probe(Some(None), None),
        TargetBinary::Present
    );
    assert_ne!(
        interpret_target_probe(Some(None), None),
        TargetBinary::Empty,
        "「服务器没给 size」被读成了「0 字节」"
    );
}

/// 映射规则④：`metadata` 失败、补问 `try_exists` **明确答不在** ⇒ `Missing`。
#[test]
fn probe_stat_failed_and_try_exists_says_no_maps_to_missing() {
    assert_eq!(
        interpret_target_probe(None, Some(false)),
        TargetBinary::Missing
    );
}

/// 映射规则⑤：`metadata` 失败、`try_exists` **也答不出来** ⇒ `Unknown`。
/// 不许滑成 `Missing`（一次 stat 失败换一次全量重传，版本门控就废了），
/// 也不许滑成 `Present`（那正是本枚举要治的那个静默）。
#[test]
fn probe_stat_failed_and_try_exists_cannot_answer_maps_to_unknown() {
    assert_eq!(interpret_target_probe(None, None), TargetBinary::Unknown);
    assert_ne!(
        interpret_target_probe(None, None),
        interpret_target_probe(None, Some(false)),
        "「问不出来」与「明确不在」判成了同一格 —— 这两者正是要分开的那两件事"
    );
}

/// **反向自检**：上面五格每一条都可能是恒真的（函数恒答那一张脸，断言照样绿）。
/// 这一格喂**全部六种输入**，钉的是「每一格只由它自己那条规则命中」——
/// 任何一臂被改到别的状态，下面必有一行不等。
#[test]
fn probe_no_cell_answers_in_place_of_another() {
    let table: [(Option<Option<u64>>, Option<bool>, TargetBinary, &str); 6] = [
        (Some(Some(9)), None, TargetBinary::Present, "有字节"),
        (Some(Some(0)), None, TargetBinary::Empty, "恰好 0 字节"),
        (Some(None), None, TargetBinary::Present, "服务器不给 size"),
        (
            None,
            Some(false),
            TargetBinary::Missing,
            "stat 失败 + try_exists 说不在",
        ),
        (
            None,
            Some(true),
            TargetBinary::Present,
            "stat 失败 + try_exists 说在",
        ),
        (
            None,
            None,
            TargetBinary::Unknown,
            "stat 失败 + try_exists 也答不出",
        ),
    ];
    for (size, exists, want, what) in table {
        assert_eq!(interpret_target_probe(size, exists), want, "{what}");
    }
    // 四个状态一个不少地被这张表喂到 —— 少一行就等于那一格没人看。
    for want in [
        TargetBinary::Present,
        TargetBinary::Missing,
        TargetBinary::Empty,
        TargetBinary::Unknown,
    ] {
        assert!(
            table.iter().any(|(_, _, w, _)| *w == want),
            "{want:?} 这一格没有输入喂给它"
        );
    }
    // 恒答任何一张脸都会被这三对逮住（不是「函数存在」那种空真）。
    assert_ne!(
        interpret_target_probe(Some(Some(0)), None),
        interpret_target_probe(Some(Some(9)), None)
    );
    assert_ne!(
        interpret_target_probe(Some(None), None),
        interpret_target_probe(Some(Some(0)), None)
    );
    assert_ne!(
        interpret_target_probe(None, Some(false)),
        interpret_target_probe(None, None)
    );
}

/// B1a 后半（随 `is_newer` 从 `deploy-core` 搬来）：只有两边都解得出、且这一版严格大才算新。
#[test]
fn hx2_newer_means_both_orders_parse_and_mine_is_strictly_greater() {
    assert!(is_newer("p3n-a", "p3m-b") && is_newer("p4a-a", "p3z-b"));
    assert!(!is_newer("p3m-a", "p3m-b"), "同序不同名 ⇒ 不算新");
    assert!(
        !is_newer("p3m-a", "p3n-b") && !is_newer("p3n-a", "junk") && !is_newer("junk", "p1a-x")
    );
}

/// 🔴 B2：`identity_decision` 的「另一版」那一格按新旧拆开（期望手写）：旧 ⇒ 换；新 · 同序不同名 · 解不出 ⇒ 不动。
#[test]
fn hx2_a_different_build_is_replaced_only_when_it_is_older() {
    const MINE: &str = "p3n-mine";
    let d = |s: &str| {
        identity_decision(
            &RemoteIdentity::Stamp(s.into()),
            MINE,
            "devbox",
            "/h/.cc-monitor/bin/ccm",
        )
    };
    assert!(
        matches!(d("p3m-older"), Ok(DeployAction::Deploy(_))),
        "旧 ⇒ 换"
    );
    assert!(
        matches!(d("p2z-older"), Ok(DeployAction::Deploy(_))),
        "旧一代 ⇒ 换"
    );
    assert_eq!(d(MINE), Ok(DeployAction::Skip), "同一版 ⇒ 复用");
    for theirs in ["p3o-newer", "p4a-newer", "p3n-sibling", "hand-built"] {
        match d(theirs) {
            Ok(DeployAction::Keep { theirs: t, why }) => {
                assert_eq!(t, theirs, "Keep 回的不是那台上的身份");
                assert!(
                    why.contains(theirs) && why.contains(MINE) && why.contains("devbox"),
                    "{why}"
                );
            }
            other => panic!("{theirs:?} 不比 {MINE} 旧 ⇒ 该不动它，却是 {other:?}"),
        }
    }
}

/// 已部署的机器上落点是旧的三行入口（无身份戳）：认得出 ⇒ 换成后端本体；认不出的无戳文件照旧显式失败。
/// 原是 `sftp.rs` 那个落点判定函数体的源码切片判据，判定搬来之后改成行为判据。
#[test]
fn an_old_three_line_entry_at_the_landing_is_recognised_as_ours() {
    let old = "#!/bin/sh\n# cc-monitor: ccm = 后端本体的一次性模式（K33：所有命令只许有一处）\nexec '/home/u/.cc-monitor/bin/cc-monitor-backend' ccm \"$@\"\n";
    assert!(is_ours(old), "旧入口没认出来");
    assert!(
        !is_ours("#!/bin/sh\necho mine\n"),
        "用户自己的脚本被当成了我们的"
    );
    let v = |id: RemoteIdentity, bytes: Option<&[u8]>| {
        landing_verdict(&id, bytes, "p9b-mine", "devbox", "~/.cc-monitor/bin/ccm")
    };
    assert!(matches!(
        v(RemoteIdentity::NoStamp, Some(old.as_bytes())),
        Ok(DeployAction::Deploy(_))
    ));
    assert!(v(RemoteIdentity::NoStamp, Some(b"#!/bin/sh\necho mine\n")).is_err());
    assert!(
        v(RemoteIdentity::NoStamp, None).is_err(),
        "读不到那份 ⇒ 照旧显式失败"
    );
    // 认旧入口只在「无戳」那一格：别的格子里带着同样的字节也照身份判。
    assert_eq!(
        v(
            RemoteIdentity::Stamp("p9b-mine".into()),
            Some(old.as_bytes())
        ),
        Ok(DeployAction::Skip)
    );
    assert!(v(
        RemoteIdentity::Ambiguous(vec!["a1".into(), "b2".into()]),
        Some(old.as_bytes())
    )
    .is_err());
}

/// 拒绝点的前三步各在一步上：键拒 · 产线拒 · 承诺拒，第四步（带没带）不在这里。
#[test]
fn judge_refuses_at_the_key_the_line_and_the_promise_and_nowhere_else() {
    let key_of = deploy_contract::key_of;
    let linux = key_of("Linux", "x86_64");
    assert_eq!(judge(Route::Remote, linux.clone()), linux);
    assert!(matches!(
        judge(Route::Remote, key_of("", "")),
        Err(Refusal::OsUnknown { .. })
    ));
    assert!(matches!(
        judge(Route::Remote, key_of("Darwin", "arm64")),
        Err(Refusal::UnsupportedMachine { .. })
    ));
    assert!(matches!(
        judge(Route::Remote, key_of("Windows", "x86_64")),
        Err(Refusal::NotPromisedHere {
            route: Route::Remote,
            ..
        })
    ));
}

// （本机 (Linux, aarch64) 不承诺）· 「判定只在后端」· HX2 D-b「只在我的比盘上的新时才换」。
// ═══ `place-verdict`：本机那一份放不放（monitor 自举时问手上那份字节自己）══════════════════════
// 形状 A：自举那一刻问手上那份字节自己 ⇒ 判定只住后端一家。期望手写。

fn landed(id: &str) -> Result<Option<Vec<u8>>, String> {
    Ok(Some(format!("junk{}junk", stamp(id)).into_bytes()))
}

/// 缺 / 0 字节 ⇒ 放；同一版（只在字节不同时被问）⇒ 放；更旧 ⇒ 放；不旧 ⇒ 不动；不说自己是谁 · 身份不唯一 · 读不了 ⇒ `undecidable`；
/// 表 B 不承诺 · 表 A 无产线 ⇒ `refused`，且那句话就是 `Refusal::say` 那一句（本机那一形）。
#[test]
fn place_verdict_places_only_upward_and_refuses_what_it_cannot_judge() {
    let me = || deploy_contract::key_of("Linux", "x86_64");
    let v = |disk| place_verdict(me(), disk, "p5v-mine", "本机", "/h/.cc-monitor/bin/ccm");
    let place = |r: Result<Placed, (&'static str, String)>| matches!(r, Ok(Placed::Place(_)));
    assert!(place(v(Ok(None))), "没装 ⇒ 放");
    assert!(place(v(Ok(Some(Vec::new())))), "0 字节 ⇒ 放");
    assert!(place(v(landed("p5v-mine"))), "同一版、字节不同 ⇒ 放");
    assert!(place(v(landed("p5u-older"))), "更旧 ⇒ 放");
    for newer in ["p5w-newer", "p6a-newer", "p5v-sibling", "hand-built"] {
        match v(landed(newer)) {
            Ok(Placed::Keep(why)) => {
                assert!(why.contains(newer) && why.contains("p5v-mine"), "{why}")
            }
            other => panic!("{newer:?} 不比这一份旧 ⇒ 该不动，却是 {other:?}"),
        }
    }
    let two = format!("{}{}", stamp("a1"), stamp("b2")).into_bytes();
    for (what, disk) in [
        ("不说自己是谁", Ok(Some(b"#!/bin/sh\necho mine\n".to_vec()))),
        ("身份不唯一", Ok(Some(two))),
        ("读不了", Err("Permission denied".to_string())),
    ] {
        let (code, _) = v(disk).expect_err(what);
        assert_eq!(code, "undecidable", "{what}");
    }
    let local_arm = deploy_contract::key_of("Linux", "aarch64");
    let (code, said) = place_verdict(local_arm, Ok(None), "p5v-mine", "本机", "/d").unwrap_err();
    assert_eq!(code, "refused");
    assert_eq!(
        said,
        Refusal::NotPromisedHere {
            os: "Linux".into(),
            arch: "arm64".into(),
            route: Route::Local
        }
        .say("本机"),
        "本机 (Linux, aarch64) 说的不是本机那一句不承诺"
    );
    let (code, _) = place_verdict(
        deploy_contract::key_of("Darwin", "arm64"),
        Ok(None),
        "p5v-mine",
        "本机",
        "/d",
    )
    .unwrap_err();
    assert_eq!(code, "refused", "表 A 没有产线的机器");
}

/// 帧面：入参缺 / 相对路径 ⇒ `bad_args`；读真盘（不在 ⇒ 放；是自己这一版的戳 ⇒ 放）；答话恰两格。
#[test]
fn the_place_frame_reads_the_one_file_and_answers_two_keys() {
    let d = std::env::temp_dir().join(format!("be-p1-place-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let dest = d.join("ccm");
    let args = serde_json::json!({ "dest": dest.to_string_lossy(), "machine": "本机" });
    for bad in [
        serde_json::json!({ "machine": "本机" }),
        serde_json::json!({ "dest": "rel/ccm", "machine": "本机" }),
        serde_json::json!({ "dest": dest.to_string_lossy() }),
    ] {
        assert_eq!(answer_place(&bad).unwrap_err().0, "bad_args", "{bad}");
    }
    if Key::this_machine()
        .and_then(|k| judge(Route::Local, Ok(k)))
        .is_ok()
    {
        let v = answer_place(&args).expect("不在 ⇒ 答得出");
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["action", "why"]);
        assert_eq!(v["action"], "place");
        std::fs::write(&dest, stamp("p0a-ancient")).unwrap();
        assert_eq!(answer_place(&args).unwrap()["action"], "place", "盘上更旧");
        std::fs::write(&dest, stamp("p999z-future")).unwrap();
        assert_eq!(answer_place(&args).unwrap()["action"], "keep", "盘上更新");
    } else {
        assert_eq!(answer_place(&args).unwrap_err().0, "refused", "这台不承诺");
    }
    let _ = std::fs::remove_dir_all(&d);
}

// ═══ 承诺面：账本 == 代码（两向；随 `promised` 从 monitor `byte_table_tests` 搬来）══════

/// 承诺面的唯一住址是 `tests/evidence/K-G4-platform-ledger.py`（`PROMISE_FACE` · `NOT_PROMISED`），代码那一份是 [`promised`]。
///
/// 要求：用户原话「不承诺. 适配部分, 即os适配部分后面单独写单独做.」；承诺是 (键 × origin) 的属性。
/// 人群 = 表 A 里有后端产线的每个键（`LINES`）× 两个 origin；每一格恰好落在账本两表之一，且落在 `PROMISE_FACE` ⇔ `promised(route, key)`。
/// 异源：账本是 Python 源码里的字面量，代码是 Rust 的 `matches!`。
#[test]
fn the_promise_face_in_the_ledger_equals_the_code() {
    use std::collections::BTreeSet;
    let p = crate::guard_support::repo_root().join("tests/evidence/K-G4-platform-ledger.py");
    let ledger = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}：{e}"));
    let table = |name: &str| -> BTreeSet<(String, String, String)> {
        let at = ledger
            .find(&format!("\n{name} = ["))
            .unwrap_or_else(|| panic!("账本里找不到 `{name} = [`"));
        let body = &ledger[at..];
        let body = &body[..body.find("\n]").expect("那张表没收尾")];
        let mut out = BTreeSet::new();
        for line in body.lines() {
            let t = line.trim_start();
            // 表里的一行恰以 `("` 开头：第一个引号之前恰是 `(`（整段相等，不是前缀匹配）。
            let q: Vec<&str> = t.split('"').collect();
            if q.first() != Some(&"(") {
                continue;
            }
            assert!(q.len() >= 6, "认不出这一行：{t}");
            out.insert((q[1].to_string(), q[3].to_string(), q[5].to_string()));
        }
        assert!(
            !out.is_empty(),
            "`{name}` 读出来是空的 —— 下面的两向相等会空真"
        );
        out
    };
    let face = table("PROMISE_FACE");
    let not = table("NOT_PROMISED");
    assert!(face.is_disjoint(&not), "同一格既承诺又不承诺");
    let os_name = |o: Os| match o {
        Os::Linux => "Linux",
        Os::Windows => "Windows",
        Os::Mac => "macOS",
    };
    let arch_name = |a: Arch| match a {
        Arch::X86_64 => "x86_64",
        Arch::Aarch64 => "aarch64",
    };
    let (mut population, mut code_yes) = (BTreeSet::new(), BTreeSet::new());
    for &key in LINES {
        for (route, rname) in [(Route::Local, "Local"), (Route::Remote, "Remote")] {
            let cell = (
                rname.to_string(),
                os_name(key.os).to_string(),
                arch_name(key.arch).to_string(),
            );
            population.insert(cell.clone());
            if promised(route, key) {
                code_yes.insert(cell);
            }
        }
    }
    let ledger_all: BTreeSet<_> = face.union(&not).cloned().collect();
    assert_eq!(
        ledger_all, population,
        "账本两张表并起来 ≠ 表 A 有产线的键 × 两个 origin"
    );
    assert_eq!(
        code_yes, face,
        "代码 `promised` 放行的格 ≠ 账本 `PROMISE_FACE`（两向）"
    );
    assert!(
        not.contains(&("Local".into(), "Linux".into(), "aarch64".into())),
        "本机 (Linux, aarch64) 不在「不承诺」里"
    );
}
