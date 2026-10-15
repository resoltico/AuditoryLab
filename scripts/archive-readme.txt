Auditory Lab {version}

Auditory Lab plays and exports sounds used in hearing and brain-rhythm
research: binaural and monaural beats, tones, clicks and pulsed tones. Each
built-in sound says what it is, what its source study tested and what the
study found. It runs in a terminal.

Before you listen
  Turn the volume down first, then raise it slowly. Digital levels do not
  limit how loud the sound is, and long sessions at high volume can damage
  hearing. Use headphones for binaural beats: each ear must hear its own tone.

Start here, in a terminal opened in this folder
  macOS or Linux:  ./auditory-lab list
  Windows:         .\auditory-lab.exe list

  list                          every built-in sound
  about NAME                    what it is and what its study found
  play NAME                     play it; Ctrl-C stops
  export NAME --output a.wav    save it as a WAV file instead
  show NAME --output mine.toml  save an editable copy of its recipe
  --help                        every command and option

Check the download
  Compare the archive's SHA-256 digest with its line in SHA256SUMS from the
  release page:
    macOS:    shasum -a 256 FILE
    Linux:    sha256sum FILE
    Windows:  certutil -hashfile FILE SHA256

macOS
  Built for macOS {macos_aarch64} or later on Apple silicon and {macos_x86_64} or later on Intel;
  release testing runs on macOS 15. The program is signed ad hoc, not by an
  Apple developer identity. If macOS blocks it after you verified the
  download, use the app-specific Open Anyway option if offered in System
  Settings > Privacy & Security, or System Preferences > Security & Privacy
  > General.

Linux
  Needs the ALSA sound library (libasound.so.2) and a glibc at least as new as
  Ubuntu 22.04's (2.35), where release testing runs. On Ubuntu 22.04:
    sudo apt-get install libasound2
  On other distributions, install the package that provides libasound.so.2.

Windows
  Needs 64-bit Windows 10 or later (x86_64) or Windows 11 (ARM64); release
  testing runs on Windows Server 2022 and Windows 11 ARM64. The x86_64 build
  requires SSE3, CMPXCHG16B and LAHF/SAHF instructions. Startup/support and
  unwind code are open-source MinGW and LLVM; the Universal C Runtime (UCRT)
  comes from Windows. No separate runtime needs installing. Windows may warn
  about an unrecognized publisher.

Documentation: https://github.com/{repository}/tree/v{version}#readme
Auditory Lab is licensed under the Mozilla Public License 2.0 (LICENSE);
its source code is at the link above. Third-party software notices are in
THIRD-PARTY-NOTICES.txt.
