#!/usr/bin/env bash
# cc-bus-agent-codex.sh —— ① agent 适配面的 **codex 词典**(照 `cc-bus-adapt.sh` 的契约)。
#
# Codex 有回合末钩子(`Stop`,接法见 `cc-bus-install.sh` 的 C1 段 / `examples/codex-hooks.json`),
# 钩子的回包与 claude 同形(`{decision:"block", reason}`)⇒ 能拦停。
# 敲门经后端 `session-say`(与 claude 同一条);有 cc-recv / cc-peek ⇒ 能读回。
# 降级链照 `cc-bus-adapt.sh`:拦停 → 敲门 → 只入收件箱。
# 没有 can_peer:Codex 没有原生的会话间通道,它与别的会话之间只走 cc-bus。
#
# 只提供函数、不自执行;由 `cc-bus-adapt.sh` 的 `ccbus_adapt_load` source 进来(`CCBUS_ADAPT_AGENT=codex`)。

agent_caps() { printf 'agent=codex can_block=yes can_nudge=yes can_readback=yes\n'; }

# 回合末钩子的回包:从 `<文件>` 读正文(走 `--rawfile`,不走 argv —— 大批消息会撞单个参数的上限)。
agent_block_reason() { jq -cn --rawfile r "$1" '{decision:"block", reason:$r}'; }

# Codex 不往它起的子进程导「我是哪个会话」⇒ 登记时说不出,投递时读窗格上的身份标记。
agent_self_sid() { printf ''; }
