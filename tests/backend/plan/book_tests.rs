//! 留上一次好的：某片读不成给旧的 ＋ 原因 ＋ 时刻；整次失败给整份旧的；摘要只随输出变；不写工作区一个字节。

use super::*;
use crate::plan::fixture::{dump, dump_broken, fake_pb, scratch, who, workspace};

/// 判据用的几个口（生产不用，住这里不进生产树）。
impl Book {
    /// [`Book::take_with`]，本子里没记过的「哪次进的那一步」就用这一次的 rev。
    pub(crate) fn take(
        &self,
        ran: Ran,
        dir: &Path,
        who: WhoPort,
        now_ms: u64,
    ) -> Result<Value, Miss> {
        self.take_with(ran, dir, who, now_ms, &|_, _| None)
    }

    /// 上一次读好的那一份的摘要。
    pub(crate) fn rev(&self, workspace: &str) -> Option<String> {
        self.last(workspace)
            .and_then(|d| d.get("rev").and_then(Value::as_str).map(str::to_string))
    }
}

fn ran(doc: &Value) -> Ran {
    let raw = doc.to_string().into_bytes();
    Ran::Dump {
        doc: doc.clone(),
        raw,
    }
}

#[test]
fn a_slice_that_cannot_be_read_now_shows_the_last_good_one_with_reason_and_time() {
    let b = Book::default();
    let dir = Path::new("/w");
    let first = b.take(ran(&dump("/w")), dir, &who, 1_000).unwrap();
    assert_eq!(first["slices"][0]["stale"], Value::Null);
    let second = b.take(ran(&dump_broken("/w")), dir, &who, 2_000).unwrap();
    let sl = &second["slices"][0];
    assert_eq!(
        sl["cells"].as_array().unwrap().len(),
        4,
        "给的是上一次好的那一份"
    );
    assert_eq!(sl["stale"]["said"], "图.md 第 3 行：元行缺 id");
    assert_eq!(sl["stale"]["since"], 1_000);
    assert_eq!(
        b.view("/w", "alpha", "A1").as_deref(),
        Some("── A1 甲功能（能力 · 没做完）\n> id: A1")
    );
}

#[test]
fn a_slice_never_read_well_stays_an_error() {
    let b = Book::default();
    let out = b
        .take(ran(&dump_broken("/w")), Path::new("/w"), &who, 1)
        .unwrap();
    assert_eq!(out["slices"][0]["error"], "图.md 第 3 行：元行缺 id");
    assert_eq!(out["slices"][0]["stale"], Value::Null);
}

#[test]
fn a_failed_run_gives_the_whole_last_good_one_or_a_miss() {
    let b = Book::default();
    let dir = Path::new("/w/alpha");
    let fail = || Ran::Failed {
        said: "超时".into(),
        raw: None,
    };
    assert!(matches!(
        b.take(fail(), dir, &who, 1),
        Err(Miss::Failed { .. })
    ));
    b.take(ran(&dump("/w")), dir, &who, 5).unwrap();
    let out = b.take(fail(), dir, &who, 9).unwrap();
    assert_eq!(out["stale"]["said"], "超时");
    assert_eq!(out["stale"]["since"], 5);
    assert_eq!(out["slices"][0]["cells"].as_array().unwrap().len(), 4);
}

#[test]
fn the_rev_moves_only_with_the_output() {
    let b = Book::default();
    let dir = Path::new("/w");
    let a = b.take(ran(&dump("/w")), dir, &who, 1).unwrap();
    let again = b.take(ran(&dump("/w")), dir, &who, 2).unwrap();
    assert_eq!(a["rev"], again["rev"]);
    let mut changed = dump("/w");
    changed["slices"][0]["cells"][1]["status"] = serde_json::json!("没做完");
    let c = b.take(ran(&changed), dir, &who, 3).unwrap();
    assert_ne!(a["rev"], c["rev"]);
    assert_eq!(b.rev("/w"), c["rev"].as_str().map(str::to_string));
}

#[test]
fn dirs_are_remembered_both_ways() {
    let b = Book::default();
    b.take(Ran::NotWorkspace("x".into()), Path::new("/plain"), &who, 1)
        .unwrap_err();
    assert_eq!(b.known_dir(Path::new("/plain")), Some(None));
    b.take(ran(&dump("/w")), Path::new("/w/alpha"), &who, 1)
        .unwrap();
    assert_eq!(b.known_dir(Path::new("/w/alpha")), Some(Some("/w".into())));
    assert_eq!(b.workspaces(), vec!["/w".to_string()]);
}

/// 工作区的样子：夹具建的每个目录的修改时刻（目录里多一项、少一项都会动它）＋ 每份文件的字节。不遍历目录。
fn snapshot(root: &Path) -> Vec<(PathBuf, Option<std::time::SystemTime>, Vec<u8>)> {
    let dirs = [
        "",
        ".planned-build",
        ".planned-build/alpha",
        "alpha",
        "alpha/src",
    ];
    let files = [".env", "dump.json"];
    let mut out: Vec<_> = dirs
        .iter()
        .map(|d| {
            let p = root.join(d);
            let m = std::fs::metadata(&p).unwrap().modified().ok();
            (p, m, Vec::new())
        })
        .collect();
    out.extend(files.iter().map(|f| {
        let p = root.join(f);
        let m = std::fs::metadata(&p).unwrap().modified().ok();
        let b = std::fs::read(&p).unwrap();
        (p, m, b)
    }));
    out
}

/// 真起假 pb 读一次：工作区（计划仓 ＋ `.env` ＋ 仓库）跑前跑后逐字节不变、目录一项没多没少。
#[test]
fn reading_a_workspace_writes_nothing_in_it() {
    let d = scratch("book-readonly");
    let entry = fake_pb(&d.join("pb"), crate::plan::locate::PLUGIN_NAME, None);
    let ws = workspace(&d.join("ws"), &dump("/synthetic"));
    let before = snapshot(&ws);
    let b = Book::default();
    let out = b.read(&entry, &ws, &who, 1, &|_, _| None).unwrap();
    assert_eq!(out["slices"][0]["name"], "alpha");
    assert_eq!(snapshot(&ws), before);
}

fn top_key(out: &Value) -> Option<String> {
    out["slices"][0]["needs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["kind"] == "top")
        .map(|n| n["key"].as_str().unwrap().to_string())
}

/// 每片带 `needs`；顶块看全局那一条的键是进那一步那次的 rev：在那一步上输出再变键也不变，离开再回来换新的。
#[test]
fn the_top_need_keeps_the_rev_it_entered_at_until_the_top_block_leaves_the_step() {
    let b = Book::default();
    let dir = Path::new("/w");
    let first = b.take(ran(&dump("/w")), dir, &who, 1).unwrap();
    let r1 = first["rev"].as_str().unwrap().to_string();
    assert_eq!(top_key(&first), Some(format!("top:{r1}")));
    assert!(first["slices"][0]["needs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["kind"] == "red"));
    // 还在那一步、别处变了 ⇒ 键不变。
    let mut moved = dump("/w");
    moved["slices"][0]["cells"][1]["status"] = serde_json::json!("没做完");
    let second = b.take(ran(&moved), dir, &who, 2).unwrap();
    assert_ne!(second["rev"], first["rev"]);
    assert_eq!(top_key(&second), Some(format!("top:{r1}")));
    // 离开那一步 ⇒ 没有这一条；再回来 ⇒ 键换成回来那次的 rev。
    let mut away = dump("/w");
    away["slices"][0]["blocks"][0]["phase"] = serde_json::json!("定架构");
    assert_eq!(top_key(&b.take(ran(&away), dir, &who, 3).unwrap()), None);
    let back = b.take(ran(&dump("/w")), dir, &who, 4).unwrap();
    assert_eq!(
        top_key(&back),
        Some(format!("top:{r1}")),
        "输出与第一次一模一样 ⇒ 摘要也一样"
    );
    let back2 = b.take(ran(&moved), dir, &who, 5).unwrap();
    assert_eq!(top_key(&back2), Some(format!("top:{r1}")));
    b.take(ran(&away), dir, &who, 6).unwrap();
    let back3 = b.take(ran(&moved), dir, &who, 7).unwrap();
    let r3 = back3["rev"].as_str().unwrap().to_string();
    assert_eq!(top_key(&back3), Some(format!("top:{r3}")));
}

/// 进程刚起、本子里没记过：认可里记着的那次 rev 先用（重启不让认可过的那一条凭空再冒出来）。
#[test]
fn a_fresh_book_adopts_the_entered_rev_from_a_prior_ack() {
    let b = Book::default();
    let prior = |ws: &str, slice: &str| (ws == "/w" && slice == "alpha").then(|| "r0".to_string());
    let out = b
        .take_with(ran(&dump("/w")), Path::new("/w"), &who, 1, &prior)
        .unwrap();
    assert_eq!(top_key(&out), Some("top:r0".into()));
}

/// 这一刻读不成的那一片给上一次那一份时，`needs` 也跟着是上一次那一份。
#[test]
fn a_stale_slice_keeps_its_last_needs() {
    let b = Book::default();
    let dir = Path::new("/w");
    let first = b.take(ran(&dump("/w")), dir, &who, 1).unwrap();
    let second = b.take(ran(&dump_broken("/w")), dir, &who, 2).unwrap();
    assert_eq!(second["slices"][0]["needs"], first["slices"][0]["needs"]);
    // 从没读好过的那一片：空的。
    let c = Book::default();
    let out = c.take(ran(&dump_broken("/w")), dir, &who, 1).unwrap();
    assert_eq!(out["slices"][0]["needs"], serde_json::json!([]));
}
