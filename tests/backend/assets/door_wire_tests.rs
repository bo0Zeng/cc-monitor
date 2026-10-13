//! 资产域那扇门（`door.rs` 那几样）对这台文件管理面的每一问、每一种结局：换形前后逐字节不许变。
//! 换形之前用旧代码（按名找命令、JSON 进 JSON 出）跑出来、钉成字面量（临时目录换 `<D>`）；没有重写开关。
//! 只在 POSIX 跑（链接 · 权限位那几问 Windows 上是另一套结局）。
use super::*;
use crate::assets::aliases::tests::HomeDoor;

#[cfg(unix)]
#[test]
fn every_door_answer_keeps_its_bytes() {
    let base = std::env::temp_dir().join(format!(
        "ccm-door-wire-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|x| x.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&base).unwrap();
    let b = base.display().to_string();
    let d = HomeDoor(base.clone());
    let mut got: Vec<String> = Vec::new();
    let mut note = |what: &str, v: String| got.push(format!("{what} = {v}"));
    note("home", format!("{:?}", home(&d)));
    note("peek absent", format!("{:?}", peek(&d, &b, "a.txt")));
    note("peek no parent", format!("{:?}", peek(&d, &b, "no/a.txt")));
    let landed = |r: Result<Landed, Refused>| {
        format!(
            "{:?}",
            r.map(|l| (l.path, l.changed, l.created, l.backup.is_some()))
        )
    };
    note(
        "put new",
        landed(put(&d, &b, "a.txt", "one", None, false, false)),
    );
    note(
        "put same",
        landed(put(&d, &b, "a.txt", "one", Some("one"), false, false)),
    );
    note(
        "put backup",
        landed(put(&d, &b, "a.txt", "two", Some("one"), true, false)),
    );
    note(
        "put stale",
        landed(put(&d, &b, "a.txt", "x", Some("WRONG"), false, false)),
    );
    note(
        "put no parent",
        landed(put(&d, &b, "p/q/a.txt", "x", None, false, false)),
    );
    note(
        "put parents",
        landed(put(&d, &b, "p/q/a.txt", "x", None, false, true)),
    );
    note("peek present", format!("{:?}", peek(&d, &b, "a.txt")));
    note(
        "stat file",
        format!("{:?}", stat_kind(&d, &format!("{b}/a.txt"))),
    );
    note("stat dir", format!("{:?}", stat_kind(&d, &b)));
    note(
        "stat absent",
        format!("{:?}", stat_kind(&d, &format!("{b}/none"))),
    );
    note("mkdir", format!("{:?}", mkdir(&d, &b, "d")));
    note("mkdir again", format!("{:?}", mkdir(&d, &b, "d")));
    note("mkdir no parent", format!("{:?}", mkdir(&d, &b, "x/y")));
    note(
        "copy",
        format!("{:?}", copy(&d, &b, "a.txt", "d/b.txt", false)),
    );
    note(
        "copy again",
        format!("{:?}", copy(&d, &b, "a.txt", "d/b.txt", false)),
    );
    note(
        "copy dir flat",
        format!("{:?}", copy(&d, &b, "d", "e", false)),
    );
    note(
        "copy dir tree",
        format!("{:?}", copy(&d, &b, "d", "e", true)),
    );
    note("link", format!("{:?}", link(&d, &b, "d/l", "a.txt")));
    note("link again", format!("{:?}", link(&d, &b, "d/l", "a.txt")));
    note(
        "stat link",
        format!("{:?}", stat_kind(&d, &format!("{b}/d/l"))),
    );
    note(
        "rename",
        format!("{:?}", rename(&d, &b, "d/b.txt", "d/c.txt")),
    );
    note(
        "rename absent",
        format!("{:?}", rename(&d, &b, "d/b.txt", "d/z.txt")),
    );
    note("chmod", format!("{:?}", chmod(&d, &b, "a.txt", 0o600)));
    note(
        "chmod absent",
        format!("{:?}", chmod(&d, &b, "none", 0o600)),
    );
    note(
        "delete stale",
        format!("{:?}", delete(&d, &b, "a.txt", "WRONG")),
    );
    note("delete", format!("{:?}", delete(&d, &b, "a.txt", "two")));
    note(
        "delete gone",
        format!("{:?}", delete(&d, &b, "a.txt", "two")),
    );
    note(
        "rmdir not empty",
        format!("{:?}", delete_empty_dir(&d, &b, "d")),
    );
    note(
        "remove flat dir",
        format!("{:?}", remove(&d, &b, "d", false)),
    );
    note("remove tree", format!("{:?}", remove(&d, &b, "d", true)));
    note("remove absent", format!("{:?}", remove(&d, &b, "d", true)));
    note("mkdir empty", format!("{:?}", mkdir(&d, &b, "m")));
    note("rmdir", format!("{:?}", delete_empty_dir(&d, &b, "m")));
    note("rmdir gone", format!("{:?}", delete_empty_dir(&d, &b, "m")));
    note(
        "edit parents",
        format!(
            "{:?}",
            edit(&d, &b, "r/s/t.txt", false, true, |t| Ok(
                (t.is_none()).then(|| "new".to_string())
            ))
            .map(|e| match e {
                Edited::Unchanged => "unchanged".to_string(),
                Edited::Written(l) => format!("written {} {}", l.path, l.created),
            })
        ),
    );
    // 别名那边直接问的两样：那个目录里我们放的链接（`files-ls` ＋ `files-stat` 的 `link_target`）。
    let bin = crate::assets::aliases::links::bin_rel();
    let _ = std::fs::create_dir_all(base.join(bin));
    std::fs::write(base.join(bin).join("ccm"), "").unwrap();
    std::os::unix::fs::symlink(
        crate::assets::aliases::links::target(),
        base.join(bin).join("x"),
    )
    .unwrap();
    std::os::unix::fs::symlink("elsewhere", base.join(bin).join("y")).unwrap();
    note(
        "ours",
        format!("{:?}", crate::assets::aliases::links::ours(&d, &b)),
    );
    note(
        "ours absent",
        format!(
            "{:?}",
            crate::assets::aliases::links::ours(&d, &format!("{b}/none"))
        ),
    );
    let _ = std::fs::remove_dir_all(&base);
    let got = got.join("\n").replace(&*b, "<D>");
    assert_eq!(got, DOOR_WIRE, "\n{got}");
}

const DOOR_WIRE: &str = r#"home = Ok("<D>")
peek absent = Ok(Peeked { path: "<D>/a.txt", text: None })
peek no parent = Err("解析 <D>/no 失败 · 路径不存在")
put new = Ok(("<D>/a.txt", true, true, false))
put same = Ok(("<D>/a.txt", false, false, false))
put backup = Ok(("<D>/a.txt", true, false, true))
put stale = Err(Stale("<D>/a.txt 读过后被改过 · 未写入 · 刷新后再试"))
put no parent = Err(Peer { code: "refused", said: "解析 <D>/p/q 失败 · 路径不存在" })
put parents = Ok(("<D>/p/q/a.txt", true, true, false))
peek present = Ok(Peeked { path: "<D>/a.txt", text: Some("two") })
stat file = Ok(Some("file"))
stat dir = Ok(Some("dir"))
stat absent = Ok(None)
mkdir = Ok(())
mkdir again = Err("写入失败 · <D>/d · 已存在")
mkdir no parent = Err("解析 <D>/x 失败 · 路径不存在")
copy = Ok(())
copy again = Err("新建失败 · <D>/d/b.txt · 已存在")
copy dir flat = Err("<D>/d 不是普通文件 · 复制目录要带 recursive")
copy dir tree = Ok(())
link = Ok(())
link again = Err("写入失败 · <D>/d/l · 已存在")
stat link = Ok(None)
rename = Ok(())
rename absent = Err("改名失败 · 路径不存在 · <D>/d/b.txt → <D>/d/z.txt")
chmod = Ok(())
chmod absent = Err("解析 <D>/none 失败 · 路径不存在")
delete stale = Err(Stale("<D>/a.txt 读过后被改过 · 未删除 · 刷新后再试"))
delete = Ok(())
delete gone = Err(Stale("<D>/a.txt 已不在 · 未删除 · 刷新后再试"))
rmdir not empty = Err(Stale("<D>/d 非空 · 此处只删空目录 · 未删除"))
remove flat dir = Err("删除失败 · <D>/d · 原因不明")
remove tree = Ok(())
remove absent = Err("读取失败 · <D>/d · 路径不存在")
mkdir empty = Ok(())
rmdir = Ok(())
rmdir gone = Err(Stale("<D>/m 已不在 · 未删除"))
edit parents = Ok("written <D>/r/s/t.txt true")
ours = Ok(["x"])
ours absent = Ok([])"#;
