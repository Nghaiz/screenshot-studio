// Shared setup for the UI tests that render the whole app against the mock IPC (jsdom has no layout, so every
// element gets a viewport-sized box for the virtualizer). Not a test file: vitest only collects *.test.tsx.
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeAll, beforeEach, expect } from "vitest";
import { installMockIpc, type MockCall, type MockHandle } from "../mocks/ipc";
import { createQueryClient, Root } from "../Root";
import { DEFAULT_PREFS, usePrefs, type Prefs } from "../state/prefs";
import { useView } from "../state/view";

export const W = 1200;
export const H = 800;
/** The two commands no keyboard or palette path may ever send (design-intent § Safety). */
export const PURGE_COMMANDS = ["request_permanent_delete", "delete_permanently"];

const initialView = useView.getState();

/** Installs the mock before each test; `get()` returns the current handle. */
export function useMockApp(prefs: Partial<Prefs> = {}): { get: () => MockHandle } {
  let mock: MockHandle;
  beforeAll(() => {
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => W });
    Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => H });
    Element.prototype.scrollTo ??= function scrollTo() {};
    window.matchMedia ??= ((q: string) => ({
      matches: false, media: q, onchange: null, addListener() {}, removeListener() {},
      addEventListener() {}, removeEventListener() {}, dispatchEvent: () => false,
    })) as unknown as typeof window.matchMedia;
  });
  beforeEach(() => {
    localStorage.clear();
    usePrefs.setState({ ...DEFAULT_PREFS, motion: "off", animatedBg: false, ...prefs });
    useView.setState({ ...initialView, selection: new Set(), jobs: {}, toasts: [], dialog: null }, true);
    mock = installMockIpc();
  });
  afterEach(() => {
    cleanup();
    mock.uninstall();
  });
  return { get: () => mock };
}

/** Renders the app and waits until the grid shows thumbnails. */
export async function renderApp(): Promise<HTMLElement> {
  render(<Root client={createQueryClient()} />);
  const grid = await screen.findByRole("grid");
  await waitFor(() => expect(grid.querySelectorAll("img").length).toBeGreaterThan(0));
  return grid;
}

export const cmds = (mock: MockHandle) => mock.calls.map((c) => c.cmd);
export const callsOf = (mock: MockHandle, cmd: string): MockCall[] => mock.calls.filter((c) => c.cmd === cmd);
export const lastCall = (mock: MockHandle, cmd: string): MockCall | undefined => callsOf(mock, cmd).at(-1);
export const reqOf = (c: MockCall | undefined) => (c?.payload as { req?: unknown } | undefined)?.req;
export const cell = (grid: HTMLElement, index: number) => grid.querySelector(`[data-index="${index}"]`) as HTMLElement;
