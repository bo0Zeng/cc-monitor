package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.PrefsState
import com.ccmonitor.mobile.core.claude.link.wire.TabPref
import com.ccmonitor.mobile.core.claude.link.wire.UsageTone
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 设置屏那几条：偏好照类型解（读不懂也照答那一份）、改只交点到的那一格；用量那一格出过数的与没出过数的合一张。 */
class MachinePrefsTest {
    private fun prefs(
        state: String = "ok",
        child: String = "hide",
    ) = mapOf(
        "state" to state,
        "reason" to if (state == "unreadable") "第 4 行格式不对" else null,
        "detail" to null,
        "path" to "/h/.cc-monitor/prefs.json",
        "prefs" to
            mapOf(
                "launchers" to emptyMap<String, Any>(),
                "resumeInTmux" to true,
                "models" to mapOf("team" to "opus"),
                "contextLimits" to emptyMap<String, Any>(),
                "agentHomes" to emptyMap<String, Any>(),
                "pbDir" to null,
                "childTab" to child,
                "bgTab" to "show",
            ),
    )

    @Test
    fun `prefs-read：两种会话的那一格 · 每号模型照那台；读不懂照答那一份、带那一句；缺格 ⇒ 解不出`() {
        val r = MachinePrefs.of(prefs())!!
        assertEquals(PrefsState.OK, r.state)
        assertEquals(TabPref.HIDE to TabPref.SHOW, r.prefs.childTab to r.prefs.bgTab)
        assertEquals(mapOf("team" to "opus"), r.prefs.models)
        val bad = MachinePrefs.of(prefs(state = "unreadable", child = "show"))!!
        assertEquals(PrefsState.UNREADABLE to "第 4 行格式不对", bad.state to bad.reason)
        assertEquals(TabPref.SHOW, bad.prefs.childTab)
        assertNull("认不出的值 ⇒ 契约对不上", MachinePrefs.of(prefs(child = "maybe")))
    }

    @Test
    fun `prefs-set：只交点到的那一格；模型 null ⇒ 删掉那一条`() {
        assertEquals(mapOf("childTab" to "show"), MachinePrefs.tab(TabKind.CHILD, TabPref.SHOW))
        assertEquals(mapOf("bgTab" to "hide"), MachinePrefs.tab(TabKind.BG, TabPref.HIDE))
        assertEquals(mapOf("models" to mapOf("team" to "opus")), MachinePrefs.model("team", "opus"))
        assertEquals(mapOf("models" to mapOf("team" to null)), MachinePrefs.model("team", null))
    }

    @Test
    fun `quota-read：每号用量那一格照那台写的字与语气，没出过数的号也在`() {
        val usage = { text: String, tone: String -> mapOf("value" to text, "text" to text, "tone" to tone) }
        val data =
            mapOf(
                "accounts" to listOf(mapOf("account" to "work", "usage" to usage("5h 63%", "plain")), mapOf("account" to "personal", "usage" to usage("5h ✕ ↻19:00", "fail"))),
                "unseen" to listOf(mapOf("account" to "lab", "usage" to usage("5h —", "plain"))),
            )
        val u = QuotaRead.usage(data)!!
        assertEquals(mapOf("work" to "5h 63%", "personal" to "5h ✕ ↻19:00", "lab" to "5h —"), u.mapValues { it.value.text })
        assertEquals(UsageTone.FAIL, u["personal"]?.tone)
        assertNull("缺 unseen ⇒ 解不出", QuotaRead.usage(data - "unseen"))
    }
}
