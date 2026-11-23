#!/usr/bin/env node
// Renders the hand-authored app icon source to the 1024 px PNG that `cargo tauri icon` consumes (phase 2.3).
//
//   node src-tauri/icons/source/render.mjs            -> src-tauri/icons/source/icon-1024.png
//   cargo tauri icon src-tauri/icons/source/icon-1024.png   (run from the repo root; writes src-tauri/icons/*)
//
// Rendered with Playwright Chromium (already a dev dependency for the Accept probes): one 1024x1024 page, the SVG
// inline, transparent background. Prints `PASS|FAIL render <path> <bytes>` and exits 0 only on success.
// Env `SS_CHROMIUM=<chrome.exe>` picks another Chromium build (e.g. an older `ms-playwright\chromium-*` revision when
// the one this Playwright version wants is not installed); unset = Playwright's own browser.
import { readFileSync, statSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const SIZE = 1024;
const here = path.dirname(fileURLToPath(import.meta.url));
const src = path.join(here, "icon.svg");
const out = path.join(here, "icon-1024.png");

async function main() {
  const svg = readFileSync(src, "utf8");
  const browser = await chromium.launch({ executablePath: process.env.SS_CHROMIUM || undefined });
  try {
    const page = await browser.newPage({ viewport: { width: SIZE, height: SIZE }, deviceScaleFactor: 1 });
    await page.setContent(
      `<!doctype html><html><head><style>html,body{margin:0;background:transparent}svg{display:block}</style></head><body>${svg}</body></html>`,
    );
    await page.locator("svg").screenshot({ path: out, omitBackground: true });
  } finally {
    await browser.close();
  }
  const bytes = statSync(out).size;
  console.log(`PASS render ${path.relative(process.cwd(), out)} ${bytes}`);
}

main().catch((e) => {
  console.log(`FAIL render ${out} ${e.message.split("\n")[0]}`);
  process.exitCode = 1;
});
