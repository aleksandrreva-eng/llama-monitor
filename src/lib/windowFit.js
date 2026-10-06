import { tick } from "svelte";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { pushLog } from "../store";

/** How long to wait for the content to settle before re-measuring. */
const FIT_DEBOUNCE_MS = 150;

/** Height changes smaller than this are treated as "nothing changed". */
const HEIGHT_EPSILON = 2;

/**
 * Keeps the OS window sized to the widget's natural content height.
 *
 * The widget is `height: 100%` of the window with `overflow: hidden`, so its
 * `scrollHeight` just echoes the window height — not the content height. The
 * element's fixed height is therefore temporarily released (`-> auto`) so it
 * grows to its natural size, measured, and restored **before** the async IPC so
 * the flip stays inside one frame. Do not reorder those three steps.
 *
 * @param {() => HTMLElement | null} getElement reads the bound widget element.
 */
export function createWindowFitter(getElement) {
  // Last content height actually applied to the window. Lets us skip redundant
  // resizes AND leave a manual height resize alone until the content changes.
  let lastHeight = 0;
  let timer = null;

  /** Measure the content and resize the window if it actually changed. */
  async function fit() {
    const el = getElement();
    if (!el) return;
    try {
      const win = getCurrentWindow();
      await tick();
      const prevHeight = el.style.height;
      el.style.height = "auto";
      await tick();
      const height = Math.round(el.scrollHeight);
      el.style.height = prevHeight; // "" -> CSS height:100%
      if (Math.abs(height - lastHeight) < HEIGHT_EPSILON) return;
      lastHeight = height;
      const scale = await win.scaleFactor();
      const current = await win.innerSize(); // physical pixels
      const logicalWidth = current.width / scale;
      await win.setSize(new LogicalSize(logicalWidth, height));
    } catch (err) {
      // A failed resize must never take the widget down; the next content
      // change will try again.
      pushLog("fit to content failed: " + err);
    }
  }

  /**
   * Debounced `fit()`. The content height is not constant: in compact mode the
   * speed block grows the moment telemetry arrives (the tok/s unit and the 30 s
   * average appear, and a "split unavailable" note may too), and the
   * diagnostics block changes length. Fitting only on mount/toggle would leave
   * the bottom clipped until the user toggled.
   */
  function schedule() {
    if (timer) clearTimeout(timer);
    timer = setTimeout(fit, FIT_DEBOUNCE_MS);
  }

  /** Forget the last applied height, forcing the next `fit()` to resize. */
  function reset() {
    lastHeight = 0;
  }

  return { fit, schedule, reset };
}
