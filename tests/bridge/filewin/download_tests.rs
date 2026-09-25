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

// ═══════════════════════════════════════════════════════════════════
// 落点那条纯函数
// ═══════════════════════════════════════════════════════════════════

/// 用户给了完整路径 ⇒ 原样用（包括改名）。
#[test]
fn a_full_path_is_used_as_given() {
    assert_eq!(plan_dest("/tmp/a.txt", "orig.txt").unwrap(), "/tmp/a.txt");
    // 周围空白不算内容。
    assert_eq!(
        plan_dest("  /tmp/b.txt \n", "orig.txt").unwrap(),
        "/tmp/b.txt"
    );
}

/// 以分隔符结尾 ⇒ 当成目录，把**原名**接上去。
#[test]
fn a_directory_shaped_answer_gets_the_original_name_appended() {
    assert_eq!(
        plan_dest("/tmp/dl/", "报表.csv").unwrap(),
        "/tmp/dl/报表.csv"
    );
    // 多个尾分隔符也算一个。
    assert_eq!(plan_dest("/tmp/dl//", "x").unwrap(), "/tmp/dl/x");
    // 🔴 目录形但这一行没名字 ⇒ 报错，**不生成 `/tmp/dl/`** 那种以分隔符结尾的落点
    //（那个串交给 `sftp_download` 会落成一个叫空名的文件，或者直接失败在别处）。
    let e = plan_dest("/tmp/dl/", "   ").expect_err("没名字可接竟然过了");
    assert!(e.contains("/tmp/dl/"), "报错没说是哪条路径：{e}");
}

/// 🔴 空的 ⇒ 报错，**不拿缺省值兜底**。
///
/// 少了它，把框清空之后点确认会悄悄用一个用户看不见的路径 —— 而下载**会覆盖**。
#[test]
fn an_empty_answer_is_refused_rather_than_silently_defaulted() {
    for bad in ["", "   ", "\n", "\t "] {
        let e = plan_dest(bad, "orig.txt").expect_err(&format!("`{bad:?}` 竟然过了"));
        assert!(e.contains("空的"), "报错没说清是什么问题：{e}");
    }
    // 阴性对照：缺省值本身**是**合法的（否则上面那一比可以靠「什么都拒」全绿）。
    let d = default_dest("orig.txt");
    assert_eq!(plan_dest(&d, "orig.txt").unwrap(), d);
    assert!(d.ends_with("/orig.txt"), "缺省落点没带上原名：{d}");
}

// ═══════════════════════════════════════════════════════════════════
// 行上那道闸
// ═══════════════════════════════════════════════════════════════════

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

    for r in &corpus {
        assert_eq!(
            is_downloadable(r),
            crate::filewin::copy::is_copyable(r),
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

/// 落点没占用 ⇒ 直接可以做（不多问一句）。
#[test]
fn a_free_destination_goes_straight_through() {
    let ask = Ask::for_row(&row("a.txt", false, false));
    match judge_dest(&ask, |_| false) {
        DestVerdict::Go { src_path, dest } => {
            assert_eq!(src_path, "/srv/data/a.txt");
            assert!(dest.ends_with("/a.txt"), "落点没带上原名：{dest}");
        }
        other => panic!("落点是空的却没直接放行：{other:?}"),
    }
}

/// 🔴 落点已经有东西 ⇒ **再问一次**，而不是盖掉。
///
/// # 少了它会怎样（这一条是承重的）
///
/// `download_inner` 先写 `{dest}.part` 再 `rename` 上位 ⇒ 原处那个文件被
/// **原子地盖掉**，没有备份、不可撤销。而缺省落点是 `<home>/<原名>` ——
/// 用户点两下确认就可能盖掉自己 home 里的同名文件。
#[test]
fn an_occupied_destination_asks_once_more_instead_of_clobbering() {
    let ask = Ask::for_row(&row("a.txt", false, false));
    match judge_dest(&ask, |_| true) {
        DestVerdict::NeedsOverwrite(Ask::Overwrite {
            src_path,
            src_name,
            dest,
            ..
        }) => {
            assert_eq!(src_path, "/srv/data/a.txt");
            assert_eq!(src_name, "a.txt");
            assert!(dest.ends_with("/a.txt"));
        }
        other => panic!("落点上有东西却没再问一次：{other:?}"),
    }
}

/// 路径不合法 ⇒ 报错那一支，**而且它与「要再问一次」是两支**。
#[test]
fn an_illegal_path_is_its_own_verdict_not_a_confirmation() {
    let mut ask = Ask::for_row(&row("a.txt", false, false));
    if let Ask::Dest { text, .. } = &mut ask {
        *text = "   ".into();
    }
    // ⚠ `exists` 恒真 —— 这样「不合法」必须**先**出来，否则它会被当成「要确认覆盖」。
    match judge_dest(&ask, |_| true) {
        DestVerdict::Rejected(e) => assert!(e.contains("空的"), "{e}"),
        other => panic!("空落点被判成了别的东西：{other:?}"),
    }
}

/// 存在性那一问是**注进来的**，而生产那一侧只有一个住址。
///
/// ⚠ 判源码是代理（同族先例 `transfer_tests::the_real_adapters_speak_only_through_the_channel`）。
/// 买的是：判据走的那条路与生产走的那条路，**存在性判定只有一份**。
#[test]
fn the_existence_check_has_exactly_one_production_address() {
    let prod =
        guard_core::production_code(include_str!("../../../src/bridge/src/filewin/download.rs"));
    let needle = format!("{}::{}", "std::path::Path", "new(p).exists()");
    assert_eq!(
        prod.matches(needle.as_str()).count(),
        1,
        "`download.rs` 生产段里碰盘判存在的地方不是恰好一处 —— \
         多了就是那一问长出了第二个住址"
    );
    // 而 `judge_dest` 自己**不许**直接碰盘（它是注入式的，判据靠这一点走两条分支）。
    let at_judge = prod
        .find("pub fn judge_dest(")
        .expect("`judge_dest` 不在生产段里 —— 抽取器坏了");
    let body_end = prod[at_judge..]
        .find("\npub fn dest_exists")
        .map(|i| at_judge + i)
        .expect("`dest_exists` 没紧跟在它后面 —— 本条的窗口口径要重定");
    assert!(
        !prod[at_judge..body_end].contains(".exists()"),
        "`judge_dest` 自己碰盘了 —— 那样判据就得在盘上摆真文件才走得到两条分支"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 那一格共享状态
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

/// 失败那一支要把**下层那句原话**带着走（围栏的拒绝、连接失败、落地失败）。
///
/// ⚠ 这一条与 `sftp_pool_tests::a_download_onto_a_live_session_file_is_refused_before_anything_is_sent`
/// 是两件事：那一条钉「中继真的拒（在转给本机后端之前）」，本条钉「拒的那句话到得了这一格」。
#[test]
fn a_failure_carries_the_reason_it_was_given() {
    let b = DownloadBoard::default();
    let why =
        "拒绝写 Claude 数据源文件(/home/u/.claude/projects/p/s.jsonl)——管理会话文件请用历史浏览器";
    b.finish(Outcome::Failed {
        dest: "/home/u/.claude/projects/p/s.jsonl".into(),
        why: why.to_string(),
    });
    match b.last().expect("没有结局") {
        Outcome::Failed { dest, why: got } => {
            assert_eq!(got, why, "原话被改写了 —— 用户看到的就不是下层说的那句");
            assert!(dest.contains(".jsonl"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(b.rounds(), 1, "失败也算跑完一趟（否则界面会一直等）");
}
