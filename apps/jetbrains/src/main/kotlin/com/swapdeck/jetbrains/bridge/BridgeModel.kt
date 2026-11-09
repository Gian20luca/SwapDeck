package com.swapdeck.jetbrains.bridge

import com.swapdeck.jetbrains.cli.CliException
import com.swapdeck.jetbrains.engine.EnginePause
import com.swapdeck.jetbrains.state.CliState
import kotlinx.serialization.json.JsonObject
import java.util.concurrent.CompletableFuture

/**
 * Lato «servizi» del ponte (stato, motore, sincronia): uno per processo IDE. L'implementazione vera
 * è [IdeBridgeHost]; i test ne usano una finta.
 */
interface BridgeHost {
    /** Ultimo stato letto, se c'è. */
    fun lastState(): CliState?

    /** Nuova lettura (condivisa con il polling). */
    fun refresh(): CompletableFuture<CliState>

    /**
     * Dopo un'azione riuscita (`ActionOutcome`): stato dall'esito, segnale agli altri host,
     * `configChanged` alle viste. Si completa con lo stato aggiornato.
     */
    fun afterAction(outcome: JsonObject): CompletableFuture<*>

    /**
     * Ferma il motore di questo processo (`upgrade` o `relogin`): la pausa vale subito, `stopped` si
     * completa a motore fermo, `resume` (sempre, in un `finally`) lo riavvia.
     */
    fun pauseEngine(why: String): EnginePause

    /** Dopo installazione, aggiornamento o «Rifai login»: segnale agli altri host e `configChanged`. */
    fun changed()

    /** Dopo `setPreferences` riuscito: le viste rileggono lingua (e tema) e si ridisegnano. */
    fun preferencesChanged()

    /** Chi ospita, per la diagnostica (es. `IntelliJ IDEA 2024.3.7, plugin 0.2.4`); `null` = niente `--client`. */
    val clientInfo: String?
}

/** Tipo di selettore nativo. */
enum class PickKind { OPEN_FILE, SAVE_FILE, FOLDER }

/** Filtro di un selettore: nome e estensioni senza punto. */
data class PickFilter(val name: String, val extensions: List<String>)

/** Opzioni già validate di un selettore (solo i campi ammessi). */
data class PickOptions(
    val title: String? = null,
    val defaultPath: String? = null,
    val filters: List<PickFilter>? = null,
)

/** Conferma dovuta per un'azione (`confirm -` della CLI), quando serve. */
sealed interface Confirmation {
    /** Dialogo modale con «Conferma». */
    data object Simple : Confirmation

    /** Dialogo modale e poi la parola da digitare (`purge`). */
    data class Typed(val word: String) : Confirmation
}

/**
 * Lato «interfaccia» del ponte, uno per pannello (progetto della finestra). Tutti i metodi si
 * bloccano finché l'utente non risponde e si chiamano dai thread del plugin, mai dall'EDT:
 * l'implementazione vera ([IdeBridgeUi]) passa all'EDT da sé.
 */
interface BridgeUi {
    /**
     * Dialogo modale per l'azione [description] ([describeAction]: mai il token); per
     * [Confirmation.Typed] anche la parola da digitare. `false` = annullata.
     */
    fun confirmAction(description: String, confirmation: Confirmation): Boolean

    /** Conferma di installazione (`install`) o aggiornamento (`upgrade`) di cswap. */
    fun confirmSetup(kind: String): Boolean

    /** Selettore nativo; `null` se annullato. */
    fun pick(kind: PickKind, options: PickOptions): String?

    fun copyText(text: String)

    /** Apre la cartella nel file manager del sistema. */
    fun openFolder(dir: String)

    /** Comando di `run` (`outcome.command`): terminale dell'IDE se c'è, altrimenti il comando da copiare. */
    fun openCommand(program: String, args: List<String>)

    /** Avviso dell'IDE (notifica), per `outcome.warning` e i cambi di account non riusciti. */
    fun warn(text: String)
}

/**
 * Durata di un pannello: chiuderlo ([abort]) chiama una volta sola le funzioni registrate (richieste
 * lunghe: annulla del «Rifai login»). Registrare dopo la chiusura chiama subito.
 */
class AbortScope {
    private val callbacks = LinkedHashSet<() -> Unit>()

    @Volatile
    var aborted: Boolean = false
        private set

    /** Registra [callback]; chiudere il risultato lo toglie. */
    fun onAbort(callback: () -> Unit): AutoCloseable {
        val now = synchronized(callbacks) {
            if (aborted) {
                true
            } else {
                callbacks.add(callback)
                false
            }
        }
        if (now) callback()
        return AutoCloseable { synchronized(callbacks) { callbacks.remove(callback) } }
    }

    fun abort() {
        val pending = synchronized(callbacks) {
            if (aborted) return
            aborted = true
            callbacks.toList().also { callbacks.clear() }
        }
        pending.forEach { runCatching(it) }
    }
}

internal fun invalid(message: String): CliException = CliException("invalidUsage", message)

internal fun cancelled(message: String = "operazione annullata"): CliException = CliException("cancelled", message)
