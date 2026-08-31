use garive_adapter_browser_attached::{AttachedConfig, AttachedConfigError, AttachedLimits};

const ORIGIN: &str = "chrome-extension://abcdefghijklmnopabcdefghijklmnop/";

#[test]
fn construction_is_exact_and_has_no_discovery_input() {
    let limits = AttachedLimits::new(1_048_576).expect("limits");
    let config = AttachedConfig::new(ORIGIN, limits).expect("config");
    assert_eq!(config.expected_extension_origin(), ORIGIN);
    assert_eq!(config.limits(), limits);
    assert_eq!(config.admit_caller(ORIGIN), Ok(()));
    assert_eq!(
        config.admit_caller("chrome-extension://pppppppppppppppppppppppppppppppp/"),
        Err(AttachedConfigError::CallerDenied)
    );
}

#[test]
fn invalid_limits_and_non_exact_origins_fail_closed() {
    assert_eq!(
        AttachedLimits::new(0),
        Err(AttachedConfigError::InvalidLimit)
    );
    assert_eq!(
        AttachedLimits::new(1_048_577),
        Err(AttachedConfigError::InvalidLimit)
    );
    let limits = AttachedLimits::new(1024).expect("limits");
    for origin in [
        "https://example.test/",
        "chrome-extension://abcdefghijklmnopabcdefghijklmnop",
        "chrome-extension://abcdefghijklmnopabcdefghijklmnop/path",
        "chrome-extension://zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz/",
    ] {
        assert_eq!(
            AttachedConfig::new(origin, limits),
            Err(AttachedConfigError::InvalidExtensionOrigin)
        );
    }
}
