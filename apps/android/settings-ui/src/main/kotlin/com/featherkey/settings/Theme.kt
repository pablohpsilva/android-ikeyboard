package com.featherkey.settings

/*
 * FeatherKey's Material 3 theme. The brand is signal-teal on graphite (from the
 * design brief); this makes that real in both light and dark rather than leaving
 * the app on M3's default purple.
 *
 * The teal brand is forced in every state — we deliberately do NOT defer to
 * Android 12+ dynamic (wallpaper) colour, so FeatherKey always reads as itself.
 * Trust is the brand, so the palette stays calm: teal is the only saturated hue,
 * everything else is a near-neutral graphite.
 *
 * ⚠️ Authored, not compiled.
 */

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val TealLight = lightColorScheme(
    primary = Color(0xFF0A7A6E),
    onPrimary = Color(0xFFFFFFFF),
    primaryContainer = Color(0xFFA8F0E4),
    onPrimaryContainer = Color(0xFF00201C),
    secondary = Color(0xFF4A635F),
    surface = Color(0xFFFFFFFF),
    onSurface = Color(0xFF131C1F),
    surfaceVariant = Color(0xFFE1E7E8),
    onSurfaceVariant = Color(0xFF4F6167),
    outline = Color(0xFF7B8C91),
    error = Color(0xFFB3352C),
)

private val TealDark = darkColorScheme(
    primary = Color(0xFF17BBA8),
    onPrimary = Color(0xFF00201C),
    primaryContainer = Color(0xFF005047),
    onPrimaryContainer = Color(0xFFA8F0E4),
    secondary = Color(0xFFB0CCC6),
    surface = Color(0xFF0F171A),
    onSurface = Color(0xFFE7EEEF),
    surfaceVariant = Color(0xFF25333A),
    onSurfaceVariant = Color(0xFF9FB0B5),
    outline = Color(0xFF6E8288),
    error = Color(0xFFE4685F),
)

@Composable
fun FeatherKeyTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    val colors = if (darkTheme) TealDark else TealLight
    MaterialTheme(colorScheme = colors, content = content)
}
