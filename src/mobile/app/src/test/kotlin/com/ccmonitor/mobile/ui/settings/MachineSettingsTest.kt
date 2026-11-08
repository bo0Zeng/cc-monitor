package com.ccmonitor.mobile.ui.settings

import com.ccmonitor.mobile.core.claude.link.QuotaRead
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.TabKind
import com.ccmonitor.mobile.core.wire.gen.Changed
import com.ccmonitor.mobile.core.wire.gen.Failed
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.MachineStateKind
import com.ccmonitor.mobile.core.wire.gen.Tab
import com.ccmonitor.mobile.core.wire.gen.Tone
import com.ccmonitor.mobile.link.Fetched
import com.ccmonitor.mobile.ui.Wires
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import com.ccmonitor.mobile.ui.chat.linkConnecting
import com.ccmonitor.mobile.ui.chat.linkDown
import com.ccmonitor.mobile.ui.chat.machineOf
import com.ccmonitor.mobile.ui.usage.QuotaGolden
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
        slots: List<Map<String, Any?>> = emptyList(),
    ) = Wires.accountRow(
        "name" to name,
        "label" to name,
        "selectable" to selectable,
        "slots" to slots,
        "plate" to name.take(2),
        "kindLine" to "",
        "unselectable" to if (selectable) null else "需登录",
        "login" to !selectable,
        "badge" to mapOf("text" to if (selectable) "已登录" else "需登录", "warn" to !selectable, "title" to ""),
    )

    /** 号行里那台写好的一格额度。 */
    private fun cell(
        text: String,
        tone: String,
    ) = mapOf("slot" to "5h", "text" to text, "tone" to tone)

    private fun prefs(
        child: String = "hide",
        models: Map<String, String> = emptyMap(),
    ) = mapOf(
        "state" to "ok",
        "reason" to null,
        "detail" to null,
        "path" to null,
        "prefs" to Wires.prefs("models" to models, "childTab" to child),
    )

    /** 出过数的 `personal`（5h 63%）＋ 没出过数的那两个（金样两例拼成一份）。 */
    private val quota = QuotaGolden.reply("订阅 · 正常") + ("unseen" to QuotaGolden.reply("没出过数")["unseen"])

    private fun link(
        quotaReply: () -> Reply = { Reply.Ok(quota) },
        set: (Map<String, Any?>?) -> Reply = { Reply.Ok(prefs(child = "show")) },
    ) =
        FakeCoreLink().also { l ->
            l.answer = { cmd, args ->
                when (cmd) {
                    "accounts-list" ->
                        Reply.Ok(mapOf("accounts" to listOf(acct("work", slots = listOf(cell("后端·5h 63% ↻13:00", "warn"), cell("后端·7d", "plain"))), acct("lab", selectable = false)), "meta" to Wires.accountsMeta("effectiveDefault" to "work"), "notice" to null))
                    "quota-read" -> quotaReply()
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
            assertEquals("号旁那一格是号行里的第一格", "后端·5h 63% ↻13:00" to Tone.WARN, s.usage("work")?.let { it.text to it.tone })
            assertNull("号行里没有格 ⇒ 不出", s.usage("lab"))
            assertNull("不在号列表里的号 ⇒ 不出（额度账里有也不另拼）", s.usage("personal"))
            assertEquals(Tab.HIDE to Tab.SHOW, s.shown(TabKind.CHILD) to s.shown(TabKind.BG))
        }

    @Test
    fun `流上 quota 变了重读用量与账号（号行里有额度格）· accounts 变了只重读账号；换一条流整份重读`() =
        runTest {
            val l = link()
            val (_, changes) = open(l)
            changes.emit(Changed.Quota())
            runCurrent()
            assertEquals(listOf(2, 2, 1), listOf("accounts-list", "quota-read", "prefs-read").map { l.calls(it).size })
            changes.emit(Changed.Accounts)
            runCurrent()
            assertEquals(listOf(3, 2, 1), listOf("accounts-list", "quota-read", "prefs-read").map { l.calls(it).size })
            l.state.value = linkDown("x")
            runCurrent()
            l.state.value = LinkLine.Up(1L, machineOf(MachineStateKind.UP))
            l.epoch.value = 1
            runCurrent()
            assertEquals(listOf(4, 3, 2), listOf("accounts-list", "quota-read", "prefs-read").map { l.calls(it).size })
        }

    @Test
    fun `那台接上之前开的屏：接上那一刻账号 · 用量 · 偏好各只读一次`() =
        runTest {
            val l = link()
            l.state.value = linkConnecting()
            open(l)
            assertEquals(listOf(0, 0, 0), listOf("accounts-list", "quota-read", "prefs-read").map { l.calls(it).size })
            l.state.value = LinkLine.Up(1L, machineOf(MachineStateKind.UP))
            l.epoch.value = 1
            runCurrent()
            assertEquals(listOf(1, 1, 1), listOf("accounts-list", "quota-read", "prefs-read").map { l.calls(it).size })
        }

    @Test
    fun `用量读不成也记下：超时 ⇒ 读不成、那一句是核心写好的那一句`() =
        runTest {
            val (m, _) = open(link(quotaReply = { Reply.TimedOut("t-said", "") }))
            assertEquals("t-said", (m.state.value.quota as? Fetched.Failed)?.problem?.text)
            assertNull(m.state.value.usage("personal"))
        }

    @Test
    fun `流上 quota 来了重读：新的到之前手上那份照留，再读没成也不清`() =
        runTest {
            var answer: () -> Reply = { Reply.Ok(quota) }
            val (m, changes) = open(link(quotaReply = { answer() }))
            val held = m.state.value.quota
            answer = { Reply.TimedOut("t-said", "") }
            changes.emit(Changed.Quota())
            runCurrent()
            assertEquals(held, m.state.value.quota)
            assertEquals(
                "t-said",
                m.state.value.quotaRereadFailed
                    ?.text,
            )
            answer = { Reply.Ok(QuotaGolden.reply("订阅 · 快满")) }
            changes.emit(Changed.Quota())
            runCurrent()
            assertEquals(QuotaGolden.reply("订阅 · 快满")["verdict"].let { (it as Map<*, *>)["text"] }, (m.state.value.quota as Fetched.Got).value.verdict?.text)
            assertNull(m.state.value.quotaRereadFailed)
        }

    @Test
    fun `流上 quota 带着体：照体画、不再问那台；再读没成的那一句一起清`() =
        runTest {
            var answer: () -> Reply = { Reply.Ok(quota) }
            val l = link(quotaReply = { answer() })
            val (m, changes) = open(l)
            answer = { Reply.TimedOut("t-said", "") }
            changes.emit(Changed.Quota())
            runCurrent()
            val pushed = QuotaRead.of(QuotaGolden.reply("订阅 · 快满"))!!
            changes.emit(Changed.Quota(pushed))
            runCurrent()
            assertEquals(2, l.calls("quota-read").size)
            assertEquals(Fetched.Got(pushed), m.state.value.quota)
            assertNull(m.state.value.quotaRereadFailed)
        }

    @Test
    fun `拨一行：只交那一格，照回包画`() =
        runTest {
            val l = link()
            val (m, _) = open(l)
            m.setTab(TabKind.CHILD, Tab.SHOW)
            runCurrent()
            assertEquals(listOf<Map<String, Any?>?>(mapOf("childTab" to "show")), l.calls("prefs-set"))
            assertEquals(Tab.SHOW, m.state.value.shown(TabKind.CHILD))
            assertNull(m.state.value.notSaved[TabKind.CHILD])
        }

    @Test
    fun `没存上：退回读到的值，那一行下面写「子会话未保存 · 那台那一句」`() =
        runTest {
            val l = link(set = { Reply.Failed(Failed("io_failed", "写入失败 · 磁盘满", "d", null)) })
            val (m, _) = open(l)
            m.setTab(TabKind.CHILD, Tab.SHOW)
            runCurrent()
            assertEquals("退回读到的值", Tab.HIDE, m.state.value.shown(TabKind.CHILD))
            assertEquals("子会话未保存 · 写入失败 · 磁盘满", m.state.value.notSaved[TabKind.CHILD])
            m.setTab(TabKind.CHILD, Tab.HIDE)
            runCurrent()
            assertEquals("拨到此刻画着的那一段不再交", 1, l.calls("prefs-set").size)
        }

    @Test
    fun `默认模型：交那个号那一条，跟随 ⇒ null；读回那一份照画`() =
        runTest {
            val l = link(set = { args -> Reply.Ok(prefs(models = if ((args!!["models"] as Map<*, *>)["work"] == null) emptyMap() else mapOf("work" to "opus"))) })
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
