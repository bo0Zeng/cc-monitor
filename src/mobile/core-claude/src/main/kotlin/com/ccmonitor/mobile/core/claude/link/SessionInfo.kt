package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.core.wire.gen.SessionAccountReply
import com.ccmonitor.mobile.core.wire.gen.SessionAccountReq
import com.ccmonitor.mobile.core.wire.gen.TasksList
import com.ccmonitor.mobile.core.wire.gen.TasksListReq

/*
 * 「会话信息」抽屉读的两问：这条会话跑在哪个号下、那一号的额度（`session-account`）· 它的任务清单（`tasks-list`）。
 * 字都是那台写好的（`accountLine` · `chip` · `hint` · 每条的 `statusText`），手机只拼入参、解成品。
 */

/** `session-account`：这条会话 ⇒ 号 ＋ 额度那一句（`accountLine`；判不了号 ⇒ `null`）。 */
object SessionAccount {
    const val COMMAND: String = Commands.SessionAccount.NAME

    fun args(sid: String): Map<String, Any?> = Commands.SessionAccount.args(SessionAccountReq(sid = sid))

    fun of(data: Any?): SessionAccountReply? = Commands.SessionAccount.reply(data)
}

/** `tasks-list`：这条会话的任务（每条带状态字）＋ 整份的计数那一格（`chip`）与那一句（`hint`）。 */
object SessionTasks {
    const val COMMAND: String = Commands.TasksList.NAME

    fun args(sid: String): Map<String, Any?> = Commands.TasksList.args(TasksListReq(sid = sid))

    fun of(data: Any?): TasksList? = Commands.TasksList.reply(data)
}
