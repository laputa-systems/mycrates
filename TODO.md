Prefix: MYC
Next: 6

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
