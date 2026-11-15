# Auditory Lab licensing and source

Copyright © 2026 Ervins Strauhmanis. Contributors retain copyright in their contributions.

Original Auditory Lab source, tests, tooling and accompanying documentation in this checkout are
licensed under the GNU Affero General Public License, version 3 only (`AGPL-3.0-only`), unless
separate terms are identified for a file. See the unmodified license text in [LICENSE](LICENSE).
You may copy, modify and convey this work under those terms. It comes WITHOUT ANY WARRANTY,
including the implied warranties of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE;
see sections 15–16. Third-party components retain their own licenses and notices. This notice
neither relicenses third-party material nor grants exceptions to its terms.

Commercial use is permitted by the AGPL. Charging money, internal business use or offering a
SaaS service does not, by itself, require a paid license. Sections 4–6 govern conveyance of
covered source and object code. Under section 13, a modified version that supports remote
network interaction must prominently offer all users interacting with it an opportunity to
receive its Corresponding Source, without charge, through a standard means of copying software.
The required scope follows the license's definitions; it is not automatically every unrelated
system, service or item of business data. Read the license for the actual conditions.

Original project-owned artwork, drawing inputs, editable masters, embedded artwork
payloads and generated artwork use the same AGPL-3.0-only license. This grants no
trademark rights. Third-party artwork, fonts and other components keep their own terms.

## Source for this build

Release packaging produces `auditorylab-VERSION-source.tar.gz` alongside the
binary archives. Obtain the source archive for the same build as the binaries;
`SOURCE.txt` identifies its source revision and download location. The archive contains
the tracked project sources, artwork and build/install inputs plus Cargo's vendored
locked dependency sources, including local patches. It also includes Cargo configuration
to restore those dependencies without a registry download. Retain the matching archive
with binaries you convey. The general-purpose Rust toolchain and system libraries are
not included; prerequisites and build commands are in [CONTRIBUTING.md](CONTRIBUTING.md)
and [docs/releasing.md](docs/releasing.md).

When conveying modified binaries or deploying a modified covered work under section 13,
provide the matching Corresponding Source, including additional required build/install
inputs and affected notices. An upstream source link or unchanged archive does not cover
modifications it omits. Keep credentials, private runtime configuration and customer data
out of source publications. This archive mechanism aids delivery; it does not determine
that every deployment satisfies the license.

Upstream: https://github.com/resoltico/AuditoryLab.
For separate commercial licensing status, see [LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md).

## Contribution terms

By submitting a contribution (code, documentation, tests or other material) to this
repository, you:

- confirm that you wrote it or otherwise have the right to submit it on these terms;
- license it to everyone under this repository's public license, `AGPL-3.0-only`; and
- also grant Ervins Strauhmanis a perpetual, worldwide, non-exclusive, royalty-free,
  irrevocable license to use, reproduce, modify, distribute and sublicense it, and to license
  it under other terms, including the commercial terms described in
  [LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md).

You keep the copyright in your contribution. Do not submit material you cannot license on
these terms; third-party components keep their own licenses. Each pull request confirms these
terms with the checkbox in its template.
