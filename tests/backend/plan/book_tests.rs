//! 留上一次好的：某片读不成给旧的 ＋ 原因 ＋ 时刻；整次失败给整份旧的；摘要只随输出变；不写工作区一个字节。

use super::*;
use crate::plan::fixture::{
    as_json, dump, dump_broken, dump_top_signed, fake_pb, scratch, waits_per, who, workspace,
};
use serde_json::Value;

/// 判据用的口（生产不用，住这里不进生产树）。
impl Book {
    /// 上一次读好的那一份的摘要。
    pub(crate) fn rev(&self, workspace: &str) -> Option<String> {
        self.last(workspace).map(|d| d.rev)
    }
}

/// [`Book::ingest`]（夹具的身份表）出的那一份，线上那一形（判据按键看）。
fn ingest_json(b: &Book, ran: Ran, dir: &Path, now_ms: u64) -> Result<Value, Miss> {
    b.ingest(ran, dir, &who, now_ms).map(|r| as_json(&r))
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
    let first = ingest_json(&b, ran(&dump("/w")), dir, 1_000).unwrap();
    assert_eq!(first["slices"][0]["stale"], Value::Null);
    let second = ingest_json(&b, ran(&dump_broken("/w")), dir, 2_000).unwrap();
    let sl = &second["slices"][0];
    assert_eq!(
        sl["cells"].as_array().unwrap().len(),
        15,
        "给的是上一次好的那一份"
    );
    assert_eq!(sl["stale"]["said"], "图.md 第 3 行：元数据缺 id");
    assert_eq!(sl["stale"]["since"], 1_000);
}

#[test]
fn a_slice_never_read_well_stays_an_error() {
    let b = Book::default();
    let out = ingest_json(&b, ran(&dump_broken("/w")), Path::new("/w"), 1).unwrap();
    assert_eq!(out["slices"][0]["error"], "图.md 第 3 行：元数据缺 id");
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
        ingest_json(&b, fail(), dir, 1),
        Err(Miss::Failed { .. })
    ));
    ingest_json(&b, ran(&dump("/w")), dir, 5).unwrap();
    let out = ingest_json(&b, fail(), dir, 9).unwrap();
    assert_eq!(out["stale"]["said"], "超时");
    assert_eq!(out["stale"]["since"], 5);
    assert_eq!(out["slices"][0]["cells"].as_array().unwrap().len(), 15);
}

#[test]
fn the_rev_moves_only_with_the_output() {
    let b = Book::default();
    let dir = Path::new("/w");
    let a = ingest_json(&b, ran(&dump("/w")), dir, 1).unwrap();
    let again = ingest_json(&b, ran(&dump("/w")), dir, 2).unwrap();
    assert_eq!(a["rev"], again["rev"]);
    let mut changed = dump("/w");
    changed["repos"][0]["nodes"][1]["body"] = serde_json::json!("改了");
    let c = ingest_json(&b, ran(&changed), dir, 3).unwrap();
    assert_ne!(a["rev"], c["rev"]);
    assert_eq!(b.rev("/w"), c["rev"].as_str().map(str::to_string));
}

#[test]
fn dirs_are_remembered_both_ways() {
    let b = Book::default();
    ingest_json(&b, Ran::NotWorkspace("x".into()), Path::new("/plain"), 1).unwrap_err();
    assert_eq!(b.known_dir(Path::new("/plain")), Some(None));
    ingest_json(&b, ran(&dump("/w")), Path::new("/w/alpha"), 1).unwrap();
    assert_eq!(b.known_dir(Path::new("/w/alpha")), Some(Some("/w".into())));
    assert_eq!(b.workspaces(), vec!["/w".to_string()]);
}

/// 工作区的样子：夹具建的每个目录的修改时刻（目录里多一项、少一项都会动它）＋ 每份文件的字节。不遍历目录。
fn snapshot(root: &Path) -> Vec<(PathBuf, Option<std::time::SystemTime>, Vec<u8>)> {
    let dirs = [
        "",
        ".planned-build",
        ".planned-build/alpha",
        "repos/alpha",
        "repos/alpha/src",
    ];
    let files = [".planned-build/.env", "dump.json", "node.json"];
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

/// 真起假 pb 读一次：工作区（计划仓连工作区配置 ＋ 仓库）跑前跑后逐字节不变、目录一项没多没少。
#[test]
fn reading_a_workspace_writes_nothing_in_it() {
    let d = scratch("book-readonly");
    let entry = fake_pb(&d.join("pb"), crate::plan::locate::PLUGIN_NAME, None);
    let ws = workspace(&d.join("ws"), &dump("/synthetic"));
    let before = snapshot(&ws);
    let b = Book::default();
    let out = b
        .read(&entry, &ws, &who, &|_| false, 1, &|_, _| None)
        .unwrap();
    assert_eq!(out.slices[0].name, "alpha");
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

/// 每片带 `needs`；顶层全签收那一条的键是全签收那一次的 rev：一直全签收着、输出再变键也不变，又有没签收的、再签全了换新的。
#[test]
fn the_top_need_keeps_the_rev_it_got_signed_at_until_something_is_unsigned_again() {
    let b = Book::default();
    let dir = Path::new("/w");
    let first = ingest_json(&b, ran(&dump_top_signed("/w")), dir, 1).unwrap();
    let r1 = first["rev"].as_str().unwrap().to_string();
    assert_eq!(top_key(&first), Some(format!("top:{r1}")));
    assert!(first["slices"][0]["needs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["kind"] == "red"));
    // 还全签收着、别处变了 ⇒ 键不变。
    let mut moved = dump_top_signed("/w");
    moved["repos"][0]["nodes"][1]["body"] = serde_json::json!("改了");
    let second = ingest_json(&b, ran(&moved), dir, 2).unwrap();
    assert_ne!(second["rev"], first["rev"]);
    assert_eq!(top_key(&second), Some(format!("top:{r1}")));
    // 又有没签收的 ⇒ 没有这一条；再签全了 ⇒ 键换成那一次的 rev。
    let away = dump("/w");
    assert_eq!(top_key(&ingest_json(&b, ran(&away), dir, 3).unwrap()), None);
    let back = ingest_json(&b, ran(&dump_top_signed("/w")), dir, 4).unwrap();
    assert_eq!(
        top_key(&back),
        Some(format!("top:{r1}")),
        "输出与第一次一模一样 ⇒ 摘要也一样"
    );
    let back2 = ingest_json(&b, ran(&moved), dir, 5).unwrap();
    assert_eq!(top_key(&back2), Some(format!("top:{r1}")));
    ingest_json(&b, ran(&away), dir, 6).unwrap();
    let back3 = ingest_json(&b, ran(&moved), dir, 7).unwrap();
    let r3 = back3["rev"].as_str().unwrap().to_string();
    assert_eq!(top_key(&back3), Some(format!("top:{r3}")));
}

/// 进程刚起、本子里没记过：认可里记着的那次 rev 先用（重启不让认可过的那一条凭空再冒出来）。
#[test]
fn a_fresh_book_adopts_the_entered_rev_from_a_prior_ack() {
    let b = Book::default();
    let prior = |ws: &str, slice: &str| (ws == "/w" && slice == "alpha").then(|| "r0".to_string());
    let out = b
        .ingest_with(
            ran(&dump_top_signed("/w")),
            Path::new("/w"),
            &who,
            &|sid| waits_per(&who, sid),
            1,
            &prior,
        )
        .unwrap();
    assert_eq!(top_key(&as_json(&out)), Some("top:r0".into()));
}

/// 这一刻读不成的那一片给上一次那一份时，`needs` 也跟着是上一次那一份。
#[test]
fn a_stale_slice_keeps_its_last_needs() {
    let b = Book::default();
    let dir = Path::new("/w");
    let first = ingest_json(&b, ran(&dump("/w")), dir, 1).unwrap();
    let second = ingest_json(&b, ran(&dump_broken("/w")), dir, 2).unwrap();
    assert_eq!(second["slices"][0]["needs"], first["slices"][0]["needs"]);
    // 从没读好过的那一片：空的。
    let c = Book::default();
    let out = ingest_json(&c, ran(&dump_broken("/w")), dir, 1).unwrap();
    assert_eq!(out["slices"][0]["needs"], serde_json::json!([]));
}
