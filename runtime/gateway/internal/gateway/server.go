package gateway

import (
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	"net"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"sync"
	"time"
)

const apiVersion = "v1"

var admittedRoute = regexp.MustCompile(`^/v1/(agent-definitions|sessions(?:/[A-Za-z0-9_-]+(?:/timeline|/events|/turns(?:/[A-Za-z0-9_-]+(?:/events|/cancel))?)?)?)$`)

type Config struct {
	RuntimeOrigin *url.URL
	PairingCode   string
	AdminToken    string
	GrantTTL      time.Duration
	MaxBodyBytes  int64
	Transport     http.RoundTripper
	Now           func() time.Time
	Random        io.Reader
	PushSender    PushSender
	WakeTTL       time.Duration
}

type grant struct {
	deviceID string
	platform string
	expires  time.Time
	revoked  bool
	push     *PushRegistration
}

type Server struct {
	runtime      *url.URL
	pairingHash  [32]byte
	adminHash    [32]byte
	grantTTL     time.Duration
	maxBodyBytes int64
	transport    http.RoundTripper
	now          func() time.Time
	random       io.Reader
	pushSender   PushSender
	wakeTTL      time.Duration

	mu            sync.RWMutex
	pairingUnused bool
	grants        map[[32]byte]*grant
	devices       map[string]*grant
	wakeRoutes    map[[32]byte]wakeRoute
}

func New(config Config) (*Server, error) {
	if config.RuntimeOrigin == nil || config.RuntimeOrigin.Scheme != "http" || !isLoopback(config.RuntimeOrigin.Hostname()) ||
		config.RuntimeOrigin.User != nil || config.RuntimeOrigin.RawQuery != "" || config.RuntimeOrigin.Fragment != "" ||
		(config.RuntimeOrigin.Path != "" && config.RuntimeOrigin.Path != "/") {
		return nil, errors.New("runtime origin must be a bare loopback HTTP origin")
	}
	if len(config.PairingCode) < 6 || len(config.PairingCode) > 128 || len(config.AdminToken) < 20 {
		return nil, errors.New("pairing code or admin token is outside bounds")
	}
	if config.GrantTTL <= 0 {
		config.GrantTTL = 30 * 24 * time.Hour
	}
	if config.MaxBodyBytes <= 0 {
		config.MaxBodyBytes = 64 * 1024
	}
	if config.Transport == nil {
		config.Transport = http.DefaultTransport
	}
	if config.Now == nil {
		config.Now = time.Now
	}
	if config.Random == nil {
		config.Random = rand.Reader
	}
	if config.WakeTTL <= 0 {
		config.WakeTTL = 10 * time.Minute
	}
	return &Server{
		runtime: config.RuntimeOrigin, pairingHash: sha256.Sum256([]byte(config.PairingCode)),
		adminHash: sha256.Sum256([]byte(config.AdminToken)), grantTTL: config.GrantTTL,
		maxBodyBytes: config.MaxBodyBytes, transport: config.Transport, now: config.Now,
		random: config.Random, pushSender: config.PushSender, wakeTTL: config.WakeTTL,
		pairingUnused: true, grants: make(map[[32]byte]*grant), devices: make(map[string]*grant),
		wakeRoutes: make(map[[32]byte]wakeRoute),
	}, nil
}

func (s *Server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Cache-Control", "no-store")
	w.Header().Set("X-Content-Type-Options", "nosniff")
	switch {
	case r.Method == http.MethodPost && r.URL.Path == "/v1/mobile/pair":
		s.pair(w, r)
	case r.Method == http.MethodPost && r.URL.Path == "/v1/mobile/grants/self:revoke":
		s.revokeSelf(w, r)
	case r.Method == http.MethodPost && r.URL.Path == "/v1/mobile/push/registrations":
		s.registerPush(w, r)
	case r.Method == http.MethodDelete && r.URL.Path == "/v1/mobile/push/registrations/self":
		s.unregisterPush(w, r)
	case r.Method == http.MethodPost && r.URL.Path == "/v1/mobile/wake-hints":
		s.dispatchWake(w, r)
	case r.Method == http.MethodPost && strings.HasPrefix(r.URL.Path, "/v1/mobile/wake/") && strings.HasSuffix(r.URL.Path, ":resolve"):
		s.resolveWake(w, r)
	case r.Method == http.MethodPost && strings.HasPrefix(r.URL.Path, "/v1/mobile/devices/") && strings.HasSuffix(r.URL.Path, ":revoke"):
		s.revoke(w, r)
	case admittedRoute.MatchString(r.URL.Path) && admittedMethod(r.Method, r.URL.Path):
		s.proxy(w, r)
	default:
		writeError(w, http.StatusNotFound, "route_not_admitted")
	}
}

func (s *Server) revokeSelf(w http.ResponseWriter, r *http.Request) {
	token, ok := bearer(r.Header.Get("Authorization"))
	if !ok {
		writeError(w, http.StatusUnauthorized, "authentication_required")
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	value := s.grants[sha256.Sum256([]byte(token))]
	if value == nil || value.revoked || !s.now().Before(value.expires) {
		writeError(w, http.StatusUnauthorized, "authentication_required")
		return
	}
	value.revoked = true
	w.WriteHeader(http.StatusNoContent)
}

type pairRequest struct {
	APIVersion string `json:"api_version"`
	Code       string `json:"code"`
	DeviceName string `json:"device_name"`
	Platform   string `json:"platform"`
	PublicKey  string `json:"device_public_key"`
}

func (s *Server) pair(w http.ResponseWriter, r *http.Request) {
	var request pairRequest
	if !decodeStrict(w, r, s.maxBodyBytes, &request) {
		return
	}
	key, keyError := base64.RawURLEncoding.DecodeString(request.PublicKey)
	if request.APIVersion != apiVersion || len(request.DeviceName) == 0 || len(request.DeviceName) > 100 ||
		(request.Platform != "ios" && request.Platform != "android") || keyError != nil || len(key) < 32 || len(key) > 2048 {
		writeError(w, http.StatusBadRequest, "invalid_pairing_request")
		return
	}
	provided := sha256.Sum256([]byte(request.Code))
	s.mu.Lock()
	defer s.mu.Unlock()
	if !s.pairingUnused || subtle.ConstantTimeCompare(provided[:], s.pairingHash[:]) != 1 {
		writeError(w, http.StatusUnauthorized, "pairing_rejected")
		return
	}
	tokenBytes := make([]byte, 32)
	deviceBytes := make([]byte, 16)
	if _, err := io.ReadFull(s.random, tokenBytes); err != nil {
		writeError(w, http.StatusServiceUnavailable, "entropy_unavailable")
		return
	}
	if _, err := io.ReadFull(s.random, deviceBytes); err != nil {
		writeError(w, http.StatusServiceUnavailable, "entropy_unavailable")
		return
	}
	token := base64.RawURLEncoding.EncodeToString(tokenBytes)
	deviceID := base64.RawURLEncoding.EncodeToString(deviceBytes)
	value := &grant{deviceID: deviceID, platform: request.Platform, expires: s.now().Add(s.grantTTL)}
	s.grants[sha256.Sum256([]byte(token))] = value
	s.devices[deviceID] = value
	s.pairingUnused = false
	writeJSON(w, http.StatusCreated, map[string]any{
		"api_version": apiVersion, "access_grant": token, "device_id": deviceID,
		"expires_at": value.expires.UTC().Format(time.RFC3339),
	})
}

func (s *Server) revoke(w http.ResponseWriter, r *http.Request) {
	if !s.isAdmin(r.Header.Get("Authorization")) {
		writeError(w, http.StatusUnauthorized, "authentication_required")
		return
	}
	deviceID := strings.TrimSuffix(strings.TrimPrefix(r.URL.Path, "/v1/mobile/devices/"), ":revoke")
	s.mu.Lock()
	defer s.mu.Unlock()
	value := s.devices[deviceID]
	if value == nil {
		writeError(w, http.StatusNotFound, "device_not_found")
		return
	}
	value.revoked = true
	w.WriteHeader(http.StatusNoContent)
}

func (s *Server) proxy(w http.ResponseWriter, r *http.Request) {
	if !s.authorized(r.Header.Get("Authorization")) {
		writeError(w, http.StatusUnauthorized, "authentication_required")
		return
	}
	if r.ContentLength > s.maxBodyBytes {
		writeError(w, http.StatusRequestEntityTooLarge, "request_too_large")
		return
	}
	target := *s.runtime
	target.Path, target.RawPath, target.RawQuery = r.URL.Path, r.URL.RawPath, r.URL.RawQuery
	body := http.MaxBytesReader(w, r.Body, s.maxBodyBytes)
	request, err := http.NewRequestWithContext(r.Context(), r.Method, target.String(), body)
	if err != nil {
		writeError(w, http.StatusBadRequest, "invalid_request")
		return
	}
	for _, name := range []string{"Accept", "Content-Type", "Idempotency-Key", "Last-Event-ID"} {
		if value := r.Header.Get(name); value != "" {
			request.Header.Set(name, value)
		}
	}
	response, err := s.transport.RoundTrip(request)
	if err != nil {
		writeError(w, http.StatusServiceUnavailable, "runtime_unavailable")
		return
	}
	defer response.Body.Close()
	for _, name := range []string{"Content-Type", "Retry-After"} {
		if value := response.Header.Get(name); value != "" {
			w.Header().Set(name, value)
		}
	}
	w.WriteHeader(response.StatusCode)
	buffer := make([]byte, 32*1024)
	for {
		count, readError := response.Body.Read(buffer)
		if count > 0 {
			if _, err = w.Write(buffer[:count]); err != nil {
				return
			}
			if flusher, ok := w.(http.Flusher); ok {
				flusher.Flush()
			}
		}
		if readError != nil {
			return
		}
	}
}

func (s *Server) authorized(header string) bool {
	token, ok := bearer(header)
	if !ok {
		return false
	}
	s.mu.RLock()
	defer s.mu.RUnlock()
	value := s.grants[sha256.Sum256([]byte(token))]
	return value != nil && !value.revoked && s.now().Before(value.expires)
}

func (s *Server) isAdmin(header string) bool {
	token, ok := bearer(header)
	if !ok {
		return false
	}
	value := sha256.Sum256([]byte(token))
	return subtle.ConstantTimeCompare(value[:], s.adminHash[:]) == 1
}

func bearer(header string) (string, bool) {
	if !strings.HasPrefix(header, "Bearer ") || strings.Count(header, " ") != 1 {
		return "", false
	}
	token := strings.TrimPrefix(header, "Bearer ")
	return token, len(token) >= 20 && len(token) <= 4096
}

func admittedMethod(method, path string) bool {
	if method == http.MethodGet {
		return path == "/v1/agent-definitions" || strings.HasPrefix(path, "/v1/sessions")
	}
	return method == http.MethodPost && (path == "/v1/sessions" || strings.Contains(path, "/turns"))
}

func isLoopback(host string) bool {
	ip := net.ParseIP(host)
	return host == "localhost" || ip != nil && ip.IsLoopback()
}

func decodeStrict(w http.ResponseWriter, r *http.Request, limit int64, value any) bool {
	decoder := json.NewDecoder(http.MaxBytesReader(w, r.Body, limit))
	decoder.DisallowUnknownFields()
	if decoder.Decode(value) != nil || decoder.Decode(&struct{}{}) != io.EOF {
		writeError(w, http.StatusBadRequest, "invalid_json")
		return false
	}
	return true
}

func writeError(w http.ResponseWriter, status int, code string) {
	writeJSON(w, status, map[string]string{"code": code})
}
func writeJSON(w http.ResponseWriter, status int, value any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(value)
}
