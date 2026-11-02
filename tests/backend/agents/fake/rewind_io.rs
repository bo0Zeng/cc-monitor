//! 假 agent 的终端（`agents::fake::rewind::FakeTerm`）接上控制面回退那一串的口子（[`RewindIo`]）：
//! 口子住控制面、终端住适配层 ⇒ 这一半挂在 `control/rewind.rs` 底下（控制面拆成包时跟着走）。

use crate::agents::fake::rewind::FakeTerm;
use crate::common::said::Said;
use crate::control::rewind::{RewindIo, UserLine};
use crate::control::terminals_wire::{Sent, SentResult};

fn delivered() -> Sent {
    Sent {
        result: SentResult::Delivered,
        why: None,
        said: None,
        screen: None,
        acts: vec![],
    }
}

impl RewindIo for FakeTerm {
    fn screen(&mut self) -> Result<Vec<String>, Said> {
        Ok(self.drawn())
    }
    fn press(&mut self, key: &str) -> Result<Sent, Said> {
        self.key(key);
        Ok(delivered())
    }
    fn redrawn(&mut self) -> bool {
        false
    }
    fn pause(&mut self, _ms: u64) {}
    fn say(&mut self, text: &str) -> Result<Sent, Said> {
        self.type_in(text);
        Ok(delivered())
    }
    fn lines(&mut self) -> Result<Vec<UserLine>, Said> {
        Ok(self
            .said
            .iter()
            .map(|l| UserLine {
                uuid: l.uuid.clone(),
                text: l.text.clone(),
            })
            .collect())
    }
}
