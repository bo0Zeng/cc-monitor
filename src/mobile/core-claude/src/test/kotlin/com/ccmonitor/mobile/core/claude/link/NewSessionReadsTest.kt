package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.PasteKind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

/** 新建会话那几屏问那台的几条：入参照协议的格，回包照 `IPC-COMMANDS.md` 那一形解，缺必填格 ⇒ 解不出。 */
class NewSessionReadsTest {
    private val what = mapOf("kind" to "new", "agent" to "claude", "cwd" to "~/proj", "prompt" to "你好")
    private val base = mapOf("what" to what, "local" to false, "ticket" to "t-1", "client" to "mobile")

    @Test
    fun `session-open 新起：账号 · 轮换只在改过时交；改了交那一形（named · strategy，不轮换 ＝ null）`() {
        assertEquals(base, SessionOpen.newIn(cwd = "~/proj", ticket = "t-1", agent = "claude", prompt = "你好", account = null, rotation = null))
        assertEquals(
            base + ("account" to mapOf("kind" to "named", "name" to "work")),
            SessionOpen.newIn(cwd = "~/proj", ticket = "t-1", agent = "claude", prompt = "你好", account = "work"),
        )
        assertEquals(
            base + ("what" to what + ("rotation" to mapOf("strategy" to null))),
            SessionOpen.newIn(cwd = "~/proj", ticket = "t-1", agent = "claude", prompt = "你好", rotation = SessionOpen.Rotation(null)),
        )
        assertEquals(
            base + ("what" to what + ("rotation" to mapOf("strategy" to "night"))),
            SessionOpen.newIn(cwd = "~/proj", ticket = "t-1", agent = "claude", prompt = "你好", rotation = SessionOpen.Rotation("night")),
        )
    }

    @Test
    fun `session-open-facts：最近的目录照那台给的序，带写好的时刻字`() {
        val facts =
            SessionOpenFacts.of(
                mapOf(
                    "recent" to listOf(mapOf("cwd" to "/h/a", "shown" to "~/a", "lastMs" to 2.0, "lastText" to "19:02"), mapOf("cwd" to "/h/b", "shown" to "~/b", "lastMs" to 1.0, "lastText" to "昨天")),
                    "hosts" to listOf("tmux", "direct"),
                    "run" to "tmux",
                    "agents" to listOf(mapOf("kind" to "claude", "text" to "Claude Code", "models" to emptyList<Any>(), "permissionModes" to emptyList<Any>())),
                    "fork" to null,
                ),
            )!!
        assertEquals(listOf("/h/a" to "19:02", "/h/b" to "昨天"), facts.recent.map { it.cwd to it.lastText })
        assertEquals("显示形照那台写好的", listOf("~/a", "~/b"), facts.recent.map { it.shown })
        assertNull("最近一格缺 lastText ⇒ 解不出", SessionOpenFacts.of(mapOf("recent" to listOf(mapOf("cwd" to "/h/a", "lastMs" to 1.0)), "agents" to emptyList<Any>())))
    }

    @Test
    fun `session-open-dir：只交目录；在不在 · 显示形照回包`() {
        assertEquals(mapOf("cwd" to "~/x"), SessionOpenDir.args("~/x"))
        assertEquals(false, SessionOpenDir.of(mapOf("exists" to false, "shown" to "/x", "tmuxName" to null))?.exists)
        val here = SessionOpenDir.of(mapOf("exists" to true, "shown" to "~/a", "tmuxName" to "x-cc"))
        assertEquals(true, here?.exists)
        assertEquals("那台家目录 /home/x、目录 /home/x/a ⇒ 照抄它写的 ~/a", "~/a", here?.shown)
        assertNull(SessionOpenDir.of(mapOf("tmuxName" to null)))
    }

    @Test
    fun `accounts-list：照类型解，列不列只照那台的 newSession；号牌字 · 5h 那一格 · 选不了的那一句照那台写的；这台的默认照 meta；缺格 ⇒ 解不出`() {
        val row = { name: String, listed: Boolean, ok: Boolean, color: Int ->
            mapOf<String, Any?>(
                "name" to name,
                "label" to name.uppercase(),
                "newSession" to listed,
                "selectable" to ok,
                "color" to color,
                "plate" to "夹" + name,
                "kindLine" to "",
                "slots" to if (ok) listOf(mapOf("slot" to "5h", "text" to "5h 41%", "tone" to "plain")) else emptyList(),
                "unselectable" to if (ok) null else "需登录",
                "authKind" to "subscription",
                "badge" to mapOf("text" to if (ok) "已登录" else "需登录", "warn" to !ok, "title" to ""),
                "login" to !ok,
                "keyMasked" to null,
                "endpointHost" to null,
                "email" to "",
                "configDir" to null,
                "configDirShown" to null,
                "isDefault" to false,
                "mode" to "subscription",
                "exists" to true,
                "loggedIn" to ok,
                "authReady" to ok,
            )
        }
        val data =
            mapOf(
                "accounts" to listOf(row("0", false, true, 0), row("work", true, true, 3), row("team", true, false, 1)),
                "meta" to mapOf("enabled" to true, "acctsDir" to "/h/.cc", "manifestPath" to "/h/.cc/accounts.json", "count" to 3, "effectiveDefault" to "work"),
                "notice" to null,
            )
        val a = AccountsList.of(data)!!
        assertEquals("work", a.meta.effectiveDefault)
        assertEquals("那台说不进选号表的不列（有目录、隔离也不列；界面不另判，同桌面新建会话框）", listOf("work", "team"), a.accounts.map { it.name })
        assertEquals(listOf(3, 1), a.accounts.map { it.color })
        assertEquals(listOf("夹work", "夹team"), a.accounts.map { it.plate })
        assertEquals(listOf("5h 41%", null), a.accounts.map { it.slots.firstOrNull()?.text })
        assertEquals(listOf(null, "需登录"), a.accounts.map { it.unselectable })
        assertEquals(mapOf("agent" to "claude"), AccountsList.args("claude"))
        assertNull("缺号牌字 ⇒ 契约对不上", AccountsList.of(data + ("accounts" to listOf(row("x", true, true, 0) - "plate"))))
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
                            mapOf("id" to "night", "description" to "夜里只用 personal\n第二行", "summary" to "夜里只用 personal", "rev" to "r", "remove" to mapOf("enabled" to true), "forNew" to false, "tzUnknown" to false, "hand" to false),
                            mapOf("id" to "spread", "description" to "", "summary" to "", "rev" to "r", "remove" to mapOf("enabled" to true), "forNew" to true, "tzUnknown" to false, "hand" to false),
                        ),
                    "forNew" to "spread",
                    "oldScripts" to 0.0,
                ),
            )!!
        assertEquals("spread", r.forNew)
        assertEquals(listOf("night", "spread"), r.strategies.map { it.id })
        assertEquals("夜里只用 personal", r.strategies[0].summary)
        assertNull(RotationStrategies.of(mapOf("strategies" to emptyList<Any>())))
    }

    @Test
    fun `files-ls：只列文件夹；路径 · 名字 · 上一级都是那台写好的；读不成字的不列`() {
        assertEquals(mapOf("path" to "/h/work", "dirs_only" to true), FilesLs.dirs("/h/work"))
        val got =
            FilesLs.dirsOf(
                lsWhole(
                    listOf(
                        mapOf("path" to "/h/work/billing", "shown" to "~/work/billing", "name" to "billing", "kind" to "dir", "class" to "folder", "type_text" to "文件夹"),
                        mapOf("path" to mapOf("b16" to "ff"), "shown" to mapOf("b16" to "ff"), "name" to mapOf("b16" to "ff"), "kind" to "dir", "class" to "folder", "type_text" to "文件夹"),
                        mapOf("path" to "/h/work/notes", "shown" to "~/work/notes", "name" to "notes", "kind" to "symlink", "class" to "link", "type_text" to "链接", "link_dir" to true),
                    ),
                    mapOf(
                        "shown" to "~/work",
                        "up" to "/h",
                        "crumbs" to listOf(mapOf("name" to "/", "path" to "/"), mapOf("name" to "h", "path" to "/h"), mapOf("name" to "work", "path" to "/h/work")),
                        "truncated" to false,
                        "total" to 3.0,
                        "unreadable" to 0.0,
                    ),
                ),
            )!!
        assertEquals(listOf("/h/work/billing", "/h/work/notes"), got.dirs.map { it.path })
        assertEquals(listOf("billing", "notes"), got.dirs.map { it.name })
        assertEquals("显示形照那台写好的（手机不拿家目录缩）", listOf("~/work/billing", "~/work/notes"), got.dirs.map { it.shown })
        assertEquals("~/work", got.shown)
        assertEquals("/h", got.up)
        assertEquals("这一层的绝对路径是面包屑最后一段（问 `~/work` 时那台展开的）", "/h/work", got.at)
        assertEquals("~/work", FilesLs.dirs("~/work")["path"])
        assertNull(FilesLs.dirsOf(lsWhole(listOf(mapOf("kind" to "dir", "class" to "folder", "type_text" to "文件夹")), mapOf("up" to null, "crumbs" to emptyList<Any>(), "truncated" to false, "total" to 1.0))))
    }

    @Test
    fun `login-start · finish · cancel：账号那一形的入参；回包照 LoginStarted 解（粘回来的是什么照线上字认）`() {
        assertEquals(mapOf("kind" to "account", "agent" to "claude", "account" to "team"), Login.start(Login.account("claude", "team")))
        assertEquals(mapOf("job" to "j1", "pasted" to "abc"), Login.finish("j1", "abc"))
        assertEquals(mapOf("job" to "j1"), Login.cancel("j1"))
        val s = Login.started(mapOf("job" to "j1", "url" to "https://example.com/x", "paste" to "redirectUrl", "auto" to false, "opened" to false))!!
        assertEquals(PasteKind.REDIRECT_URL, s.paste)
        assertFalse(s.auto)
        assertNull(Login.started(mapOf("job" to "j1", "url" to "u", "paste" to "other", "auto" to false, "opened" to false)))
    }

    @Test
    fun `accounts-login-cmd：只交号名；回包照 AccountLoginCmd 解出那一行；缺格 ⇒ 解不出`() {
        assertEquals("accounts-login-cmd", Login.CMD)
        assertEquals(mapOf("name" to "team"), Login.cmd("team"))
        assertEquals("CLAUDE_CONFIG_DIR=/x claude /login", Login.cmdOf(mapOf("cmd" to "CLAUDE_CONFIG_DIR=/x claude /login")))
        assertNull(Login.cmdOf(mapOf("line" to "x")))
    }
}
