#!/usr/bin/env python3
"""Verify binding text modes against a pre-005 CLI without touching old archives."""

import argparse
import copy
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile


def run(binary, arguments):
    return subprocess.check_output([str(binary), *map(str, arguments)])


def normalize(data, evidence):
    """Only existing runtime identity and presentation-path differences."""
    def text(value):
        return re.sub(r"ifds-[0-9a-f]{16}", "ifds-<runtime-dependent-id>", value.replace(str(evidence), "full.json"))

    def value(item):
        if isinstance(item, dict):
            return {key: value(child) for key, child in item.items() if key != "elapsed_ms"}
        if isinstance(item, list):
            return [value(child) for child in item]
        if isinstance(item, str):
            return text(item)
        return item

    return value(json.loads(data))


def text_stats(data, evidence):
    text = data.decode().replace(str(evidence), "full.json")
    return {"bytes": len(text.encode()), "lines": len(text.splitlines()), "text": text}


def report_id(full):
    identity = copy.deepcopy(full)
    identity["human_summary"] = ""
    canonical = json.dumps(identity, ensure_ascii=False, separators=(",", ":")).encode()
    result = 0xCBF29CE484222325
    for byte in canonical:
        result = ((result ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"ifds-{result:016x}"


def validate_references(compact, full, before, after):
    identity = report_id(full)
    assert compact["full_report_id"] == identity
    assert compact["snapshots"] == full["snapshots"]
    sections = {
        "before_graph.nodes": full["before_graph"]["nodes"],
        "after_graph.nodes": full["after_graph"]["nodes"],
        "before_graph.edges": full["before_graph"]["edges"],
        "after_graph.edges": full["after_graph"]["edges"],
        "alignment": full["alignment"],
        "deltas": full["deltas"],
    }
    count = 0

    def visit(value):
        nonlocal count
        if isinstance(value, list):
            for child in value:
                visit(child)
        elif isinstance(value, dict):
            if set(value) == {"report_id", "section", "index"}:
                assert value["report_id"] == identity
                assert 0 <= value["index"] < len(sections[value["section"]])
                count += 1
            for child in value.values():
                visit(child)

    visit(compact)
    for definition in compact["sources"] + compact["controls"] + compact["observations"]:
        for side, source in [("before", before), ("after", after)]:
            site = definition[side]
            if site is None:
                continue
            assert site["node"]["snapshot"]["side"] == side
            span = site["span"]
            encoded = source.encode()
            assert 0 <= span["byte_start"] <= span["byte_end"] <= len(encoded)
            assert span["start_line"] == encoded[:span["byte_start"]].count(b"\n") + 1
            reference = site["evidence"]
            if reference["section"].endswith(".nodes"):
                node = sections[reference["section"]][reference["index"]]
                assert node["id"] == site["node"] and node["span"] == span
            else:
                assert reference["section"] == "alignment"
                assert full["alignment"][reference["index"]][side] == site["node"]
    return count


def source_pairs(root):
    pairs = {}
    for name in ["overwrite", "copy_then_overwrite", "full_downstream_chain"]:
        fixture = root / "tests/ifds/fixtures" / name
        pairs[name] = ((fixture / "before/main.ts").read_text(), (fixture / "after/main.ts").read_text())
    pairs.update({
        "guard": (
            "function f(flag: boolean) {\n  let x = 1;\n  if (flag) x = 2;\n  return x;\n}\n",
            "function f(flag: boolean) {\n  let x = 1;\n  if (!flag) x = 2;\n  return x;\n}\n"),
        "priority": (
            "function f(a: boolean, b: boolean) {\n  let x = 1;\n  if (a) x = 2;\n  if (b) x = 3;\n  return x;\n}\n",
            "function f(a: boolean, b: boolean) {\n  let x = 1;\n  if (b) x = 3;\n  if (a) x = 2;\n  return x;\n}\n"),
        "early_return": (
            "function f(flag: boolean) {\n  let x = 0;\n  x = 1;\n  x = 2;\n  x = 3;\n  return x + 4;\n}\n",
            "function f(flag: boolean) {\n  let x = 0;\n  x = 1;\n  x = 2;\n  if (flag) return x;\n  x = 3;\n  return x + 4;\n}\n"),
        "unknown": (
            "function f() {\n  let x = 1;\n  return x;\n}\n",
            "function f() {\n  let x = opaque();\n  return x;\n}\n"),
        "unchanged": (
            "function f() {\n  let x = 1;\n  return x;\n}\n",
            "function f() {\n  let x = 1;\n  return x;\n}\n"),
        "copy_chain": (
            "function f() {\n  let x = 1;\n  const saved = x;\n  const y = saved + 1;\n  return y * 2;\n}\n",
            "function f() {\n  let x = 2;\n  const saved = x;\n  const y = saved + 1;\n  return y * 2;\n}\n"),
        "nested_guard": (
            "function f(a: boolean, b: boolean) {\n  let x = 1;\n  if (a) {\n    if (b) x = 2;\n    else x = 3;\n  }\n  return x;\n}\n",
            "function f(a: boolean, b: boolean) {\n  let x = 1;\n  if (a) {\n    if (!b) x = 2;\n    else x = 3;\n  }\n  return x;\n}\n"),
        "guard_versions": (
            "function f(flag: boolean) {\n  let x = 1;\n  if (flag) x = 2;\n  flag = false;\n  if (flag) x = 3;\n  return x;\n}\n",
            "function f(flag: boolean) {\n  let x = 1;\n  if (flag) x = 2;\n  flag = true;\n  if (flag) x = 3;\n  return x;\n}\n"),
        "moved_positions": (
            "function f(flag: boolean) {\n  let x = 1;\n  if (flag) x = 2;\n  return x;\n}\n",
            "function f(flag: boolean) {\n  let x = 1;\n\n  if (!flag) x = 2;\n\n  return x;\n}\n"),
        "overwritten_edit": (
            "function f() {\n  let x = 1;\n  x = 9;\n  return x;\n}\n",
            "function f() {\n  let x = 2;\n  x = 9;\n  return x;\n}\n"),
        "independent_expressions": (
            "function f() {\n  let x = 1;\n  const y = x + 1;\n  return y;\n}\n",
            "function f() {\n  let x = 2;\n  const y = x + 2;\n  return y;\n}\n"),
        "operand_expression": (
            "function f() {\n  let x = 1;\n  return x + 1;\n}\n",
            "function f() {\n  let x = 1;\n  return x + 2;\n}\n"),
        "new_copy": (
            "function f() {\n  let x = 1;\n}\n",
            "function f() {\n  let x = 1;\n  const saved = x;\n}\n"),
    })
    return pairs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--baseline-commit", default="374919a6429de2d3741335cb5ce21c4b3fcf4011")
    parser.add_argument("--current", type=Path, default=Path("target/debug/examples/ifds_compare"))
    parser.add_argument("--output", type=Path, default=Path(__file__).with_name("005-verification.json"))
    parser.add_argument("--reports-dir", type=Path, help="New empty directory for persistent binding reports")
    options = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    baseline, current = options.baseline.resolve(), options.current.resolve()
    results, functions, invalid = [], [], []
    with tempfile.TemporaryDirectory(prefix="review-005-") as temporary:
        temporary = Path(temporary)
        reports = options.reports_dir if options.reports_dir else temporary / "binding-reports"
        if reports.exists() and any(reports.iterdir()):
            parser.error("--reports-dir must be empty; archived reports are never overwritten")
        reports.mkdir(parents=True, exist_ok=True)
        for name, (before, after) in sorted(source_pairs(root).items()):
            directory = reports / name
            directory.mkdir()
            (directory / "before.ts").write_text(before)
            (directory / "after.ts").write_text(after)
            args = [directory / "before.ts", directory / "after.ts", "x"]
            old_path, short_path = directory / "legacy-full.json", directory / "full.json"
            verbose_path, compact_path = directory / "verbose-full.json", directory / "compact-full.json"
            old = run(baseline, [*args, "--evidence-out", old_path])
            short = run(current, [*args, "--evidence-out", short_path])
            verbose = run(current, [*args, "--verbose", "--evidence-out", verbose_path])
            compact = run(current, [*args, "--format", "compact-json", "--evidence-out", compact_path])
            old_compact_path = temporary / f"{name}-old-compact-full.json"
            old_compact = run(baseline, [*args, "--format", "compact-json", "--evidence-out", old_compact_path])
            assert normalize(old_compact, old_compact_path) == normalize(compact, compact_path), name
            old_full = run(baseline, [*args, "--format", "full-json"])
            full = run(current, [*args, "--format", "full-json"])
            assert normalize(old_full, "<unused>") == normalize(full, "<unused>"), name
            assert normalize(old_path.read_bytes(), old_path) == normalize(short_path.read_bytes(), short_path), name
            assert normalize(short_path.read_bytes(), short_path) == normalize(verbose_path.read_bytes(), verbose_path), name
            assert normalize(short_path.read_bytes(), short_path) == normalize(compact_path.read_bytes(), compact_path), name
            for mode, path in [(short, short_path), (verbose, verbose_path)]:
                evidence = json.loads(path.read_bytes())
                assert f"Evidence: {path} ({report_id(evidence)})" in mode.decode(), name
            restored_compact = json.loads(compact)
            references = validate_references(restored_compact, json.loads(compact_path.read_bytes()), before, after)
            for mode, expected in [([], short), (["--verbose"], verbose)]:
                path = temporary / f"{name}-repeat-{len(mode)}.json"
                repeated = run(current, [*args, *mode, "--format", "text", "--evidence-out", path])
                expected_path = short_path if not mode else verbose_path
                norm = lambda text, evidence: re.sub(r"ifds-[0-9a-f]{16}", "ifds-<runtime-dependent-id>", text.decode().replace(str(evidence), "full.json"))
                assert norm(repeated, path) == norm(expected, expected_path), name
            for filename, data in [("legacy.txt", old), ("short.txt", short), ("verbose.txt", verbose), ("compact.json", compact)]:
                (directory / filename).write_bytes(data)
            results.append({
                "case": name,
                "query": {"binding": "x", "entry": "containing_function", "limits": json.loads(short_path.read_bytes())["query"]["request"]["limits"]},
                "before_source": before, "after_source": after,
                "full_json_parity": True, "compact_json_parity": True,
                "legacy_summary_parity": True, "text_mode_sidecar_parity": True,
                "text_deterministic_except_existing_runtime_identity": True,
                "valid_reference_count": references,
                "short_report_id": report_id(json.loads(short_path.read_bytes())),
                "verbose_report_id": report_id(json.loads(verbose_path.read_bytes())),
                "legacy": text_stats(old, old_path), "short": text_stats(short, short_path),
                "verbose": text_stats(verbose, verbose_path),
            })

        # Function 004 output must remain byte-identical at a shared sidecar path.
        for case in sorted((root / "spec/function-result-review/reports").iterdir()):
            if not (case / "before.ts").exists():
                continue
            evidence = temporary / f"function-{case.name}.json"
            args = [case / "before.ts", case / "after.ts", "--function", "result", "--evidence-out", evidence]
            for mode in [[], ["--verbose"], ["--format", "compact-json"], ["--format", "full-json"]]:
                assert run(baseline, [*args, *mode]) == run(current, [*args, *mode]), (case.name, mode)
            functions.append({"case": case.name, "both_text_modes_and_json_byte_identical": True})

        for target in [["x"], ["--function", "result"]]:
            for format_name in ["compact-json", "full-json"]:
                evidence = temporary / "invalid-full.json"
                failure = subprocess.run([str(current), str(temporary / "missing-before.ts"), str(temporary / "missing-after.ts"), *target,
                                          "--verbose", "--format", format_name, "--evidence-out", str(evidence)], capture_output=True)
                assert failure.returncode != 0
                assert b"--verbose requires --format text" in failure.stderr
                assert not failure.stdout and not evidence.exists()
                invalid.append({"target": target[0], "format": format_name, "rejected_before_source_io": True})
        conflict = temporary / "conflicting-evidence.json"
        conflict.write_bytes(b"existing evidence must survive")
        directory = reports / "overwrite"
        failure = subprocess.run([str(current), str(directory / "before.ts"), str(directory / "after.ts"), "x", "--verbose", "--evidence-out", str(conflict)], capture_output=True)
        assert failure.returncode != 0 and not failure.stdout
        assert b"already contains a different report" in failure.stderr
        assert conflict.read_bytes() == b"existing evidence must survive"

    totals = {mode: {metric: sum(case[mode][metric] for case in results) for metric in ["bytes", "lines"]}
              for mode in ["legacy", "short", "verbose"]}
    document = {
        "task": "005: concise binding human reports", "baseline_commit": options.baseline_commit,
        "normalization": "Only stats.elapsed_ms, its derived binding IDs, and evidence paths normalize for CLI parity. Legacy human_summary wording and every other structured value must match. Fixed-report determinism and byte parity are additionally tested in Rust.",
        "measurement_scope": f"{len(results)} synthetic binding queries; no fixed size cap; not a project performance estimate",
        "totals": totals, "binding_cases": results, "function_controls": functions,
        "invalid_verbose_options": invalid, "existing_sidecar_preserved": True,
    }
    options.output.write_text(json.dumps(document, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps(totals, indent=2))
    print(f"Verified {len(results)} binding cases, {len(functions)} function controls, and {len(invalid)} invalid combinations.")


if __name__ == "__main__":
    main()
