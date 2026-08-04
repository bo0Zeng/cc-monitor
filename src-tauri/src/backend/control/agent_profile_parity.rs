//! F06：**agent 适配表的跨语言对拍** —— C4「ccm 变零决策执行臂」的前置。
//!
//! # 为什么这是前置，而不是 F06 本体
//!
//! C4 要 ccm 只「上报上下文 → 拿 argv → 设 env → `exec`」。摸底实测**今天搬不动**
//! （见功能件 §0）：本机没有 daemon 在跑（F05b 未做），而远端那条 `--resolve` 的契约
//! **与仓外 aterm 冻结在 2026-07-18**、范围是 MVP（只产「首个候选 + `--resume <sid>`」），
//! 给不出 ccm 要的 argv，**为 ccm 扩它就是破坏那份冻结的契约**。
//!
//! 那今天该做什么？——**把「搬之前必须成立的那个前提」钉住**：
//! **三份副本今天逐字一致。** 不一致的话搬完不知道搬没搬对，
//! 而且那种不一致**今天不会红**（三份各自的测试都过）。
//!
//! # 三份副本，形状还不一样
//!
//! | 副本 | 覆盖的 agent | 形态 |
//! |---|---|---|
//! | monitor Rust `adapter::for_kind` | claude + codex | trait 方法 |
//! | `shared/ccm` 的 `agent_*` | claude + codex | shell `case` |
//! | 前端 `AGENT_PROFILE` | **只有 claude** | 单 profile 常量 |
//!
//! ⚠ 第三份**只有 claude** —— 那不是漏，是它今天只服务 claude 那条路。
//! 本模块只对拍 Rust 那一轨与夹具；TS 那轨由 `src/agent-profile.vitest.ts` 自己读同一份夹具。
//!
//! # ★ 两项 ccm **独有**的决策，Rust 侧根本没有对侧
//!
//! `agent_has_identity`（有没有 per-PID session 文件 ⇒ 要不要起身份回填 poller）与
//! `agent_needs_bus_id`（要不要把 tmux 会话名注入 `CC_BUS_ID`）——
//! **monitor 的 `AgentAdapter` trait 里没有这两个方法。**
//!
//! ⇒ C4 要求 ccm 零决策，而这两个决策**今天没有地方可搬** ——
//! 得先在 Rust 侧建它们。**如实登记为 F06 的真实阻塞**（见 `THE_TWO_CCM_ONLY_DECISIONS`），
//! 不假装「搬一搬就好了」。

#[cfg(test)]
mod tests {
    use crate::adapter::{self, AgentKind};

    const GOLDEN: &str = include_str!("fixtures/agent-profile-golden.tsv");

    /// ★ `shared/ccm` 独有、**Rust 侧无对侧**的两个决策。
    ///
    /// 它们是 C4 的真实阻塞：ccm 要零决策，就得先有人接这两个。
    /// **写在这里而不是只写在文档里** —— 下面那条判据会核对它们仍然没有对侧，
    /// 一旦 Rust 侧真加了同名方法，本条**主动红**，提醒回 F06 把它们搬过去。
    const THE_TWO_CCM_ONLY_DECISIONS: &[(&str, &str)] = &[
        (
            "agent_has_identity",
            "该 agent 有没有 per-PID session 文件（决定要不要起身份回填 poller）",
        ),
        (
            "agent_needs_bus_id",
            "要不要把 tmux 会话名注入 CC_BUS_ID（codex 的沙箱够不着 tmux socket）",
        ),
    ];

    fn rows() -> Vec<(String, String, String)> {
        GOLDEN
            .lines()
            .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
            .map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                assert_eq!(f.len(), 3, "夹具行不是 3 列：{l:?}");
                let v = if f[2] == "<empty>" { "" } else { f[2] };
                (f[0].to_string(), f[1].to_string(), v.to_string())
            })
            .collect()
    }

    /// ★ 抽取器自检：夹具解析空 / 偏了时下面那条会零命中地绿。
    #[test]
    fn the_golden_table_actually_parses_and_covers_both_agents() {
        let r = rows();
        assert!(
            r.len() >= 6,
            "只解析出 {} 行 —— 夹具路径或解析坏了",
            r.len()
        );
        for a in ["claude", "codex"] {
            assert!(r.iter().any(|x| x.0 == a), "夹具里没有 agent `{a}`");
        }
        for k in [
            "default_launcher",
            "resume_kind",
            "resume_token",
            "nested_env",
        ] {
            assert!(r.iter().any(|x| x.1 == k), "夹具里没有 key `{k}`");
        }
        // 必须有一行**空值**（codex 的 resume_flag/nested_env）—— 否则 `<empty>` 那条路没被走过。
        assert!(
            r.iter().any(|x| x.2.is_empty()),
            "夹具里一行空值都没有 —— `<empty>` 的解析没被覆盖"
        );
    }

    /// ★ Rust 这一轨与夹具逐字相同。
    #[test]
    fn the_rust_adapter_agrees_with_the_golden_table() {
        let mut bad = Vec::new();
        for (agent, key, want) in rows() {
            let kind = match agent.as_str() {
                "claude" => AgentKind::ClaudeCode,
                "codex" => AgentKind::Codex,
                other => panic!("夹具里出现了未知 agent `{other}` —— 加 agent 要来这里表态"),
            };
            let a = adapter::for_kind(kind);
            let got = match key.as_str() {
                "default_launcher" => a.default_launcher().to_string(),
                // ⚠ Rust 的 `resume_flag()` 存的是**那个字面量**（claude `--resume`、codex `resume`），
                // 不是「调用形态」。形态那一列由下面 `the_resume_kind_column_matches_reality` 钉。
                "resume_token" => a.resume_flag().to_string(),
                // Rust 侧今天**没有**「形态」这个方法 —— 形态是从字面量推的：
                // 以 `--` 开头 = flag，否则 = subcommand。这条推法本身由那条判据钉住。
                "resume_kind" => {
                    if a.resume_flag().starts_with("--") {
                        "flag".to_string()
                    } else {
                        "subcommand".to_string()
                    }
                }
                "nested_env" => a.nested_env_to_scrub().join(" "),
                other => panic!("夹具里出现了未知 key `{other}` —— 加一项要来这里表态"),
            };
            if got != want {
                bad.push(format!("  {agent}.{key}: 期望 {want:?} 实得 {got:?}"));
            }
        }
        assert!(
            bad.is_empty(),
            "Rust adapter 与 agent 适配表不一致：\n{}\n\
             ⚠ 这张表是 **C4「ccm 变零决策」的前置** —— 三份副本必须先逐字一致，\n\
             不然搬完不知道搬没搬对，而那种不一致今天不会红（三份各自的测试都过）。",
            bad.join("\n")
        );
    }

    /// ★ `resume_kind` 那一列不是凭空写的：**它与 Rust 的字面量互相印证**。
    ///
    /// 推法是「以 `--` 开头 = flag，否则 = subcommand」。这条推法很朴素，
    /// 所以要**双向**钉：夹具说 flag 的必须以 `--` 开头，说 subcommand 的必须不以 `--` 开头。
    /// 否则夹具那一列就成了一句没人验证的散文。
    #[test]
    fn the_resume_kind_column_matches_reality() {
        let r = rows();
        let get = |agent: &str, key: &str| -> String {
            r.iter()
                .find(|x| x.0 == agent && x.1 == key)
                .map(|x| x.2.clone())
                .unwrap_or_else(|| panic!("夹具里缺 {agent}.{key}"))
        };
        for agent in ["claude", "codex"] {
            let kind = get(agent, "resume_kind");
            let token = get(agent, "resume_token");
            match kind.as_str() {
                "flag" => assert!(
                    token.starts_with("--"),
                    "{agent} 记成 flag 但 token `{token}` 不以 `--` 开头"
                ),
                "subcommand" => assert!(
                    !token.starts_with("--") && !token.is_empty(),
                    "{agent} 记成 subcommand 但 token `{token}` 不像子命令名"
                ),
                other => panic!("{agent} 的 resume_kind `{other}` 不在 flag/subcommand 里"),
            }
        }
    }

    /// ★ **F06 摸底顺出的真缺口**：`shared/ccm` **不支持 codex 的 subcommand 形 resume**。
    ///
    /// 实测：`agent_resume_flag` 对 codex 返回**空**，而 `ccm:250` 拿空值当
    /// 「不支持 resume」的哨兵直接 `die "agent=$agent 不支持 resume"`；
    /// argv 构建也只会 `argv+=("$rf" "$sid")`（flag 形），**没有子命令形那一支**。
    ///
    /// 而 Rust 那边的注释逐字写着「**F6 让命令构建支持 subcommand 形**」——
    /// 那个 F6 就是本件。但**本件刻意不做那个功能改动**：
    /// 「让 ccm 支持 codex resume」属 codex 支持那一族（`codex-phase2` / `daemon-codex` 工作区），
    /// **不是 C4「ccm 变零决策」**。⇒ 按三档走「绕」：如实登记，不削判据、不顺手改。
    ///
    /// 本条钉住那个**前提**：ccm 今天仍然拒绝 codex resume。
    /// 它一旦支持了 ⇒ **主动红**，回来把夹具的 ccm 那一轨补上真对拍。
    #[test]
    fn ccm_still_refuses_codex_resume_so_the_gap_is_still_real() {
        let ccm = read_ccm();
        // ccm 用「空 resume flag」当不支持的哨兵 —— 两处都要在，缺一处这个前提就变了。
        assert!(
            ccm.contains(r#"codex) printf '' ;;"#),
            "`agent_resume_flag` 对 codex 不再返回空 —— **这多半是好事**：\n\
             ccm 可能支持了 codex 的 subcommand 形 resume ⇒ 回 F06 把夹具的 ccm 那一轨补上真对拍。"
        );
        assert!(
            ccm.contains("不支持 resume"),
            "`ccm` 里那句「不支持 resume」的 die 不见了 —— 同上，前提变了，回 F06 重裁。"
        );
    }

    fn read_ccm() -> String {
        let s = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("src-tauri 的上级")
                .join("shared/ccm"),
        )
        .expect("读不到 shared/ccm");
        assert!(s.len() > 5000, "shared/ccm 只有 {} 字节，抽错了？", s.len());
        s
    }

    /// ★ **前提触发器**：那两个 ccm 独有的决策**仍然没有 Rust 对侧**。
    ///
    /// 一旦 `AgentAdapter` trait 里出现同名方法 ⇒ 本条**主动红**：
    /// 那时 C4 的那一半就能搬了，回 F06 重新裁定。
    #[test]
    fn the_two_ccm_only_decisions_still_have_no_rust_counterpart() {
        let src = guard_core::production_code(include_str!("../../adapter.rs"));
        assert!(
            src.contains("pub trait AgentAdapter"),
            "抽不到 `AgentAdapter` trait —— 路径或剥法坏了，本条会零命中地绿"
        );
        for (name, what) in THE_TWO_CCM_ONLY_DECISIONS {
            // ccm 的 `agent_has_identity` 在 Rust 里会叫 `has_identity`。
            let rust_name = name.trim_start_matches("agent_");
            assert!(
                !src.contains(&format!("fn {rust_name}(")),
                "`AgentAdapter` 里出现了 `{rust_name}()` —— **这多半是好事**：\n\
                 `shared/ccm` 的 `{name}`（{what}）终于有 Rust 对侧了，\n\
                 ⇒ C4 的那一半可以搬了。请回 F06 重新裁定，并把本条与那份登记一起更新。"
            );
        }
        // 反向锚点：ccm 里**确实**还有这两个决策 —— 否则本条在断言「谁都没有」。
        let ccm = read_ccm();
        for (name, _) in THE_TWO_CCM_ONLY_DECISIONS {
            assert!(
                ccm.contains(name),
                "`shared/ccm` 里找不到 `{name}` —— 它被改名或删了，\
                 那上面那条就退化成「谁都没有这个决策」了"
            );
        }
    }
}
