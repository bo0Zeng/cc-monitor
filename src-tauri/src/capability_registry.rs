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

    /// 读 `src-tauri/` 下的一个同级配置文件。
    fn capability_json_sibling(name: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
        std::fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("读不到 {p:?}：{e} —— 路径变了就把本条一起改"))
    }

    fn capability_json() -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities/default.json");
        std::fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("读不到 {p:?}：{e} —— 路径变了就把本条一起改"))
    }

    /// ★★ **前端的执行面（CSP + 全局 Tauri）也是那三张表的前提**
    /// 〔audit-0805 08-08，Phase G 第 66 件〕。
    ///
    /// 上面那条钉的是「webview 被授予哪些能力」。**还有一半**：webview 里跑的
    /// **代码本身**从哪来。本产品渲染的是**不受信的会话文本**（Claude 的输出、
    /// 远端 tmux 的 capture-pane），一旦脚本源被放开，注入的内容就能直接 `invoke`
    /// 我们所有的命令 —— 那同样绕过写盘 / 远端执行 / 起进程那三张表。
    ///
    /// 今天的形态是对的（CSP 无 `script-src 'unsafe-inline'`、无远端源，
    /// `withGlobalTauri: false`），**但没人钉**：08-08 实测把 CSP 放开成
    /// `default-src 'self' 'unsafe-inline' *` 并打开 `withGlobalTauri`，
    /// **monitor 992 + vitest 1290 全绿**。
    ///
    /// ⚠ 只钉**会扩大执行面**的那几件，不逐字钉整条 CSP：
    /// 逐字钉会把「加一个 `img-src` 源」这种无关改动也判红，
    /// 而误红最省事的消法是把判据放宽 —— 那条路本工作区走过太多次了。
    #[test]
    fn the_webview_execution_surface_stays_closed() {
        let raw = capability_json_sibling("tauri.conf.json");
        let v: serde_json::Value =
            serde_json::from_str(&raw).expect("tauri.conf.json 不是合法 JSON");
        let app = v
            .get("app")
            .expect("找不到 `app` —— 配置形状变了，本条此刻无效");

        // ① 全局 Tauri 注入：打开的话，任何在页面里跑起来的脚本都能直接 `invoke`。
        let global = app.get("withGlobalTauri").and_then(|g| g.as_bool());
        assert_eq!(
            global,
            Some(false),
            "`withGlobalTauri` 不再是 `false`（实得 {global:?}）。\n\
             ⚠ 打开它 = 把 `invoke` 挂到 `window` 上 —— 页面里任何跑起来的脚本都能调\n\
             我们的全部命令，绕过写盘 / 远端执行 / 起进程那三张登记表。"
        );

        // ② CSP 必须存在，且不许放开**脚本**执行面。
        let csp = app
            .get("security")
            .and_then(|s| s.get("csp"))
            .and_then(|c| c.as_str())
            .expect("`app.security.csp` 不见了 —— 没有 CSP 等于执行面全开");
        assert!(
            csp.contains("default-src 'self'"),
            "CSP 的 `default-src` 不再是 `'self'`（实得 {csp:?}）—— 那是兜底源，放开它等于全放开"
        );
        for bad in [
            "script-src 'unsafe-inline'",
            "'unsafe-eval'",
            "default-src 'self' *",
        ] {
            assert!(
                !csp.contains(bad),
                "CSP 里出现了 {bad:?}：{csp}\n\
                 ⚠ 本产品渲染的是**不受信的会话文本**（Claude 输出、远端 capture-pane）。\n\
                 脚本执行面一放开，注入的内容就能直接 `invoke` 我们的命令。\n\
                 `style-src 'unsafe-inline'` 是**刻意允许**的（样式不是执行面），别把它一起收掉。"
            );
        }
        // 常驻自检：那条刻意允许的样式豁免必须还在，否则上面这组禁词是在一个
        // 「CSP 已经被整个换掉」的文本上空转。
        assert!(
            csp.contains("style-src 'self' 'unsafe-inline'"),
            "CSP 里那条**刻意允许**的 `style-src 'unsafe-inline'` 不见了（实得 {csp:?}）—— \
             要么 CSP 被整个换掉了（那上面几条禁词就是在空转），要么样式策略真改了。两种都要人看一眼。"
        );
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
    /// ★★ **构建／安装期的执行面**〔audit-0805 08-08，Phase G 第 83 件〕。
    ///
    /// 上面几条钉的是**运行时**谁能扩大执行面（webview 权限 · CSP · 全局 Tauri）。
    /// 本条钉**更早的那一段**：从 `npm install` 到 `cargo build` 到 `tauri build`，
    /// 有哪些地方能让代码在开发者机器上跑起来。08-08 逐个量过，三处**全是零判据**：
    ///
    /// | 面 | 今天 | 谁在管 |
    /// |---|---|---|
    /// | `src-tauri/build.rs` | 起 `sh`/`git`、往 `OUT_DIR` 写 | **08-08 刚并进** `write_site_registry`（第 82 件） |
    /// | `tauri.conf.json` 的 `before*Command` | `npm run dev` / `npm run build` | 本条 |
    /// | `package.json` 的 npm **生命周期钩子** | 一个都没有 | 本条 |
    /// | `.cargo/config.toml` 的 `runner` | 文件不存在 | 本条 |
    ///
    /// ★ 后两行钉的是**「今天没有」这件事**。它们的危险恰恰在于「加一条就自动执行」：
    /// `postinstall` 在**每一次 `npm install`** 上跑（含 CI、含任何人 clone 之后第一件事）；
    /// `[target.*.runner]` 会让 **`cargo test` 去执行任意二进制**。
    /// 「今天没有」不是判据 —— 没人钉的话，加进来的那天没有任何信号（本会话反复量到的
    /// 「没人守着」与「碰巧没坏」是两回事）。
    #[test]
    fn the_build_time_execution_surface_stays_registered() {
        // ① `tauri.conf.json` 的构建前置命令：登记值 + 理由。
        const BEFORE: &[(&str, &str, &str)] = &[
            ("beforeDevCommand", "npm run dev", "起前端 dev server；`tauri dev` 会执行它"),
            (
                "beforeBuildCommand",
                "npm run build",
                "打包前构建前端产物；`tauri build` 会执行它 —— 改这里等于改「发版时在构建机上跑什么」",
            ),
        ];
        let v: serde_json::Value =
            serde_json::from_str(&capability_json_sibling("tauri.conf.json"))
                .expect("tauri.conf.json 不是合法 JSON");
        let build = v
            .get("build")
            .expect("`tauri.conf.json` 里没有 `build` 段 —— 形状变了，本条会零命中地绿");
        for (key, want, why) in BEFORE {
            let got = build.get(key).and_then(|x| x.as_str()).unwrap_or("<缺失>");
            assert_eq!(
                got, *want,
                "`build.{key}` 现在是 {got:?}，登记的是 {want:?}（{why}）。\n\
                 ★ 这一格是**构建期的执行面**：`tauri dev`/`tauri build` 会原样执行它。\n\
                 真要改，就把新值和理由一起写进本条的 `BEFORE` 表 —— 改值不改表 = 没人看过。"
            );
        }

        // ② npm 生命周期钩子：**默认拒绝**（今天一个都没有）。
        const LIFECYCLE: &[&str] = &[
            "preinstall",
            "install",
            "postinstall",
            "prepare",
            "prepublish",
            "prepublishOnly",
            "prepack",
            "postpack",
        ];
        let pkg: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("仓根")
                    .join("package.json"),
            )
            .expect("读不到 package.json"),
        )
        .expect("package.json 不是合法 JSON");
        let scripts = pkg
            .get("scripts")
            .and_then(|s| s.as_object())
            .expect("`package.json` 里没有 `scripts` —— 形状变了，本条会零命中地绿");
        // 抽取器自检：脚本表塌了的话下面那条就是废话。
        assert!(
            scripts.len() >= 20,
            "`package.json` 只解析出 {} 条 script（08-08 实测 60+）—— 读法坏了",
            scripts.len()
        );
        let hooks: Vec<&str> = LIFECYCLE
            .iter()
            .copied()
            .filter(|k| scripts.contains_key(*k))
            .collect();
        assert!(
            hooks.is_empty(),
            "`package.json` 里出现了 npm **生命周期钩子**：{hooks:?}\n\
             ★ 它们**不需要谁去调**：`postinstall`/`prepare` 在每一次 `npm install` 上自动跑 ——\n\
             包括 CI，也包括任何人 clone 之后的第一条命令。⇒ 那是本仓最省事的一条\n\
             「让代码在别人机器上执行」的路，而在本条之前**没有任何判据看着它**。\n\
             真要加：把它和理由写进本条（并想清楚「为什么它不能是一条普通的 `npm run xxx`」）。"
        );

        // ③ `.cargo/config.toml`：它能设 `runner`，让 `cargo test` 去执行任意二进制。
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf();
        let cargo_cfgs: Vec<String> = [
            ".cargo/config.toml",
            ".cargo/config",
            "src-tauri/.cargo/config.toml",
            "src-tauri/.cargo/config",
            "remote-daemon-proto/.cargo/config.toml",
        ]
        .iter()
        .filter(|rel| root.join(rel).exists())
        .map(|rel| rel.to_string())
        .collect();
        assert!(
            cargo_cfgs.is_empty(),
            "仓里出现了 cargo 配置文件：{cargo_cfgs:?}\n\
             ★ 它能设 `[target.*.runner]` —— 那会让 **`cargo test` 把测试二进制交给另一个程序去跑**，\n\
             也能设 `rustflags`/`linker`。本仓今天一个都没有，所以「跑测试」这件事没有中间人。\n\
             真要加（比如交叉测试需要 runner）：写进本条并说清它执行的是什么。"
        );
    }
}
