# Security

Report suspected vulnerabilities through [GitHub's private vulnerability reporting](https://github.com/resoltico/AuditoryLab/security/advisories/new). Include the affected version, operating system, a minimal input and reproducible steps. Keep credentials and private data out of reports.

Use public issues for ordinary defects and questions. The current public release is the supported version for security fixes; a fix may require upgrading. No response-time commitment is made.

## Input and native boundaries

Treat custom recipes, their metadata and native paths as untrusted input. Both frontends use the same bounded strict parser, rate-dependent validation and staged no-clobber publisher through auditory-app. Diagnostics retain the exact bounded failed-input snapshot when a span is available; they do not reread a changing file. Display escaping and typed diagnostic context are separate from trusted exact built-in recipe output. The program does not automatically open recipe source URLs.

GUI file selection, graphics, accessibility and audio involve native system components. Dependency and source guards enforce architectural boundaries; they do not provide a sandbox or detect arbitrary copied algorithms. No updater, elevation helper, network debug server or general GPU API is exposed. Supported Linux GUI builds run from the repository root, link system FreeType/Fontconfig and prevent compilation of the bundled native font fallback through Cargo links overrides. Keep operating-system font libraries updated and use the checked Cargo configuration. Graphics drivers/loaders can be dynamic; import lists and source hashes do not establish their runtime behavior or legal completeness.

Generated signal observation is optional, bounded and lossy. It cannot backpressure audio or substitute for terminal completion/cancellation. Charts show generated digital samples, not microphone input, sound pressure or proof of acoustic delivery. Ordinary close/quit waits for actual worker cleanup; force termination, power loss and native calls can still leave documented staging/durability limits. Do not change devices or volume to reproduce a report without the user's authorization.

Build identity distinguishes unqualified local, source-bound candidate and release-kind artifacts. It detects wrong/stale products inside the controlled pipeline; it is not an authenticity guarantee for an arbitrary binary. A candidate is not a public security-supported release, and no test count or attestation substitutes for the specific platform/accessibility/audio property it does not establish.
