package com.ccmonitor.mobile.ui.settings

import com.ccmonitor.mobile.core.claude.link.Changed
import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.TabKind
import com.ccmonitor.mobile.core.claude.link.wire.TabPref
import com.ccmonitor.mobile.core.claude.link.wire.UsageTone
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * 设置屏读写那台的那几份：进来各读一次；流上哪一样变了重读哪一样；换了一条流整份重读。
 * 偏好拨了马上存、照回包画；没存上退回读到的值、那一行下面一句（桌面同一个键）。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class MachineSettingsTest {
    private fun acct(
        name: String,
        selectable: Boolean = true,
    ) = mapOf(
        "name" to name,
        "label" to name,
        "configDir" to "/a/$name",
        "mode" to "isolated",
        "selectable" to selectable,
        "color" to 0.0,
        "plate" to name.take(2),
        "quota" to null,
        "unselectable" to if (selectable) null else "需登录",
        "authKind" to "subscription",
        "badge" to mapOf("text" to if (selectable) "已登录" else "需登录", "warn" to !selectable),
    )

    private fun prefs(
        child: String = "hide",
        models: Map<String, String> = emptyMap(),
    ) = mapOf(
        "state" to "ok",
        "reason" to null,
        "detail" to null,
        "path" to null,
        "prefs" to mapOf("models" to models, "childTab" to child, "bgTab" to "show"),
    )

    private val quota =
        mapOf(
            "accounts" to listOf(mapOf("account" to "work", "usage" to mapOf("text" to "5h 63%", "tone" to "plain"))),
            "unseen" to listOf(mapOf("account" to "lab", "usage" to mapOf("text" to "5h —", "tone" to "plain"))),
        )

    private fun link(set: (Map<String, Any?>?) -> Reply = { Reply.Ok(prefs(child = "show")) }) =
        FakeCoreLink().also { l ->
            l.answer = { cmd, args ->
                when (cmd) {
                    "accounts-list" -> Reply.Ok(mapOf("accounts" to listOf(acct("work"), acct("lab", selectable = false)), "meta" to mapOf("effectiveDefault" to "work", "home" to "/h")))
                    "quota-read" -> Reply.Ok(quota)
                    "prefs-read" -> Reply.Ok(prefs())
                    "prefs-set" -> set(args)
                    "accounts-set-default" -> Reply.Ok(null)
                    else -> Reply.LinkDown
                }
            }
        }

    private fun TestScope.open(l: FakeCoreLink): Pair<MachineSettings, MutableSharedFlow<Changed>> {
        val changes = MutableSharedFlow<Changed>(extraBufferCapacity = 8)
        val m = MachineSettings(l, "devbox", changes, backgroundScope)
        runCurrent()
        return m to changes
    }

    @Test
    fun `进来各读一次：账号 · 默认号 · 用量 · 偏好照那台答的`() =
        runTest {
            val l = link()
            val (m, _) = open(l)
            assertEquals(listOf(1, 1, 1), listOf("accounts-list", "quota-read", "prefs-read").map { l.calls(it).size })
            val s = m.state.value
            assertEquals(listOf("work", "lab"), s.accounts.map { it.name })
            assertEquals("work", s.defaultAccount)
            assertEquals("5h 63%" to UsageTone.PLAIN, s.usage["work"]?.let { it.text to it.tone })
            assertEquals("5h —", s.usage["lab"]?.text)
            assertEquals(TabPref.HIDE to TabPref.SHOW, s.shown(TabKind.CHILD) to s.shown(TabKind.BG))
        }

    @Test
    fun `流上 quota 变了只重读用量 · accounts 变了只重读账号；换一条流整份重读`() =
        runTest {
            val l = link()
            val (_, changes) = open(l)
            changes.emit(Changed("quota", null))
            runCurrent()
            assertEquals(listOf(1, 2, 1), listOf("accounts-list", "quota-read", "prefs-read").map { l.calls(it).size })
            changes.emit(Changed("accounts", null))
            runCurrent()
            assertEquals(listOf(2, 2, 1), listOf("accounts-list", "quota-read", "prefs-read").map { l.calls(it).size })
            l.epoch.value = 1
            runCurrent()
            assertEquals(listOf(3, 3, 2), listOf("accounts-list", "quota-read", "prefs-read").map { l.calls(it).size })
        }

    @Test
    fun `拨一行：只交那一格，照回包画`() =
        runTest {
            val l = link()
            val (m, _) = open(l)
            m.setTab(TabKind.CHILD, TabPref.SHOW)
            runCurrent()
            assertEquals(listOf<Map<String, Any?>?>(mapOf("childTab" to "show")), l.calls("prefs-set"))
            assertEquals(TabPref.SHOW, m.state.value.shown(TabKind.CHILD))
            assertNull(m.state.value.notSaved[TabKind.CHILD])
        }

    @Test
    fun `没存上：退回读到的值，那一行下面写「子会话未保存 · 那台那一句」`() =
        runTest {
            val l = link { Reply.Failed(CoreFailure("io_failed", "写入失败 · 磁盘满", "d", null)) }
            val (m, _) = open(l)
            m.setTab(TabKind.CHILD, TabPref.SHOW)
            runCurrent()
            assertEquals("退回读到的值", TabPref.HIDE, m.state.value.shown(TabKind.CHILD))
            assertEquals("子会话未保存 · 写入失败 · 磁盘满", m.state.value.notSaved[TabKind.CHILD])
            m.setTab(TabKind.CHILD, TabPref.HIDE)
            runCurrent()
            assertEquals("拨到此刻画着的那一段不再交", 1, l.calls("prefs-set").size)
        }

    @Test
    fun `默认模型：交那个号那一条，跟随 ⇒ null；读回那一份照画`() =
        runTest {
            val l = link { args -> Reply.Ok(prefs(models = if ((args!!["models"] as Map<*, *>)["work"] == null) emptyMap() else mapOf("work" to "opus"))) }
            val (m, _) = open(l)
            m.setModel("work", "opus")
            runCurrent()
            assertEquals("opus", m.state.value.model("work"))
            m.setModel("work", null)
            runCurrent()
            assertEquals(listOf<Map<String, Any?>?>(mapOf("models" to mapOf("work" to "opus")), mapOf("models" to mapOf("work" to null))), l.calls("prefs-set"))
            assertNull(m.state.value.model("work"))
        }

    @Test
    fun `设为默认：交号名，成了再问一次账号`() =
        runTest {
            val l = link()
            val (m, _) = open(l)
            m.setDefault("lab")
            runCurrent()
            assertEquals(listOf<Map<String, Any?>?>(mapOf("name" to "lab")), l.calls("accounts-set-default"))
            assertEquals(2, l.calls("accounts-list").size)
        }
}
