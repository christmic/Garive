package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestApprovalWalkthroughCommitsBothDecisions(t *testing.T) {
	tests := []struct {
		input string
		want  string
	}{
		{input: "true", want: "Approved."},
		{input: "false", want: "Declined."},
	}
	for _, test := range tests {
		t.Run(test.input, func(t *testing.T) {
			item := &session{ID: "release-approval", TurnID: "turn-approval", State: "suspended", Position: 12}
			host := &demoHost{sessions: []*session{item}}
			request := httptest.NewRequest(
				http.MethodPost,
				"/v1/turns/turn-approval:continue",
				strings.NewReader(`{"input_json":"`+test.input+`"}`),
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
