//! # 要求住址：`INVARIANTS §2.1`（`auto-launch.json` 两个字段的类）＋ `设计/01 §5 D3`（诚实的默认 ＝ 不作为）
//!
//! 核原文：`INVARIANTS §2.1` 那一行逐字「`enabled`=真相；`monitor_exe_path`=派生」—— `auto_launch.rs::save` /
//! `auto_launch.rs::load` 往返不丢这两格；`D3` 逐字「诚实的默认 ＝ 恒等 / 不作为 / 沿用调用者已有状态」—— 文件不在 ⇒ 不自启。
//! ⚠ 住址偏弱：这项功能本身（cc 函数在 monitor 没跑时拉起它）在设计篇没有行为节。
//! 〔JA1 点址 2026-09-24〕〔TL1 · 4C〕原先的 `roundtrip_serialize`（只测 serde 往返）退役：五刀里凡红它的，`save_and_load_roundtrip`
//! 都红（它走的链是后者的子链）；两条都放过的那一刀（字段改线上名）〔TL1 拍板 ④〕今天由 `the_wire_names_are_the_same_in_the_powershell_reader_and_the_rust_writer` 接住（读数在 `TL1.md` 件 1）。

use super::*;
use std::fs;

#[test]
fn default_when_missing() {
    let tmp = std::env::temp_dir().join(format!("ccm-auto-launch-{}.json", std::process::id()));
    let _ = fs::remove_file(&tmp);
    let cfg = load(&tmp);
    assert!(!cfg.auto_launch_enabled);
    assert!(cfg.monitor_exe_path.is_none());
}

#[test]
fn save_and_load_roundtrip() {
    let tmp = std::env::temp_dir().join(format!("ccm-auto-launch-rt-{}.json", std::process::id()));
    let _ = fs::remove_file(&tmp);
    let cfg = AutoLaunchConfig {
        auto_launch_enabled: true,
        monitor_exe_path: Some(r"C:\bar\monitor.exe".to_string()),
    };
    save(&tmp, &cfg).unwrap();
    let loaded = load(&tmp);
    assert!(loaded.auto_launch_enabled);
    assert_eq!(loaded.monitor_exe_path.unwrap(), r"C:\bar\monitor.exe");
    let _ = fs::remove_file(&tmp);
}

/// ★〔TL1 · 4C · 拍板 ④〕**`auto-launch.json` 的线上名，两种语言两向相等。**
///
/// 要求住址：`INVARIANTS §2.1` 那一行逐字「`enabled`=真相；`monitor_exe_path`=派生」—— 写它的是 monitor（Rust，
/// `auto_launch.rs::save`），读它的是 PowerShell 别名块里的 `__ccm_bind`（`scripts/cc.ps1.tpl`，monitor 没在跑时据它拉起 monitor）。
/// 两边各自改名，今天**哪一边都不会红**（`TL1.md` 件 1 的读数：字段改线上名那一刀两族都放过）——而读的那一侧读不到就是
/// 「开关开着却永远不自启」，没有任何报错。
///
/// 两侧**异源**：左 = 模板里 `$alCfg.<名>` 的名字集合（读模板文本）；右 = 真把一份配置**序列化**出来的 JSON 键集合
/// （走 serde 派生，`rename` 之类的属性一并算进去，不读 Rust 源码的字段名）。另钉文件名：模板 `Join-Path … '<名>'` 那个名字
/// == 生产段里 `join("<名>")` 的名字。
/// ⚠ 目录那一半（模板的 `$env:USERPROFILE\.claude\work` 与 monitor 的数据目录）本条不判。
/// ⚠ 模板内部的变量一致性本条不判（死值验现打：只把 `$alCfg = …` 那一句的变量改名、用处不改 ⇒ 本条不红，
/// 而 PowerShell 里那就是读不到 ⇒ 永不自启）—— 那是 PowerShell 脚本自己的对不上，不是跨语言的线上名。
#[test]
fn the_wire_names_are_the_same_in_the_powershell_reader_and_the_rust_writer() {
    const TPL: &str = include_str!("../../src/bridge/scripts/cc.ps1.tpl");
    let var = "$alCfg.";
    let mut read: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut rest = TPL;
    while let Some(at) = rest.find(var) {
        let tail = &rest[at + var.len()..];
        let name: String = tail
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        assert!(!name.is_empty(), "模板里 `$alCfg.` 后面抠不出名字");
        read.insert(name);
        rest = tail;
    }
    let written: std::collections::BTreeSet<String> = serde_json::to_value(AutoLaunchConfig {
        auto_launch_enabled: true,
        monitor_exe_path: Some("x".to_string()),
    })
    .expect("序列化")
    .as_object()
    .expect("是个对象")
    .keys()
    .cloned()
    .collect();
    // 反空真：两侧都非空（模板那一段被挪走 / 改了变量名 ⇒ 左边空 ⇒ 相等判断会退化）。
    assert!(
        !read.is_empty(),
        "模板里一个 `$alCfg.` 都没抠到 —— 读那一段挪了或改了变量名，本条在空转"
    );
    assert_eq!(
        read, written,
        "PowerShell 那一侧读的字段 ≠ monitor 写出去的字段：改名要两边同拍改（读不到 = 开关开着却永远不自启、不报错）"
    );
    // 文件名：模板 `Join-Path $ccmDir '<名>'` == 生产段 `join("<名>")`。
    let key = "Join-Path $ccmDir '";
    let at = TPL
        .find(&format!("{key}auto"))
        .expect("模板里找不到 auto-launch 那个文件名");
    let tail = &TPL[at + key.len()..];
    let file = &tail[..tail.find('\'').expect("文件名没收尾")];
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/auto_launch.rs"));
    assert!(
        prod.contains(&format!("join(\"{file}\")")),
        "模板读 `{file}`，monitor 生产段里没有 `join(\"{file}\")` —— 文件名两边对不上"
    );
}
