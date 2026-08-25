//! `K9` 裁定四第 2 个已知坑的**静态**半：中转生产段里**不许**出现「把 Nagle 打开」的写法。
//!
//! # 分工（这一条与行为那一条各买什么，别混着读）
//!
//! - **行为那一半**住 `server.rs::both_directions_really_disable_nagle_on_the_socket`：
//!   它用 `try_clone()`（= `dup`，两个 fd 同一条 socket）在**真的跑过一遍生产段之后**
//!   `getsockopt` 读 `TCP_NODELAY`，**两个方向各一格、各带非空对照**。
//!   「两个方向都真的关了 Nagle」这句承诺由**它**背书。
//! - **本文件这一条**只买一样东西：`relay/` 生产段里**零处**把 Nagle 打开的写法
//!   （`set_nodelay(false)`）。它是源码扫描，买的是「全目录、包括还没有行为判据的新代码」。
//!
//! # ⚠ 订正〔回修轮之四 08-25，D2 `重要-1(D2)` / `重要-6(D2)`〕：先前这一条把话说大了两格
//!
//! 先前它叫 `both_directions_disable_nagle_in_relay_production_code`，
//! 断的是「`set_nodelay(true)` **恰好两处** + 落点里要同时有 `server.rs` 与 `upstream.rs`」。
//! 两条实测把它钉死了：
//!
//! 1. ★ **它数的是文本，不是调用** —— `production_code()` 只剥 `#[cfg(test)]` 段与**行首**
//!    `//` 的行；字符串字面量 / 行尾注释 / 块注释里的同形文本**照样被数进去**。
//!    把 `handle` 里真的 `down.set_nodelay(true)?;` **整个删掉**、只留
//!    `let _nagle_note = "set_nodelay(true)";` ⇒ 那两格**都被这一行喂饱**，
//!    **389 条判据全绿**（D2 `D2NG1`；本轮重打，见件文件 §8.18.2）。
//!    ⇒ 名字承诺的「两个方向都关了 Nagle」，它证不了。
//! 2. ★ **「参数是变量」那一形先前被写反了** —— 头注原话写在标题「它哪天会**变瞎**」底下，
//!    读起来是「有违规它会漏掉（假绿）」。实测相反：`let nd = true; set_nodelay(nd)`
//!    是一次**行为完全正确**的重构，而它当场**红**（`left: 1` / `right: 2`，D2 `D2NG2`；
//!    本轮重打，读数同）。⇒ 那一形不是**变瞎**，是**变吵** —— 假阳性，不是假绿。
//!
//! ⇒ 处置：「恰好两处 + 落点」那两格**整个搬到行为判据上**（它没有这两个瞎点，也没有那个假阳性），
//! 本文件只留下**零处 `set_nodelay(false)`** 这一格，名字也跟着收窄。
//!
//! # 它今天的射程（照实写，别让名字承诺它没有的覆盖）
//!
//! - 它仍然是**源码扫描**：字符串字面量 / 行尾 `//` 注释 / `/* */` 块注释里写
//!   `set_nodelay(false)` 会让它**红**。⇒ 这个方向是**误报**，不是假绿 —— 失败是安全的那一侧。
//!   〔三形里我打了字符串字面量那一形（见 §8.18.2）；另两形是从 `production_code()`
//!    的实现推的（它只滤 `l.trim_start().starts_with("//")`），**我没打**。〕
//! - 参数是变量（`set_nodelay(flag)`）时它看不见 —— 今天这**不再**是漏洞：
//!   「关没关」由行为那一条判，本条只管「有没有人显式打开」。
//! - 它**不证明** p95 没塌 —— p95 那条今天仍然没有任何判据（件文件 `判不了-4`）。
//!
//! # 为什么它单住一个文件
//!
//! 与 `bind_guard` 同一条理由：`guard_core::scan_tree!` **按构造摘掉调用者自己那份**，
//! 治的是「判据在自己的源码里找到自己 ⇒ 恒绿」那一族。判据与被扫的代码不能同住一个文件。
//! 而它要扫的正是 `server.rs` 与 `upstream.rs` 两个文件 ⇒ 只能住第三个。

#[cfg(test)]
mod tests {
    #[test]
    fn no_relay_production_code_turns_nagle_back_on() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/relay");
        // 针**运行时拼**：直接写字面量的话本文件自己就是命中源
        //（`scan_tree!` 已经摘掉了本文件 —— 两道保险，别只靠一道）。
        let off = format!("set_nodelay({})", false);
        let files = guard_core::scan_tree!(&dir, &["rs"]);
        assert!(
            files.len() >= 5,
            "只扫到 {} 个文件 —— 取法坏了，本断言在空转",
            files.len()
        );
        let mut off_hits: Vec<String> = Vec::new();
        // 非空对照：这把尺子在**同一趟**里量得到东西 —— 不然 `off_hits` 为空
        // 只说明「命令没跑」而不是「跑了结果是空」〔`brief` 12〕。
        let on = format!("set_nodelay({})", true);
        let mut on_hits = 0usize;
        for (path, raw) in &files {
            let prod = crate::guard_support::production_code(raw);
            for (no, line) in prod.lines().enumerate() {
                if line.contains(off.as_str()) {
                    off_hits.push(format!("{}:{}: {}", path.display(), no + 1, line.trim()));
                }
                if line.contains(on.as_str()) {
                    on_hits += 1;
                }
            }
        }
        assert!(
            on_hits >= 1,
            "非空对照：这把尺子今天在 relay/ 生产段里一处 `set_nodelay` 都量不到 —— 取法坏了"
        );
        assert!(
            off_hits.is_empty(),
            "中转生产段出现把 Nagle 打开的写法：{off_hits:?}"
        );
    }
}
