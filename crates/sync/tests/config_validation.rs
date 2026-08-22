//! Configurations the simulator refuses.
//!
//! Covers AC9: a run duration of zero, or a ppm skew or jitter parameter
//! outside the documented valid range, is rejected and no playout-error result
//! is reported for it.
//!
//! The ranges themselves are in `docs/decisions/0006-sync-simulator-and-servo.md`.

use chorus_sync::{run, ConfigError, JitterModel, SimConfig, MAX_DELAY_US, MAX_SKEW_PPM};

fn valid() -> SimConfig {
    SimConfig::default()
}

#[test]
fn the_default_configuration_is_accepted() {
    // Otherwise every rejection below could be rejecting the wrong thing.
    assert_eq!(valid().validate(), Ok(()));
    assert!(run(&valid()).is_ok());
}

#[test]
fn a_zero_duration_run_is_refused() {
    let config = SimConfig {
        duration_ms: 0,
        ..valid()
    };
    assert_eq!(config.validate(), Err(ConfigError::ZeroDuration));
    assert_eq!(run(&config), Err(ConfigError::ZeroDuration));
}

#[test]
fn a_run_shorter_than_one_step_is_refused() {
    let config = SimConfig {
        duration_ms: 5,
        step_ms: 10,
        ..valid()
    };
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidStep { .. })
    ));
    assert!(run(&config).is_err());
}

#[test]
fn a_zero_step_is_refused() {
    let config = SimConfig {
        step_ms: 0,
        ..valid()
    };
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidStep { .. })
    ));
    assert!(run(&config).is_err());
}

#[test]
fn a_zero_sync_interval_is_refused() {
    let config = SimConfig {
        sync_interval_ms: 0,
        ..valid()
    };
    assert_eq!(config.validate(), Err(ConfigError::ZeroSyncInterval));
    assert!(run(&config).is_err());
}

#[test]
fn a_skew_outside_the_modelled_range_is_refused() {
    let cases: Vec<(&str, f64)> = vec![
        ("just past the limit", MAX_SKEW_PPM + 0.001),
        ("far past the limit", 25_000.0),
        ("negative and past the limit", -MAX_SKEW_PPM - 1.0),
        ("not a number", f64::NAN),
        ("infinite", f64::INFINITY),
    ];

    for (name, ppm) in cases {
        let client = SimConfig {
            client_ppm: ppm,
            ..valid()
        };
        assert!(
            matches!(
                client.validate(),
                Err(ConfigError::SkewOutOfRange { clock: "client", .. })
            ),
            "client case: {}",
            name
        );
        assert!(run(&client).is_err(), "client case: {}", name);

        let server = SimConfig {
            server_ppm: ppm,
            ..valid()
        };
        assert!(
            matches!(
                server.validate(),
                Err(ConfigError::SkewOutOfRange { clock: "server", .. })
            ),
            "server case: {}",
            name
        );
        assert!(run(&server).is_err(), "server case: {}", name);
    }
}

#[test]
fn a_skew_at_the_limit_is_accepted() {
    // The boundary belongs to the accepted side, and the test says so rather
    // than leaving it to be discovered.
    let config = SimConfig {
        client_ppm: MAX_SKEW_PPM,
        server_ppm: -MAX_SKEW_PPM,
        ..valid()
    };
    assert_eq!(config.validate(), Ok(()));
}

#[test]
fn a_jitter_parameter_outside_the_modelled_range_is_refused() {
    let cases: Vec<(&str, JitterModel)> = vec![
        (
            "negative uniform bound",
            JitterModel::Uniform { max_us: -1.0 },
        ),
        (
            "uniform past the limit",
            JitterModel::Uniform {
                max_us: MAX_DELAY_US + 1.0,
            },
        ),
        (
            "negative exponential mean",
            JitterModel::Exponential { mean_us: -0.001 },
        ),
        (
            "exponential mean past the limit",
            JitterModel::Exponential {
                mean_us: 1_000_000.0,
            },
        ),
        (
            "not a number",
            JitterModel::Exponential { mean_us: f64::NAN },
        ),
        (
            "infinite",
            JitterModel::Uniform {
                max_us: f64::INFINITY,
            },
        ),
    ];

    for (name, jitter) in cases {
        let config = SimConfig { jitter, ..valid() };
        assert!(
            matches!(
                config.validate(),
                Err(ConfigError::DelayOutOfRange {
                    parameter: "jitter scale",
                    ..
                })
            ),
            "case: {}",
            name
        );
        assert!(
            run(&config).is_err(),
            "case {}: no playout-error result is reported",
            name
        );
    }
}

#[test]
fn a_base_delay_outside_the_modelled_range_is_refused() {
    for value in [-1.0f64, MAX_DELAY_US + 1.0, f64::NAN, f64::NEG_INFINITY] {
        let config = SimConfig {
            base_one_way_delay_us: value,
            ..valid()
        };
        assert!(
            matches!(
                config.validate(),
                Err(ConfigError::DelayOutOfRange {
                    parameter: "base one way delay",
                    ..
                })
            ),
            "value: {}",
            value
        );
        assert!(run(&config).is_err(), "value: {}", value);
    }
}

#[test]
fn a_servo_that_cannot_correct_anything_is_refused() {
    let mut config = valid();
    config.servo.max_correction_ppm = 0.0;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidServoParameter {
            parameter: "max_correction_ppm",
            ..
        })
    ));

    let mut config = valid();
    config.servo.smoothing_alpha = 1.5;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidServoParameter {
            parameter: "smoothing_alpha",
            ..
        })
    ));

    let mut config = valid();
    config.servo.filter_window = 0;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidServoParameter {
            parameter: "filter_window",
            ..
        })
    ));

    let mut config = valid();
    config.servo.kp = -0.1;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidServoParameter {
            parameter: "kp",
            ..
        })
    ));
}

#[test]
fn a_run_that_would_never_finish_is_refused() {
    let config = SimConfig {
        duration_ms: 1_000_000_000,
        step_ms: 1,
        ..valid()
    };
    assert!(matches!(
        config.validate(),
        Err(ConfigError::TooManySteps { .. })
    ));
    assert!(run(&config).is_err());
}

#[test]
fn a_refused_configuration_produces_no_result_at_all() {
    // The point of AC9: not a result with a caveat, no result.
    let config = SimConfig {
        duration_ms: 0,
        ..valid()
    };
    match run(&config) {
        Ok(result) => panic!(
            "a zero length run reported {} samples",
            result.samples.len()
        ),
        Err(e) => assert_eq!(e, ConfigError::ZeroDuration),
    }
}
