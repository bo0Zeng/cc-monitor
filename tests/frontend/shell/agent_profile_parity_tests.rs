use crate::adapter::{self, AgentKind};

const GOLDEN: &str = include_str!("../../../tests/__fixtures__/agent-profile-golden.tsv");

/// ★ `shared/ccm` 独有、**Rust 侧无对侧**的两个决策。
///
/// 它们是 C4 的真实阻塞：ccm 要零决策，就得先有人接这两个。
/// **写在这里而不是只写在文档里** —— 下面那条判据会核对它们仍然没有对侧，
/// 一旦 Rust 侧真加了同名方法，本条**主动红**，提醒回 F06 把它们搬过去。
const THE_TWO_CCM_ONLY_DECISIONS: &[(&str, &str)] = &[
    (
        "agent_has_identity",
        // ⚠ `U-NP④`（08-14）改过这句：ccm 里那条身份 poller 已删，这个决策今天
        // 决定的是「要不要要求后端在场」（身份 `@ccm_sid` 只由后端打）。
        // 与 `shared/ccm` 里同一行的措辞保持一致 —— 两处一起改，别只改一边。
        "该 agent 有没有 per-PID session 文件（决定这个会话有没有身份、要不要要求后端在场）",
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

/// ★ `K-R93`：**前端那份画像的取数口**也与夹具一致。
///
/// 上面那条钉的是 `AgentAdapter` trait；本条钉的是 `adapter::agent_profile_facts`
/// —— 它才是**前端今天真正拿到的那份值**的源头（经生成物 `src/frontend/ui/generated/
/// agent-profile-table.ts` 送到 TS）。**两条都要**：trait 与取数口是两个能各自漂的面，
/// 取数口完全有可能被人就地写死一份而 trait 一个字没动。
#[test]
fn the_front_end_read_port_agrees_with_the_golden_table() {
    let mut bad = Vec::new();
    for (agent, key, want) in rows() {
        let kind = match agent.as_str() {
            "claude" => AgentKind::ClaudeCode,
            "codex" => AgentKind::Codex,
            other => panic!("夹具里出现了未知 agent `{other}` —— 加 agent 要来这里表态"),
        };
        let f = adapter::agent_profile_facts(kind);
        assert_eq!(f.agent, agent, "取数口的表键与夹具第一列对不上");
        let got = match key.as_str() {
            "default_launcher" => f.default_launcher.to_string(),
            "resume_token" => f.resume_token.to_string(),
            "resume_kind" => f.resume_kind.to_string(),
            "nested_env" => f.nested_env.join(" "),
            other => panic!("夹具里出现了未知 key `{other}` —— 加一项要来这里表态"),
        };
        if got != want {
            bad.push(format!("  {agent}.{key}: 期望 {want:?} 实得 {got:?}"));
        }
    }
    assert!(
        bad.is_empty(),
        "`agent_profile_facts`（前端那份画像的取数口）与 agent 适配表不一致：\n{}\n\
             ⚠ 前端拿到的值是从这里生成的 —— 这里漂了，前端就跟着漂。",
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

/// ★ **F06 那个缺口的前提**：从前 ccm 拒 codex resume（`resume_flag` 对 codex 回 `None` ⇒ `noResume`）。
///
/// 〔V138〕ccm 成了 claude 的壳：它不再有 resume 这个决定，`--resume` / `resume <sid>` 原样交给 agent
/// （codex 用户敲 `ccm --agent codex resume <sid>` 就是 `codex resume <sid>`）⇒ 那个缺口按「不需要 ccm 管」结案。
/// 本条钉新前提：ccm 里没有 resume 旗标表、也没有「不支持 resume」那句拒。它一旦长回来 ⇒ 红，回 F06 重裁。
#[test]
fn ccm_makes_no_resume_decision_since_it_became_a_shell() {
    let ccm = read_ccm();
    assert!(
        !ccm.contains("fn resume_flag(") && !ccm.contains("beArgv.validate.noResume"),
        "`control/ccm/` 又长出了 resume 的决定 —— V138 之后 ccm 只看不吃 `--resume`，回 F06 重裁。"
    );
}

/// 🔴 〔`K-R48` 第二拍 09-11〕**住址换了：`shared/ccm` → `control/ccm/`（backend crate）。**
///
/// 〔用@09-11 `K33`〕「不要有什么 bash 脚本」⇒ 那个脚本删了，它那几个 `agent_*` 决策
/// 整条搬进了 `src/backend/control/ccm/`（Rust）。
/// 本文件那两条判据问的是「**那两个决策今天住在哪一侧**」—— 问题没变，读的文本换了语言。
fn read_ccm() -> String {
    let dir = crate::guard_support::repo_root().join("src/backend/control/ccm");
    let s: String = ["mod.rs", "argv.rs", "plan.rs"]
        .iter()
        .map(|f| {
            std::fs::read_to_string(dir.join(f))
                .unwrap_or_else(|e| panic!("读不到 control/ccm/{f}：{e}"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        s.len() > 5000,
        "control/ccm 三份只有 {} 字节，抽错了？",
        s.len()
    );
    s
}

/// ★ **前提触发器**：那两个 ccm 独有的决策**仍然没有 Rust 对侧**。
///
/// 一旦 `AgentAdapter` trait 里出现同名方法 ⇒ 本条**主动红**：
/// 那时 C4 的那一半就能搬了，回 F06 重新裁定。
#[test]
fn the_two_ccm_only_decisions_still_have_no_rust_counterpart() {
    let src =
        guard_core::production_code(include_str!("../../../src/frontend/shell/src/adapter.rs"));
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
        // 🔴 〔`K-R48` 第二拍〕搬进 Rust 之后名字掉了 `agent_` 前缀
        //    （`agent_has_identity` → `has_identity`）——**登记表那两个名字刻意不改**：
        //    它们是 `F06` 那笔账的原文，改了就对不上账。这里按前缀差异找。
        let in_ccm = name.trim_start_matches("agent_");
        assert!(
            ccm.contains(&format!("fn {in_ccm}(")),
            "`control/ccm/` 里找不到 `{in_ccm}()`（登记表里叫 `{name}`）—— 它被改名或删了，\
                 那上面那条就退化成「谁都没有这个决策」了"
        );
    }
}
