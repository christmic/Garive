package main

import (
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
