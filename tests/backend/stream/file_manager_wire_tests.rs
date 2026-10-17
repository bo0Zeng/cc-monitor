//! 文件管理那一族换成类型前后，文件窗口读的回包线上字节不变。
//!
//! 换形之前用旧代码跑出的字节存在 `tests/__fixtures__/files.wire-bytes.txt`（每行 `名字<TAB>字节`），之后逐字节比。
//! 都经登记表跑（同分派那一层），回包经 `serde_json::Value` 那一跳（同帧面）。没有重写开关：字节变了就是线上形状变了。
//! 夹具是沙箱里现铺的：沙箱根换成 `<ROOT>`（原始字节那一形换成 `<ROOTHEX>`），属主名换成 `<OWNER>`，时刻钉在 2001 年。
//! 不钉的几条：find · index-status · index-rebuild · browse 读的是进程里那一份索引和监听名单（别的判据同时在动），
//! 那几条的形状由类型和指纹表管。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::wire::spec::{invoke, Prepared};
use crate::wire::Request;

/// 走登记表跑一条（阻塞档当场跑，异步档在当场起的运行时上等）。
fn run(cmd: &str, args: Value) -> Result<Value, String> {
    let spec =
        crate::stream::inbound::lookup(cmd).unwrap_or_else(|| panic!("`{cmd}` 不在登记表里"));
    let req = Request {
        id: "t".to_string(),
        cmd: cmd.to_string(),
        args,
        within_ms: None,
        view: Value::Null,
        tz: Default::default(),
        until: None,
    };
    let got = match invoke(
        spec.run,
        &crate::stream::inbound::hands(Some(&crate::platform::fs::Own::for_tests())),
    ) {
        Some(Prepared::Blocking(f)) => f(req),
        Some(Prepared::Async(f)) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(f(req)),
        None => panic!("`{cmd}` 是内建档"),
    };
    got.map(|v| v.unwrap_or(Value::Null))
        .map_err(|f| format!("{}: {}", f.code, f.message))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// 沙箱根 ⇒ 占位；属主名 ⇒ 占位。
fn scrub(v: &mut Value, root: &str) {
    match v {
        Value::String(s) => {
            *s = s
                .replace(root, "<ROOT>")
                .replace(&hex(root.as_bytes()), "<ROOTHEX>");
        }
        Value::Array(a) => a.iter_mut().for_each(|x| scrub(x, root)),
        Value::Object(m) => {
            // 目录的大小看文件系统（tmpfs 与 ext4 不同），不是契约。
            if m.get("kind") == Some(&json!("dir")) || m.get("link_to") == Some(&json!("dir")) {
                if m.contains_key("size") {
                    m.insert("size".to_string(), json!("<DIRSIZE>"));
                }
            }
            if let Some(o) = m.get_mut("owner") {
                if o.is_string() {
                    *o = json!("<OWNER>");
                }
            }
            // 面包屑在夹具根之上那几段看临时目录落在哪（进程号也在里面），不是契约：只留根及以下那几段。
            if let Some(Value::Array(c)) = m.get_mut("crumbs") {
                c.retain(|seg| {
                    seg["path"]
                        .as_str()
                        .is_some_and(|p| p.starts_with(root) && p != root)
                });
            }
            m.values_mut().for_each(|x| scrub(x, root));
        }
        _ => {}
    }
}

const PINNED_AT: u64 = 1_000_000_000;

fn pin_time(p: &Path) {
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(PINNED_AT);
    let f = std::fs::File::options()
        .read(true)
        .open(p)
        .expect("开来设时刻");
    f.set_modified(t).expect("设时刻");
}

#[cfg(unix)]
fn plant() -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("ccm-files-wire-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    // 每个目录只放一项：目录项的次序不是契约，字节才稳。
    for d in [
        "one-file",
        "one-link-dir",
        "one-link-gone",
        "one-odd",
        "size/d",
    ] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    std::fs::create_dir_all(root.join("target")).unwrap();
    std::fs::write(root.join("one-file/a.txt"), "hello\nwire\n").unwrap();
    std::fs::set_permissions(
        root.join("one-file/a.txt"),
        std::fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    pin_time(&root.join("one-file/a.txt"));
    std::os::unix::fs::symlink(root.join("target"), root.join("one-link-dir/to-dir")).unwrap();
    std::fs::set_permissions(root.join("target"), std::fs::Permissions::from_mode(0o750)).unwrap();
    pin_time(&root.join("target"));
    std::os::unix::fs::symlink(root.join("nowhere"), root.join("one-link-gone/gone")).unwrap();
    let odd = std::ffi::OsStr::from_bytes(b"odd-\xff.bin");
    std::fs::write(root.join("one-odd").join(odd), b"xy").unwrap();
    std::fs::set_permissions(
        root.join("one-odd").join(odd),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    pin_time(&root.join("one-odd").join(odd));
    std::fs::write(root.join("size/d/b.bin"), b"12345").unwrap();
    std::fs::write(root.join("size/a.bin"), b"abc").unwrap();
    root
}

#[cfg(unix)]
fn cases(root: &Path) -> Vec<(&'static str, &'static str, Value)> {
    let r = |s: &str| root.join(s).to_string_lossy().to_string();
    let odd = {
        let mut b = root.join("one-odd").into_os_string().into_encoded_bytes();
        b.extend_from_slice(b"/odd-\xff.bin");
        json!({ "b16": hex(&b) })
    };
    vec![
        (
            "ls-file",
            "files-ls",
            json!({ "path": r("one-file"), "limit": 10 }),
        ),
        (
            "ls-link-dir",
            "files-ls",
            json!({ "path": r("one-link-dir") }),
        ),
        (
            "ls-link-gone",
            "files-ls",
            json!({ "path": r("one-link-gone") }),
        ),
        ("ls-odd", "files-ls", json!({ "path": r("one-odd") })),
        (
            "stat-file",
            "files-stat",
            json!({ "path": r("one-file/a.txt") }),
        ),
        (
            "stat-link",
            "files-stat",
            json!({ "path": r("one-link-dir/to-dir") }),
        ),
        ("stat-odd", "files-stat", json!({ "path": odd.clone() })),
        (
            "read-text",
            "files-read-text",
            json!({ "path": r("one-file/a.txt"), "max_bytes": 1024 }),
        ),
        (
            "read-chunk",
            "files-read-chunk",
            json!({ "path": odd.clone(), "offset": 1, "len": 8 }),
        ),
        (
            "read-chunk-text",
            "files-read-chunk",
            json!({ "path": r("one-file/a.txt"), "offset": 0, "len": 5 }),
        ),
        ("size", "files-size", json!({ "path": r("size") })),
        ("home", "files-home", json!({})),
        (
            "grep",
            "files-grep",
            json!({ "path": r("one-file"), "needle": "wire", "limit": 5 }),
        ),
        (
            "grep-case",
            "files-grep",
            json!({ "path": r("one-file"), "needle": "HELLO", "ignore_ascii_case": true }),
        ),
    ]
}

fn pinned() -> BTreeMap<String, String> {
    let p = crate::guard_support::repo_root().join("tests/__fixtures__/files.wire-bytes.txt");
    std::fs::read_to_string(&p)
        .unwrap_or_default()
        .lines()
        .map(|l| {
            let (n, b) = l.split_once('\t').expect("名字<TAB>字节");
            (n.to_string(), b.to_string())
        })
        .collect()
}

/// ★ 文件窗口读的那几条：回包字节与换形之前那一版逐字节相同。
#[test]
#[cfg(unix)]
fn the_replies_the_file_window_reads_keep_their_wire_bytes() {
    let root = plant();
    let home = crate::platform::paths::home_dir().expect("沙箱家");
    let root_s = root.to_string_lossy().to_string();
    let mut got = BTreeMap::new();
    for (name, cmd, args) in cases(&root) {
        let mut v = run(cmd, args).unwrap_or_else(|e| panic!("{name}（{cmd}）没答出来：{e}"));
        if name == "home" {
            v = json!({ "path": if v["path"] == json!(home.to_string_lossy()) { json!("<HOME>") } else { v["path"].clone() } });
        }
        scrub(&mut v, &root_s);
        got.insert(name.to_string(), serde_json::to_string(&v).unwrap());
    }
    std::fs::remove_dir_all(&root).ok();
    let want = pinned();
    let dump: String = got.iter().map(|(n, b)| format!("{n}\t{b}\n")).collect();
    assert_eq!(
        want, got,
        "文件管理那几条的线上字节变了（文件窗口读的那一形）。这一版写出来的是：\n{dump}"
    );
}

/// 登记那一行的名字与回包类型，跟文件窗口照着发的那张表（`cmd_wire::files::Call`）是同一份：名字抄错一处就红。
#[test]
fn the_registered_names_are_the_ones_the_file_window_calls() {
    use crate::wire::typed::Command;
    use cmd_wire::files::Call;
    fn same<C: Command>()
    where
        C::Req: Call<Reply = C::Reply>,
    {
        assert_eq!(C::NAME, <C::Req as Call>::NAME);
        assert!(
            crate::stream::inbound::lookup(C::NAME).is_some(),
            "`{}` 不在登记表里",
            C::NAME
        );
    }
    use crate::stream::inbound::files_face::*;
    same::<FilesLs>();
    same::<FilesStat>();
    same::<FilesFind>();
    same::<FilesIndexStatus>();
    same::<FilesIndexRebuild>();
    same::<FilesBrowse>();
    same::<FilesReadText>();
    same::<FilesReadChunk>();
    same::<FilesSize>();
    same::<FilesHome>();
    same::<FilesGrep>();
}

// ── 写的那几条 ─────────────────────────────────────

/// 写面那几条的回包换形之前那一版的字节（每行 `名字<TAB>字节`），同读的那一份的写法。
fn pinned_writes() -> BTreeMap<String, String> {
    let p = crate::guard_support::repo_root().join("tests/__fixtures__/files-write.wire-bytes.txt");
    std::fs::read_to_string(&p)
        .unwrap_or_default()
        .lines()
        .map(|l| {
            let (n, b) = l.split_once('\t').expect("名字<TAB>字节");
            (n.to_string(), b.to_string())
        })
        .collect()
}

#[cfg(unix)]
fn tar_of(items: &[(&str, &[u8])]) -> Vec<u8> {
    let mut b = tar::Builder::new(Vec::new());
    for (name, body) in items {
        let mut h = tar::Header::new_gnu();
        h.set_path(name).unwrap();
        h.set_size(body.len() as u64);
        h.set_mode(0o644);
        h.set_mtime(PINNED_AT);
        h.set_entry_type(tar::EntryType::Regular);
        h.set_cksum();
        b.append(&h, *body).unwrap();
    }
    b.into_inner().unwrap()
}

/// ★ 文件窗口读的写面回包（写面 · 上传提交 · 解压）：回包字节与换形之前那一版逐字节相同。
/// 一趟按次序跑（写会改盘）；删会话那一条要铺一份会话记录，回包只有一格 `path`，不钉（形状由类型和指纹表管）。
#[test]
#[cfg(unix)]
fn the_write_replies_the_file_window_reads_keep_their_wire_bytes() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("ccm-files-wire-w-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(root.join("tree/sub")).unwrap();
    std::fs::write(root.join("tree/x.txt"), b"xx").unwrap();
    std::fs::write(root.join("tree/sub/y.txt"), b"yyy").unwrap();
    std::fs::write(root.join("ct.txt"), b"old").unwrap();
    std::fs::write(
        root.join("a.tar"),
        tar_of(&[("a/one.txt", b"1"), ("a/two.txt", b"22")]),
    )
    .unwrap();
    std::fs::set_permissions(root.join("ct.txt"), std::fs::Permissions::from_mode(0o644)).unwrap();
    let r = root.to_string_lossy().to_string();
    let sha = |b: &[u8]| json!({ "sha256": crate::files::content_sha256(b) });
    let odd = json!({ "b16": hex(b"odd-\xff.txt") });
    let (k1, k2) = (
        "0123456789abcdef0123456789abcdef",
        "fedcba9876543210fedcba9876543210",
    );
    let steps: Vec<(&str, &str, Value)> = vec![
        (
            "create",
            "files-create",
            json!({ "root": r, "rel": "made.txt", "content": "abc" }),
        ),
        (
            "create-odd",
            "files-create",
            json!({ "root": r, "rel": odd }),
        ),
        (
            "mkdir",
            "files-mkdir",
            json!({ "root": r, "rel": "dir1", "single": true }),
        ),
        (
            "rename",
            "files-rename",
            json!({ "root": r, "from": "made.txt", "to": "moved.txt" }),
        ),
        (
            "copy",
            "files-copy",
            json!({ "root": r, "items": [{ "from": "moved.txt", "to": "copied.txt" }] }),
        ),
        (
            "copy-tree",
            "files-copy",
            json!({ "root": r, "items": [{ "from": "tree", "to": "tree2" }] }),
        ),
        (
            "chmod-first",
            "files-chmod",
            json!({ "root": r, "items": [{ "rel": "copied.txt" }], "mode": 384 }),
        ),
        (
            "chmod",
            "files-chmod",
            json!({ "root": r, "items": [{ "rel": "copied.txt" }], "mode": 416 }),
        ),
        (
            "write-text",
            "files-write-text",
            json!({ "root": r, "rel": "moved.txt", "content": "abcd", "expect": sha(b"abc") }),
        ),
        (
            "delete",
            "files-delete",
            json!({ "root": r, "items": [{ "rel": "copied.txt" }] }),
        ),
        (
            "delete-expect",
            "files-delete",
            json!({ "root": r, "items": [{ "rel": "moved.txt", "expect": "abcd" }] }),
        ),
        (
            "delete-empty-dir",
            "files-delete",
            json!({ "root": r, "items": [{ "rel": "dir1", "expect": { "empty_dir": true } }] }),
        ),
        (
            "delete-tree",
            "files-delete",
            json!({ "root": r, "items": [{ "rel": "tree2", "recursive": true }] }),
        ),
        (
            "extract",
            "files-extract",
            json!({ "root": r, "rel": "a.tar" }),
        ),
        (
            "extract-fresh",
            "files-extract",
            json!({ "root": r, "rel": "a.tar", "fresh": true }),
        ),
        (
            "stage-chunk",
            "files-stage-chunk",
            json!({ "key": k1, "seq": 0, "content": "hello" }),
        ),
        (
            "commit-text",
            "files-commit-text",
            json!({ "root": r, "rel": "ct.txt", "key": k1, "chunks": 1, "bytes": 5, "expect": sha(b"old") }),
        ),
        (
            "stage-chunk-b16",
            "files-stage-chunk",
            json!({ "key": k2, "seq": 0, "content": { "b16": "0001" } }),
        ),
        (
            "stage-chunk-2",
            "files-stage-chunk",
            json!({ "key": k2, "seq": 1, "content": "zz" }),
        ),
        (
            "commit-upload",
            "files-commit-upload",
            json!({ "root": r, "rel": "up.bin", "key": k2, "expect": sha(b"\x00\x01zz"), "chunks": 2, "bytes": 4 }),
        ),
    ];
    let mut got = BTreeMap::new();
    for (name, cmd, args) in steps {
        let mut v = run(cmd, args).unwrap_or_else(|e| panic!("{name}（{cmd}）没答出来：{e}"));
        if let Some(b) = v.get_mut("backup") {
            if !b.is_null() {
                *b = json!("<BACKUP>");
            }
        }
        // 第一次改权限位：改之前那一格跟这台的 umask 走，不钉（只为让下一趟的 `before` 定下来）。
        if name == "chmod-first" {
            continue;
        }
        scrub(&mut v, &r);
        got.insert(name.to_string(), serde_json::to_string(&v).unwrap());
    }
    std::fs::remove_dir_all(&root).ok();
    let want = pinned_writes();
    let dump: String = got.iter().map(|(n, b)| format!("{n}\t{b}\n")).collect();
    assert_eq!(
        want, got,
        "文件管理写面那几条的线上字节变了（文件窗口读的那一形）。这一版写出来的是：\n{dump}"
    );
}
