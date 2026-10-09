//! 命令表 · 计划读面：`plan-*`（planned-build 的工作区 · 片 · 一格的 agent 视角；只读）。

use crate::stream::inbound::spec::{arg, out, CommandSpec, Run};

pub(super) const SPECS: &[CommandSpec] = &[
    // 三条都在阻塞档：起一次 pb（研究盘 252 格 1.1 秒），开跑之后打不断 ⇒ `cancel` 命中时回 `not_cancellable`。
    CommandSpec {
        name: "plan-list",
        summary: "这台的 pb 工作区与片（目录 ＝ 活会话的工作目录 ∪ `dirs`，pb 自己往上找工作区）",
        codes: &["bad_args"],
        fields: &[arg("dirs", "可选：另要问的目录（串的数组）"), arg("fresh", "可选布尔：`true` ⇒ 认过的目录也重问"), out("pb", "`{state: ok|missing|unsupported, said, version}`：pb 装没装、认不认得它的输出"), out("workspaces", "每个工作区一格 `{workspace, repo, auto, rev, stale, slices: [{name, domain, current, progress, error, stale}]}`")],
        takes_input: true,
        run: Run::BlockingData(|r| crate::faces::plan_face::answer(&r.cmd, &r.args).map(Some)),
    },
    CommandSpec {
        name: "plan-read",
        summary: "一个工作区的成品（几片的图 · 状态 · 签收 · 块 · 判据；接手与签收人对到会话；不带 agent_view）",
        codes: &["bad_args", "failed", "no_pb", "not_workspace", "pb_unsupported"],
        fields: &[arg("workspace", "工作区根（`plan-list` 给的那个）"), out("rev", "这一份输出的摘要：变了才算计划变了"), out("readAt", "读到的时刻（epoch ms）"), out("slices", "每片一格：读不成 ⇒ `error`；这一刻读不成但读好过 ⇒ 上一次那一份 ＋ `stale {said, since}`"), out("stale", "整次读不成、给的是上一次那一份 ⇒ `{said, raw, since}`；否则 `null`")],
        takes_input: true,
        run: Run::BlockingData(|r| crate::faces::plan_face::answer(&r.cmd, &r.args).map(Some)),
    },
    CommandSpec {
        name: "plan-cell-view",
        summary: "一格的 agent 视角（agent 站在这一格时 pb 印给它的那一段，原样）",
        codes: &["bad_args", "no_view"],
        fields: &[arg("id", "格的编号"), arg("slice", "片名"), out("view", "原样那一段"), arg("workspace", "工作区根")],
        takes_input: true,
        run: Run::BlockingData(|r| crate::faces::plan_face::answer(&r.cmd, &r.args).map(Some)),
    },
];
