package com.swapdeck.jetbrains.ui

import com.swapdeck.jetbrains.Translate
import com.swapdeck.jetbrains.bridge.displaySafe
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlin.math.roundToLong

/** Bersaglio di un cambio di account: un numero o una strategia (`best`, `next-available`). */
sealed interface SwitchTarget {
    /** `CswapAction` di `switch` per `confirm -` e `action -`. */
    fun action(): JsonObject

    data class Account(val number: Long) : SwitchTarget {
        override fun action(): JsonObject = buildJsonObject {
            put("kind", JsonPrimitive("switch"))
            put("target", JsonPrimitive(number))
        }
    }

    data class Strategy(val name: String) : SwitchTarget {
        override fun action(): JsonObject = buildJsonObject {
            put("kind", JsonPrimitive("switch"))
            put("strategy", JsonPrimitive(name))
        }
    }
}

/**
 * Voce di un popup.
 *
 * @property separatorAbove titolo del separatore prima della voce (`""` = linea senza titolo).
 * @property selectable `false` per l'account attivo o disattivato (si vede ma non si sceglie).
 */
data class MenuItem<T>(
    val text: String,
    val value: T?,
    val selectable: Boolean = value != null,
    val checked: Boolean = false,
    val separatorAbove: String? = null,
)

/** Popup «Cambia account» (come il quick pick di VS Code). */
sealed interface SwitchMenu {
    /** cswap non gestisce ancora account: solo un messaggio. */
    data object NoAccounts : SwitchMenu

    data class Items(val items: List<MenuItem<SwitchTarget>>) : SwitchMenu

    companion object {
        /**
         * Voci dalla lista di cswap ([list] = `ListResponse`, `null` se la lettura è fallita). Le
         * strategie solo con almeno due account (con uno non c'è verso chi cambiare) o senza lista
         * (l'errore lo spiega cswap); account attivi e disattivati visibili ma non selezionabili.
         */
        fun of(list: JsonObject?, t: Translate): SwitchMenu {
            val accounts = (list?.get("accounts") as? JsonArray)?.mapNotNull { it as? JsonObject }.orEmpty()
            if (list != null && accounts.isEmpty()) return NoAccounts
            val items = mutableListOf<MenuItem<SwitchTarget>>()
            if (list == null || accounts.size >= 2) {
                items += MenuItem(t.message("switch.best"), SwitchTarget.Strategy("best"))
                items += MenuItem(t.message("switch.next"), SwitchTarget.Strategy("next-available"))
            }
            for ((index, a) in accounts.withIndex()) {
                val number = a.long("number") ?: continue
                val active = a.flag("active")
                val disabled = a.flag("disabled")
                val usage = (a["usage"] as? JsonObject) ?: (a["lastGoodUsage"] as? JsonObject)
                val flags = listOfNotNull(
                    t.message("switch.active").takeIf { active },
                    t.message("switch.disabled").takeIf { disabled },
                ).joinToString(", ")
                val name = StatusFormatter.accountLabel(a.text("alias"), number)
                val email = a.text("email")?.let { shortEmail(it, 40) }
                val text = buildString {
                    append("#").append(number).append(' ').append(name)
                    append("  5h ").append(pctOf(usage, "fiveHour"))
                    append(" · 7d ").append(pctOf(usage, "sevenDay"))
                    if (flags.isNotEmpty()) append(" · ").append(flags)
                    if (email != null) append("  ").append(email)
                }
                items += MenuItem(
                    text = text,
                    value = SwitchTarget.Account(number).takeIf { !active && !disabled },
                    checked = active,
                    separatorAbove = t.message("switch.accounts").takeIf { index == 0 && items.isNotEmpty() },
                )
            }
            return Items(items)
        }

        private fun pctOf(usage: JsonObject?, window: String): String {
            val pct = ((usage?.get(window) as? JsonObject)?.get("pct") as? JsonPrimitive)
                ?.takeIf { !it.isString && it !is JsonNull }?.content?.toDoubleOrNull()
                ?: return "—"
            return "${displayPct(pct)}%"
        }
    }
}

/** Popup «Cambia mappa»: prima le mappe personali, poi i preset dopo un separatore. */
object MapMenu {
    /** Voci da `engine profiles` (`EngineProfiles`); il valore è l'`id`, la mappa attiva è spuntata. */
    fun of(profiles: JsonObject, t: Translate): List<MenuItem<String>> {
        val activeId = profiles.text("activeId")
        val all = (profiles["profiles"] as? JsonArray)?.mapNotNull { it as? JsonObject }.orEmpty()
        fun item(p: JsonObject, separator: String?): MenuItem<String>? {
            val id = p.text("id") ?: return null
            val active = id == activeId
            val name = displaySafe(p.text("name") ?: id, NAME_MAX)
            return MenuItem(
                text = if (active) "$name (${t.message("switch.active")})" else name,
                value = id,
                checked = active,
                separatorAbove = separator,
            )
        }
        val personal = all.filter { it["preset"] == null || it["preset"] is JsonNull }.mapNotNull { item(it, null) }
        val presets = all.filter { it["preset"] != null && it["preset"] !is JsonNull }
            .mapIndexedNotNull { i, p -> item(p, if (i == 0) "preset" else null) }
        return personal + presets
    }

    /** Nome della mappa [id] per il messaggio di conferma. */
    fun nameOf(profiles: JsonObject, id: String): String = displaySafe(
        (profiles["profiles"] as? JsonArray)?.mapNotNull { it as? JsonObject }
            ?.firstOrNull { it.text("id") == id }?.text("name") ?: id,
        NAME_MAX,
    )

    private const val NAME_MAX = 80
}

/** Percentuale arrotondata e limitata a 0..100 (`displayPct` dell'interfaccia). */
fun displayPct(pct: Double): Long = if (pct.isNaN()) 0 else Math.round(pct.coerceIn(0.0, 100.0))

/**
 * Accorcia un'email mantenendo il dominio (`shortEmail` dell'interfaccia):
 * `lungonomeutente@example.com` → `lungonome…@example.com`; senza `@` tronca.
 */
fun shortEmail(email: String, max: Int = 24): String {
    if (email.length <= max) return email
    val at = email.lastIndexOf('@')
    if (at <= 0) return email.take(maxOf(1, max - 1)) + "…"
    val local = email.substring(0, at)
    val domain = email.substring(at)
    val room = max - domain.length - 1
    if (room >= 3) return local.take(room) + "…" + domain
    return local.take(3) + "…" + domain.take(maxOf(1, max - 4)) + "…"
}

private fun JsonObject.text(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

private fun JsonObject.flag(key: String): Boolean =
    (this[key] as? JsonPrimitive)?.takeIf { !it.isString && it !is JsonNull }?.content == "true"

private fun JsonObject.long(key: String): Long? =
    (this[key] as? JsonPrimitive)?.takeIf { !it.isString && it !is JsonNull }?.content?.toDoubleOrNull()?.roundToLong()
