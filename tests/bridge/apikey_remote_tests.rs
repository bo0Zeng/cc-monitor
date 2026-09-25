//! 〔RM1a · 第四波〕`apikey_remote.rs` 的判据 —— 上游选择那份凭据文件按机器读写的 monitor 半。
//!
//! # 买到的
//!
//! - 〔US1〕读的两个分派函数退役（读者换成那台后端）：本模块零本机读口。
//! - 〔GP1 · 第四波〕写的那一个：两臂都交那台机器的后端（本机 ＝ 本机常驻后端）；本机那一臂**先核路径**，
//!   那台后端写的不是本 monitor 认的那一份 ⇒ 零次 `apikey-key-set`（真 `InboundClient` × 照脚本答的假后端）。
//! - 远端那一臂在「没有通道」「说不出账号 id」两种失败上**说得出话、话里不带明文**（真走生产函数）。
//! - 命令名跨半边对拍：本侧两个常量 ⇔ 后端 `REGISTRY` 里的 `name:` 字面量。
//!
//! # 买不到的
//!
//! - 真远端那一趟（真 SSH、真后端进程写真文件）：本仓测试不起远端；后端那一半的行为判据住
//!   `tests/backend/accounts/upstream/file_face_tests.rs`，两半之间的线上字节没有金标准。

use super::*;

/// 夹具 key：结构样本，不是任何真 key。
const PLAIN: &str = "sk-FIXTURE-REMOTE-ARM-NOT-A-REAL-KEY";

fn own_production() -> String {
    guard_core::production_code(include_str!("../../src/bridge/src/apikey_remote.rs"))
}

/// 切出一个分派函数里 `Route::Local =>` 与 `Route::Remote(host) =>` 两臂各自那一行。
fn arms_of(src: &str, anchor: &str) -> (String, String) {
    let at = guard_core::find_pinned(src, anchor)
        .unwrap_or_else(|e| panic!("切不出 `{anchor}`（{e}）—— 按红处理"));
    let rest = &src[at..];
    let end = rest.find("\n}\n").expect("函数没有收尾");
    let body = &rest[..end];
    let line_of = |needle: &str| -> String {
        let hits: Vec<&str> = body.lines().filter(|l| l.contains(needle)).collect();
        assert_eq!(
            hits.len(),
            1,
            "`{anchor}` 里 `{needle}` 不是恰好一行：{hits:?}"
        );
        hits[0].to_string()
    };
    (
        line_of("Route::Local =>"),
        line_of("Route::Remote(host) =>"),
    )
}

/// 〔GP1 · 第四波〕**写的那一臂两台同一条路**（主会话 09-25 裁「每台机器一个写者 ＝ 那台的后端」）。
///
/// - 〔US1〕读的两臂整删（读者换成那台后端），本条钉它们不回来；
/// - `write_key_on`：两臂都交 `send_key`（发 `apikey-key-set`），**本机那一臂带 `Some(local_file()?)`**（先核那台后端写的就是
///   本 monitor 认的那一份），远端那一臂 `None`。行为那一半见下面两条台架判据。
/// 〔墓碑 —— RM1a 那一版这里叫 `the_local_arm_never_sends_the_key_to_a_backend`〔散文墓碑〕，钉「本机那一臂只进 `creds_store` 的写口」。〕
#[test]
fn gp1_the_read_arms_stay_local_and_the_write_arm_goes_to_that_machines_backend() {
    let src = own_production();
    // 〔US1 · 4D〕读的那两个分派（`status_on` / `rows_on`）退役：读者换成那台后端（界面经通道问 `apikey-read` /
    //   `apikey-routing`，起会话问 `launch-endpoint`）。本模块生产段从此零本机读口（零命中带正控：写臂那一行在）。
    for gone in ["status_on", "rows_on", "creds_store::read_status", "inject_facts"] {
        assert!(!guard_core::contains_word(&src, gone), "读口又回到了 monitor：`{gone}`");
    }
    let (local, remote) = arms_of(&src, "pub(crate) async fn write_key_on(");
    assert!(
        local.contains("send_key(LOCAL,") && local.contains("Some(local_file()?)"),
        "`write_key_on` 的本机那一臂不是「交本机后端、先核路径」：{local}"
    );
    assert!(
        remote.contains("send_key(host,") && remote.contains("None)"),
        "`write_key_on` 的远端那一臂不是「交那台后端、不核本机路径」：{remote}"
    );
    for arm in [&local, &remote] {
        assert!(
            !arm.contains("creds_store::write"),
            "写臂又进了 monitor 的写口：{arm}"
        );
    }
    assert_eq!(
        LOCAL,
        crate::origin::LOCAL,
        "本机那条长连接的名字与 origin 的本机名对不上"
    );
}

/// 一台照脚本答话的假后端（真 `InboundClient`，登记在一个假主机名下）：记下收到的每一条请求（整行 JSON）。
/// 形状照 `history_tests::relay_endpoint_rig`（RL1）。
mod gp1_rig {
    use crate::backend::control::inbound_client::{
        park, register, unregister, BackendHello, InboundClient,
    };
    use crate::ssh_source::{parse_frame, InboundFrame};
    use serde_json::Value;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    pub(super) struct Rig {
        pub(super) host: String,
        pub(super) seen: Arc<Mutex<Vec<Value>>>,
        client: Arc<InboundClient>,
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            unregister(&self.host, &self.client);
        }
    }

    pub(super) fn rig(host: &str, script: Vec<(&'static str, Value)>) -> Rig {
        let (mon_w, be_r) = tokio::io::duplex(1 << 16);
        let (be_w, mon_r) = tokio::io::duplex(1 << 16);
        let hello = InboundFrame::Hello {
            v: 1,
            build_id: "t".into(),
            host_arch: "x86_64".into(),
            claude_dir: "/tmp".into(),
            homes: vec![],
            capabilities: vec![],
            commands: ["apikey-read", "apikey-key-set"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        };
        let client =
            park(mon_w).into_client(BackendHello::from_hello_frame(&hello).expect("hello"));
        let c2 = Arc::clone(&client);
        tauri::async_runtime::spawn(async move {
            let mut lines = tokio::io::BufReader::new(mon_r).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                if let Some(f) = parse_frame(&l) {
                    crate::backend::control::local_backend::absorb_local_frame(f, Some(&c2));
                }
            }
        });
        let seen = Arc::new(Mutex::new(Vec::<Value>::new()));
        let seen2 = Arc::clone(&seen);
        let mut plan: VecDeque<(&'static str, Value)> = script.into();
        tauri::async_runtime::spawn(async move {
            let mut w = be_w;
            let mut lines = tokio::io::BufReader::new(be_r).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let v: Value = serde_json::from_str(&l).expect("请求不是 JSON");
                let id = v["id"].as_str().unwrap_or_default().to_string();
                let cmd = v["cmd"].as_str().unwrap_or_default().to_string();
                seen2.lock().expect("lock").push(v);
                let line = match plan.front() {
                    Some((c, _)) if *c == cmd => {
                        let (_, data) = plan.pop_front().expect("刚看过");
                        serde_json::json!({"kind":"reply","id":id,"ok":true,"data":data})
                            .to_string()
                    }
                    _ => serde_json::json!({"kind":"reply","id":id,"ok":false,
                        "error":{"code":"unscripted","message":format!("脚本里下一条不是 {cmd}")}})
                    .to_string(),
                };
                if w.write_all(format!("{line}\n").as_bytes()).await.is_err() {
                    break;
                }
            }
        });
        register(host, Arc::clone(&client));
        Rig {
            host: host.to_string(),
            seen,
            client,
        }
    }
}

fn gp1_read_reply(path: &str) -> serde_json::Value {
    serde_json::json!({"configured": false, "masked": "", "path": path, "notice": null, "problem": null})
}

/// 〔GP1 · 第四波 · K1〕本机那一臂**先核路径、再写**：那台后端答的 `path` == 本 monitor 认的那一份 ⇒ 恰一次
/// `apikey-key-set`（`args` 带的是那把 key 与推出来的账号 id）；序列 == `[apikey-read, apikey-key-set]`（数的是假后端收到的）。
#[test]
fn gp1_the_local_write_checks_the_file_then_hands_the_key_to_the_backend() {
    let want = std::path::PathBuf::from("/data/mine/apikey-credentials.json");
    let rig = gp1_rig::rig(
        "gp1-local-same",
        vec![
            (
                "apikey-read",
                gp1_read_reply("/data/mine/apikey-credentials.json"),
            ),
            (
                "apikey-key-set",
                serde_json::json!({"account": "work", "path": "/data/mine/apikey-credentials.json", "masked": "sk-…KEY"}),
            ),
        ],
    );
    tauri::async_runtime::block_on(send_key(
        &rig.host,
        "/home/u/.claude-accts/work",
        PLAIN.to_string(),
        None,
        Some(want),
    ))
    .expect("路径对得上却没写成");
    let seen = rig.seen.lock().unwrap().clone();
    assert_eq!(
        seen.iter()
            .map(|v| v["cmd"].as_str().unwrap_or("").to_string())
            .collect::<Vec<_>>(),
        vec!["apikey-read".to_string(), "apikey-key-set".to_string()]
    );
    assert_eq!(
        seen[0]["args"],
        serde_json::json!({}),
        "核路径那一问带了东西（它不许带明文）"
    );
    assert_eq!(seen[1]["args"]["key"], PLAIN);
    assert_eq!(seen[1]["args"]["account"], "work");
}

/// 〔GP1 · 第四波 · K1′〕🔴 **路径不等 ⇒ 零次 `apikey-key-set`**（`CCM_DATA_DIR` 隔离跑接上了别的数据目录起的后端那一形），
/// 话里两个路径都说出来、不带明文；应答里干脆没有 `path` ⇒ 同样不写。
#[test]
fn gp1_a_backend_writing_another_file_gets_no_key() {
    let want = std::path::PathBuf::from("/tmp/isolated/apikey-credentials.json");
    let rig = gp1_rig::rig(
        "gp1-local-other",
        vec![(
            "apikey-read",
            gp1_read_reply("/home/u/.claude/claudecode-frontend/apikey-credentials.json"),
        )],
    );
    let err = tauri::async_runtime::block_on(send_key(
        &rig.host,
        "/home/u/.claude-accts/work",
        PLAIN.to_string(),
        None,
        Some(want),
    ))
    .expect_err("写的不是同一份却写了");
    assert!(
        err.contains("/tmp/isolated/apikey-credentials.json"),
        "没说这个 monitor 用的是哪一份：{err}"
    );
    assert!(
        err.contains("/home/u/.claude/claudecode-frontend/apikey-credentials.json"),
        "没说那台后端写的是哪一份：{err}"
    );
    assert!(!err.contains(PLAIN), "报错里带着明文：{err}");
    let cmds: Vec<String> = rig
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|v| v["cmd"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(cmds, vec!["apikey-read".to_string()], "路径不等还发了写");

    let rig2 = gp1_rig::rig(
        "gp1-local-nopath",
        vec![("apikey-read", serde_json::json!({"configured": false}))],
    );
    let err = tauri::async_runtime::block_on(send_key(
        &rig2.host,
        "/home/u/.claude-accts/work",
        PLAIN.to_string(),
        None,
        Some(std::path::PathBuf::from("/x")),
    ))
    .expect_err("说不出写哪一份却写了");
    assert!(err.contains("path"), "实得：{err}");
    assert_eq!(rig2.seen.lock().unwrap().len(), 1, "说不出路径还发了写");
}

#[test]
fn the_remote_arm_says_what_went_wrong_without_the_plaintext() {
    // 一个一定没有通道的机器名（生产登记表里不会有它）。
    let host = Origin("rm1a-no-such-host-for-tests".to_string());
    let err = tauri::async_runtime::block_on(write_key_on(
        &host,
        "/home/u/.claude-accts/work",
        PLAIN.to_string(),
        Some("https://up.example.invalid".to_string()),
    ))
    .expect_err("没有通道还说写成了");
    assert!(
        err.contains("rm1a-no-such-host-for-tests"),
        "报错里说不出是哪台机器：{err}"
    );
    assert!(!err.contains(PLAIN), "报错里带着明文：{err}");

    // 说不出账号 id ⇒ 连通道都不问就拒，话里同样不带明文。
    let err = tauri::async_runtime::block_on(write_key_on(&host, "   ", PLAIN.to_string(), None))
        .expect_err("说不出账号 id 还说写成了");
    assert!(err.contains("说不出这是哪个账号"), "实得：{err}");
    assert!(!err.contains(PLAIN), "报错里带着明文：{err}");

}

#[test]
fn the_command_names_are_the_ones_the_backend_registers() {
    let src = include_str!("../../src/backend/inbound.rs");
    for name in [CMD_KEY_SET, CMD_READ] {
        assert_eq!(
            src.matches(&format!("name: \"{name}\",")).count(),
            1,
            "后端 REGISTRY 里 `{name}` 不是恰好一条"
        );
    }
}

// 〔US1 · 4D〕`the_read_answer_is_parsed_from_the_fields_the_backend_declares`〔散文墓碑〕退役：monitor 不再解析 `apikey-read`
//   的状态（界面经通道直接收，形状由跨语言金样 `tests/__fixtures__/apikey.golden.json` 两侧对拍）；写前核路径那一格只读 `path`
//   （`path_from_wire`，由上面 GP1 那两条台架量）。
