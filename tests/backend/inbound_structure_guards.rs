/// ★ 本文件**不许出现任何 `observe::`**。
///
/// 头注宣称了这条，但 D 审计指出它**没有机检** —— `layering_guard::layer_sources`
/// 只遍历 `src/observe` 与 `src/control`，顶层的 `inbound.rs` 不在采集面内。
/// 变异 `use crate::observe::watcher as _;` 之后全量 211 passed。
///
/// 在一份通篇强调「跨两处的约束必须机检」的文件里，这条自己是注释。现在不是了。
#[test]
fn inbound_never_reaches_into_the_observe_layer() {
    let src = crate::guard_support::production_code(include_str!("../../src/backend/inbound.rs"));
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
        "`inbound.rs` 伸进了读面：{hits:?}\n\
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
    let src = crate::guard_support::production_code(include_str!("../../src/backend/inbound.rs"));
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

/// ★ `COMMANDS` 这面**镜子**必须与注册表一致。
///
/// # 它替掉了什么
///
/// 上一版是 `hello_commands_match_the_dispatch_table` —— 扫 `dispatch` 分派臂的**文本**
/// （按 8 空格缩进切）。那种判据既对 rustfmt 脆，又只能事后比对。
/// U8a-2d 把名字与处理器绑进**同一个值**（[`super::REGISTRY`]）之后，
/// 「声明了却不接 / 接了却不声明」在注册表这一侧**不可表示** ——
/// 剩下的只有 `COMMANDS` 这面镜子，而这条是**数据对数据**，不是扫文本。
///
/// # 为什么还留着这面镜子
///
/// monitor 侧 `inbound_client.rs` 与 `tests/e2e/inbound-backend-frames.sh` 都在**文本抽取**
/// `const COMMANDS`（拿它做跨轨对拍）。把它换成运行时派生会同时打断那两处。
/// ⇒ 保留字面量，由本条钉住它不漂。
#[test]
fn the_commands_mirror_matches_the_registry() {
    let mut from_registry: Vec<&str> = super::REGISTRY.iter().map(|s| s.name).collect();
    from_registry.sort_unstable();
    assert!(
        from_registry.len() >= 4,
        "注册表只有 {} 条 —— 本断言在空转",
        from_registry.len()
    );
    let mut mirror: Vec<&str> = super::COMMANDS.to_vec();
    mirror.sort_unstable();
    assert_eq!(
        mirror, from_registry,
        "\n`COMMANDS`（hello 上线的那份）与 `REGISTRY` 对不上。\n\
             它今天只是一面镜子 —— 改注册表就把它一起改。\n\
             （它之所以还是字面量：monitor 与 e2e 都在文本抽取它做跨轨对拍。）"
    );
    // 名字不许重复（`lookup` 取第一个，重复会让后一条静默失效）。
    let mut uniq = from_registry.clone();
    uniq.dedup();
    assert_eq!(uniq.len(), from_registry.len(), "注册表里有重名命令");
    // 排序：`COMMANDS` 上线时是有序的，别让它随手插到中间变成无序。
    let mut sorted = super::COMMANDS.to_vec();
    sorted.sort_unstable();
    assert_eq!(
        super::COMMANDS.to_vec(),
        sorted,
        "`COMMANDS` 不是字典序 —— 上线的能力集应当稳定可读"
    );
}

/// ★ 每条命令的 `run` 档位必须与它真实的性质相符，**且新增命令必须来这里表态**。
///
/// 这条与 `the_dispatch_table_puts_blocking_commands_on_the_blocking_arm` 是两件事：
/// 那条走**真的 `dispatch`**（接缝），这条查**注册表的声明**（源）。两条都要有。
#[test]
fn every_registered_command_declares_its_run_kind() {
    use super::Run;
    for spec in super::REGISTRY {
        // F04a：`kill` 也是阻塞档 —— 它要起 tmux 子进程（探测 + kill-session）。
        // P4f：`bus-list` / `bus-send` 同样是阻塞档 —— 它们要起 cc-bus 子进程并等它退出。
        // `K-R104`：`capture-pane` / `oneshot-session` 起 tmux 子进程并等它退出。
        // 〔步 `24f` 第二刀〕`files-read` 四条同为阻塞档：`files::answer` 是**同步**函数，
        // 前两条真做文件系统 I/O，`files-find` 在 64 万条量纲上外推 20–50 ms。
        // 理由整段在 `inbound::REGISTRY` 上它们那一段（含「为什么不拆两档」）。
        // 〔步 `24f` 第三刀〕新那两条同档，而且这一档对它们**更承重**：
        // `files-index-rebuild` 是**走一整棵树**（64 万条现打 0.99 秒，热缓存；冷缓存没量过），
        // `files-browse` 要把名单上那几个目录各 `read_dir` 一遍。
        // 放 `Run::Async` 就是拿 tokio worker 去跑一趟秒级遍历 —— 单核机上会把
        // 出方向 writer 与入方向 reader 所在的那条 runtime 一起占住。
        // 〔波 5 ㈠ 09-23〕`files-create` 同为阻塞档，理由同形而且**更硬**：
        // 它真的开一个文件句柄并 `write_all` 一遍（`control/files_write.rs`），
        // 那是同步阻塞 I/O；而且它走的是围栏② 那条 `canonicalize`（一次真实的路径解析）。
        // 代价如实写：这一档开跑之后打不断 ⇒ `cancel` 命中时回 `not_cancellable`，不撒谎。
        let expected_blocking = matches!(
            spec.name,
            "launch"
                | "kill"
                | "bus-list"
                | "bus-send"
                | "bus-kill"
                | "bus-spawn"
                | "bus-state"
                | "capture-pane"
                | "files-browse"
                | "files-create"
                | "files-commit-upload"
                // 〔F9c · 第四波〕存盘的两步同档（同步文件 I/O ＋ 围栏的 `canonicalize`）。
                | "files-stage-chunk"
                | "files-commit-text"
                | "files-chmod"
                | "files-delete"
                | "files-mkdir"
                | "files-rename"
                | "files-write-text"
                | "files-copy"
                // 〔RW1 · 第四波 09-24〕读改写两条 ＋ 删历史会话：同步文件 I/O（围栏的 `canonicalize`
                // ＋ 读 / 暂存旁名写满 ＋ 换名 / 删），同写面其余几条一档。
                | "files-peek"
                | "files-put"
                | "files-delete-session"
                | "files-ls"
                | "files-stat"
                | "files-find"
                | "files-index-rebuild"
                | "files-index-status"
                // 〔F7a · 第三波 09-24〕同族第七、第八条同档（同步文件 I/O / 读环境）。
                | "files-read-text"
                | "files-home"
                // 〔`C1` · 09-24〕只读查询面八条同为阻塞档：全做文件 I/O，
                // `history-search` 扫全库、`history-tail` 扫整份会话 —— 不许占 tokio worker。
                | "history-projects"
                | "history-index"
                | "history-user-inputs"
                | "history-find"
                | "history-read"
                | "history-lines" // 〔CF2〕按行号取回：从文件头数，同档
                | "history-record" // 〔U4b〕记录还在不在：一次目录枚举，同档
                | "history-search"
                | "history-sessions"
                | "history-subagents"
                | "history-tail"
                | "accounts-list"
                | "accounts-sessions"
                // 〔B2 · 条 66〕「退出行为」那两条：同步文件 I/O（读一份小文件 / 原子写一份），
                // 不许占 tokio worker。开跑之后打不断 ⇒ `cancel` 命中回 `not_cancellable`。
                | "exit-policy-read"
                | "exit-policy-set"
                // 〔RM1b · 第四波〕功能侧只读查询：读一个目录 ＋ 每个文件各一次（同步文件 I/O）。
                | "plugins-marketplaces"
                | "tasks-list"
                // 〔RM1c · 第四波〕代码全景：起一个进程、等它退出（建索引可到分钟级）。
                | "panorama"
                // 〔RM1a · 第四波〕账号层那份凭据文件的两条：同步文件 I/O（读一份小文件 / 原子写一份）。
                | "apikey-key-set"
                | "apikey-read"
                // 〔RM1a · 第四波〕中转那两条：回环连一次 / 起一个进程，同步阻塞。
                | "relay-ensure"
                | "relay-status"
                // 〔RM1a · 第四波〕足迹那一条：一批 stat / 读几份小文件，同步文件 I/O。
                | "footprint-probe"
        );
        let is_blocking = matches!(spec.run, Run::Blocking(_));
        assert_eq!(
            is_blocking, expected_blocking,
            "`{}` 的 Run 档位与预期不符 —— 放错档的代价是「占住 worker」或「假装能取消」",
            spec.name
        );
        let is_builtin = matches!(spec.run, Run::Builtin);
        // 〔SR1a〕链路四条也是硬臂：要碰**本连接的链路表**与应答通道（`dial/link.rs`），
        // 而且 `link-data` 必须在读循环里就地分派（保序）—— 交给独立 task 就不再保序。
        // 〔SR1b〕传输四条也是硬臂：要碰**本连接的票表**与应答通道（进度帧走应答通道，`control/transfer.rs`）。
        let expected_builtin = matches!(
            spec.name,
            "cancel"
                | "link-open"
                | "link-data"
                | "link-credit"
                | "link-close"
                | "transfer-upload"
                | "transfer-download"
                | "transfer-start"
                | "transfer-stop"
        );
        assert_eq!(
            is_builtin, expected_builtin,
            "`{}` 的 Builtin 档位不对 —— 只有 `cancel`、链路四条与传输四条该是硬臂",
            spec.name
        );
    }
    // 计数自检：新增命令而这里没表态 ⇒ 上面那条 `expected_blocking` 会把它当非阻塞，
    // 于是真加了一条阻塞命令却没登记时会红。这里再加一条显式的覆盖面断言。
    // P4f：`bus-list` / `bus-send` 起 cc-bus 子进程并等它退出 ⇒ 与 `launch`/`kill` 同为阻塞档。
    let known = [
        "cancel",
        "kill",
        "launch",
        "link-close",
        "link-credit",
        "link-data",
        "link-open",
        "ping",
        "resolve",
        "bus-list",
        "bus-send",
        "bus-kill",
        "bus-spawn",
        "bus-state",
        "capture-pane",
        "files-browse",
        "files-create",
        "files-commit-upload",
        "files-stage-chunk",
        "files-commit-text",
        "files-chmod",
        "files-delete",
        "files-mkdir",
        "files-rename",
        "files-write-text",
        "files-copy",
        "files-ls",
        "files-stat",
        "files-find",
        "files-index-rebuild",
        "files-index-status",
        "files-read-text",
        "files-home",
        "history-projects",
        "history-index",
        "history-user-inputs",
        "history-find",
        "history-read",
        "history-lines",  // 〔CF2〕
        "history-record", // 〔U4b〕
        "history-search",
        "history-sessions",
        "history-subagents",
        "history-tail",
        "accounts-list",
        "accounts-sessions",
        "exit-policy-read",
        "exit-policy-set",
        "plugins-marketplaces",
        "tasks-list",
        "panorama",
        "apikey-key-set",
        "apikey-read",
        "relay-ensure",
        "relay-status",
        "footprint-probe",
        // 〔RW1 · 第四波 09-24〕读改写两条 ＋ 删历史会话（阻塞档，理由在上面 `expected_blocking`）。
        "files-peek",
        "files-put",
        "files-delete-session",
        // 〔SR1b〕传输四条：内建（硬臂）。
        "transfer-upload",
        "transfer-download",
        "transfer-start",
        "transfer-stop",
    ];
    let missing: Vec<&str> = super::REGISTRY
        .iter()
        .map(|s| s.name)
        .filter(|n| !known.contains(n))
        .collect();
    assert!(
        missing.is_empty(),
        "这些命令没在本条里表态「阻塞 / 异步 / 内建」：{missing:?}"
    );
}

/// ★ `launch` 的 `fields` 必须与**真正的解析器 / 输出构造器**实测一致。
///
/// # 为什么非有这一层不可
///
/// `CommandSpec::fields` 是**手写镜子**。拿它去钉文档（R3）时，如果它自己会漂，
/// 那就是**用一个手写清单证明另一个手写清单** —— 一点强度都没有。
/// 所以它必须先与代码里真正读/写这些键的地方对上。
///
/// 抽取面：`parse_request` 里每个 `get_str("…")`（= args 侧）+
/// `launch_for_inbound` 里 `json!` 的键（= data 侧）。
#[test]
fn launch_fields_match_its_parser_and_output() {
    let src =
        crate::guard_support::production_code(include_str!("../../src/backend/control/launch.rs"));
    let mut found: Vec<String> = Vec::new();

    // args 侧：`get_str("<key>")`
    let key = "get_str(\"";
    let mut from = 0usize;
    while let Some(rel) = src[from..].find(key) {
        let at = from + rel + key.len();
        let end = src[at..].find('"').map(|k| at + k).unwrap_or(at);
        found.push(src[at..end].to_string());
        from = end;
    }
    let args_n = found.len();
    assert!(
        args_n >= 5,
        "只从解析器抠到 {args_n} 个 args 字段 —— 抽取坏了，本断言在空转"
    );

    // data 侧：`json!({ "<key>": … })` —— 取 `json!` 块里的所有 `"key":`
    let j = src
        .find("serde_json::json!({")
        .expect("找不到 launch 的输出构造 —— 抽取坏了");
    let jend = src[j..].find("})").map(|k| j + k).expect("json! 没收尾");
    let block = &src[j..jend];
    let mut data_n = 0usize;
    for part in block.split('"').skip(1).step_by(2) {
        if !part.is_empty() && part.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
            found.push(part.to_string());
            data_n += 1;
        }
    }
    assert!(
        data_n >= 3,
        "只从输出构造抠到 {data_n} 个 data 字段 —— 抽取坏了，本断言在空转"
    );

    found.sort();
    found.dedup();
    let spec = super::REGISTRY
        .iter()
        .find(|s| s.name == "launch")
        .expect("注册表里没有 launch");
    let mut declared: Vec<String> = spec.fields.iter().map(|s| s.to_string()).collect();
    declared.sort();
    assert_eq!(
        declared, found,
        "\n`launch` 登记的 fields 与它真正读/写的键对不上。\n\
             这面镜子是用来钉文档的（R3）—— 它自己先漂了，钉出来的就是假的。"
    );
}

/// ★〔audit-0805 08-06〕**「声明零字段」不许成为免检开关**。
///
/// # 它补的洞
///
/// 两条判据（本文件这条 + `protocol_doc_guard` 的
/// `every_command_payload_field_appears_in_its_own_doc_section`）**开头都是**
/// `if spec.fields.is_empty() { continue; }`。
/// ⇒ 一条命令只要把 `fields` 声明成 `&[]`，就**同时**从两条判据里消失 ——
/// 而没有任何东西检查那个声明是不是诚实的。**声明本身成了豁免开关。**
///
/// ⚠ `resolve` 今天就是「有 `doc_anchor`、`fields: &[]`」。逐字读过它的文档段之后
/// 判定**它是诚实的**：载荷记作 `{ResumeSpec}`，按**结构体引用**写、不逐字段列。
/// ⇒ 所以本条不写成「有文档段就不许零字段」（那会当场误红），
/// 而写成**默认拒绝 + 豁免登记**：零字段可以，但要写明为什么。
#[test]
fn declaring_zero_fields_needs_a_reason() {
    /// `(命令名, 为什么它可以声明零载荷字段)`。
    const ZERO_FIELD_REASONS: &[(&str, &str)] = &[(
        "resolve",
        "载荷是 `ResumeSpec` **结构体**，文档段按结构体引用记（`args:{ResumeSpec}`）\
             而不逐字段列；字段契约由那个 struct 的定义与它自己的序列化测试守。\
             ⇒ 在这里列一份字段清单反而会造出第二个真相源（E3）。",
    )];
    let mut unexplained: Vec<&str> = Vec::new();
    let mut zero_with_doc = 0usize;
    for spec in super::REGISTRY {
        if !spec.fields.is_empty() {
            continue;
        }
        if spec.doc_anchor.is_none() {
            continue;
        }
        zero_with_doc += 1;
        if !ZERO_FIELD_REASONS.iter().any(|(n, _)| *n == spec.name) {
            unexplained.push(spec.name);
        }
    }
    assert!(
        zero_with_doc >= 1,
        "没有任何命令处于「有文档段 + 零字段声明」状态 —— 本条在空转。\
             若确实全都列了字段，请把本条连同 `ZERO_FIELD_REASONS` 一起删掉（别留空转的判据）"
    );
    assert!(
        unexplained.is_empty(),
        "这些命令有自己的文档小节，却把 `fields` 声明成空：{unexplained:?}\n\
             ⚠ 空声明会让它**同时**从两条判据里消失（本文件这条 + `protocol_doc_guard` 那条，\n\
             两条开头都是 `if spec.fields.is_empty() {{ continue; }}`）——\n\
             也就是说**声明本身是免检开关**。\n\
             要么把载荷字段列出来，要么在 `ZERO_FIELD_REASONS` 里写明为什么它没有可列的字段。"
    );
    for (name, _) in ZERO_FIELD_REASONS {
        let spec = super::REGISTRY
            .iter()
            .find(|s| s.name == *name)
            .unwrap_or_else(|| panic!("`ZERO_FIELD_REASONS` 里的 `{name}` 已经不在注册表里"));
        assert!(
            spec.fields.is_empty(),
            "`{name}` 现在列了 {} 个字段，豁免登记该删了（登记表腐烂比没有登记更糟）",
            spec.fields.len()
        );
    }
}

/// ★★ 〔步 `24f` 第二刀 09-20〕**`files-read` 上线的那四条，登记的就是它声明的那四条。**
///
/// # 它治的是哪一形
///
/// 这一族的能力声明住 `files::CAPABILITIES`（`设计/96 §2.9` 那张表，由
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
    // 🔴 〔波 5 ㈠ 09-23〕`files-` 这个线上前缀底下**从此有两族**，而它们**互不相交**：
    //   读那一族的住址是 `files::CAPABILITIES`（`src/backend/files/`，整族纯读），
    //   写那一面的住址是 `control::files_write::MANAGE_COMMANDS`（`control/`，会改变世界）。
    //   ⇒ 本条的人群必须**先把写那一面减掉**，否则它会把写那一面当成「读族漏声明了一条」
    //   （现打：不减的话逐字报 `files-create` 在 wired 里而不在 declared 里）。
    //   ⚠ 减法是**按那张表**减，不是按名字手写一份 —— 手写第二份就是下一个漂移源。
    // 〔F7c · 第三波 09-24〕写那一侧从此是**两张表**：写面 `MANAGE_COMMANDS` ＋ 上传提交
    //   `files_commit::COMMIT_COMMANDS`（`readonly_guard` 第三层第二个登记的模块）。减法按两张表的并。
    let write_face: std::collections::BTreeSet<String> =
        crate::control::files_write::manage_command_names()
            .into_iter()
            .chain(crate::control::files_commit::commit_command_names())
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
         而 `readonly_guard` 第三层那条「只从声明过的那一面来」判的正是这张表。"
    );
    // ★ 写那一面的每条命令，`codes` / `fields` 与它自己的登记逐字对上（同读族那一半）。
    type WriteAnswer = fn(&str, &serde_json::Value) -> crate::control::files_write::Answer;
    let tables: [(&[crate::control::files_write::ManageCommand], WriteAnswer); 2] = [
        (
            crate::control::files_write::MANAGE_COMMANDS,
            crate::control::files_write::answer_wire,
        ),
        (
            crate::control::files_commit::COMMIT_COMMANDS,
            crate::control::files_commit::answer_wire,
        ),
    ];
    for (cap, answer) in tables
        .iter()
        .flat_map(|(t, a)| t.iter().map(move |c| (c, *a)))
    {
        let spec = super::REGISTRY
            .iter()
            .find(|s| s.name == cap.name)
            .unwrap_or_else(|| panic!("`{}` 不在 REGISTRY 上 —— 上面那条应该先红", cap.name));
        let mut want_codes: Vec<&str> = cap.codes.to_vec();
        want_codes.sort_unstable();
        let mut got_codes: Vec<&str> = spec.codes.to_vec();
        got_codes.sort_unstable();
        assert_eq!(
            got_codes, want_codes,
            "`{}` 登记的 code 与写面声明的对不上",
            cap.name
        );
        let mut want_fields: Vec<&str> =
            cap.args.iter().chain(cap.fields.iter()).copied().collect();
        want_fields.sort_unstable();
        want_fields.dedup();
        let mut got_fields: Vec<&str> = spec.fields.to_vec();
        got_fields.sort_unstable();
        got_fields.dedup();
        assert_eq!(
            got_fields, want_fields,
            "`{}` 的 `fields` 与写面声明的 `args` ∪ `fields` 对不上 —— \
             那份镜子是拿去钉 `IPC-PROTOCOL.md §10` 的，它自己先漂了钉出来的文档就是假的",
            cap.name
        );
        // 真的调一次：分派必须够得到它（`unknown_command` / 不认的名字都算断线）。
        let out = answer(cap.name, &serde_json::json!({}));
        if let Err((code, msg)) = out {
            assert!(
                cap.codes.contains(&code),
                "`{}` 回了一个它没登记的 code `{code}`（{msg}）",
                cap.name
            );
        }
    }

    // ② ③ 逐条：错误码逐字相等 · 载荷字段 == `args` ∪ `fields`。
    let mut checked = 0usize;
    for cap in crate::files::CAPABILITIES {
        let online = cap.name.replace('.', "-");
        let spec = super::REGISTRY
            .iter()
            .find(|s| s.name == online)
            .unwrap_or_else(|| panic!("`{online}` 不在 REGISTRY 上 —— 上面那条应该先红"));
        let mut want_codes: Vec<&str> = cap.codes.to_vec();
        want_codes.sort_unstable();
        let mut got_codes: Vec<&str> = spec.codes.to_vec();
        got_codes.sort_unstable();
        assert_eq!(
            got_codes, want_codes,
            "`{online}` 登记的 code 与能力 `{}` 声明的对不上",
            cap.name
        );

        let mut want_fields: Vec<&str> =
            cap.args.iter().chain(cap.fields.iter()).copied().collect();
        want_fields.sort_unstable();
        want_fields.dedup();
        let mut got_fields: Vec<&str> = spec.fields.to_vec();
        got_fields.sort_unstable();
        got_fields.dedup();
        assert_eq!(
            got_fields, want_fields,
            "\n`{online}` 的 `fields` 与能力 `{}` 的 `args` ∪ `fields` 对不上。\n\
             `CommandSpec::fields` 是拿去钉 `IPC-PROTOCOL.md §10` 那一小节的镜子 ——\n\
             它自己先漂了，钉出来的文档就是假的（那段头注逐字）。",
            cap.name
        );
        checked += 1;

        // ④ 真的调一次：`run` 必须**够得到**那条能力。
        //    喂空 `args` ⇒ 要么成功，要么落在这条能力自己声明的 code 上；
        //    落到 `unknown_capability` 就说明翻译或名字接错了。
        let out = crate::files::answer_wire(&online, &serde_json::json!({}));
        if let Err((code, msg)) = out {
            assert_ne!(
                code, "unknown_capability",
                "`{online}` 经 `answer_wire` 够不到任何能力（{msg}）—— 线上那一跳是断的"
            );
            assert!(
                cap.codes.contains(&code),
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

/// ★ **有 `fields` 就必须有自己的文档小节。**
///
/// 没有小节就没地方钉字段名 —— 那正是设计审计 P2 说的
/// 「帧的字段有对拍，命令的载荷没有」。
#[test]
fn a_command_with_a_payload_must_own_a_doc_section() {
    for spec in super::REGISTRY {
        if spec.fields.is_empty() {
            continue;
        }
        // `cancel` 的 `target` 记在入方向节的正文里（它是取消机制本身，不另开小节）。
        if spec.name == "cancel" {
            continue;
        }
        assert!(
            spec.doc_anchor.is_some(),
            "`{}` 有 {} 个载荷字段却没有自己的文档小节 —— 那些字段没地方钉",
            spec.name,
            spec.fields.len()
        );
    }
}
