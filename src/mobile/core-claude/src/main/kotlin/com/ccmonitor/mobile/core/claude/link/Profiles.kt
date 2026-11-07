package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.Commands

/**
 * 那台的别名（配置文件里的各段）：手机只读（与桌面别名页同一条命令），起会话时挑一段照它那一行起。
 * 一段能不能用、合下来是什么都是那台判的。
 */
object Profiles {
    const val READ: String = Commands.ProfilesRead.NAME

    /** 能用的一段：名字 · 在终端里起它的那一行（那台写好的，照敲）。 */
    data class Alias(
        val name: String,
        val start: String,
    )

    /** 读回的那一份里手机用的那一样：能用的那几段（按那台的顺序）。 */
    data class Book(
        val usable: List<Alias>,
    )

    fun of(data: Any?): Book? = Commands.ProfilesRead.reply(data)?.let { b -> Book(b.profiles.filter { it.usable }.map { Alias(it.name, it.start) }) }
}
