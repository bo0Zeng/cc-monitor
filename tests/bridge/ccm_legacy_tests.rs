//! 〔GP1 · 第四波〕`ccm_legacy.rs` 的判据 —— 旧版放在 `~/.local/bin/ccm` 的那一份，认出是我们放的就删。
//!
//! 要求住址：`设计/01 §6.7b` 逐字「远端要同拍做三件 —— … ② 清掉旧的 `~/.local/bin/ccm` shim · ③ 清掉同目录下与后端重复的字节。
//! 按 `96 §4`，三件都要在足迹里有入口」· 主会话 09-25 裁（按 V28 / V41 清掉，部署 / 升级时顺手删）。设计与编号：`GP1.md §2`（L1–L3）。
//!
//! 替身门是 `user_files::tests::DiskDoor`（临时目录上的「读 · CAS · 删」），结构夹具（不采任何真会话正文）。
//! 两形记号的**期望**是字面量，取自 git 史上那两份真文件的头两行（`git show e8f9e08e^:shared/ccm` ·
//! `ccm_entry_shim` 在 `b2bab98f` / `9c20ce0f` 的两代）—— 不从被测函数派生。

use super::*;
use crate::user_files::tests::{temp_home, DiskDoor};

/// 09-15 那一代 shim（带 `CCM_SELF` 那一行；devbox 上现打就是这一形）—— 手写字面量。
const SHIM_0915: &str = "#!/bin/sh\n# cc-monitor: ccm = 后端本体的一次性模式（K33：所有命令只许有一处）\n# CCM_SELF：内层载荷要用「我是被当作什么叫的」那个名字，不是二进制真身（容器路靠它）。\nCCM_SELF=\"${CCM_SELF:-$0}\" exec '/home/u/.cc-monitor/bin/cc-monitor-backend' ccm \"$@\"\n";
/// 09-11 之前那份 bash 启动器的头两行（后面几千行略去 —— 认它只看头两行）。
const LAUNCHER_HEAD: &str =
    "#!/usr/bin/env bash\n# ccm — cc-monitor 统一启动器（unify-launch F02）\n#\n# 核心思想……\n";

/// L1：`is_ours` 两形都认、别的一律不认（手写表）。
#[test]
fn gp1_only_the_two_forms_we_ever_placed_are_recognised() {
    let today = crate::backend::control::local_backend::ccm_entry_shim(
        "/home/u/.cc-monitor/bin/cc-monitor-backend",
    );
    let cells: [(&str, &str, bool); 8] = [
        ("09-15 那一代 shim", SHIM_0915, true),
        ("今天这一代 shim", &today, true),
        ("09-11 之前的 bash 启动器", LAUNCHER_HEAD, true),
        ("用户自己的脚本", "#!/bin/sh\nexec my-own-ccm \"$@\"\n", false),
        ("空文件", "", false),
        ("只有一行", "#!/bin/sh\n", false),
        (
            "记号不在第二行",
            "#!/bin/sh\n# 我自己的包装\n# cc-monitor: ccm = 后端本体的一次性模式（K33：所有命令只许有一处）\n",
            false,
        ),
        (
            "第一行不是 #!",
            "# cc-monitor: ccm = 后端本体的一次性模式（K33：所有命令只许有一处）\n# cc-monitor: ccm = 后端本体的一次性模式（K33：所有命令只许有一处）\n",
            false,
        ),
    ];
    for (what, text, want) in cells {
        assert_eq!(is_ours(text), want, "{what}");
    }
}

fn plant(home: &std::path::Path, text: &str) -> std::path::PathBuf {
    let p = home.join(LEGACY_REL);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
    p
}

/// L2：`sweep` × 替身门 —— 不在 ⇒ 零删；认得出 ⇒ 恰一次删、`expect` == 读到的全文、盘上真没了；
/// 认不出 ⇒ 零删、盘上逐字节还在；读与删之间被改 ⇒ `Kept`、盘上是改过的那一份。
#[test]
fn gp1_sweep_removes_only_what_it_recognises_and_only_what_it_read() {
    // 不在。
    let h = temp_home("gp1-legacy-absent");
    let door = DiskDoor::new(&h);
    assert_eq!(
        tauri::async_runtime::block_on(sweep(&door)).unwrap(),
        Swept::Absent
    );
    assert!(door.deleted.borrow().is_empty());

    // 认得出（09-15 那一代）⇒ 删。
    let h = temp_home("gp1-legacy-ours");
    let p = plant(&h, SHIM_0915);
    let door = DiskDoor::new(&h);
    assert!(matches!(
        tauri::async_runtime::block_on(sweep(&door)).unwrap(),
        Swept::Removed { .. }
    ));
    assert_eq!(
        door.deleted.borrow().clone(),
        vec![(LEGACY_REL.to_string(), SHIM_0915.to_string())],
        "删的不是那一份 / 交的期望不是读到的全文"
    );
    assert!(!p.exists(), "说删了、盘上还在");

    // 认不出（用户自己的）⇒ 不动。
    let h = temp_home("gp1-legacy-theirs");
    let mine = "#!/bin/sh\nexec my-own-ccm \"$@\"\n";
    let p = plant(&h, mine);
    let door = DiskDoor::new(&h);
    let got = tauri::async_runtime::block_on(sweep(&door)).unwrap();
    assert!(matches!(got, Swept::Kept { .. }), "实得 {got:?}");
    assert!(
        door.deleted.borrow().is_empty(),
        "用户自己的那一份被交去删了"
    );
    assert_eq!(std::fs::read_to_string(&p).unwrap(), mine);

    // 读完之后被改（CAS 那一格）⇒ 不删。替身在 `delete` 之前没有插改动的口，这里直接在盘上改：
    // 先让 sweep 读到 shim，再在它删之前改 —— 用一扇「读的时候看到的是 A、删的时候盘上是 B」的门。
    let h = temp_home("gp1-legacy-stale");
    let p = plant(&h, SHIM_0915);
    let door = StaleDoor {
        inner: DiskDoor::new(&h),
        swap_to: "#!/bin/sh\nexec my-own-ccm \"$@\"\n".to_string(),
    };
    let got = tauri::async_runtime::block_on(sweep(&door)).unwrap();
    assert!(matches!(got, Swept::Kept { .. }), "实得 {got:?}");
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        door.swap_to,
        "改过的那一份被删了"
    );
}

/// 读的那一刻之后、删之前把盘上换掉的门（其余照 [`DiskDoor`]）。
struct StaleDoor {
    inner: DiskDoor,
    swap_to: String,
}

impl crate::user_files::Door for StaleDoor {
    fn machine(&self) -> String {
        self.inner.machine()
    }
    async fn home(&self) -> Result<String, String> {
        self.inner.home().await
    }
    async fn peek(&self, root: &str, rel: &str) -> Result<crate::user_files::Peeked, String> {
        let got = self.inner.peek(root, rel).await?;
        std::fs::write(std::path::Path::new(root).join(rel), &self.swap_to).unwrap();
        Ok(got)
    }
    async fn put(
        &self,
        root: &str,
        rel: &str,
        content: &str,
        expect: Option<&str>,
        backup: bool,
        parents: bool,
    ) -> Result<crate::user_files::Landed, Refused> {
        self.inner
            .put(root, rel, content, expect, backup, parents)
            .await
    }
    async fn rename(&self, root: &str, from: &str, to: &str) -> Result<(), String> {
        self.inner.rename(root, from, to).await
    }
    async fn delete(&self, root: &str, rel: &str, expect: &str) -> Result<(), Refused> {
        self.inner.delete(root, rel, expect).await
    }
    async fn chmod(&self, root: &str, rel: &str, mode: u32) -> Result<(), String> {
        self.inner.chmod(root, rel, mode).await
    }
    async fn delete_session(&self, sid: &str) -> Result<String, String> {
        self.inner.delete_session(sid).await
    }
    async fn stat_kind(&self, path: &str) -> Result<Option<String>, String> {
        self.inner.stat_kind(path).await
    }
    async fn list_dir(&self, path: &str) -> Result<Vec<(String, bool)>, String> {
        self.inner.list_dir(path).await
    }
}

/// L3：两个触发点各恰一处（部署按钮 · 那台长连接握手完成）＋ 足迹那一行在册（读 `TOOLS`，路径 == [`LEGACY_REL`]）。
#[test]
fn gp1_both_triggers_are_wired_once_and_the_footprint_lists_the_legacy_file() {
    let sftp = guard_core::production_code(include_str!("../../src/bridge/src/sftp.rs"));
    assert_eq!(
        sftp.matches("crate::ccm_legacy::sweep(").count(),
        1,
        "部署按钮那一处没扫（或扫了不止一次）"
    );
    let src = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    assert_eq!(
        src.matches("crate::ccm_legacy::on_remote_ready(cfg)")
            .count(),
        1,
        "长连接握手那一处没扫（或扫了不止一次）"
    );
    let want = format!("~/{LEGACY_REL}");
    let rows: Vec<_> = crate::tool_registry::TOOLS
        .iter()
        .flat_map(|t| t.touches())
        .filter(|f| f.path == want)
        .collect();
    assert_eq!(rows.len(), 1, "足迹里 `{want}` 那一行不是恰好一行");
    assert_eq!(
        rows[0].effect,
        crate::tool_registry::TouchEffect::RetiredLegacy
    );
    assert_eq!(rows[0].host, crate::tool_registry::HostScope::Remote);
}
