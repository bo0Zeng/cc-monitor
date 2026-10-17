package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.DirChecked
import com.ccmonitor.mobile.core.claude.link.wire.LoginStarted
import com.ccmonitor.mobile.core.claude.link.wire.RotationStrategiesReply
import com.ccmonitor.mobile.core.claude.link.wire.Wire
import com.ccmonitor.mobile.core.claude.link.wire.AccountsList as WireAccounts

/*
 * 新建会话那几屏问那台的几条（`IPC-COMMANDS.md` 各节）：手机只拼入参、解成品，默认值 · 能不能选 · 在不在都是那台的事实。
 */

/** `session-open-dir`：起之前核一个目录在不在（开头的 `~` 那台按它的家目录读）。 */
object SessionOpenDir {
    const val COMMAND: String = "session-open-dir"

    fun args(cwd: String): Map<String, Any?> = mapOf("cwd" to cwd)

    /** 在不在；形状对不上 ⇒ `null`。 */
    fun of(data: Any?): Boolean? = Wire.bind<DirChecked>(data)?.exists
}

/**
 * `accounts-list`：这台账号库里的号（跟着起哪一家问），按 `cmd-wire` 那一份解过。号牌字 · 5h 那一格 · 选不了的那一句都照那台写好的画。
 * 只列隔离的那几个（账号 0 不经选号，同桌面新建会话框）。
 */
object AccountsList {
    const val COMMAND: String = "accounts-list"

    fun args(agent: String): Map<String, Any?> = mapOf("agent" to agent)

    fun of(data: Any?): WireAccounts? = Wire.bind<WireAccounts>(data)?.let { l -> l.copy(accounts = l.accounts.filter { it.configDir != null && it.mode == ISOLATED }) }

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
