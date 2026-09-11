//! `K-W2D` · `R2` ④：**按需拉取那条路上，四样东西各自的机器判据。**
//!
//! 判的对象是 `sidecars/codepicture/fetch.rs`（协议与判定）与它在线上那一侧的对账面。
//! 被判的那四样是什么、为什么这么定，整段住那份文件的头注 —— 这里不复述第二份，
//! 只说**每一格钉的是什么、认不出什么**。
//!
//! # 一 · 禁词那几格（「唯一失败面」的机器面）
//!
//! 「一种失败只许有一个出口」这句话本身不可机检；能机检的是**第二条出口长什么样**。
//! 四种形状各占一格（[`tests::FORBIDDEN`]），每一种都有一次真实来历：
//!
//! - 目录级标记那个名字 —— `K-W4` 那个坑的本体；落点目录**已经有两个写入方**，
//!   而 `R3` 逐字记着 08-11 那次「两条写入路共用一个目录 ⇒ 无限重装循环」。
//! - 「最新的那一份」那个词 —— 一条静默的第二退路：有了它，「这份 daemon 没带钉」
//!   这个真病因就再也不会被人看见。
//! - `unwrap_or` 那一族 —— 给缺失的钉配一个默认值，等于把失败面换成一个假答案。
//! - 在 `PATH` 上再找一份 —— 「拉不到就用机器上碰巧有的那个」，那是第二条来历不明的路。
//!
//! # 二 · 🔴 这几格的尺子是**代码**，不是整份文件
//!
//! 量的是 `production_code` 的产物，而**它自己就把注释剥掉了**（块注释 · 整行 `//` ·
//! 行尾 `//`，剥法与它剥不干净的那三种情形全部住 `guard_core`，这里不复述）
//! ⇒ **注释里可以（而且必须）把这几个词写出来**，写不出来就没人知道躲的是什么。
//! ⇒ 本条**认不出**「有人把禁词藏进注释里当文档」这一形 —— 那本来就不是它要管的事。
//!
//! ⚠ 〔本轮自查订正〕头一版在 `production_code` 后面**又串了一道** `strip_comment_lines`，
//! 并把「尺子是代码」这件事记在那一道上。实测（切刀 M19）：那一道是**空转的** ——
//! 把它整把换成恒等，这几格一个都不红。真正在剥注释的是 `production_code`。
//! ⇒ 删掉那一道。留着的话，这份文件里就有**两份关于「注释怎么剥」的说法**，
//! 而其中一份是假的。
//!
//! ⚠ 同理它**认不出语义等价物**：换个名字写一个目录级标记、或用 `unwrap_or_else`
//! 的别名，它一个字都不会说。这几格买的是「**这四种已经栽过的写法回不来**」，
//! **不是**「不存在第二条路」。别把它读大一格。
//!
//! # 三 · 线上那一半今天对不齐，本模块把这件事钉成会红的东西
//!
//! `R2` ④ 逐字要「握手帧 `unavailable` 里挂命令 + **明确的失败文案**」，
//! 而 `wire::Unavailable` 今天只有两个字段、装不下那句话。
//! [`tests::the_wire_unavailable_still_cannot_carry_the_sentence`] 钉住这个现状：
//! **它红的那天 = 有人给 wire 加了字段的那天**，那时回到
//! `sidecars::codepicture::fetch::unavailable_for` 把两半合成一个值。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
mod tests {
    use crate::guard_support::production_code;
    use std::path::{Path, PathBuf};

    /// 本 crate 的 `src/`。
    fn src_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    /// sidecar 那一层的树根。
    fn sidecars_dir() -> PathBuf {
        src_dir().join("sidecars")
    }

    /// 那一层每份文件的**代码文本**（`production_code`：剥测试段 + 剥注释，一次做完）。
    fn sidecar_code() -> Vec<(String, String)> {
        guard_core::scan_tree!(&sidecars_dir(), &["rs"])
            .into_iter()
            .map(|(p, src)| {
                let rel = p
                    .strip_prefix(src_dir())
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel, production_code(&src))
            })
            .collect()
    }

    /// 四种**已经栽过**的第二条出口。`(禁词, 它为什么在这张表上)`。
    ///
    /// 🔴 **不许为了让新红变绿从这里删行。** 真有正当理由要写其中一种，
    /// 那是一次要论证的改动（论据 + 读数），不是一次顺手的删除。
    const FORBIDDEN: &[(&str, &str)] = &[
        (
            ".build_id",
            "目录级标记：落点目录已经有两个写入方，第三份会互相覆盖（R3 记的 08-11 那次无限重装循环）",
        ),
        (
            "latest",
            "「拿最新那份顶上」是一条静默的第二退路，它会把「没带钉」这个真病因藏起来",
        ),
        (
            "unwrap_or",
            "给缺失的钉配默认值 = 把一个失败面换成一个假答案",
        ),
        (
            "on_path(",
            "「拉不到就用机器上碰巧有的那个」——第二条来历不明的路",
        ),
    ];

    /// 一段代码文本里命中了哪几条禁词。
    ///
    /// 单独成函数，是为了让下面那条反向自检拿**合成文本**正反各喂一遍 ——
    /// 直接在真树上判的话，「扫到了、而它是干净的」与「压根没扫到」输出一模一样。
    fn offenders(code: &str) -> Vec<&'static str> {
        FORBIDDEN
            .iter()
            .filter(|(needle, _)| code.contains(needle))
            .map(|(needle, _)| *needle)
            .collect()
    }

    /// ① 拉取那条路的代码里，四种第二出口**一种都没有**。
    #[test]
    fn the_fetch_path_has_no_second_silent_exit() {
        let files = sidecar_code();
        assert!(
            files.len() >= 3,
            "只扫到 {} 份 sidecar 源码 —— 采集器坏了，本条在空转",
            files.len()
        );
        let mut bad: Vec<String> = Vec::new();
        for (rel, code) in &files {
            for needle in offenders(code) {
                let why = FORBIDDEN
                    .iter()
                    .find(|(n, _)| *n == needle)
                    .map(|(_, w)| *w)
                    .unwrap_or_default();
                bad.push(format!("{rel} 里出现了 `{needle}` —— {why}"));
            }
        }
        assert!(
            bad.is_empty(),
            "按需拉取那条路上长出了第二条出口：\n  {}",
            bad.join("\n  ")
        );
    }

    /// ② 反向那半：四条针**逐条单断** —— 每条针自己真的会咬人，而且只咬自己那一条。
    ///
    /// 🔴 只有「全断」那一格的话，买到的是目录级塌陷：四条针里死掉三条也照样绿。
    #[test]
    fn each_forbidden_shape_is_really_detected_on_its_own() {
        for (needle, _) in FORBIDDEN {
            let synthetic = format!("fn f() {{ let _ = \"{needle}\"; }}\n");
            let hit = offenders(&production_code(&synthetic));
            assert_eq!(
                hit,
                vec![*needle],
                "喂了一份只含 `{needle}` 的合成代码，报出来的却是 {hit:?}"
            );
        }
        assert!(
            offenders("fn f() {}\n").is_empty(),
            "一份干净的合成代码被报成有问题 —— 那是假阳，会让人把整条判据关掉"
        );
    }

    /// ③ 那把尺子**量的是代码，不是注释** —— 把这条边界钉成读数，别让它只是一句话。
    #[test]
    fn the_ruler_looks_at_code_not_at_prose() {
        let prose = format!(
            "//! 这里讲的正是 `{}` 那个坑\nfn f() {{}}\n",
            FORBIDDEN[0].0
        );
        assert!(
            offenders(&production_code(&prose)).is_empty(),
            "注释里提到禁词被判成了违规 —— 那会逼着人删掉解释，而解释正是这几格的价值所在"
        );
    }

    /// `fetch.rs` 的**代码文本**（本模块几条判据共用的语料）。
    fn fetch_code() -> String {
        let src = include_str!("sidecars/codepicture/fetch.rs");
        assert!(
            src.len() > 4_000,
            "只读到 {} 字节的 fetch.rs —— include_str! 没读到，下面几条在空转",
            src.len()
        );
        production_code(src)
    }

    /// `Face::code` 那个 `match` 的体。
    fn code_arms() -> String {
        let src = fetch_code();
        let head = "fn code(&self) -> &'static str {";
        let at = src
            .find(head)
            .expect("`fetch.rs` 生产段里找不到 `Face::code` 的签名 —— 锚点挪了");
        let body = &src[at + head.len()..];
        let end = body
            .find("\n    }")
            .expect("`Face::code` 的体收不了尾 —— 抽取器坏了");
        body[..end].to_string()
    }

    /// 失败面那个闭集**现算**出来的 code 清单 —— 从 `Face::code` 的体里读，不另立一张表。
    fn face_codes() -> Vec<String> {
        let body = code_arms();
        let mut out = Vec::new();
        let mut rest = body.as_str();
        while let Some(i) = rest.find("\"sidecar_") {
            let after = &rest[i + 1..];
            let j = after.find('"').expect("code 字面量没有收尾引号");
            out.push(after[..j].to_string());
            rest = &after[j..];
        }
        out
    }

    /// ④ 闭集的每个成员都真的给了一个 code（没有一支落在命名空间之外）。
    #[test]
    fn every_arm_of_the_closed_set_produces_one_namespaced_code() {
        let arms = code_arms().matches("=>").count();
        let codes = face_codes();
        assert!(arms >= 5, "只数出 {arms} 支 —— 抽取器切窄了，本条在空转");
        assert_eq!(
            codes.len(),
            arms,
            "`Face::code` 有 {arms} 支，却只产出 {} 个 `sidecar_` 开头的 code：{codes:?}\n\
             有一支的 code 掉出了本层的命名空间，或者被写成了别处算出来的值。",
            codes.len()
        );
        let mut uniq = codes.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(
            uniq.len(),
            codes.len(),
            "两种坏法共用了一个 code：{codes:?} —— 那等于没有失败面"
        );
    }

    /// 协议文档那一份。
    fn doc() -> &'static str {
        const DOC: &str = include_str!("../../doc/IPC-PROTOCOL.md");
        assert!(
            DOC.len() > 10_000,
            "只读到 {} 字节的 IPC-PROTOCOL.md —— 本条在空转",
            DOC.len()
        );
        DOC
    }

    /// 文档里**当成一个词印出来**的那些 `sidecar_*`。
    ///
    /// 🔴 **匹配单位是「反引号包住的一整个词」，不是裸子串** —— 这一条是本轮实测逼出来的：
    /// 头一版按裸子串取，当场把文档里那句指路
    /// （`…/src/sidecar_fetch_guard.rs`，反引号里是一整条路径）读成了一个 code，
    /// 报出「文档里有一个代码产不出来的 code」。**那是假红，而假红会让人把判据关掉。**
    /// ⇒ 收窄成「反引号紧挨着 `sidecar_`」：表格里那些是这一形，路径不是。
    ///
    /// ⚠ 收窄之后漏的那一侧如实写：文档里**没加反引号**地提一个 code，本条看不见。
    /// 接住它的是另一半方向（代码里每个 code 都必须在文档里找得到，而找的也是这一形）。
    fn codes_in_doc() -> Vec<String> {
        const OPEN: &str = "`sidecar_";
        let mut out = Vec::new();
        let mut rest = doc();
        while let Some(i) = rest.find(OPEN) {
            let after = &rest[i + 1..];
            let end = match after.find('`') {
                Some(e) => e,
                None => break,
            };
            out.push(after[..end].to_string());
            rest = &after[end..];
        }
        out.sort();
        out.dedup();
        out
    }

    /// ⑤ 两个方向都对：代码里的每个 code 文档里都有，文档里的每个 code 代码里都产得出来。
    ///
    /// ⚠ 单向那一半买不到东西：只断「代码 ⊆ 文档」，文档里留一堆早就删掉的 code 也照样绿。
    #[test]
    fn the_failure_faces_and_the_protocol_doc_agree_in_both_directions() {
        let mut codes = face_codes();
        codes.sort();
        let in_doc = codes_in_doc();
        assert!(
            in_doc.len() >= 5,
            "文档里只取到 {} 个 code —— 取法收窄过头了，本条在空转：{in_doc:?}",
            in_doc.len()
        );
        let missing: Vec<&String> = codes.iter().filter(|c| !in_doc.contains(c)).collect();
        assert!(
            missing.is_empty(),
            "这些失败面没有落进 `doc/IPC-PROTOCOL.md`：{missing:?}\n\
             客户端要把 code 翻成人话，翻不出来的那几个对用户就是一句「未知错误」。"
        );
        let ghosts: Vec<&String> = in_doc.iter().filter(|c| !codes.contains(c)).collect();
        assert!(
            ghosts.is_empty(),
            "`doc/IPC-PROTOCOL.md` 里写着这些 code，而代码一个都产不出来：{ghosts:?}"
        );
    }

    /// `wire.rs` 里 `Unavailable` 那个结构的体。
    fn unavailable_struct_body() -> String {
        let src = production_code(include_str!("wire.rs"));
        let head = "pub struct Unavailable {";
        let at = src
            .find(head)
            .expect("`wire.rs` 生产段里找不到 `Unavailable` —— 锚点挪了");
        let body = &src[at + head.len()..];
        let end = body.find("\n}").expect("`Unavailable` 的体收不了尾");
        body[..end].to_string()
    }

    /// ⑥ 🔴 线上那一半**今天装不下那句话** —— 把这个现状钉成会红的东西。
    ///
    /// 它红 = 有人给 `wire::Unavailable` 加了字段。那时该做的**不是**改这条判据，
    /// 是回 `fetch::unavailable_for` 把「线上那一半」与「那一句话」合成一个值。
    #[test]
    fn the_wire_unavailable_still_cannot_carry_the_sentence() {
        let body = unavailable_struct_body();
        let fields: Vec<&str> = body
            .lines()
            .map(|l| l.trim())
            .filter(|l| l.starts_with("pub "))
            .collect();
        assert_eq!(
            fields.len(),
            2,
            "`wire::Unavailable` 的字段数变了：{fields:?}\n\
             R2 ④ 要的「明确的失败文案」在线上一直没有住址，本条盯的就是这件事。"
        );
        assert!(
            !body.contains("message"),
            "`wire::Unavailable` 长出了一个装人话的字段 —— 去 `fetch::unavailable_for` 把两半合起来"
        );
    }

    /// ⑦ 这一层今天**恰好 0 个生产调用点** —— 墓碑，不是缺口。
    ///
    /// 它红的那天就是接线那天；同轮要做的四件事逐条写在 `fetch.rs` 头注最后一节，
    /// 其中一件就是把 `sidecars/mod.rs` 那个 `allow(dead_code)` 摘掉。
    #[test]
    fn the_sidecar_layer_has_no_production_caller_yet() {
        let mut callers: Vec<String> = Vec::new();
        for (path, src) in guard_core::scan_tree!(&src_dir(), &["rs"]) {
            let rel = path
                .strip_prefix(src_dir())
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if rel.starts_with("sidecars/") {
                continue; // 层内自己引自己不算调用点
            }
            if production_code(&src).contains("sidecars::") {
                callers.push(rel);
            }
        }
        assert!(
            callers.is_empty(),
            "sidecar 那一层有生产调用点了：{callers:?}\n\
             这不是坏事 —— 是接线那一刻到了。同轮要做的四件事在 `fetch.rs` 头注最后一节。"
        );
    }
}
