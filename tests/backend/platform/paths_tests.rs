//! 路径归属的比较（`is_under`）：两边同一种规范化再比。折大小写那一臂（Windows）在这里直接喂 `fold = true` 测，任何平台都跑得到。

use super::{key_as, under_as};
use std::path::Path;

/// Windows 上记录游标按折过的键存，根是原样大小写：拿键认根得对得上（之前的行补不出来就是这里对不上）。
#[test]
fn a_folded_key_is_under_its_root_written_in_original_case() {
    let root = Path::new("/Users/user/.claude/projects");
    let raw = Path::new("/Users/user/.claude/projects/-Work/0a1b.jsonl");
    let key = key_as(raw, true);
    assert_eq!(
        key,
        Path::new("/users/user/.claude/projects/-work/0a1b.jsonl")
    );
    assert!(under_as(&key, root, true), "折过的键认不出原样的根");
    assert!(under_as(raw, root, true), "原样路径认不出原样的根");
    assert!(
        under_as(raw, &key_as(root, true), true),
        "原样路径认不出折过的根"
    );
}

/// 按段比、不按字符前缀；不折的那一臂（unix）大小写不同就是不同的路径。
#[test]
fn being_under_is_by_segment_and_folds_only_where_the_filesystem_does() {
    let root = Path::new("/Users/user/.claude/projects");
    assert!(!under_as(
        Path::new("/Users/user/.claude/projects2/a.jsonl"),
        root,
        true
    ));
    assert!(!under_as(
        Path::new("/Users/u/.claude/projects/a.jsonl"),
        root,
        true
    ));
    assert!(!under_as(
        Path::new("/users/user/.claude/projects/a.jsonl"),
        root,
        false
    ));
    assert_eq!(
        key_as(Path::new("/Users/user"), false),
        Path::new("/Users/user")
    );
}

/// Windows 上 `canonicalize` 交回扩展长度那一形（`\\?\C:\…` · `\\?\UNC\…`），记录路径是平常那一形：
/// 规范化之后是同一处（`agents::record_kind_among` 拿解开链接的根去认记录归哪一家就靠它）。
#[test]
fn an_extended_length_path_is_the_same_place_as_its_plain_form() {
    assert_eq!(
        key_as(Path::new(r"\\?\C:\Users\U\.codex\sessions"), true).to_string_lossy(),
        r"c:\users\u\.codex\sessions"
    );
    assert_eq!(
        key_as(Path::new(r"\\?\UNC\Srv\Share\x"), true).to_string_lossy(),
        r"\\srv\share\x"
    );
    // 设备路径（`\\.\`）不是同一处的另一种写法，不动。
    assert_eq!(
        key_as(Path::new(r"\\.\PIPE\x"), true).to_string_lossy(),
        r"\\.\pipe\x"
    );
    // 按段比的那一半在这里只认 `/`：同一形写成 `/` 分隔再比一次。
    let root = Path::new("//?/C:/Users/u/.codex/sessions");
    let rec = Path::new("C:/Users/u/.codex/sessions/2026/10/r.jsonl");
    assert!(
        under_as(rec, root, true),
        "扩展长度的根认不出平常那一形的记录"
    );
    assert!(
        !under_as(rec, root, false),
        "不折的那一臂（unix）不该认 `\\\\?\\`"
    );
}

use super::{expand_tilde_from, home_join};

/// 家写得成字时两形同一份：比字面期望照文字那一形比。
fn text(p: &str, home: Option<&std::path::Path>) -> String {
    super::expand_tilde_text(p, home).expect("写得成字")
}

/// 打头的 `~` 按家目录展开（`~/` 与 Windows 的 `~\` 都认），分隔符照家目录自己的写法；别的形原样（`~别人/…` · 绝对 · 相对）；
/// 家目录说不出 ⇒ 原样。期望手写。
#[test]
fn a_leading_tilde_expands_under_the_home_in_its_own_separator() {
    let unix = Some(std::path::Path::new("/home/user"));
    let win = Some(std::path::Path::new(r"C:\Users\user"));
    assert_eq!(
        text("~/.ssh/id_ed25519", unix),
        "/home/user/.ssh/id_ed25519"
    );
    assert_eq!(text("~", unix), "/home/user");
    assert_eq!(
        text(r"~\.ssh\id_ed25519", win),
        r"C:\Users\user\.ssh\id_ed25519"
    );
    assert_eq!(
        text("~/.ssh/id_ed25519", win),
        r"C:\Users\user\.ssh\id_ed25519"
    );
    assert_eq!(text("~bob/key", unix), "~bob/key");
    assert_eq!(text("/etc/ssh/key", unix), "/etc/ssh/key");
    assert_eq!(text("keys/a", unix), "keys/a");
    assert_eq!(text("~/.ssh/id", None), "~/.ssh/id");
}

/// 家目录底下一段 `/` 写的相对路径（契约 crate 里那些 `.cc-monitor/…`）⇒ 分隔符照家目录自己的写法拼，Windows 上不混 `/`。
#[test]
fn a_path_under_the_home_uses_the_homes_own_separator() {
    assert_eq!(
        home_join(
            std::path::Path::new(r"C:\Users\user"),
            ".cc-monitor/accounts"
        ),
        std::path::PathBuf::from(r"C:\Users\user\.cc-monitor\accounts")
    );
    assert_eq!(
        home_join(std::path::Path::new("/home/user"), ".cc-monitor/accounts"),
        std::path::PathBuf::from("/home/user/.cc-monitor/accounts")
    );
}

/// 🔴 家说不出只判这一处：家没有（`None`）与家是空串两形都拒、拒的是同一句，不拿根目录兜底拼成 `/<rel>`。正控：说得出的家照拼。
#[test]
fn an_unsaid_home_is_refused_alike_whether_none_or_empty() {
    use super::under_home;
    let rel = ".cc-monitor/accounts";
    let none = under_home(None, rel).expect_err("没有家 ⇒ 拒");
    let empty = under_home(Some(std::path::Path::new("")), rel).expect_err("空串家 ⇒ 拒");
    assert_eq!(none, empty, "两形拒的不是同一句");
    assert_eq!(
        none,
        copy_core::copy_text("bePaths.underHome.noHome", &[("rel", rel)])
    );
    assert_eq!(
        under_home(Some(std::path::Path::new("/home/user")), rel),
        Ok(std::path::PathBuf::from("/home/user/.cc-monitor/accounts"))
    );
}

/// 裸 `~` 展开成家本身（家是 `/` 也是 `/`，不剥成空串）；家说不出（`None` · 空串）⇒ 原样。期望手写。
#[test]
fn a_bare_tilde_is_the_home_itself_and_an_unsaid_home_leaves_it_alone() {
    assert_eq!(text("~", Some(std::path::Path::new("/"))), "/");
    assert_eq!(text("~", Some(std::path::Path::new(r"C:\"))), r"C:\");
    assert_eq!(text("~/x", Some(std::path::Path::new(""))), "~/x");
    assert_eq!(text("~", Some(std::path::Path::new(""))), "~");
}

/// 家名里有非 UTF-8 的字节（替身家）：读盘那一形照家原样拼（字节不换成替换字符）；要写成字的那一形照实拒，
/// 一句说清是哪个路径；不碰家的那几形两形都原样。
#[cfg(unix)]
#[test]
fn a_non_utf8_home_expands_as_it_is_and_is_refused_where_text_is_needed() {
    use super::expand_tilde_text;
    use std::os::unix::ffi::OsStrExt as _;
    let home = std::path::Path::new(std::ffi::OsStr::from_bytes(b"/h\xffome"));
    assert_eq!(
        expand_tilde_from("~/.ssh/id", Some(home)),
        home.join(".ssh").join("id")
    );
    assert_eq!(expand_tilde_from("~", Some(home)), home);
    let said = expand_tilde_text("~/.ssh/id", Some(home)).expect_err("有损写成字了");
    assert_eq!(
        said,
        copy_core::copy_text("bePaths.expandTilde.notUtf8", &[("path", "~/.ssh/id")])
    );
    assert_eq!(
        expand_tilde_text("/etc/x", Some(home)).as_deref(),
        Ok("/etc/x")
    );
    assert_eq!(
        expand_tilde_text("~/x", Some(std::path::Path::new("/home/user"))).as_deref(),
        Ok("/home/user/x")
    );
}
