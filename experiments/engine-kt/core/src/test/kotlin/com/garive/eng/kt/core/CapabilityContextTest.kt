package com.garive.eng.kt.core

import com.garive.eng.kt.llm.ModelInputContent
import com.garive.eng.kt.llm.ModelInputItem
import com.garive.eng.kt.llm.ModelRole
import java.nio.file.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class CapabilityContextTest {
    private val document: JsonObject by lazy {
        val root = Path.of(System.getProperty("garive.repo.root"))
        Json.parseToJsonElement(
            root.resolve("spec/fixtures/agent/capability-context-admission-v1.json").readText(),
        ).jsonObject
    }

    @Test
    fun `Kotlin consumes every capability context case`() {
        document.getValue("merge_cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            when (val result = mergeContextCandidates(case.candidates("base"), case.candidates("capability"))) {
                is ContextMergeResult.Success -> assertEquals(
                    case.positions("merged"), result.candidates.map { it.factRef.position }, case.text("name"),
                )
                is ContextMergeResult.Failure -> assertEquals(case.text("status"), result.error.code, case.text("name"))
            }
        }
        val case = document.getValue("budget_case").jsonObject
        val merged = mergeContextCandidates(case.candidates("base"), case.candidates("capability"))
            as ContextMergeResult.Success
        val result = deriveContext(
            ContextRequest(
                "session", "turn", ContextPurpose.INFERENCE, null, 4uL,
                case.text("max_items").toInt(), case.text("max_bytes").toInt(),
            ),
            merged.candidates,
        ) as ContextDerivationResult.Success
        assertEquals(case.positions("retained"), result.surface.retainedRefs.map { it.position })
        assertEquals(case.positions("dropped"), result.surface.droppedRefs.map { it.position })
        assertEquals(
            case.strings("item_kinds"),
            result.surface.items.mapNotNull { (it as? ContextItem.Input)?.kind?.fixtureName() },
        )

        val assembly = document.getValue("assembly_case").jsonObject
        val items = assembly.getValue("items").jsonArray.map { element ->
            val item = element.jsonObject
            ContextItem.Input(
                FactRef("session", item.text("position").toULong()),
                fixtureKind(item.text("kind")),
                ModelInputItem.Message(
                    fixtureRole(item.text("role")),
                    listOf(ModelInputContent.Text(item.text("text"))),
                ),
            )
        }
        val surface = ContextSurface(
            ContextPurpose.INFERENCE, 1uL, items.size.toULong(), items,
            emptyList(), emptyList(), emptyList(), items.size, 1,
        )
        val texts = assembleModelInputs(surface).map { item ->
            ((item as ModelInputItem.Message).content.single() as ModelInputContent.Text).text
        }
        assertEquals(assembly.strings("expected_texts"), texts)
    }
}

private fun fixtureKind(value: String): CandidateKind = when (value) {
    "instruction" -> CandidateKind.INSTRUCTION
    "user_input" -> CandidateKind.USER_INPUT
    "skill" -> CandidateKind.SKILL
    "memory" -> CandidateKind.MEMORY
    "knowledge" -> CandidateKind.KNOWLEDGE
    "system_notice" -> CandidateKind.SYSTEM_NOTICE
    "model_output" -> CandidateKind.MODEL_OUTPUT
    else -> error("unknown kind")
}

private fun fixtureRole(value: String): ModelRole = when (value) {
    "system" -> ModelRole.SYSTEM
    "developer" -> ModelRole.DEVELOPER
    "user" -> ModelRole.USER
    "assistant" -> ModelRole.ASSISTANT
    else -> error("unknown role")
}

private fun JsonObject.candidates(name: String): List<ContextCandidate> = positions(name).map(::candidate)
private fun JsonObject.positions(name: String): List<ULong> =
    getValue(name).jsonArray.map { it.jsonPrimitive.content.toULong() }
private fun JsonObject.strings(name: String): List<String> =
    getValue(name).jsonArray.map { it.jsonPrimitive.content }
private fun JsonObject.text(name: String): String = getValue(name).jsonPrimitive.content

private fun candidate(position: ULong): ContextCandidate {
    val (kind, retention, text) = when (position) {
        1uL -> Triple(CandidateKind.USER_INPUT, Retention.REQUIRED, "input")
        2uL -> Triple(CandidateKind.SKILL, Retention.REQUIRED, "skill")
        3uL -> Triple(CandidateKind.MEMORY, Retention.OPTIONAL, "memory")
        4uL -> Triple(CandidateKind.KNOWLEDGE, Retention.OPTIONAL, "knowledge")
        else -> error("unknown position")
    }
    return ContextCandidate(
        FactRef("session", position), kind, retention,
        Visibility.Purposes(setOf(ContextPurpose.INFERENCE)),
        listOf(ModelInputItem.Message(
            if (kind == CandidateKind.SKILL) ModelRole.DEVELOPER else ModelRole.USER,
            listOf(ModelInputContent.Text(text)),
        )),
    )
}

private fun CandidateKind.fixtureName(): String? = when (this) {
    CandidateKind.USER_INPUT -> "user_input"
    CandidateKind.SKILL -> "skill"
    CandidateKind.KNOWLEDGE -> "knowledge"
    else -> null
}
