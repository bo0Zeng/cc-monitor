package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.PasteKind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 新建会话那几屏问那台的几条：入参照协议的格，回包照 `IPC-COMMANDS.md` 那一形解，缺必填格 ⇒ 解不出。 */
class NewSessionReadsTest {
    private val base = mapOf("agent" to "claude", "cwd" to "~/proj", "place" to "tmux", "local" to false, "ticket" to "t-1", "prompt" to "你好")

    @Test
    fun `session-new：账号 · 轮换只在改过时交；改了交那一形（named · strategy，不轮换 ＝ null）`() {
        assertEquals(base, SessionNew.args(cwd = "~/proj", ticket = "t-1", agent = "claude", prompt = "你好", account = null, rotation = null))
        assertEquals(
            base + ("account" to mapOf("kind" to "named", "name" to "work")),
            SessionNew.args(cwd = "~/proj", ticket = "t-1", agent = "claude", prompt = "你好", account = "work"),
        )
        assertEquals(
            base + ("rotation" to mapOf("strategy" to null)),
            SessionNew.args(cwd = "~/proj", ticket = "t-1", agent = "claude", prompt = "你好", rotation = SessionNew.Rotation(null)),
        )
        assertEquals(
            base + ("rotation" to mapOf("strategy" to "night")),
            SessionNew.args(cwd = "~/proj", ticket = "t-1", agent = "claude", prompt = "你好", rotation = SessionNew.Rotation("night")),
        )
    }

    @Test
    fun `session-new-facts：最近的目录照那台给的序，带写好的时刻字`() {
        val facts =
            SessionNewFacts.of(
                mapOf(
                    "recent" to listOf(mapOf("cwd" to "/h/a", "lastMs" to 2.0, "lastText" to "19:02"), mapOf("cwd" to "/h/b", "lastMs" to 1.0, "lastText" to "昨天")),
                    "tmux" to true,
                    "agents" to listOf(mapOf("kind" to "claude", "text" to "Claude Code", "models" to emptyList<Any>(), "permissionModes" to emptyList<Any>())),
                    "fork" to null,
                ),
            )!!
        assertEquals(listOf("/h/a" to "19:02", "/h/b" to "昨天"), facts.recent.map { it.cwd to it.lastText })
        assertNull("最近一格缺 lastText ⇒ 解不出", SessionNewFacts.of(mapOf("recent" to listOf(mapOf("cwd" to "/h/a", "lastMs" to 1.0)), "agents" to emptyList<Any>())))
    }

    @Test
    fun `session-new-dir：只交目录；在不在照回包`() {
        assertEquals(mapOf("cwd" to "~/x"), SessionNewDir.args("~/x"))
        assertEquals(false, SessionNewDir.of(mapOf("exists" to false, "tmuxName" to null)))
        assertEquals(true, SessionNewDir.of(mapOf("exists" to true, "tmuxName" to "x-cc")))
        assertNull(SessionNewDir.of(mapOf("tmuxName" to null)))
    }

    @Test
    fun `accounts-list：每号名 · 号色 · 能不能选；这台的默认 · 家目录照 meta；缺名 ⇒ 解不出`() {
        val data =
            mapOf(
                "accounts" to
                    listOf(
                        mapOf("name" to "0", "configDir" to null, "mode" to "base", "selectable" to true, "loggedIn" to true, "color" to null),
                        mapOf("name" to "work", "label" to "工作号", "configDir" to "/a/work", "mode" to "isolated", "selectable" to true, "loggedIn" to true, "color" to 0.0),
                        mapOf("name" to "team", "configDir" to "/a/team", "mode" to "isolated", "selectable" to false, "loggedIn" to false, "color" to 1.0),
                    ),
                "meta" to mapOf("enabled" to true, "effectiveDefault" to "work", "home" to "/h"),
                "notice" to null,
            )
        val a = AccountsList.of(data)!!
        assertEquals("work", a.effectiveDefault)
        assertEquals("/h", a.home)
        assertEquals(listOf("work", "team"), a.accounts.map { it.name })
        assertEquals("账号 0（不是隔离的号）不列（同桌面新建会话框）", null, a.accounts.firstOrNull { it.name == "0" })
        assertEquals(listOf(0, 1), a.accounts.map { it.color })
        assertEquals("号名上屏照那台写的 label；没有 ⇒ 号名", listOf("工作号", "team"), a.accounts.map { it.label })
        assertEquals(listOf(true, false), a.accounts.map { it.selectable })
        assertEquals(mapOf("agent" to "claude"), AccountsList.args("claude"))
        assertNull(AccountsList.of(data + ("accounts" to listOf(mapOf("configDir" to "/a/x", "mode" to "isolated")))))
        assertTrue("没开多账号 ⇒ 一个号都不列", AccountsList.of(mapOf("accounts" to emptyList<Any>(), "meta" to mapOf("enabled" to false)))!!.accounts.isEmpty())
    }

    @Test
    fun `accounts-set-default：只交号名`() {
        assertEquals(mapOf("name" to "team"), AccountsSetDefault.args("team"))
    }

    @Test
    fun `rotation-strategies：每份 id · 描述第一行 · 新会话用不用；新会话用的那份 id 照 forNew`() {
        val r =
            RotationStrategies.of(
                mapOf(
                    "strategies" to
                        listOf(
                            mapOf("id" to "night", "description" to "夜里只用 personal\n第二行", "rev" to "r", "inUse" to emptyList<Any>(), "forNew" to false, "wordsBad" to false, "tzUnknown" to false),
                            mapOf("id" to "spread", "description" to "", "rev" to "r", "inUse" to emptyList<Any>(), "forNew" to true, "wordsBad" to false, "tzUnknown" to false),
                        ),
                    "forNew" to "spread",
                    "oldScripts" to 0.0,
                ),
            )!!
        assertEquals("spread", r.forNew)
        assertEquals(listOf("night", "spread"), r.strategies.map { it.id })
        assertEquals("夜里只用 personal", RotationStrategies.firstLine(r.strategies[0].description))
        assertNull(RotationStrategies.of(mapOf("strategies" to emptyList<Any>())))
    }

    @Test
    fun `files-ls：只列文件夹；路径 · 名字 · 上一级都是那台写好的；读不成字的不列`() {
        assertEquals(mapOf("path" to "/h/work", "dirs_only" to true), FilesLs.dirs("/h/work"))
        val got =
            FilesLs.dirsOf(
                mapOf(
                    "entries" to
                        listOf(
                            mapOf("path" to "/h/work/billing", "name" to "billing", "kind" to "dir"),
                            mapOf("path" to mapOf("b16" to "ff"), "name" to mapOf("b16" to "ff"), "kind" to "dir"),
                            mapOf("path" to "/h/work/notes", "name" to "notes", "kind" to "symlink", "link_dir" to true),
                        ),
                    "up" to "/h",
                    "crumbs" to emptyList<Any>(),
                    "truncated" to false,
                    "total" to 3.0,
                    "unreadable" to 0.0,
                ),
            )!!
        assertEquals(listOf("/h/work/billing", "/h/work/notes"), got.dirs.map { it.path })
        assertEquals(listOf("billing", "notes"), got.dirs.map { it.name })
        assertEquals("/h", got.up)
        assertNull(FilesLs.dirsOf(mapOf("entries" to listOf(mapOf("kind" to "dir")))))
    }

    @Test
    fun `files-home：那台的家目录`() {
        assertEquals("/h", FilesHome.text(mapOf("path" to "/h")))
        assertNull(FilesHome.text(mapOf("path" to mapOf("b16" to "ff"))))
    }

    @Test
    fun `login-start · finish · cancel：账号那一形的入参；回包照 LoginStarted 解（粘回来的是什么照线上字认）`() {
        assertEquals(mapOf("kind" to "account", "agent" to "claude", "account" to "team"), Login.startAccount("claude", "team"))
        assertEquals(mapOf("job" to "j1", "pasted" to "abc"), Login.finish("j1", "abc"))
        assertEquals(mapOf("job" to "j1"), Login.cancel("j1"))
        val s = Login.started(mapOf("job" to "j1", "url" to "https://example.com/x", "paste" to "redirectUrl", "auto" to false))!!
        assertEquals(PasteKind.REDIRECT_URL, s.paste)
        assertFalse(s.auto)
        assertNull(Login.started(mapOf("job" to "j1", "url" to "u", "paste" to "other", "auto" to false)))
    }
}
