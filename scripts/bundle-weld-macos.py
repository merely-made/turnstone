#!/usr/bin/env python3
# Copyright 2026 Mark Alan Boykin
# SPDX-License-Identifier: MPL-2.0

"""Assemble a local Turnstone app from the two already-built Weld targets."""
from argparse import ArgumentParser
from pathlib import Path
import plistlib
import shutil
import subprocess


FRAMEWORK = "Chromium Embedded Framework.framework"
SUFFIXES = ("", " (GPU)", " (Renderer)", " (Plugin)", " (Alerts)")


def bundle_app(app, executable, name, identifier, helper=False):
    contents = app / "Contents"
    macos = contents / "MacOS"
    macos.mkdir(parents=True, exist_ok=False)
    shutil.copy2(executable, macos / name)
    values = {
        "CFBundleExecutable": name,
        "CFBundleIdentifier": identifier,
        "CFBundleName": name if helper else "Turnstone",
        "CFBundlePackageType": "APPL",
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleDevelopmentRegion": "en",
        "NSHighResolutionCapable": True,
    }
    if helper:
        values["LSUIElement"] = True
    (contents / "Info.plist").write_bytes(plistlib.dumps(values))
    return contents


def main():
    parser = ArgumentParser(description=__doc__)
    parser.add_argument("binaries", type=Path, help="directory with turnstone and turnstone-weld-helper")
    parser.add_argument("cef", type=Path, help="directory containing the matching CEF framework")
    parser.add_argument("output", type=Path, help="new path ending in .app")
    args = parser.parse_args()
    binary = args.binaries / "turnstone"
    helper = args.binaries / "turnstone-weld-helper"
    framework = args.cef / FRAMEWORK
    if not all(path.is_file() for path in (binary, helper)) or not framework.is_dir():
        parser.error("both Weld binaries and the CEF framework must exist")
    if args.output.suffix != ".app" or args.output.exists():
        parser.error("the output must be a new .app path")

    identifier = "made.merely.turnstone"
    contents = bundle_app(args.output, binary, "turnstone", identifier)
    frameworks = contents / "Frameworks"
    frameworks.mkdir()
    bundled_framework = frameworks / FRAMEWORK
    subprocess.run(["ditto", framework, bundled_framework], check=True)
    for library in bundled_framework.rglob("*.dylib"):
        if not library.is_symlink():
            subprocess.run(["codesign", "--force", "--sign", "-", library], check=True)
    subprocess.run(["codesign", "--force", "--sign", "-", bundled_framework], check=True)
    for suffix in SUFFIXES:
        name = f"turnstone Helper{suffix}"
        app = frameworks / f"{name}.app"
        qualifier = suffix.strip(" ()").lower()
        helper_id = identifier + ".helper" + (f".{qualifier}" if qualifier else "")
        bundle_app(app, helper, name, helper_id, helper=True)
        subprocess.run(["codesign", "--force", "--sign", "-", app], check=True)
    subprocess.run(["codesign", "--force", "--sign", "-", args.output], check=True)


if __name__ == "__main__":
    main()
