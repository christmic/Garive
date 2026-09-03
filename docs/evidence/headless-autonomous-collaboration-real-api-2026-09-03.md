# Headless autonomous collaboration — real-provider evidence

Date: 2026-09-03  
Runtime: compiled `garive-headless` on loopback  
Provider profile: `anthropic.messages.v1`  
Model target: configured `deepseek-v4-flash` deployment

## Claim boundary

This run proves that collaboration can originate inside the Agent loop. H1 was
used only to create a Session, join AtlasAuto and BirchAuto, and submit the
user input. The API caller did not submit a sender identity, peer message, or
delegation command on the Agent's behalf.

The real model selected `garive.collaboration.message_agent`. Runtime derived
AtlasAuto from the active `turn.started`, evaluated the Prepared-v3 call under
F0, committed its receipt, and then published the addressed Session message.

Two later real-provider delegation attempts ended at `model.uncertain` with
`provider_state_unknown` before either produced a Tool intent. They are not
counted as collaboration acceptance. Named autonomous delegation is covered by
the deterministic end-to-end Runtime test, while a successful real-provider
delegation and the ten-peer autonomous matrix remain open.

No credential is included in this record.

## Real autonomous message

Session:

```text
session-8bb55ac3fe8c45d4f80c4e78cde9eb4dd5a37da117f56b154d459b13cf5112bb
```

The first real model response durably contained this Tool intent:

```json
{
  "tool_name": "garive.collaboration.message_agent",
  "arguments_json": "{\"recipient\":\"BirchAuto\",\"text\":\"REAL_AUTONOMOUS_HELLO\"}"
}
```

F0 recorded the actor authority as the active Agent:

```text
agent:agent-294c34b37103a5d3b8bf275c750fc5fe0e4b8347a2b29253ac43b3463fe1f231
```

After the receipt and parent completion, Runtime appended exactly one
`session.agent_message` with that AtlasAuto instance as sender, BirchAuto as
recipient, and `REAL_AUTONOMOUS_HELLO` as content. The second real model call
observed the successful tool result and completed with `REAL_MESSAGE_DONE`.

## Deterministic regression evidence

`runtime/replica/tests/autonomous_collaboration.rs` proves:

- model-originated addressed messaging with no model-supplied actor field;
- rejection of a missing named target before external dispatch;
- reconstruction of a receipt-committed, unpublished message into a fresh
  outbox, followed by exactly-once publication;
- actor authority binding to the active Agent in `safety.decided`;
- model-originated named delegation, parent completion without suspension,
  real child Turn execution, and addressed result delivery to the dispatcher.

`engine/multiagent/tests/collaboration_tools.rs` additionally proves the exact
Prepared-v3 schemas, Runtime access lanes, and rejection of forged actor input.
