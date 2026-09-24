//! `filewin/select.rs` 的纯判据：选中态 · 键位翻译 · 打字跳转 · 「能做什么」那张表。
//!
//! ⚠ 这里只判**零件**。接到窗口上的那几跳（真喂事件跑 `frame_body`）住 `shell_tests`
//! 末尾那一摞 —— 零件全绿而胶水断了的那一形，由那一摞看着。
//!
//! 🔴 期望值**全部手写**（异源）：不拿被测函数自己算期望，
//! 否则「两侧同源恒真」（本仓死值验五形之一）。

use super::*;
use crate::filewin::source::{Listed, Row};

fn row(name: &str, is_dir: bool, size: u64, lossy: bool) -> Listed {
    Listed::plain(Row {
        name: name.to_string(),
        path: format!("/srv/sel/{name}"),
        is_dir,
        size,
        lossy_name: lossy,
    })
}

/// 五行：`a.txt · B.log · c.bin · dir · zeta.txt`（大小写刻意混着，给打字跳转那一格用）。
fn five() -> Vec<Listed> {
    vec![
        row("a.txt", false, 3, false),
        row("B.log", false, 3, false),
        row("c.bin", false, 3, false),
        row("dir", true, 0, false),
        row("zeta.txt", false, 3, false),
    ]
}

fn set(names: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = names.iter().map(|s| s.to_string()).collect();
    v.sort();
    v
}

const CTRL: egui::Modifiers = egui::Modifiers::COMMAND;
const SHIFT: egui::Modifiers = egui::Modifiers::SHIFT;
const NONE: egui::Modifiers = egui::Modifiers::NONE;

// ════════════════════════════════════════════════════════════════════════
// 选中态
// ════════════════════════════════════════════════════════════════════════

/// 单击 · Ctrl+单击 · Shift+单击 三种手感，每一步都用**集合相等**判（两向）。
#[test]
fn plain_ctrl_and_shift_clicks_pick_exactly_what_a_file_manager_picks() {
    let rows = five();
    let mut s = Selection::default();
    assert!(s.is_empty());

    assert!(s.click(&rows, 1, NONE));
    assert_eq!(s.names(), set(&["B.log"]));
    assert_eq!(s.cursor(), Some("B.log"));

    // Ctrl 加一行、别的不动。
    assert!(s.click(&rows, 3, CTRL));
    assert_eq!(s.names(), set(&["B.log", "dir"]));
    // Ctrl 再点一次 ⇒ 取消那一行，光标**留在**那一行（头注：光标与选中是两件事）。
    assert!(s.click(&rows, 3, CTRL));
    assert_eq!(s.names(), set(&["B.log"]));
    assert_eq!(s.cursor(), Some("dir"));

    // Shift：从锚（上一次非 Shift 那一下 = 第 3 行）到这一行整段、替换原选中。
    assert!(s.click(&rows, 0, SHIFT));
    assert_eq!(s.names(), set(&["a.txt", "B.log", "c.bin", "dir"]));
    // 锚不动 ⇒ 再 Shift 到另一头，整段换成另一段（不是累加）。
    assert!(s.click(&rows, 4, SHIFT));
    assert_eq!(s.names(), set(&["dir", "zeta.txt"]));

    // 单击 ⇒ 回到只选一行。
    assert!(s.click(&rows, 2, NONE));
    assert_eq!(s.names(), set(&["c.bin"]));
    // 同一行再单击一次 ⇒ 没变。
    assert!(!s.click(&rows, 2, NONE), "什么都没变却说变了");
    // 越界 ⇒ 不接。
    assert!(!s.click(&rows, 99, NONE));
    assert_eq!(s.names(), set(&["c.bin"]));
}

/// 🔴 选中按**名字**记：换了排序之后，选中的还是**那几个文件**，不是那几个下标。
#[test]
fn a_pick_follows_the_file_not_the_index_when_the_rows_are_reordered() {
    let rows = five();
    let mut s = Selection::default();
    s.click(&rows, 0, NONE);
    s.click(&rows, 2, CTRL);
    let mut reordered = rows.clone();
    reordered.reverse();
    let got: Vec<String> = s
        .picked_indices(&reordered)
        .into_iter()
        .map(|i| reordered[i].name.clone())
        .collect();
    // 按**新**列表顺序、恰好这两个名字。
    assert_eq!(got, vec!["c.bin".to_string(), "a.txt".to_string()]);
    // 反空真：同一份选中在旧列表上给的下标不一样 ⇒ 上面那一比买的真是「按名字」。
    assert_eq!(s.picked_indices(&rows), vec![0, 2]);
}

/// Ctrl+A：全选（两向相等），光标落在第一行（原先没有光标时）。
#[test]
fn select_all_picks_every_row_and_nothing_else() {
    let rows = five();
    let mut s = Selection::default();
    s.select_all(&rows);
    assert_eq!(
        s.names(),
        set(&["a.txt", "B.log", "c.bin", "dir", "zeta.txt"])
    );
    assert_eq!(s.cursor(), Some("a.txt"));
    // 空列表上全选 ⇒ 空，而不是留着上一摞的名字。
    s.select_all(&[]);
    assert!(s.is_empty());
}

/// 右键点一行：**不在选中里 ⇒ 改成只选它；在 ⇒ 整摞不动。**
#[test]
fn a_right_click_keeps_the_pick_it_lands_in_and_replaces_one_it_misses() {
    let rows = five();
    let mut s = Selection::default();
    s.click(&rows, 0, NONE);
    s.click(&rows, 1, CTRL);
    s.pick_for_menu(&rows, 1);
    assert_eq!(
        s.names(),
        set(&["a.txt", "B.log"]),
        "点在选中里，整摞却变了"
    );
    s.pick_for_menu(&rows, 4);
    assert_eq!(s.names(), set(&["zeta.txt"]), "点在选中外，没换成只选它");
}

/// 方向键落点：没有光标从哪头进 · 到头停住不绕回 · 空列表不动。
#[test]
fn stepping_stops_at_the_ends_and_enters_from_the_right_side() {
    assert_eq!(step_target(0, None, 1), None);
    assert_eq!(step_target(5, None, 1), Some(0));
    assert_eq!(step_target(5, None, -1), Some(4));
    assert_eq!(step_target(5, Some(2), 1), Some(3));
    assert_eq!(step_target(5, Some(2), -1), Some(1));
    assert_eq!(step_target(5, Some(4), 1), Some(4), "到底了却绕回去了");
    assert_eq!(step_target(5, Some(0), -1), Some(0), "到顶了却绕回去了");
    // Home / End 走的就是「挪一个很大的步」。
    assert_eq!(step_target(5, Some(2), isize::MIN / 2), Some(0));
    assert_eq!(step_target(5, Some(2), isize::MAX / 2), Some(4));
}

/// Shift+方向键：从锚扩到光标（两向相等）。
#[test]
fn shift_stepping_extends_from_the_anchor() {
    let rows = five();
    let mut s = Selection::default();
    assert!(s.move_to(&rows, 1, false));
    assert!(s.move_to(&rows, 2, true));
    assert!(s.move_to(&rows, 3, true));
    assert_eq!(s.names(), set(&["B.log", "c.bin", "dir"]));
    assert_eq!(s.cursor(), Some("dir"));
    // 往回扩过锚 ⇒ 另一侧那一段。
    assert!(s.move_to(&rows, 0, true));
    assert_eq!(s.names(), set(&["a.txt", "B.log"]));
    // 不按 Shift ⇒ 只剩光标那一行。
    assert!(s.move_to(&rows, 4, false));
    assert_eq!(s.names(), set(&["zeta.txt"]));
    assert!(!s.move_to(&rows, 9, false), "越界却说挪到了");
}

/// 滚不滚、滚到哪：在视野里 ⇒ **不动**；上面看不见 ⇒ 顶到第一行；下面看不见 ⇒ 倒数第二行。
#[test]
fn scrolling_only_happens_when_the_cursor_left_the_view() {
    let pitch = 21.0;
    // 上一帧物化 [10, 30)。
    assert_eq!(scroll_for(15, 10, 30, pitch), None, "在视野里也滚了");
    assert_eq!(scroll_for(10, 10, 30, pitch), None);
    assert_eq!(scroll_for(9, 10, 30, pitch), Some(9.0 * pitch));
    // 最后那一行（常常只露半截）与再往下 ⇒ 它落在倒数第二行：新首行 = i + 2 - 20。
    assert_eq!(scroll_for(29, 10, 30, pitch), Some(11.0 * pitch));
    assert_eq!(scroll_for(40, 10, 30, pitch), Some(22.0 * pitch));
    // 第一帧（还没物化过）⇒ 直接顶到那一行。
    assert_eq!(scroll_for(7, 0, 0, pitch), Some(7.0 * pitch));
}

// ════════════════════════════════════════════════════════════════════════
// 键位
// ════════════════════════════════════════════════════════════════════════

fn key(k: egui::Key, m: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: m,
    }
}

/// 🔴 **键位表的每一行都有一格活的** —— 这一行的键喂进 `intents` 真出这一行的意图。
///
/// 表本体住 `select::intents` 的头注（刻意不做成常量，理由住那儿）；这里的九格与那张表
/// 逐行对应，格数钉成恒等 9 —— 表里加一行而这里没加一格，靠复核者看到这个数。
#[test]
fn every_row_of_the_keymap_has_a_live_cell() {
    use egui::Key;
    let alt = egui::Modifiers::ALT;
    let cells: Vec<(&str, Vec<egui::Event>, Vec<Intent>)> = vec![
        (
            "↑ / ↓",
            vec![key(Key::ArrowUp, NONE), key(Key::ArrowDown, NONE)],
            vec![
                Intent::Step {
                    by: -1,
                    extend: false,
                },
                Intent::Step {
                    by: 1,
                    extend: false,
                },
            ],
        ),
        (
            "Shift+↑ / Shift+↓",
            vec![key(Key::ArrowUp, SHIFT), key(Key::ArrowDown, SHIFT)],
            vec![
                Intent::Step {
                    by: -1,
                    extend: true,
                },
                Intent::Step {
                    by: 1,
                    extend: true,
                },
            ],
        ),
        (
            "Home / End",
            vec![key(Key::Home, NONE), key(Key::End, NONE)],
            vec![
                Intent::Edge {
                    end: false,
                    extend: false,
                },
                Intent::Edge {
                    end: true,
                    extend: false,
                },
            ],
        ),
        ("Alt+↑", vec![key(Key::ArrowUp, alt)], vec![Intent::Parent]),
        ("回车", vec![key(Key::Enter, NONE)], vec![Intent::Open]),
        ("Delete", vec![key(Key::Delete, NONE)], vec![Intent::Delete]),
        ("F2", vec![key(Key::F2, NONE)], vec![Intent::Rename]),
        ("Ctrl+A", vec![key(Key::A, CTRL)], vec![Intent::SelectAll]),
        (
            "直接打字",
            vec![egui::Event::Text("ze".into())],
            vec![Intent::Type("ze".into())],
        ),
    ];
    assert_eq!(cells.len(), 9, "键位表九行，这里的格数不等");
    // 反空真：头注里那张表真有这九行（按键名逐个在源码里找得到）。
    let src = include_str!("../../../src/bridge/src/filewin/select.rs");
    for (k, _, _) in &cells {
        assert!(
            src.contains(&format!("/// | {k} |")),
            "`intents` 头注那张表里没有「{k}」这一行"
        );
    }
    for (k, evs, want) in cells {
        assert_eq!(intents(&evs), want, "「{k}」那一格翻出来的不对");
    }
}

/// 反空真：不在表里的键、**松开**的键、不带 Ctrl 的 A 一个意图都不出。
#[test]
fn keys_outside_the_table_and_releases_translate_to_nothing() {
    use egui::Key;
    let released = egui::Event::Key {
        key: Key::Delete,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: NONE,
    };
    let evs = vec![
        released,
        key(Key::A, NONE),
        key(Key::F3, NONE),
        key(Key::Tab, NONE),
        key(Key::ArrowLeft, NONE),
        key(Key::ArrowDown, egui::Modifiers::ALT),
        egui::Event::Text(String::new()),
    ];
    assert_eq!(intents(&evs), Vec::<Intent>::new());
    // 正控：同一串里插一个表内的键 ⇒ 恰好出它一个。
    let mut with = evs.clone();
    with.push(key(Key::F2, NONE));
    assert_eq!(intents(&with), vec![Intent::Rename]);
}

// ════════════════════════════════════════════════════════════════════════
// 打字跳转
// ════════════════════════════════════════════════════════════════════════

/// 攒字：一秒内接着攒 · 隔久了重来。**时钟是喂进去的**（egui 输入时钟），不是一个定时器。
#[test]
fn typed_letters_accumulate_until_the_user_pauses() {
    let mut t = TypeAhead::default();
    assert_eq!(t.feed(10.0, "z"), "z");
    assert_eq!(t.feed(10.4, "e"), "ze");
    assert_eq!(
        t.feed(10.4 + TYPE_AHEAD_RESET_SECS, "t"),
        "zet",
        "刚好一秒不该重来"
    );
    assert_eq!(
        t.feed(10.4 + 2.0 * TYPE_AHEAD_RESET_SECS + 0.01, "b"),
        "b",
        "停了超过一秒却还接着攒"
    );
}

/// 跳到名字以这几个字开头的**第一行**，不分大小写；没有就是 `None`。
#[test]
fn jumping_finds_the_first_name_with_that_prefix_ignoring_case() {
    let rows = five();
    assert_eq!(jump_target(&rows, "b"), Some(1), "小写 b 没找到 B.log");
    assert_eq!(jump_target(&rows, "ZE"), Some(4));
    assert_eq!(jump_target(&rows, "zeta.txt"), Some(4));
    assert_eq!(jump_target(&rows, "zeta.txtx"), None);
    assert_eq!(jump_target(&rows, "q"), None);
    assert_eq!(jump_target(&rows, ""), None, "空前缀跳到了第一行");
    // 多字节：中文名照样认。
    let cn = vec![
        row("甲.txt", false, 1, false),
        row("乙.txt", false, 1, false),
    ];
    assert_eq!(jump_target(&cn, "乙"), Some(1));
}

// ════════════════════════════════════════════════════════════════════════
// 能做什么
// ════════════════════════════════════════════════════════════════════════

/// 🔴 **「能做什么」逐格手写**（异源）：单选的几种行 · 多选全可写 · 多选混着有损名 · 空。
#[test]
fn what_can_be_done_matches_a_hand_written_table() {
    use Action::*;
    let big = crate::sftp_pool::MAX_EDIT_BYTES as u64 + 1;
    let file = row("f.txt", false, 3, false);
    let dir = row("d", true, 0, false);
    let huge = row("h.bin", false, big, false);
    let lossy = row("\u{FFFD}x", false, 3, true);
    let lossy_dir = row("\u{FFFD}d", true, 0, true);
    let cases: Vec<(&str, Vec<&Listed>, Vec<Action>)> = vec![
        ("空", vec![], vec![]),
        (
            "一个普通文件",
            vec![&file],
            vec![Edit, Copy, Download, Rename, Chmod, Delete],
        ),
        ("一个目录", vec![&dir], vec![Open, Rename, Chmod, Delete]),
        (
            "一个超编辑上限的文件",
            vec![&huge],
            vec![Copy, Download, Rename, Chmod, Delete],
        ),
        ("一个有损名文件", vec![&lossy], vec![]),
        ("一个有损名目录", vec![&lossy_dir], vec![Open]),
        ("两项全可写", vec![&file, &dir], vec![Delete]),
        ("三项全可写", vec![&file, &dir, &huge], vec![Delete]),
        ("两项混着有损名", vec![&file, &lossy], vec![]),
    ];
    for (what, picked, want) in cases {
        assert_eq!(actions_for(&picked), want, "「{what}」那一格不对");
    }
}

/// 菜单上的字**复用**行上那几颗按钮的字（各自的唯一住址）；只有「删除」对多项另说。
#[test]
fn menu_labels_are_the_row_buttons_labels() {
    use Action::*;
    assert_eq!(Edit.label(1), crate::filewin::editor::EDIT_LABEL);
    assert_eq!(Copy.label(1), crate::filewin::copy::COPY_LABEL);
    assert_eq!(Download.label(1), crate::filewin::download::DOWNLOAD_LABEL);
    assert_eq!(Rename.label(1), crate::filewin::writeops::RENAME_LABEL);
    assert_eq!(Chmod.label(1), crate::filewin::writeops::CHMOD_LABEL);
    assert_eq!(Delete.label(1), crate::filewin::writeops::DELETE_LABEL);
    assert_eq!(Delete.label(3), "删除这 3 项");
    assert_eq!(Open.label(1), OPEN_LABEL);
}

/// 做不了的每一形都有一句话（非空、互不相同的几档各说各的）。
#[test]
fn every_refusal_says_something() {
    use Action::*;
    let said: Vec<String> = [(Delete, 0), (Delete, 2), (Open, 3), (Rename, 2), (Chmod, 2)]
        .into_iter()
        .map(|(a, n)| refusal(a, n))
        .collect();
    for s in &said {
        assert!(!s.trim().is_empty());
    }
    let uniq: std::collections::BTreeSet<&String> = said.iter().collect();
    assert_eq!(uniq.len(), said.len(), "几种做不了说的是同一句话：{said:?}");
}
