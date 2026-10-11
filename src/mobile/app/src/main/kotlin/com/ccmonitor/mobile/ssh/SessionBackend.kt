package com.ccmonitor.mobile.ssh

/*
 * 会话后端：用户在 ssh 里自己会敲的那两条（进 / 建一个会话）由它造串，调用方不写 tmux 字面。
 * 方法只返回命令串、不执行；在 pty 还是 exec 旁路执行由调用方定。
 */

/** 会话后端的唯一入口，只有 [TmuxBackend] 一个实现。`ShellDialect.sessionBackend` 为 null（Windows）⇒ 没有。 */
interface SessionBackend {
    /** 连上之后「进某个会话」那一条（有则接、无则建），注进 pty。 */
    fun enter(
        sessionName: String,
        workingDir: String?,
    ): String

    /** 后台新建一个会话、不进入（exec 旁路）。 */
    fun newDetachedCommand(sessionName: String): String
}
