use super::*;

// ═══════════════════════════════════════════════════════════════════
// 两问那个状态机
// ═══════════════════════════════════════════════════════════════════

/// 🔴 收工的顺序：**先落结局、再加趟数。**
///
/// 反了的话，判据（与界面）看见 `rounds > 0` 之后去读 `last`，读到的可能是上一趟的。
/// 本条把那个顺序钉成可观测的：趟数一变，结局就必须已经是这一趟的。
#[test]
fn the_outcome_is_visible_before_the_round_counter_moves() {
    let b = DownloadBoard::default();
    assert_eq!(b.rounds(), 0);
    assert!(b.last().is_none(), "一趟都没跑，却已经有结局了");
    assert!(b.in_flight().is_none());

    b.begin("a.txt");
    assert_eq!(b.in_flight().as_deref(), Some("a.txt"));
    b.progress(512, 4096);
    assert_eq!(b.seen(), (512, 4096));
    // 在飞的时候**还没有**结局（否则界面会先报一个结果再继续走进度条）。
    assert!(b.last().is_none(), "还在飞就有结局了");
    assert_eq!(b.rounds(), 0, "还在飞，趟数就加了");

    b.finish(Outcome::Done {
        dest: "/tmp/a.txt".into(),
        bytes: 4096,
    });
    assert_eq!(b.rounds(), 1);
    assert!(b.in_flight().is_none(), "跑完了还挂着在飞");
    match b.last().expect("跑完了却没有结局") {
        Outcome::Done { dest, bytes } => {
            assert_eq!(dest, "/tmp/a.txt");
            assert_eq!(bytes, 4096);
        }
        other => panic!("{other:?}"),
    }
}

/// 失败那一支要把**下层那句原话**带着走（后端路径解析的拒绝、连接失败、落地失败）。
///
/// 从前这里指着 `sftp_pool_tests` 里那条「中继真的拒（在转给本机后端之前）」；那道拒绝删了
/// （用户「文件管理器全部都可以改. 不需要任何围栏」），那条翻成了 `sftp_pool_tests::a_download_onto_a_session_file_is_forwarded_like_any_other`。
/// 本条钉的仍是「拒的那句话（不管是谁拒的）到得了这一格」；下面那句样本是历史上那道围栏的原话，只当一句字符串用。
#[test]
fn a_failure_carries_the_reason_it_was_given() {
    let b = DownloadBoard::default();
    let why =
        "拒绝写 Claude 数据源文件(/home/u/.claude/projects/p/s.jsonl)——管理会话文件请用历史浏览器";
    b.finish(Outcome::Failed {
        dest: "/home/u/.claude/projects/p/s.jsonl".into(),
        why: why.into(),
    });
    match b.last().expect("没有结局") {
        Outcome::Failed { dest, why: got } => {
            assert_eq!(
                got.said, why,
                "原话被改写了 —— 用户看到的就不是下层说的那句"
            );
            assert!(dest.contains(".jsonl"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(b.rounds(), 1, "失败也算跑完一趟（否则界面会一直等）");
}

/// 下载到 Windows：远端名字里 Windows 不认的字换成 `_`、结尾的点去掉、保留的设备名前面加 `_`（不然落点照抄、下载失败，
/// `con.txt` 这类名字还会被当成「已经在了」）；别的平台原样。
#[test]
fn a_remote_name_windows_cannot_take_gets_a_legal_default_name() {
    for (remote, local) in [
        ("a:b.txt", "a_b.txt"),
        ("x?", "x_"),
        ("name.", "name"),
        ("a<b>|c*\"d\"", "a_b__c__d_"),
        ("con.txt", "_con.txt"),
        ("COM1", "_COM1"),
        ("lpt9.log", "_lpt9.log"),
        ("com0.txt", "com0.txt"),
        ("console.txt", "console.txt"),
        ("ok.txt", "ok.txt"),
        ("...", "_"),
    ] {
        assert_eq!(super::local_name(remote, true), local, "「{remote}」");
        assert_eq!(
            super::local_name(remote, false),
            remote,
            "非 Windows 上「{remote}」被改了"
        );
    }
}

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
