package com.garive.mobile.model

/** Runtime-owned configuration availability projected into the client. */
public enum class AppConfiguration(public val wireName: String) {
    CONFIGURED("configured"), NOT_CONFIGURED("not_configured"),
}

/** Product shell lifecycle. */
public enum class ShellState(public val wireName: String) {
    BOOTING("booting"), NOT_CONFIGURED("not_configured"), LOADING_NAVIGATION("loading_navigation"),
    READY("ready"), UNAVAILABLE("unavailable"),
}

/** Selected conversation workflow state. */
public enum class ExecutionState(public val wireName: String) {
    IDLE("idle"), SUBMITTING("submitting"), FOLLOWING("following"), CANCELLING("cancelling"),
    DISCONNECTED("disconnected"), RECONNECTING("reconnecting"), SUSPENDED("suspended"), CONTINUING("continuing"),
}

/** Public application failure family; content and raw transport details are excluded. */
public enum class AppErrorKind(public val wireName: String) {
    CONFIGURATION("configuration"), VALIDATION("validation"), COMMAND_UNKNOWN("command_unknown"),
    HOST("host"), TRANSPORT("transport"), PROTOCOL("protocol"), LOCAL_PREFERENCE("local_preference"),
}

/** Safe localized-error coordinate. */
public data class AppError(public val kind: AppErrorKind, public val code: String)

/** Disposable local composer content. */
public data class Draft(public val sessionId: String, public val text: String)

/** Complete installed immutable Agent definition projection. */
public data class DefinitionItem(
    public val definitionId: String,
    public val definitionRevision: String,
    public val capabilities: List<String>,
)

/** Bounded H2 Session summary needed by the application controller. */
public data class SessionItem(
    public val sessionId: String,
    public val agentInstanceId: String? = null,
    public val definitionId: String? = null,
    public val definitionRevision: String? = null,
    public val openedAt: String? = null,
    public val latestPosition: Long? = null,
    public val latestTurnId: String? = null,
    public val state: String? = null,
    public val turnCount: Long? = null,
)

/** Parsed redacted public suspension prompt and exact continuation binding. */
public data class SuspensionItem(
    public val suspensionId: String,
    public val sessionVersion: Long,
    public val kind: String,
    public val titleKey: String? = null,
    public val messageText: String? = null,
    public val actionLabelKey: String? = null,
    public val cancelLabelKey: String? = null,
    public val promptDigest: String? = null,
    public val responseSchemaDigest: String? = null,
)

/** Bounded H2 Turn projection with optional actionable suspension coordinates. */
public data class TimelineItem(
    public val turnId: String,
    public val state: String,
    public val latestPosition: Long,
    public val startedPosition: Long? = null,
    public val userText: String? = null,
    public val completionText: String? = null,
    public val suspension: SuspensionItem? = null,
    public val contentTruncated: Boolean = false,
    public val activities: List<ActivityItem> = emptyList(),
)

/** Public H3 activity item or neutral unknown-event marker. */
public data class ActivityItem(
    public val activityId: String,
    public val kind: String,
    public val state: String,
    public val turnId: String? = null,
    public val position: Long,
    public val neutral: Boolean,
    public val labelKey: String? = null,
    public val terminal: Boolean? = null,
    public val safeCode: String? = null,
)

/** External mutation kind. */
public enum class CommandKind(public val wireName: String) {
    CREATE_SESSION("create_session"), START_TURN("start_turn"), CANCEL_TURN("cancel_turn"), CONTINUE_TURN("continue_turn"),
}

/** Known or uncertain mutation state. */
public enum class PendingStatus(public val wireName: String) { PENDING("pending"), UNKNOWN("unknown") }

/** Wire value category required by the suspension response schema. */
public enum class ContinuationValueKind { STRING, JSON_BOOLEAN }

/** Crash-safe, content-minimal command correlation. */
public data class PendingCommand(
    public val kind: CommandKind,
    public val commandId: String,
    public val requestDigest: String,
    public val generation: Long,
    public val sessionId: String? = null,
    public val turnId: String? = null,
    public val status: PendingStatus,
)

/** Application-owned effect vocabulary. */
public enum class EffectKind(public val wireName: String) {
    LOAD_PREFERENCES("load_preferences"), SAVE_PREFERENCES("save_preferences"),
    LOAD_DEFINITIONS("load_definitions"), LOAD_SESSION_PAGE("load_session_page"),
    LOAD_TIMELINE("load_timeline"), FOLLOW_EVENTS("follow_events"),
    CREATE_SESSION("create_session"), START_TURN("start_turn"), CANCEL_TURN("cancel_turn"), CONTINUE_TURN("continue_turn"),
}

/** One immutable side-effect request emitted by the pure reducer. */
public data class AppEffect(
    public val effectId: String,
    public val kind: EffectKind,
    public val generation: Long,
    public val sessionId: String? = null,
    public val commandId: String? = null,
    public val requestDigest: String? = null,
    public val afterPosition: Long? = null,
    public val definitionId: String? = null,
    public val text: String? = null,
    public val turnId: String? = null,
    public val suspensionId: String? = null,
    public val sessionVersion: Long? = null,
    public val responseSchemaDigest: String? = null,
    public val continuationValueKind: ContinuationValueKind? = null,
)

/** Complete immutable product controller state; durable truth remains in Host. */
public data class AppViewState(
    public val configuration: AppConfiguration,
    public val shell: ShellState = ShellState.BOOTING,
    public val generation: Long = 0,
    public val nextEffect: Long = 1,
    public val definitions: List<DefinitionItem> = emptyList(),
    public val sessions: List<SessionItem> = emptyList(),
    public val selectedSessionId: String? = null,
    public val timelineSessionId: String? = null,
    public val timeline: List<TimelineItem> = emptyList(),
    public val cursor: Long = 0,
    public val drafts: List<Draft> = emptyList(),
    public val execution: ExecutionState = ExecutionState.IDLE,
    public val pending: List<PendingCommand> = emptyList(),
    public val activities: List<ActivityItem> = emptyList(),
    public val outstanding: List<AppEffect> = emptyList(),
    public val preferenceDirty: Boolean = false,
    public val notice: AppError? = null,
)

/** Creates the empty product state from an explicit configuration projection. */
public fun initialAppViewState(configuration: AppConfiguration = AppConfiguration.CONFIGURED): AppViewState =
    AppViewState(configuration = configuration)

/** Explicit reducer bounds. */
public data class ControllerLimits(public val maxDraftBytes: Int = 4_096, public val maxActivities: Int = 128)

/** Pure reduction output. */
public data class Reduction(public val state: AppViewState, public val effects: List<AppEffect> = emptyList())
