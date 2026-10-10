package com.ccmonitor.mobile.core.ssh

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** ssh config 解析。 */
class SshConfigParserTest {
    @Test
    fun `parses a full host block`() {
        val hosts =
            SshConfigParser.parse(
                """
                # my servers
                Host nano
                  HostName 198.51.100.36
                  Port 2222
                  User pi
                  IdentityFile ~/.ssh/id_ed25519
                """.trimIndent(),
            )
        assertEquals(1, hosts.size)
        val h = hosts[0]
        assertEquals("nano", h.alias)
        assertEquals("198.51.100.36", h.hostName)
        assertEquals(2222, h.port)
        assertEquals("pi", h.user)
        assertEquals("~/.ssh/id_ed25519", h.identityFile)
    }

    @Test
    fun `missing hostname falls back to alias, missing port defaults 22`() {
        val h = SshConfigParser.parse("Host box\n  User root").single()
        assertEquals("box", h.hostName)
        assertEquals(22, h.port)
        assertEquals("root", h.user)
        assertNull(h.identityFile)
    }

    @Test
    fun `multiple aliases on one Host line each become a host`() {
        val hosts = SshConfigParser.parse("Host a b c\n  HostName 198.51.100.1")
        assertEquals(listOf("a", "b", "c"), hosts.map { it.alias })
        assertTrue(hosts.all { it.hostName == "198.51.100.1" })
    }

    @Test
    fun `wildcard host blocks are skipped`() {
        val hosts =
            SshConfigParser.parse(
                """
                Host *
                  User default
                Host real
                  HostName 1.1.1.1
                """.trimIndent(),
            )
        assertEquals(listOf("real"), hosts.map { it.alias })
    }

    @Test
    fun `supports key=value and inline comments and blank lines`() {
        val hosts =
            SshConfigParser.parse(
                """

                Host eq
                Port=2022   # non-standard port
                HostName=example.com

                """.trimIndent(),
            )
        val h = hosts.single()
        assertEquals(2022, h.port)
        assertEquals("example.com", h.hostName)
    }

    @Test
    fun `two host blocks parse independently`() {
        val hosts = SshConfigParser.parse("Host a\n HostName 1.1.1.1\nHost b\n HostName 2.2.2.2\n User x")
        assertEquals(2, hosts.size)
        assertNull(hosts[0].user) // a 不应继承 b 的 User
        assertEquals("x", hosts[1].user)
    }

    @Test
    fun `empty or keyless input yields nothing`() {
        assertTrue(SshConfigParser.parse("").isEmpty())
        assertTrue(SshConfigParser.parse("# just a comment\n\n").isEmpty())
    }
}
