//! F03：**monitor 这一侧对 §34 Gate 2 判定表的独立对拍**。
//!
//! 判定表 `fixtures/gate2-golden.tsv` 有三个读者，**各自独立读它**：
//!
//! | 轨道 | 谁 | 读法 |
//! |---|---|---|
//! | monitor（Rust） | **本模块** | `include_str!` + `gate_core::gate2` |
//! | daemon（Rust） | `remote-daemon-proto/src/control/gate.rs` 的测试 | 跨仓相对路径 `include_str!` |
//! | 真二进制（bash） | `e2e/daemon-gate2-acceptance.sh` | `cut -f`，跑真 daemon + 真 tmux |
//!
//! ⚠ **绝不许一侧在运行时去调另一侧** —— 那样两侧一起错也全绿。
//! 这条纪律与 `launch_cli_parity` / `launch_payload_parity` 同族：
//! 夹具入库，两侧各自对夹具，夹具本身进 git ⇒ 谁改了判定表 diff 里看得见。
//!
//! # 本模块与 `gate-core` 自己的单测有什么不同
//!
//! `gate-core` 的单测是**作者写给自己的**；这张表是**跨轨契约**。
//! 区别在改动成本：改 gate-core 的单测只影响那个 crate，
//! 改这张表会让**三条轨道同时**要重新解释 —— 这正是我们要的摩擦。

#[cfg(test)]
mod tests {
    const GOLDEN: &str = include_str!("fixtures/gate2-golden.tsv");

    fn rows() -> Vec<(String, String, Option<String>, String)> {
        GOLDEN
            .lines()
            .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
            .map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                assert_eq!(f.len(), 4, "夹具行不是 4 列：{l:?}");
                let sid = match f[2] {
                    "<none>" => None,
                    "<unset>" => Some(String::new()),
                    v => Some(v.to_string()),
                };
                (f[0].to_string(), f[1].to_string(), sid, f[3].to_string())
            })
            .collect()
    }

    /// ★ 抽取器自检：夹具解析空时下面那条会零命中地绿。
    #[test]
    fn the_golden_table_actually_parses() {
        let r = rows();
        assert!(
            r.len() >= 20,
            "只解析出 {} 行 —— 夹具路径或解析坏了",
            r.len()
        );
        for want in ["allowed_by_name", "allowed_by_remote_sid", "rejected"] {
            assert!(
                r.iter().any(|x| x.3 == want),
                "夹具里一行 `{want}` 都没有 —— 表偏了"
            );
        }
        // case_id 不许重复：重复的 id 会让 e2e 那一轨 `grep` 到两行。
        let mut ids: Vec<&str> = r.iter().map(|x| x.0.as_str()).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(
            n,
            ids.len(),
            "夹具里有重复的 case_id —— e2e 那一轨会 grep 到两行"
        );
    }

    /// ★ monitor 这一侧对同一张表给出同样的判定。
    #[test]
    fn the_monitor_side_agrees_with_the_golden_table() {
        let mut bad = Vec::new();
        for (id, name, sid, want) in rows() {
            let got = gate_core::gate2(&name, sid.as_deref()).as_str();
            if got != want {
                bad.push(format!(
                    "  {id}: name={name:?} sid={sid:?} 期望={want} 实得={got}"
                ));
            }
        }
        assert!(
            bad.is_empty(),
            "monitor 侧与判定表不一致：\n{}",
            bad.join("\n")
        );
    }

    /// ★ **判定表覆盖了 monitor 生产路径真正会分支的那一处**：
    /// `tmux.rs` 用 `is_ccm_tmux_name` 决定「要不要多花一次 round-trip 问远端 `@ccm_sid`」。
    ///
    /// 表里 `allowed_by_name` 的那些行 ⇔ 不需要问远端；其余都需要。
    /// 这条钉的是「两个函数没有各走各的」——`needs_remote_sid` 漂了本条就红。
    #[test]
    fn needs_remote_sid_is_the_exact_complement_of_allowed_by_name() {
        for (id, name, _sid, want) in rows() {
            let needs = gate_core::needs_remote_sid(&name);
            let by_name = want == "allowed_by_name";
            assert_eq!(
                needs, !by_name,
                "{id}: name={name:?} —— `needs_remote_sid` 与判定表的 `allowed_by_name` 对不上。\n\
                 这两者必须互补：名字命中 ⇒ 零 IO 直接放行；不命中 ⇒ 必须问远端。\n\
                 对不上意味着生产路径会**跳过一次本该做的远端核验**（或白花一次 round-trip）。"
            );
        }
    }
}
