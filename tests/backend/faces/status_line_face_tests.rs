//! 状态行那几格的值与预览：沙箱数据目录里摆一个托管终端的家（事实 · 队），照盘上取；分支照 `.git/HEAD`；耳朵跟着打标 · 队走。

use super::*;
use crate::host::managed::home::{Facts, Home};
use cmd_wire::{Machine, Sid};

/// 托管终端的沙箱：数据目录（`hosts/`）与家目录（队）分开，都在判据的沙箱里。
fn host_own(tag: &str) -> (Own, PathBuf) {
    let root = test_home::sandbox_root().join(format!("sl-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("data")).expect("建沙箱数据目录");
    std::fs::create_dir_all(root.join("home")).expect("建沙箱家目录");
    let d = root.join("data").to_string_lossy().into_owned();
    let h = root.join("home").to_string_lossy().into_owned();
    let own = Own::issue_with(&move |k| match k {
        "CCM_DATA_DIR" => Some(d.clone()),
        "HOME" => Some(h.clone()),
        _ => None,
    });
    (own, root)
}

fn put(own: &Own, sid: &str, id: &str) {
    let req = cmd_wire::queue::SessionQueuePutReq {
        machine: Machine::local(),
        sid: sid.parse::<Sid>().unwrap(),
        id: id.to_string(),
        end: "mobile".into(),
        n: None,
        at_ms: 1,
        text: format!("text of {id}"),
        attachments: Vec::new(),
        rewind: None,
    };
    crate::history::session_queue::put(own, &req, crate::common::time::now_ms() as i64).unwrap();
}

fn waiting(n: usize) -> String {
    copy_text("beQueue.said.waiting", &[("n", &n.to_string())])
}

fn git_head(dir: &Path, head: &str) {
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::write(dir.join(".git/HEAD"), head).unwrap();
}

#[test]
fn the_branch_comes_from_the_nearest_git_head() {
    let root = test_home::scratch("sl-branch");
    let repo = root.join("repo");
    std::fs::create_dir_all(repo.join("a/b")).unwrap();
    git_head(&repo, "ref: refs/heads/main\n");
    assert_eq!(branch_of(&repo), "main");
    assert_eq!(branch_of(&repo.join("a/b")), "main", "子目录里往上找");
    git_head(&repo, "0123456789abcdef0123456789abcdef01234567\n");
    assert_eq!(branch_of(&repo), "0123456", "游离 ⇒ 提交号前 7 位");
    let wt = root.join("wt");
    let gd = root.join("gitdir/worktrees/wt");
    std::fs::create_dir_all(&wt).unwrap();
    std::fs::create_dir_all(&gd).unwrap();
    std::fs::write(gd.join("HEAD"), "ref: refs/heads/topic/feature\n").unwrap();
    std::fs::write(wt.join(".git"), format!("gitdir: {}\n", gd.display())).unwrap();
    assert_eq!(branch_of(&wt), "topic/feature", "工作树那一形照 gitdir");
    let none = root.join("none");
    std::fs::create_dir_all(&none).unwrap();
    // 沙箱根往上没有仓（判据的沙箱在临时目录里）。
    if !std::process::Command::new("git")
        .arg("-C")
        .arg(&none)
        .arg("rev-parse")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        assert_eq!(branch_of(&none), "", "不在仓里 ⇒ 空");
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// ★ 名 · 路径 · 号 · 排队 · 分支 · 机器名照盘上那一份；没打标 ⇒ 模型 · 上下文 · 排队空。
#[test]
fn the_values_come_from_the_hosts_facts_and_the_queue() {
    let (own, root) = host_own("values");
    let cwd = root.join("work");
    std::fs::create_dir_all(&cwd).unwrap();
    git_head(&cwd, "ref: refs/heads/dev\n");
    let id = "sl-values";
    let home = Home::claim(&own, id).expect("占家");
    let mut facts = Facts {
        id: id.into(),
        name: "w1".into(),
        cwd: cwd.display().to_string(),
        agent: "claude".into(),
        account: "acct-x".into(),
        ..Facts::default()
    };
    home.write_facts(&facts).unwrap();
    let (v, record) = values_of(&own, id);
    assert_eq!(v["name"], "w1");
    assert_eq!(v["branch"], "dev");
    assert_eq!(v["host"], host_name());
    let user_home = crate::platform::paths::home_dir().map(|h| h.display().to_string());
    assert_eq!(
        v["cwd"],
        status::path_shown(&cwd.display().to_string(), user_home.as_deref())
    );
    assert_eq!(
        (v["5h"].as_str(), v["7d"].as_str()),
        ("", ""),
        "额度账里没那个号 ⇒ 空"
    );
    assert_eq!(
        v["account"],
        crate::accounts::quota::name_words::account_text_of("claude", "acct-x")
    );
    assert_eq!(
        (v["model"].as_str(), v["ctx"].as_str(), v["queue"].as_str()),
        ("", "", "")
    );
    assert_eq!(record, None);
    put(&own, "sl-s1", "a");
    put(&own, "sl-s1", "b");
    facts.sid = "sl-s1".into();
    home.write_facts(&facts).unwrap();
    assert_eq!(values_of(&own, id).0["queue"], waiting(2));
    home.clear();
    assert!(values_of(&own, id).0.is_empty(), "那个终端没了 ⇒ 空的一份");
}

/// ★ 耳朵：起来那一下说一次；打标 · 队变了跟着说；撤完那一格空；什么都没变不说。
#[test]
fn the_ears_follow_the_tag_and_the_queue_and_stay_quiet_otherwise() {
    let (own, _root) = host_own("ears");
    let id = "sl-ears";
    let home = Home::claim(&own, id).expect("占家");
    let mut facts = Facts {
        id: id.into(),
        name: "w2".into(),
        ..Facts::default()
    };
    home.write_facts(&facts).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let _ears = ears(
        &own,
        id,
        Box::new(move |s| {
            let _ = tx.lock().unwrap().send(s);
        }),
    )
    .expect("挂上");
    let next = || {
        rx.recv_timeout(guard_core::wall::CEILING)
            .expect("等到下一份")
    };
    let first = next();
    assert_eq!(first.values["name"], "w2");
    assert_eq!(
        first.template,
        status::DEFAULT_TEMPLATE,
        "沙箱家里没设 ⇒ 缺省"
    );
    put(&own, "sl-s2", "a");
    put(&own, "sl-s2", "b");
    facts.sid = "sl-s2".into();
    home.write_facts(&facts).unwrap();
    let mut s = next();
    while s.values["queue"] != waiting(2) {
        s = next();
    }
    crate::history::session_queue::cancel(&own, "sl-s2", "a", crate::common::time::now_ms() as i64)
        .unwrap();
    while s.values["queue"] != waiting(1) {
        s = next();
    }
    crate::history::session_queue::cancel(&own, "sl-s2", "b", crate::common::time::now_ms() as i64)
        .unwrap();
    while !s.values["queue"].is_empty() {
        s = next();
    }
    // 只改了一份不相干的事实以外的写法：照原样再写一次 ⇒ 值没变，不说。
    home.write_facts(&facts).unwrap();
    assert!(
        rx.recv_timeout(std::time::Duration::from_millis(500))
            .is_err(),
        "没变不说"
    );
    // 偏好里的模板改了（写的是这个沙箱自己的家，不碰别的判据共用的那一份）⇒ 跟着说新的模板。
    let prefs = crate::common::machine_prefs::Prefs {
        status_line: Some("{name}|{host}".into()),
        ..Default::default()
    };
    crate::common::machine_prefs::write(&own, &prefs).unwrap();
    let mut s = next();
    while s.template != "{name}|{host}" {
        s = next();
    }
    home.clear();
}

/// ★ 预览：没有活着的托管终端 ⇒ 照示例值、那一句照实说；有 ⇒ 照它算；认不出的键列出来。
#[test]
fn the_preview_uses_samples_without_a_host_and_the_newest_host_otherwise() {
    let tz = crate::common::time::Tz::of(Some("UTC"));
    let p = preview_of("{name}|{nope} {time}", None, 0, &tz);
    assert_eq!(p.from, copy_text("beStatusLine.preview.sample", &[]));
    let line = p.line.clone().expect("留那一行");
    assert!(
        line.starts_with(&(status::field("name").unwrap().sample)()),
        "{line:?}"
    );
    assert_eq!(status::width(&line), PREVIEW_COLS);
    assert_eq!(
        p.unknown_said.as_deref(),
        Some(copy_text("beStatusLine.preview.unknown", &[("keys", "{nope}")]).as_str())
    );
    assert_eq!(p.default_template, status::DEFAULT_TEMPLATE);
    assert_eq!(p.fields.len(), status::FIELDS.len());

    let (own, _root) = host_own("preview");
    let id = "sl-preview";
    let home = Home::claim(&own, id).expect("占家");
    let facts = Facts {
        id: id.into(),
        name: "w3".into(),
        ..Facts::default()
    };
    home.write_facts(&facts).unwrap();
    let p = preview_of(
        "{name}|{time}",
        Some((&own, &facts)),
        3600 * 9 + 60 * 5,
        &tz,
    );
    assert_eq!(
        p.from,
        copy_text("beStatusLine.preview.from", &[("name", "w3")])
    );
    let line = p.line.clone().expect("留那一行");
    assert!(
        line.starts_with("w3 ") && line.ends_with(" 09:05"),
        "{line:?}"
    );
    assert!(p.unknown_said.is_none());
    let p = preview_of("", Some((&own, &facts)), 0, &tz);
    assert_eq!(p.line, None, "空模板不留那一行");
    assert_eq!(p.from, copy_text("beStatusLine.preview.off", &[]));
    home.clear();
}

/// 几个活着的托管终端 ⇒ 预览照最近起的那一个（`alive` 照起来的先后排，取最后一个）。
#[test]
fn the_preview_takes_the_newest_live_host() {
    let (own, _root) = host_own("newest");
    let mut homes = Vec::new();
    for (i, name) in ["old", "new"].iter().enumerate() {
        let id = format!("sl-n{i}");
        let h = Home::claim(&own, &id).expect("占家");
        h.write_facts(&Facts {
            id: id.clone(),
            name: (*name).into(),
            started_ms: i as i64 + 1,
            ..Facts::default()
        })
        .unwrap();
        homes.push(h);
    }
    let newest = newest_host(&own).expect("有活着的");
    assert_eq!(newest.name, "new");
    for h in homes {
        h.clear();
    }
}

/// 号那一格照那一行的家问：账号 0 叫那一家默认家目录的短形：默认那一家有本机布局、写得出短形，替身家不在注册表里、说不出 ⇒ 空串；两家叫法不同，传错家就对不上。
#[test]
fn the_account_cell_names_account_zero_after_that_rows_family() {
    use crate::accounts::quota::name_words::account_text_of;
    let (default, fake) = (
        crate::agents::default_kind(),
        crate::agents::fake::AGENT_KIND,
    );
    assert_ne!(
        account_text_of(default, "0"),
        account_text_of(fake, "0"),
        "两家账号 0 的叫法一样 ⇒ 下面分不出是不是照家问的"
    );
    for kind in [default, fake] {
        let cells = account_cells(kind, "0", &crate::common::time::Tz::default());
        assert_eq!(cells[0].1, account_text_of(kind, "0"), "{kind}");
    }
}
