//! [`super`] 的判据 —— **窗口上的字画不画得出来**。
//!
//! 设计住 `调研/设计/60 §6.1`（「字体（CJK）」那一行；此前引的 `§5.4g` 那一节从来不存在）；读数住 `调研/真相源/102`。
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
    let dir = crate::guard_support::crate_src_root().join("filewin");
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
    //   · bridge 这一侧的根一律过 `repo_root()`（`.parent().parent()`，**规范化过**）⇒ 串里没有 `..` ⇒ 命不中；
    //   · 而 backend 的 `tests_root()` 是 `CARGO_MANIFEST_DIR.join("../../tests/backend")`
    //     ⇒ 串里**有** `..` ⇒ 在那一棵上**自摘真的落刀**。
    //   ⇒ 正确的说法是「**这一处**不生效」，不是「本仓一处都不生效」。
    //   ⚠ 那句全称的源头是 `guard-core` 判据抬头里的原话，我**没复核就当事实转了出去**（还转给了四路 agent）。
    //   逐处读数住 `真相源/102`。
    //
    // ⇒ 于是本条为什么仍旧走显式排除：**本条要摘的压根不是调用者自己**
    //   （调用者住 `tests/bridge/filewin/`，不在被扫的那棵树里），而是 `fonts.rs` ——
    //   `PROBE` 就住那儿，让它进人群等于**恒等两侧同源**。自摘那一刀在这里落不落都无关。
    //
    // 🔴〔第十三刀 2026-09-23〕**排除名单多了两份，理由与 `fonts.rs` 那一份不同：**
    //   `proc.rs`（窗口改独立进程那一侧）与 `win_main.rs`（那个 `[[bin]]` 的入口）
    //   **一个字都不画在 egui 上**：
    //   · `proc.rs` 的中文串两个去处 —— monitor 那条 Tauri 命令的返回值（走 **webview**，
    //     字体归浏览器）与窗口进程的 **stderr**（终端，字体归终端）；
    //   · `win_main.rs` 只有头注与一行 `main`。
    //   ⇒ 收进人群的后果是**现打**出来的，不是推的：非 ASCII 字符从 249 涨到 **355**
    //   ⇒ 要往 `PROBE` 里塞 106 个**永远不会出现在窗口上**的字，而 `PROBE` 是
    //   「窗口上会不会出豆腐块」那把尺子 —— 掺进去只会让它对真问题更钝。
    //   ⚠ 这条排除**与 `fonts.rs` 那条不同源**：那一条防的是恒等两侧同源，
    //   这一条防的是**人群定义漂了**（「会被画出去」被读成「在 `filewin/` 里」）。
    //   ⚠ 它买不到「`proc.rs` 以后也不会画东西」—— 哪天它真往 `ui.label` 上写字，
    //   这条排除就成了一个洞。如实记；补它要一条「哪些字面量会走到 `ui` 上」的判准，
    //   而那正是本条今天刻意不判的东西。
    let mut out: Vec<(String, String)> = guard_core::scan_tree_excluding(
        &dir,
        &["rs"],
        &["fonts.rs", "mod.rs", "proc.rs", "win_main.rs"],
    )
    .into_iter()
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
    for (_, src) in drawn_files() {
        for s in string_literals(&src) {
            set.extend(s.chars().filter(|c| !c.is_ascii()));
        }
    }
    set
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
    let src =
        std::fs::read_to_string(crate::guard_support::crate_src_root().join("filewin/fonts.rs"))
            .expect("fonts.rs 读不动");
    // 只看代码，不看注释 —— 注释里当然会提到这些名字
    let code: String = {
        let mut keep = String::new();
        let mut in_line = false;
        let mut prev = '\0';
        for c in src.chars() {
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
        // 〔F7c · 第三波 09-24〕主线 15 → 16，多的是 `upload.rs`（工具栏「上传」那一问）。
        //   字形一个没多：那几句刻意只用探针里已有的字。
        // 〔FW34 · 第四波 09-24〕16 → 17，多的是 `bookmarks.rs`（书签栏：☆ / ★ 那颗按钮 · 那一排书签 · 存不了那几句）；
        //   17 → 18，多的是 `workspace.rs`（标签栏 · 双栏 · 复制到另一栏那几句）；
        //   18 → 19，多的是 `preview.rs`（预览面板那几句）。
        19,
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
          `proc.rs` 与 `win_main.rs` 进了排除名单（它们一个字都不画在 egui 上，\
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
        352,
        "现扫出 {} 个不同的非 ASCII 字符（2026-09-21 现打 249；第五刀之前是 205、\
         `24f` 第四刀之前是 130）。\
         〔F9c 与 FW34 合并 09-24：359 → 352（现打），−7 零 ＋（F9c 那 −9 里 `变 收` 仍被 `preview.rs` / `workspace.rs` 用着，留下）〕\
         〔FW34 09-24：328 → 337，恰好 ＋9、零 −（`× ★ ☆ 书 加 懂 找 移 签`，书签栏，逐笔来路住 `fonts.rs` 的 `PROBE` 上方）〕\
         〔FW34 预览 09-24：356 → 359，恰好 ＋3、零 −（`显 示 预`）〕\
         〔FW34 标签页 ＋ 双栏 ＋ 搬家 09-24：337 → 356，恰好 ＋19、零 −（标签页 ＋ 双栏 14 个 ＋ `entry.rs` 搬不进去那两句 5 个，逐笔来路住 `fonts.rs` 的 `PROBE` 上方）〕\
         〔F7a 09-24：314 → 305，−9 零 ＋（`务 己 慢 搬 服 流 自 量 零`，复制换到后端之后\
          「零流量 / 慢路」那两句没了，逐笔来路住 `fonts.rs` 的 `PROBE` 上方）〕\
         〔F9c 第四波 09-24：328 → 319，恰好 −9、零 ＋（`么 反 变 号 控 收 斜 杠 至` —— F9 续那两句「存不回去」随只读一档删了；\
          新写的两句「超过上限没有发出去」「存到第 N 段（共 M 段）时断了」刻意只用探针里已有的字，逐笔来路住 `fonts.rs` 的 `PROBE` 上方）〕\
         〔F9 续 09-24：319 → 328，恰好 ＋9、零 −（`么 反 变 号 控 收 斜 杠 至`，存不回去那句话，逐笔来路住 `fonts.rs` 的 `PROBE` 上方）〕\
         〔FW1+FW2 09-24：313 → 319（合并后现打），恰好 ＋6、零 −（`事 任 何 头 选 项`，逐笔来路住 `fonts.rs` 的 `PROBE` 上方）〕\
         〔F2 09-24：306 → 314，＋11 −3（逐笔来路住 `fonts.rs` 的 `PROBE` 上方）〕\
         〔F9 09-24：314 → 322，恰好 ＋8、零 −（`共 向 式 幕 横 模 滚 长`，全来自 `bigfile.rs`，\
          逐笔来路住 `fonts.rs` 的 `PROBE` 上方）〕\
         〔补齐五项 09-23：298 → 306，恰好 ＋8、零 −（`小 序 排 此 称 终 请 🔗`，\
          逐笔来路住 `fonts.rs` 的 `PROBE` 上方）〕\
         这个数本身没有对错，但它变了说明标签动过 —— 连着下面那条一起看。\
         〔第七刀 09-21：249 → 253，而它是 `−1 +5` ——「哪」随 `entry.rs` 那句\
          「要打开哪个目录？路径是空的」一起没了，「以必绝解须」来自 `source.rs`\
          那两句「对面把 home 解成了空串/相对路径」。🔴 只钉字数的话，`−1 +1`\
          那种改动会安安静静地过去；出声的是下面那条两向差集。〕",
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

/// 🔴 **不装字体时，探针里画不出的字数恰好是 194（比例）／ 193（等宽）。**
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
        338,
        "比例字体下画不出的字数变了（2026-09-21 现打 238 / 探针 {} 字；\
         〔F9c 与 FW34 合并 09-24：345 → **338**（现打），探针 359 → 352 字 —— 差额恰好 −7：掉出探针的\
          `么 反 号 控 斜 杠 至` 本来全都画不出〕\
         〔FW34 预览 09-24：342 → **345**，探针 356 → 359 —— ＋3 ＝ `显 示 预` 不装字体全画不出〕\
         〔FW34 标签页 ＋ 双栏 ＋ 搬家 09-24：323 → **342**，探针 337 → 356 字 —— ＋19 ＝ 新进来的 19 个字不装字体全画不出\
          （连 `●` 与全角 `＋` 也是：egui 自带字体里没有它们）〕\
         〔FW34 09-24：317 → **323**，探针 328 → 337 字 —— **＋6 不是 ＋9**：新进来的 6 个汉字不装字体画不出，\
          `× ★ ☆` 三个住 egui 自带字体、画得出（书签按钮的两态因此不靠系统字体）〕\
         第五刀之前是 194 / 205、`24f` 第四刀之前是 121 / 130）\
         〔F9c 第四波 09-24：317 → **308**（现打），探针 328 → 319 字 —— 差额恰好 −9：掉出探针的\
          `么 反 变 号 控 收 斜 杠 至` 本来全都画不出〕\
         〔F7a 09-24：303 → **294**（现打），探针 314 → 305 字 —— 差额恰好 −9：掉出探针的\
          `务 己 慢 搬 服 流 自 量 零` 本来全都画不出〕\
         〔F9 续 09-24：308 → **317**，探针 319 → 328 字 —— ＋9 ＝ 新进来的 9 个汉字不装字体全画不出〕\
         〔FW1+FW2 09-24：302 → **308**（合并后现打），探针 313 → 319 字 —— ＋6 ＝ 新进来的 6 个汉字\
          （`事 任 何 头 选 项`）不装字体全画不出〕\
         〔F2 09-24：294 → **303**，探针 306 → 314 字 —— ＋9 ＝ ＋11 −2：新进来的 11 个汉字全画不出；\
          走掉的 `§ 但 案` 里 `§` 本来就画得出（拉丁 1 区），`但 案` 画不出 ⇒ 只减 2〕\
         〔补齐五项 09-23：287 → **294**，探针 298 → 306 字 —— **＋7 不是 ＋8**：\
          新进来的 8 个字里 7 个汉字不装字体画不出，`🔗` 住 egui 自带 emoji 字体、画得出。\
          那一格正是换掉 `↳` 的理由（`↳` 装了 CJK 字体也画不出，见下面那条）。〕\
         〔本机侧退役 09-23：293 → **287**，探针 304 → 298 字 —— **差额恰好是 −6**，\
          也就是掉出探针的那 6 个字（`⇒ 么 什 四 标 立`，逐笔理由住 `fonts.rs` 的\
          `PROBE` 上方）**本来全都画不出**；画得出的那一批一个没动。\
          ⚠ 这个数照旧**不许**由上面那个 298 算出来（两侧同源就退化成恒真）——\
          287 是现打的，是这一趟自己印出来的读数。〕\
         〔第十三刀 09-23：302 → 293，探针 313 → 304 字 —— **差额恰好是 −9**\
          （`−10 +1`，逐笔理由住 `fonts.rs` 的 `PROBE` 上方）。\
          🔴 换句话说：掉出探针的那 10 个字与新进来的那 1 个字**全都是本来画不出的** ——\
          画得出的那 11 个（拉丁标点与两个 emoji 之外的那几个）一个没动。\
          这个数**不许**由上面那个 304 算出来（两侧同源就退化成恒真），\
          它是现打的：`cargo test -p monitor --lib filewin::fonts::` 那一趟自己印的〕",
        PROBE.chars().count()
    );
    assert_eq!(
        super::unrenderable(&ctx, &mono(), PROBE).len(),
        336,
        "等宽字体下画不出的字数变了（2026-09-22 现打 300 —— 那时比比例少**两**个\
         〔F9c 与 FW34 合并 09-24：343 → **336**（现打），与比例那一格同一个 −7；338 − 336 = 2 那条关系没动〕\
         〔F9c 第四波 09-24：316 → **307**（现打），与比例那一格同一个 −9；308 − 307 = 1 那条关系没动〕\
         （`→` 与第十二刀新进来的 `⇒`）；今天少**一**个，理由见下方本机侧退役那一条；\
         第五刀之前是 193、`24f` 第四刀之前是 120）\
         〔F7a 09-24：302 → **293**（现打），与比例那一格同一个 −9；294 − 293 = 1 那条关系没动〕\
         〔F9 续 09-24：307 → **316**，与比例那一格同一个 ＋9；317 − 316 = 1 那条关系没动〕\
         〔FW1+FW2 09-24：301 → **307**（合并后现打），与比例那一格同一个 ＋6；308 − 307 = 1 那条关系没动〕\
         〔FW34 09-24：316 → **322**，与比例那一格同一个 ＋6（`× ★ ☆` 两条链上都画得出）；323 − 322 = 1 那条关系没动〕\
         〔FW34 标签页 ＋ 双栏 ＋ 搬家 09-24：322 → **340**，＋18 不是 ＋19：`●` 在等宽这条链上**画得出**（比例那条画不出）\
          ⇒ 「比比例少几个」这条关系今天是 **2**（`→` 与 `●`）：342 − 340 = 2。两个数各自现打〕\
         〔FW34 预览 09-24：340 → **343**，与比例那一格同一个 ＋3；345 − 343 = 2 那条关系没动〕\
         〔第十三刀 09-23：300 → 291，与比例那一格**同一个 −9**，\
          而「比比例少两个」这条关系一格没动〕\
         〔F2 09-24：293 → **302**，与比例那一格同一个 ＋9；303 − 302 = 1 那条关系没动〕\
         〔F9 09-24（与 F7a 同拍合并）：293 → **301**，与比例那一格同一个 ＋8；311 − 310 = 1 那条关系没动〕\
         〔补齐五项 09-23：286 → **293**，与比例那一格**同一个 ＋7**（`🔗` 两条链上都画得出），\
          而「比比例少一个（`→`）」那条关系一格没动：294 − 293 = 1〕\
         〔本机侧退役 09-23：291 → **286**，也就是 **−5，不是 −6** ——\
          而那个差一格**恰好把上面那句话验了一遍**：掉出探针的 6 个字里，\
          `⇒` 在等宽这条链上**本来就画得出**（`Hack` 有它）⇒ 它离开探针不减这个数。\
          🔴 于是「比比例少两个（`→` 与 `⇒`）」这条关系**今天只剩一个**：\
          287 − 286 = 1，就是 `→`。两个数各自现打，没有一个是从另一个算出来的。〕"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 装上之后
// ═══════════════════════════════════════════════════════════════════

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
    let attempt = super::install_from(&ctx, &[path.clone()]);
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
        super::install_from(&ctx, &[path]);
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
    let attempt = super::install_from(&ctx, &[]);
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
    let attempt = super::install_from(&ctx, &[]);
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
