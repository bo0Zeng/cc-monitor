use super::*;
use crate::structural_scan::ScanReport;
use std::collections::HashSet;

// ===== 从源码枚举结构（要件 1），而不是硬编码现有字段 =====
//
// 前置条件（与 structural_scan 的 comment_prefix 同类，如实写明）：本模块的字符串
// 字面量里**不含 `//`、也不含花括号/方括号**，否则朴素的注释剥离与括号配对会算错。
// 这条由下面 parser_actually_sees_the_real_source 的反向自检兜底：真算错了，
// 字段集合就对不上，测试会红在那里而不是静默放过。

/// 剥掉 `//` 行注释与整个 `#[cfg(test)]` 段，只留生产代码文本。
///
/// **顺序不能反：先剥注释，再切测试段。** 第一版是反的，于是本模块文档里那句
/// 「已在 `lib.rs` 标 `#[cfg(test)]`」——一句**散文**——把切点提到了结构声明**之前**，
/// `production_code` 只返回前 50 行文档注释，5 条测试全红。
/// 是 `parser_actually_sees_the_real_source` 的反向自检（`assert!(code.contains(
/// "pub struct ToolSpec {"), "剥过头了")`）报出来的——**要件 3 又救了一次**。
/// 附带教训：我提交 `a6d4b63` 前改了这句文档却**没重跑 cargo test**，
/// 于是那个 commit 的 message 写着「cargo test 474」而实际是 469+5 红。
fn production_code(src: &str) -> String {
    let no_comments: String = src
        .lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n");
    // 切点还要求 `#[cfg(test)]` **顶格**（模块级属性），免得将来被缩进的同名属性骗到
    let code = no_comments
        .split(concat!("\n#[cfg", "(test)]"))
        .next()
        .unwrap_or(&no_comments)
        .to_string();
    code.lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 从 `from` 起找 `opener`，返回它**配对括号内**那段在 `text` 里的下标区间。
/// 括号种类取 `opener` 的最后一个字符（`{` / `[` / `(`）。
fn matched_span(text: &str, opener: &str, from: usize) -> Option<(usize, usize)> {
    let p = text[from..].find(opener)? + from;
    let open = opener.trim_end().chars().last()?;
    let close = match open {
        '{' => '}',
        '[' => ']',
        '(' => ')',
        _ => return None,
    };
    let start = p + opener.len();
    let mut depth = 1i32;
    for (i, c) in text[start..].char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some((start, start + i));
            }
        }
    }
    None
}

/// 某个结构声明的字段：`(名, 类型)`，**按源码里实际写的枚举**。
///
/// `struct_name` 是参数而不是硬编码 needle（T02 审计重要 3）：原先只扫 `ToolSpec`，
/// 于是**同一套审计手法下移一层仍然有效**——审计给 `TouchedFile` 加一个
/// `pub needs_sudo: bool`（10 个字面量里 1 真 9 假）→ **492 全绿、零 warning**
/// （`pub` 字段在 lib crate 里连 `dead_code` 都不报，连 T01 依赖的"clippy 存根"都没有）。
/// 参数化之后 `TouchedFile` 与 `ToolSpec` 走同一条纪律。
fn declared_fields_of(code: &str, struct_name: &str) -> Vec<(String, String)> {
    let (a, b) = matched_span(code, &format!("pub struct {struct_name} {{"), 0)
        .unwrap_or_else(|| panic!("取不到 {struct_name} 的声明体——扫描器失效了"));
    code[a..b]
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let l = l.strip_prefix("pub ").unwrap_or(l);
            let (name, ty) = l.split_once(':')?;
            let name = name.trim();
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                return None;
            }
            Some((
                name.to_string(),
                ty.trim().trim_end_matches(',').trim().to_string(),
            ))
        })
        .collect()
}

fn declared_fields(code: &str) -> Vec<(String, String)> {
    declared_fields_of(code, "ToolSpec")
}

/// `TOOLS` 里每一个 `ToolSpec { … }` 字面量的**体**文本。
fn literals_of<'a>(code: &'a str, type_name: &str) -> Vec<&'a str> {
    let (a, b) = matched_span(code, "pub const TOOLS: &[ToolSpec] = &[", 0)
        .expect("取不到 TOOLS 常量体——扫描器失效了");
    let body = &code[a..b];
    let mut out = Vec::new();
    let mut off = 0usize;
    // 配对之后从**本块结束处**继续找：找 `ToolSpec {` 时嵌套的 `TouchedFile` 块
    // 不会被重复计入；找 `TouchedFile {` 时则是逐个取那些嵌套块本身。
    let opener = format!("{type_name} {{");
    while let Some((s, e)) = matched_span(body, &opener, off) {
        out.push(&body[s..e]);
        off = e;
    }
    out
}

fn tool_literals(code: &str) -> Vec<&str> {
    literals_of(code, "ToolSpec")
}

/// 取字面量里 `field:` 在**顶层**（相对本字面量体）的取值文本。
/// 嵌套块里的同名字段（如 `TouchedFile { path: … }` 的 `path`）depth>0，不会命中。
fn field_value<'a>(lit: &'a str, field: &str) -> Option<&'a str> {
    let needle = format!("{field}:");
    let mut depth = 0i32;
    let mut prev: Option<char> = None;
    let mut start: Option<usize> = None;
    for (i, c) in lit.char_indices() {
        if start.is_none()
            && depth == 0
            && lit[i..].starts_with(&needle)
            && !prev.is_some_and(|p| p.is_alphanumeric() || p == '_')
        {
            start = Some(i + needle.len());
            prev = Some(c);
            continue;
        }
        match c {
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                if let Some(s) = start {
                    return Some(&lit[s..i]);
                }
            }
            _ => {}
        }
        prev = Some(c);
    }
    start.map(|s| &lit[s..])
}

/// 「实质取值」= 不是中性/空值。中性值意味着**这个工具其实不需要这个字段**，
/// 只是被 Rust 逼着填一个。审计塞的 `needs_elevation` 正是 5 个 `false` + 1 个 `true`。
fn is_substantive(v: Option<&str>) -> bool {
    match v {
        None => false,
        Some(v) => !matches!(
            v.trim(),
            "" | "false" | "None" | "\"\"" | "&[]" | "0" | "vec![]" | "Default::default()"
        ),
    }
}

/// **字段纪律扫描**：枚举声明的每个字段 → 数 `TOOLS` 里的实质取值 → <2 判违规。
fn field_discipline_of(code: &str, struct_name: &str, literal_name: &str) -> ScanReport {
    let fields = declared_fields_of(code, struct_name);
    let lits = literals_of(code, literal_name);
    let mut r = ScanReport {
        checked: 0,
        violations: Vec::new(),
    };
    if lits.len() < 2 {
        r.violations
            .push(format!("只找到 {} 个 {literal_name} 字面量", lits.len()));
        return r;
    }
    for (name, ty) in &fields {
        r.checked += 1;
        let users: Vec<&str> = lits
            .iter()
            .filter(|l| is_substantive(field_value(l, name)))
            .map(|l| field_value(l, "id").unwrap_or("?").trim())
            .collect();
        if users.len() < 2 {
            r.violations.push(format!(
                "字段 `{name}: {ty}` 只被 {} 个 {struct_name} 字面量实质实例化（{users:?}）\
                     ——只有一套需要的东西不进 {struct_name}",
                users.len()
            ));
        }
        if ty == "bool" && users.len() == lits.len() {
            r.violations.push(format!(
                "字段 `{name}: bool` 在全部 {} 个 {struct_name} 上都为真，没有区分力",
                lits.len()
            ));
        }
    }
    r
}

fn field_discipline(code: &str) -> ScanReport {
    field_discipline_of(code, "ToolSpec", "ToolSpec")
}

/// **声明式数据**扫描：枚举字段类型，白名单放行；行为（函数指针/`dyn`/需分配的容器）判违规。
fn declarative_only(code: &str) -> ScanReport {
    let mut r = ScanReport {
        checked: 0,
        violations: Vec::new(),
    };
    for (name, ty) in declared_fields(code) {
        r.checked += 1;
        let ok = ty == "&'static str"
            || ty == "bool"
            || (ty.starts_with("&'static [") && ty.ends_with(']'))
            || (ty.chars().all(|c| c.is_alphanumeric() || c == '_')
                && (code.contains(&format!("pub enum {ty}"))
                    || code.contains(&format!("pub struct {ty}"))));
        if !ok {
            r.violations.push(format!(
                "字段 `{name}: {ty}` 不是 const-可构造的声明式数据\
                     ——`ToolSpec` 只收数据，探测/装卸这类**行为**留在各工具自己那里"
            ));
        }
    }
    r
}

// ===== 反向自检（要件 3）：证明上面这套解析器真看见了真代码 =====

#[test]
fn parser_actually_sees_the_real_source() {
    let code = production_code(include_str!("../../src/bridge/src/tool_registry.rs"));
    assert!(code.contains("pub struct ToolSpec {"), "剥过头了");
    assert!(!code.contains("fn production_code"), "测试段没剥掉");
    let names: Vec<String> = declared_fields(&code).into_iter().map(|(n, _)| n).collect();
    assert_eq!(
        names,
        vec![
            "id",
            "display_name",
            "installable",
            "uninstallable",
            "carriers"
        ],
        "解析出的字段集合与源码不符——先查解析器，别改断言"
    );
    assert_eq!(
        tool_literals(&code).len(),
        TOOLS.len(),
        "字面量数应等于 TOOLS 长度"
    );
    // 顶层取值取对了，且不会被嵌套的同名字段污染
    let ccm = tool_literals(&code)[0];
    assert_eq!(field_value(ccm, "id").map(str::trim), Some("\"ccm\""));
    assert_eq!(field_value(ccm, "installable").map(str::trim), Some("true"));
    assert!(
        field_value(ccm, "path").is_none(),
        "`path` 只在嵌套块里，不该被顶层取到"
    );
}

/// **文档里提到 `#[cfg(test)]` 不许把切点提前**（这是真踩过的：5 条测试当场全红）。
#[test]
fn prose_mentioning_the_test_attribute_does_not_truncate_the_scan() {
    let src = concat!(
        "//! 已在 `lib.rs` 标 `#[cfg",
        "(test)]`，不占这笔债。\n",
        "pub struct ToolSpec {\n    pub id: &'static str,\n}\n",
        "\n#[cfg",
        "(test)]\nmod tests { fn helper() {} }\n"
    );
    let code = production_code(src);
    assert!(code.contains("pub struct ToolSpec {"), "散文把切点提前了");
    assert!(!code.contains("fn helper"), "测试段没被切掉");
}

/// **防上帝结构的门禁**（计划 §5 P2）。不是形式主义：本会话四次拒绝提前抽象
/// （R12 registry / R15 passThrough / B02 `--bus-id` / B03 `inbox_id_from_filename`），
/// 靠的都是"数真实消费者"。
#[test]
fn every_declared_field_has_at_least_two_instantiations() {
    let code = production_code(include_str!("../../src/bridge/src/tool_registry.rs"));
    field_discipline(&code)
        .require(5, "ToolSpec 字段纪律")
        .unwrap();
}

/// **同一条纪律也管 `TouchedFile`**（T02 审计重要 3）。
///
/// 原先字段纪律只扫 `ToolSpec`，于是 T01 那条审计手法**下移一层仍然有效**——
/// 审计给 `TouchedFile` 加 `pub needs_sudo: bool`（10 个字面量里 1 真 9 假）→
/// **492 全绿、零 warning**。`pub` 字段在 lib crate 里连 `dead_code` 都不报，
/// 所以连 T01 依赖的"clippy 存根"这条兜底都没有。
///
/// 顺带**更正我自己文档里说反的一句**：`TouchedFile` 的文档写着「`note` 的 ≥2 判据是
/// 人工数的，不谎称有门禁」——低估了。`note` 其实有一条机器门禁
/// （`config_surface` 的 `rows_cover_…` 里 `with_note.len() >= 2`），
/// 真正一条门禁都没有的是 `path` / `effect` 和**将来新增的字段**。现在这条补上了。
#[test]
fn touched_file_fields_follow_the_same_discipline() {
    let code = production_code(include_str!("../../src/bridge/src/tool_registry.rs"));
    field_discipline_of(&code, "TouchedFile", "TouchedFile")
        .require(3, "TouchedFile 字段纪律")
        .unwrap();
}

/// 用审计那条**下移一层**的手法验证上一条：给 `TouchedFile` 塞一个单实例化字段必须红。
#[test]
fn the_scan_catches_a_single_use_field_on_touched_file_too() {
    let code = production_code(include_str!("../../src/bridge/src/tool_registry.rs"));
    let lit_count = code.matches("            TouchedFile {").count()
        + code.matches("        touches: &[TouchedFile {").count();
    assert!(lit_count >= 6, "字面量锚点数不对：{lit_count}");
    let mutated = code
        .replace(
            "    pub effect: TouchEffect,\n}",
            "    pub effect: TouchEffect,\n    pub needs_sudo: bool,\n}",
        )
        .replace(
            "                effect: TouchEffect::",
            "                needs_sudo: false,\n                effect: TouchEffect::",
        )
        .replace(
            "            effect: TouchEffect::",
            "            needs_sudo: false,\n            effect: TouchEffect::",
        )
        .replacen("needs_sudo: false", "needs_sudo: true", 1);
    // **先确认变异真落位**（本会话两次"全绿"其实是变异没写进文件）
    let n = mutated.matches("needs_sudo").count();
    assert!(
        n >= 1 + 10,
        "变异没落到位：声明 1 处 + 每个 TouchedFile 一处，实得 {n}"
    );
    assert_eq!(mutated.matches("needs_sudo: true").count(), 1);
    let r = field_discipline_of(&mutated, "TouchedFile", "TouchedFile");
    assert!(
        r.violations.iter().any(|v| v.contains("needs_sudo")),
        "TouchedFile 上的单实例化字段必须被抓，实得 {:?}",
        r.violations
    );
}

/// **审计那条手法，钉成常驻测试**：直接变异**真文件**，塞一个中性命名的单实例化
/// 字段 `needs_elevation`。上一版硬编码断言对此**21 项全绿**。
#[test]
fn the_scan_catches_the_audits_own_single_use_field() {
    let code = production_code(include_str!("../../src/bridge/src/tool_registry.rs"));
    let mutated = code
        .replace(
            "    pub carriers: &'static [Carrier],",
            "    pub carriers: &'static [Carrier],\n    pub needs_elevation: bool,",
        )
        // 🔴 **锚点必须只认 `ToolSpec` 那一族的 `id:`**〔`K-R60` 09-11 现打〕：
        // 上一版的锚点是裸的 `"        id: \""` —— 它认的是「本文件里任何 8 空格缩进的
        // `id:` 行」，而那时**本文件只有 `TOOLS` 一张表**，所以它看起来是对的。
        // `K-R60` 往本文件加了第二张表（`UNMANAGED_ENV`，同样的缩进）之后，
        // 这一刀当场打到 17 处、计数自检红在「变异没落到位」。
        // ⇒ 收窄成 `ToolSpec {` + 下一行的 `id:`，只认该打的那一族。
        // ⚠ 这不是放水：命中数**仍然**由下面那条 `1 + TOOLS.len()` 的等号自检守着。
        .replace(
            "    ToolSpec {\n        id: \"",
            "    ToolSpec {\n        needs_elevation: false,\n        id: \"",
        )
        .replacen(
            "        needs_elevation: false,\n        id: \"ccm\"",
            "        needs_elevation: true,\n        id: \"ccm\"",
            1,
        );
    // **先确认变异真落进去了**（本会话两次"全绿"其实是变异没写进文件）
    assert_eq!(
        mutated.matches("needs_elevation").count(),
        1 + TOOLS.len(),
        "变异没落到位：声明 1 处 + 每个字面量 1 处"
    );
    assert_eq!(mutated.matches("needs_elevation: true").count(), 1);
    let r = field_discipline(&mutated);
    assert!(
        r.violations.iter().any(|v| v.contains("needs_elevation")),
        "单实例化字段必须被抓，实得 {:?}",
        r.violations
    );
    assert!(r.require(5, "ToolSpec 字段纪律").is_err());
    // 且不能顺手把好字段也误判
    assert_eq!(
        r.violations.len(),
        1,
        "只该有一条违规，实得 {:?}",
        r.violations
    );
}

/// 删掉一个字段的实质取值（把 `installable: true` 全改成 `false`）也必须红
/// ——否则这条扫描只对"新增"敏感，对"退化"是瞎的。
#[test]
fn the_scan_also_catches_a_field_degraded_to_neutral() {
    let code = production_code(include_str!("../../src/bridge/src/tool_registry.rs"));
    let mutated = code.replace("        installable: true,", "        installable: false,");
    // 自检必须带 8 空格前缀：不带的话 `uninstallable: false` 也会被数进去
    // （第一版就是这么错的，实得 9 而非 6，测试当场红在这一行——**先确认变异落位**再判色）
    assert!(!mutated.contains("        installable: true,"));
    assert_eq!(
        mutated.matches("        installable: false,").count(),
        TOOLS.len(),
        "5 处被改 + cc-bus 原本那 1 处"
    );
    let r = field_discipline(&mutated);
    assert!(
        r.violations.iter().any(|v| v.contains("installable")),
        "实得 {:?}",
        r.violations
    );
}

/// **探测机制不进 `ToolSpec`**，且这条守卫不是名字黑名单——上一版列的是
/// `["probe", "detect", "check_cmd", "fingerprint_cmd"]`，换个名字就穿。
/// 现在守的是**结构性质**：字段类型必须是 const-可构造的声明式数据。
#[test]
fn tool_spec_is_declarative_data_not_behavior() {
    let code = production_code(include_str!("../../src/bridge/src/tool_registry.rs"));
    declarative_only(&code)
        .require(5, "ToolSpec 只收声明式数据")
        .unwrap();
}

/// 用**改了名的**探测机制验证上一条：叫什么都拦得住，因为拦的是类型。
#[test]
fn a_renamed_probe_mechanism_is_still_caught() {
    for smuggled in [
        "pub how_to_look: fn(&str) -> bool,",
        "pub sniff: Box<dyn Fn(&str) -> bool>,",
        "pub tag: String,",
        "pub caps: Vec<String>,",
    ] {
        let synthetic = format!(
            "pub struct ToolSpec {{\n    pub id: &'static str,\n    {smuggled}\n}}\n\
                 pub const TOOLS: &[ToolSpec] = &[\n    ToolSpec {{ id: \"a\" }},\n];\n"
        );
        let r = declarative_only(&synthetic);
        assert_eq!(r.checked, 2, "两个字段都要进枚举：{smuggled}");
        assert_eq!(
            r.violations.len(),
            1,
            "只有 {smuggled} 该违规，实得 {:?}",
            r.violations
        );
    }
}

#[test]
fn ids_are_unique_and_stable() {
    let ids: HashSet<_> = TOOLS.iter().map(|t| t.id).collect();
    assert_eq!(ids.len(), TOOLS.len(), "id 必须唯一（T02 会拿它当键）");
    for t in TOOLS {
        assert!(!t.id.is_empty() && !t.display_name.get().is_empty());
        // id 用于持久化/UI dataset，限制字符集免得以后踩 B03 那种 `--help` 的坑
        assert!(
            t.id.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
            "id {:?} 只允许小写字母与连字符",
            t.id
        );
    }
}

/// `TouchEffect` 的每个变体都得有真实使用者——只有一个用户的变体同样是过度设计。
#[test]
fn touch_effects_are_all_really_used() {
    let effects: HashSet<_> = TOOLS
        .iter()
        .flat_map(|t| t.touches().map(|f| f.effect))
        .collect();
    assert!(
        effects.len() >= 3,
        "TouchEffect 至少要有三种被真实用到，实得 {effects:?}"
    );
}

/// cc-bus 的部署**还没实现**，声明必须如实为 false。
/// 「计划里写了要做」不等于「已经能做」——注册表是给 UI 看的，标错了 UI 就会给出
/// 一个点了没反应的按钮。
#[test]
fn declarations_match_reality_not_intent() {
    let ccbus = TOOLS.iter().find(|t| t.id == "cc-bus").unwrap();
    // ⚠⚠ 〔`PS1` 08-13〕**这条断言今天落后于代码一格，理由如实写在这里**：
    // `U10b`〔用@08-13〕裁「开」之后，cc-bus 的部署**真的实现了**
    // （`cc_bus_deploy.rs`，6 条行为判据：围栏拒软链 / 幂等不重写 / 覆盖留备份 /
    //  装完可执行 / 内嵌清单与仓对拍）。按本条的名字（「声明要配现实」），
    // `installable` 该翻成 `true`。
    //
    // **而翻了它当场撞上另一条判据** —— `every_declared_field_has_at_least_two_instantiations`：
    // cc-bus 是最后一个 `false`，翻掉之后 `installable` 在**全部 6 个** `ToolSpec` 上都为真
    // ⇒ 「没有区分力」。那条报得**对**：字段退化成常量，按本仓的既定准则就该**删字段**
    // （同 `P4c` 撞 `host_is_not_a_function_of_destination` 那次的处置）。
    // 而删它要动 `config_surface` 的行、UI 文案（`config-surface-section.ts:50`）与
    // `skill_host` 里那条钉着 `"installable: false"` 字面量的判据 —— **是一次跨文件重构**。
    //
    // ⇒ 本件**不顺手做那次重构**（`PS1` 的正题是部署，不是字段治理），
    // 声明位暂留 `false`，代价如实登记在这里：**声明表比代码晚一格**。
    // ★ 解锁条件：删掉 `installable` 字段（或给它找回区分力）之后，把这条断言反过来。
    //
    // 🔴 **09-11 `K-R60`：上面那个解锁条件兑现了，本条按它自己写的话反过来。**
    // 走的是「**给它找回区分力**」那一支，不是删字段 —— 删了就再没有任何字段能申报
    // 「cc-bus 装不装得了」，而这一格恰恰是本件在治的。
    // 区分力从哪儿回来的：`claude-code` 那条（装不了、只读）进表，
    // 理由写在它自己那个字面量上头。
    // ⚠ 那一整段「暂留 false」的理由**留着不删**：它是这处假申报活了一个月的来路，
    //   而本条的名字（「声明要配现实」）说的正是那件事。
    //
    // 🔴 **09-11 `K-R63`：本条原先在这里有两条专名断言，已经收走了。**
    // 原文逐字是 `assert!(ccbus.installable, …)` 与 `assert!(!ccbus.uninstallable, "卸载没做，不得声明可卸")`。
    // 它们判的正是「申报 ↔ 现实」，而那件事今天由一条**覆盖全表**的性质判
    // （`every_tool_declares_install_and_uninstall_as_the_implementations_really_are`）。
    // 留着它们不是双保险，是两个坏处：
    //   ① 专名钉子只把静默从一个工具挪走，下一个工具照样静默（件文件 `§0c` 的正题）；
    //   ② 第二条会**在事情变好的那天错红** —— 真给 cc-bus 补上卸载实现并如实把字段翻成
    //      `true`，它会拦一次，而那时它拦的是一句真话。
    // ⇒ 本条今天只剩下**不属于那条性质**的那一格：`settings.json` 的 effect。
    // ⚠ 如实登记：本条的**名字**因此比它现在做的事宽了一格（改名要连带跑生成命令，
    //   PM 的窗口开着时不许跑）⇒ 改名的事走上报口交回 PM，不在这一拍自批。
    // settings.json 只生成待贴文本，绝不写
    let hooks = ccbus
        .touches()
        .find(|f| f.path.contains("settings.json"))
        .expect("cc-bus 应声明它需要 settings.json 的钩子");
    assert_eq!(
        hooks.effect,
        TouchEffect::GenerateOnly,
        "settings.json 是共享全局配置，只能生成待贴文本"
    );
}

/// **声明「整个文件由我们拥有」就必须真的装得了它**（T02 审计阻塞 2）。
///
/// 原先 cc-bus 的 `~/.local/bin/cc-*` 是 `OwnedFile` 而 `installable: false`
/// ——审计页于是同时显示「12 项匹配」+「由 cc-monitor 拥有、部署时整体覆盖」+
/// 「尚未支持部署，也就无所谓撤销」。用户读到的是：cc-monitor 宣称拥有 12 个
/// 它没建、装不了也撤不了的文件。真机核实：那 12 条软链是用户自己的安装脚本
/// 于 7/17 与 7/26 建的，cc-monitor 侧**一行创建代码都没有**。
#[test]
fn owned_file_implies_installable() {
    for t in TOOLS {
        if t.touches().any(|f| f.effect == TouchEffect::OwnedFile) {
            assert!(
                t.installable,
                "{} 声称拥有某个文件却装不了它——那这个「拥有」是假的",
                t.id
            );
        }
    }
}

/// **装得了，就必须申报装到哪**（替换掉那条同义反复的测试，见下）。
///
/// 这一条替代原先的 `locality_is_derivable_from_destination_today`。审计实测那条是
/// **同义反复**：`PathResolution::Remote` 只由 `RemoteHomeRelative` 臂产生且必然产生，
/// 所以断言恒真——把 `ccm` 的 `destination` 翻成 `LocalHomeRelative`（会让两行从
/// "远端未确定"变成去 stat 本机 `~/.bashrc`）**492 项照样全绿**。
/// 而它承诺守的那件事（"本机落点却申报远端文件"）在类型上根本表达不出来，
/// 永远不会红。**不留永远不会红的钉子。**
///
/// 换成这条有牙的跨字段一致性：`installable` 的工具，其 `destination` 指的那个路径
/// 必须出现在 `touches` 里。改任一边就会红。
/// （`installable: false` 的 cc-bus 豁免——它的 `destination` 目前是**愿景**，
///  部署还没实现，硬要它出现在 touches 里就得给一个假的 effect，那正是阻塞 2 的病。）
/// ★ PS1（重摸底 08-12）：cc-bus 的 `installable: false` **必须写着深一层的理由**。
///
/// 浅理由（「部署尚未实现」）会让下一个人以为补个递归拷贝就能翻 true；
/// 而真实的墙是 `src/doc/INVARIANTS.md` 那条只读铁律 —— 落点 `~/.claude/skills/` 不在
/// 它穷举的 6 条例外里。**那是裁定，不是实现工作。**
///
/// 这是「禁词守卫」的反面：**必需词**守卫。删掉那段话的人会被拦一次。
#[test]
fn cc_bus_says_why_it_is_not_installable_at_the_real_depth() {
    // 🔴 〔步 7c 剖分 2026-09-19 · `设计/16 §6.2` D 类 ＋ `§4.2` 那条预言**这一格兑现了**〕
    //
    // 原来这里是 `me.matches(must).count() >= 2`，理由写在原注里：
    //「本判据自己的数组里就写着这两句 ⇒ `contains` 恒真」——
    // 那个 `>= 2` 的前提是**针与草垛同住一份文件**（一次算针、一次算草垛）。
    // 剖分之后针住 `tests/`、草垛住 `src/` ⇒ 同一份文件里**永远凑不到 2**
    // （现打：两份各 1 次），前提恒假。
    //
    // ⇒ 改成它今天该有的形状，而这个形状**比原来严**：
    //   ① 正题直接读**那份生产文件**，要求 `>= 1` ——
    //      针已经不在草垛里了，`contains` 不再恒真，`>= 2` 那个绕法**不需要了**。
    //      这正是 `设计/16 §4.2` 预言的「判据和语料物理不同文件 ⇒ 自指不可能发生」，
    //      而它在这一格上**第一次真的成立**（`§4.2` 订正说的是「全仓不成立」，不是「一处都不成立」）。
    //   ② 反向钉一句「针在本文件里恰好一处」：哪天有人把那段头注搬回测试段，
    //      本条当场红 —— 也就是「自指回来了」这件事从此有人看着。
    let target = include_str!("../../src/bridge/src/tool_registry.rs");
    let me = include_str!("tool_registry_tests.rs");
    for must in ["开第 7 条豁免", "绝不碰"] {
        assert_eq!(
            me.matches(must).count(),
            1,
            "{must:?} 在本文件里出现了 {} 次（应恰好 1 次 —— 就是上面那个数组里的针）。\n\
                 ⇒ 多于 1 次 = 被钉的那段注释**搬进测试段**了，自指又回来了。",
            me.matches(must).count()
        );
        assert!(
            target.matches(must).count() >= 1,
            "cc-bus 那条 `installable: false` 的注释里少了 {must:?}。\n\
                 只写「部署尚未实现」是**浅一层**的理由 —— 真实的墙是只读铁律\n\
                 （`~/.claude/skills/` 不在它穷举的 6 条例外里）。\n\
                 删掉它，下一个人会以为补个递归拷贝就能翻 true。"
        );
    }
}

/// 一个载体的**申报落点**（`destination` ⇒ 那条 touch 该写成什么）。
///
/// **唯一一份口径**〔`13b`〕：下面三条判据（落点在不在 touches 里 · 同一个东西有几个
/// 落点 · 那几个落点逐条钉死）都从这里取，别在第二处再写一份 `match`。
/// `None` = 这一格回答的不是「装到哪」（[`ToolDestination::NotInstalledByUs`]）。
fn landing_path_of(d: &ToolDestination) -> Option<String> {
    match d {
        ToolDestination::RemoteHomeRelative(p) | ToolDestination::LocalHomeRelative(p) => {
            Some(format!("~/{p}"))
        }
        ToolDestination::ProjectRelative(p) => Some((*p).to_string()),
        ToolDestination::UserShellProfile => Some("$PROFILE".to_string()),
        ToolDestination::UserConfiguredPath { token, .. } => Some((*token).to_string()),
        ToolDestination::NotInstalledByUs { .. } => None,
    }
}

/// **这个工具说得出几个落点** —— 就是本件那条 dod 判的那个「关系」。
///
/// 🔴 **为什么数的是载体而不是枚举值**（`KR81D1` 逐字写死的失效方向）：
/// `destination` 是单值的时候这个数**恒等于 1**，往 `ToolDestination` 里
/// **加多少个枚举值它都还是 1** —— 那只是「值多了一个」，不是
/// 「一个东西对多个落点」。这个数 >1 当且仅当**载体这一维真的存在**。
fn landing_sites_of(t: &ToolSpec) -> Vec<String> {
    t.carriers
        .iter()
        .filter_map(|c| landing_path_of(&c.destination))
        .collect()
}

#[test]
fn installable_tools_declare_where_they_land() {
    for t in TOOLS {
        if !t.installable {
            continue;
        }
        // 🔴 〔`K-R81` 09-12〕**逐载体判，而这一步比先前严**。
        //    `K-R69` 那一版是「一个工具一个 `destination`、一串期望落点、去这个工具的
        //    **全部** touches 里找」—— 那时 M 个落点 × N 条 touch **没有 key**，
        //    一个落点被另一个载体的 touch「凑巧接住」也照样绿。
        //    今天落点与 touch 都挂在同一个载体下 ⇒ 接住它的必须是**它自己那一份**。
        for c in t.carriers {
            // 〔`K-R60`〕跨字段：「这不是我们的落点」与「装得了」不许同时成立。
            let Some(want) = landing_path_of(&c.destination) else {
                panic!(
                    "{} 声明 installable: true，而载体「{}」的落点写着「不是我们装的」——\
                         两句话有一句是假的",
                    t.id, c.what
                )
            };
            assert!(
                c.touches.iter().any(|f| f.path == want),
                "{} 的载体「{}」可安装，但它自己的 touches 里没有它的落点 {want:?}（实得 {:?}）",
                t.id,
                c.what,
                c.touches.iter().map(|f| f.path).collect::<Vec<_>>()
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 🔴 `K-R81` / `KR81D1` ＋ `KR81D3`：**一个后端，几处使用**
//
// 用户 09-12 逐字：「一个后端要两处使用 / 即远程后端就是远程本地机器的后端」。
// 下面四条判的是**这句话在闭集里说得出来**，不是「表好看一点」。
// ═══════════════════════════════════════════════════════════════════════

/// 后端那条 `ToolSpec` 的 id —— **只许有一个住址**〔`13b`〕，
/// 下面几条判据与别处引用它的地方都从这里取。
const BACKEND_ID: &str = "backend";

fn backend() -> &'static ToolSpec {
    TOOLS
        .iter()
        .find(|t| t.id == BACKEND_ID)
        .unwrap_or_else(|| panic!("闭集里找不到 id 为 `{BACKEND_ID}` 的那一条"))
}

/// ★ `KR81D1` **正面**：闭集说得出「**同一个后端**落在哪几处」。
///
/// # 死值验（`KR81D1` 逐字要的那一向）
///
/// 把三个落点里**任意一个**从闭集里摘掉 ⇒ 本条红，**并点名是哪一个没了**。
///
/// # 🔴 失效方向写死在这里（`KR81D1` 逐字）
///
/// 「给 `destination` 加第二个枚举值就算多落点」——**不算**。
/// [`landing_sites_of`] 数的是**载体**：`destination` 单值时它恒等于 1，
/// 往 `ToolDestination` 里加多少个变体它都还是 1。
/// 本条要的是那个数 **> 1**，也就是「一个东西对多个落点」这个**关系**存在。
///
/// # 它守什么、**不守什么**
///
/// 守的是**申报**（这张表说不说得出那三份）。「那三份是不是同一次构建出来的」
/// **本条不判，而且今天没有任何东西判得了** —— `.build_id` 三者从同一处源码常量抠，
/// 恒等，那句恒等一格证据都不提供（`DECISIONS.md#R26` 裁定零）。
#[test]
fn the_backend_is_one_thing_landing_in_several_places() {
    let t = backend();

    // ① 关系存在：不是「一个落点」，也不是「一个值多了几个枚举变体」。
    let sites = landing_sites_of(t);
    assert!(
        sites.len() > 1,
        "`{BACKEND_ID}` 只说得出 {} 个落点（实得 {sites:?}）——\n\
             用户 09-12 逐字「一个后端要两处使用」，而闭集里它还是一处。\n\
             ⚠ 往 `ToolDestination` 里加枚举值买不到这一格：这个数数的是**载体**。",
        sites.len()
    );

    // ②b 反向自检：这把尺子在**单载体**的工具上真的给 1（否则上面那条是空真）。
    let single: Vec<usize> = TOOLS
        .iter()
        .filter(|t| t.carriers.len() == 1)
        .map(landing_sites_of)
        .map(|v| v.len())
        .collect();
    assert!(
        single.iter().any(|n| *n <= 1),
        "尺子失准：单载体的工具也数出 >1 个落点（实得 {single:?}）——\n\
             那说明它数的不是载体，上面那条 `>1` 于是恒真"
    );

    // ② 逐条钉死：`(这一份是从哪来的, 它落到哪, 在哪台机器上)`。
    //    改 `TOOLS` 就要来改这张表 —— 这是**有意的摩擦**（同
    //    `config_surface::every_host_declaration_is_pinned` 那张表的理由）。
    let mut got: Vec<(String, String, HostScope)> = t
        .carriers
        .iter()
        .map(|c| {
            let src = match &c.source {
                ToolSource::EmbeddedBinary { repo_path } => (*repo_path).to_string(),
                other => panic!(
                    "后端的载体来源不该是 {other:?} —— 三份都是**二进制**（`K25`：\
                         一份代码、每个平台一份原生产物）"
                ),
            };
            let dst = landing_path_of(&c.destination)
                .unwrap_or_else(|| panic!("后端的载体「{}」没有落点", c.what));
            let host = c
                .touches
                .iter()
                .find(|f| f.path == dst)
                .unwrap_or_else(|| panic!("载体「{}」的落点不在它自己的 touches 里", c.what))
                .host;
            (src, dst, host)
        })
        .collect();
    let mut want: Vec<(String, String, HostScope)> = vec![
        // ③ 安装包放在 app 可执行文件旁边的那份（`tauri.sidecar.conf.json` 的 `externalBin`）
        (
            "src/bridge/binaries/cc-monitor-backend".into(),
            "$APP_DIR".into(),
            HostScope::Client,
        ),
        // ① 这一份产物自己带着、旁边没有本机后端时自释放的那份
        (
            "src/bridge/native-backend/cc-monitor-native".into(),
            "~/.cc-monitor/bin/cc-monitor-backend-*".into(),
            HostScope::Client,
        ),
        // ② 推给远端那台机器、在那台机器上当**它的本地后端**跑的那份
        (
            "embedded-backends".into(),
            "$BACKEND_PATH".into(),
            HostScope::Remote,
        ),
    ];
    // `HostScope` 没有 `Ord`（它是描述型 enum），按前两栏排 —— 同
    // `config_surface::every_host_declaration_is_pinned` 那张表的写法。
    got.sort_by(|x, y| (&x.0, &x.1).cmp(&(&y.0, &y.1)));
    want.sort_by(|x, y| (&x.0, &x.1).cmp(&(&y.0, &y.1)));
    assert_eq!(
        got, want,
        "\n后端的载体清单与钉死的表对不上 —— 少一份就是「闭集说不出它」，\n\
             而那正是 `K-R68` 立件时的读数（3 种载体 4 个落点，闭集表达 1 个半）。\n\
             改 `TOOLS` 就要来改这张表，并说清为什么。"
    );
}

/// ★ `KR81D3`：**`ccm` 那两份的来源不是同一个，而闭集今天说得出来。**
///
/// # 立件时这一条是红的（那正是这半格的题面）
///
/// `R26` 裁定二逐字：`ToolSource` 与 `destination` 是**同一个形状问题的两半**。
/// 在载体这一维立起来之前，`ccm` 只有一个 `source`，而它的注释**自己承认是假的**
/// （逐字：「本机那一半的来源不是这个 shim —— 是后端二进制自己的改名副本……
/// `ToolSource` 一个字段同样装不下两个来源」）—— **用散文顶替一个字段**。
///
/// # 死值验（`KR81D3` 逐字要的那一向）
///
/// 把本机那份的 `source` 改回今天那个假值（＝ 与远端那份同一个 `ccm_entry_shim`）
/// ⇒ 本条红。
#[test]
fn the_two_ccm_carriers_do_not_share_one_false_source() {
    let ccm = TOOLS
        .iter()
        .find(|t| t.id == "ccm")
        .expect("闭集里没有 `ccm` 那一条");
    assert_eq!(
        ccm.carriers.len(),
        2,
        "`ccm` 今天是**两份**：远端那条 shim + 本机那份后端二进制的改名副本"
    );
    let srcs: Vec<&ToolSource> = ccm.carriers.iter().map(|c| &c.source).collect();
    assert_ne!(
        srcs[0], srcs[1],
        "`ccm` 两个载体的 `source` 逐字相同 —— 那正是本件治的那句假话：\n\
             远端那条是 `local_backend::ccm_entry_shim` 现造的三行 `exec` 串，\n\
             本机那条是**后端二进制自己的改名副本**（`install_local_ccm_entry`）。\n\
             一个 `source` 装不下两个来源，而「在注释里如实写清」不是一个字段。"
    );
    // 本机那一份的来源必须指到那个**放二进制**的符号，不是那个造 shim 的符号。
    let local = ccm
        .carriers
        .iter()
        .find(|c| matches!(c.destination, ToolDestination::LocalHomeRelative(_)))
        .expect("`ccm` 本机那个载体不见了（`K-R69` 建的那条）");
    match &local.source {
        ToolSource::EmbeddedBinary { repo_path } => assert!(
            repo_path.ends_with("::install_local_ccm_entry"),
            "本机那条 `ccm` 的来源指到了 {repo_path:?} —— 它该指到真把那份字节\
                 放下去的那个符号（`local_backend::install_local_ccm_entry`）"
        ),
        other => {
            panic!("本机那条 `ccm` 是**一份二进制**（后端本体的改名副本），不是 {other:?}")
        }
    }
}

/// **一个工具的几个载体，不许一半是我们的、一半是别人的。**
///
/// 这一条是 [`Provisioning::of_tool`] 那条聚合规则（「全部载体都是
/// `NotInstalledByUs` 才算不是装出来的」）的门禁：混合的那一形一出现，
/// 那条规则就得**重裁**，而不是让它静默地选一边
/// （`references/testing.md` 硬规则 11：钉「今天恰好如此」的判据要写清去哪儿重裁）。
#[test]
fn carriers_do_not_mix_ours_and_not_ours() {
    for t in TOOLS {
        let n = t
            .carriers
            .iter()
            .filter(|c| matches!(c.destination, ToolDestination::NotInstalledByUs { .. }))
            .count();
        assert!(
            n == 0 || n == t.carriers.len(),
            "`{}` 的 {} 个载体里有 {n} 个写着「不是我们装的」——\n\
                 `Provisioning::of_tool` 那条聚合规则（全是才算）此刻在**替你选一边**。\n\
                 ⇒ 去 `Provisioning::of_tool` 的头注重裁那条规则，别把这条判据放宽。",
            t.id,
            t.carriers.len()
        );
    }
    // 反向自检：混合的那一形真的判得出来（合成一条，不动真表）。
    const MIXED: &[Carrier] = &[
        Carrier {
            what: Text(|| "自检：我们装的那一份".to_string()),
            source: ToolSource::Generated,
            destination: ToolDestination::LocalHomeRelative(".x/y"),
            touches: &[],
        },
        Carrier {
            what: Text(|| "自检：别人的那一份".to_string()),
            source: ToolSource::NotOurs {
                who: Text(|| "自检".to_string()),
            },
            destination: ToolDestination::NotInstalledByUs {
                whose: Text(|| "自检".to_string()),
            },
            touches: &[],
        },
    ];
    let n = MIXED
        .iter()
        .filter(|c| matches!(c.destination, ToolDestination::NotInstalledByUs { .. }))
        .count();
    assert!(
        n != 0 && n != MIXED.len(),
        "自检夹具没造出混合那一形 —— 上面那条断言此刻是空真"
    );
}

/// **每个载体都说得出自己是哪一份**，而同一个工具里两份不许说同一句话。
///
/// 没有这一格，多载体的工具在配置面上就是几行长得一样的字 ——
/// 「说得出几个落点」于是退化成「表里多了几行」。
#[test]
fn every_carrier_says_which_one_it_is() {
    let mut n_multi = 0;
    for t in TOOLS {
        let mut seen: HashSet<String> = HashSet::new();
        for c in t.carriers {
            assert!(
                !c.what.get().trim().is_empty(),
                "`{}` 有一个载体没说自己是哪一份",
                t.id
            );
            assert!(
                seen.insert(c.what.get()),
                "`{}` 有两个载体说着同一句话（{:?}）—— 那就分不出是哪一份了",
                t.id,
                c.what
            );
        }
        if t.carriers.len() > 1 {
            n_multi += 1;
        }
    }
    // 地板反向自检：真表里得有**多载体**的工具，否则上面那条唯一性是空真。
    assert!(
        n_multi >= 2,
        "闭集里多载体的工具只有 {n_multi} 个 —— 唯一性那半此刻几乎是空真。\n\
             今天该有两个：`ccm`（远端 shim / 本机改名副本）与 `{BACKEND_ID}`（三种载体）"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 🔴 `K-R81` / `KR81D2`：**那个名字改到哪儿了 —— 一张可核的账**
//
// dod 逐字：「**不要求一次全改**，但**要求给出「改了哪些、没改哪些、为什么」的
// 可核读数**」，失效方向逐字：「**只改闭集那个 id，而 70 份 `.rs` 照旧** ——
// 那是把账做平，不是把名字改对」。
// ⇒ 下面两条各买一半：
//   · 第一条是**零命中守卫**，射程 = 闭集那张表的**数据**本身（改回旧 id ⇒ 当场红）；
//   · 第二条是**登记 ＋ 递减棘轮**，射程 = `src/bridge/src` ＋ `src/bridge/crates` 两棵树 ——
//     没登记就不许带旧名，登记了就只许变少。**那张表就是那份读数**，不是一句话。
// ═══════════════════════════════════════════════════════════════════════

/// 旧名字今天还留在哪儿，**按「改它要动什么」分档**。闭集。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Why {
    /// **符号名**（那几个 `*_backend` 的 `pub async fn`、以及对它们的逐字引用）。
    /// 改它要与 `structural_scan` 的逐字签名钉、`sftp_move_ledger`〔散文墓碑〕（〔SR1b〕已退役）、
    /// `parity_ledger`、`remote_write_registry` 那几张登记表**同拍**改 ——
    /// 那是一件纯符号改名件，与本件的正题（名字说错了「它**是什么**」）不同轴。
    /// **解锁条件**：另立一件「符号改名」，把那几张表一起带上。
    /// ✅ 〔步 8 2026-09-19〕**那一件做了**（`设计/99 §4` 步 8，全仓冻结窗口）——
    /// 这一档降到 12 处，剩下的 12 处**不是符号改名问题**：它们全在「远端那份后端的路径」
    /// 那一族符号上，而那个拼写由**用户盘上那个配置键**锁着（见 [`SITES`]）。
    SymbolName,
    /// **措辞**（注释 / 文档 / 印给用户的串里那句「远端的那个后台进程」）。
    /// 🔴 **这些句子今天多数不假** —— 那确实是远端那台上的那一份；
    /// 用户裁的是它的**身份**（那是**那台机器的本地后端**，`K36`）。
    /// 订正措辞要连**前端那一面**一起过（`src/**/*.ts` 现打 30 余处），
    /// 只改 Rust 半边会让两边说两种话。**解锁条件**：另立一件「文案面」，两侧同拍。
    /// ✅ 〔步 8 2026-09-19〕**那一件做了，这一档清零** —— Rust 报文、TS 界面串与散文
    /// 一趟同拍改成「远端后端」。**这一档今天在 [`SITES`] 里一行都没有。**
    Wording,
    /// **逐字引用旧的闭集 id** —— 全部是订正段 / 墓碑 / 病史
    /// （「本条落地当场逮到 `<旧 id>`」这一族）。
    /// 🔴 **刻意保留，不改**：一句话写下时真、后来被别的裁定推翻，
    /// **那是历史，不是错误**（同 `MASTERPLAN#K25` 对 `C18` 三处的处置）。
    /// 改掉它等于抹掉推翻的过程。**没有解锁条件 —— 它本来就不该被改。**
    OldId,
}

/// **旧名字的存量账。闭集。**
///
/// `(相对 `src/bridge/` 的路径, 哪一档, 今天的处数)`
///
/// 🔴 表名叫 `SITES` 不是随手起的：`scanning_guard_registry::TABLE_DECLS`
/// 那条纪律逐字「新写一条『扫描面 ＋ 常量表』型的判据，那张表要起成
/// `TABLE_DECLS` 里已有的名字之一」——起别的名字，那条元判据**看不见本文件**，
/// 而看不见与「合规」在输出上一模一样。
///
/// ⚠ **这张表不是愿望清单，是读数**：每一行都由下面那条判据在**真树**上对拍，
/// 多一处红、少一处也红（少 ⇒ 那一行该删了，账不许挂着空号）。
const SITES: &[(&str, Why, usize)] = &[
    // 🔴 〔步 8 改名一刀 2026-09-19 · `设计/99 §4` 步 8〕**这张账从 43 行塌到 12 行。**
    //
    // 塌下去的那 31 行**不是被删掉了，是债真的还了**：`Why::SymbolName` 与 `Why::Wording`
    // 两档当年各自写着「解锁条件：另立一件『符号改名』／『文案面，两侧同拍』」——
    // **步 8 就是那一件**（全仓冻结窗口，Rust ＋ TS ＋ 散文一趟做完）。
    // 下面那条判据的 `stale` 那一向逐字：「那一处已经改完了，**把这一行删掉**
    // （账不许挂空号：一张挂着空号的表会让人以为债还在那儿，而它其实早还了）」⇒ 照办。
    //
    // **今天还剩的 8 行，逐档说清为什么还在**：
    //   · `Why::OldId`（9 行）—— **本来就不该改**（那一档头注逐字「没有解锁条件」）：
    //     订正段 / 墓碑 / 病史里对旧闭集 id 的逐字引用。步 8 的机械替换把这个拼写
    //     **明写进了保护名单**（`tests/evidence/w8-rename.py` 的 `PROTECTED`）。
    //   · `Why::SymbolName`（**0 行 · 这一档 2026-09-19 清零**）—— 上一版是 3 行共 12 处，
    //     全部是「远端那份后端的路径」那一族符号。它们跟着**用户盘上 `config.json` 里那个键**走
    //     （`RemoteConfig` 带 `#[serde(rename_all = "camelCase")]` ⇒ Rust 字段名就是线上键名），
    //     而那一版写的解锁条件逐字是：「`backendPath` **那条迁移**落地的同一拍」。
    //     🔴 **它不是被迁移清掉的，是被一条裁决清掉的** —— 条 80（用户 2026-09-19 逐字
    //     「新版本要完全抛弃旧的」）把「为盘上已有状态写回落」这件事整个撤了 ⇒
    //     `A1` 那一刀直接改名、**没有写迁移**，代价（已配好的远端主机路径丢一次）由用户承担。
    //     ⚠ **这条账的写法值得记**：解锁条件写的是「做完某件事」，而真实的出口是
    //     「那件事被裁定不必做」。⇒ 解锁条件最好写成**可观测的状态**（「盘上不再有这个拼写」），
    //     而不是**某个动作**（「迁移落地」）——后者遇到「动作被取消」就会指空。
    //   · `Why::Wording`（0 行）—— **这一档清零了**。界面串与散文里那句「远端 ＋ 旧词」
    //     两侧同拍改成了「远端后端」，一处不剩。
    ("src/skill_host.rs", Why::OldId, 1),
    ("src/structural_scan.rs", Why::OldId, 1),
    ("src/tool_registry.rs", Why::OldId, 2),
    ("tests/bridge/config_surface_tests.rs", Why::OldId, 4),
    ("tests/bridge/fenced_block_tests.rs", Why::OldId, 1),
    ("tests/bridge/skill_host_tests.rs", Why::OldId, 1),
    (
        "tests/bridge/tool_registry_environment_tests.rs",
        Why::OldId,
        1,
    ),
    ("tests/bridge/tool_registry_tests.rs", Why::OldId, 3),
];

/// 三档各自的处数。**针全部运行期拼**〔同 `scanning_guard_registry` 头注里
/// `attr` 那一处的写法〕—— 本文件自己在扫描面里（见下），字面量写在这儿
/// 会把量具自己算进被测量。
///
/// **顺序即口径**：先把「crate 目录 / 包名的那个拼写」整个剥掉再数 ——
/// 那是**住址**，不是名字（本拍刻意不碰，理由在下面那条判据的头注里）。
fn old_name_counts(text: &str) -> [usize; 3] {
    let d = "-";
    let u = "_";
    let stem_id = format!("remote{d}daemon");
    let stem_sym = format!("remote{u}daemon");
    // 住址拼写：crate 目录 `…-proto` 与它的 Rust 模块名 `…_proto`
    let rest = text
        .replace(&format!("{stem_id}{d}proto"), "")
        .replace(&format!("{stem_sym}{u}proto"), "");
    let sym =
        rest.matches(&stem_sym).count() + rest.matches(&format!("Remote{}aemon", "D")).count();
    let old_id = rest.matches(&stem_id).count();
    let wording = rest.matches(&format!("远端 {}", "daemon")).count()
        + rest.matches(&format!("远端{}", "daemon")).count();
    [sym, old_id, wording]
}

fn count_of(text: &str, w: Why) -> usize {
    let [sym, old_id, wording] = old_name_counts(text);
    match w {
        Why::SymbolName => sym,
        Why::OldId => old_id,
        Why::Wording => wording,
    }
}

/// `pub const TOOLS` 那个常量的**体**（配对方括号之间那一段），注释已剥掉。
///
/// ⚠ 剥注释是**有意的**：闭集的**数据**不许再叫旧名字，而注释里的墓碑与病史
/// （`Why::OldId` 那一档）**刻意保留**。两件事分开判。
///
/// 🔴 剥法**借共享原语** [`guard_core::strip_comment_lines`]，不自己写第二份 ——
/// `structural_scan.rs` 那张 `TRANSFORMERS` 登记表背后的判据逐字
/// 「换个名字的同一份剥法仍然是第二份剥法」，**本函数第一版就是那样，当场被它逮到**。
/// 顺序也照它的纪律：**先剥整份，再切块**（`strip_comment_lines` 头注的 `K-R25` 那一段）。
fn tools_literal_data() -> String {
    let me = guard_core::strip_comment_lines(include_str!("../../src/bridge/src/tool_registry.rs"));
    let opener = "pub const TOOLS: &[ToolSpec] = &[";
    let at = me.find(opener).expect("取不到 TOOLS 常量 —— 扫描器失效了");
    let start = at + opener.len();
    let mut depth = 1i32;
    let mut end = start;
    for (i, c) in me[start..].char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = start + i;
                    break;
                }
            }
            _ => {}
        }
    }
    assert!(end > start, "配对没找到收尾的 `]`");
    let body = &me[start..end];
    // 〔CP2b · 4C〕表里给人看的那几格进了文案表（`Text(|| copy_text("key", &[]))`）⇒ 「数据」＝
    // 源码体 ＋ 它引用的那几条表项的原文。只看源码体的话，措辞那一档（`Why::Wording`）
    // 从此永远数到 0 —— 字搬了家，尺子得跟着去新家量。
    let mut out = body.to_string();
    let mut rest = body;
    let needle = "copy_text(";
    while let Some(at) = rest.find(needle) {
        // `cargo fmt` 会把长调用拆行：`copy_text(` 与 key 之间可以隔着换行与缩进。
        let tail = rest[at + needle.len()..].trim_start();
        rest = tail;
        let Some(tail) = tail.strip_prefix('"') else {
            continue;
        };
        let key = &tail[..tail.find('"').expect("取文口的 key 没收尾")];
        out.push('\n');
        out.push_str(&crate::copy_table::copy_text(key, &[]));
        rest = tail;
    }
    out
}

/// ★ `KR81D2` **正面（零命中守卫）**：闭集那张表的**数据里**，旧名字一处都没有。
///
/// # 死值验（`KR81D2` 逐字要的那一向）
///
/// 把改过的任意一处改回旧名（`id:` 那一格、或 `display_name:` 那一格）⇒ 本条红。
///
/// # 它守什么、**不守什么**
///
/// 守的是**闭集的数据**。注释里的墓碑、别处 `.rs` 里的存量，本条一概不管 ——
/// 那一半归下面那条登记 ＋ 棘轮。**两条合起来才是那一格，单独任何一条都不够。**
#[test]
fn the_old_backend_name_is_gone_from_the_closed_set_itself() {
    let data = tools_literal_data();
    // 反向自检①：尺子够得着 —— 取到的真是那张表，不是一段空串或半截。
    assert!(
        data.len() > 8_000 && data.matches("ToolSpec {").count() == TOOLS.len(),
        "取到的 `TOOLS` 体不对：{} 字节 / {} 个 `ToolSpec {{`（应为 {} 个）——\
             先查提取器，别改断言",
        data.len(),
        data.matches("ToolSpec {").count(),
        TOOLS.len()
    );
    // 反向自检②：阳性对照 —— 把旧名字塞回一份副本里，量具必须数得出来。
    let poisoned = data.replace(
        &format!("id: \"{BACKEND_ID}\""),
        &format!("id: \"remote{}daemon\"", "-"),
    );
    assert_ne!(
        poisoned, data,
        "变异没落地：`id: \"{BACKEND_ID}\"` 没在那段里"
    );
    assert_eq!(
        old_name_counts(&poisoned)[1],
        1,
        "量具在阳性对照上数不出来 —— 它此刻无效，下面那条断言是空真"
    );
    // 正题
    assert_eq!(
        old_name_counts(&data),
        [0, 0, 0],
        "\n闭集那张表的**数据**里还留着旧名字（[符号名, 旧 id, 措辞]）。\n\
             用户 09-12 逐字：「一个后端要两处使用 / 即远程后端就是远程本地机器的后端」——\n\
             远端那台上跑的那一份是**那台机器的本地后端**，不是「远端的后端」。\n\
             ⚠ 这一格已经花过一次真钱（`ROADMAP#KU26`：Linux 裸 exe 装出来没有本机后端）。"
    );
}

/// ★ `KR81D2` **另一半（登记 ＋ 递减棘轮）**：还没改的每一处都登记着，而且只许变少。
///
/// # 它买的是「改了哪些 / 没改哪些 / 为什么」这句话**有分母**
///
/// 人群 = `src/bridge/src` ＋ `src/bridge/crates` 两棵树的 `.rs`（**现算**，不写死份数）。
/// 三向都判：
///   ① 盘上带旧名而 [`SITES`] 里没有 ⇒ 红（**别再往盘上加旧名**）；
///   ② `SITES` 里有而盘上已经没有 ⇒ 红（**账不许挂空号**）；
///   ③ 盘上比登记的多 ⇒ 红（棘轮只许降）。
///
/// # 🔴 射程与**刻意不管**的两样，写死在这里
///
/// - **crate 目录（`…-proto`）与包名（`cc-monitor-backend`）本拍不碰**，
///   而且它们**根本不进这把尺子**（`old_name_counts` 第一步就把那个拼写剥掉了）。
///   理由不是嫌麻烦：改那两样要**同拍**改发版流水线的产物名、`embedded-backends/`
///   的文件名约定、`tauri.sidecar.conf.json` 的 `externalBin`、`build.rs` 的清单
///   与 CI —— 而件文件 `§0d` 逐字「**不改发版流水线**（`KU26` 那一步归 `K-R42`）」。
///   ⇒ 它是**住址**，不是名字；名字改对了，住址跟着搬是另一件事。
/// - **`src/backend/` 那棵树**（另一个 workspace）与**前端 `src/**.ts`**
///   不在本尺子的面里。⚠ 这是**判不了**，不是「那边干净」——
///   现打：前端 30 余处、backend 树 26 处，逐条读数落在 `evidence/K-R81-….md`。
///
/// # ⚠ 本文件自己在面里（`K-R31` 那一形，`scanning_guard_registry` 登记为「第五形」）
///
/// ⚠ 〔`P4` 2026-09-21〕先前这里写着「`scan_tree!` 按构造摘掉调用者那一份 —— 而调用者
/// 恰恰是**闭集的家**，摘掉等于在最该看的那一份上瞎掉」。那一刀**在这一处不生效**
/// （判据由 `#[path]` 挂载 ⇒ `file!()` 是带 `..` 的折返路径 ⇒ 后缀比不命中），
/// 而且今天的调用者是本判据文件、**不是**闭集的家 `tool_registry.rs`。
/// ⇒ 下面那句 `include_str!` 的 `push` 今天是**冗余**的第二份（`src/` 那棵已经收过它，
/// 而 `got` 是按 `(住址, Why)` 入 `BTreeMap`，同键覆盖 ⇒ 不会数两遍）；**刻意不删**：
/// 它把「闭集的家一定在面里」钉成一件不依赖根清单的事。
/// ⇒ 而「本文件自己在面里」这句**今天是真的**：第三棵根逐字是 `tests/bridge`，
/// 本文件在里面，并且在 [`SITES`] 里有自己那一行（按等号认）。
/// 对价是本文件的针**全部运行期拼**（见 [`old_name_counts`]），否则量具自己会被自己数进去。
#[test]
fn every_place_that_still_says_the_old_name_is_registered_and_only_shrinks() {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_root = manifest.join("src");
    let crates_root = manifest.join("crates");
    let mut files: Vec<(PathBuf, String)> = guard_core::scan_tree!(&src_root, &["rs"]);
    files.extend(guard_core::scan_tree!(&crates_root, &["rs"]));
    // 🔴 〔搬树 2026-09-18 · `设计/16 §5.4b` 纪律 3〕monitor 这半边今天**第三棵树**：
    //    测试段整个住 `<repo>/tests/bridge`。少扫它 ⇒ 搬过去的那几处旧名字
    //    整批掉出人群，读起来像「债还了」，而那句话一个字没改。
    files.extend(guard_core::scan_tree!(
        &crate::guard_support::tests_root().join("bridge"),
        &["rs"]
    ));
    // 把自己那一份加回来（上面头注那一段说的就是这里）。
    files.push((
        manifest.join("src").join("tool_registry.rs"),
        include_str!("../../src/bridge/src/tool_registry.rs").to_string(),
    ));
    // 地板：尺子真的够得着一棵树（空集会让下面三向全部空真）。
    assert!(
        files.len() > 100,
        "扫描面只有 {} 份 `.rs` —— 剥法坏了，下面三向都是空真",
        files.len()
    );

    let mut got: BTreeMap<(String, Why), usize> = BTreeMap::new();
    for (p, text) in &files {
        // 🔴 〔搬树 2026-09-18〕住址两种前缀：本 crate 里的按 `manifest` 相对
        //    （`src/…` / `crates/…`，与表里既有的几十行同形），第三棵树 `tests/bridge`
        //    不在 `manifest` 下面 ⇒ 退回按**仓根**相对（`tests/bridge/…`）。
        //    不这么做的话那棵树的住址会印成绝对路径，表一写死就换台机器就假。
        let rel = p
            .strip_prefix(manifest)
            .or_else(|_| p.strip_prefix(crate::guard_support::repo_root()))
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/");
        for w in [Why::SymbolName, Why::OldId, Why::Wording] {
            let n = count_of(text, w);
            if n > 0 {
                got.insert((rel.clone(), w), n);
            }
        }
    }
    let want: BTreeMap<(String, Why), usize> = SITES
        .iter()
        .map(|(p, w, n)| (((*p).to_string(), *w), *n))
        .collect();

    let mut unregistered = Vec::new();
    let mut grown = Vec::new();
    for (k, n) in &got {
        match want.get(k) {
            None => unregistered.push(format!("{} · {:?} × {n}", k.0, k.1)),
            // 🔴 **逐格等号，不是 `<=`** —— 只判「涨了」的话，
            //    「把上限调上去让今天好过」这一手是**静默通过**的
            //    （`references/testing.md` 硬规则 12 明禁那一手，而纪律这一档
            //     在本仓已经被证伪过）。等号让那一手当场红。
            Some(cap) if n != cap => grown.push(format!(
                "{} · {:?}：登记 {cap}，盘上 {n}（{}）",
                k.0,
                k.1,
                if n > cap { "涨了" } else { "少了" }
            )),
            Some(_) => {}
        }
    }
    let stale: Vec<String> = want
        .keys()
        .filter(|k| !got.contains_key(*k))
        .map(|k| format!("{} · {:?}", k.0, k.1))
        .collect();

    assert!(
        unregistered.is_empty(),
        "\n这几处带着后端的**旧名字**而 `SITES` 里没有登记：\n  {}\n\n\
             ⇒ 两条出路：**把名字改对**（它是「后端」，在两台机器上各跑一份），\n\
             或者往 `SITES` 里加一行、并在 `Why` 那个闭集里说清**为什么这一拍改不动**。\n\
             ⚠ 不许为了变绿就往表里塞一行了事 —— 那正是这条账要防的。",
        unregistered.join("\n  ")
    );
    assert!(
        stale.is_empty(),
        "\n`SITES` 里这几行在盘上已经没有对应物了：\n  {}\n\n\
             ⇒ 那一处已经改完了，**把这一行删掉**（账不许挂空号：\n\
             一张挂着空号的表会让人以为债还在那儿，而它其实早还了）。",
        stale.join("\n  ")
    );
    assert!(
        grown.is_empty(),
        "\n旧名字的处数与登记对不上（这张账逐格按**等号**认）：\n  {}\n\n\
             ⇒ **涨了**：别再往盘上加旧名，也别把上限调上去让今天好过\n\
             （`references/testing.md` 硬规则 12 逐字：把上限调上去不是出路）。\n\
             ⇒ **少了**：好事 —— 把那一行的数**改小**，让这张账继续说真话。",
        grown.join("\n  ")
    );

    // 读数印出来 —— 「改了哪些 / 没改哪些」这句话要有一个**数**（现算，不写死）。
    let total: usize = got.values().sum();
    let by_kind: Vec<String> = [Why::SymbolName, Why::OldId, Why::Wording]
        .iter()
        .map(|w| {
            let n: usize = got.iter().filter(|(k, _)| k.1 == *w).map(|(_, v)| v).sum();
            format!("{w:?} {n}")
        })
        .collect();
    println!(
        "【KR81D2 存量读数】面 = `src/bridge/src` ＋ `src/bridge/crates` 共 {} 份 `.rs`（现算）· \
             还带旧名的 {} 份 / {total} 处（{}）· 登记 {} 行 · \
             ⚠ 面外判不了：`remote{}daemon{}proto/` 那棵树与前端 `src/**/*.ts`",
        files.len(),
        got.keys()
            .map(|k| k.0.clone())
            .collect::<std::collections::HashSet<_>>()
            .len(),
        by_kind.join(" · "),
        SITES.len(),
        "-",
        "-"
    );
}

/// **同一条纪律也管 [`Carrier`]** —— 同 `TouchedFile` 那条的理由：
/// 字段纪律只扫上面两层，审计那条手法**再下移一层仍然有效**。
#[test]
fn carrier_fields_follow_the_same_discipline() {
    let code = production_code(include_str!("../../src/bridge/src/tool_registry.rs"));
    field_discipline_of(&code, "Carrier", "Carrier")
        .require(4, "Carrier 字段纪律")
        .unwrap();
}

// ═══════════════════════════════════════════════════════════════════════
// 🔴 `K-R69` / `KR69D1`：**本机有一条 `ccm` 落点，而且它与远端那条同源**
// ═══════════════════════════════════════════════════════════════════════

/// 闭集里所有**末段是 `ccm` 那个词**的落点，按「在哪台机器上」分。
///
/// ⚠ 人群**现算**、不写死一个名单〔`13b`〕：`TOOLS` 是唯一一份，
/// 而 `ccm` 那个词的唯一住址是 `local_backend::CCM_ENTRY_WORD`。
/// 末段允许带一个尾 `*`（本机那条要盖住 Windows 上的 `.exe`，见它自己的 note）。
fn ccm_landing_sites() -> Vec<(&'static str, HostScope)> {
    let word = crate::backend::control::local_backend::CCM_ENTRY_WORD;
    TOOLS
        .iter()
        .flat_map(|t| t.touches())
        .filter(|f| {
            let last = f.path.rsplit('/').next().unwrap_or(f.path);
            last == word || last == format!("{word}*")
        })
        .map(|f| (f.path, f.host))
        .collect()
}

/// `KR69D1` 正面：**本机侧有落点，远端侧也有，而且它们不是同一条。**
///
/// # 立件时这一条是红的（那正是本件的题面）
///
/// 09-12 现打（量于 `79bf97d`）：闭集里末段是 `ccm` 的落点**恰好 1 条** ——
/// `TOOLS` 里 id 为 `ccm` 那条声明的 `~/.local/bin/ccm`，`host: Remote` ⇒ **本机侧 0 条**。
/// 而别名生成器（`launcher-diagnostics.ts::buildAliasLine`）吐的是**裸 `ccm`**，
/// 靠 PATH 解析 ⇒ 用户贴上去之后解析到的仍是他自己那份旧的。
/// ⇒ 用户 `K34` 逐字「装了新版后 `~/.local/bin/ccm` 可以干净退役」
/// **在结构上做不到**：退役没有承接方。
///
/// # 死值验（`KR69D1` 逐字要的第一向）
///
/// 把本机那条 `TouchedFile` 摘掉 ⇒ 本条**必须红**。
///
/// # ⚠ 它守什么、**不守什么**
///
/// 守的是**申报**（这张表里有没有这条落点、在哪台机器上）。
/// 「那个文件真的被放下去了吗」「放下去的是不是后端本体」由
/// `local_backend` 那两条管（`the_local_ccm_entry_is_a_copy_of_the_backend_itself`
/// 与 `the_resolution_path_really_puts_the_local_ccm_entry_down`）。
/// **三条合起来才是那一格，单独任何一条都不够。**
#[test]
fn the_closed_set_declares_a_ccm_landing_site_on_this_machine_too() {
    let sites = ccm_landing_sites();
    let local: Vec<&str> = sites
        .iter()
        .filter(|(_, h)| matches!(h, HostScope::Client | HostScope::Either))
        .map(|(p, _)| *p)
        .collect();
    let remote: Vec<&str> = sites
        .iter()
        .filter(|(_, h)| matches!(h, HostScope::Remote))
        .map(|(p, _)| *p)
        .collect();
    // 反向自检：人群塌了（没有任何 `ccm` 落点）时下面两条会**零命中地**分别红/绿，
    // 先把「尺子还够得着被测对象」这件事断出来。
    assert!(
        sites.len() >= 2,
        "闭集里末段是 `ccm` 的落点只有 {} 条（分母 = `TOOLS` 全部 touches，现算）——\n\
             本条要的是**两条**：本机一条、远端一条。实得 {sites:?}",
        sites.len()
    );
    assert!(
        !local.is_empty(),
        "闭集里**本机侧一条 `ccm` 落点都没有**（远端侧 {remote:?}）。\n\
             这正是 `K-R69` 立件时的读数：app 从来没有在本机装过 `ccm`，\n\
             于是用户 `K34` 逐字「装了新版后 `~/.local/bin/ccm` 可以干净退役」\n\
             **没有承接方** —— 缺的不是一次真机读数，是这个入口本身。"
    );
    assert!(
        !remote.is_empty(),
        "远端那条 `ccm` 落点没了 —— 那是 `sftp::deploy_remote_backend` 推过去的那份，\n\
             本件只**加**本机那条，不许把远端那条顺手弄丢（实得本机 {local:?}）"
    );
    // 两条不许是同一个路径：同一个串出现两次说明有人把 host 抄错了，
    // 而那时「本机有一条」是**靠一条远端的记录冒充的**。
    for l in &local {
        assert!(
            !remote.contains(l),
            "本机那条与远端那条是同一个路径 {l:?} —— 落点撞在一起，\n\
                 而本机那份**不许**写进 `~/.local/bin`：那是用户旧 `ccm` 住的地方，\n\
                 `K34` 逐字「原本的配置**要手动删除**」，产品一个字节都不动它。"
        );
    }
}

/// `KR69D1` 的**同源那一半（申报侧）**：申报的那条本机路径，
/// 真的能盖住我们生产上放下去的那个文件名。
///
/// # 没有这一条会怎样
///
/// 名字的唯一真相源是 `local_backend::local_ccm_entry_name()`（后缀由 `build.rs`
/// 按 `TARGET` 算，Windows 上是 `ccm.exe`）。这张表里写的是一个**常量串**。
/// 两边一漂，审计页会在 Windows 上对着一个**我们真的装了**的东西显示「缺失」——
/// 那正是本模块头注禁的「对能用的安装报假警报」。
#[test]
fn the_declared_local_ccm_path_really_matches_the_name_we_install() {
    let name = crate::backend::control::local_backend::local_ccm_entry_name();
    let declared: Vec<&str> = ccm_landing_sites()
        .into_iter()
        .filter(|(_, h)| matches!(h, HostScope::Client | HostScope::Either))
        .map(|(p, _)| p)
        .collect();
    assert_eq!(
        declared.len(),
        1,
        "本机侧的 `ccm` 落点不是恰好一条（实得 {declared:?}）——\
             多一条就是多一份要跟着改的东西（`K33`：所有命令只许有一处）"
    );
    let last = declared[0].rsplit('/').next().unwrap_or(declared[0]);
    let ok = match last.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => last == name,
    };
    assert!(
        ok,
        "闭集里申报的本机落点末段是 {last:?}，而生产上真放下去的名字是 {name:?} —— \n\
             两边漂了。名字的唯一真相源是 `local_backend::local_ccm_entry_name()`；\n\
             这一格漂开的后果不是编译错，是审计页在**装得好好的**机器上显示「缺失」。"
    );
}

/// 有围栏的块必须可卸载——否则用户没法干净地退出。
#[test]
fn fenced_block_implies_uninstallable() {
    for t in TOOLS {
        if t.touches().any(|f| f.effect == TouchEffect::FencedBlock) {
            assert!(
                t.uninstallable,
                "{} 往用户文件里插了围栏块，就必须能按围栏剥离",
                t.id
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// `K-R63`：**申报 ↔ 现实** —— 覆盖全表的一条性质，不是逐工具一条 `assert`
//
// # 病理（件文件 `§0b`）
//
// 这张表上两个 `bool`，先前**各只有半边被守**：
//   · `installable` —— 一条**专名**判据（只服务 `cc-bus` 一个工具）；
//   · `uninstallable` —— 上面那条 `fenced_block_implies_uninstallable` 只守
//     「有围栏 ⇒ 必须声明可卸」（**少报**那一向），**多报**（声明可卸而盘上
//     根本没有卸载实现）一条判据都没有。
// PM 09-11 的刀 C 实打（量于 `cd26954`）：把 `cc-acct-iso` 的 `uninstallable`
// 由 `false` 翻成 `true` ⇒ 全表 **1379 条一条没红**。
// 〔`K-R63` 实现方 09-11 在本件分支尖上复打同一刀：**红 1 条，就是下面这一条**
//  （`-p monitor` 基线 1384 → 1383 passed / 1 failed）。〕
//
// # 为什么处方不是「再补一条专名 `assert`」
//
// 那只是把静默从 1 个工具挪到下一个工具（`§0c` 逐字写死的失效方向）。
// ⇒ 做成**一条性质**：左边现读字段值，右边现钉盘上那个符号，两边 `assert_eq!`。
// ⚠ 失效方向也写死在判据的**名字**上：名字里出现任何一个工具 id ⇒ 不算兑现。
//    这一条自己也有判据（见下面那条自守）。
// ═══════════════════════════════════════════════════════════════════════

/// 一处**实现的住址**：给人读的 `<文件>.rs::<符号>` ＋ 给机器钉的**逐字签名**。
///
/// 两格互相校验（下面那条性质会断言符号名与签名里那个 `fn` 名逐字相等）：
/// 只留住址是一句没人核的话；只留签名，改了名没人读得出它指哪儿。
/// 而 `addr` 这一格**同时**被全仓那条符号地址判据盯着
/// （`structural_scan.rs::symbol_addresses` 抽、全仓解析）—— 实现改名 / 删掉，那一条先红。
struct ImplSite {
    addr: &'static str,
    definition: &'static str,
}

/// 一个工具的装 / 卸实现**住在哪份文件**。
///
/// `text` 是 `include_str!` 现取的整份内容，**不是一个路径串** —— 路径串会烂，
/// 而 `include_str!` 指错了地方**编都编不过**。
struct ImplHome {
    addr: &'static str,
    text: &'static str,
}

/// `TOOLS` 每一条的两格申报，各自该去盘上哪儿对拍。
///
/// `None` = **今天盘上根本没有这么一处**（形状抄 `fenced_block.rs::FENCE_SHAPES`
/// 的 `uninstall_site`）。
///
/// ⚠ **为什么缺口只能写 `None`，不能写一个「它将来会住哪」的地址**：一个不存在的
/// 符号一旦写成 `<文件>.rs::<符号>`，全仓那条符号地址判据当场把它判成
/// 「找不到这个符号」。⇒ 缺口用 `None` 表示，而「`None` 今天还成不成立」
/// 由下面那道**负向扫描**守着，不靠人记得 —— `remote-daemon` 正是栽在这一格上。
struct Claim {
    tool: &'static str,
    home: Option<ImplHome>,
    install: Option<ImplSite>,
    uninstall: Option<ImplSite>,
}

/// **唯一一份**对拍表。覆盖由下面那条性质的第 ① 步钉死（多一条少一条都红）。
fn claims() -> Vec<Claim> {
    const SFTP: &str = include_str!("../../src/bridge/src/sftp.rs");
    const PROFILE_INSTALLER: &str = include_str!("../../src/bridge/src/profile_installer.rs");
    const MCP: &str = include_str!("../../src/bridge/src/mcp.rs");
    const CC_BUS_DEPLOY: &str = include_str!("../../src/bridge/src/cc_bus_deploy.rs");
    // 〔AS2 · 第四波 4B〕skill「装到这台」的家。
    const SKILL_INSTALL: &str = include_str!("../../src/bridge/src/skill_install.rs");
    const ACCT_ISO_DEPLOY: &str = include_str!("../../src/bridge/src/acct_iso_deploy.rs");
    // 〔TL1 · 4C〕代码全景小程序的家（本机放 · 远端推，同一个入口 `push_to` 按 origin 分）。
    const PANORAMA_BYTES: &str = include_str!("../../src/bridge/src/panorama_bytes.rs");
    let sftp = || ImplHome {
        addr: "sftp.rs",
        text: SFTP,
    };
    let profile = || ImplHome {
        addr: "profile_installer.rs",
        text: PROFILE_INSTALLER,
    };
    // 两条 profile 系工具走的是**同一台安装器**（分岔在 `plan_install` / `plan_uninstall`，
    // 落盘那一整套共用）—— 与 `fenced_block.rs::FENCE_SHAPES` 里那两行同源。
    let profile_install = || {
        ImplSite {
        addr: "profile_installer.rs::install_to_profile",
        // 〔RW1 · 第四波 09-24〕签名变了：落盘经「门」（生产 = 本机后端的文件管理那一面），本进程不写。
        definition: "pub async fn install_to_profile(\n    door: &impl crate::user_files::Door,\n    path: &Path,\n    command_name: &str,\n    include_cc_function: bool,\n) -> Result<(), String> {",
    }
    };
    let profile_uninstall = || {
        ImplSite {
        addr: "profile_installer.rs::uninstall_from_profile",
        definition: "pub async fn uninstall_from_profile(\n    door: &impl crate::user_files::Door,\n    path: &Path,\n) -> Result<(), String> {",
    }
    };
    vec![
        Claim {
            tool: "ccm",
            home: Some(sftp()),
            install: Some(ImplSite {
                // 〔MC1 · 2026-09-24〕改名：「装 ccm 助手」→ 装别名块（推入口那一半并进了部署后端）。
                addr: "sftp.rs::install_remote_alias_block",
                definition: "pub async fn install_remote_alias_block(\n    cfg: RemoteConfig,\n    profile: String,\n) -> Result<String, String> {",
            }),
            uninstall: Some(ImplSite {
                addr: "sftp.rs::uninstall_remote_alias_block",
                definition: "pub async fn uninstall_remote_alias_block(\n    cfg: RemoteConfig,\n    profile: String,\n) -> Result<String, String> {",
            }),
        },
        Claim {
            tool: "cc-bus",
            home: Some(ImplHome {
                addr: "cc_bus_deploy.rs",
                text: CC_BUS_DEPLOY,
            }),
            install: Some(ImplSite {
                addr: "cc_bus_deploy.rs::deploy_local_cc_bus",
                definition: "pub async fn deploy_local_cc_bus() -> Result<CcBusDeployReport, String> {",
            }),
            uninstall: None,
        },
        Claim {
            tool: "cc-acct-iso",
            home: Some(ImplHome {
                addr: "acct_iso_deploy.rs",
                text: ACCT_ISO_DEPLOY,
            }),
            install: Some(ImplSite {
                addr: "acct_iso_deploy.rs::deploy_remote_acct_iso",
                definition: "pub async fn deploy_remote_acct_iso(cfg: RemoteConfig, dest_dir: String) -> Result<String, String> {",
            }),
            uninstall: None,
        },
        Claim {
            tool: "backend",
            home: Some(sftp()),
            install: Some(ImplSite {
                addr: "sftp.rs::deploy_remote_backend",
                definition: "pub async fn deploy_remote_backend(cfg: RemoteConfig) -> Result<String, String> {",
            }),
            uninstall: Some(ImplSite {
                addr: "sftp.rs::uninstall_remote_backend",
                definition: "pub async fn uninstall_remote_backend(cfg: RemoteConfig) -> Result<String, String> {",
            }),
        },
        // 〔TL1 · 4C〕代码全景小程序：装口 `push_to`（本机那一臂落 `place_local`，远端那一臂经那台后端的文件链路推）；
        //   **没有卸口**（`uninstall: None`，负向扫描守着这个家）。
        Claim {
            tool: "panorama",
            home: Some(ImplHome {
                addr: "panorama_bytes.rs",
                text: PANORAMA_BYTES,
            }),
            install: Some(ImplSite {
                addr: "panorama_bytes.rs::push_to",
                definition: "pub(crate) async fn push_to(origin: &crate::origin::Origin) -> Result<(), String> {",
            }),
            uninstall: None,
        },
        Claim {
            tool: "project-mcp",
            home: Some(ImplHome {
                addr: "mcp.rs",
                text: MCP,
            }),
            install: Some(ImplSite {
                addr: "mcp.rs::write_project_mcp_server",
                // 🔴 〔步 12·C 收尾 09-20〕签名多了一个 `origin: Origin` —— 本机与远端
                //    两条 MCP 写命令合成了一条。**逐字签名是钉住实现的那把锁**，
                //    实现真的变了就得跟着改；判法（住址 ↔ 逐字签名互校）一个字没动。
                definition: "pub async fn write_project_mcp_server(\n    origin: Origin,\n    project_dir: String,\n    name: String,\n    server: Value,\n) -> Result<(), String> {",
            }),
            uninstall: Some(ImplSite {
                addr: "mcp.rs::remove_project_mcp_server",
                // 同上那一条：`remove_remote_mcp_server` 并进来之后，签名多了 `origin`
                // 并因此被 rustfmt 折成多行。
                definition: "pub async fn remove_project_mcp_server(\n    origin: Origin,\n    project_dir: String,\n    name: String,\n) -> Result<(), String> {",
            }),
        },
        // 〔AS2 · 第四波 4B · V113〕资产目录里「装到这台」的 skill：装口是 `skill_install_apply`（经那台后端 `files-put`）；
        //   **没有卸口**（`uninstall: None`，负向扫描守着：这个家里长出一个 `uninstall… / remove… / strip… / purge…` 就红）。
        Claim {
            tool: "skill-install",
            home: Some(ImplHome {
                addr: "skill_install.rs",
                text: SKILL_INSTALL,
            }),
            install: Some(ImplSite {
                addr: "skill_install.rs::skill_install_apply",
                definition: "pub async fn skill_install_apply(\n    to: Origin,\n    name: String,\n    source: Vec<SkillFile>,\n    target: Vec<SkillTargetText>,\n    take: Vec<String>,\n    overwrite: Vec<String>,\n) -> Result<SkillInstallApplied, String> {",
            }),
            uninstall: None,
        },
        Claim {
            tool: "posix-rc-aliases",
            home: Some(profile()),
            install: Some(profile_install()),
            uninstall: Some(profile_uninstall()),
        },
        Claim {
            tool: "powershell-profile",
            home: Some(profile()),
            install: Some(profile_install()),
            uninstall: Some(profile_uninstall()),
        },
        // **我们从不装它** ⇒ 没有「它的实现该住哪」这回事。这一行的 `home: None`
        // 不是手写的豁免：下面那条性质断言 `home.is_none()` **当且仅当**
        // `destination` 是 `NotInstalledByUs` —— 换句话说这一格由类型系统里那个
        // 变体说了算，不由填表的人说了算。
        Claim {
            tool: "claude-code",
            home: None,
            install: None,
            uninstall: None,
        },
    ]
}

/// 从逐字签名里抠出 `pin_definition` 要的**赋值前缀**（签名到 `fn <名>` 之后第一个 `(` 为止）。
/// **算出来的，不再写第二份字面量**〔`13b`〕。
/// 〔TL1 · 4C〕从前是「到第一个 `(` 为止」—— 可见性带括号（`pub(crate)`）时会抠成 `pub`；`panorama` 那一行
/// 的装口是 `pub(crate)`，当场逮住（住址 `…::push_to` 对上抠出来的 `…::pub`）。
fn assign_prefix_of(definition: &str) -> &str {
    let from = definition.find("fn ").unwrap_or(0);
    let end = definition[from..]
        .find('(')
        .map_or(definition.len(), |i| from + i);
    definition[..end].trim_end()
}

/// 从逐字签名里抠出那个 `fn` 名。
fn fn_name_of(definition: &str) -> &str {
    assign_prefix_of(definition)
        .rsplit(' ')
        .next()
        .unwrap_or_default()
}

/// ★★ `KR63D1` ＋ `KR63D2`：**`TOOLS` 每一条的两格申报，都必须与盘上真有没有那个实现一致。**
///
/// 左边现读字段值（`installable` / `uninstallable`），右边用
/// `structural_scan.rs::pin_definition` 去钉那个装 / 卸实现的**逐字签名**
/// （它同时守住「只被定义一次」：追加一个同名定义也会红）。两边 `assert_eq!`。
///
/// **两个方向都判**：
///   · 多报（声明可装 / 可卸而实现不在）⇒ 右边 `false`、左边 `true` ⇒ 红；
///   · 少报（实现在而字段写着不行）⇒ 反过来 ⇒ 红。
///     少报**不是假想** —— 本条落地当场逮到 `remote-daemon`：`uninstall_remote_backend`
///     是设置面板上的按钮，而字段写着 `uninstallable: false`。
///
/// **`None` 那一侧靠负向扫描兜底**（不然「今天没有卸口」这句话永远没人核）：
/// 那个工具的家里出现一个**没人认领**的 `fn uninstall… / remove… / strip… / purge…`
/// ⇒ 红。动词表的分母如实写在这里：**登记过的就这四个**，不是穷举 ——
/// 一个叫别的名字的卸载实现今天扫不到，那一格判不了，不假装覆盖。
///
/// ⚠ **装那一侧今天没有负向扫描，而这不是漏写**：`install: None` 的行今天只有
/// `claude-code` 一条，它连家都没有（`NotInstalledByUs`）⇒ 人群是空的，
/// 写一条扫不到任何东西的扫描买不到牙。真出现「有家而声明装不了」的行，
/// 下面那一支会 `panic!` 点名，**不会静默放过**。
#[test]
fn every_tool_declares_install_and_uninstall_as_the_implementations_really_are() {
    use crate::structural_scan::pin_definition;

    let claims = claims();

    // ① 覆盖：与 `TOOLS` 一一对应，多一条少一条都红（这一步买的是「全表」二字）。
    let mut got: Vec<&str> = claims.iter().map(|c| c.tool).collect();
    let mut want: Vec<&str> = TOOLS.iter().map(|t| t.id).collect();
    got.sort_unstable();
    want.sort_unstable();
    assert_eq!(
        got, want,
        "对拍表与 TOOLS 对不上 —— 加了工具却没登记它的装 / 卸实现住哪，\
             那一条就悄悄不在这条性质的射程里了"
    );

    // ② 反向自检：`pin_definition` 真的会说「不在」（否则下面全是空真）。
    assert!(pin_definition("fn a() {}\n", "fn b() {}", "fn b", "自检").is_err());
    assert!(pin_definition("fn a() {}\n", "fn a() {}", "fn a", "自检").is_ok());
    // ②b 反向自检：负向扫描在**真树**上不是零命中的（零命中 ⇒ 那一半是空真）。
    assert!(
        crate::structural_scan::fn_names_starting_with(
            include_str!("../../src/bridge/src/sftp.rs"),
            &["uninstall"]
        )
        .contains(&"uninstall_remote_backend".to_string()),
        "负向扫描在真树上零命中 —— 它此刻无效，先查剥法别改断言"
    );

    // 认领集：哪些实现符号已经被某一行认走了（负向扫描要用）。
    let claimed: HashSet<&str> = claims
        .iter()
        .flat_map(|c| [c.install.as_ref(), c.uninstall.as_ref()])
        .flatten()
        .map(|s| fn_name_of(s.definition))
        .collect();

    let mut checked = 0usize;
    for t in TOOLS {
        let c = claims.iter().find(|c| c.tool == t.id).expect("① 已经钉过");

        // ③ 「有没有家」不由填表的人说了算，由 `destination` 那个变体说了算。
        //    🔴 〔`K-R81`〕多载体之后走 `Provisioning::of_tool` 的**聚合规则**
        //    （全部载体都是 `NotInstalledByUs` 才算「不是装出来的」）——
        //    那条规则只许有一个住址，这里不再手写第二份 `matches!`〔`13b`〕。
        assert_eq!(
            c.home.is_none(),
            Provisioning::of_tool(t) == Provisioning::NotAnInstall,
            "`{}`：`home` 那一格与 `destination` 打架 —— 「我们不装它」与\
                 「它的装 / 卸实现住在某份文件里」有一句是假的",
            t.id
        );

        for (field, declared, site, verbs) in [
            ("installable", t.installable, c.install.as_ref(), &[][..]),
            (
                "uninstallable",
                t.uninstallable,
                c.uninstall.as_ref(),
                &["uninstall", "remove", "strip", "purge"][..],
            ),
        ] {
            checked += 1;
            let real = match (c.home.as_ref(), site) {
                (Some(home), Some(s)) => {
                    // 住址与签名互相校验：符号名必须逐字相等，文件必须就是那个家。
                    assert_eq!(
                        s.addr,
                        format!("{}::{}", home.addr, fn_name_of(s.definition)),
                        "`{}` 的 `{field}` 那一格：住址与逐字签名对不上 —— \
                             其中一格是摆设",
                        t.id
                    );
                    pin_definition(
                        home.text,
                        s.definition,
                        assign_prefix_of(s.definition),
                        &format!("`{}` 的 `{field}` 背后那个实现", t.id),
                    )
                    .is_ok()
                }
                (_, None) => {
                    // 负向扫描：家里躺着一个没人认领的同族实现 ⇒ 「今天没有」这句话是假的。
                    if let Some(home) = c.home.as_ref() {
                        if verbs.is_empty() {
                            panic!(
                                "`{}` 声明 `{field}: {declared}` 而没有登记实现住址，\
                                     它却有家（{}）—— 这一形今天没有负向扫描，\
                                     写一条再走（别静默放过）",
                                t.id, home.addr
                            );
                        }
                        let stray: Vec<String> =
                            crate::structural_scan::fn_names_starting_with(home.text, verbs)
                                .into_iter()
                                .filter(|n| !claimed.contains(n.as_str()))
                                .collect();
                        assert!(
                            stray.is_empty(),
                            "`{}` 的 `{field}` 登记着「今天盘上没有这么一处」，\
                                 而它家（{}）里躺着没人认领的 {stray:?} —— \
                                 要么它就是那个实现（那就登记进对拍表并把字段翻过来），\
                                 要么它不是（那就说清它是什么）。\n\
                                 ⚠ `remote-daemon` 就是这么假申报了一个月的。",
                            t.id,
                            home.addr
                        );
                    }
                    false
                }
                (None, Some(s)) => panic!(
                    "`{}` 没有家，却给 `{field}` 登记了实现住址 {:?}",
                    t.id, s.addr
                ),
            };
            assert_eq!(
                declared,
                real,
                "`{}` 的 `{field}` 申报为 {declared}，而盘上那个实现{}。\n\
                     这两句话必须一致 —— 配置面那一列「能否装/撤」直接印到用户眼前：\n\
                     多报 = 一个点了没反应的按钮；少报 = 按钮就在旁边而页面写着做不到。\n\
                     ⚠ 只改注释没有用：本条读的是**字段值**，不是注释里的词频。",
                t.id,
                if real { "在" } else { "不在" }
            );
        }
    }
    // 计数自检：两格 × 全表，一格都没跳过。
    assert_eq!(
        checked,
        2 * TOOLS.len(),
        "只对拍了 {checked} 格，而全表应有 {} 格",
        2 * TOOLS.len()
    );
}

/// ★★ `KR63D1` / `KR63D2` 把**失效方向**写死在名字上：
/// **判据的名字里出现任何一个工具 id ⇒ 本件不算兑现。**
///
/// 一条名字里带工具名的判据，读的人会以为「那一格有人守」，而它守的只有那一个工具 ——
/// `K-R60` 留下的就是这样一颗钉子，`K-R63` 把它收掉了。这一条让那件事**别再回来**。
#[test]
fn the_property_that_pins_both_declarations_is_not_named_after_any_tool() {
    const NAME: &str =
        "every_tool_declares_install_and_uninstall_as_the_implementations_really_are";
    // 🔴 〔步 7c 剖分 2026-09-19 · C 类〕嵌的是**本文件** —— 被反向自检点名的那条
    // `fn every_tool_declares_install_and_uninstall_as_the_implementations_really_are`
    // 这一轮跟着测试段搬进了本文件。
    let me = include_str!("tool_registry_tests.rs");
    // 反向自检：那条判据真的叫这个名字（改了名而没改这里 ⇒ 本条先红）。
    assert_eq!(
        me.matches(&format!("fn {NAME}(")).count(),
        1,
        "本文件里找不到（或不止一个）`{NAME}` —— 它改名了，本条此刻在空转"
    );
    for t in TOOLS {
        let snake = t.id.replace('-', "_");
        assert!(
            !NAME.contains(t.id) && !NAME.contains(&snake),
            "判据名 `{NAME}` 里出现了工具 id `{}`（或它的 snake 形 `{snake}`）—— \
                 那就又是一颗只服务一个工具的钉子",
            t.id
        );
    }
}
