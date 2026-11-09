//! `platform::proc` 挑程序那两样：打开（`opener_on`）· 在文件夹中显示（`revealer_on`）。
//! 三个系统的挑法在哪台上都量得到（挑法是按 [`Os`] 分的纯函数；`os_opener` · `os_revealer` 只是拿本机那一格调它）。

use super::*;

const URL_WITH_AMP: &str = "https://login.example/oauth?client_id=a&redirect_uri=b&state=c";

/// 安全：网址与路径原样作为**一个**实参交给程序，程序不是 shell。Windows 上经 `cmd /C start` 时
/// `&` 会被 cmd 当成命令分隔：登录网址被截断，`…&calc` 这一形还会多起一个程序。
#[test]
fn the_target_reaches_the_program_as_one_argument_and_no_shell_sits_between() {
    for os in [Os::Windows, Os::Mac, Os::Linux] {
        for target in [URL_WITH_AMP, r"C:\a&b\c d.txt", "/home/u/a&b/c d.txt"] {
            let (bin, args) = opener_on(os, target);
            assert!(
                !matches!(bin, "cmd" | "cmd.exe" | "sh" | "bash" | "powershell"),
                "{os:?} 打开 {target:?} 挑了 shell `{bin}`：& 会被拆开"
            );
            assert_eq!(
                args,
                vec![target.to_string()],
                "{os:?}：目标得原样、单独一个实参"
            );
        }
    }
}

#[test]
fn each_os_opens_with_its_own_program() {
    assert_eq!(opener_on(Os::Windows, "x").0, "explorer");
    assert_eq!(opener_on(Os::Mac, "x").0, "open");
    assert_eq!(opener_on(Os::Linux, "x").0, "xdg-open");
}

/// 在文件夹中显示：Windows 选中那一项 · mac 选中那一项 · Linux 这一挑是退路（会话总线答不上时）：开它所在的目录。
#[test]
fn reveal_selects_the_item_or_falls_back_to_its_folder() {
    assert_eq!(
        revealer_on(Os::Windows, r"C:\a&b\c d.txt"),
        ("explorer", vec![r"/select,C:\a&b\c d.txt".to_string()])
    );
    assert_eq!(
        revealer_on(Os::Mac, "/Users/u/a b.txt"),
        (
            "open",
            vec!["-R".to_string(), "/Users/u/a b.txt".to_string()]
        )
    );
    assert_eq!(
        revealer_on(Os::Linux, "/home/u/a&b/c d.txt"),
        ("xdg-open", vec!["/home/u/a&b".to_string()])
    );
}

/// 本机那一格就是这台编出来的系统。
#[test]
fn here_is_the_os_this_was_built_for() {
    let want = if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::Mac
    } else {
        Os::Linux
    };
    assert_eq!(Os::HERE, want);
    assert_eq!(os_opener("/x"), opener_on(want, "/x"));
    assert_eq!(os_revealer("/x/y"), revealer_on(want, "/x/y"));
}

/// 网址只认 http · https · mailto：别的 scheme 交给系统就是起它登记的程序。
#[test]
fn only_web_and_mail_urls_are_opened() {
    for ok in ["https://a.example/x?a=1&b=2", "HTTP://a", "mailto:a@b.c"] {
        assert!(openable_url(ok), "{ok}");
    }
    for no in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "ms-settings:",
        "/home/u/x",
        "",
        "http:",
    ] {
        assert!(!openable_url(no), "{no}");
    }
}

/// 会话总线那一问要的是 `file://` 网址：路径里的空格 · & · 非 ASCII 都转义成 %XX，`/` 留着。
#[test]
fn the_bus_gets_a_file_uri_with_bytes_escaped() {
    assert_eq!(
        file_uri("/home/u/a&b/c d.txt"),
        "file:///home/u/a%26b/c%20d.txt"
    );
    assert_eq!(file_uri("/家/x"), "file:///%E5%AE%B6/x");
}
