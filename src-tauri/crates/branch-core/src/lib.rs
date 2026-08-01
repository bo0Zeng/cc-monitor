//! **会话分叉的记录变换** —— monitor 与远端 daemon 共用的**唯一**一份实现。
//!
//! # 为什么要单独一个 crate（G1，branch-anywhere）
//!
//! 分叉这件事本地和远端都要做：本地由 monitor 直接算，远端由 daemon 在**远端本地**算
//! （几十 MB 的 jsonl 不该为了分叉拉过 ssh）。两边跑的必须是**同一段逻辑**。
//!
//! 备选过三条路，选这条的理由见 `.claude/planned-build/branch-anywhere/features/G1-*.md`：
//! 仓里已有三个「复制 + 漂移守卫」的双写点先例（`TMUX_LS_FMT` / `observation` 取值集 /
//! `RemovalCause` 字面量），但那三个都是**常量**；把一个 80 行的算法复制一份，
//! 守卫要么脆（改个变量名就假红），要么退化成整体字节比对。**共享 crate 让漂移在结构上不存在。**
//!
//! # 硬约束：不能把 daemon 拖进 monitor 的 workspace
//!
//! `remote-daemon-proto` 是**独立 crate、刻意不在 workspace 里** —— 否则 Windows CI 的
//! `cargo test --all` 会去构建这个 Linux-only 的 daemon 并炸掉。
//! 本 crate 位于 monitor 的 workspace 内（`cargo test --all` 会跑它的测试），
//! 而 daemon 只是**单向 path 依赖**过来 —— 依赖不会反向制造 workspace 成员关系。
//!
//! # 落盘格式的判据
//!
//! **与 CC 原生 `/branch` 逐字段一致**，不是与 `claude-agent-sdk` 的 `fork_session` 一致。
//! 详见 `build_branch_records` 上方那段实证记录与 `branch_matches_native_fork_shape`。

/// **纯函数**（可注入直测）：从解析好的记录里，取分叉点 `message_uuid` 沿 parentUuid 回溯
/// 到根的线性前缀，逐条改写成原生分支格式。返回按「根 → 分叉点」顺序的输出记录。
///
/// - `message_uuid` 不在记录集中 → Err（前端传了不存在的 uuid）。
/// - 环防御：parentUuid 指回已访问节点即停（append-only jsonl 理论无环，防御性）。
pub fn build_branch_records(
    lines: &[serde_json::Value],
    message_uuid: &str,
    src_sid: &str,
    new_sid: &str,
) -> Result<Vec<serde_json::Value>, String> {
    use std::collections::{HashMap, HashSet};
    let mut by_uuid: HashMap<&str, &serde_json::Value> = HashMap::new();
    for v in lines {
        if let Some(u) = v.get("uuid").and_then(|x| x.as_str()) {
            by_uuid.entry(u).or_insert(v); // 保首见（幂等，容 at-least-once 重复行）
        }
    }
    if !by_uuid.contains_key(message_uuid) {
        return Err(format!(
            "refuse branch: message_uuid {message_uuid:?} not found in source session"
        ));
    }
    // G0：**子 agent 记录不是可分叉的会话**。
    //
    // `create_branch_session` 是 Tauri 命令，前端传什么 uuid 它就用什么；F77 只是在
    // 「子 agent 查看器」里不挂那个按钮，**后端从来没拦过**。真让一条 sidechain 记录
    // 进来，产出的是一段 subagent 转录冒充会话。
    //
    // **判据是 truthy，不是「字段存在」**：真机原生 fork 的复制段上 244/244 条都**带着**
    // `isSidechain` 这个键，值是 `false`。按「存在即拒」会把正常会话全拒光。
    if by_uuid[message_uuid]
        .get("isSidechain")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Err(format!(
            "refuse branch: message_uuid {message_uuid:?} is a sidechain (subagent) record"
        ));
    }
    // 沿 parentUuid 回溯到根
    let mut chain: Vec<&str> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut cur = Some(message_uuid);
    while let Some(u) = cur {
        if !by_uuid.contains_key(u) || seen.contains(u) {
            break;
        }
        seen.insert(u);
        chain.push(u);
        cur = by_uuid[u]
            .get("parentUuid")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty());
    }
    chain.reverse(); // 根 → 分叉点

    let mut out = Vec::with_capacity(chain.len());
    for u in chain {
        let mut rec = by_uuid[u].clone();
        let obj = rec
            .as_object_mut()
            .ok_or("refuse branch: record is not a JSON object")?;
        obj.insert(
            "sessionId".into(),
            serde_json::Value::String(new_sid.to_string()),
        );
        obj.insert(
            "forkedFrom".into(),
            serde_json::json!({ "sessionId": src_sid, "messageUuid": u }),
        );
        out.push(rec);
    }
    // 原生 root 恒 parentUuid=null。回溯若因链断（parentUuid 指向集合外 / 坏行被跳）
    // 而止，新 root(out[0]) 会残留悬空 parentUuid → 置 null，产出干净自洽的根。
    if let Some(first) = out.first_mut() {
        if let Some(obj) = first.as_object_mut() {
            if obj.get("parentUuid").is_some_and(|p| !p.is_null()) {
                obj.insert("parentUuid".into(), serde_json::Value::Null);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::build_branch_records;

    /// G0：**按真机原生 fork 的形状**造的合成夹具（无任何真实对话内容）。
    ///
    /// 与 `sample_session` 的关键差别：**被 ESC 回退的旁支夹在两条主链记录中间**。
    /// `sample_session` 把废弃兄弟 `u6` 放在文件末尾，于是从 `u4` 分叉时
    /// 「线性切片 `[0..=3]`」与「祖先回溯」**给出同一个答案** —— 那条夹具
    /// **区分不了两种算法**，也就守不住本实现与 SDK 的核心分歧。
    ///
    /// 真机不是那样：原生 fork 的复制段跨 1964 行只取 1402 条，旁支就夹在中间。
    ///
    /// 从 `u4` 分叉时两种算法的答案：
    /// - 祖先回溯 → `[u1, u2, u3, u4]`（4 条）
    /// - 线性切片 `[0..=8]` → 9 条（多出 `mode`/`u6`/`u7`/`ai-title`/`file-history-snapshot`）
    fn native_shape_session() -> Vec<serde_json::Value> {
        vec![
            // 根：带 schema 外字段与「泄漏字段」slug（原生 fork 照样保留它）
            serde_json::json!({"type":"user","uuid":"u1","parentUuid":null,"timestamp":"t1",
                "sessionId":"SRC","gitBranch":"main","slug":"some-slug","isSidechain":false,
                "message":{"role":"user","content":"q1"}}),
            // 分叉点
            serde_json::json!({"type":"assistant","uuid":"u2","parentUuid":"u1","timestamp":"t2",
                "sessionId":"SRC","isSidechain":false,"message":{"role":"assistant","content":"a1"}}),
            // ↓ 无 uuid 的旁挂记录（原生 fork 一条都不带过去）
            serde_json::json!({"type":"mode","sessionId":"SRC","mode":"default"}),
            // ★ 被 ESC 回退的旁支，**夹在主链中间** —— 线性切片会把它卷进来
            serde_json::json!({"type":"user","uuid":"u6","parentUuid":"u2","timestamp":"t6",
                "sessionId":"SRC","message":{"role":"user","content":"abandoned"}}),
            serde_json::json!({"type":"assistant","uuid":"u7","parentUuid":"u6","timestamp":"t7",
                "sessionId":"SRC","message":{"role":"assistant","content":"abandoned-reply"}}),
            serde_json::json!({"type":"ai-title","sessionId":"SRC","title":"t"}),
            // 主链继续（parent 回到 u2 ⇒ u2 是分叉点）
            serde_json::json!({"type":"system","uuid":"u3","parentUuid":"u2","timestamp":"t3",
                "sessionId":"SRC","isSidechain":false}),
            serde_json::json!({"type":"file-history-snapshot","sessionId":"SRC","snapshot":{}}),
            // 分叉点：带另一个泄漏字段 + 一条**指向链外**的 logicalParentUuid
            serde_json::json!({"type":"user","uuid":"u4","parentUuid":"u3","timestamp":"t4",
                "sessionId":"SRC","sourceToolAssistantUUID":"tool-x",
                "logicalParentUuid":"not-in-this-file","message":{"role":"user","content":"q2"}}),
            serde_json::json!({"type":"assistant","uuid":"u5","parentUuid":"u4","timestamp":"t5",
                "sessionId":"SRC","message":{"role":"assistant","content":"a2"}}),
        ]
    }

    /// ★ G0 的核心：**我们的产出 == 原生 `/branch` 的形状**。
    ///
    /// 每条断言都对应一处「SDK 会做、而 CC 原生不做」的改动。这条测试存在的意义
    /// 就是拦住下一次「照 SDK 改回去」——详见 `build_branch_records` 上方那段注释。
    #[test]
    fn branch_matches_native_fork_shape() {
        let lines = native_shape_session();
        let out = build_branch_records(&lines, "u4", "SRC", "NEW").unwrap();

        // ① 祖先回溯，**不是**线性切片。
        //    线性切片会给出 9 条（多出 mode/u6/u7/ai-title/file-history-snapshot）。
        let uuids: Vec<&str> = out
            .iter()
            .map(|r| {
                r.get("uuid")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<no-uuid>")
            })
            .collect();
        assert_eq!(
            uuids,
            vec!["u1", "u2", "u3", "u4"],
            "只该取祖先链；混进旁支 = 退回线性切片，混进 <no-uuid> = 带上了旁挂记录"
        );

        // ② uuid **原样保留**（SDK 会 remap 成全新 uuid）。
        //    上面那条断言已隐含，这里再对源逐条核一次，把意图写明白。
        let src_uuids: Vec<&str> = lines
            .iter()
            .filter_map(|r| r.get("uuid").and_then(|v| v.as_str()))
            .collect();
        for u in &uuids {
            assert!(src_uuids.contains(u), "uuid {u} 不在源里 ⇒ 被 remap 了");
        }

        // ③ timestamp **一条都不改**（SDK 会把末条改成 now，理由是 resume 的叶子检测）。
        let want_ts = ["t1", "t2", "t3", "t4"];
        for (r, want) in out.iter().zip(want_ts) {
            assert_eq!(
                r.get("timestamp").unwrap().as_str().unwrap(),
                want,
                "timestamp 被改过；原生 fork 的复制段与源逐字相同"
            );
        }

        // ④ 「泄漏字段」**保留**（SDK 会清掉 slug / sourceToolAssistantUUID 等）。
        assert_eq!(out[0].get("slug").unwrap(), "some-slug", "slug 被清掉了");
        assert_eq!(
            out[3].get("sourceToolAssistantUUID").unwrap(),
            "tool-x",
            "sourceToolAssistantUUID 被清掉了"
        );

        // ⑤ `logicalParentUuid` 指向链外时：**原样带着，不 remap 也不报错**
        //    （原生 fork 自己就带着指向文件外的目标 ⇒ 官方不保证这条边）。
        assert_eq!(out[3].get("logicalParentUuid").unwrap(), "not-in-this-file");

        // ⑥ 根的 parentUuid 是 null；复制段只有那四类记录类型。
        assert!(out[0].get("parentUuid").unwrap().is_null());
        let types: Vec<&str> = out
            .iter()
            .map(|r| r.get("type").unwrap().as_str().unwrap())
            .collect();
        assert_eq!(types, vec!["user", "assistant", "system", "user"]);
    }

    /// ★ G4：**实时会话的 jsonl 还在增长** —— 分叉产出不受后续追加影响。
    ///
    /// 祖先回溯只依赖被选节点**及其祖先**，那些在选中那一刻都已经落盘了。
    /// 后续追加的记录（对话继续、甚至又长出新的 ESC 旁支）都在分叉点**之后**，
    /// 不可能成为它的祖先 ⇒ 产出逐字节相同。
    ///
    /// 这条不是理论推演：G4 把入口接到实时 tab 上之后，用户会在**正在写的文件**上点分叉。
    #[test]
    fn branch_output_is_stable_while_source_keeps_growing() {
        let base = native_shape_session();
        let before = build_branch_records(&base, "u4", "SRC", "NEW").unwrap();

        // 模拟：分叉之后对话继续，还顺手 ESC 回退出一条新旁支
        let mut grown = base.clone();
        grown.push(
            serde_json::json!({"type":"assistant","uuid":"n1","parentUuid":"u5",
            "timestamp":"t90","sessionId":"SRC"}),
        );
        grown.push(
            serde_json::json!({"type":"user","uuid":"n2","parentUuid":"u4",
            "timestamp":"t91","sessionId":"SRC"}),
        );
        grown.push(serde_json::json!({"type":"mode","sessionId":"SRC","mode":"plan"}));
        let after = build_branch_records(&grown, "u4", "SRC", "NEW").unwrap();

        assert_eq!(
            before, after,
            "源文件继续增长后，同一分叉点的产出应逐字段相同"
        );
    }

    /// G0：子 agent 记录不是可分叉的会话 —— 后端自己要拦，不能只靠前端不挂按钮。
    #[test]
    fn branch_rejects_sidechain_record() {
        let mut lines = native_shape_session();
        lines.push(
            serde_json::json!({"type":"assistant","uuid":"sc1","parentUuid":"u4",
            "timestamp":"t8","sessionId":"SRC","isSidechain":true,
            "message":{"role":"assistant","content":"subagent"}}),
        );
        let err = build_branch_records(&lines, "sc1", "SRC", "NEW").unwrap_err();
        assert!(err.contains("sidechain"), "got: {err}");

        // 反向自检：`isSidechain: false` 是**正常会话的常态**（真机 244/244 条都带着这个键），
        // 按「字段存在即拒」会把所有正常会话拒光。
        assert!(
            build_branch_records(&lines, "u4", "SRC", "NEW").is_ok(),
            "isSidechain:false 的记录必须照常可分叉"
        );
    }

    /// 一棵含「废弃 ESC 兄弟 + 分叉点后续」的小会话树：
    /// u1(user) → u2(asst) → u3(system) → u4(user 分叉点) → u5(asst 分叉后)
    ///                                  └→ u6(user 废弃 ESC 兄弟，parent 同 u3)
    fn sample_session() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({"type":"user","uuid":"u1","parentUuid":null,"timestamp":"t1","sessionId":"SRC","gitBranch":"main","message":{"role":"user","content":"q1"}}),
            serde_json::json!({"type":"assistant","uuid":"u2","parentUuid":"u1","timestamp":"t2","sessionId":"SRC","message":{"role":"assistant","content":"a1"}}),
            serde_json::json!({"type":"system","uuid":"u3","parentUuid":"u2","timestamp":"t3","sessionId":"SRC"}),
            serde_json::json!({"type":"user","uuid":"u4","parentUuid":"u3","timestamp":"t4","sessionId":"SRC","message":{"role":"user","content":"q2"}}),
            serde_json::json!({"type":"assistant","uuid":"u5","parentUuid":"u4","timestamp":"t5","sessionId":"SRC","message":{"role":"assistant","content":"a2"}}),
            serde_json::json!({"type":"user","uuid":"u6","parentUuid":"u3","timestamp":"t6","sessionId":"SRC","message":{"role":"user","content":"q2-alt"}}),
        ]
    }

    #[test]
    fn branch_copies_ancestor_prefix_in_native_format() {
        let lines = sample_session();
        // 从分叉点 u4 建分支
        let out = build_branch_records(&lines, "u4", "SRC", "NEWSID").unwrap();
        // 只保留祖先链 u1→u4（顺序、根→分叉点），排除分叉后 u5 与废弃兄弟 u6
        let uuids: Vec<&str> = out
            .iter()
            .map(|r| r.get("uuid").unwrap().as_str().unwrap())
            .collect();
        assert_eq!(uuids, vec!["u1", "u2", "u3", "u4"], "祖先链顺序不对或漏/多");
        for r in &out {
            let u = r.get("uuid").unwrap().as_str().unwrap();
            // sessionId 改新 id
            assert_eq!(r.get("sessionId").unwrap(), "NEWSID");
            // forkedFrom{sessionId:源, messageUuid:自身 uuid}
            let ff = r.get("forkedFrom").unwrap();
            assert_eq!(ff.get("sessionId").unwrap(), "SRC");
            assert_eq!(ff.get("messageUuid").unwrap().as_str().unwrap(), u);
            // parentUuid 原样保留（链完整）
        }
        // 逐字段忠实：schema 外字段 gitBranch 不丢
        assert_eq!(out[0].get("gitBranch").unwrap(), "main");
        // 分叉点 u4 的 parentUuid 仍指 u3
        assert_eq!(out[3].get("parentUuid").unwrap(), "u3");
    }

    #[test]
    fn branch_at_leaf_includes_whole_active_path() {
        let lines = sample_session();
        // 从活跃叶 u5 建分支 → 整条主干 u1..u5，废弃兄弟 u6 仍排除
        let out = build_branch_records(&lines, "u5", "SRC", "N2").unwrap();
        let uuids: Vec<&str> = out
            .iter()
            .map(|r| r.get("uuid").unwrap().as_str().unwrap())
            .collect();
        assert_eq!(uuids, vec!["u1", "u2", "u3", "u4", "u5"]);
    }

    #[test]
    fn branch_rejects_unknown_message_uuid() {
        let lines = sample_session();
        let err = build_branch_records(&lines, "does-not-exist", "SRC", "N3").unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn branch_at_root_yields_single_clean_root() {
        let out = build_branch_records(&sample_session(), "u1", "SRC", "N").unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].get("uuid").unwrap(), "u1");
        assert!(
            out[0].get("parentUuid").unwrap().is_null(),
            "root parentUuid 应为 null"
        );
        assert_eq!(
            out[0]
                .get("forkedFrom")
                .unwrap()
                .get("messageUuid")
                .unwrap(),
            "u1"
        );
    }

    #[test]
    fn branch_nulls_dangling_root_parent() {
        // 模拟链断：把 u3 的 parentUuid 改成集合外 ghost；从 u4 分叉 → 链 u4→u3→(止)，
        // 新 root=u3 的悬空 parentUuid 应被置 null（原生 root 恒 null）。
        let mut lines = sample_session();
        lines[2]
            .as_object_mut()
            .unwrap()
            .insert("parentUuid".into(), serde_json::json!("ghost-not-here"));
        let out = build_branch_records(&lines, "u4", "SRC", "N").unwrap();
        let uuids: Vec<&str> = out
            .iter()
            .map(|r| r.get("uuid").unwrap().as_str().unwrap())
            .collect();
        assert_eq!(uuids, vec!["u3", "u4"]);
        assert!(
            out[0].get("parentUuid").unwrap().is_null(),
            "链断的新 root parentUuid 应置 null"
        );
    }
}
