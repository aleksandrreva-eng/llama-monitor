# dev/ — manual verification scaffolding

None of this is part of the product build. It is the tooling that was used to
chase two problems that unit tests cannot reach: whether the widget survives a
window resize, and whether the packaged MSI actually re-registers itself.

Kept here (rather than in the repository root, where it used to sit) so the
root contains only product files. The paths inside these scripts are absolute
and machine-specific — adjust them before use.

| File | What it does |
|---|---|
| `run_resize_harness.py` | Mounts the **real built bundle** in headless Chrome with a stubbed Tauri layer (`verify_resize/index.html`) and fires `tauri://resize` events. Exit code 1 means the frontend threw. Run from the repository root: `python dev/run_resize_harness.py` |
| `check_resize.py` / `check_resize.ps1` | Launch the widget, resize its window a few times, then dump the log to a report file |
| `launch_check.cmd` / `launch_check_release.cmd` | Start the installed / freshly built exe, wait for the WebView2 frontend to mount, then print the log |
| `install_verify.ps1` | Uninstall + reinstall the MSI and report whether the installed exe was actually replaced (needs an elevated shell) |

The automated checks that *do* run in CI live in `../scripts/`.
