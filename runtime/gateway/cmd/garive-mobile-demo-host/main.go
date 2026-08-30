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
	promptJSON     = `{"message":"Approve the release after verified mobile checks?","schema_version":1}`
	promptDigest   = "ab6f115afb7bc38d321ecfa221ad15736e1c057c7265ac10ce75d32ece4a4dff"
	responseJSON   = `{"type":"boolean"}`
	responseDigest = "7cb541e84f226754a46c21c79f131fa2898354e1242456e6fd1c162bce319553"
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
}

type demoHost struct {
	mu       sync.Mutex
	sessions []*session
	next     int
}

func main() {
	host := &demoHost{next: 4, sessions: []*session{
		{ID: "release-approval", Definition: "mobile-orchestrator", State: "suspended", Position: 12, TurnID: "turn-approval", Turns: 3, UserText: "Finish the mobile release and verify every platform."},
		{ID: "runtime-monitor", Definition: "incident-responder", State: "running", Position: 8, TurnID: "turn-running", Turns: 2, UserText: "Monitor the production rollout and report anomalies."},
		{ID: "design-review", Definition: "product-reviewer", State: "completed", Position: 16, TurnID: "turn-complete", Turns: 4, UserText: "Review the mobile interaction design.", Completion: "Review complete: navigation, typography, accessibility, and remote controls meet the accepted mobile specification."},
	}}
	address := os.Getenv("GARIVE_DEMO_HOST_LISTEN")
	if address == "" {
		address = "127.0.0.1:4318"
	}
	log.Printf("Garive mobile demo Host ready on %s", address)
	log.Fatal(http.ListenAndServe(address, host))
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
		item.Turns++
		item.TurnID = fmt.Sprintf("turn-%s-%d", item.ID, item.Turns)
		item.UserText, item.State, item.Completion = body.Text, "running", ""
		item.Position += 2
		d.turnResponse(w, item)
	case r.Method == http.MethodPost && strings.HasPrefix(path, "/v1/turns/") && strings.HasSuffix(path, ":continue"):
		item := d.byTurn(strings.TrimSuffix(strings.TrimPrefix(path, "/v1/turns/"), ":continue"))
		if item == nil {
			d.error(w, http.StatusNotFound, "not_found")
			return
		}
		var body struct {
			InputJSON string `json:"input_json"`
		}
		if json.NewDecoder(r.Body).Decode(&body) != nil || (body.InputJSON != "true" && body.InputJSON != "false") {
			d.error(w, http.StatusBadRequest, "invalid_suspension_response")
			return
		}
		item.State = "completed"
		if body.InputJSON == "true" {
			item.Completion = "Approved. The agent resumed on the server and completed the release checks."
		} else {
			item.Completion = "Declined. The protected action was skipped and the decision was committed."
		}
		item.Position++
		d.turnResponse(w, item)
	case r.Method == http.MethodPost && strings.HasPrefix(path, "/v1/turns/") && strings.HasSuffix(path, ":cancel"):
		item := d.byTurn(strings.TrimSuffix(strings.TrimPrefix(path, "/v1/turns/"), ":cancel"))
		if item == nil {
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
	if item.TurnID != "" {
		activityState, terminal := "running", false
		switch item.State {
		case "completed":
			activityState, terminal = "completed", true
		case "stopped":
			activityState, terminal = "cancelled", true
		}
		turn := map[string]any{"turn_id": item.TurnID, "started_position": max(1, item.Position-2), "latest_position": item.Position, "state": item.State, "user_text": item.UserText, "content_truncated": false, "activities": []any{map[string]any{"api_version": "v1", "activity_id": "activity-" + item.TurnID, "kind": "work", "label_key": "agent.activity.verification", "state": activityState, "source_position": item.Position, "terminal": terminal}}}
		if item.Completion != "" {
			turn["completion_text"] = item.Completion
		}
		if item.State == "suspended" {
			turn["suspension"] = map[string]any{"suspension_id": "suspension-release", "session_version": 3, "kind": "approval_required", "prompt_schema": "garive.public-suspension-prompt.v1", "prompt_json": promptJSON, "prompt_digest": promptDigest, "response_schema_json": responseJSON, "response_schema_digest": responseDigest}
		}
		items = append(items, turn)
	}
	d.write(w, map[string]any{"api_version": "v1", "session_id": item.ID, "items": items, "scanned_through_position": item.Position, "observed_max_position": item.Position, "has_more": false})
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
