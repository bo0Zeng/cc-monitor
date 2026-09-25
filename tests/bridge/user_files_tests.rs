//! 〔RW1 · 第四波 · 2026-09-24〕`user_files` 的判据 ＋ monitor 这一侧判据共用的**替身门**。
//!
//! ⚠ 替身门 [`DiskDoor`] 在临时目录上做「读 · CAS · 写」，**只为让 monitor 这一侧的规划能被真跑一遍**
//! （算出来写了什么、交给后端的期望是哪一份、`stale` 之后有没有重读重算）。
//! 写的规则（备份 · 暂存旁名换名上位 · 回读比对 · 回滚 · 围栏）**不在这里判** ——
//! 那一份只住后端，判据在 `tests/backend/control/files_write_tests.rs`。替身这里刻意不备份、不回滚。

use super::*;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

/// 替身门记下的一次 `put`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PutCall {
    pub rel: String,
    pub content: String,
    pub expect: Option<String>,
    pub backup: bool,
    pub parents: bool,
}

/// 一扇**只在判据里**用的门：落在本机临时目录上。
pub(crate) struct DiskDoor {
    pub home: PathBuf,
    /// 下一次 `put` 之前**插进来的外部改动**（模拟「读完之后别人改了盘上那份」）：
    /// 每次 `put` 先弹一条、原样写进目标，于是那一趟必然 `stale`。
    pub interfere: RefCell<Vec<String>>,
    /// 交给后端的每一次 `put` —— 判据据此核「交出去的期望是读到的那一份」「要没要备份」。
    pub puts: RefCell<Vec<PutCall>>,
    /// 删会话那一条收到的 sid。
    pub deleted_sids: RefCell<Vec<String>>,
}

impl DiskDoor {
    pub(crate) fn new(home: &Path) -> Self {
        Self {
            home: home.to_path_buf(),
            interfere: RefCell::new(Vec::new()),
            puts: RefCell::new(Vec::new()),
            deleted_sids: RefCell::new(Vec::new()),
        }
    }

    fn at(root: &str, rel: &str) -> PathBuf {
        Path::new(root).join(rel)
    }
}

impl Door for DiskDoor {
    fn machine(&self) -> String {
        "本机".to_string()
    }

    async fn home(&self) -> Result<String, String> {
        Ok(self.home.display().to_string())
    }

    async fn peek(&self, root: &str, rel: &str) -> Result<Peeked, String> {
        let p = Self::at(root, rel);
        let text = if p.exists() {
            Some(
                std::fs::read_to_string(&p)
                    .map_err(|e| format!("替身读不了 {}：{e}", p.display()))?,
            )
        } else {
            None
        };
        Ok(Peeked {
            path: p.display().to_string(),
            text,
        })
    }

    async fn put(
        &self,
        root: &str,
        rel: &str,
        content: &str,
        expect: Option<&str>,
        backup: bool,
        parents: bool,
    ) -> Result<Landed, Refused> {
        let p = Self::at(root, rel);
        if let Some(x) = self.interfere.borrow_mut().pop() {
            std::fs::write(&p, x).expect("替身插外部改动");
        }
        self.puts.borrow_mut().push(PutCall {
            rel: rel.to_string(),
            content: content.to_string(),
            expect: expect.map(str::to_string),
            backup,
            parents,
        });
        let current = std::fs::read_to_string(&p).ok();
        if current.as_deref() != expect {
            return Err(Refused::Stale(format!(
                "替身：{} 在读过之后变了",
                p.display()
            )));
        }
        if parents {
            if let Some(d) = p.parent() {
                std::fs::create_dir_all(d).expect("替身补父目录");
            }
        } else if !p.parent().is_some_and(Path::exists) {
            return Err(Refused::Other(format!(
                "替身：{} 的父目录不在",
                p.display()
            )));
        }
        let changed = current.as_deref() != Some(content);
        if changed {
            std::fs::write(&p, content).expect("替身写");
        }
        Ok(Landed {
            path: p.display().to_string(),
            changed,
            created: current.is_none(),
            backup: None,
        })
    }

    async fn rename(&self, root: &str, from: &str, to: &str) -> Result<(), String> {
        let (a, b) = (Self::at(root, from), Self::at(root, to));
        if b.exists() {
            return Err(format!("替身：{} 已经在了", b.display()));
        }
        std::fs::rename(&a, &b).map_err(|e| e.to_string())
    }

    async fn chmod(&self, root: &str, rel: &str, mode: u32) -> Result<(), String> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(Self::at(root, rel), std::fs::Permissions::from_mode(mode))
                .map_err(|e| e.to_string())
        }
        #[cfg(not(unix))]
        {
            let _ = (root, rel, mode);
            Ok(())
        }
    }

    async fn delete_session(&self, sid: &str) -> Result<String, String> {
        self.deleted_sids.borrow_mut().push(sid.to_string());
        Ok(format!("<替身>/{sid}.jsonl"))
    }
}

/// 本族共用的临时 home。
pub(crate) fn temp_home(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-uf-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&p).ok();
    std::fs::create_dir_all(&p).expect("建临时 home");
    p
}

#[test]
fn edit_hands_the_backend_exactly_what_it_read_as_the_expectation() {
    let h = temp_home("exp");
    let root = h.display().to_string();
    std::fs::write(h.join("rc"), "a\n").expect("铺");
    let door = DiskDoor::new(&h);
    let got = futures::executor::block_on(edit(&door, &root, "rc", true, false, |t| {
        Ok(Some(format!("{}b\n", t.unwrap_or(""))))
    }))
    .expect("写");
    assert!(matches!(got, Edited::Written(_)));
    assert_eq!(
        door.puts.borrow().as_slice(),
        &[PutCall {
            rel: "rc".to_string(),
            content: "a\nb\n".to_string(),
            expect: Some("a\n".to_string()),
            backup: true,
            parents: false,
        }],
        "交给后端的期望必须是**读到的那一份**"
    );
    std::fs::remove_dir_all(&h).ok();
}

#[test]
fn a_missing_file_is_handed_over_as_an_absent_expectation_not_an_empty_one() {
    let h = temp_home("absent");
    let root = h.display().to_string();
    let door = DiskDoor::new(&h);
    futures::executor::block_on(edit(&door, &root, "new", false, false, |t| {
        assert_eq!(t, None, "不存在该交 `None`，不是空串");
        Ok(Some("x".to_string()))
    }))
    .expect("建");
    assert_eq!(
        door.puts.borrow()[0].expect,
        None,
        "不存在 ⇒ `expect: null`"
    );
    std::fs::remove_dir_all(&h).ok();
}

#[test]
fn stale_means_read_again_and_plan_again_and_it_gives_up_after_the_cap() {
    let h = temp_home("stale");
    let root = h.display().to_string();
    std::fs::write(h.join("rc"), "a\n").expect("铺");
    let door = DiskDoor::new(&h);
    // 第一趟被插了一条外部改动 ⇒ stale ⇒ 重读（读到的是 "z\n"）重算。
    door.interfere.borrow_mut().push("z\n".to_string());
    futures::executor::block_on(edit(&door, &root, "rc", false, false, |t| {
        Ok(Some(format!("{}b\n", t.unwrap_or(""))))
    }))
    .expect("第二趟该成");
    assert_eq!(
        std::fs::read_to_string(h.join("rc")).expect("rc"),
        "z\nb\n",
        "没按重读的那一份重算"
    );
    // 每一趟都被插 ⇒ 到上限就停，盘上是最后那条外部改动，不是我们算的。
    *door.interfere.borrow_mut() = (0..EDIT_ATTEMPTS).map(|i| format!("q{i}\n")).collect();
    let e = futures::executor::block_on(edit(&door, &root, "rc", false, false, |t| {
        Ok(Some(format!("{}b\n", t.unwrap_or(""))))
    }))
    .expect_err("每趟都 stale 该停下");
    assert!(e.contains(&EDIT_ATTEMPTS.to_string()), "{e}");
    assert_eq!(
        std::fs::read_to_string(h.join("rc")).expect("rc"),
        "q0\n",
        "盘上该是最后那条外部改动"
    );
    std::fs::remove_dir_all(&h).ok();
}

#[test]
fn nothing_to_do_and_same_content_both_hand_over_nothing() {
    let h = temp_home("noop");
    let root = h.display().to_string();
    std::fs::write(h.join("rc"), "a\n").expect("铺");
    let door = DiskDoor::new(&h);
    for plan in [None, Some("a\n".to_string())] {
        let got =
            futures::executor::block_on(edit(
                &door,
                &root,
                "rc",
                true,
                false,
                |_| Ok(plan.clone()),
            ))
            .expect("不写");
        assert_eq!(got, Edited::Unchanged);
    }
    assert!(door.puts.borrow().is_empty(), "没事可做却交了一次写");
    std::fs::remove_dir_all(&h).ok();
}

#[test]
fn rel_under_keeps_writes_inside_home_on_both_path_styles() {
    assert_eq!(rel_under("/home/u", "/home/u/.bashrc").unwrap(), ".bashrc");
    assert_eq!(rel_under("/home/u/", "/home/u/a/b").unwrap(), "a/b");
    assert_eq!(
        rel_under(r"C:\Users\u", r"C:\Users\u\Documents\PowerShell\p.ps1").unwrap(),
        "Documents/PowerShell/p.ps1"
    );
    for bad in ["/home/uu/.bashrc", "/etc/passwd", "/home/u", "/home/u/"] {
        assert!(rel_under("/home/u", bad).is_err(), "{bad} 竟然过了");
    }
}
