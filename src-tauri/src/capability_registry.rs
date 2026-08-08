//! ★★ **webview 的能力清单是所有 Rust 侧登记表的共同前提**
//! 〔audit-0805 08-08，Phase G 第 65 件〕。
//!
//! # 为什么这一张表值一条判据
//!
//! 本仓已经有三张「谁能碰这台机器」的登记表 —— 写盘（[`crate::write_site_registry`]）、
//! 远端执行（[`crate::exec_site_registry`]）、本机起进程（同上模块的 `spawn_sites`）。
//! 它们**都只扫 Rust 源码**，因为它们共享一个没写下来的前提：
//! **前端（webview）碰不到机器，只能 `invoke` 我们自己的命令。**
//!
//! 那个前提由 `src-tauri/capabilities/default.json` 一个文件决定。
//! 往它里面加一条 `fs:*` / `shell:*`，webview 就能绕过**上面每一张表** ——
//! 而 08-08 实测：往清单里加一条权限，**monitor 991 + vitest 1290 全绿**。
//!
//! ⇒ 本条把那个前提变成会红的判据：**权限只许在登记过的集合里**（默认拒绝）。
//!
//! # 它守什么、不守什么
//!
//! **守**：清单里出现没登记的权限 · 登记表里留下已经不在清单里的死行 ·
//! 窗口模式集合被改动（`viewer-*` 那类通配决定了哪些窗口继承这套权限）。
//!
//! **不守**：某条已登记权限**本身**危不危险（那要判语义）；
//! Tauri 自己的 `core:default` 展开成了什么（那是上游的事，版本升级时由
//! 「默认拒绝」在这里当场提问：`core:default` 变了要不要重新看一眼）。

#[cfg(test)]
mod tests {
    /// `(权限标识, 它为什么在这里)`。**默认拒绝**：清单里多一条就红。
    const ALLOWED: &[(&str, &str)] = &[
        (
            "core:default",
            "Tauri 核心默认集（窗口/事件/路径等基础能力）",
        ),
        ("opener:default", "用系统默认程序打开路径/URL 的默认集"),
        (
            "opener:allow-open-path",
            "★ 带 `path: **` 的宽授权：用户点「在文件管理器里打开」时要能开任意目录。\
             ⚠ 它只能**打开**，不能读写内容",
        ),
        ("dialog:default", "文件选择框（SFTP 上传/下载选本地路径）"),
        ("notification:default", "「Claude 完成一轮」系统通知（F42）"),
        ("core:window:allow-minimize", "最小化"),
        ("core:window:allow-set-fullscreen", "全屏"),
        ("core:window:allow-is-fullscreen", "查全屏状态"),
        ("core:window:allow-close", "关窗"),
    ];

    /// 继承这套权限的窗口模式。改动它 = 改动「谁拿到这些能力」。
    const WINDOWS: &[&str] = &["main", "viewer-*", "settings"];

    fn capability_json() -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities/default.json");
        std::fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("读不到 {p:?}：{e} —— 路径变了就把本条一起改"))
    }

    #[test]
    fn every_webview_permission_is_registered() {
        let raw = capability_json();
        let v: serde_json::Value =
            serde_json::from_str(&raw).expect("capabilities/default.json 不是合法 JSON");
        let perms = v
            .get("permissions")
            .and_then(|p| p.as_array())
            .expect("找不到 `permissions` 数组 —— 文件形状变了，本条此刻无效");
        // 权限可以是裸串，也可以是带 `identifier` 的对象。
        let names: Vec<String> = perms
            .iter()
            .map(|p| match p {
                serde_json::Value::String(s) => s.clone(),
                other => other
                    .get("identifier")
                    .and_then(|i| i.as_str())
                    .unwrap_or("<无 identifier>")
                    .to_string(),
            })
            .collect();
        // 抽取器自检：一条都没抽到 ⇒ 下面整条空转。
        assert!(
            names.len() >= 5,
            "只抽到 {} 条权限（08-08 实测 9）—— 抽取器坏了，本条此刻无效：{names:?}",
            names.len()
        );

        let extra: Vec<&String> = names
            .iter()
            .filter(|n| !ALLOWED.iter().any(|(a, _)| *a == n.as_str()))
            .collect();
        assert!(
            extra.is_empty(),
            "webview 的能力清单里出现了**没登记**的权限：{extra:?}\n\n\
             ⚠ 本仓那三张「谁能碰这台机器」的登记表（写盘 / 远端执行 / 本机起进程）\n\
             **都只扫 Rust 源码**，它们共享一个前提：**前端碰不到机器，只能 invoke 我们的命令**。\n\
             这个文件就是那个前提。加一条 `fs:*` / `shell:*`，webview 就绕过了上面每一张表。\n\
             真要加：在 `ALLOWED` 里写清它为什么必须有，并回头看看那三张表还成不成立。"
        );

        let stale: Vec<&str> = ALLOWED
            .iter()
            .map(|(a, _)| *a)
            .filter(|a| !names.iter().any(|n| n.as_str() == *a))
            .collect();
        assert!(
            stale.is_empty(),
            "登记表里这些权限清单里已经没有了：{stale:?}\n\
             删掉它们 —— 留着会让「已登记」看起来还覆盖着，实则那条已经不在了。"
        );

        // 窗口模式：谁继承这套权限。
        let wins: Vec<String> = v
            .get("windows")
            .and_then(|w| w.as_array())
            .expect("找不到 `windows` 数组")
            .iter()
            .filter_map(|w| w.as_str().map(str::to_string))
            .collect();
        assert_eq!(
            wins, WINDOWS,
            "继承这套权限的窗口模式变了。\n\
             ⚠ `viewer-*` 是通配：它决定了**将来每一个 viewer 窗口**都拿到这套能力。\n\
             加一个模式 = 把能力发给一类新窗口，要有人看一眼。"
        );
    }
}
