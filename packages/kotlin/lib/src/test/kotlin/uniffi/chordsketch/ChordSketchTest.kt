package uniffi.chordsketch

import kotlin.test.Test
import kotlin.test.assertTrue
import kotlin.test.assertFalse
import kotlin.test.assertEquals

class ChordSketchTest {
    private val minimalInput = "{title: Test}\n[C]Hello"

    @Test
    fun testVersion() {
        val v = version()
        assertTrue(v.isNotEmpty())
    }

    @Test
    fun testRenderText() {
        val text = parseAndRenderText(minimalInput, null, null)
        assertTrue(text.contains("Test"))
        assertTrue(text.contains("Hello"))
    }

    @Test
    fun testRenderHtml() {
        val html = parseAndRenderHtml(minimalInput, null, null)
        assertTrue(html.contains("Test"))
    }

    @Test
    fun testRenderPdf() {
        val pdf = parseAndRenderPdf(minimalInput, null, null)
        assertTrue(pdf.isNotEmpty())
        // PDF files start with %PDF
        assertEquals("%PDF", String(pdf.sliceArray(0..3)))
    }

    @Test
    fun testValidate() {
        val errors = validate(minimalInput)
        assertTrue(errors.isEmpty())
    }

    @Test
    fun testRenderWithPreset() {
        val text = parseAndRenderText(minimalInput, "guitar", null)
        assertTrue(text.contains("Test"))
    }

    @Test
    fun testRenderWithTranspose() {
        val text = parseAndRenderText(minimalInput, null, 2)
        assertTrue(text.isNotEmpty())
    }
}
