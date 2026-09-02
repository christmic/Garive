package com.garive.eng.kt.provider.compatible

import com.garive.eng.kt.llm.InvokeOutcome
import com.garive.eng.kt.llm.ModelItem
import com.garive.eng.kt.llm.ToolDescriptor
import java.security.MessageDigest

private const val MAX_WIRE_NAME_BYTES: Int = 64

/** Maps one neutral tool identity into the shared portable protocol grammar. */
public fun wireToolName(name: String): String {
    if (name.isNotEmpty() && name.length <= MAX_WIRE_NAME_BYTES &&
        name.all { it.isLetterOrDigit() && it.code < 128 || it == '_' || it == '-' }
    ) return name
    val digest = MessageDigest.getInstance("SHA-256").digest(name.toByteArray())
        .joinToString("") { "%02x".format(it) }
    return "garive_" + digest.take(MAX_WIRE_NAME_BYTES - 7)
}

/** Restores request-local neutral names after protocol normalization. */
public fun restoreNeutralToolNames(outcome: InvokeOutcome, tools: List<ToolDescriptor>): InvokeOutcome {
    val names = tools.groupBy { wireToolName(it.name) }
    if (names.values.any { it.size != 1 }) fail(CompatibleProviderError.PROTOCOL_INVARIANT)
    fun restore(items: List<ModelItem>): List<ModelItem> = items.map { item ->
        if (item !is ModelItem.ToolIntent) return@map item
        val neutral = names[item.toolName]?.singleOrNull()?.name
            ?: fail(CompatibleProviderError.PROTOCOL_INVARIANT)
        item.copy(toolName = neutral)
    }
    return when (outcome) {
        is InvokeOutcome.Completed -> outcome.copy(items = restore(outcome.items))
        is InvokeOutcome.Interrupted -> outcome.copy(partialItems = restore(outcome.partialItems))
        else -> outcome
    }
}
