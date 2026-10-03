//! The OpenHome Playlist as a state machine: the list, the cursor, and the
//! effects it asks of the player, with the player's reports played back by
//! the test. No socket and no clock. The rules are the reference's
//! (ohPipeline at `cccd06dd`, cited in `chorus_upnp::openhome::playlist`).

use chorus_upnp::avtransport::Effect;
use chorus_upnp::description::table;
use chorus_upnp::openhome::playlist::{decode_ids, Playlist, TRACKS_MAX};
use chorus_upnp::soap::Invocation;
use chorus_upnp::{error, Outputs, Service, UpnpError};

const A: &str = "http://192.0.2.9/a.flac";
const B: &str = "http://192.0.2.9/b.flac";
const C: &str = "http://192.0.2.9/c.flac";

fn call(
    list: &mut Playlist,
    action: &str,
    inputs: &[&str],
) -> Result<(Outputs, Vec<Effect>), UpnpError> {
    list.invoke(
        &Invocation {
            action: table(Service::Playlist)
                .action(action)
                .unwrap_or_else(|| panic!("no action {action}")),
            inputs: inputs.iter().map(|s| s.to_string()).collect(),
        },
        42,
    )
}

fn ok(list: &mut Playlist, action: &str, inputs: &[&str]) -> Vec<Effect> {
    call(list, action, inputs)
        .unwrap_or_else(|e| panic!("{action}: {e:?}"))
        .1
}

fn value(list: &mut Playlist, action: &str) -> String {
    call(list, action, &[]).unwrap().0[0].1.clone()
}

fn insert(list: &mut Playlist, after: u32, uri: &str) -> u32 {
    let (out, _) = call(
        list,
        "Insert",
        &[&after.to_string(), uri, &format!("<m>{uri}</m>")],
    )
    .unwrap();
    out[0].1.parse().unwrap()
}

/// A list of A, B, C with ids 1, 2, 3.
fn three() -> Playlist {
    let mut list = Playlist::new();
    let a = insert(&mut list, 0, A);
    let b = insert(&mut list, a, B);
    let c = insert(&mut list, b, C);
    assert_eq!((a, b, c), (1, 2, 3));
    list
}

fn load(uri: &str) -> Effect {
    Effect::Load {
        uri: uri.into(),
        metadata: format!("<m>{uri}</m>"),
    }
}

fn queue(uri: &str) -> Effect {
    Effect::QueueNext {
        uri: uri.into(),
        metadata: format!("<m>{uri}</m>"),
    }
}

/// The player says: opened, playing.
fn started(list: &mut Playlist) {
    let epoch = list.deck().epoch();
    assert!(list.media_opened(epoch, Some(3000), true));
    assert!(list.playing(epoch));
    assert_eq!(list.transport_state(), "Playing");
}

#[test]
fn an_empty_list_is_stopped_with_id_zero() {
    let mut list = Playlist::new();
    assert_eq!(value(&mut list, "TransportState"), "Stopped");
    assert_eq!(value(&mut list, "Id"), "0");
    assert_eq!(value(&mut list, "TracksMax"), "1000");
    assert_eq!(value(&mut list, "Repeat"), "0");
    assert_eq!(value(&mut list, "Shuffle"), "0");
    assert!(value(&mut list, "ProtocolInfo").contains("http-get:*:audio/flac:*"));
    let (out, _) = call(&mut list, "IdArray", &[]).unwrap();
    assert_eq!(out, [("Token", "0".to_string()), ("Array", String::new())]);
    // Play on an empty list does nothing audible.
    assert!(ok(&mut list, "Play", &[]).is_empty());
    assert_eq!(list.transport_state(), "Stopped");
    for action in ["Pause", "Stop", "Next", "Previous", "DeleteAll"] {
        assert!(ok(&mut list, action, &[]).is_empty(), "{action}");
    }
    assert_eq!(
        call(&mut list, "SeekSecondAbsolute", &["1"]).unwrap_err(),
        error::OH_PLAYLIST_SEEK_FAILED
    );
}

#[test]
fn insert_cues_the_first_track_and_keeps_ids_tokens_and_order() {
    let mut list = Playlist::new();
    let a = insert(&mut list, 0, A);
    assert_eq!(list.id(), a, "the first track of an empty list is cued");
    assert_eq!(list.transport_state(), "Stopped");
    // At the head, and after a given id.
    let c = insert(&mut list, a, C);
    let b = insert(&mut list, a, B);
    let head = insert(&mut list, 0, "https://192.0.2.9/head");
    assert_eq!(list.ids(), [head, a, b, c]);
    assert_eq!(list.id(), a, "an insert never moves the cursor");
    let (out, _) = call(&mut list, "IdArray", &[]).unwrap();
    assert_eq!(out[0].1, "4", "the token counts the changes");
    assert_eq!(decode_ids(&out[1].1), Some(vec![head, a, b, c]));
    let changed = |list: &mut Playlist, token: &str| {
        call(list, "IdArrayChanged", &[token]).unwrap().0[0]
            .1
            .clone()
    };
    assert_eq!(changed(&mut list, "4"), "0");
    assert_eq!(changed(&mut list, "3"), "1");
    // Refusals: an unknown AfterId, a URI the player would never fetch.
    assert_eq!(
        call(&mut list, "Insert", &["999", A, ""]).unwrap_err(),
        error::OH_PLAYLIST_ID_NOT_FOUND
    );
    for bad in [
        "",
        "file:///etc/passwd",
        "ftp://192.0.2.9/a",
        "rtsp://192.0.2.9/a",
    ] {
        assert_eq!(
            call(&mut list, "Insert", &["0", bad, ""]).unwrap_err(),
            error::ARGUMENT_VALUE_INVALID,
            "{bad}"
        );
    }
    assert_eq!(
        call(&mut list, "Insert", &["x", A, ""]).unwrap_err(),
        error::INVALID_ARGS
    );
    assert_eq!(
        changed(&mut list, "4"),
        "0",
        "a refused insert changes nothing"
    );
    // Read gives the URI and the metadata back verbatim.
    let (out, _) = call(&mut list, "Read", &[&b.to_string()]).unwrap();
    assert_eq!(
        out,
        [("Uri", B.to_string()), ("Metadata", format!("<m>{B}</m>"))]
    );
    assert_eq!(
        call(&mut list, "Read", &["999"]).unwrap_err(),
        error::OH_PLAYLIST_ID_NOT_FOUND
    );
}

#[test]
fn the_list_is_full_at_tracks_max() {
    let mut list = Playlist::new();
    let mut after = 0;
    for _ in 0..TRACKS_MAX {
        after = insert(&mut list, after, A);
    }
    assert_eq!(list.len(), TRACKS_MAX);
    assert_eq!(
        call(&mut list, "Insert", &[&after.to_string(), A, ""]).unwrap_err(),
        error::OH_PLAYLIST_FULL
    );
    assert!(ok(&mut list, "DeleteAll", &[]).is_empty());
    assert!(list.is_empty());
    assert_eq!(list.id(), 0);
    // Ids are never used twice.
    assert_eq!(insert(&mut list, 0, A), TRACKS_MAX as u32 + 1);
}

#[test]
fn play_loads_the_current_track_starts_it_and_queues_the_following_one() {
    let mut list = three();
    assert_eq!(
        ok(&mut list, "Play", &[]),
        [load(A), Effect::Start, queue(B)]
    );
    assert_eq!(list.transport_state(), "Buffering");
    assert_eq!(list.deck().current().0, A);
    assert_eq!(list.deck().next_queued().0, B);
    started(&mut list);
    // The gapless handover: no action from anybody; the player reports B.
    let epoch = list.deck().epoch();
    assert_eq!(list.track_boundary(epoch, B, Some(3000), true), [queue(C)]);
    assert_eq!((list.id(), list.transport_state()), (2, "Playing"));
    assert_eq!(list.track_boundary(epoch, C, Some(3000), true), []);
    assert_eq!((list.id(), list.transport_state()), (3, "Playing"));
    // The end of the list with Repeat off: stopped, the first track cued.
    assert!(list.ended(epoch).is_empty());
    assert_eq!((list.id(), list.transport_state()), (1, "Stopped"));
    // Play again starts over from the cued track.
    assert_eq!(
        ok(&mut list, "Play", &[]),
        [load(A), Effect::Start, queue(B)]
    );
}

#[test]
fn repeat_wraps_the_following_track_and_plays_on() {
    let mut list = three();
    ok(&mut list, "SetRepeat", &["1"]);
    assert_eq!(value(&mut list, "Repeat"), "1");
    assert_eq!(
        ok(&mut list, "SeekId", &["3"]),
        [load(C), Effect::Start, queue(A)]
    );
    started(&mut list);
    let epoch = list.deck().epoch();
    assert_eq!(list.track_boundary(epoch, A, None, true), [queue(B)]);
    assert_eq!((list.id(), list.transport_state()), (1, "Playing"));
    // Switching Repeat off while the last track plays clears what was queued.
    list.track_boundary(epoch, B, None, true);
    assert_eq!(list.track_boundary(epoch, C, None, true), [queue(A)]);
    assert_eq!(ok(&mut list, "SetRepeat", &["0"]), [Effect::ClearNext]);
    assert_eq!(ok(&mut list, "SetRepeat", &["true"]), [queue(A)]);
    assert_eq!(
        call(&mut list, "SetRepeat", &["maybe"]).unwrap_err(),
        error::INVALID_ARGS
    );
}

#[test]
fn next_previous_and_the_seeks_by_id_and_index() {
    let mut list = three();
    ok(&mut list, "Play", &[]);
    started(&mut list);
    assert_eq!(
        ok(&mut list, "Next", &[]),
        [load(B), Effect::Start, queue(C)],
        "from playing, the new track loads and starts"
    );
    assert_eq!((list.id(), list.transport_state()), (2, "Buffering"));
    started(&mut list);
    assert_eq!(ok(&mut list, "Next", &[]), [load(C), Effect::Start]);
    started(&mut list);
    assert_eq!(
        ok(&mut list, "Previous", &[]),
        [load(B), Effect::Start, queue(C)]
    );
    assert_eq!(list.id(), 2);
    started(&mut list);
    assert_eq!(
        ok(&mut list, "SeekId", &["1"]),
        [load(A), Effect::Start, queue(B)]
    );
    assert_eq!(list.id(), 1);
    assert_eq!(
        call(&mut list, "SeekId", &["999"]).unwrap_err(),
        error::OH_PLAYLIST_ID_NOT_FOUND
    );
    started(&mut list);
    assert_eq!(ok(&mut list, "SeekIndex", &["2"]), [load(C), Effect::Start]);
    assert_eq!(list.id(), 3);
    assert_eq!(
        call(&mut list, "SeekIndex", &["3"]).unwrap_err(),
        error::OH_PLAYLIST_INDEX_NOT_FOUND
    );
    assert_eq!(list.id(), 3, "a refused seek changes nothing");
    // Next at the last track with Repeat off: stopped, the first cued.
    started(&mut list);
    assert_eq!(ok(&mut list, "Next", &[]), [Effect::Stop]);
    assert_eq!((list.id(), list.transport_state()), (1, "Stopped"));
    // Previous at the first track does the same.
    ok(&mut list, "Play", &[]);
    started(&mut list);
    assert_eq!(ok(&mut list, "Previous", &[]), [Effect::Stop]);
    assert_eq!((list.id(), list.transport_state()), (1, "Stopped"));
}

#[test]
fn pause_resume_stop_and_play_while_playing() {
    let mut list = three();
    ok(&mut list, "Play", &[]);
    started(&mut list);
    assert_eq!(ok(&mut list, "Pause", &[]), [Effect::Pause]);
    assert_eq!(list.transport_state(), "Paused");
    assert_eq!(ok(&mut list, "Play", &[]), [Effect::Resume]);
    assert_eq!(list.transport_state(), "Playing");
    // Play while playing starts the current track again.
    assert_eq!(
        ok(&mut list, "Play", &[]),
        [load(A), Effect::Start, queue(B)]
    );
    started(&mut list);
    assert_eq!(ok(&mut list, "Stop", &[]), [Effect::Stop]);
    assert_eq!((list.id(), list.transport_state()), (1, "Stopped"));
    // Stop keeps the track cued and on the deck: Play starts it.
    assert_eq!(ok(&mut list, "Play", &[]), [Effect::Start]);
    assert_eq!(list.deck().next_queued().0, B);
    // Pause while stopped is no fault.
    let mut idle = three();
    assert!(ok(&mut idle, "Pause", &[]).is_empty());
}

#[test]
fn seeking_by_seconds_is_within_the_current_track_and_then_plays() {
    let mut list = three();
    ok(&mut list, "Play", &[]);
    let epoch = list.deck().epoch();
    list.media_opened(epoch, Some(60_000), true);
    // While buffering the deck takes no seek.
    assert_eq!(
        call(&mut list, "SeekSecondAbsolute", &["5"]).unwrap_err(),
        error::OH_PLAYLIST_SEEK_FAILED
    );
    list.playing(epoch);
    assert_eq!(
        ok(&mut list, "SeekSecondAbsolute", &["5"]),
        [Effect::SeekTo { ms: 5000 }]
    );
    assert_eq!(list.transport_state(), "Buffering");
    list.playing(epoch);
    list.position(12_300);
    assert_eq!(
        ok(&mut list, "SeekSecondRelative", &["10"]),
        [Effect::SeekTo { ms: 22_000 }]
    );
    list.playing(epoch);
    list.position(3_000);
    // Before the start lands on 0.
    assert_eq!(
        ok(&mut list, "SeekSecondRelative", &["-30"]),
        [Effect::SeekTo { ms: 0 }]
    );
    list.playing(epoch);
    // Past the end: refused, nothing changed.
    assert_eq!(
        call(&mut list, "SeekSecondAbsolute", &["61"]).unwrap_err(),
        error::OH_PLAYLIST_SEEK_FAILED
    );
    assert_eq!(list.transport_state(), "Playing");
    // From stopped: the position is set and the track plays from there.
    ok(&mut list, "Stop", &[]);
    assert_eq!(
        ok(&mut list, "SeekSecondAbsolute", &["7"]),
        [Effect::SeekTo { ms: 7000 }, Effect::Start]
    );
}

#[test]
fn deleting_tracks_follows_the_reference() {
    let mut list = three();
    ok(&mut list, "Play", &[]);
    started(&mut list);
    assert_eq!(
        call(&mut list, "DeleteId", &["999"]).unwrap_err(),
        error::OH_PLAYLIST_ID_NOT_FOUND
    );
    // The queued following track is deleted: the one after it is queued.
    assert_eq!(ok(&mut list, "DeleteId", &["2"]), [queue(C)]);
    assert_eq!((list.id(), list.ids()), (1, vec![1, 3]));
    // The playing track is deleted: playback moves on to the next.
    assert_eq!(ok(&mut list, "DeleteId", &["1"]), [load(C), Effect::Start]);
    assert_eq!((list.id(), list.ids()), (3, vec![3]));
    started(&mut list);
    // The last one: the list is empty, the transport stops, Id is 0.
    assert_eq!(ok(&mut list, "DeleteId", &["3"]), [Effect::Stop]);
    assert_eq!((list.id(), list.transport_state()), (0, "Stopped"));
    assert!(list.is_empty());

    // While nothing plays, deleting the cued track cues the one after it.
    let mut list = three();
    assert!(ok(&mut list, "DeleteId", &["1"]).is_empty());
    assert_eq!(list.id(), 2);
    assert!(ok(&mut list, "DeleteId", &["3"]).is_empty());
    assert_eq!(list.id(), 2);
    // The last track that plays is deleted with Repeat off: stop, cue first.
    let mut list = three();
    ok(&mut list, "SeekId", &["3"]);
    started(&mut list);
    assert_eq!(ok(&mut list, "DeleteId", &["3"]), [Effect::Stop]);
    assert_eq!((list.id(), list.transport_state()), (1, "Stopped"));
    // DeleteAll while playing.
    let mut list = three();
    ok(&mut list, "Play", &[]);
    started(&mut list);
    assert_eq!(ok(&mut list, "DeleteAll", &[]), [Effect::Stop]);
    assert_eq!((list.id(), list.len()), (0, 0));
    assert_eq!(list.deck().current().0, "");
}

#[test]
fn an_insert_behind_the_playing_track_replaces_what_was_queued() {
    let mut list = three();
    ok(&mut list, "Play", &[]);
    started(&mut list);
    let (out, effects) = call(
        &mut list,
        "Insert",
        &["1", "http://192.0.2.9/x", "<m>http://192.0.2.9/x</m>"],
    )
    .unwrap();
    assert_eq!(out[0].1, "4");
    assert_eq!(effects, [queue("http://192.0.2.9/x")]);
    // An insert elsewhere queues nothing.
    let (_, effects) = call(&mut list, "Insert", &["3", "http://192.0.2.9/y", ""]).unwrap();
    assert!(effects.is_empty());
}

#[test]
fn a_boundary_that_races_a_list_change_ends_where_the_list_says() {
    // B was joined to A in the audio; before it was heard, X was inserted
    // behind A. The player reports B: B is what plays, and C follows it.
    let mut list = three();
    ok(&mut list, "Play", &[]);
    started(&mut list);
    let epoch = list.deck().epoch();
    call(&mut list, "Insert", &["1", "http://192.0.2.9/x", ""]).unwrap();
    assert_eq!(list.deck().next_queued().0, "http://192.0.2.9/x");
    assert_eq!(list.track_boundary(epoch, B, None, true), [queue(C)]);
    assert_eq!((list.id(), list.transport_state()), (2, "Playing"));
    assert_eq!(list.deck().current(), (B, "<m>http://192.0.2.9/b.flac</m>"));

    // B was joined and then deleted before it was heard: the list moves on
    // from A to what follows it now.
    let mut list = three();
    ok(&mut list, "Play", &[]);
    started(&mut list);
    let epoch = list.deck().epoch();
    assert_eq!(ok(&mut list, "DeleteId", &["2"]), [queue(C)]);
    assert_eq!(
        list.track_boundary(epoch, B, None, true),
        [load(C), Effect::Start]
    );
    assert_eq!((list.id(), list.transport_state()), (3, "Buffering"));

    // A report from before a Stop is ignored.
    let mut list = three();
    ok(&mut list, "Play", &[]);
    started(&mut list);
    let old = list.deck().epoch();
    ok(&mut list, "Stop", &[]);
    assert!(list.track_boundary(old, B, None, true).is_empty());
    assert!(list.ended(old).is_empty());
    assert_eq!((list.id(), list.transport_state()), (1, "Stopped"));
}

#[test]
fn a_following_track_that_cannot_play_stops_the_list_on_it() {
    let mut list = three();
    ok(&mut list, "Play", &[]);
    started(&mut list);
    let epoch = list.deck().epoch();
    assert!(list.next_failed(epoch, "unsupported: aac"));
    assert!(list.ended(epoch).is_empty());
    assert_eq!((list.id(), list.transport_state()), (2, "Stopped"));
    // A good following track that was not ready in time is loaded and
    // started, not joined.
    let mut list = three();
    ok(&mut list, "Play", &[]);
    started(&mut list);
    let epoch = list.deck().epoch();
    assert_eq!(list.ended(epoch), [load(B), Effect::Start, queue(C)]);
    assert_eq!((list.id(), list.transport_state()), (2, "Buffering"));
    // The current track fails: stopped, still on it.
    let epoch = list.deck().epoch();
    assert!(list.failed(epoch, "http status 404"));
    assert_eq!((list.id(), list.transport_state()), (2, "Stopped"));
    assert_eq!(list.deck().last_failure(), Some("http status 404"));
}

#[test]
fn shuffle_needs_two_tracks_and_plays_every_track_once() {
    let mut one = Playlist::new();
    insert(&mut one, 0, A);
    assert_eq!(
        call(&mut one, "SetShuffle", &["1"]).unwrap_err(),
        error::OH_PLAYLIST_SHUFFLE_NOT_POSSIBLE
    );
    assert!(ok(&mut one, "SetShuffle", &["0"]).is_empty());

    let mut list = Playlist::new();
    let mut after = 0;
    for i in 0..8 {
        after = insert(&mut list, after, &format!("http://192.0.2.9/{i}"));
    }
    ok(&mut list, "SetShuffle", &["1"]);
    assert_eq!(value(&mut list, "Shuffle"), "1");
    // IdArray stays in list order.
    assert_eq!(list.ids(), (1..=8).collect::<Vec<u32>>());
    ok(&mut list, "Play", &[]);
    started(&mut list);
    let mut played = vec![list.id()];
    let epoch = list.deck().epoch();
    loop {
        let next = list.deck().next_queued().0.to_string();
        if next.is_empty() {
            break;
        }
        list.track_boundary(epoch, &next, None, true);
        played.push(list.id());
        assert!(played.len() <= 8, "{played:?}");
    }
    let mut sorted = played.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), played.len(), "no track twice: {played:?}");
    // Play starts at the cued track, wherever the shuffled order has it, and
    // goes on to the end of that order; with Repeat it wraps.
    assert_eq!(played[0], 1);
    assert_eq!(list.following_in_order(), None);
    assert_ne!(
        played,
        (1..=8).collect::<Vec<u32>>(),
        "the order is shuffled"
    );
}

#[test]
fn another_source_stops_the_list_and_keeps_it() {
    let mut list = three();
    ok(&mut list, "Play", &[]);
    started(&mut list);
    assert_eq!(list.deactivate(), [Effect::Stop]);
    assert!(!list.is_active());
    assert_eq!((list.id(), list.transport_state()), (1, "Stopped"));
    assert_eq!(list.ids(), [1, 2, 3]);
    // While it is not the source, the transport actions do nothing.
    for action in ["Pause", "Stop", "Next", "Previous"] {
        assert!(ok(&mut list, action, &[]).is_empty(), "{action}");
    }
    assert!(ok(&mut list, "SeekSecondAbsolute", &["1"]).is_empty());
    assert_eq!(list.id(), 1);
    // Play, SeekId and SeekIndex take the source back (the server switches,
    // then invokes).
    for action in ["Play", "SeekId", "SeekIndex"] {
        assert!(Playlist::activates(action));
    }
    assert!(!Playlist::activates("Next"));
    list.activate();
    assert_eq!(ok(&mut list, "Play", &[]), [Effect::Start]);
}

#[test]
fn every_action_answers_the_out_arguments_of_its_table() {
    let mut list = three();
    for action in table(Service::Playlist).actions {
        let inputs: Vec<&str> = action
            .inputs()
            .map(|a| match a.name {
                "Uri" => A,
                "Metadata" | "IdList" => "",
                _ => "1",
            })
            .collect();
        // A seek while the deck buffers is refused; every other call answers.
        let Ok((out, _)) = call(&mut list, action.name, &inputs) else {
            assert!(action.name.starts_with("SeekSecond"), "{}", action.name);
            continue;
        };
        let names: Vec<&str> = out.iter().map(|(n, _)| *n).collect();
        let expected: Vec<&str> = action.outputs().map(|a| a.name).collect();
        assert_eq!(names, expected, "{}", action.name);
    }
    let evented: Vec<&str> = list.evented().iter().map(|(n, _)| *n).collect();
    let expected: Vec<&str> = table(Service::Playlist)
        .variables
        .iter()
        .filter(|v| v.evented)
        .map(|v| v.name)
        .collect();
    assert_eq!(evented, expected);
}
