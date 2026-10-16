package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.DirChecked
import com.ccmonitor.mobile.core.claude.link.wire.LoginStarted
import com.ccmonitor.mobile.core.claude.link.wire.RotationStrategiesReply
import com.ccmonitor.mobile.core.claude.link.wire.Wire

/*
 * 新建会话那几屏问那台的几条（`IPC-COMMANDS.md` 各节）：手机只拼入参、解成品，默认值 · 能不能选 · 在不在都是那台的事实。
 */

/** `session-new-dir`：起之前核一个目录在不在（开头的 `~` 那台按它的家目录读）。 */
object SessionNewDir {
    const val COMMAND: String = "session-new-dir"

    fun args(cwd: String): Map<String, Any?> = mapOf("cwd" to cwd)

    /** 在不在；形状对不上 ⇒ `null`。 */
    fun of(data: Any?): Boolean? = Wire.bind<DirChecked>(data)?.exists
}

/** `accounts-list`：这台账号库里的号（跟着起哪一家问）。只列隔离的那几个（账号 0 不列，同桌面新建会话框）。 */
object AccountsList {
    const val COMMAND: String = "accounts-list"

    fun args(agent: String): Map<String, Any?> = mapOf("agent" to agent)

    data class Account(
        val name: String,
        /** 号在人眼里叫什么（那台写的 `label`；旧后端没有这一格 ⇒ 号名）。 */
        val label: String,
        /** 号色第几格（`--acct-c0..c7`，账号库里的事实）；没分到 ⇒ `null`。 */
        val color: Int?,
        /** 能拿来起会话（那台判的）。 */
        val selectable: Boolean,
    )

    data class Answer(
        val accounts: List<Account>,
        /** 这台的新会话默认号（那台判的）；没有 ⇒ `null`。 */
        val effectiveDefault: String?,
        /** 这台的家目录（路径里缩成 `~` 用）；推不出 ⇒ `null`。 */
        val home: String?,
    )

    fun of(data: Any?): Answer? =
        decoding {
            val m = data.asObj()
            val meta = m.obj("meta").need()
            val accounts =
                m
                    .objs("accounts")
                    .need()
                    .onEach { a -> a.str("name").need() }
                    .filter { it.str("configDir") != null && it.str("mode") == ISOLATED }
                    .map { a -> Account(a.str("name").need(), a.str("label") ?: a.str("name").need(), a.num("color")?.toInt(), a.bool("selectable") == true) }
            Answer(accounts, meta.str("effectiveDefault"), meta.str("home"))
        }

    private const val ISOLATED = "isolated"
}

/** `accounts-set-default`：设这台的新会话默认号。 */
object AccountsSetDefault {
    const val COMMAND: String = "accounts-set-default"

    fun args(name: String): Map<String, Any?> = mapOf("name" to name)
}

/** `rotation-strategies`：这台的轮换策略目录（每份 id · 说明 · 是不是「新会话用」的那份）。 */
object RotationStrategies {
    const val COMMAND: String = "rotation-strategies"

    fun of(data: Any?): RotationStrategiesReply? = Wire.bind<RotationStrategiesReply>(data)
}

/** 在 app 里登录一个号：`login-start` 起那一家的登录、交回链接；粘回来的那一段经 `login-finish` 交给它；`login-cancel` 收。 */
object Login {
    const val START: String = "login-start"
    const val FINISH: String = "login-finish"
    const val CANCEL: String = "login-cancel"

    fun startAccount(
        agent: String,
        account: String,
    ): Map<String, Any?> = mapOf("kind" to "account", "agent" to agent, "account" to account)

    fun finish(
        job: String,
        pasted: String,
    ): Map<String, Any?> = mapOf("job" to job, "pasted" to pasted)

    fun cancel(job: String): Map<String, Any?> = mapOf("job" to job)

    fun started(data: Any?): LoginStarted? = Wire.bind<LoginStarted>(data)
}
