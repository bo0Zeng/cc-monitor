package com.ccmonitor.mobile.core.claude.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class JsonlParserTest {
    @Test fun userStringContent() {
        val r = JsonlParser.parse("""{"type":"user","uuid":"u1","parentUuid":"p0","message":{"role":"user","content":"hello"}}""")
        assertTrue(r is JsonlRecord.User)
        r as JsonlRecord.User
        assertEquals("u1", r.uuid)
        assertEquals("p0", r.parentUuid)
        assertEquals(1, r.blocks.size)
        assertEquals("hello", (r.blocks[0] as ContentBlock.Text).text)
    }

    // 逃生舱：未知 type（未来 schema）保留 uuid/parentUuid/timestamp + 原始行 → 参与 MainBranch 链、不孤儿化其后对话。
    @Test fun unknownTypeKeepsUuidParentRaw() {
        val line = """{"type":"future-thing","uuid":"x1","parentUuid":"p0","timestamp":"t1","foo":42}"""
        val r = JsonlParser.parse(line) as JsonlRecord.Unknown
        assertEquals("future-thing", r.type)
        assertEquals("x1", r.uuid)
        assertEquals("p0", r.parentUuid)
        assertEquals("t1", r.timestamp)
        assertEquals(line, r.rawJson)
    }

    // user 的 forkedFrom + toolUseResult.{filePath,structuredPatch}。
    @Test fun userForkedFromAndStructuredPatch() {
        val line =
            """{"type":"user","uuid":"u1","message":{"role":"user","content":"x"},""" +
                """"forkedFrom":{"sessionId":"s9","messageUuid":"m9"},""" +
                """"toolUseResult":{"filePath":"/a/b.kt","structuredPatch":[{"oldStart":10,"oldLines":2,"newStart":10,"newLines":3,"lines":["-a","+b","+c"]}]}}"""
        val r = JsonlParser.parse(line) as JsonlRecord.User
        assertEquals("s9", r.forkedFrom?.sessionId)
        assertEquals("m9", r.forkedFrom?.messageUuid)
        assertEquals("/a/b.kt", r.patchFilePath)
        assertEquals(1, r.structuredPatch?.size)
        val h = r.structuredPatch!![0]
        assertEquals(10, h.oldStart)
        assertEquals(3, h.newLines)
        assertEquals(listOf("-a", "+b", "+c"), h.lines)
    }

    // assistant 的 message.usage（*_tokens + cache_creation 5m/1h 拆分）+ top-level requestId（用量去重键）。
    @Test fun assistantUsageAndRequestId() {
        val line =
            """{"type":"assistant","uuid":"a1","requestId":"req_9","message":{"role":"assistant","content":[],""" +
                """"usage":{"input_tokens":5,"output_tokens":7,"cache_creation_input_tokens":100,"cache_read_input_tokens":200,""" +
                """"cache_creation":{"ephemeral_5m_input_tokens":30,"ephemeral_1h_input_tokens":70}}}}"""
        val r = JsonlParser.parse(line) as JsonlRecord.Assistant
        assertEquals(5L, r.usage?.input)
        assertEquals(7L, r.usage?.output)
        assertEquals(100L, r.usage?.cacheCreation)
        assertEquals(200L, r.usage?.cacheRead)
        assertEquals(30L, r.usage?.cacheCreation5m)
        assertEquals(70L, r.usage?.cacheCreation1h)
        assertEquals("req_9", r.requestId)
    }

    @Test fun assistantTextThinkingAndToolUse() {
        val line = """{"type":"assistant","uuid":"a1","message":{"role":"assistant","model":"claude-x","content":[
            {"type":"thinking","thinking":"hmm"},
            {"type":"text","text":"hi"},
            {"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls","timeout":5}}]}}"""
        val r = JsonlParser.parse(line) as JsonlRecord.Assistant
        assertEquals("claude-x", r.model)
        assertEquals(3, r.blocks.size)
        assertTrue(r.blocks[0] is ContentBlock.Thinking)
        assertTrue(r.blocks[1] is ContentBlock.Text)
        val tu = r.blocks[2] as ContentBlock.ToolUse
        assertEquals("Bash", tu.name)
        assertEquals("t1", tu.id)
        assertEquals("ls", tu.input["command"])
    }

    @Test fun toolResultStringAndArray() {
        val s =
            JsonlParser.parse(
                """{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"out","is_error":false}]}}""",
            ) as JsonlRecord.User
        val tr = s.blocks[0] as ContentBlock.ToolResult
        assertEquals("t1", tr.toolUseId)
        assertEquals("out", tr.text)
        assertFalse(tr.isError)

        val arr =
            JsonlParser.parse(
                """{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":[{"type":"text","text":"a"},{"type":"text","text":"b"}],"is_error":true}]}}""",
            ) as JsonlRecord.User
        val tr2 = arr.blocks[0] as ContentBlock.ToolResult
        assertEquals("a\nb", tr2.text)
        assertTrue(tr2.isError)
    }

    @Test fun titleAndAttachment() {
        assertEquals("My Session", (JsonlParser.parse("""{"type":"ai-title","aiTitle":"My Session","sessionId":"s1"}""") as JsonlRecord.Title).title)
        assertTrue(JsonlParser.parse("""{"type":"attachment","uuid":"x","message":{}}""") is JsonlRecord.Attachment)
    }

    @Test fun unknownTypesPreserveName() {
        assertEquals("permission-mode", (JsonlParser.parse("""{"type":"permission-mode","permissionMode":"x"}""") as JsonlRecord.Unknown).type)
        assertEquals("file-history-snapshot", (JsonlParser.parse("""{"type":"file-history-snapshot","snapshot":{}}""") as JsonlRecord.Unknown).type)
    }

    @Test fun garbageNeverThrows() {
        assertTrue(JsonlParser.parse("not json") is JsonlRecord.Unknown)
        assertTrue(JsonlParser.parse("") is JsonlRecord.Unknown)
        assertTrue(JsonlParser.parse("{}") is JsonlRecord.Unknown)
        assertTrue(JsonlParser.parse("""{"type":123}""") is JsonlRecord.Unknown) // type 非字符串
        assertTrue(JsonlParser.parse("""{"type":"user","message":"oops"}""") is JsonlRecord.User) // message 非对象 → 空 blocks
    }

    /** 真实样本：解析真机抓取的每一行都不得抛异常，且能识别出 user+assistant。 */
    @Test fun realSampleParsesWithoutThrowing() {
        val stream = javaClass.classLoader?.getResourceAsStream("sample.jsonl")
        org.junit.Assume.assumeTrue("无本地 sample.jsonl，跳过", stream != null)
        val lines = stream!!.bufferedReader().readLines().filter { it.isNotBlank() }
        org.junit.Assume.assumeTrue(lines.isNotEmpty())
        var users = 0
        var assistants = 0
        for (l in lines) {
            when (JsonlParser.parse(l)) { // 不得抛
                is JsonlRecord.User -> users++
                is JsonlRecord.Assistant -> assistants++
                else -> {}
            }
        }
        assertTrue("真实样本应含 user+assistant", users > 0 && assistants > 0)
    }
}
