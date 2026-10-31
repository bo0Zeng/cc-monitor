//! [`super`] 的判据 —— **窗口上的字画不画得出来**。
//!
//! 设计住（「字体（CJK）」那一行；此前引的 `§5.4g` 那一节从来不存在）；读数住。
//!
//! # 🔴 这一摞里哪一条是**反空真**的锚
//!
//! [`without_a_cjk_font_the_probe_is_almost_entirely_unrenderable`] ——
//! 它钉的是「**不装字体时画不出的字数恰好等于 238 / 237**」
//! （`24f` 第四刀之前那句写的是 121 / 120，第五刀之前是 194 / 193）。
//! 没有它，剩下那几条（「装上之后一个都不缺」）可以靠**量具永远说「不缺」**来全绿：
//! 量具翻向过一次（§三，拿 U+FFFD 当基准那一版），不是假想。
//!
//! # ⚠ 这一摞买不到什么
//!
//! 探针只有**我们自己写的标签**。文件名是任意的（日文 · 韩文 · 生僻字 · emoji），
//! 没有任何判据覆盖得了。**「探针全绿」≠「任何文件名都画得出来」。**

use super::{Attempt, FontState, PROBE};

/// 一个**字体已经就绪**的 `Context`。
///
/// ⚠ 必须先跑一帧 —— 之前碰 `fonts_mut` 会 panic（`fonts.rs §四`）。
/// `textures_delta` 必须清掉，否则 `TexturesDelta` 的 `Drop` 自己会 panic。
fn ready(ctx: &egui::Context) {
    let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
    out.textures_delta.clear();
}

fn prop() -> egui::FontId {
    egui::FontId::proportional(14.0)
}
fn mono() -> egui::FontId {
    egui::FontId::monospace(14.0)
}

/// 从一份 Rust 源码里取出**普通字符串字面量的内容**。
///
/// 🔴 为什么要一台状态机、不能用「按引号切」：`filewin/` 那几份文件的注释里
/// **满是中文**，漏进来就把判据变成恒真（注释里的字当然「用不着画」）。
/// ⇒ 跳行注释 · 跳块注释（可嵌套）· 跳裸字符串 · 认转义。
///
/// ⚠ 本函数自己由 [`the_literal_scanner_finds_exactly_what_it_should`] 钉着。
fn string_literals(src: &str) -> Vec<String> {
    let b: Vec<char> = src.chars().collect();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < n {
        if b[i] == '/' && i + 1 < n && b[i + 1] == '/' {
            while i < n && b[i] != '\n' {
                i += 1;
            }
        } else if b[i] == '/' && i + 1 < n && b[i + 1] == '*' {
            let mut depth = 1usize;
            i += 2;
            while i < n && depth > 0 {
                if b[i] == '/' && i + 1 < n && b[i + 1] == '*' {
                    depth += 1;
                    i += 2;
                } else if b[i] == '*' && i + 1 < n && b[i + 1] == '/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
        } else if b[i] == 'r' && i + 1 < n && (b[i + 1] == '"' || b[i + 1] == '#') {
            let mut j = i + 1;
            let mut hashes = 0usize;
            while j < n && b[j] == '#' {
                hashes += 1;
                j += 1;
            }
            if j < n && b[j] == '"' {
                // 裸字符串：跳到 `"###…`（井号个数相同）
                let mut k = j + 1;
                loop {
                    if k >= n {
                        i = n;
                        break;
                    }
                    if b[k] == '"' && (k + 1..=k + hashes).all(|m| m < n && b[m] == '#') {
                        i = k + 1 + hashes;
                        break;
                    }
                    k += 1;
                }
            } else {
                i += 1;
            }
        } else if b[i] == '"' {
            let mut buf = String::new();
            i += 1;
            while i < n && b[i] != '"' {
                if b[i] == '\\' {
                    i += 2;
                } else {
                    buf.push(b[i]);
                    i += 1;
                }
            }
            out.push(buf);
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

/// `filewin/` 里**会被画出去**的那些文件。
///
/// 🔴 **`fonts.rs` 自己不在人群里** —— [`PROBE`] 就住那儿，
/// 让它进人群等于**恒等两侧同源**：`PROBE` 里随手多一个字，扫描也照样「找到」它。
/// 这条排除的正当性由 [`fonts_rs_draws_nothing_at_all`] 买。
fn drawn_files() -> Vec<(String, String)> {
    let dir = guard_core::addr::filewin_src_root();
    // 🔴 走 `guard_core` 而不是裸 `read_dir` —— `scanning_guard_registry` 那条判据
    //    钉着「扫描型判据不许自己遍历」，理由是一个不摘掉自己的扫描判据会
    //    **在自己的语料里找到自己 ⇒ 恒绿**。
    //
    // ⚠ 但用的是**显式排除**那一支，**不是** `scan_tree!` 的自摘 ——
    //    因为那一刀**在这一处不生效**（理由见下一段；今天承重的是**明写名单**）。
    //
    // 🔴 **〔2026-09-21 订正 —— 我上一拍在这里写了一句过宽的假话〕**
    //   原文写的是「`scan_tree!` 的自摘**在本仓一处都不生效**」。**那是个全称，而它是假的。**
    //   自摘靠**后缀比对**，而草垛是 `root.join(相对路径)` 拼出来的**字符串**
    //   ⇒ 命不命中只取决于**那个根自己的串里有没有和 `file!()` 同一段 `..`**：
    //   · 壳这一侧的根一律过 `repo_root()`（`.parent().parent()`，**规范化过**）⇒ 串里没有 `..` ⇒ 命不中；
    //   · 而 backend 的 `tests_root()` 是 `CARGO_MANIFEST_DIR.join("../../tests/backend")`
    //     ⇒ 串里**有** `..` ⇒ 在那一棵上**自摘真的落刀**。
    //   ⇒ 正确的说法是「**这一处**不生效」，不是「本仓一处都不生效」。
    //   ⚠ 那句全称的源头是 `guard-core` 判据抬头里的原话，我**没复核就当事实转了出去**（还转给了四路 agent）。
    //   逐处读数住。
    //
    // ⇒ 于是本条为什么仍旧走显式排除：**本条要摘的压根不是调用者自己**
    //   （调用者住 `tests/frontend/shell/filewin/`，不在被扫的那棵树里），而是 `fonts.rs` ——
    //   `PROBE` 就住那儿，让它进人群等于**恒等两侧同源**。自摘那一刀在这里落不落都无关。
    //
    // 🔴**排除名单多了两份，理由与 `fonts.rs` 那一份不同：**
    //   `proc.rs`（窗口改独立进程那一侧）与 `main.rs`（本包那个 `[[bin]]` 的入口）
    //   **一个字都不画在 egui 上**：
    //   · `proc.rs` 的中文串两个去处 —— monitor 那条 Tauri 命令的返回值（走 **webview**，
    //     字体归浏览器）与窗口进程的 **stderr**（终端，字体归终端）；
    //   · `main.rs` 只有头注与一行 `main`。
    //   ⇒ 收进人群的后果是**现打**出来的，不是推的：非 ASCII 字符从 249 涨到 **355**
    //   ⇒ 要往 `PROBE` 里塞 106 个**永远不会出现在窗口上**的字，而 `PROBE` 是
    //   「窗口上会不会出豆腐块」那把尺子 —— 掺进去只会让它对真问题更钝。
    //   ⚠ 这条排除**与 `fonts.rs` 那条不同源**：那一条防的是恒等两侧同源，
    //   这一条防的是**人群定义漂了**（「会被画出去」被读成「在 `filewin/` 里」）。
    //   ⚠ 它买不到「`proc.rs` 以后也不会画东西」—— 哪天它真往 `ui.label` 上写字，
    //   这条排除就成了一个洞。如实记；补它要一条「哪些字面量会走到 `ui` 上」的判准，
    //   而那正是本条今天刻意不判的东西。
    // 窗口独立成包：「`filewin/` 那棵树」今天是两棵 —— 本包 ＋ monitor 那一侧 `src/frontend/shell/src/filewin/`
    //   （`entry.rs` 在那边）。`mod.rs` 的躯体成了本包的 `lib.rs`（两份都摘）；两侧各有一份 `proc.rs`（都摘）；
    //   `frame_log.rs`（帧日志）同理：它写的是一份日志文件，一个字都不画在 egui 上。
    //   `render.rs`（挑哪块显卡）同理：它的话只上 stderr（挑中的那一行 · 挑不出时那一句），一个字都不画在 egui 上。
    //   通信层的 `chan/said.rs` 进人群：通道没成时窗口上画的那一句从它取（全产品一份，不住本包）。
    let monitor_side = guard_core::addr::repo_root().join("src/frontend/shell/src/filewin");
    let said = guard_core::addr::repo_root().join("src/comms/inward/chan/said.rs");
    let mut out: Vec<(String, String)> = guard_core::scan_tree_excluding(
        &dir,
        &["rs"],
        &[
            "fonts.rs",
            "lib.rs",
            "proc.rs",
            "frame_log.rs",
            "render.rs",
            "main.rs",
        ],
    )
    .into_iter()
    .chain(guard_core::scan_tree_excluding(
        &monitor_side,
        &["rs"],
        &["mod.rs", "proc.rs"],
    ))
    .chain(std::iter::once((
        said.clone(),
        std::fs::read_to_string(&said).expect("读不到 chan/said.rs"),
    )))
    .map(|(path, src)| {
        let name = path
            .file_name()
            .expect("扫到的每一项都是文件")
            .to_string_lossy()
            .to_string();
        (name, src)
    })
    .collect();
    out.sort();
    out
}

/// 人群里每一份文件的字面量里出现过的非 ASCII 字符。
fn label_chars() -> std::collections::BTreeSet<char> {
    let mut set = std::collections::BTreeSet::new();
    // 窗口上的字搬进了文案表（`src/shared/copy/table.json`），源码里只剩
    //   `copy_text("key", …)` ⇒ 「窗口会画出去的字」= 源码字面量 ＋ 这些调用取的表条目原文。
    //   只数字面量的话，抽完表之后这条会对着一个缩水的人群相等（它真画出去的字没被数到）。
    let table: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            guard_core::addr::filewin_src_root().join("../../../shared/copy/table.json"),
        )
        .expect("读不到文案表"),
    )
    .expect("文案表不是 JSON");
    let entries = table["entries"].as_object().expect("文案表没有 entries");
    for (_, src) in drawn_files() {
        for s in string_literals(&src) {
            set.extend(s.chars().filter(|c| !c.is_ascii()));
        }
        for key in copy_text_keys(&src) {
            let zh = entries[&key]["zh"]
                .as_str()
                .unwrap_or_else(|| panic!("文案表里没有 {key}"));
            set.extend(zh.chars().filter(|c| !c.is_ascii()));
        }
    }
    // 字住两端共用的核心、窗口照画的那几句：`pick-core`（选中几项 ⇒ 菜单那张表 · 灰着的为什么）·
    //   `batch-core`（一组的标题 · 结局句 · 失败那几行 · 断开那一行 · 冲突框那一行）。
    for core in ["pick-core", "batch-core"] {
        let lib = std::fs::read_to_string(
            guard_core::addr::repo_root().join(format!("src/common/{core}/src/lib.rs")),
        )
        .unwrap_or_else(|_| panic!("读不到 {core}"));
        for key in copy_text_keys(&lib) {
            let zh = entries[&key]["zh"]
                .as_str()
                .unwrap_or_else(|| panic!("文案表里没有 {key}"));
            set.extend(zh.chars().filter(|c| !c.is_ascii()));
        }
    }
    // 后端写好、窗口照画的那几句（种类 · 「类型」那一列 · 预览 / 看图 / 编辑那几道门不许时那一句）。
    for (key, e) in entries {
        if key.starts_with("beFilesKind.") || key.starts_with("beFilesOpen.") {
            set.extend(
                e["zh"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .filter(|c| !c.is_ascii()),
            );
        }
    }
    set
}

/// 源码里 `copy_text("key"` 的 key（取文口的调用形状由 `copy-table.vitest.ts` 钉着：key 必须是字面量）。
fn copy_text_keys(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = src;
    // rustfmt 会把长调用折成 `copy_text(\n    "key",` ⇒ 括号后面先跳空白再认引号。
    while let Some(i) = rest.find("copy_text(") {
        let after = rest[i + "copy_text(".len()..].trim_start();
        if let Some(body) = after.strip_prefix('"') {
            if let Some(j) = body.find('"') {
                out.push(body[..j].to_string());
            }
        }
        rest = &rest[i + 1..];
    }
    out
}

// ═══════════════════════════════════════════════════════════════════
// 量具自己
// ═══════════════════════════════════════════════════════════════════

/// 🔴 扫描器自己得先对 —— 它是下面每一条的**量具**。
///
/// 喂一份**自带正确答案**的源码：注释里放中文（不许收）、字面量里放中文（必须收）、
/// 裸字符串里放中文（不许收 —— 裸字符串在本模块里只用来写 Windows 路径，全 ASCII）。
#[test]
fn the_literal_scanner_finds_exactly_what_it_should() {
    let fixture = r###"
// 行注释里的中文：甲乙丙
/* 块注释里的：丁戊
   /* 嵌套的：己庚 */ 还是注释：辛 */
fn f() {
    ui.label("要收的一");          // 尾注释里的：壬癸
    let p = r"C:\Windows\不收这个";
    let q = r#"也不收：子丑"#;
    println!("要收的二 {}", "要收的三");
    let esc = "带转义 \" 的：寅";
}
"###;
    let got: std::collections::BTreeSet<char> = string_literals(fixture)
        .iter()
        .flat_map(|s| s.chars())
        .filter(|c| !c.is_ascii())
        .collect();
    let want: std::collections::BTreeSet<char> =
        "要收的一要收的二要收的三带转义的：寅".chars().collect();
    assert_eq!(
        got,
        want,
        "字面量扫描器跑偏了。多收了 {:?}，漏收了 {:?}",
        got.difference(&want).collect::<Vec<_>>(),
        want.difference(&got).collect::<Vec<_>>()
    );
}

/// 排除 `fonts.rs` 的正当性：**它一个控件都不画**。
///
/// ⇒ 把它摘出人群不会藏掉任何一个真标签。哪天它开始画东西，本条红。
#[test]
fn fonts_rs_draws_nothing_at_all() {
    // 变量名不叫 `src`：包根下的路径变短、rustfmt 把读盘并成一行之后，`needle_anchor_registry` 那条按**变量名**认语料的棘轮
    //   会把同文件 `copy_text_keys(src)` 里的 `rest` 误认成它的派生（两个 `src` 不是同一个东西）。
    let fonts_rs = std::fs::read_to_string(guard_core::addr::filewin_src_root().join("fonts.rs"))
        .expect("fonts.rs 读不动");
    // 只看代码，不看注释 —— 注释里当然会提到这些名字
    let code: String = {
        let mut keep = String::new();
        let mut in_line = false;
        let mut prev = '\0';
        for c in fonts_rs.chars() {
            if in_line {
                if c == '\n' {
                    in_line = false;
                    keep.push(c);
                }
            } else if prev == '/' && c == '/' {
                keep.pop();
                in_line = true;
            } else {
                keep.push(c);
            }
            prev = c;
        }
        keep
    };
    for needle in [
        "ui.label(",
        "ui.button(",
        "ui.heading(",
        "colored_label(",
        "ui.strong(",
    ] {
        assert!(
            !code.contains(needle),
            "`fonts.rs` 里出现了 `{needle}` —— 它开始画东西了。\
             那么 `drawn_files()` 把它摘出去就会藏掉真标签：\
             要么把画的部分搬走，要么改人群并重新论证。"
        );
    }
}

/// 量具在**窗口今天的实况**（只有 egui 自带四份字体）下过两个方向的对照。
#[test]
fn the_ruler_passes_both_controls_on_the_bundled_fonts() {
    let ctx = egui::Context::default();
    ready(&ctx);
    for fid in [prop(), mono()] {
        assert_eq!(
            super::ruler_self_check(&ctx, &fid),
            None,
            "量具自检没过（字族 {:?}）—— 下面每一条的读数都不许用",
            fid.family
        );
    }
}

// ═══════════════════════════════════════════════════════════════════
// 探针与源码对账
// ═══════════════════════════════════════════════════════════════════

/// 🔴 [`PROBE`] **恰好**等于窗口会画出去的那些非 ASCII 字符。
///
/// 两侧不同源：这一侧现扫 `filewin/` 的源码树，那一侧是 `fonts.rs` 里手写的常量。
/// ⇒ 加了新标签忘了加进探针 ⇒ 红；探针里留了个用不着的字 ⇒ 也红。
#[test]
fn the_probe_equals_every_non_ascii_char_in_the_window_labels() {
    let files = drawn_files();
    assert_eq!(
        files.len(),
        // 主线 15 → 16，多的是 `upload.rs`（工具栏「上传」那一问）。
        //   字形一个没多：那几句刻意只用探针里已有的字。
        // 16 → 17，多的是 `bookmarks.rs`（书签栏：☆ / ★ 那颗按钮 · 那一排书签 · 存不了那几句）；
        //   17 → 18，多的是 `workspace.rs`（标签栏 · 双栏 · 复制到另一栏那几句）；
        //   18 → 19，多的是 `preview.rs`（预览面板那几句）。
        // 19 → 20，多的是 `size.rs`（算大小：菜单那一项 · 在算那一行 · 结局那一句）；
        //   20 → 21，多的是 `picker.rs`（原生选文件框；它自己一个字都不画，「浏览…」那颗按钮在 upload.rs / shell.rs 上）。
        // 21 → 22，多的是 `extract.rs`（解压到这里：菜单那一项 · 撞名那一问 · 在解那一行 · 结局那一句）；
        //   22 → 23，多的是 `lossy_pull.rs`（有损名下载：Windows 上「已改成 X」那一句 · 拷进暂存区没成那一句；字形一个没多）；
        //   23 → 24，多的是 `chunk_upload.rs`（上传块形：读不到本机文件那一句；「改经后端一段一段写」那一句画在 transfer.rs 上；字形一个没多）；
        //   24 → 25，多的是 `cross_copy.rs`（复制到另一台：填机器名那一问 · 盖不盖那一问 · 一条进度 · 结局；字形一个没多）。
        // 25 → 26，多的是 `grep.rs`（按内容搜：命中那一行 · 总述 · 停在哪一道上界 · 读不懂那一句）。
        // 26 → 27，多的是 `platform.rs`（窗口包的平台层：有损名下载 Windows 上「已改成 X」那一句随平台臂搬来；字形一个没多）。
        // 27 → 31，多的是 `chrome.rs`（工具条 · 命令栏 · 左栏 · 状态栏）· `kind.rs`（「类型」那一列的词）·
        //   `props.rs`（属性那个框）· `theme.rs`（样子；它一个字都不画，按文件数进人群）。
        // 31 → 32，多的是 `kit.rs`（条 · 状态行 · 加底色的字：窗口里的通用件）。
        // 32 → 33，多的是 `progress.rs`（窗口底部那张「进度」表：一趟一行的标题 · 小字 · 读数 · 停 / 重试）。
        // 33 → 34，多的是 `plan.rs`（文件窗口反查：「格」那一列 · 预览头「归属」· 右键「在计划里看」）。
        // 34 → 36，多的是 `imgview.rs`（图片查看页：头条 · 画面 · 状态栏那几句）· `picture.rs`（读图解图没成的那几句）。
        //   字形一个没多：两份的字全经文案表取，源码里不留非 ASCII 字面量。
        // 36 → 37，多的是 `batch.rs`（一组一条命令那一层：冲突框 · 逐项选 · 进度那一行 · 断开那一句）。
        // 37 → 38，多的是通信层的 `chan/said.rs`（通道没成时窗口上那一句，全产品一份）。
        // 38 → 36，少的是 `corpus.rs` · `scale.rs`（只在测试档编的语料与秤，住 `tests/`，一个字都不画）。
        // 36 → 37，多的是 `trip.rs`（一趟：各看板共用的那一格与落地时那一个结局；一个字都不画，按文件数进人群，同 `theme.rs`）。
        // 37 → 46，多的是 `shell/` 底下按职责拆出去的九份（cross · edit · frame · keys · listing · search · trips · window · writes）：
        //   都是从 `shell.rs` 原样搬过去的，字形一个没多。
        46,
        "人群应当是 10 份（2026-09-21 现打：copy · corpus · entry · **find** · rows · scale · \
         shell · source · transfer · **writeops**；`fonts.rs` 与 `mod.rs` 摘掉了。\
         ⚠ `corpus.rs` 的非 ASCII 字面量是 0，按字符数统计时看不见它 —— \
         人群按**文件**数，别按有没有贡献字符数）\
         〔`24f` 第四刀 09-21：8 → 9，多的是 `find.rs`（搜索那一侧的命令面与新鲜度那一行）〕\
         〔`24e` 第五刀 09-21：9 → 10，多的是 `writeops.rs`（那四条写操作的按钮与确认框）〕\
         〔`24e` 第八刀 09-21：10 → 11，多的是 `download.rs`（往外拖那两问的标题与\
          那句「盖掉它就没有备份了，不可撤销」）。🔴 那句话是这一批里最要紧的：\
          它是「这一下不可撤销」的唯一出口，变成豆腐块的时候用户会照点〕\
         〔第十三刀 09-23：**这个数一格没动，而 `filewin/` 多了两份文件** ——\
          `proc.rs` 与 `main.rs` 进了排除名单（它们一个字都不画在 egui 上，\
          逐条理由与那条现打读数「收进来 249 → 355」住 `drawn_files`）〕\
         〔F7b 09-24：12 → 14，多的是 `create.rs` 与 `bigfile.rs`（「新建空文件」那颗按钮与它那个框）〕\
         〔F9 09-24：（与 F7b 同拍合并，现打 14）多的是 `bigfile.rs`（大文件模式：顶上那句说明与行尾那枚「这一行共 N 字」）〕\
         〔FW1+FW2 09-24：14 → 15（与 F7b / F9 同拍合并，现打 15），多的是 `select.rs`（右键菜单上「打开」「删除这 N 项」、\
          做不了时那几句、打字跳转没找到那一句 —— 都画在 egui 上）〕\
         —— 现在是 {}，人群变了就重新论证一遍",
        files.len()
    );
    let scanned = label_chars();
    assert_eq!(
        scanned.len(),
        // 现扫出的字数本身没有对错：变了说明窗口标签动过，增减的是哪几个字看下面那条两向差集。
        339,
        "现扫出 {} 个不同的非 ASCII 字符：数变了说明窗口标签动过，连着下面那条两向差集一起看\
         （只钉字数的话，`−1 ＋1` 那种改动会安安静静地过去）",
        scanned.len()
    );
    let declared: std::collections::BTreeSet<char> = PROBE.chars().collect();
    assert_eq!(
        declared, scanned,
        "探针与真标签对不上。\n  探针里多出来（界面上没有）：{:?}\n  探针里漏了（界面上有、画不出也没人知道）：{:?}",
        declared.difference(&scanned).collect::<Vec<_>>(),
        scanned.difference(&declared).collect::<Vec<_>>()
    );
}

/// 量具**判不了** `'◻'` 与 `'?'` 这两个字（它们就是替换字形本身）。
/// ⇒ 探针里不许有它们，否则那两个字会永远报「画不出」。
#[test]
fn the_probe_excludes_the_two_chars_the_ruler_cannot_judge() {
    for c in ['◻', '?'] {
        assert!(
            !PROBE.contains(c),
            "探针里有 `{c}` —— 它是替换字形本身，量具对它永远报「画不出」\
             ⇒ `verify` 会永远出声。要么别在界面上用它，要么给量具换一条判法。"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════
// 🔴 反空真的锚
// ═══════════════════════════════════════════════════════════════════

/// 🔴 **不装字体时，探针里画不出的字数恰好是 333（比例）／ 332（等宽）。**
///
/// 这是这一摞的**反空真锚**：它同时钉住两件事 ——
/// ① 量具**能**说「画不出」（不是恒说「都能画」）；
/// ② 这个缺陷是**真的**，数是量出来的。
///
/// 等宽比比例多认一个字：`→`（U+2192）住 `Hack`，而 `Hack` 只在等宽那条链上。
///
/// ⚠ 这个数会随标签增减而动。动了就重新量、连理由一起改，**不许为了绿把它算出来**
/// （两侧同源就退化成恒真）。
#[test]
fn without_a_cjk_font_the_probe_is_almost_entirely_unrenderable() {
    let ctx = egui::Context::default();
    ready(&ctx);
    assert_eq!(
        super::ruler_self_check(&ctx, &prop()),
        None,
        "量具先得是好的"
    );
    assert_eq!(
        super::unrenderable(&ctx, &prop(), PROBE).len(),
        // 现打的读数：汉字不装字体全画不出，画得出的只有 egui 自带字体里有的那几个符号。
        // 10-31 现打 333：多的「请」「求」（通道那一句「请求格式不对」进了窗口），两个都是汉字、不装字体都画不出。
        333,
        "比例字体下画不出的字数变了（探针 {} 字）：重新量，这个数不许由探针字数算出来（两侧同源就退化成恒真）",
        PROBE.chars().count()
    );
    assert_eq!(
        super::unrenderable(&ctx, &mono(), PROBE).len(),
        // 现打：比比例少一个，就是 `→`（它住 `Hack`，只在等宽这条链上）。10-31 现打 332（同比例那格多「请」「求」）。
        332,
        "等宽字体下画不出的字数变了（探针 {} 字）：重新量，连「比比例那一格少几个、少的是哪几个」一起核",
        PROBE.chars().count()
    );
}

// ═══════════════════════════════════════════════════════════════════
// 装上之后
// ═══════════════════════════════════════════════════════════════════

/// 备好字体、装进 `ctx`（只 CJK 回退与图标字，候选表由判据给）。
fn install_from(ctx: &egui::Context, candidates: &[String]) -> Attempt {
    let (defs, attempt) = super::definitions(candidates, &super::Faces::default());
    ctx.set_fonts(defs);
    attempt
}

/// 本机上第一个存在的候选字体。`None` = 这台机器一份 CJK 字体都没有。
fn first_available_font() -> Option<String> {
    super::candidates()
        .into_iter()
        .find(|p| std::path::Path::new(p).is_file())
}

/// 装上系统字体之后，探针里**一个字都不缺**，而且 [`super::verify`] 回 `None`。
#[test]
fn installing_a_system_font_makes_the_whole_probe_renderable() {
    let Some(path) = first_available_font() else {
        // 🔴 不许静默跳过 —— 「跳过」和「过了」在终端上长得一样。
        panic!(
            "这台机器一份 CJK 字体都没有，候选表试过：{:?}\n\
             这是**环境缺件**，不是代码缺陷：装一份（Debian/Ubuntu: `apt install fonts-noto-cjk`）\
             或者设 `CCM_CJK_FONT=/path/to/font.ttc` 再跑。",
            super::candidates()
        );
    };
    let ctx = egui::Context::default();
    let attempt = install_from(&ctx, &[path.clone()]);
    assert!(
        matches!(attempt, Attempt::Loaded { .. }),
        "装 {path} 没成功：{attempt:?}"
    );
    ready(&ctx);
    assert_eq!(
        super::ruler_self_check(&ctx, &prop()),
        None,
        "量具先得是好的"
    );
    for fid in [prop(), mono()] {
        let miss = super::unrenderable(&ctx, &fid, PROBE);
        assert_eq!(
            miss.len(),
            0,
            "装了 {path} 之后 {:?} 字族还缺 {} 个字：{:?}",
            fid.family,
            miss.len(),
            &miss[..miss.len().min(12)]
        );
    }
    assert_eq!(
        super::verify(&ctx, &attempt),
        None,
        "探针一个字不缺，`verify` 却还在出声 —— 那两条对不上"
    );
}

/// 装字体是**追加在链尾当兜底**，不是插在链首 ⇒ 拉丁字仍旧由原来那份字体画。
///
/// 量的是 `'A'` 的字宽：换字体画就会变（CJK 字体的拉丁部分宽度不一样）。
/// **不钉具体数字 —— 钉的是「装前装后相等」**，两侧同源不了。
#[test]
fn installing_appends_as_fallback_so_latin_keeps_its_own_font() {
    let Some(path) = first_available_font() else {
        panic!("这台机器没有 CJK 字体 —— 同上一条，先装字体再跑");
    };
    let before = {
        let ctx = egui::Context::default();
        ready(&ctx);
        ctx.fonts_mut(|f| (f.glyph_width(&prop(), 'A'), f.glyph_width(&mono(), 'A')))
    };
    let after = {
        let ctx = egui::Context::default();
        install_from(&ctx, &[path]);
        ready(&ctx);
        ctx.fonts_mut(|f| (f.glyph_width(&prop(), 'A'), f.glyph_width(&mono(), 'A')))
    };
    assert_eq!(
        before, after,
        "装 CJK 字体之后 `'A'` 的字宽变了（比例/等宽 {before:?} -> {after:?}）\
         —— 说明它被插在了链首、把拉丁字也抢走了。等宽被抢走就不再等宽。"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 出声
// ═══════════════════════════════════════════════════════════════════

/// 一份字体都找不到时 [`super::verify`] **必须出声**，而且说清试过哪些路径。
#[test]
fn verify_says_out_loud_when_no_font_was_found() {
    let ctx = egui::Context::default();
    let attempt = install_from(&ctx, &[]);
    assert_eq!(attempt, Attempt::NoneFound { tried: Vec::new() });
    ready(&ctx);
    let note = super::verify(&ctx, &attempt).expect("一个字体都没装上，居然不出声");
    for needle in ["no CJK font found", "CCM_CJK_FONT", "boxes"] {
        assert!(
            note.contains(needle),
            "出声那句话里没有 `{needle}`，用户看了不知道该干什么：{note}"
        );
    }
}

/// 🔴 出声那句话**必须是纯 ASCII** —— 字体坏了的时候它是唯一还画得出来的东西。
#[test]
fn every_notice_this_module_can_produce_is_pure_ascii() {
    let ctx = egui::Context::default();
    let attempt = install_from(&ctx, &[]);
    ready(&ctx);
    let mut notes: Vec<String> = vec![
        super::NOTICE_PREFIX.to_string(),
        super::verify(&ctx, &attempt).expect("这一趟该出声"),
    ];
    for st in [
        FontState::NotInstalled,
        FontState::Pending(attempt.clone()),
        FontState::Pending(Attempt::ReadFailed {
            path: "/nope".into(),
            err: "boom".into(),
        }),
    ] {
        notes.push(st.notice().expect("这三态都该出声").to_string());
    }
    for n in &notes {
        assert!(
            n.is_ascii(),
            "出声的话里有非 ASCII 字符 —— 字体坏了的时候它自己也会变成豆腐块：{n:?}"
        );
    }
}

/// 三态**分得开**：没装过 · 装了没复核 · 复核过没问题。
///
/// 🔴 前两态都出声。合成两态的话，「这一步根本没接上」就长得跟「一切正常」一样。
#[test]
fn the_three_font_states_are_distinguishable() {
    assert!(
        FontState::NotInstalled.notice().is_some(),
        "「没装过」必须出声 —— 否则装字体那一步被谁摘了都没人知道"
    );
    assert!(
        FontState::Pending(Attempt::NoneFound { tried: Vec::new() })
            .notice()
            .is_some(),
        "「装了没复核」必须出声"
    );
    assert_eq!(
        FontState::Checked(None).notice(),
        None,
        "「复核过、没问题」才是唯一一个不出声的态"
    );
    assert_eq!(
        FontState::Checked(Some("[font] x".into())).notice(),
        Some("[font] x"),
        "复核出问题时出声的就是复核那句话本身"
    );
}

/// `settle` **只走一次** `Pending -> Checked`，回值说清有没有走。
#[test]
fn settle_moves_pending_to_checked_exactly_once() {
    let ctx = egui::Context::default();
    ready(&ctx);
    let mut st = FontState::Pending(Attempt::NoneFound { tried: Vec::new() });
    assert!(st.settle(&ctx), "第一趟该定下来");
    assert!(matches!(st, FontState::Checked(_)), "定完了该是 Checked");
    assert!(!st.settle(&ctx), "第二趟不该再动 —— 否则每帧都去抢那把锁");
    let mut n = FontState::NotInstalled;
    assert!(!n.settle(&ctx), "没装过的不该被 settle 蒙成「复核过」");
    assert_eq!(n, FontState::NotInstalled);
}

/// 载入那条线程没交回就没了 ⇒ 收到的是 [`Attempt::Lost`]，复核照样出声、说清是载入没交回（不 panic：发布档 panic 即整个窗口进程没了）。
#[test]
fn a_loader_that_dies_is_reported_not_a_panic() {
    let ctx = egui::Context::default();
    let loading = super::load_with(&ctx, || panic!("载入半路没了（判据造的）"));
    let mut got = None;
    for () in guard_core::wall::rounds() {
        got = loading.poll(&ctx);
        if got.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(got, Some(Attempt::Lost));
    ready(&ctx);
    let note = super::verify(&ctx, &Attempt::Lost).expect("载入没交回，复核居然不出声");
    assert!(note.contains("font loading stopped"), "{note}");
}
