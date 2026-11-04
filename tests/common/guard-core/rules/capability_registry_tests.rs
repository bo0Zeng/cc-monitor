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
    ("core:window:allow-minimize", "最小化"),
    ("core:window:allow-set-title", "窗口标题写「cc-monitor · 需手动 N」（任务栏、Alt+Tab 里看得见谁在等你）"),
    ("core:window:allow-set-fullscreen", "全屏"),
    ("core:window:allow-is-fullscreen", "查全屏状态"),
    ("core:window:allow-close", "关窗"),
    (
        "core:window:allow-unminimize",
        "把窗口拉到前面那一下先还原：主窗口（设置窗跳回账号面板 · 起会话落地）与 agent 窗口（滚到派出卡）",
    ),
    ("core:window:allow-set-focus", "同上，还原之后给它焦点"),
    (
        "core:window:allow-destroy",
        "agent 窗口接管了关窗（关之前告诉别的窗口「这扇关了」）又放行：Tauri 在处理完之后替它 `destroy()`，\
             没有这一格 ⇒ 点 × 关不掉。同发给三类窗口（理由同 `allow-hide` 那一条）",
    ),
    ("core:webview:allow-set-webview-zoom", "主窗口放大 / 缩小 / 还原（Ctrl+= · Ctrl+- · Ctrl+0，记住）"),
    (
        "core:window:allow-hide",
        "ST1「关窗改隐藏」：设置窗接管了 close-requested，\
             放行动作是 `hide()`（不销毁、下次 show 出来复用）。⚠ 这份 capability 同时发给 \
             `main` / `viewer-*` / `settings` 三类窗口 —— 另开一份只给 settings 的 capability 文件能收窄，\
             但本条判据只读 `default.json`，另开文件等于绕过它，所以不收窄、如实登记。\
             藏起来的设置窗会吊住进程 ⇒ 主窗销毁时后端把它一起 destroy（`lib.rs::windows_to_destroy_after`）",
    ),
];

/// 继承这套权限的窗口模式。改动它 = 改动「谁拿到这些能力」。
const WINDOWS: &[&str] = &["main", "viewer-*", "settings"];

/// 读 `src/frontend/shell/` 下的一个同级配置文件。
fn capability_json_sibling(name: &str) -> String {
    let p = guard_core::addr::shell_root().join(name);
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("读不到 {p:?}：{e} —— 路径变了就把本条一起改"))
}

fn capability_json() -> String {
    let p = guard_core::addr::shell_root().join("capabilities/default.json");
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("读不到 {p:?}：{e} —— 路径变了就把本条一起改"))
}

/// ★★ **前端的执行面（CSP + 全局 Tauri）也是那三张表的前提**。
///
/// 上面那条钉的是「webview 被授予哪些能力」。**还有一半**：webview 里跑的
/// **代码本身**从哪来。本产品渲染的是**不受信的会话文本**（Claude 的输出、
/// 远端 tmux 的 capture-pane），一旦脚本源被放开，注入的内容就能直接 `invoke`
/// 我们所有的命令 —— 那同样绕过写盘 / 远端执行 / 起进程那三张表。
///
/// 今天的形态是对的（CSP 无 `script-src 'unsafe-inline'`、无远端源，
/// `withGlobalTauri: false`），但它需要一条判据钉着：把 CSP 放开成
/// `default-src 'self' 'unsafe-inline' *` 并打开 `withGlobalTauri`，别的测试一条都不红。
///
/// ⚠ 只钉**会扩大执行面**的那几件，不逐字钉整条 CSP：
/// 逐字钉会把「加一个 `img-src` 源」这种无关改动也判红，
/// 而误红最省事的消法是把判据放宽 —— 那条路本工作区走过太多次了。
#[test]
fn the_webview_execution_surface_stays_closed() {
    let raw = capability_json_sibling("tauri.conf.json");
    let v: serde_json::Value = serde_json::from_str(&raw).expect("tauri.conf.json 不是合法 JSON");
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
    // 抽取器坏了抽不到东西 ⇒ 下面反向那一条（登记的必须真在清单里）红。

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

/// cargo 配置里**真的会扩大执行面**的三个键：
///
/// - `runner` —— `cargo test` / `cargo run` 把产物交给它去执行。「跑测试」的中间人就是它。
/// - `linker` —— 链接期换掉链接器：构建机上真正跑起来的是它指定的那个程序。
/// - `rustflags` —— 原样透传给 rustc 的旗标；`-C linker=…` / `-Z …` 都能从这一格进来。
///
/// ⚠ **不守**：`rustc-wrapper` / `rustdocflags` / `[alias]` 这些同族的键没进本表
/// （本条的射程只有上面三个）。
/// `[alias]` 里塞 `--config build.rustflags=…` 这一形会被下面的整词匹配顺带逮到，
/// 但那是**副产品，不是判据** —— 别当它有覆盖。
const CARGO_EXEC_KEYS: &[&str] = &["runner", "linker", "rustflags"];

/// **不是执行面键，是「会让本条看瞎」的键**。
///
/// `include` 让一份 cargo 配置**把另一份文件整个拉进来**，而那份文件可以在
/// [`CARGO_CFG_DIRS`] × [`CARGO_CFG_NAMES`] 那 6 格**之外的任意位置**。
/// 在 cargo 1.96.1 stable 上实测（一次性 crate + `[alias]` 探针，带非空对照）：
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
const CARGO_CFG_DIRS: &[&str] = &[".", "src/frontend/shell", "src/backend"];

/// cargo 认的两种配置文件名（无扩展名那个是老写法，仍然照读）。
const CARGO_CFG_NAMES: &[&str] = &["config.toml", "config"];

/// 剥掉一份 cargo 配置里的**注释**（`#` 到行尾；字符串里的 `#` 不算注释）。
///
/// ⚠ 刻意**只剥注释、不剥字符串内容**：TOML 的键可以带引号（`"runner" = "sh"`），
/// 内联表的值里也住着键（`target = { x = { runner = "sh" } }`）——
/// 把字符串一起剥掉就会在这两处**漏红**。代价：值里恰好出现那三个整词也会红。
///
/// ⚠⚠ 本条的**方向**是「宁可误红」，但**别把方向读成结论** —— 「还有没有别的写法」
/// **没有穷举的分母**：本条量过哪几种、剩哪几种不守，逐条写在
/// [`the_build_time_execution_surface_stays_registered`] 头注的「不守什么」栏。
///
/// ★★ **这一步为什么必须 fail-closed，而不是「再补一个例」**。
///
/// `#` **不是**注释只有一种情形：它在字符串里。而 TOML 的字符串恰好**四种** ——
/// 基本串 `"…"` · 字面串 `'…'`（这两种**规范上不能跨行**）·
/// 多行基本串 `"""…"""` · 多行字面串 `'''…'''`（这两种**能**跨行）。
/// 本剥法**逐行**走、引号态逐行重置 ⇒ 对前两种**完备**，对后两种**不认**。
/// ⇒ 一看见多行定界符、或看见**行尾引号还没闭合**，就把理由回报出去，
/// 由调用方**判红**。**没有「它猜错了还接着扫」的第三条路。**
///
/// 它挡的那一形：`target = { x = { ar = """` 换行 `# """, runner = "…" } }` ——
/// 续行第一个字符是 `#`，逐行重置的引号态会把**整行**当注释切掉，
/// 真键在被扫之前就没了。而那个键是**明文、零反斜杠** ⇒ [`decode_toml_escapes`] 与
/// [`backslash_in_key_position`] **两层都不触发**。⇒ 绕过要按**维**想（读文件 → 预处理 →
/// 匹配 → 判定），不是按例补：「键怎么拼」「文件怎么来」之外，**预处理**也是一维。
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

/// 把 TOML 基本串里**能拼出字母**的那几种转义解开。
///
/// TOML 的键可以是**带转义的基本串**，而 cargo 认的是**解码后**的键名 ——
/// `"runner"` 在 cargo 眼里就是 `runner`。不解码，那份配置能让 `cargo test` 把测试二进制
/// 交给 `/bin/echo`、**判据根本不被执行**，而本条照绿。三个键同形，`linker`/`rustflags` 一样。
///
/// **哪几种转义**（cargo 1.96.1 stable 上逐个实测，`[alias]` 探针 + 非空对照）：
/// `\uXXXX` · `\UXXXXXXXX` · **`\xXX`** 三种 cargo 都认（`\x` 是 TOML 1.1 的）。表头里也认
/// （`["alias"]` 实测生效）。剩下的转义（`\b \t \n \f \r \" \\`）**拼不出字母**，
/// 不进本函数。三引号多行串**不能当键**（放在键位 ⇒ cargo 报
/// `could not load Cargo configuration`；裸键与基本串键两种写法 alias 都真生效）。
///
/// ⚠ 但「不能当键」推不出「不是一条路」：三引号一出现在配置里，走的是
/// [`strip_toml_comments`] 那一维（把真键整行当注释切掉，cargo 照样认那个键）。
/// 挡住它的是那里的 fail-closed，**不是本函数**。
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

/// **键位置**出现反斜杠没有（转义解码的兜底）。
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
/// 内联表里的嵌套键**都成立 —— ⚠ 内联表那一格是量出来要的：实测
/// `alias = { "kg2probe" = "--version" }` **cargo 认**（内联表 + 转义键都认），
/// 只看「第一个 `=` 之前」会漏掉它。
///
/// ⚠ **代价**：**多行值的续行**里有反斜杠、且那一行没有 `=`，会误红。
/// 最常见的是**多行数组** —— `rustflags = [` 之后那几行正是它。三形：
/// `rustflags = [` 多行数组 ⇒ **红，点名 `rustflags`** —— 那不是误红，它真的设了；
/// `x = [` 多行数组、续行里有反斜杠、**不含**那三个键 ⇒ **红，理由是「键位置有反斜杠」**
/// —— **这一条是真误红**；同样的数组、续行里没有反斜杠 ⇒ **绿**。
/// ⇒ **判它可接受**：方向是 fail-closed，报文点名了是哪一条规则、哪一格，人一眼能消；
/// 而把它修窄（例如跟踪方括号深度、把数组续行排除）是**放宽**一条安全判据 ——
/// 没有维度级的论证就不放宽。
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
/// 理由：解码把 `\` 这个**非词字符**换成**词字符**，于是它**两个方向都会动整词边界** ——
/// 既能吃掉原文里成立的那一条，也能拼出原文里没有的整词：
/// · **原文那一遍不能省**：`"runner\x41" = "x"` —— 原文里 `runner` 后面是 `\`（非词）⇒ 整词成立；
///   解完是 `runnerA`，后面是 `A`（词）⇒ 整词不成立。探针㉑ 盯着这一遍。
/// · **解码那一遍不能省**：`"\x72unner" = "x"` —— 原文里根本没有 `runner`，解完才有。探针③ 盯着这一遍。
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
/// ★★ **它为什么是一个具名函数**：为了让探针能喂它一份夹具根（生产那一遍喂的是真仓根）。
/// 生产那一遍 **6 格全缺席** ⇒ 下面 `!p.is_file()` 那道守卫一律提前 `return None`，
/// **后面整段根本不执行**；直接调原语的探针（[`keys_in`] / [`backslash_in_key_position`] /
/// [`strip_toml_comments`]）又不经过这里 ⇒ 不喂夹具，「把原语串起来」这一层没有常驻覆盖。
/// 探针 ⑩–⑮ 逐支盯着这里的 6 支。
///
/// ⚠ **没盯着的一格**：「主判据体真的调了本函数」这件事本身没有独立探针。
/// 挡着它的是 `!p.is_file()` 那一支 —— 拆掉它，本函数会给 **6 格各回一条**「存在但读不出文本」，
/// 主 `assert!` 当场红。⇒ 这条链路是通的，但那是**顺带**，而且**有前提**：它靠「那 6 格
/// 一份都不存在」。真有人往那 6 格里放一份**读得出**的配置，那一格的机制就变了。
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
            //   看不懂却接着扫 = 拿一份被切过的文本当真相（多行串把真键整行切掉那一形）。
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
/// ⚠ 目录名与那一格的相对路径都取**中性名**（夹具的名字不许成为断言的
/// 承重词）——下面每一条断言认的都是**判据自己报文里的词**，不是这里的路径。
fn probe_slot_verdict(tag: &str, body: &[u8]) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("ccm-capreg-slot-{tag}-{}", std::process::id()));
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

/// ★★ **构建／安装期的执行面**。
///
/// 上面几条钉的是**运行时**谁能扩大执行面（webview 权限 · CSP · 全局 Tauri）。
/// 本条钉**更早的那一段**：从 `npm install` 到 `cargo build` 到 `tauri build`，
/// 有哪些地方能让代码在开发者机器上跑起来：
///
/// | 面 | 今天 | 谁在管 |
/// |---|---|---|
/// | `src/frontend/shell/build.rs` | 往 `OUT_DIR` 写 | `src/frontend/clippy.toml` 写盘那一组（clippy 也查构建脚本，那一处就地放行） |
/// | `tauri.conf.json` 的 `before*Command` | `npm run dev` / `npm run build` | 本条 |
/// | `package.json` 的 npm **生命周期钩子** | 一个都没有 | 本条 |
/// | cargo 配置里的 `runner`/`linker`/`rustflags` | 六格里一份都没设过 | 本条（判**内容**） |
///
/// ★ 后两行钉的是**「今天没有」这件事**。它们的危险恰恰在于「加一条就自动执行」：
/// `postinstall` 在**每一次 `npm install`** 上跑（含 CI、含任何人 clone 之后第一件事）；
/// `[target.*.runner]` 会让 **`cargo test` 去执行任意二进制**。
/// 「今天没有」不是判据 —— 没人钉的话，加进来的那天没有任何信号。
///
/// # ③ cargo 配置：**判内容，不判文件在不在**
///
/// 只有真的设了 `runner`/`linker`/`rustflags`
/// （**含用 TOML 转义拼出来的写法**，见 [`decode_toml_escapes`]）才红 ——
/// 剥掉注释、解开转义之后**都不含这几个整词**的配置一律放行。一份只有 `[profile.dev]`
/// （`debug` / `split-debuginfo` 那类构建瘦身）的配置**执行不了任何东西**：只问「文件在不在」
/// 会把用户本机那份瘦身配置判成恒红，而守的执行面一寸也没被碰。人群是下面那 6 格。
/// ⚠ **不许**改成「豁免某个文件名 / 某个路径」—— 那是放宽人群，不是收窄性质。
///
/// ★★ **一般教训**（写给下一个收窄守卫的人）：
/// **把一条守卫从「看一个结构性事实」收窄成「看一段文本里有没有那几个词」，
/// 就同时买进了那个文本格式的整个转义面。**
/// 前者粗，但**伪造不了**；后者准，但继承了格式的所有写法花样。
/// ⇒ 收窄之前先问：新落点是**事实**还是**文本模式**？是后者就得把那个格式的
/// 写法花样量一遍，并把量过的与没量的分开写下来。
///
/// ## 那 6 格是怎么来的：**3 个发起面 × cargo 认的 2 种文件名**
///
/// cargo 读哪一份 `.cargo/config`，由**发起命令的 cwd** 决定，并**沿 cwd 往上找**。
/// 用一次性探针 crate 实测（`[build] target-dir` 观察落点，带非空对照与反向对照）：
/// 配置放在 cwd 那一级**生效**，哪怕 manifest 在子目录（`--manifest-path sub/…`）；
/// 同一份配置放进那个子目录、cwd 留在上面 ⇒ **不生效**（落回默认 `sub/target`）；
/// 把 cwd 移进子目录 ⇒ **又生效**。⇒ 人群 = **本仓自己会把 cwd 落在哪几个目录**。
///
/// **尺子**：`git ls-files` 里的 `*.sh` / `*.ps1` / `*.yml` / `package.json`
/// 全扫一遍 `cargo <子命令>`，**逐处读它的 cwd**。出现过的 cwd 只有 [`CARGO_CFG_DIRS`] 那 3 个
/// （仓根 · `src/frontend/shell` · `src/backend`）。
///
/// 指路一律用**内容锚点**（yml 的 job 名 · 测试名 · 变量名），不写行号：上方一加注释，行号就馊了；
/// `ci.yml` / `release.yml` 那几处逐字都是同一句 `working-directory: <目录>`，能区分它们的是 job 名。
///
/// ⚠ **尺子没覆盖到的**（写下来免得把它读成穷举）：① `cargo tauri build` 那种**由工具
/// 再去起 cargo** 的，cwd 由 tauri CLI 定，本条没现打；② 人手临时 `cd` 到任意目录敲的
/// cargo —— 那个分母没人数得出，也不是一条判据守得住的。
///
/// **为什么是 6 不是 7**：往下没有第 4 个发起面 —— `src/common/*` 与
/// `src/frontend/shell/vendor/*` 里放一份配置，**上面那张表里的命令一条都读不到它**
/// （分母就是那张表 = 上面那把尺子量出来的全部；那些命令的 cwd 都停在 `src/frontend/shell/`，
/// 靠 `-p`/`--workspace` 选包，而 cargo 不往下找）。
/// 往上也没有 —— `~/.cargo/config.toml` / `$CARGO_HOME` / `cargo --config` 命令行
/// 都在仓外，**一次 commit 改不到**，不在本条的人群里（那是另一类威胁模型）。
/// 文件名两种：`config.toml` 与**无扩展名** `config`。后者 cargo 照读 —— 实测
/// 报文逐字 `deprecated in favor of config.toml`，且 alias **真生效**；
/// 改名成 `config.toml` 行为相同；两个都删 ⇒ `no such command`。
///
/// ⚠ 加一个发起面（新 crate、某个 job 换 cwd）就回这张表补一格。
/// 叫你回来的是下面那条按字面量逐格钉人群的断言（加 / 删 / 换一格都红）。
///
/// ## ③ 这一格有**四维**，改它之前先看这张表
///
/// 本条从输入到判定是四步，**每一步都是一个可以被单独绕过的维**：
///
/// | 维 | 这一步做什么 | 绕过它的那一形 | 靠什么守 |
/// |---|---|---|---|
/// | ① **读文件** | 6 格 = 3 发起面 × 2 文件名 | `include` 把 6 格之外的文件拉进来；换一格而格数不变 | 人群按字面量逐格钉住（探针⑨）+ `CARGO_BLINDING_KEYS` 判红 |
/// | ② **预处理** | 剥注释 → 解转义 | 多行串让剥注释把真键**整行切掉** | [`strip_toml_comments`] **fail-closed**：看不懂就判红（探针⑦ · ⑦e） |
/// | ③ **匹配** | 整词扫那三个键 | 转义拼键 | [`decode_toml_escapes`] 解码 + [`backslash_in_key_position`] 兜底 |
/// | ④ **判定** | 收集 offender → `assert!` | 装配层某一支恒假，而判据照绿 | 常驻探针 ⑩–⑮（[`offenders_under`] 一支一条） |
///
/// 🔴 **改本条时的纪律**：绕过**按维想，不按例补** —— 在同一维上补十个例，
/// 计数上像很彻底，覆盖上是一个点。
///
/// ### 哪几支有探针盯着
///
/// **一支** = 一个「能一处改成恒定答案、而语法与类型契约都不变」的最小语法位置；
/// `||`/`&&` 的每个操作数各算一支；不算循环边界 · 通配臂 · 纯计算 · 判据自己的 `assert!`
/// 与那几组常驻探针 · `BEFORE`/`LIFECYCLE` 那两块人群。
///
/// **配探针的判准**：这一支拆掉会产生**已量到的实害形状**（**漏红**：真 `runner` 不再被点名；
/// **误红**：一份纯瘦身配置被点名），**且**它守的是这几个函数 docstring **自己声明的契约**，
/// 而不是 fail-closed 那个**近似的具体形状**。⇒ 换一种剥法只要仍满足契约，那几条探针照样绿。
///
/// | 支 | 配了没有 | 谁挡住它 · 或者为什么不配 |
/// |---|---|---|
/// | [`strip_toml_comments`] 的 `if escaped` 与 `c == '\\'` | ✅ | 探针⑯（基本串里的 `\"` 不结束该串 ⇒ 后面那个 `#` 不是注释）。⚠ **这两支在函数外面行为上分不开**，任拆一支读数逐格相同 ⇒ **一条探针挡两支** |
/// | [`strip_toml_comments`] 的 `quote == Some('"')` | ✅ | 探针⑰（字面串里没有转义 —— `'C:\tools\'` 那种 Windows 路径不许被读成「引号没闭合」）。方向是**误红** |
/// | [`strip_toml_comments`] 的 `c == '\''` | ✅ | 探针⑱（字面串里的 `#` 不是注释）。方向是**漏红**，与多行串切键同一维 |
/// | [`strip_toml_comments`] 的 `line.contains("'''")` | ❌ **不配** | 它守的是 **fail-closed 那个近似本身**（「看见 `'''` 就判红」），不是契约 ⇒ 配了就把「换一种剥法（真去认多行串）」变成一次判据大修。而**实害形状已被另一支挡住**：一个 `'''` 开头的续行样本会让行尾引号态不闭合 ⇒ 「行尾引号未闭合」那一支（探针⑦）先开火；这一支单独失守时**只剩「同一行内定界符成对」那一形，而那一形什么都没藏住**。⚠ 这条理由对已配的 `"""`（探针⑦e）同样成立 |
/// | [`decode_toml_escapes`] 的 `\U` 臂 | ✅ | 探针⑲（`\u` 是③、`\x` 是④） |
/// | [`word_in`] 的 `c.is_alphanumeric()` · `c == '_'` · 整词边界的**前**半 | ✅ | 探针⑳（两个样本：`myrunner` · `my_runner`）。⚠ **「前半」那一支分不开**：它是「前面那一侧」的总闸，任何前半样本都同时挡住它与所用的那一类词字符；两个样本各自把「字母数字」与「下划线」**单独**挡住 |
/// | [`backslash_in_key_position`] 的 `b'{'` 候选 | ❌ **不配** | **构造不出实害输入**：那一支的作用是「在 `{` 处重置键位段」，而它与前一个重置点（`=` / `,`）之间**在 TOML 里只可能隔着空白**（`{` 只出现在 `=` 之后）⇒ 拆掉它，键位段最多多含几个空格，答案不变。⚠ **分母 = 试过的那几形**（顶层键 · 点分键 · 表头 · 内联表 · 嵌套内联表 · 多行数组续行），**不是穷举**；要配它只能拿一个不合法的 TOML 当样本，那就把实现细节钉死了 |
/// | [`offenders_under`] 的 6 支 | ✅ | 探针⑩–⑮，**一支一条** |
/// | [`keys_in`] 的 `word_in(text, key)`（原文那一遍） | ✅ | 探针㉑ |
///
/// 上表那两行 ❌ 是诚实边界：逐支拆掉，判据与整道 cargo 门照绿；不配的理由逐支写在表里。
///
/// ## ③b **不守什么**
///
/// ⚠ 下面是**量过的那几条**，不是「所有绕法」的穷举 —— 那个分母没人给得出。
/// 量法一律是：一次性 crate + `[alias]` 探针，带非空对照与反向对照，
/// cargo **1.96.1 stable**（换版本要重量）。
///
/// 🔴 **「够不着」的理由是「不在那 6 条路径上」，不是「不是仓里的文件」**：
/// 本条的人群是 6 条特定**路径**（`CARGO_CFG_DIRS` 的 **3** 个目录 × `CARGO_CFG_NAMES` 的
/// **2** 个文件名）。一份**就在仓里、就在仓根、一次 commit 就落地**的文件照样够不着本条，
/// 只要它**不落在那 6 条路径**之一上（下面 `rust-toolchain.toml` 那一行就是）。
/// ⚠ **例外**：那 6 格里的 `include` **能把格外的文件拉进来**，而那一形本条逮的是
/// **那句 `include` 本身**，**不是**去读被拉进来的那份文件（所以它的理由是「看不见」）：
/// 被拉进来的那份里写 `rustflags`，报文只说 `设了 ["include"] —— 本条看不进它拉进来的文件`。
/// ★ 「不守什么」这一栏里写一条**会骗人的理由，比不写还坏** —— 不写只是留白，
/// 写错会让下一个人**不再去想这一格**。
///
/// | 不守的东西 | 为什么 |
/// |---|---|
/// | 同族的执行面键：`rustc-wrapper` · `rustdocflags` · `build.rustc` | 射程只有那三个 |
/// | 🔴 **`rust-toolchain.toml` 的 `[toolchain] path`** —— **仓内文件**，仓根一份就够 | 见下面那一段；另一条人群，**这里不扩射程** |
/// | **环境变量那条配置源**：`CARGO_TARGET_<TRIPLE>_RUNNER` · `RUSTFLAGS` · `CARGO_BUILD_RUSTFLAGS` | cargo 认它们，但**它们不在上面那 6 个路径里** ⇒ 够不着。⚠ CI 的 yml 能设环境变量，那是另一个面，本条不声称守它 |
/// | `cargo --config <k>=<v>` 命令行 · 仓外的 `$CARGO_HOME/config.toml` | 一次 commit 改不到 ⇒ 不在人群里 |
/// | **值**里的写法花样 | 本条对**值**只是整词顺带命中（`[alias]` 里塞 `--config …rustflags=…` 会被逮到）。那是副产品，不是判据 |
/// | **`[env]` 那一族** | cargo 的 `[env]` 能给构建期的进程设环境变量（`RUSTFLAGS` 那一类正好是环境变量读的）—— 射程外，而它是那一族里最狠的一条 |
/// | 多行字符串 `"""` / `'''` | 它不能当键，但它**是**一条路，走的是**剥注释**那一维。由 [`strip_toml_comments`] 的 fail-closed 挡着：看见定界符或行尾引号未闭合 ⇒ 判红。**不是「不守」，是「守法换了一维」** |
///
/// ### 🔴 `rust-toolchain.toml` —— 一个仓内文件，换掉 `cargo` 和 `rustc` **本身**
///
/// 量法：一次性 crate + 透明代理假工具链（`bin/{cargo,rustc,rustdoc}` 各是「记一笔日志 →
/// `exec` 真的那一个」）；`rustup 1.29.0`，`cargo` 是 rustup 的 shim。
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
/// **仓里没有判据守它**：`.github/workflows/{ci,release}.yml` 里的
/// `uses: dtolnay/rust-toolchain@stable` 是**装工具链的 action，不是判据**。
///
/// ⚠ **别把它和上面那张「四维」表混起来**：那张表切的是**本条内部的数据流**
/// （读文件 → 预处理 → 匹配 → 判定），全在**同一个人群内部**（「判据看不见那个 `runner`」）。
/// `rust-toolchain.toml` **换掉的是配置源本身** —— 它根本不经过那 6 个格位，
/// 连第 ① 步都进不去。⇒ 它不是「再补一个例」，是**另一条人群**，该由另一条判据立；
/// 这里**不扩射程**。
///
/// ⚠ 上面的读数跑在一次性 crate 上；门禁那 8 个包上没量过（要让整个 workspace 换一条工具链重编），
/// 只有**机制**：假工具链是 `exec` 的透明代理 ⇒ 进程映像被替换。**这是论证，不是读数。**
///
/// ⚠⚠ **一条本条自己盖不住的：`runner` 让本条不被执行**
///
/// 量过的 `runner` 形（表下逐形；**不是穷举** ——「哪一类 `runner` 会让本条不被执行」的完整刻画给不出）。
/// 承重词是「**本条被没被执行**」，不是「透明不透明」：形 1 与形 2s 都让本条不被执行，
/// 而它们分处「透明」的两侧。
///
/// | 形 | 谁兜得住 |
/// |---|---|
/// | **0 · 基线**：6 格全缺席 | —— 门禁绿 |
/// | **1 · 不透明 `runner`**（不真跑测试，如 `/bin/echo`） | ✅ **门禁兜得住** —— 一条 `test result: ok` 都产不出 ⇒ 包数自检当场红 |
/// | **2 · 透明代理 `runner`**（起真二进制、原样透出） | ✅ **本条自己兜得住** —— 透明代理让本条**真的跑起来**，而它看见 `runner` 就红 |
/// | 🔴 **2s · 透明 skip-runner**：`exec "$@" --skip <本条名>` —— **真跑测试，只把本条过滤掉** | 🔴 **没人兜得住** —— 判据**根本没跑**（`1 filtered out`），门禁只数包数与「非 0」，**对合计的具体值没有断言** |
/// | **3 · 透明代理 + 判据看不见那个 `runner`** | 🔴 **没人兜得住** —— 「净变宽」那一族 |
///
/// 🔴 **形 2s 是实害**：它**只要落一份配置文件，而那份文件就落在被守的 6 格里** ——
/// 判据只要跑就逮得到它（报文点名 `设了执行面的键 ["runner"]`），而它让判据不跑；
/// 门禁那一侧 `run_gate_sum` 只断言包数与「合计非 0」，合计少一条一声不吭。
/// 形 3 要**同时**改 `CARGO_CFG_DIRS` 与探针⑨ 的 `want_slots` 才做得出（探针⑨ 按字面量逐格钉住人群），
/// 比形 2s 贵；另一条更贵的路是上面「不守什么」栏的 `rust-toolchain.toml`。
///
/// 形 1 那一格：cargo 自己**成功退出**（`/bin/echo` 退出码是 0）⇒ `run_gate_sum` 的退出码那一支
/// 在这一形上失效，先开火的是包数那一支；包数那一支松掉或删掉，「合计为 0 / 找不到」那一支照样接住。
/// 这几支住 `tests/scripts/gate.sh`，**不在本条的射程内**，也不算本条的覆盖。
///
/// 本条能逮到形 2 / 形 3 的，还有「那份配置**还没生效**就被看见」的时机
/// （配置在别的发起面 · 别人 clone 之后第一次跑 · 审 diff 的人）。
#[test]
fn the_build_time_execution_surface_stays_registered() {
    // ① `tauri.conf.json` 的构建前置命令：登记值 + 理由。
    const BEFORE: &[(&str, &str, &str)] = &[
        (
            "beforeDevCommand",
            "npm run dev",
            "起前端 dev server；`tauri dev` 会执行它",
        ),
        (
            "beforeBuildCommand",
            "npm run build",
            "打包前构建前端产物；`tauri build` 会执行它 —— 改这里等于改「发版时在构建机上跑什么」",
        ),
    ];
    let v: serde_json::Value = serde_json::from_str(&capability_json_sibling("tauri.conf.json"))
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
        &std::fs::read_to_string(guard_core::addr::repo_root().join("package.json"))
            .expect("读不到 package.json"),
    )
    .expect("package.json 不是合法 JSON");
    let scripts = pkg
        .get("scripts")
        .and_then(|s| s.as_object())
        .expect("`package.json` 里没有 `scripts` —— 形状变了，本条会零命中地绿");
    // 抽取器自检：脚本表塌了的话下面那条就是废话（正控：生成类型那一条必在）。
    assert!(
        scripts.contains_key("gen:types"),
        "`package.json` 的 scripts 里没解析出 `gen:types`（解析出 {} 条）—— 读法坏了",
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
    let root = guard_core::addr::repo_root().to_path_buf();
    let mut slots: Vec<String> = Vec::new();
    for dir in CARGO_CFG_DIRS {
        for name in CARGO_CFG_NAMES {
            slots.push(format!("{dir}/.cargo/{name}"));
        }
    }
    // 人群按**字面量逐格**钉住（换一格而格数不变也要红）。
    let want_slots: &[&str] = &[
        "./.cargo/config.toml",
        "./.cargo/config",
        "src/frontend/shell/.cargo/config.toml",
        "src/frontend/shell/.cargo/config",
        "src/backend/.cargo/config.toml",
        "src/backend/.cargo/config",
    ];
    assert_eq!(
        slots.iter().map(String::as_str).collect::<Vec<_>>(),
        want_slots,
        "本条的人群**逐格**变了。\n\
             真要改（新发起面 / cargo 换了认的文件名）：连本条头注那张\n\
             「谁从这里发起 cargo」的表一起改，并把新的尺子读数写进去。"
    );

    // 抽取器自检：探针坏了的话，下面整条就是**零命中地绿**。
    // ⚠ 探针③ 盯的是转义拼键那一形 —— 它一旦变绿，本条就又瞎了。
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
        "探针②：只有 `[profile.*]` 的构建瘦身配置必须放行 —— 本条判内容，不判文件在不在"
    );
    assert_eq!(
        keys_in(&stripped_ok(probe_escaped), CARGO_EXEC_KEYS),
        vec!["runner"],
        "探针③：**用 TOML 转义拼出来的键**必须被逮到。\n\
             cargo 认解码后的键名；只扫原文的话，`cargo test` 会被交给 `/bin/echo`，本条却是绿的。"
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
    // ⑤f：兜底的**第二支** ——「整行没有 `=`」那一支。
    // ⚠ ⑤b/⑤d 两个样本都有 `=`，走的是「每个 `=` 往前回溯」那一支：
    //   把 `!line.contains('=') && line.contains('\\')` 拆掉，上面每一条探针照绿。
    //   这个样本整份没有 `=`，**只**由那一支挡住。
    // ⚠ 它也不会被 `keys_in` 顺带逮到：解开是 `[build]`，不在那三个执行键里。
    let probe_header_escape = "[bui\\u006Cd]\n";
    assert!(
        keys_in(&stripped_ok(probe_header_escape), CARGO_EXEC_KEYS).is_empty(),
        "探针⑤f 前半：这个样本必须**不**被整词扫命中，否则它证不了兜底那一支"
    );
    assert!(
        backslash_in_key_position(&stripped_ok(probe_header_escape)),
        "探针⑤f：**没有 `=` 的行**（表头 / 多行值的续行）里的反斜杠必须被兜底逮到。\n\
             实测 cargo 认表头里的转义（`[\"ali\\u0061s\"]` 生效）⇒ 转义面不止叶子键。"
    );
    // ⑧：**整词边界**这一维。
    // ⚠ 把 [`word_in`] 退化成 `text.contains(key)`，上面每一条探针照绿；
    //   而同一份变异下，`[profile.dev.package."linker-utils"]` 这份**纯瘦身**配置
    //   会被点名 `设了执行面的键 ["linker"]`，用户的仓库恒红。
    let probe_word_boundary = "[profile.dev.package.\"linker-utils\"]\ndebug = false\n";
    assert!(
        keys_in(&stripped_ok(probe_word_boundary), CARGO_EXEC_KEYS).is_empty(),
        "探针⑧：`linker-utils` 这类**包含**那三个词、却不是那个键的写法必须放行 ——\n\
             本条判的是整词，不是子串。这一条一旦变红，纯瘦身配置就又会被判红。"
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

    // ⑦ **预处理这一维**的常驻探针。三个样本的真键都是**明文 `runner`**，
    //    零反斜杠 ⇒ 上面那两层（解码 · 键位反斜杠）**一层都不会被触发**，
    //    唯一挡住它们的就是「剥注释看不懂 ⇒ 判红」。⚠ 这三条一旦变绿，本条就又瞎了。
    let probe_ml_hash = "target = { x = { ar = \"\"\"\n# \"\"\", runner = \"/bin/echo\" } }\n";
    let probe_ml_literal = "target = { x = { ar = '''\n# ''', runner = \"/bin/echo\" } }\n";
    let probe_ml_midline = "a = \"\"\"\nzz # \"\"\"\nrunner = \"/bin/echo\"\n";
    // ⚠ ⑦d 是「行尾引号未闭合」那一支的专属样本：上面三个样本同时被「多行定界符」那一支挡着，
    //    只拆这一支它们照绿。这个样本里没有三引号，**只有**未闭合引号这一个信号。
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
                 这一形：续行那个 `#` 把明文的 `runner` 整行切掉，\n\
                 而它零转义零反斜杠 ⇒ 解码与键位兜底两层都不会被触发。\n\
                 剥完的文本是 {text:?} —— 真键已经不在里面了。"
        );
    }
    // ⑦e：剥注释的**第一支** ——「多行字符串定界符」。
    // ⚠ 上面四个样本里，前三个同时触发两支信号（定界符 · 行尾引号未闭合），⑦d 只触发第二支
    //   ⇒ 只拆定界符那一支，上面四条探针照绿。这个样本的三引号在同一行内闭合 ⇒ 行尾引号态是闭合的，**只**触发定界符那一支。
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
    // ⑩–⑮ **装配层那 6 支**（[`offenders_under`]）
    //
    // ★ 上面 ①–⑨ **全都直接调那几个原语**，不经过 [`offenders_under`]；而生产那一遍
    //   6 格全缺席 ⇒ `!p.is_file()` 一律提前 `return None` ⇒ 后面整段不执行。
    //   本组喂它夹具根，逐支盯着那 6 支。
    //
    // 🔴 **一支一条**：这 6 支每拆一支，只有它自己那一条开火。
    //   「只由这一支挡住」说的是**装配层这 6 支之内**：上游原语的信号
    //   （[`strip_toml_comments`] 的定界符那一支 · `CARGO_EXEC_KEYS` 那张表 · [`word_in`] 的整词边界 …）
    //   拆掉，下面几条同样会开火，因为样本要路过它们 —— 那是「上游坏了下游也报」。
    //
    // ⚠ `!p.is_file()` 那一支本组不配探针：拆掉它，主 `assert!` 逐字列出全部 6 格。
    //   它的边界写在 [`offenders_under`] 头注。

    // ⑩ `let Ok(raw) … else`：**存在但读不出文本** ⇒ 必须上报。
    //    ⚠ 样本是**非法 UTF-8 字节**：`is_file()` 对它恒真 ⇒ 拆掉 `!p.is_file()` 这条一声不吭；
    //      后面几支它够不着（在那之前就 `return` 了）⇒ 装配层这 6 支里只由它挡住。
    //      它连上游那些原语都够不着（样本不是合法文本，一层剥法都没跑到）。
    let why = probe_slot_verdict("a", &[0xff, 0xfe, 0x00, 0x80]);
    assert!(
        why.as_deref()
            .is_some_and(|w| w.contains("存在但读不出文本")),
        "探针⑩（`let Ok(raw) … else` 那道守卫；**装配层这 6 支里只由它挡住**）：\n\
             一格**存在、却读不出文本**时本条必须上报它 —— 那正是「本条看不了它的内容」。\n\
             改成 `return None` 就等于「读不出就当没事」。这一格实得 {why:?}。"
    );

    // ⑪ 预处理 fail-closed：样本**只**触发「剥注释看不懂」这一支 ——
    //    它不含那三个执行键、不含 `include`、键位零反斜杠 ⇒ 拆掉执行键 · 致盲键 · 键位反斜杠
    //    那三支它照样上报，拆掉这一支当场变绿。
    let why = probe_slot_verdict("b", b"a = \"\"\"x\"\"\"\n");
    assert!(
        why.as_deref().is_some_and(|w| w.contains("剥注释这一步")),
        "探针⑪（预处理 fail-closed；**装配层这 6 支里只由它挡住**）：\n\
             剥注释回报「看不懂」时，装配这一层必须把它变成一条 offender 理由。\n\
             这一支一旦恒假，多行串把真键整行切掉那一形就原样回来。实得 {why:?}。"
    );

    // ⑫ 执行面键：样本只设了 `runner`，别的信号一支不触发。
    let why = probe_slot_verdict("c", b"[target.x]\nrunner = \"sh\"\n");
    assert!(
        why.as_deref().is_some_and(|w| w.contains("设了执行面的键")),
        "探针⑫（`exec` 那一支；**装配层这 6 支里只由它挡住**）：\n\
             [`keys_in`] 逮到了执行面键，而装配这一层必须把它变成一条 offender 理由。\n\
             ⚠ 原语有牙不等于装配有牙：这一支恒假时 [`keys_in`] 照常返回 `[\"runner\"]`，\n\
             只是没人再看它一眼。实得 {why:?}。"
    );

    // ⑬ 致盲键：样本只设了 `include`，不含那三个执行键、键位零反斜杠。
    let why = probe_slot_verdict("d", b"include = [\"../elsewhere/x.toml\"]\n");
    assert!(
        why.as_deref()
            .is_some_and(|w| w.contains("本条看不进它拉进来的文件")),
        "探针⑬（`blind` 那一支；**装配层这 6 支里只由它挡住**）：\n\
             `include` 能把那 6 格之外的任意文件拉进来，而本条读不到那一份 ⇒ 按 fail-closed 判红。\n\
             这一支恒假 = 本条被蒙上眼睛还自称绿。实得 {why:?}。"
    );

    // ⑭ 键位反斜杠：样本整行没有 `=`、解开是 `[build]`（不在那三个执行键里）
    //    ⇒ 预处理 · 执行键 · 致盲键那三支一支都不触发。
    let why = probe_slot_verdict("e", b"[bui\\u006Cd]\n");
    assert!(
        why.as_deref().is_some_and(|w| w.contains("键位置有反斜杠")),
        "探针⑭（键位反斜杠那一支；**装配层这 6 支里只由它挡住**）：\n\
             [`backslash_in_key_position`] 是「不靠数全转义种类」的那一半兜底，\n\
             而装配这一层必须把它的 `true` 变成一条 offender 理由。实得 {why:?}。"
    );

    // ⑮ `why.is_empty()`：**干净的一格必须判 `None`**。
    //    ⚠ 这一支有**两张脸**，本条买的是其中一张，另一张说清在哪：
    //      · `if false`（恒当成 offender）⇒ **只**由本条挡住（下面这条断言当场红）；
    //      · `if true`（永不上报）⇒ 由 ⑪⑫⑬⑭ **四条一起**挡住 —— 它是那四条的
    //        **下游**，没有任何探针能「只」挡住它。**这不是探针的毛病，是这段代码的形状。**
    let why = probe_slot_verdict("f", b"[profile.dev]\ndebug = false\n");
    assert!(
        why.is_none(),
        "探针⑮（`why.is_empty()` 那道分岔的 `if false` 那张脸；**装配层这 6 支里只由它挡住**）：\n\
             一条理由都没有的一格**必须**判 `None` —— 恒当成 offender 会让用户的仓库恒红，\n\
             而误红最省事的消法是把判据放宽，那条路不许走。实得 {why:?}。"
    );

    // ══════════════════════════════════════════════════════════════════════
    // ⑯–㉑ 原语那几支里**配了探针的**
    //
    // 配探针的判准与没配的两支（`'''` 定界符 · `b'{'` 候选）写在本条头注「哪几支有探针盯着」。
    // ⇒ 换一种剥法（比如真去认多行串）只要仍满足契约，下面这几条照样绿。

    // ⑯ 剥注释契约的一半：**基本串里被转义的引号不结束这个串**。
    //    docstring 逐字：「`#` 到行尾；**字符串里的 `#` 不算注释**」。
    //    ⚠ `if escaped` 与 `c == '\\'` 两支**在函数外面行为上分不开**：
    //      任拆一支，`\"` 都会当场把串关掉 ⇒ 同一条探针挡住两支；
    //      要分开得去断言内部状态，那就把实现细节钉死了。
    //    ⚠ 这条也会被 `quote == Some(c)` 拆掉时打红，那一支另有探针⑦。
    let sample_escaped_quote = "target = { x = \"a\\\"#b\", runner = \"sh\" }\n";
    assert_eq!(
        keys_in(&stripped_ok(sample_escaped_quote), CARGO_EXEC_KEYS),
        vec!["runner"],
        "探针⑯：基本串里的 `\\\"` **不结束这个串** ⇒ 它后面那个 `#` 不是注释，\n\
             真键 `runner` 必须活着走出剥注释这一步。\n\
             这一支一恒假，`#` 之后整行被切掉，而剥注释**一声不吭**（引号态在 break 时是闭合的）\n\
             ⇒ 那是一次**静默漏红**，与多行串切键那一形同在预处理这一维。"
    );

    // ⑰ 剥注释契约的另一半：**字面串 `'…'` 里没有转义**。
    //    ⚠ 方向是**误红**：这一支一旦恒真，一条以反斜杠结尾的 Windows 路径
    //      （`'C:\tools\'` —— TOML 字面串最教科书的写法）会被读成「引号没闭合」
    //      ⇒ fail-closed 判红 ⇒ 用户的仓库恒红。
    let (_, why_literal_path) = strip_toml_comments("rustc = 'C:\\tools\\'\n");
    assert!(
        why_literal_path.is_empty(),
        "探针⑰：TOML **字面串**里反斜杠就是反斜杠（没有转义）—— \
             `'C:\\tools\\'` 这一行本剥法必须认得，不许回报「看不懂」。\n\
             实得 {why_literal_path:?}。⚠ 这一条护的是**误红**那一侧：误红最省事的消法\
             是把判据放宽，那条路不许走。"
    );

    // ⑱ 剥注释契约的第三条：**字面串里的 `#` 不算注释**。
    //    ⚠ 拆掉这一支，`'` 不再开串 ⇒ 串里那个 `#` 把整行切掉，而引号态是闭合的
    //      ⇒ **一声不吭地漏红**。⚠ 这条也会被 `quote == Some(c)` 拆掉时打红（那一支另有探针⑦）。
    let sample_hash_in_literal = "target = { x = 'a#b', runner = \"sh\" }\n";
    assert_eq!(
        keys_in(&stripped_ok(sample_hash_in_literal), CARGO_EXEC_KEYS),
        vec!["runner"],
        "探针⑱：**字面串**里的 `#` 不是注释 —— 真键 `runner` 必须活着走出剥注释这一步。\n\
             这一支恒假 ⇒ 整行从 `#` 处被切掉，而「看不懂」一个字都不报。"
    );

    // ⑲ 转义面的第三种：`\UXXXXXXXX`。
    //    [`decode_toml_escapes`] 认三种（`\u` · `\U` · `\x`），三种 cargo 都认（头注那段实测）；
    //    `\u` 是③、`\x` 是④、这一条盯 `\U`。
    let probe_upper_escape = "[target.x]\n\"\\U00000072unner\" = 1\n";
    assert_eq!(
        keys_in(&stripped_ok(probe_upper_escape), CARGO_EXEC_KEYS),
        vec!["runner"],
        "探针⑲：`\\UXXXXXXXX`（8 位十六进制）那种写法 cargo 也认，本条也要认。\n\
             与探针③ 同形：cargo 认**解码后**的键名，\n\
             而这一臂一旦答通配臂那个 0，`\"\\U00000072unner\"` 就再也解不成 `runner`。"
    );

    // ⑳ 整词边界的**前**半与词字符那两类（字母数字 · 下划线）。
    //    ⚠ 探针⑧ 盯的是**后**半（`linker-utils`）。
    //    ⚠ **分不开的一格**：「前面那一侧」的总闸 ⇒ 任何前半样本都同时挡住它与所用的那一类
    //      词字符；没有任何样本能「只」挡住总闸。两个样本各自把字母数字 / 下划线单独挡住
    //      （换一类词字符，另一类拆掉照绿）。
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
                 探针⑧ 只盯「后」那半（`linker-utils`）—— 前半一旦失守，\n\
                 纯瘦身配置就从另一侧又被判红，用户的仓库恒红。"
        );
    }

    // ㉑ [`keys_in`] 扫**原文**那一遍。
    //    ⚠ 解码那一遍由探针③ 挡着；原文那一遍只有这一条盯着。
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

    // ⚠ 这一段的 7 支信号住 [`offenders_under`]：探针 ⑩–⑮ 要能喂它一份夹具根，理由见那个函数的头注。
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

/// 一份 JSON 里**任意深度**、键名恰为 `capabilities` 的处数。
///
/// 为什么按键名全树找、而不只看 `app.security.capabilities` 那一个住址：
/// 那是 Tauri 2 今天的字段名（`tauri-utils` 的 `SecurityConfig::capabilities`，现打），
/// 上游挪了住址，只盯一个路径的判据会**零命中地绿**；本仓两份配置里这个键名今天零处，
/// 按键名全树找没有误报的代价。
fn capability_keys_anywhere(v: &serde_json::Value) -> usize {
    match v {
        serde_json::Value::Object(m) => m
            .iter()
            .map(|(k, x)| usize::from(k == "capabilities") + capability_keys_anywhere(x))
            .sum(),
        serde_json::Value::Array(xs) => xs.iter().map(capability_keys_anywhere).sum(),
        _ => 0,
    }
}

/// ★★ **Tauri 会加载的能力来源 == {`capabilities/default.json`}**〔`INVARIANTS §45`〕。
///
/// # 它补的洞
///
/// [`every_webview_permission_is_registered`] 只读 `capabilities/default.json` **一个文件**。
/// 而 Tauri 2 给 webview 发能力的来源不止那一份（现打 `tauri-build` 2.6 / `tauri-utils` 2.9 源码）：
///
/// | 来源 | 上游怎么读 | 本条怎么判 |
/// |---|---|---|
/// | `capabilities/` 目录 | `tauri_build::build()` 默认模式 `./capabilities/**/*` —— **目录下全部文件** | 目录里的文件集**两向等于** `{default.json}` |
/// | 配置里内联 | `app.security.capabilities`（标识符或整份内联对象都行） | 两份配置（主 ＋ sidecar 合并配置）里 `capabilities` 键**零处** |
/// | 平台配置 | `tauri.{linux,windows,macos,android,ios}.conf.json` 等会被合并进主配置 | 那几个文件名**零存在**（它们一出现，上一格就少看了一份） |
/// | 运行期 | `Manager::add_capability` | 生产源码零调用 |
///
/// 任一来源多出东西，webview 就拿到了**没过 `ALLOWED` 那张表**的能力 ——
/// 而 `ALLOWED` 那一行 `core:window:allow-hide` 的理由自己就写着「另开文件等于绕过它」。
/// ⇒ 本条把「绕过」这条路本身堵成会红的：**能力只有一个来源，那一个来源才有登记表。**
///
/// ⚠ 真要加第二份能力文件（比如给 settings 窗单独收窄）：不是在这里加豁免，
/// 是把 [`every_webview_permission_is_registered`] 改成读**全部**来源、逐份对 `ALLOWED`，再改本条的集合。
#[test]
fn tauri_loads_capabilities_from_exactly_one_source() {
    let shell = guard_core::addr::shell_root();

    // ① 目录：全部文件（空扩展名列表 = 不筛）两向等于 {default.json}。
    let files: std::collections::BTreeSet<String> =
        guard_core::scan_tree_excluding(&shell.join("capabilities"), &[], &[])
            .into_iter()
            .map(|(p, _)| {
                p.strip_prefix(shell.join("capabilities"))
                    .expect("扫出来的文件不在 capabilities/ 下")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
    let want: std::collections::BTreeSet<String> = ["default.json".to_string()].into();
    assert_eq!(
        files, want,
        "`src/frontend/shell/capabilities/` 里的文件集 ≠ {{default.json}}（两向）。\n\
         ⚠ `tauri_build::build()` 会加载那个目录下的**全部**文件 —— 多出来的每一份都在给 webview 发能力，\n\
         而 `every_webview_permission_is_registered` 只读 `default.json`，那几份**绕过了登记表**。"
    );

    // ② 配置里的内联能力：零处。先给计数器一个正控 —— 认得出一条合成的内联能力。
    let probe: serde_json::Value = serde_json::from_str(
        r#"{"app":{"security":{"csp":"x","capabilities":["main",{"identifier":"d","permissions":[]}]}}}"#,
    )
    .expect("正控夹具不是合法 JSON");
    assert_eq!(
        capability_keys_anywhere(&probe),
        1,
        "计数器认不出一条合成的 `app.security.capabilities` —— 下面的零命中在空转"
    );
    for name in ["tauri.conf.json", "tauri.sidecar.conf.json"] {
        let raw = capability_json_sibling(name);
        let v: serde_json::Value =
            serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{name} 不是合法 JSON：{e}"));
        let n = capability_keys_anywhere(&v);
        assert_eq!(
            n, 0,
            "`{name}` 里出现了 {n} 处 `capabilities` 键 —— 配置里内联的能力（`app.security.capabilities`）\n\
             同样会发给 webview，而它**不过 `ALLOWED` 那张表**。能力只许住 `capabilities/default.json`。"
        );
    }

    // ③ 平台配置：会被 Tauri 合并进主配置的那几个文件名，一个都不许在。
    //    正控：同一个探针认得出真在的那份主配置。
    assert!(
        shell.join("tauri.conf.json").is_file(),
        "探针认不出真在的 `tauri.conf.json` —— 下面的「零存在」在空转"
    );
    let mut merged: Vec<String> = Vec::new();
    for plat in ["linux", "windows", "macos", "android", "ios"] {
        for form in [
            format!("tauri.{plat}.conf.json"),
            format!("tauri.{plat}.conf.json5"),
            format!("Tauri.{plat}.toml"),
        ] {
            if shell.join(&form).exists() {
                merged.push(form);
            }
        }
    }
    for form in ["tauri.conf.json5", "Tauri.toml"] {
        if shell.join(form).exists() {
            merged.push(form.to_string());
        }
    }
    assert!(
        merged.is_empty(),
        "出现了会被 Tauri 合并进主配置的文件：{merged:?}\n\
         ⇒ 上面第 ② 格只看两份配置，这几份里的内联能力它看不见。要加就把它们一并纳入第 ② 格。"
    );

    // ④ 运行期：生产源码零 `add_capability`（针运行期拼，免得本文件的散文自己成为命中 ——
    //    虽然本文件住 `tests/`、结构上不在被扫的那棵树里）。
    let needle = format!("{}_{}", "add", "capability");
    let hits: Vec<String> = guard_core::scan_tree_excluding(&shell.join("src"), &["rs"], &[])
        .into_iter()
        .filter(|(_, src)| guard_core::production_code(src).contains(&needle))
        .map(|(p, _)| p.display().to_string())
        .collect();
    assert!(
        hits.is_empty(),
        "生产源码里出现了运行期加能力（`{needle}`）：{hits:?}\n\
         ⇒ 那是第四个来源，同样不过 `ALLOWED` 那张表。"
    );
}

/// ★ 界面里调到的窗口 / 网页视图写口，每一个都要在 `default.json` 的权限里。
/// `core:default` 只带读的那几样（`is*` · 尺寸 · 位置 · 标题读 · 监听）；写口（最小化 · 还原 · 拉到前面 · 关 · 藏 · 改标题 · 全屏 · 缩放）
/// 各要一格，缺了 ⇒ 调用被 ACL 静默拒绝、`.catch(() => {})` 吞掉 ⇒ 点了没反应（agent 窗口「滚到派出卡」那下
/// `unminimize()` / `setFocus()` 就是这样：窗口不还原也不到前面）。
///
/// 人群：界面与通信层的 `.ts`（`src/frontend/ui/**` · `src/comms/**`），接收者 = `getCurrentWindow()` /
/// `getCurrentWebviewWindow()` / `getCurrentWebview()` 本身，或 `const <名> = 上面三者()` 绑出来的名字；
/// 在接收者上的每一个 `.<方法>(` 查下表。表外的方法名 ⇒ 红（新写口先查它要哪格，再登记）。
#[test]
fn every_window_write_the_ui_calls_is_granted() {
    /// `(方法名, 要的权限；None = core:default 已含，读或监听)`。
    const METHODS: &[(&str, Option<&str>)] = &[
        ("minimize", Some("core:window:allow-minimize")),
        ("unminimize", Some("core:window:allow-unminimize")),
        ("setFocus", Some("core:window:allow-set-focus")),
        ("setTitle", Some("core:window:allow-set-title")),
        ("setFullscreen", Some("core:window:allow-set-fullscreen")),
        ("close", Some("core:window:allow-close")),
        ("hide", Some("core:window:allow-hide")),
        ("destroy", Some("core:window:allow-destroy")),
        ("setZoom", Some("core:webview:allow-set-webview-zoom")),
        ("isFullscreen", None),
        ("listen", None),
        ("once", None),
        ("onCloseRequested", None),
        ("onFocusChanged", None),
    ];
    const RECEIVERS: &[&str] = &[
        "getCurrentWindow()",
        "getCurrentWebviewWindow()",
        "getCurrentWebview()",
    ];
    fn ident(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_' || c == '$'
    }
    /// `rest` 以接收者之后的文本开头：跳过空白（含换行）找 `.方法(`。
    fn method_after(rest: &str) -> Option<&str> {
        let rest = rest.trim_start().strip_prefix('.')?.trim_start();
        let end = rest.find(|c: char| !ident(c)).unwrap_or(rest.len());
        let (name, tail) = rest.split_at(end);
        (!name.is_empty() && tail.trim_start().starts_with('(')).then_some(name)
    }
    let shell = guard_core::addr::shell_root();
    let mut calls: Vec<(String, String)> = Vec::new();
    for root in ["../ui", "../../comms"] {
        for (path, ts_text) in guard_core::scan_tree_excluding(&shell.join(root), &["ts"], &[]) {
            let file = path.display().to_string();
            let mut names: Vec<(String, &str)> = Vec::new();
            for r in RECEIVERS {
                for (at, _) in ts_text.match_indices(r) {
                    if let Some(m) = method_after(&ts_text[at + r.len()..]) {
                        calls.push((file.clone(), m.to_string()));
                    }
                    // `const <名> = getCurrentWindow();`
                    let head = ts_text[..at].trim_end();
                    if let Some(head) = head.strip_suffix('=') {
                        let head = head.trim_end();
                        let start = head.rfind(|c: char| !ident(c)).map_or(0, |i| i + 1);
                        let before = head[..start].trim_end();
                        let kw = &before[before.rfind(|c: char| !ident(c)).map_or(0, |i| i + 1)..];
                        if kw == "const" || kw == "let" {
                            // 名字只在绑定所在的块里算（同一文件里别处的 `w` 可能是别的东西）：到外层 `}` 为止。
                            let body = &ts_text[at..];
                            let mut depth = 0i32;
                            let end = body
                                .char_indices()
                                .find(|&(_, c)| {
                                    match c {
                                        '{' => depth += 1,
                                        '}' => depth -= 1,
                                        _ => {}
                                    }
                                    depth < 0
                                })
                                .map_or(body.len(), |(i, _)| i);
                            names.push((head[start..].to_string(), &ts_text[at..at + end]));
                        }
                    }
                }
            }
            for (n, scope) in names {
                for (at, _) in scope.match_indices(n.as_str()) {
                    let left_ok = scope[..at]
                        .chars()
                        .next_back()
                        .is_none_or(|c| !ident(c) && c != '.');
                    let after = &scope[at + n.len()..];
                    let right_ok = after.chars().next().is_none_or(|c| !ident(c));
                    if left_ok && right_ok {
                        if let Some(m) = method_after(after) {
                            calls.push((file.clone(), m.to_string()));
                        }
                    }
                }
            }
        }
    }
    assert!(
        calls.iter().any(|(_, m)| m == "minimize") && calls.iter().any(|(_, m)| m == "setZoom"),
        "最小化 / 缩放那几处一处没扫到 —— 扫描面坏了：{calls:?}"
    );
    let caps: serde_json::Value =
        serde_json::from_str(&capability_json()).expect("capabilities/default.json 不是合法 JSON");
    let granted: Vec<&str> = caps["permissions"]
        .as_array()
        .expect("找不到 permissions")
        .iter()
        .filter_map(|p| p.as_str().or_else(|| p["identifier"].as_str()))
        .collect();
    let mut unknown = Vec::new();
    let mut missing = Vec::new();
    for (file, m) in &calls {
        match METHODS.iter().find(|(name, _)| name == m) {
            None => unknown.push(format!("{file}: .{m}(")),
            Some((_, Some(need))) if !granted.contains(need) => {
                missing.push(format!("{file}: .{m}( 要 {need}"))
            }
            Some(_) => {}
        }
    }
    unknown.sort();
    unknown.dedup();
    missing.sort();
    missing.dedup();
    assert!(
        unknown.is_empty(),
        "界面在窗口上调了表外的方法：{unknown:#?}\n⇒ 查它走哪个 `plugin:window|…` / `plugin:webview|…` 命令、`core:default` 含不含，登记进 METHODS"
    );
    assert!(
        missing.is_empty(),
        "界面调到的窗口写口不在权限里（会被静默拒绝，点了没反应）：{missing:#?}"
    );
}
