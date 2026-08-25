//! `K9` 裁定四第 2 个已知坑的零命中守卫：**两个方向的 Nagle 都必须在生产段关掉。**
//!
//! # 为什么要它（D1 `重要-5`）
//!
//! `upstream.rs` 的注释逐字引着参考实现那条读数：「没关会让 p95 塌到 **3504ms**」——
//! 而实测把 `server.rs` 与 `upstream.rs` 那两处 `set_nodelay(true)` **各自**改成 `false`，
//! **384 条判据两趟都全绿**（审计 `CG3`/`CG4`）。⇒ **注释里承诺、判据里没有。**
//! 一条零命中守卫就够，比测 p95 便宜得多（p95 那条要真流量，本轮禁打真 API）。
//!
//! # 为什么它单住一个文件
//!
//! 与 `bind_guard` 同一条理由：`guard_core::scan_tree!` **按构造摘掉调用者自己那份**，
//! 治的是「判据在自己的源码里找到自己 ⇒ 恒绿」那一族。判据与被扫的代码不能同住一个文件。
//! 而它要扫的正是 `server.rs` 与 `upstream.rs` 两个文件 ⇒ 只能住第三个。
//!
//! # 它哪天会变瞎（照实写，别让名字承诺它没有的覆盖）
//!
//! - 它是**源码扫描**：`set_nodelay` 的参数一旦是**变量**（`set_nodelay(flag)`），两个 needle 都看不见。
//!   〔这一形是从 needle 表推的，**我没打过这一刀** —— 与 `bind_guard` 头注先前那句「只有拼出来的
//!    才看不见」栽的是同一个坑，所以这里逐字写明射程。〕
//! - 它只数**处数与落点文件**，**不证明**这两个调用真的执行到了（那要行为验），
//!   更**不证明** p95 没塌 —— p95 那条今天仍然没有任何判据（件文件 `判不了-4`）。

#[cfg(test)]
mod tests {
    #[test]
    fn both_directions_disable_nagle_in_relay_production_code() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/relay");
        // 针**运行时拼**：直接写字面量的话本文件自己就是命中源
        //（`scan_tree!` 已经摘掉了本文件 —— 两道保险，别只靠一道）。
        let on = format!("set_nodelay({})", true);
        let off = format!("set_nodelay({})", false);
        let files = guard_core::scan_tree!(&dir, &["rs"]);
        assert!(
            files.len() >= 5,
            "只扫到 {} 个文件 —— 取法坏了，本断言在空转",
            files.len()
        );
        let mut on_hits: Vec<String> = Vec::new();
        let mut off_hits: Vec<String> = Vec::new();
        for (path, raw) in &files {
            let prod = crate::guard_support::production_code(raw);
            for (no, line) in prod.lines().enumerate() {
                let at = format!("{}:{}: {}", path.display(), no + 1, line.trim());
                if line.contains(on.as_str()) {
                    on_hits.push(at.clone());
                }
                if line.contains(off.as_str()) {
                    off_hits.push(at);
                }
            }
        }
        assert!(
            off_hits.is_empty(),
            "中转生产段出现关不掉 Nagle 的写法：{off_hits:?}"
        );
        // 恰好两处 —— **两个方向各一处**。只断「>= 1」会被「删掉上游那一处」骗过去。
        assert_eq!(
            on_hits.len(),
            2,
            "中转生产段应当恰好两处关 Nagle（下游一处 + 上游一处），实际：{on_hits:?}"
        );
        // 只数总数会被「同一个文件里写两遍」骗过 ⇒ 落点也要断。
        assert!(
            on_hits.iter().any(|h| h.contains("server.rs")),
            "下游方向（server.rs）那一处不见了：{on_hits:?}"
        );
        assert!(
            on_hits.iter().any(|h| h.contains("upstream.rs")),
            "上游方向（upstream.rs）那一处不见了：{on_hits:?}"
        );
    }
}
