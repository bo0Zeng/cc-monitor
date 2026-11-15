package com.ccmonitor.mobile.detekt

import dev.detekt.api.Config
import dev.detekt.test.utils.compileContentForTest
import org.jetbrains.kotlin.config.LanguageVersionSettingsImpl
import org.junit.Assert.assertEquals
import org.junit.Test

/** 机器怎么称呼只走 `machineNames`：自己拿 `label` 兜机器 id 的两形算一处，近形不算。 */
class HostNameRuleTest {
    private fun hits(code: String) = HostNameScan.scan(compileContentForTest(code)).size

    private val cases: List<Pair<String, Int>> =
        listOf(
            // 阳性：右边是机器 id。
            """suspend fun f(hosts: R, hostId: String) = hosts.get(hostId)?.label ?: hostId""" to 1,
            """fun f(h: H?, hostId: String) = h?.label ?: hostId""" to 1,
            """fun f(hosts: List<H>, s: S) = hosts.firstOrNull { it.id == s.hostId }?.label ?: s.hostId""" to 1,
            """val labelOf = { id: String -> hosts.get(id)?.label ?: id }""" to 1,
            """fun f(r: R) = r.observeAll().map { hs -> hs.map { HostName(it.id, it.label.ifBlank { it.id }) } }""" to 1,
            """fun f(h: H, hostId: String) = h.label.ifEmpty { hostId }""" to 1,
            """fun f(h: H?) = h?.let { Here(it.id, it.label.ifEmpty { it.id }) }""" to 1,
            """suspend fun f(hosts: R, hostId: String) = runCatching { hosts.get(hostId)?.label }.getOrNull()?.ifBlank { null } ?: hostId""" to 1,
            """suspend fun f(hosts: R, hostId: String): String { val label = hosts.get(hostId)?.label; return label ?: hostId }""" to 1,
            // 同一形、兜的不是机器（身份）也算：只看语法认不出是哪一类，那一处自己标 @Suppress 说明。
            """fun f(ids: List<I>) { ids.forEach { id -> Text(id.label.ifBlank { id.id }) } }""" to 1,
            // 阴性：按键 · 号牌 · 新建机器那一下用地址兜 · 会话 tab 自己的名字 · 键名兜修饰键 · 一句里有 host 字样但兜的不是 id。
            """fun f(k: K, ghost: G) = k.label ?: k.name""" to 0,
            // 按名字找 id（label 在条件里、不是兜的那一个）。
            """fun f(hosts: List<H>, q: String, id: String) = hosts.firstOrNull { it.label == q }?.id ?: id""" to 0,
            """fun f(label: String?, modifier: M?) = label ?: modifier?.let(::modName) ?: "" """ to 0,
            """fun f(account: String) = plate(account)?.label ?: account""" to 0,
            """fun f(label: String, host: String) = H(label = label.ifBlank { host })""" to 0,
            """fun f(s: S, label: String) = s.copy(label = s.label ?: label)""" to 0,
            """fun f(d: D) = d.label.ifBlank { d.modifier?.let(::modName).orEmpty() }""" to 0,
        )

    @Test
    fun `each sample hits exactly what it says`() {
        val bad = cases.mapNotNull { (code, want) -> hits(code).takeIf { it != want }?.let { "「$code」命中 $it 处，该是 $want" } }
        assertEquals(bad.joinToString("\n"), emptyList<String>(), bad)
    }

    @Test
    fun `the rule reports every hit`() {
        val file = compileContentForTest("fun f(h: H?, hostId: String) = h?.label ?: hostId\nfun g(h: H?, hostId: String) = h?.label ?: hostId\n")
        assertEquals(2, HostNameFallback(Config.empty).visitFile(file, LanguageVersionSettingsImpl.DEFAULT).size)
    }
}
