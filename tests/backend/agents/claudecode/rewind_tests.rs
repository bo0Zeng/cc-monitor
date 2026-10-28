//! Claude 回退菜单的认屏：照 10-27 实测（v2.1.312）的样子合成那几屏（不是真屏抓的原样，字照实测记下的写）。

use super::*;

fn lines(s: &str) -> Vec<String> {
    s.lines().map(str::to_string).collect()
}

const PROMPT_EMPTY: &str = "
 ✻ Welcome

────────────────────────────────────────────────
> Try \"refactor foo.rs\"
────────────────────────────────────────────────
  ? for shortcuts
";

const PROMPT_TYPED: &str = "
────────────────────────────────────────────────
> 半句话
────────────────────────────────────────────────
";

const BUSY: &str = "
● Reading files…
✶ Thinking… (12s · esc to interrupt)
────────────────────────────────────────────────
>
────────────────────────────────────────────────
";

const BACKGROUND_ONLY: &str = "
────────────────────────────────────────────────
>
────────────────────────────────────────────────
  1 shell still running
";

const LIST: &str = "
╭──────────────────────────────────────────────╮
│ Rewind                                       │
│                                              │
│   第一句                                     │
│     No code changes                          │
│ ❯ 第二句                                     │
│     src/a.rs +3 -1                           │
│   (current)                                  │
╰──────────────────────────────────────────────╯
";

const CONFIRM: &str = "
 Rewind

 This message and everything after it will be removed. The code will be unchanged.

 ❯ 1. Restore conversation
   2. Summarize from here
   3. Summarize up to here
   4. Never mind
";

#[test]
fn the_empty_prompt_counts_the_placeholder_as_empty() {
    assert_eq!(
        read(&lines(PROMPT_EMPTY)),
        RewindScreen::Prompt {
            typed: String::new()
        }
    );
}

#[test]
fn typed_words_on_the_prompt_are_read_back() {
    assert_eq!(
        read(&lines(PROMPT_TYPED)),
        RewindScreen::Prompt {
            typed: "半句话".into()
        }
    );
}

#[test]
fn esc_to_interrupt_is_busy_and_a_background_shell_alone_is_not() {
    assert_eq!(read(&lines(BUSY)), RewindScreen::Busy);
    assert_eq!(
        read(&lines(BACKGROUND_ONLY)),
        RewindScreen::Prompt {
            typed: String::new()
        }
    );
}

#[test]
fn the_list_has_one_row_per_said_line_and_the_current_row_last() {
    let RewindScreen::List { rows } = read(&lines(LIST)) else {
        panic!("该认成列表：{:?}", read(&lines(LIST)));
    };
    let got: Vec<(&str, bool, bool)> = rows
        .iter()
        .map(|r| (r.text.as_str(), r.selected, r.current))
        .collect();
    assert_eq!(
        got,
        [
            ("第一句", false, false),
            ("第二句", true, false),
            ("(current)", false, true)
        ],
        "下面更深一层的改动说明不算一行"
    );
}

#[test]
fn the_confirm_page_lists_four_items_with_the_first_selected() {
    let RewindScreen::Confirm { items } = read(&lines(CONFIRM)) else {
        panic!("该认成确认页");
    };
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            RESTORE_CONVERSATION,
            "Summarize from here",
            "Summarize up to here",
            "Never mind"
        ]
    );
    assert!(items[0].selected && items.iter().filter(|i| i.selected).count() == 1);
    assert!(
        FACE.choices.iter().any(|c| c.label == items[0].label),
        "缺省那一项在确认页上认得出"
    );
}

/// 标题换了 · 少了选中标 · 少了那句说明 ⇒ 认不出（一个键不按）。
#[test]
fn screens_that_look_different_are_unknown() {
    assert_eq!(
        read(&lines(&LIST.replace("Rewind", "Restore"))),
        RewindScreen::Unknown
    );
    assert_eq!(read(&lines(&LIST.replace('❯', " "))), RewindScreen::Unknown);
    assert_eq!(
        read(&lines(&CONFIRM.replace("will be removed", "goes away"))),
        RewindScreen::Unknown
    );
    assert_eq!(read(&lines("随便一屏\n没有输入行")), RewindScreen::Unknown);
}
