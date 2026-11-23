// CDP probes against the app's WebView2 (Playwright `connectOverCDP`, phase 1.4): grid fps and cold start.
import { chromium } from "playwright";

export const GRID = '[role="grid"]';
export const GRID_IMG = '[role="grid"] [role="gridcell"] img';

const sleep = (ms) => new Promise((res) => setTimeout(res, ms));

/** Connects and returns the app page (the one loaded from the embedded assets). `close()` detaches, the app keeps running. */
export async function connectApp(cdpUrl, { urlPrefix = "http://tauri.localhost", timeoutMs = 30_000 } = {}) {
  const browser = await chromium.connectOverCDP(cdpUrl);
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    const page = browser.contexts().flatMap((c) => c.pages()).find((p) => p.url().startsWith(urlPrefix));
    if (page) return { browser, page, close: () => browser.close() };
    await sleep(100);
  }
  await browser.close();
  throw new Error(`no page under ${urlPrefix} after ${timeoutMs} ms`);
}

/** Waits until the grid shows at least one loaded thumbnail; returns the grid's item count (aria-rowcount). */
export async function waitForGrid(page, timeoutMs = 30_000) {
  await page.waitForFunction(
    (sel) => [...document.querySelectorAll(sel)].some((i) => i.complete && i.naturalWidth > 0),
    GRID_IMG,
    { timeout: timeoutMs, polling: 50 },
  );
  return page.$eval(GRID, (g) => Number(g.getAttribute("aria-rowcount")));
}

/**
 * Epoch ms of the first grid thumbnail paint, read from the page's own buffered timing, so it holds even
 * when CDP attached after the paint. Uses the earliest `largest-contentful-paint` entry whose element is a
 * grid thumbnail (renderTime = paint), else the earliest grid thumbnail's resource `responseEnd` (just
 * before paint). Returns { epochMs, method }.
 */
export async function firstGridPaint(page) {
  return page.evaluate(async (sel) => {
    const lcp = await new Promise((resolve) => {
      const entries = [];
      const po = new PerformanceObserver((list) => entries.push(...list.getEntries()));
      try {
        po.observe({ type: "largest-contentful-paint", buffered: true });
      } catch {
        resolve([]);
        return;
      }
      setTimeout(() => {
        po.disconnect();
        resolve(entries);
      }, 100);
    });
    const imgs = new Set(document.querySelectorAll(sel));
    const lcpHit = lcp.filter((e) => e.element && imgs.has(e.element)).map((e) => e.renderTime || e.startTime);
    if (lcpHit.length) return { epochMs: performance.timeOrigin + Math.min(...lcpHit), method: "lcp" };
    const res = [...imgs]
      .map((i) => performance.getEntriesByName(i.currentSrc)[0]?.responseEnd)
      .filter((t) => typeof t === "number" && t > 0);
    if (res.length) return { epochMs: performance.timeOrigin + Math.min(...res), method: "resource" };
    return { epochMs: null, method: "none" };
  }, GRID_IMG);
}

/**
 * Scrolls the grid linearly from top to bottom over `durationMs`, one scrollTop step per animation frame,
 * and returns the frame-to-frame times (ms) measured with requestAnimationFrame timestamps.
 */
export async function scrollFrames(page, durationMs = 10_000) {
  return page.evaluate(
    ({ sel, durationMs }) =>
      new Promise((resolve, reject) => {
        const el = document.querySelector(sel);
        if (!el) return reject(new Error(`no ${sel}`));
        el.scrollTop = 0;
        const max = el.scrollHeight - el.clientHeight;
        if (max <= 0) return reject(new Error("grid does not scroll"));
        const frames = [];
        let start = null;
        let prev = null;
        const tick = (ts) => {
          if (start === null) start = ts;
          else frames.push(ts - prev);
          prev = ts;
          const p = Math.min(1, (ts - start) / durationMs);
          el.scrollTop = p * max;
          if (p < 1) requestAnimationFrame(tick);
          else resolve({ frames, scrolledPx: el.scrollTop, maxPx: max });
        };
        // two frames first so the jump to the top is not counted
        requestAnimationFrame(() => requestAnimationFrame(tick));
      }),
    { sel: GRID, durationMs },
  );
}

/** Percentile (nearest-rank) of a numeric array. */
export function percentile(values, p) {
  if (!values.length) return NaN;
  const s = [...values].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.max(0, Math.ceil((p / 100) * s.length) - 1))];
}
