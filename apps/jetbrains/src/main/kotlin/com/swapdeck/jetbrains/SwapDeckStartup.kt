package com.swapdeck.jetbrains

import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.project.Project
import com.intellij.openapi.startup.ProjectActivity
import com.swapdeck.jetbrains.state.StateService
import com.swapdeck.jetbrains.toolwindow.ViewSettingsService

/**
 * Avvia stato, motore, sincronia e lingua all'apertura del primo progetto. I servizi sono dell'applicazione:
 * con più progetti aperti [StateService.start] non fa nulla dopo la prima volta.
 */
class SwapDeckStartup : ProjectActivity {
    override suspend fun execute(project: Project) {
        if (ApplicationManager.getApplication().isUnitTestMode) return
        StateService.getInstance().start()
        // Lingua delle preferenze di SwapDeck per barra di stato, azioni e notifiche (una lettura della CLI).
        ViewSettingsService.getInstance()
    }
}
