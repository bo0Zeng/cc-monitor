#!/usr/bin/env bash
# cc-bus-agent-claude.sh —— ① agent 适配面的 **claude 词典**(设计)。
#
# ## 为什么是"词典"而不是"trait"
#
# 「只有 claude 那一份是已知的 ⇒ **先做词典,别立 trait**」(`D4`:
# 接口由现有能力反推)。⇒ 本文件不设计"通用 agent 接口",只**把 claude 这一份说清楚**;
# 第二个 agent(codex / …)进来时,共性才有资格被抽成 trait。
#
# ## 它比翻译官多一维:**能力**,不是"说法"
#
# claude 有 Stop 钩子、返回 `{decision:"block", reason}` ⇒ cc-bus 的"拦停"做得到。
# **别的 agent 可能根本没有这种钩子** —— 那不是"说法不同",是能力缺失,翻译官在这一格失效。
# ⇒ 每个词典必须自陈三位能力,上层按 `cc-bus-adapt.sh` 里那条降级链降级(不硬失败)。
#
# 只提供函数、不自执行;由 `cc-bus-adapt.sh` 的 `ccbus_adapt_load` source 进来。

# 三位能力。**这三个键名是判据的分母**(与 `CCBUS_BEHAVIOR_CAP` 那张表两向对拍)。
#   can_block    有结束钩子 ⇒ 能把"本轮结束"拦下来
#   can_nudge    能往它的输入打字 ⇒ 能敲门
#   can_readback 有"读消息"这个动作(cc-recv / cc-peek)⇒ 能让它自己来读
agent_caps() { printf 'agent=claude can_block=yes can_nudge=yes can_readback=yes\n'; }

# 结束钩子的形状:从 `<文件>` 读正文,吐 claude 认的那个 JSON。
# 🔴 **走 `--rawfile` 不走 argv**:40 条 ×4KB 累计 160KB 时 `--arg` 会撞 `MAX_ARG_STRLEN`
#   (128 KiB)⇒ 钩子 rc=126、stdout 空。那条实测事故是两阶段读口的立案理由。
agent_block_reason() { jq -cn --rawfile r "$1" '{decision:"block", reason:$r}'; }

# claude 的输入要**另打一个 Enter** 才算提交(先 send-keys 文本,再 send-keys Enter)。
# ⚠ 这一位属于 agent 而不是 OS:通道是 tmux(②),"打完还要回车"是 claude 这一侧的事。
# 返回 0 = 要另打 Enter。
agent_submit_enter() { return 0; }
