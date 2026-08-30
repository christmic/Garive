package gateway

import (
	"context"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/rsa"
	"crypto/x509"
	"encoding/json"
	"encoding/pem"
	"io"
	"net/http"
	"strings"
	"testing"
	"time"
)

func TestAPNSSenderUsesBackgroundHeadersAndContentFreePayload(t *testing.T) {
	key, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	encoded, _ := x509.MarshalPKCS8PrivateKey(key)
	var observed *http.Request
	var payload []byte
	client := &http.Client{Transport: roundTripFunc(func(request *http.Request) (*http.Response, error) {
		observed = request
		payload, _ = io.ReadAll(request.Body)
		return providerResponse(http.StatusOK, `{}`), nil
	})}
	sender, err := NewAPNSSender(APNSConfig{
		TeamID: "TEAMID1234", KeyID: "KEYID12345", Topic: "com.garive.mobile",
		PrivateKey: pem.EncodeToMemory(&pem.Block{Type: "PRIVATE KEY", Bytes: encoded}),
		Client:     client, Now: func() time.Time { return time.Unix(1_788_070_400, 0) },
	})
	if err != nil {
		t.Fatal(err)
	}
	hint := testWakeHint()
	registration := PushRegistration{Transport: "apns", Address: strings.Repeat("ab", 32)}
	if err = sender.Send(context.Background(), registration, hint); err != nil {
		t.Fatal(err)
	}
	if observed.URL.Host != "api.push.apple.com" ||
		observed.Header.Get("apns-push-type") != "alert" || observed.Header.Get("apns-priority") != "10" ||
		observed.Header.Get("apns-topic") != "com.garive.mobile" ||
		!strings.HasPrefix(observed.Header.Get("authorization"), "bearer ") {
		t.Fatalf("invalid APNs request: %#v", observed.Header)
	}
	var body map[string]any
	if json.Unmarshal(payload, &body) != nil || strings.Contains(string(payload), "session") ||
		!strings.Contains(string(payload), `"content-available":1`) || !strings.Contains(string(payload), "Garive update") ||
		!strings.Contains(string(payload), hint.RouteToken) {
		t.Fatalf("unsafe APNs payload: %s", payload)
	}
}

func TestFCMSenderUsesOAuthDataMessageAndCachesToken(t *testing.T) {
	key, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	encoded := x509.MarshalPKCS1PrivateKey(key)
	now := time.Unix(1_788_070_400, 0)
	tokenCalls, messageCalls := 0, 0
	var message []byte
	client := &http.Client{Transport: roundTripFunc(func(request *http.Request) (*http.Response, error) {
		switch request.URL.Host {
		case "oauth.example.test":
			tokenCalls++
			body, _ := io.ReadAll(request.Body)
			if request.Method != http.MethodPost || !strings.Contains(string(body), "grant_type=") ||
				!strings.Contains(string(body), "assertion=") {
				t.Fatalf("invalid OAuth request: %s", body)
			}
			return providerResponse(http.StatusOK, `{"access_token":"provider-access","expires_in":3600}`), nil
		case "fcm.googleapis.com":
			messageCalls++
			message, _ = io.ReadAll(request.Body)
			if request.Header.Get("authorization") != "Bearer provider-access" {
				t.Fatal("missing FCM access token")
			}
			return providerResponse(http.StatusOK, `{"name":"accepted"}`), nil
		default:
			t.Fatalf("unexpected host %s", request.URL.Host)
			return nil, nil
		}
	})}
	sender, err := NewFCMSender(FCMConfig{
		ProjectID: "garive-project", ClientEmail: "gateway@example.test",
		PrivateKey: pem.EncodeToMemory(&pem.Block{Type: "RSA PRIVATE KEY", Bytes: encoded}),
		TokenURI:   "https://oauth.example.test/token", Client: client, Now: func() time.Time { return now },
	})
	if err != nil {
		t.Fatal(err)
	}
	registration := PushRegistration{Transport: "fcm", Address: "long-enough-fcm-registration-token"}
	if err = sender.Send(context.Background(), registration, testWakeHint()); err != nil {
		t.Fatal(err)
	}
	if err = sender.Send(context.Background(), registration, testWakeHint()); err != nil {
		t.Fatal(err)
	}
	if tokenCalls != 1 || messageCalls != 2 {
		t.Fatalf("token calls = %d message calls = %d", tokenCalls, messageCalls)
	}
	contents := string(message)
	if strings.Contains(contents, "notification") || strings.Contains(contents, "session") ||
		!strings.Contains(contents, `"priority":"high"`) || !strings.Contains(contents, `"route_token"`) {
		t.Fatalf("unsafe FCM payload: %s", contents)
	}
}

func TestProviderFailuresExposeNoRemoteResponse(t *testing.T) {
	key, _ := rsa.GenerateKey(rand.Reader, 2048)
	sender, err := NewFCMSender(FCMConfig{
		ProjectID: "garive-project", ClientEmail: "gateway@example.test",
		PrivateKey: pem.EncodeToMemory(&pem.Block{Type: "RSA PRIVATE KEY", Bytes: x509.MarshalPKCS1PrivateKey(key)}),
		TokenURI:   "https://oauth.example.test/token",
		Client: &http.Client{Transport: roundTripFunc(func(*http.Request) (*http.Response, error) {
			return providerResponse(http.StatusUnauthorized, `{"error":"provider-secret-detail"}`), nil
		})},
	})
	if err != nil {
		t.Fatal(err)
	}
	err = sender.Send(context.Background(), PushRegistration{Transport: "fcm", Address: "long-enough-fcm-registration-token"}, testWakeHint())
	if err == nil || strings.Contains(err.Error(), "provider-secret-detail") {
		t.Fatalf("unsafe provider error: %v", err)
	}
}

func testWakeHint() MobileWakeHint {
	return MobileWakeHint{
		SchemaVersion: 1, RouteToken: strings.Repeat("r", 43),
		Category: "attention", CollapseKey: "attention",
	}
}

func providerResponse(status int, body string) *http.Response {
	return &http.Response{
		StatusCode: status, Header: make(http.Header),
		Body: io.NopCloser(strings.NewReader(body)),
	}
}
