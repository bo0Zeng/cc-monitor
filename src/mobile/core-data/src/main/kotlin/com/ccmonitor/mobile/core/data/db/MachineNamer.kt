package com.ccmonitor.mobile.core.data.db

/**
 * 每台机器的名字（跳板指的就是它），与 [names] 收的那几台一一对应。名字由拨号核心算（同桌面那张表）；
 * core-data 只声明这一口，app 绑核心那一份（`machineNames`），23→24 迁移经注入拿它（[Migration23To24]）。
 */
fun interface MachineNamer {
    fun names(hosts: List<Host>): List<String>
}
