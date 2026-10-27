#!/usr/bin/env bash
# cc-bus-agent-claude.sh —— agent 那一面的 **claude 词典**:结束钩子的回包怎么写。
#   身份不在这里:两家都问 cc-monitor 的 `--session-self`(`cc-bus whoami`)。
#   只提供函数、不自执行;由 `cc-bus-lib.sh` 的 `ccbus_agent_load` source 进来(`CCBUS_ADAPT_AGENT` 缺省 claude)。
#   同一台上 claude ↔ claude 也可以走 Claude 自带的 ListAgents / SendMessage;cc-bus 照投,不替它改道。

# 结束钩子的回包:从 `<文件>` 读正文,吐 claude 认的那个 JSON。
# 🔴 **走 `--rawfile` 不走 argv**:40 条 ×4KB 累计 160KB 时 `--arg` 会撞 `MAX_ARG_STRLEN`(128 KiB)⇒ 钩子 rc=126、stdout 空。
agent_block_reason() { jq -cn --rawfile r "$1" '{decision:"block", reason:$r}'; }
