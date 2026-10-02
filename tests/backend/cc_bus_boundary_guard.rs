//! `P4f-Y2`〔用@08-13「后面我可能要改ccbus」〕：**backend 不许碰 cc-bus 的数据布局**。
//!
//! # 为什么单独一个模块
//!
//! 判据要扫的**最要紧的那个文件正是 `control/cc_bus.rs`** —— 真有人绕到 cc-bus 背后
//! 读文件，第一个下手的地方就是那儿。而 `scanning_guard_registry` 要求扫描型判据走
//! `guard_core::scan_tree!`，那条纪律治的是「判据在自己的语料里找到自己 ⇒ 恒绿」
//! 那一族（audit-0805 实测五次）。
//!
//! ⚠ 先前这里写着「它**按构造摘掉调用者自己**…自排除仍然成立」。
//! 那一刀**在这一处不生效**：判据一律由 `#[path]` 挂进生产树 ⇒ `file!()` 是带 `..`
//! 的折返路径 ⇒ 后缀比不命中
//! （`the_scan_tree_macro_no_longer_excludes_its_caller_after_the_split` 守着这件事）。
//! ⇒ 判据搬来这里买到的**不是**「自排除仍然成立」，而是一件更硬的事：本文件住
//! `tests/backend/`、**根本不在被扫的那棵树里**，而覆盖面同时全了。
//!
//! # 允许什么、禁什么
//!
//! · **允许**「命令的地址」：`~/.local/bin` / `skills/cc-bus/scripts` 那条查找规则 ——
//!   那是接口的门牌号，不是数据布局；
//! · **禁**数据文件：地址簿 / 收件箱 / 已读位置 / 台账。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
mod tests {
    #[test]
    fn no_cc_bus_data_layout_leaks_into_the_backend() {
        // ★ `scan_tree!` 而不是自己 `read_dir`：`scanning_guard_registry` 逼的
        //（治「判据在自己的语料里找到自己 ⇒ 恒绿」那族）。
        // ⚠ 宏自称的那一刀「摘除**调用者自己**那一份」今天**不生效**（理由见模块头注）。
        // ⇒ 但下面这条结论**仍然成立**：本判据不能住在 `control/cc_bus.rs` 的
        //   `#[cfg(test)]` 段里 —— 那种**不经 `#[path]`** 的测试模块，`file!()` 给的
        //   就是 `control/cc_bus.rs` 本身，后缀比**会命中** ⇒ 最该被扫的那份被摘走。
        let src_dir = crate::guard_support::src_root();
        let files: Vec<(String, String)> = guard_core::scan_tree!(&src_dir, &["rs"])
            .into_iter()
            .map(|(p, s)| {
                let rel = p
                    .strip_prefix(&src_dir)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel, s)
            })
            .collect();
        assert!(
            files.len() >= 20,
            "只遍历到 {} 个源文件 —— 遍历坏了，本断言在空转",
            files.len()
        );
        // ⚠ **针要专有**〔08-13 当场踩到〕：第一版把 `.jsonl` 也列进来了，结果打中
        //   `history_query.rs` / `fork_write.rs` —— 那两处读的是 **Claude 自己的转录**，
        //   与 cc-bus 毫无关系。判据一旦误伤，它的诊断文案（「碰了 cc-bus 的数据布局」）
        //   就成了假话，而假话比没有判据更坏。⇒ 只留 cc-bus **专有**的名字。
        let needles = [
            format!("agents{}tsv", "."),
            format!("spawned{}tsv", "."),
            format!("{}cc-bus", "."),
            format!("cc-bus{}inbox", "/"),
            format!("lastread{}", "-"),
        ];
        // 〔待定〕足迹申报表随「一处后端」进了后端：cc-bus 那一行申报「这个目录会因为你在
        //   cc-monitor 里点了一下而被 cc-bus 的命令写」（`IndirectWrite`）。它只 stat 那个目录、数一层名字，一个数据文件都不读 ——
        //   是**告知**，不是绕到 cc-bus 背后读数据。在 monitor 那一半时这一格本来就在（它不在本判据射程里）。
        const DECLARED_NOT_READ: &[(&str, &str)] = &[("agents/claudecode/footprint.rs", ".cc-bus")];
        let mut hits: Vec<String> = Vec::new();
        for (name, raw) in &files {
            let prod = crate::guard_support::production_code(raw);
            for n in &needles {
                if DECLARED_NOT_READ.iter().any(|(f, k)| f == name && k == n) {
                    assert_eq!(
                        prod.matches(n.as_str()).count(),
                        1,
                        "{name} 里 `{n}` 不再恰好是足迹申报那一处 —— 多出来的那处是在读 cc-bus 的数据"
                    );
                    continue;
                }
                if prod.contains(n.as_str()) {
                    hits.push(format!("{name} 里有 `{n}`"));
                }
            }
        }
        assert!(
            hits.is_empty(),
            "backend 的生产段碰了 cc-bus 的**数据布局**：{hits:?}\n\
             用户 08-13 逐字说过「后面我可能要改ccbus」⇒ 这一层只许把 cc-bus 的**命令**当接口。\n\
             读文件确实更快、还省一个进程 —— 代价是他改 cc-bus 的那天这里会静悄悄地错。\n\
             要拿的东西命令给不出来时，正确做法是**给 cc-bus 加一条命令**，不是绕到它背后读文件。"
        );
    }
}
