package com.swapdeck.jetbrains.settings

import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.PersistentStateComponent
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.State
import com.intellij.openapi.components.Storage
import com.intellij.openapi.components.service
import com.intellij.util.messages.Topic
import com.intellij.util.xmlb.annotations.XCollection

/** Valori delle impostazioni, già normalizzati (intervallo nei limiti, soglie valide). */
data class SettingsSnapshot(
    val refreshIntervalSeconds: Int = DEFAULT_INTERVAL_SECONDS,
    val cliPath: String = "",
    val engineEnabled: Boolean = true,
    val thresholds: List<Int> = DEFAULT_THRESHOLDS,
) {
    fun normalized(): SettingsSnapshot = copy(
        refreshIntervalSeconds = refreshIntervalSeconds.coerceIn(MIN_INTERVAL_SECONDS, MAX_INTERVAL_SECONDS),
        cliPath = cliPath.trim(),
        // La CLI ignora i valori non validi; qui si tengono solo quelli in (0, 100].
        thresholds = thresholds.filter { it in 1..100 },
    )

    companion object {
        const val DEFAULT_INTERVAL_SECONDS = 60
        const val MIN_INTERVAL_SECONDS = 15
        const val MAX_INTERVAL_SECONDS = 3600
        val DEFAULT_THRESHOLDS = listOf(80, 95)
    }
}

/** Avviso di cambio delle impostazioni (bus dell'applicazione). */
fun interface SwapDeckSettingsListener {
    fun settingsChanged(old: SettingsSnapshot, new: SettingsSnapshot)

    companion object {
        @JvmField
        @Topic.AppLevel
        val TOPIC: Topic<SwapDeckSettingsListener> =
            Topic(SwapDeckSettingsListener::class.java, Topic.BroadcastDirection.NONE)
    }
}

/**
 * Impostazioni del plugin, per utente e per tutta l'IDE (`swapdeck.xml` nella configurazione
 * dell'IDE). La pagina Impostazioni arriva col sottotask 10; qui il deposito e l'avviso dei cambi.
 */
@Service(Service.Level.APP)
@State(name = "SwapDeckSettings", storages = [Storage("swapdeck.xml")])
class SwapDeckSettings : PersistentStateComponent<SwapDeckSettings.Data> {
    /** Forma salvata (serializzatore XML della piattaforma: campi pubblici modificabili). */
    class Data {
        var refreshIntervalSeconds: Int = SettingsSnapshot.DEFAULT_INTERVAL_SECONDS
        var cliPath: String = ""
        var engineEnabled: Boolean = true

        @XCollection
        var thresholds: MutableList<Int> = SettingsSnapshot.DEFAULT_THRESHOLDS.toMutableList()
    }

    @Volatile
    private var current = SettingsSnapshot()

    val snapshot: SettingsSnapshot get() = current

    override fun getState(): Data = Data().apply {
        val s = current
        refreshIntervalSeconds = s.refreshIntervalSeconds
        cliPath = s.cliPath
        engineEnabled = s.engineEnabled
        thresholds = s.thresholds.toMutableList()
    }

    override fun loadState(state: Data) {
        current = SettingsSnapshot(
            refreshIntervalSeconds = state.refreshIntervalSeconds,
            cliPath = state.cliPath,
            engineEnabled = state.engineEnabled,
            thresholds = state.thresholds.toList(),
        ).normalized()
    }

    /** Cambia le impostazioni e avvisa chi ascolta ([SwapDeckSettingsListener.TOPIC]) se cambiano. */
    fun update(transform: (SettingsSnapshot) -> SettingsSnapshot) {
        val (old, new) = synchronized(this) {
            val old = current
            val new = transform(old).normalized()
            current = new
            old to new
        }
        if (old != new) {
            ApplicationManager.getApplication().messageBus.syncPublisher(SwapDeckSettingsListener.TOPIC)
                .settingsChanged(old, new)
        }
    }

    companion object {
        fun getInstance(): SwapDeckSettings = service()
    }
}
