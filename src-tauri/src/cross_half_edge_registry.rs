//! F20：**两半之间的编译期边**（`include_str!` / `include_bytes!` 跨到对面那一半）。
//!
//! # C2 的反面
//!
//! 定框 C2 说「backend 不是 monitor 的外部依赖，是 monitor 自己的一半」。
//! 它的**反面**是：两半不该在**编译期**互相咬住 —— 一半的源码布局变了，
//! 另一半就编不过，那两半其实是一个不可分割的整体，「一份代码两种承载」只是说法。
//!
//! # 摸底把这件的前提**证伪了一半**
//!
//! 08-04 架构重估报的是「10 条跨 crate `include_str!` 边，其中 daemon 读 monitor 那个
//! 60KB 平铺 `tmux.rs` ⇒ **拆那个文件会让 daemon 编不过**，而 daemon 自称可在目标机原生构建」。
//!
//! 逐条量 + 一次决定性实验（把两条 daemon→monitor 的路径打断，再分别跑两条命令）：
//!
//! | 命令 | 打断后 | 说明 |
//! |---|---|---|
//! | `cargo build` | **exit=0，0 error** | ★ **部署路径完全不受影响** —— 目标机原生构建走的就是这条 |
//! | `cargo test --no-run` | exit=101，两条边都被逐字点名 | 判据路径才咬住 |
//!
//! ⇒ **`10` 这个数对得上**（monitor→daemon 8 · daemon→monitor 2），
//! 但「编不过」说的是 **`cargo test`**，不是 `cargo build`。
//! daemon 的 `Cargo.toml` 逐字写的是「Standalone crate, intentionally NOT part of a
//! workspace」—— 那句**没有假**：它讲的是 workspace 成员身份与构建。
//! 真实的代价是另一句、而且**以前谁都没写下来**：
//! **daemon 的 `cargo test` 需要旁边那棵 `src-tauri/` 树在。**
//!
//! # 于是这件钉的不是「有几条边」，是**边只许长在判据里**
//!
//! 十条边全是**跨轨对拍**（一侧的判据去读另一侧的源码，断言两侧同形）——
//! 那是本仓最有价值的一类判据，砍掉它们等于砍掉「两侧不许偷偷漂开」这条保障。
//! 所以本模块**不减少边**，它钉两件事：
//!
//! 1. **每条边都要登记**，写清「谁读谁 · 为什么必须编译期读」（发现机制是**遍历两棵树**）；
//! 2. ★★ **没有一条边长在生产段** —— 那才是「部署路径不受影响」这句话的机检形态。
//!    生产段一旦出现跨半边的 `include_str!`，`cargo build` 就真的咬住了，本条当场红。
//!
//! ⚠ **刻意不做的一件**：不拆 monitor 那个 60KB 的 `tmux.rs`。
//! 「文件大」不是拆的理由 —— 那条边读它是为了找一个 12 字符的 needle
//! （`format!("={target}:")`，F01 那条「裸 `-t` 会打到兄弟会话上」的两侧同形）。
//! 真要拆，理由得是具体的架构病，不是行数。已进 `ROADMAP §5`。

/// 两半之间**每一条**编译期边的登记：`(方向, 读者文件, 被读的文件, 为什么必须编译期读)`。
///
/// # 为什么按「读者 → 被读」这一**对**做键，而不是行号
///
/// 行号是最易腐的键（`ssh_source.rs` 六千行，动一处上面全移位）。
/// 实测这十对**两两不同**，所以「文件对」是够用且稳定的键。
#[cfg(test)]
const CROSS_EDGES: &[(&str, &str, &str, &str)] = &[
    // ── monitor → daemon（8 条）：monitor 的判据去读 daemon 的源码 ────────────
    (
        "monitor→daemon",
        "src-tauri/src/backend/control/daemon_kill.rs",
        "remote-daemon-proto/src/control/kill.rs",
        "拒绝文案两侧逐字同形：daemon 那边改了措辞，monitor 的用户可见提示就跟着变",
    ),
    (
        "monitor→daemon",
        "src-tauri/src/backend/control/daemon_launch.rs",
        "remote-daemon-proto/src/inbound.rs",
        "本通道发的字段由 daemon 的登记表说了算 —— 读它才能断言两侧字段集一致",
    ),
    (
        "monitor→daemon",
        "src-tauri/src/backend/control/daemon_send_keys.rs",
        "remote-daemon-proto/src/control/launch.rs",
        "两个 mode 名必须是 daemon 真能 parse 的那两个（`parse_request` 不 deny unknown \
         fields ⇒ 打错字会被静默忽略、照样附 Enter）",
    ),
    (
        "monitor→daemon",
        "src-tauri/src/inbound_client.rs",
        "remote-daemon-proto/src/inbound.rs",
        "入方向帧的种类与错误码两侧同形",
    ),
    (
        "monitor→daemon",
        "src-tauri/src/inbound_client.rs",
        "remote-daemon-proto/src/control/launch.rs",
        "launch 请求的字段名两侧同形",
    ),
    (
        "monitor→daemon",
        "src-tauri/src/polling_registry.rs",
        "remote-daemon-proto/src/control/tmux_hook.rs",
        "C14 那条登记在案的例外（预信任的等信任框以 shell 字符串形态产出）真实存在的证据 —— \
         它是「零轮询」那条零命中守卫的反向锚点",
    ),
    (
        "monitor→daemon",
        "src-tauri/src/ssh_source.rs",
        "remote-daemon-proto/src/main.rs",
        "daemon 的启动契约（身份清单 / hello）两侧同形",
    ),
    (
        "monitor→daemon",
        "src-tauri/src/ssh_source.rs",
        "remote-daemon-proto/src/wire.rs",
        "wire 帧的形状两侧同形",
    ),
    // ── daemon → monitor（2 条）：daemon 的判据去读 monitor ────────────────────
    (
        "daemon→monitor",
        "remote-daemon-proto/src/control/gate.rs",
        "src-tauri/src/backend/control/fixtures/gate2-golden.tsv",
        "§34 Gate 2 的黄金夹具**只有一个家**（定框 §4：同一个数不许两侧各写一份）—— \
         daemon 与 monitor 各自独立读同一张表",
    ),
    (
        "daemon→monitor",
        "remote-daemon-proto/src/control/launch.rs",
        "src-tauri/src/tmux.rs",
        "★ 跨轨对拍：`format!(\"={target}:\")` 这个精确匹配形状两侧必须同形 —— \
         F01 实测过，一边写裸 `-t` 就会打到兄弟会话上，而另一边不会，排查极难",
    ),
];

#[cfg(test)]
mod tests {

    use super::CROSS_EDGES;
    use std::path::{Path, PathBuf};

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .to_path_buf()
    }

    /// 两棵树里所有 `.rs`（相对仓根，`/` 分隔）。
    fn both_halves() -> Vec<String> {
        let root = repo_root();
        let mut out = Vec::new();
        for sub in ["src-tauri/src", "remote-daemon-proto/src"] {
            let mut stack = vec![root.join(sub)];
            while let Some(d) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&d) else {
                    continue;
                };
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if p.extension().is_some_and(|x| x == "rs") {
                        out.push(
                            p.strip_prefix(&root)
                                .unwrap_or(&p)
                                .to_string_lossy()
                                .replace('\\', "/"),
                        );
                    }
                }
            }
        }
        out.sort();
        out
    }

    /// 哪一半：`monitor` / `daemon` / 其它（`doc/` `shared/` `e2e/` `src/` …）。
    ///
    /// ⚠ 只有 monitor ↔ daemon 这两个方向算「两半互相咬」。
    /// 读 `doc/` `shared/` `e2e/` 与前端 `src/*.ts` 的边**另有 13 处**，它们不是这件的标的 ——
    /// 那些是「判据去读文档/脚本/前端源」，两半的关系不在其中。
    /// ⚠ 这条口径是**量出来的**：全仓 123 处 `include_*!`，跨侧 24 处，
    /// 其中 monitor↔daemon 恰好 **8 + 2**。别把 24 当成这件的数。
    fn half_of(rel: &str) -> &'static str {
        if rel.starts_with("src-tauri/") {
            "monitor"
        } else if rel.starts_with("remote-daemon-proto/") {
            "daemon"
        } else {
            "外部"
        }
    }

    /// 一个文件里的所有 `include_str!` / `include_bytes!` 目标（原样字面量 + 归一化后的相对仓根路径）。
    fn includes_in(rel: &str) -> Vec<(String, String)> {
        let root = repo_root();
        let raw = std::fs::read_to_string(root.join(rel)).unwrap_or_default();
        includes_of(rel, &raw)
    }

    /// 同上，但只看**给定文本**（生产段那条断言要在剥完的切片上跑）。
    fn includes_in_text(text: &str) -> Vec<(String, String)> {
        includes_of("", text)
    }

    fn includes_of(rel: &str, raw: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        // 运行时拼，免得命中本文件自己的说明文字。
        //
        // ⚠ **不许要求 `!(` 与 `"` 连着。** 第一版要求连着，于是 **rustfmt 折行的那两条边
        // 直接消失了**（`daemon_kill.rs` 与 `daemon_send_keys.rs` 的路径太长，被折成
        // `include_str!(\n    "…"\n)`）—— 抽取器读出 8 条，真值 10 条。
        // 那个洞**只被「条数自检」那一条断言逮住**，别的断言都会跟着一起错得很一致。
        for verb in [
            format!("include_{}!", "str"),
            format!("include_{}!", "bytes"),
        ] {
            let mut from = 0usize;
            while let Some(at) = raw[from..].find(verb.as_str()) {
                let mut s = from + at + verb.len();
                // 跳过 `(` 与任意空白/换行，落到那个字面量的第一个引号上。
                let b = raw.as_bytes();
                while s < b.len() && (b[s] as char).is_whitespace() {
                    s += 1;
                }
                if s < b.len() && b[s] == b'(' {
                    s += 1;
                }
                while s < b.len() && (b[s] as char).is_whitespace() {
                    s += 1;
                }
                if s >= b.len() || b[s] != b'"' {
                    from = at + from + verb.len();
                    continue;
                }
                s += 1;
                let Some(len) = raw[s..].find('"') else { break };
                let lit = &raw[s..s + len];
                let dir = Path::new(rel).parent().unwrap_or(Path::new(""));
                let joined = dir.join(lit).to_string_lossy().replace('\\', "/");
                // 手工归一化（`..` 不碰文件系统，符号链接在这仓里不存在）。
                let mut parts: Vec<&str> = Vec::new();
                for c in joined.split('/') {
                    match c {
                        "." | "" => {}
                        ".." => {
                            parts.pop();
                        }
                        other => parts.push(other),
                    }
                }
                out.push((lit.to_string(), parts.join("/")));
                from = s + len;
            }
        }
        out
    }

    /// 遍历两棵树，找出所有**跨半边**的编译期边：`(方向, 读者, 被读)`。
    fn discovered_edges() -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        for rel in both_halves() {
            let mine = half_of(&rel);
            for (_, target) in includes_in(&rel) {
                let theirs = half_of(&target);
                if theirs != mine && theirs != "外部" && mine != "外部" {
                    out.push((format!("{mine}→{theirs}"), rel.clone(), target));
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// ★ 抽取器自检：遍历与解析都没坏。坏了的话下面两条会零命中地绿。
    ///
    /// ⚠ 中间量都断言：**扫到几个文件** · **看见几处 `include_*!`** · **跨侧几处**。
    /// 只比最终数是不够的 —— F+08 那轮四个抽取器出错，全是中间量没人看。
    #[test]
    fn the_edge_scan_sees_both_trees_and_actually_parses() {
        let files = both_halves();
        let monitor = files.iter().filter(|f| half_of(f) == "monitor").count();
        let daemon = files.iter().filter(|f| half_of(f) == "daemon").count();
        // 实测（F20 摸底）：monitor 侧 77 个 `.rs`、daemon 侧 37 个。
        assert!(
            monitor >= 70 && daemon >= 30,
            "只扫到 monitor {monitor} 个 / daemon {daemon} 个 `.rs` —— 遍历坏了\
             （摸底实测 77 / 37）"
        );
        let total: usize = files.iter().map(|f| includes_in(f).len()).sum();
        assert!(
            total >= 90,
            "两棵树里只解析出 {total} 处 `include_*!` —— 解析坏了（摸底实测全仓 123 处，\
             其中这两棵树占绝大多数）"
        );
        let n = discovered_edges().len();
        assert_eq!(
            n,
            CROSS_EDGES.len(),
            "跨半边的编译期边**条数**变了（实得 {n}，登记 {}）：\n{:#?}\n\
             ⚠ 别改这个数了事：新增一条就把它登记进 `CROSS_EDGES` 并写清\
             「为什么必须编译期读」；少一条说明有判据被删了或路径改了。",
            CROSS_EDGES.len(),
            discovered_edges()
        );
    }

    /// ★★ 遍历发现的边 == 登记表，**两个方向都查**。
    #[test]
    fn every_cross_half_edge_is_registered_with_a_reason() {
        let mut found: Vec<(String, String, String)> = discovered_edges();
        let mut registered: Vec<(String, String, String)> = CROSS_EDGES
            .iter()
            .map(|(d, r, t, _)| (d.to_string(), r.to_string(), t.to_string()))
            .collect();
        found.sort();
        registered.sort();
        assert_eq!(
            found, registered,
            "两半之间的编译期边与登记表对不上。\n\
             多出来的边请登记进 `CROSS_EDGES`，第四列写清**为什么必须编译期读**\n\
             （「跨轨对拍：两侧必须同形」是正当理由；「顺手方便」不是）；\n\
             登记表里多出来的说明那条边被删了或路径改了。"
        );
        for (d, r, t, why) in CROSS_EDGES {
            assert!(
                why.chars().count() >= 12,
                "{d} {r} → {t} 的理由太短（{why:?}）—— 这一列的读者是下一个想加边的人"
            );
        }
    }

    /// ★★★ **本件真正的那条**：没有一条跨半边的边长在**生产段**。
    ///
    /// 这是「部署路径不受影响」那句话的机检形态：
    /// 十条边全在判据里 ⇒ `cargo build`（目标机原生构建走的就是它）看不见它们，
    /// 摸底那次决定性实验实测过 —— 把两条 daemon→monitor 的路径打断，
    /// `cargo build` 仍然 **exit=0 / 0 error**，只有 `cargo test --no-run` 炸。
    ///
    /// 一旦有人把跨半边的 `include_str!` 写进生产段，两半就**真的**在编译期咬住了
    /// （daemon 的部署构建从此需要 monitor 那棵树），本条当场红。
    ///
    /// ⚠ 用的是仓里那把尺子 `guard_core::production_code`，不自己手搓
    /// （F+08 的教训：手搓的粗切在 `lib.rs` 第 57 行砍掉了 2108 行）。
    /// ⚠ 中间量自检：生产段剥完必须还有东西（否则这条零命中地绿），
    /// 且原文里必须真能看见那个 `include_*!`（否则是读错文件了）。
    #[test]
    fn no_cross_half_edge_lives_in_production_code() {
        let root = repo_root();
        let verb = format!("include_{}!", "str");
        let mut offenders = Vec::new();
        // ⚠ **剥法健康是个「全局」性质，不是逐文件性质。**
        // 第一版逐文件断言「生产段 > 200 字节」，而 `polling_registry.rs` 的生产段
        // 实测 **0 字节** —— 那不是剥法坏了，是那个模块**整体就是判据**
        // （`lib.rs` 里它的 `mod` 前面带着 `#[cfg(test)]`，与 `parity_ledger` 同族）。
        // 0 字节在那儿恰恰是「这条边不在生产段」最强的证明。
        // ⇒ 改成数「有几个读者有非空生产段」：实测 10 个读者里 **9 个**有
        //（只有 `polling_registry.rs` 整体是判据）。地板留一格余量。
        let mut with_production = 0usize;
        for (dir, reader, target, _) in CROSS_EDGES {
            let raw = std::fs::read_to_string(root.join(reader)).unwrap_or_default();
            assert!(
                raw.len() > 500,
                "读 {reader} 只拿到 {} 字节 —— 读不到的文件只会静默返回空串",
                raw.len()
            );
            let prod = guard_core::production_code(&raw);
            guard_core::assert_no_test_code(reader, &prod);
            if prod.len() > 200 {
                with_production += 1;
            }
            // 中间量：原文里真的看得见这条边（不然是路径/文件对错了）。
            let leaf = Path::new(target)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            assert!(
                raw.contains(&leaf) && raw.contains(verb.as_str()),
                "{reader} 里看不到 `{leaf}` 或任何 `include_*!` —— 登记表与现实漂了"
            );
            // 真正的断言：生产段里不许出现指向对面那一半的 include。
            for (lit, resolved) in includes_in(reader) {
                // ⚠ 同一个坑的第二处：生产段那条断言也不能要求连写 ⇒
                // 改成「生产段里存在一处 include，且它的字面量就是这一条」。
                let in_prod = includes_in_text(&prod).iter().any(|(l, _)| *l == lit);
                if resolved == *target && in_prod {
                    offenders.push(format!("  [{dir}] {reader} 的**生产段**读 {target}"));
                }
            }
        }
        assert!(
            with_production >= 8,
            "十个读者里只有 {with_production} 个有非空生产段 —— \
             `guard_core::production_code` 多半坏了，此刻本条在拿空串做零命中\
             （实测 9 个：只有 `polling_registry.rs` 整体是判据模块）"
        );
        assert!(
            offenders.is_empty(),
            "跨半边的编译期边长进了生产段：\n{}\n\
             ⇒ 从此 daemon 的部署构建（`cargo build`）需要 monitor 那棵树在，\
             而它的 `Cargo.toml` 逐字写着 standalone。\n\
             正解：把这次对拍搬进 `#[cfg(test)]`；真需要在运行期拿到那份内容，\
             就把它做成协议字段或夹具，别做成编译期依赖。",
            offenders.join("\n")
        );
    }
}
