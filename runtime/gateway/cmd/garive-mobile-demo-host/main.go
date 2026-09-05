// Command garive-mobile-demo-host serves a deterministic loopback H2/H3 Host
// for native UI walkthroughs. It is not a production Runtime.
package main

import (
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"os"
	"strings"
	"sync"
)

const (
	approvalPromptJSON     = `{"message":"Approve the release after verified mobile checks?","schema_version":1}`
	approvalPromptDigest   = "ab6f115afb7bc38d321ecfa221ad15736e1c057c7265ac10ce75d32ece4a4dff"
	approvalResponseJSON   = `{"type":"boolean"}`
	approvalResponseDigest = "7cb541e84f226754a46c21c79f131fa2898354e1242456e6fd1c162bce319553"
	inputPromptJSON        = `{"message":"Which audience should receive the handoff?","schema_version":1}`
	inputPromptDigest      = "d80f7c636d9d5060fe3863ecb58fae11d4efece0d479347e4afbed7698be652b"
	inputResponseJSON      = `{"type":"string","maxLength":16384}`
	inputResponseDigest    = "194fa31a328180198253b70187424d9ada203b9fee669c2e14e3a1c7abd78b6a"
)

type session struct {
	ID         string
	Definition string
	State      string
	Position   int
	TurnID     string
	Turns      int
	UserText   string
	Completion string
	History    []turnSnapshot
	Suspension *suspensionFixture
}

type suspensionFixture struct {
	Kind, PromptJSON, PromptDigest, ResponseJSON, ResponseDigest string
}

var approvalSuspension = &suspensionFixture{
	Kind: "approval_required", PromptJSON: approvalPromptJSON, PromptDigest: approvalPromptDigest,
	ResponseJSON: approvalResponseJSON, ResponseDigest: approvalResponseDigest,
}

var inputSuspension = &suspensionFixture{
	Kind: "input_required", PromptJSON: inputPromptJSON, PromptDigest: inputPromptDigest,
	ResponseJSON: inputResponseJSON, ResponseDigest: inputResponseDigest,
}

type turnSnapshot struct {
	ID         string
	State      string
	Position   int
	UserText   string
	Completion string
}

type demoHost struct {
	mu       sync.Mutex
	sessions []*session
	next     int
}

func main() {
	host := newDemoHost()
	address := os.Getenv("GARIVE_DEMO_HOST_LISTEN")
	if address == "" {
		address = "127.0.0.1:4318"
	}
	log.Printf("Garive mobile demo Host ready on %s", address)
	log.Fatal(http.ListenAndServe(address, host))
}

func newDemoHost() *demoHost {
	return &demoHost{next: 4, sessions: []*session{
		{ID: "release-approval", Definition: "mobile-orchestrator", State: "suspended", Position: 12, TurnID: "turn-approval", Turns: 3, UserText: "Finish the mobile release and verify every platform.", Suspension: approvalSuspension},
		{ID: "release-decline", Definition: "mobile-orchestrator", State: "suspended", Position: 10, TurnID: "turn-decline", Turns: 2, UserText: "Run the protected release action only if the mobile checks are approved.", Suspension: approvalSuspension},
		{ID: "clarification-input", Definition: "product-reviewer", State: "suspended", Position: 14, TurnID: "turn-clarification", Turns: 3, UserText: "Prepare the incident handoff for the right audience.", Suspension: inputSuspension},
		{ID: "runtime-monitor", Definition: "incident-responder", State: "running", Position: 8, TurnID: "turn-running", Turns: 2, UserText: "Monitor the production rollout and report anomalies."},
		{ID: "design-review", Definition: "product-reviewer", State: "completed", Position: 16, TurnID: "turn-complete", Turns: 4, UserText: "Review the mobile interaction design.", Completion: "Review complete: navigation, typography, accessibility, and remote controls meet the accepted mobile specification.\n\n```swift\nlet releaseStatus = \"ready\"\nlet nextStep = \"ship after physical-device admission\"\n```"},
	}}
}

func (d *demoHost) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	d.mu.Lock()
	defer d.mu.Unlock()
	path := r.URL.Path
	switch {
	case r.Method == http.MethodGet && path == "/v1/agent-definitions":
		d.write(w, map[string]any{"api_version": "v1", "definitions": []any{
			definition("mobile-orchestrator", "rev-mobile-7", "repository_analysis", "code_change", "verification", "remote_decision"),
			definition("incident-responder", "rev-ops-4", "monitoring", "diagnostics", "safe_recovery"),
			definition("product-reviewer", "rev-design-3", "design_review", "accessibility", "quality_gate"),
		}})
	case r.Method == http.MethodGet && path == "/v1/sessions":
		items := make([]any, 0, len(d.sessions))
		for _, item := range d.sessions {
			items = append(items, summary(item))
		}
		d.write(w, map[string]any{"api_version": "v1", "sessions": items})
	case r.Method == http.MethodPost && path == "/v1/sessions":
		var body struct {
			Definition string `json:"agent_definition_id"`
		}
		if json.NewDecoder(r.Body).Decode(&body) != nil || body.Definition == "" {
			d.error(w, http.StatusBadRequest, "invalid_request")
			return
		}
		item := &session{ID: fmt.Sprintf("remote-task-%d", d.next), Definition: body.Definition, State: "", Position: 1}
		d.next++
		d.sessions = append([]*session{item}, d.sessions...)
		d.writeStatus(w, http.StatusCreated, map[string]any{"session_id": item.ID, "agent_instance_id": "demo-" + item.ID, "committed_position": 1})
	case r.Method == http.MethodGet && strings.HasSuffix(path, "/timeline"):
		id := strings.TrimSuffix(strings.TrimPrefix(path, "/v1/sessions/"), "/timeline")
		item := d.find(id)
		if item == nil {
			d.error(w, http.StatusNotFound, "not_found")
			return
		}
		d.timeline(w, item)
	case r.Method == http.MethodGet && strings.HasPrefix(path, "/v1/sessions/") && strings.HasSuffix(path, "/events"):
		id := strings.TrimSuffix(strings.TrimPrefix(path, "/v1/sessions/"), "/events")
		item := d.find(id)
		if item == nil {
			d.error(w, http.StatusNotFound, "not_found")
			return
		}
		w.Header().Set("Content-Type", "text/event-stream")
		fmt.Fprintf(w, "data: {\"api_version\":\"v1\",\"session_id\":%q,\"position\":%d,\"event\":\"turn.completed\",\"turn_id\":%q,\"execution_id\":\"demo-execution\",\"text\":%q}\n\n", item.ID, item.Position, item.TurnID, item.Completion)
	case r.Method == http.MethodGet && strings.HasPrefix(path, "/v1/sessions/"):
		item := d.find(strings.TrimPrefix(path, "/v1/sessions/"))
		if item == nil {
			d.error(w, http.StatusNotFound, "not_found")
			return
		}
		d.write(w, map[string]any{"api_version": "v1", "session": summary(item), "observed_max_position": item.Position})
	case r.Method == http.MethodPost && strings.HasSuffix(path, "/turns"):
		id := strings.TrimSuffix(strings.TrimPrefix(path, "/v1/sessions/"), "/turns")
		item := d.find(id)
		if item == nil {
			d.error(w, http.StatusNotFound, "not_found")
			return
		}
		var body struct {
			Text string `json:"text"`
		}
		if json.NewDecoder(r.Body).Decode(&body) != nil || body.Text == "" {
			d.error(w, http.StatusBadRequest, "invalid_request")
			return
		}
		item.archiveCurrent()
		item.Turns++
		item.TurnID = fmt.Sprintf("turn-%s-%d", item.ID, item.Turns)
		item.UserText, item.State, item.Completion = body.Text, "running", ""
		item.Position += 2
		d.turnResponse(w, item)
	case r.Method == http.MethodPost && strings.Contains(path, "/turns/") && strings.HasSuffix(path, "/events"):
		item, ok := d.byTurnScoped(path)
		if !ok {
			d.error(w, http.StatusNotFound, "not_found")
			return
		}
		var body struct {
			Kind                   string `json:"kind"`
			SessionID              string `json:"session_id"`
			Text                   string `json:"text"`
			InputJSON              string `json:"input_json"`
			Decision               string `json:"decision"`
			SuspensionID           string `json:"suspension_id"`
			ExpectedSessionVersion uint64 `json:"expected_session_version"`
		}
		if json.NewDecoder(r.Body).Decode(&body) != nil || body.SessionID != item.ID {
			d.error(w, http.StatusBadRequest, "invalid_request")
			return
		}
		switch body.Kind {
		case "steer":
			if body.Text == "" {
				d.error(w, http.StatusBadRequest, "invalid_request")
				return
			}
			item.Turns++
			item.TurnID = fmt.Sprintf("turn-%s-%d", item.ID, item.Turns)
			item.UserText, item.State, item.Completion = body.Text, "running", ""
			item.Position += 2
		case "approval":
			if item.Suspension == nil || item.Suspension.Kind != "approval_required" {
				d.error(w, http.StatusBadRequest, "invalid_suspension_response")
				return
			}
			switch body.Decision {
			case "approve":
				item.State, item.Completion = "completed", "Approved. The agent resumed on the server and completed the release checks."
			case "deny":
				item.State, item.Completion = "completed", "Declined. The protected action was skipped and the decision was committed."
			default:
				d.error(w, http.StatusBadRequest, "invalid_suspension_response")
				return
			}
			item.Suspension = nil
			item.Position++
		case "external_input":
			if item.Suspension == nil || item.Suspension.Kind != "input_required" || body.Text == "" || len(body.Text) > 16_384 {
				d.error(w, http.StatusBadRequest, "invalid_suspension_response")
				return
			}
			item.State, item.Completion = "completed", fmt.Sprintf("Response committed for %s. The agent resumed the handoff.", body.Text)
			item.Suspension = nil
			item.Position++
		default:
			d.error(w, http.StatusBadRequest, "invalid_request")
			return
		}
		d.turnResponse(w, item)
	case r.Method == http.MethodPost && strings.Contains(path, "/turns/") && strings.HasSuffix(path, "/cancel"):
		item, ok := d.byTurnScoped(path)
		if !ok {
			d.error(w, http.StatusNotFound, "not_found")
			return
		}
		item.State, item.Completion = "stopped", "Cancellation recorded. Committed work remains available."
		item.Position++
		d.turnResponse(w, item)
	case r.Method == http.MethodGet && path == "/internal/mobile/wake-snapshot":
		observations := make([]any, 0, len(d.sessions))
		for _, item := range d.sessions {
			observation := map[string]any{"session_id": item.ID, "latest_position": item.Position}
			if item.State == "suspended" {
				observation["wake_category"] = "attention"
			}
			if item.State == "completed" {
				observation["wake_category"] = "completed"
			}
			observations = append(observations, observation)
		}
		d.write(w, map[string]any{"api_version": "v1", "observations": observations})
	default:
		d.error(w, http.StatusNotFound, "not_found")
	}
}

func definition(id, revision string, capabilities ...string) map[string]any {
	return map[string]any{"api_version": "v1", "definition_id": id, "definition_revision": revision, "capabilities": capabilities}
}

func summary(item *session) map[string]any {
	value := map[string]any{"api_version": "v1", "session_id": item.ID, "agent_instance_id": "demo-" + item.ID, "definition_id": item.Definition, "definition_revision": "demo-revision", "opened_at": "2026-08-30T08:00:00Z", "latest_position": item.Position, "turn_count": item.Turns}
	if item.TurnID != "" {
		value["latest_turn_id"], value["latest_turn_state"] = item.TurnID, item.State
	}
	return value
}

func (d *demoHost) timeline(w http.ResponseWriter, item *session) {
	items := []any{}
	for _, turn := range item.History {
		items = append(items, timelineTurn(turn.ID, turn.State, turn.Position, turn.UserText, turn.Completion))
	}
	if item.TurnID != "" {
		turn := timelineTurn(item.TurnID, item.State, item.Position, item.UserText, item.Completion)
		if item.State == "suspended" && item.Suspension != nil {
			turn["suspension"] = suspensionView(item.Suspension)
		}
		items = append(items, turn)
	}
	d.write(w, map[string]any{"api_version": "v1", "session_id": item.ID, "items": items, "scanned_through_position": item.Position, "observed_max_position": item.Position, "has_more": false})
}

func (item *session) archiveCurrent() {
	if item.TurnID == "" {
		return
	}
	item.History = append(item.History, turnSnapshot{
		ID: item.TurnID, State: item.State, Position: item.Position,
		UserText: item.UserText, Completion: item.Completion,
	})
}

func timelineTurn(id, state string, position int, userText, completion string) map[string]any {
	activityState, terminal := "running", false
	switch state {
	case "completed":
		activityState, terminal = "completed", true
	case "stopped":
		activityState, terminal = "cancelled", true
	}
	turn := map[string]any{"turn_id": id, "started_position": max(1, position-2), "latest_position": position, "state": state, "user_text": userText, "content_truncated": false, "activities": []any{map[string]any{"api_version": "v1", "activity_id": "activity-" + id, "kind": "work", "label_key": "agent.activity.verification", "state": activityState, "source_position": position, "terminal": terminal, "safe_code": "verification_checked"}}}
	if completion != "" {
		turn["completion_text"] = completion
	}
	return turn
}

func suspensionView(value *suspensionFixture) map[string]any {
	return map[string]any{"suspension_id": "suspension-" + value.Kind, "session_version": 3, "kind": value.Kind, "prompt_schema": "garive.public-suspension-prompt.v1", "prompt_json": value.PromptJSON, "prompt_digest": value.PromptDigest, "response_schema_json": value.ResponseJSON, "response_schema_digest": value.ResponseDigest}
}

func (d *demoHost) find(id string) *session {
	for _, item := range d.sessions {
		if item.ID == id {
			return item
		}
	}
	return nil
}
func (d *demoHost) byTurn(id string) *session {
	for _, item := range d.sessions {
		if item.TurnID == id {
			return item
		}
	}
	return nil
}
func (d *demoHost) byTurnScoped(path string) (*session, bool) {
	const prefix = "/v1/sessions/"
	if !strings.HasPrefix(path, prefix) {
		return nil, false
	}
	rest := strings.TrimPrefix(path, prefix)
	slash := strings.Index(rest, "/turns/")
	if slash < 0 {
		return nil, false
	}
	sessionID := rest[:slash]
	afterTurns := rest[slash+len("/turns/"):]
	for _, item := range d.sessions {
		if item.ID != sessionID {
			continue
		}
		if strings.HasPrefix(afterTurns, item.TurnID+"/events") ||
			strings.HasPrefix(afterTurns, item.TurnID+"/cancel") ||
			afterTurns == item.TurnID+"/events" ||
			afterTurns == item.TurnID+"/cancel" {
			return item, true
		}
	}
	return nil, false
}
func (d *demoHost) turnResponse(w http.ResponseWriter, item *session) {
	d.write(w, map[string]any{"session_id": item.ID, "turn_id": item.TurnID, "execution_id": "demo-execution", "committed_position": item.Position})
}
func (d *demoHost) write(w http.ResponseWriter, value any) { d.writeStatus(w, http.StatusOK, value) }
func (d *demoHost) writeStatus(w http.ResponseWriter, status int, value any) {
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(value)
}
func (d *demoHost) error(w http.ResponseWriter, status int, code string) {
	d.writeStatus(w, status, map[string]string{"code": code})
}
