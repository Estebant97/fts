#!/usr/bin/env python3
"""Build and manage the per-user macOS Dock app and launch agent."""

import argparse
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
LABEL = "dev.fts.switcher"
APP_NAME = "Fast True Switcher.app"
DOMAIN = f"gui/{os.getuid()}"
SERVICE = f"{DOMAIN}/{LABEL}"
INSTALLED_APP = Path.home() / "Applications" / APP_NAME
AGENT = Path.home() / "Library/LaunchAgents" / f"{LABEL}.plist"
LOGS = Path.home() / "Library/Logs/Fast True Switcher"


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def built_app():
    metadata = run(
        "cargo", "metadata", "--no-deps", "--format-version=1", "--locked",
        cwd=ROOT, capture_output=True, text=True,
    )
    return Path(json.loads(metadata.stdout)["target_directory"]) / "release" / APP_NAME


def build():
    run("cargo", "build", "--release", "--locked", "--bin", "fts", cwd=ROOT)
    app = built_app()
    executable_dir = app / "Contents/MacOS"
    executable_dir.mkdir(parents=True, exist_ok=True)
    # Replacing the inode also permits rebuilding while the old app is running.
    temporary_binary = executable_dir / "fts.new"
    shutil.copy2(app.parent / "fts", temporary_binary)
    temporary_binary.replace(executable_dir / "fts")
    shutil.copy2(ROOT / "macos/Info.plist", app / "Contents/Info.plist")
    run("plutil", "-lint", str(app / "Contents/Info.plist"))
    # Local ad-hoc signing gives the bundle a consistent identifier. Distributed
    # builds should use a Developer ID certificate and notarization instead.
    run("codesign", "--force", "--sign", "-", "--identifier", LABEL, str(app))
    run("codesign", "--verify", "--strict", str(app))
    print(f"Built {app}", flush=True)
    return app


def loaded():
    return subprocess.run(
        ["launchctl", "print", SERVICE], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    ).returncode == 0


def bootout():
    if loaded():
        run("launchctl", "bootout", SERVICE)


def install(skip_build):
    source = built_app() if skip_build else build()
    if not (source / "Contents/MacOS/fts").is_file():
        raise RuntimeError("Build the app first: python3 scripts/macos.py build")
    run("codesign", "--verify", "--strict", str(source))
    bootout()
    INSTALLED_APP.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, INSTALLED_APP, dirs_exist_ok=True)
    AGENT.parent.mkdir(parents=True, exist_ok=True)
    LOGS.mkdir(parents=True, exist_ok=True)
    agent = {
        "Label": LABEL,
        "ProgramArguments": [str(INSTALLED_APP / "Contents/MacOS/fts"), "--background"],
        "RunAtLoad": True,
        # A normal Dock/menu Quit exits successfully and stays stopped.
        "KeepAlive": {"SuccessfulExit": False},
        "ThrottleInterval": 10,
        "LimitLoadToSessionType": "Aqua",
        "ProcessType": "Interactive",
        "StandardOutPath": str(LOGS / "fts.log"),
        "StandardErrorPath": str(LOGS / "fts-error.log"),
    }
    temporary_agent = AGENT.with_suffix(".plist.new")
    with temporary_agent.open("wb") as output:
        plistlib.dump(agent, output)
    temporary_agent.replace(AGENT)
    run("plutil", "-lint", str(AGENT))
    run("launchctl", "enable", SERVICE)
    run("launchctl", "bootstrap", DOMAIN, str(AGENT))
    print(f"Installed {INSTALLED_APP}\nStarts at login; crash recovery is enabled.")
    print(f"Logs: {LOGS}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("build", help="Build a signed .app bundle in target/release")
    install_parser = commands.add_parser("install", help="Install the app and enable launch at login")
    install_parser.add_argument("--skip-build", action="store_true", help="Install the existing release bundle")
    commands.add_parser("start", help="Start the installed app without opening a window")
    commands.add_parser("stop", help="Stop the app until the next start or login")
    commands.add_parser("status", help="Show launchd service status")
    commands.add_parser("disable-login", help="Stop the service and disable launch at login; keep the app")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("This helper requires macOS.")
    if args.command == "build":
        build()
    elif args.command == "install":
        install(args.skip_build)
    elif args.command == "start":
        if not AGENT.is_file():
            raise RuntimeError("Install the app first: python3 scripts/macos.py install")
        if not loaded():
            run("launchctl", "bootstrap", DOMAIN, str(AGENT))
        else:
            run("launchctl", "kickstart", SERVICE)
    elif args.command == "stop":
        bootout()
    elif args.command == "status":
        run("launchctl", "print", SERVICE)
    elif args.command == "disable-login":
        bootout()
        AGENT.unlink(missing_ok=True)
        print("Launch at login disabled. The app remains in ~/Applications.")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"fts: {error}", file=sys.stderr)
        sys.exit(1)
