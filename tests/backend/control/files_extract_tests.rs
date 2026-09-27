//! 〔FILES2 · 第四波 · 2026-09-27〕`control/files_link.rs` 的行为判据。
//! 要求住址：`设计/60 §6.2` · `§7` 第 9 条 Q1；主会话 09-27 裁「复制**链接本身**（目标文本原样）」——
//! 建链接那一下只住这里，链接自己那条路径先过路径解析、目标文本不解不判。

use super::*;

fn temp_root(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-fl-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&p).ok();
    std::fs::create_dir_all(&p).expect("建临时根");
    p
}

/// 建成：目标文本逐字节原样（相对 · 根外绝对都原样）；链接自己的路径越根 ⇒ `refused`、根外一个字节没多；已在 ⇒ `io_failed`、已在的那份没动。
#[test]
#[cfg(unix)]
fn a_link_lands_with_its_target_text_verbatim_and_only_under_the_root() {
    let base = temp_root("land");
    let root = base.join("r");
    std::fs::create_dir(&root).expect("根");
    for (rel, to) in [("rel", "../x/y"), ("abs", "/etc/passwd")] {
        let at = land_link(&root, Path::new(rel), Path::new(to)).expect("建链接被拒");
        assert_eq!(at, root.canonicalize().expect("根").join(rel));
        assert_eq!(
            std::fs::read_link(root.join(rel)).expect("读"),
            Path::new(to)
        );
    }
    let e = land_link(&root, Path::new("../esc"), Path::new("x")).expect_err("越根的链接建成了");
    assert_eq!(e.code(), "refused", "{e:?}");
    assert!(
        std::fs::symlink_metadata(base.join("esc")).is_err(),
        "根外多了一条"
    );
    std::fs::write(root.join("keep"), b"K").expect("铺");
    let e = land_link(&root, Path::new("keep"), Path::new("x")).expect_err("盖掉了已在的");
    assert_eq!(e.code(), "io_failed", "{e:?}");
    assert_eq!(std::fs::read(root.join("keep")).expect("keep"), b"K");
    std::fs::remove_dir_all(&base).ok();
}

// ════════════════════════════════════════════════════════════════════════════
//  解压（Q3）。要求住址：`设计/60 §6.2` · `§7` 第 9 条 Q3；主会话 09-27 裁「每一条先过路径解析（挡 zip-slip：`..` / 绝对路径 /
//  链接出根），撞名就问；支持 zip · tar · tar.gz · tgz，其余格式说『不认这种包』」。
// ════════════════════════════════════════════════════════════════════════════

/// 一条手铺的 tar 条目：名字原样写进头（绕开 `Builder` 自己的路径检查 —— 坏包就是要这样的名字）。
enum TarItem<'a> {
    Dir(&'a [u8]),
    File(&'a [u8], &'a [u8], u32),
    Sym(&'a [u8], &'a [u8]),
    Hard(&'a [u8], &'a [u8]),
    Fifo(&'a [u8]),
}

fn tar_bytes(items: &[TarItem<'_>]) -> Vec<u8> {
    let mut b = tar::Builder::new(Vec::new());
    for it in items {
        let mut h = tar::Header::new_gnu();
        let (name, kind, body, mode, link): (&[u8], tar::EntryType, &[u8], u32, Option<&[u8]>) =
            match it {
                TarItem::Dir(n) => (n, tar::EntryType::Directory, b"", 0o755, None),
                TarItem::File(n, body, m) => (n, tar::EntryType::Regular, body, *m, None),
                TarItem::Sym(n, to) => (n, tar::EntryType::Symlink, b"", 0o777, Some(to)),
                TarItem::Hard(n, to) => (n, tar::EntryType::Link, b"", 0o644, Some(to)),
                TarItem::Fifo(n) => (n, tar::EntryType::Fifo, b"", 0o644, None),
            };
        {
            let g = h.as_gnu_mut().expect("gnu 头");
            g.name[..name.len()].copy_from_slice(name);
            if let Some(to) = link {
                g.linkname[..to.len()].copy_from_slice(to);
            }
        }
        h.set_entry_type(kind);
        h.set_mode(mode);
        h.set_size(body.len() as u64);
        h.set_cksum();
        b.append(&h, body).expect("写 tar 条目");
    }
    b.into_inner().expect("tar 收尾")
}

fn gz(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write as _;
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(bytes).expect("gz");
    e.finish().expect("gz 收尾")
}

/// 一个 zip：`(名字, Some(正文) | None=目录, 链接目标)`，一律 Stored（测试不依赖压缩那一支）。
fn zip_bytes(items: &[(&str, Option<&[u8]>, Option<&str>)]) -> Vec<u8> {
    use std::io::Write as _;
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let o = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o640);
    for (name, body, link) in items {
        match (body, link) {
            (_, Some(to)) => w.add_symlink(*name, *to, o).expect("zip 链接"),
            (Some(b), None) => {
                w.start_file(*name, o).expect("zip 文件");
                w.write_all(b).expect("zip 写");
            }
            (None, None) => w.add_directory(*name, o).expect("zip 目录"),
        }
    }
    w.finish().expect("zip 收尾").into_inner()
}

fn plant(dir: &Path, name: &str, bytes: &[u8]) {
    std::fs::write(dir.join(name), bytes).expect("铺包");
}

/// ★ tar.gz 整包解对：目录 · 文件（逐字节 ＋ rwx 位）· 包里的链接（照原样）· 硬链接（落成拷贝）· `./` 前缀剥掉 · 没写出来的上级补上；
/// 应答的几个数与手算相等；`.tgz` 同一条路。
#[test]
#[cfg(unix)]
fn a_tar_gz_lands_whole_with_bytes_modes_links_and_implied_parents() {
    use std::os::unix::fs::PermissionsExt as _;
    let base = temp_root("tgz");
    let body = tar_bytes(&[
        TarItem::Dir(b"./top/"),
        TarItem::File(b"./top/a.txt", b"hello", 0o751),
        TarItem::File(b"top/deep/er/b.bin", b"\x00\x01\x02", 0o600),
        TarItem::Sym(b"top/ln", b"a.txt"),
        TarItem::Hard(b"top/hard", b"top/a.txt"),
    ]);
    for name in ["p.tar.gz", "q.TGZ"] {
        plant(&base, name, &gz(&body));
        let into = format!("{name}.out");
        let got = extract(&base, Path::new(name), Path::new(&into)).expect("干净的包被拒了");
        assert_eq!(
            (got.files, got.dirs, got.links, got.bytes),
            (3, 3, 1, 5 + 3 + 5),
            "条数 / 字节数与手算不等"
        );
        let d = base.join(&into).join("top");
        assert_eq!(std::fs::read(d.join("a.txt")).expect("a"), b"hello");
        assert_eq!(
            std::fs::read(d.join("deep/er/b.bin")).expect("b"),
            b"\x00\x01\x02"
        );
        assert_eq!(std::fs::read(d.join("hard")).expect("hard"), b"hello");
        let mode = |p: &Path| std::fs::metadata(p).expect("md").permissions().mode() & 0o777;
        assert_eq!(mode(&d.join("a.txt")), 0o751);
        assert_eq!(mode(&d.join("deep/er/b.bin")), 0o600);
        assert_eq!(
            std::fs::read_link(d.join("ln")).expect("ln"),
            Path::new("a.txt")
        );
    }
    std::fs::remove_dir_all(&base).ok();
}

/// ★ zip 整包解对（含目录条目 · 没写出来的上级 · 链接 · 反斜杠分隔）。
#[test]
#[cfg(unix)]
fn a_zip_lands_whole() {
    let base = temp_root("zip");
    plant(
        &base,
        "z.zip",
        &zip_bytes(&[
            ("d/", None, None),
            ("d/x.txt", Some(b"X"), None),
            ("e\\f\\y.txt", Some(b"YY"), None),
            ("d/ln", None, Some("x.txt")),
        ]),
    );
    let got = extract(&base, Path::new("z.zip"), Path::new("z")).expect("干净的 zip 被拒了");
    assert_eq!((got.files, got.dirs, got.links, got.bytes), (2, 3, 1, 3));
    assert_eq!(std::fs::read(base.join("z/d/x.txt")).expect("x"), b"X");
    assert_eq!(std::fs::read(base.join("z/e/f/y.txt")).expect("y"), b"YY");
    assert_eq!(
        std::fs::read_link(base.join("z/d/ln")).expect("ln"),
        Path::new("x.txt")
    );
    std::fs::remove_dir_all(&base).ok();
}

/// ★★ zip-slip 与别的坏形：每一形**整趟拒、一个字节都没建**（落点目录也不在）、根外一个字节没多。
#[test]
#[cfg(unix)]
fn every_escaping_or_special_entry_refuses_the_whole_archive_and_builds_nothing() {
    let base = temp_root("slip");
    let root = base.join("r");
    std::fs::create_dir(&root).expect("根");
    let tars: Vec<(&str, Vec<u8>)> = vec![
        (
            "up",
            tar_bytes(&[
                TarItem::File(b"ok.txt", b"1", 0o644),
                TarItem::File(b"../evil", b"E", 0o644),
            ]),
        ),
        (
            "mid-up",
            tar_bytes(&[TarItem::File(b"a/../../evil", b"E", 0o644)]),
        ),
        (
            "abs",
            tar_bytes(&[TarItem::File(b"/tmp/evil", b"E", 0o644)]),
        ),
        (
            "link-out",
            tar_bytes(&[TarItem::Sym(b"a/ln", b"../../evil")]),
        ),
        ("link-abs", tar_bytes(&[TarItem::Sym(b"ln", b"/etc")])),
        (
            "under-link",
            tar_bytes(&[
                TarItem::Sym(b"ln", b"sub"),
                TarItem::Dir(b"sub/"),
                TarItem::File(b"ln/f", b"E", 0o644),
            ]),
        ),
        (
            "hard-out",
            tar_bytes(&[TarItem::Hard(b"h", b"../../etc/passwd")]),
        ),
        ("fifo", tar_bytes(&[TarItem::Fifo(b"p")])),
        (
            "twice",
            tar_bytes(&[
                TarItem::File(b"f", b"1", 0o644),
                TarItem::File(b"f", b"2", 0o644),
            ]),
        ),
    ];
    for (tag, bytes) in &tars {
        let name = format!("{tag}.tar");
        plant(&root, &name, bytes);
        let e = extract(&root, Path::new(&name), Path::new(tag))
            .expect_err(&format!("{tag}：坏包解成了"));
        assert_eq!(e.0, "refused", "{tag}：{e:?}");
        assert!(
            std::fs::symlink_metadata(root.join(tag)).is_err(),
            "{tag}：落点目录被建了"
        );
    }
    plant(
        &root,
        "bs.zip",
        &zip_bytes(&[("..\\evil", Some(b"E"), None)]),
    );
    let e = extract(&root, Path::new("bs.zip"), Path::new("bs")).expect_err("zip 反斜杠上跳解成了");
    assert_eq!(e.0, "refused", "{e:?}");
    assert!(
        std::fs::symlink_metadata(base.join("evil")).is_err(),
        "根外多了东西"
    );
    assert!(
        std::fs::symlink_metadata(root.join("evil")).is_err(),
        "根里多了东西"
    );
    // 正控：同一个判定放行真实的形（相对链接留在落点里、`./` 起头）。
    plant(
        &root,
        "good.tar",
        &tar_bytes(&[
            TarItem::Dir(b"./a/"),
            TarItem::Sym(b"a/ln", b"../b"),
            TarItem::File(b"b", b"B", 0o644),
        ]),
    );
    extract(&root, Path::new("good.tar"), Path::new("good")).expect("真实形被拒了");
    std::fs::remove_dir_all(&base).ok();
}

/// 落点已在 ⇒ `exists`、已在那一份一个字节不动；认不得的后缀 ⇒ `unsupported`；超条目上限 ⇒ 整趟拒。
#[test]
fn the_three_whole_refusals_touch_nothing() {
    let base = temp_root("refuse");
    plant(
        &base,
        "p.tar",
        &tar_bytes(&[
            TarItem::File(b"f", b"1", 0o644),
            TarItem::File(b"g", b"2", 0o644),
        ]),
    );
    std::fs::create_dir(base.join("p")).expect("铺已在");
    std::fs::write(base.join("p/keep"), b"K").expect("铺");
    let e = extract(&base, Path::new("p.tar"), Path::new("p")).expect_err("落点已在却解进去了");
    assert_eq!(e.0, "exists", "{e:?}");
    assert_eq!(std::fs::read(base.join("p/keep")).expect("keep"), b"K");
    assert!(
        std::fs::symlink_metadata(base.join("p/f")).is_err(),
        "合并进去了"
    );
    plant(&base, "p.rar", b"Rar!");
    let e = extract(&base, Path::new("p.rar"), Path::new("r")).expect_err("rar 解成了");
    assert_eq!(e.0, "unsupported", "{e:?}");
    let e = extract_with(&base, Path::new("p.tar"), Path::new("c"), 1).expect_err("超上限却解了");
    assert_eq!(e.0, "refused", "{e:?}");
    assert!(std::fs::symlink_metadata(base.join("c")).is_err());
    extract_with(&base, Path::new("p.tar"), Path::new("c"), 2).expect("上限恰好够却被拒了");
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 执行趟中途失败 ⇒ 这一趟建的**全撤掉**（含落点目录）。注入：zip 第二份的正文改坏一个字节（计划只读头，读到正文才失败）。
#[test]
fn an_extract_that_fails_midway_undoes_everything_it_built() {
    let base = temp_root("undo");
    let mut z = zip_bytes(&[
        ("a/first.txt", Some(b"FIRST-CONTENT"), None),
        ("a/second.txt", Some(b"SECOND-CONTENT"), None),
    ]);
    let at = z
        .windows(14)
        .position(|w| w == b"SECOND-CONTENT")
        .expect("找到正文");
    z[at] ^= 0x20;
    plant(&base, "bad.zip", &z);
    let e = extract(&base, Path::new("bad.zip"), Path::new("bad")).expect_err("坏正文却解成了");
    assert_eq!(e.0, "io_failed", "{e:?}");
    assert!(e.1.contains("都撤掉了"), "没说回滚：{}", e.1);
    assert!(
        std::fs::symlink_metadata(base.join("bad")).is_err(),
        "🔴 半截解压留在了盘上"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// 命令面：回的键 == 声明的 `fields`；路径参数收 b16。
#[test]
fn the_command_face_answers_with_the_declared_fields() {
    let base = temp_root("face");
    plant(
        &base,
        "p.tar",
        &tar_bytes(&[TarItem::File(b"f", b"1", 0o644)]),
    );
    let v = answer_wire(
        "files-extract",
        &serde_json::json!({"root": base.to_string_lossy(), "rel": {"b16": "702e746172"}}),
    )
    .expect("解压被拒");
    let got: std::collections::BTreeSet<&str> = v
        .as_object()
        .expect("对象")
        .keys()
        .map(String::as_str)
        .collect();
    let declared: std::collections::BTreeSet<&str> =
        EXTRACT_COMMANDS[0].fields.iter().copied().collect();
    assert_eq!(got, declared);
    assert_eq!(std::fs::read(base.join("p/f")).expect("f"), b"1");
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 撞名就问（线上那一形）：落点「包名去后缀」已在 ⇒ `exists`、一个字节不动；问过之后 `fresh: true` ⇒ 落到第一个不在的 `名 (n)`。
#[test]
fn a_taken_name_answers_exists_and_fresh_takes_the_first_free_number() {
    let base = temp_root("fresh");
    plant(
        &base,
        "p.tar",
        &tar_bytes(&[TarItem::File(b"f", b"1", 0o644)]),
    );
    std::fs::create_dir(base.join("p")).expect("铺已在");
    std::fs::create_dir(base.join("p (2)")).expect("铺已在 2");
    let e = extract_here(&base, Path::new("p.tar"), false).expect_err("撞名却没问");
    assert_eq!(e.0, "exists", "{e:?}");
    assert!(
        std::fs::symlink_metadata(base.join("p/f")).is_err(),
        "解进了已在的目录"
    );
    let got = extract_here(&base, Path::new("p.tar"), true).expect("问过之后仍被拒");
    assert_eq!(got.path, base.canonicalize().expect("根").join("p (3)"));
    assert_eq!(std::fs::read(base.join("p (3)/f")).expect("f"), b"1");
    std::fs::remove_dir_all(&base).ok();
}

/// zip 里一条链接的目标文本超过 `LINK_TARGET_MAX_BYTES` ⇒ 整趟拒（不截一半当目标）；上限以内（Linux 建得出的最长那一档）⇒ 照收。
#[test]
#[cfg(unix)]
fn a_zip_link_target_over_the_cap_refuses_instead_of_truncating() {
    let base = temp_root("longlink");
    let long = "a/".repeat(LINK_TARGET_MAX_BYTES as usize / 2 + 1);
    plant(
        &base,
        "l.zip",
        &zip_bytes(&[("ln", None, Some(long.as_str()))]),
    );
    let e = extract(&base, Path::new("l.zip"), Path::new("l")).expect_err("超长目标却解成了");
    assert_eq!(e.0, "refused", "{e:?}");
    assert!(std::fs::symlink_metadata(base.join("l")).is_err());
    let ok = "a/".repeat(LINK_TARGET_MAX_BYTES as usize / 2 - 1);
    plant(
        &base,
        "k.zip",
        &zip_bytes(&[("ln", None, Some(ok.as_str()))]),
    );
    extract(&base, Path::new("k.zip"), Path::new("k")).expect("恰好到上限的目标被拒了");
    std::fs::remove_dir_all(&base).ok();
}
