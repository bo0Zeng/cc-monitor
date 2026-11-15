//! `filewin/select.rs` 的纯判据：选中态 · 键位翻译 · 打字跳转 · 「能做什么」那张表。
//!
//! ⚠ 这里只判**零件**。接到窗口上的那几跳（真喂事件跑 `frame_body`）住 `shell_tests`
//! 末尾那一摞 —— 零件全绿而胶水断了的那一形，由那一摞看着。
//!
//! 🔴 期望值**全部手写**（异源）：不拿被测函数自己算期望，
//! 否则「两侧同源恒真」（本仓死值验五形之一）。

use super::*;
use crate::source::{Listed, Row};

/// 判据读选中态用的那一件（生产那一份里没有它；本模块挂在 `select` 底下，读得到私有字段）。
impl Selection {
    /// 光标那一行的名字（`None` = 还没有光标）。
    pub(crate) fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }
}

fn row(name: &str, is_dir: bool, size: u64, lossy: bool) -> Listed {
    crate::find::testing::gated(Listed::plain(Row {
        name: name.to_string(),
        path: format!("/srv/sel/{name}"),
        is_dir,
        size,
        lossy_name: lossy,
    }))
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
    // Home / End 另有一个落点函数，**没有光标时 End 也落最后一行**
    // （借「挪一个很大的步」的话，没光标时 End 会落在第一行）。
    assert_eq!(edge_target(0, true), None);
    assert_eq!(edge_target(5, false), Some(0));
    assert_eq!(edge_target(5, true), Some(4));
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
        (
            "Backspace",
            vec![key(Key::Backspace, NONE)],
            vec![Intent::Parent],
        ),
        // 空格：「看一眼」；跟着来的那个 `Text(" ")` 不进打字跳转。
        (
            "空格",
            vec![key(Key::Space, NONE), egui::Event::Text(" ".into())],
            vec![Intent::Peek],
        ),
        (
            "Ctrl+Shift+N",
            vec![key(Key::N, CTRL | SHIFT)],
            vec![Intent::NewFolder],
        ),
        ("Ctrl+D", vec![key(Key::D, CTRL)], vec![Intent::Download]),
        ("Ctrl+U", vec![key(Key::U, CTRL)], vec![Intent::Upload]),
        (
            "Ctrl+Shift+C",
            vec![key(Key::C, CTRL | SHIFT)],
            vec![Intent::CopyPath],
        ),
        ("Esc", vec![key(Key::Escape, NONE)], vec![Intent::Clear]),
    ];
    assert_eq!(cells.len(), 16, "键位表十六行，这里的格数不等");
    // 反空真：头注里那张表真有这十六行（按键名逐个在源码里找得到）。
    let src = include_str!("../../../src/frontend/filewin/src/select.rs");
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

// 这几条判的是窗口交出去的原行（`Listed::ls_row`）连着本机后端那张表（`backend_files::menu::desk_menu`，
// 真函数）一起答出来的：窗口把链接 · 断链 · 名字字节交错了，或者表里多 / 少一项，都红。
// 名字有损的行恒带原始字节（`files-ls` 的行就是字节）⇒ 「名字寻址不到」那几种窗口里交不出来，不在这里。
use crate::find::testing::{can_do, menu_for, menu_items};

/// 名字有损、照真行那样带着原始字节的一行。
fn lossy_row(name: &str, is_dir: bool) -> Listed {
    crate::find::testing::gated_raw(Listed::plain(Row {
        name: name.to_string(),
        path: format!("/srv/sel/{name}"),
        is_dir,
        size: if is_dir { 0 } else { 3 },
        lossy_name: true,
    }))
}

/// 🔴 **「能做什么」逐格手写**（异源）：单选的几种行 · 多选全可写 · 多选混着有损名 · 空。
///
/// 10-11 起照菜单那张表：算大小只给目录与多选 · 解压只给压缩包 · 查看给图片 · 编辑给文本；
/// 超编辑上限的那一份「编辑」在表里、但做不了（菜单上灰着说为什么），所以这里没有它。
#[test]
fn what_can_be_done_matches_a_hand_written_table() {
    use Action::*;
    let big = 8 * 1024 * 1024 + 1;
    let file = row("f.txt", false, 3, false);
    let dir = row("d", true, 0, false);
    let huge = row("h.bin", false, big, false);
    let png = row("logo.png", false, 3, false);
    let code = row("main.rs", false, 3, false);
    let tgz = row("release.tar.gz", false, 3, false);
    let pdf = row("report.pdf", false, 3, false);
    let broken = Listed {
        link: true,
        link_broken: true,
        ..row("gone", false, 0, false)
    };
    let lossy = lossy_row("\u{FFFD}x.txt", false);
    let lossy_dir = lossy_row("\u{FFFD}d", true);
    let text = vec![
        Edit, Download, CrossCopy, Copy, Rename, Chmod, Properties, Delete,
    ];
    let cases: Vec<(&str, Vec<&Listed>, Vec<Action>)> = vec![
        ("空", vec![], vec![]),
        ("一个普通文件", vec![&file], text.clone()),
        ("一份代码", vec![&code], text.clone()),
        (
            "一张图：查看顶替编辑，不算大小、不解压",
            vec![&png],
            vec![
                View, Download, CrossCopy, Copy, Rename, Chmod, Properties, Delete,
            ],
        ),
        (
            "一个压缩包：解压，不编辑",
            vec![&tgz],
            vec![
                Download, CrossCopy, Copy, Extract, Rename, Chmod, Properties, Delete,
            ],
        ),
        (
            "一份 PDF：不编辑、不查看",
            vec![&pdf],
            vec![Download, CrossCopy, Copy, Rename, Chmod, Properties, Delete],
        ),
        (
            "一个目录",
            vec![&dir],
            vec![Open, Copy, Size, Rename, Chmod, Properties, Delete],
        ),
        (
            "一个超编辑上限的文件",
            vec![&huge],
            vec![Download, CrossCopy, Copy, Rename, Chmod, Properties, Delete],
        ),
        (
            "一个断了的链接",
            vec![&broken],
            vec![Rename, Properties, Delete],
        ),
        // 带着字节 ⇒ 编辑 / 复制 / 下载也放开（线上走字节；下载是一组 `transfer-copy`，源照原始字节）。
        ("一个有损名文件", vec![&lossy], text.clone()),
        (
            "一个有损名目录",
            vec![&lossy_dir],
            vec![Open, Copy, Size, Rename, Chmod, Properties, Delete],
        ),
        // 有目录 ⇒ 下载灰着（下载只收文件）；跨机复制目录整棵逐个文件 ⇒ 给。
        (
            "两项全可写",
            vec![&file, &dir],
            vec![CrossCopy, Across, Size, Chmod, Delete],
        ),
        (
            "三项全可写",
            vec![&file, &dir, &huge],
            vec![CrossCopy, Across, Size, Chmod, Delete],
        ),
        // 名字只有原始字节的那一项 ⇒ 跨机灰着（那一条按路径串寻址）。
        (
            "两项混着有损名",
            vec![&file, &lossy],
            vec![Download, Across, Size, Chmod, Delete],
        ),
    ];
    for (what, picked, want) in cases {
        assert_eq!(can_do(&picked), want, "「{what}」那一格不对");
    }
}

/// 🔴 **菜单那张表逐种手写**（异源）：每一种选中给的恰好是这几组、这个顺序。
///
/// 行从真名字造（`logo.png` · `main.rs` · `release.tar.gz` …），交出去的原行先过后端「这是哪一种」那一跳，再查表 ——
/// 两跳一起判：分错了种，或者表里多 / 少一项，都红。
#[test]
fn each_kind_of_selection_gets_exactly_the_groups_in_the_table() {
    use Action::*;
    use Item::Act as I;
    let link = |name: &str, to_dir: bool, broken: bool| Listed {
        link: true,
        link_dir: to_dir,
        link_broken: broken,
        ..row(name, false, 4096, false)
    };
    let png = row("logo.png", false, 3, false);
    let svg = row("icon.svg", false, 3, false);
    let code = row("main.rs", false, 3, false);
    let env = row(".env", false, 3, false);
    let tgz = row("release.tar.gz", false, 3, false);
    let pdf = row("report.pdf", false, 3, false);
    let dir = row("docs", true, 0, false);
    let to_dir = link("deploy", true, false);
    let to_log = link("latest.log", false, false);
    let gone = link("old", false, true);
    let text_groups = vec![
        vec![I(Edit), I(Download), I(CrossCopy), I(Copy)],
        vec![I(Rename), I(Chmod), I(Properties)],
        vec![I(Delete)],
    ];
    let cases: Vec<(&str, Vec<&Listed>, Vec<Vec<Item>>)> = vec![
        (
            "空白处",
            vec![],
            vec![
                vec![Item::Mkdir, Item::NewFile, Item::Upload],
                vec![Item::Term],
            ],
        ),
        (
            "一个目录",
            vec![&dir],
            vec![
                vec![I(Open), I(Copy)],
                vec![I(Size), I(Rename), I(Chmod), I(Properties)],
                vec![I(Delete)],
            ],
        ),
        (
            "指向目录的链接",
            vec![&to_dir],
            vec![
                vec![I(Open), I(Copy)],
                vec![I(Rename), I(Chmod), I(Properties)],
                vec![I(Delete)],
            ],
        ),
        ("一份代码", vec![&code], text_groups.clone()),
        ("没有扩展名的", vec![&env], text_groups.clone()),
        ("指向文本的链接", vec![&to_log], text_groups.clone()),
        (
            "窗口解不开的图（svg）照旧当文本",
            vec![&svg],
            text_groups.clone(),
        ),
        (
            "一张图片",
            vec![&png],
            vec![
                vec![I(View), I(Download), I(CrossCopy), I(Copy)],
                vec![I(Rename), I(Chmod), I(Properties)],
                vec![I(Delete)],
            ],
        ),
        (
            "一个压缩包",
            vec![&tgz],
            vec![
                vec![I(Download), I(CrossCopy), I(Copy)],
                vec![I(Extract), I(Rename), I(Chmod), I(Properties)],
                vec![I(Delete)],
            ],
        ),
        (
            "PDF",
            vec![&pdf],
            vec![
                vec![I(Download), I(CrossCopy), I(Copy)],
                vec![I(Rename), I(Chmod), I(Properties)],
                vec![I(Delete)],
            ],
        ),
        (
            "断了的链接",
            vec![&gone],
            vec![vec![I(Rename), I(Properties)], vec![I(Delete)]],
        ),
        (
            "多选",
            vec![&dir, &code, &tgz],
            vec![
                vec![I(Download), I(CrossCopy), I(Across)],
                vec![I(Size), I(Chmod)],
                vec![I(Delete)],
            ],
        ),
    ];
    for (what, picked, want) in cases {
        assert_eq!(
            menu_items(&menu_for(&picked, None)),
            want,
            "「{what}」那一种的菜单不对"
        );
    }
    // 带计划：危险那一组之前多「在计划里看」一组。
    assert_eq!(
        menu_items(&menu_for(&[&code], Some("第一片"))),
        vec![
            vec![I(Edit), I(Download), I(CrossCopy), I(Copy)],
            vec![I(Rename), I(Chmod), I(Properties)],
            vec![Item::InPlan],
            vec![I(Delete)],
        ]
    );
}

/// 🔴 菜单右端写的快捷键**真是那一件的键**：每一个喂回键位翻译（[`intents`]），出的是做这一件的那个意图。
#[test]
fn every_shortcut_on_the_menu_is_the_key_that_does_that_item() {
    use Action::*;
    let want: Vec<(Item, Intent)> = vec![
        (Item::Act(Open), Intent::Open),
        (Item::Act(Edit), Intent::Open),
        (Item::Act(View), Intent::Open),
        (Item::Act(Download), Intent::Download),
        (Item::Act(Rename), Intent::Rename),
        (Item::Act(Delete), Intent::Delete),
        (Item::Mkdir, Intent::NewFolder),
        (Item::Upload, Intent::Upload),
    ];
    for (item, intent) in &want {
        let k = item
            .shortcut()
            .unwrap_or_else(|| panic!("{item:?} 在菜单上没写快捷键"));
        assert_eq!(
            intents(&[key(k.logical_key, k.modifiers)]),
            vec![intent.clone()],
            "{item:?} 写的键不是做它的那个键"
        );
    }
    // 反面：别的项不写键（写了却按不出来就是说假话）。
    for item in [
        Item::Act(Copy),
        Item::Act(Size),
        Item::Act(Extract),
        Item::Act(CrossCopy),
        Item::Act(Chmod),
        Item::Act(Properties),
        Item::InPlan,
        Item::NewFile,
        Item::Term,
    ] {
        assert_eq!(item.shortcut(), None, "{item:?} 写了一个按不出它的键");
    }
}

/// 命令栏 · 菜单上那几件的字（成品 `acts` 里那一行）**就是**行上那几颗按钮的字（各自的唯一住址）。
#[test]
fn menu_labels_are_the_row_buttons_labels() {
    use Action::*;
    let file = row("f.txt", false, 3, false);
    let m = menu_for(&[&file], None);
    let text = |a: Action| m.act(a).expect("十四件每件一行").text.clone();
    assert_eq!(text(Edit), crate::editor::EDIT_LABEL.as_str());
    assert_eq!(text(Copy), crate::copy::COPY_LABEL.as_str());
    assert_eq!(text(Download), crate::download::DOWNLOAD_LABEL.as_str());
    let many = [&file, &file, &file];
    let m = menu_for(&many, None);
    assert_eq!(
        m.act(Download).unwrap().text,
        copy_core::copy_text("rsFilewinSelect.label.downloadMany", &[("n", "3")])
    );
}

// ════════════════════════════════════════════════════════════════════════
// 有损名按原始字节记选中（头注 §四）
// ════════════════════════════════════════════════════════════════════════

/// 🔴 两个**不同字节**的有损名解成**同一个**显示串 —— 点一个，只选中那一个。
///
/// 按显示串记的话，点一下就选中两行，按 Delete 删掉两个文件（「选中态 == 批量那一摞」的反面）。
#[test]
fn two_lossy_names_that_look_the_same_are_still_two_picks() {
    let mk = |raw: &[u8]| Listed {
        raw_name: Some(raw.to_vec()),
        ..row("a\u{FFFD}", false, 1, true)
    };
    let rows = vec![mk(b"a\xff"), mk(b"a\xfe"), row("b.txt", false, 1, false)];
    assert_eq!(rows[0].name, rows[1].name, "前提：两行的显示串相同");
    let mut s = Selection::default();
    s.click(&rows, 0, egui::Modifiers::NONE);
    assert_eq!(
        s.picked_indices(&rows),
        vec![0],
        "🔴 点一个有损名，选中了两行"
    );
    s.click(&rows, 1, egui::Modifiers::COMMAND);
    assert_eq!(s.picked_indices(&rows), vec![0, 1]);
    s.click(&rows, 0, egui::Modifiers::COMMAND);
    assert_eq!(
        s.picked_indices(&rows),
        vec![1],
        "取消一个，另一个也跟着没了"
    );
    assert_eq!(s.cursor_index(&rows), Some(0), "光标认错了行");
    // 无损名的键就是名字本身（那一族判据按名字断言）。
    assert_eq!(pick_key(&rows[2]), "b.txt");
    // 有损而没有字节 ⇒ 退回显示串（它反正不许写，选中只用来看）。
    assert_eq!(pick_key(&row("x\u{FFFD}", false, 1, true)), "x\u{FFFD}");
}

/// 指向目录的链接：给「打开」，不给编辑 / 解压 / 跨机复制 / 下载（那几件只对文件）；指向文件的链接照旧给编辑。
#[test]
fn a_link_to_a_directory_opens_and_a_link_to_a_file_edits() {
    let link = |name: &str, to_dir: bool| Listed {
        link: true,
        link_dir: to_dir,
        ..row(name, false, 4096, false)
    };
    let to_dir = link("bin", true);
    let to_file = link("conf", false);
    let got = can_do(&[&to_dir]);
    assert!(
        got.contains(&Action::Open),
        "指向目录的链接没有「打开」：{got:?}"
    );
    for no in [
        Action::Edit,
        Action::Extract,
        Action::CrossCopy,
        Action::Download,
    ] {
        assert!(!got.contains(&no), "指向目录的链接给了 {no:?}：{got:?}");
    }
    let got = can_do(&[&to_file]);
    assert!(
        got.contains(&Action::Edit) && !got.contains(&Action::Open),
        "{got:?}"
    );
}

/// 多选那一种的菜单照表：头一组「下载 n 项 · 复制 n 项到另一台… · 复制 n 项到另一栏」，再「算大小 · 改权限」，危险那一组「删除」在最后。
#[test]
fn the_many_menu_offers_download_cross_and_across_by_the_table() {
    let rows = five();
    let picked: Vec<&Listed> = vec![&rows[0], &rows[1], &rows[2]];
    let m = menu_for(&picked, None);
    assert_eq!(
        menu_items(&m),
        vec![
            vec![
                Item::Act(Action::Download),
                Item::Act(Action::CrossCopy),
                Item::Act(Action::Across)
            ],
            vec![Item::Act(Action::Size), Item::Act(Action::Chmod)],
            vec![Item::Act(Action::Delete)],
        ]
    );
    let first: Vec<&str> = m.groups[0].iter().map(|e| e.text.as_str()).collect();
    assert_eq!(
        first,
        ["下载 3 项", "复制 3 项到另一台…", "复制 3 项到另一栏"]
    );
    let can = can_do(&picked);
    for a in [Action::Download, Action::CrossCopy, Action::Across] {
        assert!(can.contains(&a), "{a:?} 没给");
    }
}

/// 某一项不行 ⇒ 那一行灰着、悬停说哪几项为什么：目录不能下载（跨机复制目录可以，整棵逐个文件）；名字读不出的不能复制到另一台。
#[test]
fn a_many_item_that_cannot_go_is_greyed_naming_which_ones() {
    let rows = five();
    let why = |picked: &[&Listed], a: Action| {
        menu_for(picked, None)
            .groups
            .into_iter()
            .flatten()
            .find(|e| e.item == Item::Act(a))
            .unwrap_or_else(|| panic!("菜单上没有 {a:?}"))
            .why
    };
    let with_dir: Vec<&Listed> = vec![&rows[0], &rows[3]];
    assert_eq!(
        why(&with_dir, Action::Download),
        Some(copy_core::copy_text(
            "rsFilewinSelect.why.dirDownload",
            &[("names", "dir")]
        )),
        "有目录 ⇒ 下载灰着、说是哪一项"
    );
    assert_eq!(
        why(&with_dir, Action::CrossCopy),
        None,
        "目录能复制到另一台"
    );
    let lossy = lossy_row("q\u{FFFD}", false);
    let with_lossy: Vec<&Listed> = vec![&rows[0], &lossy];
    let said = why(&with_lossy, Action::CrossCopy).expect("名字读不出 ⇒ 跨机灰着");
    assert!(said.contains("q\u{FFFD}"), "{said}");
    assert_eq!(why(&with_lossy, Action::Download), None);
}

/// 框选：框到的那几行选中；按着 Ctrl ⇒ 加进按下之前的选中；框里一行都没有 ⇒ 不按 Ctrl 时清空。
#[test]
fn a_band_picks_the_rows_it_covers_and_ctrl_adds_to_what_was_there() {
    let rows = five();
    let mut s = Selection::default();
    s.click(&rows, 0, egui::Modifiers::NONE);
    let base = s.clone();
    s.band(&rows, &base, Some((2, 4)), false);
    assert_eq!(s.names(), set(&["c.bin", "dir", "zeta.txt"]));
    assert_eq!(s.cursor(), Some("zeta.txt"));
    s.band(&rows, &base, Some((2, 3)), true);
    assert_eq!(
        s.names(),
        set(&["a.txt", "c.bin", "dir"]),
        "Ctrl 框 ⇒ 加进已选"
    );
    s.band(&rows, &base, None, false);
    assert!(s.is_empty());
    // 锚在框的第一行：之后 Shift 点从那里扩。
    s.band(&rows, &base, Some((1, 2)), false);
    s.click(&rows, 4, SHIFT);
    assert_eq!(s.names(), set(&["B.log", "c.bin", "dir", "zeta.txt"]));
}
