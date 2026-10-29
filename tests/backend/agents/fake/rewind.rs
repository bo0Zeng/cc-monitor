//! 假 agent 的回退菜单（[`FACE`]）与它的一个终端（[`FakeTerm`]）：屏字 · 键 · 确认页那几项都与 Claude 不同形，
//! 通用层的回退那一串（`control/rewind.rs`）拿它跑 —— 跑得通才算那一串没认 Claude 的字。
//!
//! | 样子 | Claude | 本假 agent |
//! |---|---|---|
//! | 输入行 | `> …`（横线框里） | `fake> …` |
//! | 在出字 | `esc to interrupt` | `~ working (ctrl-c stops)` |
//! | 回退列表 | 标题 `Rewind`、选中标 `❯`、末行 `(current)` | 标题 `<< go back to >>`、`[x]` / `[ ]`、末行 `<now>` |
//! | 确认页 | `❯ 1. Restore conversation` … | 标题 `<< then what >>`、`(*)` / `( )` |
//! | 退一层 · 打断 | Esc · Esc | ← · Ctrl-C |
//!
//! 终端按收到的键改自己的状态，像那一家那样：两下 Esc 被并成一下时列表不出来、输入行上按 ↑ 调出输入历史、
//! 有字时按 Enter 就把它说出去 —— 通用层哪一步没核对就按，判据当场看得见。

use crate::agents::{RewindChoice, RewindFace, RewindItem, RewindRow, RewindScreen};
use crate::control::rewind::{RewindIo, UserLine};
use crate::control::terminals::CmdErr;
use crate::control::terminals_wire::{Sent, SentResult};
use cmd_wire::rewind::RewindHow;

const PROMPT: &str = "fake> ";
const BUSY: &str = "~ working (ctrl-c stops)";
const LIST_TITLE: &str = "<< go back to >>";
const CONFIRM_TITLE: &str = "<< then what >>";
const NOW_ROW: &str = "<now>";
/// 确认页那几项（「连文件一起」只在那一句之后改过文件时有）。
const TALK: &str = "talk only";
const FILES: &str = "talk + files";
const NEVER: &str = "forget it";
/// 列表上一句最多显示几个字（多了截短、加 `…`）。
const ROW_CHARS: usize = 12;

/// 本假 agent 的回退菜单。
pub(crate) const FACE: RewindFace = RewindFace {
    open: &["esc", "esc"],
    open_gap_ms: 40,
    up: "up",
    down: "down",
    enter: "enter",
    back: "left",
    interrupt: Some("ctrl-c"),
    erase: Some("backspace"),
    read,
    choices: CHOICES,
};

const CHOICES: &[RewindChoice] = &[
    RewindChoice {
        how: RewindHow::Conversation,
        label: TALK,
        text: || "fake: talk only".to_string(),
        default: true,
    },
    RewindChoice {
        how: RewindHow::CodeAndConversation,
        label: FILES,
        text: || "fake: talk + files".to_string(),
        default: false,
    },
];

/// 认屏（这一家自己的样子）。
pub(crate) fn read(lines: &[String]) -> RewindScreen {
    let at = |title: &str| lines.iter().position(|l| l.trim() == title);
    if let Some(t) = at(LIST_TITLE) {
        let rows = lines[t + 1..]
            .iter()
            .filter_map(|l| {
                let l = l.trim();
                let (selected, rest) = if let Some(r) = l.strip_prefix("[x] ") {
                    (true, r)
                } else {
                    (false, l.strip_prefix("[ ] ")?)
                };
                Some(RewindRow {
                    text: rest.to_string(),
                    selected,
                    current: rest == NOW_ROW,
                })
            })
            .collect::<Vec<_>>();
        return if rows.is_empty() {
            RewindScreen::Unknown
        } else {
            RewindScreen::List { rows }
        };
    }
    if let Some(t) = at(CONFIRM_TITLE) {
        let items = lines[t + 1..]
            .iter()
            .filter_map(|l| {
                let l = l.trim();
                let (selected, rest) = if let Some(r) = l.strip_prefix("(*) ") {
                    (true, r)
                } else {
                    (false, l.strip_prefix("( ) ")?)
                };
                Some(RewindItem {
                    label: rest.to_string(),
                    selected,
                })
            })
            .collect::<Vec<_>>();
        return if items.is_empty() {
            RewindScreen::Unknown
        } else {
            RewindScreen::Confirm { items }
        };
    }
    if lines.iter().any(|l| l.trim() == BUSY) {
        return RewindScreen::Busy;
    }
    match lines
        .iter()
        .rev()
        .find_map(|l| l.strip_prefix(PROMPT.trim_end()))
    {
        Some(rest) => RewindScreen::Prompt {
            typed: rest.trim().to_string(),
        },
        None => RewindScreen::Unknown,
    }
}

/// 此刻在哪一屏。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Prompt,
    List { sel: usize },
    Confirm { line: usize, sel: usize },
}

/// 主线上的一句：id · 字 · 这一句之后改没改过文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Line {
    pub(crate) uuid: String,
    pub(crate) text: String,
    pub(crate) files: bool,
}

/// 假 agent 的一个终端：收键改状态、收字记进主线。
#[derive(Debug, Clone)]
pub(crate) struct FakeTerm {
    /// 主线上人说过的那几句。
    pub(crate) said: Vec<Line>,
    /// 输入行里的字。
    pub(crate) typed: String,
    /// 主 agent 在出字；打断时 `interrupted` 那一句放回输入行。
    pub(crate) busy: bool,
    pub(crate) interrupted: Option<String>,
    /// 回退列表的标题换了样子（那一家升级了）。
    pub(crate) new_look: bool,
    /// 调出菜单那两下被并成一下（tmux escape-time）。
    pub(crate) merge_open: bool,
    /// 退完之后把原句填回输入行。
    pub(crate) refill: bool,
    /// 收到的每一个键（照先后）。
    pub(crate) keys: Vec<String>,
    /// 送进来的每一句。
    pub(crate) typed_in: Vec<String>,
    /// 退过几回、每回退的是哪一种。
    pub(crate) restored: Vec<&'static str>,
    mode: Mode,
    esc_once: bool,
    next: usize,
}

impl FakeTerm {
    /// 主线上已说过 `lines` 那几句（id 依次为 `u1`、`u2` …）。
    pub(crate) fn with(lines: &[&str]) -> Self {
        let said = lines
            .iter()
            .enumerate()
            .map(|(i, t)| Line {
                uuid: format!("u{}", i + 1),
                text: (*t).to_string(),
                files: false,
            })
            .collect();
        Self {
            said,
            typed: String::new(),
            busy: false,
            interrupted: None,
            new_look: false,
            merge_open: false,
            refill: false,
            keys: Vec::new(),
            typed_in: Vec::new(),
            restored: Vec::new(),
            mode: Mode::Prompt,
            esc_once: false,
            next: lines.len(),
        }
    }

    fn row(text: &str) -> String {
        if text.chars().count() > ROW_CHARS {
            format!("{}…", text.chars().take(ROW_CHARS).collect::<String>())
        } else {
            text.to_string()
        }
    }

    fn items(&self, line: usize) -> Vec<&'static str> {
        let files = self.said.get(line).is_some_and(|l| l.files);
        let mut v = vec![TALK];
        if files {
            v.push(FILES);
        }
        v.push(NEVER);
        v
    }

    /// 说出一句（主线上接一句）。
    fn submit(&mut self, text: String) {
        self.next += 1;
        self.said.push(Line {
            uuid: format!("u{}", self.next),
            text,
            files: false,
        });
    }

    /// 此刻那一屏。
    pub(crate) fn drawn(&self) -> Vec<String> {
        let mut out = vec!["fake agent v0".to_string()];
        match self.mode {
            Mode::Prompt => {
                if self.busy {
                    out.push(BUSY.to_string());
                }
                out.push(format!("{PROMPT}{}", self.typed));
            }
            Mode::List { sel } => {
                out.push(
                    if self.new_look {
                        "<< rewind v2 >>"
                    } else {
                        LIST_TITLE
                    }
                    .to_string(),
                );
                let rows = self
                    .said
                    .iter()
                    .map(|l| Self::row(&l.text))
                    .chain([NOW_ROW.to_string()]);
                for (i, r) in rows.enumerate() {
                    out.push(format!(" {} {r}", if i == sel { "[x]" } else { "[ ]" }));
                }
            }
            Mode::Confirm { line, sel } => {
                out.push(CONFIRM_TITLE.to_string());
                for (i, it) in self.items(line).into_iter().enumerate() {
                    out.push(format!(" {} {it}", if i == sel { "(*)" } else { "( )" }));
                }
            }
        }
        out
    }

    fn key(&mut self, key: &str) {
        self.keys.push(key.to_string());
        match self.mode {
            Mode::Prompt if self.busy => {
                if key == "ctrl-c" {
                    self.busy = false;
                    self.typed = self.interrupted.take().unwrap_or_default();
                }
            }
            Mode::Prompt => match key {
                "esc" if self.typed.is_empty() => {
                    if self.esc_once {
                        self.esc_once = false;
                        if self.merge_open {
                            self.merge_open = false;
                        } else {
                            self.mode = Mode::List {
                                sel: self.said.len(),
                            };
                        }
                    } else {
                        self.esc_once = true;
                    }
                }
                "backspace" => {
                    self.typed.pop();
                }
                // 输入历史：调出上一句。
                "up" => self.typed = self.said.last().map(|l| l.text.clone()).unwrap_or_default(),
                "enter" if !self.typed.is_empty() => {
                    let t = std::mem::take(&mut self.typed);
                    self.submit(t);
                }
                _ => {}
            },
            Mode::List { sel } => match key {
                "up" => {
                    self.mode = Mode::List {
                        sel: sel.saturating_sub(1),
                    }
                }
                "down" => {
                    self.mode = Mode::List {
                        sel: (sel + 1).min(self.said.len()),
                    }
                }
                "enter" if sel < self.said.len() => self.mode = Mode::Confirm { line: sel, sel: 0 },
                "enter" | "left" => self.mode = Mode::Prompt,
                _ => {}
            },
            Mode::Confirm { line, sel } => {
                let items = self.items(line);
                match key {
                    "up" => {
                        self.mode = Mode::Confirm {
                            line,
                            sel: sel.saturating_sub(1),
                        }
                    }
                    "down" => {
                        self.mode = Mode::Confirm {
                            line,
                            sel: (sel + 1).min(items.len() - 1),
                        }
                    }
                    "left" => self.mode = Mode::List { sel: line },
                    "enter" => {
                        let it = items[sel];
                        self.mode = Mode::Prompt;
                        if it != NEVER {
                            let original = self.said[line].text.clone();
                            self.said.truncate(line);
                            self.restored.push(it);
                            if self.refill {
                                self.typed = original;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if key != "esc" {
            self.esc_once = false;
        }
    }
}

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
    fn screen(&mut self) -> Result<Vec<String>, CmdErr> {
        Ok(self.drawn())
    }
    fn press(&mut self, key: &str) -> Result<Sent, CmdErr> {
        self.key(key);
        Ok(delivered())
    }
    fn redrawn(&mut self) -> bool {
        false
    }
    fn pause(&mut self, _ms: u64) {}
    fn say(&mut self, text: &str) -> Result<Sent, CmdErr> {
        self.typed_in.push(text.to_string());
        if self.mode == Mode::Prompt && !self.busy {
            let t = format!("{}{text}", std::mem::take(&mut self.typed));
            self.submit(t);
        }
        Ok(delivered())
    }
    fn lines(&mut self) -> Result<Vec<UserLine>, CmdErr> {
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

/// 假终端画出来的每一屏，它自己的认屏都认得出（判据拿它当 Claude 的替身之前先自检）。
#[test]
fn the_fake_reads_back_every_screen_it_draws() {
    let mut t = FakeTerm::with(&["一", "二"]);
    assert_eq!(
        read(&t.drawn()),
        RewindScreen::Prompt {
            typed: String::new()
        }
    );
    t.busy = true;
    assert_eq!(read(&t.drawn()), RewindScreen::Busy);
    t.busy = false;
    t.key("esc");
    t.key("esc");
    assert!(
        matches!(read(&t.drawn()), RewindScreen::List { rows } if rows.len() == 3 && rows[2].current && rows[2].selected)
    );
    t.key("up");
    t.key("enter");
    assert!(
        matches!(read(&t.drawn()), RewindScreen::Confirm { items } if items[0].label == TALK && items[0].selected)
    );
    t.new_look = true;
    t.key("left");
    assert_eq!(
        read(&t.drawn()),
        RewindScreen::Unknown,
        "换了样子的列表认不出"
    );
}
