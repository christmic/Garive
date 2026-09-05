package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestWalkthroughSeedsIndependentApprovalDecisions(t *testing.T) {
	host := newDemoHost()
	approve := host.find("release-approval")
	decline := host.find("release-decline")

	if approve == nil || decline == nil {
		t.Fatalf("approval sessions = %#v", host.sessions)
	}
	if approve.State != "suspended" || decline.State != "suspended" || approve.TurnID == decline.TurnID {
		t.Fatalf("approve = %#v, decline = %#v", approve, decline)
	}
}

func TestApprovalWalkthroughCommitsBothDecisions(t *testing.T) {
	tests := []struct {
		decision string
		want     string
	}{
		{decision: "approve", want: "Approved."},
		{decision: "deny", want: "Declined."},
	}
	for _, test := range tests {
		t.Run(test.decision, func(t *testing.T) {
			item := &session{ID: "release-approval", TurnID: "turn-approval", State: "suspended", Position: 12, Suspension: approvalSuspension}
			host := &demoHost{sessions: []*session{item}}
			request := httptest.NewRequest(
				http.MethodPost,
				"/v1/sessions/release-approval/turns/turn-approval/events",
				strings.NewReader(`{"kind":"approval","session_id":"release-approval","suspension_id":"suspension-x","expected_session_version":4,"decision":"`+test.decision+`"}`),
			)
			response := httptest.NewRecorder()

			host.ServeHTTP(response, request)

			if response.Code != http.StatusOK {
				t.Fatalf("status = %d, body = %s", response.Code, response.Body.String())
			}
			if item.State != "completed" || !strings.HasPrefix(item.Completion, test.want) {
				t.Fatalf("state = %q, completion = %q", item.State, item.Completion)
			}
		})
	}
}

func TestTextSuspensionCommitsTheExactPublicResponse(t *testing.T) {
	item := &session{ID: "clarification-input", TurnID: "turn-clarification", State: "suspended", Position: 14, Suspension: inputSuspension}
	host := &demoHost{sessions: []*session{item}}
	request := httptest.NewRequest(
		http.MethodPost,
		"/v1/sessions/clarification-input/turns/turn-clarification/events",
		strings.NewReader(`{"kind":"external_input","session_id":"clarification-input","suspension_id":"suspension-y","expected_session_version":5,"text":"Release managers"}`),
	)
	response := httptest.NewRecorder()

	host.ServeHTTP(response, request)

	if response.Code != http.StatusOK || item.State != "completed" ||
		item.Completion != "Response committed for Release managers. The agent resumed the handoff." {
		t.Fatalf("status = %d, state = %q, completion = %q", response.Code, item.State, item.Completion)
	}
}

func TestAppendingTurnKeepsCommittedHistory(t *testing.T) {
	item := &session{
		ID: "session-a", TurnID: "turn-1", State: "stopped", Position: 4,
		Turns: 1, UserText: "first", Completion: "Cancellation recorded.",
	}
	host := &demoHost{sessions: []*session{item}}
	request := httptest.NewRequest(http.MethodPost, "/v1/sessions/session-a/turns", strings.NewReader(`{"text":"second"}`))
	host.ServeHTTP(httptest.NewRecorder(), request)

	timelineRequest := httptest.NewRequest(http.MethodGet, "/v1/sessions/session-a/timeline", nil)
	response := httptest.NewRecorder()
	host.ServeHTTP(response, timelineRequest)
	var payload struct {
		Items []map[string]any `json:"items"`
	}
	if err := json.NewDecoder(response.Body).Decode(&payload); err != nil {
		t.Fatal(err)
	}
	if len(payload.Items) != 2 || payload.Items[0]["user_text"] != "first" || payload.Items[1]["user_text"] != "second" {
		t.Fatalf("timeline = %#v", payload.Items)
	}
}
