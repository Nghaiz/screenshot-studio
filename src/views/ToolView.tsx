// The smart tools' main area: the open tool, mounted only while its view is open. Imported eagerly, not as lazy
// chunks: React holds a Suspense fallback for at least 300 ms (FALLBACK_THROTTLE_MS), so a 1-2 ms chunk load still
// opened every tool on an empty pane for ~0.3 s (phase 3.4b, installed app, real library). The four chunks were
// 22 KB together; nothing of a tool runs until it is mounted (memory gate, Deferred decisions #31 and #34).
import type { ToolView as ToolViewId } from "../lib/query";
import CleanupWizard from "./smart/CleanupWizard";
import DuplicatesView from "./smart/DuplicatesView";
import StatsDashboard from "./smart/StatsDashboard";

export function ToolView({ view }: { view: ToolViewId }) {
  return view === "duplicates" ? <DuplicatesView /> : view === "cleanup" ? <CleanupWizard /> : <StatsDashboard />;
}
