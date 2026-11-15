package com.ccmonitor.mobile.ui.host

import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.ssh.machineNames
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 机器列表每一行的名字（行标题 · 撤销提示 · ⋯ 的无障碍字 · ⋯ 菜单标题都用它）是核心照整张表取的那一个（[machineNames]），
 * 不是 `Host.label` 原样：label 空着的那台照样有名字。
 */
class HostListNamesTest {
    private fun h(
        id: String,
        label: String,
        host: String,
        group: String? = null,
    ) = Host(id = id, label = label, host = host, username = "u", groupId = group)

    @Test
    fun `label 空着那台的行名字是核心取的、不是空串`() {
        val hosts = listOf(h("a", "", "a.example"), h("b", "box", "b.example"))
        val rows = hostGroups(hosts).flatMap { it.second }
        val core = machineNames(hosts)
        assertTrue("前提：核心给 label 空着那台取的名字不该是空串", core[0].isNotBlank())
        assertEquals(hosts.map { it.id }.zip(core), rows.map { it.host.id to it.name })
    }

    @Test
    fun `分组后名字还跟着那一台`() {
        val hosts = listOf(h("a", "", "a.example"), h("b", "box", "b.example", group = "g"))
        val names = hosts.map { it.id }.zip(machineNames(hosts)).toMap()
        val groups = hostGroups(hosts)
        assertEquals(listOf("g", null), groups.map { it.first })
        groups.flatMap { it.second }.forEach { assertEquals(names[it.host.id], it.name) }
    }
}
