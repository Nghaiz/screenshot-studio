// The smart tools' main area. Each tool is its own lazy chunk, mounted only while its view is open, so the library
// view never pays for them (memory gate, Deferred decision #31).
import { lazy, Suspense } from "react";
import type { ToolView as ToolViewId } from "../lib/query";

const DuplicatesView = lazy(() => import("./smart/DuplicatesView"));
const CleanupWizard = lazy(() => import("./smart/CleanupWizard"));
const StatsDashboard = lazy(() => import("./smart/StatsDashboard"));

export function ToolView({ view }: { view: ToolViewId }) {
  return (
    <Suspense fallback={<div className="ss-panel ss-shimmer min-h-0 flex-1" aria-busy="true" />}>
      {view === "duplicates" ? <DuplicatesView /> : view === "cleanup" ? <CleanupWizard /> : <StatsDashboard />}
    </Suspense>
  );
}
