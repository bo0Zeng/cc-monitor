package com.ccmonitor.mobile.core.claude.model

/**
 * Claude 侧 turn-end 谓词：该记录是否为一条已结束的 assistant 轮次
 * （`stop_reason=="end_turn"`、非 API 错误、非 sidechain 子 agent）。
 *
 * 注意：后端的 turn-end 判定与这里逐字一致（判词、`stop_reason` 嵌在 `message` 下、缺字段安全默认），
 * 动了任何一个条件，两端说的就不是同一件事。
 *
 * 用 stop_reason 而不是「无 tool_use 块」：Claude Code 把一条 assistant 消息按内容块拆成多条 JSONL 记录，
 * 每条共享同一 stop_reason，纯 text 子记录也可能是 `tool_use`。只有 `end_turn` 才是真 turn-end。
 *
 * 排除 sidechain：子 agent 行可能写进主 jsonl，子 agent 完成不等于主轮结束。
 */
fun JsonlRecord.isCompletedAssistantTurn(): Boolean =
    this is JsonlRecord.Assistant && stopReason == "end_turn" && !isApiError && !isSidechain
