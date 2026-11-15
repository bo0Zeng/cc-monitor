//! 登记表本身（另起一份，不碰进程里那一份、不响铃）：挂着的那几条照挂上的先后存。夹具正文都是编的。
//! 什么时候删：挂着的请求不再有先后（名单不再照挂上的先后交）时。

use super::{Book, Waits};
use crate::agents::{AskIn, AskKind};
use serde_json::json;

fn run(sid: &str) -> AskIn {
    AskIn {
        sid: sid.into(),
        kind: AskKind::Approve,
        tool: "Run".into(),
        what: Some("touch a".into()),
        detail: "touch a".into(),
        suggestions: vec![],
        questions: vec![],
        agent_id: None,
        agent_type: None,
        blocks_terminal: false,
        input: json!({"command": "touch a"}),
    }
}

/// ★ 两条会话交错着挂、一共过十条（号跨过 10），每条会话的名单都照它挂上的先后出。
/// 另起一份登记表，号从 1 起：字串序（`ask-10` 排在 `ask-2` 前面）与挂上的先后必然不同。
#[test]
fn hung_asks_come_out_in_the_order_hung_past_ten() {
    let face = crate::agents::fake::ASK;
    let mut book = Book::default();
    let mut hung: [Vec<String>; 2] = [vec![], vec![]];
    for who in [1, 0, 0, 1, 0, 1, 1, 0, 0, 0, 1, 0] {
        let id = book.hang(
            face,
            run(["a", "b"][who]),
            None,
            Waits::Terminal { _watch: None },
        );
        hung[who].push(id);
    }
    for (who, sid) in ["a", "b"].into_iter().enumerate() {
        assert_eq!(
            book.pending(sid)
                .iter()
                .map(|a| a.ask.id.clone())
                .collect::<Vec<_>>(),
            hung[who],
            "{sid} 的名单照挂上的先后"
        );
    }
}
