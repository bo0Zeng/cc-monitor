package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.data.db.Host
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 一台登哪一种只判一处（[login]）：记着钥匙先用钥匙；没有才用存的密码；都没有 ⇒ 说不出（连接时临时选）。 */
class HostLoginTest {
    private val bare = Host(id = "h", label = "h", host = "a", username = "u")

    @Test
    fun `只记着钥匙 ⇒ 钥匙`() {
        assertEquals(HostLogin.Key, bare.copy(authRef = "k").login())
    }

    @Test
    fun `只存着密码 ⇒ 密码`() {
        assertEquals(HostLogin.Password, bare.copy(passwordEnc = "x").login())
    }

    @Test
    fun `两样都有（换成密钥之后密码留着给撤销） ⇒ 钥匙`() {
        assertEquals(HostLogin.Key, bare.copy(authRef = "k", passwordEnc = "x").login())
    }

    @Test
    fun `都没有 ⇒ null`() {
        assertNull(bare.login())
    }
}
