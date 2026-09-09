import { clsx } from "clsx";
import "./plugin-runtime-smoke.mjs";
if (clsx("node", { bundled: true }) !== "node bundled") throw new Error("bundled npm dependency failed");
