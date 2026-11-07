package com.ccmonitor.mobile.ui.common

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.Identity
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 机器条上［换成密钥］那一下：有钥匙用最早那一把、没有才生成；那台并进公钥成了才记上钥匙（密码留着）；撤销摘掉钥匙；
 * 那台没并成 ⇒ 不动这台、照那一句报。夹具里的地址 · 公钥是结构占位。
 */
class KeySwitchTest {
    private fun key(
        id: String,
        at: Long,
        pub: String = "ssh-ed25519 AAAA-$id",
    ) = Identity(id = id, label = id, keyType = "ecdsa-p256", privateKeyEnc = ByteArray(0), publicKeyOpenSsh = pub, fingerprintSha256 = "", createdAt = at)

    private class Rig(
        keys: List<Identity>,
        val pushOk: Boolean = true,
    ) {
        var host = Host(id = "h1", label = "box", host = "192.0.2.1", username = "u", passwordEnc = "enc")
        val pushed = mutableListOf<String>()
        var generated = 0
        val switch =
            KeySwitch(
                hostOf = { id -> host.takeIf { it.id == id } },
                save = { host = it },
                keys = { keys },
                push = { _, k ->
                    pushed += k
                    if (pushOk) Result.success("ok") else Result.failure(IllegalStateException("那台拒了"))
                },
                generate = {
                    generated++
                    Identity(id = "new", label = "new", keyType = "ecdsa-p256", privateKeyEnc = ByteArray(0), publicKeyOpenSsh = "ssh-new", fingerprintSha256 = "", createdAt = 9)
                },
            )
    }

    @Test
    fun `有钥匙 ⇒ 用最早那一把，推成了才记上，密码留着`() =
        runBlocking {
            val r = Rig(listOf(key("b", 2), key("a", 1)))
            assertEquals(KeySwitch.Outcome.Switched("a"), r.switch.switch("h1"))
            assertEquals(listOf("ssh-ed25519 AAAA-a"), r.pushed)
            assertEquals("a", r.host.authRef)
            assertEquals("存的密码留着（给撤销）", "enc", r.host.passwordEnc)
            assertEquals(0, r.generated)
        }

    @Test
    fun `一把都没有 ⇒ 生成一把再推`() =
        runBlocking {
            val r = Rig(emptyList())
            assertEquals(KeySwitch.Outcome.Switched("new"), r.switch.switch("h1"))
            assertEquals(1, r.generated)
            assertEquals(listOf("ssh-new"), r.pushed)
        }

    @Test
    fun `那台没并成 ⇒ 这台不动，照那一句报；撤销 ⇒ 摘掉钥匙`() =
        runBlocking {
            val failed = Rig(listOf(key("a", 1)), pushOk = false)
            val o = failed.switch.switch("h1")
            assertTrue(o is KeySwitch.Outcome.Failed)
            assertEquals(copyText("mobKeyPush.result.failed"), (o as KeySwitch.Outcome.Failed).notice.text)
            assertEquals("那台拒了", o.notice.detail)
            assertNull("那台没收下公钥就记上钥匙 ⇒ 之后登不上", failed.host.authRef)

            val r = Rig(listOf(key("a", 1)))
            r.switch.switch("h1")
            r.switch.undo("h1")
            assertNull(r.host.authRef)
            assertEquals("enc", r.host.passwordEnc)
        }

    @Test
    fun `钥匙没有公钥 ⇒ 不推、不动这台`() =
        runBlocking {
            val r = Rig(listOf(key("a", 1, pub = "")))
            assertTrue(r.switch.switch("h1") is KeySwitch.Outcome.Failed)
            assertEquals(emptyList<String>(), r.pushed)
            assertNull(r.host.authRef)
        }
}
