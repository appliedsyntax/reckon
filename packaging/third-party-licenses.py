#!/usr/bin/env python3
"""Writes THIRD_PARTY_LICENSES.txt: the license notices for every crate that
is compiled into the shipped Reckon binaries.

    python3 packaging/third-party-licenses.py

Run it again whenever Cargo.lock changes. It uses only `cargo metadata` and
the crate sources cargo has already downloaded (standard library only, no
extra tools).

What counts as "shipped": crates reachable through normal (not build or dev)
dependencies from reckon-app and reckon-cli, on macOS, Windows and Linux.
Procedural macros run at compile time and are not in the binaries, so they
and their compile-time dependencies are left out unless something shipped
also uses them.
"""

import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "THIRD_PARTY_LICENSES.txt"
ROOTS = ["reckon-app", "reckon-cli"]
TARGETS = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
]
LICENSE_FILE = re.compile(r"^(LICEN[CS]E|COPYING|COPYRIGHT|NOTICE|UNLICENSE)", re.I)
# For "A OR B" crates, the license we take, in order of preference.
PREFERRED = ["MIT", "Apache-2.0", "BSD-3-Clause", "BSD-2-Clause", "Zlib", "ISC",
             "Unicode-3.0", "BSL-1.0", "0BSD", "CC0-1.0", "Unlicense", "MPL-2.0"]
# File-name fragment that identifies each license's file (LICENSE-MIT, ...).
FILE_KEYS = {"Apache-2.0": "APACHE", "BSD-3-Clause": "BSD", "BSD-2-Clause": "BSD",
             "Unicode-3.0": "UNICODE", "BSL-1.0": "BOOST", "CC0-1.0": "CC0"}


def metadata(target):
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked",
         "--filter-platform", target],
        cwd=ROOT, check=True, capture_output=True, text=True).stdout
    return json.loads(out)


def shipped(meta):
    """Package ids reachable from ROOTS through normal dependencies."""
    pkgs = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    roots = [p["id"] for p in meta["packages"] if p["name"] in ROOTS and p["source"] is None]
    seen, stack = set(), list(roots)
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        if any("proc-macro" in t["kind"] for t in pkgs[pid]["targets"]):
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    return {pid: pkgs[pid] for pid in seen if pkgs[pid]["source"] is not None}


def chosen(expr):
    """For an SPDX expression with OR, the alternative we use."""
    if not expr:
        return None
    alts = [a.strip(" ()") for a in re.split(r"\s+OR\s+|/", expr)]
    if len(alts) == 1 or " AND " in expr:
        return None
    for p in PREFERRED:
        if p in alts:
            return p
    return alts[0]


def license_files(pkg):
    root = Path(pkg["manifest_path"]).parent
    files = []
    for f in sorted(os.listdir(root)):
        path = root / f
        if LICENSE_FILE.match(f) and path.is_file():
            files.append(path)
    if pkg.get("license_file"):
        extra = root / pkg["license_file"]
        if extra.is_file() and extra not in files:
            files.append(extra)
    return files


def standard_text(spdx):
    sysroot = subprocess.run(["rustc", "--print", "sysroot"], capture_output=True,
                             text=True, check=True).stdout.strip()
    path = Path(sysroot) / "share/doc/rust/licenses" / f"{spdx}.txt"
    if not path.is_file():
        return None
    # The template's "Copyright (c) <year> <copyright holders>" line is filled
    # in by the caller, next to the crate name.
    text = re.sub(r"^Copyright \(c\) <year> <copyright holders>\n+", "", path.read_text(errors="replace"),
                  flags=re.M)
    return text.strip()


def main():
    crates = {}
    for t in TARGETS:
        for pid, pkg in shipped(metadata(t)).items():
            crates[(pkg["name"], pkg["version"])] = pkg
    if not crates:
        sys.exit("no crates found")

    groups = {}      # normalized notice text -> (text, [crate labels])
    missing = 0
    index = []
    for (name, version), pkg in sorted(crates.items()):
        expr = pkg.get("license") or "(see license file)"
        pick = chosen(pkg.get("license"))
        index.append(f"  {name} {version}  {expr}" + (f"  [used under {pick}]" if pick else ""))
        label = f"{name} {version}"
        files = license_files(pkg)
        if pick:
            # Only the license we use needs its notice; NOTICE files always go.
            key = FILE_KEYS.get(pick, pick).upper()
            match = [f for f in files if key in f.name.upper() or f.name.upper().startswith("NOTICE")]
            if any(not f.name.upper().startswith("NOTICE") for f in match):
                files = match
        texts = [f.read_text(errors="replace").strip() for f in files]
        if not texts:
            # No license file in the published crate: use the standard text
            # with the authors from its manifest as the copyright holders.
            missing += 1
            spdx = pick or expr
            authors = ", ".join(pkg.get("authors") or []) or f"the {name} authors"
            label += f" (no license file; Copyright (c) {authors})"
            texts = [standard_text(spdx) or f"Licensed under {expr}."]
        for text in texts:
            norm = " ".join(text.split())
            groups.setdefault(norm, (text, []))[1].append(label)

    lines = [
        "Third-party software in Reckon",
        "==============================",
        "",
        "Reckon itself is released under the MIT License (see LICENSE).",
        "",
        "The desktop app (reckon-app) includes the open-source components",
        "listed below. Each is used under the license shown; where a crate",
        "offers a choice (\"A OR B\"), the license in brackets is the one",
        "Reckon uses. Their notices follow the list.",
        "",
        "The command-line tool (reckon) and the iOS and Android apps contain",
        "only Reckon's own code, the Rust standard library and the operating",
        "system's frameworks.",
        "",
        "Generated by packaging/third-party-licenses.py from Cargo.lock.",
        "",
        f"Components ({len(crates)} crates, plus the Rust standard library)",
        "-" * 60,
        "  Rust standard library  MIT OR Apache-2.0  [used under MIT]",
        *index,
        "",
    ]

    std_mit = standard_text("MIT")
    lines += ["", "=" * 72, "Rust standard library", "=" * 72, "",
              "Copyright (c) The Rust Project Contributors", "",
              std_mit or "Licensed under the MIT license.", ""]

    for text, names in sorted(groups.values(), key=lambda g: g[1][0]):
        lines += ["", "=" * 72, *wrap_names(names), "=" * 72, "", text, ""]

    OUT.write_text("\n".join(lines).rstrip() + "\n")
    print(f"{OUT.relative_to(ROOT)}: {len(crates)} crates, {len(groups)} distinct notices, "
          f"{missing} without a license file (standard text used)", file=sys.stderr)


def wrap_names(names, width=72):
    out, line = [], ""
    for n in names:
        piece = n if not line else ", " + n
        if len(line) + len(piece) > width:
            out.append(line + ",")
            line = n
        else:
            line += piece
    out.append(line)
    return out


if __name__ == "__main__":
    main()
