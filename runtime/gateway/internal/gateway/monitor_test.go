package gateway

import (
	"context"
	"io"
	"net/http"
	"strings"
	"sync"
	"testing"
	"time"
)

type pushCounter struct {
	mu    sync.Mutex
	hints []MobileWakeHint
}

func (counter *pushCounter) Send(_ context.Context, _ PushRegistration, hint MobileWakeHint) error {
	counter.mu.Lock()
	defer counter.mu.Unlock()
	counter.hints = append(counter.hints, hint)
	return nil
}

func (counter *pushCounter) count() int {
	counter.mu.Lock()
	defer counter.mu.Unlock()
	return len(counter.hints)
}

func TestWakeSnapshotPaginationIsStrict(t *testing.T) {
	server := newPushServer(t, &recordedPush{})
	server.transport = roundTripFunc(func(request *http.Request) (*http.Response, error) {
		if request.URL.Query().Get("before") == "" {
			return providerResponse(http.StatusOK, `{"api_version":"v1","observations":[{"session_id":"session_2","latest_position":8,"wake_category":"attention"}],"next_before":"session_2"}`), nil
		}
		return providerResponse(http.StatusOK, `{"api_version":"v1","observations":[{"session_id":"session_1","latest_position":4}]}`), nil
	})
	snapshot, err := server.fetchWakeSnapshot(context.Background(), 64)
	if err != nil {
		t.Fatal(err)
	}
	if len(snapshot) != 2 || *snapshot["session_2"].WakeCategory != "attention" || snapshot["session_1"].WakeCategory != nil {
		t.Fatalf("invalid snapshot: %#v", snapshot)
	}

	server.transport = roundTripFunc(func(*http.Request) (*http.Response, error) {
		return providerResponse(http.StatusOK, `{"api_version":"v1","observations":[{"session_id":"session_1","latest_position":4,"prompt":"secret"}]}`), nil
	})
	if _, err = server.fetchWakeSnapshot(context.Background(), 64); err == nil || strings.Contains(err.Error(), "secret") {
		t.Fatalf("unsafe strict failure: %v", err)
	}
}

func TestWakeMonitorRelaysOnlyCategoryTransitions(t *testing.T) {
	sender := &pushCounter{}
	server := newPushServer(t, sender)
	grant, _ := pair(t, server)
	registration := `{"api_version":"v1","transport":"apns","registration_id":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"}`
	if got := serve(server, http.MethodPost, "/v1/mobile/push/registrations", registration, grant); got.Code != http.StatusNoContent {
		t.Fatalf("register status = %d", got.Code)
	}
	attention := "attention"
	previous := map[string]wakeObservation{
		"session_1": {SessionID: "session_1", LatestPosition: 4},
	}
	current := map[string]wakeObservation{
		"session_1": {SessionID: "session_1", LatestPosition: 8, WakeCategory: &attention},
	}
	server.relayWakeChanges(context.Background(), previous, current)
	if sender.count() != 1 {
		t.Fatalf("push count = %d", sender.count())
	}
	current["session_1"] = wakeObservation{SessionID: "session_1", LatestPosition: 9, WakeCategory: &attention}
	server.relayWakeChanges(context.Background(), previous, current)
	if sender.count() != 2 {
		t.Fatal("expected transition relative to running baseline")
	}
	server.relayWakeChanges(context.Background(), current, current)
	if sender.count() != 2 {
		t.Fatal("same category emitted duplicate push")
	}
}

func TestWakeMonitorSuppressesStartupHistory(t *testing.T) {
	sender := &pushCounter{}
	server := newPushServer(t, sender)
	grant, _ := pair(t, server)
	registration := `{"api_version":"v1","transport":"apns","registration_id":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"}`
	serve(server, http.MethodPost, "/v1/mobile/push/registrations", registration, grant)
	ctx, cancel := context.WithCancel(context.Background())
	calls := 0
	server.transport = roundTripFunc(func(*http.Request) (*http.Response, error) {
		calls++
		if calls == 2 {
			cancel()
		}
		body := `{"api_version":"v1","observations":[{"session_id":"session_1","latest_position":8,"wake_category":"completed"}]}`
		return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(body))}, nil
	})
	server.RunWakeMonitor(ctx, time.Millisecond, 64)
	if calls < 2 || sender.count() != 0 {
		t.Fatalf("calls = %d pushes = %d", calls, sender.count())
	}
}
