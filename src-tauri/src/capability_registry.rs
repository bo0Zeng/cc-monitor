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
    /// ★★ **这一步为什么必须 fail-closed，而不是「再补一个例」**〔`D2` 回修，08-28；
    /// `K21` 收紧后的第一条〕。
    ///
    /// `#` **不是**注释只有一种情形：它在字符串里。而 TOML 的字符串恰好**四种** ——
    /// 基本串 `"…"` · 字面串 `'…'`（这两种**规范上不能跨行**）·
    /// 多行基本串 `"""…"""` · 多行字面串 `'''…'''`（这两种**能**跨行）。
    /// 本剥法**逐行**走、引号态逐行重置 ⇒ 对前两种**完备**，对后两种**不认**。
    /// ⇒ 一看见多行定界符、或看见**行尾引号还没闭合**，就把理由回报出去，
    /// 由调用方**判红**。**没有「它猜错了还接着扫」的第三条路。**
    ///
    /// 这就是 `D2` 逮到的那个洞（本条第三次净变宽）：
    /// `target = { x = { ar = """` 换行 `# """, runner = "…" } }` ——
    /// 续行第一个字符是 `#`，逐行重置的引号态把**整行**当注释切掉，
    /// 真键在被扫之前就没了。而那个键 `od -c` 逐字是 `r u n n e r`：
    /// **明文、零反斜杠** ⇒ [`decode_toml_escapes`] 与 [`backslash_in_key_position`]
    /// **两层从头到尾没被触发**。⇒ 绕过刀要按**维**打（读文件 → 预处理 → 匹配 → 判定），
    /// 不是按例打：前两次补的都是「键怎么拼」和「文件怎么来」，这一刀打在**预处理**这一维上。
    ///
    /// ⚠ 刻意**只剥注释、不剥字符串内容**：TOML 的键可以带引号（`"runner" = "sh"`），
    /// 内联表的值里也住着键（`target = { x = { runner = "sh" } }`）——
    /// 把字符串一起剥掉就会在这两处**漏红**。代价：值里恰好出现那三个整词也会红。
    ///
    /// 返回 `(剥完的文本, 看不懂的理由)`；**理由非空 ⇒ 调用方必须判红**。
    fn strip_toml_comments(src: &str) -> (String, Vec<String>) {
        let mut out = String::with_capacity(src.len());
        let mut unmodeled: Vec<String> = Vec::new();
        for (idx, line) in src.lines().enumerate() {
            let no = idx + 1;
            if line.contains("\"\"\"") || line.contains("'''") {
                unmodeled.push(format!("第 {no} 行有多行字符串定界符（`\"\"\"` / `'''`）"));
            }
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
            if quote.is_some() {
                unmodeled.push(format!("第 {no} 行的引号到行尾还没闭合"));
            }
            out.push_str(&line[..cut]);
            out.push('\n');
        }
        (out, unmodeled)
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
    /// 不进本函数。三引号多行串**不能当键**（08-29 复打：`"""kg2r5probe"""` 放在键位
    /// ⇒ cargo 逐字 `could not load Cargo configuration`；同一把尺子的非空对照 ——
    /// 裸键 `kg2r5probe` 与基本串键 `"kg2r5probe"` 两种写法 alias 都真生效；
    /// 反向对照没有配置 ⇒ `no such command`）。
    ///
    /// 🔴 **⚠ 但「不能当键」推不出「不是一条路」 —— 这一处上一版就是这么写的，改掉了**
    /// 〔`D4` 08-29 逮到：`③b` 那一栏 08-28 已经逐字裁定过这句话「前后半句都真、
    /// 合起来是假的」，而**这一处漏订正了三版**（`cdb6095`/`c197917`/`88dad49` 各命中 1 处）〕。
    /// 它**是**一条路，只是**换了一维**：三引号一出现在配置里，走的是
    /// [`strip_toml_comments`] 那一维（把真键整行当注释切掉），`D2` 08-28 就是从这条路进来的。
    /// **两头我 08-29 各打了一刀**：
    /// ① cargo 这一头 —— `alias = { ar = """` 换行 `# """, kg2r5probe = "--version" }`
    /// ⇒ **alias 真生效**（报文从 `no such command` 变成
    /// `subcommand is required … alias.kg2r5probe`；反向对照回到 `no such command`）；
    /// ② 判据这一头 —— 同一形状（键换成 `runner`）放进真格位 `src-tauri/.cargo/config.toml`
    /// ⇒ **红，而报文逐字是「剥注释这一步**看不懂它**（第 1 行有多行字符串定界符…共 3 处）——
    /// 按 fail-closed 判红」，一个字都没提 `runner`** ⇒ 今天挡住它的是 fail-closed 那一维，
    /// **不是本函数**；非空对照（同一份文件，那个 `#` 换成两个空格）⇒ 报文里同时出现
    /// `设了执行面的键 ["runner"]` ⇒ 那个 `#` 真的把键吃掉了。
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

    /// 探针专用：剥注释，并**要求本剥法认得这个样本**（认不得就当场把样本报出来）。
    /// ⇒ 探针样本自己落进「看不懂」那一支时，不会静悄悄地按空文本判绿。
    fn stripped_ok(src: &str) -> String {
        let (text, unmodeled) = strip_toml_comments(src);
        assert!(
            unmodeled.is_empty(),
            "探针样本自己就让剥注释看不懂了，这条探针此刻无效：{unmodeled:?}"
        );
        text
    }

    /// **键位置**出现反斜杠没有〔`K-G2` `D1` 回修的兜底，08-28〕。
    ///
    /// ★ 这一条是为了让本条的结论**不靠「我把转义种类数全了」**。论证：
    /// TOML 的键只有三种写法 —— 裸键 · 字面串键（`'…'`，**不认转义**）· 基本串键（`"…"`）。
    /// 前两种里那几个字母只能是**明文**，原文那一遍就逮得到；第三种要把字母藏起来
    /// **必须用反斜杠**。⇒ 键位置只要出现反斜杠就红，**不管那是今天认识的
    /// `\u`/`\U`/`\x` 还是明天新加的哪一种**。
    ///
    /// **「键位置」的近似**：每个 `=` 往前数到上一个 `{` / `,` / 行首的那一段。
    /// 没有 `=` 的行（表头 `[…]`、多行值的续行）整行算。
    /// TOML 要求键与它的 `=` 同行，所以这个近似对**顶层键 · 点分键 · 表头 ·
    /// 内联表里的嵌套键**都成立 —— ⚠ 内联表那一格是量出来要的：08-28 实测
    /// `alias = { "kg2probe" = "--version" }` **cargo 认**（内联表 + 转义键都认），
    /// 只看「第一个 `=` 之前」会漏掉它。
    ///
    /// ⚠ **代价（说准，08-28 `D2` 打回时订正过一次 —— 原话把它说窄了）**：
    /// **多行值的续行**里有反斜杠、且那一行没有 `=`，会误红。
    /// 原话只写「多行**字符串**值」，而实际最常见的是**多行数组** ——
    /// `rustflags = [` 之后那几行正是它，那是 `rustflags` **最教科书的写法**。
    /// 现打三条（`§6` 变异台 F1/F2/F3）：
    /// `rustflags = [` 多行数组 ⇒ **红，点名 `rustflags`** —— 那不是误红，它真的设了；
    /// `x = [` 多行数组、续行里有反斜杠、**不含**那三个键 ⇒ **红，理由是「键位置有反斜杠」**
    /// —— **这一条是真误红**；同样的数组、续行里没有反斜杠 ⇒ **绿**。
    /// ⇒ **判它可接受**：方向是 fail-closed，报文点名了是哪一条规则、哪一格，人一眼能消；
    /// 而把它修窄（例如跟踪方括号深度、把数组续行排除）是**放宽**一条安全判据 ——
    /// 本件已经因为「没有维度级论证就放宽」净变宽三次，这一刀不在本轮自批的范围里。
    fn backslash_in_key_position(text: &str) -> bool {
        text.lines().any(|line| {
            let mut start = 0usize;
            for (i, b) in line.bytes().enumerate() {
                match b {
                    b'{' | b',' => start = i + 1,
                    b'=' => {
                        if line[start..i].contains('\\') {
                            return true;
                        }
                        start = i + 1;
                    }
                    _ => {}
                }
            }
            !line.contains('=') && line.contains('\\')
        })
    }

    /// `keys` 里哪几个在这份配置文本里出现过。
    ///
    /// ⚠ **扫两遍取并集**：原文一遍、[`decode_toml_escapes`] 解码后再一遍。**两遍都要。**
    ///
    /// 🔴 **上一版这里给的理由是反的，例子也退化了**〔`D4` `记-3` 逮到、`D5` 复核、08-29 改〕。
    /// 上一版逐字：「解码只会**新增**字符…（`runnerA` 解完成了 `runnerA`）」——
    /// ① **方向反了**：[`decode_toml_escapes`] 认的三种写法**全是减** ——
    ///    `\xXX` 4→1、`\uXXXX` 6→1、`\UXXXXXXXX` 10→1。
    ///    ⚠ 这一格是**论证不是读数**，读的是它自己那段实现：每个臂只 `out.push` **一个**字符，
    ///    而 `i` 前进 `2 + width`（`width` ∈ {2,4,8}）⇒ 输出长度**恒 ≤ 输入长度**，
    ///    解开一处就是严格小于；**没有任何一条路会让它变长。**
    /// ② **例子退化**：那两段反引号里**逐字节相同**（08-29 现打十六进制，两边都是
    ///    `72 75 6e 6e 65 72 41` = `runnerA`）⇒「`X` 解完成了 `X`」，证不了任何事。
    ///
    /// **真正的理由**（⚠ 结论没变，两遍确实都要）：解码把 `\` 这个**非词字符**换成**词字符**，
    /// 于是它**两个方向都会动整词边界** —— 既能吃掉原文里成立的那一条，也能拼出原文里没有的整词。
    /// ⇒ 两边各有一形，08-29 各现打了一次（放进真格位 `src-tauri/.cargo/config.toml`、只跑本条）：
    /// · **原文那一遍不能省**：`"runner\x41" = "x"` —— 原文里 `runner` 后面是 `\`（非词）⇒ 整词成立；
    ///   解完是 `runnerA`，后面是 `A`（词）⇒ 整词不成立。
    /// · **解码那一遍不能省**：`"\x72unner" = "x"` —— 原文里根本没有 `runner`，解完才有。
    ///
    /// 🔴 **两形的正/反刀读数**（锚点 = 本函数那一行 `.filter(…)`，全仓命中 **1**）：
    /// 原样两形都报 `设了执行面的键 ["runner"]`；把 `word_in(text, key)` 那一半拆掉
    /// ⇒ `"runner\x41"` 那一形的 `设了执行面的键 ["runner"]` **当场消失**（只剩「键位置有反斜杠」
    /// 那条理由），而 `"\x72unner"` 那一形**照样点名** ⇒ 尺子不是恒红。
    /// 把 `word_in(&decoded, key)` 那一半拆掉 ⇒ **探针③ 当场开火**（`left: []`、`right: ["runner"]`）。
    ///
    /// ⚠⚠ **顺手量到的一格欠账，本轮没补，如实记在这里**：把 `word_in(text, key)` 那一半
    /// **整个拆掉**之后，**6 格全缺席时整道 cargo 门照绿**（08-29 现打：`rc=0` · 包数 **8** ·
    /// 合计 **1303** · 判定绿）⇒ **原文那一遍今天没有任何常驻探针盯着**
    /// （解码那一遍有探针③ 盯着 —— 拆掉它探针③ 当场开火）。这是「有一支信号没人盯」那一族，与 `S4` 同族。
    /// ⚠ **第七轮（08-29）按统一切法 `R7` 复打，这一格逐格相同**（`rc=0` · 8 · 1303），
    /// 而**这一族在同一片面上共有 17 支** —— 逐支住址见本条判据头注「两格分母 ①」那一段。
    fn keys_in(text: &str, keys: &[&'static str]) -> Vec<&'static str> {
        let decoded = decode_toml_escapes(text);
        keys.iter()
            .copied()
            .filter(|key| word_in(text, key) || word_in(&decoded, key))
            .collect()
    }

    /// **装配层**：把 `root` 下那几格 cargo 配置逐格读出来，每格给一条 offender 理由
    /// （这一格没问题就 `None`）。
    ///
    /// ★★ **它为什么是一个具名函数**〔`K-R5` 09-02〕。
    ///
    /// 这一段原本是主判据体里的一个**匿名 `filter_map` 闭包**。而生产那一遍
    /// **6 格全缺席** ⇒ 下面 `!p.is_file()` 那道守卫一律提前 `return None`，
    /// **闭包体后面整段根本不执行**；上面那些探针又**全都直接调那几个原语**
    /// （[`keys_in`] / [`backslash_in_key_position`] / [`strip_toml_comments`]），
    /// **没有一条走过这个闭包** ⇒ **「把原语串起来」这一层零常驻覆盖**。
    /// `K-G2` 第七轮按切法 `R7` 现打、`K-R5` 09-02 在今天的主干上逐支复打：
    /// 这一段 **7 支信号里 6 支拆掉，判据与整道 cargo 门都绿**。
    ///
    /// ⇒ 抽成具名函数**就是为了让探针能喂它一份夹具根**（生产那一遍喂的是真仓根）——
    /// 探针 ⑩–⑮ 逐支把那 6 支买下来。
    ///
    /// ⚠ **抽出来这一刀本身不改行为**：`root` 与 `slots` 就是原来那个闭包捕获的同两个值，
    /// 闭包体一个字节没动。
    ///
    /// ⚠ **它没买到的一格，如实记**：「主判据体真的调了本函数」这件事本身没有独立探针。
    /// 今天挡着它的是 `F1`（`!p.is_file()`）那一支的牙 —— 拆掉 `F1`，本函数会给
    /// **6 格各回一条**「存在但读不出文本」，主 `assert!` 当场红。⇒ 这条链路是通的，
    /// 但那是**顺带**，不是「只由这一支挡住」。⚠ 而 `F1` 的牙**有前提**：它靠「那 6 格今天
    /// 一份都不存在」。真有人往那 6 格里放一份**读得出**的配置，`F1` 那一格的机制就变了。
    fn offenders_under(root: &std::path::Path, slots: &[String]) -> Vec<String> {
        slots
            .iter()
            .filter_map(|rel| {
                let p = root.join(rel);
                if !p.is_file() {
                    return None;
                }
                let Ok(raw) = std::fs::read_to_string(&p) else {
                    return Some(format!("{rel}（存在但读不出文本，本条看不了它的内容）"));
                };
                let (text, unmodeled) = strip_toml_comments(&raw);
                let mut why: Vec<String> = Vec::new();
                // ★ 预处理这一维的 fail-closed：剥注释看不懂它 ⇒ 当场红，不往下扫。
                //   `D2` 逮到的那个洞就在这里：看不懂却接着扫 = 拿一份被切过的文本当真相。
                if !unmodeled.is_empty() {
                    why.push(format!(
                        "剥注释这一步**看不懂它**（{}，共 {} 处）—— 按 fail-closed 判红",
                        unmodeled[0],
                        unmodeled.len()
                    ));
                }
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
            .collect()
    }

    /// 探针专用夹具：在临时目录里摆**一格**真配置文件，整段喂给 [`offenders_under`]，
    /// 返回那一格的判词（`None` = 这一格没被判成 offender）。
    ///
    /// ⚠ **绝不碰仓里被守的那 6 格** —— 往那 6 格里写东西 = 把判据自己弄红。
    /// 夹具落 `std::env::temp_dir()`（本仓测试的既有写法，21 个文件在用），跑完就删。
    /// ⚠ 目录名与那一格的相对路径都取**中性名**（`brief` `6g`：夹具的名字不许成为断言的
    /// 承重词）——下面每一条断言认的都是**判据自己报文里的词**，不是这里的路径。
    fn probe_slot_verdict(tag: &str, body: &[u8]) -> Option<String> {
        let dir =
            std::env::temp_dir().join(format!("ccm-capreg-slot-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let rel = "d/.cargo/config.toml".to_string();
        let p = dir.join(&rel);
        std::fs::create_dir_all(p.parent().expect("夹具的父目录"))
            .expect("建不出夹具目录 —— 这条探针此刻无效");
        std::fs::write(&p, body).expect("写不出夹具 —— 这条探针此刻无效");
        let out = offenders_under(&dir, std::slice::from_ref(&rel));
        let _ = std::fs::remove_dir_all(&dir);
        // 夹具自检：只摆了一格，判出两条以上说明喂错了东西，下面那条断言就不算数。
        assert!(
            out.len() <= 1,
            "夹具只摆了一格，却判出 {} 条：{out:?} —— 这条探针此刻无效",
            out.len()
        );
        out.into_iter().next()
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
    /// 🔴 **这张表一律用「内容锚点」指路，不写行号**〔`K-R5` 09-02 订正，全表 8 处〕。
    /// **行号是快照** —— `brief` 12 逐字：「带具体读数的与描述盘上现状的话都是那一刻的快照，
    /// 引用前重打，别当常量」；`K-R9` `§3` 记的是同一个病：「**谁再在上方加注释就会把它推馊**」。
    /// **它真的馊过一次**：`K-G3` 09-01 往 `scripts/gate.sh` 加了门六、往
    /// `e2e/local-backend-supervise.sh` 加了头注，行号整体往下推
    /// ⇒ 本表与下面 `§12.2` 那几处引它们的行号**当天全部指到注释行**。
    /// **09-02 现打的读数**（分母 = 本文件当时全部 23 条 `文件:行号` 引用）：**8 条馊了**，
    /// 分处 5 个地方 —— 引 `scripts/gate.sh` 的 6 条（本表 2 条、下面 `§12.2` 4 条：那条 cargo 命令 ·
    /// `run_gate_sum()` 的行段 · `set -uo pipefail` 两处）与引 `e2e/local-backend-supervise.sh`
    /// 的 2 条。**它们今天全部指到注释行**（其中一条指到一个光秃秃的 `#`）。
    /// 🔴 **本轮一处都不再写行号** —— 连「今天它搬到第几行」都不写：那个数下一次加注释又会假。
    /// ⚠ **量具**（可重跑）：`evidence/K-R5-C-line-refs.py` —— 把本文件里每一条 `文件:行号`
    /// 引用拿到盘上现打一次，印出那一行今天长什么样。它**判不了「引用的意图对不对」**，只印原文；
    /// 它也**不是判据**（没有机器口径的对错），是一把尺子。跑一次就知道这一栏有没有回潮。
    ///
    /// | 发起面 | 谁从这里发起 cargo（认**这条命令**，不认行号） |
    /// |---|---|
    /// | 仓根 `.` | `ci.yml` 的 `e2e-tmux-rust` job（**没有** `working-directory` ⇒ cwd = 仓根；那一条是 `cargo build --manifest-path remote-daemon-proto/Cargo.toml`）· `scripts/run.ps1` 的 `"check"` 分支（`cargo check --manifest-path src-tauri\Cargo.toml`）。它同时是下面两个的**祖先** —— 往上找一定路过 |
    /// | `src-tauri/` | `scripts/gate.sh` 的 `run_gate_sum cargo 8 …`（`cd src-tauri && cargo test --workspace --exclude code-picture-core --lib`）· `package.json` 的 `gen:types` · `ci.yml` 里**两处** `working-directory: src-tauri`（`rust` job 与 `linux-app-build` job）· `e2e/` 四个脚本共 **6 处**（`tmux-guarded-acceptance.sh` 的 `emit_guarded_commands_for_e2e` · `usage-probe-acceptance.sh` 的 `emit_usage_probe_cmd_for_e2e` 与 `e2e_the_local_execution_surface` · `local-backend-supervise.sh` 的 `local_backend` 与 `local_daemon` 两条 `cargo test --lib -- --ignored` · `p3t-local-tmux.sh` 的 `P3T_E2E_SID=…` 那一条） |
    /// | `remote-daemon-proto/` | `scripts/gate.sh` 的 `run_gate daemon …`（`cd remote-daemon-proto && cargo test`）· `ci.yml` 的 `daemon` job（`working-directory: remote-daemon-proto`）· `release.yml` 里**三处** `working-directory: remote-daemon-proto`（`build-daemons` · `build-windows` · `build-linux` 三个 job）· `e2e/daemon-fork-session.sh` 的 `cd "$ROOT/remote-daemon-proto" && cargo build` |
    ///
    /// ⚠ **为什么这里只能用锚点、不能用「函数名 + 行号」两样都给**〔`K-R5` `§4` 那一问的答〕：
    /// `ci.yml` / `release.yml` 那几处逐字都是同一句 `working-directory: <目录>`，**行号是它们
    /// 今天唯一的区分**；所以这里改成**按 job 名**指路（job 名是 yml 自己的标识符，改名会连带改
    /// 那一段的语义，不会被「上面加两行注释」推馊）。`e2e/` 那几处同理，用**测试名 / 变量名**。
    /// ⇒ **一处都不靠行号**；代价是「共几处」这个数要人回来数，那个数本身也写在表里了。
    ///
    /// ⚠ **尺子没覆盖到的**（写下来免得把它读成穷举）：① `cargo tauri build` 那种**由工具
    /// 再去起 cargo** 的，cwd 由 tauri CLI 定，本条没现打；② 人手临时 `cd` 到任意目录敲的
    /// cargo —— 那个分母没人数得出，也不是一条判据守得住的。
    /// ③ `scripts/verify-committed-state.sh` 的 `run monitor-lib` / `run daemon` / `run daemon-win`
    /// 三条在**另开的临时工作树**里跑，相对目录仍是这两个，不新增发起面。
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
    /// ⚠ 加一个发起面（新 crate、某个 job 换 cwd）就回这张表补一格。
    /// **⚠ 把「叫你回来的是哪一条」说准**〔`D4` 08-29 逮到上一版这句话说宽了：
    /// 它把功劳记在 `slots.len() == 6` 上，而本文件下面的代码注释自己就称它「表面量」〕：
    /// - **加/删**一格 ⇒ `slots.len() == 6` 那条自检开火。08-29 现打：`CARGO_CFG_NAMES`
    ///   去掉一个文件名 ⇒ **红**，报文逐字「本条的人群应当是 3 个发起面 × cargo 认的
    ///   2 种文件名 = 6 格，实得 3」。
    /// - **换**一格 ⇒ `len == 6` **一声不吭**（格数仍是 6），开火的是**探针⑨**。08-29 现打：
    ///   `CARGO_CFG_DIRS` 的 `src-tauri` 换成 `e2e` ⇒ **红**，报文逐字「本条的人群**逐格**变了。」
    ///   ⚠ 这里刻意**不写行号** —— 行号是本文件自己每加一行注释就变的量（`brief` 12），
    ///   写进头注下一轮自动变假；认那条 assert 认**报文**。
    ///
    /// ⇒ `len == 6` 只买「**个数**」那一半（`testing.md` 硬规则 6 说的表面量正是它），
    /// 买「**身份**」那一半的是探针⑨。两条都要，**别把后者的功劳记到前者头上**。
    ///
    /// ## ③ 这一格有**四维**，改它之前先看这张表〔`K21` 收紧后的第一条，`D2` 08-28〕
    ///
    /// 本条从输入到判定是四步，**每一步都是一个可以被单独绕过的维**。
    /// 08-28 连着三次净变宽，洞分别落在**三个不同的维**上 —— 而每一次我补的都是「那个例」，
    /// 下一刀就从没补过的那一维进来：
    ///
    /// | 维 | 这一步做什么 | 被绕过过没有 | 今天靠什么守 |
    /// |---|---|---|---|
    /// | ① **读文件** | 6 格 = 3 发起面 × 2 文件名 | ✅ `include` 把 6 格之外的文件拉进来 | 人群由数组算出（`slots.len() == 6` 自检）+ `CARGO_BLINDING_KEYS` 判红 |
    /// | ② **预处理** | 剥注释 → 解转义 | ✅ 多行串让剥注释把真键**整行切掉**（`D2`） | [`strip_toml_comments`] **fail-closed**：看不懂就判红 |
    /// | ③ **匹配** | 整词扫那三个键 | ✅ `"runner"` 转义拼键（`D1`） | [`decode_toml_escapes`] 解码 + [`backslash_in_key_position`] 兜底 |
    /// | ④ **判定** | 收集 offender → `assert!` | ✅ 被绕过**两次**；`K-R5` 09-02 之前**装配这一层零常驻覆盖** —— 见紧接着这张表的那一段 | **二十一组**常驻探针 ①–㉑（⑩–㉑ 是 `K-R5` 09-02 补的；**加/删探针就回来改这个数**） |
    ///
    /// 🔴 **维 ④ 那一格：别再写「还没被绕过」**〔`D4` 08-29 逮到，而写下那句话的
    /// 那个 commit 自己的账里就记着一次；**当时那个「九组」**（今天是二十一组，见上表）
    /// 也是 `D4` 逮的 —— `c197917`
    /// 把探针从 ①–⑦ 加到 ①–⑨，而这一格的数没跟着改，盘上一度同时躺着「探针⑨」和「八组」〕。
    /// 三条逐字，全是 08-29 我自己复打的：
    /// ① **`D2` 轮被绕过一次** —— 拆掉「行尾引号未闭合」那一支，**判据照绿**
    ///    （当时三个探针样本都含三引号定界符，被**另一支**信号挡着 ⇒ 那一支没有只由它挡住的探针）。
    ///    补了探针⑦ 之后**今天再拆当场红**：现打 `FAILED`，报文逐字
    ///    「探针⑦（行尾引号未闭合（**只**由这一支信号挡住））：剥注释**必须回报看不懂**。」
    /// ② **`D3` 轮被绕过一次** —— `M5` 把人群换一格（格数仍 6），**判据照绿**。
    ///    补了探针⑨ 之后**今天再换当场红**：现打**红**，报文逐字「本条的人群**逐格**变了。」
    /// ③ 🔴 **08-29 那一批活口，`K-R5` 09-02 关掉了 15 支，还剩 2 支** —— 逐支见下面
    ///    「两格分母」那一段。最老的那个（「存在但读不出文本」那一支 fail-closed 没有
    ///    「只由这一支挡住」的探针）**今天关掉了**：现打，把那一行 `return Some(…)` 改成
    ///    `return None` ⇒ **红**，报文逐字「探针⑩（`let Ok(raw) … else` 那道守卫；
    ///    **装配层这 6 支里只由它挡住**）」。它曾经是 `K22` 的欠账，`D3`/`D4` 都登记过。
    ///    〔🔴 `D6` 那一版逐字写「今天仍有**一个**活口」——**说窄了**，而且是同一句里的分母病：
    ///     按 `D3` 那条切法当时已经是 **2** 个，按统一切法 `R7` 是 **17** 个。
    ///     ⚠ 这一句原本长在「加了探针而这一格的数没跟着改」那段病史的 8 行之内。〕
    ///
    /// ⚠ **两格分母**（免得把上面读成穷举）：
    /// ① 上面这三条出自**逐支复打**，而「几支」这个数**随切法变 —— 不写切法就没有这个数**：
    ///    · `D3` 按「一条 `if` / 一个 `filter` 条件」的**语义**切法数出 **11 支**（08-29 一轮 10 红 1 绿），
    ///      而那张表的粒度从「一整个函数」（`S8` = [`decode_toml_escapes`]）一路到「一个 `if`」（`S5`–`S7`）；
    ///    · 第七轮定了一条**统一**的切法 `R7`（**一支 = 一个「能用一刀改成恒定答案、而语法与类型契约
    ///      都不变」的最小语法位置**；`||`/`&&` 的每个操作数各算一支；**排除**循环边界 · 通配臂 ·
    ///      纯计算 · 判据自己的 `assert!` 与那几组常驻探针 · `BEFORE`/`LIFECYCLE` 那两块人群），
    ///      **在同一片面上数出 41 支，08-29 逐支现打 24 红 17 绿**（17 绿逐支都在**整道 cargo 门**
    ///      这个宽分母上复打：`rc=0` · 包数 **8** · 合计 **1303**）。
    ///    · 🔴 **`K-R5` 09-02 把这 41 支在今天的主干上从头数了一遍、逐支重打**（**一个旧数都没沿用**）：
    ///      分母仍是 **41**（量具 `evidence/K-R5-C-r7-census.py --census`，逐支锚点现打命中 1 次，
    ///      41/41 命中），逐支读数仍是 **24 红 17 绿**，**而 17 绿那一组的成员逐支相同**。
    ///      ⚠ **门那四个数变了**：08-29 是 `rc=0` · 包数 **8** · 合计 **1303**，
    ///      09-02 是 `rc=0` · 包数 **8** · 合计 **1313** —— 主干这几天动过，
    ///      **不是同一个读数，只是同一个判定**。⚠ 那个合计还带第二维：
    ///      `src-tauri/embedded-daemons/` **铺没铺**（09-02 这棵树**没铺**，现打 `ls` 不存在）。
    ///    🔴 **所以这个读数答的不是「哪几支没人盯着」，是「在某一条切法下哪几支没人盯着」** ——
    ///    `D3` 的切法下 1 支，`R7` 的切法下 17 支，**而多出来的不是新洞，是同一片面被切得更细**。
    ///    〔🔴 上一版逐字写「**它答得了「哪几支没人盯着」**，答不了『支数对不对』」——**说宽了**：
    ///     `D6` 08-29 现打，本轮从 [`keys_in`] 那个 `filter` 切出来的一支**恰好落在这一句自己写的
    ///     切法规则里** ⇒ 切法依赖同样吃到那一问。⚠ 而这句话就长在上面那段病史之内 ——
    ///     那段逐字讲的正是「`c197917` 把探针从 ①–⑦ 加到 ①–⑨，**而这一格的数没跟着改**」。
    ///     **同一段里，上一句在讲一个病，下一句就犯了它。**〕
    ///    🔴 **那 17 支后来怎么了：`K-R5` 09-02 买了 15 支，明说不买 2 支。**
    ///
    ///    **买的判准**（逐支过一遍铁律 18「宁可宽松让模型自己判断，也不要用严格的错误引入噪声」）：
    ///    这一支拆掉会产生**已量到的实害形状**（**漏红**：真 `runner` 不再被点名；
    ///    **误红**：一份纯瘦身配置被点名），**且**它守的是这几个函数 docstring **自己声明的契约**，
    ///    而不是 fail-closed 那个**近似的具体形状**。⇒ 换一种剥法只要仍满足契约，那几条探针照样绿。
    ///
    ///    | 支（`R7`） | 买了没有 | 谁挡住它 · 或者为什么不买 |
    ///    |---|---|---|
    ///    | [`strip_toml_comments`] 的 `if escaped` 与 `c == '\\'` | ✅ | 探针⑯（基本串里的 `\"` 不结束该串 ⇒ 后面那个 `#` 不是注释）。⚠ **这两支在函数外面行为上分不开**，实测任拆一支读数逐格相同 ⇒ **一条探针挡两支**，如实记 |
    ///    | [`strip_toml_comments`] 的 `quote == Some('"')` | ✅ | 探针⑰（字面串里没有转义 —— `'C:\tools\'` 那种 Windows 路径不许被读成「引号没闭合」）。方向是**误红** |
    ///    | [`strip_toml_comments`] 的 `c == '\''` | ✅ | 探针⑱（字面串里的 `#` 不是注释）。方向是**漏红**，与 `D2` 那个洞同一维 |
    ///    | [`strip_toml_comments`] 的 `line.contains("'''")` | ❌ **不买** | 它守的是 **fail-closed 那个近似本身**（「看见 `'''` 就判红」），不是契约 ⇒ 买了就把「换一种剥法（真去认多行串）」变成一次判据大修。而**实害形状已被另一支挡住**：现打，一个 `'''` 开头的续行样本会让行尾引号态不闭合 ⇒ 「行尾引号未闭合」那一支（有牙，探针⑦）先开火；这一支单独失守时**只剩「同一行内定界符成对」那一形，而那一形什么都没藏住** |
    ///    | [`decode_toml_escapes`] 的 `\U` 臂 | ✅ | 探针⑲。三种转义 cargo 都认，而在此之前只有 `\u`（③）与 `\x`（④）有探针 |
    ///    | [`word_in`] 的 `c.is_alphanumeric()` · `c == '_'` · 整词边界的**前**半 | ✅ | 探针⑳（两个样本：`myrunner` · `my_runner`）。⚠ **「前半」那一支分不开**：它是「前面那一侧」的总闸，任何前半样本都同时挡住它与所用的那一类词字符；两个样本各自把「字母数字」与「下划线」**单独**挡住 |
    ///    | [`backslash_in_key_position`] 的 `b'{'` 候选 | ❌ **不买** | 🔴 **我构造不出实害输入**：那一支的作用是「在 `{` 处重置键位段」，而它与前一个重置点（`=` / `,`）之间**在 TOML 里只可能隔着空白**（`{` 只出现在 `=` 之后）⇒ 拆掉它，键位段最多多含几个空格，答案不变。⚠ **分母 = 我试过的那几形**（顶层键 · 点分键 · 表头 · 内联表 · 嵌套内联表 · 多行数组续行），**不是穷举**；要买它只能拿一个不合法的 TOML 当样本，那就把实现细节钉死了 |
    ///    | 主判据体那个 `filter_map` 闭包的 6 支 | ✅ | 探针⑩–⑮，**一支一条**（`K22`）。见 [`offenders_under`] 头注 |
    ///    | [`keys_in`] 的 `word_in(text, key)`（原文那一遍） | ✅ | 探针㉑。本文件 [`keys_in`] 头注 08-29 就把它记成欠账了 |
    ///
    ///    🔴 **补完之后还剩几支没牙：2 支** —— 上表那两行 ❌（`'''` 那个析取项 · `b'{'` 那个候选）。
    ///    **09-02 现打确认它们今天仍然无牙**：逐支拆掉 ⇒ `rc=0` · 包数 **8** · 合计 **1313** · 判定绿。
    ///    ⚠ **这两支是诚实边界，不是「下一轮该补」** —— 上表逐支写了为什么判它不值。
    ///    ⚠ **一格对称性如实记**：不买 `'''` 的那条理由，对**已经买了的** `"""`（探针⑦e）**同样成立**。
    ///    我没有去动⑦e —— 删一条已经买下的探针不在本件射程里，也不是实现方能自批的事。
    /// ② 「二十一组」是**组数**，不是「二十一组都验过牙」：`K-R5` 09-02 这一轮真被打红过的是
    ///    **⑩–㉑ 那 12 组全部**（逐组现打，每组由它自己那一支挡住）+ 旧的 ①③⑤⑥⑦⑧⑨；
    ///    **旧的 ② 与 ④ 这两组我这一轮没打**（08-29 那一轮也没打）。
    ///
    /// 🔴 **改本条时的纪律**：绕过刀**按维打，不是按例打**。
    /// 上一轮打了九刀**全在维 ③**（`\u` · `\U` · `\x` · 表头 · 内联表 …）——
    /// **在计数上像很彻底，在覆盖上是一个点**，`D2` 那一刀从维 ② 进来，九刀一刀都没碰到。
    ///
    /// ## ③b **不守什么**（`D1`/`D2` 回修时逐条量出来的；铁律 14：诚实边界落进被守对象）
    ///
    /// ⚠ 下面是**我量过的那几条**，不是「所有绕法」的穷举 —— 那个分母没人给得出。
    /// 量法一律是：`scratchpad` 里一次性 crate + `[alias]` 探针，带非空对照与反向对照，
    /// cargo **1.96.1 stable**（换版本要重量）。
    ///
    /// 🔴 **先把「够不着」的理由写对**〔`K-G2` `D3` 打回，08-28 裁定；08-29 落盘〕：
    /// 这一栏上一版把「够不着」归到「**它不是仓里的文件**」上。**那句话是假的** ——
    /// 本条的人群**不是「文件」，是上面那 6 个特定路径**。一份**就在仓里、就在仓根、
    /// 一次 commit 就落地**的文件照样够不着本条，只要它**不落在那 6 条路径**之一上
    /// （下面 `rust-toolchain.toml` 那一行就是活体，而上一版那句理由**恰恰把它遮住了**）。
    /// ⚠ **08-29 订正一个词**〔`D4` `记-1` 逮到、`D5` 复列为存量〕：上一版这里写「不叫那 6 个
    /// **名字**之一」，而同一段**前一句**刚把这个词说准成「6 个特定**路径**」——
    /// **同一段里两句话用了两个词**，本轮改齐。现打：那 6 条路径是
    /// `CARGO_CFG_DIRS`（**3** 个目录）× `CARGO_CFG_NAMES`（**2** 个文件名）算出来的
    /// ⇒ **名字只有 2 个，路径才是 6 条**；按「名字」读会把「仓根的 `config.toml`」
    /// 和「`src-tauri/` 的 `config.toml`」读成同一格。
    /// ⚠ **还有一个例外要说清，别把这句读成「格外的文件本条一概不管」**：
    /// 那 6 格里的 `include` **能把格外的文件拉进来**，而那一形本条逮的是
    /// **那句 `include` 本身**，**不是**去读被拉进来的那份文件（所以它的理由是「看不见」）。
    /// 08-29 现打：`src-tauri/.cargo/config.toml` 写 `include = ["elsewhere.toml"]`、
    /// 被拉进来的那份里写 `[build]` + `rustflags = []`
    /// ⇒ 报文逐字 `["src-tauri/.cargo/config.toml：设了 [\"include\"] —— 本条看不进它拉进来的文件"]`，
    /// **offenders 那一格里一个字都没提 `rustflags`**；非空对照把同一段 `rustflags`
    /// **直接**写进那一格 ⇒ 报文逐字 `设了执行面的键 ["rustflags"]` ⇒ 尺子活着。
    /// ★ **定性，写给下一个改这一栏的人**：「不守什么」这一栏里写一条**会骗人的理由，
    /// 比不写还坏** —— 不写只是留白，写错会让下一个人**不再去想这一格**。
    ///
    /// | 不守的东西 | 现打的读数 |
    /// |---|---|
    /// | 同族的执行面键：`rustc-wrapper` · `rustdocflags` · `build.rustc` | 射程逐字只有那三个，这一族归跟进件 `己1-f36` |
    /// | 🔴 **`rust-toolchain.toml` 的 `[toolchain] path`** —— **仓内文件**，仓根一份就够 | 见下面那一段（08-29 现打，`K-G2` `D3` 逮到）。归跟进件（`己1-f47`），**本件不扩射程** |
    /// | **环境变量那条配置源**：`CARGO_TARGET_<TRIPLE>_RUNNER` · `RUSTFLAGS` · `CARGO_BUILD_RUSTFLAGS` | cargo 认它们，但**它们不在上面那 6 个路径里** ⇒ 够不着（⚠ 理由是「不在那 6 格」，**不是**「不是文件」）。⚠ CI 的 yml 能设环境变量，那是另一个面，本条不声称守它 |
    /// | `cargo --config <k>=<v>` 命令行 · 仓外的 `$CARGO_HOME/config.toml` | 一次 commit 改不到 ⇒ 不在人群里 |
    /// | **值**里的写法花样 | 本条对**值**只是整词顺带命中（`[alias]` 里塞 `--config …rustflags=…` 会被逮到）。那是副产品，不是判据 |
    /// | **`[env]` 那一族** | cargo 的 `[env]` 能给构建期的进程设环境变量（`RUSTFLAGS` 那一类正好是环境变量读的） —— 射程外，归 `己1-f36`。⚠ 08-28 之前这一栏**整个漏了它**，而 `己1-f36` 把它记成那一族里最狠的一条 |
    /// | 多行字符串 `"""` / `'''` | ⚠ **08-28 `D2` 订正**：原话写「三引号串不能当键 ⇒ 键那一侧不是路」，**前后半句都真、合起来是假的** —— 它确实不能当键，但它**是**一条路，走的是**剥注释**那一维（`D2` 的洞）。今天由 [`strip_toml_comments`] 的 fail-closed 挡着：看见定界符或行尾引号未闭合 ⇒ 判红。**不是「不守」，是「守法换了一维」** |
    ///
    /// ### 🔴 `rust-toolchain.toml` —— 一个仓内文件，换掉 `cargo` 和 `rustc` **本身**
    ///
    /// 〔`K-G2` `D3` 08-28 逮到；下面的读数是 08-29 回修那一拍**我自己复打的**，
    /// 量具：一次性 crate `/home/zbl/.cache/kg2r4-probe-tc` + 透明代理假工具链
    /// `/home/zbl/.cache/kg2r4-fake-tc`（`bin/{cargo,rustc,rustdoc}` 各是「记一笔日志 →
    /// `exec` 真的那一个」），日志 `/home/zbl/.cache/kg2r4-tc.log`；`rustup 1.29.0`，
    /// 本机 `cargo` 是 rustup 的 shim（`ls -l $(command -v cargo)` ⇒ `…/.cargo/bin/cargo -> rustup`）〕
    ///
    /// | 刀 | 摆法 | 读数 |
    /// |---|---|---|
    /// | 反向对照 | 没有 `rust-toolchain.toml` | `test result: ok. 1 passed` · 假工具链被调用 **0 次** |
    /// | 正 | 仓里放一份：`[toolchain]` / `path = "<假工具链>"` | `test result: ok. 1 passed` · 假工具链被调用 **7 次**（`CARGO test` ×1 · `RUSTC` ×5 · `RUSTDOC` ×1）⇒ **完全透明** |
    ///
    /// **为什么它比 `runner` 更狠**：`runner` 只劫持**测试二进制的执行**，而且它就住在
    /// 本条**看得见**的那一格里；`path` 工具链换掉的是 **`cargo` 与 `rustc` 本身**
    /// （连编译都过它的手），而且**不在本条人群的任何一格**。它一次 commit 就落地，
    /// 不需要环境变量、不需要 `$CARGO_HOME`、不需要 `include`。
    ///
    /// **仓里今天有没有人守它：0 条判据。** 分母现打（量于 `track/k-g2` @ `c197917`，
    /// `git ls-files` **703** 个跟踪文件，去掉 `src-tauri/vendor/` 的 **33** 个 ⇒ **670**）：
    /// 搜 `rust-toolchain` 命中 **2 个文件 · 7 处**，逐字全是 `.github/workflows/{ci,release}.yml`
    /// 里的 `uses: dtolnay/rust-toolchain@stable`（**装工具链的 action，不是判据**）；
    /// 同一把尺子的非空对照搜 `capability` ⇒ 命中 **23** 个文件 ⇒ 尺子活着，零命中是真零命中。
    ///
    /// ⚠ **别把它和上面那张「四维」表混起来**：那张表切的是**本条内部的数据流**
    /// （读文件 → 预处理 → 匹配 → 判定），三次净变宽全落在那四步里，
    /// 而且全在**同一个人群内部**（「判据看不见那个 `runner`」）。
    /// `rust-toolchain.toml` **换掉的是配置源本身** —— 它根本不经过那 6 个格位，
    /// 连第 ① 步都进不去。⇒ 它不是「再补一个例」，是**另一条人群**；
    /// 而本件**不扩射程**的理由也正是这个（`K-G2` 的射程逐字只有那三个 cargo 键，
    /// 与 `rustc-wrapper` 那次同一先例：说得对，但整条人群该由跟进件立）。
    ///
    /// ⚠ **门禁那 8 个包上我没打这一刀**（要让整个 workspace 换一条工具链重编）。
    /// 只能给**机制**：假工具链是 `exec` 的透明代理 ⇒ 进程映像被替换。
    /// **这是论证，不是读数 —— 别把它读成我打过了。**
    ///
    /// 🔴 **「那四个数必然不变」这句话我收回**〔`D5` 08-29 列为存量：上一版逐字写
    /// 「包装层对 stdout/stderr 与退出码一个字节都不贡献 ⇒ 下面表里那四个数**必然**不变」——
    /// **既没有前提也没有分母的全称**〕。它至少要三条前提，**每条我都现打了一个
    /// 「这一条不成立 ⇒ 读数当场变」的实打反例**（08-29；分母 = 我打过的这 3 刀，
    /// ⚠ 不是「凡不成立必变」，只是「这一形不成立时它变了」）：
    /// ① **cargo 会去调的每一个工具都被包到**：同一份一次性 crate、同一条 `rust-toolchain.toml`
    ///    的 `[toolchain] path` —— 全套假工具链（`bin/{cargo,rustc,rustdoc}`）⇒ `rc=0`、被调 **7** 次；
    ///    **少包一个 `bin/rustdoc`** ⇒ `rc=101`、被调 **6** 次、`No such file or directory (os error 2)`。
    ///    ⚠ **这个 `101` 要带口径**（少了口径，照它复不出来）：它成立的条件是
    ///    **假 `cargo` 里显式 `export RUSTDOC=<该工具链的 rustdoc>`**，而该工具链没有 `bin/rustdoc`
    ///    ⇒ cargo 去 `exec` 一个**具体的、不存在的路径** ⇒ `could not execute process …(never executed)`
    ///    ／ `No such file or directory (os error 2)` ⇒ **`rc=101`**。
    ///    🔴 **同一格 `D5` 的网格报 `rc=1`，成因我 08-29 查了，一趟就够**（只切一个变量、两个变体）：
    ///    **不设 `RUSTDOC` 时 `rustdoc` 由 rustup 代理接管** ⇒ 它自己报
    ///    `error: 'rustdoc' is not installed for the custom toolchain '<路径>'` 并退 **1**。
    ///    两份量具除**那一行 `export RUSTDOC=`** 之外可执行内容逐字相同
    ///    （两份 `rustc` 包装 md5 同为 `70421eaf642a89b3c67e93997765a10d`；两份 `cargo` 去注释、
    ///    把自指目录名归一之后 `diff` 只剩那一行）⇒ **切它一次，读数在 101 与 1 之间来回。**
    ///    〔🔴 上一版逐字写「**两把假工具链不是同一份**（我这份的假 `cargo` 是 `exec` 真 cargo）」——
    ///     那两句都是真的，但**摆在「差在哪」那一格上是假举证**：`D5` 那份逐字也是 `exec` 真 cargo
    ///     ⇒ 我挑的属性正是两把**共有**的那一个。`D6` 08-29 逮到。
    ///     ★ **「我不解释」是对的；「放一个真事实在解释位」不是不解释，是解释错了。**〕
    ///    ⚠ **我没做到的一格，如实记**：`D6` 那张机制表还有一行「把 `~/.cargo/bin` 从 `PATH` 上摘掉
    ///    ⇒ `101`」—— **我复不出来**：现打 `rc=1`（与不摘的口径逐格相同、被调都是 **6** 次）。
    ///    因为 **rustup 代理会把 `/home/zbl/.cargo/bin` 重新塞回子进程 `PATH` 的第 1 位**
    ///    （现打：在假 `cargo` 里打印 `PATH` ⇒ `1:/home/zbl/.cargo/bin`）
    ///    ⇒ **在父进程上摘 `PATH` 这把尺子够不着那一维**，那一行我这一拍**判不了**。
    ///    ⚠ 承重的那一格不受影响：**`rc` 由 0 变非 0**，三把尺子（101 · 1 · 1）都成立。
    /// ② **包装层不改 argv**：下面形 2s 只多一个 `--skip`，合计就从 **1303** 变 **1302**。
    /// ③ **包装层自己不往 stdout 写**：一个完备、不改 argv、只多 `echo` 一行
    ///    `test result: ok. 99 passed` 的 `exec` 代理（经环境变量 runner 挂上、**6 格全缺席**
    ///    ⇒ 本条自己不会开火，读数干净）⇒ 包数 **8 → 16**、合计 **1303 → 2095**、判定**绿 → 红**。
    /// **三条同时成立的那一形我也量了一次**：纯透明代理（`exec "$@"`，同样经环境变量挂）
    /// ⇒ `rc=0` · 包数 **8** · 合计 **1303** · **绿** · marker **8**，与形 0 逐个相等。
    /// ⚠ **自查逮到我自己一处，改了**：这里第一版写的是「**这三条是必要条件**」——
    ///    那又是一句全称（「凡不满足它的包装层都会改数」），而它有**现打的**反例：
    ///    上面那条「全套假工具链」其实只包了 **3** 个二进制（`cargo`/`rustc`/`rustdoc`）——
    ///    真工具链里的 `rustfmt`/`clippy-driver`/`rust-gdb` 一个都没包，**而 `rc=0`、7 次调用**。
    ///    ⇒ **「有工具没被包到」并不必然改数**，改数的是「**cargo 这一趟真会去调**的那个没被包到」。
    ///    ⇒ 逐字改成：**每条我只打过一个反例**（这一形不成立时它变了），
    ///    **既不敢说「凡不成立必变」，更没证「三条凑齐就够」。**
    /// ⚠ **这四刀分处两层，别读成一层**：**② ③ 与正刀**打的是 **`runner` 那一层的包装**
    ///    （经环境变量 `CARGO_TARGET_<TRIPLE>_RUNNER` 挂），**而 ① 打的正是假工具链本身**
    ///    —— `rust-toolchain.toml` 的 `[toolchain] path`，08-29 现打的那一趟里
    ///    **一个 `runner` 都没有**（那个一次性 crate 没有 `.cargo/` 目录、环境里零个 `*_RUNNER` 变量）。
    ///    ⚠ **① 跑在一次性 crate 上，不是门禁那 8 个包上** —— 「让整个 workspace 换一条工具链重编」
    ///    这一刀我仍然没打 ⇒ **对门禁那 8 个包仍是论证 + 同形旁证。**
    ///    〔🔴 上一版逐字写「这四刀打的**都是** `runner` 那一层的包装，**不是**假工具链本身」——
    ///     **把自己的证据说弱了**（读它的人会以为工具链那一层一刀都没打过）。`D6` 08-29 逮到并复现了 ①。〕
    ///
    /// ⚠⚠ **一条本条自己盖不住的 —— 而「盖不住哪一形」必须说准**
    /// 〔🔴 **这一句连着两版都是假全称，两版方向还相反**：`D4` 08-29 打穿第一版
    /// 「`src-tauri/` 那一格上**真生效**的 `runner` 会让本条**自己不被执行**」（对形 2 假）；
    /// `D5` 08-29 用**同一把刀**打穿了**订正后**那一版「让本条自己不被执行的**只有**不透明的
    /// `runner`」（对形 2s 假）。★ **纪律：订正一句假全称时，换上去的那一句同样是全称 ——
    /// 同一拍要对新句子再打一次，别只打旧句子。**〕
    ///
    /// **所以这里不写全称。我量过的这 4 形里**（0/1/2/2s，见下表），
    /// **让本条「自己不被执行」的有两形，而它们分处「透明」的两侧**：
    /// · **形 1 · 不透明**（`/bin/echo`，根本不跑测试）—— 08-29 现打：包数 **0**、
    ///   一条 `test result: ok` 都产不出；
    /// · **形 2s · 透明**（`exec "$@" --skip <本条名>`，**真跑测试**）—— 08-29 现打：
    ///   `rc=0` · 包数 **8** · 合计 **1302** · 门禁**绿** · 代理 marker **8**，
    ///   而本条 `stays_registered` 在整份输出里**命中 0 次**（那个包 `1 filtered out`）。
    /// ⇒ 🔴 **「透明与否」不是判别的那一维** —— 形 2s 是它的单证。
    /// ⚠ 而且**「透明」这个词本身撑不住承重**：按上面那个定义（**不真跑测试**的叫不透明）
    /// 形 2s 是透明的；换一个定义（**原样透传 argv**）同一形就成了不透明的。
    /// ⇒ **承重词只能是「本条被没被执行」，不能是「透明不透明」。**
    /// ⚠ **分母 = 我量过的这 4 形 {0, 1, 2, 2s}，不是穷举** ——「哪一类 `runner` 会让本条不被执行」
    /// 的完整刻画**我给不出，也不写**。
    /// ⚠ **别把这个「4 形」与下面表下那句「4 个真 `runner` 形」当成同一个集合**：那一句是 **{1, 2, 2s, 3}**。
    ///
    /// 🔴 **形 2s 这一刀比形 3 更该记（是实害，不是措辞）**：它**只要落一份配置文件，
    /// 而那份文件就落在被守的 6 格里**。08-29 现打的单证：**同一份**
    /// `src-tauri/.cargo/config.toml`（`runner` 指向那个 skip-runner），
    /// 拿环境变量 `CARGO_TARGET_<TRIPLE>_RUNNER` 换一个透明代理**让判据跑起来**
    /// ⇒ 当场红，报文逐字 `["src-tauri/.cargo/config.toml：设了执行面的键 [\"runner\"]"]`。
    /// ⇒ **判据只要跑就逮得到它，而它让判据不跑。** 门禁那一侧也没兜住：
    /// `run_gate_sum` 只断言 `pkgs == 8` 与 `n != 0`，**对合计的具体值没有任何断言**
    /// ⇒ 1303 掉到 1302 一声不吭。
    ///
    /// ⚠⚠ **别把上面那几行读成「本条盖住了透明那一侧」**：形 2s 已经是反面的单证。
    /// 下面的**形 3**（透明代理 + 判据跑起来了却**看不见**那个 `runner`）是**另一条路**
    /// 通向同一个结果 —— ⚠ **两形机制不同**：形 3 是「**跑了但看不见**」，形 2s 是「**根本没跑**」；
    /// 共有的是那四条性质：**配置真生效 · 本条没红 · 门禁绿 · 两边都没兜住**。
    /// ⚠ **这也是一句要给分母的话**：分母是下面那张表的 **5 行 = 基线形 0 + 4 个真 `runner` 形（1/2/2s/3）**
    /// —— 形 1 门禁兜住、形 2 本条自己兜住、**形 2s 与形 3 两边都没兜住**。
    /// 🔴 ⚠ **别把这里的「4 个」和上面那句「我量过的这 4 形」当成同一个集合**〔`D6` 08-29 逮到，措辞级〕：
    /// 上面那句的 4 形是 **{0, 1, 2, 2s}**（含基线、不含形 3），这里的 4 个是 **{1, 2, 2s, 3}**（不含基线）。
    /// **两句都对，而同一个数字承了两件事** ⇒ 现在两处都把成员逐个列出来，不再只写数字。
    /// ⚠ **哪一行是哪一拍量的，看表里的署名列**；🔴 而「它今天的活体不在这 6 格里」那句话
    /// **是假的**（`D5` 08-29 打穿，我复打确认）—— 这份文件里它有**两个副本**，本轮两处都改了，
    /// 见本节末尾「形 3 今天要怎么才复现得出」那一段。
    ///
    /// 🔴 **这一段连着写错过两版，而两版错在相反的方向**：
    /// 第一版写「兜住它的是 `gate.sh` 的 `run_gate_sum` 包数自检」（**说得比实际强** ——
    /// 那只在 runner 不真跑测试时成立）；`D2` 改成「**今天没有任何判据兜得住**，
    /// 本条兜不住，门禁也兜不住」（**说得比实际弱** —— 那句话对下面的形 1 与形 2 都是假的）。
    /// ★ **过度自责与过度自信一样，都是话与实测对不上。**
    ///
    /// 正确的说法要**分形说**〔PM 08-28 裁定的三形表；`D3` 逐形实打、`D4` 08-29 独立复核；
    /// **形 2s 是 `D5` 08-29 加的第四形**，我 08-29 复打。
    /// ⚠ **哪一行是哪一拍量的、开没开 pipefail，看最后一列的署名** ——
    /// `K-G2` 那几拍的量具是 `/home/zbl/.cache/kg2r6/gatedoor.sh`（逐字复刻 `scripts/gate.sh` 里
    /// **`run_gate_sum cargo 8 …` 那条 cargo 命令**与 **`run_gate_sum()` 那个函数**，
    /// ⚠ **连 `gate.sh` 顶上那一行 `set -uo pipefail` 一起复刻**，
    /// 被测对象是本工作树、`CARGO_TARGET_DIR` 独立）。
    /// ⚠ **`K-R5` 09-02 那一拍换了一把**：`evidence/K-R5-C-r7-door.py`（同样是复刻那三样，
    /// 但**跑在沙箱里**、`CARGO_TARGET_DIR` 落 `.claude/pm-targets/`，
    /// 并且复刻了 `.claude/devbox/gate` 里那句 `mkdir -p "$HOME/.claude/projects"` ——
    /// 少了它 `history` 那条围栏判据会因为「目录不存在」被拒，基线**假红**；09-02 现打过这个反例）。
    /// 🔴 **上面这三处原本写的是行号**，**09-02 现打三处全指到注释行** ——
    /// 成因（`K-G3` 09-01 加门六，把行号整体推下去）与订正法（一律换成内容锚点）
    /// 见上面那张「谁从这里发起 cargo」表的头一段。〕：
    ///
    /// | 形 | 谁兜得住 · 靠哪一支 | 四个数 | 谁在哪一拍量的 |
    /// |---|---|---|---|
    /// | **0 · 基线**：6 格全缺席 | —— | `rc=0` · 包数 **8** · 合计 **1303** · 判定**绿** · 代理 marker 0 | **08-29 本拍现打**（`kg2r6`，pipefail 开） |
    /// | **1 · 不透明 `runner`**（不真跑测试，如 `/bin/echo`） | ✅ **门禁兜得住** —— 一条 `test result: ok` 都产不出 ⇒ 采集面自检当场红 | `rc=0` · 包数 **0** · 合计 **`0`** · 判定**红**（只跑到 0 个包，应当 8） | **08-29 本拍现打**（`kg2r6`，pipefail 开） |
    /// | **2 · 透明代理 `runner`**（起真二进制、原样透出） | ✅ **本条自己兜得住** —— 透明代理让本条**真的跑起来**，而它看见 `runner` 就红 | `rc=101` · 包数 5 · 合计 83 · 判定**红**；本条逐字 `… stays_registered ... FAILED`，报文点名 `["runner"]`；代理真起了 6 个二进制 | ⚠ **这四个数我这一拍没重打**（住址：`§12.2` 与 `audits/K-G2-D5.md#§3.2` 各一次）。本拍现打的是它承重的那一半：判据一跑就点名 `["runner"]`（见上面形 2s 那段的单证） |
    /// | 🔴 **2s · 透明 skip-runner**：`exec "$@" --skip <本条名>` —— **真跑测试，只把本条过滤掉** | 🔴 **没人兜得住** —— 判据**根本没跑**（`1 filtered out`），门禁只数包数与「非 0」，**对合计的具体值没有断言** | `rc=0` · 包数 **8** · 合计 **1302** · 判定**绿** · 代理 marker **8** · 本条命中 **0** 次 | **08-29 本拍现打**（`kg2r6`，pipefail 开）；这一形是 `D5` 找出来的 |
    /// | **3 · 透明代理 + 判据看不见那个 `runner`** | 🔴 **没人兜得住** —— 而这正是「净变宽」那一族，三次被逮到的都是这一形 | `rc=0` · 包数 **8** · 合计 **1303** · 判定**绿** · 代理 marker **8 条** ⇒ 与形 0 **逐个相等** | ⚠ **我这一拍没重打**（住址：`§9.2` 与 `audits/K-G2-D4.md#§3` 各一次独立读数） |
    ///
    /// 🔴 **形 1 那一格要点名是哪一支 —— ⚠ 但它不是独木桥**
    /// 〔`D3` 加的这一格，PM 的表里没有；「独木桥 / 唯一 / 松一松就变形 3」那半句
    /// `D4` 08-29 打穿了，下面是我 08-29 自己复打的四种松法〕：
    /// 形 1 里 **`rc = 0`** —— cargo 自己**成功退出**（`/bin/echo` 的退出码是 0）⇒
    /// `run_gate_sum` 的**退出码那一支在这一形上完全失效**；**先开火的**是
    /// `elif [ "$pkgs" -ne "$want_pkgs" ]`（第三支排在它后面、这一趟轮不到）。
    ///
    /// ⚠ **「唯一 / 独木桥」是假的，反例现打**：把第二支松掉或删掉，形 1 会落到第三支
    /// `[ -z "$n" ] || [ "$n" -eq 0 ]` 上，**仍然红**。08-29 拿形 1 的**真实输出**
    /// （`rc=0` · `pkgs=0` · `n=0`）跑分支模拟，**四种松法四种全红，分母 = 4**：
    /// `-lt 8` ⇒ 第二支照开火（`0 < 8`）· `-ne 0` ⇒ **第三支**接住 ·
    /// `-ne 1` ⇒ 第二支 · **整支删掉** ⇒ **第三支**接住。
    /// 非空对照：同一把尺子喂形 0 的输出 ⇒ **绿**（`1303 passed`／8 个包）⇒ 尺子不是恒红。
    ///
    /// ⚠ **口径一格，复打前先看**：`scripts/gate.sh` 顶上那一行逐字是 `set -uo pipefail`
    /// （**认这一行的字面，别认行号** —— 09-02 现打，它已经从 `:22` 被推到 `:72`）
    /// ⇒ 形 1 那条 `n=` 管道里 `grep` 无命中让整条管道失败、`|| echo 0` 兜出 **`"0"`**，
    /// 接住形 1 的是第三支的 **`[ "$n" -eq 0 ]`** 这个子条件，报文逐字
    /// 「读数是 0 —— 0 passed 不是绿」。**不开 pipefail** 时 `n` 是空串、走 `[ -z "$n" ]`、
    /// 报文是「读数是 `<找不到>`」—— 08-29 现打：`D3`/`K-G2 R4` 两份复刻量具都没开 pipefail，
    /// 上面那张表里形 1 的「合计」一度写成 `<找不到>`，**那是量具口径差，不是门禁的读数**。
    /// **两种都红**，差的只是哪个子条件先真；复打这条读数时要连「pipefail 开没开」一起写。
    ///
    /// ⇒ 真正只剩独木桥的，是**退出码那一支在这一形上完全失效**这件事本身。
    /// ⚠ 这几支都住 `scripts/gate.sh`，**不在本条的射程内**，本条只如实记这几句 ——
    /// 它归门禁自己那一件（`K-G3`），**别在这里修它，也别把它算成本条的覆盖**。
    /// 🔴 `K-G3` 注意：**别照「独木桥」那句去修一个不存在的洞** —— 那一支松掉/删掉，
    /// 第三支当场接住（上面四种松法的读数就是它的单证）。
    ///
    /// ⚠ **形 3 今天要怎么才复现得出**（08-29 复打时量到的，写下来免得下一个人以为它已经没了）：
    /// `D3` 那一刀是只把 `CARGO_CFG_DIRS` 里的 `src-tauri` 换掉（格数仍是 6）——
    /// **今天那一刀做不出形 3 了**，探针⑨（按字面量逐格钉住人群）会当场把本条打红。
    /// 我要**同时**改 `CARGO_CFG_DIRS` 与探针⑨ 的 `want_slots` 才复现得出。
    /// ⇒ 探针⑨ 买到的东西是真的；而形 3 这个**形状**照样成立。
    ///
    /// 🔴 **「它今天的活体不在这 6 格里」是假的 —— 这句话在本文件里有两个副本，08-29 两处都改了**
    /// 〔`D5` 08-29 打穿，我复打确认；⚠ `D5` 只点了这一处，另一处在上面「哪一行是哪一拍量的」
    /// 那一段 —— **审计的尺子有作用域，而病没有**，所以我 `grep` 了承重词
    /// 「活体不在这 6 格」——**量于改动前的 `05cbf2c`**：`git ls-files` **703** 个跟踪文件全扫
    /// ⇒ 命中 **2 处**，**都在本文件**（另一处就是上面那一段），两处都改了。
    /// ⚠ 这个数是**那一刻的快照**：本段自己也含这几个字，改完之后再扫会更多〕。
    /// **按那四条性质（配置真生效 · 本条没红 · 门禁绿 · 两边都没兜住）算，这 6 格里今天就有活体**：
    /// **形 2s** —— 一份 `src-tauri/.cargo/config.toml` 就够（08-29 现打：`rc=0` · 包数 **8** ·
    /// 合计 **1302** · 门禁**绿** · 本条命中 **0** 次）。
    /// ⚠ **说准一格**：形 2s 与形 3 **机制不同**（「根本没跑」vs「跑了但看不见」）；
    /// 而**就「一次 commit 要动几处」这个口径**，形 2s 便宜得多 —— 形 3 要**同时改判据源码两处**
    /// （`CARGO_CFG_DIRS` 与探针⑨ 的 `want_slots`，上面刚写过），形 2s **只落一份配置文件**，
    /// 而且那份文件落在**被守的格位里**（判据只要跑就逮得到它 —— 上面形 2s 那段有单证）。
    /// 另一个（更贵的）活体在上面「不守什么」栏 `rust-toolchain.toml` 那一行：它至少要改仓根。
    ///
    /// 本条能逮到形 2 / 形 3 的，还有「那份配置**还没生效**就被看见」的时机
    /// （配置在别的发起面 · 别人 clone 之后第一次跑 · 审 diff 的人）。
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
        // 探针⑨：**人群的身份**，不是它的个数〔`K-G2` `D3` 补，08-28；`K22`〕。
        // ⚠ 上面那条 `slots.len() == 6` 是一个**表面量**（`testing.md` 硬规则 6）——
        //   `D3` 现打：把 `CARGO_CFG_DIRS` 的 `remote-daemon-proto` 换成 `e2e`，格数仍是 6
        //   ⇒ 上面那条照过，而**真 `runner` 放进 `remote-daemon-proto/.cargo/config.toml`
        //   判据也照绿** —— 那一格正是 `KG2B` 这一件买来的。
        //   ⇒ 人群必须按**字面量逐格**钉住，个数只是它的副产品。
        let want_slots: &[&str] = &[
            "./.cargo/config.toml",
            "./.cargo/config",
            "src-tauri/.cargo/config.toml",
            "src-tauri/.cargo/config",
            "remote-daemon-proto/.cargo/config.toml",
            "remote-daemon-proto/.cargo/config",
        ];
        assert_eq!(
            slots.iter().map(String::as_str).collect::<Vec<_>>(),
            want_slots,
            "本条的人群**逐格**变了。\n\
             ⚠ 换掉一格而不改格数，上面那条 `len == 6` 一个字都不会说 —— 这一条才认得出来。\n\
             真要改（新发起面 / cargo 换了认的文件名）：连本条头注那张\n\
             「谁从这里发起 cargo」的表一起改，并把新的尺子读数写进去。"
        );

        // 抽取器自检：探针坏了的话，下面整条就是**零命中地绿**。
        // ⚠ 探针③ 是 `D1` 那个洞的**常驻**版本 —— 它一旦变绿，本条就又瞎了。
        let probe_exec = "[target.'cfg(all())']\nrunner = \"sh\"  # 这里的 linker 是注释\n";
        let probe_profile = "[profile.dev]\ndebug = \"line-tables-only\"\n";
        let probe_escaped = "[target.x86_64-unknown-linux-gnu]\n\"run\\u006Eer\" = \"/bin/echo\"\n";
        let probe_hex = "[build]\n\"rustfla\\x67s\" = []\n";
        let probe_backslash_value = "[profile.dev]\nrustc = \"C:\\\\tools\\\\x.exe\"\n";
        assert_eq!(
            keys_in(&stripped_ok(probe_exec), CARGO_EXEC_KEYS),
            vec!["runner"],
            "探针①：真设了 `runner` 的配置必须被逮到，且注释里的 `linker` 不算"
        );
        assert!(
            keys_in(&stripped_ok(probe_profile), CARGO_EXEC_KEYS).is_empty(),
            "探针②：只有 `[profile.*]` 的构建瘦身配置必须放行 —— 那正是 `K-G2` 收窄掉的那一半"
        );
        assert_eq!(
            keys_in(&stripped_ok(probe_escaped), CARGO_EXEC_KEYS),
            vec!["runner"],
            "探针③：**用 TOML 转义拼出来的键**必须被逮到。\n\
             这一格是 `K-G2` `D1` 现场逮到的洞：cargo 认解码后的键名，\n\
             而当时的整词扫看的是原文 ⇒ `cargo test` 被交给了 `/bin/echo`，本条却是绿的。"
        );
        assert_eq!(
            keys_in(&stripped_ok(probe_hex), CARGO_EXEC_KEYS),
            vec!["rustflags"],
            "探针④：`\\xXX`（TOML 1.1）那种写法 cargo 1.96.1 也认，本条也要认"
        );
        assert!(
            keys_in(&stripped_ok(probe_backslash_value), CARGO_EXEC_KEYS).is_empty(),
            "探针⑤：**值**里有反斜杠（Windows 路径）不许误红 —— 本条判的是键，不是有没有 `\\`"
        );
        assert!(
            backslash_in_key_position(&stripped_ok(probe_escaped)),
            "探针⑤b：键位置那道**兜底**必须逮到转义键 —— 它是「不靠数全转义种类」的那一半"
        );
        assert!(
            !backslash_in_key_position(&stripped_ok(probe_backslash_value)),
            "探针⑤c：值里的反斜杠不许触发兜底，否则 Windows 路径会误红成灾"
        );
        // ⑤d/⑤e：内联表那一格 —— 实测 cargo 认内联表 + 转义键，兜底必须跟进去。
        let probe_inline_key = "target = { x = { \"run\\u006Eer\" = \"/bin/echo\" } }\n";
        let probe_inline_value = "env = { P = { value = \"C:\\\\t\", relative = false } }\n";
        assert!(
            backslash_in_key_position(&stripped_ok(probe_inline_key)),
            "探针⑤d：**内联表里**的转义键也要被兜底逮到（只看第一个 `=` 之前会漏）"
        );
        assert!(
            !backslash_in_key_position(&stripped_ok(probe_inline_value)),
            "探针⑤e：内联表里**值**的反斜杠不许误红 —— 兜底认的是键位置，不是整行"
        );
        // ⑤f：兜底的**第二支** ——「整行没有 `=`」那一支〔`K-G2` `D3` 补，08-28；`K22`〕。
        // ⚠ ⑤b/⑤d 两个样本都有 `=`，走的是「每个 `=` 往前回溯」那一支
        //   ⇒ `D3` 现打：把 `!line.contains('=') && line.contains('\\')` 拆掉，
        //   **上面每一条探针照绿**。这个样本整份没有 `=`，**只**由那一支挡住。
        // ⚠ 它也不会被 `keys_in` 顺带逮到：解开是 `[build]`，不在那三个执行键里。
        let probe_header_escape = "[bui\\u006Cd]\n";
        assert!(
            keys_in(&stripped_ok(probe_header_escape), CARGO_EXEC_KEYS).is_empty(),
            "探针⑤f 前半：这个样本必须**不**被整词扫命中，否则它证不了兜底那一支"
        );
        assert!(
            backslash_in_key_position(&stripped_ok(probe_header_escape)),
            "探针⑤f：**没有 `=` 的行**（表头 / 多行值的续行）里的反斜杠必须被兜底逮到。\n\
             08-28 实测 cargo 认表头里的转义（`[\"ali\\u0061s\"]` 生效）⇒ 转义面不止叶子键。"
        );
        // ⑧：**整词边界**这一维〔`K-G2` `D3` 补，08-28；`K22`〕。
        // ⚠ `D3` 现打：把 [`word_in`] 退化成 `text.contains(key)`，**上面每一条探针照绿**
        //   —— 而那一维正是 `K-G2` 这一件买的东西（「人群比性质大」的那一半）：
        //   同一份变异下，`[profile.dev.package."linker-utils"]` 这份**纯瘦身**配置
        //   会被点名 `设了执行面的键 ["linker"]`，用户的仓库又恒红一次。
        let probe_word_boundary = "[profile.dev.package.\"linker-utils\"]\ndebug = false\n";
        assert!(
            keys_in(&stripped_ok(probe_word_boundary), CARGO_EXEC_KEYS).is_empty(),
            "探针⑧：`linker-utils` 这类**包含**那三个词、却不是那个键的写法必须放行 ——\n\
             本条判的是整词，不是子串。这一条一旦变红，`K-G2` 收窄掉的那一半就回来了。"
        );
        let probe_include = "include = [\"../elsewhere/x.toml\"]\n[profile.dev]\ndebug = false\n";
        assert!(
            keys_in(&stripped_ok(probe_include), CARGO_EXEC_KEYS).is_empty(),
            "探针⑥前半：`include` 不是执行面键，别把它算进 `CARGO_EXEC_KEYS`"
        );
        assert_eq!(
            keys_in(&stripped_ok(probe_include), CARGO_BLINDING_KEYS),
            vec!["include"],
            "探针⑥后半：`include` 必须被逮到 —— 它能把 6 格之外的文件拉进来，本条读不到那份"
        );

        // ⑦ **预处理这一维**的常驻探针〔`D2` 回修〕。三个样本的真键都是**明文 `runner`**，
        //    零反斜杠 ⇒ 上面那两层（解码 · 键位反斜杠）**一层都不会被触发**，
        //    唯一挡住它们的就是「剥注释看不懂 ⇒ 判红」。⚠ 这三条一旦变绿，本条就又瞎了。
        let probe_ml_hash = "target = { x = { ar = \"\"\"\n# \"\"\", runner = \"/bin/echo\" } }\n";
        let probe_ml_literal = "target = { x = { ar = '''\n# ''', runner = \"/bin/echo\" } }\n";
        let probe_ml_midline = "a = \"\"\"\nzz # \"\"\"\nrunner = \"/bin/echo\"\n";
        // ⚠ ⑦d 是**变异逼出来的**：把「行尾引号未闭合」那一支拆掉之后，上面三个样本
        //    仍被「多行定界符」那一支挡着 ⇒ 判据照绿。⇒ 那一支当时**没有任何探针盯着**。
        //    这个样本里没有三引号，**只有**未闭合引号这一个信号。
        let probe_unterminated = "a = \"unterminated\nrunner = \"/bin/echo\"\n";
        for (sample, name) in [
            (probe_ml_hash, "多行基本串 + `#` 起头的续行"),
            (probe_ml_literal, "多行字面串 `'''` 那一版"),
            (probe_ml_midline, "`#` 不在行首那一版"),
            (
                probe_unterminated,
                "行尾引号未闭合（**只**由这一支信号挡住）",
            ),
        ] {
            let (text, unmodeled) = strip_toml_comments(sample);
            assert!(
                !unmodeled.is_empty(),
                "探针⑦（{name}）：剥注释**必须回报看不懂**。\n\
                 这一格是 `D2` 现场逮到的洞：续行那个 `#` 把明文的 `runner` 整行切掉，\n\
                 而它零转义零反斜杠 ⇒ 解码与键位兜底两层都不会被触发。\n\
                 剥完的文本是 {text:?} —— 真键已经不在里面了。"
            );
        }
        // ⑦e：剥注释的**第一支** ——「多行字符串定界符」〔`K-G2` `D3` 补，08-28；`K22`〕。
        // ⚠ 上面四个样本里，前三个同时触发两支信号（定界符 · 行尾引号未闭合），⑦d 只触发第二支
        //   ⇒ `D3` 现打：把定界符那一支拆掉，**上面四条探针照绿**（`1 passed`）。
        //   这个样本的三引号在同一行内闭合 ⇒ 行尾引号态是闭合的，**只**触发定界符那一支。
        let probe_ml_balanced = "a = \"\"\"x\"\"\"\n";
        let (_, ml_balanced_why) = strip_toml_comments(probe_ml_balanced);
        assert_eq!(
            ml_balanced_why.len(),
            1,
            "探针⑦e：这个样本应当**只**触发一支信号（定界符），实得 {ml_balanced_why:?} ——\n\
             触发了两支就证不了「只由这一支挡住」，换一个样本。"
        );
        assert!(
            ml_balanced_why[0].contains("多行字符串定界符"),
            "探针⑦e：看见 `\"\"\"` / `'''` 就判红这一支必须活着。\n\
             本剥法**逐行**走、引号态逐行重置 ⇒ 它对跨行的那两种字符串是**不认**的，\n\
             fail-closed 是它唯一诚实的出路。实得的理由是 {ml_balanced_why:?}。"
        );

        // ══════════════════════════════════════════════════════════════════════
        // ⑩–⑮ **装配层那 6 支**〔`K-R5` 09-02；`K22`〕
        //
        // ★ 治的是什么：上面 ①–⑨ **全都直接调那几个原语**，没有一条走过
        //   [`offenders_under`] 那个闭包；而生产那一遍 6 格全缺席 ⇒ `!p.is_file()`
        //   一律提前 `return None` ⇒ **闭包体后面整段今天根本不执行**。
        //   `K-R5` 09-02 在今天的主干上逐支现打：那 7 支里 **6 支拆掉、判据与整道
        //   cargo 门都绿**（`rc=0` · 包数 8 · 合计 1313）。本组把这 6 支买下来。
        //
        // 🔴 **一支一条，不许一条挡 6 支**（`K22` 逐字：N 支信号要 N 个只由这一支挡住
        //   的探针）。逐刀现打：**这 6 支每拆一支，只有它自己那一条开火，排在它前面的全绿。**
        //
        // ⚠⚠ **「只由这一支挡住」的作用域，说准**〔09-02 自查逮到我自己一次说宽，改了〕：
        //   它说的是**装配层这 6 支之内**。**不是**「全 41 支里只由它挡住」——
        //   上游那几个原语的信号（[`strip_toml_comments`] 的定界符那一支 · `CARGO_EXEC_KEYS`
        //   那张表 · [`word_in`] 的整词边界 …）拆掉，下面几条同样会开火，因为样本要路过它们。
        //   **那几支今天本来就有牙**，所以这不是「一条探针挡多支」，是「上游坏了下游也报」。
        //   `K22` 要的是**同一层之内**一支一条，这一格是满足的。
        //
        // ⚠ **`F1`（`!p.is_file()`）本组没配探针** —— 它今天**有牙**（拆掉它，主 `assert!`
        //   逐字列出全部 6 格），再配一条是重复。它的诚实边界写在 [`offenders_under`] 头注。

        // ⑩ `let Ok(raw) … else`（`K-G2` 的 `S4`）：**存在但读不出文本** ⇒ 必须上报。
        //    ⚠ 样本是**非法 UTF-8 字节**：`is_file()` 对它恒真 ⇒ 拆掉 `F1` 这条一声不吭；
        //      拆掉 `F3`–`F7` 它够不着（在那之前就 `return` 了）⇒ 装配层这 6 支里只由它挡住。
        //      ⚠ 它连**上游那些原语**都够不着（样本压根不是合法文本，一层剥法都没跑到）
        //      ⇒ 这一条恰好是 6 条里唯一一条**在全 41 支上也只由它挡住**的。
        let why = probe_slot_verdict("a", &[0xff, 0xfe, 0x00, 0x80]);
        assert!(
            why.as_deref()
                .is_some_and(|w| w.contains("存在但读不出文本")),
            "探针⑩（`let Ok(raw) … else` 那道守卫；**装配层这 6 支里只由它挡住**）：\n\
             一格**存在、却读不出文本**时本条必须上报它 —— 那正是「本条看不了它的内容」。\n\
             改成 `return None` 就等于「读不出就当没事」。这一格实得 {why:?}。"
        );

        // ⑪ 预处理 fail-closed（`F3`）：样本**只**触发「剥注释看不懂」这一支 ——
        //    它不含那三个执行键、不含 `include`、键位零反斜杠 ⇒ 拆掉 `F4`/`F5`/`F6`
        //    它照样上报，拆掉 `F3` 当场变绿。
        let why = probe_slot_verdict("b", b"a = \"\"\"x\"\"\"\n");
        assert!(
            why.as_deref().is_some_and(|w| w.contains("剥注释这一步")),
            "探针⑪（预处理 fail-closed；**装配层这 6 支里只由它挡住**）：\n\
             剥注释回报「看不懂」时，装配这一层必须把它变成一条 offender 理由。\n\
             这一支一旦恒假，`D2` 那个洞（多行串把真键整行切掉）就原样回来。实得 {why:?}。"
        );

        // ⑫ 执行面键（`F4`）：样本只设了 `runner`，别的信号一支不触发。
        let why = probe_slot_verdict("c", b"[target.x]\nrunner = \"sh\"\n");
        assert!(
            why.as_deref().is_some_and(|w| w.contains("设了执行面的键")),
            "探针⑫（`exec` 那一支；**装配层这 6 支里只由它挡住**）：\n\
             [`keys_in`] 逮到了执行面键，而装配这一层必须把它变成一条 offender 理由。\n\
             ⚠ 原语有牙不等于装配有牙：这一支恒假时 [`keys_in`] 照常返回 `[\"runner\"]`，\n\
             只是没人再看它一眼。实得 {why:?}。"
        );

        // ⑬ 致盲键（`F5`）：样本只设了 `include`，不含那三个执行键、键位零反斜杠。
        let why = probe_slot_verdict("d", b"include = [\"../elsewhere/x.toml\"]\n");
        assert!(
            why.as_deref()
                .is_some_and(|w| w.contains("本条看不进它拉进来的文件")),
            "探针⑬（`blind` 那一支；**装配层这 6 支里只由它挡住**）：\n\
             `include` 能把那 6 格之外的任意文件拉进来，而本条读不到那一份 ⇒ 按 fail-closed 判红。\n\
             这一支恒假 = 本条被蒙上眼睛还自称绿。实得 {why:?}。"
        );

        // ⑭ 键位反斜杠（`F6`）：样本整行没有 `=`、解开是 `[build]`（不在那三个执行键里）
        //    ⇒ `F3`/`F4`/`F5` 一支都不触发。
        let why = probe_slot_verdict("e", b"[bui\\u006Cd]\n");
        assert!(
            why.as_deref().is_some_and(|w| w.contains("键位置有反斜杠")),
            "探针⑭（键位反斜杠那一支；**装配层这 6 支里只由它挡住**）：\n\
             [`backslash_in_key_position`] 是「不靠数全转义种类」的那一半兜底，\n\
             而装配这一层必须把它的 `true` 变成一条 offender 理由。实得 {why:?}。"
        );

        // ⑮ `why.is_empty()`（`F7`）：**干净的一格必须判 `None`**。
        //    ⚠ 这一支有**两张脸**，本条买的是其中一张，另一张说清在哪：
        //      · `if false`（恒当成 offender）⇒ **只**由本条挡住（下面这条断言当场红）；
        //      · `if true`（永不上报）⇒ 由 ⑪⑫⑬⑭ **四条一起**挡住 —— 它是那四条的
        //        **下游**，没有任何探针能「只」挡住它。**这不是探针的毛病，是这段代码的形状。**
        let why = probe_slot_verdict("f", b"[profile.dev]\ndebug = false\n");
        assert!(
            why.is_none(),
            "探针⑮（`why.is_empty()` 那道分岔的 `if false` 那张脸；**装配层这 6 支里只由它挡住**）：\n\
             一条理由都没有的一格**必须**判 `None` —— 恒当成 offender 会让用户的仓库恒红，\n\
             而 08-27 真发生过一次（那次的消法是把判据放宽，本工作区走过太多次了）。实得 {why:?}。"
        );

        // ══════════════════════════════════════════════════════════════════════
        // ⑯–㉑ 原语那几支里**买下来的**〔`K-R5` 09-02〕
        //
        // 🔴 **买的判准**（逐支过一遍铁律 18「宁可宽松，别用严格的错误引入噪声」）：
        //   这一支拆掉会产生**已量到的实害形状**（漏红：真 `runner` 不再被点名；
        //   误红：一份纯瘦身配置被点名），**且**它守的是这几个函数 docstring
        //   **自己声明的契约**，而不是 fail-closed 那个**近似的具体形状**。
        //   ⇒ 换一种剥法（比如真去认多行串）只要仍满足契约，下面这几条照样绿。
        // 🔴 **没买的两支写在头注**（`K-R5-D3`）：`B2`（`'''` 定界符）· `E1`（或模式候选 `b'{'`）。

        // ⑯ 剥注释契约的一半：**基本串里被转义的引号不结束这个串**〔`B4`+`B5`〕。
        //    docstring 逐字：「`#` 到行尾；**字符串里的 `#` 不算注释**」。
        //    ⚠ `B4`（`if escaped`）与 `B5`（`c == '\\'`）**在函数外面behaviour 上分不开**：
        //      任拆一支，`\"` 都会当场把串关掉 ⇒ 同一条探针挡住两支。这是**实测出来的**，
        //      不是偷懒；要分开得去断言内部状态，那就把实现细节钉死了。
        //    ⚠ 这条也会被 `B7`（`quote == Some(c)`）拆掉时打红，而 `B7` 今天本来就有牙（⑦）。
        let sample_escaped_quote = "target = { x = \"a\\\"#b\", runner = \"sh\" }\n";
        assert_eq!(
            keys_in(&stripped_ok(sample_escaped_quote), CARGO_EXEC_KEYS),
            vec!["runner"],
            "探针⑯：基本串里的 `\\\"` **不结束这个串** ⇒ 它后面那个 `#` 不是注释，\n\
             真键 `runner` 必须活着走出剥注释这一步。\n\
             这一支一恒假，`#` 之后整行被切掉，而剥注释**一声不吭**（引号态在 break 时是闭合的）\n\
             ⇒ 那是一次**静默漏红**，与 `D2` 逮到的那个洞同一维。"
        );

        // ⑰ 剥注释契约的另一半：**字面串 `'…'` 里没有转义**〔`B6`〕。
        //    ⚠ 方向是**误红**：这一支一旦恒真，一条以反斜杠结尾的 Windows 路径
        //      （`'C:\tools\'` —— TOML 字面串最教科书的写法）会被读成「引号没闭合」
        //      ⇒ fail-closed 判红 ⇒ 用户的仓库恒红。08-27 真发生过一次同族的事。
        let (_, why_literal_path) = strip_toml_comments("rustc = 'C:\\tools\\'\n");
        assert!(
            why_literal_path.is_empty(),
            "探针⑰：TOML **字面串**里反斜杠就是反斜杠（没有转义）—— \
             `'C:\\tools\\'` 这一行本剥法必须认得，不许回报「看不懂」。\n\
             实得 {why_literal_path:?}。⚠ 这一条护的是**误红**那一侧：误红最省事的消法\
             是把判据放宽，那条路本工作区走过太多次了。"
        );

        // ⑱ 剥注释契约的第三条：**字面串里的 `#` 不算注释**〔`B9`〕。
        //    ⚠ 拆掉这一支，`'` 不再开串 ⇒ 串里那个 `#` 把整行切掉，而引号态是闭合的
        //      ⇒ **一声不吭地漏红**。⚠ 这条也会被 `B7` 拆掉时打红（`B7` 今天有牙）。
        let sample_hash_in_literal = "target = { x = 'a#b', runner = \"sh\" }\n";
        assert_eq!(
            keys_in(&stripped_ok(sample_hash_in_literal), CARGO_EXEC_KEYS),
            vec!["runner"],
            "探针⑱：**字面串**里的 `#` 不是注释 —— 真键 `runner` 必须活着走出剥注释这一步。\n\
             这一支恒假 ⇒ 整行从 `#` 处被切掉，而「看不懂」一个字都不报。"
        );

        // ⑲ 转义面的第三种：`\UXXXXXXXX`〔`C2`〕。
        //    [`decode_toml_escapes`] 认三种（`\u` · `\U` · `\x`），而 08-29 之前
        //    **只有 `\u`（③）与 `\x`（④）有探针**。三种 cargo 都认（头注那段实测）。
        let probe_upper_escape = "[target.x]\n\"\\U00000072unner\" = 1\n";
        assert_eq!(
            keys_in(&stripped_ok(probe_upper_escape), CARGO_EXEC_KEYS),
            vec!["runner"],
            "探针⑲：`\\UXXXXXXXX`（8 位十六进制）那种写法 cargo 也认，本条也要认。\n\
             这一格与 `D1` 逮到的那个洞同形：cargo 认**解码后**的键名，\n\
             而这一臂一旦答通配臂那个 0，`\"\\U00000072unner\"` 就再也解不成 `runner`。"
        );

        // ⑳ 整词边界的**前**半〔`D4`〕与词字符那两类〔`D1` 字母数字 · `D2` 下划线〕。
        //    ⚠ 探针⑧ 买的是**后**半（`linker-utils`），前半今天没人盯着。
        //    ⚠ **分不开的一格如实记**：`D4` 是「前面那一侧」的总闸 ⇒ 任何前半样本都同时
        //      挡住 `D4` 与它用到的那一类词字符；没有任何样本能「只」挡住 `D4`。
        //      两个样本各自把 `D1` / `D2` 单独挡住（换一类词字符，另一类拆掉照绿）。
        for (sample, cls) in [
            (
                "[profile.dev.package.myrunner]\ndebug = false\n",
                "字母数字",
            ),
            ("[profile.dev.package.my_runner]\ndebug = false\n", "下划线"),
        ] {
            assert!(
                keys_in(&stripped_ok(sample), CARGO_EXEC_KEYS).is_empty(),
                "探针⑳（整词边界的**前**半 + 词字符「{cls}」那一类）：\n\
                 `myrunner` / `my_runner` 这类**前面接着词字符**的写法不是那个键，必须放行。\n\
                 探针⑧ 只买了「后」那半（`linker-utils`）—— 前半一旦失守，\n\
                 `K-G2` 收窄掉的那一半（「人群比性质大」）就从另一侧回来，\n\
                 而它的代价是实打实的：08-27 用户主树上的瘦身配置让本条恒红了一次。"
            );
        }

        // ㉑ [`keys_in`] 扫**原文**那一遍〔`G1`〕—— 本文件头注 08-29 就记着这条欠账。
        //    ⚠ 解码那一遍由探针③ 挡着；原文那一遍在本条之前**零常驻覆盖**。
        //    机制：解码把 `\`（非词字符）换成词字符 ⇒ **两个方向都会动整词边界**。
        //    这个样本走的是「解码之后整词反而不成立」那个方向。
        let probe_raw_pass = "[target.x]\n\"runner\\x41\" = \"y\"\n";
        assert_eq!(
            keys_in(&stripped_ok(probe_raw_pass), CARGO_EXEC_KEYS),
            vec!["runner"],
            "探针㉑：`\"runner\\x41\"` —— **原文**里 `runner` 后面是 `\\`（非词字符）⇒ 整词成立；\n\
             解完是 `runnerA`，后面是 `A`（词字符）⇒ 整词**不**成立。\n\
             ⇒ 只有**原文那一遍**逮得到它。那一遍恒假，这一形当场从报文里消失，\n\
             而 [`keys_in`] 头注逐字写着「两遍都要」。"
        );

        // ⚠ 这一段的 7 支信号住 [`offenders_under`]（`K-R5` 09-02 抽出来的，**行为没动**）——
        //   抽出来是为了让探针 ⑩–⑮ 能喂它一份夹具根，理由见那个函数的头注。
        let offenders: Vec<String> = offenders_under(&root, &slots);
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
