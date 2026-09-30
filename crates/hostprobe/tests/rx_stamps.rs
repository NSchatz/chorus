//! The kernel delivers software receive stamps on the socket kinds the check uses.
//!
//! Structural assertions only: that a stamp arrives for every receive, and that a
//! kernel stamp is never after the user stamp taken once `recvmsg` has returned
//! (both on `CLOCK_REALTIME`; a sample the settable clock stepped under is
//! discarded by the check itself, not by this test). How large the delta is, is a
//! host measurement for docs/measurements/, never an assertion here.

use chorus_hostprobe::rxstamp::{run, Config, Transport};

fn check(transport: Transport) {
    let o = run(&Config {
        transport,
        samples: 200,
        period_ns: 1_000_000,
    })
    .expect("the check runs over loopback");
    assert_eq!(
        o.unstamped,
        0,
        "{}: receives without a kernel stamp",
        transport.name()
    );
    assert!(
        o.user_minus_kernel_ns.len() + o.step_discards >= 1,
        "{}: nothing was received",
        transport.name()
    );
    for d in &o.user_minus_kernel_ns {
        assert!(
            *d >= 0,
            "{}: a kernel stamp {} ns after the user stamp",
            transport.name(),
            -d
        );
    }
}

#[test]
fn tcp_over_loopback_carries_a_kernel_receive_stamp_on_every_read() {
    check(Transport::TcpLoopback);
}

#[test]
fn udp_over_loopback_carries_a_kernel_receive_stamp_on_every_datagram() {
    let o = run(&Config {
        transport: Transport::UdpLoopback,
        samples: 200,
        period_ns: 1_000_000,
    })
    .unwrap();
    assert_eq!(o.lost, 0, "a loopback datagram was lost");
    assert_eq!(o.user_minus_kernel_ns.len() + o.step_discards, 200);
    check(Transport::UdpLoopback);
}
