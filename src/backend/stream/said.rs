//! 那几条命令被拒时**给人看的那一句**（从界面搬来的五张「码 → 句」表：结束会话 · 读画面 / 送字 · cc-bus 六条 · 账号库那几条）。
//!
//! 后端这一端写好句子，界面原样上屏；处理器原来那句（里面带着下层原话）整句进「复制详情」的原话那一项。
//! 句子里只有后端知道的对象（结束会话的会话名 · cc-bus 的收件人 / 被收掉的 id）；后端不知道界面怎么称呼的（终端的显示名 ·
//! 名单里要查的那个 id）不写，界面在句子旁边放对象（灰字 / 区块头）。
//! 没有表的命令 ⇒ `None`（处理器那句照原样）。

use copy_core::copy_text;

/// `cmd` 这一条被拒成 `code` 时的那一句；这条命令没有表 ⇒ `None`。
pub(crate) fn reword(cmd: &str, args: &serde_json::Value, code: &str) -> Option<String> {
    let arg = |k: &str| {
        args.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    Some(match cmd {
        "kill" => kill(&arg("name"), code),
        "terminal-preview" => preview(code),
        "terminal-input" => copy_text("terminalReads.input.refused", &[]),
        "bus-list" => match code {
            "not_installed" => copy_text("ccBus.online.notInstalled", &[]),
            "timed_out" => copy_text("ccBus.online.timedOut", &[]),
            _ => copy_text("ccBus.online.otherCode", &[]),
        },
        "bus-send" => bus_send(&arg("to"), code),
        "bus-kill" => {
            let id = arg("id");
            match code {
                "bad_args" => copy_text("ccBus.kill.invalidArgs", &[("id", &id)]),
                "bad_id" => copy_text("ccBus.kill.badId", &[]),
                "not_installed" => copy_text("ccBus.kill.notInstalled", &[("id", &id)]),
                "timed_out" => copy_text("ccBus.kill.timedOut", &[("id", &id)]),
                _ => copy_text("ccBus.kill.otherCode", &[("id", &id)]),
            }
        }
        "bus-spawn" => match code {
            "bad_args" => copy_text("ccBus.spawn.invalidArgs", &[]),
            "bad_id" => copy_text("ccBus.spawn.badId", &[]),
            "not_installed" => copy_text("ccBus.spawn.notInstalled", &[]),
            "timed_out" => copy_text("ccBus.spawn.timedOut", &[]),
            _ => copy_text("ccBus.spawn.otherCode", &[]),
        },
        "bus-broadcast" => match code {
            "bad_args" => copy_text("ccBus.broadcast.invalidArgs", &[]),
            "bad_id" => copy_text("ccBus.broadcast.badId", &[]),
            "not_installed" => copy_text("ccBus.broadcast.notInstalled", &[]),
            "timed_out" => copy_text("ccBus.broadcast.timedOut", &[]),
            _ => copy_text("ccBus.broadcast.otherCode", &[]),
        },
        "bus-state" | "bus-inbox" => match code {
            "not_installed" => copy_text("ccBus.read.notInstalled", &[]),
            "timed_out" => copy_text("ccBus.read.timedOut", &[]),
            "bad_id" => copy_text("ccBus.read.badId", &[]),
            _ => copy_text("ccBus.read.otherCode", &[]),
        },
        // 账号库那几条：契约对不上是两端版本不配；其余各档处理器已经说成人话了，照原样。
        c if c.starts_with("accounts-") && code == "bad_args" => {
            copy_text("accountOps.said.contract", &[])
        }
        _ => return None,
    })
}

fn kill(target: &str, code: &str) -> String {
    match code {
        "bad_args" => copy_text("tmuxControl.kill.badName", &[("target", target)]),
        "no_tmux" => copy_text("tmuxControl.kill.noTmux", &[("target", target)]),
        "no_such_session" => copy_text("tmuxControl.kill.noSuchSession", &[("target", target)]),
        "wrong_owner" => copy_text("tmuxControl.kill.wrongOwner", &[("target", target)]),
        "too_many_windows" => copy_text("tmuxControl.kill.tooManyWindows", &[("target", target)]),
        "kill_failed" => copy_text("tmuxControl.kill.failed", &[("target", target)]),
        "child_timed_out" => copy_text("tmuxControl.kill.childTimedOut", &[("target", target)]),
        _ => copy_text("tmuxControl.kill.otherCode", &[("target", target)]),
    }
}

fn preview(code: &str) -> String {
    match code {
        "no_tmux" => copy_text("terminalReads.preview.noTmux", &[]),
        "no_server" => copy_text("terminalReads.preview.noServer", &[]),
        "no_such_session" => copy_text("terminalReads.preview.noSuchSession", &[]),
        "capture_failed" => copy_text("terminalReads.preview.failed", &[]),
        "bad_target" | "bad_args" => copy_text("terminalReads.preview.badTarget", &[]),
        "not_known" => copy_text("terminalReads.preview.notKnown", &[]),
        "ambiguous" => copy_text("terminalReads.preview.ambiguous", &[]),
        "unobservable" => copy_text("terminalReads.preview.unobservable", &[]),
        "child_timed_out" => copy_text("terminalReads.preview.childTimedOut", &[]),
        _ => copy_text("terminalReads.preview.otherCode", &[]),
    }
}

fn bus_send(id: &str, code: &str) -> String {
    match code {
        "bad_args" => copy_text("ccBus.send.invalidArgs", &[("id", id)]),
        "bad_id" => copy_text("ccBus.send.badId", &[]),
        "not_installed" => copy_text("ccBus.send.notInstalled", &[("id", id)]),
        "rejected" => copy_text("ccBus.send.rejected", &[("id", id)]),
        "timed_out" => copy_text("ccBus.send.timedOut", &[("id", id)]),
        "too_long" => copy_text("ccBus.send.tooLong", &[("id", id)]),
        _ => copy_text("ccBus.send.otherCode", &[("id", id)]),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/stream/said_tests.rs"]
mod tests;
