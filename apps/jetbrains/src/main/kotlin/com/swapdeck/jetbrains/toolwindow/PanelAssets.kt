package com.swapdeck.jetbrains.toolwindow

import java.net.URI
import java.net.URISyntaxException
import java.security.SecureRandom
import java.util.Base64

/** Risposta a una richiesta del pannello: stato HTTP, tipo e contenuto. */
class PanelResource(val status: Int, val mimeType: String, val body: ByteArray) {
    /** `Content-Type` completo: i testi sono sempre UTF-8. */
    val contentType: String get() = "$mimeType; charset=utf-8"
}

/**
 * File del pannello serviti dal plugin all'indirizzo [ORIGIN] (nessuna rete: il browser JCEF li
 * chiede al gestore delle richieste della tool window). Solo tre file, presi da `webview/` nel jar:
 * `index.html` con un nonce nuovo e la lingua, `main.js`, `main.css`. Il resto è 404.
 */
object PanelAssets {
    const val SCHEME = "https"
    const val HOST = "swapdeck.plugin"
    const val ORIGIN = "$SCHEME://$HOST/"
    const val PAGE_URL = "${ORIGIN}index.html"

    private const val NONCE_PLACEHOLDER = "__SWAPDECK_NONCE__"
    private const val LANG_PLACEHOLDER = "__SWAPDECK_LANG__"
    private val random = SecureRandom()

    /** Percorso nell'indirizzo → (risorsa nel jar, tipo). */
    private val FILES = mapOf(
        "/index.html" to ("webview/index.html" to "text/html"),
        "/main.js" to ("webview/main.js" to "text/javascript"),
        "/main.css" to ("webview/main.css" to "text/css"),
    )

    /** Risorsa del jar del plugin (`null` se manca). */
    fun jarResource(path: String): ByteArray? =
        PanelAssets::class.java.classLoader.getResourceAsStream(path)?.use { it.readBytes() }

    /** Indirizzo dell'origine del pannello (`https://swapdeck.plugin`, porta di default). */
    fun isOwnUrl(url: String?): Boolean {
        val uri = parse(url) ?: return false
        return uri.scheme.equals(SCHEME, ignoreCase = true) &&
            uri.rawUserInfo == null &&
            uri.host.equals(HOST, ignoreCase = true) &&
            uri.port == -1
    }

    /** Navigazione permessa nel browser del pannello: solo la sua origine (e la pagina vuota iniziale). */
    fun isNavigationAllowed(url: String?): Boolean = url == "about:blank" || isOwnUrl(url)

    /** Indirizzo da aprire nel browser di sistema (solo `http` e `https`), altrimenti `null`. */
    fun externalUrl(url: String?): String? {
        val uri = parse(url) ?: return null
        val scheme = uri.scheme?.lowercase() ?: return null
        if (scheme != "http" && scheme != "https") return null
        if (uri.host.isNullOrEmpty() || isOwnUrl(url)) return null
        return uri.toASCIIString()
    }

    /** Nonce per la CSP (128 bit casuali, base64). */
    fun newNonce(): String = ByteArray(16).also { random.nextBytes(it) }.let { Base64.getEncoder().encodeToString(it) }

    /**
     * Risposta per [method] [url]: i tre file noti con `GET`, la pagina con [nonce] e [lang]
     * (`it` o `en`, altrimenti `en`); tutto il resto 404. [load] legge una risorsa del jar.
     */
    fun serve(
        method: String?,
        url: String?,
        lang: String,
        load: (String) -> ByteArray? = ::jarResource,
        nonce: () -> String = ::newNonce,
    ): PanelResource {
        if (!isOwnUrl(url) || !method.equals("GET", ignoreCase = true)) return notFound()
        val path = parse(url)?.rawPath ?: return notFound()
        val (resource, mime) = FILES[path] ?: return notFound()
        val bytes = load(resource) ?: return notFound()
        if (path != "/index.html") return PanelResource(200, mime, bytes)
        val page = bytes.toString(Charsets.UTF_8)
            .replace(NONCE_PLACEHOLDER, nonce())
            .replace(LANG_PLACEHOLDER, if (lang == "it") "it" else "en")
        return PanelResource(200, mime, page.toByteArray(Charsets.UTF_8))
    }

    private fun notFound() = PanelResource(404, "text/plain", "Not found".toByteArray(Charsets.UTF_8))

    private fun parse(url: String?): URI? = try {
        url?.let { URI(it) }?.takeIf { it.isAbsolute }
    } catch (_: URISyntaxException) {
        null
    }
}
