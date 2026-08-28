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

    /// cargo 配置里**真的会扩大执行面**的三个键（`K-G2` 08-28 收窄后的性质本身）：
    ///
    /// - `runner` —— `cargo test` / `cargo run` 把产物交给它去执行。「跑测试」的中间人就是它。
    /// - `linker` —— 链接期换掉链接器：构建机上真正跑起来的是它指定的那个程序。
    /// - `rustflags` —— 原样透传给 rustc 的旗标；`-C linker=…` / `-Z …` 都能从这一格进来。
    ///
    /// ⚠ **不守**：`rustc-wrapper` / `rustdocflags` / `[alias]` 这些同族的键没进本表
    /// （`K-G2` 的射程逐字只有上面三个；那一族归跟进件 `己1-f36`）。
    /// `[alias]` 里塞 `--config build.rustflags=…` 这一形会被下面的整词匹配顺带逮到，
    /// 但那是**副产品，不是判据** —— 别当它有覆盖。
    const CARGO_EXEC_KEYS: &[&str] = &["runner", "linker", "rustflags"];

    /// **不是执行面键，是「会让本条看瞎」的键**〔`K-G2` `D1` 回修，08-28〕。
    ///
    /// `include` 让一份 cargo 配置**把另一份文件整个拉进来**，而那份文件可以在
    /// [`CARGO_CFG_DIRS`] × [`CARGO_CFG_NAMES`] 那 6 格**之外的任意位置**。
    /// 08-28 在 cargo 1.96.1 stable 上实测（一次性 crate + `[alias]` 探针，带非空对照）：
    /// `include = ["../hidden/x.toml"]` ⇒ 被拉进来的别名**真生效**；把 `include` 那行删掉
    /// ⇒ 回到 `no such command`。⚠ 值必须是**列表**，写成裸字符串 cargo 会报
    /// `expected a list of strings or a list of tables` —— 别把那个报错读成「include 不支持」。
    ///
    /// ⇒ 本条**看不进**被拉进来的文件。按 fail-closed：**出现 `include` 就红**，
    /// 并在报文里说清红的理由是「看不见」而不是「你设了执行面」。
    /// ⚠ 这**不是**扩射程（射程仍是上面那三个执行键），是不许本条被蒙住眼睛。
    const CARGO_BLINDING_KEYS: &[&str] = &["include"];

    /// 本仓自己会把 cargo 的 cwd 落在这几个目录（相对仓根）。依据见
    /// [`the_build_time_execution_surface_stays_registered`] 头注那张表。
    const CARGO_CFG_DIRS: &[&str] = &[".", "src-tauri", "remote-daemon-proto"];

    /// cargo 认的两种配置文件名（无扩展名那个是老写法，仍然照读）。
    const CARGO_CFG_NAMES: &[&str] = &["config.toml", "config"];

    /// 剥掉一份 cargo 配置里的**注释**（`#` 到行尾；字符串里的 `#` 不算注释）。
    ///
    /// ⚠ 刻意**只剥注释、不剥字符串内容**：TOML 的键可以带引号（`"runner" = "sh"`），
    /// 内联表的值里也住着键（`target = { x = { runner = "sh" } }`）——
    /// 把字符串一起剥掉就会在这两处**漏红**。代价：值里恰好出现那三个整词也会红。
    ///
    /// ⚠⚠ 本条的**方向**是「宁可误红」，但**别把方向读成结论** ——
    /// 08-28 `K-G2` `D1` 现场证过它当时**真的漏红**（转义键那一形，见
    /// [`decode_toml_escapes`]）。今天补了转义解码，但「还有没有别的写法」这个问题
    /// **没有穷举的分母**：本条量过哪几种、剩哪几种不守，逐条写在
    /// [`the_build_time_execution_surface_stays_registered`] 头注的「不守什么」栏。
    ///
    /// ⚠ 边界：三引号多行字符串（`"""` / `'''`）本剥法**不认**（引号状态逐行重置），
    /// 那种字符串里的 `#` 会被当成注释切掉。cargo 配置里它实际不出现；
    /// 写下这一条是为了不假装它被处理过。
    fn strip_toml_comments(src: &str) -> String {
        let mut out = String::with_capacity(src.len());
        for line in src.lines() {
            let mut quote: Option<char> = None;
            let mut escaped = false;
            let mut cut = line.len();
            for (i, c) in line.char_indices() {
                if quote.is_some() {
                    if escaped {
                        escaped = false;
                    } else if c == '\\' && quote == Some('"') {
                        escaped = true;
                    } else if quote == Some(c) {
                        quote = None;
                    }
                } else if c == '"' || c == '\'' {
                    quote = Some(c);
                } else if c == '#' {
                    cut = i;
                    break;
                }
            }
            out.push_str(&line[..cut]);
            out.push('\n');
        }
        out
    }

    /// 把 TOML 基本串里**能拼出字母**的那几种转义解开〔`K-G2` `D1` 回修，08-28〕。
    ///
    /// ★ 这是 `D1` 逮到的那个洞的修法。TOML 的键可以是**带转义的基本串**，
    /// 而 cargo 认的是**解码后**的键名 —— `"runner"` 在 cargo 眼里逐字就是 `runner`。
    /// `D1` 在真判据、真工作树上跑过：那份配置让 `cargo test` 把测试二进制交给了
    /// `/bin/echo`，**判据根本没被执行**，而它当时是绿的。三个键同形，`linker`/`rustflags` 一样。
    ///
    /// **哪几种转义**（08-28 在 cargo 1.96.1 stable 上逐个实测，`[alias]` 探针 + 非空对照）：
    /// `\uXXXX` · `\UXXXXXXXX` · **`\xXX`** 三种 cargo 都认（`\x` 是 TOML 1.1 的，
    /// ⚠ 派工单只点了前两种 —— 第三种是我自己量出来的）。表头里也认
    /// （`["alias"]` 实测生效）。剩下的转义（`\b \t \n \f \r \" \\`）**拼不出字母**，
    /// 不进本函数。三引号多行串**不能当键**（实测 cargo 直接
    /// `could not load Cargo configuration`）⇒ 不是一条路。
    ///
    /// ⚠ 刻意**不**特判 `\\`：`"run\\u006Eer"` 在 TOML 里其实是字面量 `runner`、
    /// cargo 不认它，而本函数会把它解成 `runner` 从而**误红**。那是 fail-closed 的方向，
    /// 按本条的定盘（宁可误红）留着。
    fn decode_toml_escapes(src: &str) -> String {
        let chars: Vec<char> = src.chars().collect();
        let mut out = String::with_capacity(src.len());
        let mut i = 0;
        while i < chars.len() {
            let width = match chars.get(i..i + 2) {
                Some(['\\', 'u']) => 4,
                Some(['\\', 'U']) => 8,
                Some(['\\', 'x']) => 2,
                _ => 0,
            };
            if width > 0 {
                if let Some(hex) = chars.get(i + 2..i + 2 + width) {
                    let s: String = hex.iter().collect();
                    let decoded = u32::from_str_radix(&s, 16).ok().and_then(char::from_u32);
                    if let Some(ch) = decoded {
                        out.push(ch);
                        i += 2 + width;
                        continue;
                    }
                }
            }
            out.push(chars[i]);
            i += 1;
        }
        out
    }

    /// `text` 里有没有**整词**的 `key`（前后不许是字母/数字/`_`/`-`，
    /// 否则 `my-runner-name` 这类会假红）。
    fn word_in(text: &str, key: &str) -> bool {
        fn is_word(c: char) -> bool {
            c.is_alphanumeric() || c == '_' || c == '-'
        }
        text.match_indices(key).any(|(i, _)| {
            let before = text[..i].chars().next_back();
            let after = text[i + key.len()..].chars().next();
            !before.is_some_and(is_word) && !after.is_some_and(is_word)
        })
    }

    /// **键位置**出现反斜杠没有〔`K-G2` `D1` 回修的兜底，08-28〕。
    ///
    /// ★ 这一条是为了让本条的结论**不靠「我把转义种类数全了」**。论证：
    /// TOML 的键只有三种写法 —— 裸键 · 字面串键（`'…'`，**不认转义**）· 基本串键（`"…"`）。
    /// 前两种里那几个字母只能是**明文**，原文那一遍就逮得到；第三种要把字母藏起来
    /// **必须用反斜杠**。⇒ 键位置只要出现反斜杠就红，**不管那是今天认识的
    /// `\u`/`\U`/`\x` 还是明天新加的哪一种**。
    ///
    /// **「键位置」的近似**：每行第一个 `=` 之前的那段（没有 `=` 的行 —— 表头 `[…]`、
    /// 多行值的续行 —— 整行算）。TOML 要求键与它的 `=` 同行，所以这个近似对
    /// **顶层键 / 点分键 / 表头**都成立。
    /// ⚠ **盖不住内联表里的嵌套键**（`x = { "runer" = "sh" }`，反斜杠在第一个 `=` 之后）——
    /// 那一形今天靠 [`decode_toml_escapes`] 认识的那三种转义兜着，
    /// 「内联表 + 将来某种新转义」是本条**已知的、没兜住的**一格。
    /// ⚠ 代价：多行字符串**值**的续行里有反斜杠、且那一行没有 `=`，会误红。
    /// cargo 配置里这形状实际不出现；方向照本条的定盘（宁可误红）留着。
    fn backslash_in_key_position(text: &str) -> bool {
        text.lines().any(|line| {
            let head = match line.find('=') {
                Some(i) => &line[..i],
                None => line,
            };
            head.contains('\\')
        })
    }

    /// `keys` 里哪几个在这份配置文本里出现过。
    ///
    /// ⚠ **扫两遍取并集**：原文一遍、[`decode_toml_escapes`] 解码后再一遍。
    /// 两遍都要 —— 解码只会**新增**字符，可能把原本挨着 `\` 的整词边界吃掉
    /// （`runnerA` 解完成了 `runnerA`，整词就不成立了），所以原文那一遍不能省。
    fn keys_in(text: &str, keys: &[&'static str]) -> Vec<&'static str> {
        let decoded = decode_toml_escapes(text);
        keys.iter()
            .copied()
            .filter(|key| word_in(text, key) || word_in(&decoded, key))
            .collect()
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
    /// | cargo 配置里的 `runner`/`linker`/`rustflags` | 六格里一份都没设过 | 本条（08-28 起判**内容**） |
    ///
    /// ★ 后两行钉的是**「今天没有」这件事**。它们的危险恰恰在于「加一条就自动执行」：
    /// `postinstall` 在**每一次 `npm install`** 上跑（含 CI、含任何人 clone 之后第一件事）；
    /// `[target.*.runner]` 会让 **`cargo test` 去执行任意二进制**。
    /// 「今天没有」不是判据 —— 没人钉的话，加进来的那天没有任何信号（本会话反复量到的
    /// 「没人守着」与「碰巧没坏」是两回事）。
    ///
    /// # ③ 那一格 08-28 被改过：**判内容，不判文件在不在**〔`K-G2`〕
    ///
    /// 原来那一格手抄了 **5** 条路径、断言这 5 条一个都不存在。**它的人群与它自己声称的性质对不上，
    /// 两个方向都不对**：
    ///
    /// - **人群比性质大**：一份只有 `[profile.dev]`（`debug` / `split-debuginfo` 那类
    ///   构建瘦身）的配置**执行不了任何东西**，却照样把它判红。08-27 真发生过：
    ///   用户本机那份瘦身配置搬进 `src-tauri/.cargo/config.toml` 之后，
    ///   这一条在用户主树上**恒红**，而它守的执行面一寸也没被碰。
    /// - **人群比性质小**：它列了 5 个路径，漏掉 `remote-daemon-proto/.cargo/config`
    ///   —— 那一格 cargo **照读**（见下）。
    ///
    /// ⇒ 现在①**收窄性质**：读文件内容，只有真的设了 `runner`/`linker`/`rustflags`
    /// （**含用 TOML 转义拼出来的写法**，见 [`decode_toml_escapes`]）才红 ——
    /// 剥掉注释、解开转义之后**都不含这几个整词**的配置一律放行（`[profile.*]`
    /// 那类构建瘦身正是这一类）；②**补齐人群**到下面那 6 格。
    /// ⚠ **不许**改成「豁免某个文件名 / 某个路径」—— 那是放宽人群，不是收窄性质。
    ///
    /// ## ⚠ 这次收窄**引入过一个洞**，记在这里〔`D1` 打回，08-28〕
    ///
    /// 第一版只做了「剥注释 + 整词扫原文」。`D1` 用
    /// `"runner" = "/bin/echo"` 在**真判据、真工作树**上跑通了绕过：
    /// cargo 认**解码后**的键名，把测试二进制交给了 `/bin/echo`，
    /// **本条根本没被执行，而它当时是绿的**。三个键同形。
    /// ★ 而**旧判据在同一份文件上必红** —— 旧的只问「文件在不在」。
    /// ⇒ 那一版**净额是变宽的**：收窄性质买到了正确性，同时丢了一格覆盖。
    ///
    /// ★★ **一般教训**（`K-G2` `D1` 裁定，写在这里给下一个收窄守卫的人）：
    /// **把一条守卫从「看一个结构性事实」收窄成「看一段文本里有没有那几个词」，
    /// 就同时买进了那个文本格式的整个转义面。**
    /// 前者粗，但**伪造不了**；后者准，但继承了格式的所有写法花样。
    /// ⇒ 收窄之前先问：新落点是**事实**还是**文本模式**？是后者就得把那个格式的
    /// 写法花样量一遍，并把量过的与没量的分开写下来。
    ///
    /// ## 那 6 格是怎么来的：**3 个发起面 × cargo 认的 2 种文件名**
    ///
    /// cargo 读哪一份 `.cargo/config`，由**发起命令的 cwd** 决定，并**沿 cwd 往上找**。
    /// 08-28 用一次性探针 crate 实测（`[build] target-dir` 观察落点，带非空对照与反向对照）：
    /// 配置放在 cwd 那一级**生效**，哪怕 manifest 在子目录（`--manifest-path sub/…`）；
    /// 同一份配置放进那个子目录、cwd 留在上面 ⇒ **不生效**（落回默认 `sub/target`）；
    /// 把 cwd 移进子目录 ⇒ **又生效**。⇒ 人群 = **本仓自己会把 cwd 落在哪几个目录**。
    ///
    /// **尺子**（08-28 现打）：`git ls-files` 里的 `*.sh` / `*.ps1` / `*.yml` / `package.json`
    /// 全扫一遍 `cargo <子命令>`，**逐处读它的 cwd**。出现过的 cwd 只有下面 3 个：
    ///
    /// | 发起面 | 谁从这里发起 cargo |
    /// |---|---|
    /// | 仓根 `.` | `ci.yml:662`（`e2e-tmux-rust` job **没有** `working-directory` ⇒ cwd = 仓根）· `scripts/run.ps1:39`。它同时是下面两个的**祖先** —— 往上找一定路过 |
    /// | `src-tauri/` | `scripts/gate.sh:105` · `package.json` 的 `gen:types` · `ci.yml:26`（`rust` job 的 `working-directory`）· `ci.yml:295` · `e2e/` 四个脚本共 6 处（`tmux-guarded-acceptance.sh:21` · `usage-probe-acceptance.sh:23`/`:129` · `local-backend-supervise.sh:78`/`:86` · `p3t-local-tmux.sh:125`） |
    /// | `remote-daemon-proto/` | `scripts/gate.sh:157` · `ci.yml:160`（`daemon` job）· `release.yml:42`/`:152`/`:271` · `e2e/daemon-fork-session.sh:24` |
    ///
    /// ⚠ **尺子没覆盖到的**（写下来免得把它读成穷举）：① `cargo tauri build` 那种**由工具
    /// 再去起 cargo** 的，cwd 由 tauri CLI 定，本条没现打；② 人手临时 `cd` 到任意目录敲的
    /// cargo —— 那个分母没人数得出，也不是一条判据守得住的。
    /// ③ `scripts/verify-committed-state.sh:60`/`:61`/`:65` 在**另开的临时工作树**里跑，
    /// 相对目录仍是这两个，不新增发起面。
    ///
    /// **为什么是 6 不是 7**：往下没有第 4 个发起面 —— `src-tauri/crates/*` 与
    /// `src-tauri/vendor/*` 里放一份配置，**上面那张表里的命令一条都读不到它**
    /// （分母就是那张表 = 上面那把尺子量出来的全部；那些命令的 cwd 都停在 `src-tauri/`，
    /// 靠 `-p`/`--workspace` 选包，而 cargo 不往下找）。
    /// 往上也没有 —— `~/.cargo/config.toml` / `$CARGO_HOME` / `cargo --config` 命令行
    /// 都在仓外，**一次 commit 改不到**，不在本条的人群里（那是另一类威胁模型）。
    /// 文件名两种：`config.toml` 与**无扩展名** `config`。后者 cargo 照读 —— 08-28 实测
    /// 报文逐字 `deprecated in favor of config.toml`，且 alias **真生效**；
    /// 改名成 `config.toml` 行为相同；两个都删 ⇒ `no such command`。
    ///
    /// ⚠ 加一个发起面（新 crate、某个 job 换 cwd）就回这张表补一格：
    /// 下面 `slots.len() == 6` 那条自检会在你只改数组不改本注时把你叫回来。
    ///
    /// ## ③ **不守什么**（`D1` 回修时逐条量出来的；铁律 14：诚实边界落进被守对象）
    ///
    /// ⚠ 下面是**我量过的那几条**，不是「所有绕法」的穷举 —— 那个分母没人给得出。
    /// 量法一律是：`scratchpad` 里一次性 crate + `[alias]` 探针，带非空对照与反向对照，
    /// cargo **1.96.1 stable**（换版本要重量）。
    ///
    /// | 不守的东西 | 08-28 现打的读数 |
    /// |---|---|
    /// | 同族的执行面键：`rustc-wrapper` · `rustdocflags` · `build.rustc` | 射程逐字只有那三个，这一族归跟进件 `己1-f36` |
    /// | **环境变量那条配置源**：`CARGO_TARGET_<TRIPLE>_RUNNER` · `RUSTFLAGS` · `CARGO_BUILD_RUSTFLAGS` | cargo 认它们，但它们**不是仓里的文件** ⇒ 本条的人群是文件，够不着。⚠ CI 的 yml 能设环境变量，那是另一个面，本条不声称守它 |
    /// | `cargo --config <k>=<v>` 命令行 · 仓外的 `$CARGO_HOME/config.toml` | 一次 commit 改不到 ⇒ 不在人群里 |
    /// | **值**里的写法花样 | 本条对**值**只是整词顺带命中（`[alias]` 里塞 `--config …rustflags=…` 会被逮到）。那是副产品，不是判据 |
    /// | 多行基本串的续行 `\` 拼词 | 只对**值**成立：三引号串**不能当键**（实测 cargo 直接 `could not load Cargo configuration`）⇒ 键那一侧不是路 |
    ///
    /// ⚠⚠ **一条本条自己盖不住的**：`src-tauri/` 那一格上**真生效**的 `runner`
    /// 会让本条**自己不被执行**（`D1` 现场就是这样：测试二进制被交给 `/bin/echo`）。
    /// ⇒ 兜住那一形的**不是本条**，是 `scripts/gate.sh` 的 `run_gate_sum`
    /// 采集面自检：跑到的包数 ≠ 8 就红（现打读过那段代码）。**别把这一格算到本条头上。**
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

        // ③ cargo 配置：红不红取决于**文件里有没有那三个键**，不取决于文件在不在。
        //    人群 = 3 个发起面 × cargo 认的 2 种文件名 = 6 格（依据见本条头注那张表）。
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf();
        let mut slots: Vec<String> = Vec::new();
        for dir in CARGO_CFG_DIRS {
            for name in CARGO_CFG_NAMES {
                slots.push(format!("{dir}/.cargo/{name}"));
            }
        }
        assert_eq!(
            slots.len(),
            6,
            "本条的人群应当是 3 个发起面 × cargo 认的 2 种文件名 = 6 格，实得 {}：{slots:?}\n\
             ⚠ 动了那两个数组就回本条头注，把那张「谁从这里发起 cargo」的表一起改 ——\n\
             只改数组不改头注 = 下一个人读不出「为什么是 6 不是 7」。",
            slots.len()
        );

        // 抽取器自检：探针坏了的话，下面整条就是**零命中地绿**。
        // ⚠ 探针③ 是 `D1` 那个洞的**常驻**版本 —— 它一旦变绿，本条就又瞎了。
        let probe_exec = "[target.'cfg(all())']\nrunner = \"sh\"  # 这里的 linker 是注释\n";
        let probe_profile = "[profile.dev]\ndebug = \"line-tables-only\"\n";
        let probe_escaped = "[target.x86_64-unknown-linux-gnu]\n\"run\\u006Eer\" = \"/bin/echo\"\n";
        let probe_hex = "[build]\n\"rustfla\\x67s\" = []\n";
        let probe_backslash_value = "[profile.dev]\nrustc = \"C:\\\\tools\\\\x.exe\"\n";
        assert_eq!(
            keys_in(&strip_toml_comments(probe_exec), CARGO_EXEC_KEYS),
            vec!["runner"],
            "探针①：真设了 `runner` 的配置必须被逮到，且注释里的 `linker` 不算"
        );
        assert!(
            keys_in(&strip_toml_comments(probe_profile), CARGO_EXEC_KEYS).is_empty(),
            "探针②：只有 `[profile.*]` 的构建瘦身配置必须放行 —— 那正是 `K-G2` 收窄掉的那一半"
        );
        assert_eq!(
            keys_in(&strip_toml_comments(probe_escaped), CARGO_EXEC_KEYS),
            vec!["runner"],
            "探针③：**用 TOML 转义拼出来的键**必须被逮到。\n\
             这一格是 `K-G2` `D1` 现场逮到的洞：cargo 认解码后的键名，\n\
             而当时的整词扫看的是原文 ⇒ `cargo test` 被交给了 `/bin/echo`，本条却是绿的。"
        );
        assert_eq!(
            keys_in(&strip_toml_comments(probe_hex), CARGO_EXEC_KEYS),
            vec!["rustflags"],
            "探针④：`\\xXX`（TOML 1.1）那种写法 cargo 1.96.1 也认，本条也要认"
        );
        assert!(
            keys_in(&strip_toml_comments(probe_backslash_value), CARGO_EXEC_KEYS).is_empty(),
            "探针⑤：**值**里有反斜杠（Windows 路径）不许误红 —— 本条判的是键，不是有没有 `\\`"
        );
        assert!(
            backslash_in_key_position(&strip_toml_comments(probe_escaped)),
            "探针⑤b：键位置那道**兜底**必须逮到转义键 —— 它是「不靠数全转义种类」的那一半"
        );
        assert!(
            !backslash_in_key_position(&strip_toml_comments(probe_backslash_value)),
            "探针⑤c：值里的反斜杠不许触发兜底，否则 Windows 路径会误红成灾"
        );
        let probe_include = "include = [\"../elsewhere/x.toml\"]\n[profile.dev]\ndebug = false\n";
        assert!(
            keys_in(&strip_toml_comments(probe_include), CARGO_EXEC_KEYS).is_empty(),
            "探针⑥前半：`include` 不是执行面键，别把它算进 `CARGO_EXEC_KEYS`"
        );
        assert_eq!(
            keys_in(&strip_toml_comments(probe_include), CARGO_BLINDING_KEYS),
            vec!["include"],
            "探针⑥后半：`include` 必须被逮到 —— 它能把 6 格之外的文件拉进来，本条读不到那份"
        );

        let offenders: Vec<String> = slots
            .iter()
            .filter_map(|rel| {
                let p = root.join(rel);
                if !p.is_file() {
                    return None;
                }
                let Ok(raw) = std::fs::read_to_string(&p) else {
                    return Some(format!("{rel}（存在但读不出文本，本条看不了它的内容）"));
                };
                let text = strip_toml_comments(&raw);
                let mut why: Vec<String> = Vec::new();
                let exec = keys_in(&text, CARGO_EXEC_KEYS);
                if !exec.is_empty() {
                    why.push(format!("设了执行面的键 {exec:?}"));
                }
                let blind = keys_in(&text, CARGO_BLINDING_KEYS);
                if !blind.is_empty() {
                    why.push(format!("设了 {blind:?} —— 本条看不进它拉进来的文件"));
                }
                if backslash_in_key_position(&text) {
                    why.push("键位置有反斜杠 —— 转义拼键是已知的绕法，一律按红处理".to_string());
                }
                if why.is_empty() {
                    None
                } else {
                    Some(format!("{rel}：{}", why.join("；")))
                }
            })
            .collect();
        assert!(
            offenders.is_empty(),
            "cargo 配置里出现了本条不许出现的键：{offenders:?}\n\
             ★ `runner` 会让 **`cargo test` 把测试二进制交给另一个程序去跑**；\n\
             `linker` 换掉构建机上真正跑的链接器；`rustflags` 原样透传给 rustc\n\
             （`-C linker=…` 也能从那里进来）。本仓今天一份都没设，所以「跑测试」没有中间人。\n\
             ★ `include` 是另一回事：它**不是**执行面，但它能把 6 格之外的任意文件拉进来，\n\
             而本条只读那 6 格 ⇒ 按 fail-closed 判红，理由是「看不见」而不是「你设了什么」。\n\
             ⚠ 本条判的是**内容**不是**文件在不在**，而且认**解码后**的键名\n\
             （`\"run\\u006Eer\"` 一样算 `runner`）：剥掉注释后不含这些整词的配置一律放行\n\
             （`[profile.*]` 那类构建瘦身正是这一类）。\n\
             真要加（比如交叉测试需要 runner）：写进 `CARGO_EXEC_KEYS` 旁边说清它执行的是什么。\n\
             ⚠ **不许**改成豁免某个文件名或某个路径 —— 那是放宽人群，不是收窄性质。"
        );
    }
}
