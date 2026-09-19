/// ★★ 〔audit-0805 08-06〕**把「不许」的对象从一个名字换成一族入口**。
///
/// # 原来那条漏了什么
///
/// 它禁的是字面量 `list_remote_tmux`。而**轮询回潮的真实形状不必用那个名字** ——
/// 08-06 实测：在本模块生产段里直接写
/// `ssh_source::connect_and_exec_cmd(cfg, "tmux ls …")`，**monitor 1013 条全绿**。
/// 也就是说这条护栏防的那件事（每 8s 一条新 SSH）可以原样回来而它不响。
///
/// # 改法：危险集合**从 `ssh_source` 的公开面派生**，不写死
///
/// 本仓的原则是「枚举式白名单优于黑名单」（`structural_scan.rs` 头注）。
/// 这里做不到纯白名单（生产段该引用什么无法穷举），但能把黑名单**从固定名单
/// 换成派生集合**：扫 `ssh_source.rs` 的 `pub (async) fn`，凡名字里带
/// `exec` / `connect` / `list_remote` 的都算「能开 SSH 的入口」。
/// ⇒ **将来新增一个 exec 入口，自动被纳入** —— 这正是固定 needle 做不到的。
///
/// 实测：今天派生出 4 个入口，而本模块生产代码里对 `ssh_source::` 的引用
/// **剥掉注释后是零处**。
#[test]
fn the_reconcile_path_touches_no_ssh_exec_entry_point() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let ssh =
        std::fs::read_to_string(root.join("ssh_source.rs")).expect("读不到 ssh_source.rs");
    let mut entries: Vec<String> = Vec::new();
    for l in guard_core::production_code(&ssh).lines() {
        let t = l.trim();
        let Some(rest) = t
            .strip_prefix("pub async fn ")
            .or_else(|| t.strip_prefix("pub fn "))
        else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.contains("exec") || name.contains("connect") || name.contains("list_remote") {
            entries.push(name);
        }
    }
    entries.sort();
    entries.dedup();
    // ★ 派生自检：一个都派生不出来 ⇒ 下面这条是空转的。
    assert!(
        entries.len() >= 3,
        "从 `ssh_source` 只派生出 {} 个 SSH 入口 —— 派生坏了（08-06 实测 4 个）：{entries:?}",
        entries.len()
    );

    let me = guard_core::production_code(include_str!("../../src/bridge/src/tmux_reconcile.rs"));
    let used: Vec<&String> = entries
        .iter()
        .filter(|e| me.contains(format!("ssh_source::{e}").as_str()))
        .collect();
    assert!(
        used.is_empty(),
        "对账模块的生产段用了这些 SSH 入口：{used:?}\n\n\
             ⇒ 「对账拿不到数据时顺手 exec 一下」正是 B2 治好的那件事的回潮形状\n\
             （每 8s 一条新 SSH，远端 sshd 日志刷屏）。\n\
             ★ 与上面那条的区别：那条只禁一个名字 `list_remote_tmux`，\n\
             而 08-06 实测**换个入口自己拼 `tmux ls` 就能绕过去**（全绿）。\n\
             本条的危险集合从 `ssh_source` 的公开面派生，新增入口自动纳入。"
    );

    // ★★ **再收一层：本模块生产段不许引用 `ssh_source::` 的任何东西**
    // 〔audit-0805 08-09〕。
    //
    // 上面那半是**按名字**派生的（`exec` / `connect` / `list_remote`）—— 而
    // `ssh_source` 的公开面里就有一个 **`pub async fn run(…)`**：它是那条流循环、
    // 真会开 SSH，**三个词一个都不带** ⇒ 用它就能绕过上面那半。
    // 「按怎么写的取样」这一族，本区治了一路，这条派生规则自己也是。
    //
    // ⇒ 换成**按引用**取：08-06 那次改造时就量到「本模块对 `ssh_source::` 的引用
    // 剥掉注释后是**零处**」——既然是零，就把零钉住，不必再猜哪些名字算危险。
    // 真要用（比如将来对账确实需要某个纯本地的助手），在这里登记并说清
    // 「它为什么不会开 SSH」。
    let refs: Vec<&str> = me
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("ssh_source::"))
        .collect();
    assert!(
        refs.is_empty(),
        "对账模块的生产段引用了 `ssh_source::`：\n{}\n\n\
             ★ 上面那半按名字派生，而 `ssh_source::run` 这类**不带 exec/connect 字样**的入口\n\
             照样开 SSH ⇒ 只按名字挡不住。本条按**引用**挡：零引用就没有绕法。\n\
             真要用：在这里登记，并说清它为什么不会开 SSH。",
        refs.join("\n")
    );
}

/// ★ 对账路径不许自己去开 SSH 拉 tmux。
#[test]
fn the_reconcile_path_never_execs_its_own_tmux_listing() {
    let me = include_str!("../../src/bridge/src/tmux_reconcile.rs");
    assert!(
        me.len() > 3000,
        "只读到 {} 字节 —— include_str! 没读到，本断言在空转",
        me.len()
    );
    // 只看生产段：本护栏自己的文档里就写着那个名字。
    //
    // U8a-2a：从 `me.split("\n#[cfg(test)]").next()` 换成共享的 `guard_core`。
    // **动之前实测过**（血泪第 8 条）：本文件里第一个测试模块之后确实没有生产代码，
    // 所以那个近似**今天是对的** —— 换掉不是修 bug，是拆掉一颗定时炸弹：
    // 哪天有人在 `mod tests` 后面加一个生产函数，扫描面会**静默**把它漏掉
    // （`ssh_source.rs` 上同一个近似会砍掉三分之二的扫描面，那边是真的踩了）。
    let prod = guard_core::production_code(me);
    guard_core::assert_no_test_code("tmux_reconcile.rs", &prod);
    assert!(
        prod.len() > 1000 && prod.len() < me.len(),
        "剥完生产段只剩 {} 字节（原文 {}）—— 剥法坏了",
        prod.len(),
        me.len()
    );
    assert!(
        prod.contains("fn reconcile_step"),
        "生产段里没有 `reconcile_step` —— 剥过头了，本护栏此刻扫的不是对账路径"
    );
    assert!(
        !prod.contains("list_remote_tmux"),
        "对账路径开始自己 exec `tmux ls` 了 —— 那是**轮询回潮**。\n\
             B2 加推送帧正是为了替掉每 8s 新建 SSH 的轮询（治远端 sshd 日志刷屏）。\n\
             对账拿不到数据时应当**等下一帧**，不是顺手 exec 一条。\n\
             决策点（`tabs.ts`）用 `list_remote_tmux` 是另一回事，那条路要保留。"
    );
}
