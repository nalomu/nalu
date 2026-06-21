package com.nalomu.nalu.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val NaluColors = lightColorScheme(
    primary = Color(0xFF2563EB),
    secondary = Color(0xFF0F766E),
    tertiary = Color(0xFFB45309),
    surface = Color(0xFFFAFAFA),
    background = Color(0xFFFFFFFF)
)

@Composable
fun NaluTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = NaluColors,
        typography = MaterialTheme.typography,
        content = content
    )
}
