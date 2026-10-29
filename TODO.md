Prefix: MYC
Next: 6

### MYC-5 · P4 · Stale milestone and numbered implementation comments in production source

- **Area:** Dead code
- **Where:** `src/lib.rs:8`, `src/validate.rs:31`, `src/publish.rs:133`, `src/acquire.rs:81`, `src/serve.rs:98`
- **Problem:** Numbered milestone comments and step markers from campaign execution remain in production modules, contrary to the repository working contract prohibiting unnecessary code comments.
- **Recommendation:** Remove obsolete milestone markers and numbered step comments across `src/`.
- **Fix with:** comment-cleanup
- **Effort:** S
