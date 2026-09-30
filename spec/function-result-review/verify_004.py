#!/usr/bin/env python3
"""Compare the pre-004 and current CLI on archived sources without editing archives."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile


def run(binary, arguments):
    return subprocess.check_output([str(binary), *map(str, arguments)])


def digest(data):
    return hashlib.sha256(data).hexdigest()


def text_stats(data, evidence):
    text = data.decode().replace(str(evidence), "full.json")
    return {"bytes": len(text.encode()), "lines": len(text.splitlines()), "text": text}


def normalize_binding(data, format_name, evidence):
    # Existing binding IDs hash runtime stats, unlike function-result IDs.
    # Compare the remaining fields while explicitly retaining this limitation.
    def identity_text(text):
        return re.sub(r"ifds-[0-9a-f]{16}", "ifds-<runtime-dependent-id>", text.replace(str(evidence), "full.json"))

    def normalize(value):
        if isinstance(value, list):
            return [normalize(item) for item in value]
        if isinstance(value, dict):
            return {
                key: ("<runtime-dependent-id>" if key in ["report_id", "full_report_id"]
                      else identity_text(item) if key in ["human_summary", "evidence_file"]
                      else normalize(item))
                for key, item in value.items() if key != "elapsed_ms"
            }
        return value

    return identity_text(data.decode()) if format_name == "text" else normalize(json.loads(data))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--baseline-commit", default="c012ef8")
    parser.add_argument("--current", type=Path, default=Path("target/debug/examples/ifds_compare"))
    parser.add_argument("--output", type=Path, default=Path(__file__).with_name("004-verification.json"))
    options = parser.parse_args()
    baseline = options.baseline.resolve()
    current = options.current.resolve()
    root = Path(__file__).resolve().parents[2]
    results = []
    binding_results = []
    with tempfile.TemporaryDirectory(prefix="review-004-") as temporary:
        temporary = Path(temporary)
        for case in sorted((root / "spec/function-result-review/reports").iterdir()):
            if not (case / "before.ts").is_file():
                continue
            evidence = temporary / (case.name + "-full.json")
            args = [case / "before.ts", case / "after.ts", "--function", "result", "--evidence-out", evidence]
            before_text = run(baseline, args)
            before_full = run(baseline, [*args, "--format", "full-json"])
            before_compact = run(baseline, [*args, "--format", "compact-json"])
            short = run(current, args)
            verbose = run(current, [*args, "--verbose"])
            full = run(current, [*args, "--format", "full-json"])
            compact = run(current, [*args, "--format", "compact-json"])
            assert full == before_full, case.name
            assert compact == before_compact, case.name
            assert verbose == before_text, case.name
            assert full.rstrip(b"\n") == evidence.read_bytes(), case.name
            assert short == run(current, [*args, "--format", "text"]), case.name
            assert verbose == run(current, [*args, "--format", "text", "--verbose"]), case.name
            normalized = json.loads(compact)
            normalized["evidence_file"] = "full.json"
            results.append({
                "case": case.name,
                "full_report_id": normalized["full_report_id"],
                "full_json_identical": True,
                "compact_json_identical": True,
                "verbose_matches_baseline": True,
                "sidecar_matches_full_stdout_except_final_newline": True,
                "text_modes_deterministic": True,
                "full_stdout_sha256": digest(full),
                "compact_normalized_sha256": digest(json.dumps(normalized, sort_keys=True, separators=(",", ":")).encode()),
                "before": text_stats(before_text, evidence),
                "short": text_stats(short, evidence),
                "verbose": text_stats(verbose, evidence),
            })

        for name in ["overwrite", "copy_then_overwrite", "full_downstream_chain"]:
            fixture = root / "tests/ifds/fixtures" / name
            for format_name in ["text", "compact-json", "full-json"]:
                outputs = []
                for label, binary in [("baseline", baseline), ("current", current)]:
                    evidence = temporary / f"{name}-binding-{format_name}-{label}.json"
                    args = [fixture / "before/main.ts", fixture / "after/main.ts", "x", "--evidence-out", evidence, "--format", format_name]
                    outputs.append(normalize_binding(run(binary, args), format_name, evidence))
                assert outputs[0] == outputs[1], (name, format_name)
            binding_results.append({"case": name, "all_three_formats_equivalent_except_runtime_stats_and_derived_ids": True})

        invalid_options = []
        for target in [["--function", "result"], ["x"]]:
            for format_name in ["text", "compact-json", "full-json"]:
                if target[0] == "--function" and format_name == "text":
                    continue
                evidence = temporary / "invalid-full.json"
                args = [str(temporary / "missing-before.ts"), str(temporary / "missing-after.ts"), *target,
                        "--format", format_name, "--verbose", "--evidence-out", str(evidence)]
                failure = subprocess.run([str(current), *args], capture_output=True)
                assert failure.returncode != 0
                assert b"--verbose requires a function target and --format text" in failure.stderr
                assert not failure.stdout and not evidence.exists()
                invalid_options.append({"target": target[0], "format": format_name, "rejected_before_file_io": True})

    totals = {
        mode: {metric: sum(case[mode][metric] for case in results) for metric in ["bytes", "lines"]}
        for mode in ["before", "short", "verbose"]
    }
    document = {
        "task": "004: concise function human reports",
        "baseline_commit": options.baseline_commit,
        "normalization": "Text evidence paths and compact evidence_file normalize to full.json only for portable measurements/hashes. Parity checks compare actual bytes with the same fresh sidecar path.",
        "measurement_scope": "13 synthetic archived source pairs; current query defaults; not a project performance estimate.",
        "binding_normalization": "Existing binding stats.elapsed_ms and its derived report IDs vary between executions. Controls normalize only elapsed_ms, report/full_report IDs, and their occurrences in human_summary/evidence_file, plus temporary sidecar paths.",
        "totals": totals,
        "cases": results,
        "binding_controls": binding_results,
        "invalid_verbose_options": invalid_options,
    }
    options.output.write_text(json.dumps(document, indent=2) + "\n")
    print(json.dumps(totals, indent=2))
    print(f"Verified {len(results)} function cases, {len(binding_results)} binding controls, and {len(invalid_options)} invalid option combinations.")


if __name__ == "__main__":
    main()
