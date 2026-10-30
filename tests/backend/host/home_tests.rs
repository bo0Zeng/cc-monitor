//! 托管终端的家：同一个 id 的家里留着上一个托管进程的 `facts.json`（读不懂 · 比声明新）⇒ 占家那一下删掉，事实照写。

use super::*;

fn left_behind(tag: &str, seed: &[u8]) {
    let (own, data) = super::super::run::tests::sandbox_own(tag);
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
