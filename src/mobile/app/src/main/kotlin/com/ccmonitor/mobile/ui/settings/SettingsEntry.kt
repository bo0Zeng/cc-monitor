package com.ccmonitor.mobile.ui.settings

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.ui.nav.Screen

/** 行尾那个数从哪数（手机自己的机器表 · 保险箱）。 */
enum class EntryCount {
    MACHINES,
    KEYS,
}

/**
 * 「机器与账号」那一节里一条可点的去处。
 *
 * @property route 目的地路由，必须是 [Screen] 里真有的那一条。
 * @property readsMachine 那一屏读当前那台：那台离线 · 要更新时这一行灰着、点不了。
 */
data class SettingsEntry(
    val icon: Ph,
    val title: String,
    val route: String,
    val readsMachine: Boolean,
    val count: EntryCount? = null,
)

/**
 * 「机器与账号」那几条的唯一住址，设置屏只渲染这份数据。
 * 终端从抽屉进、身份从密钥进：每个功能只有一处入口。
 */
fun settingsEntries(): List<SettingsEntry> =
    listOf(
        SettingsEntry(Ph.HardDrives, copyText("mobSettings.entry.machines"), Screen.Hosts.route, readsMachine = false, count = EntryCount.MACHINES),
        SettingsEntry(Ph.UserCircle, copyText("mobSettings.entry.accounts"), Screen.Accounts.route, readsMachine = true),
        // 轮换看的是当前那台新会话用的那份。
        SettingsEntry(Ph.ArrowCounterClockwise, copyText("mobSettings.entry.rotation"), Screen.RotationHere.route, readsMachine = true),
        SettingsEntry(Ph.Key, copyText("mobSettings.entry.keys"), Screen.Identities.route, readsMachine = false, count = EntryCount.KEYS),
    )
