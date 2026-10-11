package com.ccmonitor.mobile.agent

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.model.AgentKind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 整张档案表钉住：改任何一格都要重新过一遍眼。
 *
 * 它不是「消费方真走档案」的证据：本条对每一刀变异都会红，因为它就是直接读档案。
 * 「消费方真的读了那一格」得由消费方侧的判据来证（`AgentArchetypeTest` · `ChatAccountGateTest` 等）。
 *
 * 本条的职责只有一个：改表要留痕。
 */
class AgentProfileTableTest {
    /** 一份档案压成一行（每一格都进串：少写一格，那一格的变异就溜过去了）。 */
    private fun render(p: AgentProfile): String =
        listOf(
            p.kind.name,
            p.displayName,
            p.wireName,
            "accountGate=${p.requiresAccountGate}",
            p.invocation::class.simpleName,
        ).joinToString(" | ")

    @Test
    fun theWholeProfileTableIsPinnedSoAnyChangeGetsReVetted() {
        assertEquals(
            "档案改了。这不是错，是提醒：改的那一格有没有对应的消费方侧判据？",
            PINNED_TABLE,
            AgentProfile.ALL.map(::render),
        )
    }

    /**
     * `of(kind)` 是唯一取法，且每种 agent 都有且只有一份档案。
     *
     * 加一种 `AgentKind` 却忘了配档案 ⇒ `of` 那个 `when` 编译不过（编译期就拦住）；
     * 这里再钉住运行期的两条：每种都取得到、取到的是同一个实例（档案是单例，不是每次现造）。
     */
    @Test
    fun everyAgentKindHasExactlyOneProfileAndOnlyOneWayToGetIt() {
        assertEquals("一种 agent 一份档案", AgentKind.entries.size, AgentProfile.ALL.size)
        assertEquals("ALL 与 of() 必须是同一批", AgentKind.entries.map(AgentProfile::of), AgentProfile.ALL)
        for (k in AgentKind.entries) {
            assertEquals("of($k) 取到的必须是 $k 那一份", k, AgentProfile.of(k).kind)
            assertSame("档案是单例（每次现造会让 `===` 比较和缓存全失效）", AgentProfile.of(k), AgentProfile.of(k))
        }
        val names: Set<String> = AgentProfile.ALL.mapTo(mutableSetOf()) { it.displayName }
        val wires: Set<String> = AgentProfile.ALL.mapTo(mutableSetOf()) { it.wireName }
        assertEquals("显示名必须互不相同：$names", AgentProfile.ALL.size, names.size)
        assertEquals("wire 串必须互不相同：$wires", AgentProfile.ALL.size, wires.size)
        assertTrue("名字不能是空的", AgentProfile.ALL.all { it.displayName.isNotBlank() && it.wireName.isNotBlank() })
    }

    /**
     * 缺省档 = 保守的一侧（Claude 档）。
     *
     * 「读不出种类就用它」的那几处落点（`SessionBackend` · `SessionViewModel` · `ChatRoute` · `HostScreens` 等）全指着它：
     * 把它改成 Codex 档，等于让所有读不出种类的服务器都不再问账号。
     */
    @Test
    fun theDefaultProfileIsTheConservativeOne() {
        assertSame("缺省档必须是表里的一份，不是另造的", AgentProfile.of(AgentProfile.DEFAULT.kind), AgentProfile.DEFAULT)
        assertTrue("缺省档必须走账号门（保守的一侧：宁可多问一次）", AgentProfile.DEFAULT.requiresAccountGate)
    }

    /**
     * wire 串 → 档案：`"codex"` 是 Codex，其余都是 Claude。
     *
     * 表在这边：改 `wireName` 那一格时，这边先红。
     */
    @Test
    fun theWireNameMappingIsVerbatimWhatItWasBefore() {
        assertEquals(AgentKind.Codex, AgentProfile.fromWire("codex").kind)
        assertEquals(AgentKind.ClaudeCode, AgentProfile.fromWire("claude").kind)
        assertEquals("缺 agent_kind → 缺省档", AgentKind.ClaudeCode, AgentProfile.fromWire(null).kind)
        assertEquals("未知 wire kind → 缺省档（前向兼容）", AgentKind.ClaudeCode, AgentProfile.fromWire("gemini").kind)
        // wire 串是小写，与 DB 存的 `enum.name` 不是一回事：混用会让 "Codex" 走成缺省档
        assertEquals("wire 面不认 enum.name 的大小写", AgentKind.ClaudeCode, AgentProfile.fromWire("Codex").kind)
    }

    /**
     * Room 存量值 → 档案：两支口径不一样。
     *
     * `ofStoredName`（精确）给 `Host.agentKindOrDefault`；`ofStoredNameIgnoringCase`（忽略大小写）
     * 给服务器编辑页回显。合并成一支会改掉「存了小写 `codex` 时编辑页显示哪一档」这个真实行为，
     * 所以本条把差异本身钉住，谁要统一得先来改这里。
     */
    @Test
    fun theTwoStoredNameLookupsKeepTheirPreExistingDisagreement() {
        assertEquals(AgentKind.Codex, AgentProfile.ofStoredName("Codex").kind)
        assertEquals(AgentKind.ClaudeCode, AgentProfile.ofStoredName("ClaudeCode").kind)
        assertEquals(AgentKind.ClaudeCode, AgentProfile.ofStoredName(null).kind)
        assertEquals(AgentKind.ClaudeCode, AgentProfile.ofStoredName("Gemini").kind)
        // 差异就在这一行：精确那支把小写当未知值（→ 缺省档），忽略大小写那支认得出
        assertEquals("精确匹配：小写 `codex` 是未知值 → 缺省档", AgentKind.ClaudeCode, AgentProfile.ofStoredName("codex").kind)
        assertEquals("忽略大小写：小写 `codex` 认得出", AgentKind.Codex, AgentProfile.ofStoredNameIgnoringCase("codex").kind)
        assertEquals(AgentKind.Codex, AgentProfile.ofStoredNameIgnoringCase("Codex").kind)
        assertEquals(AgentKind.ClaudeCode, AgentProfile.ofStoredNameIgnoringCase(null).kind)
        assertEquals(AgentKind.ClaudeCode, AgentProfile.ofStoredNameIgnoringCase("Gemini").kind)
    }

    companion object {
        /**
         * 两份档案，每一格。改这张表 = 声明「我知道我在改行为」。
         *
         * 每一格的产品理由写在 `AgentProfile.kt` 里那一格自己的 KDoc 上，别在这里复述。
         */
        private val PINNED_TABLE =
            listOf(
                "ClaudeCode | Claude | claude | accountGate=true | ClaudeInvocation",
                "Codex | Codex | codex | accountGate=false | CodexInvocation",
            )
    }
}
