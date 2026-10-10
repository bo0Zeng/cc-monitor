package com.ccmonitor.mobile.core.claude.model

/**
 * turn-end 探测：按 [AgentKind] 判定一条记录是不是一个 turn-end 边沿，是就返回 uuid（去重键）。
 * Claude 是谓词推断（[isCompletedAssistantTurn]）；Codex 是显式事件 `event_msg/task_complete`（uuid=turn_id）。
 * 消费方逐行喂，经 `AgentProfile.turnEndDetector` 取实现。
 */
interface TurnEndDetector {
    /** 该记录是否为 turn-end 边沿；是 → 返其 uuid（去重键），否 → null。 */
    fun turnEndUuid(record: JsonlRecord): String?
}

/** Claude：谓词推断，即 [isCompletedAssistantTurn]。 */
object ClaudeTurnEndDetector : TurnEndDetector {
    override fun turnEndUuid(record: JsonlRecord): String? = if (record.isCompletedAssistantTurn()) record.uuid else null
}

/**
 * Codex：读 `event_msg`，`payload.type` 为 `task_complete` 或别名 `turn_complete` → 返 `payload.turn_id`。
 * `turn_aborted`（中止轮）不算 turn-end，返 null，同 Claude 的非 end_turn；其它子型也是 null。
 * 缺 turn_id 时回退信封的 [JsonlRecord.timestamp]：完成轮必须给出非空键，否则逐行消费方会把它当「非 end」跳过，漏报最新一轮。
 */
object CodexTurnEndDetector : TurnEndDetector {
    private val TURN_END_TYPES = setOf("task_complete", "turn_complete")

    override fun turnEndUuid(record: JsonlRecord): String? {
        val payload = codexEventPayload(record) ?: return null
        if (payload["type"] !in TURN_END_TYPES) return null
        return payload["turn_id"] as? String ?: record.timestamp
    }
}
