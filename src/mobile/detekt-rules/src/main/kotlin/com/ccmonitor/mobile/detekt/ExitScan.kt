package com.ccmonitor.mobile.detekt

import com.intellij.psi.util.PsiTreeUtil
import org.jetbrains.kotlin.psi.KtBinaryExpression
import org.jetbrains.kotlin.psi.KtBlockExpression
import org.jetbrains.kotlin.psi.KtCallExpression
import org.jetbrains.kotlin.psi.KtClassOrObject
import org.jetbrains.kotlin.psi.KtDotQualifiedExpression
import org.jetbrains.kotlin.psi.KtElement
import org.jetbrains.kotlin.psi.KtExpression
import org.jetbrains.kotlin.psi.KtFile
import org.jetbrains.kotlin.psi.KtIfExpression
import org.jetbrains.kotlin.psi.KtNameReferenceExpression
import org.jetbrains.kotlin.psi.KtNamedFunction
import org.jetbrains.kotlin.psi.KtParenthesizedExpression
import org.jetbrains.kotlin.psi.KtProperty
import org.jetbrains.kotlin.psi.KtStringTemplateExpression
import org.jetbrains.kotlin.psi.KtWhenConditionWithExpression
import org.jetbrains.kotlin.psi.KtWhenEntry
import org.jetbrains.kotlin.psi.KtWhenExpression
import org.jetbrains.kotlin.psi.psiUtil.collectDescendantsOfType

/**
 * 出口扫描判据的两句，Kotlin 那一半（同 `tests/frontend/ui/exit-rules.ts`）：
 * - `key`：取的文案键（`copyText("…", …)`）只许骨架那几档（文案表 `kind` 的 title · control · action · aria · body；error 是结局、由后端交成品），且那一条不带插值；
 *   键不是字面量、表里没有这把键，也算一处。
 * - `branch`：条件读了线上值的分支里不许取文。「读了线上值」＝ 条件拿一个值去比字面的码（`x == "dup"` · `x in CODES` ·
 *   `CODES.contains(x)`，`CODES` 是本文件里一组字面码的 `setOf` / `listOf` / `arrayOf`），或 `when (x)` 的臂里有字面码（那个 `when` 的每一臂都算）。
 *   「取文」：分支里的 `copyText(…)`、带汉字的字面量，以及分支的结果直接是一个枚举常量（`DeliveryState.FAILED` 这一形：手机按码挑的多是枚举）。
 *   进到里面再定义的具名函数、类就不算；lambda 跟着外面的分支算。
 *
 * 买不到：比的码不是字面量就认不出；枚举常量只认 `类型.全大写` 这一形；先 `if (…) return` 再在后面取文的认不出。
 */
object ExitScan {
    val SKELETON_KINDS = setOf("title", "control", "action", "aria", "body")

    data class Entry(
        val kind: String,
        val args: Int,
    )

    data class Hits(
        val key: Int,
        val branch: Int,
    )

    fun skeletonKey(
        key: String?,
        table: Map<String, Entry>,
    ): Boolean {
        val e = key?.let { table[it] } ?: return false
        return e.kind in SKELETON_KINDS && e.args == 0
    }

    fun scan(
        file: KtFile,
        table: Map<String, Entry>,
    ): Hits {
        val lists = codeLists(file)
        val calls = file.collectDescendantsOfType<KtCallExpression> { it.calleeExpression?.text == "copyText" }
        val key = calls.count { !skeletonKey(literal(it.valueArguments.firstOrNull()?.getArgumentExpression()), table) }
        val han = file.collectDescendantsOfType<KtStringTemplateExpression> { hasHan(it) }
        val enums = file.collectDescendantsOfType<KtDotQualifiedExpression> { isEnumConstant(it) && isBranchResult(it, lists) }
        val branch = (calls + han).count { governed(it, lists) } + enums.size
        return Hits(key, branch)
    }

    private val CONSTANT = Regex("[A-Z][A-Z0-9_]*")
    private val LIST_BUILDERS = setOf("setOf", "listOf", "arrayOf", "hashSetOf", "mutableSetOf")

    private fun hasHan(s: KtStringTemplateExpression): Boolean = s.text.any { Character.UnicodeScript.of(it.code) == Character.UnicodeScript.HAN }

    /** 字面的字（没有插值）。 */
    private fun literal(e: KtExpression?): String? {
        val s = e as? KtStringTemplateExpression ?: return null
        if (s.hasInterpolation()) return null
        return s.entries.joinToString("") { it.text }
    }

    private fun unwrap(e: KtExpression?): KtExpression? {
        var x = e
        while (x is KtParenthesizedExpression) x = x.expression
        return x
    }

    private fun isCodeList(e: KtExpression?): Boolean {
        val c = unwrap(e) as? KtCallExpression ?: return false
        return c.calleeExpression?.text in LIST_BUILDERS &&
            c.valueArguments.isNotEmpty() &&
            c.valueArguments.all { literal(it.getArgumentExpression()) != null }
    }

    /** 本文件里绑着一组字面码的名字。 */
    private fun codeLists(file: KtFile): Set<String> =
        file
            .collectDescendantsOfType<KtProperty> { isCodeList(it.initializer) }
            .mapNotNull { it.name }
            .toSet()

    private fun isList(
        e: KtExpression?,
        lists: Set<String>,
    ): Boolean {
        val x = unwrap(e)
        return (x is KtNameReferenceExpression && x.getReferencedName() in lists) || isCodeList(x)
    }

    private fun comparesCode(
        n: KtElement,
        lists: Set<String>,
    ): Boolean =
        when (n) {
            is KtBinaryExpression ->
                when (n.operationReference.text) {
                    "==", "!=" -> (literal(unwrap(n.left)) != null) != (literal(unwrap(n.right)) != null)
                    "in", "!in" -> isList(n.right, lists)
                    else -> false
                }
            is KtDotQualifiedExpression ->
                (n.selectorExpression as? KtCallExpression)?.calleeExpression?.text == "contains" && isList(n.receiverExpression, lists)
            else -> false
        }

    /** 条件里拿一个值去比字面的码。 */
    private fun readsCode(
        cond: KtElement?,
        lists: Set<String>,
    ): Boolean = cond != null && (comparesCode(cond, lists) || cond.collectDescendantsOfType<KtElement>().any { comparesCode(it, lists) })

    private fun whenOnCode(w: KtWhenExpression): Boolean =
        w.subjectExpression != null &&
            w.entries.any { en -> en.conditions.any { c -> c is KtWhenConditionWithExpression && literal(unwrap(c.expression)) != null } }

    /** [e] 在不在 [parent] 那个按码判的分支底下（条件本身不算）。 */
    private fun branchOnCode(
        parent: KtElement,
        e: KtElement,
        lists: Set<String>,
    ): Boolean =
        when (parent) {
            is KtIfExpression ->
                !PsiTreeUtil.isAncestor(parent.condition, e, false) && readsCode(parent.condition, lists)
            is KtWhenEntry -> {
                val w = parent.parent as? KtWhenExpression
                PsiTreeUtil.isAncestor(parent.expression, e, false) &&
                    w != null &&
                    (whenOnCode(w) || parent.conditions.any { readsCode(it, lists) })
            }
            else -> false
        }

    private fun governed(
        e: KtElement,
        lists: Set<String>,
    ): Boolean {
        var p = e.parent
        while (p != null && p !is KtFile) {
            if (p is KtNamedFunction || p is KtClassOrObject) return false
            if (p is KtElement && branchOnCode(p, e, lists)) return true
            p = p.parent
        }
        return false
    }

    private fun isEnumConstant(e: KtDotQualifiedExpression): Boolean {
        val sel = e.selectorExpression as? KtNameReferenceExpression ?: return false
        val recv = e.receiverExpression
        val head = recv as? KtNameReferenceExpression ?: (recv as? KtDotQualifiedExpression)?.selectorExpression as? KtNameReferenceExpression
        return CONSTANT.matches(sel.getReferencedName()) && head?.getReferencedName()?.firstOrNull()?.isUpperCase() == true
    }

    /** 它就是一个按码判的分支的结果（`if` 的两头 · `when` 臂的值；块就看最后一句）。 */
    private fun isBranchResult(
        e: KtExpression,
        lists: Set<String>,
    ): Boolean {
        var x: KtExpression = e
        while (true) {
            val p = x.parent
            x =
                when {
                    p is KtParenthesizedExpression -> p
                    p is KtBlockExpression && p.statements.lastOrNull() == x && p.parent !is KtNamedFunction -> p
                    else -> break
                }
        }
        val up = x.parent
        val ifE = up?.parent as? KtIfExpression
        if (ifE != null && (ifE.then == x || ifE.`else` == x)) return readsCode(ifE.condition, lists)
        val entry = up as? KtWhenEntry
        if (entry != null && entry.expression == x) return branchOnCode(entry, x, lists)
        return false
    }
}
