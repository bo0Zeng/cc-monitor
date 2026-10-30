package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.AccountNameArgs
import com.ccmonitor.mobile.core.wire.gen.AccountsListReq
import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.core.wire.gen.DirChecked
import com.ccmonitor.mobile.core.wire.gen.LoginCancelReq
import com.ccmonitor.mobile.core.wire.gen.LoginFinishReq
import com.ccmonitor.mobile.core.wire.gen.LoginStartArgs
import com.ccmonitor.mobile.core.wire.gen.LoginStartReq
import com.ccmonitor.mobile.core.wire.gen.LoginStarted
import com.ccmonitor.mobile.core.wire.gen.OpenDirReq
import com.ccmonitor.mobile.core.wire.gen.RotationStrategiesReply
import com.ccmonitor.mobile.core.wire.gen.AccountsList as WireAccounts

/*
 * 新建会话那几屏问那台的几条（`IPC-COMMANDS.md` 各节）：手机只拼入参、解成品，默认值 · 能不能选 · 在不在都是那台的事实。
 */

/** `session-open-dir`：起之前核一个目录在不在（开头的 `~` 那台按它的家目录读）。 */
object SessionOpenDir {
    const val COMMAND: String = Commands.SessionOpenDir.NAME

    fun args(cwd: String): Map<String, Any?> = Commands.SessionOpenDir.args(OpenDirReq(cwd = cwd))

    /** 在不在 ＋ 屏上那一串（那台按它的家目录写好的 `~/…`）；形状对不上 ⇒ `null`。 */
    fun of(data: Any?): DirChecked? = Commands.SessionOpenDir.reply(data)
}

/**
 * `accounts-list`：这台账号库里的号（跟着起哪一家问），按 `cmd-wire` 那一份解过。号牌字 · 5h 那一格 · 选不了的那一句都照那台写好的画。
 * 只列那台说进选号表的那几个（`newSession`，同桌面新建会话框）。
 */
object AccountsList {
    const val COMMAND: String = Commands.AccountsList.NAME

    fun args(agent: String): Map<String, Any?> = Commands.AccountsList.args(AccountsListReq(agent))

    fun of(data: Any?): WireAccounts? = Commands.AccountsList.reply(data)?.let { l -> l.copy(accounts = l.accounts.filter { it.newSession }) }
}

/** `accounts-set-default`：设这台的新会话默认号。 */
object AccountsSetDefault {
    const val COMMAND: String = Commands.AccountsSetDefault.NAME

    fun args(name: String): Map<String, Any?> = Commands.AccountsSetDefault.args(AccountNameArgs(name = name))
}

/** `rotation-strategies`：这台的轮换策略目录（每份 id · 说明 · 是不是「新会话用」的那份）。 */
object RotationStrategies {
    const val COMMAND: String = Commands.RotationStrategies.NAME

    fun of(data: Any?): RotationStrategiesReply? = Commands.RotationStrategies.reply(data)
}

/** 在 app 里登录一个号：`login-start` 起那一家的登录、交回链接；粘回来的那一段经 `login-finish` 交给它；`login-cancel` 收。 */
object Login {
    const val START: String = Commands.LoginStart.NAME
    const val FINISH: String = Commands.LoginFinish.NAME
    const val CANCEL: String = Commands.LoginCancel.NAME

    /** 起一趟登录：入参就是目标原样（会话外那一项的 `fix.args`，或 [account] 拼的那一形）；手机不带 `browserHere`（后端缺省答不在）。 */
    fun start(target: LoginStartReq): Map<String, Any?> =
        Commands.LoginStart.args(
            when (target) {
                is LoginStartReq.Account -> LoginStartArgs.Account(agent = target.agent, account = target.account)
                is LoginStartReq.Mcp -> LoginStartArgs.Mcp(agent = target.agent, account = target.account, server = target.server, projectDir = target.projectDir)
            },
        )

    /** 号那一形的目标（新建会话 · 设置账号那边拼：同 `fix.args` 那一形）。 */
    fun account(
        agent: String,
        account: String,
    ): LoginStartReq = LoginStartReq.Account(agent = agent, account = account)

    fun finish(
        job: String,
        pasted: String,
    ): Map<String, Any?> = Commands.LoginFinish.args(LoginFinishReq(job = job, pasted = pasted))

    fun cancel(job: String): Map<String, Any?> = Commands.LoginCancel.args(LoginCancelReq(job = job))

    fun started(data: Any?): LoginStarted? = Commands.LoginStart.reply(data)
}
