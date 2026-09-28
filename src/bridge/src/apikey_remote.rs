//! 〔RM1a · 第四波〕上游选择那份凭据文件**按机器**读写 —— monitor 侧留下的那一个发送口。
//!
//! 〔HX2 · 第四波 4D〕今天这里只剩 [`call`]：起会话那一侧问那台机器的后端 `launch-endpoint`（`history::ask_launch_endpoint`）走它。 〔散文墓碑〕
//! 〔墓碑 —— 从前这里还有写 key 那一半（`write_key_on`〔散文墓碑〕· `send_key`〔散文墓碑〕· `local_file`〔散文墓碑〕· `path_from_wire`〔散文墓碑〕，
//!  Tauri 命令 `write_apikey_credentials_key`〔散文墓碑〕的分派）：〔RM1a〕按 origin 交那台机器的后端，〔GP1〕本机那一臂先问 `apikey-read`
//!  核「本机后端写的那份 == 这个 monitor 用的那份」。常驻后端身份带上数据目录之后（`local_backend_host::hello_verdict` 比 hello 的
//!  `host_env`），那一问由**连接本身**答 ⇒ 界面经通道直接发 `apikey-key-set`（`src/apikey-reads.ts::writeApikeyKey`），
//!  账号 id 由那台后端推（`acct_core::apikey_account_id_of_dir`）。monitor 里从此没有明文 key 的具名绑定。〕
//!〔US1〕读状态与「表里有哪几行」早先已退（`status_from_wire`〔散文墓碑〕· `rows_from_wire`〔散文墓碑〕；界面经 `chan.call` 直接问 `apikey-read` / `apikey-routing`）。
//! 〔更早 · RM1a〕本机那一臂进 monitor 自己的写口、「从不把 `apikey-key-set` 发给本机后端」，判据 `the_local_arm_never_sends_the_key_to_a_backend`〔散文墓碑〕。
//!
//! ⇒ **每台机器上这份文件的程序写者恰好一个 ＝ 那台的后端**（主会话 09-25 裁；`调研/第四波记录/GP1.md §3`）。

use crate::backend::control::backend_route::{no_channel, route_call_error, Routed};
use crate::backend::control::inbound_client;
use crate::copy_table::copy_text;
use serde_json::Value;

/// 一趟往返的上限。远端要走一趟长连接，给宽一点（同 `backend_policy::EXIT_POLICY_BUDGET`）。
const BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// 发送口（形状照 `backend_policy::exit_policy_call`）：没通道 / 旧后端不认 / 调用失败，各说各的话。
/// 〔US1〕起会话那一侧问 `launch-endpoint` 也走这一个（`history::ask_launch_endpoint`）—— 同一族（上游选择的帧面），不另写一份。 〔散文墓碑〕
pub(crate) async fn call(host: &str, cmd: &str, args: Value) -> Result<Value, String> {
    let Some(client) = inbound_client::client_for(host) else {
        return Err(said(no_channel(host)));
    };
    if !client.accepts(cmd) {
        return Err(copy_text(
            "rsApikeyRemote.call.tooOld",
            &[("host", &host.to_string())],
        ));
    }
    let data = client.call(cmd, args, BUDGET).await.map_err(|e| {
        said(route_call_error(&e, |_code, message| {
            copy_text(
                "rsApikeyRemote.call.failed",
                &[
                    ("host", &host.to_string()),
                    ("message", &message.to_string()),
                ],
            )
        }))
    })?;
    data.ok_or_else(|| copy_text("rsApikeyRemote.call.noData", &[("host", &host.to_string())]))
}

/// 三态里给人看的那句话（同 `backend_policy::said`）。`Done` 在本族走不到。
fn said(r: Routed) -> String {
    match r {
        Routed::NoChannel(s) | Routed::Refused(s) => s,
        Routed::Done => copy_text("rsApikeyRemote.call.internal", &[]),
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/apikey_remote_tests.rs"]
mod tests;
