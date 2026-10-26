package com.ccmonitor.mobile.ui.host

import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.wire.gen.Endpoint
import org.junit.Assert.assertEquals
import org.junit.Test

/** 删一台时忘哪几把主机钥匙：拨号地址用假的（真的那份是核心 `machineCheck` 算的，要 .so）。 */
class HostRemovalKeysTest {
    private fun host(
        id: String,
        addr: String,
    ) = Host(id = id, label = id, host = addr, username = id)

    /** 假的拨号地址：主地址 22 口 ＋ 备用地址那几行（这里不解析，原样当主机名、22 口）。 */
    private val endpoints: (List<Host>, Int) -> List<Endpoint> = { table, at ->
        table[at].let { h -> (listOf(h.host) + h.addresses).map { Endpoint(it, 22) } }
    }

    @Test
    fun onlyTheAddressesNoOtherMachineUsesAreForgotten() {
        val table =
            listOf(
                host("a", "192.0.2.1").copy(addresses = listOf("a.lan")),
                // 同址另一个用户：192.0.2.1 那把钥匙它还用着。
                host("b", "192.0.2.1"),
                host("c", "192.0.2.3"),
            )
        assertEquals(listOf(Endpoint("a.lan", 22)), keysOnlyItUsed(table, "a", endpoints))
    }

    @Test
    fun theLastMachineOnAnAddressTakesItsKeysWithIt() {
        val table = listOf(host("a", "192.0.2.1").copy(addresses = listOf("a.lan")), host("c", "192.0.2.3"))
        assertEquals(listOf(Endpoint("192.0.2.1", 22), Endpoint("a.lan", 22)), keysOnlyItUsed(table, "a", endpoints))
    }

    @Test
    fun aMachineNotInTheTableForgetsNothing() {
        assertEquals(emptyList<Endpoint>(), keysOnlyItUsed(listOf(host("c", "192.0.2.3")), "a", endpoints))
    }
}
