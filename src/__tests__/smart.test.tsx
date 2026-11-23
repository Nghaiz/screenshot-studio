// Phase 3.3 smart features against the mock IPC: duplicate finder (best first, "trash others" = trash_images with
// the non-best ids), cleanup wizard (reversible actions only: never request_permanent_delete), sessions in the
// timeline, the stats dashboard, sidebar sub-folders from get_stats.byFolder, and "Run AI on library".
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { DuplicateGroup } from "@bindings/DuplicateGroup";
import type { FindDuplicatesReq } from "@bindings/FindDuplicatesReq";
import type { GetSessionsReq } from "@bindings/GetSessionsReq";
import { DAY_MS, DUP_HAMMING_DEFAULT, OLD_TRASH_DAYS } from "../lib/defaults";
import { MOCK_SIDECAR_READY } from "../mocks/smart";
import { duplicateCopies } from "../views/smart/CleanupWizard";
import { othersOf } from "../views/smart/DuplicatesView";
import { oldestFirst } from "../views/smart/StatsDashboard";
import { callsOf, cmds, lastCall, PURGE_COMMANDS, renderApp, reqOf, useMockApp } from "./harness";

const app = useMockApp({ language: "en" });
const mock = () => app.get();
const sorted = (ids: readonly number[]) => [...ids].sort((a, b) => a - b);
const noPurgeSent = () => PURGE_COMMANDS.forEach((c) => expect(cmds(mock())).not.toContain(c));
const exactGroups = () => mock().invoke("find_duplicates", { mode: "exact", hamming: null, rootId: null }).groups;

async function openDuplicates(): Promise<HTMLElement> {
  await renderApp();
  fireEvent.click(screen.getByRole("button", { name: "Duplicates" }));
  const list = await screen.findByRole("list", { name: "Duplicates" });
  await waitFor(() => expect(list.querySelectorAll("[data-group]").length).toBeGreaterThan(0));
  return list;
}

const groupEl = (list: HTMLElement, g: DuplicateGroup) => list.querySelector(`[data-group="${g.groupId}"]`) as HTMLElement;
const memberIds = (el: HTMLElement) => [...el.querySelectorAll("[data-image-id]")].map((m) => Number(m.getAttribute("data-image-id")));

describe("duplicate finder", () => {
  it("renders each group with its best image first, marked as best", async () => {
    const groups = exactGroups();
    // A group whose best image is NOT first in the backend's order, among the rows the virtualizer renders.
    const g = groups.slice(0, 3).find((x) => x.bestId !== x.imageIds[0]);
    expect(g, "the mock plants an early group whose best image is not listed first").toBeDefined();
    const list = await openDuplicates();
    const el = groupEl(list, g as DuplicateGroup);
    expect(memberIds(el)).toEqual([g!.bestId, ...g!.imageIds.filter((id) => id !== g!.bestId)]);
    const first = el.querySelector("[data-image-id]") as HTMLElement;
    expect(first.getAttribute("data-best")).toBe("true");
    expect(within(first).getByTestId("dup-best").textContent).toMatch(/^★ Best \((largest|newest|sharpest)\)$/);
    expect(el.querySelectorAll('[data-best="true"]').length).toBe(1);
    expect(screen.getByTestId("dup-summary").textContent).toBe(
      `${groups.length} groups · ${groups.reduce((a, x) => a + x.imageIds.length - 1, 0)} extra copies`,
    );
    expect(reqOf(lastCall(mock(), "find_duplicates"))).toEqual({ mode: "exact", hamming: null, rootId: null });
  });

  it('"select every extra copy" + trash calls trash_images with exactly the non-best ids, then reads the groups again', async () => {
    const groups = exactGroups();
    const others = groups.flatMap(othersOf);
    const list = await openDuplicates();
    const before = callsOf(mock(), "find_duplicates").length;
    fireEvent.click(screen.getByRole("button", { name: "Select every extra copy" }));
    fireEvent.click(screen.getByRole("button", { name: `Move ${others.length} selected to the trash` }));
    await waitFor(() => expect(cmds(mock())).toContain("trash_images"));
    const ids = (reqOf(lastCall(mock(), "trash_images")) as { imageIds: number[] }).imageIds;
    expect(sorted(ids)).toEqual(sorted(others));
    for (const g of groups) expect(ids).not.toContain(g.bestId);
    expect(cmds(mock())).not.toContain("recycle_images");
    noPurgeSent();
    // The trashed copies leave every group, so the finder has nothing left to show.
    await waitFor(() => expect(callsOf(mock(), "find_duplicates").length).toBeGreaterThan(before));
    await screen.findByText("No duplicates found.");
    expect(list.isConnected).toBe(false);
    await screen.findByText(`Moved ${others.length} images to the trash`);
  });

  it('a group\'s "keep the best" trashes only that group\'s other images', async () => {
    const g = exactGroups()[0];
    const list = await openDuplicates();
    const el = groupEl(list, g);
    fireEvent.click(within(el).getByRole("button", { name: `Keep the best, trash the other ${g.imageIds.length - 1}` }));
    await waitFor(() => expect(cmds(mock())).toContain("trash_images"));
    expect(sorted((reqOf(lastCall(mock(), "trash_images")) as { imageIds: number[] }).imageIds)).toEqual(sorted(othersOf(g)));
    noPurgeSent();
  });

  it("similar mode sends the slider's Hamming distance", async () => {
    await openDuplicates();
    fireEvent.click(screen.getByRole("button", { name: "Similar" }));
    await waitFor(() => expect(reqOf(lastCall(mock(), "find_duplicates"))).toEqual({ mode: "similar", hamming: DUP_HAMMING_DEFAULT, rootId: null }));
    fireEvent.change(screen.getByRole("slider", { name: `Allowed difference: ${DUP_HAMMING_DEFAULT}` }), { target: { value: "2" } });
    await waitFor(() => expect(reqOf(lastCall(mock(), "find_duplicates"))).toEqual({ mode: "similar", hamming: 2, rootId: null } satisfies FindDuplicatesReq));
  });
});

describe("cleanup wizard", () => {
  it("only ever sends trash_images and recycle_images, never permanent delete, with every step included", async () => {
    // 5 images in the in-app trash for longer than the cutoff, 3 trashed just now.
    const oldIds = mock().images.slice(20, 25).map((i) => i.id);
    const recentIds = mock().images.slice(30, 33).map((i) => i.id);
    mock().invoke("trash_images", { imageIds: [...oldIds, ...recentIds] });
    const old = Date.now() - (OLD_TRASH_DAYS + 5) * DAY_MS;
    for (const i of mock().images) if (oldIds.includes(i.id)) i.trashedAt = old;
    const exact = mock().invoke("find_duplicates", { mode: "exact", hamming: null, rootId: null });
    const near = mock().invoke("find_duplicates", { mode: "similar", hamming: DUP_HAMMING_DEFAULT, rootId: null });
    const issues = mock().invoke("detect_issues", { rootId: null });
    const expectTrash = sorted([...new Set([...duplicateCopies(exact, near), ...issues.blurry, ...issues.tiny])]);
    expect(issues.blurry.length * issues.tiny.length).toBeGreaterThan(0);

    await renderApp();
    fireEvent.click(screen.getByRole("button", { name: "Cleanup" }));
    const wizard = await screen.findByRole("region", { name: "Cleanup" });
    const w = within(wizard);
    fireEvent.click(w.getByRole("button", { name: "Start" }));
    // Step 1, duplicates (included by default): add the similar ones too.
    await w.findByRole("heading", { name: "Exact copies" });
    fireEvent.click(w.getByRole("checkbox", { name: `Also similar images (difference ${DUP_HAMMING_DEFAULT})` }));
    await waitFor(() => expect(w.getByTestId("clean-count").textContent).toBe(`${duplicateCopies(exact, near).length} images`));
    expect((w.getByRole("checkbox", { name: /Include in the plan/ }) as HTMLInputElement).checked).toBe(true);
    for (const [title, count] of [["Blurry images", issues.blurry.length], ["Tiny images", issues.tiny.length], ["Old in-app trash", oldIds.length]] as const) {
      fireEvent.click(w.getByRole("button", { name: "Next" }));
      await w.findByRole("heading", { name: title });
      await waitFor(() => expect(w.getByTestId("clean-count").textContent).toBe(`${count} images`));
      const include = w.getByRole("checkbox", { name: /Include in the plan/ }) as HTMLInputElement;
      expect(include.checked, title).toBe(false); // only exact duplicates are proposed by default
      fireEvent.click(include);
    }
    fireEvent.click(w.getByRole("button", { name: "Next" }));
    await w.findByRole("heading", { name: "Review the plan" });
    expect(w.getByTestId("clean-review").textContent).toContain(`${expectTrash.length} images to the in-app trash`);
    expect(w.getByTestId("clean-review").textContent).toContain(`${oldIds.length} images to the Windows Recycle Bin`);
    expect(wizard.textContent).toContain("Nothing is deleted permanently.");
    noPurgeSent(); // reviewing sends nothing
    expect(cmds(mock()).filter((c) => c === "trash_images" || c === "recycle_images")).toEqual([]);

    fireEvent.click(w.getByRole("button", { name: "Apply" }));
    await w.findByText("Cleanup done. Undo with Ctrl+Z or the History panel.");
    expect(sorted((reqOf(lastCall(mock(), "trash_images")) as { imageIds: number[] }).imageIds)).toEqual(expectTrash);
    expect(sorted((reqOf(lastCall(mock(), "recycle_images")) as { imageIds: number[] }).imageIds)).toEqual(sorted(oldIds));
    // The recently trashed images were not proposed for the Recycle Bin.
    for (const id of recentIds) expect((reqOf(lastCall(mock(), "recycle_images")) as { imageIds: number[] }).imageIds).not.toContain(id);
    noPurgeSent();
    expect(screen.queryByRole("alertdialog", { name: /Permanently delete/ })).toBeNull();
  });

  it("never proposes the best image of any group, even when similar groups overlap exact ones", () => {
    const exact = { groups: [{ groupId: 1, imageIds: [1, 2], bestId: 2, reason: "largest" as const }] };
    const near = { groups: [{ groupId: 1, imageIds: [1, 2, 3], bestId: 1, reason: "newest" as const }] };
    expect(sorted(duplicateCopies(exact, near))).toEqual([3]);
  });
});

describe("sessions, dashboard, sidebar folders", () => {
  it("the timeline groups by capture session from get_sessions (same query, setting gap)", async () => {
    await renderApp();
    fireEvent.click(screen.getByRole("button", { name: "Timeline" }));
    await waitFor(() => expect(screen.getAllByTestId("timeline-header").length).toBeGreaterThan(0));
    expect(cmds(mock())).not.toContain("get_sessions");
    fireEvent.click(screen.getByRole("button", { name: "Session" }));
    await waitFor(() => expect(cmds(mock())).toContain("get_sessions"));
    const req = reqOf(lastCall(mock(), "get_sessions")) as GetSessionsReq;
    expect(req.gapMinutes).toBeNull();
    expect(req.query).toMatchObject({ trash: "exclude", sort: { field: "taken", dir: "desc" }, cursor: null });
    const first = mock().invoke("get_sessions", req).sessions[0];
    const headers = await screen.findAllByTestId("timeline-header");
    const day = new Date(first.startMs).toLocaleDateString("en", { weekday: "short", day: "numeric", month: "long", year: "numeric" });
    await waitFor(() => expect(screen.getAllByTestId("timeline-header")[0].textContent).toContain(day));
    expect(headers[0].textContent).toContain(String(first.imageIds.length));
  });

  it("the dashboard draws get_stats aggregates (tiles, SVG month columns, bar lists, table view)", async () => {
    const stats = mock().invoke("get_stats", null);
    await renderApp();
    fireEvent.click(screen.getByRole("button", { name: "Dashboard" }));
    const dash = await screen.findByRole("region", { name: "Dashboard" });
    expect(within(dash).getByTestId("tile-Images").textContent).toBe(stats.total.toLocaleString("en"));
    expect(within(dash).getByTestId("tile-Exact copies").textContent).toBe(String(stats.duplicatesExact));
    const months = within(dash).getByRole("region", { name: "Screenshots per month" });
    const monthKeys = [...months.querySelectorAll("svg .ss-bar")].map((g) => g.getAttribute("data-key"));
    expect(monthKeys).toEqual(stats.byMonth.map((m) => m.key).sort());
    // Oldest first whatever order get_stats used (the contract does not fix it).
    expect(oldestFirst([...stats.byMonth].reverse()).map((m) => m.key)).toEqual(monthKeys);
    const folders = within(dash).getByRole("region", { name: "By folder" });
    const folderCounts = [...folders.querySelectorAll("li[data-key]")].map((li) => stats.byFolder.find((f) => f.key === li.getAttribute("data-key"))?.count ?? -1);
    expect(folderCounts).toEqual(stats.byFolder.map((f) => f.count).sort((a, b) => b - a));
    for (const f of stats.byFolder) expect(folders.querySelector(`[data-key="${f.key}"]`)?.textContent).toContain(f.count.toLocaleString("en"));
    fireEvent.click(within(folders).getByRole("button", { name: "Show as a table" }));
    expect(within(folders).getByRole("table")).toBeTruthy();
    expect(within(dash).getByRole("region", { name: "By category" }).textContent).toContain("run AI on the library");
    expect(cmds(mock()).filter((c) => c === "get_stats").length).toBeGreaterThan(0);
  });

  it("the sidebar sub-folders render from get_stats.byFolder, and a failed read shows its reason with a retry", async () => {
    const byFolder = mock().invoke("get_stats", null).byFolder;
    expect(byFolder.map((f) => f.key).sort()).toEqual(["", "1ERD", "4ERD"]);
    await renderApp();
    const section = screen.getByRole("heading", { name: "Folders" }).closest("section") as HTMLElement;
    for (const f of byFolder) {
      const name = f.key || "(root)";
      await waitFor(() => expect(within(section).getByRole("button", { name: new RegExp(`^${name.replace(/[()]/g, "\\$&")}`) }).textContent).toBe(`${name}${f.count}`));
    }
    expect(section.textContent).not.toContain("not available");
    fireEvent.click(within(section).getByRole("button", { name: /^1ERD/ }));
    await waitFor(() => expect(reqOf(lastCall(mock(), "query_images"))).toMatchObject({ folders: ["1ERD"] }));
  });

  it("a failed get_stats shows the reason in the folder list; Retry reads it again", async () => {
    mock().fail("get_stats", { kind: "db", message: "database is locked", detail: null });
    await renderApp();
    const section = screen.getByRole("heading", { name: "Folders" }).closest("section") as HTMLElement;
    await within(section).findByText("Folder list not available yet: database is locked");
    mock().fail("get_stats", null);
    fireEvent.click(within(section).getByRole("button", { name: "Retry" }));
    await within(section).findByRole("button", { name: /^4ERD/ });
    expect(section.textContent).not.toContain("not available");
  });
});

describe("Run AI on library", () => {
  it("sends start_ai with every task the ready sidecar can run and follows job-progress to the end", async () => {
    await renderApp();
    await act(() => mock().emit("sidecar-status", MOCK_SIDECAR_READY));
    fireEvent.click(screen.getByTestId("ai-pill"));
    const settings = await screen.findByRole("dialog", { name: "Settings" });
    const run = await within(settings).findByRole("button", { name: "Run AI on library" });
    expect(within(settings).getByTestId("ai-state").textContent).toContain("Ready");
    fireEvent.click(run);
    await waitFor(() => expect(cmds(mock())).toContain("start_ai"));
    expect(reqOf(lastCall(mock(), "start_ai"))).toEqual({ tasks: ["ocr", "embed", "classify", "color"], imageIds: null });
    await within(settings).findByTestId("ai-job");
    // The status bar follows the same job by kind and stage.
    await waitFor(() => expect(document.querySelector('[data-job-kind="ai"]')?.textContent).toMatch(/^AI · (Reading text|Meaning index|Categories|Colours) \d+\/\d+/));
    await waitFor(() => expect(within(settings).queryByTestId("ai-job")).toBeNull(), { timeout: 4000 });
    noPurgeSent();
  });
});
