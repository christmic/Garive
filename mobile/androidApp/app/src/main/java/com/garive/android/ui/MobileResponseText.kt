package com.garive.android.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp

internal sealed interface MobileResponseBlock {
    data class Prose(val text: String) : MobileResponseBlock
    data class Code(val language: String?, val text: String) : MobileResponseBlock
}

internal fun parseMobileResponseBlocks(value: String): List<MobileResponseBlock> {
    val result = mutableListOf<MobileResponseBlock>()
    val buffer = mutableListOf<String>()
    var language: String? = null
    var inCode = false

    fun flushProse() {
        while (buffer.firstOrNull()?.isEmpty() == true) buffer.removeAt(0)
        while (buffer.lastOrNull()?.isEmpty() == true) buffer.removeAt(buffer.lastIndex)
        if (buffer.isNotEmpty()) result += MobileResponseBlock.Prose(buffer.joinToString("\n"))
        buffer.clear()
    }

    fun flushCode() {
        result += MobileResponseBlock.Code(language, buffer.joinToString("\n"))
        buffer.clear()
        language = null
    }

    value.replace("\r\n", "\n").split('\n').forEach { line ->
        val marker = line.trim()
        if (!inCode && line.trimStart().startsWith("```")) {
            flushProse()
            language = line.trimStart().removePrefix("```").trim().take(32).ifEmpty { null }
            inCode = true
        } else if (inCode && marker == "```") {
            flushCode()
            inCode = false
        } else {
            buffer += line
        }
    }
    if (inCode) flushCode() else flushProse()
    return result
}

@Composable
internal fun MobileResponseText(text: String) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        parseMobileResponseBlocks(text).forEach { block ->
            when (block) {
                is MobileResponseBlock.Prose -> SelectionContainer {
                    Text(
                        block.text,
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onBackground,
                    )
                }
                is MobileResponseBlock.Code -> Surface(
                    shape = RoundedCornerShape(14.dp),
                    color = MaterialTheme.colorScheme.surfaceVariant,
                    border = BorderStroke(1.dp, MaterialTheme.colorScheme.outline.copy(alpha = 0.28f)),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Column(Modifier.padding(vertical = 10.dp)) {
                        block.language?.let {
                            Text(
                                it,
                                modifier = Modifier.padding(horizontal = 12.dp, vertical = 2.dp),
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        SelectionContainer {
                            Text(
                                block.text,
                                modifier = Modifier
                                    .horizontalScroll(rememberScrollState())
                                    .testTag("Agent code block")
                                    .padding(horizontal = 12.dp, vertical = 4.dp),
                                fontFamily = FontFamily.Monospace,
                                softWrap = false,
                                color = MaterialTheme.colorScheme.onSurface,
                            )
                        }
                    }
                }
            }
        }
    }
}
