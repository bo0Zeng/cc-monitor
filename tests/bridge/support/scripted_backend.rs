//! 〔LOC1a · 第四波 4D〕测试台架：在某个 origin 键上登记一条**真的** `InboundClient`，对面是一个**照脚本答**的假后端。
//!
//! 要求住址：`设计/05 §14.6`「本机那几问从『exec 一次性本机后端』改走 `<local>` 长连接」—— 判「真走了长连接」
//! 要数**假后端那一侧**收到的帧命令（异源：不是被测函数自己说的）。
//! 形状照 `history_tests.rs::relay_endpoint_rig`（同一个 `park → into_client` 造法、同一个 `absorb_local_frame` 收回程）。
//!
//! 用法：`#[path = "support/scripted_backend.rs"] mod scripted;`（路径相对引用它的那个测试文件所在目录）。
//! ⚠ 在 `<local>` 上登记前先拿 `inbound_client::local_origin_test_lock()`（登记表是进程内全局的）。

use crate::backend::control::inbound_client::{
    park, register, unregister, BackendHello, InboundClient,
};
use crate::ssh_source::{parse_frame, InboundFrame};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

/// 一格脚本：这条命令来了回什么。`Ok(data)` ⇒ `ok:true`；`Err((code, message))` ⇒ `ok:false`。
pub(crate) type Step = (&'static str, Result<Value, (&'static str, &'static str)>);

pub(crate) struct Rig {
    host: String,
    /// 假后端**收到**的每一条请求：`(cmd, args)`，按到达顺序。
    pub(crate) seen: Arc<Mutex<Vec<(String, Value)>>>,
    client: Arc<InboundClient>,
}

impl Rig {
    pub(crate) fn cmds(&self) -> Vec<String> {
        self.seen
            .lock()
            .expect("lock")
            .iter()
            .map(|(c, _)| c.clone())
            .collect()
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        unregister(&self.host, &self.client);
    }
}

/// `accepts`：hello 里声明认得的命令（`client.accepts` 据此答）。`script` 按顺序答；
/// 答完了还来、或下一条不是它 ⇒ 回 `ok:false`（`unscripted`），让多发 / 错发的那一条当场现形。
pub(crate) fn rig(host: &str, accepts: &[&str], script: Vec<Step>) -> Rig {
    let (mon_w, be_r) = tokio::io::duplex(1 << 20);
    let (be_w, mon_r) = tokio::io::duplex(1 << 20);
    let hello = InboundFrame::Hello {
        v: 1,
        build_id: "t".into(),
        host_arch: "x86_64".into(),
        claude_dir: "/tmp".into(),
        homes: vec![],
        capabilities: vec![],
        commands: accepts.iter().map(|s| s.to_string()).collect(),
        unavailable: vec![],
        uncancellable: vec![],
    };
    let client = park(mon_w).into_client(BackendHello::from_hello_frame(&hello).expect("hello"));
    let c2 = Arc::clone(&client);
    tauri::async_runtime::spawn(async move {
        let mut lines = tokio::io::BufReader::new(mon_r).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            if let Some(f) = parse_frame(&l) {
                crate::backend::control::local_backend::absorb_local_frame(f, Some(&c2));
            }
        }
    });
    let seen = Arc::new(Mutex::new(Vec::<(String, Value)>::new()));
    let seen2 = Arc::clone(&seen);
    let mut plan: VecDeque<Step> = script.into();
    tauri::async_runtime::spawn(async move {
        let mut w = be_w;
        let mut lines = tokio::io::BufReader::new(be_r).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            let v: Value = serde_json::from_str(&l).expect("请求不是 JSON");
            let id = v["id"].as_str().unwrap_or_default().to_string();
            let cmd = v["cmd"].as_str().unwrap_or_default().to_string();
            seen2
                .lock()
                .expect("lock")
                .push((cmd.clone(), v.get("args").cloned().unwrap_or(Value::Null)));
            let line = match plan.front() {
                Some((c, _)) if *c == cmd => match plan.pop_front().expect("刚看过").1 {
                    Ok(data) => serde_json::json!({"kind":"reply","id":id,"ok":true,"data":data}),
                    Err((code, message)) => serde_json::json!({"kind":"reply","id":id,"ok":false,
                        "code":code,"message":message}),
                },
                _ => serde_json::json!({"kind":"reply","id":id,"ok":false,
                    "code":"unscripted","message":format!("脚本里下一条不是 {cmd}")}),
            }
            .to_string();
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
