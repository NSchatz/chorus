//! The topic table, the prefix rule and the level rule.

use chorus_mqtt::codec::{Publish, Qos};
use chorus_mqtt::topic::{
    check_client_id, segment, Topics, DEFAULT_CLIENT_ID, DEFAULT_PREFIX, OFFLINE, ONLINE,
};

#[test]
fn the_topics_are_the_table_in_docs_mqtt_md() {
    let t = Topics::new(DEFAULT_PREFIX).unwrap();
    assert_eq!(t.prefix(), "chorus/v1");
    assert_eq!(t.status(), "chorus/v1/server/status");
    assert_eq!(t.room("kitchen"), "chorus/v1/rooms/kitchen/state");
    assert_eq!(t.group("downstairs"), "chorus/v1/groups/downstairs/state");
    assert_eq!(
        t.speaker_event("chorus-0123456789ab"),
        "chorus/v1/speakers/chorus-0123456789ab/event"
    );
    assert_eq!((ONLINE, OFFLINE), ("online", "offline"));
    let other = Topics::new("house/audio").unwrap();
    assert_eq!(other.room("den"), "house/audio/rooms/den/state");
}

#[test]
fn an_id_is_always_exactly_one_level() {
    // The catalog's ids pass through unchanged.
    for id in ["kitchen", "live-1", "endpoint-c", "chorus-0123456789ab"] {
        assert_eq!(segment(id), id);
    }
    assert_eq!(segment("a/b"), "a%2Fb");
    assert_eq!(segment("a+b#c"), "a%2Bb%23c");
    assert_eq!(segment("50%"), "50%25");
    assert_eq!(segment("a\0b"), "a%00b");
    assert_eq!(segment("tab\there"), "tab%09here");
    assert_eq!(segment("\u{85}"), "%C2%85");
    assert_eq!(segment(""), "%");
    // Other text is kept: MQTT topics are UTF-8 and case sensitive.
    assert_eq!(segment("Küche oben"), "Küche oben");
    // One to one: the escape character is itself escaped.
    assert_ne!(segment("a%2Fb"), segment("a/b"));
    let t = Topics::new(DEFAULT_PREFIX).unwrap();
    for id in ["a/b", "+", "#", "x/#", "\0", "", "%", "a\nb", "ok"] {
        for topic in [t.room(id), t.group(id), t.speaker_event(id)] {
            assert_eq!(topic.split('/').count(), 5, "{:?} gave {}", id, topic);
            assert!(topic.split('/').all(|level| !level.is_empty()), "{}", topic);
            // And the codec accepts it as a topic name.
            Publish {
                topic: &topic,
                payload: b"",
                qos: Qos::AtLeastOnce,
                retain: true,
                packet_id: 1,
            }
            .encode()
            .unwrap_or_else(|e| panic!("{:?} gave {}: {}", id, topic, e));
        }
    }
}

#[test]
fn a_prefix_that_is_not_a_topic_prefix_is_refused() {
    for good in ["chorus/v1", "chorus", "house/audio/v1", "Home Audio"] {
        assert!(Topics::new(good).is_ok(), "{}", good);
    }
    for bad in [
        "",
        "/chorus",
        "chorus/",
        "chorus//v1",
        "chorus/+",
        "chorus/#",
        "#",
        "cho\0rus",
        "cho\nrus",
        "$SYS/chorus",
    ] {
        assert!(Topics::new(bad).is_err(), "{:?}", bad);
    }
    assert!(Topics::new(&"a".repeat(129)).is_err());
}

#[test]
fn no_prefix_puts_chorus_under_home_assistants_discovery_topics() {
    for bad in [
        "homeassistant",
        "homeassistant/chorus",
        "homeassistant/sensor/chorus",
    ] {
        let refused = Topics::new(bad).unwrap_err();
        assert!(refused.contains("homeassistant/"), "{}", refused);
    }
    // Only the first level is Home Assistant's.
    assert!(Topics::new("chorus/homeassistant").is_ok());
    assert!(Topics::new("homeassistant2").is_ok());
}

#[test]
fn a_client_id_is_the_portable_set() {
    assert!(check_client_id(DEFAULT_CLIENT_ID).is_ok());
    assert!(check_client_id("Chorus2").is_ok());
    assert!(check_client_id(&"a".repeat(23)).is_ok());
    for bad in ["", "chorus-server", "chorus_1", "chorus 1", "chörus"] {
        assert!(check_client_id(bad).is_err(), "{:?}", bad);
    }
    assert!(check_client_id(&"a".repeat(24)).is_err());
}
