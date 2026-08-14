//! `P4f-Y2`〔用@08-13「后面我可能要改ccbus」〕：**daemon 不许碰 cc-bus 的数据布局**。
//!
//! # 为什么单独一个模块
//!
//! 判据要扫的**最要紧的那个文件正是 `control/cc_bus.rs`** —— 真有人绕到 cc-bus 背后
//! 读文件，第一个下手的地方就是那儿。而 `scanning_guard_registry` 要求扫描型判据走
//! `guard_core::scan_tree!`，它**按构造摘掉调用者自己**（治「判据在自己的语料里
//! 找到自己 ⇒ 恒绿」那一族，audit-0805 实测五次）。
//!
//! 两条要求撞在一起 ⇒ 判据搬来这里：自排除仍然成立，而覆盖面**反而全了**。
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
    fn no_cc_bus_data_layout_leaks_into_the_daemon() {
        // ★ `scan_tree!` 而不是自己 `read_dir`：它按构造摘除**调用者自己**那一份
        //（`scanning_guard_registry` 逼的，治「判据在自己的语料里找到自己 ⇒ 恒绿」那族）。
        // ⇒ 本判据因此**不能**住在 `control/cc_bus.rs` 里 —— 那正是最该被扫的文件。
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
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
        let mut hits: Vec<String> = Vec::new();
        for (name, raw) in &files {
            let prod = crate::guard_support::production_code(raw);
            for n in &needles {
                if prod.contains(n.as_str()) {
                    hits.push(format!("{name} 里有 `{n}`"));
                }
            }
        }
        assert!(
            hits.is_empty(),
            "daemon 的生产段碰了 cc-bus 的**数据布局**：{hits:?}\n\
             用户 08-13 逐字说过「后面我可能要改ccbus」⇒ 这一层只许把 cc-bus 的**命令**当接口。\n\
             读文件确实更快、还省一个进程 —— 代价是他改 cc-bus 的那天这里会静悄悄地错。\n\
             要拿的东西命令给不出来时，正确做法是**给 cc-bus 加一条命令**，不是绕到它背后读文件。"
        );
    }
}
