# Coding agent campaign: Build `mycrates`

Implement a new project, `joshuarli/mycrates`, following the architectural philosophy and engineering standards of:

- ../mynpm
- ../mypypi

This is an end-to-end implementation campaign, not a design exercise. Build, test, validate, document, and commit the working result.

## 1. Mission

Build a lean, self-hosted Rust crate registry and dependency archive that eliminates my reliance on crates.io for publishing and preserves exact upstream dependencies for reproducible builds.

I want the Rust equivalent of my existing `../mynpm` and `../mypypi` projects.

The fundamental design is:

**Cargo-compatible sparse indexes + verified immutable `.crate` artifacts + deterministic static publication + minimal optional authenticated upload API.**

The result must support:

1. Publishing my own Rust crates without crates.io-specific size, rate, or metadata restrictions.
2. Native `cargo publish --registry mycrates`.
3. Consuming privately published crates using ordinary Cargo dependency declarations.
4. Importing exact public crate versions from an existing `Cargo.lock`.
5. Archiving those dependencies with verified original checksums.
6. Serving an independent crates.io mirror through Cargo source replacement.
7. Reproducible, atomic generation of static registry metadata.
8. Validation using the real Cargo client, not merely internally simulated requests.

The implementation must be production-quality but aggressively avoid unnecessary complexity.

## 2. Study existing projects first

Clone and thoroughly inspect `mynpm` and `mypypi`, especially their READMEs, AGENTS.md files, publication machinery, artifact stores, validation mechanisms, and integration tests.

Specifically study:

**mynpm**
- `src/acquire.ts`
- `src/inventory.ts`
- `src/inventory-store.ts`
- `src/publish.ts`
- `src/validate.ts`
- `src/lock-npm.ts`
- `src/project-snapshot.ts`

**mypypi**
- `src/mypypi/acquire.py`
- `src/mypypi/catalog.py`
- `src/mypypi/index.py`
- `src/mypypi/store.py`
- `src/mypypi/validate.py`

Extract the architectural lessons: exact package identity, separated acquisition and publication, integrity verification, immutable artifacts, filesystem safety, deterministic indexes, generation staging, crash recovery, and independent client validation.

Do not blindly port their implementation. Cargo's registry and lockfile semantics differ substantially from npm and Python packaging.

Consult the current official Cargo documentation:

- https://doc.rust-lang.org/cargo/reference/registries.html
- https://doc.rust-lang.org/cargo/reference/registry-index.html
- https://doc.rust-lang.org/cargo/reference/registry-web-api.html
- https://doc.rust-lang.org/cargo/reference/source-replacement.html

Use the latest stable Cargo for compatibility testing.

## 3. Core architecture

Implement in Rust, using the stable toolchain.

Prefer a straightforward library-plus-CLI layout with small, cohesive modules.

No database server, background job queue, Kubernetes, Docker requirement, web UI, Git-backed registry index, or large application framework.

The registry has two logically distinct parts.

### A. Native `mycrates` registry

A genuine alternate Cargo registry containing packages that I independently publish.

It must support native Cargo publishing and consumption.

It has its own sparse index and its own package identities. Crate names may overlap with crates.io names without conflating the two registries.

### B. Read-only crates.io archive

An independently maintained, exact-byte archive of selected crates.io releases.

This must use Cargo's source-replacement mechanism so existing Cargo.lock entries retain their crates.io source identity.

Do not treat this mirror as an alternate registry containing independently published packages.

Cargo requires replacement sources to preserve the original artifact contents. Never modify or rebuild public crates inside this mirror.

Do not mix independently published private crates into the crates.io replacement index.

Both registry views may reuse the same underlying content-addressed artifact store.

## 4. Native Cargo sparse index

Implement the official HTTP sparse registry protocol.

Generate:

- Correct `config.json` files.
- Crate metadata using Cargo's official index schema.
- Correct lowercase name-based index sharding.
- JSON Lines records for published crate versions.
- SHA-256 checksums matching actual `.crate` bytes.
- Dependencies, including renamed dependencies and registry identity.
- Feature definitions, optional dependencies, target-specific dependencies, dependency kinds, `links`, and `rust_version`.
- Schema-version fields when needed.
- Yanked state.

Preserve new index schema fields when archiving upstream metadata. Do not silently truncate unsupported fields.

Pay particular attention to the semantic differences between the publish API, Cargo metadata, and registry index formats.

For a non-crates.io index, `registry: null` in an index dependency means the current registry, not automatically crates.io.

Explicitly validate cross-registry dependencies. Test crates.io dependencies, private registry dependencies, and renamed dependencies.

Use the modern sparse protocol exclusively. Do not implement a legacy Git index.

Generate correct absolute download URLs using Cargo's supported index configuration substitutions, preferably `{sha256-checksum}` for immutable artifacts.

Support hosting at both an origin root and beneath an arbitrary path prefix.

## 5. Artifact acquisition and storage

Implement a verified, immutable artifact store.

Use SHA-256 for `.crate` identity and index checksums.

Store artifacts using content-addressed filenames, with their original crate names and versions recorded in persistent metadata.

Requirements:

- Stream downloads and hashing rather than loading large archives into memory.
- Verify upstream SHA-256 checksums against the official crates.io index.
- Never accept a checksum mismatch.
- Reject duplicate crate/version publication when bytes differ.
- Treat identical repeated ingestion as idempotent.
- Preserve previously published versions.
- Defend against corrupted gzip archives, malformed tar entries, path traversal, symlink escape, archive bombs, and resource exhaustion.
- Verify embedded package name and version against declared metadata.
- Avoid executing untrusted build scripts or procedural macros during acquisition.
- Use configurable safety limits for upload size and archive expansion. Do not inherit crates.io's arbitrary publishing limits as fixed policy.

Keep network fetches bounded by explicit timeouts, concurrency limits, redirect policy, and download budgets.

A verified artifact must not become publishable if its metadata or bytes subsequently change.

## 6. Native publishing

This is a primary objective, not optional polish.

Support the normal workflow:

`cargo publish --registry mycrates`

Implement the required Cargo Registry Web API endpoint:

`PUT /api/v1/crates/new`

Decode Cargo's actual binary publish format: length-prefixed JSON metadata followed by length-prefixed `.crate` data.

Provide strict bounds checking and graceful Cargo-compatible error responses.

Authentication:

- Support ordinary Cargo registry tokens.
- Use `Authorization` header validation.
- Support `cargo login --registry mycrates` and the `CARGO_REGISTRIES_MYCRATES_TOKEN` environment variable.
- A single-operator token model is sufficient initially.
- Never log tokens or persist plaintext secrets in public output.
- Reject anonymous publishing and invalid credentials.
- Keep the API accessible over HTTPS when remotely deployed.

Keep the write component extremely small. It may run as a localhost HTTP service behind an existing HTTPS reverse proxy. It must not become a large always-on registry application.

On successful publication:

1. Authenticate and validate the request.
2. Verify crate metadata against the uploaded archive.
3. Enforce name/version uniqueness.
4. Persist the immutable artifact durably.
5. Generate an updated index.
6. Atomically expose the new publication.
7. Return Cargo-compatible success JSON.

A successful publication must become visible to Cargo's subsequent index polling. Do not acknowledge success while a disconnected deployment process is still required to expose the crate.

Implement the minimal yank/unyank endpoints as well.

Do not implement user profiles, organization accounts, owner invitation flows, package search APIs, dashboards, or crates.io's social features.

## 7. Static publishing

Preserve the strongest feature of `mynpm` and `mypypi`: the normal read path should be entirely static.

Publish immutable generations with a layout conceptually similar to:

    site/
      blobs/
        <sha256>.crate
      generations/
        <generation-id>/
          registry/
            config.json
            ...
          crates-io/
            config.json
            ...
      current -> generations/<generation-id>

Adapt the precise paths where required for HTTP serving, Cargo URL resolution, and filesystem portability.

The two indexes must remain separate.

Implement:

- Deterministic metadata rendering and stable ordering.
- A generation ID derived from canonical publication content.
- Staging before publication.
- Atomic generation switching.
- Concurrent publisher exclusion.
- Filesystem synchronization where required.
- Clean recovery after interrupted writes.
- No-op detection for identical publication state.
- Integrity verification of existing stored artifacts.
- Safe handling of symlinks and managed paths.
- Preservation of older generations for rollback.

Ensure new metadata never references unavailable blobs.

Keep the artifact store stable across generations.

Support ordinary static HTTP servers without requiring custom application logic for dependency resolution.

Document cache and ETag behavior, including how a newly published index becomes visible through an HTTP cache or CDN.

Do not assume filesystem symlinks work on object storage. Keep deployment backend concerns separate from the local publication model.

## 8. Cargo.lock import and upstream mirroring

Implement a workflow analogous to `mynpm import-lock`.

Example:

`mycrates import-lock ../laputa/Cargo.lock --project laputa`

It must:

- Parse modern Cargo.lock formats correctly.
- Identify exact registry package names, versions, checksums, and sources.
- Preserve the distinction between crates.io, alternate registries, Git sources, and local packages.
- Record the exact dependency inventory.
- Retrieve the appropriate crates.io sparse index entries.
- Download the corresponding original `.crate` artifacts.
- Verify all expected SHA-256 checksums.
- Preserve exact upstream metadata semantics.
- Never silently substitute newer versions.
- Never modify the input project or its lockfile.

Support direct exact-version selection:

`mycrates add serde@1.0.228`

Do not quietly resolve this to the latest version.

For Git dependencies, path dependencies, patches, or unsupported external sources, report explicitly what cannot be archived through the registry mechanism.

A complete Cargo.lock-based archive should be independently consumable without downloading corresponding artifacts from crates.io.

Crucially, **do not claim the archive contains every possible semver resolution** merely because it contains the versions locked by one project.

## 9. CLI design

Provide a concise, consistent CLI.

Suggested commands:

- `mycrates init`
- `mycrates add`
- `mycrates import-lock`
- `mycrates plan`
- `mycrates fetch`
- `mycrates verify`
- `mycrates publish`
- `mycrates validate`
- `mycrates doctor`
- `mycrates serve-publish`

Here, `mycrates publish` means rendering and publishing the static index generation; native `cargo publish` uses the separately exposed upload endpoint.

Use explicit arguments for configuration, state directory, and site output.

Avoid an elaborate plugin architecture or subcommand hierarchy.

Use a small, versioned TOML configuration file containing public URLs, storage paths, upstream registry settings, and resource policies.

Keep persistent state human-inspectable where practical.

Separate command parsing, state management, registry protocol logic, acquisition, storage, publication, and validation.

## 10. Cargo client configuration

Document the actual configuration users need.

For the native registry, support:

    [registries.mycrates]
    index = "sparse+https://deps.example.net/mycrates/current/registry/"

Users should be able to publish with:

`cargo publish --registry mycrates`

And consume a native package using:

`my-library = { version = "1.2.3", registry = "mycrates" }`

Document use of:

`publish = ["mycrates"]`

to prevent accidentally publishing a package to crates.io.

For the upstream archive, document proper Cargo source replacement, conceptually:

    [source.crates-io]
    replace-with = "mycrates-mirror"

    [source.mycrates-mirror]
    registry = "sparse+https://deps.example.net/mycrates/current/crates-io/"

Prove that this preserves Cargo.lock source identity and checksum verification.

Do not silently rewrite existing package manifests or Cargo.lock files.

Treat the example hostname as a placeholder, not an actual deployed endpoint.

## 11. Real Cargo validation

Tests must exercise actual Cargo behavior.

Build a local HTTP fixture server supporting the generated static registry and minimal upload API.

Use isolated temporary `CARGO_HOME` directories and disposable consumer projects.

Mandatory integration scenarios:

**Native registry**
- Publish a simple crate using actual `cargo publish --registry mycrates`.
- Fetch and build it as a dependency from a fresh consumer project.
- Publish another version and verify version selection.
- Publish a crate depending on a public crates.io package.
- Publish a crate depending on another native `mycrates` package.
- Test renamed, optional, target-specific, and build dependencies.
- Verify authentication failures and publish conflicts.
- Yank and unyank a version; test actual Cargo resolution behavior.

**Upstream archive**
- Import an actual Cargo.lock dependency closure.
- Verify archived checksums against the original registry.
- Configure Cargo source replacement.
- Run real `cargo metadata --locked` and `cargo fetch --locked`.
- Verify that all relevant artifacts come from the archive.
- Ensure original lockfile identity and checksum expectations are maintained.
- Test with incomplete archives and unsupported source types.

**Reliability/security**
- Same version, different contents.
- Duplicate ingestion.
- Case-insensitive name collisions and normalization conflicts.
- SemVer build-metadata version uniqueness.
- Malformed upload framing.
- Malicious archive paths.
- Excessive file sizes and archive expansion.
- Crash during publication.
- Concurrent writers.
- Corrupted stored blobs.
- Broken or stale publication generations.
- Root-path versus prefix-path hosting.

Test network isolation explicitly where feasible. A strict archive validation should not need external registry access after complete acquisition.

Do not claim a test proves air-gapped execution unless external network access was actually denied.

Cargo builds may execute third-party code. Keep fetch-only verification separate from trusted build execution.

## 12. Implementation quality

Prefer Rust-native components with a modest dependency tree.

Avoid OpenSSL dependencies if practical; prefer a suitable Rust TLS implementation for outbound HTTPS.

Do not introduce unnecessary Tokio services, ORM layers, distributed locks, complex generic abstractions, or configuration machinery.

Use POSIX-friendly deployment and service management. Do not require systemd or D-Bus.

Follow modern Rust practices:

- Explicit error types and useful diagnostics.
- Typed metadata rather than loosely structured JSON throughout.
- Structured validation at trust boundaries.
- RAII for resource and lock management.
- Streaming I/O and bounded allocation.
- Deterministic serialization.
- Minimal global mutable state.
- Testable pure index-rendering and metadata-transformation functions.
- Fail closed for unsupported protocol features.

Prefer existing high-quality Rust libraries for well-defined formats, but evaluate dependency weight rather than adding crates indiscriminately.

No unnecessary `unsafe`.

Use CI for formatting, linting, unit tests, and real Cargo integration tests.

Pin GitHub Actions appropriately following the conventions of my related repositories.

## 13. Non-goals

Do not build a complete crates.io replacement.

Explicitly exclude:

- Web-based package browsing.
- Search ranking.
- GitHub or OAuth login.
- Multi-tenant accounts and organizations.
- Dependency analytics.
- Download statistics.
- Vulnerability scanning infrastructure.
- Automatic publishing to crates.io.
- Automatic mirroring of the entire crates.io catalog.
- Container orchestration.
- Database services.
- General-purpose cloud deployment frameworks.

A minimal HTTPS deployment example is useful. A deeply integrated hosting platform is not necessary.

The output should remain portable to an ordinary machine, Nginx/Caddy static server, or a future Cloudflare R2 deployment adapter.

## 14. Documentation and delivery

Write a concise README with working commands covering:

- Installation.
- Configuration.
- Importing locked upstream dependencies.
- Fetching and verifying artifacts.
- Publishing a static archive.
- Hosting the resulting site.
- Configuring Cargo source replacement.
- Publishing a first private crate.
- Consuming a private crate.
- Yanking a release.
- Validating the installation.
- Backup, restore, and recovery.
- Security assumptions and limitations.

Add an AGENTS.md reflecting the actual project layout and development commands.

Make sure examples are executable and verified rather than hypothetical.

Include a realistic Cargo consumer fixture, not only synthetic unit tests.

## 15. Acceptance criteria

The campaign is complete only when all of the following are demonstrated:

1. A new Rust crate can be uploaded with the genuine `cargo publish --registry mycrates` command.
2. Another clean project can download and compile that crate using the generated sparse index.
3. A native `mycrates` package can depend on both another private package and an ordinary crates.io package.
4. An imported crates.io Cargo.lock dependency closure can be fetched from the private archive with source replacement.
5. All archived bytes match their original SHA-256 checksums.
6. Repeated publication is safe, and conflicting publication cannot overwrite an existing version.
7. Static generation switching is deterministic and crash-safe.
8. The system can run without a database or continuously running read-side application.
9. Unit and integration tests pass on the available development host.
10. The README provides verified end-to-end instructions.

Do not stop after scaffolding, writing a design document, or generating a few static index files.

Implement the system fully, test it using real Cargo clients, and fix the issues you discover.

Commit working changes directly to `main` following my normal repository workflow. If `joshuarli/mycrates` does not yet exist, initialize the project locally and create/push the repository when authorized credentials permit.

At completion, summarize the architecture, actual test results, final commit SHA, and any genuinely unresolved limitations.

**Optimize for the smallest maintainable system that correctly implements Cargo's protocols—not the largest feature set.**
