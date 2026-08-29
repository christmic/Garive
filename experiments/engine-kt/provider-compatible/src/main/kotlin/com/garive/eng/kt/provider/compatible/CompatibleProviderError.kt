package com.garive.eng.kt.provider.compatible

import com.garive.eng.kt.llm.ModelCapability
import com.garive.eng.kt.llm.RequestValidationError

/** Stable compatible-provider mapping failure. */
public enum class CompatibleProviderError(public val code: String) {
    INVALID_REQUEST("invalid_request"),
    TARGET_MISMATCH("target_mismatch"),
    UNSUPPORTED_CAPABILITY("unsupported_capability"),
    UNSUPPORTED_INPUT("unsupported_input"),
    INVALID_JSON_OBJECT("invalid_json_object"),
    MISSING_OUTPUT_LIMIT("missing_output_limit"),
    UNSUPPORTED_METADATA("unsupported_metadata"),
    MISSING_MEDIA_BINDING("missing_media_binding"),
    INVALID_PROTOCOL_REQUEST("invalid_protocol_request"),
    UNSUPPORTED_EXTENSION("unsupported_extension"),
    UNCLASSIFIED_PROTOCOL_ERROR("unclassified_protocol_error"),
    PROTOCOL_INVARIANT("protocol_invariant"),
}

/** Exception carrying one stable mapping [error] and no secret protocol message. */
public class CompatibleProviderException(
    public val error: CompatibleProviderError,
    public val requestValidation: RequestValidationError? = null,
    public val capability: ModelCapability? = null,
) : IllegalArgumentException(error.code)
