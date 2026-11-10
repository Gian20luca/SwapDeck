package com.swapdeck.jetbrains

import com.intellij.openapi.util.text.StringUtil

/**
 * Testo semplice (della CLI, del pannello, delle traduzioni) dentro componenti che l'IDE legge come
 * HTML: notifiche (titolo e contenuto) e dialoghi di `Messages` (la versione «alert» della 2024.3 usa
 * sempre un `JEditorPane` HTML). Senza escape un alias o un messaggio con `<img src=…>` diventerebbe
 * markup, con caricamenti dalla rete.
 */
object IdeText {
    /** Escape di `& < > " '` e a capo come `<br>`: per il contenuto delle notifiche. */
    fun html(text: String): String = StringUtil.escapeXmlEntities(text).replace("\n", "<br>")

    /**
     * Messaggio di un dialogo: sempre HTML (`<html>…</html>`) con il testo sotto escape, così la
     * versione classica (HTML solo se il testo inizia con `<html>`) e quella «alert» lo mostrano uguale.
     */
    fun dialog(text: String): String = "<html>${html(text)}</html>"
}
