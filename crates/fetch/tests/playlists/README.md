# Playlists for the HLS parser's tests

Written by hand for `crates/fetch/src/hls/playlist.rs` from RFC 8216's tag
definitions (section 4.3) and its examples' shape (section 8); every host is a
documentation name. One file per case the parser must tell apart:

| File | What it is | Expected |
|---|---|---|
| `vod.m3u8` | an ended media playlist with a discontinuity and an absolute URI | parses, three segments |
| `live.m3u8` | a live media playlist, six 6 s segments from sequence 2680 | parses, entered at the fourth segment |
| `master-mp3.m3u8` | MP3, AAC and video variants | the 192 kbit/s MP3 variant is chosen |
| `master-no-codecs.m3u8` | variants without `CODECS` | the highest `BANDWIDTH` is chosen |
| `master-aac.m3u8` | every variant AAC | `unsupported: hls: aac (mp4a.40.2)` |
| `key-aes128.m3u8`, `key-sample-aes.m3u8` | encrypted | `unsupported: hls: encrypted segments (EXT-X-KEY METHOD=...)` |
| `key-none.m3u8` | `EXT-X-KEY:METHOD=NONE` | parses |
| `fmp4.m3u8` | `EXT-X-MAP` | `unsupported: hls: fragmented mp4 segments (EXT-X-MAP)` |
| `byterange.m3u8` | `EXT-X-BYTERANGE` | `unsupported: hls: byte-range segments (EXT-X-BYTERANGE)` |
| `version-8.m3u8` | a protocol version above 7 | `unsupported: hls: protocol version 8 (EXT-X-VERSION above 7)` |
| `plain.m3u` | an M3U list of stream URLs, not HLS | `unsupported: m3u: a plain playlist, not HLS ...` |
