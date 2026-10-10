package com.ccmonitor.mobile.core.claude.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * [CodexRecordParser] 解析 Codex rollout（信封 {timestamp,type,payload} + Responses-API）→ 统一 JsonlRecord。
 * fixtures 按 codex-cli 0.144.6 的真实结构构造（不拷真会话，避 PII）。覆盖 response_item 全子型映射 + 事件/元→Unknown(保 rawJson)
 * + maximally-defensive（坏 JSON/未知型/缺字段/空行永不抛）。
 */
class CodexRecordParserTest {
    private fun parse(line: String) = CodexRecordParser.parse(line)

    @Test fun messageUserMapsToUserWithText() {
        val r = parse("""{"timestamp":"2026-07-18T07:34:25Z","type":"response_item","payload":{"type":"message","role":"user","id":"m1","content":[{"type":"input_text","text":"hello"}]}}""")
        assertTrue(r is JsonlRecord.User)
        r as JsonlRecord.User
        assertEquals("m1", r.uuid)
        assertEquals(listOf("hello"), r.blocks.filterIsInstance<ContentBlock.Text>().map { it.text })
        assertEquals(false, r.isMeta)
        assertEquals("2026-07-18T07:34:25Z", r.timestamp)
    }

    @Test fun messageAssistantMapsToAssistantWithText() {
        val r = parse("""{"timestamp":"t","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hi there"}]}}""")
        assertTrue(r is JsonlRecord.Assistant)
        assertEquals(listOf("hi there"), (r as JsonlRecord.Assistant).blocks.filterIsInstance<ContentBlock.Text>().map { it.text })
    }

    @Test fun contentDropsNoTextItemsNoBlankLines() {
        // 丢无 text 项（与后端同）：input_image 等无 text 项不留空行、不夹多余 \n。
        val r = parse("""{"timestamp":"t","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"前"},{"type":"input_image","image_url":"x"},{"type":"input_text","text":"后"}]}}""")
        assertEquals(listOf("前\n后"), (r as JsonlRecord.User).blocks.filterIsInstance<ContentBlock.Text>().map { it.text })
    }

    @Test fun sessionMetaCwdExtracted() {
        // Codex cwd 在 session_meta.payload.cwd（≠ Claude 在 User 记录）→ codexSessionMetaCwd 提取（供 resume）。
        val meta = parse("""{"timestamp":"t","type":"session_meta","payload":{"id":"019f78e8-84dd-7ac0-b479-e9c1b9caec67","cwd":"/home/u/proj","cli_version":"0.144.6"}}""")
        assertEquals("/home/u/proj", codexSessionMetaCwd(meta))
        // 非 session_meta / 无 cwd → null。
        assertEquals(null, codexSessionMetaCwd(parse("""{"timestamp":"t","type":"event_msg","payload":{"type":"token_count"}}""")))
        assertEquals(null, codexSessionMetaCwd(JsonlRecord.Unknown("session_meta", rawJson = """{"payload":{}}""")))
    }

    @Test fun injectedUserContextMarkedMeta() {
        // role=user 但正文是注入上下文（environment/plugins/AGENTS.md）→ isMeta=true 隐藏
        // （否则每个 Codex 会话开场渲染一个大噪音气泡）。真机上的注入都以三种特征前缀之一起头。
        // 正文无引号/反斜杠/换行 → 可直接内插进 JSON（denoise 只看 startsWith，无需真换行）。
        fun userMsg(text: String) =
            parse("""{"timestamp":"t","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"$text"}]}}""") as JsonlRecord.User
        assertEquals("environment_context 注入 → meta", true, userMsg("<environment_context> cwd=/x </environment_context>").isMeta)
        assertEquals("recommended_plugins 注入 → meta", true, userMsg("<recommended_plugins> - foo </recommended_plugins>").isMeta)
        assertEquals("AGENTS.md 注入头 → meta", true, userMsg("# AGENTS.md instructions <INSTRUCTIONS> # AGENTS.md 本文").isMeta)
        assertEquals("真用户输入 → 非 meta（照渲）", false, userMsg("帮我看看这个 bug").isMeta)
        assertEquals("提到 environment 但非注入起头 → 非 meta（防误伤）", false, userMsg("我的 environment_context 该怎么写").isMeta)
    }

    @Test fun messageDeveloperMapsToMetaUser() {
        // developer = 系统指令/元 → User(isMeta=true)（保文本、渲染当 meta 隐藏）。
        val r = parse("""{"timestamp":"t","type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"be concise"}]}}""")
        assertTrue(r is JsonlRecord.User)
        assertEquals(true, (r as JsonlRecord.User).isMeta)
        assertEquals(listOf("be concise"), r.blocks.filterIsInstance<ContentBlock.Text>().map { it.text })
    }

    @Test fun multiPartContentJoined() {
        val r = parse("""{"timestamp":"t","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"a"},{"type":"input_text","text":"b"}]}}""")
        assertEquals(
            "a\nb",
            (r as JsonlRecord.User)
                .blocks
                .filterIsInstance<ContentBlock.Text>()
                .single()
                .text,
        )
    }

    @Test fun reasoningMapsToAssistantThinking() {
        val r = parse("""{"timestamp":"t","type":"response_item","payload":{"type":"reasoning","summary":[{"type":"summary_text","text":"thinking hard"}],"encrypted_content":"xxx"}}""")
        assertTrue(r is JsonlRecord.Assistant)
        assertEquals(listOf("thinking hard"), (r as JsonlRecord.Assistant).blocks.filterIsInstance<ContentBlock.Thinking>().map { it.thinking })
    }

    @Test fun reasoningEncryptedOnlyGivesEmptyBlocks() {
        // 真机 summary 恒 []（仅 encrypted_content）→ 空 blocks（免 11 个空 thinking 渲染噪音）。
        val r = parse("""{"timestamp":"t","type":"response_item","payload":{"type":"reasoning","encrypted_content":"xxx"}}""")
        assertTrue((r as JsonlRecord.Assistant).blocks.isEmpty())
    }

    @Test fun customToolCallMapsToToolUse() {
        val r = parse("""{"timestamp":"t","type":"response_item","payload":{"type":"custom_tool_call","call_id":"c1","name":"shell","input":{"cmd":"ls"},"status":"completed"}}""")
        val tu = (r as JsonlRecord.Assistant).blocks.filterIsInstance<ContentBlock.ToolUse>().single()
        assertEquals("c1", tu.id)
        assertEquals("shell", tu.name)
        assertEquals("ls", tu.input["cmd"])
    }

    @Test fun functionCallAliasAlsoMapsToToolUse() {
        // 源码另有 function_call/_output——容忍同映。
        val r = parse("""{"timestamp":"t","type":"response_item","payload":{"type":"function_call","call_id":"c2","name":"grep","input":"pattern"}}""")
        val tu = (r as JsonlRecord.Assistant).blocks.filterIsInstance<ContentBlock.ToolUse>().single()
        assertEquals("c2", tu.id)
        assertEquals("pattern", tu.input["input"]) // String input → 包 {input}
    }

    @Test fun customToolCallOutputMapsToToolResult() {
        // 真机 output 恒为数组 [{type,text}]（非 String）；按 String 读会落空串、工具返回全丢。
        val r = parse("""{"timestamp":"t","type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"c1","output":[{"type":"input_text","text":"tool done"}]}}""")
        val tr = (r as JsonlRecord.User).blocks.filterIsInstance<ContentBlock.ToolResult>().single()
        assertEquals("c1", tr.toolUseId)
        assertEquals("tool done", tr.text)
    }

    @Test fun toolOutputMultiItemArrayJoined() {
        val r = parse("""{"timestamp":"t","type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"c1","output":[{"type":"output_text","text":"a"},{"type":"output_text","text":"b"}]}}""")
        assertEquals(
            "a\nb",
            (r as JsonlRecord.User)
                .blocks
                .filterIsInstance<ContentBlock.ToolResult>()
                .single()
                .text,
        )
    }

    @Test fun toolOutputStringStillSupported() {
        // 防御：String output 仍兜（源码可能变）。
        val r = parse("""{"timestamp":"t","type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"c1","output":"legacy"}}""")
        assertEquals(
            "legacy",
            (r as JsonlRecord.User)
                .blocks
                .filterIsInstance<ContentBlock.ToolResult>()
                .single()
                .text,
        )
    }

    @Test fun sessionMetaAndEventsGoToUnknownPreservingRaw() {
        val meta = """{"timestamp":"t","type":"session_meta","payload":{"session_id":"s1","cwd":"/p"}}"""
        val ev = """{"timestamp":"t","type":"event_msg","payload":{"type":"task_complete","turn_id":"tid","last_agent_message":"done"}}"""
        for (line in listOf(meta, ev, """{"timestamp":"t","type":"turn_context","payload":{"model":"gpt-5"}}""", """{"timestamp":"t","type":"world_state","payload":{}}""")) {
            val r = parse(line)
            assertTrue("$line → Unknown", r is JsonlRecord.Unknown)
            assertEquals("rawJson 保留供 2B 读", line, (r as JsonlRecord.Unknown).rawJson)
        }
        // event_msg 顶层 type 入 Unknown.type（回合结束探测再从 rawJson 读 payload.type、归一别名）
        assertEquals("event_msg", (parse(ev) as JsonlRecord.Unknown).type)
    }

    @Test fun defensiveNeverThrows() {
        // 坏 JSON / 未知顶层型 / 缺 payload / 空行 → Unknown，永不抛。
        assertTrue(parse("not json") is JsonlRecord.Unknown)
        assertTrue(parse("") is JsonlRecord.Unknown)
        assertTrue(parse("""{"type":"future_type","payload":{}}""") is JsonlRecord.Unknown)
        assertTrue(parse("""{"type":"response_item"}""") is JsonlRecord.Unknown) // 缺 payload
        assertTrue(parse("""{"type":"response_item","payload":{"type":"unknown_item"}}""").let { it is JsonlRecord.Unknown && it.type == "response_item:unknown_item" })
        // 缺字段：message 无 content → 空 blocks，不崩。
        val noContent = parse("""{"type":"response_item","payload":{"type":"message","role":"user"}}""")
        assertTrue(noContent is JsonlRecord.User)
        assertTrue((noContent as JsonlRecord.User).blocks.isEmpty())
    }

    @Test fun unparseableTopTypeNullGivesNoType() {
        assertEquals("(no-type)", (parse("""{"timestamp":"t","payload":{}}""") as JsonlRecord.Unknown).type)
    }
}
