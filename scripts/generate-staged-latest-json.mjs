#!/usr/bin/env node
/**
 * Build staged-latest.json for the background install/update switcher.
 *
 * Windows uses a raw staged zip that can be extracted while the app keeps
 * running. macOS reuses Tauri's signed .app.tar.gz updater artifact.
 */
import { writeFileSync } from "node:fs";

const repo = process.env.GITHUB_REPOSITORY;
const tag = process.env.RELEASE_TAG;
const token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN;
const notes = process.env.RELEASE_NOTES ?? "";

if (!repo || !tag) {
  throw new Error("GITHUB_REPOSITORY and RELEASE_TAG are required");
}
if (!token) {
  throw new Error("GH_TOKEN or GITHUB_TOKEN is required");
}

const apiHeaders = {
  Authorization: `Bearer ${token}`,
  Accept: "application/vnd.github+json",
  "User-Agent": "Tempo-Staged-Updater",
  "X-GitHub-Api-Version": "2022-11-28",
};

async function api(path) {
  const res = await fetch(`https://api.github.com/${path}`, { headers: apiHeaders });
  if (!res.ok) {
    throw new Error(`GitHub API ${path} failed: ${res.status} ${await res.text()}`);
  }
  return res.json();
}

async function readSig(assetId) {
  const res = await fetch(`https://api.github.com/repos/${repo}/releases/assets/${assetId}`, {
    headers: {
      ...apiHeaders,
      Accept: "application/octet-stream",
    },
    redirect: "follow",
  });
  if (!res.ok) {
    throw new Error(`Failed to download signature asset ${assetId}: ${res.status}`);
  }
  return (await res.text()).trim();
}

function pickBySuffix(assets, suffix) {
  return assets.find((asset) => asset.name.endsWith(suffix)) ?? null;
}

async function platformEntry(bundleAsset, sigAsset) {
  if (!bundleAsset || !sigAsset) return null;
  return {
    signature: await readSig(sigAsset.id),
    url: bundleAsset.browser_download_url,
  };
}

const release = await api(`repos/${repo}/releases/tags/${tag}`);
const assets = release.assets ?? [];
const byName = Object.fromEntries(assets.map((asset) => [asset.name, asset]));
const platforms = {};

const winZip = pickBySuffix(assets, "_x64-staged.zip");
const winZipSig = winZip ? byName[`${winZip.name}.sig`] : null;
const winEntry = await platformEntry(winZip, winZipSig);
if (winEntry) {
  platforms["windows-x86_64-staged"] = winEntry;
}

const macArm = pickBySuffix(assets, "_aarch64.app.tar.gz");
const macArmSig = macArm ? byName[`${macArm.name}.sig`] : null;
const macX64 = pickBySuffix(assets, "_x64.app.tar.gz");
const macX64Sig = macX64 ? byName[`${macX64.name}.sig`] : null;

const armEntry = await platformEntry(macArm, macArmSig);
const x64Entry = await platformEntry(macX64, macX64Sig);

if (armEntry) {
  platforms["darwin-aarch64"] = armEntry;
}
if (x64Entry) {
  platforms["darwin-x86_64"] = x64Entry;
}

if (Object.keys(platforms).length === 0) {
  throw new Error("No staged updater platforms found on release assets");
}

const version = String(release.tag_name || tag).replace(/^v/, "");
const latest = {
  version,
  notes: notes || release.body || `Tempo ${release.tag_name}`,
  pub_date: new Date().toISOString(),
  platforms,
};

writeFileSync("staged-latest.json", `${JSON.stringify(latest, null, 2)}\n`);
console.log(JSON.stringify(latest.platforms, null, 2));
