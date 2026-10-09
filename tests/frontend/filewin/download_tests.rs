use super::*;

fn row(name: &str, is_dir: bool, lossy: bool) -> Row {
    Row {
        name: name.to_string(),
        path: format!("/srv/data/{name}"),
        is_dir,
        size: 4096,
        lossy_name: lossy,
    }
}

/// 目录与有损名拉不下来；普通文件可以。
#[test]
fn only_a_plain_addressable_file_can_be_pulled() {
    assert!(is_downloadable(&row("a.txt", false, false)));
    assert!(!is_downloadable(&row("sub", true, false)), "目录竟然可下载");
    assert!(
        !is_downloadable(&row("bad\u{FFFD}", false, true)),
        "有损名竟然可下载"
    );
    assert!(!is_downloadable(&row("d", true, true)));
}

/// 🔴 它与 `is_copyable` **今天逐行一致** —— 而那是一条相等断言，不是巧合。
///
/// 两份各自存在的理由住 `is_downloadable` 的头注（它们答的是两个问题，
/// 将来递归下载会让目录那一档只在**一侧**放开）。
/// ⇒ 本条在它们**开始漂**的那一天红，而那正是该回去重读那段头注的时刻。
#[test]
fn the_two_row_gates_agree_today_and_say_so_when_they_stop() {
    let corpus = [
        row("a.txt", false, false),
        row("sub", true, false),
        row("bad", false, true),
        row("weird", true, true),
    ];
    // 反空真：语料里**两种答案都要出现**，否则下面那一比可以靠「全真」或「全假」成立。
    let any_true = corpus.iter().any(|r| is_downloadable(r));
    let any_false = corpus.iter().any(|r| !is_downloadable(r));
    assert!(any_true && any_false, "语料退化了 —— 本条此刻是空真的");

    // **刻意只放开了复制那一侧**：目录能复制（后端 `recursive: true`），
    //   下载仍只收文件（没有「递归下载」这条路）⇒ 两道闸恰好在「名字寻址得到的目录」这一格上分开，其余逐格相等。
    for r in &corpus {
        let copy = crate::copy::is_copyable(r);
        if r.is_dir && !r.lossy_name {
            assert!(
                copy && !is_downloadable(r),
                "目录那一格的差异不是「能复制、不能下载」"
            );
            continue;
        }
        assert_eq!(
            is_downloadable(r),
            copy,
            "两道闸对 `{}`（目录={} 有损={}）给了不同答案。\n\
             ⇒ 它们开始漂了。这不一定是 bug —— 回去读 `is_downloadable` 的头注：\n\
               如果是**刻意**只放开一侧（例：递归下载），把本条改成写明那一格的差异；\n\
               如果不是，那就是有一侧被改错了。",
            r.name,
            r.is_dir,
            r.lossy_name
        );
    }
}

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
