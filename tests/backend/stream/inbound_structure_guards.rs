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
    let src: String = crate::guard_support::inbound_sources()
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
    let src: String = crate::guard_support::inbound_sources()
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
    let main = crate::guard_support::production_code(include_str!("../../../src/backend/main.rs"));
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
    let sources = crate::guard_support::inbound_sources();
    let on_disk: std::collections::BTreeSet<String> = sources
        .iter()
        .filter_map(|(rel, _)| rel.strip_prefix("stream/inbound/registry/"))
        .map(|f| f.trim_end_matches(".rs").to_string())
        .collect();
    let (_, mod_rs) = sources
        .iter()
        .find(|(rel, _)| rel == "stream/inbound/mod.rs")
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
                | "bus-broadcast"
                | "bus-kill"
                | "bus-spawn"
                | "bus-state"
                // 起一个 `cc-log` 子进程并等它退出。
                | "bus-inbox"
                // 分叉（读整份 jsonl ＋ `O_EXCL` 写）。
                | "session-fork"
                | "capture-pane"
                // 终端管理 L1 三条：起 tmux（列会话 · 抓屏 · 探身份 · 送字送键）并等它退出。
                | "terminals-list"
                | "terminal-preview"
                | "terminal-input"
                | "files-browse"
                | "files-create"
                | "files-commit-upload"
                // 存盘的两步同档（同步文件 I/O ＋ 围栏的 `canonicalize`）。
                | "files-stage-chunk"
                | "files-commit-text"
                | "files-chmod"
                | "files-delete"
                | "files-mkdir"
                | "files-rename"
                | "files-write-text"
                | "files-copy"
                // 解压：同步读包 ＋ 落盘，同写面一档。
                | "files-extract"
                // 读改写两条 ＋ 删历史会话：同步文件 I/O（围栏的 `canonicalize`
                // ＋ 读 / 暂存旁名写满 ＋ 换名 / 删），同写面其余几条一档。
                | "files-peek"
                | "files-put"
                | "files-delete-session"
                | "files-ls"
                | "files-stat"
                | "files-find"
                | "files-index-rebuild"
                | "files-index-status"
                // 同族第七、第八条同档（同步文件 I/O / 读环境）。
                | "files-read-text"
                | "files-home"
                // 读族第九条（算目录大小）同档：走一整棵树的同步 I/O。
                | "files-size"
                // 读族第十条（分块读回）同档：同步文件 I/O。
                | "files-read-chunk"
                // 只读查询面八条同为阻塞档：全做文件 I/O，
                // `history-search` 扫全库、`history-tail` 扫整份会话 —— 不许占 tokio worker。
                | "history-index"
                | "history-user-inputs"
                | "history-find"
                | "backend-log" // 读一份诊断文件的尾部（同步文件 I/O），同档
                | "history-turns" // 一轮的摘要：扫一段会话，同档
                | "history-facts" // 会话事实：扫一份会话（首次整份，续传只读新写的一截），同档
                | "history-read"
                | "history-lines" // 按行号取回：从文件头数，同档
                | "history-record" // 记录还在不在：一次目录枚举，同档
                | "history-search"
                | "history-run" // 按运行读一个子运行的记录（同步文件 I/O），同档
                | "history-page" // 按字节分页出记录行，同档
                | "history-tail"
                | "accounts-list"
                | "accounts-sessions"
                | "machine-interrupts" // 读这台的活会话，与上一条同档
                | "accounts-trust" // 信任预检：读 manifest ＋ `.claude.json`，同档
                // 〔条 66〕「退出行为」那两条：同步文件 I/O（读一份小文件 / 原子写一份），
                // 不许占 tokio worker。开跑之后打不断 ⇒ `cancel` 命中回 `not_cancellable`。
                | "exit-policy-read"
                | "exit-policy-set"
                // 额度账：读一份小文件（同步文件 I/O）。
                | "quota-read"
                // 用某个号查一次额度：起官方客户端并等它退出（期限 30 秒）＋ 锁里写额度账。
                | "quota-probe"
                // 轮换：读 / 锁里原子写 `rotation.json`（同步文件 I/O）。
                | "rotation-rules-read"
                | "rotation-rule-save"
                | "rotation-rule-rename"
                | "rotation-rule-delete"
                | "rotation-default-set"
                | "rotation-plan"
                | "rotation-session-read"
                | "rotation-session-set"
                // 功能侧只读查询：读一个目录 ＋ 每个文件各一次（同步文件 I/O）。
                | "tasks-list"
                // 动一个会话之前会打断什么：读 pidfile 目录 ＋ 任务目录（同步文件 I/O）。
                | "session-interrupts"
                // MCP 列表：读 `.claude.json` ＋ 一份 `.mcp.json`（同步文件 I/O）。
                | "mcp-read"
                // 列 tmux 会话：起一次 `sh` ＋ `tmux` 并等它退出。
                | "tmux-list"
                // 谁在显示这个会话：读 `/proc` ＋ 在 tmux 里时起一次 `tmux list-clients` 并等它退出。
                | "session-terminals"
                // 认终端进程：读系统连接表 ＋ 进程表（每个进程开一次句柄问启动时刻）。
                | "terminal-processes"
                // 铸 tmux 名：问一次会话快照 = 起一次 `tmux` 并等它退出。
                | "terminal-name-mint"
                // `~/.ssh/config` 三条：读一份文件 ／ 起 `ssh -G` 并等它退出。
                | "ssh-config-aliases"
                | "ssh-config-import"
                | "ssh-config-resolve"
                // 钩子诊断：读一份 settings ＋ 几次 stat（同步文件 I/O）。
                | "hooks-diag"
                // 手动对齐：等每份 watcher 做完（对表 ＋ 打标起 tmux）。
                | "resync"
                // 上游选择那份凭据文件的两条：同步文件 I/O（读一份小文件 / 原子写一份）。
                | "apikey-key-set"
                | "apikey-read"
                // 上游选择出的成品：读一份凭据文件 ＋ 装表，同步阻塞。
                | "apikey-routing"
                // 直接敲的也走中转：读一份用户级设置文件 ＋ 一份钥匙文件（同步文件 I/O）。
                | "relay-optin"
                // `relay-ensure` / `relay-status` 两条随脱离 `--relay` 一族删了。
                // 足迹那一条：一批 stat / 读几份小文件，同步文件 I/O。
                | "footprint-report"
                // 「文件与数据」那一份：同一份足迹（同一批 stat）重排。
                | "data-report"
                // 换 Claude 目录前那一问：stat 两次。
                | "agent-home-check"
                // 「要你动手」记下的选择：读—改—写后端自己那份小文件（同步文件 I/O）。
                | "chores-mark"
                // 离线那台的上次值：读 / 读—改—写后端自己那份小文件（同步文件 I/O）。
                | "last-seen-read"
                | "last-seen-write"
                // 首次运行那份数：读启动文件 ＋ 账号库清单（同步文件 I/O）。
                | "first-run"
                // 别名预览：读账号库 manifest ＋ 问会话快照（同步 I/O），不起进程。
                | "ccm-print"
                // MCP 同步的判定：对可疑路径逐条 stat、在 PATH 上找名字（同步文件 I/O）。
                | "mcp-sync-plan"
                // MCP 写两条 ＋ 推拉三条 ＋ skill 装卸两条：读 / 规划 / 经本进程文件管理面写（同步文件 I/O）。
                | "skill-install-apply"
                | "cc-bus-install"
                // 账号库那一族：读账号库 ＋ 经本进程文件管理面建目录 · 建链接 · 复制 · 写清单（同步文件 I/O）。
                | "accounts-init"
                | "accounts-add"
                | "accounts-remove"
                | "accounts-set-default"
                | "accounts-repair"
                | "accounts-isolate"
                | "accounts-rollback"
                | "accounts-verify"
                | "accounts-login-cmd"
                // 各账号共用的用户级 MCP：读各号配置文件、只改一个键、写回共享集合（同步文件 I/O，经本进程文件管理面）。
                | "accounts-mcp-read"
                | "accounts-mcp-remove"
                | "accounts-mcp-pick"
                | "accounts-mcp-sync"
                // 公钥并进这台的 `authorized_keys`：同步文件 I/O（经本进程文件管理面）。
                | "authorized-keys-add"
                | "files-link"
                | "cc-bus-install-state"
                | "aliases-read"
                | "aliases-block-render"
                | "aliases-block-install"
                | "aliases-block-remove"
                | "profiles-read"
                | "profiles-resolve"
                | "profiles-impact"
                | "profiles-bases"
                | "profiles-write"
                // 起一次那一代 PowerShell 设执行策略再现问（同步子进程）。
                | "powershell-policy-set"
                | "ext-uninstall-apply"
                | "mcp-server-put"
                | "mcp-server-remove"
                | "mcp-sync-source"
                | "mcp-sync-preview"
                | "mcp-sync-apply"
                // 资产目录两条：扫 skill 目录 / 读项目 `.mcp.json` ＋ 原子写目录文件，同步文件 I/O。
                | "assets-catalog"
                | "assets-catalog-merge"
                // skill「装到这台」两条：走 skill 目录、读文件原文、stat 可疑路径（同步文件 I/O）。
                | "skill-read"
                | "skill-install-plan"
                // 装记录的写口 ＋ 扩展页那张表（扫盘 ＋ 原子写目录文件）＋ 卸之前那张卡（读装记录 · 逐个读盘比摘要），同步文件 I/O。
                | "skill-install-record"
                | "ext-list"
                | "ext-uninstall-preview"
                // 扩展页写备注：现扫 ＋ 原子写目录文件。
                | "ext-note-set"
                // 历史注解三条：读 / 原子写一份小文件（同步文件 I/O）。
                | "history-annotate"
                | "history-forget"
                | "history-last-accounts"
                // 本机起会话那一行：核一次「新起」的目录在不在（stat）。
                | "launch-local"
                // 开终端那一串：查几个文件找本机 ssh 客户端（stat）。
                | "terminal-ssh"
                // 远端那一行：判号要读这台的账号清单与起会话账号记录（同步文件 I/O）。
                | "launch-render-cli"
                // 本机那一份放不放：读一遍落点那个文件（约 10 MB，同步文件 I/O）。
                | "place-verdict"
                // 批量停 / 起：逐个起 tmux 子进程（同 `kill` / `launch`）。
                | "sessions-stop"
                | "sessions-start"
                | "sessions-where"
                // 起新会话：起 tmux 子进程 · 读账号清单 · 写分支记录 · 扫各家记录目录（同步 I/O）。
                | "session-new"
                | "session-new-facts"
                | "session-new-dir"
                // 计划读面三条：起一次 pb 子进程（研究盘约 1 秒）· 读记录树对会话（同步 I/O）。
                | "plan-list"
                | "plan-read"
                | "plan-cell-view"
                // 计划审面三条：读—改—写后端自己的小文件（跨进程锁）· 退回现读一次计划（起 pb）再送字（起 tmux）。
                | "plan-ack"
                | "plan-unack"
                | "plan-return"
        );
        let is_blocking = matches!(spec.run, Run::Blocking(_) | Run::BlockingData(_));
        assert_eq!(
            is_blocking, expected_blocking,
            "`{}` 的 Run 档位与预期不符 —— 放错档的代价是「占住 worker」或「假装能取消」",
            spec.name
        );
        let is_builtin = matches!(spec.run, Run::Builtin);
        // 链路四条也是硬臂：要碰**本连接的链路表**与应答通道（`dial/link.rs`），
        // 而且 `link-data` 必须在读循环里就地分派（保序）—— 交给独立 task 就不再保序。
        // 传输四条也是硬臂：要碰**本连接的票表**与应答通道（进度帧走应答通道，`control/transfer.rs`）。
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
                // 测试连接：进度格走本连接的应答通道（不丢、与应答同序）⇒ 要拿到应答通道，只能是硬臂。
                | "remote-probe"
                // 终端实时预览三条：要碰本连接的订阅票表与应答通道（画面帧走应答通道，`control/terminal_follow.rs`）。
                | "terminal-follow"
                | "terminal-follow-ack"
                | "terminal-unfollow"
        );
        assert_eq!(
            is_builtin, expected_builtin,
            "`{}` 的 Builtin 档位不对 —— 只有 `cancel`、链路四条、传输四条、测试连接与终端实时预览三条该是硬臂",
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
        // `ccm-probe`：拼 `--ccm-probe` 那几行，纯函数 ⇒ 不进阻塞档。
        "ccm-probe",
        // `terminal-ssh`：开终端那一串，查几个文件找本机 ssh 客户端 ⇒ 阻塞档。
        "terminal-ssh",
        // `history-search-merge`：各台结果合一份，纯计算 ⇒ 不进阻塞档。
        "history-search-merge",
        // 资产目录的同步：真异步（拨号 / 等远端 capture），在 await 点可取消。
        "assets-sync",
        // 两台之间「装」那一件的枢纽：等远端 capture（真异步，在 await 点可取消），本机那一跳挪到阻塞线程池。
        "ext-hub-preview",
        "ext-hub-apply",
        "pubkey-push", // 等远端（问那台后端 / 一次 exec），真异步
        "files-grep",  // 可撤：走那一趟在阻塞线程池上、看取消位，future 被丢即收手
        // 部署计划：真异步（拨号 / 等远端 capture · SFTP），在 await 点可取消。
        "deploy-plan",
        // 远端常驻后端 hello 的新旧：纯判定，不碰盘不拨号。
        "resident-verdict",
        // 可达表登记：纯内存，普通 spawn。
        "remote-reach",
        // 端口转发：起 = 真异步（查可达表 · 开链路 · 等 ack），停 / 列 = 纯内存一把锁。
        "forward-start",
        "forward-stop",
        "forward-list",
        "drift-report", // 漂移账：纯内存一把锁
        // 测试连接：真异步（拨号 · 读 hello · 往返），在 await 点可取消。
        "remote-probe",
        "bus-list",
        "bus-send",
        "bus-broadcast",
        "bus-kill",
        "bus-spawn",
        "bus-state",
        "bus-inbox",
        "session-fork",
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
        "files-extract",
        "files-ls",
        "files-stat",
        "files-find",
        "files-index-rebuild",
        "files-index-status",
        "files-read-text",
        "files-home",
        "files-size",
        "files-read-chunk",
        "history-list",
        "history-index",
        "history-user-inputs",
        "history-find",
        "backend-log",   //
        "history-turns", //
        "history-facts", //
        "history-read",
        "history-lines",  //
        "history-record", //
        "history-search",
        "history-run",
        "history-page", //
        "history-tail",
        "accounts-list",
        "accounts-sessions",
        "machine-interrupts",
        "accounts-trust", //
        "exit-policy-read",
        "exit-policy-set",
        "quota-read",
        "quota-probe",
        "rotation-rules-read",
        "rotation-rule-save",
        "rotation-rule-rename",
        "rotation-rule-delete",
        "rotation-default-set",
        "rotation-plan",
        "rotation-session-read",
        "rotation-session-set",
        "tasks-list",
        "session-interrupts",
        "mcp-read",
        "tmux-list",
        "session-terminals",
        "terminal-processes",
        "terminal-name-mint", //
        "hooks-diag",         //
        "resync",             //
        "apikey-key-set",
        "apikey-read",
        "apikey-routing", //
        "relay-optin",    //
        "footprint-report",
        "data-report",
        "agent-home-check",
        "chores-mark",
        "last-seen-read",
        "last-seen-write",
        "first-run",
        // 别名预览（阻塞档，理由在上面 `expected_blocking`）。
        "ccm-print",
        // 资产目录两条（阻塞档，理由在上面 `expected_blocking`）。
        "assets-catalog",
        "assets-catalog-merge",
        "skill-read",
        "skill-install-plan",
        // skill 卸三条（阻塞档，理由在上面 `expected_blocking`）。
        "skill-install-record",
        "ext-list",
        "ext-uninstall-preview",
        "ext-note-set",
        // 历史注解三条（阻塞档，理由在上面 `expected_blocking`）。
        "history-annotate",
        "history-forget",
        "history-last-accounts",
        // 读改写两条 ＋ 删历史会话（阻塞档，理由在上面 `expected_blocking`）。
        "files-peek",
        "files-put",
        "files-delete-session",
        // MCP 同步的判定（阻塞档，理由在上面 `expected_blocking`）。
        "mcp-sync-plan",
        // `~/.ssh/config` 三条（阻塞档，理由在上面 `expected_blocking`）。
        "ssh-config-aliases",
        "ssh-config-import",
        "ssh-config-resolve",
        // 起会话的计划与渲染（本机那条阻塞档，两条渲染异步）。
        "launch-local",
        "launch-render-cli",
        "launch-render-payload",
        // 本机那一份放不放（阻塞档，理由在上面 `expected_blocking`）。
        "place-verdict",
        // MCP 写两条 ＋ 推拉三条 ＋ skill 装卸两条（阻塞档，理由在上面 `expected_blocking`）。
        "skill-install-apply",
        "cc-bus-install",
        // 账号库那一族（阻塞档，理由在上面 `expected_blocking`）。
        "accounts-init",
        "accounts-add",
        "accounts-remove",
        "accounts-set-default",
        "accounts-repair",
        "accounts-isolate",
        "accounts-rollback",
        "accounts-verify",
        "accounts-login-cmd",
        "accounts-mcp-read",
        "accounts-mcp-remove",
        "accounts-mcp-pick",
        "accounts-mcp-sync",
        "authorized-keys-add", // 同步文件 I/O（经本进程文件管理面）
        "files-link",
        "cc-bus-install-state",
        "aliases-read",
        "aliases-block-render",
        "aliases-block-install",
        "aliases-block-remove",
        "profiles-read",
        "profiles-resolve",
        "profiles-impact",
        "profiles-bases",
        "profiles-write",
        "powershell-policy-set",
        "ext-uninstall-apply",
        "mcp-server-put",
        "mcp-server-remove",
        "mcp-sync-source",
        "mcp-sync-preview",
        "mcp-sync-apply",
        // 批量停 / 起：阻塞。
        "sessions-stop",
        "sessions-start",
        "sessions-where",
        // 起新会话那三问：阻塞。
        "session-new",
        "session-new-facts",
        "session-new-dir",
        // 计划读面三条：阻塞（起 pb 子进程）。
        "plan-list",
        "plan-read",
        "plan-cell-view",
        // 计划审面三条：阻塞（小文件 I/O · 起 pb · 起 tmux）。
        "plan-ack",
        "plan-unack",
        "plan-return",
        // 换号重启：可撤档（步与步之间 await，起 tmux 的几步自己挪到阻塞线程池）。
        "session-restart",
        // 现在就换：异步（重启换那一半等 `session-restart`；不重启换那一半自己挪到阻塞线程池）。
        "rotation-switch",
        // 终端管理 L1 三条：阻塞（起 tmux）。
        "terminals-list",
        "terminal-preview",
        "terminal-input",
        // 传输四条：内建（硬臂）。
        "transfer-upload",
        "transfer-download",
        "transfer-start",
        "transfer-stop",
        // 终端实时预览三条：内建（硬臂；订上那一下在异步档里挪进阻塞线程池）。
        "terminal-follow",
        "terminal-follow-ack",
        "terminal-unfollow",
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
    let src = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/launch.rs"
    ));
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
    // 请求自报的前端经 `gate::requester_of`（与 `kill` · `sessions-*` 同一个取法）读 `client`。
    if src.contains("requester_of(args)") {
        found.push("client".to_string());
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
    let mut declared: Vec<String> = spec.field_names().map(str::to_string).collect();
    declared.sort();
    assert_eq!(
        declared, found,
        "\n`launch` 登记的 fields 与它真正读/写的键对不上。\n\
             这面镜子是用来钉文档的（R3）—— 它自己先漂了，钉出来的就是假的。"
    );
}

/// ★**「声明零字段」不许成为免检开关**：零字段的命令在协议参考里就没有字段表，
/// 而有载荷却声明成空等于让契约面整个消失 ⇒ 默认拒绝 ＋ 豁免登记（写明为什么）。
#[test]
fn declaring_zero_fields_needs_a_reason() {
    /// `(命令名, 为什么它可以声明零载荷字段)`。
    const ZERO_FIELD_REASONS: &[(&str, &str)] = &[
        (
            "resolve",
            "载荷是 `ResumeSpec` / `CommandPlan` 两个结构体：协议参考的出入参那一节从结构体本身生成，\
             在这里再列一份字段名就是第二个源头。",
        ),
        ("ping", "零载荷：问活，回 `ok`。"),
    ];
    let mut unexplained: Vec<&str> = Vec::new();
    let mut zero = 0usize;
    for spec in super::REGISTRY {
        if !spec.fields.is_empty() {
            continue;
        }
        zero += 1;
        if !ZERO_FIELD_REASONS.iter().any(|(n, _)| *n == spec.name) {
            unexplained.push(spec.name);
        }
    }
    assert!(
        zero >= 1,
        "没有任何命令声明零字段 —— 本条在空转。\
             若确实全都列了字段，请把本条连同 `ZERO_FIELD_REASONS` 一起删掉（别留空转的判据）"
    );
    assert!(
        unexplained.is_empty(),
        "这些命令把 `fields` 声明成空：{unexplained:?}\n\
             空声明 ⇒ 协议参考里没有它的字段表。要么把载荷字段列出来，要么在 `ZERO_FIELD_REASONS` 里写明为什么它没有可列的字段。"
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
    // 写那一侧从此是**两张表**：写面 `MANAGE_COMMANDS` ＋ 上传提交
    //   `files_commit::COMMIT_COMMANDS`（`readonly_guard` 第三层第二个登记的模块）。减法按两张表的并。
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
            // 解压（`control/files_extract.rs`，第三层第四个模块）。
            .chain(
                crate::control::files_extract::EXTRACT_COMMANDS
                    .iter()
                    .map(|c| c.name)
                    .collect::<Vec<_>>(),
            )
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
        (crate::control::files_write::MANAGE_COMMANDS, |c, a| {
            crate::control::files_write::answer_wire(
                c,
                a,
                &super::registry::file_manager::SESSION_PORT,
            )
        }),
        (crate::control::files_commit::COMMIT_COMMANDS, |c, a| {
            crate::control::files_commit::answer_wire(c, a)
        }),
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
        let mut got_fields: Vec<&str> = spec.field_names().collect();
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
        if let Err(crate::control::files_write::WriteFail {
            code, said: msg, ..
        }) = out
        {
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
        let mut got_fields: Vec<&str> = spec.field_names().collect();
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
        if let Err(crate::files::Refused {
            code, said: msg, ..
        }) = out
        {
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
        .filter(|s| crate::control::cli_control::cli_exposed(s))
        .map(|s| crate::control::cli_control::flag_of(s.name))
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
