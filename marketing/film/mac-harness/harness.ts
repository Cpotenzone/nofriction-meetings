// Film harness entry: set the stage, install the mocked Tauri backend, then
// load the real app (src/main.tsx). Order matters: the app must see the
// mocks (window.__TAURI_INTERNALS__) from its first import.
//
// URL parameters:
//   ?clock=10:14   the page's local time of day (default 10:14, a weekday morning)
//   ?film=1        slow-motion support for filming (time.ts)
//   ?cursor=1      keep the mouse cursor and scrollbars (for poking around)
//   ?setup=1       first run: show the setup screen
//   ?pro=0         not subscribed: AI actions open the paywall (mock/backend.ts)
//   ?ai=none       no AI set up yet (mock/backend.ts)

// 1. The clock first (imports run in order): the demo data reads it
import { setSlow } from "./time";
import { harnessApi, installMocks } from "./mock/backend";

const params = new URLSearchParams(location.search);

// 2. First run is done (no setup wizard); remembered UI choices start fresh
try {
    localStorage.clear();
    if (params.get("setup") !== "1") localStorage.setItem("nofriction_setup_complete", "true");
} catch {
    /* storage unavailable */
}

if (params.get("cursor") !== "1") document.documentElement.classList.add("harness-no-cursor");

// 3. The backend, and the controls capture.mjs uses (window.__harness)
installMocks();
(window as any).__harness = Object.assign(harnessApi, { setSlow });

// 4. The app
await import("../../../src/main.tsx");
