package com.ccmonitor.mobile.core.claude.model

import org.junit.Assert.assertEquals
import org.junit.Test

/** parser 捕获 `stop_reason`。回合结束的判定本身在 [TurnEndDetectorTest]。 */
class TurnDetectorTest {
    @Test fun parserCapturesStopReason() {
        val r =
            JsonlParser.parse(
                """{"type":"assistant","uuid":"a1","message":{"role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"done"}]}}""",
            ) as JsonlRecord.Assistant
        assertEquals("end_turn", r.stopReason)
    }

    @Test fun parserStopReasonToolUse() {
        val r =
            JsonlParser.parse(
                """{"type":"assistant","uuid":"a1","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"text","text":"hi"}]}}""",
            ) as JsonlRecord.Assistant
        assertEquals("tool_use", r.stopReason)
    }
}
