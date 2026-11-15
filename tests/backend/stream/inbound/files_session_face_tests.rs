//! `files-delete-session` 一条命令做完：删成那份会话记录，那台的那一条历史注解也没了（界面不再另问一条）；
//! 删不成 ⇒ 注解一个字节不动。
//!
//! 沙箱：家（`projects/`）与数据目录（注解那份文件）都在这次运行独占的临时目录里。

use super::*;
use crate::platform::fs::{Face, Own};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 这次运行的替身家：落点那一问是 `fn` 指针，指不到局部变量 ⇒ 住一份进程内只设一次的静态，各条判据共用、各用各的 sid。
static HOME: OnceLock<PathBuf> = OnceLock::new();

fn home() -> &'static Path {
    HOME.get_or_init(|| {
        let home = std::env::temp_dir()
            .join(format!("ccm-dsf-{}", std::process::id()))
            .join("home");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("projects/-p")).expect("建项目目录");
        home
    })
}

fn locate_in_sandbox(sid: &str) -> Result<PathBuf, String> {
    crate::agents::claudecode::paths::session_file_for_delete_in(home(), sid)
}

/// 生产那一个窄口，只把落点换成替身家里找。
const PORT: crate::files::write::SessionPort = crate::files::write::SessionPort {
    locate: locate_in_sandbox,
    ..SESSION_PORT
};

/// 这一条判据自己的数据目录，铺好两条注解：`sid` 那条 ＋ 一条邻居。
fn data_with(tag: &str, sid: &str, other: &str) -> PathBuf {
    let data = home().with_file_name(format!("data-{tag}"));
    let _ = std::fs::remove_dir_all(&data);
    std::fs::create_dir_all(&data).expect("建数据目录");
    std::fs::write(
        data.join(relay_route_core::file_name_of(
            relay_route_core::HISTORY_ANNOTATIONS_REL,
        )),
        serde_json::json!({
            "version": 1,
            "entries": {
                sid: {"starred": true, "customTitle": "要删的那条", "updatedAt": 1},
                other: {"starred": true, "updatedAt": 2},
            },
        })
        .to_string(),
    )
    .expect("铺注解");
    data
}

fn annotations(data: &Path) -> String {
    std::fs::read_to_string(data.join(relay_route_core::file_name_of(
        relay_route_core::HISTORY_ANNOTATIONS_REL,
    )))
    .expect("注解那份文件在")
}

fn entries(data: &Path) -> serde_json::Map<String, serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(&annotations(data)).expect("注解是 JSON");
    v["entries"].as_object().expect("entries 是对象").clone()
}

fn delete(own: &Own, sid: &str) -> Result<cmd_wire::files::FilesPathReply, FilesDeleteSessionErr> {
    delete_then_forget(
        &Face::issue(),
        own,
        cmd_wire::files::FilesDeleteSessionReq {
            sid: sid.to_string(),
        },
        &PORT,
        |_| None,
    )
}

/// 替身那一家：删会话交一条命令。替身程序把收到的 argv 一行一个、连 `FAKE_HOME` 那一格写进 `argv-<sid>`；
/// sid 以 `bad` 打头 ⇒ 往 stderr 说一句、退出码 3。替身程序是 sh 脚本，这一组只在 unix 上（起它的判据也只在 unix 上）。
#[cfg(unix)]
fn fake_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| fake_program_in(home().with_file_name("fake-cli")))
}

/// 替身程序的本体。
#[cfg(unix)]
const FAKE_DEL: &str = "#!/bin/sh\n\
     out=\"$FAKE_HOME/argv-$2\"\n\
     for a in \"$@\"; do printf '%s\\n' \"$a\"; done > \"$out\"\n\
     printf 'FAKE_HOME=%s\\n' \"$FAKE_HOME\" >> \"$out\"\n\
     case \"$2\" in bad*) echo 'no such thread' >&2; exit 3;; esac\n";

/// 在 `d` 里放下替身程序 `del`（`d` 先清空重建），交回 `d`。经 `test_home::write_exe` 写：本进程里从没有过它的写端，
/// 旁边判据线程正 fork 时马上起它也不撞 ETXTBSY。
#[cfg(unix)]
fn fake_program_in(d: PathBuf) -> PathBuf {
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("建替身目录");
    test_home::write_exe(&d.join("del"), FAKE_DEL);
    d
}

/// 替身目录 `d` 里那一份替身程序的命令：`del delete <sid> --force`，带 `FAKE_HOME`，记录住替身目录里。
#[cfg(unix)]
fn fake_command_in(d: &Path, sid: &str) -> Option<crate::agents::DeleteCommand> {
    Some(crate::agents::DeleteCommand {
        program: d.join("del").display().to_string(),
        args: vec!["delete".into(), sid.into(), "--force".into()],
        env: vec![("FAKE_HOME", d.as_os_str().to_os_string())],
        record: d.join(format!("{sid}.jsonl")),
    })
}

/// 替身那一家给的命令（[`fake_dir`] 那一份）。
#[cfg(unix)]
fn by_fake(sid: &str) -> Option<crate::agents::DeleteCommand> {
    fake_command_in(fake_dir(), sid)
}

#[cfg(unix)]
fn delete_by_fake(
    own: &Own,
    sid: &str,
) -> Result<cmd_wire::files::FilesPathReply, FilesDeleteSessionErr> {
    delete_then_forget(
        &Face::issue(),
        own,
        cmd_wire::files::FilesDeleteSessionReq {
            sid: sid.to_string(),
        },
        &PORT,
        by_fake,
    )
}

/// 那一家声明「跑一条命令」⇒ 门照它给的 argv 与环境起那一条、删成了注解跟着删；记录文件后端一个字节不碰（那一家自己删、状态跟着改）。
#[cfg(unix)]
#[test]
fn a_session_deleted_by_its_agents_command_runs_exactly_that_command() {
    let sid = "5a6b7c8d-0000-4000-8000-0000000000c1";
    let other = "0000aaaa-0000-4000-8000-0000000000fd";
    let record = fake_dir().join(format!("{sid}.jsonl"));
    std::fs::write(&record, "{}\n").expect("铺会话");
    let data = data_with("by-cmd", sid, other);

    let got = delete_by_fake(&Own::issue_data_at(&data), sid).expect("命令退出码 0 ⇒ 删成");

    let argv = std::fs::read_to_string(fake_dir().join(format!("argv-{sid}"))).expect("替身起过");
    assert_eq!(
        argv,
        format!(
            "delete\n{sid}\n--force\nFAKE_HOME={}\n",
            fake_dir().display()
        )
    );
    assert_eq!(got.path, crate::files::write::wire_of(&record));
    assert!(record.exists(), "后端不该自己删记录文件");
    let left = entries(&data);
    assert!(!left.contains_key(sid), "删成了，注解还在：{left:?}");
    assert!(left.contains_key(other), "邻居那条注解被连带删了：{left:?}");
}

/// 那一家的命令说删不成（非 0）⇒ 照删不成回，话里带它说的那一句；注解一个字节不动。
#[cfg(unix)]
#[test]
fn a_failing_delete_command_is_a_failed_delete_and_keeps_the_annotation() {
    let sid = "bad00000-0000-4000-8000-0000000000c2";
    let other = "0000aaaa-0000-4000-8000-0000000000fc";
    let data = data_with("by-cmd-bad", sid, other);
    let before = annotations(&data);

    let got = delete_by_fake(&Own::issue_data_at(&data), sid);

    match got {
        Err(FilesDeleteSessionErr::IoFailed(said)) => {
            assert!(said.contains("no such thread"), "{said}")
        }
        other => panic!("该是删不成：{other:?}"),
    }
    assert_eq!(annotations(&data), before, "删会话没成，注解被动了");
}

/// 旁边判据线程正 fork 时现放下替身程序、马上起它：删命令说的照样是替身那一句（一次 ETXTBSY 都没有，撞上一次当场红、不重试）。
/// 替身程序在本进程里写过 ⇒ 别的线程 fork 出的子进程在 exec 之前攥着那份写端，这时起它 ＝ ETXTBSY（快检里这一份时红时绿的那一下）。
#[cfg(unix)]
#[test]
fn the_fake_delete_command_starts_while_other_threads_fork() {
    fn dir() -> PathBuf {
        home().with_file_name("fake-cli-forking")
    }
    fn by_fresh_fake(sid: &str) -> Option<crate::agents::DeleteCommand> {
        fake_command_in(&dir(), sid)
    }
    let sid = "bad00000-0000-4000-8000-0000000000c3";
    let data = data_with(
        "by-cmd-forking",
        sid,
        "0000aaaa-0000-4000-8000-0000000000fb",
    );
    let own = Own::issue_data_at(&data);
    let _f = test_home::forking(4);
    for i in 0..200 {
        fake_program_in(dir());
        let got = delete_then_forget(
            &Face::issue(),
            &own,
            cmd_wire::files::FilesDeleteSessionReq {
                sid: sid.to_string(),
            },
            &PORT,
            by_fresh_fake,
        );
        match got {
            Err(FilesDeleteSessionErr::IoFailed(said)) => {
                assert!(said.contains("no such thread"), "第 {i} 次：{said}")
            }
            other => panic!("第 {i} 次：该是删不成：{other:?}"),
        }
    }
}

#[test]
fn deleting_an_annotated_session_also_drops_its_annotation() {
    let sid = "2f1c9a4e-0b7d-4c1e-9a55-3b2f0c8d1e77";
    let other = "0000aaaa-0000-4000-8000-0000000000ff";
    let record = home().join(format!("projects/-p/{sid}.jsonl"));
    std::fs::write(&record, "{}\n").expect("铺会话");
    let data = data_with("annot", sid, other);

    let got = delete(&Own::issue_data_at(&data), sid).expect("删一份真在的会话");

    assert!(!record.exists(), "说删了，记录还在：{got:?}");
    let left = entries(&data);
    assert!(
        !left.contains_key(sid),
        "会话删了，它那条注解还在：{left:?}"
    );
    assert!(left.contains_key(other), "邻居那条注解被连带删了：{left:?}");
}

#[test]
fn a_session_that_does_not_delete_keeps_its_annotation() {
    let sid = "7d0e1b2c-3a4f-4e5d-8c6b-9a0f1e2d3c4b";
    let other = "0000aaaa-0000-4000-8000-0000000000fe";
    let data = data_with("gone", sid, other);
    let before = annotations(&data);

    let got = delete(&Own::issue_data_at(&data), sid);

    assert!(got.is_err(), "盘上没有那份记录，删会话却说成了：{got:?}");
    assert_eq!(annotations(&data), before, "删会话没成，注解被动了");
}

// ── 归档 · 取消归档 ─────────────────────────

use crate::agents::fake::runs::history as chats;
use crate::agents::{fake, AccountHome, Adapter, ArchiveSide, RecordRoot};

/// 替身家（记录树那一形）的家：`<临时>/fam/convos/-p/` 底下放会话。
fn fam_home() -> Option<PathBuf> {
    Some(home().with_file_name("fam"))
}

/// 在替身家（记录树那一形）里铺一份 `sid` 的会话（归档面按 sid 认会话，得先在盘上）。
fn fam_session(sid: &str) -> PathBuf {
    let dir = fake::records_root(&fam_home().unwrap()).join("-p");
    std::fs::create_dir_all(&dir).expect("建替身家的项目目录");
    let p = dir.join(fake::session_file_name(sid));
    std::fs::write(&p, "{}\n").expect("铺替身家的会话");
    p
}

/// 替身家（声明了归档面）。
fn with_archive() -> [Adapter; 1] {
    [fake::full_row(fam_home)]
}

/// 替身家换成不声明归档面的那一形（别的格同形）。
fn without_archive() -> [Adapter; 1] {
    let mut row = fake::full_row(fam_home);
    if let Some(r) = row.records.as_mut() {
        r.archive = None;
    }
    [row]
}

fn roots_of(reg: &[Adapter]) -> Vec<RecordRoot> {
    crate::agents::record_roots_of(reg)
}

/// 替身程序记下 argv 的那一个目录：照替身家给的那一条命令里的 `FAKE_LOG` 取。
fn cli_dir() -> PathBuf {
    let sid = "5a6b7c8d-0000-4000-8000-0000000000f0";
    fam_session(sid);
    let reg = with_archive();
    let cmd = crate::agents::session_archive_command_among(
        &reg,
        &roots_of(&reg),
        sid,
        ArchiveSide::Archive,
    )
    .expect("替身家声明了归档面");
    PathBuf::from(&cmd.env[0].1)
}

fn ran(verb: &str, sid: &str) -> Option<String> {
    std::fs::read_to_string(cli_dir().join(format!("{verb}-{sid}"))).ok()
}

fn archive_fam(
    reg: &[Adapter],
    sid: &str,
    side: ArchiveSide,
) -> Result<(), FilesArchiveSessionErr> {
    archive_with(reg, &roots_of(reg), sid, side)
}

/// 那一家声明了归档面 ⇒ 归档 · 取消归档各跑它给的那一条（argv 与环境逐字）、退出码 0 ⇒ 成。
#[cfg(unix)]
#[test]
fn archiving_runs_exactly_the_command_its_agent_declares() {
    let sid = "5a6b7c8d-0000-4000-8000-0000000000a1";
    fam_session(sid);
    let log = format!("FAKE_LOG={}\n", cli_dir().display());

    archive_fam(&with_archive(), sid, ArchiveSide::Archive).expect("退出码 0 ⇒ 归档成");
    assert_eq!(ran("shelve", sid), Some(format!("shelve\n{sid}\n{log}")));
    assert_eq!(ran("unshelve", sid), None, "归档那一下跑了取消归档");

    archive_fam(&with_archive(), sid, ArchiveSide::Unarchive).expect("退出码 0 ⇒ 取消归档成");
    assert_eq!(
        ran("unshelve", sid),
        Some(format!("unshelve\n{sid}\n{log}"))
    );
}

/// 那一家的命令非 0 ⇒ 回失败那一码。
#[cfg(unix)]
#[test]
fn a_failing_archive_command_is_the_failed_code() {
    let sid = "bad00000-0000-4000-8000-0000000000a2";
    fam_session(sid);
    for side in [ArchiveSide::Archive, ArchiveSide::Unarchive] {
        match archive_fam(&with_archive(), sid, side) {
            Err(FilesArchiveSessionErr::IoFailed(_)) => {}
            other => panic!("{side:?} 该是失败那一码：{other:?}"),
        }
    }
}

/// 没有哪一家声明归档面 ⇒ 照实拒，一条命令都不跑。
#[cfg(unix)]
#[test]
fn no_archive_face_is_refused_and_runs_nothing() {
    let sid = "5a6b7c8d-0000-4000-8000-0000000000a3";
    fam_session(sid);
    for side in [ArchiveSide::Archive, ArchiveSide::Unarchive] {
        match archive_fam(&without_archive(), sid, side) {
            Err(FilesArchiveSessionErr::Refused(_)) => {}
            other => panic!("{side:?} 该是拒：{other:?}"),
        }
    }
    assert_eq!(ran("shelve", sid), None);
    assert_eq!(ran("unshelve", sid), None);
}

/// 清单行 `archive` 那一格照归档面填：声明了 ⇒ `archived` 照文件名上的记号、那一项的字与没成那一句照文案表写（带这一行的标题）；
/// 没声明 ⇒ `null`。
#[test]
fn the_listing_archive_item_follows_the_archive_face() {
    use crate::history::listing::archive_item_among;
    use cmd_wire::cells::Words;
    use cmd_wire::listing::RowArchive;
    use copy_core::copy_text;
    let dir = home().with_file_name("convos/-p");
    let plain = dir.join(fake::session_file_name(
        "5a6b7c8d-0000-4000-8000-0000000000a4",
    ));
    let shelved = dir.join("sess-5a6b7c8d-0000-4000-8000-0000000000a5.shelved.ndjson");
    assert_eq!(
        archive_item_among(&with_archive(), &plain, "标题甲"),
        Some(RowArchive {
            archived: false,
            label: Words(copy_text("history.menu.archive", &[])),
            failed: Words(copy_text("history.archive.failed", &[("label", "标题甲")])),
        })
    );
    assert_eq!(
        archive_item_among(&with_archive(), &shelved, "标题乙"),
        Some(RowArchive {
            archived: true,
            label: Words(copy_text("history.menu.unarchive", &[])),
            failed: Words(copy_text(
                "history.unarchive.failed",
                &[("label", "标题乙")]
            )),
        })
    );
    assert_eq!(
        archive_item_among(&without_archive(), &shelved, "标题乙"),
        None
    );
}

// ── 一家几个记录根 · 一家几个号（替身合成历史家：`<家>/chats/` ＋ 归档根 `<家>/shelf/`）─────────

/// 替身合成历史家账号 0 的家。
fn chats_home() -> Option<PathBuf> {
    Some(home().with_file_name("chats-0"))
}

/// 记录树那一家 ＋ 替身合成历史家（两个根）。
fn tree_and_chats() -> [Adapter; 2] {
    [fake::full_row(fam_home), chats::row(chats_home)]
}

/// 在 `home` 的 `root` 那一根底下铺一份 `sid` 的会话。
fn chats_session(root: PathBuf, sid: &str) -> PathBuf {
    std::fs::create_dir_all(&root).expect("建替身合成历史家的根");
    let p = root.join(chats::file_name(sid));
    std::fs::write(&p, "{}\n").expect("铺替身合成历史家的会话");
    p
}

/// 那一家一个家报两个根 ⇒ 归档根底下那一份认成那一家（不落到记录树那一家），行上归档那一项照它的归档面出「取消归档」。
#[test]
fn a_record_under_the_archive_root_is_its_familys_and_offers_unarchive() {
    use crate::history::listing::archive_item_among;
    let reg = tree_and_chats();
    let h = chats_home().unwrap();
    let shelved = chats_session(chats::shelf_root(&h), "c-shelved");
    let live = chats_session(chats::records_root(&h), "c-live");
    assert_eq!(
        crate::agents::record_kind_among(&reg, &shelved),
        Some(chats::KIND)
    );
    assert_eq!(
        crate::agents::record_kind_among(&reg, &live),
        Some(chats::KIND)
    );
    let item = archive_item_among(&reg, &shelved, "标题丙").expect("那一家声明了归档面");
    assert!(item.archived, "归档根底下那一份该是已归档：{item:?}");
    assert_eq!(
        item.label.0,
        copy_core::copy_text("history.menu.unarchive", &[])
    );
    let item = archive_item_among(&reg, &live, "标题丁").expect("那一家声明了归档面");
    assert!(!item.archived, "日期树底下那一份该是没归档：{item:?}");
}

/// 一家两个号各一份会话 ⇒ 删 · 归档那一条命令带的是会话所在那个号的家；已归档的那一份删会话照样给出命令。
#[test]
fn delete_and_archive_carry_the_home_the_session_lives_in() {
    let reg = tree_and_chats();
    let h0 = chats_home().unwrap();
    let h1 = home().with_file_name("chats-1");
    chats_session(chats::records_root(&h0), "s-zero");
    chats_session(chats::records_root(&h1), "s-one");
    chats_session(chats::shelf_root(&h1), "s-shelved");
    let homes = [
        AccountHome {
            account: None,
            home: h0.clone(),
        },
        AccountHome {
            account: Some("b".to_string()),
            home: h1.clone(),
        },
    ];
    let roots = crate::agents::record_roots_among(&reg, &|a| {
        if a.kind == chats::KIND {
            homes.to_vec()
        } else {
            Vec::new()
        }
    });
    let home_env = |env: &[(&'static str, std::ffi::OsString)]| {
        env.iter()
            .find(|(k, _)| *k == "FAKE_HOME")
            .map(|(_, v)| PathBuf::from(v))
    };
    for (sid, want) in [("s-zero", &h0), ("s-one", &h1)] {
        let cmd =
            crate::agents::session_archive_command_among(&reg, &roots, sid, ArchiveSide::Archive)
                .expect("那一家认得这个 sid");
        assert_eq!(
            home_env(&cmd.env).as_ref(),
            Some(want),
            "{sid} 归档带错了家"
        );
        let cmd = crate::agents::session_delete_command_among(&reg, &roots, sid)
            .expect("那一家删会话交命令");
        assert_eq!(home_env(&cmd.env).as_ref(), Some(want), "{sid} 删带错了家");
    }
    let cmd = crate::agents::session_delete_command_among(&reg, &roots, "s-shelved")
        .expect("已归档的那一份删会话照样给出命令");
    assert_eq!(home_env(&cmd.env).as_ref(), Some(&h1));
    assert_eq!(
        cmd.record,
        chats::shelf_root(&h1).join(chats::file_name("s-shelved"))
    );
    let cmd = crate::agents::session_archive_command_among(
        &reg,
        &roots,
        "s-shelved",
        ArchiveSide::Unarchive,
    )
    .expect("已归档的那一份取消归档");
    assert_eq!(home_env(&cmd.env).as_ref(), Some(&h1));
    assert!(
        crate::agents::session_archive_command_among(
            &reg,
            &roots,
            "s-shelved",
            ArchiveSide::Archive
        )
        .is_none(),
        "已归档的不再出归档"
    );
    assert!(
        crate::agents::session_delete_command_among(&reg, &roots, "nobody").is_none(),
        "谁都不认的 sid 不出命令"
    );
}
