package com.ccmonitor.mobile.link

import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.Said
import com.ccmonitor.mobile.core.copy.copyText

/**
 * 屏上那一句 ＋ 复制详情。核心写好的失败（[CoreFailure]）原样：`message` 上屏、`detail` 进详情；
 * 常驻流走不通 · 通着但那台后端旧了，那一句也是核心写好的（[Said]，桌面同一套字），手机不另判、不另写。
 */
data class Problem(
    val text: String,
    val detail: String,
)

private fun core(f: CoreFailure) = Problem(f.message, f.detail)

private fun Said.problem() = Problem(text, detail)

/** 接一台走不通 ⇒ 核心说的那一句。 */
fun LinkState.Down.problem(): Problem = said.problem()

/** 通着、但那台后端旧了（手机还装不了后端，照用对得上的那几条）⇒ 那一句「需手动」；不旧 ⇒ `null`。 */
fun LinkState.Up.notice(): Problem? = notice?.problem()

/** 一问一答没答成 ⇒ 那一句；答成了 ⇒ `null`。[command] 是问的那条命令（解不出时进详情）。 */
fun Reply.problem(
    machine: String,
    command: String,
): Problem? =
    when (this) {
        is Reply.Ok -> null
        is Reply.Failed -> core(failure)
        Reply.TimedOut -> Problem(copyText("front.body.timeout", "machine" to machine), "")
        Reply.Cancelled, Reply.LinkDown -> Problem(copyText("front.body.offline", "machine" to machine), "")
        is Reply.Unreadable -> unreadableReply(machine, command, raw)
    }

/**
 * 回包解不出（帧上缺格 · 成品形状对不上，同一个说法）。握手过了 hello，那头就是 cc-monitor 后端，
 * 所以说「读取那台后端回复失败 · 内容无法解析」，不说「不是 cc-monitor 后端」；详情带命令名与原文。
 */
fun unreadableReply(
    machine: String,
    command: String,
    raw: String,
): Problem = Problem(copyText("peerVersion.said.unreadable", "machine" to machine), "command: $command\nreply: $raw")
