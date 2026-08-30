package gateway

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"io"
	"net/http"
	"regexp"
	"time"
)

var wakeSessionID = regexp.MustCompile(`^[A-Za-z0-9_-]+$`)

// PushSender is the provider boundary. Implementations receive content-free hints only.
type PushSender interface {
	Send(context.Context, PushRegistration, MobileWakeHint) error
}

// PushRegistration contains the provider address needed for delivery and is never logged.
type PushRegistration struct {
	Transport string
	Address   string
}

// MobileWakeHint is the complete privacy-safe provider payload.
type MobileWakeHint struct {
	SchemaVersion int    `json:"schema_version"`
	RouteToken    string `json:"route_token"`
	Category      string `json:"category"`
	CollapseKey   string `json:"collapse_key"`
}

type wakeRoute struct {
	deviceID    string
	destination string
	sessionID   string
	category    string
	expires     time.Time
}

type pushRegistrationRequest struct {
	APIVersion     string `json:"api_version"`
	Transport      string `json:"transport"`
	RegistrationID string `json:"registration_id"`
}

func (s *Server) registerPush(w http.ResponseWriter, r *http.Request) {
	var request pushRegistrationRequest
	if !decodeStrict(w, r, s.maxBodyBytes, &request) {
		return
	}
	if request.APIVersion != apiVersion || !validPushToken(request.RegistrationID) {
		writeError(w, http.StatusBadRequest, "invalid_push_registration")
		return
	}
	token, ok := bearer(r.Header.Get("Authorization"))
	if !ok {
		writeError(w, http.StatusUnauthorized, "authentication_required")
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	value := s.grants[sha256.Sum256([]byte(token))]
	if !s.active(value) {
		writeError(w, http.StatusUnauthorized, "authentication_required")
		return
	}
	if (value.platform == "ios" && request.Transport != "apns") ||
		(value.platform == "android" && request.Transport != "fcm") {
		writeError(w, http.StatusBadRequest, "invalid_push_registration")
		return
	}
	value.push = &PushRegistration{Transport: request.Transport, Address: request.RegistrationID}
	w.WriteHeader(http.StatusNoContent)
}

func (s *Server) unregisterPush(w http.ResponseWriter, r *http.Request) {
	token, ok := bearer(r.Header.Get("Authorization"))
	if !ok {
		writeError(w, http.StatusUnauthorized, "authentication_required")
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	value := s.grants[sha256.Sum256([]byte(token))]
	if !s.active(value) {
		writeError(w, http.StatusUnauthorized, "authentication_required")
		return
	}
	value.push = nil
	w.WriteHeader(http.StatusNoContent)
}

type dispatchWakeRequest struct {
	APIVersion  string `json:"api_version"`
	DeviceID    string `json:"device_id"`
	Destination string `json:"destination"`
	SessionID   string `json:"session_id,omitempty"`
	Category    string `json:"category"`
}

func (s *Server) dispatchWake(w http.ResponseWriter, r *http.Request) {
	if !s.isAdmin(r.Header.Get("Authorization")) {
		writeError(w, http.StatusUnauthorized, "authentication_required")
		return
	}
	var request dispatchWakeRequest
	if !decodeStrict(w, r, s.maxBodyBytes, &request) {
		return
	}
	if !validWakeRequest(request) {
		writeError(w, http.StatusBadRequest, "invalid_wake_hint")
		return
	}
	raw := make([]byte, 32)
	if _, err := io.ReadFull(s.random, raw); err != nil {
		writeError(w, http.StatusServiceUnavailable, "entropy_unavailable")
		return
	}
	routeToken := base64.RawURLEncoding.EncodeToString(raw)
	digest := sha256.Sum256([]byte(routeToken))

	s.mu.Lock()
	value := s.devices[request.DeviceID]
	if !s.active(value) || value.push == nil || s.pushSender == nil {
		s.mu.Unlock()
		writeError(w, http.StatusServiceUnavailable, "push_unavailable")
		return
	}
	registration := *value.push
	s.wakeRoutes[digest] = wakeRoute{
		deviceID: value.deviceID, destination: request.Destination, sessionID: request.SessionID,
		category: request.Category, expires: s.now().Add(s.wakeTTL),
	}
	s.mu.Unlock()

	hint := MobileWakeHint{SchemaVersion: 1, RouteToken: routeToken, Category: request.Category, CollapseKey: request.Category}
	if err := s.pushSender.Send(r.Context(), registration, hint); err != nil {
		s.mu.Lock()
		delete(s.wakeRoutes, digest)
		s.mu.Unlock()
		writeError(w, http.StatusServiceUnavailable, "push_unavailable")
		return
	}
	w.WriteHeader(http.StatusAccepted)
}

func (s *Server) resolveWake(w http.ResponseWriter, r *http.Request) {
	grantToken, ok := bearer(r.Header.Get("Authorization"))
	routeToken := r.URL.Path[len("/v1/mobile/wake/") : len(r.URL.Path)-len(":resolve")]
	if !ok || len(routeToken) != 43 {
		writeError(w, http.StatusUnauthorized, "authentication_required")
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	value := s.grants[sha256.Sum256([]byte(grantToken))]
	route, exists := s.wakeRoutes[sha256.Sum256([]byte(routeToken))]
	if !s.active(value) || !exists || route.deviceID != value.deviceID || !s.now().Before(route.expires) {
		writeError(w, http.StatusNotFound, "wake_hint_not_found")
		return
	}
	delete(s.wakeRoutes, sha256.Sum256([]byte(routeToken)))
	writeJSON(w, http.StatusOK, map[string]any{
		"api_version": apiVersion, "destination": route.destination,
		"session_id": route.sessionID, "category": route.category,
	})
}

func (s *Server) active(value *grant) bool {
	return value != nil && !value.revoked && s.now().Before(value.expires)
}

func validPushToken(value string) bool {
	if len(value) < 20 || len(value) > 4096 {
		return false
	}
	for _, character := range value {
		if character < 0x21 || character > 0x7e {
			return false
		}
	}
	return true
}

func validWakeRequest(value dispatchWakeRequest) bool {
	if value.APIVersion != apiVersion || len(value.DeviceID) < 20 || len(value.DeviceID) > 128 {
		return false
	}
	validCategory := value.Category == "attention" || value.Category == "completed" ||
		value.Category == "failed" || value.Category == "connection_security"
	if !validCategory {
		return false
	}
	if value.Destination == "settings" {
		return value.Category == "connection_security" && value.SessionID == ""
	}
	return value.Destination == "session" && wakeSessionID.MatchString(value.SessionID)
}
