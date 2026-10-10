package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * [TurnEndDetector] per-kind turn-end 探测。Claude=谓词（复用 isCompletedAssistantTurn）、
 * Codex=event_msg/task_complete 事件（alias task_*↔turn_* 归一，turn_aborted 静默）。
 */
class TurnEndDetectorTest {
    private fun assistant(
        uuid: String?,
        stop: String?,
        apiErr: Boolean = false,
        sidechain: Boolean = false,
    ) = JsonlRecord.Assistant(uuid, null, emptyList(), null, null, null, apiErr, stop, sidechain)

    private fun event(
        payloadType: String,
        turnId: String? = "t1",
        ts: String? = null,
    ) =
        JsonlRecord.Unknown(
            "event_msg",
            timestamp = ts,
            rawJson = """{"type":"event_msg","payload":{"type":"$payloadType"${turnId?.let { ",\"turn_id\":\"$it\"" } ?: ""}}}""",
        )

    // ---- Claude：谓词 ----
    @Test fun claudeEndTurnAssistantIsTurnEnd() {
        assertEquals("u1", ClaudeTurnEndDetector.turnEndUuid(assistant("u1", "end_turn")))
    }

    @Test fun claudeNonEndTurnAndGuardsExcluded() {
        assertNull(ClaudeTurnEndDetector.turnEndUuid(assistant("u", "tool_use")))
        assertNull("apiError 排除", ClaudeTurnEndDetector.turnEndUuid(assistant("u", "end_turn", apiErr = true)))
        assertNull("sidechain 排除", ClaudeTurnEndDetector.turnEndUuid(assistant("u", "end_turn", sidechain = true)))
        assertNull("非 assistant 记录", ClaudeTurnEndDetector.turnEndUuid(JsonlRecord.Unknown("event_msg")))
    }

    // ---- Codex：event_msg/task_complete ----
    @Test fun codexTaskCompleteGivesTurnId() {
        assertEquals("t1", CodexTurnEndDetector.turnEndUuid(event("task_complete")))
    }

    @Test fun codexTurnCompleteAliasAlsoGivesTurnId() {
        // v1 别名 turn_complete = task_complete。
        assertEquals("t1", CodexTurnEndDetector.turnEndUuid(event("turn_complete")))
    }

    @Test fun codexTurnAbortedIsSilent() {
        // 中止轮 → null（静默、非 turn-end）。
        assertNull(CodexTurnEndDetector.turnEndUuid(event("turn_aborted")))
    }

    @Test fun codexTaskCompleteMissingTurnIdFallsBackToTimestamp() {
        // turn_id 缺 → 回退 envelope timestamp，保证完成轮仍产非空键：
        // 消费方（watcher 的逐行 rolling）把 null 当「非 turn-end」跳过，返 null 就等于漏报这一轮完成。
        assertEquals("2026-07-18T22:45:32Z", CodexTurnEndDetector.turnEndUuid(event("task_complete", turnId = null, ts = "2026-07-18T22:45:32Z")))
        // turn_id 与 timestamp 都缺 → 真无键 → null。
        assertNull(CodexTurnEndDetector.turnEndUuid(event("task_complete", turnId = null, ts = null)))
    }

    @Test fun codexOtherEventsAndNonEventNull() {
        assertNull(CodexTurnEndDetector.turnEndUuid(event("token_count")))
        assertNull(CodexTurnEndDetector.turnEndUuid(event("task_started")))
        assertNull("非 event_msg Unknown", CodexTurnEndDetector.turnEndUuid(JsonlRecord.Unknown("session_meta", rawJson = "{}")))
        assertNull("非 Unknown 记录", CodexTurnEndDetector.turnEndUuid(assistant("u", "end_turn")))
        assertNull("坏 rawJson 不抛", CodexTurnEndDetector.turnEndUuid(JsonlRecord.Unknown("event_msg", rawJson = "not json")))
        assertNull("event_msg 无 rawJson", CodexTurnEndDetector.turnEndUuid(JsonlRecord.Unknown("event_msg", rawJson = null)))
    }

    /** 按 kind 经 `AgentProfile.of(kind).turnEndDetector` 取到对应探测器。 */
    @Test fun selectorRoutesPerKind() {
        assertEquals(ClaudeTurnEndDetector, AgentProfile.of(AgentKind.ClaudeCode).turnEndDetector)
        assertEquals(CodexTurnEndDetector, AgentProfile.of(AgentKind.Codex).turnEndDetector)
    }
}
