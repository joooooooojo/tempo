import { execFile } from "node:child_process";
import { cp, mkdir, rm } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const run = promisify(execFile);
const require = createRequire(import.meta.url);
const sdkRoot = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const repositoryRoot = path.resolve(sdkRoot, "..");
const distRoot = path.join(sdkRoot, "dist");
const assetRoot = path.join(distRoot, "dev-assets");
const tsc = require.resolve("typescript/bin/tsc");

await rm(distRoot, { recursive: true, force: true });
await run(process.execPath, [tsc, "-p", path.join(sdkRoot, "tsconfig.json")], {
  cwd: sdkRoot,
});

await mkdir(assetRoot, { recursive: true });
for (const fileName of ["structured-clone.js", "bridge-client.js"]) {
  await cp(
    path.join(repositoryRoot, "core", "plugin-ui", fileName),
    path.join(assetRoot, fileName),
  );
}
