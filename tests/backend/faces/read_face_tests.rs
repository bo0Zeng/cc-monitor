//! 只读查询帧面宿主的判据。
//!
//! 夹具只造**结构**（目录名、行数、字节边界），行内容是占位的最小 JSON —— 不采任何真会话正文。

use super::*;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// 本族的应答：`data` 或 `(code, message)`（判据这一层按登记表那一层的做法走一遍之后的样子）。
type Answer = Result<Value, (&'static str, String)>;

/// 读一份会话记录的那几条换成了 typed（`faces/history_face.rs`）：判据照旧按命令名问，这里照登记表那一层的做法走一遍
///   （入参经 typed 那一处解、回包过一遍真序列化器、失败落成码 ＋ 那一句），盖住 `super::*` 里的同名口；别的命令照旧交只读宿主。
fn answer_at(home: &Path, cmd: &str, args: &serde_json::Value, tz: &crate::Tz) -> Answer {
    use crate::faces::history_face as h;
    use crate::wire::typed::{take, Codes, Command};
    fn via<C: Command>(
        args: &serde_json::Value,
        f: impl FnOnce(C::Req) -> Result<C::Reply, C::Err>,
    ) -> Answer {
        let code = |c: &str| -> &'static str {
            <C::Err as Codes>::ALL
                .iter()
                .find(|k| **k == c)
                .copied()
                .unwrap_or_else(|| panic!("码表外的码 `{c}`"))
        };
        let req = take::<C::Req>(args).map_err(|e| (code(&e.code), e.message))?;
        let got = f(req).map_err(|e| {
            let fail = e.into_fail();
            (code(&fail.code), fail.message)
        })?;
        Ok(serde_json::to_value(got).unwrap())
    }
    match cmd {
        "history-read" => via::<h::HistoryReadCmd>(args, |r| h::read_at(home, r, tz)),
        "history-page" => via::<h::HistoryPageCmd>(args, |r| h::page_at(home, r, tz)),
        "history-lines" => via::<h::HistoryLinesCmd>(args, |r| h::lines_at(home, r, tz)),
        "history-run" => via::<h::HistoryRunCmd>(args, |r| h::run_at(home, r, tz)),
        "history-branch" => via::<h::HistoryBranchCmd>(args, |r| h::branch_at(home, r)),
        "history-tail" => via::<h::HistoryTailCmd>(args, |r| h::tail_at(home, r)),
        "history-record" => via::<h::HistoryRecordCmd>(args, |r| h::record_at(home, r)),
        "sessions-needs" => via::<h::SessionsNeedsCmd>(args, |_| Ok(h::needs_at(home))),
        "history-index" => via::<h::HistoryIndexCmd>(args, |r| h::index_at(home, r)),
        "history-user-inputs" => via::<h::UserInputsCmd>(args, |r| h::user_inputs_at(home, r)),
        "history-turns" => via::<h::HistoryTurnsCmd>(args, |r| h::turns_at(home, r, tz)),
        "history-find" => via::<h::HistoryFindCmd>(args, |r| h::find_at(home, r, tz)),
        "history-facts" => via::<h::HistoryFactsCmd>(args, |r| {
            // 上限表住这台的偏好：夹具家目录里那份 `prefs.json`（没有 ⇒ 空表）。
            let limits = crate::control::machine_prefs::read_at(
                &crate::control::machine_prefs::path_in(home),
            )
            .prefs()
            .limits();
            h::facts_at(home, r, &limits)
        }),
        "backend-log" => via::<super::BackendLog>(args, |r| {
            crate::wire::spec::finish(<super::BackendLog as crate::wire::typed::Handle>::run(
                r,
                crate::wire::typed::Cx {
                    tz: tz.clone(),
                    until: None,
                    needs: (),
                },
            ))
        }),
        "accounts-sessions" => via::<super::AccountsSessions>(args, |_| super::sessions_at(home)),
        "machine-interrupts" => {
            via::<super::MachineInterrupts>(args, |r| Ok(super::interrupts_at(home, &r)))
        }
        "accounts-trust" => via::<super::AccountsTrust>(args, |r| {
            crate::wire::spec::finish(<super::AccountsTrust as crate::wire::typed::Handle>::run(
                r,
                crate::wire::typed::Cx {
                    tz: tz.clone(),
                    until: None,
                    needs: (),
                },
            ))
        }),
        "accounts-list" => via::<super::AccountsListCmd>(args, |r| {
            crate::wire::spec::finish(<super::AccountsListCmd as crate::wire::typed::Handle>::run(
                r,
                crate::wire::typed::Cx {
                    tz: tz.clone(),
                    until: None,
                    needs: (),
                },
            ))
        }),
        other => Err((
            "bad_args",
            crate::common::contract::malformed(&format!("this face has no command `{other}`")),
        )),
    }
}

/// 本族的八条帧命令 —— **要求点名的那八条**，
/// 写成帧面名。它是判据的**异源**那一侧：下面那条从 `stream/inbound/` 源码里数「谁把活交给了
/// `read_face::answer`」，两边必须相等。
/// +2：`history-index` / `history-user-inputs`（要求「`--list-user-inputs` 与骨架
/// `--read-session-from-offset --index` 上帧面」那一句 —— 异源仍是手抄的要求，不是 `stream/inbound/`）。
/// +1：`accounts-trust`（「仍在拨号的 `--account-trust` / `--account-trust-zero`」
/// 随账号域一起上帧面 —— 异源是手抄的要求，不是 `stream/inbound/`）。
const FAMILY: &[&str] = &[
    "accounts-list",
    "accounts-sessions",
    "machine-interrupts",
    "accounts-trust",
    "history-index",
    "history-user-inputs",
    // 按行号取回（异源是手抄的要求「后端给『从第 N 行起 k 行』的读口」，不是 `stream/inbound/`）。
    "history-lines",
    // 会话内查找。
    "history-find",
    // 会话事实出成品（异源是手抄的要求「三样由后端出成品」，不是 `stream/inbound/`）。
    "history-facts",
    // 这台上需手动的会话清单（异源是手机端的要求「一次问一台，不是一条一问」，不是 `stream/inbound/`）。
    "sessions-needs",
    // 主线外清单的冷读（共用扫描图）。
    "history-branch",
    "history-turns",
    "history-read",
    // 记录还在不在（resume 一跳先问；异源是手抄的要求，不是 `stream/inbound/`）。
    "history-record",
    // 按字节分页出记录行（异源是手抄的要求，不是 `stream/inbound/`）。
    // 按运行读一个子运行的记录（异源是「子 agent 的流归各自的运行、通用层按运行读」那条要求，不是 `stream/inbound/`）。
    "history-run",
    "history-page",
    "history-tail",
];

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("c1-read-face-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("projects")).unwrap();
    d
}

/// 一份会话：`n` 行最小 JSON（结构占位），外加可选的 torn 残尾。
fn session(home: &Path, proj: &str, sid: &str, n: usize, torn: bool) -> PathBuf {
    let dir = home.join("projects").join(proj);
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(format!("{sid}.jsonl"));
    let mut body = String::new();
    for i in 0..n {
        body.push_str(&format!("{{\"i\":{i}}}\n"));
    }
    if torn {
        body.push_str("{\"torn\":");
    }
    std::fs::write(&p, body).unwrap();
    p
}

/// ★ 两向相等：`inbound::REGISTRY` 里把活交给本宿主的命令 == [`FAMILY`]。
///
/// 左边从命令表各族的**生产段源码**切 `CommandSpec {` 块数出来（异源：不读本文件的任何常量）。
#[test]
fn the_registry_hands_exactly_the_eight_to_this_host() {
    let families = guard_core::addr::backend_registry_sources();
    let mut got: Vec<String> = families
        .iter()
        .flat_map(|(_, prod)| prod.split("CommandSpec {").skip(1))
        // 块截到它后面紧跟的 typed 那一行为止（那一行不是这一块的）。
        .map(|blk| blk.split("typed::").next().unwrap_or(blk))
        .filter(|blk| blk.contains("read_face::answer"))
        .filter_map(|blk| {
            let at = blk.find("name: \"")? + "name: \"".len();
            Some(blk[at..].split('"').next()?.to_string())
        })
        .collect();
    // 读一份会话记录的那几条（`faces/history_face.rs`）与账号那几条（本宿主）成了 typed：名字从那个类型的 `impl Command` 里 `const NAME` 读。
    for (module, face) in [
        (
            "::faces::history_face::",
            include_str!("../../../src/backend/faces/history_face.rs"),
        ),
        (
            "::faces::read_face::",
            include_str!("../../../src/backend/faces/read_face.rs"),
        ),
    ] {
        for (_, prod) in &families {
            for (at, _) in prod.match_indices(module) {
                let rest = &prod[at + module.len()..];
                // 只认 typed 那一行（`…::read_face::类型>`）；还交给 `read_face::answer` 的那几块上面数过了。
                if !rest.starts_with(|c: char| c.is_ascii_uppercase()) {
                    continue;
                }
                let ty = &rest[..rest.find('>').expect("typed 那一行没收尾")];
                // 这几条与本族同住一个模块、不读会话也不读账号（诊断那两条 · 各台需手动清单的纯计算合并），不在本族里。
                if ["BackendLog", "DriftReportCmd", "NeedsMergeCmd"].contains(&ty) {
                    continue;
                }
                let imp = [
                    format!("impl Command for {ty} {{"),
                    format!("impl crate::wire::typed::Command for {ty} {{"),
                ]
                .iter()
                .find_map(|h| face.find(h.as_str()))
                .unwrap_or_else(|| panic!("`{module}` 里没有 `impl Command for {ty}`"));
                let key = "const NAME: &'static str = \"";
                let n = face[imp..].find(key).expect("没有 `const NAME`") + imp + key.len();
                got.push(face[n..].chars().take_while(|c| *c != '"').collect());
            }
        }
    }
    got.sort();
    let mut want: Vec<String> = FAMILY.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(
        got, want,
        "交给 `read_face::answer` 的帧命令与要求点名的那几条不相等"
    );
    // 那八条也都真在 `hello.commands` 里（`command_names`，从注册表派生）。
    for n in FAMILY {
        assert!(
            crate::stream::inbound::command_names().contains(n),
            "`{n}` 不在 `inbound::command_names()` —— hello 不会宣告它，monitor 的 `accepts` 会拒"
        );
    }
}

/// ★ 本宿主对八条**每一条**都有自己的臂（不是落进兜底那句「本族不认识」）；兜底对别的名字成立（正控）。
#[test]
fn every_member_has_its_own_arm_and_strangers_do_not() {
    let home = scratch("arms");
    for n in FAMILY {
        let (r, diag) = crate::common::contract::tests::diag(|| {
            answer_at(&home, n, &serde_json::json!({}), &Default::default())
        });
        if r.is_err() {
            assert!(
                !diag.contains("has no command"),
                "`{n}` 落进了兜底臂：{diag}"
            );
        }
    }
    let (r, diag) = crate::common::contract::tests::diag(|| {
        answer_at(
            &home,
            "history-nope",
            &serde_json::json!({}),
            &Default::default(),
        )
    });
    match r {
        Err((c, _)) => {
            assert!(
                c == "bad_args" && diag.contains("has no command"),
                "{c}: {diag}"
            )
        }
        Ok(v) => panic!("陌生命令被答了：{v}"),
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 分页读：拼起来**逐字节**等于原文件（含 torn 残尾）；非末页都切在行尾；续点单调。
#[test]
fn paged_read_reassembles_the_file_byte_for_byte() {
    let home = scratch("pages");
    let p = session(&home, "-p", "s1", 50, true);
    let want = std::fs::read(&p).unwrap();
    let path = p.to_string_lossy().into_owned();
    let mut got: Vec<u8> = Vec::new();
    let mut off = 0u64;
    let mut pages = 0;
    loop {
        let pg = crate::observe::history_query::read_page(&home, &path, off, None, 37, 1 << 20)
            .expect("读页");
        assert_eq!(
            pg.next,
            off + pg.bytes.len() as u64,
            "续点必须恰好推进这一页的字节数"
        );
        if !pg.eof {
            assert_eq!(pg.bytes.last(), Some(&b'\n'), "非末页必须切在行尾");
        }
        got.extend_from_slice(&pg.bytes);
        off = pg.next;
        pages += 1;
        if pg.eof {
            break;
        }
        assert!(pages < 10_000, "分页不收敛");
    }
    assert!(pages > 1, "页太大，本条没测到切页");
    assert_eq!(got, want);
    // `until` 收口：[a, b) 恰好是那一段。
    let (a, b) = (8u64, 60u64);
    let pg = crate::observe::history_query::read_page(&home, &path, a, Some(b), 1 << 20, 1 << 20)
        .unwrap();
    assert!(pg.eof);
    assert_eq!(pg.bytes, want[a as usize..b as usize].to_vec());
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 单行比一页还长：续读到行尾；超过 `line_cap` ⇒ `oversized_line`，**不交半行**。
#[test]
fn an_oversized_line_is_refused_not_cut() {
    let home = scratch("big");
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s2.jsonl");
    let long = format!("{{\"x\":\"{}\"}}\n{{\"i\":1}}\n", "a".repeat(5000));
    std::fs::write(&p, &long).unwrap();
    let path = p.to_string_lossy().into_owned();
    let ok = crate::observe::history_query::read_page(&home, &path, 0, None, 100, 1 << 20).unwrap();
    assert_eq!(ok.bytes.last(), Some(&b'\n'));
    assert_eq!(
        ok.bytes.len(),
        long.find('\n').unwrap() + 1,
        "应恰好是那一整行"
    );
    match crate::observe::history_query::read_page(&home, &path, 0, None, 100, 1000) {
        Err((c, _)) => assert_eq!(c, "oversized_line"),
        Ok(pg) => panic!("超长行被交出去了（{} 字节）", pg.bytes.len()),
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 围栏照旧：`projects/` 之外的路径被拒（帧面没有绕开 `fence_under_projects`）。
#[test]
fn the_frame_read_keeps_the_projects_fence() {
    let home = scratch("fence");
    let outside = home.join("outside.jsonl");
    std::fs::write(&outside, "{}\n").unwrap();
    let args = serde_json::json!({"path": outside.to_string_lossy()});
    match answer_at(&home, "history-read", &args, &Default::default()) {
        Err((c, _)) => assert_eq!(c, "refused"),
        Ok(v) => panic!("围栏外的文件被读了：{v}"),
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 尾段那张图：最后 `n` 个可计行的起点与区间。
///
/// 期望值由测试自己从夹具算（最后 `n` 个可计行的起点），不借被测函数。
#[test]
fn the_tail_plan_matches_an_independent_count() {
    let home = scratch("tail");
    let p = session(&home, "-p", "t1", 20, true);
    let bytes = std::fs::read(&p).unwrap();
    let path = p.to_string_lossy().into_owned();
    let v = answer_at(
        &home,
        "history-tail",
        &serde_json::json!({"path": path, "n": 5}),
        &Default::default(),
    )
    .unwrap();
    // 独立算：完整行的起点
    let complete_end = bytes.iter().rposition(|&b| b == b'\n').unwrap() + 1;
    let mut starts = vec![0usize];
    for (i, b) in bytes[..complete_end].iter().enumerate() {
        if *b == b'\n' && i + 1 < complete_end {
            starts.push(i + 1);
        }
    }
    assert_eq!(v["total"].as_u64(), Some(20));
    assert_eq!(v["tail_from"].as_u64(), Some(15));
    assert_eq!(v["split_at"].as_u64(), Some(starts[15] as u64));
    assert_eq!(v["end"].as_u64(), Some(complete_end as u64));
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ F2（→出成品）：骨架索引与大纲清单两条帧命令的应答**就是成品**，
/// 条目与**夹具算出来的**逐条相等（异源：期望的偏移 / 行长 / uuid 从夹具字节自己数，不借被测函数）；
/// 键集合恒等（`{from, end, rows}` / `{from, end, entries}`）—— 不再是按行的头尾三段。
#[test]
fn index_and_user_input_answers_carry_the_rows_the_fixture_predicts() {
    let home = scratch("sr1a-index");
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    // 结构占位：一条用户输入（uuid 命名 `in-*`）、一条 meta 用户行、一条助手行 —— 不采任何真会话正文。
    let rows = [
        r#"{"type":"user","uuid":"in-1","timestamp":"t1","message":{"content":"ask"}}"#,
        r#"{"type":"user","uuid":"out-meta","isMeta":true,"message":{"content":"x"}}"#,
        r#"{"type":"assistant","uuid":"out-asst","message":{"content":[]}}"#,
    ];
    let body: String = rows.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(&p, &body).unwrap();
    let path = p.to_string_lossy().to_string();
    let keys = |v: &serde_json::Value| -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };

    let v = answer_at(
        &home,
        "history-index",
        &serde_json::json!({"path": path, "offset": 0}),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        keys(&v),
        [
            "body", "classes", "count", "end", "fd", "from", "inputs", "n", "next", "o", "sp", "t",
            "u"
        ],
        "骨架索引的成品形状变了"
    );
    let mut o = 0u64;
    let want: Vec<(u64, u64)> = rows
        .iter()
        .map(|r| {
            let n = r.len() as u64 + 1;
            let at = o;
            o += n;
            (at, n)
        })
        .collect();
    let got: Vec<(u64, u64)> = rows_of_index(&v)
        .iter()
        .map(|r| (r["o"].as_u64().unwrap(), r["n"].as_u64().unwrap()))
        .collect();
    assert_eq!(got, want, "索引行的偏移 / 行长与夹具对不上");
    assert_eq!(
        (v["from"].as_u64(), v["end"].as_u64()),
        (Some(0), Some(body.len() as u64))
    );

    let v = answer_at(
        &home,
        "history-user-inputs",
        &serde_json::json!({"path": path}),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        keys(&v),
        ["end", "entries", "from", "next"],
        "大纲清单的成品形状变了"
    );
    let uuids: Vec<&str> = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uuid"].as_str().unwrap())
        .collect();
    assert_eq!(uuids, ["in-1"], "清单里的 uuid 与夹具里的 `in-*` 不相等");
    assert_eq!(v["end"].as_u64(), Some(body.len() as u64));
    // 起点越过文件尾 ⇒ failed（不回一份空清单冒充「没有新的」）。
    let e = answer_at(
        &home,
        "history-user-inputs",
        &serde_json::json!({"path": path, "from": body.len() as u64 + 1}),
        &Default::default(),
    )
    .unwrap_err();
    assert_eq!(e.0, "failed");
    // 围栏：同一套（围栏外 ⇒ 拒）。
    let outside = home.join("outside.jsonl");
    std::fs::write(&outside, "{}\n").unwrap();
    for cmd in ["history-index", "history-user-inputs"] {
        assert!(
            answer_at(
                &home,
                cmd,
                &serde_json::json!({"path": outside.to_string_lossy()}),
                &Default::default()
            )
            .is_err(),
            "`{cmd}` 读了围栏外的文件"
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ →〔C4b 出成品〕`history-find` 的 `hits`：命中集合 == 夹具里 `hit-*` 那几条（异源：期望取自夹具的 uuid 命名），
/// 成品键集合恒等（`{total, hits}`）；围栏同一套。
#[test]
fn the_find_answer_hits_exactly_the_fixture_hits() {
    let home = scratch("sr1a-find");
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    // 结构占位：只有 `hit-*` 那几条的正文里有那个词 —— 不采任何真会话正文。
    let rows = [
        r#"{"type":"user","uuid":"hit-1","message":{"content":"zqxneedle one"}}"#,
        r#"{"type":"user","uuid":"miss-1","message":{"content":"nothing here"}}"#,
        r#"{"type":"user","uuid":"hit-2","message":{"content":"two zqxneedle"}}"#,
    ];
    let body: String = rows.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(&p, &body).unwrap();
    let path = p.to_string_lossy().to_string();
    let v = answer_at(
        &home,
        "history-find",
        &serde_json::json!({"path": path, "query": "zqxneedle"}),
        &Default::default(),
    )
    .unwrap();
    let mut keys: Vec<&String> = v.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["hits", "total"], "查找的成品形状变了");
    assert_eq!(v["total"].as_u64(), Some(2));
    let hits: Vec<&str> = v["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uuid"].as_str().unwrap())
        .collect();
    assert_eq!(
        hits,
        ["hit-1", "hit-2"],
        "命中集合与夹具里的 `hit-*` 不相等"
    );
    assert_eq!(
        answer_at(
            &home,
            "history-find",
            &serde_json::json!({"path": path}),
            &Default::default()
        )
        .unwrap_err()
        .0,
        "bad_args",
        "缺 query 该是 bad_args"
    );
    let outside = home.join("outside.jsonl");
    std::fs::write(&outside, "{}\n").unwrap();
    assert!(answer_at(
        &home,
        "history-find",
        &serde_json::json!({"path": outside.to_string_lossy(), "query": "x"}),
        &Default::default()
    )
    .is_err());
    let _ = std::fs::remove_dir_all(&home);
}

/// 金样那份夹具会话：结构占位（uuid 按角色命名、正文是无意义占位词），不采任何真会话正文。
fn golden_session(home: &Path) -> String {
    let dir = home.join("projects").join("-golden");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("g.jsonl");
    let body = [
        r#"{"type":"user","uuid":"in-1","timestamp":"t1","message":{"role":"user","content":"alpha zqx beta"}}"#,
        r#"{"type":"assistant","uuid":"out-1","timestamp":"t1a","parentUuid":"in-1","message":{"role":"assistant","content":[{"type":"text","text":"gamma zqx"},{"type":"tool_use","name":"x","input":{}}]}}"#,
        "",
        r#"{"type":"user","uuid":"meta-1","timestamp":"t1aa","parentUuid":"out-1","isMeta":true,"message":{"role":"user","content":"meta"}}"#,
        r#"{"type":"user","uuid":"in-2","parentUuid":"meta-1","timestamp":"t2","message":{"role":"user","content":"delta"}}"#,
    ]
    .iter()
    .map(|r| format!("{r}\n"))
    .collect::<String>();
    std::fs::write(&p, body).unwrap();
    p.to_string_lossy().to_string()
}

/// 会话事实那一格的金样夹具：结构占位（id / uuid 按角色命名、正文是无意义占位词），不采任何真会话正文。
/// 四格各走到一次：分叉（首条 user 记录）· 一串接上了的重试 ＋ 一串还没下文的· 写类工具 · usage · 项目目录（开头那条的 cwd，后面进了子目录不跟）；派出子运行的调用与它的结果也在，会话事实不认它们。
fn golden_facts_session(home: &Path) -> String {
    let dir = home.join("projects").join("-golden");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("f.jsonl");
    let body = [
        r#"{"type":"user","uuid":"f-1","timestamp":"t0a","cwd":"/g/proj","forkedFrom":{"sessionId":"src-0","messageUuid":"m-0"},"message":{"content":"q"}}"#,
        r#"{"type":"user","uuid":"f-1b","timestamp":"t0aa","parentUuid":"f-1","isMeta":true,"origin":{"kind":"peer","from":"ag-7","handback":true,"body":"report"},"message":{"content":"<agent-message from=\"ag-7\">report</agent-message>"}}"#,
        r#"{"type":"system","subtype":"api_error","uuid":"rt-1","timestamp":"t0aaa","parentUuid":"f-1b","retryAttempt":1,"maxRetries":10}"#,
        r#"{"type":"assistant","uuid":"f-2","parentUuid":"rt-1","timestamp":"t3","message":{"model":"m-g","usage":{"input_tokens":1,"cache_creation_input_tokens":2,"cache_read_input_tokens":3},"content":[{"type":"tool_use","id":"tu-1","name":"Edit","input":{"file_path":"/w/a.ts"}},{"type":"tool_use","id":"tu-2","name":"Task","input":{"description":"scan","subagent_type":"Explore"}}]}}"#,
        r#"{"type":"user","uuid":"f-3","timestamp":"t3a","parentUuid":"f-2","cwd":"/g/proj/sub","message":{"content":[{"type":"tool_result","tool_use_id":"tu-2","content":"ok"}]}}"#,
        r#"{"type":"assistant","uuid":"f-4","parentUuid":"f-3","timestamp":"t4","message":{"content":[{"type":"tool_use","id":"tu-3","name":"Agent","input":{"prompt":"p1\np2"}}]}}"#,
        r#"{"type":"assistant","uuid":"f-5","parentUuid":"f-4","timestamp":"t5","message":{"content":[{"type":"text","text":"done\nmore"}]}}"#,
        // 后台命令两条：一条还在跑（make test-all），一条收到了收场通知（住 queue-operation）⇒ 账上只剩前一条。
        r#"{"type":"assistant","uuid":"f-6","parentUuid":"f-5","timestamp":"2026-10-09T08:00:00.000Z","message":{"content":[{"type":"tool_use","id":"tu-b1","name":"Bash","input":{"command":"make test-all","run_in_background":true}}]}}"#,
        r#"{"type":"user","uuid":"f-7","parentUuid":"f-6","timestamp":"2026-10-09T08:00:01.000Z","toolUseResult":{"stdout":"","backgroundTaskId":"bb1"},"message":{"content":[{"type":"tool_result","tool_use_id":"tu-b1","content":"Command running in background with ID: bb1."}]}}"#,
        r#"{"type":"assistant","uuid":"f-8","parentUuid":"f-7","timestamp":"2026-10-09T08:10:00.000Z","message":{"content":[{"type":"tool_use","id":"tu-b2","name":"Bash","input":{"command":"python train.py","run_in_background":true}}]}}"#,
        r#"{"type":"user","uuid":"f-9","parentUuid":"f-8","timestamp":"2026-10-09T08:10:01.000Z","toolUseResult":{"stdout":"","backgroundTaskId":"bb2"},"message":{"content":[{"type":"tool_result","tool_use_id":"tu-b2","content":"Command running in background with ID: bb2."}]}}"#,
        r#"{"type":"queue-operation","operation":"enqueue","timestamp":"2026-10-09T08:20:00.000Z","content":"<task-notification>\n<task-id>bb2</task-id>\n<tool-use-id>tu-b2</tool-use-id>\n<status>completed</status>\n<summary>Background command \"train\" completed (exit code 0)</summary>\n</task-notification>"}"#,
        r#"{"type":"system","subtype":"api_error","uuid":"rt-2","timestamp":"t5a","parentUuid":"f-5","retryAttempt":1,"maxRetries":10}"#,
        r#"{"type":"permission-mode","permissionMode":"acceptEdits","sessionId":"s-g"}"#,
        r#"{"type":"cost-state","totalCostUSD":0.4242,"modelUsage":{},"hasUnknownModelCost":false}"#,
        r#"{"type":"attachment","attachment":{"type":"deferred_tools_delta","addedNames":[],"needsAuthMcpServers":["m-n"],"failedMcpServers":[{"name":"m-f","error":"e-f"}]}}"#,
        // 没见过的类型、没有链上身份（那一家的簿记行）：不成卡，`quietUnread` 记一行。
        r#"{"type":"golden-future-ledger","sessionId":"s-g"}"#,
    ]
    .iter()
    .map(|r| format!("{r}\n"))
    .collect::<String>();
    std::fs::write(&p, body).unwrap();
    p.to_string_lossy().to_string()
}

/// 一轮的摘要那一格的金样夹具（结构占位）：第一轮 思考 · 两次调用（一次失败、一次被拒）· 中间的话 · 结论（`end_turn`）；
/// 第二轮还在跑（调用发出去、还没结果）。子运行的记录不算进主线的轮。
fn golden_turns_session(home: &Path) -> String {
    let dir = home.join("projects").join("-golden");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("t.jsonl");
    let body = [
        r#"{"type":"user","uuid":"t-1","timestamp":"t1","message":{"content":"first line\nsecond"}}"#,
        r#"{"type":"assistant","uuid":"t-2","parentUuid":"t-1","timestamp":"t2","message":{"content":[{"type":"thinking","thinking":"h"}]}}"#,
        r#"{"type":"assistant","uuid":"t-3","parentUuid":"t-2","timestamp":"t3","message":{"content":[{"type":"text","text":"between"}]}}"#,
        r#"{"type":"assistant","uuid":"t-4","parentUuid":"t-3","timestamp":"t4","message":{"content":[{"type":"tool_use","id":"u-1","name":"Bash","input":{"command":"c"}}]}}"#,
        r#"{"type":"user","uuid":"t-5","parentUuid":"t-4","timestamp":"t5","message":{"content":[{"type":"tool_result","tool_use_id":"u-1","content":"Exit code 1","is_error":true}]}}"#,
        r#"{"type":"assistant","uuid":"t-6","parentUuid":"t-5","timestamp":"t6","message":{"content":[{"type":"tool_use","id":"u-2","name":"Edit","input":{"file_path":"/w/a"}}]}}"#,
        r#"{"type":"user","uuid":"t-7","parentUuid":"t-6","timestamp":"t7","message":{"content":[{"type":"tool_result","tool_use_id":"u-2","content":"The user doesn't want to proceed with this tool use.","is_error":true}]}}"#,
        r#"{"type":"assistant","uuid":"t-8","parentUuid":"t-7","timestamp":"t8","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"r1\n\nr2\nr3\nr4"}]}}"#,
        r#"{"type":"user","uuid":"t-9","parentUuid":"t-8","timestamp":"t9","message":{"content":"next"}}"#,
        r#"{"type":"assistant","uuid":"t-10","parentUuid":"t-9","timestamp":"t10","message":{"content":[{"type":"tool_use","id":"u-3","name":"Read","input":{"file_path":"/w/b"}}]}}"#,
    ]
    .iter()
    .map(|r| format!("{r}\n"))
    .collect::<String>();
    std::fs::write(&p, body).unwrap();
    p.to_string_lossy().to_string()
}

/// ★★**跨语言金样**：三条帧命令对同一份夹具会话的成品 == `tests/__fixtures__/session-reads.golden.json`。
/// ＋ 第四条 `history-facts`（对它自己那份夹具 [`golden_facts_session`]）。
///
/// 那份金样的另一个读者是 TS 解码器（`tests/frontend/ui/session-reads.vitest.ts` 读同一份文件、逐字段断言）⇒ 两侧**异源**：
/// 后端改一个键名 ⇒ 本条红；TS 解码器改一个键名 ⇒ 那边红。金样是手写落盘的，不是任一侧跑出来就算数的
/// （本条红时印出现打的成品，人读过再改金样）。
#[test]
fn the_three_products_match_the_cross_language_golden() {
    let home = scratch("c4b-golden");
    let path = golden_session(&home);
    // `history-facts` 的 `rev` 是续算表现铸的号（从进程起来那一刻的钟往上数）：比较前换成金样里的 1，只核它在、是数。
    let mut facts = answer_at(
        &home,
        "history-facts",
        &serde_json::json!({"path": golden_facts_session(&home)}),
        &Default::default(),
    )
    .unwrap();
    assert!(facts["rev"].as_u64().is_some_and(|r| r > 1), "{facts}");
    facts["rev"] = serde_json::json!(1);
    let got = serde_json::json!({
        "history-index": answer_at(&home, "history-index", &serde_json::json!({"path": path, "offset": 0}), &Default::default()).unwrap(),
        "history-user-inputs": answer_at(&home, "history-user-inputs", &serde_json::json!({"path": path, "from": 0}), &Default::default()).unwrap(),
        "history-find": answer_at(&home, "history-find", &serde_json::json!({"path": path, "query": "zqx", "include_tools": false, "limit": 500}), &Default::default()).unwrap(),
        "history-facts": facts,
        "history-turns": answer_at(&home, "history-turns", &serde_json::json!({"path": golden_turns_session(&home), "from": 0}), &Default::default()).unwrap(),
    });
    let want: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/session-reads.golden.json"))
            .expect("金样不是合法 JSON");
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(
        got,
        want,
        "帧面成品与跨语言金样不一致。现打：\n{}",
        serde_json::to_string_pretty(&got).unwrap()
    );
}

/// `history-facts` 的 `writers`：这台 pidfile 里同一个会话 id 有两个活进程 ⇒ 两个 pid 都在（升序）；
/// 一个活一个死 ⇒ 只剩活的那一个（界面那一句只在不止一个时说）。pidfile 照真 claude 的形状写（`procStart` 对得上）。
#[cfg(target_os = "linux")]
#[test]
fn facts_name_every_live_process_writing_the_session() {
    let home = scratch("writers");
    let sid = "bbbbbbbb-1111-2222-3333-444444444444";
    let dir = home.join("projects").join("-w");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{sid}.jsonl"));
    std::fs::write(
        &path,
        "{\"type\":\"user\",\"uuid\":\"w-1\",\"message\":{\"content\":\"q\"}}\n",
    )
    .unwrap();
    let pids = crate::observe::watcher::pidfile_dir(&home);
    std::fs::create_dir_all(&pids).unwrap();
    let pidfile = |pid: u32, ticks: u64| {
        let body = format!(
            r#"{{"pid":{pid},"sessionId":"{sid}","kind":"interactive","procStart":"{ticks}"}}"#
        );
        std::fs::write(pids.join(format!("{pid}.json")), body).unwrap();
    };
    let spawn = || {
        let c = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let ticks = crate::platform::proc::proc_starttime(c.id()).expect("读得到起始时刻");
        pidfile(c.id(), ticks);
        c
    };
    let writers = || {
        answer_at(
            &home,
            "history-facts",
            &serde_json::json!({ "path": path }),
            &Default::default(),
        )
        .unwrap()["writers"]
            .clone()
    };
    let (mut a, mut b) = (spawn(), spawn());
    let mut want = vec![a.id(), b.id()];
    want.sort_unstable();
    let two = writers();
    // 一个死了（pidfile 还留着）⇒ 不算。
    let _ = b.kill();
    let _ = b.wait();
    let one = writers();
    let _ = a.kill();
    let _ = a.wait();
    let none = writers();
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(
        two,
        serde_json::json!(want),
        "两个活进程持着同一个会话 id，没都报出来"
    );
    assert_eq!(
        one,
        serde_json::json!([a.id()]),
        "死了的那个进程还被算成在写"
    );
    assert_eq!(none, serde_json::json!([]));
}

/// `history-facts` 的 `needs`：这台 pidfile 说在等（`status: waiting`）⇒ 配上记录里没结果的那一步出成品（种类 · 那一句 · 何时起等）；
/// 不在等 ⇒ `null`；在等的那个进程死了（pidfile 还留着）⇒ 不算。pidfile 照真 claude 的形状写（`procStart` 对得上）。
#[cfg(target_os = "linux")]
#[test]
fn facts_say_what_the_session_is_waiting_for() {
    let home = scratch("needs");
    let sid = "cccccccc-1111-2222-3333-444444444444";
    let dir = home.join("projects").join("-n");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{sid}.jsonl"));
    std::fs::write(
        &path,
        "{\"type\":\"assistant\",\"timestamp\":\"t1\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"id\":\"b1\",\"name\":\"Bash\",\"input\":{\"command\":\"rm -rf build/\"}}]}}\n",
    )
    .unwrap();
    let pids = crate::observe::watcher::pidfile_dir(&home);
    std::fs::create_dir_all(&pids).unwrap();
    // 「没挂答题口」那一句照持着它的进程的环境与命令行现判：环境不从本进程继承那两格（本进程的环境是全局的），
    // 等它真 exec 完再问（见 `held`）。
    let mut c = held(
        std::process::Command::new("cat")
            .env_remove(crate::agents::asks::ANSWER_HOOK_OFF_ENV)
            .env_remove(crate::agents::claudecode::asks::PLUGIN_ENV),
    );
    let ticks = crate::platform::proc::proc_starttime(c.id()).expect("读得到起始时刻");
    let pidfile = |status: &str| {
        let body = format!(
            r#"{{"pid":{},"sessionId":"{sid}","kind":"interactive","procStart":"{ticks}","status":"{status}","waitingFor":"permission prompt","statusUpdatedAt":1700000000000}}"#,
            c.id()
        );
        std::fs::write(pids.join(format!("{}.json", c.id())), body).unwrap();
    };
    let needs = || {
        answer_at(
            &home,
            "history-facts",
            &serde_json::json!({ "path": path }),
            &Default::default(),
        )
        .unwrap()["needs"]
            .clone()
    };
    pidfile("waiting");
    let waiting = needs();
    pidfile("busy");
    let busy = needs();
    pidfile("waiting");
    let _ = c.kill();
    let _ = c.wait();
    let dead = needs();
    let _ = std::fs::remove_dir_all(&home);
    let (waited, rest) = split_waited(&waiting);
    assert_eq!(
        rest,
        serde_json::json!({"kind": "approve", "tool": "Bash", "call": "b1", "what": "rm -rf build/", "sinceMs": 1_700_000_000_000u64, "text": copy_core::copy_text("beSession.needs.approve", &[]), "head": copy_core::copy_text("needs.bar.approve", &[("tool", "Bash")]), "headCode": "rm -rf build/", "tone": "need", "ask": null, "manualText": copy_core::copy_text("beAsk.manual.offElsewhere", &[])})
    );
    // 已等多久在这台算（这台读 pidfile 那一刻减那份 pidfile 里的起点），字由时长那一处写。
    let ms = waited.expect("有起点就有已等多久");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let span = now - 1_700_000_000_000;
    assert!(ms <= span && ms + 60_000 > span, "{ms}");
    assert_eq!(
        waiting["waitedText"],
        copy_core::short_duration(ms),
        "与过程行耗时同一种短写法"
    );
    assert_eq!(busy, serde_json::Value::Null, "不在等却报了需手动");
    assert_eq!(dead, serde_json::Value::Null, "在等的进程死了还算需手动");
}

/// `history-facts` 的上限表住这台的偏好（`prefs.json` 的 `contextLimits`）：上限在这里定；请求里再交一张 ⇒ `bad_args`（那一格删了）。
#[test]
fn history_facts_applies_the_limits_this_machine_keeps() {
    let home = scratch("facts-limits");
    let path = golden_facts_session(&home);
    let ask =
        |args: serde_json::Value| answer_at(&home, "history-facts", &args, &Default::default());
    let v = ask(serde_json::json!({ "path": path })).unwrap();
    assert_ne!(
        v["usage"]["limitFrom"],
        serde_json::json!("setting"),
        "没设就用上了设置"
    );
    let own = crate::platform::fs::Own::issue_at(&home);
    crate::control::machine_prefs::answer_set_in(
        Some(&home),
        &own,
        &serde_json::from_value(serde_json::json!({"contextLimits": {"M-G": 100}})).unwrap(),
    )
    .unwrap();
    let v = ask(serde_json::json!({ "path": path })).unwrap();
    assert_eq!(
        (v["usage"]["limit"].clone(), v["usage"]["limitFrom"].clone()),
        (serde_json::json!(100), serde_json::json!("setting"))
    );
    assert_eq!(
        ask(serde_json::json!({ "path": path, "limits": {"m": 1} }))
            .unwrap_err()
            .0,
        "bad_args"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 上限的真来源是中转：中转看见过这个会话（会话 id ＝ 记录文件名）的请求带了扩展上下文那一项 ⇒ 1M（`relay`），
/// 看见过但没带 ⇒ 默认 200k（`relay`）；没看见过 ⇒ 照原来的次序（这里判不出 ⇒ `assumed`）。
#[test]
fn history_facts_takes_the_context_window_from_what_the_relay_saw() {
    let home = scratch("facts-relay");
    let dir = home.join("projects").join("-relay");
    std::fs::create_dir_all(&dir).unwrap();
    let rec = r#"{"type":"assistant","uuid":"r-1","message":{"model":"claude-x","usage":{"input_tokens":90000},"content":[]}}"#;
    let mut paths = Vec::new();
    for sid in ["rf-wide", "rf-std", "rf-direct"] {
        let p = dir.join(format!("{sid}.jsonl"));
        std::fs::write(&p, format!("{rec}\n")).unwrap();
        paths.push(p.to_string_lossy().to_string());
    }
    let item = ("anthropic-beta", "context-1m");
    crate::observe::relay_marks::note("rf-wide", &[(item, true)]);
    crate::observe::relay_marks::note("rf-std", &[(item, false)]);
    let ask = |p: &str| {
        answer_at(
            &home,
            "history-facts",
            &serde_json::json!({ "path": p }),
            &Default::default(),
        )
        .unwrap()
    };
    let got: Vec<(serde_json::Value, serde_json::Value)> = paths
        .iter()
        .map(|p| {
            let v = ask(p);
            (v["usage"]["limit"].clone(), v["usage"]["limitFrom"].clone())
        })
        .collect();
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(
        got,
        vec![
            (serde_json::json!(1_000_000), serde_json::json!("relay")),
            (serde_json::json!(200_000), serde_json::json!("relay")),
            (serde_json::json!(1_000_000), serde_json::json!("assumed")),
        ]
    );
}

/// ★`history-facts` 经帧面续传：令牌只是应答里的 `{end, rev}` 两格（与线上同形：过一遍 JSON 文本），
/// 文件长了一截之后接着问 == 对长了之后的整份从 0 问（除了新铸的 `rev`）。令牌对不上表里那一份（`rev` 认不得 ·
/// `end` 不是那一份的 · 已经续过一次被换掉了）⇒ `stale_prior`；续点的两道校验：截断 ⇒ `failed`；改写到续点不在行边界上 ⇒ `failed`；
/// 令牌形状不对（缺 `rev` · 多一格 · 旧的整份成品）/ 缺 `path` ⇒ `bad_args`。
#[test]
fn history_facts_resumes_from_its_end_rev_token_and_refuses_a_stale_one() {
    let home = scratch("facts");
    let path = golden_facts_session(&home);
    let ask =
        |args: serde_json::Value| answer_at(&home, "history-facts", &args, &Default::default());
    let token = |v: &serde_json::Value| serde_json::json!({ "end": v["end"], "rev": v["rev"] });
    let no_rev = |v: &serde_json::Value| {
        let mut v = v.clone();
        v.as_object_mut().unwrap().remove("rev");
        v
    };
    let first = ask(serde_json::json!({ "path": path })).unwrap();
    let wire: serde_json::Value =
        serde_json::from_str(&first.to_string()).expect("应答过一遍 JSON 文本");
    assert!(wire["rev"].as_u64().is_some(), "成品里没有 rev：{wire}");
    // 旧的整份成品当令牌 ⇒ 认不得（不留旧兼容）。
    assert_eq!(
        ask(serde_json::json!({ "path": path, "prior": wire }))
            .unwrap_err()
            .0,
        "bad_args"
    );
    let more = r#"{"type":"assistant","uuid":"f-5","message":{"content":[{"type":"tool_use","id":"tu-4","name":"Write","input":{"file_path":"/w/b.ts"}}]}}"#;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    std::io::Write::write_all(&mut f, format!("{more}\n").as_bytes()).unwrap();
    drop(f);
    // `end` 不是那一份的 · `rev` 认不得 ⇒ stale_prior（调用方从 0 重要）。
    let mut off = token(&wire);
    off["end"] = serde_json::json!(wire["end"].as_u64().unwrap() - 1);
    assert_eq!(
        ask(serde_json::json!({ "path": path, "prior": off }))
            .unwrap_err()
            .0,
        "stale_prior"
    );
    let mut unknown = token(&wire);
    unknown["rev"] = serde_json::json!(wire["rev"].as_u64().unwrap() + 1_000_000);
    assert_eq!(
        ask(serde_json::json!({ "path": path, "prior": unknown }))
            .unwrap_err()
            .0,
        "stale_prior"
    );
    let resumed = ask(serde_json::json!({ "path": path, "prior": token(&wire) })).unwrap();
    let whole = ask(serde_json::json!({ "path": path })).unwrap();
    assert_eq!(no_rev(&resumed), no_rev(&whole), "接着问与从 0 问不相等");
    assert_ne!(resumed["rev"], wire["rev"], "续上之后该铸新的 rev");
    assert_ne!(
        no_rev(&resumed),
        no_rev(&first),
        "长了的那一截没进成品（夹具那条写类调用没进改动文件）"
    );
    assert_eq!(
        whole["touchedFiles"],
        serde_json::json!(["/w/a.ts", "/w/b.ts"])
    );
    // 续过一次的旧令牌被换掉了 ⇒ stale_prior。
    assert_eq!(
        ask(serde_json::json!({ "path": path, "prior": token(&wire) }))
            .unwrap_err()
            .0,
        "stale_prior"
    );

    // 截断：续点越过文件尾。
    let wire = whole;
    std::fs::write(&path, "{}\n").unwrap();
    assert_eq!(
        ask(serde_json::json!({ "path": path, "prior": token(&wire) }))
            .unwrap_err()
            .0,
        "failed"
    );
    // 改写：长度够，但续点前一个字节不是换行。
    let end = wire["end"].as_u64().unwrap() as usize;
    std::fs::write(&path, "x".repeat(end + 10)).unwrap();
    let e = ask(serde_json::json!({ "path": path, "prior": token(&wire) })).unwrap_err();
    assert!(e.0 == "failed" && e.1.contains("line boundary"), "{e:?}");
    // 正控：同一个长度、续点恰在行边界上 ⇒ 接着读（证明上一条红的是「不在行边界」而不是别的；读不成那两次表里那一份留着）。
    std::fs::write(&path, format!("{}\n{}\n", "x".repeat(end - 1), "{}")).unwrap();
    assert!(ask(serde_json::json!({ "path": path, "prior": token(&wire) })).is_ok());
    // 形状不对 / 缺 path。
    for bad in [
        serde_json::json!({ "end": wire["end"] }),
        serde_json::json!({ "end": wire["end"], "rev": wire["rev"], "usage": null }),
    ] {
        assert_eq!(
            ask(serde_json::json!({ "path": path, "prior": bad }))
                .unwrap_err()
                .0,
            "bad_args"
        );
    }
    assert_eq!(
        ask(serde_json::json!({ "prior": token(&wire) }))
            .unwrap_err()
            .0,
        "bad_args"
    );
    // 围栏同族：`projects` 之外的文件不读。
    let outside = home.join("outside.jsonl");
    std::fs::write(&outside, "{}\n").unwrap();
    assert!(ask(serde_json::json!({ "path": outside.to_string_lossy() })).is_err());
    let _ = std::fs::remove_dir_all(&home);
}

/// `history-record`：在 ⇒ `present:true`；不在 ⇒ `present:false`（一个答案，不是错误）；
/// 坏 sid ⇒ `bad_args`；`root` == 这棵夹具树的 `projects`。
///
/// 夹具只造结构（目录名 ＋ 占位行），不采真会话正文。符号链接那一格由 `branch_core::find_session_file`
/// 自己的两侧判据管（本条不重验它）。
#[test]
fn history_record_answers_present_absent_and_refuses_a_bad_sid() {
    let home = scratch("record");
    session(&home, "-p", "aaaa-1111", 1, false);
    let ask = |sid: &str| {
        answer_at(
            &home,
            "history-record",
            &serde_json::json!({ "sid": sid }),
            &Default::default(),
        )
    };
    let root = home.join("projects").to_string_lossy().into_owned();
    assert_eq!(
        ask("aaaa-1111").unwrap(),
        serde_json::json!({ "present": true, "root": root })
    );
    assert_eq!(
        ask("bbbb-2222").unwrap(),
        serde_json::json!({ "present": false, "root": root })
    );
    for bad in ["", "../x", "a/b", "a b"] {
        match ask(bad) {
            Err(("bad_args", _)) => {}
            other => panic!("坏 sid {bad:?} 应当 bad_args，实得 {other:?}"),
        }
    }
    // 缺 `sid` 也是 bad_args。
    assert!(matches!(
        answer_at(
            &home,
            "history-record",
            &serde_json::json!({}),
            &Default::default()
        ),
        Err(("bad_args", _))
    ));
    std::fs::remove_dir_all(&home).ok();
}

/// `history-record` 按**这次 resume 要用的账号根**查。
///
/// 夹具两棵树：这台的家（`home`）与一个账号目录（`acct`），sid 只在账号目录里。
/// 不带 `configDir` ⇒ 答不在（与改之前逐字同一问）；带了 ⇒ 答在，`root` == 那棵树的 `projects`（两向）。
/// 坏 `configDir`（相对 · 上跳 · shell 元字符 · 非字符串）⇒ `bad_args`，且**不碰盘**：同一个坏参数配一个
/// 不存在的目录也是 `bad_args`（碰了盘就会答「不在」而不是拒）。结构夹具，不采会话正文。
#[test]
fn gp1_history_record_looks_in_the_account_root_it_is_given() {
    let home = scratch("record-gp1-home");
    let acct = scratch("record-gp1-acct");
    session(&acct, "-p", "cccc-3333", 1, false);
    let ask =
        |args: serde_json::Value| answer_at(&home, "history-record", &args, &Default::default());
    let home_root = home.join("projects").to_string_lossy().into_owned();
    let acct_root = acct.join("projects").to_string_lossy().into_owned();
    let acct_dir = acct.to_string_lossy().into_owned();
    assert_eq!(
        ask(serde_json::json!({ "sid": "cccc-3333" })).unwrap(),
        serde_json::json!({ "present": false, "root": home_root }),
        "不带 configDir ⇒ 查这台的家目录"
    );
    assert_eq!(
        ask(serde_json::json!({ "sid": "cccc-3333", "configDir": null })).unwrap(),
        serde_json::json!({ "present": false, "root": home_root }),
        "null == 缺席"
    );
    assert_eq!(
        ask(serde_json::json!({ "sid": "cccc-3333", "configDir": acct_dir })).unwrap(),
        serde_json::json!({ "present": true, "root": acct_root }),
        "带了 configDir ⇒ 在那棵树里找"
    );
    for bad in [
        serde_json::json!("relative/acct"),
        serde_json::json!(format!("{acct_dir}/../x")),
        serde_json::json!(format!("{acct_dir}$(id)")),
        serde_json::json!(7),
    ] {
        match ask(serde_json::json!({ "sid": "cccc-3333", "configDir": bad })) {
            Err(("bad_args", _)) => {}
            other => panic!("坏 configDir {bad} 应当 bad_args，实得 {other:?}"),
        }
    }
    std::fs::remove_dir_all(&home).ok();
    std::fs::remove_dir_all(&acct).ok();
}

// ════════════════════════════════════════════════════════════════════════════
// `history-lines`：按行号取回
//
//  要求：「⇒ **级 3 是判据**：任何一个订阅侧缓冲都要有上界，满了必须落级 1 或级 2，
//  **不许静默堆**」· 「无索引会话的重放缓冲上界（要先有不依赖索引的取回路）」——
//  本族是那条「不依赖索引的取回路」，它取回来的行号必须与实时 `seq` 同一个空间，否则取回的正文落错位置。
// ════════════════════════════════════════════════════════════════════════════

/// 手写夹具：可计行、空行、全空白行、只有 BOM 的行、带 BOM 的行、CRLF、一行超长、torn 残尾。
/// 行内容是结构占位（不采真会话正文）。
fn lines_fixture() -> (String, Vec<String>) {
    let big = format!("{{\"big\":\"{}\"}}", "x".repeat(200));
    let body = format!(
        "{{\"i\":0}}\n\n\u{feff}{{\"i\":1}}\n   \t\n\u{feff}\n{{\"i\":2}}\r\n{{\"i\":3}}\n{big}\n{{\"i\":5}}\n{{\"torn\":"
    );
    // **手写**的期望：可计行按出现顺序（不从被测函数派生）。
    let want = vec![
        "{\"i\":0}".to_string(),
        "\u{feff}{\"i\":1}".to_string(),
        "{\"i\":2}\r".to_string(),
        "{\"i\":3}".to_string(),
        big,
        "{\"i\":5}".to_string(),
    ];
    (body, want)
}

/// ★ L1：任意 `[from, until)`（含越界、含 `until` 缺席）取回的 == 手写期望的那一段；
/// `next == from + 条数`；`eof` 当且仅当读过了最后一个完整行。
#[test]
fn lines_by_number_match_the_hand_written_countable_rows() {
    let (body, want) = lines_fixture();
    let n = want.len() as u64;
    for from in 0..=n + 1 {
        for until in (from..=n + 2).map(Some).chain([None]) {
            let got = crate::observe::history_query::read_lines_from(
                std::io::Cursor::new(body.as_bytes()),
                from,
                until,
                1 << 20,
                1 << 20,
            )
            .unwrap();
            let hi = until.unwrap_or(u64::MAX).min(n).max(from.min(n));
            let exp: Vec<String> = want[(from.min(n) as usize)..(hi as usize)].to_vec();
            assert_eq!(got.lines, exp, "[{from}, {until:?})");
            assert_eq!(got.from, from);
            assert_eq!(
                got.next,
                from + exp.len() as u64,
                "[{from}, {until:?}) 的续点"
            );
            assert_eq!(
                got.eof,
                until.is_none_or(|u| u > n),
                "[{from}, {until:?}) 的 eof"
            );
        }
    }
}

/// ★ L1：一页装不下就停在行边界、至少交一行；按续点接着要，拼起来 == 全部可计行（逐条）。
#[test]
fn lines_by_number_page_on_row_boundaries_and_reassemble() {
    let (body, want) = lines_fixture();
    for page in [1usize, 7, 30, 250, 1 << 20] {
        let mut got: Vec<String> = Vec::new();
        let mut from = 0u64;
        let mut rounds = 0;
        loop {
            let pg = crate::observe::history_query::read_lines_from(
                std::io::Cursor::new(body.as_bytes()),
                from,
                None,
                page,
                1 << 20,
            )
            .unwrap();
            assert!(
                pg.eof || !pg.lines.is_empty(),
                "page={page}：没到头却一行没交（死循环）"
            );
            got.extend(pg.lines);
            from = pg.next;
            rounds += 1;
            assert!(rounds < 100, "page={page}：翻不完");
            if pg.eof {
                break;
            }
        }
        assert_eq!(got, want, "page={page} 拼回来的不对");
    }
}

/// ★ L1：要的那一段里有超长行 ⇒ `oversized_line`（不截半行）；只是**数过**它（不交）⇒ 照常答。
#[test]
fn lines_by_number_refuse_an_oversized_row_but_can_count_past_it() {
    let (body, want) = lines_fixture();
    let cap = 100; // 夹具那一行 > 200 字节
    match crate::observe::history_query::read_lines_from(
        std::io::Cursor::new(body.as_bytes()),
        3,
        Some(5),
        1 << 20,
        cap,
    ) {
        Err(("oversized_line", _)) => {}
        other => panic!("超长行被交出去了：{other:?}"),
    }
    let after = crate::observe::history_query::read_lines_from(
        std::io::Cursor::new(body.as_bytes()),
        5,
        None,
        1 << 20,
        cap,
    )
    .unwrap();
    assert_eq!(after.lines, want[5..].to_vec());
}

/// ★ L1：同一份夹具的三个出口数的是**同一个**行号空间 —— `history-tail` 的 `total` ·
/// `history-index` 的行数 · `history-lines` 从 0 取到底的条数，两两相等（三个扫描各写各的循环，只共用
/// `lines::counts` 那一个判定）；帧面那一臂的键集合恒等；围栏照旧。
#[test]
fn lines_by_number_share_the_seq_space_with_tail_and_index() {
    let home = scratch("cf2-lines");
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    let (body, want) = lines_fixture();
    std::fs::write(&p, &body).unwrap();
    let path = p.to_string_lossy().into_owned();
    let tail = answer_at(
        &home,
        "history-tail",
        &serde_json::json!({"path": path, "n": 2}),
        &Default::default(),
    )
    .unwrap();
    let index = answer_at(
        &home,
        "history-index",
        &serde_json::json!({"path": path, "offset": 0}),
        &Default::default(),
    )
    .unwrap();
    let lines = answer_at(
        &home,
        "history-lines",
        &serde_json::json!({"path": path}),
        &Default::default(),
    )
    .unwrap();
    let mut keys: Vec<&String> = lines.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(
        keys,
        ["end", "eof", "from", "lines", "next"],
        "成品形状变了"
    );
    let n = want.len() as u64;
    assert_eq!(tail["total"].as_u64(), Some(n), "history-tail 的 total");
    assert_eq!(
        rows_of_index(&index).len() as u64,
        n,
        "history-index 的行数"
    );
    // `history-lines` 出的是**记录行**（只装进界面的那些）⇒ 条数不再等于行数，行号空间由 `next`（数的是可计行）钉。
    //   这份结构占位语料每行都没有 `type` ⇒ 每行一条认不出的，相邻同类并成一条（`count` 是可计行数）。
    assert_eq!(lines["lines"].as_array().map(Vec::len), Some(1));
    assert_eq!(lines["lines"][0]["record"]["count"].as_u64(), Some(n));
    assert_eq!(
        lines["next"].as_u64(),
        Some(n),
        "history-lines 的 next（可计行数）"
    );
    assert_eq!(lines["eof"].as_bool(), Some(true));
    let outside = home.join("outside.jsonl");
    std::fs::write(&outside, "{}\n").unwrap();
    match answer_at(
        &home,
        "history-lines",
        &serde_json::json!({"path": outside.to_string_lossy()}),
        &Default::default(),
    ) {
        Err((c, _)) => assert_eq!(c, "refused"),
        Ok(v) => panic!("围栏外的文件被读了：{v}"),
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ L1 跨 crate：`history-lines` 从 0 取到底，第 k 条的 uuid == 金标准第 k 行（`skeleton-seq-space.golden`，手算；
/// monitor 那一侧 `LineNumberer` 与后端索引对的也是它）；条数 == `#count`（torn 残尾不交）。
#[test]
fn lines_by_number_match_the_shared_seq_space_golden() {
    let data: &[u8] = include_bytes!("../../__fixtures__/skeleton-seq-space.jsonl");
    let golden = include_str!("../../__fixtures__/skeleton-seq-space.golden");
    let mut want: Vec<Option<String>> = Vec::new();
    let mut count = None;
    for l in golden.lines() {
        if let Some(v) = l.strip_prefix("#count\t") {
            count = v.parse::<usize>().ok();
        } else if !l.starts_with('#') {
            let (_, u) = l.split_once('\t').unwrap();
            want.push((u != "-").then(|| u.to_string()));
        }
    }
    assert!(
        !want.is_empty(),
        "金标准一行都没抽到 —— 下面的相等在空集上绿"
    );
    let pg = crate::observe::history_query::read_lines_from(
        std::io::Cursor::new(data),
        0,
        None,
        1 << 20,
        1 << 20,
    )
    .unwrap();
    let got: Vec<Option<String>> = pg
        .lines
        .iter()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l.trim_start_matches('\u{feff}').trim())
                .ok()
                .and_then(|v| v.get("uuid").and_then(|u| u.as_str()).map(str::to_string))
        })
        .collect();
    assert_eq!(got, want);
    assert_eq!(Some(pg.lines.len()), count);
    assert!(pg.eof);
}

/// 账号那两条的入参闸：`accounts-list` 缺 `agent` ⇒ `bad_args`（不猜是哪一家 ——
/// 猜错就是把别家的号按 apikey 号报）；`accounts-trust` 缺 `cwd` / `configDir` 类型不对 ⇒ `bad_args`。
/// 要求：「② 对端错 —— 通道是通的，答案是『不行』」（入参错是对端的明拒，不是一个空答案）。
#[test]
fn the_account_pair_refuses_missing_or_mistyped_arguments() {
    let home = scratch("c4c-args");
    for (cmd, args) in [
        ("accounts-list", serde_json::json!({})),
        ("accounts-list", serde_json::json!({"agent": 7})),
        ("accounts-list", serde_json::json!({"agent": "claud-code"})),
        ("accounts-trust", serde_json::json!({"configDir": null})),
        (
            "accounts-trust",
            serde_json::json!({"configDir": 3, "cwd": "/w"}),
        ),
    ] {
        match answer_at(&home, cmd, &args, &Default::default()) {
            Err((c, _)) => assert_eq!(c, "bad_args", "{cmd} {args}"),
            Ok(v) => panic!("{cmd} {args} 被答了：{v}"),
        }
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ `backend-log`：没装 ⇒ `path: null`；装了 ⇒ 尾部，截断时从截点后第一个换行起（不给半行）。期望手写。
#[test]
fn gap1_backend_log_returns_the_tail_cut_at_a_line() {
    let home = scratch("gap1-log");
    let p = home.join("stderr.log");
    std::fs::write(&p, "first line\nsecond line\nthird\n").unwrap();
    let log_tail = |p: Option<&Path>, max| serde_json::to_value(log_tail(p, max).unwrap()).unwrap();
    assert_eq!(
        log_tail(None, 100),
        serde_json::json!({"path": null, "size": 0, "text": "", "truncated": false})
    );
    let all = log_tail(Some(&p), 1000);
    assert_eq!(
        (
            all["text"].as_str(),
            all["size"].as_u64(),
            all["truncated"].as_bool()
        ),
        (
            Some("first line\nsecond line\nthird\n"),
            Some(29),
            Some(false)
        )
    );
    // 尾 10 字节 = "ine\nthird\n" ⇒ 截点落在 second line 中间 ⇒ 从下一行起。
    let tail = log_tail(Some(&p), 10);
    assert_eq!(
        (tail["text"].as_str(), tail["truncated"].as_bool()),
        (Some("third\n"), Some(true))
    );
    // 帧面那一臂：没被交路径的进程（判据进程就是）⇒ `path: null`，`maxBytes` 形状不对 ⇒ `bad_args`。
    assert_eq!(
        answer_at(
            &home,
            "backend-log",
            &serde_json::json!({}),
            &Default::default()
        )
        .unwrap()["path"],
        Value::Null
    );
    assert_eq!(
        answer_at(
            &home,
            "backend-log",
            &serde_json::json!({"maxBytes": "x"}),
            &Default::default()
        )
        .unwrap_err()
        .0,
        "bad_args"
    );
    let _ = std::fs::remove_dir_all(&home);
}

// ════════════════════════════════════════════════════════════════════════════
// 会话正文那几条出成品：跨语言金样
// ════════════════════════════════════════════════════════════════════════════

/// 金样夹具：结构占位（uuid 按角色命名、正文是无意义占位词），不采任何真会话正文。
/// 四形各一次：不进界面的元数据（占号不出）· 空白行（不占号）· user（带 `cwd`）· assistant；外加一个子 agent。
fn golden_record_session(home: &Path) -> PathBuf {
    let dir = home.join("projects").join("-golden");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("r.jsonl");
    let body = [
        r#"{"type":"permission-mode","permissionMode":"default"}"#,
        "",
        r#"{"type":"user","uuid":"r-1","timestamp":"t1","cwd":"/w","message":{"role":"user","content":"q"}}"#,
        r#"{"type":"assistant","uuid":"r-2","timestamp":"t2","message":{"role":"assistant","content":[{"type":"text","text":"a"}]}}"#,
    ]
    .iter()
    .map(|r| format!("{r}\n"))
    .collect::<String>();
    std::fs::write(&p, body).unwrap();
    let sub = dir.join("r").join("subagents");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(
        sub.join("agent-a1.jsonl"),
        concat!(
            r#"{"type":"user","uuid":"s-1","timestamp":"t3","isSidechain":true,"agentId":"a1","message":{"role":"user","content":"go"}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"s-2","timestamp":"t4","isSidechain":true,"agentId":"a1","message":{"id":"m-s2","role":"assistant","content":[{"type":"text","text":"ok"}]}}"#,
            "\n"
        ),
    )
    .unwrap();
    p
}

/// ★★**跨语言金样**：`history-read`（monitor 旁路快照收）· `history-page` · `history-lines` · `history-branch` · `history-run`
/// （界面收）对同一份夹具的成品 == `tests/__fixtures__/record-reads.golden.json`（路径里夹具那一截换成 `<home>`）。
///
/// 另两个读者读同一份：monitor `frame_query::row_of`（`tests/frontend/shell/frame_query_tests.rs`）·
/// TS 解码器（`tests/frontend/ui/session-reads.vitest.ts` 那一节）⇒ 三侧**异源**：后端改一个键名本条红，收的那两侧改一个键名各自红。
#[test]
fn the_record_products_match_the_cross_language_golden() {
    let home = scratch("mod-golden");
    let p = golden_record_session(&home);
    let path = p.to_string_lossy().into_owned();
    let got = serde_json::json!({
        "history-read": answer_at(&home, "history-read", &serde_json::json!({"path": path}), &Default::default()).unwrap(),
        "history-page": answer_at(&home, "history-page", &serde_json::json!({"path": path}), &Default::default()).unwrap(),
        "history-lines": answer_at(&home, "history-lines", &serde_json::json!({"path": path, "from": 1}), &Default::default()).unwrap(),
        "history-branch": answer_at(&home, "history-branch", &serde_json::json!({"path": path}), &Default::default()).unwrap(),
        "history-run": answer_at(
            &home,
            "history-run",
            &serde_json::json!({"parent": path, "run": "a1"}), &Default::default(),
        )
        .unwrap(),
    });
    let canon = std::fs::canonicalize(&home)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let got: serde_json::Value =
        serde_json::from_str(&got.to_string().replace(&canon, "<home>")).unwrap();
    let want: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/record-reads.golden.json"))
            .expect("金样不是合法 JSON");
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(
        got,
        want,
        "帧面成品与跨语言金样不一致。现打：\n{}",
        serde_json::to_string_pretty(&got).unwrap()
    );
}

/// `history-find` 每条命中旁边一格 `tsText`：那条记录的时刻按这台本地钟写好（今天 `HH:MM` · 昨天 · 更早带日期），界面照抄；读不出时刻 ⇒ 空串。
#[test]
fn find_hits_carry_their_time_text() {
    let home = scratch("p15-find-ts");
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    let ts = "2026-10-07T20:30:15.123Z";
    let rows = [
        format!(
            r#"{{"type":"user","uuid":"hit-1","timestamp":"{ts}","message":{{"content":"zqxneedle one"}}}}"#
        ),
        r#"{"type":"user","uuid":"hit-2","message":{"content":"two zqxneedle"}}"#.to_string(),
    ];
    let body: String = rows.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(&p, &body).unwrap();
    let v = answer_at(
        &home,
        "history-find",
        &serde_json::json!({"path": p.to_string_lossy(), "query": "zqxneedle"}),
        &Default::default(),
    )
    .unwrap();
    let ms = crate::common::time::parse_iso8601_ms(ts).unwrap();
    assert_eq!(
        v["hits"][0]["tsText"],
        crate::common::time::hit_text(ms, &crate::Tz::default()).as_str()
    );
    assert!(v["hits"][0]["tsText"]
        .as_str()
        .is_some_and(|t| t.contains(':')));
    assert_eq!(v["hits"][1]["tsText"], "", "读不出时刻 ⇒ 空串");
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ **「记录正文按需」那份声明真的作用在三条读记录命令上，而且两头都断。**
///
/// 挑格去格的本体（`faces/project.rs`）由 `project_tests` 逐格钉着；本条钉的是**这几条命令登记的成品住处**
/// （`stream/inbound/views.rs::PLACES`）对得上它们真出的应答：照那一处投影之后，工具入参与工具结果正文一个不剩、
/// 正文与推理一格不少、条数与 `next` / `eof` 一类的格不变。住处登记错了（`lines[].record` 写成 `rows[].record`），
/// 投影就落空、应答原样 —— 前一头会红。
///
/// 两头都断：不带声明 ⇒ 去掉的那几样**必须**在（夹具打到了）；带了 ⇒ 一个不剩，且只少了它们。
#[test]
fn the_three_record_reads_each_honour_the_body_on_demand_view_both_ways() {
    let home = scratch("body-on-demand");
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    // 结构占位：`ZQBODY-*` 只住工具入参与工具结果，`ZQKEEP-*` 住正文 · 推理 · 人说的话 —— 不采任何真会话正文。
    let body = [
        r#"{"type":"user","uuid":"u1","timestamp":"2026-01-02T03:04:05.000Z","message":{"role":"user","content":[{"type":"text","text":"ZQKEEP-asked"}]}}"#,
        r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","timestamp":"2026-01-02T03:04:06.000Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"ZQKEEP-think"},{"type":"text","text":"ZQKEEP-said"},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/w/f.txt","zq_extra":"ZQBODY-input"}}]}}"#,
        r#"{"type":"user","uuid":"u2","parentUuid":"a1","timestamp":"2026-01-02T03:04:07.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ZQKEEP-preview\nZQBODY-result"}]}}"#,
    ]
    .iter()
    .map(|r| format!("{r}\n"))
    .collect::<String>();
    std::fs::write(&p, &body).unwrap();
    let path = p.to_string_lossy().to_string();
    const GONE: &[&str] = &["ZQBODY-input", "ZQBODY-result"];
    // 结果的首行预览（`results.*.preview`）是核心另出的一格：省掉结果正文之后它照样在。
    const KEPT: &[&str] = &[
        "ZQKEEP-asked",
        "ZQKEEP-think",
        "ZQKEEP-said",
        "ZQKEEP-preview",
    ];
    let view = serde_json::json!({"omit": {"record": [
        "blocks[type=tool_use].input",
        "blocks[type=tool_result].content",
        "results.*.patch",
    ]}});

    // 三条各自的入参与装成品的那一格。
    let cases: [(&str, serde_json::Value, &str); 3] = [
        ("history-read", serde_json::json!({"path": path}), "rows"),
        ("history-page", serde_json::json!({"path": path}), "lines"),
        (
            "history-lines",
            serde_json::json!({"path": path, "from": 0}),
            "lines",
        ),
    ];
    for (cmd, args, key) in cases {
        let full = answer_at(&home, cmd, &args, &Default::default())
            .unwrap_or_else(|(c, m)| panic!("`{cmd}` 答错了（{c}）：{m}"));
        let plan = crate::stream::inbound::views::plan_for(cmd, &view)
            .unwrap_or_else(|f| panic!("`{cmd}` 不收这份声明：{f:?}"));
        let fold = plan.apply(Some(full.clone())).unwrap();
        let text = |v: &serde_json::Value| serde_json::to_string(&v[key]).unwrap();
        let count = |v: &serde_json::Value| v[key].as_array().map_or(0, Vec::len);

        // ── 不带声明：去掉的那几样**必须**在 ──
        let t = text(&full);
        for m in GONE.iter().chain(KEPT) {
            assert!(
                t.contains(m),
                "`{cmd}` 全量里没有 `{m}` —— 夹具没打到（下面那一半会恒绿）"
            );
        }
        // ── 带了：工具那几格一个不剩，正文一格不少，条数一条不少 ──
        let t = text(&fold);
        for m in GONE {
            assert!(!t.contains(m), "`{cmd}` 照声明去过之后还在给 `{m}`：{t}");
        }
        for m in KEPT {
            assert!(t.contains(m), "`{cmd}` 把正文 `{m}` 也去掉了");
        }
        assert_eq!(count(&fold), count(&full), "`{cmd}` 去的该是格，不是行");
        assert_eq!(count(&fold), 3, "`{cmd}` 的夹具三行都该出成品");
        let strip = |v: &serde_json::Value| {
            let mut o = v.as_object().unwrap().clone();
            o.remove(key);
            o
        };
        assert_eq!(
            strip(&fold),
            strip(&full),
            "`{cmd}` 的 `{key}` 之外的格也变了"
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ **这台上需手动的会话清单**（`sessions-needs`）：恰好是这台此刻活着、那台 pidfile 说在等人的那几个会话；
/// 每一个的 `needs` 就是 `history-facts` 对同一份记录答的那一格（同一处判、同一份字，清单不另判）；
/// 记录找不到的照列（判不出是哪一步，但框是哪种照那台说的）；在跑 · 空闲 · 进程死了的都不在。
#[cfg(target_os = "linux")]
#[test]
fn sessions_needs_lists_the_waiting_sessions_each_with_its_history_facts_needs() {
    let home = scratch("sessions-needs");
    let ask = "dddddddd-1111-2222-3333-444444444444";
    let dir = home.join("projects").join("-n");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{ask}.jsonl"));
    std::fs::write(
        &path,
        "{\"type\":\"assistant\",\"timestamp\":\"t1\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"id\":\"b1\",\"name\":\"Bash\",\"input\":{\"command\":\"rm -rf build/\"}}]}}\n",
    )
    .unwrap();
    let pids = crate::observe::watcher::pidfile_dir(&home);
    std::fs::create_dir_all(&pids).unwrap();
    let mut kids = Vec::new();
    let mut put_as = |sid: &str, status: &str, wait: &str, since: u64| {
        let c = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let ticks = crate::platform::proc::proc_starttime(c.id()).expect("起始时刻");
        std::fs::write(
            pids.join(format!("{}.json", c.id())),
            format!(r#"{{"pid":{},"sessionId":"{sid}","kind":"interactive","procStart":"{ticks}","status":"{status}","waitingFor":"{wait}","statusUpdatedAt":{since}}}"#, c.id()),
        )
        .unwrap();
        kids.push(c);
    };
    let at = 1_700_000_000_000u64;
    put_as("s-busy", "busy", "permission prompt", at);
    put_as("s-idle", "idle", "permission prompt", at);
    put_as(ask, "waiting", "permission prompt", at);
    // 同是批准、起等得晚一些 ⇒ 排在 `ask` 后面；要联网的那个起等得最晚，但危险度最高 ⇒ 排第一。
    put_as("s-norecord", "waiting", "permission prompt", at + 60_000);
    put_as("s-net", "waiting", "sandbox request", at + 120_000);
    put_as("s-dead", "waiting", "permission prompt", at);
    let mut dead = kids.pop().unwrap();
    let _ = dead.kill();
    let _ = dead.wait();
    let list = answer_at(
        &home,
        "sessions-needs",
        &serde_json::json!({}),
        &Default::default(),
    )
    .expect("答了");
    let facts = answer_at(
        &home,
        "history-facts",
        &serde_json::json!({ "path": path }),
        &Default::default(),
    )
    .unwrap();
    for mut c in kids {
        let _ = c.kill();
        let _ = c.wait();
    }
    let _ = std::fs::remove_dir_all(&home);
    let rows = list["waiting"].as_array().expect("waiting 是数组");
    // 序照核心那一处（先答哪个：危险度 · 再等得久的在前），清单不另排。
    let sids: Vec<&str> = rows.iter().map(|r| r["sid"].as_str().unwrap()).collect();
    assert_eq!(sids, vec!["s-net", ask, "s-norecord"], "{list}");
    let row = |sid: &str| rows.iter().find(|r| r["sid"] == sid).unwrap();
    assert!(!facts["needs"].is_null(), "夹具：history-facts 该说在等");
    // 两次问各自读一遍 pidfile ⇒ 已等多久差几毫秒；别的格逐字相同。
    assert_eq!(
        split_waited(&row(ask)["needs"]).1,
        split_waited(&facts["needs"]).1,
        "清单那一格与 history-facts 不是同一份"
    );
    assert_eq!(row("s-norecord")["needs"]["kind"], "approve");
    assert_eq!(row("s-norecord")["needs"]["call"], serde_json::Value::Null);
    for r in rows {
        let mut keys: Vec<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, vec!["hidden", "needs", "sid"], "一行只有这三格：{r}");
    }
}

/// ★ 「分身会话」关掉（这台偏好 `bgTab = hide`）时，分身会话的需手动**照样在**清单里（隐藏只是不占标签页），那一行带隐藏那一格说为什么；
/// 交互会话不带。从前壳在帧进门处整条丢分身会话，连批准框一起丢：隐藏只管占不占标签页，在等人的会话照样要人答。
#[cfg(target_os = "linux")]
#[test]
fn a_hidden_background_session_still_lists_its_needs() {
    let home = scratch("needs-bg-hidden");
    let pids = crate::observe::watcher::pidfile_dir(&home);
    std::fs::create_dir_all(&pids).unwrap();
    let mut kids = Vec::new();
    let mut put_as = |sid: &str, kind: &str| {
        let c = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let ticks = crate::platform::proc::proc_starttime(c.id()).expect("起始时刻");
        std::fs::write(
            pids.join(format!("{}.json", c.id())),
            format!(r#"{{"pid":{},"sessionId":"{sid}","kind":"{kind}","procStart":"{ticks}","status":"waiting","waitingFor":"permission prompt","statusUpdatedAt":1700000000000}}"#, c.id()),
        )
        .unwrap();
        kids.push(c);
    };
    put_as("s-bg", "bg");
    put_as("s-fg", "interactive");
    let here = crate::control::session_hidden::Here {
        rules: crate::control::session_hidden::Rules {
            child_tab: cmd_wire::session::Tab::Hide,
            bg_tab: cmd_wire::session::Tab::Hide,
        },
        ..Default::default()
    };
    let list = serde_json::to_value(crate::faces::history_face::needs_with(
        &home,
        &here,
        &Default::default(),
    ))
    .unwrap();
    for mut c in kids {
        let _ = c.kill();
        let _ = c.wait();
    }
    let _ = std::fs::remove_dir_all(&home);
    let rows = list["waiting"].as_array().expect("waiting 是数组");
    let row = |sid: &str| {
        rows.iter()
            .find(|r| r["sid"] == sid)
            .unwrap_or_else(|| panic!("{sid} 不在需手动清单里：{list}"))
    };
    assert_eq!(row("s-bg")["hidden"]["by"], "bg", "{list}");
    assert!(row("s-bg")["needs"].is_object(), "{list}");
    assert_eq!(
        row("s-fg").get("hidden"),
        Some(&serde_json::Value::Null),
        "不隐藏 ⇒ 格在、写 null：{list}"
    );
}

/// 一份 `needs` 拆成「已等多久（毫秒）」与其余各格（已等的那两格随问的时刻变，单拿出来判）。
/// 先答序跟着已等多久走 ⇒ 一起摘掉，先核它就是那一处算出来的（同一种类、同一个已等多久）。
fn split_waited(n: &serde_json::Value) -> (Option<u64>, serde_json::Value) {
    let mut rest = n.clone();
    let o = rest.as_object_mut().expect("needs 是对象");
    let ms = o.remove("waitedMs").and_then(|v| v.as_u64());
    o.remove("waitedText");
    let kind: crate::observe::facts_query::NeedsKind =
        serde_json::from_value(o["kind"].clone()).expect("needs.kind");
    assert_eq!(
        o.remove("rank").and_then(|v| v.as_u64()),
        Some(crate::observe::facts_query::needs_rank(kind, ms)),
        "needs.rank 不是先答序那一处算的"
    );
    (ms, rest)
}

/// ★ **续算令牌不被投影弄残**：带声明去掉 `touchedFiles`（连同点名去掉 `end` · `rev`）之后，令牌那两格照样在
/// （`views::TOKENS`：投影不去它们），应答里不再另塞一整份；拿它续算出来的与不带声明那一路**逐字相同**（除了各自的 `rev`）。
#[test]
fn a_projected_facts_reply_still_continues_exactly_like_the_whole_one() {
    let home = scratch("facts-token");
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    let edit = |uuid: &str, file: &str| {
        format!(
            r#"{{"type":"assistant","uuid":"{uuid}","timestamp":"2026-01-02T03:04:06.000Z","message":{{"role":"assistant","model":"m","content":[{{"type":"tool_use","id":"t-{uuid}","name":"Edit","input":{{"file_path":"{file}","old_string":"a","new_string":"b"}}}}],"usage":{{"input_tokens":3,"output_tokens":4}}}}}}"#
        ) + "\n"
    };
    std::fs::write(&p, edit("a1", "/w/one.txt")).unwrap();
    let path = p.to_string_lossy().to_string();
    let view = serde_json::json!({"omit": {"facts": ["touchedFiles", "end", "rev"]}});
    let plan = crate::stream::inbound::views::plan_for("history-facts", &view).unwrap();
    let ask = |prior: Option<&serde_json::Value>| {
        let mut args = serde_json::json!({ "path": path });
        if let Some(p) = prior {
            args["prior"] = p.clone();
        }
        answer_at(&home, "history-facts", &args, &Default::default()).unwrap()
    };

    let token = |v: &serde_json::Value| serde_json::json!({ "end": v["end"], "rev": v["rev"] });
    let whole1 = ask(None);
    let seen1 = plan.apply(Some(ask(None))).unwrap();
    assert!(seen1.get("touchedFiles").is_none(), "{seen1}");
    assert!(
        seen1.get("prior").is_none(),
        "应答里不该再塞一整份：{seen1}"
    );
    assert_eq!(
        seen1["end"], whole1["end"],
        "令牌那两格被投影去掉了：{seen1}"
    );
    assert!(seen1["rev"].as_u64().is_some(), "{seen1}");
    assert_ne!(seen1["rev"], whole1["rev"], "两问各铸各的 rev");
    assert_eq!(
        whole1["touchedFiles"],
        serde_json::json!(["/w/one.txt"]),
        "夹具没打到（下面那一半会恒绿）"
    );

    // 记录又长了一行，两路各自续算。
    use std::io::Write as _;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&p)
        .unwrap()
        .write_all(edit("a2", "/w/two.txt").as_bytes())
        .unwrap();
    let mut whole2 = ask(Some(&token(&whole1)));
    let mut seen2 = plan.apply(Some(ask(Some(&token(&seen1))))).unwrap();
    whole2.as_object_mut().unwrap().remove("rev");
    seen2.as_object_mut().unwrap().remove("rev");
    let mut want = whole2.clone();
    want.as_object_mut().unwrap().remove("touchedFiles");
    assert_eq!(seen2, want, "带声明那一路续算出来的与不带的不一样");
    assert_eq!(
        whole2["touchedFiles"],
        serde_json::json!(["/w/one.txt", "/w/two.txt"])
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// 帧面骨架索引按列装回的逐行（判据用：与夹具逐行比）。
/// 读法同界面 `session-reads.ts::decodeIndex`（那一侧的读者由跨语言金样钉）；`sc` 不上帧面（占位不用它）。
pub(crate) fn rows_of_index(v: &serde_json::Value) -> Vec<serde_json::Value> {
    let col = |k: &str| v[k].as_array().cloned().unwrap_or_default();
    let (o, n, t, fd, u) = (col("o"), col("n"), col("t"), col("fd"), col("u"));
    let classes = col("classes");
    let mut at = v["from"].as_u64().unwrap();
    let mut rows: Vec<serde_json::Map<String, serde_json::Value>> = (0
        ..v["count"].as_u64().unwrap() as usize)
        .map(|i| {
            let mut r = serde_json::Map::new();
            at += o[i].as_u64().unwrap();
            r.insert("o".into(), at.into());
            r.insert("n".into(), n[i].clone());
            at += n[i].as_u64().unwrap();
            let k = t[i].as_u64().unwrap() as usize;
            if k > 0 {
                r.insert("t".into(), classes[k - 1].clone());
            }
            if !u[i].is_null() {
                r.insert("u".into(), u[i].clone());
            }
            if fd[i].as_u64() != Some(0) {
                r.insert("fd".into(), fd[i].clone());
            }
            r
        })
        .collect();
    let mut sparse =
        |table: &str, keys: &[&str], skip: &dyn Fn(&str, &serde_json::Value) -> bool| {
            let tv = &v[table];
            let mut i: i64 = -1;
            for (k, gap) in tv["at"].as_array().unwrap().iter().enumerate() {
                i += gap.as_i64().unwrap() + 1;
                for key in keys {
                    let val = &tv[*key][k];
                    if !skip(key, val) {
                        rows[i as usize].insert((*key).into(), val.clone());
                    }
                }
            }
        };
    sparse("body", &["ch", "cj", "pl", "cb", "cl"], &|_, x| {
        x.as_u64() == Some(0)
    });
    sparse("sp", &["v"], &|_, _| false);
    sparse("inputs", &["x", "ts"], &|k, x| {
        k == "ts" && x.as_str() == Some("")
    });
    rows.into_iter()
        .map(|mut r| {
            if let Some(sp) = r.remove("v") {
                r.insert("sp".into(), sp);
            }
            serde_json::Value::Object(r)
        })
        .collect()
}

/// ★ 骨架索引在帧面上**按列排**：每格一列（偏移是与上一行末尾的差，`t` 是本份 `classes` 里的号，0 ＝ 不进界面）；
/// 稀疏的几格（正文那五个数 · 来源 · 用户输入的摘要与时刻）各一张小表，`at` 是与上一项的行号差（连着 ＝ 0）。`sc` 不上帧面。
#[test]
fn the_index_is_packed_by_column_with_offsets_as_differences() {
    use crate::agents::record::RecordClass;
    use crate::observe::history_query::IndexRow;
    let rows = vec![
        IndexRow {
            o: 0,
            n: 100,
            t: Some(RecordClass::Said),
            u: Some("in-1".into()),
            ch: 14,
            pl: 1,
            x: Some("alpha".into()),
            ts: Some("t1".into()),
            ..Default::default()
        },
        IndexRow {
            o: 100,
            n: 194,
            t: Some(RecordClass::Reply),
            u: Some("out-1".into()),
            fd: 2,
            ..Default::default()
        },
        IndexRow {
            o: 300,
            n: 10,
            u: Some("m".into()),
            ..Default::default()
        },
        IndexRow {
            o: 310,
            n: 5,
            t: Some(RecordClass::Said),
            sp: Some("system"),
            sc: true,
            ..Default::default()
        },
    ];
    let v = serde_json::to_value(crate::faces::history_face::pack_index(0, 315, &rows)).unwrap();
    assert_eq!(
        v,
        serde_json::json!({
            "from": 0, "end": 315, "next": null, "count": 4,
            "classes": ["said", "reply"],
            "o": [0, 0, 6, 0], "n": [100, 194, 10, 5], "t": [1, 2, 0, 1], "fd": [0, 2, 0, 0],
            "u": ["in-1", "out-1", "m", null],
            "body": {"at": [0], "ch": [14], "cj": [0], "pl": [1], "cb": [0], "cl": [0]},
            "sp": {"at": [3], "v": ["system"]},
            "inputs": {"at": [0], "x": ["alpha"], "ts": ["t1"]},
        })
    );
    let mut back: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
    back[3].as_object_mut().unwrap().remove("sc");
    assert_eq!(rows_of_index(&v), back, "按列装回来与原来的逐行不一样");
}

/// 起一个持着会话的进程，等它**真 exec 完**再交回：写一行进去、读回同一行 —— 回声只能出自 exec 之后的那个程序，
/// 那时它的命令行与环境都已就位（exec 当中读 /proc/<pid>/environ 回 0 字节，会判成「取不到」）。
/// 它回声后接着等标准输入，活到调用方亲手停它为止，不按墙钟定寿命。
#[cfg(target_os = "linux")]
fn held(cmd: &mut std::process::Command) -> std::process::Child {
    use std::io::{BufRead, Write};
    let mut c = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(c.stdin.as_mut().unwrap(), "up").unwrap();
    let mut line = String::new();
    std::io::BufReader::new(c.stdout.as_mut().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(line, "up\n", "持着会话的进程没回声");
    c
}

/// ccm 起会话时答题 hook 挂不上（起的进程环境里带那个变量）⇒ 会话事实 `answerOff` 写明只能在终端里答批准，
/// 在等批准时 `needs.manualText` 也是这一句（没带那个变量的进程见下一条：看它是不是 ccm 起的）。
#[cfg(target_os = "linux")]
#[test]
fn a_session_whose_answer_hook_could_not_be_hung_says_so() {
    let home = scratch("answer-off");
    let sid = "cccccccc-1111-2222-3333-555555555555";
    let dir = home.join("projects").join("-n");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{sid}.jsonl"));
    std::fs::write(
        &path,
        "{\"type\":\"assistant\",\"timestamp\":\"t1\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"id\":\"b1\",\"name\":\"Bash\",\"input\":{\"command\":\"true\"}}]}}\n",
    )
    .unwrap();
    let pids = crate::observe::watcher::pidfile_dir(&home);
    std::fs::create_dir_all(&pids).unwrap();
    let facts_with = |off: Option<&str>| {
        // 持着会话的那个进程活到这里亲手停它为止（`cat` 等着读它的标准输入，那一头在这里攥着）：
        //   不按墙钟给它定寿命 —— 从前是 `sleep 30`，机器忙到这一问拖过 30 秒时它先走了，留下的僵尸读不出环境，那一格成了 `null`。
        let mut cmd = std::process::Command::new("cat");
        if let Some(v) = off {
            cmd.env(crate::agents::asks::ANSWER_HOOK_OFF_ENV, v);
        } else {
            cmd.env_remove(crate::agents::asks::ANSWER_HOOK_OFF_ENV);
        }
        let mut c = held(&mut cmd);
        let ticks = crate::platform::proc::proc_starttime(c.id()).expect("读得到起始时刻");
        let body = format!(
            r#"{{"pid":{},"sessionId":"{sid}","kind":"interactive","procStart":"{ticks}","status":"waiting","waitingFor":"permission prompt","statusUpdatedAt":1700000000000}}"#,
            c.id()
        );
        let pf = pids.join(format!("{}.json", c.id()));
        std::fs::write(&pf, body).unwrap();
        let v = answer_at(
            &home,
            "history-facts",
            &serde_json::json!({ "path": path }),
            &Default::default(),
        )
        .unwrap();
        let _ = c.kill();
        let _ = c.wait();
        let _ = std::fs::remove_file(&pf);
        v
    };
    use crate::agents::asks::{HookOff, SettingsWhy};
    // 每一种原因各写那一句（自带的会话级设置读不出 · 带上为什么；自定义启动器起的）。
    let cases = [
        (
            HookOff::Settings(SettingsWhy::NotJson),
            copy_core::copy_text(
                "beAsk.manual.offSettings",
                &[("why", &copy_core::copy_text("beAsk.manual.whyJson", &[]))],
            ),
        ),
        (
            HookOff::Settings(SettingsWhy::Unread),
            copy_core::copy_text(
                "beAsk.manual.offSettings",
                &[("why", &copy_core::copy_text("beAsk.manual.whyUnread", &[]))],
            ),
        ),
        (
            HookOff::Launcher,
            copy_core::copy_text("beAsk.manual.offLauncher", &[]),
        ),
        (
            HookOff::Policy,
            copy_core::copy_text("beAsk.manual.offPolicy", &[]),
        ),
        (
            HookOff::Version,
            copy_core::copy_text("beAsk.manual.offVersion", &[]),
        ),
    ];
    let mut seen = Vec::new();
    for (off, said) in cases {
        let v = facts_with(Some(off.env_value()));
        assert_eq!(v["answerOff"], serde_json::json!(said), "{off:?}");
        assert_eq!(v["needs"]["manualText"], serde_json::json!(said), "{off:?}");
        seen.push(said);
    }
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 5, "五种原因五句不同的话");
    let _ = std::fs::remove_dir_all(&home);
}

/// 不是 ccm 起的会话（持着它的进程身上看不出挂过答题口：命令行里没有 hook 客户端那个旗标 · 插件目录变量里没有我们那个目录 ·
/// ccm 没给它留过便条）⇒ `answerOff` 与 `needs.manualText` 写明「别处起的会话，没挂答题口」；看得出挂过的 ⇒ `answerOff` 是 `null`、
/// 只说「只能在终端里答」（那一步不是答题口管得着的框）。
#[cfg(target_os = "linux")]
#[test]
fn a_session_started_elsewhere_says_why_it_cannot_be_answered_here() {
    let home = scratch("answer-elsewhere");
    let sid = "cccccccc-1111-2222-3333-777777777777";
    let dir = home.join("projects").join("-n");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{sid}.jsonl"));
    std::fs::write(
        &path,
        "{\"type\":\"assistant\",\"timestamp\":\"t1\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"id\":\"b1\",\"name\":\"Bash\",\"input\":{\"command\":\"true\"}}]}}\n",
    )
    .unwrap();
    let pids = crate::observe::watcher::pidfile_dir(&home);
    std::fs::create_dir_all(&pids).unwrap();
    let data = crate::platform::paths::data_home().expect("测试档有数据目录");
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let var = crate::agents::claudecode::asks::PLUGIN_ENV;
    // `extra` 接在 sh 的位置参数后面（只为让它出现在命令行里）；`noted` ⇒ 像 ccm 那样给这个进程留一张便条。
    let facts_with = |extra: &[&str], noted: bool| {
        // 同上一条：`sh` 等着读标准输入，活到这里亲手停它为止（不按墙钟给它定寿命）。
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "read l; echo \"$l\"; read _", "sh"])
            .args(extra);
        cmd.env_remove(crate::agents::asks::ANSWER_HOOK_OFF_ENV);
        cmd.env_remove(var);
        let mut c = held(&mut cmd);
        if noted {
            let now = crate::common::time::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            crate::control::launch_account::leave_note(
                &own,
                c.id(),
                Some("acct"),
                false,
                None,
                now + 1,
            )
            .unwrap();
        }
        let ticks = crate::platform::proc::proc_starttime(c.id()).expect("读得到起始时刻");
        let body = format!(
            r#"{{"pid":{},"sessionId":"{sid}","kind":"interactive","procStart":"{ticks}","status":"waiting","waitingFor":"permission prompt","statusUpdatedAt":1700000000000}}"#,
            c.id()
        );
        let pf = pids.join(format!("{}.json", c.id()));
        std::fs::write(&pf, body).unwrap();
        let v = answer_at(
            &home,
            "history-facts",
            &serde_json::json!({ "path": path }),
            &Default::default(),
        )
        .unwrap();
        let _ = c.kill();
        let _ = c.wait();
        let _ = std::fs::remove_file(&pf);
        v
    };
    let elsewhere = facts_with(&[], false);
    let hooked = facts_with(&[crate::agents::asks::ANSWER_HOOK_FLAG, "claude"], false);
    let noted = facts_with(&[], true);
    let _ = std::fs::remove_dir_all(&home);
    let said = copy_core::copy_text("beAsk.manual.offElsewhere", &[]);
    assert_eq!(elsewhere["answerOff"], serde_json::json!(said));
    assert_eq!(elsewhere["needs"]["manualText"], serde_json::json!(said));
    let terminal = serde_json::json!(copy_core::copy_text("beAsk.manual.terminal", &[]));
    for (what, v) in [("命令行里挂着 hook", &hooked), ("ccm 留过便条", &noted)] {
        assert_eq!(v["answerOff"], serde_json::Value::Null, "{what}");
        assert_eq!(v["needs"]["manualText"], terminal, "{what}");
    }
}

/// 手机读的那几条（`history-read` · `history-lines` · `history-tail` · `sessions-needs`）：照手机发的那一份入参
///   （`until: null` 照发）跑出来的应答，与换成 typed 之前那一版写出来的**逐字节**相同。
///   金样两份（read · lines）就是跨语言金样那一份重新序列化（键恒排好序）；别的形是换形之前用旧代码跑出来钉成的字面量。
#[test]
fn the_history_replies_the_phone_reads_keep_their_wire_bytes() {
    let home = scratch("phone-bytes");
    let p = golden_record_session(&home);
    let path = p.to_string_lossy().into_owned();
    let canon = std::fs::canonicalize(&home)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let bytes = |cmd: &str, args: serde_json::Value| {
        serde_json::to_string(&phone_wire(&home, cmd, args))
            .unwrap()
            .replace(&canon, "<home>")
    };
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/record-reads.golden.json")).unwrap();
    let read = bytes(
        "history-read",
        serde_json::json!({"path": path, "offset": 0, "until": null}),
    );
    let lines = bytes(
        "history-lines",
        serde_json::json!({"path": path, "from": 1, "until": null}),
    );
    let read_mid = bytes(
        "history-read",
        serde_json::json!({"path": path, "offset": 55, "until": 200}),
    );
    let lines_mid = bytes(
        "history-lines",
        serde_json::json!({"path": path, "from": 0, "until": 2}),
    );
    let tail = bytes("history-tail", serde_json::json!({"path": path, "n": 2}));
    let needs = bytes("sessions-needs", serde_json::Value::Null);
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(
        read,
        serde_json::to_string(&golden["history-read"]).unwrap()
    );
    assert_eq!(
        lines,
        serde_json::to_string(&golden["history-lines"]).unwrap()
    );
    assert_eq!(
        read_mid,
        r#"{"eof":true,"next":200,"rows":[{"cwd":"/w","end":152,"hash":12521928789128290262,"record":{"agent":"claude","at":"t1","blocks":[{"text":"q","type":"text"}],"cwd":"/w","id":"r-1","t":"said","who":{"speaker":{"kind":"human"},"text":"q"}}},{"end":null,"hash":3774565367099417574}]}"#
    );
    assert_eq!(
        lines_mid,
        r#"{"end":152,"eof":false,"from":0,"lines":[{"cwd":"/w","end":152,"path":"<home>/projects/-golden/r.jsonl","record":{"agent":"claude","at":"t1","blocks":[{"text":"q","type":"text"}],"cwd":"/w","id":"r-1","t":"said","who":{"speaker":{"kind":"human"},"text":"q"}},"seq":1,"session_id":"r"}],"next":2}"#
    );
    assert_eq!(
        tail,
        r#"{"end":273,"file":"dfb90402727f63bf","split_at":55,"tail_from":1,"total":3}"#
    );
    assert_eq!(needs, r#"{"answered":[],"waiting":[]}"#);
    // 清单里有一行的那一形（`needs` 那一份成品与 `history-facts` 同一个类型）。
    let row = {
        use crate::observe::facts_query::{needs_of, PidWait};
        crate::faces::history_face::NeedsList {
            waiting: vec![crate::observe::accounts_query::NeedsRow {
                sid: "s-1".into(),
                needs: needs_of(
                    &[],
                    Some(&PidWait {
                        waiting_for: Some(crate::agents::WaitOn::Worker),
                        since_ms: Some(1),
                        read_at_ms: 2,
                    }),
                )
                .expect("在等就有需手动"),
                hidden: None,
            }],
            answered: vec![crate::faces::history_face::AnsweredRow {
                sid: "s-2".into(),
                said: crate::common::cells::Words("w".into()),
            }],
        }
    };
    let row_bytes = serde_json::to_string(&serde_json::to_value(&row).unwrap()).unwrap();
    // 手机嵌的 Rust 核心按 `cmd-wire` 那一份解这几份（`replies::decode_reply`）：解得开、按类型重写出来逐字节相同。
    for (cmd, b) in [
        ("history-tail", &tail),
        ("sessions-needs", &needs),
        ("sessions-needs", &row_bytes),
        ("history-read", &read),
        ("history-read", &read_mid),
        ("history-lines", &lines),
        ("history-lines", &lines_mid),
    ] {
        phone_decodes_back(cmd, b);
    }
    assert_eq!(
        row_bytes,
        r#"{"answered":[{"said":"w","sid":"s-2"}],"waiting":[{"hidden":null,"needs":{"ask":null,"call":null,"head":"等批准 · 协作请求","headCode":null,"kind":"worker","manualText":null,"rank":3298534883325,"sinceMs":1,"text":"等批准","tone":"need","tool":null,"waitedMs":1,"waitedText":"<1s","what":null},"sid":"s-1"}]}"#
    );
}

/// 读一份会话记录的那几条：入参类型里没有的键 · 类型不对的格 ⇒ `bad_args`（从前悄悄当没给）。
#[test]
fn the_record_reads_refuse_fields_their_request_types_do_not_have() {
    let home = scratch("refuse-stray");
    let p = golden_record_session(&home);
    let path = p.to_string_lossy().into_owned();
    for (cmd, args) in [
        (
            "history-page",
            serde_json::json!({"path": path, "whole": true}),
        ),
        (
            "history-read",
            serde_json::json!({"path": path, "offset": "0"}),
        ),
        (
            "history-lines",
            serde_json::json!({"path": path, "from": -1}),
        ),
        ("history-tail", serde_json::json!({"path": path})),
        (
            "history-branch",
            serde_json::json!({"path": path, "until": 3}),
        ),
        (
            "history-run",
            serde_json::json!({"parent": path, "run": "a1", "seq": 0}),
        ),
        (
            "history-record",
            serde_json::json!({"sid": "s", "config_dir": "/x"}),
        ),
        ("sessions-needs", serde_json::json!({"machine": "m"})),
    ] {
        match answer_at(&home, cmd, &args, &Default::default()) {
            Err(("bad_args", _)) => {}
            other => panic!("{cmd} {args}：该回 bad_args，回的是 {other:?}"),
        }
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// 手机嵌的 Rust 核心按 `cmd-wire` 那张表解 `cmd` 的这份回包：解得开，按类型重写出来与后端写的逐字节相同（不丢格、不改字）。
/// 通用记录的两份金样（各类全格 ＋ 最少格，两家）：每一条放进 `history-read` · `history-lines` 的回包里，
/// 手机按 `cmd-wire` 那一份完整记录解得开、按类型重写出来逐字节相同（缺省格 · 平铺的类别 · 内容块的标签都对得上）。
#[test]
fn every_golden_record_is_decoded_by_the_phone_as_a_whole_record() {
    let mut n = 0;
    for golden in [
        include_str!("../../__fixtures__/record.golden.jsonl"),
        include_str!("../../__fixtures__/record-codex.golden.jsonl"),
    ] {
        for line in golden.lines().filter(|l| !l.trim().is_empty()) {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            let record = row["record"].clone();
            let read = serde_json::json!({"eof": true, "next": 9, "rows": [{"cwd": "/w", "end": 9, "hash": 1, "record": record}]});
            let lines = serde_json::json!({"end": 9, "eof": true, "from": 0, "next": 1, "lines": [{"cwd": null, "end": 9, "path": "p", "record": record, "seq": 0, "session_id": "s"}]});
            phone_decodes_back("history-read", &read.to_string());
            phone_decodes_back("history-lines", &lines.to_string());
            n += 1;
        }
    }
    assert!(n > 40, "金样只有 {n} 条");
}

fn phone_decodes_back(cmd: &str, bytes: &str) {
    let v: serde_json::Value = serde_json::from_str(bytes).unwrap();
    match cmd_wire::replies::decode_reply(cmd, v) {
        // 记录一条一条地解（解不开的那一条成认不出那一条、整份不失败）⇒ 逐字节相同才说明每一条都按完整记录解开了。
        Some(Ok(back)) => assert_eq!(
            serde_json::to_string(&back).unwrap(),
            bytes,
            "{cmd}：重写出来不是原样（有一条没按完整记录解开）"
        ),
        other => panic!("{cmd}：手机解不开后端写的那一份：{other:?}"),
    }
}

/// 照手机那一份入参问一条只读命令，应答原样（成功那一支）。
fn phone_wire(home: &Path, cmd: &str, args: serde_json::Value) -> serde_json::Value {
    answer_at(home, cmd, &args, &Default::default()).unwrap_or_else(|e| panic!("{cmd}: {e:?}"))
}

/// 换了启动器那一支：ccm 给那个进程留的便条说「走的是环境那一路」，而持着会话的进程环境里没有我们那个插件目录
/// （启动器把环境清掉了）⇒ `answerOff` 写明启动器没把环境传下去；带着那个目录 ⇒ 接上了（`null`）。按进程的实际环境判，不按声明。
#[cfg(target_os = "linux")]
#[test]
fn a_session_whose_launcher_dropped_the_environment_says_so() {
    let home = scratch("answer-env-lost");
    let sid = "cccccccc-1111-2222-3333-666666666666";
    let dir = home.join("projects").join("-n");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{sid}.jsonl"));
    std::fs::write(
        &path,
        "{\"type\":\"assistant\",\"timestamp\":\"t1\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"id\":\"b1\",\"name\":\"Bash\",\"input\":{\"command\":\"true\"}}]}}\n",
    )
    .unwrap();
    let pids = crate::observe::watcher::pidfile_dir(&home);
    std::fs::create_dir_all(&pids).unwrap();
    let data = crate::platform::paths::data_home().expect("测试档有数据目录");
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let ours = crate::agents::asks::plugin_dir_under(
        &crate::platform::paths::home_dir()
            .expect("测试档有家目录")
            .to_string_lossy(),
    );
    let var = crate::agents::claudecode::asks::PLUGIN_ENV;
    let facts_with = |dirs: Option<String>| {
        // 活到这里亲手停它为止（`cat` 等着读标准输入），不按墙钟给它定寿命。
        let mut cmd = std::process::Command::new("cat");
        cmd.env_remove(crate::agents::asks::ANSWER_HOOK_OFF_ENV);
        match &dirs {
            Some(v) => cmd.env(var, v),
            None => cmd.env_remove(var),
        };
        let mut c = held(&mut cmd);
        let now = crate::common::time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        crate::control::launch_account::leave_note(&own, c.id(), None, true, None, now + 1)
            .unwrap();
        let ticks = crate::platform::proc::proc_starttime(c.id()).expect("读得到起始时刻");
        let body = format!(
            r#"{{"pid":{},"sessionId":"{sid}","kind":"interactive","procStart":"{ticks}","status":"waiting","waitingFor":"permission prompt","statusUpdatedAt":1700000000000}}"#,
            c.id()
        );
        let pf = pids.join(format!("{}.json", c.id()));
        std::fs::write(&pf, body).unwrap();
        let v = answer_at(
            &home,
            "history-facts",
            &serde_json::json!({ "path": path }),
            &Default::default(),
        )
        .unwrap();
        let _ = c.kill();
        let _ = c.wait();
        let _ = std::fs::remove_file(&pf);
        v
    };
    let lost = facts_with(None);
    let mine = facts_with(Some(format!("/u/mine:{ours}")));
    let _ = std::fs::remove_dir_all(&home);
    let said = copy_core::copy_text("beAsk.manual.offEnvLost", &[]);
    assert_eq!(lost["answerOff"], serde_json::json!(said));
    assert_eq!(lost["needs"]["manualText"], serde_json::json!(said));
    assert_eq!(
        mine["answerOff"],
        serde_json::Value::Null,
        "带着我们的目录 ⇒ 接上了"
    );
}

/// `history-facts` 的入参是类型：多一格 · 一格类型不对 ⇒ `bad_args`（不碰盘）。
#[test]
fn history_facts_refuses_what_its_request_type_does_not_have() {
    let home = scratch("facts-args");
    for args in [
        serde_json::json!({ "path": "/x.jsonl", "extra": 1 }),
        serde_json::json!({ "path": 1 }),
        serde_json::json!({ "path": "/x.jsonl", "prior": 1 }),
        serde_json::json!({}),
    ] {
        let got = answer_at(&home, "history-facts", &args, &Default::default());
        assert_eq!(
            got.as_ref().err().map(|e| e.0),
            Some("bad_args"),
            "{args} ⇒ {got:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// `history-facts` 的 `modelText`：最后一条带用量的回复写的模型原值，由那一家的模型那一处写成字（同 `permissionModeText` 那一形）；
/// 认不出那个值 ⇒ `null`。
#[test]
fn facts_name_the_model_of_the_last_reply() {
    let home = scratch("model-text");
    let dir = home.join("projects").join("-m");
    std::fs::create_dir_all(&dir).unwrap();
    let facts_of = |name: &str, model: &str| {
        let p = dir.join(format!("{name}.jsonl"));
        let body = [
            r#"{"type":"user","uuid":"m-1","cwd":"/m","message":{"content":"q"}}"#.to_string(),
            format!(
                r#"{{"type":"assistant","uuid":"m-2","parentUuid":"m-1","message":{{"model":"{model}","usage":{{"input_tokens":1}},"content":[{{"type":"text","text":"a"}}]}}}}"#
            ),
        ]
        .map(|r| format!("{r}\n"))
        .concat();
        std::fs::write(&p, body).unwrap();
        answer_at(
            &home,
            "history-facts",
            &serde_json::json!({"path": p.to_string_lossy()}),
            &Default::default(),
        )
        .unwrap()
    };
    let named = facts_of("a", "claude-opus-4-5-20251101");
    assert_eq!(named["agent"], "claude", "{named}");
    assert_eq!(
        named["modelText"],
        serde_json::json!(crate::agents::model_text("claude", "claude-opus-4-5-20251101").unwrap()),
        "{named}"
    );
    let odd = facts_of("b", "m-odd");
    assert_eq!(odd["modelText"], serde_json::Value::Null, "{odd}");
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 账号清单每号带的「5h 那一格」就是 `quota-read` 那一格（同一份成品里取，按 kind ＋ 号名对）：账读不出 ⇒ 顶上那一格；
/// 别家的同名号 · 账上没有的号 ⇒ 没有。
#[test]
fn each_listed_account_carries_its_five_hour_cell_from_the_quota_read() {
    let row = |name: &str| {
        serde_json::json!({"name": name, "label": name, "email": "", "configDir": "/d", "isDefault": false, "mode": "isolated",
            "exists": true, "loggedIn": true, "authKind": "subscription", "authReady": true, "selectable": true, "color": 0,
            "keyMasked": null, "baseUrl": null, "badge": {"text": "", "warn": false, "title": ""}, "plate": "x", "quota": null, "unselectable": null, "login": false})
    };
    let mut list: crate::observe::accounts_query::AccountsList = serde_json::from_value(serde_json::json!({
        "meta": {"enabled": true, "acctsDir": "", "manifestPath": "", "updatedAt": null, "sharedStore": null, "count": 2,
            "error": null, "unsupported": null, "nextDefault": null, "effectiveDefault": null, "home": null},
        "accounts": [row("a"), row("z")], "notice": null}))
    .expect("清单形状");
    let mut q = crate::faces::quota_read::specimen();
    q.five_hour = None;
    let agent = q.accounts[0].head.agent.as_str().to_string();
    with_quota(&mut list, &q, &agent);
    let cells = |l: &crate::observe::accounts_query::AccountsList| {
        l.accounts
            .iter()
            .map(|r| r.quota.as_ref().map(|w| w.0.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        cells(&list),
        [q.accounts[0].five_hour.as_ref().map(|w| w.0.clone()), None]
    );
    with_quota(&mut list, &q, "another-agent");
    assert_eq!(cells(&list), [None, None], "别家的同名号不对上");
    q.five_hour = Some(cmd_wire::cells::Words("5h 读不到".into()));
    with_quota(&mut list, &q, &agent);
    assert_eq!(
        cells(&list),
        [Some("5h 读不到".into()), Some("5h 读不到".into())],
        "账读不出 ⇒ 顶上那一格"
    );
}
