Prefix: MYC
Next: 6

### MYC-2 · P2 · HTTP connection handling inherits nonblocking socket on BSD/macOS

- **Area:** Concurrency
- **Where:** `src/serve.rs:276`
- **Problem:** When `listener` is nonblocking, accepted sockets inherit nonblocking mode on BSD/macOS. `handle_connection` does not call `stream.set_nonblocking(false)`, causing synchronous `read_until` header parsing to fail spuriously with `WouldBlock`.
- **Recommendation:** Call `let _ = stream.set_nonblocking(false);` at the start of `handle_connection`.
- **Fix with:** lifecycle-concurrency
- **Effort:** S

### MYC-3 · P2 · scan_crate buffers unbounded compressed input stream into memory

- **Area:** Security
- **Where:** `src/crate_file.rs:34`
- **Problem:** `scan_crate` uses `std::io::copy(&mut reader, &mut tmp)` into an unbounded vector before checking length against `limits.max_compressed`. An attacker or client streaming excessive data can cause memory exhaustion before limit enforcement triggers.
- **Recommendation:** Bound stream ingestion with `reader.take(limits.max_compressed + 1)` and reject if copied bytes exceed `max_compressed`.
- **Fix with:** os-boundaries
- **Effort:** S

### MYC-4 · P3 · Publish HTTP daemon does not throttle concurrent connections

- **Area:** Concurrency
- **Where:** `src/serve.rs:330`
- **Problem:** `run_server` spawns an unbounded thread per accepted TCP connection, allowing connection spikes to exhaust system thread limits rather than capping active handlers at `limits.concurrent_requests`.
- **Recommendation:** Bound active connection worker threads with an atomic counter or semaphore, rejecting or queuing connections when saturated.
- **Fix with:** lifecycle-concurrency
- **Effort:** M

### MYC-5 · P4 · Stale milestone and numbered implementation comments in production source

- **Area:** Dead code
- **Where:** `src/lib.rs:8`, `src/validate.rs:31`, `src/publish.rs:133`, `src/acquire.rs:81`, `src/serve.rs:98`
- **Problem:** Numbered milestone comments and step markers from campaign execution remain in production modules, contrary to the repository working contract prohibiting unnecessary code comments.
- **Recommendation:** Remove obsolete milestone markers and numbered step comments across `src/`.
- **Fix with:** comment-cleanup
- **Effort:** S
