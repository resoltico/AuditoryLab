#!/usr/bin/env python3
# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Build and package both native products without any publication capability."""
import argparse
import json
import hashlib
import os
import signal
import re
from pathlib import Path
import subprocess
import sys
import tempfile

from release import (ROOT, PRODUCTS, TARGETS, cargo_identities, MACOS_MINIMUM,
                     validate_build_request)
from package import package, run, subprocess_failure

FREESTANDING_TARGET = "thumbv7em-none-eabi"


def check_source(revision):
    """Require the exact clean immutable source at each qualification boundary."""
    if run("git", "rev-parse", "HEAD", cwd=ROOT).strip() != revision:
        raise ValueError("qualification source revision differs from HEAD")
    if run("git", "status", "--porcelain", "--untracked-files=all", cwd=ROOT).strip():
        raise ValueError("qualified builds require a clean checkout; local builds must use local kind")


def check_ambient_cargo_configuration(environment):
    cargo_home = Path(environment.get("CARGO_HOME", str(Path.home() / ".cargo")))
    external = [cargo_home / name for name in ("config", "config.toml")]
    external += [parent / ".cargo" / name for parent in ROOT.resolve().parents for name in ("config", "config.toml")]
    if any(path.exists() or path.is_symlink() for path in external):
        raise ValueError("native qualification requires a clean Cargo configuration environment; external ancestor/CARGO_HOME config exists (settings were not changed)")


def controlled_environment(product, entry, build_kind, source_revision):
    environment = os.environ.copy()
    ambient = [name for name in environment if name in {
        "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS", "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
        "RUSTC", "CARGO_BUILD_RUSTC", "RUSTDOC", "CARGO_BUILD_RUSTDOC", "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTDOCFLAGS"
    } or (name.startswith("CARGO_TARGET_") and name.endswith(("_RUSTFLAGS", "_LINKER", "_RUNNER")))]
    if ambient:
        raise ValueError("unset unsupported native compiler overrides: " + ", ".join(sorted(ambient)))
    forbidden = [name for name, value in environment.items()
                 if (name.startswith("SLINT_") and not (name == "SLINT_COMPILER_DENY_WARNINGS" and value == "1"))
                 or name.startswith("DEP_MCU_BOARD_SUPPORT_")]
    if forbidden:
        raise ValueError("unset unsupported GUI compiler inputs: " + ", ".join(sorted(forbidden)))
    check_ambient_cargo_configuration(environment)
    environment.pop("MACOSX_DEPLOYMENT_TARGET", None)
    environment.pop("AUDITORY_BUILD_KIND", None)
    environment.pop("AUDITORY_SOURCE_REVISION", None)
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/native" / build_kind / product)
    if build_kind != "local":
        environment["AUDITORY_BUILD_KIND"] = build_kind
        environment["AUDITORY_SOURCE_REVISION"] = source_revision
    if "apple" in entry["target"]:
        environment["MACOSX_DEPLOYMENT_TARGET"] = MACOS_MINIMUM
    environment["SLINT_COMPILER_DENY_WARNINGS"] = "1"
    return environment


def check_linux_font_overrides(config=None):
    """Fail closed before Cargo can run the bundled FreeType build script."""
    if (ROOT / ".cargo/config").exists():
        raise ValueError("extensionless .cargo/config must not shadow system FreeType policy")
    config = (ROOT / ".cargo/config.toml").read_text(encoding="utf-8") if config is None else config
    uncommented = "\n".join(line.split("#", 1)[0] for line in config.splitlines())
    if '"""' in config or "'''" in config or "\\" in config or re.search(r"\binclude\b", uncommented):
        raise ValueError("multiline strings and Cargo config includes are outside the checked native policy")
    tables = re.split(r"(?m)^\[([^]\n]+)\]\s*$", config)
    for target in ("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"):
        name = "target." + target + ".freetype"
        bodies = [tables[index + 1] for index in range(1, len(tables), 2) if tables[index] == name]
        if len(bodies) != 1:
            raise ValueError("required Cargo system FreeType override missing or repeated: " + target)
        body = "\n".join(line.split("#", 1)[0].strip() for line in bodies[0].splitlines()).strip()
        if not re.fullmatch(r'rustc-link-lib\s*=\s*\[\s*"dylib=freetype"\s*\]', body):
            raise ValueError("Cargo FreeType override must select only dylib=freetype: " + target)


def check_gui_profile(target):
    check_linux_font_overrides()
    output = run("cargo", "tree", "--locked", "-p", PRODUCTS["gui"]["package"],
                 "--target", target, "--edges", "normal,build", "--prefix", "none", "--format", "{p}|{f}", cwd=ROOT, encoding="utf-8")
    selected = {}
    for line in output.splitlines():
        if not line:
            continue
        record = line.split("|", 1)
        if len(record) != 2 or len(record[0].split()) < 2:
            raise ValueError("cannot understand Cargo GUI profile output: " + line)
        words = record[0].split()
        selected.setdefault((words[0], words[1]), set()).update(
            record[1].removesuffix(" (*)").split(","))
    required = {
        ("slint", "v1.18.1"): {"std", "compat-1-18", "backend-winit", "renderer-software", "renderer-femtovg-wgpu", "i-slint-renderer-femtovg", "accessibility", "raw-window-handle-06", "unstable-winit-030"},
        ("slint-build", "v1.18.1"): {"compat-1-18"},
    }
    for identity, wanted in required.items():
        if selected.get(identity) != wanted:
            raise ValueError("GUI Slint version/features differ from the reviewed GPU/software profile: " + identity[0])
    if "linux" in target:
        linux_required = {
            ("sctk-adwaita", "v0.10.1"): {"crossfont"},
            ("crossfont", "v0.9.0"): {""},
            ("freetype-sys", "v0.20.1"): {""},
            ("yeslogic-fontconfig-sys", "v6.0.1"): {""},
        }
        if any(selected.get(identity) != features for identity, features in linux_required.items()):
            raise ValueError("Linux GUI must use the reviewed system-font producers/features")
        winit = selected.get(("winit", "v0.30.13"), set())
        if "wayland-csd-adwaita-crossfont" not in winit or winit & {"wayland-csd-adwaita", "wayland-csd-adwaita-notitle"}:
            raise ValueError("Linux GUI requires crossfont CSD with title text and controls")
        if any(name in {"ab_glyph", "owned_ttf_parser", "ttf-parser", "bincode"} for name, _ in selected):
            raise ValueError("unreviewed Linux GUI font/codec producer selected")
    for (name, version), features in selected.items():
        if name in {"slint", "slint-build"} and version != "v1.18.1":
            raise ValueError("GUI runtime/compiler must both be the reviewed Slint 1.18.1")
        if name in {"i-slint-renderer-skia", "i-slint-renderer-anyrender",
                    "i-slint-backend-qt", "ksni", "mach-dxcompiler-rs"}:
            raise ValueError("unreviewed GUI renderer/backend selected: " + name)
        gpu_internal = {
            "i-slint-core": {"unstable-wgpu-30", "wgpu-30"},
            "i-slint-backend-winit": {"unstable-wgpu-30"},
            "i-slint-renderer-femtovg": {"unstable-wgpu-30", "wgpu-30"},
        }
        if any("static-dxc" in feature for feature in features):
            raise ValueError("static DXC is not part of the reviewed GPU profile")
        if name.startswith("i-slint-") and any(
            feature.startswith(("system-testing", "live-preview", "mcp", "system-tray"))
            or (feature.startswith(("unstable-wgpu", "wgpu-")) and feature not in gpu_internal.get(name, set()))
            or feature in {"renderer-skia", "renderer-femtovg", "renderer-vello", "backend-qt"}
            for feature in features):
            raise ValueError("unreviewed internal GUI feature selected: " + name)
        if name == "libm" and "force-soft-floats" not in features:
            raise ValueError("portable signal arithmetic requires libm force-soft-floats after feature unification")
        if name in {"wgpu", "wgpu-core", "wgpu-hal"} and version != "v30.0.1":
            raise ValueError("locked WGPU producer changed; review the new profile/source")


def run_logged(command, environment, log):
    log.parent.mkdir(parents=True, exist_ok=True)
    print("Running " + " ".join(map(str, command)) + "; evidence: " + str(log), flush=True)
    with log.open("w", encoding="utf-8") as output:
        result = subprocess.run(command, cwd=ROOT, env=environment, stdout=output, stderr=subprocess.STDOUT)
    if result.returncode:
        tail = log.read_bytes()[-16384:].decode("utf-8", errors="backslashreplace")
        error = subprocess.CalledProcessError(result.returncode, command, output=tail)
        raise ValueError(f"native command failed; full evidence: {log}\n" + subprocess_failure(error)) from error
    return log.read_bytes().decode("utf-8", errors="backslashreplace")


def validate_offline_records(name, evidence):
    """Check discovery/workload evidence without turning timing into a threshold."""
    if name.startswith("warmed_") and not any(line.startswith("BENCH,") for line in evidence.splitlines()):
        raise ValueError("offline callback budget produced no active-workload records")
    if name == "observed_production_callback_budget":
        rows = [line.split(",") for line in evidence.splitlines() if line.startswith("OBS_BENCH,")]
        from itertools import product
        expected = set(product(("disabled", "drained", "full", "disconnected"), (44100, 48000, 96000, 768000),
                               (64, 256, 1024), (1, 4, 16), ("plain", "sam", "gated"), (2, 6),
                               ("f64", "f32", "i32", "i24", "i16")))
        actual = []
        for row in rows:
            if len(row) != 17:
                raise ValueError("malformed observed callback record")
            actual.append((row[1], int(row[2]), int(row[3]), int(row[4]), row[5], int(row[6]), row[7]))
            if int(row[8]) != 128 or int(row[9]) != int(row[3]) * 128 or int(row[10]) <= 0:
                raise ValueError("observed callback record did not measure the active workload")
        if len(actual) != len(expected) or set(actual) != expected:
            raise ValueError("observed callback modes/workloads are missing, repeated or unexpected")
    if name == "maximum_rate_observation_and_snapshot_budget":
        rows = [line for line in evidence.splitlines() if line.startswith("OBSERVATION_MAX_RATE,")]
        if len(rows) != 1:
            raise ValueError("maximum-rate observation measurement missing or repeated")
        values = dict(field.split("=", 1) for field in rows[0].split(",")[1:])
        if (int(values["hz"]), int(values["polls"]), int(values["active_frames"])) != (768000, 60, 1536000):
            raise ValueError("maximum-rate observation active workload differs")
        total = int(values["capacity_bytes"])
        owned = int(values["owned_capacity_bytes"])
        reserved = int(values["presentation_reserved_bytes"])
        if (not 0 < owned <= total <= 8 * 1024 * 1024 or reserved != 0 or owned + reserved != total
                or int(values["packets_per_poll"]) != 188):
            raise ValueError("maximum-rate observation capacity/budget evidence differs")


def renderer_evidence(text, requested):
    """Use pinned Slint first-frame instrumentation, never requested mode alone."""
    if "NATIVE_WINDOW_STARTED" not in text:
        raise ValueError("native GUI window startup assertions did not execute")
    # Backend selection is logged at collector construction; the reporter starts
    # only after measure_frame_rendered. Lazy one-second counts may be zero.
    reported_frame = "average frames per second:" in text
    gpu = "Backend: FemtoVG renderer with WGPU backend" in text and reported_frame
    software = "Backend: software" in text and reported_frame
    if gpu and software:
        raise ValueError("native renderer smoke has ambiguous GPU/software first-frame reports")
    if requested == "software" and not software:
        raise ValueError("forced software smoke did not render a measured first frame")
    if requested == "gpu" and not (gpu or software):
        raise ValueError("GPU request produced no measured GPU or software fallback first frame")
    return {"requested": requested, "actual": "femtovg-wgpu" if gpu else "software",
            "wgpu_first_frame": gpu, "wgpu_unavailable": requested == "gpu" and not gpu,
            "hardware_adapter": "unverified",
            "display": "native"}


def renderer_environment(environment, mode):
    child = environment.copy()
    for name in ("SLINT_WGPU_CPU", "WGPU_ADAPTER_NAME", "WGPU_BACKEND"):
        child.pop(name, None)
    child["SLINT_BACKEND"] = "winit-femtovg-wgpu" if mode == "gpu" else "winit-software"
    child["SLINT_DEBUG_PERFORMANCE"] = "refresh_lazy,console"
    return child


def run_native_test(binary, target, environment, log, timeout):
    """Run a finished nonshipping UI executable with bounded child cleanup."""
    command = [str(binary)]
    if "linux" in target:
        command = ["xvfb-run", "-a", *command]
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open("w", encoding="utf-8") as output:
        process = subprocess.Popen(command, cwd=ROOT, env=environment, stdout=output,
                                   stderr=subprocess.STDOUT, start_new_session=os.name != "nt")
        try:
            status = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired as error:
            if os.name == "nt":
                process.kill()
            else:
                os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                if os.name == "nt":
                    process.kill()
                else:
                    os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            raise ValueError(f"native UI test timed out; evidence retained at {log}") from error
    text = log.read_bytes().decode("utf-8", errors="backslashreplace")
    if status:
        raise ValueError(f"native UI test exited {status}; evidence: {log}\n{text[-16384:]}")
    return text


def run_renderer(binary, target, environment, mode, log):
    child = renderer_environment(environment, mode)
    child["AUDITORY_GUI_WINDOW_TEST"] = "1"
    text = run_native_test(binary, target, child, log, 30)
    evidence = renderer_evidence(text, mode)
    evidence["display"] = "virtual X11 (Xvfb)" if "linux" in target else "native"
    return evidence


def build_native_test(target, environment, results, gui_id, name):
    # Runtime overrides are supplied only to the finished test executable. They
    # must never enter GUI build.rs, even when Cargo would reuse a cached build.
    command = ["cargo", "test", "-p", PRODUCTS["gui"]["package"], "--release", "--test", name,
               "--locked", "--target", target, "--no-run", "--message-format=json"]
    build = run_logged(command, environment, results / (name.replace("_", "-") + "-build.log"))
    artifacts = []
    for line in build.splitlines():
        if not line.startswith("{"):
            continue
        message = json.loads(line)
        if (message.get("reason") == "compiler-artifact" and message.get("package_id") == gui_id
                and message.get("target", {}).get("name") == name
                and "test" in message.get("target", {}).get("kind", []) and message.get("executable")):
            artifacts.append(Path(message["executable"]))
    if len(artifacts) != 1:
        raise ValueError("Cargo did not report exactly one GUI " + name + " test executable")
    return artifacts[0]


def native_window_checks(target, environment, results, gui_id):
    binary = build_native_test(target, environment, results, gui_id, "native_window")
    records = [run_renderer(binary, target, environment, mode, results / ("native-window-" + mode + ".log"))
               for mode in ("gpu", "software")]
    (results / "renderers.json").write_text(json.dumps(records, indent=2) + "\n", encoding="utf-8")
    return records


def native_signal_geometry_checks(target, environment, results, gui_id):
    binary = build_native_test(target, environment, results, gui_id, "native_signal_geometry")
    captures = results / "signal-geometry"
    captures.mkdir(parents=True, exist_ok=True)
    records = []
    for mode in ("gpu", "software"):
        for scale in ("1", "1.25"):
            # A fresh owned directory prevents old captures from satisfying a new run.
            directory = Path(tempfile.mkdtemp(prefix=mode + "-scale-" + scale + "-", dir=captures)).resolve()
            directory = directory / "captures"
            directory.mkdir()
            child = renderer_environment(environment, mode)
            child.update({"SLINT_SCALE_FACTOR": scale, "AUDITORY_GUI_GEOMETRY_TEST": "1",
                          "AUDITORY_GUI_GEOMETRY_DIR": str(directory)})
            text = run_native_test(binary, target, child, directory.parent / "capture.log", 45)
            if ("NATIVE_SIGNAL_GEOMETRY_COMPLETE cases=9" not in text
                    or sum(line.startswith("NATIVE_SIGNAL_GEOMETRY_CAPTURED ") for line in text.splitlines()) != 9):
                raise ValueError("native signal geometry fixture did not capture every case: " + str(directory))
            evidence = renderer_evidence(text, mode)
            evidence["display"] = "virtual X11 (Xvfb)" if "linux" in target else "native"
            oracle = run_logged([sys.executable, str(ROOT / "scripts/signal_pixels.py"), str(directory)],
                                environment, directory.parent / "oracle.log")
            if "NATIVE_SIGNAL_GEOMETRY_PIXELS_OK cases=9" not in oracle:
                raise ValueError("native signal pixel oracle did not verify every case")
            manifest = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
            if len(manifest.get("cases", [])) != 9 or any(
                    abs(case["scale_factor"] - float(scale)) > .001 for case in manifest["cases"]):
                raise ValueError("native signal captures did not exercise requested scale " + scale)
            pixels = json.loads((directory / "pixels.json").read_text(encoding="utf-8"))
            if pixels.get("status") != "passed" or len(pixels.get("cases", [])) != 9:
                raise ValueError("native signal pixel report is incomplete")
            records.append({"requested_scale": float(scale), "directory": str(directory),
                            "cases": 9, "renderer": evidence})
    (results / "signal-geometry.json").write_text(json.dumps(records, indent=2) + "\n", encoding="utf-8")
    return records



# This finite contract names essential behavioral controls, not the total project
# test count. New unrelated tests may be added without changing qualification.
ARCHITECTURE_CONTROLS = frozenset({
    "delivered_frontends_obey_boundary",
    "roles::tests::declared_dependencies_reject_aliases_targets_and_builds",
    "roles::tests::whole_workspace_roles_reject_wrong_directions_and_unclassified_members",
    "source::tests::canonical_source_containment_is_checked",
    "source::tests::workspace_package_and_source_roots_cannot_relocate",
    "source::tests::internal_production_inclusions_cannot_escape_admitted_inputs",
    "roles::tests::resolved_gpu_features_and_math_override_survive_feature_unification",
})


ESSENTIAL_APP_TESTS = {
    "file_admission": frozenset({"regular_documents_and_explicit_streams_retain_limits_and_identity"}),
    "frontend_boundary": ARCHITECTURE_CONTROLS,
    "workflows": frozenset({
        "all_input_adapters_preserve_identity_and_agree",
        "strict_input_limit_encoding_schema_and_reader_failure",
        "catalog_source_description_save_and_no_clobber_use_one_path",
        "cancellation_before_start_and_invalid_parent_leave_no_output",
    }),
    "diagnostics": frozenset({
        "input_categories_carry_limits_schema_and_original_native_queries",
        "filesystem_input_and_create_commit_phases_keep_native_request_context",
        "validation_covers_duration_fade_overlap_component_limits_and_both_ear_peaks",
    }),
    "observation": frozenset({
        "public_analysis_preserves_one_frame_pulses_and_separate_ear_extrema",
        "public_analysis_rejects_invalid_inputs_and_reports_actual_low_rate_extent",
    }),
    "build_identity": frozenset({
        "qualification_inputs_reject_partial_malformed_and_contradictory_values",
        "one_compiled_app_selects_distinct_products_at_each_call",
        "real_build_script_refreshes_source_kind_and_product_in_one_cargo_cache",
    }),
}


APP_CAPACITY_CONTROLS = frozenset({
    "observation::tests::all_supported_rates_fit_app_owned_types_and_checked_presentation_reservations",
    "observation::tests::future_monitoring_capacity_growth_rejects_before_audio",
})


GUI_UNIT_CONTROLS = frozenset({
    "presentation::tests::monitor_caption_and_context_allocations_obey_renderer_capacities",
    "signal_geometry::tests::empty_and_maximal_geometry_obey_actual_allocation_bounds",
    "signal_geometry::tests::short_overallocated_string_and_growth_are_rejected",
    "signal_geometry::tests::source_preflight_uses_exact_escaping_and_fixed_replacement_budget",
    "signal_geometry::tests::pinned_shared_string_conversion_has_one_accounted_allocation",
    "signal::tests::rendering_failure_suspends_only_display_until_a_new_source",
    "controller::tests::about_failure_cannot_strand_real_export_at_any_execution_phase",
    "controller::tests::about_failure_preserves_real_export_stop_close_and_join",
    "controller::tests::preparation_requires_ui_application_then_matching_permit",
    "controller::tests::committed_success_survives_stop_selection_change_and_close",
})


def require_gui_unit_target(metadata, root=ROOT):
    members = set(metadata.get("workspace_members", []))
    packages = [package for package in metadata.get("packages", [])
                if package.get("id") in members and package.get("name") == "auditory-gui"]
    expected = root.resolve() / "crates/auditory-gui"
    if len(packages) != 1 or Path(packages[0]["manifest_path"]).resolve() != expected / "Cargo.toml":
        raise ValueError("mandatory GUI unit tests require the admitted auditory-gui package")
    targets = [target for target in packages[0].get("targets", []) if target.get("name") == "auditorylab-gui"]
    if (len(targets) != 1 or targets[0].get("kind") != ["bin"] or targets[0].get("test") is not True
            or Path(targets[0]["src_path"]).resolve() != expected / "src/main.rs"):
        raise ValueError("Cargo excluded the mandatory GUI binary unit tests; check bin test=false and source paths")


def gui_unit_controls(target):
    required = set(GUI_UNIT_CONTROLS)
    if "windows" not in target:
        required.add("controller::tests::fifo_import_close_joins_after_typed_rejection")
    return required


def require_application_test_targets(metadata, root=ROOT):
    """Reject Cargo exclusions before relying on an architecture test's source."""
    root = root.resolve()
    members = set(metadata.get("workspace_members", []))
    packages = [package for package in metadata.get("packages", [])
                if package.get("id") in members and package.get("name") == "auditory-app"]
    if Path(metadata.get("workspace_root", "")).resolve() != root or len(packages) != 1:
        raise ValueError("mandatory architecture checks require the workspace auditory-app package")
    package = packages[0]
    expected = root / "crates/auditory-app"
    if Path(package["manifest_path"]).resolve() != expected / "Cargo.toml":
        raise ValueError("mandatory architecture-check package escaped its admitted workspace root")
    libraries = [target for target in package.get("targets", []) if target.get("kind") == ["lib"]]
    if (len(libraries) != 1 or libraries[0].get("test") is not True
            or Path(libraries[0]["src_path"]).resolve() != expected / "src/lib.rs"):
        raise ValueError("Cargo excluded the mandatory app library capacity controls")
    for name in ESSENTIAL_APP_TESTS:
        targets = [target for target in package.get("targets", []) if target.get("name") == name]
        if (len(targets) != 1 or targets[0].get("kind") != ["test"] or targets[0].get("test") is not True
                or Path(targets[0]["src_path"]).resolve() != expected / ("tests/" + name + ".rs")):
            raise ValueError("Cargo did not discover mandatory application integration test " + name
                             + "; check autotests, target exclusions and source paths")


def architecture_test_names(listing, required):
    names = re.findall(r"(?m)^(\S+): test$", listing)
    if not names or len(set(names)) != len(names) or not required.issubset(names):
        raise ValueError("architecture discovery omitted required positive/rejection controls")
    return set(names)


def require_architecture_execution(evidence, discovered, filtered=0):
    passed = re.findall(r"(?m)^test (\S+) \.\.\. ok$", evidence)
    summaries = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;", evidence)
    if (not discovered or len(passed) != len(set(passed)) or set(passed) != discovered
            or summaries != [(str(len(discovered)), "0", "0", "0", str(filtered))]):
        raise ValueError("mandatory controls did not all execute successfully with the required selection; empty or ignored runs are insufficient")


def essential_app_tests(target):
    required = dict(ESSENTIAL_APP_TESTS)
    platform_control = ("connected_owned_pipe_sending_no_bytes_and_native_devices_reject"
                        if "windows" in target else "fifo_symlink_and_devices_reject_before_read_without_a_writer")
    required["file_admission"] |= {platform_control}
    return required


def freestanding_core_check(environment, results):
    """Compile real core without std, and prove std-bearing edges are rejected."""
    run_logged(["cargo", "check", "--locked", "-p", "auditory-core", "--lib", "--target", FREESTANDING_TARGET],
               environment, results / "core-freestanding.log")
    with tempfile.TemporaryDirectory(prefix="auditorylab-no-std-", dir=ROOT.parent) as temporary:
        root = Path(temporary)
        (root / "src").mkdir()
        (root / "producer/src").mkdir(parents=True)
        (root / "Cargo.toml").write_text('[package]\nname="core-std-rejection-control"\nversion="0.0.0"\nedition="2024"\n[dependencies]\nproducer={path="producer"}\n[workspace]\n', encoding="utf-8")
        (root / "src/lib.rs").write_text('#![no_std]\npub use producer::value;\n', encoding="utf-8")
        (root / "producer/Cargo.toml").write_text('[package]\nname="producer"\nversion="0.0.0"\nedition="2024"\n', encoding="utf-8")
        (root / "producer/src/lib.rs").write_text('pub fn value() -> std::time::Duration { std::time::Duration::ZERO }\n', encoding="utf-8")
        child = environment.copy()
        child["CARGO_HOME"] = str(root / "cargo-home")
        child["CARGO_TARGET_DIR"] = str(root / "artifacts")
        child["RUSTUP_TOOLCHAIN"] = run("rustup", "show", "active-toolchain", cwd=ROOT).split()[0]
        log = results / "core-std-dependency-rejection.log"
        with log.open("w", encoding="utf-8") as output:
            rejected = subprocess.run(["cargo", "check", "--offline", "--lib", "--target", FREESTANDING_TARGET],
                                      cwd=root, env=child, stdout=output, stderr=subprocess.STDOUT, timeout=60)
        text = log.read_text(encoding="utf-8")
        if rejected.returncode == 0 or "can't find crate for `std`" not in text:
            raise ValueError("freestanding compiler did not reject the std-bearing dependency as required; evidence: " + str(log))


def architecture_checks(target, environment, results):
    metadata = json.loads(run_logged(["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
                                     environment, results / "architecture-metadata.log"))
    require_application_test_targets(metadata)
    require_gui_unit_target(metadata)
    library = ["cargo", "test", "-p", "auditory-app", "--lib", "--locked", "--target", target]
    listing = run_logged([*library, "--", "--list"], environment, results / "app-library-discovery.log")
    discovered = architecture_test_names(listing, APP_CAPACITY_CONTROLS)
    for name in sorted(APP_CAPACITY_CONTROLS):
        evidence = run_logged([*library, name, "--", "--exact", "--test-threads=1"], environment,
                              results / (name.rsplit("::", 1)[-1] + "-execution.log"))
        require_architecture_execution(evidence, {name}, filtered=len(discovered) - 1)
    for name, required in essential_app_tests(target).items():
        command = ["cargo", "test", "-p", "auditory-app", "--test", name, "--locked", "--target", target]
        listing = run_logged([*command, "--", "--list"], environment, results / (name + "-discovery.log"))
        discovered = architecture_test_names(listing, required)
        evidence = run_logged([*command, "--", "--test-threads=1"], environment, results / (name + "-execution.log"))
        require_architecture_execution(evidence, discovered)
    command = ["cargo", "test", "-p", "auditory-gui", "--bin", "auditorylab-gui", "--locked", "--target", target]
    listing = run_logged([*command, "--", "--list"], environment, results / "gui-unit-discovery.log")
    discovered = architecture_test_names(listing, gui_unit_controls(target))
    evidence = run_logged([*command, "--", "--test-threads=1"], environment, results / "gui-unit-execution.log")
    require_architecture_execution(evidence, discovered)


def qualify(target, build_kind, source_revision, output, repository):
    validate_build_request(build_kind, source_revision)
    entry = next((item for item in TARGETS if item["target"] == target), None)
    if entry is None:
        raise ValueError(f"unsupported native qualification target: {target}")
    # Validate ambient producer/configuration inputs before any tool query.
    environment = controlled_environment("gui", entry, build_kind, source_revision)
    host = next(line[6:] for line in run("rustc", "-vV").splitlines() if line.startswith("host: "))
    if host != target:
        raise ValueError(f"native qualification requires host {target}, found {host}")
    if build_kind != "local":
        check_source(source_revision)
    identities = cargo_identities()
    check_gui_profile(target)
    # The same native test path is used by Quality, candidates and releases.
    results = ROOT / "target/qualification-results" / build_kind / target
    temporary = ROOT / "target/qualification-temp"
    temporary.mkdir(parents=True, exist_ok=True)
    environment.update({name: str(temporary) for name in ("TMPDIR", "TEMP", "TMP")})
    environment["PYTHONDONTWRITEBYTECODE"] = "1"
    for label, command in (
        ("format", ["cargo", "fmt", "--all", "--", "--check"]),
        ("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--target", target]),
        ("documentation", ["cargo", "doc", "--workspace", "--no-deps", "--locked", "--target", target]),
        ("python", [sys.executable, "-m", "unittest", "discover", "-s", "tests", "-v"]),
        ("notices", [sys.executable, str(ROOT / "scripts/notices.py"), "--check"]),
    ):
        run_logged(command, environment, results / (label + ".log"))
    freestanding_core_check(environment, results)
    architecture_checks(target, environment, results)
    for profile in ([], ["--release"]):
        run_logged(["cargo", "test", "--workspace", "--locked", "--target", target, *profile],
                   environment, results / ("workspace-release.log" if profile else "workspace-debug.log"))
    environment["AUDITORYLAB_BENCH_SOURCE"] = source_revision or "local/unqualified"
    for crate, name in (("auditory-core", "hour_long_accumulation"),
                        ("auditory-core", "the_longest_builtin_stream"),
                        ("auditory-device", "warmed_production_callback_render_and_conversion_budget"),
                        ("auditory-device", "observed_production_callback_budget"),
                        ("auditory-app", "maximum_rate_observation_and_snapshot_budget")):
        evidence = run_logged(["cargo", "test", "-p", crate, "--release", "--lib", "--locked", "--target", target,
                               name, "--", "--ignored", "--nocapture"], environment, results / (name + ".log"))
        if "test result: ok. 1 passed; 0 failed; 0 ignored;" not in evidence:
            raise ValueError("offline qualification test was not actually discovered/executed: " + name)
        validate_offline_records(name, evidence)
    native_window_checks(target, environment, results, identities["gui"]["id"])
    native_signal_geometry_checks(target, environment, results, identities["gui"]["id"])
    if "apple" in target:
        termination_environment = environment.copy()
        termination_environment["AUDITORY_GUI_TERMINATION_TEST"] = "1"
        termination = run_logged(["cargo", "test", "-p", PRODUCTS["gui"]["package"], "--release", "--test", "native_termination",
                                  "--locked", "--target", target], termination_environment, results / "native-termination.log")
        if any("native_termination " + mode + ":" not in termination for mode in ("idle", "blocked", "repeated", "dialog")):
            raise ValueError("native macOS termination fixture did not execute every lifetime case")
    if "linux" in target:
        run_logged(["pkg-config", "--modversion", "freetype2", "fontconfig"], environment,
                   results / "system-font-pkg-config.log")
        run_logged(["dpkg-query", "--show", "--showformat=${binary:Package}\t${Version}\n",
                    "libfreetype6", "libfontconfig1", "libfreetype6-dev", "libfontconfig1-dev",
                    "libxkbcommon0", "libxkbcommon-x11-0"],
                   environment, results / "system-font-distro.log")
    archives = []
    for product, identity in identities.items():
        if build_kind != "local":
            check_source(source_revision)
        environment = controlled_environment(product, entry, build_kind, source_revision)
        environment.update({name: str(temporary) for name in ("TMPDIR", "TEMP", "TMP")})
        environment["PYTHONDONTWRITEBYTECODE"] = "1"
        receipt = None
        if "windows" in target:
            receipt = ROOT / "target/runtime-link" / build_kind / product / (target + ".json")
            run_logged([sys.executable, str(ROOT / "scripts/windows_runtime.py"), "build",
                        "--target", target, "--product", product, "--trace", str(receipt)],
                       environment, results / (product + "-build.log"))
        else:
            run_logged(["cargo", "build", "--release", "--locked", "--target", target,
                        "-p", identity["name"], "--bin", identity["bin"]],
                       environment, results / (product + "-build.log"))
        binary = Path(environment["CARGO_TARGET_DIR"]) / target / "release" / (
            identity["bin"] + (".exe" if "windows" in target else ""))
        archives.append(package(target, identity["version"], repository, binary, output,
                                receipt, product, build_kind, source_revision))
        if build_kind != "local":
            check_source(source_revision)
    (results / "packages.json").write_text(json.dumps([
        {"file": archive.name, "product": product, "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
         "build_kind": build_kind, "source_revision": source_revision, "target": target}
        for product, archive in zip(identities, archives)], indent=2) + "\n", encoding="utf-8")
    return archives


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--build-kind", choices=("local", "candidate", "release"), default="local")
    parser.add_argument("--source-revision")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repository", default="resoltico/AuditoryLab")
    args = parser.parse_args()
    try:
        for archive in qualify(args.target, args.build_kind, args.source_revision, args.output, args.repository):
            print(archive)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(f"qualification: {error}")


if __name__ == "__main__":
    main()
