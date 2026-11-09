package com.ccmonitor.mobile.ui.chat.newsession

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionOpen
import com.ccmonitor.mobile.core.claude.link.SessionOpenFacts
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.MachineStateKind
import com.ccmonitor.mobile.ui.Wires
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import com.ccmonitor.mobile.ui.chat.linkConnecting
import com.ccmonitor.mobile.ui.chat.linkDown
import com.ccmonitor.mobile.ui.chat.machineOf
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 新建会话的选项：打开就是填好的，四样默认值都读那台；点 chip 只为改，改了才交那一格。
 * 判据：默认值从哪来 · 哪颗 chip 不画 · 目录不在灰掉 · 换 agent 账号重问 · 设为默认走那台 · 交给 session-open 的只有改过的。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class NewSessionPickerTest {
    private fun ok(data: Any?) = Reply.Ok(data)

    /** 那台写的显示形（替身写成手机推不出的样子：手机只照抄、不拿家目录自己缩）。 */
    private fun shown(p: String) = p.replaceFirst(Regex("^/h/"), "家/")

    private fun agent(
        kind: String,
        text: String,
    ) = mapOf("kind" to kind, "text" to text, "models" to emptyList<Any>(), "permissionModes" to emptyList<Any>())

    private fun facts(
        recent: List<String> = listOf("/h/work/orders", "/h/work/billing"),
        agents: List<Map<String, Any?>> = listOf(agent("claude", "Claude Code"), agent("codex", "Codex")),
    ) = mapOf(
        "recent" to recent.mapIndexed { i, c -> mapOf("cwd" to c, "shown" to shown(c), "lastMs" to i.toDouble(), "lastText" to "19:0$i") },
        "hosts" to listOf("tmux", "direct"),
        "run" to "tmux",
        "agents" to agents,
        "fork" to null,
    )

    private fun acct(
        name: String,
        selectable: Boolean = true,
        color: Int = 0,
    ) = Wires.accountRow(
        "name" to name,
        "label" to name,
        "selectable" to selectable,
        "color" to color,
        "plate" to name.take(2),
        "kindLine" to "",
        "slots" to if (selectable) listOf(mapOf("slot" to "5h", "text" to "5h ${name.length}%", "tone" to "plain")) else emptyList(),
        "unselectable" to if (selectable) null else "需登录",
        "badge" to mapOf("text" to if (selectable) "已登录" else "需登录", "warn" to !selectable, "title" to ""),
        "login" to !selectable,
    )

    private fun accounts(
        names: List<Map<String, Any?>> = listOf(acct("work"), acct("personal", color = 2), acct("team", selectable = false, color = 1)),
        default: String = "work",
    ) = mapOf("accounts" to names, "meta" to Wires.accountsMeta("effectiveDefault" to default), "notice" to null)

    private val strategies =
        mapOf(
            "strategies" to
                listOf(
                    mapOf("id" to "night", "description" to "夜里只用 personal", "summary" to "夜里只用 personal", "rev" to "r", "remove" to mapOf("enabled" to true), "forNew" to false, "tzUnknown" to false, "hand" to false),
                    mapOf("id" to "spread", "description" to "work → personal", "summary" to "work → personal", "rev" to "r", "remove" to mapOf("enabled" to true), "forNew" to true, "tzUnknown" to false, "hand" to false),
                ),
            "forNew" to "spread",
            "oldScripts" to 0.0,
        )

    private fun link(
        f: Map<String, Any?> = facts(),
        a: (Map<String, Any?>?) -> Map<String, Any?> = { accounts() },
    ) = FakeCoreLink().also { l ->
        l.answer = { cmd, args ->
            when (cmd) {
                "session-open-facts" -> ok(f)
                "files-home" -> ok(mapOf("path" to "/h/me"))
                "accounts-list" -> ok(a(args))
                "rotation-strategies" -> ok(strategies)
                "session-open-dir" -> ok(mapOf("exists" to (args?.get("cwd") != "/h/gone"), "shown" to shown(args?.get("cwd") as String), "tmuxName" to null))
                "accounts-set-default" -> ok(mapOf("applied" to true))
                else -> Reply.LinkDown
            }
        }
    }

    private fun TestScope.picker(
        l: FakeCoreLink,
        from: String? = null,
    ) = NewSessionPicker(l, "smokebox", from, backgroundScope).also { runCurrent() }

    @Test
    fun `默认值：目录 ＝ 那台最近用过的第一个；账号 ＝ 那台的默认号；agent ＝ 那台列的第一家（手机不存缺省哪一家）`() =
        runTest {
            val p = picker(link())
            val s = p.state.value
            assertEquals("/h/work/orders", s.cwd)
            assertEquals("work", s.account)
            assertEquals("claude", s.agent)
            assertEquals(listOf("/h/work/orders", "/h/work/billing"), s.recent.map { it.cwd })
            assertEquals("最近的目录照抄那台写的显示形", listOf("家/work/orders", "家/work/billing"), s.recent.map { it.shown })
            assertEquals("目录 chip 照抄那台写的显示形", "家/work/orders", s.cwdShown)
        }

    @Test
    fun `路径的显示形全照那台写的：不为缩 ~ 去问家目录`() =
        runTest {
            val l = link()
            picker(l, from = "/h/x")
            assertEquals(emptyList<Any?>(), l.calls("files-home"))
        }

    @Test
    fun `默认目录：从一条会话里点新建 ⇒ 那条的目录（显示形问那台）；那台一条记录都没有 ⇒ 家目录`() =
        runTest {
            val from = picker(link(), from = "/h/x").state.value
            assertEquals("/h/x", from.cwd)
            assertEquals("家/x", from.cwdShown)
            val none = picker(link(f = facts(recent = emptyList()))).state.value
            assertEquals("~", none.cwd)
            assertEquals("~", none.cwdShown)
        }

    @Test
    fun `只有一个号 · 一家 agent ⇒ 那颗 chip 不画`() =
        runTest {
            val many = picker(link()).state.value
            assertTrue(many.showAgentChip)
            assertTrue(many.showAccountChip)
            val one = picker(link(f = facts(agents = listOf(agent("claude", "Claude Code"))), a = { accounts(names = listOf(acct("work"))) })).state.value
            assertFalse(one.showAgentChip)
            assertFalse(one.showAccountChip)
        }

    @Test
    fun `什么都没改 ⇒ 交给 session-open 的只有目录与 agent，账号 · 轮换不交（那台按它的默认）`() =
        runTest {
            val p = picker(link())
            p.openAccounts()
            runCurrent()
            p.settled()
            assertEquals(Chosen("/h/work/orders", "claude", null, null), p.chosen())
        }

    @Test
    fun `改了账号 · 轮换（含不轮换）⇒ 交那几格`() =
        runTest {
            val p = picker(link())
            p.openAccounts()
            runCurrent()
            p.pickAccount("personal")
            p.pickRotation(null)
            assertEquals(Chosen("/h/work/orders", "claude", "personal", SessionOpen.Rotation(null)), p.chosen())
            p.pickRotation("night")
            assertEquals(SessionOpen.Rotation("night"), p.chosen().rotation)
            p.pickRotation("spread")
            assertNull("又选回新会话用的那份 ⇒ 不交", p.chosen().rotation)
        }

    @Test
    fun `选不了的号（需登录）点了不选上`() =
        runTest {
            val p = picker(link())
            p.pickAccount("team")
            assertEquals("work", p.state.value.account)
        }

    @Test
    fun `账号抽屉：打开那一刻问一次账号 · 轮换（5h 那一格就在号行上，不另问额度）；新会话用的那份是默认`() =
        runTest {
            val l = link()
            val p = picker(l)
            val before = l.calls("accounts-list").size
            p.openAccounts()
            runCurrent()
            assertEquals(before + 1, l.calls("accounts-list").size)
            assertEquals(0, l.calls("quota-read").size)
            val s = p.state.value
            assertEquals(listOf("5h 4%", "5h 8%", null), s.accounts.map { it.slots.firstOrNull()?.text })
            assertEquals(listOf("night", "spread"), s.strategies?.map { it.id })
            assertEquals("spread", s.rotation)
            assertEquals("spread", s.forNew)
        }

    @Test
    fun `换 agent ⇒ 账号照那一家重问，回到那一家的默认号`() =
        runTest {
            val l = link(a = { args -> if (args?.get("agent") == "codex") accounts(names = listOf(acct("cx1"), acct("cx2")), default = "cx2") else accounts() })
            val p = picker(l)
            p.pickAccount("personal")
            p.pickAgent("codex")
            runCurrent()
            assertEquals(mapOf("agent" to "codex"), l.calls("accounts-list").last())
            assertEquals("cx2", p.state.value.account)
            assertEquals(Chosen("/h/work/orders", "codex", null, null), p.chosen())
        }

    @Test
    fun `设为默认：交那台（accounts-set-default），再问一次账号；默认那一格照新的`() =
        runTest {
            var def = "work"
            val l = link(a = { accounts(default = def) })
            val base = l.answer
            l.answer = { cmd, args -> if (cmd == "accounts-set-default") ok(mapOf("applied" to true)).also { def = args?.get("name") as String } else base(cmd, args) }
            val p = picker(l)
            p.setDefault("personal")
            runCurrent()
            assertEquals(listOf(mapOf("name" to "personal")), l.calls("accounts-set-default"))
            assertEquals("personal", p.state.value.defaultAccount)
        }

    @Test
    fun `最近的目录：点一行先问那台在不在；不在 ⇒ 那一行灰掉写「目录不在」、不选；在 ⇒ 选上`() =
        runTest {
            val p = picker(link(f = facts(recent = listOf("/h/work/orders", "/h/gone"))))
            p.pickRecent("/h/gone")
            runCurrent()
            assertEquals(
                listOf("/h/gone"),
                p.state.value.recent
                    .filter { it.missing }
                    .map { it.cwd },
            )
            assertEquals("/h/work/orders", p.state.value.cwd)
            p.pickRecent("/h/work/orders")
            runCurrent()
            assertEquals("/h/work/orders", p.state.value.cwd)
            assertEquals("家/work/orders", p.state.value.cwdShown)
            p.pickDir(Picked("/h/work/billing", "家/work/billing"))
            assertEquals("/h/work/billing", p.chosen().cwd)
            assertEquals("浏览屏选的照抄那一行的显示形", "家/work/billing", p.state.value.cwdShown)
        }

    @Test
    fun `那台问不到（新建会话信息）⇒ 那一句照文案表带核心那句；目录退回从哪进来的 · 家目录`() =
        runTest {
            val l = FakeCoreLink().also { it.answer = { _, _ -> Reply.TimedOut("t-said", "") } }
            val p = picker(l)
            assertEquals("~", p.state.value.cwd)
            assertEquals(copyText("newSession.facts.failed", "machine" to "smokebox", "why" to "t-said"), p.state.value.factsFailed)
        }

    @Test
    fun `打开时那台离线、之后常驻流接上了 ⇒ 自己再问一遍（不等人重开这一张）；接上过、问到了 ⇒ 再换流不重问`() =
        runTest {
            val l = link()
            val good = l.answer
            l.answer = { _, _ -> Reply.LinkDown }
            val p = picker(l)
            assertTrue(p.state.value.factsFailed != null)
            l.answer = good
            l.connected(1)
            runCurrent()
            assertNull(p.state.value.factsFailed)
            assertEquals("/h/work/orders", p.state.value.cwd)
            assertEquals("work", p.state.value.account)
            l.answer = { _, _ -> Reply.LinkDown }
            l.connected(2)
            runCurrent()
            assertNull("问到了就不再问（换流不把填好的冲掉）", p.state.value.factsFailed)
            assertEquals("work", p.state.value.account)
        }

    @Test
    fun `打开时还在连：接上一次 ⇒ 那一问只发一次（照核心那一格 seen，不另加开屏那一问）`() =
        runTest {
            val l = link()
            val good = l.answer
            l.state.value = linkConnecting()
            l.answer = { _, _ -> Reply.LinkDown }
            val p = picker(l)
            l.answer = good
            l.connected(1)
            runCurrent()
            assertEquals(1, l.calls(SessionOpenFacts.COMMAND).size)
            assertNull(p.state.value.factsFailed)
            assertEquals("work", p.state.value.account)
        }

    @Test
    fun `那台断着（核心那一行是 Down）⇒ 不问，但 settled 不挂着：起会话照此刻的选项交、由那台那一问说没送到`() =
        runTest {
            val l = link()
            l.state.value = linkDown("x")
            val p = picker(l)
            val done = backgroundScope.async { p.settled() }
            runCurrent()
            assertTrue("断着时等选项那一下挂住了（第一句会一直「发送中」）", done.isCompleted)
            assertEquals(0, l.calls(SessionOpenFacts.COMMAND).size)
            l.connected(1)
            runCurrent()
            assertEquals("接上了照常问一次", 1, l.calls(SessionOpenFacts.COMMAND).size)
        }

    /** 核心交一行新的 Up（换了一条流，`epoch` 跟着换：真的那一份两样同一刻变）。 */
    private fun FakeCoreLink.connected(epoch: Long) {
        state.value = LinkLine.Up(epoch, machineOf(MachineStateKind.UP))
        this.epoch.value = epoch
    }
}
