package com.swapdeck.jetbrains.toolwindow

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNotEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/** File e indirizzi del pannello, senza browser. */
class PanelAssetsTest {
    private val files = mapOf(
        "webview/index.html" to "<html lang=\"__SWAPDECK_LANG__\"><script nonce=\"__SWAPDECK_NONCE__\" src=\"main.js\"></script>",
        "webview/main.js" to "console.log('è')",
        "webview/main.css" to "body{}",
    )
    private val load: (String) -> ByteArray? = { files[it]?.toByteArray(Charsets.UTF_8) }

    private fun serve(url: String?, method: String? = "GET", lang: String = "it") =
        PanelAssets.serve(method, url, lang, load) { "N0nce==" }

    @Test
    fun `pagina con nonce e lingua sostituiti`() {
        val page = serve("https://swapdeck.plugin/index.html")
        assertEquals(200, page.status)
        assertEquals("text/html", page.mimeType)
        assertEquals("text/html; charset=utf-8", page.contentType)
        assertEquals("<html lang=\"it\"><script nonce=\"N0nce==\" src=\"main.js\"></script>", page.body.toString(Charsets.UTF_8))
        assertTrue(serve(PanelAssets.PAGE_URL, lang = "en").body.toString(Charsets.UTF_8).contains("lang=\"en\""))
        // Una lingua inattesa non entra nella pagina.
        assertTrue(serve(PanelAssets.PAGE_URL, lang = "\"><script>").body.toString(Charsets.UTF_8).contains("lang=\"en\""))
    }

    @Test
    fun `script e stile con il loro tipo e senza modifiche`() {
        val js = serve("https://swapdeck.plugin/main.js?v=1")
        assertEquals(200, js.status)
        assertEquals("text/javascript", js.mimeType)
        assertEquals("console.log('è')", js.body.toString(Charsets.UTF_8))
        val css = serve("https://SWAPDECK.plugin/main.css")
        assertEquals(200, css.status)
        assertEquals("text/css", css.mimeType)
    }

    @Test
    fun `tutto il resto è 404`() {
        for (
        url in listOf(
            "https://swapdeck.plugin/",
            "https://swapdeck.plugin/other.js",
            "https://swapdeck.plugin/webview/main.js",
            "https://swapdeck.plugin/../main.js",
            "https://swapdeck.plugin/%6Dain.js",
            "https://swapdeck.plugin/main.js/",
            "https://swapdeck.plugin:8443/main.js",
            "http://swapdeck.plugin/main.js",
            "https://evil.example/main.js",
            "https://swapdeck.plugin@evil.example/main.js",
            "file:///etc/passwd",
            "not a url",
            "",
        )
        ) {
            val res = serve(url)
            assertEquals(404, res.status, url)
            assertEquals("text/plain", res.mimeType)
        }
        assertEquals(404, serve(null).status)
        assertEquals(404, serve(PanelAssets.PAGE_URL, method = "POST").status)
        // Risorsa mancante nel jar.
        assertEquals(404, PanelAssets.serve("GET", PanelAssets.PAGE_URL, "it", { null }).status)
    }

    @Test
    fun `nonce nuovo a ogni pagina`() {
        val a = PanelAssets.newNonce()
        val b = PanelAssets.newNonce()
        assertEquals(24, a.length)
        assertNotEquals(a, b)
        assertTrue(a.matches(Regex("[A-Za-z0-9+/]+=*")))
    }

    @Test
    fun `navigazione solo nella propria origine`() {
        assertTrue(PanelAssets.isNavigationAllowed("https://swapdeck.plugin/index.html"))
        assertTrue(PanelAssets.isNavigationAllowed("about:blank"))
        assertFalse(PanelAssets.isNavigationAllowed("https://swapdeck.plugin.evil.example/"))
        assertFalse(PanelAssets.isNavigationAllowed("https://swapdeck.plugin@evil.example/"))
        assertFalse(PanelAssets.isNavigationAllowed("http://swapdeck.plugin/index.html"))
        assertFalse(PanelAssets.isNavigationAllowed("javascript:alert(1)"))
        assertFalse(PanelAssets.isNavigationAllowed("file:///tmp/x.html"))
        assertFalse(PanelAssets.isNavigationAllowed(null))
    }

    @Test
    fun `link esterni solo http e https`() {
        assertEquals("https://example.com/a?b=1", PanelAssets.externalUrl("https://example.com/a?b=1"))
        assertEquals("http://example.com/", PanelAssets.externalUrl("http://example.com/"))
        assertNull(PanelAssets.externalUrl("https://swapdeck.plugin/index.html"))
        assertNull(PanelAssets.externalUrl("file:///etc/passwd"))
        assertNull(PanelAssets.externalUrl("javascript:alert(1)"))
        assertNull(PanelAssets.externalUrl("mailto:a@example.com"))
        assertNull(PanelAssets.externalUrl("https:///nohost"))
        assertNull(PanelAssets.externalUrl("not a url"))
        assertNull(PanelAssets.externalUrl(null))
    }
}
