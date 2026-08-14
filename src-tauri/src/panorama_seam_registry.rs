//! `P7c-2` 第一刀：**让「索引引擎住哪一侧」变成可换的**〔用@08-13〕。
//!
//! # 它守的是用户那条约束
//!
//! 〔用 08-13〕逐字：「**此外daemon的实现记得解耦清晰. 如果以后要改索引方式以及解析方式
//! 或者添加新语言才方便**」。
//!
//! `P7c-2 §1b` 量完之后这条被**缩窄过**，如实记着：三轴里只有**索引方式**（查询面）
//! 归我们管；解析方式与加新语言住在 `vendor/code-picture-core`，而 `C7` 逐字
//! 「**vendor code-picture-core 不动**」+ vendor 自己的 SS-10 铁律「副本是上游的镜子，
//! 不是分身」⇒ 那两轴的可改性由「改上游 → re-vendor」的流程决定，不由这一层的接口形状决定。
//!
//! ⇒ 本模块只钉**我们真管得着的那一轴**，且钉的是两条**今天已经成立**的性质
//! （`P7c-2 §4` 的读数）——第一刀因此不是重构，是**把成立的东西变成会红的判据**。
//!
//! # 两条性质
//!
//! ① **命令面只暴露查询语义**：那 20 多条 tauri 命令的签名里不许出现
//!    存储 / grammar / 解析开关这类实现细节。换引擎那天，协议一个字不用动。
//! ② **引擎取用口恰好一处**：`Engine::open` 在生产段恰好一次。
//!    换侧（内嵌 / 侧车 / 编进 daemon）要改的就是那一处。
//!
//! ★ ② **今天就在防一件事**，不只是为了以后：`panorama.rs` 自己的注释记着
//! 「rusqlite 连接对同一 `index.db` 并发写 → `SQLITE_BUSY` + 缓存不一致」。
//! **第二处 `Engine::open` = 第二条连接。**
//!
//! # ⚠ 只扫签名，不扫注释
//!
//! 注释里写清楚「底下是 SQLite / 走 tree-sitter」是**好事** —— 那是给人看的实现说明。
//! 泄漏指的是**接口形状**里出现实现细节。⇒ 判据先剥注释再看。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
mod tests {
    const PANORAMA: &str = include_str!("panorama.rs");

    /// 只留生产段（剥 `//` 注释与 `#[cfg(test)] mod`）。
    fn prod() -> String {
        guard_core::production_code(PANORAMA)
    }

    /// 取所有 tauri 命令属性之后那条函数签名的**文本**（到第一个 `{` 为止）。
    ///
    /// ⚠ 取的是**签名**不是整个函数体：函数体里出现 `SELECT` 之类的字样，
    /// 说明这一层在自己拼 SQL —— 那是另一条病（今天不存在），
    /// 而本条钉的是**接口形状**。两件事不要混在一条判据里。
    /// tauri 命令属性的字面写法 —— **运行时拼**。
    ///
    /// ⚠ 血的教训（08-13 当场踩到，本会话第三次同族）：这个字面量直接写在本文件里，
    /// 会**污染另一条判据的语料** —— `src/ipc/commands.vitest.ts` 扫全仓 `.rs` 找
    /// 「属性 + 紧随其后的 fn 名」当命令名，于是把**我的测试函数名**抠成了一条命令，
    /// 报「声明了却没注册 ⇒ 前端调不到」。判据的针不只会读到自己，**还会喂给别人**。
    fn cmd_attr() -> String {
        format!("#[tauri::{}]", "command")
    }

    fn command_signatures(prod: &str) -> Vec<String> {
        let attr = cmd_attr();
        let mut out = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = prod[from..].find(attr.as_str()) {
            let at = from + rel;
            let tail = &prod[at..];
            let brace = tail.find('{').unwrap_or(tail.len());
            out.push(tail[..brace].to_string());
            from = at + attr.len();
        }
        out
    }

    /// `P7c2-Y1`：**命令面只暴露查询语义**。
    #[test]
    fn the_panorama_command_surface_leaks_no_storage_or_parser_detail() {
        let prod = prod();
        let sigs = command_signatures(&prod);
        // 反向自检：抽取坏了的话下面的空集会"恰好通过"。
        assert!(
            sigs.len() >= 20,
            "只抽到 {} 条命令签名 —— 抽取坏了，本断言在空转",
            sigs.len()
        );
        // ⚠ 针**运行时拼**：本文件的头注里就写着这些词，写字面量会读到判据自己
        //（本仓栽过四次，见 `P4b §6` 那几条头注）。
        let banned = [
            format!("sql{}", "ite"),
            format!("SEL{}", "ECT"),
            format!("rus{}qlite", ""),
            format!("tree{}sitter", "_"),
            format!("gram{}mar", ""),
            format!("index{}db", "."),
        ];
        let mut hits: Vec<String> = Vec::new();
        for sig in &sigs {
            let low = sig.to_lowercase();
            for b in &banned {
                if low.contains(&b.to_lowercase()) {
                    hits.push(format!("{b} 出现在签名：{}", sig.replace('\n', " ").trim()));
                }
            }
        }
        assert!(
            hits.is_empty(),
            "全景的命令面泄漏了实现细节：{hits:?}\n\
             ⇒ 用户 08-13 逐字要的是「以后要改索引方式…才方便」。协议里一旦出现存储/解析细节，\n\
             换引擎那天就要改协议，而协议的对面是**别人的代码**。\n\
             要拿的东西查询语义表达不了时，正确做法是**加一条查询语义的命令**，\n\
             不是把底下的实现漏上来。"
        );
    }

    /// `P7c2-Y1` 的另一半：**新增命令必须来这里被看一眼**。
    ///
    /// 禁词表挡不住「用一个中性名字包装一个泄漏的口」（如 `panorama_raw_query`）。
    /// 条数钉住之后，加命令的人必然会撞到本条，那时才有机会问一句「它是查询语义吗」。
    #[test]
    fn adding_a_panorama_command_forces_a_look_at_this_seam() {
        // ⚠ **21 不是 22**：裸 grep 数到 22，其中一处命令属性写在注释里
        //（`production_code` 剥掉了它）。判据数的是**生产段**，两个数不一样是对的。
        const COMMANDS_TODAY: usize = 21;
        let n = prod().matches(cmd_attr().as_str()).count();
        assert_eq!(
            n, COMMANDS_TODAY,
            "全景命令数从 {COMMANDS_TODAY} 变成了 {n}。\n\
             **这不是要你改数字了事** —— 先回答：新加的那条是**查询语义**吗？\n\
             （`overview`/`node`/`callers`/`impact` 那样，说的是「代码里有什么」，\n\
             而不是「存储里怎么放的」。）是，就把数字改了；不是，就换个问法。"
        );
    }

    /// `P7c2-Y2`：**引擎取用口恰好一处**。
    #[test]
    fn the_engine_is_opened_in_exactly_one_place() {
        let prod = prod();
        let needle = format!("Engine{}open", "::");
        guard_core::find_pinned(&prod, &needle).unwrap_or_else(|e| {
            panic!(
                "`{needle}` 在生产段不是恰好一处：{e}\n\
                 ⇒ 第二处 = 第二条 rusqlite 连接。`panorama.rs` 自己的注释记着那条真事故：\n\
                 「对同一 index.db 并发写 → SQLITE_BUSY + 缓存不一致」。\n\
                 而且换引擎（内嵌 / 侧车 / 编进 daemon）那天要改的就是这一处 —— 多一处多一份漏改。"
            )
        });
    }

    /// `P7c2-Y2` 的射程：vendor 引擎**只被 `panorama.rs` 导入**。
    ///
    /// 上一条只看 `panorama.rs` 自己。若别的模块也拿到 `Engine`，
    /// 「恰好一处」就只是本文件内的局部真理。
    #[test]
    fn the_engine_type_does_not_escape_the_panorama_module() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut elsewhere: Vec<String> = Vec::new();
        for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if rel == "panorama.rs" {
                continue;
            }
            let p = guard_core::production_code(&src);
            // ⚠ 钉的是**导入**，不是"文件里出现过 Engine 这个词"。
            //   第一版用整词匹配 `Engine`，当场误伤 `tool_registry.rs` —— 那里的 `Engine`
            //   在一段**字符串字面量**里（给人看的文案），`production_code` 不剥字符串。
            //   ⇒ 类型要跑出去，必然先出现在 `use` 上。钉那一处，既准又没有假阳性。
            if guard_core::contains_word(&p, &format!("code_picture{}core", "_")) {
                elsewhere.push(rel);
            }
        }
        assert!(
            elsewhere.is_empty(),
            "`Engine` 跑出了 panorama 模块：{elsewhere:?}\n\
             ⇒ 那样「取用口恰好一处」就只是 panorama.rs 内部的局部真理，\n\
             换侧那天要追着改的地方不止一处。要用引擎请走 `panorama.rs` 里那个池子。"
        );
    }
}
