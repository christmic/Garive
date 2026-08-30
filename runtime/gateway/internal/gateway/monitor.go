package gateway

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"time"
)

type wakeObservation struct {
	SessionID      string  `json:"session_id"`
	LatestPosition uint64  `json:"latest_position"`
	WakeCategory   *string `json:"wake_category,omitempty"`
}

type wakeSnapshotPage struct {
	APIVersion   string            `json:"api_version"`
	Observations []wakeObservation `json:"observations"`
	NextBefore   *string           `json:"next_before,omitempty"`
}

// RunWakeMonitor relays Runtime-owned public transitions as hints until ctx ends.
func (s *Server) RunWakeMonitor(ctx context.Context, interval time.Duration, pageSize int) {
	if s.pushSender == nil || interval <= 0 || pageSize <= 0 || pageSize > 256 {
		return
	}
	known := make(map[string]wakeObservation)
	initialized := false
	ticker := time.NewTicker(interval)
	defer ticker.Stop()
	for {
		snapshot, err := s.fetchWakeSnapshot(ctx, pageSize)
		if err == nil {
			if initialized {
				s.relayWakeChanges(ctx, known, snapshot)
			}
			known = snapshot
			initialized = true
		}
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
	}
}

func (s *Server) fetchWakeSnapshot(ctx context.Context, pageSize int) (map[string]wakeObservation, error) {
	result := make(map[string]wakeObservation)
	before := ""
	seenCursors := make(map[string]struct{})
	for pageNumber := 0; pageNumber < 10_000; pageNumber++ {
		target := *s.runtime
		target.Path = "/internal/mobile/wake-snapshot"
		target.RawQuery = "limit=" + fmt.Sprint(pageSize)
		if before != "" {
			target.RawQuery += "&before=" + url.QueryEscape(before)
		}
		request, err := http.NewRequestWithContext(ctx, http.MethodGet, target.String(), nil)
		if err != nil {
			return nil, errors.New("invalid Runtime wake request")
		}
		response, err := s.transport.RoundTrip(request)
		if err != nil {
			return nil, errors.New("Runtime wake snapshot unavailable")
		}
		var page wakeSnapshotPage
		decoder := json.NewDecoder(io.LimitReader(response.Body, 1024*1024))
		decoder.DisallowUnknownFields()
		decodeError := decoder.Decode(&page)
		response.Body.Close()
		if response.StatusCode != http.StatusOK || decodeError != nil || page.APIVersion != apiVersion {
			return nil, errors.New("invalid Runtime wake snapshot")
		}
		for _, observation := range page.Observations {
			if !wakeSessionID.MatchString(observation.SessionID) || observation.LatestPosition == 0 ||
				!validWakeCategory(observation.WakeCategory) {
				return nil, errors.New("invalid Runtime wake observation")
			}
			if _, duplicate := result[observation.SessionID]; duplicate {
				return nil, errors.New("duplicate Runtime wake observation")
			}
			result[observation.SessionID] = observation
		}
		if page.NextBefore == nil {
			return result, nil
		}
		before = *page.NextBefore
		if before == "" {
			return nil, errors.New("invalid Runtime wake cursor")
		}
		if _, duplicate := seenCursors[before]; duplicate {
			return nil, errors.New("repeated Runtime wake cursor")
		}
		seenCursors[before] = struct{}{}
	}
	return nil, errors.New("Runtime wake snapshot page limit exceeded")
}

func (s *Server) relayWakeChanges(ctx context.Context, previous, current map[string]wakeObservation) {
	devices := s.registeredDeviceIDs()
	for sessionID, observation := range current {
		if observation.WakeCategory == nil {
			continue
		}
		prior, existed := previous[sessionID]
		if existed && observation.LatestPosition < prior.LatestPosition {
			continue
		}
		if existed && sameCategory(prior.WakeCategory, observation.WakeCategory) {
			continue
		}
		for _, deviceID := range devices {
			_ = s.sendWake(ctx, dispatchWakeRequest{
				APIVersion: apiVersion, DeviceID: deviceID, Destination: "session",
				SessionID: sessionID, Category: *observation.WakeCategory,
			})
		}
	}
}

func (s *Server) registeredDeviceIDs() []string {
	s.mu.RLock()
	defer s.mu.RUnlock()
	result := make([]string, 0, len(s.devices))
	for id, value := range s.devices {
		if s.active(value) && value.push != nil {
			result = append(result, id)
		}
	}
	return result
}

func validWakeCategory(value *string) bool {
	return value == nil || *value == "attention" || *value == "completed" || *value == "failed"
}

func sameCategory(left, right *string) bool {
	return (left == nil && right == nil) || (left != nil && right != nil && *left == *right)
}
