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
 * - `branch`：条件读了线上值的分支里不许取文。「读了线上值」＝ 条件拿一个值去比码（`x == "dup"` · `x in CODES` ·
 *   `CODES.contains(x)`，`CODES` 是本文件里一组码的 `setOf` / `listOf` / `arrayOf`），或 `when (x)` 的臂里有码（那个 `when` 的每一臂都算）。
 *   「码」是字面的字，或线上枚举的一个值：`类型.值`（值首字母大写），类型是从线上类型那一包（[WIRE_PACKAGE]）import 进来的
 *   （带 `as` 的按别名认），或写成那一包的全路径（`com.ccmonitor.mobile.core.wire.gen.Relation.Older`）。
 *   「取文」：分支里的 `copyText(…)`、带汉字的字面量，以及分支的结果直接是一个枚举常量（`DeliveryState.FAILED` 这一形：手机按码挑的多是枚举）。
 *   进到里面再定义的具名函数、类就不算；lambda 跟着外面的分支算。
 *
 * 买不到：比的码不是字面量、也不是上面那一形的线上枚举值就认不出（本文件里另起名字的常量、import 进来的顶层常量 `LOCAL`
 * （Rust 那一半认 `cmd_wire::LOCAL`，这里不认）、`import …gen.*`、`import …gen.类型.值` 之后单写的值、小写别名）；
 * `is 类型.变体 ->` 按线上类型的子类挑的、`in CODES ->` 那一形的 `when` 臂认不出；枚举常量当结果只认 `类型.全大写` 这一形；
 * 先 `if (…) return` 再在后面取文的认不出。
 */
object ExitScan {
    val SKELETON_KINDS = setOf("title", "control", "action", "aria", "body")

    /** 生成的线上类型住的那一包（`core-wire` 的 `gen`）。 */
    const val WIRE_PACKAGE = "com.ccmonitor.mobile.core.wire.gen"

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
        val types = wireTypes(file)
        val cx = Codes(types, codeLists(file, types))
        val calls = file.collectDescendantsOfType<KtCallExpression> { it.calleeExpression?.text == "copyText" }
        val key = calls.count { !skeletonKey(literal(it.valueArguments.firstOrNull()?.getArgumentExpression()), table) }
        val han = file.collectDescendantsOfType<KtStringTemplateExpression> { hasHan(it) }
        val enums = file.collectDescendantsOfType<KtDotQualifiedExpression> { isEnumConstant(it) && isBranchResult(it, cx) }
        val branch = (calls + han).count { governed(it, cx) } + enums.size
        return Hits(key, branch)
    }

    /** 本文件里认得出的码：import 进来的线上类型名，与绑着一组码的名字。 */
    private class Codes(
        val wireTypes: Set<String>,
        val lists: Set<String>,
    )

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

    private fun capitalized(name: String?): Boolean = name?.firstOrNull()?.isUpperCase() == true

    /** 本文件从线上类型那一包 import 进来的类型名（带 `as` 的取别名）。 */
    private fun wireTypes(file: KtFile): Set<String> =
        file.importDirectives
            .filter { d -> d.importedFqName?.parent()?.asString() == WIRE_PACKAGE && !d.isAllUnder }
            .mapNotNull { d -> d.aliasName ?: d.importedFqName?.shortName()?.asString() }
            .filter { capitalized(it) }
            .toSet()

    /** 线上枚举的一个值：`类型.值`，类型是 import 进来的线上类型，或那一包的全路径。 */
    private fun wireValue(
        e: KtExpression?,
        types: Set<String>,
    ): Boolean {
        val d = unwrap(e) as? KtDotQualifiedExpression ?: return false
        val sel = d.selectorExpression as? KtNameReferenceExpression ?: return false
        if (!capitalized(sel.getReferencedName())) return false
        return when (val recv = d.receiverExpression) {
            is KtNameReferenceExpression -> recv.getReferencedName() in types
            is KtDotQualifiedExpression -> recv.receiverExpression.text == WIRE_PACKAGE && capitalized(recv.selectorExpression?.text)
            else -> false
        }
    }

    /** 一个码：字面的字，或线上枚举的一个值。 */
    private fun isCode(
        e: KtExpression?,
        types: Set<String>,
    ): Boolean = literal(unwrap(e)) != null || wireValue(e, types)

    private fun isCodeList(
        e: KtExpression?,
        types: Set<String>,
    ): Boolean {
        val c = unwrap(e) as? KtCallExpression ?: return false
        return c.calleeExpression?.text in LIST_BUILDERS &&
            c.valueArguments.isNotEmpty() &&
            c.valueArguments.all { isCode(it.getArgumentExpression(), types) }
    }

    /** 本文件里绑着一组码的名字。 */
    private fun codeLists(
        file: KtFile,
        types: Set<String>,
    ): Set<String> =
        file
            .collectDescendantsOfType<KtProperty> { isCodeList(it.initializer, types) }
            .mapNotNull { it.name }
            .toSet()

    private fun isList(
        e: KtExpression?,
        cx: Codes,
    ): Boolean {
        val x = unwrap(e)
        return (x is KtNameReferenceExpression && x.getReferencedName() in cx.lists) || isCodeList(x, cx.wireTypes)
    }

    private fun comparesCode(
        n: KtElement,
        cx: Codes,
    ): Boolean =
        when (n) {
            is KtBinaryExpression ->
                when (n.operationReference.text) {
                    "==", "!=" -> isCode(n.left, cx.wireTypes) != isCode(n.right, cx.wireTypes)
                    "in", "!in" -> isList(n.right, cx)
                    else -> false
                }
            is KtDotQualifiedExpression ->
                (n.selectorExpression as? KtCallExpression)?.calleeExpression?.text == "contains" && isList(n.receiverExpression, cx)
            else -> false
        }

    /** 条件里拿一个值去比码。 */
    private fun readsCode(
        cond: KtElement?,
        cx: Codes,
    ): Boolean = cond != null && (comparesCode(cond, cx) || cond.collectDescendantsOfType<KtElement>().any { comparesCode(it, cx) })

    private fun whenOnCode(
        w: KtWhenExpression,
        cx: Codes,
    ): Boolean =
        w.subjectExpression != null &&
            w.entries.any { en -> en.conditions.any { c -> c is KtWhenConditionWithExpression && isCode(c.expression, cx.wireTypes) } }

    /** [e] 在不在 [parent] 那个按码判的分支底下（条件本身不算）。 */
    private fun branchOnCode(
        parent: KtElement,
        e: KtElement,
        cx: Codes,
    ): Boolean =
        when (parent) {
            is KtIfExpression ->
                !PsiTreeUtil.isAncestor(parent.condition, e, false) && readsCode(parent.condition, cx)
            is KtWhenEntry -> {
                val w = parent.parent as? KtWhenExpression
                PsiTreeUtil.isAncestor(parent.expression, e, false) &&
                    w != null &&
                    (whenOnCode(w, cx) || parent.conditions.any { readsCode(it, cx) })
            }
            else -> false
        }

    private fun governed(
        e: KtElement,
        cx: Codes,
    ): Boolean {
        var p = e.parent
        while (p != null && p !is KtFile) {
            if (p is KtNamedFunction || p is KtClassOrObject) return false
            if (p is KtElement && branchOnCode(p, e, cx)) return true
            p = p.parent
        }
        return false
    }

    private fun isEnumConstant(e: KtDotQualifiedExpression): Boolean {
        val sel = e.selectorExpression as? KtNameReferenceExpression ?: return false
        val recv = e.receiverExpression
        val head = recv as? KtNameReferenceExpression ?: (recv as? KtDotQualifiedExpression)?.selectorExpression as? KtNameReferenceExpression
        return CONSTANT.matches(sel.getReferencedName()) && capitalized(head?.getReferencedName())
    }

    /** 它就是一个按码判的分支的结果（`if` 的两头 · `when` 臂的值；块就看最后一句）。 */
    private fun isBranchResult(
        e: KtExpression,
        cx: Codes,
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
        if (ifE != null && (ifE.then == x || ifE.`else` == x)) return readsCode(ifE.condition, cx)
        val entry = up as? KtWhenEntry
        if (entry != null && entry.expression == x) return branchOnCode(entry, x, cx)
        return false
    }
}
