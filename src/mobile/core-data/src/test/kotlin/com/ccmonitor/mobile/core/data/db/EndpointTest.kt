package com.ccmonitor.mobile.core.data.db

import org.junit.Assert.assertEquals
import org.junit.Test

class EndpointTest {
    private fun host(
        h: String,
        p: Int,
        extra: String?,
    ) =
        Host(id = "x", label = "x", host = h, port = p, username = "u", extraAddresses = extra)

    @Test fun primaryOnlyWhenNoExtras() {
        assertEquals(listOf(Endpoint("192.0.2.2", 22)), host("192.0.2.2", 22, null).endpoints())
    }

    @Test fun extrasParsedHostAndHostPort() {
        val eps = host("192.0.2.2", 22, "198.51.100.1\nwan.example.com:2222").endpoints()
        assertEquals(
            listOf(Endpoint("192.0.2.2", 22), Endpoint("198.51.100.1", 22), Endpoint("wan.example.com", 2222)),
            eps,
        )
    }

    @Test fun blankLinesAndWhitespaceSkipped() {
        val eps = host("a", 22, "  \n b:2200 \n\n").endpoints()
        assertEquals(listOf(Endpoint("a", 22), Endpoint("b", 2200)), eps)
    }

    @Test fun badPortFallsBackToPrimaryPort() {
        assertEquals(
            listOf(Endpoint("a", 22), Endpoint("b", 22)),
            host("a", 22, "b:notaport").endpoints(),
        )
        assertEquals(
            listOf(Endpoint("a", 22), Endpoint("c", 22)),
            host("a", 22, "c:99999").endpoints(), // 越界端口回退
        )
    }

    @Test fun dedupByHostPort() {
        // 主地址与某额外地址重复 → 去重，保留首次（主）
        val eps = host("a", 22, "a:22\nb:22\na").endpoints()
        assertEquals(listOf(Endpoint("a", 22), Endpoint("b", 22)), eps)
    }

    @Test fun ipv6BracketedWithAndWithoutPort() {
        val eps = host("a", 22, "[2001:db8::1]:2222\n[fe80::2]").endpoints()
        assertEquals(
            listOf(Endpoint("a", 22), Endpoint("2001:db8::1", 2222), Endpoint("fe80::2", 22)),
            eps,
        )
    }

    @Test fun bareIpv6TreatedAsHostWithPrimaryPort() {
        // 无方括号的裸 IPv6：末段冒号不当端口，整体作 host
        val eps = host("a", 22, "fe80::1\n[::1]:2200").endpoints()
        assertEquals(
            listOf(Endpoint("a", 22), Endpoint("fe80::1", 22), Endpoint("::1", 2200)),
            eps,
        )
    }

    // parseAddressLine 是 endpoints() 与编辑器预览共用的解析规则，直测各形态
    @Test fun parseAddressLineForms() {
        assertEquals(Endpoint("h", 22), parseAddressLine("h", 22))
        assertEquals(Endpoint("h", 2222), parseAddressLine("h:2222", 22))
        assertEquals(Endpoint("h", 22), parseAddressLine("h:bad", 22)) // 坏端口回退
        assertEquals(Endpoint("h", 22), parseAddressLine("h:70000", 22)) // 越界回退
        assertEquals(Endpoint("::1", 2200), parseAddressLine("[::1]:2200", 22))
        assertEquals(Endpoint("fe80::1", 22), parseAddressLine("fe80::1", 22)) // 裸 IPv6
        assertEquals(Endpoint("h", 22), parseAddressLine("  h  ", 22)) // 去空白
    }

    @Test fun parseAddressLineEmptyOrBlankIsNull() {
        assertEquals(null, parseAddressLine("", 22))
        assertEquals(null, parseAddressLine("   ", 22))
    }
}
