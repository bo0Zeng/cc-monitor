package com.ccmonitor.mobile.link

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.Failed
import com.ccmonitor.mobile.core.wire.gen.LinkLine

/**
 * 屏上那一句 ＋ 复制详情。核心写好的失败（[Failed]）原样：`said` 上屏、`detail` 进详情；
 * 常驻流走不通 · 通着但那台后端旧了，那一句也是核心写好的（那台状态成品的 `said`，桌面同一套字），手机不另判、不另写。
 */
data class Problem(
    val text: String,
    val detail: String,
)

private fun core(f: Failed) = Problem(f.said, f.detail)

/** 那台状态成品的那一句 ＋ 复制详情（接一台走不通 ⇒ 核心说的那一句）。 */
fun LinkLine.problem(): Problem = Problem(machine.said, machine.detail.orEmpty())

/** 通着、但那台旧了 · 较新（照用对得上的那几条）⇒ 成品那一句；成品不出问题行 ⇒ `null`。 */
fun LinkLine.Up.notice(): Problem? = problem().takeIf { it.text.isNotEmpty() }

/**
 * 一问一答没答成 ⇒ 那一句；答成了 ⇒ `null`。[command] 是问的那条命令（解不出时进详情）。
 * [offline] 是那条链上那台成品写的离线那一句（[CoreLink.offline]）：断着就照摆它；成品还没到 ⇒ 状态不明（未知不当离线）。
 */
fun Reply.problem(
    machine: String,
    command: String,
    offline: String?,
): Problem? =
    when (this) {
        is Reply.Ok -> null
        is Reply.Failed -> core(failure)
        Reply.TimedOut -> Problem(copyText("front.body.timeout", "machine" to machine), "")
        Reply.Cancelled -> Problem(copyText("reason.op.cancelled"), "")
        Reply.LinkDown -> Problem(offline ?: copyText("sessionState.unseen.name"), "")
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
