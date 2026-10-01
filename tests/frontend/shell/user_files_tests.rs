//! 〔RW1 · 第四波 · 2026-09-24〕`user_files` 的判据 ＋ monitor 这一侧判据共用的**替身门**。
//!
//! ⚠ 替身门 [`DiskDoor`] 在临时目录上做「读 · CAS · 写」，**只为让 monitor 这一侧的规划能被真跑一遍**
//! （算出来写了什么、交给后端的期望是哪一份、`stale` 之后有没有重读重算）。
//! 写的规则（备份 · 暂存旁名换名上位 · 回读比对 · 回滚 · 围栏）**不在这里判** ——
//! 那一份只住后端，判据在 `tests/backend/control/files_write_tests.rs`。替身这里刻意不备份、不回滚。

use super::*;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

// 〔MIG-3b 续〕门的 `put` 删了（最后一个用户进了那台后端）⇒ 替身记账那一格（`PutCall` / 插外部改动）随之删。

/// 一扇**只在判据里**用的门：落在本机临时目录上。
pub(crate) struct DiskDoor {
    pub home: PathBuf,
    /// 删会话那一条收到的 sid。
    /// 〔RM1d〕`delete`（`files-delete`）收到的相对段 ＋〔RM1e〕交过去的 `expect`。
    pub deleted: RefCell<Vec<(String, String)>>,
}

impl DiskDoor {
    pub(crate) fn new(home: &Path) -> Self {
        Self {
            home: home.to_path_buf(),
            deleted: RefCell::new(Vec::new()),
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

    async fn delete(&self, root: &str, rel: &str, expect: &str) -> Result<(), Refused> {
        let p = Self::at(root, rel);
        self.deleted
            .borrow_mut()
            .push((rel.to_string(), expect.to_string()));
        // 〔RM1e〕CAS 同后端 `files-delete` 的 `expect`：盘上 ≠ 读到的那一份 ⇒ stale，一个字节不动。
        if std::fs::read_to_string(&p).ok().as_deref() != Some(expect) {
            return Err(Refused::Stale(format!(
                "替身：{} 在读过之后变了",
                p.display()
            )));
        }
        std::fs::remove_file(&p)
            .map_err(|e| Refused::Other(format!("替身：删 {} 失败：{e}", p.display())))
    }
}

/// 本族共用的临时 home。
pub(crate) fn temp_home(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-uf-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&p).ok();
    std::fs::create_dir_all(&p).expect("建临时 home");
    p
}

// 〔MIG-3b 续〕读 → 算 → 交那一环的四条判据（交出去的期望是读到的那一份 · 不存在交 `None` · `stale` 重读重算到上限 · 没事可做不写）
//   随 monitor 那一环删了：同一环在后端那一份（`src/backend/assets/door.rs` 的 `edit`）由后端资产那几族的判据驱动。

// 〔MIG-3a〕`rel_under` 那条判据随函数搬进了后端（`tests/backend/assets/aliases/aliases_tests.rs`）。

// ── J2：门发出去的命令 == 后端登记的写面 ∪ 读面里真用到的那几条（两侧异源：一侧 monitor 源码，一侧后端源码）──

/// 从 monitor 的门（`BackendDoor`）生产段里抠出它发出去的命令名：每一处 `.ask(` 之后第一个字符串字面量。
fn door_commands() -> std::collections::BTreeSet<String> {
    let prod = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/user_files.rs"
    ));
    let mut out = std::collections::BTreeSet::new();
    for (at, _) in prod.match_indices(".ask(") {
        let rest = prod[at + ".ask(".len()..].trim_start();
        if let Some(body) = rest.strip_prefix('"') {
            if let Some(end) = body.find('"') {
                out.insert(body[..end].to_string());
            }
        }
    }
    out
}

/// 后端那一侧**登记了**的线上命令名（运行时读后端源码：写面 `MANAGE_COMMANDS` 的 `name:` ＋ 读族 `CAPABILITIES`
/// 的能力名换成线上写法 `.` → `-`）。⚠ 运行时读、不 `include_str!`：编译期跨半边的边有登记表管着。
fn backend_declared() -> (
    std::collections::BTreeSet<String>,
    std::collections::BTreeSet<String>,
) {
    let root = crate::guard_support::repo_root();
    let names = |rel: &str, prefix: &str| -> std::collections::BTreeSet<String> {
        let raw = std::fs::read_to_string(root.join(rel))
            .unwrap_or_else(|e| panic!("读不到后端的 {rel}：{e}"));
        let prod = guard_core::production_code(&raw);
        prod.lines()
            .filter_map(|l| l.trim().strip_prefix("name: \""))
            .filter_map(|l| l.split('"').next())
            .filter(|n| n.starts_with(prefix))
            .map(|n| n.replace('.', "-"))
            .collect()
    };
    (
        names("src/backend/control/files_write.rs", "files-"),
        names("src/backend/files/mod.rs", "files."),
    )
}

#[test]
fn every_command_the_door_sends_is_registered_on_the_backend_and_the_new_trio_has_a_consumer() {
    let sent = door_commands();
    let (write_face, read_family) = backend_declared();
    assert!(
        // 〔MIG-3b 续〕门 5 → 4：`files-chmod` 随公钥推送进本机后端出列；4 → 3：`files-put` 随它最后那个用户进了那台后端出列；
        // 〔THIN〕3 → 2：`files-peek` 随「认旧入口」进本机常驻后端出列。
        sent.len() >= 2 && write_face.len() >= 8 && read_family.len() >= 5,
        "人群塌了（门 {} 条 · 写面 {} 条 · 读族 {} 条）—— 抽取器坏了，本条空转",
        sent.len(),
        write_face.len(),
        read_family.len()
    );
    let unknown: Vec<&String> = sent
        .iter()
        .filter(|c| !write_face.contains(*c) && !read_family.contains(*c))
        .collect();
    assert!(
        unknown.is_empty(),
        "门发出去的这几条后端根本没登记：{unknown:?} —— 发过去只会拿到 unknown_command"
    );
    // 反向：`RW1` 在后端写面加的三条，每一条在 monitor 这一侧都有消费者（不许登记了没人用）。
    // 〔MIG-3b〕`files-delete-session` 出了这一组：删会话由界面经通道直说那台后端，门不再发它（消费者在 `src/frontend/ui/session-writes.ts`）。
    // 〔MIG-3b 续〕`files-put` 出了这一组：最后经门写的那个用户进了那台后端。
    // 〔THIN〕`files-peek` 也出了这一组：门上最后一个读的用户（`ccm_legacy` 认旧入口）进了本机常驻后端（`deploy-retired`）；
    //   它在后端照旧是写面「读改写」的读那一半（别的面在后端里直接调）。反向那一格随之无对象。
    // 写面里门会发的那几条，恰好是这一集合（多发一条写面命令 ⇒ 先回答它为什么经门）。
    // `files-delete`：`ccm_legacy` 删旧入口（带读到的那一份当期望值）。
    let sent_writes: std::collections::BTreeSet<&str> = sent
        .iter()
        .filter(|c| write_face.contains(*c))
        .map(String::as_str)
        .collect();
    assert_eq!(
        sent_writes,
        [
            // 〔MIG-3b 续〕`files-chmod` 出列：唯一用它的公钥推送进了本机后端。
            "files-delete",
            // 〔THIN〕`files-peek` 出列：见上。
            // 〔MIG-3b 续〕`files-put` 出列：唯一用它的那个用户写进了那台后端。
            // 〔MIG-3a · 子步 3〕`files-rename` 出列：唯一用它的 cc-bus 装前整目录备份进了本机后端。
        ]
        .into_iter()
        .collect(),
        "门经后端写面发的命令变了"
    );
}

// 〔MIG-3a〕「门发的只删空目录那一形的键 == 后端认的那一个」那一条随门上那一形删了（收空目录进了后端 `skill_flow.rs`，同一进程里直接调写面）。
