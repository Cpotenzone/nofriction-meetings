// Vite config for the film harness: the Mac app's real React UI (src/) with
// a mocked Tauri backend (mock/) and fictional demo data. Run from the repo
// root (it reuses the root node_modules):
//
//   npx vite --config marketing/film/mac-harness/vite.config.mjs
//
// HARNESS_PORT picks the port (default 5193).
import { createRequire } from "node:module";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, "../../..");
// The repo's own plugin, not one from marketing/film/node_modules (Remotion)
const require = createRequire(path.join(repo, "package.json"));
const reactPluginModule = require("@vitejs/plugin-react");
const react = reactPluginModule.default ?? reactPluginModule;

const framesDir = path.join(here, "frames");

/** Serves the rendered slide images (make-frames.mjs) at /frames/. */
function harnessFrames() {
    return {
        name: "harness-frames",
        configureServer(server) {
            server.middlewares.use("/frames", (req, res, next) => {
                const name = decodeURIComponent((req.url ?? "").split("?")[0]).replace(/^\/+/, "");
                const file = path.join(framesDir, name);
                if (!file.startsWith(framesDir + path.sep) || !fs.existsSync(file)) return next();
                res.setHeader("Content-Type", "image/jpeg");
                res.setHeader("Cache-Control", "max-age=3600");
                fs.createReadStream(file).pipe(res);
            });
        },
    };
}

export default {
    root: here,
    // The app's own public/ (trinacria.svg for the navbar logo)
    publicDir: path.join(repo, "public"),
    cacheDir: path.join(here, "node_modules", ".vite"),
    // Imports resolve from the importing file, so the app (src/) gets the
    // root node_modules. The harness's own files import only
    // @tauri-apps/api, which marketing/film/node_modules (Remotion) doesn't
    // have; they must never import React (that folder has its own copy).
    plugins: [react(), harnessFrames()],
    clearScreen: false,
    server: {
        port: Number(process.env.HARNESS_PORT || 5193),
        strictPort: true,
        host: "127.0.0.1",
        hmr: false,
        fs: { allow: [repo] },
        watch: { ignored: ["**/src-tauri/**", "**/frames/**", "**/out/**"] },
    },
};
