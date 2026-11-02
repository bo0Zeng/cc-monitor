//! 起进程那一下：选载体、抢门牌，抢到了（或这条载体不用抢）才接 stderr 落盘。`stream/entry.rs` 的 `run` 调它一次。

use crate::control::resident;
use crate::platform::{Own, OwnLock};
use crate::stderr_log;
use crate::wire::listen;

/// 选载体、抢门牌；抢到了（或这条载体不用抢）才接 stderr 落盘（`install`）。
///
/// 那两份日志属于在听的那一个：已有常驻后端时，后起的这一个若先接，就把在跑那一个的日志滚走（再来一个就删掉）。
/// ⇒ 配置不成立 / 沙箱跑却要占真家 / 抢不到锁都不接，话只落自己原来的 stderr，退出码交回 `run` 退（一条命令都还没收，没有可排空的）。
pub async fn claim_then_log(
    own: &Own,
    env: &dyn Fn(&str) -> Option<String>,
    account_home: Option<&std::path::Path>,
    install: impl FnOnce() -> stderr_log::Installed,
) -> Result<(Option<(own_chan::Listener, OwnLock)>, stderr_log::Installed), i32> {
    let mode = match listen::mode_from(env) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("常驻配置不成立 ⇒ 拒绝起：{e}");
            return Err(listen::EXIT_BAD_LISTEN_CONFIG);
        }
    };
    if mode == listen::Mode::Stdio {
        return Ok((None, install()));
    }
    let Some(dh) = resident::data_home_from(env) else {
        tracing::error!("找不到这台的家 ⇒ 拒绝起常驻");
        return Err(listen::EXIT_BAD_LISTEN_CONFIG);
    };
    // 〔台架防真家〕沙箱跑却要占本账号真家目录里的门牌 ⇒ 不起（10-08 一路台架漏清环境，改写过真家里的门牌）。
    if let Some(why) = resident::sandbox_refusal(env, &dh, account_home) {
        tracing::error!("{why}");
        return Err(listen::EXIT_BAD_LISTEN_CONFIG);
    }
    match resident::claim(own) {
        resident::Claim::Listening(l, held) => Ok((Some((l, held)), install())),
        resident::Claim::Held => {
            // ★★ **抢不到锁就退出，绝不换个地方再起一个**（换地方 = 每台机 N 个后端，中转口与全部 SSH 各 N 份）。
            tracing::error!(
                "这台的家里已有一个常驻后端在听 ⇒ 退出。宿主该**连上去读一行 hello 比对**，对不上就出声并拒绝。"
            );
            Err(listen::EXIT_ADDR_IN_USE)
        }
        resident::Claim::Failed(why) => {
            tracing::error!("{why}");
            Err(listen::EXIT_BAD_LISTEN_CONFIG)
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/main_claim_tests.rs"]
mod tests; // 抢不到门牌的后起者不碰在跑那一个的日志与套接字；沙箱跑不占真家
