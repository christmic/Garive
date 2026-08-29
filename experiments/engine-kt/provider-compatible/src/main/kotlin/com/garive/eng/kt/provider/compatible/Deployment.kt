package com.garive.eng.kt.provider.compatible

import com.garive.eng.kt.anthropic.DocumentSource
import com.garive.eng.kt.anthropic.ImageSource
import com.garive.eng.kt.anthropic.ThinkingConfig
import com.garive.eng.kt.llm.InterruptionKind
import com.garive.eng.kt.llm.ModelCapability
import com.garive.eng.kt.llm.RejectionKind
import com.garive.eng.kt.llm.UnavailableKind
import com.garive.eng.kt.openai.ImageDetail
import com.garive.eng.kt.openai.ReasoningConfig

/** Immutable configuration for one Responses-compatible deployment. */
public data class ResponsesDeployment(
    public val targetId: String,
    public val modelId: String,
    public val capabilities: Set<ModelCapability>,
    public val defaultMaxOutputTokens: ULong? = null,
    public val mediaBindings: Map<String, ResponsesMediaBinding> = emptyMap(),
    public val reasoning: ReasoningConfig? = null,
    public val errorPolicy: ProtocolErrorPolicy = ProtocolErrorPolicy.empty(),
)

/** Explicit binding for one neutral Responses media reference. */
public sealed interface ResponsesMediaBinding {
    /** URL or data-URL image binding. */
    public data class Url(public val value: String, public val detail: ImageDetail? = null) : ResponsesMediaBinding
    /** Previously uploaded protocol file binding. */
    public data class FileId(public val value: String, public val detail: ImageDetail? = null) : ResponsesMediaBinding
}

/** Immutable configuration for one Messages-compatible deployment. */
public data class MessagesDeployment(
    public val targetId: String,
    public val modelId: String,
    public val capabilities: Set<ModelCapability>,
    public val defaultMaxOutputTokens: ULong? = null,
    public val mediaBindings: Map<String, MessagesMediaBinding> = emptyMap(),
    public val thinking: ThinkingConfig? = null,
    public val errorPolicy: ProtocolErrorPolicy = ProtocolErrorPolicy.empty(),
)

/** Explicit binding for one neutral Messages media reference. */
public sealed interface MessagesMediaBinding {
    /** Official image-source binding. */
    public data class Image(public val source: ImageSource) : MessagesMediaBinding
    /** Official document-source binding. */
    public data class Document(public val source: DocumentSource) : MessagesMediaBinding
}

/** Exact protocol error identity; human-readable messages are deliberately absent. */
public data class ErrorSignature(
    public val status: UShort,
    public val protocolType: String,
    public val code: String?,
)

/** Provider-neutral disposition for one exact protocol error signature. */
public sealed interface ErrorDisposition {
    /** Rejected invocation. */
    public data class Rejected(public val kind: RejectionKind) : ErrorDisposition
    /** Temporarily unavailable invocation. */
    public data class Unavailable(public val kind: UnavailableKind) : ErrorDisposition
    /** Interrupted invocation. */
    public data class Interrupted(public val kind: InterruptionKind) : ErrorDisposition
}

/** Immutable exact-match protocol error policy. */
public class ProtocolErrorPolicy private constructor(
    private val mappings: Map<ErrorSignature, ErrorDisposition>,
) {
    /** Returns the disposition for an exact signature. */
    public fun classify(signature: ErrorSignature): ErrorDisposition? = mappings[signature]

    public companion object {
        /** Creates an empty policy. */
        public fun empty(): ProtocolErrorPolicy = ProtocolErrorPolicy(emptyMap())

        /** Creates a policy and rejects duplicate signatures. */
        public fun of(rules: List<Pair<ErrorSignature, ErrorDisposition>>): ProtocolErrorPolicy {
            require(rules.map(Pair<ErrorSignature, ErrorDisposition>::first).distinct().size == rules.size) {
                "duplicate error signature"
            }
            return ProtocolErrorPolicy(rules.toMap())
        }
    }
}
