package com.swapdeck.jetbrains.bridge

import com.intellij.openapi.util.SystemInfo

/**
 * Percorso iniziale dei selettori (`defaultPath`, scelto dal pannello non fidato): si usa solo se è
 * un percorso assoluto locale. Percorsi UNC o di dispositivo (`\\server\share`, `\\?\…`, `//host/…`)
 * si scartano senza toccarli: aprirli potrebbe contattare un server SMB (con l'NTLM dell'utente) o
 * bloccare a lungo. Un'unità di rete mappata (`Z:\`) non si riconosce dal testo: il controllo
 * d'esistenza si fa fuori dall'EDT.
 */
internal object LocalPaths {
    private val WINDOWS_ABSOLUTE = Regex("""^[A-Za-z]:[\\/]""")

    /** [raw] se è un percorso assoluto locale per il sistema ([windows]), altrimenti `null`. */
    fun localAbsolute(raw: String?, windows: Boolean = SystemInfo.isWindows): String? {
        if (raw.isNullOrBlank()) return null
        if (raw.any { it.isISOControl() }) return null
        // UNC e dispositivi, con qualsiasi combinazione di barre.
        if (raw.length >= 2 && raw[0] in SLASHES && raw[1] in SLASHES) return null
        return if (windows) {
            if (!WINDOWS_ABSOLUTE.containsMatchIn(raw)) return null
            // Due punti solo dopo la lettera dell'unità (niente flussi alternativi `file:stream`).
            if (raw.indexOf(':', 2) >= 0) return null
            raw
        } else {
            raw.takeIf { it.startsWith("/") }
        }
    }

    private const val SLASHES = "\\/"
}
