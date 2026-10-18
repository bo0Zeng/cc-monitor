//! **盘上字节的金样**：经各写口把几份自有文件写进一个临时家目录，逐份字节（沙箱路径换成 `<home>`）与金样逐字比。
//! 读那一半收成 `common/own_state::Doc` 时（不改盘上字节）拿它钉住：写出来的每一份与改之前一个字节都不差。
//!
//! 进了这份金样的：「待办」选择 · 计划认可 · 机器表 · 这台偏好 · 起会话便条 · 一份策略文件 · 主机钥匙库 ·
//! 开终端那一份 ssh 配置 · 一份骨架。没进的那几份（钥匙与资产目录带随机数、上次值收的是别的命令的整份应答、额度账 · 轮换 · 血缘 · 注解 · 装记录 · 清单缓存的写口要整套上下文）
//! 写的那一半同样没动。

use crate::platform::fs::Own;
use std::path::Path;

const GOLDEN: &str = "tests/__fixtures__/own-files.golden.txt";

fn sandbox() -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("ccm-own-bytes-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(&base).expect("sandbox");
    base
}

/// 家目录底下每一份文件：（相对家目录的路径, 内容），路径排好序。
fn files_under(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in crate::platform::fs::Ro::read_dir(&d).expect("list").flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p
                    .strip_prefix(root)
                    .expect("under")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, std::fs::read(&p).expect("read")));
            }
        }
    }
    out.sort();
    out
}

fn write_all(own: &Own, home: &Path) {
    use serde_json::json;
    let req = serde_json::from_value(json!({"op": "decline", "id": "agents"})).expect("req");
    crate::footprint::chores::marks::mark_at(own, &req).expect("marks");
    let req = serde_json::from_value(json!({"op": "selfPaste", "rc": "/x/.bashrc"})).expect("req");
    crate::footprint::chores::marks::mark_at(own, &req).expect("marks");
    crate::plan::review::answer_ack(own, "/w", "s1", "k1", &["k1".to_string()]).expect("review");
    let host = serde_json::from_value(json!({
        "label": "box", "host": "box.lan", "port": 2222, "user": "u", "keyPath": "",
        "addresses": ["box-2.example"], "jump": "", "connect": true
    }))
    .expect("host");
    crate::control::machines::write(own, &[host]).expect("machines");
    crate::control::machine_prefs::write(own, &Default::default()).expect("prefs");
    crate::control::launch_account::leave_note(own, 4242, Some("work"), true, None, 1_700_000_000)
        .expect("note");
    crate::accounts::quota::setup::shelf::write_file(own, "even", "// rotation\nfn pick() {}\n")
        .expect("shelf");
    let w = crate::dial::known_hosts::writer_at(own);
    w.rewrite(&mut |_| Some("box.lan ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA\n".to_string()))
        .expect("known hosts");
    w.terminal_config().expect("ssh config");
    let rec = home.join("rec.jsonl");
    std::fs::write(&rec, b"{}\n").expect("record");
    crate::observe::skeleton::Skeletons::at(own.clone(), 0)
        .store(&rec, &json!({"rows": [1, 2, 3], "tail": "x"}));
}

#[test]
fn own_files_are_written_byte_for_byte_as_before() {
    let home = sandbox();
    let own = Own::issue_at(&home);
    write_all(&own, &home);
    let shown = home.display().to_string();
    let system = crate::platform::ssh_client::system_config();
    let mut got = String::new();
    for (rel, bytes) in files_under(&home.join(".cc-monitor")) {
        let rel = if rel.starts_with("cache/skeleton/") {
            "cache/skeleton/<digest>.json".to_string()
        } else {
            rel
        };
        let text = String::from_utf8_lossy(&bytes)
            .replace(&shown, "<home>")
            .replace(&system, "<system ssh config>");
        got.push_str(&format!("== {rel}\n{text}\n"));
    }
    std::fs::remove_dir_all(&home).ok();
    crate::golden::assert_golden(
        GOLDEN,
        &got,
        "自有文件写出来的字节变了：盘上形状换版（6b）之外不许动",
    );
}
