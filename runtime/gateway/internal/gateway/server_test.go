package gateway

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
	"time"
)

const (
	pairingCode = "single-use-code"
	adminToken  = "admin-token-at-least-twenty-characters"
)

type roundTripFunc func(*http.Request) (*http.Response, error)

func (function roundTripFunc) RoundTrip(request *http.Request) (*http.Response, error) {
	return function(request)
}

func TestNewRejectsNonLoopbackRuntime(t *testing.T) {
	runtime, _ := url.Parse("https://runtime.example.test")
	_, err := New(Config{RuntimeOrigin: runtime, PairingCode: pairingCode, AdminToken: adminToken})
	if err == nil {
		t.Fatal("expected non-loopback Runtime rejection")
	}
}

func TestPairIsStrictAndSingleUse(t *testing.T) {
	server := newServer(t, nil, time.Now)
	request := pairBody(pairingCode)
	response := serve(server, http.MethodPost, "/v1/mobile/pair", request, "")
	if response.Code != http.StatusCreated {
		t.Fatalf("pair status = %d, body = %s", response.Code, response.Body.String())
	}
	var result struct {
		APIVersion string `json:"api_version"`
		Grant      string `json:"access_grant"`
		DeviceID   string `json:"device_id"`
	}
	if err := json.Unmarshal(response.Body.Bytes(), &result); err != nil {
		t.Fatal(err)
	}
	if result.APIVersion != apiVersion || len(result.Grant) < 40 || result.DeviceID == "" {
		t.Fatalf("invalid pair response: %#v", result)
	}

	replay := serve(server, http.MethodPost, "/v1/mobile/pair", request, "")
	assertError(t, replay, http.StatusUnauthorized, "pairing_rejected")
	unknown := strings.TrimSuffix(request, "}") + `,"extra":true}`
	assertError(t, serve(newServer(t, nil, time.Now), http.MethodPost, "/v1/mobile/pair", unknown, ""), http.StatusBadRequest, "invalid_json")
}

func TestAuthenticatedProxyPreservesContractAndStripsGrant(t *testing.T) {
	var observed *http.Request
	var body string
	transport := roundTripFunc(func(request *http.Request) (*http.Response, error) {
		observed = request
		value, _ := io.ReadAll(request.Body)
		body = string(value)
		return &http.Response{
			StatusCode: http.StatusAccepted,
			Header:     http.Header{"Content-Type": []string{"application/json"}},
			Body:       io.NopCloser(strings.NewReader(`{"api_version":"v1"}`)),
		}, nil
	})
	server := newServer(t, transport, time.Now)
	grant, _ := pair(t, server)
	response := serve(server, http.MethodPost, "/v1/sessions/session_1/turns?trace=redacted", `{"text":"work"}`, grant)
	if response.Code != http.StatusAccepted {
		t.Fatalf("proxy status = %d", response.Code)
	}
	if observed.URL.Scheme != "http" || observed.URL.Host != "127.0.0.1:4317" ||
		observed.URL.Path != "/v1/sessions/session_1/turns" || observed.URL.RawQuery != "trace=redacted" {
		t.Fatalf("unexpected target: %s", observed.URL)
	}
	if observed.Header.Get("Authorization") != "" || body != `{"text":"work"}` {
		t.Fatal("credential leaked or body changed")
	}
	if response.Header().Get("Cache-Control") != "no-store" {
		t.Fatal("missing no-store")
	}
}

func TestAbsentExpiredAndRevokedGrantsFailClosed(t *testing.T) {
	now := time.Date(2026, 8, 30, 0, 0, 0, 0, time.UTC)
	clock := func() time.Time { return now }
	server := newServer(t, okTransport(), clock)
	assertError(t, serve(server, http.MethodGet, "/v1/sessions?limit=10", "", ""), http.StatusUnauthorized, "authentication_required")
	grant, deviceID := pair(t, server)
	if got := serve(server, http.MethodGet, "/v1/sessions?limit=10", "", grant); got.Code != http.StatusOK {
		t.Fatalf("valid status = %d", got.Code)
	}

	revoke := serve(server, http.MethodPost, "/v1/mobile/devices/"+deviceID+":revoke", "", adminToken)
	if revoke.Code != http.StatusNoContent {
		t.Fatalf("revoke status = %d", revoke.Code)
	}
	assertError(t, serve(server, http.MethodGet, "/v1/sessions?limit=10", "", grant), http.StatusUnauthorized, "authentication_required")

	expiring := newServer(t, okTransport(), clock)
	expiredGrant, _ := pair(t, expiring)
	now = now.Add(31 * 24 * time.Hour)
	assertError(t, serve(expiring, http.MethodGet, "/v1/agent-definitions", "", expiredGrant), http.StatusUnauthorized, "authentication_required")
}

func TestRouteAndMethodAdmission(t *testing.T) {
	server := newServer(t, okTransport(), time.Now)
	grant, _ := pair(t, server)
	for _, path := range []string{"/v1/internal", "/v1/sessions/../secret", "/v1/turns/a:delete", "/v1/mobile/pair/extra"} {
		assertError(t, serve(server, http.MethodGet, path, "", grant), http.StatusNotFound, "route_not_admitted")
	}
	assertError(t, serve(server, http.MethodDelete, "/v1/sessions", "", grant), http.StatusNotFound, "route_not_admitted")
}

func newServer(t *testing.T, transport http.RoundTripper, now func() time.Time) *Server {
	t.Helper()
	runtime, _ := url.Parse("http://127.0.0.1:4317")
	random := bytes.NewReader(bytes.Repeat([]byte{0x5a}, 128))
	server, err := New(Config{
		RuntimeOrigin: runtime, PairingCode: pairingCode, AdminToken: adminToken,
		GrantTTL: 30 * 24 * time.Hour, Transport: transport, Now: now, Random: random,
	})
	if err != nil {
		t.Fatal(err)
	}
	return server
}

func pair(t *testing.T, server *Server) (string, string) {
	t.Helper()
	response := serve(server, http.MethodPost, "/v1/mobile/pair", pairBody(pairingCode), "")
	var result struct {
		Grant    string `json:"access_grant"`
		DeviceID string `json:"device_id"`
	}
	if err := json.Unmarshal(response.Body.Bytes(), &result); err != nil {
		t.Fatal(err)
	}
	return result.Grant, result.DeviceID
}

func pairBody(code string) string {
	key := base64.RawURLEncoding.EncodeToString(bytes.Repeat([]byte{0x42}, 32))
	return `{"api_version":"v1","code":"` + code + `","device_name":"Test phone","platform":"ios","device_public_key":"` + key + `"}`
}

func serve(server *Server, method, path, body, token string) *httptest.ResponseRecorder {
	request := httptest.NewRequest(method, path, strings.NewReader(body))
	if token != "" {
		request.Header.Set("Authorization", "Bearer "+token)
	}
	response := httptest.NewRecorder()
	server.ServeHTTP(response, request)
	return response
}

func okTransport() http.RoundTripper {
	return roundTripFunc(func(*http.Request) (*http.Response, error) {
		return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader("{}"))}, nil
	})
}

func assertError(t *testing.T, response *httptest.ResponseRecorder, status int, code string) {
	t.Helper()
	if response.Code != status || !strings.Contains(response.Body.String(), `"code":"`+code+`"`) {
		t.Fatalf("got status %d body %s", response.Code, response.Body.String())
	}
}
