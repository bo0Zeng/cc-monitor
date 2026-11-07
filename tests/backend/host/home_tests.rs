//! 托管终端的家：同一个 id 的家里留着上一个托管进程的 `facts.json`（读不懂 · 比声明新）⇒ 占家那一下删掉，事实照写。

use super::*;

fn left_behind(tag: &str, seed: &[u8]) {
    let (own, data) = super::super::run::test_support::sandbox_own(tag);
    let id = format!("hm-{tag}");
    let dir = data.join(HOSTS).join(&id);
    std::fs::create_dir_all(&dir).expect("建上一个的家");
    std::fs::write(dir.join(FACTS), seed).expect("留下上一个的事实");
    let home = Home::claim(&own, &id).expect("占家");
    let mine = Facts {
        id: id.clone(),
        name: id.clone(),
        ..Facts::default()
    };
    home.write_facts(&mine)
        .unwrap_or_else(|e| panic!("{tag}：上一个留下的事实挡住了这一个：{e:?}"));
    assert_eq!(read_facts(&dir), Some(mine), "{tag}：读回的不是这一份");
    home.clear();
    let _ = std::fs::remove_dir_all(&data);
}

#[test]
fn an_unreadable_facts_file_left_behind_does_not_block_the_next_host() {
    left_behind("bad", b"{");
}

#[test]
fn a_newer_facts_file_left_behind_does_not_block_the_next_host() {
    left_behind("newer", b"{\"version\":99,\"id\":\"x\"}");
}

/// `ccm -- --hosts`（`cch ls`）那几行：一个托管终端一行，名字 · 目录 · 账号 · 起的时刻 · 有没有人接着；没写账号的说缺省号；一个都没有说一句。
#[test]
fn the_host_listing_has_one_row_per_terminal() {
    let tz = crate::common::time::Tz::default();
    let at = 1_700_000_000_i64;
    let a = Facts {
        id: "h1".into(),
        name: "proj-cc".into(),
        cwd: "/x/proj".into(),
        account: "b".into(),
        attached: true,
        started_ms: at * 1_000,
        ..Facts::default()
    };
    let b = Facts {
        id: "h2".into(),
        name: "notes-cc".into(),
        cwd: "/x/notes".into(),
        started_ms: at * 1_000,
        ..Facts::default()
    };
    let out = listing(&[a, b], at, &tz);
    let rows: Vec<&str> = out.lines().collect();
    let when = crate::common::time::fmt_at(at, at, &tz);
    assert_eq!(
        rows,
        vec![
            copy_text(
                "beHost.hosts.row",
                &[
                    ("name", "proj-cc"),
                    ("cwd", "/x/proj"),
                    ("account", "b"),
                    ("at", &when),
                    ("attached", &copy_text("beHost.hosts.attached", &[])),
                ],
            ),
            copy_text(
                "beHost.hosts.row",
                &[
                    ("name", "notes-cc"),
                    ("cwd", "/x/notes"),
                    ("account", &copy_text("beHost.hosts.defaultAccount", &[])),
                    ("at", &when),
                    ("attached", &copy_text("beHost.hosts.detached", &[])),
                ],
            ),
        ],
        "{out}"
    );
    assert_eq!(
        listing(&[], at, &tz).trim_end(),
        copy_text("beHost.hosts.none", &[])
    );
}
