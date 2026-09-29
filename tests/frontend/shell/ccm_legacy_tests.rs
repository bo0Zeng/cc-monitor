//! 〔GP1 · 第四波〕`ccm_legacy.rs` 的判据 —— 旧版放在 `~/.local/bin/ccm` 的那一份，认出是我们放的就删。
//!
//! 要求住址：`设计/01 §6.7b` 逐字「远端要同拍做三件 —— … ② 清掉旧的 `~/.local/bin/ccm` shim · ③ 清掉同目录下与后端重复的字节。
//! 按 `96 §4`，三件都要在足迹里有入口」· 主会话 09-25 裁（按 V28 / V41 清掉，部署 / 升级时顺手删）。设计与编号：`GP1.md §2`（L1–L3）。
//! 〔THIN · 09-29〕「认不认得出」随判定进了本机常驻后端（`deploy-retired`）⇒ L1 那张两形真值表搬去
//! `tests/backend/control/deploy_plan_tests.rs`（`retired_*`）；本文件只剩照答办（L2）· 线上形状 · 接线与足迹（L3）。
//!
//! 替身门是 `user_files::tests::DiskDoor`（临时目录上的「读 · CAS · 删」），结构夹具（不采任何真会话正文）。

use super::*;
use crate::user_files::tests::{temp_home, DiskDoor};

/// 09-15 那一代 shim —— 手写字面量（后端答 `remove` 时带的就是这样一份全文）。
const SHIM_0915: &str = "#!/bin/sh\n# cc-monitor: ccm = 后端本体的一次性模式（K33：所有命令只许有一处）\n# CCM_SELF：内层载荷要用「我是被当作什么叫的」那个名字，不是二进制真身（容器路靠它）。\nCCM_SELF=\"${CCM_SELF:-$0}\" exec '/home/u/.cc-monitor/bin/cc-monitor-backend' ccm \"$@\"\n";

fn plant(home: &std::path::Path, text: &str) -> std::path::PathBuf {
    let p = home.join(LEGACY_REL);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
    p
}

/// L2：`apply` × 替身门 —— 照答办：`absent` / `keep` ⇒ 零删（盘上逐字节还在）；`remove` ⇒ 恰一次删、`expect` == 后端给的全文、
/// 盘上真没了；后端给的全文与盘上不一致（读与删之间被改）⇒ `Kept`、盘上是改过的那一份。本侧不判「是不是我们放的」。
#[test]
fn gp1_apply_does_exactly_what_the_backend_said_and_only_with_its_expect() {
    let run = |door: &DiskDoor, v: Verdict| tauri::async_runtime::block_on(apply(door, v)).unwrap();

    let h = temp_home("gp1-legacy-absent");
    let door = DiskDoor::new(&h);
    assert_eq!(run(&door, Verdict::Absent), Swept::Absent);
    assert!(door.deleted.borrow().is_empty());

    let h = temp_home("gp1-legacy-ours");
    let p = plant(&h, SHIM_0915);
    let door = DiskDoor::new(&h);
    let got = run(
        &door,
        Verdict::Remove {
            expect: SHIM_0915.to_string(),
        },
    );
    assert_eq!(got, Swept::Removed);
    assert_eq!(
        door.deleted.borrow().clone(),
        vec![(LEGACY_REL.to_string(), SHIM_0915.to_string())],
        "删的不是那一份 / 交的期望不是后端给的全文"
    );
    assert!(!p.exists(), "说删了、盘上还在");

    let h = temp_home("gp1-legacy-keep");
    let mine = "#!/bin/sh\nexec my-own-ccm \"$@\"\n";
    let p = plant(&h, mine);
    let door = DiskDoor::new(&h);
    let got = run(
        &door,
        Verdict::Keep {
            why: "不是 cc-monitor 放的".into(),
        },
    );
    assert_eq!(
        got,
        Swept::Kept {
            why: "不是 cc-monitor 放的".into()
        }
    );
    assert!(
        door.deleted.borrow().is_empty(),
        "后端说留着，这里却交去删了"
    );
    assert_eq!(std::fs::read_to_string(&p).unwrap(), mine);

    // 后端读到的是 shim，盘上此刻已被换成用户自己的 ⇒ CAS 挡住、不删。
    let h = temp_home("gp1-legacy-stale");
    let p = plant(&h, mine);
    let door = DiskDoor::new(&h);
    let got = run(
        &door,
        Verdict::Remove {
            expect: SHIM_0915.to_string(),
        },
    );
    assert!(matches!(got, Swept::Kept { .. }), "实得 {got:?}");
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        mine,
        "改过的那一份被删了"
    );
}

/// `deploy-retired` 的答严格收：恰 `{verdict, expect, why}`、三形各自那两格该空的空（形状同 `IPC-PROTOCOL.md` 那一节，手写）。
#[test]
fn the_retired_verdict_answer_is_read_strictly() {
    use serde_json::json;
    assert_eq!(
        decode_verdict(&json!({"verdict": "absent", "expect": null, "why": null})),
        Ok(Verdict::Absent)
    );
    assert_eq!(
        decode_verdict(&json!({"verdict": "remove", "expect": "#!/bin/sh\n", "why": null})),
        Ok(Verdict::Remove {
            expect: "#!/bin/sh\n".into()
        })
    );
    assert_eq!(
        decode_verdict(&json!({"verdict": "keep", "expect": null, "why": "不是 cc-monitor 放的"})),
        Ok(Verdict::Keep {
            why: "不是 cc-monitor 放的".into()
        })
    );
    for bad in [
        json!({"verdict": "remove", "expect": null, "why": null}),
        json!({"verdict": "keep", "expect": "x", "why": "y"}),
        json!({"verdict": "absent", "expect": null}),
        json!({"verdict": "absent", "expect": null, "why": null, "extra": 1}),
        json!({"verdict": "delete", "expect": "x", "why": null}),
        json!(null),
    ] {
        assert!(decode_verdict(&bad).is_err(), "{bad} 被收下了");
    }
}

/// L3：两个触发点各恰一处（部署按钮 · 那台长连接握手完成）＋ 足迹那一行在册（读 `TOOLS`，路径 == [`LEGACY_REL`]）。
#[test]
fn gp1_both_triggers_are_wired_once_and_the_footprint_lists_the_legacy_file() {
    let sftp = guard_core::production_code(include_str!("../../../src/frontend/shell/src/sftp.rs"));
    assert_eq!(
        sftp.matches("crate::ccm_legacy::sweep(").count(),
        1,
        "部署按钮那一处没扫（或扫了不止一次）"
    );
    let src = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/ssh_source.rs"
    ));
    assert_eq!(
        src.matches("crate::ccm_legacy::on_remote_ready(cfg)")
            .count(),
        1,
        "长连接握手那一处没扫（或扫了不止一次）"
    );
    // 〔MIG-3b 续〕足迹申报表进了后端（`src/backend/footprint/registry.rs`）⇒ 读它的源码：
    //   `path: "~/<LEGACY_REL>"` 恰好一处，它那一格（`TouchedFile { … }`）的 host 是远端、effect 是「旧版放的、认出才删」。
    let table =
        guard_core::production_code(include_str!("../../../src/backend/footprint/registry.rs"));
    let want = format!("path: \"~/{LEGACY_REL}\",");
    assert_eq!(
        table.matches(&want).count(),
        1,
        "足迹里 `{want}` 那一行不是恰好一行"
    );
    let at = table.find(&want).unwrap();
    let from = table[..at]
        .rfind("TouchedFile {")
        .expect("那一格不在 `TouchedFile` 里");
    let cell = &table[from + "TouchedFile {".len()..];
    let cell = &cell[..cell
        .find("TouchedFile {")
        .unwrap_or(cell.len())
        .min(cell.find("],").unwrap_or(cell.len()))];
    assert!(
        cell.contains("effect: TouchEffect::RetiredLegacy"),
        "{cell}"
    );
    assert!(cell.contains("host: HostScope::Remote"), "{cell}");
}
