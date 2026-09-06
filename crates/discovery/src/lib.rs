//! Multicast DNS and DNS-based service discovery, enough of both for one
//! server to advertise and one endpoint to find it.
//!
//! # Why this is written here
//!
//! The same reason `crates/protocol` is: this workspace has no third-party
//! dependency and the phase's own acceptance is graded on BYTES. An endpoint
//! that finds a server has to produce the query packet the committed fixture
//! holds and to make the advertised host and port out of the committed response
//! packet, so a second implementation is held to the packets rather than to
//! this code.
//!
//! # What it is, and what it is not
//!
//! [`wire`] is DNS message bytes. [`dnssd`] is the PTR, SRV and TXT convention
//! RFC 6763 lays out, and the resolver over it. [`net`] is the two sockets.
//!
//! It is not a responder in the full sense of RFC 6762. There is no probing,
//! no conflict resolution, no known-answer suppression and no cache. Those
//! matter to a general-purpose responder sharing a link with other responders;
//! what is needed here is that one server can say where it is and one endpoint
//! can hear it, with a static address as the fallback for every case where the
//! link does not carry the packet at all. `docs/control-plane.md` says which
//! parts of the RFCs are implemented and which are deliberately not.

#![warn(missing_docs)]

pub mod dnssd;
pub mod net;
pub mod wire;

pub use dnssd::{
    browse_query_bytes, resolve, Advertisement, Service, AUDIO_SERVICE, CONTROL_SERVICE,
    MDNS_GROUP_V4, MDNS_GROUP_V6, MDNS_PORT,
};
pub use net::{browse, locate, Advertiser, DiscoveryError, Located, NoServer};
