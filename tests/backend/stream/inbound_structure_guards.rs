/// ★ 本文件**不许出现任何 `observe::`**。
///
/// 头注宣称了这条，但 D 审计指出它**没有机检** —— `layering_guard::layer_sources`
/// 只遍历 `src/observe` 与 `src/control`，顶层的入方向（今天是 `stream/inbound/` 这个目录）不在采集面内。
/// 变异 `use crate::observe::watcher as _;` 之后全量 211 passed。
///
/// 在一份通篇强调「跨两处的约束必须机检」的文件里，这条自己是注释。现在不是了。
#[test]
fn inbound_never_reaches_into_the_observe_layer() {
    // 人群 = 入方向整个目录（收行分派 · 规格 · 排空 · 门 · 命令表各族）。
    let src: String = guard_core::addr::backend_inbound_sources()
        .into_iter()
        .map(|(_, prod)| prod)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        src.len() > 3000,
        "只剥出 {} 字节生产段 —— 抽取坏了，本断言在空转",
        src.len()
    );
    let hits: Vec<&str> = src
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("observe::"))
        .collect();
    assert!(
        hits.is_empty(),
        "入方向伸进了读面：{hits:?}\n\
             §1.1 的线是按**职责**画的：入方向是传输 + 控制，读面的事不归它。\n\
             依赖方向只许 `inbound → control`。"
    );
}

/// ★ **`id` 不许被解析**（F90 的代码强制）。
///
/// F90 说「登记表主键必须 opaque + 稳定」。今天 `running` 表的主键就是客户端给的
/// 不透明 `id`，天然合规 —— 本条防的是**后人「顺手」从 id 里抠信息**
/// （比如约定 `sid:xxx` 前缀然后 `strip_prefix`）。一旦那样，`id` 就不再不透明，
/// 客户端换个格式就崩，而且后端会开始依赖一个它无权定义的结构。
#[test]
fn the_request_id_is_never_parsed() {
    // 人群 = 入方向整个目录（同上一条）。
    let src: String = guard_core::addr::backend_inbound_sources()
        .into_iter()
        .map(|(_, prod)| prod)
        .collect::<Vec<_>>()
        .join("\n");
    let banned = [
        ".parse",
        ".split",
        ".strip_prefix",
        ".strip_suffix",
        ".starts_with",
        ".ends_with",
    ];
    let hits: Vec<String> = src
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("id.") || l.contains("id_for_task.") || l.contains("id_sup."))
        .filter(|l| banned.iter().any(|b| l.contains(b)))
        .map(|l| l.to_string())
        .collect();
    assert!(
        hits.is_empty(),
        "有人在解析 `id`：{hits:?}\n\
             它是**客户端给的不透明串** —— backend 只许 clone / 比较 / 回显。\n\
             从里面抠信息 = 让后端依赖一个它无权定义的结构（F90）。"
    );
}

/// ★ `hello.commands` 就是注册表的名字、按字母排、无重（[`super::command_names`]）。
///
/// 此前这里钉的是手抄的名单 `COMMANDS` 与注册表两向相等（那面镜子删了，名字只住注册表一处）。
/// 剩下要钉的：hello 真的读它（不是另起一份）、它有序（上线的能力集稳定可读，与镜子那时逐字节相同）、
/// 名字不重（`lookup` 取第一个，重复会让后一条静默失效）。
#[test]
fn hello_commands_are_the_registry_names_in_order() {
    let names = super::command_names();
    assert!(
        names.len() >= 4,
        "注册表只有 {} 条 —— 本断言在空转",
        names.len()
    );
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted, "`command_names()` 不是去重后的字典序");
    let from_registry: std::collections::BTreeSet<&str> =
        super::REGISTRY.iter().map(|s| s.name).collect();
    assert_eq!(
        names
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        from_registry
    );
    // hello 那一格读的就是它（`main.rs` 拼握手帧那一处）。
    let main = guard_core::production_code(include_str!("../../../src/backend/main.rs"));
    guard_core::find_pinned(&main, "commands: inbound::command_names()")
        .expect("`main.rs` 的 hello 不再从 `inbound::command_names()` 取 `commands`");
}

/// ★ **命令表的族划分**：`registry/` 目录下的 `.rs` 文件集合 == `FAMILIES` 列的模块集合（两向，文件系统现取）；
/// 各族的命令名两两不交；`REGISTRY` 里名字无重。
///
/// 为什么要它：一份族文件放进 `registry/` 却没进 `FAMILIES`，它的命令**一条都不会编进** `REGISTRY`，
/// 而别的判据都只看编进去的那张表 —— 那几条命令会静默消失（发过去只回 `unknown_command`）。
/// 两侧异源：一侧是盘上的文件，一侧是 `mod.rs` 里 `FAMILIES` 那张表的源码（再与编进去的族数对一次）。
#[test]
fn the_registry_families_are_exactly_the_files_in_the_registry_directory() {
    let sources = guard_core::addr::backend_inbound_sources();
    let on_disk: std::collections::BTreeSet<String> = sources
        .iter()
        .filter_map(|(rel, _)| rel.strip_prefix("src/backend/stream/inbound/registry/"))
        .map(|f| f.trim_end_matches(".rs").to_string())
        .collect();
    let (_, mod_rs) = sources
        .iter()
        .find(|(rel, _)| rel == "src/backend/stream/inbound/mod.rs")
        .expect("入方向目录里没有 mod.rs");
    let at = guard_core::find_pinned(mod_rs, "const FAMILIES: &[&[super::CommandSpec]] = &[")
        .expect("`mod.rs` 里锚不住 `FAMILIES`");
    let table = &mod_rs[at..at + mod_rs[at..].find("];").expect("`FAMILIES` 没收尾")];
    let listed: Vec<String> = table
        .split("::SPECS")
        .map(|head| {
            head.chars()
                .rev()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        })
        .filter(|name| !name.is_empty())
        .collect();
    let in_table: std::collections::BTreeSet<String> = listed.iter().cloned().collect();
    assert_eq!(
        listed.len(),
        in_table.len(),
        "`FAMILIES` 里有一族列了两遍：{listed:?}"
    );
    assert_eq!(
        listed.len(),
        super::registry::FAMILIES.len(),
        "从源码数出的族数与编进去的 `FAMILIES` 对不上 —— 数法坏了"
    );
    assert!(
        on_disk.len() >= 2,
        "`registry/` 下只扫到 {} 份 —— 遍历坏了，下面那条相等在空集上成立",
        on_disk.len()
    );
    assert_eq!(
        on_disk,
        in_table,
        "\n`registry/` 下的文件与 `FAMILIES` 列的族对不上。\n  \
         盘上有、表里没有（🔴 **那一族的命令一条都没编进去**）：{:?}\n  \
         表里有、盘上没有：{:?}",
        on_disk.difference(&in_table).collect::<Vec<_>>(),
        in_table.difference(&on_disk).collect::<Vec<_>>()
    );
    // 各族两两不交，拼起来的 `REGISTRY` 不多不少、名字无重。
    let mut seen: std::collections::BTreeMap<&str, usize> = Default::default();
    for (i, family) in super::registry::FAMILIES.iter().enumerate() {
        for spec in family.iter() {
            if let Some(j) = seen.insert(spec.name, i) {
                panic!(
                    "`{}` 同时登记在 `FAMILIES` 的第 {j} 族与第 {i} 族 —— 同一个名字只许落一族",
                    spec.name
                );
            }
        }
    }
    let total: usize = super::registry::FAMILIES.iter().map(|f| f.len()).sum();
    assert_eq!(
        super::REGISTRY.len(),
        total,
        "`REGISTRY` 的条数不是各族之和"
    );
    let names: std::collections::BTreeSet<&str> = super::REGISTRY.iter().map(|s| s.name).collect();
    assert_eq!(
        names.len(),
        super::REGISTRY.len(),
        "`REGISTRY` 里有重名命令"
    );
}

/// ★★ **`files-read` 上线的那四条，登记的就是它声明的那四条。**
///
/// # 它治的是哪一形
///
/// 这一族的能力声明住 `files::CAPABILITIES`（那张表，由
/// `files::tests::the_capability_names_match_the_design_registry_in_both_directions`
/// 两向钉着），而线上那一面是**第二张表**（本文件的 `REGISTRY`）。
/// 两张手写表之间没有判据 = 「上线少接一条」「契约面抄漏一个字段」都静默 ——
/// 而那正是 `p1t-removal-cause` 那次真 bug 的形状（表里有、分派没有）。
///
/// ⇒ 本条把两张表**判相等**，三向都判：
/// ① 命令名集合（经 `-`↔`.` 那一条翻译）**两向相等**；
/// ② 每条的 `codes` 逐字相等；
/// ③ 每条的 `fields` == 那条能力的 `args` ∪ `fields`（排序去重后**相等**）。
///
/// ⚠ 它**买不到**「文档写得对」（那是 `protocol_doc_guard` 那几条的活），
/// 也买不到「`files::CAPABILITIES` 自己说的是实话」——
/// 那一侧由 `tests/backend/files/capability_guard.rs` 的实打对拍钉着
/// （出方向字段是**真调一次**比出来的）。本条只钉「两张表没有分叉」。
#[test]
fn the_files_read_family_is_online_exactly_as_it_is_declared() {
    // ④ 那一步真调每一条读能力：`files-index-rebuild` 不给 `path` 就重走家目录、装上常驻那一份 ⇒ 拿那把唯一的锁
    //   （不拿 ⇒ 与 `files::index` 那几条「清空了再查」并发时把状态换掉，CI 上并行跑时现红过）。
    let _lock = crate::files::index::tests::resident_lock();
    // ① 名字：能力名把 `.` 换成 `-` 就是线上名（反向那一半住 `files::answer_wire`）。
    let declared: std::collections::BTreeSet<String> = crate::files::capability_names()
        .into_iter()
        .map(|n| n.replace('.', "-"))
        .collect();
    assert!(
        declared.len() >= 4,
        "`files::CAPABILITIES` 只声明了 {} 条 —— 本条在空转",
        declared.len()
    );
    // 反向替换够用的前提：能力名里**没有** `-`。它一旦被破坏，`answer_wire` 会静默地
    // 去查一个不存在的能力名，而上面那个映射照样对得上。
    for n in crate::files::capability_names() {
        assert!(
            !n.contains('-'),
            "能力名 `{n}` 里有 `-` —— `files::answer_wire` 的反向替换从此会翻错，\n\
             而名字集合那一半照样绿。要么换能力名，要么把翻译改成一张显式的表。"
        );
    }
    // 🔴 `files-` 这个线上前缀底下**从此有两族**，而它们**互不相交**：
    //   读那一族的住址是 `files::CAPABILITIES`（`src/backend/files/`，整族纯读），
    //   写那一面的住址是 `control::files_write::MANAGE_COMMANDS`（`control/`，会改变世界）。
    //   ⇒ 本条的人群必须**先把写那一面减掉**，否则它会把写那一面当成「读族漏声明了一条」
    //   （现打：不减的话逐字报 `files-create` 在 wired 里而不在 declared 里）。
    //   ⚠ 减法是**按那张表**减，不是按名字手写一份 —— 手写第二份就是下一个漂移源。
    // 写那一侧从此是**两张表**：写面 `MANAGE_COMMANDS` ＋ 上传提交
    //   `files_commit::COMMIT_COMMANDS`（写经 `Face`，同一族）。减法按两张表的并。
    let write_face: std::collections::BTreeSet<String> =
        crate::control::files_write::MANAGE_COMMANDS
            .iter()
            .map(|c| c.name)
            .collect::<Vec<_>>()
            .into_iter()
            .chain(
                crate::control::files_commit::COMMIT_COMMANDS
                    .iter()
                    .map(|c| c.name)
                    .collect::<Vec<_>>(),
            )
            // 解压（`control/files_extract.rs`，写经 `Face`）。
            .chain(
                crate::control::files_extract::EXTRACT_COMMANDS
                    .iter()
                    .map(|c| c.name)
                    .collect::<Vec<_>>(),
            )
            // 撤一组复制 · 删除 · 改权限（`control/files_batch.rs`）：写那一族的，只立旗、不碰盘。
            .chain([<crate::stream::inbound::files_write_face::FilesBatchStop as crate::wire::typed::Command>::NAME])
            .map(str::to_string)
            .collect();
    assert!(
        !write_face.is_empty(),
        "写那一面的登记表是空的 —— 下面那个减法退化成恒等，本条的分家没在钉任何东西"
    );
    // 两族**不许有交集**：同一个线上名要是两边都声明，`inbound::REGISTRY` 上那一条
    // 到底调哪一族就成了「谁先被 match 到」，而那不是契约。
    let overlap: Vec<&String> = write_face.intersection(&declared).collect();
    assert!(
        overlap.is_empty(),
        "这几个线上名同时被读族与写面声明了：{overlap:?}\n\
         ⇒ 那一条命令调哪一族取决于分派的书写顺序，而不是声明 —— 必须改掉其中一侧的名字。"
    );
    let all_files_prefixed: std::collections::BTreeSet<String> = super::REGISTRY
        .iter()
        .map(|s| s.name)
        .filter(|n| n.starts_with("files-"))
        .map(str::to_string)
        .collect();
    let wired: std::collections::BTreeSet<String> = all_files_prefixed
        .difference(&write_face)
        .cloned()
        .collect();
    assert_eq!(
        wired, declared,
        "\n`files-read` 上线的那一组与 `files::CAPABILITIES` 声明的那一组对不上。\n\
         少接一条的后果是静默的：能力在、判据在，而调用方发过去只会拿到 `unknown_command`。"
    );
    // ★ 写那一面的**同形相等断言**：登记表 ↔ `REGISTRY` 上真上线的那几条，两向。
    let wired_write: std::collections::BTreeSet<String> = all_files_prefixed
        .intersection(&write_face)
        .cloned()
        .collect();
    assert_eq!(
        wired_write, write_face,
        "\n文件管理**写**面登记的那一组与 `inbound::REGISTRY` 上真上线的那一组对不上。\n\
         登记了而没上线 ⇒ 调用方发过去拿到 `unknown_command`（静默不可用）；\n\
         上线了而没登记 ⇒ 🔴 **写能力长出了一个没签字的命令** ——\n\
         而 `readonly_guard` 那条「拿得到 `Face` 的命令 == 写面登记的命令」判的正是这张表。"
    );
    // ★ 写那一面的每条命令：喂空 `args` 落在它登记的码上（码表从错误枚举来）。
    for name in &write_face {
        let spec = super::REGISTRY
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("`{name}` 不在 REGISTRY 上 —— 上面那条应该先红"));
        // 真的调一次：喂空 `args` ⇒ 落在这条命令登记的码上（撤一组那一条不写盘、不经写面那一扇，不在这里调）。
        if name == <crate::stream::inbound::files_write_face::FilesBatchStop as crate::wire::typed::Command>::NAME {
            continue;
        }
        if let Err(crate::control::files_write::WriteFail {
            code, said: msg, ..
        }) = crate::control::files_write::tests::ask(name, &serde_json::json!({}))
        {
            assert!(
                spec.codes.contains(&code),
                "`{name}` 回了一个它没登记的 code `{code}`（{msg}）"
            );
        }
    }

    // ② ③ 逐条：读的那几条都在登记表上。
    let mut checked = 0usize;
    for cap in crate::files::CAPABILITIES {
        let online = cap.name.replace('.', "-");
        let spec = super::REGISTRY
            .iter()
            .find(|s| s.name == online)
            .unwrap_or_else(|| panic!("`{online}` 不在 REGISTRY 上 —— 上面那条应该先红"));
        checked += 1;

        // ④ 真的调一次：喂空 `args` ⇒ 要么成功，要么落在这条命令登记的码上；落到 `unknown_capability` 就说明能力接错了。
        let out =
            crate::files::tests::answer_wire(&online, &serde_json::json!({}), &Default::default());
        if let Err(crate::files::Refused {
            code, said: msg, ..
        }) = out
        {
            assert_ne!(
                code, "unknown_capability",
                "`{online}` 够不到任何能力（{msg}）"
            );
            assert!(
                spec.codes.contains(&code),
                "`{online}` 回了一个它没登记的 code `{code}`（{msg}）"
            );
        }
    }
    assert_eq!(
        checked,
        crate::files::CAPABILITIES.len(),
        "只逐条判过 {checked} 条 —— 本条在空转"
    );
}

/// 帧命令名与它们派生出来的 CLI 面里零个带宿主名 `tmux` 的（名字按「终端」叫，宿主只是实现）。
/// 人群从 `REGISTRY` 现取；正控：同一个找法在真表 `SUBCOMMANDS` 里认得出手写的 `--tmux-notify`（tmux 钩子入口，不由帧命令派生）。
#[test]
fn no_frame_command_or_derived_cli_flag_names_the_tmux_host() {
    let names_tmux = |s: &str| s.split(['-', '_']).any(|w| w == "tmux");
    let frame: Vec<&str> = super::REGISTRY
        .iter()
        .map(|s| s.name)
        .filter(|n| names_tmux(n))
        .collect();
    let derived: Vec<String> = super::REGISTRY
        .iter()
        .filter(|s| crate::stream::cli::cli_exposed(s))
        .map(|s| crate::stream::cli::flag_of(s.name))
        .filter(|f| names_tmux(f))
        .collect();
    assert!(
        super::REGISTRY.len() > 50,
        "只取到 {} 条帧命令 —— 人群塌了",
        super::REGISTRY.len()
    );
    assert_eq!(frame, Vec::<&str>::new(), "帧命令名里带了宿主名 tmux");
    assert_eq!(
        derived,
        Vec::<String>::new(),
        "派生出来的 CLI 面带了宿主名 tmux"
    );
    assert!(
        crate::SUBCOMMANDS.iter().any(|f| names_tmux(f)),
        "正控失败：同一个找法在 `SUBCOMMANDS` 里认不出 `--tmux-notify` —— 上面的零命中不可信"
    );
}

/// ★ **收了、校验了、然后丢掉** 的入参：`xxx_arg(args, "名", …)?;` 整句丢弃返回值（或 `let _ = xxx_arg(…)`）。
///
/// 那一格在注册表的字段表里写着、在协议文档里写着，调用方照着它画按钮 —— 而处理函数只验了它的形状、从不读它的值。
/// `terminal-input` 的 `take` 就是这样活了几个月（第二个前端画了一颗「接管输入」，按下去什么都不变）。
/// 要它真不起作用就删掉它（字段表 · 校验 · 调用方 · 文档一起），别留一个只校验不用的格。
#[test]
fn no_handler_validates_an_arg_and_then_throws_it_away() {
    // 扫 `src/backend`（本条住 `tests/backend/`，不在自己的语料里 —— 靠住址，见 `scan_tree!` 头注）。
    let root = guard_core::addr::backend_src_root();
    let files = guard_core::scan_tree!(&root, &["rs"]);
    assert!(
        files.len() > 50,
        "只扫到 {} 份 .rs —— 走错树了，本条在空转",
        files.len()
    );
    // 一句话的形状：可选的 `let _ =`，然后 `<名字>_arg(`，以 `)?;` 收尾（单行）。
    let discarded = |line: &str| -> bool {
        let t = line.trim();
        let t = t.strip_prefix("let _ =").map(str::trim_start).unwrap_or(t);
        let Some(open) = t.find('(') else {
            return false;
        };
        let head = &t[..open];
        // 整句就是这一次调用：调用者是一条路径（`a::b_arg`），前面没有 `let x =` 之类接住它。
        let callee = head.rsplit("::").next().unwrap_or(head);
        !callee.is_empty()
            && callee.ends_with("_arg")
            && head
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':')
            && t[open..].contains('"')
            && (t.ends_with(")?;") || (line.trim().starts_with("let _ =") && t.ends_with(");")))
    };
    let mut hits = Vec::new();
    let mut seen_calls = 0usize;
    for (f, raw) in &files {
        let prod = guard_core::addr::production_side_of(f, raw);
        for line in prod.lines() {
            seen_calls += line.matches("_arg(").count();
            if discarded(line) {
                hits.push(format!("{}: {}", f.display(), line.trim()));
            }
        }
    }
    assert!(
        seen_calls > 10,
        "生产段里 `_arg(` 一共才 {seen_calls} 处 —— 量具坏了，本条在空转"
    );
    assert!(
        hits.is_empty(),
        "这些入参验完就丢了（返回值没人用）：\n{}\n要么真用它，要么把这一格从字段表 · 校验 · 调用方 · 协议文档里一起删掉。",
        hits.join("\n")
    );
}
