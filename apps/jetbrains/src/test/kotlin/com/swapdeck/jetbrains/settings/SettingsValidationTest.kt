package com.swapdeck.jetbrains.settings

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Test

/** Regole della pagina Impostazioni. */
class SettingsValidationTest {
    private val unixAbsolute: (String) -> Boolean = { it.startsWith("/") }

    private fun check(interval: String = "60", cliPath: String = "", thresholds: String = "80, 95") =
        SettingsValidation.check(interval, cliPath, true, thresholds, unixAbsolute)

    @Test
    fun `valori validi - normalizzati`() {
        assertEquals(
            SettingsCheck.Valid(SettingsSnapshot(15, "/opt/swapdeck-cli", true, listOf(70, 90, 100))),
            check(" 15 ", " /opt/swapdeck-cli ", "70 90,100"),
        )
        assertEquals(SettingsCheck.Valid(SettingsSnapshot(3600, "", true, emptyList())), check("3600", "", "  "))
    }

    @Test
    fun `intervallo fuori dai limiti o non intero`() {
        for (bad in listOf("14", "3601", "", "abc", "60.5", "-1")) {
            assertEquals(SettingsCheck.Invalid("settings.error.interval", listOf(15, 3600)), check(interval = bad), bad)
        }
    }

    @Test
    fun `cliPath relativo o con controlli`() {
        for (bad in listOf("swapdeck-cli", "./bin/cli", "/a\nb")) {
            assertEquals(SettingsCheck.Invalid("settings.error.cliPath"), check(cliPath = bad), bad)
        }
    }

    @Test
    fun `soglie - fino a 5 interi tra 1 e 100`() {
        for (bad in listOf("0", "101", "80, x", "1,2,3,4,5,6", "50.5")) {
            assertEquals(SettingsCheck.Invalid("settings.error.thresholds", listOf(5)), check(thresholds = bad), bad)
        }
        assertEquals(listOf(1, 2, 3, 4, 5), SettingsValidation.parseThresholds("1,2;3 4 , 5"))
        assertNull(SettingsValidation.parseThresholds("1,,x"))
        assertEquals("80, 95", SettingsValidation.thresholdsText(listOf(80, 95)))
    }
}
