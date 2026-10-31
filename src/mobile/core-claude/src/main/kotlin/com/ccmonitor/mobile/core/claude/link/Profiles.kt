package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.core.wire.gen.ProfileForm
import com.ccmonitor.mobile.core.wire.gen.ProfileOp
import com.ccmonitor.mobile.core.wire.gen.ProfilesFromLineReq
import com.ccmonitor.mobile.core.wire.gen.ProfilesWriteReq

/**
 * 那台的别名（配置文件里的各段）：读 · 新建一段，与桌面别名页同两条命令。手机只交意图；
 * 一段能不能用、合下来是什么、名字合不合规都是那台判的。
 */
object Profiles {
    const val READ: String = Commands.ProfilesRead.NAME
    const val WRITE: String = Commands.ProfilesWrite.NAME
    const val FROM_LINE: String = Commands.ProfilesFromLine.NAME

    /** 能用的一段：名字 · 在终端里起它的那一行（那台写好的，照敲）。 */
    data class Alias(
        val name: String,
        val start: String,
    )

    /** 读回的那一份里手机用的两样：盘上那份的版本（写的时候当 `ifRev` 交回）· 能用的那几段（按那台的顺序）。 */
    data class Book(
        val rev: String?,
        val usable: List<Alias>,
    )

    fun of(data: Any?): Book? =
        Commands.ProfilesRead.reply(data)?.let { b -> Book(b.rev, b.profiles.filter { it.usable }.map { Alias(it.name, it.start) }) }

    /** 一条自由命令行折成别名表单（那台判：认不认这条命令、ccm 选项怎么折）；回包的表单原样交 [createArgs]。 */
    fun fromLineArgs(
        name: String,
        line: String,
        cwd: String?,
        sessionName: String?,
    ): Map<String, Any?> = Commands.ProfilesFromLine.args(ProfilesFromLineReq(name = name, line = line, cwd = cwd, sessionName = sessionName))

    /** [FROM_LINE] 的回包：别名页表单那一形。形状对不上 ⇒ `null`。 */
    fun formOf(data: Any?): ProfileForm? = Commands.ProfilesFromLine.reply(data)

    /** 新建一段（[form] 是别名页表单那一形）；[ifRev] 是刚读回的那一份的版本，盘上被别处改过 ⇒ 那台回 `stale`。 */
    fun createArgs(
        ifRev: String?,
        form: ProfileForm,
    ): Map<String, Any?> = Commands.ProfilesWrite.args(ProfilesWriteReq(changes = listOf(ProfileOp.Set(was = null, form = form)), ifRev = ifRev))

    /** 写完：终端函数那份真改了 ⇒ 「已开的终端要重读」那一句（那台写好的）；否则 `null`。形状对不上 ⇒ 解不出。 */
    fun written(data: Any?): Written? = Commands.ProfilesWrite.reply(data)?.let { Written(it.reload) }

    data class Written(
        val reload: String?,
    )
}
