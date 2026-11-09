package com.swapdeck.jetbrains.ui

import com.swapdeck.jetbrains.Translate
import com.swapdeck.jetbrains.bridge.displaySafe
import com.swapdeck.jetbrains.cli.CswapError
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/** Icona del widget: caricamento, avviso (cswap non disponibile) o account. */
enum class StatusIcon { LOADING, WARNING, ACCOUNT }

/**
 * Cosa mostra il widget della barra di stato.
 *
 * @property level `unknown`, `ok`, `warn`, `danger` o `error` (colore di sfondo).
 * @property tooltip righe di testo semplice (l'escape lo fa chi le mette in HTML).
 */
data class StatusView(val text: String, val tooltip: List<String>, val level: String, val icon: StatusIcon) {
    /** Sfondo d'avviso (`warn`) o d'errore (`danger`, `error`); `null` = quello della barra. */
    val background: String?
        get() = when (level) {
            "danger", "error" -> "error"
            "warn" -> "warning"
            else -> null
        }
}

/**
 * Testo e tooltip del widget dal riepilogo della CLI (`swapdeck-cli ide status`, tipo `IdeStatus`):
 * livelli, ultimo dato noto, titolare del motore e soglie li decide il Rust; qui solo traduzioni e
 * composizione dei testi, come `statusBar.ts` di VS Code.
 */
object StatusFormatter {
    /** Nome da mostrare: l'alias (controlli visibili, al più 40 caratteri), altrimenti il numero (`#6`). */
    fun accountLabel(alias: String?, number: Long?): String =
        if (!alias.isNullOrEmpty()) displaySafe(alias, ALIAS_MAX) else "#${number ?: "?"}"

    private const val ALIAS_MAX = 40

    fun pctText(pct: Long?): String = if (pct == null) "—" else "$pct%"

    /** Senza riepilogo (nessuna lettura ancora) è il caricamento. */
    fun of(status: JsonObject?, t: Translate, formatTime: (Long) -> String): StatusView {
        if (status == null) return StatusView("SwapDeck", listOf(t.message("status.loading")), "unknown", StatusIcon.LOADING)
        val engine = engineLine(status.obj("engine"), t)
        val content = status.obj("content")
        val level = status.text("level") ?: "unknown"
        val kind = content?.text("kind")
        if (kind == "noAccount") {
            val updated = updatedLine(status, t, formatTime)
            return StatusView("SwapDeck: —", listOf(t.message("status.noAccount"), engine, updated), level, StatusIcon.ACCOUNT)
        }
        if (content == null || kind != "account") {
            val message = CswapError.parse(content?.get("error"))?.message ?: t.message("status.unknownError")
            return StatusView("SwapDeck", listOf(t.message("status.unavailable"), message, engine), "error", StatusIcon.WARNING)
        }
        val account = content.obj("account")
        val five = content.obj("fiveHour")
        val seven = content.obj("sevenDay")
        val stale = if (content.flag("lastKnown")) "~" else ""
        val name = accountLabel(account?.text("alias"), account?.number("number"))
        val fivePct = pctText(five?.number("pct"))
        val sevenPct = pctText(seven?.number("pct"))
        val text = "$name 5h $stale$fivePct · 7d $stale$sevenPct"
        val lines = mutableListOf("SwapDeck — $name (${account?.text("shortEmail") ?: ""})")
        val countdown = five?.text("countdown")
        lines += if (countdown != null) {
            t.message("status.fiveHour.reset", fivePct, countdown)
        } else {
            t.message("status.fiveHour", fivePct)
        }
        val expected = seven?.number("expectedPct")
        lines += if (expected != null) {
            t.message("status.sevenDay.expected", sevenPct, pctText(expected))
        } else {
            t.message("status.sevenDay", sevenPct)
        }
        if (content.flag("lastKnown")) lines += t.message("status.lastKnown")
        lines += engine
        status.obj("engine")?.text("activeMap")?.takeIf { it.isNotEmpty() }?.let { lines += t.message("status.map", it) }
        lines += updatedLine(status, t, formatTime)
        return StatusView(text, lines, level, StatusIcon.ACCOUNT)
    }

    private fun updatedLine(status: JsonObject, t: Translate, formatTime: (Long) -> String): String =
        t.message("status.updatedAt", formatTime(status.number("updatedAtMs") ?: 0))

    private fun engineLine(engine: JsonObject?, t: Translate): String {
        val holder = when (engine?.text("holder")) {
            "here" -> t.message("status.engine.here")
            "other" -> engine.number("pid")?.let { t.message("status.engine.otherPid", it) } ?: t.message("status.engine.other")
            else -> t.message("status.engine.stopped")
        }
        val switching = if (engine?.flag("switching") == true) t.message("status.switching.on") else t.message("status.switching.off")
        return "$holder · $switching"
    }

    private fun JsonObject.obj(key: String): JsonObject? = this[key] as? JsonObject

    private fun JsonObject.text(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

    private fun JsonObject.flag(key: String): Boolean =
        (this[key] as? JsonPrimitive)?.takeIf { !it.isString && it !is JsonNull }?.content == "true"

    /** Numero intero (anche `12.0`), altrimenti `null`. */
    private fun JsonObject.number(key: String): Long? =
        (this[key] as? JsonPrimitive)?.takeIf { !it.isString && it !is JsonNull }?.content?.toDoubleOrNull()?.toLong()
}
