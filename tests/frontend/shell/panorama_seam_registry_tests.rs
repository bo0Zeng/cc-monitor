// 〔V108 后半句「之后本机也走这条路、monitor 摘内嵌引擎」〕**两条性质的住址都搬了家**：
//   ① 全景的命令面：monitor 这一侧只剩 `panorama_bytes.rs` 那一条（放字节 `panorama_place`；问 · 写 · 撤那三条
//      随界面直问那台后端删了），它的签名里仍不许有存储 / 解析细节；**op 词表**那一层（真正说「查什么」的地方）住后端
//      `protocol_doc_guard::the_panorama_protocol_would_only_expose_query_semantics`（两向 ＋ 禁词）。
//   ② 引擎取用口恰好一处：从 monitor `panorama.rs`（已删）搬到全景小程序 `src/panorama-engine/main.rs`
//      （monitor 进程一行引擎都不链了 —— 第二条判据改钉「monitor 源码树零引擎导入」）。
const PANORAMA: &str = include_str!("../../../src/frontend/shell/src/panorama_bytes.rs");
const ENGINE_PROGRAM: &str = include_str!("../../../src/panorama-engine/main.rs");

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
/// 会**污染另一条判据的语料** —— `tests/frontend/ui/ipc/commands.vitest.ts` 扫全仓 `.rs` 找
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
    // 地板 20 → 15：六条写命令删了（写改走当时 `panorama_call.rs` 里那条写命令），今天 17 条。
    //   这是反空真的自检地板（抽取坏了会塌到个位数），不是判据；判据是下面那条恒等计数。
    // 地板 15 → 3：人群换成 `panorama_call.rs`（进程内那 17 条随内嵌引擎删了），今天 3 条。
    // 地板 3 → 1：人群换成 `panorama_bytes.rs`（问 · 写 · 撤那三条随界面直问那台后端删了），今天 1 条。
    assert!(
        sigs.len() >= 1,
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
    // 21 → 23：多了 `panorama_diagram_kinds`〔散文墓碑〕（注册表原样透出）与
    // `panorama_diagram`（画一张图）—— 都是查询语义：说的是「代码里有什么结构」，
    // 参数是图种 id ＋ 画图旋钮，没有一个字关于存储或解析。
    // 23 → 17：少了六条**写**命令（人写 / 提议 / 批准 / 删批注、写 / 删文档关联）——
    // 它们经内嵌引擎直写被分析仓，改成「算（`plan_local`，不是命令）＋ 那台后端的文件管理写」，
    // 命令入口挪到当时 `panorama_call.rs` 里那条写命令（本机远端同一条）。只减不加，没有新面要看。
    // 人群换了一份文件：进程内那 17 条随内嵌引擎删了（本机也经 `panorama_call` 问本机后端），
    // 今天数的是 `panorama_call.rs` 的三条 —— `panorama_call`（按 origin 转一问：`op` 是不透明串，
    // 词表那一层在后端 `protocol_doc_guard` 钉着）· `panorama_edit`（写：只收 `EDITS` 那六个词）·
    // `panorama_cancel`（撤一张票）。三条的签名里都没有一个字关于存储或解析。
    // 3 → 1，人群换成 `panorama_bytes.rs`：问 · 写 · 撤那三条〔散文墓碑〕随界面经通道直问那台后端删了，
    // 剩 `panorama_place`（放字节：只收 origin）。只减不加，没有新面要看。
    const COMMANDS_TODAY: usize = 1;
    let n = prod().matches(cmd_attr().as_str()).count();
    assert_eq!(
        n, COMMANDS_TODAY,
        "全景命令数从 {COMMANDS_TODAY} 变成了 {n}。\n\
             **这不是要你改数字了事** —— 先回答：新加的那条是**查询语义**吗？\n\
             （`overview`/`node`/`callers`/`impact` 那样，说的是「代码里有什么」，\n\
             而不是「存储里怎么放的」。）是，就把数字改了；不是，就换个问法。"
    );
}

/// `P7c2-Y2`：**引擎取用口恰好一处**。那一处今天住全景小程序（`src/panorama-engine/main.rs`）。
#[test]
fn the_engine_is_opened_in_exactly_one_place() {
    let prod = guard_core::production_code(ENGINE_PROGRAM);
    assert!(
        prod.contains("fn dispatch("),
        "全景小程序的生产段没读到真文件"
    );
    let needle = format!("Engine{}open", "::");
    guard_core::find_pinned(&prod, &needle).unwrap_or_else(|e| {
        panic!(
            "`{needle}` 在生产段不是恰好一处：{e}\n\
                 ⇒ 第二处 = 第二条 rusqlite 连接。（`panorama.rs`（已删）的注释记过那条真事故：\n\
                 「对同一 index.db 并发写 → SQLITE_BUSY + 缓存不一致」。）\n\
                 而且换引擎（内嵌 / 侧车 / 编进后端）那天要改的就是这一处 —— 多一处多一份漏改。"
        )
    });
}

/// `P7c2-Y2` 的射程：**monitor 这棵源码树零引擎导入**（`EU5` 兑现：monitor 摘掉内嵌引擎）。
///
/// 〔改前是「vendor 引擎只被 `panorama.rs`（已删）导入」—— 那一份文件随内嵌引擎删了，
///  monitor 里**一处都不许有**；引擎的家只剩全景小程序（上一条钉它在那里恰好一处，
///  `engine_port_scope` 钉「每一棵我们编的树合起来」只有那一处）。〕
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
        let p = guard_core::production_code(&src);
        // ⚠ 钉的是**导入**，不是"文件里出现过 Engine 这个词"。
        //   第一版用整词匹配 `Engine`，当场误伤 `tool_registry.rs` —— 那里的 `Engine`
        //   在一段**字符串字面量**里（给人看的文案），`production_code` 不剥字符串。
        //   ⇒ 类型要跑出去，必然先出现在 `use` 上。钉那一处，既准又没有假阳性。
        if guard_core::contains_word(&p, &format!("code_picture{}core", "_")) {
            elsewhere.push(rel);
        }
    }
    // 正控：同一个判定在一份合成的导入上会命中（不然零命中什么也说明不了）。
    assert!(guard_core::contains_word(
        &guard_core::production_code(&format!("use code_picture{}core::Engine;\n", "_")),
        &format!("code_picture{}core", "_")
    ));
    assert!(
        elsewhere.is_empty(),
        "monitor 源码树里又有了引擎导入：{elsewhere:?}\n\
             ⇒ monitor 已经摘掉内嵌引擎（后半句）：全景本机远端都经那台机器的后端 → 全景小程序。\n\
             要全景里的东西请经那台后端的 `panorama`（加 op 是小程序 ＋ 后端适配层 ＋ 协议白名单三处同拍的事）。"
    );
}
