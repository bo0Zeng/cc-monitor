package com.ccmonitor.mobile.link

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.Failed

/**
 * 屏上那一句 ＋ 复制详情。核心写好的失败（[Failed]）原样：`said` 上屏、`detail` 进详情；
 * 常驻流走不通 · 通着但那台后端旧了，那一句也是核心写好的（那台状态成品的 `said`，桌面同一套字），手机不另判、不另写。
 */
data class Problem(
    val text: String,
    val detail: String,
)

private fun core(f: Failed) = Problem(f.said, f.detail)

/**
 * 一问一答没答成 ⇒ 那一句；答成了 ⇒ `null`。[command] 是问的那条命令（解不出时进详情）。
 * 期限到 · 撤了：核心照通道那一份写好的那一句与详情原样（桌面同一句），手机不另取字。
 * [offline] 是那条链上的离线那一句（[CoreLink.offline]，恒有）：断着就照摆它。
 */
fun Reply.problem(
    machine: String,
    command: String,
    offline: String,
): Problem? =
    when (this) {
        is Reply.Ok -> null
        is Reply.Failed -> core(failure)
        is Reply.TimedOut -> Problem(said, detail)
        is Reply.Cancelled -> Problem(said, detail)
        Reply.LinkDown -> Problem(offline, "")
        is Reply.Unreadable -> unreadableReply(machine, command, raw)
    }

/**
 * 回包解不出（帧上缺格 · 成品形状对不上，同一个说法）。握手过了 hello，那头就是 cc-monitor 后端，
 * 所以说「读取那台后端回复失败 · 内容无法解析」，不说「不是 cc-monitor 后端」；详情带命令名与原文（出［复制详情］，桌面同样带认不出的那一处）。
 * 手机取这一句只这一个口（界面那一侧是 `chan-caller.ts::unreadableSaid`，同一把键）。
 */
fun unreadableReply(
    machine: String,
    command: String,
    raw: String,
): Problem = Problem(copyText("peerVersion.said.unreadable", "machine" to machine), "command: $command\nreply: $raw")
