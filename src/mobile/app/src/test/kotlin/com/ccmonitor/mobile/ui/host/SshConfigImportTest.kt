package com.ccmonitor.mobile.ui.host

import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.ssh.SshConfigHost
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * ssh config 导入的去重判据。
 *
 * `HostRepository.saveAll` 是 upsert，所以导入造的 id 必须由配置本身决定：
 * 若每次都新造一个随机 id，同一份配置导两次就会生出两套机器，
 * 「从已有那台导入机器清单、第二台起 0 个字」的价值也就没了。
 */
class SshConfigImportTest {
    private val cfg =
        listOf(
            SshConfigHost(alias = "devbox", hostName = "192.0.2.20", port = 22, user = "user", identityFile = "~/.ssh/id_ed25519"),
            SshConfigHost(alias = "box", hostName = "example.com", port = 2222, user = null, identityFile = null),
        )
    private val keys =
        listOf(
            Identity(
                id = "k1",
                label = "id_ed25519",
                keyType = "ed25519",
                privateKeyEnc = ByteArray(0),
                publicKeyOpenSsh = "ssh-ed25519 AAAA",
                fingerprintSha256 = "SHA256:x",
                createdAt = 0L,
            ),
        )

    @Test
    fun importingTheSameConfigTwiceDoesNotDuplicate() {
        val first = hostsFromSshConfig(cfg, keys)
        val second = hostsFromSshConfig(cfg, keys)
        assertEquals(
            "同一份配置两次导入必须产出同一批 id（saveAll 是 upsert ⇒ id 稳定才不会生两套）",
            first.map { it.id },
            second.map { it.id },
        )
    }

    @Test
    fun idIsDerivedFromAliasNotRandom() =
        assertEquals("cfg-devbox", hostIdForSshAlias("devbox"))

    @Test
    fun aliasIsTrimmedSoStrayWhitespaceDoesNotForkTheId() =
        assertEquals(
            "别名两侧空白不该派生出第二台机器",
            hostIdForSshAlias("devbox"),
            hostIdForSshAlias("  devbox  "),
        )

    @Test
    fun handBuiltHostsAreNeverOverwritten() =
        assertTrue(
            "导入造的 id 一律带前缀，手工建的不带 ⇒ upsert 永远碰不到用户自己建的那些",
            hostsFromSshConfig(cfg, keys).all { it.id.startsWith(SSH_CONFIG_HOST_ID_PREFIX) },
        )

    @Test
    fun identityIsMatchedByIdentityFileBasename() {
        val hosts = hostsFromSshConfig(cfg, keys)
        assertEquals("带 IdentityFile 的那台按 basename 认到钥匙", "k1", hosts[0].authRef)
        assertEquals("没有 IdentityFile 的那台不认", null, hosts[1].authRef)
    }
}
