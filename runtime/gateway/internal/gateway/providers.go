package gateway

import (
	"bytes"
	"context"
	"crypto"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/rsa"
	"crypto/sha256"
	"crypto/x509"
	"encoding/base64"
	"encoding/json"
	"encoding/pem"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strings"
	"sync"
	"time"
)

// MultiplexPushSender selects the provider solely from the platform-bound registration.
type MultiplexPushSender struct {
	APNS PushSender
	FCM  PushSender
}

func (sender MultiplexPushSender) Send(ctx context.Context, registration PushRegistration, hint MobileWakeHint) error {
	switch registration.Transport {
	case "apns":
		if sender.APNS != nil {
			return sender.APNS.Send(ctx, registration, hint)
		}
	case "fcm":
		if sender.FCM != nil {
			return sender.FCM.Send(ctx, registration, hint)
		}
	}
	return errors.New("push provider unavailable")
}

type APNSConfig struct {
	TeamID     string
	KeyID      string
	Topic      string
	PrivateKey []byte
	Sandbox    bool
	Client     *http.Client
	Now        func() time.Time
}

type apnsSender struct {
	teamID, keyID, topic, endpoint string
	key                            *ecdsa.PrivateKey
	client                         *http.Client
	now                            func() time.Time
	mu                             sync.Mutex
	jwt                            string
	jwtCreated                     time.Time
}

func NewAPNSSender(config APNSConfig) (PushSender, error) {
	if len(config.TeamID) != 10 || len(config.KeyID) != 10 || config.Topic == "" {
		return nil, errors.New("invalid APNs configuration")
	}
	key, err := parseECPrivateKey(config.PrivateKey)
	if err != nil || key.Curve != elliptic.P256() {
		return nil, errors.New("invalid APNs configuration")
	}
	if config.Client == nil {
		config.Client = &http.Client{Timeout: 15 * time.Second, Transport: &http.Transport{ForceAttemptHTTP2: true}}
	}
	if config.Now == nil {
		config.Now = time.Now
	}
	endpoint := "https://api.push.apple.com"
	if config.Sandbox {
		endpoint = "https://api.development.push.apple.com"
	}
	return &apnsSender{
		teamID: config.TeamID, keyID: config.KeyID, topic: config.Topic, endpoint: endpoint,
		key: key, client: config.Client, now: config.Now,
	}, nil
}

func (sender *apnsSender) Send(ctx context.Context, registration PushRegistration, hint MobileWakeHint) error {
	if registration.Transport != "apns" || !isHexToken(registration.Address) {
		return errors.New("invalid APNs registration")
	}
	payload, _ := json.Marshal(map[string]any{
		"aps": map[string]any{
			"content-available": 1,
			"alert": map[string]string{"title": "Garive update", "body": "Open Garive to refresh verified server state"},
		},
		"garive": hint,
	})
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, sender.endpoint+"/3/device/"+registration.Address, bytes.NewReader(payload))
	if err != nil {
		return errors.New("APNs request failed")
	}
	token, err := sender.providerToken()
	if err != nil {
		return err
	}
	request.Header.Set("authorization", "bearer "+token)
	request.Header.Set("content-type", "application/json")
	request.Header.Set("apns-topic", sender.topic)
	request.Header.Set("apns-push-type", "alert")
	priority := "5"
	if hint.Category == "attention" || hint.Category == "connection_security" {
		priority = "10"
	}
	request.Header.Set("apns-priority", priority)
	request.Header.Set("apns-expiration", fmt.Sprint(sender.now().Add(10*time.Minute).Unix()))
	request.Header.Set("apns-collapse-id", hint.CollapseKey)
	response, err := sender.client.Do(request)
	if err != nil {
		return errors.New("APNs delivery failed")
	}
	defer response.Body.Close()
	_, _ = io.Copy(io.Discard, io.LimitReader(response.Body, 4096))
	if response.StatusCode != http.StatusOK {
		return errors.New("APNs delivery failed")
	}
	return nil
}

func (sender *apnsSender) providerToken() (string, error) {
	sender.mu.Lock()
	defer sender.mu.Unlock()
	if sender.jwt != "" && sender.now().Sub(sender.jwtCreated) < 50*time.Minute {
		return sender.jwt, nil
	}
	header := base64JSON(map[string]any{"alg": "ES256", "kid": sender.keyID})
	claims := base64JSON(map[string]any{"iss": sender.teamID, "iat": sender.now().Unix()})
	input := header + "." + claims
	digest := sha256.Sum256([]byte(input))
	r, s, err := ecdsa.Sign(rand.Reader, sender.key, digest[:])
	if err != nil {
		return "", errors.New("APNs authentication failed")
	}
	signature := append(r.FillBytes(make([]byte, 32)), s.FillBytes(make([]byte, 32))...)
	sender.jwt = input + "." + base64.RawURLEncoding.EncodeToString(signature)
	sender.jwtCreated = sender.now()
	return sender.jwt, nil
}

type FCMConfig struct {
	ProjectID   string
	ClientEmail string
	PrivateKey  []byte
	TokenURI    string
	Client      *http.Client
	Now         func() time.Time
}

type fcmSender struct {
	projectID, clientEmail, tokenURI string
	key                              *rsa.PrivateKey
	client                           *http.Client
	now                              func() time.Time
	mu                               sync.Mutex
	accessToken                      string
	tokenExpiry                      time.Time
}

func NewFCMSenderFromFile(path string) (PushSender, error) {
	contents, err := os.ReadFile(path)
	if err != nil {
		return nil, errors.New("invalid FCM configuration")
	}
	var account struct {
		ProjectID   string `json:"project_id"`
		ClientEmail string `json:"client_email"`
		PrivateKey  string `json:"private_key"`
		TokenURI    string `json:"token_uri"`
	}
	if json.Unmarshal(contents, &account) != nil {
		return nil, errors.New("invalid FCM configuration")
	}
	return NewFCMSender(FCMConfig{
		ProjectID: account.ProjectID, ClientEmail: account.ClientEmail,
		PrivateKey: []byte(account.PrivateKey), TokenURI: account.TokenURI,
	})
}

func NewFCMSender(config FCMConfig) (PushSender, error) {
	key, err := parseRSAPrivateKey(config.PrivateKey)
	tokenURL, urlError := url.Parse(config.TokenURI)
	if err != nil || urlError != nil || tokenURL.Scheme != "https" || tokenURL.Host == "" ||
		config.ProjectID == "" || config.ClientEmail == "" {
		return nil, errors.New("invalid FCM configuration")
	}
	if config.Client == nil {
		config.Client = &http.Client{Timeout: 15 * time.Second}
	}
	if config.Now == nil {
		config.Now = time.Now
	}
	return &fcmSender{
		projectID: config.ProjectID, clientEmail: config.ClientEmail, tokenURI: config.TokenURI,
		key: key, client: config.Client, now: config.Now,
	}, nil
}

func (sender *fcmSender) Send(ctx context.Context, registration PushRegistration, hint MobileWakeHint) error {
	if registration.Transport != "fcm" || !validPushToken(registration.Address) {
		return errors.New("invalid FCM registration")
	}
	token, err := sender.oauthToken(ctx)
	if err != nil {
		return err
	}
	priority := "normal"
	if hint.Category == "attention" || hint.Category == "connection_security" {
		priority = "high"
	}
	body, _ := json.Marshal(map[string]any{"message": map[string]any{
		"fid": registration.Address,
		"data": map[string]string{
			"schema_version": "1", "route_token": hint.RouteToken,
			"category": hint.Category, "collapse_key": hint.CollapseKey,
		},
		"android": map[string]string{"priority": priority, "collapse_key": hint.CollapseKey},
	}})
	endpoint := "https://fcm.googleapis.com/v1/projects/" + url.PathEscape(sender.projectID) + "/messages:send"
	request, _ := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, bytes.NewReader(body))
	request.Header.Set("authorization", "Bearer "+token)
	request.Header.Set("content-type", "application/json; charset=utf-8")
	response, err := sender.client.Do(request)
	if err != nil {
		return errors.New("FCM delivery failed")
	}
	defer response.Body.Close()
	_, _ = io.Copy(io.Discard, io.LimitReader(response.Body, 4096))
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return errors.New("FCM delivery failed")
	}
	return nil
}

func (sender *fcmSender) oauthToken(ctx context.Context) (string, error) {
	sender.mu.Lock()
	defer sender.mu.Unlock()
	if sender.accessToken != "" && sender.now().Add(time.Minute).Before(sender.tokenExpiry) {
		return sender.accessToken, nil
	}
	header := base64JSON(map[string]any{"alg": "RS256", "typ": "JWT"})
	claims := base64JSON(map[string]any{
		"iss": sender.clientEmail, "scope": "https://www.googleapis.com/auth/firebase.messaging",
		"aud": sender.tokenURI, "iat": sender.now().Unix(), "exp": sender.now().Add(time.Hour).Unix(),
	})
	input := header + "." + claims
	digest := sha256.Sum256([]byte(input))
	signature, err := rsa.SignPKCS1v15(rand.Reader, sender.key, crypto.SHA256, digest[:])
	if err != nil {
		return "", errors.New("FCM authentication failed")
	}
	assertion := input + "." + base64.RawURLEncoding.EncodeToString(signature)
	form := url.Values{"grant_type": {"urn:ietf:params:oauth:grant-type:jwt-bearer"}, "assertion": {assertion}}
	request, _ := http.NewRequestWithContext(ctx, http.MethodPost, sender.tokenURI, strings.NewReader(form.Encode()))
	request.Header.Set("content-type", "application/x-www-form-urlencoded")
	response, err := sender.client.Do(request)
	if err != nil {
		return "", errors.New("FCM authentication failed")
	}
	defer response.Body.Close()
	var result struct {
		AccessToken string `json:"access_token"`
		ExpiresIn   int64  `json:"expires_in"`
	}
	decoder := json.NewDecoder(io.LimitReader(response.Body, 16*1024))
	if response.StatusCode != http.StatusOK || decoder.Decode(&result) != nil || result.AccessToken == "" || result.ExpiresIn <= 0 {
		return "", errors.New("FCM authentication failed")
	}
	sender.accessToken = result.AccessToken
	sender.tokenExpiry = sender.now().Add(time.Duration(result.ExpiresIn) * time.Second)
	return sender.accessToken, nil
}

func parseECPrivateKey(contents []byte) (*ecdsa.PrivateKey, error) {
	block, _ := pem.Decode(contents)
	if block == nil {
		return nil, errors.New("invalid key")
	}
	if value, err := x509.ParsePKCS8PrivateKey(block.Bytes); err == nil {
		key, ok := value.(*ecdsa.PrivateKey)
		if ok {
			return key, nil
		}
	}
	return x509.ParseECPrivateKey(block.Bytes)
}

func parseRSAPrivateKey(contents []byte) (*rsa.PrivateKey, error) {
	block, _ := pem.Decode(contents)
	if block == nil {
		return nil, errors.New("invalid key")
	}
	if value, err := x509.ParsePKCS8PrivateKey(block.Bytes); err == nil {
		key, ok := value.(*rsa.PrivateKey)
		if ok {
			return key, nil
		}
	}
	return x509.ParsePKCS1PrivateKey(block.Bytes)
}

func base64JSON(value any) string {
	contents, _ := json.Marshal(value)
	return base64.RawURLEncoding.EncodeToString(contents)
}

func isHexToken(value string) bool {
	if len(value) < 32 || len(value) > 512 || len(value)%2 != 0 {
		return false
	}
	for _, character := range value {
		if !strings.ContainsRune("0123456789abcdefABCDEF", character) {
			return false
		}
	}
	return true
}
