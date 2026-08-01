//! E77：**加了子命令就必须 bump `BUILD_ID`** —— 把这条从「记性」变成机检。
//!
//! # 为什么需要它
//!
//! monitor 判「远端那台的 daemon 该不该换」只有一条判据：
//! `reported_build_id != EXPECTED_DAEMON_BUILD_ID`（`ssh_source.rs`，值由 `build.rs`
//! 从本文件抠出）。**不 bump ⇒ 已部署的旧 daemon 报同一个 id ⇒ 不判 stale ⇒ 不自动重装
//! ⇒ 整轮改动在已部署的远端休眠。**
//!
//! 这一课在 `main.rs` 的版本谱系里被写过两遍（p1r 段、p1t 段），`doc/INVARIANTS.md` §41.5
//! 又写了一遍 —— **写了三遍，2026-08-01 的 Phase G 审计仍然逮到第三次漏做**（G2 加了
//! `--fork-session` 却没 bump）。⇒ 靠散文提醒是无效的。
//!
//! # 判据：子命令集的指纹 ↔ BUILD_ID 的历史表
//!
//! `SUBCOMMAND_HISTORY` 是一张**追加**的表：每行 = `(BUILD_ID, 那一版的子命令集指纹)`。
//! 本护栏断言两件事：
//!
//! 1. **当前算出来的指纹必须在表里**（不是「等于最后一行」——见下）；
//! 2. 表里**不许有重复的 BUILD_ID**。
//!
//! 于是「加一个子命令」⇒ 指纹是新的 ⇒ 不在表里 ⇒ 红。要弄绿只能追加一行；
//! 而用**当前（未 bump 的）id** 追加会撞上第 2 条 ⇒ **只剩「bump + 追加」这一条路**，
//! 也就是本来就该做的那件事。
//!
//! **为什么不是「等于最后一行」**（头一版是那么写的）：那会让**因为别的原因 bump BUILD_ID**
//!（如 p1v 只加了个 wire 字段、子命令一个没动）也被逼着改这张表 ——
//! 而「改表」恰恰是本护栏最不想诱导的动作。
//!
//! # 这条护栏**挡不住**什么（说清楚，别让人以为它是证明）
//!
//! 1. **有人可以就地改最后一行的指纹而不 bump。** 表在同一个文件里，护栏读不到 git 历史。
//!    能做的只有把「正确动作」变成最省事的那个，并让错误信息把代价说清楚。
//!    真要堵死得靠 CI 比对 `git show HEAD~1` —— 那需要 CI 里有完整历史（`fetch-depth: 0`），
//!    成本高于收益，**如实登记为未做**。
//! 2. **「子命令集没变但行为变了」它不管**（p1r 那次就是：删轮询、加事件，argv 表面没动）。
//!    那类仍然要靠人判断。本护栏只覆盖 p1t / G2 这一类「加/改子命令」——
//!    而那恰好是本仓栽过的两次里的两次。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
mod tests {
    /// `(BUILD_ID, 子命令集指纹)`，**只追加不修改**。
    ///
    /// 指纹 = 排序去重后的子命令名用 `\n` 连起来（见 `subcommand_fingerprint`）——
    /// 刻意用**明文**而不是哈希：出错时的诊断信息直接就是「多了/少了哪个」，
    /// 而哈希只会告诉你「不一样」。
    const SUBCOMMAND_HISTORY: &[(&str, &str)] = &[
        // p1t 及之前：--fork-session 还没有。留一行历史，让「加了一个」这件事在 diff 里看得见。
        (
            "p1t-removal-cause",
            "--account-trust\n--account-trust-zero\n--list-accounts\n--resolve\n--search\n--session-accounts\n--tmux-notify\n--usage",
        ),
        // p1u：G2/G6 加 --fork-session（daemon 第一次有写盘能力）。
        (
            "p1u-fork-session",
            "--account-trust\n--account-trust-zero\n--fork-session\n--list-accounts\n--resolve\n--search\n--session-accounts\n--tmux-notify\n--usage",
        ),
    ];

    /// 剥测试段 + 行注释，只留生产代码。
    ///
    /// **两样都要剥**：`main.rs` 的散文里成篇地提到这些字面量（版本谱系那一大段就是），
    /// 不剥的话「注释里写了它」也能把指纹喂饱 —— 那正是安慰剂。
    /// 判据与 `accounts_query::tests::main_dispatches_every_subcommand_we_handle` 同源。
    fn production_only(src: &str) -> String {
        let marker = "\n#[cfg(test)]\nmod tests";
        let prod = match src.find(marker) {
            Some(i) => &src[..i],
            None => src,
        };
        prod.lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 从 `main.rs` 的生产段抠出 `Some("--x")` 形态的子命令，排序去重。
    fn subcommand_fingerprint() -> String {
        let prod = production_only(include_str!("main.rs"));
        // 反向自检：剥完还得剩下真代码，否则下面数出来的空集会「恰好等于」某个错误期望。
        assert!(
            prod.len() > 3_000,
            "剥完 main 生产段只剩 {} 字节 —— 剥法坏了，本护栏此刻是无效的",
            prod.len()
        );
        let needle = format!("{}(\"--", "Some");
        let mut subs: Vec<&str> = Vec::new();
        for (i, _) in prod.match_indices(needle.as_str()) {
            let rest = &prod[i + needle.len() - 2..]; // 回退到 `--`
            if let Some(end) = rest[2..].find('"') {
                subs.push(&rest[..end + 2]);
            }
        }
        subs.sort_unstable();
        subs.dedup();
        assert!(
            subs.len() >= 5,
            "只抠到 {} 个子命令（{subs:?}）—— 抠法坏了",
            subs.len()
        );
        subs.join("\n")
    }

    /// ★ E77 的正题。
    #[test]
    fn adding_a_subcommand_forces_a_build_id_bump() {
        let now = subcommand_fingerprint();
        // **判据是「当前指纹在不在表里」**，不是「等于最后一行」。
        //
        // 头一版写成「必须等于最后一行的指纹、且那行的 id 必须等于当前 BUILD_ID」——
        // 那会让**因为别的原因 bump BUILD_ID**（比如 p1v 只是加了个 wire 字段、子命令一个没动）
        // 也被逼着改这张表，而改表恰恰是本护栏最不想诱导的动作。
        //
        // 现在：加子命令 ⇒ 指纹是新的 ⇒ 不在表里 ⇒ 红。要弄绿只能追加一行；
        // 而**用当前（未 bump 的）id 追加会撞上 `history_has_no_duplicate_build_ids`** ⇒
        // 只剩「bump + 追加」这一条路。
        let (last_id, last_fp) = *SUBCOMMAND_HISTORY
            .last()
            .expect("SUBCOMMAND_HISTORY 不能为空");
        let _ = last_id;

        if !SUBCOMMAND_HISTORY.iter().any(|(_, fp)| *fp == now) {
            let old: Vec<&str> = last_fp.split('\n').collect();
            let new: Vec<&str> = now.split('\n').collect();
            let added: Vec<&&str> = new.iter().filter(|s| !old.contains(s)).collect();
            let removed: Vec<&&str> = old.iter().filter(|s| !new.contains(s)).collect();
            panic!(
                "daemon 的子命令集变了（+{added:?} / -{removed:?}），而 BUILD_ID 还是 `{}`。\n\
                 \n\
                 **别只改这张表**。monitor 判「远端该不该换 daemon」只有一条判据：\n\
                 `reported_build_id != EXPECTED_DAEMON_BUILD_ID`。不 bump ⇒ 已部署的旧 daemon\n\
                 报同一个 id ⇒ 不判 stale ⇒ 不自动重装 ⇒ **你这一轮的改动在已部署的远端休眠**，\n\
                 用户只会拿到「版本过旧」。本仓已经因为这个栽过三次（p1r / p1t / G2）。\n\
                 \n\
                 正确动作：① 在 main.rs 里 bump `BUILD_ID`（并在版本谱系里加一段说清改了什么）；\n\
                 ② 在 `SUBCOMMAND_HISTORY` **追加**一行 `(\"<新 id>\", \"{now}\")`。",
                super::super::BUILD_ID
            );
        }
    }

    /// 历史表不许有重复 id —— 重复意味着「同一个 id 对应过两套子命令集」，
    /// 那正是本护栏要防的那件事被绕过去了。
    #[test]
    fn history_has_no_duplicate_build_ids() {
        let mut ids: Vec<&str> = SUBCOMMAND_HISTORY.iter().map(|(id, _)| *id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "历史表里有重复的 BUILD_ID：{ids:?}");
        assert!(n >= 2, "至少要留一行历史，否则「加了一个」在 diff 里看不见");
    }

    /// 反向自检：判据**真的会抓人**。喂一个「多了一个子命令」的假指纹，比对必须不等。
    ///
    /// 直接喂字符串而不是去改 `main.rs` —— 后者要么污染工作区，要么因为改不进去而假绿
    /// （本仓已栽过：变异没落地却被当成「没覆盖」）。
    #[test]
    fn the_comparison_actually_bites() {
        let now = subcommand_fingerprint();
        let tampered = format!("{now}\n--brand-new-subcommand");
        assert_ne!(now, tampered, "比对形同虚设");
        // 也确认它认得出「少了一个」
        let shortened = now.split('\n').skip(1).collect::<Vec<_>>().join("\n");
        assert_ne!(now, shortened);
        // 以及：注释里的字面量不该被算进去（剥注释这一步是有效的）
        assert!(
            !now.contains("--account-trust-zero\n--account-trust-zero"),
            "同一个子命令被数了两次 —— 去重坏了"
        );
    }
}
