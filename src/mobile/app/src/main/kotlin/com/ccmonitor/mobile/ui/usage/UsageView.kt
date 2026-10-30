package com.ccmonitor.mobile.ui.usage

import com.ccmonitor.mobile.core.wire.gen.AccountRow
import com.ccmonitor.mobile.core.wire.gen.QuotaCell
import com.ccmonitor.mobile.core.wire.gen.QuotaKind
import com.ccmonitor.mobile.core.wire.gen.QuotaReadState
import com.ccmonitor.mobile.core.wire.gen.QuotaVerdict
import com.ccmonitor.mobile.core.wire.gen.SlotShow
import com.ccmonitor.mobile.core.wire.gen.Usage
import com.ccmonitor.mobile.core.wire.gen.QuotaRead as QuotaReadReply

/**
 * 用量屏要画的那一份：结论两行（[verdict]）或那台那一句（[text]，没记过账 · 一个号都没有 · 读不出）· 每号一张卡（照回包先后）。
 * 读不出 ⇒ [failed] 带复制详情（那一句画成红条）。
 */
data class UsageUi(
    val verdict: QuotaVerdict?,
    val text: String?,
    val failed: Failed?,
    val cards: List<UsageCard>,
) {
    data class Failed(
        val detail: String,
    )
}

/**
 * 一张卡。卡头 [head] 是那台写好的第一行（名 · 类型 · 需登录）；号牌对上账号库那一行（对不上 ⇒ `null`，不画号牌）。
 * 收起时画 [bars]（订阅号出过数）或 [usage]（按量号那一格）或 [plain]（没出过数的那几行）；最后 [seen] 采样那一行。
 * 点开画 [detail]（第一行之后的全部行）；没有可展开的 ⇒ 空。
 */
data class UsageCard(
    val account: String,
    val plate: String?,
    val color: Int?,
    val head: List<QuotaCell>,
    val isDefault: Boolean,
    val bars: List<UsageBar>,
    val usage: Usage?,
    val plain: List<List<QuotaCell>>,
    val seen: List<QuotaCell>?,
    val detail: List<List<QuotaCell>>,
)

/** 一条计量条：位名 · 条长（0..1；那台说不出百分比 ⇒ `null`，不当 0）· 那一格 · 数旧（斜纹）。 */
data class UsageBar(
    val label: String,
    val fill: Float?,
    val slot: SlotShow,
    val stale: Boolean,
)

/**
 * 回包 ⇒ 要画的那一份：只按号名对上两份回包（号牌 · 默认），次序 · 字 · 语气全照回包。
 * 位名照同一份回包的 `names.slots`；有一格的语义位不在里面 ⇒ 两端契约对不上，交 `null`（由屏照「回包读不出」画），不拿码当名字。
 */
fun usageView(
    reply: QuotaReadReply,
    accounts: List<AccountRow>,
    defaultAccount: String?,
): UsageUi? {
    val names = reply.names.slots
    if (reply.accounts.any { q -> q.slots.any { it.slot !in names } }) return null
    val byName = accounts.associateBy { it.name }

    fun card(
        account: String,
        rows: List<List<QuotaCell>>,
        build: (UsageCard) -> UsageCard,
    ): UsageCard {
        val a = byName[account]
        return build(
            UsageCard(account, a?.plate, a?.color, rows.firstOrNull().orEmpty(), account == defaultAccount, emptyList(), null, emptyList(), null, emptyList()),
        )
    }
    val seen =
        reply.accounts.map { q ->
            card(q.account, q.rows) { c ->
                val more = q.rows.drop(1)
                when {
                    q.kind == QuotaKind.API -> c.copy(usage = q.usage, detail = more)
                    q.slots.isEmpty() -> c.copy(plain = more, detail = more)
                    else ->
                        c.copy(
                            bars = q.slots.map { UsageBar(names.getValue(it.slot), it.pct?.let { p -> p.coerceIn(0, FULL) / FULL.toFloat() }, it, q.stale) },
                            seen = more.lastOrNull(),
                            detail = more,
                        )
                }
            }
        }
    val unseen = reply.unseen.map { u -> card(u.account, u.rows) { it.copy(plain = u.rows.drop(1)) } }
    val failed = if (reply.state == QuotaReadState.UNREADABLE) UsageUi.Failed(reply.detail.orEmpty()) else null
    return UsageUi(reply.verdict, reply.text.takeIf { reply.verdict == null }, failed, seen + unseen)
}

private const val FULL = 100
