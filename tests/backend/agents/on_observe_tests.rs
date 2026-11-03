//! agent 适配层判据里经观测面的那几条：适配层是独立的包（`backend-agents`）、够不到观测面 ⇒ 挂在 `observe` 底下，
//! 观测面拆成包时跟着走。夹具是结构占位，正文都是编的。

/// Claude 一行 ⇒ 通用记录的金样（骨架行那一条经观测面）。挂在这里而不在 `observe/mod.rs`：通用层不写哪一家的目录名。
#[path = "claudecode/record_of_tests.rs"]
mod claudecode_record_of;

/// Claude 答题口那一侧（`claudecode/asks.rs`）读出的问题，经观测面穿成需手动那一条。
mod claudecode_asks {
    use serde_json::json;

    /// 命令以变量赋值打头的一次 Bash 批准：需手动那一条的钉条与代码头照问题那一份的 `what` 摆（说得出用意的那一段，不以 `F=` 开头）。
    #[test]
    fn the_needs_line_shows_what_the_ask_says_the_command_does() {
        let v = json!({
            "session_id": "s-1",
            "transcript_path": "/x/s-1.jsonl",
            "cwd": "/w",
            "permission_mode": "default",
            "hook_event_name": "PermissionRequest",
            "tool_name": "Bash",
            "tool_input": {"command": "F=/r/tool-results/b8v1.txt; sed -n '1,40p' \"$F\" | head -5\necho done"},
        });
        let a = crate::agents::claudecode::asks::read(&v).expect("PermissionRequest 读得出");
        let ask = crate::agents::asks::product_of("ask-1", &a, None, 1);
        let want = "sed -n '1,40p' \"$F\" | head -5";
        assert_eq!(ask.what.as_deref(), Some(want));
        let n = crate::observe::recognize::facts::dress_ask_with(None, Some(ask), 2).unwrap();
        assert_eq!(n.what.as_deref(), Some(want));
        assert_eq!(n.head_code.as_deref(), Some(want));
        assert_eq!(n.ask.as_ref().and_then(|x| x.what.as_deref()), Some(want));
    }
}

/// 「谁说的」那一问（`claudecode/text.rs`）答「人」的那条记录，观测面照普通的一句收进大纲。
mod claudecode_text {
    use serde_json::json;

    /// 一轮中途人插的那一句（`attachment queued_command`，`origin.kind: human`）进大纲，身份是它自己的 uuid。
    #[test]
    fn a_mid_turn_line_from_the_person_goes_into_the_outline() {
        let mine = json!({"type":"attachment","parentUuid":"a1","isSidechain":false,
            "attachment":{"type":"queued_command","prompt":"往后排，先做乙","commandMode":"prompt","origin":{"kind":"human"},"humanTurn":true},
            "uuid":"q-1","timestamp":"2026-10-09T01:30:08.000Z","sessionId":"s"});
        let row =
            crate::observe::recognize::user_inputs::user_input_of("claude", &mine).expect("进大纲");
        assert_eq!(row.uuid, "q-1");
    }
}

/// Codex 的 rollout 经观测面冷开（`codex/beat.rs` 答每一拍）：轮次 · 大纲 · 用量 · 骨架索引。
mod codex_beat {
    use crate::observe::record_scan::ScanMap;

    const SAMPLE: &str = include_str!("../../__fixtures__/codex-rollout.turns.jsonl");

    fn cold_open() -> ScanMap {
        ScanMap::scan_with(
            SAMPLE.as_bytes(),
            SAMPLE.len() as u64,
            crate::agents::codex::AGENT_KIND,
            Some(crate::agents::codex::RECORDS),
        )
        .unwrap()
    }

    #[test]
    fn a_cold_open_of_a_codex_rollout_has_turns_an_outline_and_usage() {
        let map = cold_open();
        // 大纲：人说的两句（开发者指令 · 环境注入不算）。
        let outline: Vec<(&str, &str)> = map
            .inputs
            .iter()
            .map(|r| (r.uuid.as_str(), r.excerpt.as_str()))
            .collect();
        assert_eq!(
            outline,
            vec![("m-u1", "list the files"), ("m-u2", "now count them")]
        );
        // 轮次：两轮，各从那一句起；第一轮一次工具 · 一次思考，答完了。
        let turns: Vec<(&str, u32, u32, bool)> = map
            .turns
            .iter()
            .map(|t| (t.uuid.as_str(), t.tools, t.thinking, t.done))
            .collect();
        assert_eq!(turns, vec![("m-u1", 1, 1, true), ("m-u2", 0, 0, true)]);
        assert!(
            map.turns[0].reply.contains("two files"),
            "{:?}",
            map.turns[0].reply
        );
        assert!(map.turns.iter().all(|t| t.pending.is_empty()));
        // 用量：最后一次请求的提示 ＝ 新输入 ＋ 读缓存；全会话三次请求。
        let usage = map.facts.usage.as_ref().expect("用量");
        assert_eq!(usage.prompt_tokens, 1200);
        assert_eq!(usage.peak_prompt_tokens, 1200);
        let tokens = map.facts.tokens.as_ref().expect("全会话用量");
        assert_eq!(tokens.requests, 3);
        assert_eq!(
            (tokens.input, tokens.cache_read, tokens.output),
            (600 + 100 + 100, 400 + 1000 + 1100, 31)
        );
        assert_eq!(
            map.facts.last_say.as_ref().map(|s| s.text.as_str()),
            Some("two")
        );
    }

    /// 骨架索引：人说的那两句带 id 与大纲摘要；系统注入那一行说是系统、不进大纲。
    #[test]
    fn the_index_rows_carry_ids_and_the_outline() {
        let map = cold_open();
        let with_x: Vec<(&str, &str)> = map
            .index
            .iter()
            .filter_map(|r| Some((r.u.as_deref()?, r.x.as_deref()?)))
            .collect();
        assert_eq!(
            with_x,
            vec![("m-u1", "list the files"), ("m-u2", "now count them")]
        );
        let env = map
            .index
            .iter()
            .find(|r| r.u.as_deref() == Some("m-env"))
            .unwrap();
        // 环境说明是系统注入，这一家给不出注入原文 ⇒ 没有细条可摆、不画（同记录那一条的 `draw`）。
        assert_eq!(env.dr, crate::agents::record::Draw::None);
        assert!(env.ch > 0);
    }
}

/// Codex 一行 ⇒ 通用记录（`codex/record.rs`）经观测面读页 · 建骨架。
mod codex_record {
    use crate::agents::codex::record::record_of;
    use crate::agents::record::BodyExt as _;
    use serde_json::Value;

    /// 金样重写开关（人读过「现打」再落盘）。
    /// ★ 造的中性 rollout（`codex-rollout.sample.jsonl`，一整份会话：元记录 · 事件 · 人话 · 注入 · 推理 · 两种工具 · 回复 · 认不出的一类）
    /// 走核心读页那一遍（适配层 ＋ `body::rows_of`，与 `history-read` 同一条路）⇒ 通用记录金样 `record-codex.golden.jsonl`。
    /// 手机（`CodexRecordGoldenTest.kt`）与界面（`codex-record.vitest.ts`）解的是同一份金样，逐格对。
    #[test]
    fn codex_sample_rollout_reads_to_the_record_golden() {
        let root = guard_core::addr::repo_root();
        let sample =
            std::fs::read(root.join("tests/__fixtures__/codex-rollout.sample.jsonl")).unwrap();
        let face = crate::agents::codex::RECORDS;
        let tz = crate::common::time::Tz::named("UTC").unwrap();
        let mut reader = crate::observe::query::body::Reader::new(&face, tz);
        let rows = crate::observe::query::body::rows_of(&mut reader, 0, &sample);
        let mut got = String::new();
        for (i, row) in rows.iter().enumerate() {
            if let Some(record) = &row.record {
                let line = serde_json::json!({"case": format!("line-{i:02}"), "record": record});
                got.push_str(&line.to_string());
                got.push('\n');
            }
        }
        crate::golden::assert_golden(
            "tests/__fixtures__/record-codex.golden.jsonl",
            &got,
            "Codex 通用记录与金样不一致",
        );
    }

    /// ★ 骨架行的类 · 画法 · 人说的就是这一家整条翻译出的那条记录的，骨架索引照**这份记录那一家**的翻译取。
    /// 骨架照记录树那一家（Claude）的认法判 Codex 的行的话，每行都「不进界面」，
    /// 查看器打开有消息的 Codex 会话会写「无消息 · 0 条」（而 `history-page` 照样出得来记录）。
    #[test]
    fn the_skeleton_rows_of_a_codex_rollout_are_its_records() {
        let sample = include_str!("../../__fixtures__/codex-rollout.sample.jsonl");
        let mut shown = 0;
        let mut offset = 0u64;
        for raw in sample.lines() {
            let rec = record_of(&serde_json::from_str(raw).unwrap(), offset);
            let want = rec.as_ref().map(|r| r.body.class());
            let has_blocks = rec.as_ref().is_some_and(|r| {
                serde_json::to_value(&r.body).ok().and_then(|b| {
                    b.get("blocks")
                        .and_then(Value::as_array)
                        .map(|a| !a.is_empty())
                }) == Some(true)
            });
            let row = crate::observe::recognize::index_row::test_support::index_row(
                crate::agents::codex::AGENT_KIND,
                raw.as_bytes(),
                offset,
                raw.len() as u64 + 1,
            );
            assert_eq!(
                (row.t, row.dr, row.hu),
                rec.as_ref().map_or((None, Default::default(), false), |r| (
                    want, r.draw, r.human
                )),
                "骨架那一行：{raw}"
            );
            // 进界面的那几行骨架上要有料（正文字数 / 折叠单元），不然骨架判它「画不出东西」、查看器照样一张卡都不建。
            if want.is_some() && has_blocks {
                assert!(row.ch > 0 || row.fd > 0, "骨架那一行没有料：{raw}");
            }
            shown += usize::from(want.is_some());
            offset += raw.len() as u64 + 1;
        }
        assert!(shown >= 3, "样本里进界面的行塌了：{shown}");
    }
}
