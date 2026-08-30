package main

import (
	"context"
	"crypto/tls"
	"errors"
	"log"
	"net/http"
	"net/url"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/garive/runtime-gateway/internal/gateway"
)

func main() {
	if err := run(); err != nil {
		log.Fatal("gateway stopped: ", safeError(err))
	}
}

func run() error {
	runtimeOrigin, err := url.Parse(environment("GARIVE_RUNTIME_ORIGIN", "http://127.0.0.1:4317"))
	if err != nil {
		return errors.New("invalid runtime configuration")
	}
	pushSender, err := configuredPushSender()
	if err != nil {
		return err
	}
	handler, err := gateway.New(gateway.Config{
		RuntimeOrigin: runtimeOrigin,
		PairingCode:   os.Getenv("GARIVE_PAIRING_CODE"),
		AdminToken:    os.Getenv("GARIVE_ADMIN_TOKEN"),
		GrantTTL:      30 * 24 * time.Hour,
		MaxBodyBytes:  64 * 1024,
		PushSender:    pushSender,
		WakeTTL:       10 * time.Minute,
	})
	if err != nil {
		return err
	}
	certificate := os.Getenv("GARIVE_TLS_CERT")
	privateKey := os.Getenv("GARIVE_TLS_KEY")
	if certificate == "" || privateKey == "" {
		return errors.New("TLS certificate configuration is required")
	}

	server := &http.Server{
		Addr:              environment("GARIVE_GATEWAY_LISTEN", ":8443"),
		Handler:           handler,
		ReadHeaderTimeout: 10 * time.Second,
		IdleTimeout:       75 * time.Second,
		MaxHeaderBytes:    16 * 1024,
		TLSConfig:         &tls.Config{MinVersion: tls.VersionTLS13},
	}
	shutdownContext, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	go func() {
		<-shutdownContext.Done()
		deadline, cancel := context.WithTimeout(context.Background(), 15*time.Second)
		defer cancel()
		_ = server.Shutdown(deadline)
	}()
	log.Print("Garive mobile gateway ready")
	err = server.ListenAndServeTLS(certificate, privateKey)
	if errors.Is(err, http.ErrServerClosed) {
		return nil
	}
	return err
}

func configuredPushSender() (gateway.PushSender, error) {
	var providers gateway.MultiplexPushSender
	apnsKeyPath := os.Getenv("GARIVE_APNS_KEY_FILE")
	apnsConfigured := apnsKeyPath != "" || os.Getenv("GARIVE_APNS_TEAM_ID") != "" ||
		os.Getenv("GARIVE_APNS_KEY_ID") != "" || os.Getenv("GARIVE_APNS_TOPIC") != ""
	if apnsConfigured {
		key, err := os.ReadFile(apnsKeyPath)
		if err != nil {
			return nil, errors.New("invalid APNs configuration")
		}
		sandboxValue := os.Getenv("GARIVE_APNS_SANDBOX")
		if sandboxValue != "" && sandboxValue != "true" && sandboxValue != "false" {
			return nil, errors.New("invalid APNs configuration")
		}
		providers.APNS, err = gateway.NewAPNSSender(gateway.APNSConfig{
			TeamID: os.Getenv("GARIVE_APNS_TEAM_ID"), KeyID: os.Getenv("GARIVE_APNS_KEY_ID"),
			Topic: os.Getenv("GARIVE_APNS_TOPIC"), PrivateKey: key, Sandbox: sandboxValue == "true",
		})
		if err != nil {
			return nil, err
		}
	}
	if credentials := os.Getenv("GARIVE_FCM_CREDENTIALS"); credentials != "" {
		var err error
		providers.FCM, err = gateway.NewFCMSenderFromFile(credentials)
		if err != nil {
			return nil, err
		}
	}
	if providers.APNS == nil && providers.FCM == nil {
		return nil, nil
	}
	return providers, nil
}

func environment(name, fallback string) string {
	if value := os.Getenv(name); value != "" {
		return value
	}
	return fallback
}

func safeError(err error) string {
	switch err.Error() {
	case "runtime origin must be a bare loopback HTTP origin", "pairing code or admin token is outside bounds",
		"TLS certificate configuration is required", "invalid runtime configuration",
		"invalid APNs configuration", "invalid FCM configuration":
		return err.Error()
	default:
		return "service_failure"
	}
}
