// Phase 3.3 degraded mode (contract-sidecar-jsonrpc § Degradation) against the mock IPC: with SidecarStatus
// `absent` every AI control is disabled with a tooltip, ONE banner shows per session, and filename search still works;
// `starting` (the ~15 s after launch) is calm: no banner, "AI starting" tooltips, until `ready` enables the controls.
// Settings > AI shows the setup commands verbatim and copies exactly them.
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { SidecarStatus } from "@bindings/SidecarStatus";
import { AI_NOTICE_SETTLE_MS, DEFAULT_SIDECAR_VENV } from "../lib/defaults";
import { setupCommand } from "../lib/sidecar";
import { MOCK_SIDECAR_ABSENT, MOCK_SIDECAR_READY } from "../mocks/smart";
import { useView } from "../state/view";
import { cmds, lastCall, renderApp, reqOf, useMockApp } from "./harness";

const STARTING: SidecarStatus = { state: "starting", detail: null, capabilities: [], device: null, version: null };
const ERROR: SidecarStatus = { state: "error", detail: "sidecar exited with code 1: ImportError: torch", capabilities: [], device: null, version: null };
const AI_COMMANDS = ["start_ai", "detect_pii"];

/** Longer than the banner's settle delay, so "no banner" is not just "not yet". */
const pastSettle = () => act(() => new Promise((r) => setTimeout(r, AI_NOTICE_SETTLE_MS + 300)));
const banners = () => screen.queryAllByRole("region", { name: "AI notice" });
const modeOption = (name: RegExp) =>
  within(screen.getByRole("combobox", { name: "Search mode" })).getByRole("option", { name }) as HTMLOptionElement;

async function openFilters(): Promise<HTMLElement> {
  fireEvent.click(screen.getByRole("button", { name: /^Filters/ }));
  return screen.findByRole("region", { name: "Image filters" });
}

describe("sidecar absent", () => {
  const app = useMockApp({ language: "en" });
  const mock = () => app.get();

  it("disables semantic search, shows the banner once per session, and filename search still works", async () => {
    await renderApp();
    await waitFor(() => expect(cmds(mock())).toContain("get_sidecar_status"));
    const semantic = modeOption(/^By meaning/);
    expect(semantic.disabled).toBe(true);
    expect(semantic.textContent).toBe("By meaning (needs AI)");
    expect(semantic.title).toBe("AI features need the Python sidecar: see Settings › AI.");
    expect(modeOption(/^Text in image/).disabled).toBe(true);
    expect((screen.getByRole("combobox", { name: "Search mode" }) as HTMLSelectElement).title).toContain("Python sidecar");

    const banner = await screen.findByRole("region", { name: "AI notice" }, { timeout: AI_NOTICE_SETTLE_MS + 2000 });
    expect(banners()).toHaveLength(1);
    expect(banner.textContent).toContain("AI features need the Python sidecar.");
    expect(banner.textContent).toContain(MOCK_SIDECAR_ABSENT.detail as string);

    // Filename search does not need the sidecar.
    fireEvent.change(screen.getByRole("combobox", { name: "Search mode" }), { target: { value: "filename" } });
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "Screenshot 2026-11-2" } });
    await waitFor(() => expect(reqOf(lastCall(mock(), "query_images"))).toMatchObject({ text: "Screenshot 2026-11-2", textMode: "filename" }));
    const expected = mock().images.filter((i) => i.fileName.toLowerCase().includes("screenshot 2026-11-2")).length;
    expect(expected).toBeGreaterThan(0);
    await waitFor(() => expect(screen.getByTestId("status-total").textContent).toBe(`${expected} images`));
    expect(screen.getByRole("grid").querySelectorAll("[data-item]").length).toBeGreaterThan(0);
    expect(screen.queryByText(/Could not load images/)).toBeNull();

    // Once per session: hidden, it stays hidden through new absent/error statuses and view changes.
    fireEvent.click(within(banner).getByRole("button", { name: "Hide" }));
    expect(banners()).toHaveLength(0);
    await act(() => mock().emit("sidecar-status", ERROR));
    await act(() => mock().emit("sidecar-status", MOCK_SIDECAR_ABSENT));
    fireEvent.click(screen.getByRole("button", { name: "Timeline" }));
    fireEvent.click(screen.getByRole("button", { name: "Library" }));
    await pastSettle();
    expect(banners()).toHaveLength(0);
    AI_COMMANDS.forEach((c) => expect(cmds(mock())).not.toContain(c));
  });

  it("disables every AI filter with a tooltip and a visible marker; plain filters stay usable", async () => {
    await renderApp();
    const p = within(await openFilters());
    const hasText = p.getByRole("combobox", { name: "Has text" }) as HTMLSelectElement;
    expect(hasText.disabled).toBe(true);
    expect(hasText.title).toBe("AI features need the Python sidecar: see Settings › AI.");
    expect((p.getByRole("checkbox", { name: "Filter by colour" }) as HTMLInputElement).disabled).toBe(true);
    const categories = within(p.getByRole("group", { name: "Category" }));
    const chips = categories.getAllByRole("button") as HTMLButtonElement[];
    expect(chips.map((c) => c.textContent)).toEqual(
      ["Chat", "Code", "Game", "Browser", "Document", "Map", "Receipt", "Social", "Photo", "Settings", "Error dialog", "Meme"],
    );
    expect(chips.every((c) => c.disabled)).toBe(true);
    expect(chips[0].closest("[title]")?.getAttribute("title")).toBe("AI features need the Python sidecar: see Settings › AI.");
    expect(p.getAllByTestId("ai-marker").map((m) => m.textContent)).toEqual(["Needs AI", "Needs AI", "Needs AI"]);
    // Non-AI filters keep working.
    fireEvent.click(p.getByRole("checkbox", { name: "Favourites only" }));
    await waitFor(() => expect(reqOf(lastCall(mock(), "query_images"))).toMatchObject({ favoriteOnly: true, categories: [], hasText: null, color: null }));
  });

  it("Settings › AI shows the setup commands verbatim and Copy writes exactly that text", async () => {
    await renderApp();
    const banner = await screen.findByRole("region", { name: "AI notice" }, { timeout: AI_NOTICE_SETTLE_MS + 2000 });
    fireEvent.click(within(banner).getByRole("button", { name: "Open Settings › AI" }));
    const settings = await screen.findByRole("dialog", { name: "Settings" });
    expect(within(settings).getByRole("tab", { name: "AI" }).getAttribute("aria-selected")).toBe("true");
    const block = await within(settings).findByTestId("ai-setup-command");
    const command = setupCommand(DEFAULT_SIDECAR_VENV);
    expect(block.textContent).toBe(command);
    expect(command.split("\n")).toEqual([
      String.raw`$env:UV_PROJECT_ENVIRONMENT = "D:\IOT_Tools\data\ScreenshotStudio\sidecar-venv"`,
      String.raw`$env:HF_HOME = "D:\DevCache\huggingface"`,
      String.raw`cd "D:\Screenshot_Picture"`,
      "uv sync --project sidecar",
      String.raw`cd sidecar; & "$env:UV_PROJECT_ENVIRONMENT\Scripts\python.exe" -m ss_sidecar --prefetch`,
    ]);
    expect(within(settings).getByTestId("ai-state").textContent).toContain("Not set up");
    expect(within(settings).getByTestId("ai-detail").textContent).toBe(MOCK_SIDECAR_ABSENT.detail);
    expect((within(settings).getByRole("button", { name: "Run AI on library" }) as HTMLButtonElement).disabled).toBe(true);

    fireEvent.click(within(settings).getByRole("button", { name: "Copy commands" }));
    await waitFor(() => expect(cmds(mock())).toContain("plugin:clipboard-manager|write_text"));
    expect((lastCall(mock(), "plugin:clipboard-manager|write_text")?.payload as { text: string }).text).toBe(command);
    await screen.findByText("Copied the setup commands");
    expect(cmds(mock())).not.toContain("start_ai");
  });

  it("the setup commands follow the sidecarVenv setting", async () => {
    mock().invoke("set_setting", { key: "sidecarVenv", value: String.raw`E:\venvs\ss` });
    await renderApp();
    fireEvent.click(screen.getByTestId("ai-pill"));
    const block = await screen.findByTestId("ai-setup-command");
    await waitFor(() => expect(block.textContent).toBe(setupCommand(String.raw`E:\venvs\ss`)));
  });

  it("the duplicate finder and the dashboard work without the sidecar", async () => {
    await renderApp();
    fireEvent.click(screen.getByRole("button", { name: "Duplicates" }));
    const list = await screen.findByRole("list", { name: "Duplicates" });
    await waitFor(() => expect(list.querySelectorAll("[data-group]").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "Dashboard" }));
    await screen.findByRole("region", { name: "Screenshots per month" });
    AI_COMMANDS.forEach((c) => expect(cmds(mock())).not.toContain(c));
  });
});

describe("sidecar starting (the ~15 s after launch)", () => {
  const app = useMockApp({ language: "en" }, { sidecar: STARTING });
  const mock = () => app.get();

  it("shows a calm 'AI starting' state and no banner, then enables the AI controls when ready", async () => {
    await renderApp();
    expect(screen.getByTestId("ai-pill").textContent).toBe("AI starting…");
    const semantic = modeOption(/^By meaning/);
    expect(semantic.disabled).toBe(true);
    expect(semantic.textContent).toBe("By meaning (AI starting…)");
    expect(semantic.title).toBe("AI is starting (about 15 s after the app opens).");
    const p = within(await openFilters());
    expect(p.getAllByTestId("ai-marker").map((m) => m.textContent)).toEqual(["AI starting…", "AI starting…", "AI starting…"]);
    await pastSettle();
    expect(banners()).toHaveLength(0);
    expect(useView.getState().aiNotice).toBe("pending");

    await act(() => mock().emit("sidecar-status", MOCK_SIDECAR_READY));
    await waitFor(() => expect(modeOption(/^By meaning$/).disabled).toBe(false));
    expect(modeOption(/^Text in image$/).disabled).toBe(false);
    expect((p.getByRole("combobox", { name: "Has text" }) as HTMLSelectElement).disabled).toBe(false);
    expect(p.queryAllByTestId("ai-marker")).toHaveLength(0);
    expect(screen.getByTestId("ai-pill").textContent).toBe("AI · CUDA");
    await pastSettle();
    expect(banners()).toHaveLength(0);
  });

  it("an error after starting shows the banner with the error detail, and Settings › AI shows it too", async () => {
    await renderApp();
    await act(() => mock().emit("sidecar-status", ERROR));
    const banner = await screen.findByRole("region", { name: "AI notice" }, { timeout: AI_NOTICE_SETTLE_MS + 2000 });
    expect(banner.textContent).toContain("The AI sidecar hit an error");
    expect(banner.textContent).toContain(ERROR.detail as string);
    fireEvent.click(within(banner).getByRole("button", { name: "Open Settings › AI" }));
    const settings = await screen.findByRole("dialog", { name: "Settings" });
    await waitFor(() => expect(within(settings).getByTestId("ai-state").textContent).toContain("Error"));
    // The sidecar recovering removes the banner for good.
    await act(() => mock().emit("sidecar-status", MOCK_SIDECAR_READY));
    await waitFor(() => expect(banners()).toHaveLength(0));
    await act(() => mock().emit("sidecar-status", ERROR));
    await pastSettle();
    expect(banners()).toHaveLength(0);
  });

  it("a passing absent status shorter than the settle delay never flashes the banner", async () => {
    await renderApp();
    await act(() => mock().emit("sidecar-status", MOCK_SIDECAR_ABSENT));
    // Absent for half the settle delay (the supervisor's "not started" before launch), then starting.
    await act(() => new Promise((r) => setTimeout(r, AI_NOTICE_SETTLE_MS / 2)));
    await act(() => mock().emit("sidecar-status", STARTING));
    await pastSettle();
    expect(banners()).toHaveLength(0);
    // Not shown-then-hidden either: the once-per-session banner is still unused.
    expect(useView.getState().aiNotice).toBe("pending");
  });
});
