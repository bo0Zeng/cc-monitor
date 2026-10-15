//! 要求：手机自己给远端装 / 升级后端 —— 照桌面五步的 ①–④ 在一条连接上做完（计划 · 取字节 ＋ 自检 · 放 ＋ 读回 · 收拾残件）。
//!
//! 对面是合成 SFTP 服务端（`ssh_dial::sftp::rig`，判据看它的改动表与盘面，不信被测那一侧的自述）＋ 替身的一次性命令
//! （`uname` 照设定答；扫身份戳照台架盘上落点那一份的字节现扫，同远端那条 `grep` 的答法）。

use std::sync::{Arc, Mutex};

use deploy_contract::DeployAction;
use serde_json::Value;
use ssh_dial::sftp::rig;
use ssh_dial::sftp::Session;
use ssh_dial::Captured;

use crate::{Facing, Fail, Fut, Place, Slot, Step};

/// 判据里的「此刻」（秒）。
const NOW: u64 = 10_000_000;

const LANDING: &str = relay_route_core::BACKEND_LANDING_REL;

/// 装上拨号核心那一套（写按部署那两处，同手机进程）。
fn host() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = ssh_dial::env::install(ssh_dial::env::Env {
            files: ssh_dial::sftp::WriteRules::Deploy { dir_mode: 0o700 },
            ..ssh_dial::env::Env::bare()
        });
    });
}

/// 一份带身份戳的「后端」字节（语料 ＋ 戳，戳恰一个）。
fn backend(id: &str) -> Vec<u8> {
    let mut b = rig::corpus(40_000);
    b.extend(
        format!(
            "{}{id}{}",
            deploy_contract::STAMP_OPEN,
            deploy_contract::STAMP_CLOSE
        )
        .bytes(),
    );
    b.extend(rig::corpus(3_000));
    b
}

struct Rig {
    fs: Arc<Mutex<rig::Fs>>,
    session: Session,
    uname: String,
    asked: Mutex<Vec<String>>,
}

impl Rig {
    async fn linux(landing: Option<&[u8]>) -> Rig {
        host();
        let fs = rig::home(landing.is_some(), false);
        if let Some(b) = landing {
            fs.lock()
                .unwrap()
                .files
                .insert(LANDING.to_string(), rig::Entry { bytes: b.to_vec() });
        }
        let session = rig::session_on(fs.clone()).await;
        Rig {
            fs,
            session,
            uname: "Linux x86_64\n".into(),
            asked: Mutex::new(Vec::new()),
        }
    }

    fn landing(&self) -> Option<Vec<u8>> {
        self.fs.lock().unwrap().bytes(LANDING)
    }
}

impl Facing for Rig {
    fn exec(&self, command: String) -> Fut<'_, Result<(Captured, Value), String>> {
        self.asked.lock().unwrap().push(command.clone());
        let out = if command == deploy_contract::UNAME_CMD {
            Captured {
                stdout: self.uname.clone(),
                stderr: String::new(),
                exit_status: Some(0),
            }
        } else if command
            == deploy_contract::stamp_scan_cmd(
                relay_route_core::BACKEND_LANDING_SHELL,
                crate::MARKS,
            )
        {
            match self
                .landing()
                .map(|b| deploy_contract::identity_of_bytes(&b, crate::MARKS))
            {
                Some(deploy_contract::RemoteIdentity::Stamp(id)) => Captured {
                    stdout: format!(
                        "{}{id}{}\n",
                        deploy_contract::STAMP_OPEN,
                        deploy_contract::STAMP_CLOSE
                    ),
                    stderr: String::new(),
                    exit_status: Some(0),
                },
                _ => Captured {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_status: Some(1),
                },
            }
        } else {
            return Box::pin(async move { Err(format!("替身没登记这条命令：{command}")) });
        };
        Box::pin(async move { Ok((out, serde_json::json!({"ok": true}))) })
    }

    fn stat<'a>(
        &'a self,
        rel: &'a str,
    ) -> Fut<'a, Result<(Option<Option<u64>>, Option<bool>), String>> {
        Box::pin(async move {
            let size = ssh_dial::sftp::metadata_size(&self.session, rel).await;
            let exists = match size {
                Some(_) => None,
                None => ssh_dial::sftp::exists(&self.session, rel).await,
            };
            Ok((size, exists))
        })
    }

    fn list<'a>(&'a self, rel: &'a str) -> Fut<'a, Option<Vec<(String, Option<u64>)>>> {
        Box::pin(async move { ssh_dial::sftp::list_dir(&self.session, rel).await })
    }
}

impl Place for Rig {
    fn sftp(&self) -> Fut<'_, Result<&Session, Fail>> {
        Box::pin(async move { Ok(&self.session) })
    }
}

/// 把字节写成一格文件（同手机 App 从包里解出来的那一份）。
fn slot(dir: &std::path::Path, id: &str, bytes: &[u8]) -> Slot {
    let path = dir.join(format!("backend-{id}"));
    std::fs::write(&path, bytes).unwrap();
    Slot {
        key: deploy_contract::key_of("Linux", "x86_64").unwrap(),
        id: id.to_string(),
        path,
    }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("deploy-install-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

async fn run(r: &Rig, slots: &[Slot]) -> (Result<crate::Plan, Fail>, Vec<Step>) {
    let steps = Mutex::new(Vec::new());
    let load = |p: &std::path::Path| std::fs::read(p);
    let got = crate::install(r, slots, &load, "box", NOW, &|s| {
        steps.lock().unwrap().push(s)
    })
    .await;
    (got, steps.into_inner().unwrap())
}

/// 那台没有后端 ⇒ 装上（落点上逐字节是那一格、目录收成只给本人）；进度先「问」后「装」（不是「更新」）。
#[tokio::test]
async fn a_machine_without_a_backend_gets_this_one() {
    let d = scratch("absent");
    let bytes = backend("p9b-mine");
    let r = Rig::linux(None).await;
    let (got, steps) = run(&r, &[slot(&d, "p9b-mine", &bytes)]).await;
    let plan = got.expect("该装上");
    assert!(
        matches!(plan.action, DeployAction::Deploy(_)),
        "{:?}",
        plan.action
    );
    assert_eq!(r.landing(), Some(bytes));
    assert_eq!(steps, vec![Step::Plan, Step::Place { updating: false }]);
    assert_eq!(
        r.fs.lock().unwrap().dir_modes.get(".cc-monitor/bin"),
        Some(&0o700)
    );
}

/// 那台是旧版 ⇒ 换成这一版（进度说「更新」）；同版 ⇒ 一个字节不写；那台更新 ⇒ 不动它、回它自报的身份。
#[tokio::test]
async fn an_older_backend_is_upgraded_and_nothing_else_is_touched() {
    let d = scratch("older");
    let mine = backend("p9b-mine");
    let r = Rig::linux(Some(&backend("p9a-old"))).await;
    let (got, steps) = run(&r, &[slot(&d, "p9b-mine", &mine)]).await;
    assert!(matches!(got.unwrap().action, DeployAction::Deploy(_)));
    assert_eq!(r.landing(), Some(mine.clone()));
    assert_eq!(steps, vec![Step::Plan, Step::Place { updating: true }]);

    let before = r.fs.lock().unwrap().mutated.len();
    let (got, steps) = run(&r, &[slot(&d, "p9b-mine", &mine)]).await;
    assert_eq!(got.unwrap().action, DeployAction::Skip);
    assert_eq!(steps, vec![Step::Plan]);
    assert_eq!(r.fs.lock().unwrap().mutated.len(), before, "同版还写了东西");

    let newer = Rig::linux(Some(&backend("p9c-new"))).await;
    let (got, _) = run(&newer, &[slot(&d, "p9b-mine", &mine)]).await;
    assert!(
        matches!(&got.unwrap().action, DeployAction::Keep { theirs, .. } if theirs == "p9c-new")
    );
    assert!(
        newer.fs.lock().unwrap().mutated.is_empty(),
        "那台更新还写了东西"
    );
}

/// 自检：那一格的字节自报的身份与计划要的不同（解错了文件 · 包里那份被换过）⇒ 不推，一个字节不写。
#[tokio::test]
async fn bytes_that_do_not_say_the_planned_identity_are_not_pushed() {
    let d = scratch("selfcheck");
    let r = Rig::linux(None).await;
    let (got, _) = run(&r, &[slot(&d, "p9b-mine", &backend("p9a-other"))]).await;
    let e = got.unwrap_err();
    assert_eq!(e.code, "not_carried", "{e:?}");
    assert!(
        r.fs.lock().unwrap().mutated.is_empty(),
        "{:?}",
        r.fs.lock().unwrap().mutated
    );
    // 文件不在（App 没解出来）⇒ 同一码、原话进详情。
    let mut gone = slot(&d, "p9b-mine", &backend("p9b-mine"));
    gone.path = d.join("missing");
    let (got, _) = run(&r, &[gone]).await;
    let e = got.unwrap_err();
    assert_eq!(e.code, "not_carried");
    assert!(e.raw.is_some_and(|r| r.contains("missing")));
}

/// 读回不对 ⇒ 失败、传坏的那一份当场删掉（下次判成「没装」再来）。
#[tokio::test]
async fn a_bad_readback_fails_the_install_and_leaves_nothing_at_the_landing() {
    let d = scratch("readback");
    let r = Rig::linux(None).await;
    r.fs.lock().unwrap().corrupt_reads = true;
    let (got, _) = run(&r, &[slot(&d, "p9b-mine", &backend("p9b-mine"))]).await;
    let e = got.unwrap_err();
    assert!(
        copy_core::copy_matches("rsSftp.upload.badRemoved", &e.said),
        "{e:?}"
    );
    assert_eq!(r.landing(), None);
}

/// 要求：「中途断了下次照计划删残件」。第一趟写到一半断了、临时件也没删掉（连接没了）⇒ 留下一份残件，落点不动；
/// 下一趟（隔了一个多小时）计划认出它、装上这一版、把它删掉。
#[tokio::test]
async fn a_trip_cut_midway_leaves_a_leftover_that_the_next_trip_sweeps() {
    let d = scratch("cut");
    let mine = backend("p9b-mine");
    let r = Rig::linux(Some(&backend("p9a-old"))).await;
    {
        let mut g = r.fs.lock().unwrap();
        g.fail_write_after = Some(1);
        g.refuse_remove = true;
    }
    let (got, _) = run(&r, &[slot(&d, "p9b-mine", &mine)]).await;
    assert!(got.is_err());
    let left: Vec<String> = {
        let g = r.fs.lock().unwrap();
        g.files
            .keys()
            .filter(|k| k.ends_with(".tmp"))
            .cloned()
            .collect()
    };
    assert_eq!(left.len(), 1, "该留下恰一份临时件：{left:?}");
    assert_eq!(
        r.landing(),
        Some(backend("p9a-old")),
        "断在改名之前，落点上旧的那一份该原样不动"
    );
    {
        let mut g = r.fs.lock().unwrap();
        g.fail_write_after = None;
        g.refuse_remove = false;
        // 那份残件一个多小时没人动过（台架按秒报修改时间）。
        g.mtimes.insert(
            left[0].clone(),
            (NOW - crate::LEFTOVER_STALE_SECS - 10) as u32,
        );
    }
    let (got, _) = run(&r, &[slot(&d, "p9b-mine", &mine)]).await;
    let plan = got.expect("第二趟该装上");
    assert_eq!(plan.leftovers, left);
    assert_eq!(r.landing(), Some(mine));
    assert!(r.fs.lock().unwrap().bytes(&left[0]).is_none(), "残件没删");
}
