package com.garive.eng.kt.provider.anthropic

internal object Constants {
    const val DEFAULT_ENDPOINT: String = "https://api.anthropic.com/v1/messages"
    const val API_KEY: String = "x-api-key"
    const val AUTHORIZATION: String = "authorization"
    const val VERSION_HEADER: String = "anthropic-version"
    const val PROTOCOL_VERSION: String = "2023-06-01"
    const val CONTENT_TYPE: String = "content-type"
    const val ACCEPT: String = "accept"
    val RESERVED_HEADERS: Set<String> = setOf(API_KEY, AUTHORIZATION, VERSION_HEADER, CONTENT_TYPE, ACCEPT)
}
