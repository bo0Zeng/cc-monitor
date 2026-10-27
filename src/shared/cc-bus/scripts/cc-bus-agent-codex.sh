#!/usr/bin/env bash
# cc-bus-agent-codex.sh —— agent 那一面的 **codex 词典**:结束钩子的回包怎么写。
#   Codex 有回合末钩子(`Stop`,接法见 `cc-bus-install.sh` 印的那一段),回包与 claude 同形(`{decision:"block", reason}`)。
#   身份不在这里:两家都问 cc-monitor 的 `--session-self`(`cc-bus whoami`)。
#   只提供函数、不自执行;由 `cc-bus-lib.sh` 的 `ccbus_agent_load` source 进来(`CCBUS_ADAPT_AGENT=codex`)。

# 回合末钩子的回包:从 `<文件>` 读正文(走 `--rawfile`,不走 argv —— 大批消息会撞单个参数的上限)。
agent_block_reason() { jq -cn --rawfile r "$1" '{decision:"block", reason:$r}'; }
