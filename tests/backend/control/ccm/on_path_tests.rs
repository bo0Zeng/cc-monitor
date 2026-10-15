//! 终端里敲 `ccm` 走到哪一份：四形各判一次（同名文件逐个放在沙箱目录里），那句话按形各一句、互不相同。

use super::*;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "ccm-on-path-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn file(p: &std::path::Path) -> bool {
    p.is_file()
}

#[test]
fn reach_tells_ours_other_absent_and_unknown_apart() {
    let h = tmp("reach");
    let ours = h.join(".cc-monitor/bin");
    let other = h.join("other");
    std::fs::create_dir_all(&ours).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(ours.join("ccm"), "x").unwrap();
    std::fs::write(other.join("ccm"), "x").unwrap();
    let landing = ours.join("ccm");
    let path = |dirs: &[&PathBuf]| std::env::join_paths(dirs).unwrap().into_string().unwrap();

    assert_eq!(
        reach(Some(&path(&[&ours, &other])), &landing, false, &file),
        Reach::Ours
    );
    assert_eq!(
        reach(Some(&path(&[&other, &ours])), &landing, false, &file),
        Reach::Other(other.join("ccm"))
    );
    assert_eq!(
        reach(Some(&path(&[&h])), &landing, false, &file),
        Reach::Absent
    );
    assert_eq!(reach(None, &landing, false, &file), Reach::Unknown);
    assert_eq!(reach(Some("  "), &landing, false, &file), Reach::Unknown);

    // 经链接走到落点那一份也算「是它」（比的是解过链接的文件，不是路径串）。
    #[cfg(unix)]
    {
        let via = h.join("via");
        std::fs::create_dir_all(&via).unwrap();
        std::os::unix::fs::symlink(&landing, via.join("ccm")).unwrap();
        assert_eq!(
            reach(Some(&path(&[&via])), &landing, false, &file),
            Reach::Ours
        );
    }
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn each_reach_has_its_own_verdict_and_sentence() {
    let shown = "$HOME/.cc-monitor/bin/ccm";
    let all = [
        (Reach::Ours, OnPathVerdict::Ours),
        (
            Reach::Other(PathBuf::from("/opt/x/ccm")),
            OnPathVerdict::Other,
        ),
        (Reach::Absent, OnPathVerdict::Absent),
        (Reach::Unknown, OnPathVerdict::Unknown),
    ];
    let mut said = Vec::new();
    for (r, want) in &all {
        let got = on_path(r, shown);
        assert_eq!(&got.verdict, want, "{r:?}");
        said.push(got.said);
    }
    // 走到的就是落点那一份 ⇒ 没有话要说；其余三形都说、互不相同。
    assert_eq!(said[0], "");
    for s in &said[1..] {
        assert!(!s.is_empty());
    }
    let distinct: std::collections::BTreeSet<&String> = said[1..].iter().collect();
    assert_eq!(distinct.len(), 3, "三形的话合成了一句：{said:?}");
    // 走到别处 ⇒ 说出是哪一份、落点在哪；没有 ⇒ 说出落点在哪。
    assert!(
        said[1].contains("/opt/x/ccm") && said[1].contains(shown),
        "{}",
        said[1]
    );
    assert!(said[2].contains(shown), "{}", said[2]);
}

#[test]
fn the_landing_is_shown_the_way_that_platform_spells_paths() {
    let landing = std::path::Path::new("/home/u/.cc-monitor/bin/ccm");
    assert_eq!(landing_shown(landing, false), "$HOME/.cc-monitor/bin/ccm");
    assert_eq!(landing_shown(landing, true), landing.display().to_string());
}
