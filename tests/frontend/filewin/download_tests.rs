/// 存盘框给的落点拆成「目录 ＋ 落名」：照本机的路径规矩（`backslash` ＝ 本机认 `\`，Windows）。
/// Windows 上 `C:\…\n1.txt` 曾按 `/` 拆、目录成了空串 ⇒ 后端拒「请求格式不对」，单个文件下载恒失败；
/// 盘根那一级（`/` · `C:\`）目录带着分隔符（`C:` 在 Windows 上是「那个盘的当前目录」，不是盘根）。
/// 拆法是纯函数 ⇒ Windows 那一形在哪台上都能证。
#[test]
fn a_save_path_splits_by_the_local_platform_rules() {
    let split = |s: &str, backslash: bool| {
        super::split_dest(s.as_bytes(), backslash).map(|(d, n)| {
            (
                String::from_utf8(d.bytes().to_vec()).unwrap(),
                String::from_utf8(n.bytes().to_vec()).unwrap(),
            )
        })
    };
    let pair = |d: &str, n: &str| Some((d.to_string(), n.to_string()));
    for (path, want) in [
        (
            r"C:\Users\u\Downloads\n1.txt",
            pair(r"C:\Users\u\Downloads", "n1.txt"),
        ),
        (r"C:\n1.txt", pair(r"C:\", "n1.txt")),
        (r"D:\a/b\c.txt", pair(r"D:\a/b", "c.txt")),
        (r"\\srv\share\x.bin", pair(r"\\srv\share", "x.bin")),
        (r"C:\Users\u\", None),
    ] {
        assert_eq!(split(path, true), want, "Windows 上「{path}」");
    }
    for (path, want) in [
        ("/home/u/dl/n1.txt", pair("/home/u/dl", "n1.txt")),
        ("/n1.txt", pair("/", "n1.txt")),
        (r"/home/u/a\b.txt", pair("/home/u", r"a\b.txt")),
        ("n1.txt", None),
        ("/home/u/", None),
    ] {
        assert_eq!(split(path, false), want, "Linux 上「{path}」");
    }
}
