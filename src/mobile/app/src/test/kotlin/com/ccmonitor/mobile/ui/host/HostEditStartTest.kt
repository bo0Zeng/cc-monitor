package com.ccmonitor.mobile.ui.host

import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.wire.gen.FirstRunGo
import com.ccmonitor.mobile.core.wire.gen.FirstRunStepId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 编辑页起手：上面四格（地址 · 用户 · 密码 · 名称），别的收进「更多设置」。新建恒收着、缺省密码登录；
 * 编辑一台时更多设置里有不是缺省值的格 ⇒ 展开（不然那一格的值藏在折叠里看不见）。零台空态照「开始用」那一形只一步。
 */
class HostEditStartTest {
    private val plain = Host(id = "h", label = "h", host = "a", username = "u", passwordEnc = "x")

    @Test
    fun `新建 ⇒ 密码登录 · 更多设置收着`() {
        assertEquals(HostEditStart(authIsPassword = true, moreOpen = false), hostEditStart(null))
    }

    @Test
    fun `编辑一台只有缺省值的 ⇒ 收着 · 照它实际登的那一种（密码）`() {
        assertEquals(HostEditStart(authIsPassword = true, moreOpen = false), hostEditStart(plain))
    }

    @Test
    fun `更多设置里任一格不是缺省值 ⇒ 展开`() {
        val off =
            listOf(
                plain.copy(port = 2222),
                plain.copy(addresses = listOf("b")),
                plain.copy(defaultWorkingDir = "/w"),
                plain.copy(groupId = "g"),
                plain.copy(jump = "j"),
                plain.copy(autoReconnect = false),
                plain.copy(authRef = "k"),
                // 既没钥匙也没密码（导入的那几台）：认证要在更多设置里挑钥匙。
                plain.copy(passwordEnc = null),
            )
        for (h in off) assertTrue("$h", hostEditStart(h).moreOpen)
    }

    @Test
    fun `用钥匙登的那台 ⇒ 不是密码登录`() {
        assertFalse(hostEditStart(plain.copy(authRef = "k")).authIsPassword)
        // 换成密钥后两样并存：照实际登的那一种（先用钥匙）。
        assertFalse(hostEditStart(plain.copy(authRef = "k", passwordEnc = "x")).authIsPassword)
    }

    @Test
    fun `零台空态只一步：加一台机器 · 没打勾 · 去处 add_machine`() {
        val steps = zeroHostStart()
        assertEquals(1, steps.size)
        val st = steps.single()
        assertEquals(FirstRunStepId.REMOTE, st.id)
        assertFalse(st.done)
        assertEquals(FirstRunGo.ADD_MACHINE, st.act.go)
        assertTrue(st.name.isNotBlank() && st.act.label.isNotBlank())
    }
}
