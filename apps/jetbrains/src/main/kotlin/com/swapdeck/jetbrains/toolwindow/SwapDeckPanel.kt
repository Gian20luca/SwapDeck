package com.swapdeck.jetbrains.toolwindow

import com.intellij.ide.BrowserUtil
import com.intellij.openapi.Disposable
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.diagnostic.logger
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.Disposer
import com.intellij.ui.jcef.JBCefBrowser
import com.intellij.ui.jcef.JBCefBrowserBase
import com.intellij.ui.jcef.JBCefJSQuery
import com.swapdeck.jetbrains.bridge.BridgeService
import com.swapdeck.jetbrains.bridge.BridgeSession
import com.swapdeck.jetbrains.bridge.IdeBridgeUi
import com.swapdeck.jetbrains.engine.SwapDeckEngineListener
import com.swapdeck.jetbrains.state.CliState
import com.swapdeck.jetbrains.state.StateService
import com.swapdeck.jetbrains.state.SwapDeckStateListener
import org.cef.browser.CefBrowser
import org.cef.browser.CefFrame
import org.cef.callback.CefCallback
import org.cef.handler.CefLifeSpanHandlerAdapter
import org.cef.handler.CefLoadHandlerAdapter
import org.cef.handler.CefRequestHandlerAdapter
import org.cef.handler.CefResourceHandler
import org.cef.handler.CefResourceHandlerAdapter
import org.cef.handler.CefResourceRequestHandler
import org.cef.handler.CefResourceRequestHandlerAdapter
import org.cef.misc.BoolRef
import org.cef.misc.IntRef
import org.cef.misc.StringRef
import org.cef.network.CefRequest
import org.cef.network.CefResponse
import javax.swing.JComponent

/**
 * Il pannello SwapDeck in un browser JCEF (uno per contenuto della tool window).
 *
 * - **File:** `https://swapdeck.plugin/` non va in rete: il gestore delle richieste di questo browser
 *   risponde con i file del jar ([PanelAssets]); ogni altra navigazione si blocca (`http`/`https`
 *   con un gesto dell'utente si apre nel browser di sistema), i popup pure.
 * - **Messaggi:** `JBCefJSQuery` → [BridgeSession] e risposta al `ready` ([PanelConnection]); stato,
 *   motore, config, lingua e tema arrivano dal bus e da [ViewSettingsService].
 * - **Fine:** [dispose] (contenuto chiuso) ferma i messaggi, chiude la sessione del ponte (annulla
 *   «Rifai login»), poi smaltisce `JBCefJSQuery` e browser. Dopo, nessun `executeJavaScript`.
 */
class SwapDeckPanel(project: Project) : Disposable {
    private val internal = ApplicationManager.getApplication().isInternal

    private val browser: JBCefBrowser = JBCefBrowser.createBuilder()
        // DevTools solo in modalità interna (sviluppo del plugin), mai per gli utenti.
        .setEnableOpenDevToolsMenuItem(internal)
        .build()

    // Prima del caricamento della pagina, come chiede JCEF.
    private val query: JBCefJSQuery = JBCefJSQuery.create(browser as JBCefBrowserBase)
    private val session: BridgeSession = BridgeService.getInstance().openSession(IdeBridgeUi(project))
    private val views = ViewSettingsService.getInstance()

    @Volatile
    private var disposed = false

    private val connection = PanelConnection(
        receive = { text, send -> session.receive(text, send) },
        execute = ::runScript,
        snapshot = { views.current() to StateService.getInstance().state },
    )

    val component: JComponent get() = browser.component

    init {
        val cef = browser.cefBrowser
        browser.jbCefClient.addRequestHandler(RequestHandler(), cef)
        browser.jbCefClient.addLifeSpanHandler(PopupBlocker(), cef)
        browser.jbCefClient.addLoadHandler(LoadHandler(), cef)
        query.addHandler { text ->
            try {
                connection.fromPage(text)
            } catch (e: Exception) {
                LOG.warn("messaggio del pannello non gestito: ${e.javaClass.simpleName}")
            }
            null
        }

        val bus = ApplicationManager.getApplication().messageBus.connect(this)
        bus.subscribe(
            SwapDeckStateListener.TOPIC,
            object : SwapDeckStateListener {
                override fun stateChanged(state: CliState) {
                    PanelMessages.state(state)?.let(connection::post)
                    connection.post(PanelMessages.engine(state))
                }

                override fun configChanged() = connection.post(PanelMessages.configChanged)
            },
        )
        bus.subscribe(
            SwapDeckEngineListener.TOPIC,
            object : SwapDeckEngineListener {
                override fun engineChanged() {
                    StateService.getInstance().state?.let { connection.post(PanelMessages.engine(it)) }
                }
            },
        )
        views.addListener(this) { settings -> connection.post(PanelMessages.view(settings)) }

        browser.loadURL(PanelAssets.PAGE_URL)
    }

    /** Script nella pagina, mai dopo [dispose]. */
    private fun runScript(script: String) {
        synchronized(this) {
            if (disposed || browser.isDisposed) return
            browser.cefBrowser.executeJavaScript(script, PanelAssets.PAGE_URL, 0)
        }
    }

    override fun dispose() {
        // Abbonamenti e ascolto di lingua e tema sono figli di questo Disposable: già staccati.
        connection.close()
        synchronized(this) { disposed = true }
        Disposer.dispose(session)
        Disposer.dispose(query)
        Disposer.dispose(browser)
    }

    /** Solo l'origine del pannello; i suoi file dal jar. */
    private inner class RequestHandler : CefRequestHandlerAdapter() {
        private val resources = object : CefResourceRequestHandlerAdapter() {
            override fun getResourceHandler(browser: CefBrowser?, frame: CefFrame?, request: CefRequest): CefResourceHandler =
                BytesResourceHandler(PanelAssets.serve(request.method, request.url, views.current().locale))
        }

        override fun onBeforeBrowse(
            browser: CefBrowser?,
            frame: CefFrame?,
            request: CefRequest,
            userGesture: Boolean,
            isRedirect: Boolean,
        ): Boolean {
            val url = request.url
            if (PanelAssets.isNavigationAllowed(url)) return false
            if (userGesture) PanelAssets.externalUrl(url)?.let(BrowserUtil::browse)
            return true
        }

        override fun onOpenURLFromTab(browser: CefBrowser?, frame: CefFrame?, targetUrl: String?, userGesture: Boolean): Boolean {
            if (userGesture) PanelAssets.externalUrl(targetUrl)?.let(BrowserUtil::browse)
            return true
        }

        override fun getResourceRequestHandler(
            browser: CefBrowser?,
            frame: CefFrame?,
            request: CefRequest,
            isNavigation: Boolean,
            isDownload: Boolean,
            requestInitiator: String?,
            disableDefaultHandling: BoolRef?,
        ): CefResourceRequestHandler? = if (PanelAssets.isOwnUrl(request.url)) resources else null
    }

    /** Nessun popup: un indirizzo `http`/`https` si apre nel browser di sistema. */
    private class PopupBlocker : CefLifeSpanHandlerAdapter() {
        override fun onBeforePopup(browser: CefBrowser?, frame: CefFrame?, targetUrl: String?, targetFrameName: String?): Boolean {
            PanelAssets.externalUrl(targetUrl)?.let(BrowserUtil::browse)
            return true
        }
    }

    /** A ogni caricamento della pagina: messaggi fermi, poi `window.__swapdeckHost` e `swapdeck-host-ready`. */
    private inner class LoadHandler : CefLoadHandlerAdapter() {
        override fun onLoadStart(browser: CefBrowser?, frame: CefFrame?, transitionType: CefRequest.TransitionType?) {
            if (frame?.isMain == true) connection.pageLoading()
        }

        override fun onLoadEnd(browser: CefBrowser?, frame: CefFrame?, httpStatusCode: Int) {
            if (frame?.isMain != true || !PanelAssets.isOwnUrl(frame.url)) return
            runScript(PanelMessages.injectScript(query.inject("json")))
        }
    }

    /** Risposta già pronta in memoria. */
    // Dall'adattatore e non dall'interfaccia: dalla 2026.x `CefResourceHandler` ha anche `open`,
    // `read` e `skip` astratti (AbstractMethodError, Plugin Verifier). Le versioni dell'adattatore
    // rimandano a `processRequest` e `readResponse`, gli unici metodi della 2024.3.
    private class BytesResourceHandler(private val resource: PanelResource) : CefResourceHandlerAdapter() {
        private var offset = 0

        override fun processRequest(request: CefRequest?, callback: CefCallback): Boolean {
            callback.Continue()
            return true
        }

        override fun getResponseHeaders(response: CefResponse, responseLength: IntRef, redirectUrl: StringRef?) {
            response.status = resource.status
            response.statusText = if (resource.status == 200) "OK" else "Not Found"
            response.mimeType = resource.mimeType
            response.setHeaderByName("Content-Type", resource.contentType, true)
            response.setHeaderByName("Cache-Control", "no-store", true)
            response.setHeaderByName("X-Content-Type-Options", "nosniff", true)
            responseLength.set(resource.body.size)
        }

        override fun readResponse(dataOut: ByteArray, bytesToRead: Int, bytesRead: IntRef, callback: CefCallback?): Boolean {
            val left = resource.body.size - offset
            if (left <= 0) {
                bytesRead.set(0)
                return false
            }
            val count = minOf(left, bytesToRead, dataOut.size)
            System.arraycopy(resource.body, offset, dataOut, 0, count)
            offset += count
            bytesRead.set(count)
            return true
        }

        override fun cancel() {}
    }

    companion object {
        private val LOG = logger<SwapDeckPanel>()
    }
}
