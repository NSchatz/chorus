# Streams: what URLs chorus plays

chorus fetches media by URL when something hands it one: the UPnP renderer of goal 16
first, stored alarm streams later. This page says which URLs play, which are
refused, and the exact words of each refusal. The decision record is
`docs/decisions/0120-the-media-fetcher.md`; the code is `crates/fetch`.

## What plays

| URL | Notes |
|---|---|
| `http://` and `https://` files | MP3, FLAC, Ogg Vorbis, Ogg Opus, ALAC in MP4, WAV (the decoders' list). A server that offers byte ranges makes the file seekable, which an MP4 with its index at the end needs. |
| Internet radio (Icecast, SHOUTcast) | Endless streams with no length. The station's name, bitrate and genre and the current title (`StreamTitle`) are read from the stream's ICY metadata. |
| HLS (`.m3u8`) whose segments are MP3 | Live or on demand. A master playlist's best MP3 variant is chosen. |

Media on the household's own network (a NAS, a phone's media server) plays: private address
ranges are allowed.

## What is refused, and the words

A refusal is one line. The first word says the kind.

### `refused:` the fetch policy

| Words | Meaning |
|---|---|
| `refused: scheme <name>: only http and https are fetched` | `file:`, `ftp:`, `rtsp:` and every other scheme |
| `refused: userinfo in the url: a url with user:password@ is not fetched` | credentials inside the URL |
| `refused: loopback address <ip>` | the URL (or a redirect, or an HLS segment) points at the server itself |
| `refused: link-local address <ip>` | `169.254.x.x` or `fe80::` addresses |
| `refused: unspecified address <ip>`, `refused: multicast address <ip>`, `refused: broadcast address <ip>` | not addresses a media server has |
| `refused: the server's own port <port> at <ip>` | the URL points at one of chorus's own listeners |
| `refused: more than <n> redirects` | a redirect chain longer than the bound (5), or a loop |
| `refused: response headers larger than <n> bytes` | a server sending more than 32 KiB of headers |

The address rules are applied to what a name resolves to, at every redirect and for every
HLS playlist and segment.

### `unsupported:` chorus does not play this

| Words | Meaning |
|---|---|
| `unsupported: hls: aac (<codec>)` | every variant of the HLS stream is AAC |
| `unsupported: hls: aac (adts segments)` | the HLS segments are AAC |
| `unsupported: hls: mpeg-2 transport stream segments` | `.ts` segments |
| `unsupported: hls: fragmented mp4 segments (EXT-X-MAP)`, `unsupported: hls: fragmented mp4 segments` | fMP4 segments |
| `unsupported: hls: encrypted segments (EXT-X-KEY METHOD=<method>)` | an encrypted stream |
| `unsupported: hls: byte-range segments (EXT-X-BYTERANGE)` | segments cut out of one file |
| `unsupported: hls: ac-3 segments` | Dolby Digital audio |
| `unsupported: hls: no playable variant (codecs <list>)` | video, or codecs chorus has no decoder for |
| `unsupported: hls: protocol version <n> (EXT-X-VERSION above 7)` | a newer HLS than the one implemented |
| `unsupported: hls: an I-frame playlist (EXT-X-I-FRAMES-ONLY)` | a trick-play playlist |
| `unsupported: hls: segments that are not packed mp3 audio` | anything else in a segment |
| `unsupported: m3u: a plain playlist, not HLS (no EXT-X-TARGETDURATION); use the stream's own url` | a `.m3u` file that lists stream addresses: open it in a text editor and give chorus the address inside |
| `unsupported: http: content coding <name>`, `unsupported: http: transfer coding <name>` | a compressed response |

**Most HLS radio is refused.** HLS audio is nearly always AAC, and chorus does not decode
AAC (a decision, proposal P9). If a station offers a plain MP3 or Ogg stream address beside
its HLS one, use that.

### `tls:` https could not be trusted

| Words | Meaning |
|---|---|
| `tls: ca bundle <path>: cannot be read: ...` | the file of trusted certificate authorities is missing. chorus reads the file named by `SSL_CERT_FILE`, else `/etc/ssl/certs/ca-certificates.crt` (present in the release image). |
| `tls: ca bundle <path>: holds no usable certificate` | the file is there and empty or not PEM |
| `tls: <host>: invalid peer certificate: ...` | the server's certificate is for another name, has expired, or comes from an authority not in the bundle. An expiry error for every site means the server's clock is wrong. |

There is no setting that turns certificate checking off.

### The rest

| Words | Meaning |
|---|---|
| `http status <n>` | the server answered with an error (404: no such file; 403: not allowed) |
| `io: ...` | the network: no such host, connection refused, or nothing arrived within the timeout (10 s to connect, 15 s of silence) |
| `malformed: ...` | the URL, the response or a playlist does not parse |
