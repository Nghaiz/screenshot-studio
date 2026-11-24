// The calm loading state of a view or a step while its data is on the way (phase 3.4b): a label with a softly
// pulsing dot, in theme tokens. Always readable: the label carries it, so with motion off or prefers-reduced-motion
// (the dot holds still, index.css `.ss-pulse`) it is still plainly a loading state, never an empty pane.
export function ViewLoading({ label, panel = false }: { label: string; panel?: boolean }) {
  return (
    <div
      role="status"
      aria-busy="true"
      data-testid="view-loading"
      className={`ss-muted flex items-center gap-2 text-sm ${panel ? "ss-panel min-h-0 flex-1 justify-center p-6" : ""}`}
    >
      <span aria-hidden className="ss-dot ss-pulse" data-state="ready" />
      <span>{label}</span>
    </div>
  );
}
