"""check_resize.py - launch the widget, resize its window a few times, then read the
app log and report whether the frontend survived.

Why this exists
---------------
`App.svelte` used to read `payload.size.width` in its `onResized` handler, but Tauri v2
hands the callback a `PhysicalSize` (`{width, height}`) directly. `payload.size` was
therefore `undefined` and every resize threw
`TypeError: Cannot read properties of undefined (reading 'width')`, which the global
error boundary turned into a dead widget. Startup alone does not trigger it - the
window has to be resized - so a plain "does it launch?" check misses it.

It also exercises the environment the widget actually runs in, which a browser stub
cannot: the real WebView2 and the real Tauri event plumbing.

Usage
-----
    python scripts/check_resize.py                      # release exe
    python scripts/check_resize.py --exe <path>         # any build
    python scripts/check_resize.py --report out.txt

Exit code 0 = no frontend errors, 1 = frontend errors (or the run could not start).

Notes
-----
* Window resizing needs `MoveWindow` from user32. `ctypes` is used because the sandbox
  blocks `Add-Type` (runtime .NET compilation).
* `%LOCALAPPDATA%\\llama-monitor\\llama-monitor.log` lives outside the project and
  cannot be deleted from here, so the script remembers how many lines it had and
  reports only what the run under test appended.
* WebView2 init is flaky: with stale `msedgewebview2.exe` around, the page silently
  never loads ("started", no `set_options`, no error). The script kills them and
  retries the launch once before giving up.
"""

import argparse
import ctypes
import os
import subprocess
import sys
import time
from ctypes import wintypes

DEFAULT_EXE = r"F:\AI\Monitor_Ornith\llama-monitor\src-tauri\target\release\llama-monitor.exe"
APP_LOG = os.path.join(os.environ["LOCALAPPDATA"], "llama-monitor", "llama-monitor.log")

user32 = ctypes.windll.user32
try:
    user32.SetProcessDPIAware()
except Exception:
    pass


def kill_app(include_webview=False):
    subprocess.run(["taskkill", "/IM", "llama-monitor.exe", "/F"], capture_output=True, check=False)
    # Escalation only: killing the WebView2 host can itself break the next mount
    # ("Класс не зарегистрирован"), so we leave it alone unless a prior attempt
    # failed to mount and we are retrying.
    if include_webview:
        subprocess.run(["taskkill", "/IM", "msedgewebview2.exe", "/F"], capture_output=True, check=False)
    time.sleep(1.0)


def log_lines():
    try:
        with open(APP_LOG, "r", encoding="utf-8", errors="replace") as fh:
            return fh.read().splitlines()
    except FileNotFoundError:
        return []


def find_window(pid):
    """Return (hwnd, rect) of the first visible top-level window owned by pid."""
    found = []

    @ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
    def cb(hwnd, _lparam):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(hwnd):
            rect = wintypes.RECT()
            user32.GetWindowRect(hwnd, ctypes.byref(rect))
            if rect.right - rect.left > 50 and rect.bottom - rect.top > 50:
                found.append((hwnd, rect))
                return False
        return True

    user32.EnumWindows(cb, 0)
    return found[0] if found else (None, None)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", default=DEFAULT_EXE)
    ap.add_argument("--report", default="")
    ap.add_argument("--sizes", default="420x560,360x480,500x620,320x440")
    ap.add_argument("--timeout", type=float, default=30.0)
    args = ap.parse_args()

    report_path = args.report or os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "check_resize_report.txt"
    )
    out = []

    def say(line):
        out.append(str(line))
        print(line, flush=True)

    def finish(verdict):
        out.append("==== RESULT ====")
        out.append(verdict)
        with open(report_path, "w", encoding="utf-8") as fh:
            fh.write("\n".join(out) + "\n")
        print("report: " + report_path, flush=True)

    if not os.path.exists(args.exe):
        say("EXE NOT FOUND: " + args.exe)
        finish("FAIL (exe missing)")
        return 1

    sizes = []
    for spec in args.sizes.split(","):
        w, h = spec.lower().split("x")
        sizes.append((int(w), int(h)))

    say("exe      : " + args.exe)

    hwnd = rect = None
    offset = 0
    mounted = []
    for attempt in (1, 2, 3):
        # Only escalate to killing the WebView2 host on retries; the first try
        # leaves it running so the page has the best chance to mount cleanly.
        kill_app(include_webview=(attempt > 1))
        offset = len(log_lines())
        say("--- launch attempt %d" % attempt)
        proc = subprocess.Popen(
            [args.exe],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            creationflags=getattr(subprocess, "DETACHED_PROCESS", 0),
        )
        say("pid      : %d" % proc.pid)

        deadline = time.time() + args.timeout
        while time.time() < deadline:
            if proc.poll() is not None:
                say("process exited early with code %s" % proc.returncode)
                break
            hwnd, rect = find_window(proc.pid)
            if hwnd:
                break
            time.sleep(0.5)

        if not hwnd:
            say("no visible window found within %.0fs" % args.timeout)
            for line in log_lines()[offset:]:
                say("  " + line)
            continue

        say("hwnd     : %s" % hwnd)
        say("initial  : %d,%d %dx%d" % (rect.left, rect.top,
                                        rect.right - rect.left, rect.bottom - rect.top))

        # A mounted UI issues set_options calls; their absence means WebView2 never
        # loaded the page, which is an environment flake, not a frontend crash.
        deadline = time.time() + 30.0
        while time.time() < deadline:
            mounted = [l for l in log_lines()[offset:] if "set_options" in l]
            if mounted:
                break
            time.sleep(0.5)

        if mounted:
            say("mounted  : yes (%d set_options lines)" % len(mounted))
            break

        say("mounted  : NO - frontend never issued set_options")
        for line in log_lines()[offset:]:
            say("  " + line)

    if not hwnd or not mounted:
        kill_app()
        finish("FAIL (frontend did not mount - WebView2 flake, retry)")
        return 1

    x, y = rect.left, rect.top
    for w, h in sizes:
        user32.MoveWindow(hwnd, x, y, w, h, True)
        say("resize   : %dx%d" % (w, h))
        time.sleep(0.8)
    time.sleep(3)

    new = log_lines()[offset:]
    say("==== APP LOG (new lines) ====")
    for line in new:
        say("  " + line)
    if not new:
        say("  (nothing appended)")

    errors = [l for l in new if "frontend error" in l]
    say("frontend errors: %d" % len(errors))

    kill_app()

    if errors:
        finish("FAIL: %d frontend error line(s) after resize" % len(errors))
        return 1
    finish("PASS: 0 frontend errors after %d resizes" % len(sizes))
    return 0


if __name__ == "__main__":
    sys.exit(main())
