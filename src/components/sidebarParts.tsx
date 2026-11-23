// Building blocks shared by the sidebar sections.
import type { DragEvent, ReactNode } from "react";

export function Section({ title, action, children }: { title: string; action?: ReactNode; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-0.5">
      <div className="flex items-center px-2 pb-1">
        <h3 className="ss-muted m-0 flex-1 text-xs font-semibold uppercase tracking-wide">{title}</h3>
        {action}
      </div>
      {children}
    </section>
  );
}

export interface NavItemProps {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
  label?: string;
  drop?: {
    onDragOver: (e: DragEvent) => void;
    onDragLeave: (e: DragEvent) => void;
    onDrop: (e: DragEvent) => void;
  };
  /** Small trailing buttons (edit, delete); rendered outside the main button so they stay separately focusable. */
  tools?: ReactNode;
}

export function NavItem({ active, onClick, children, label, drop, tools }: NavItemProps) {
  return (
    <div className="ss-nav group flex items-center rounded" {...drop}>
      <button
        type="button"
        aria-current={active ? "page" : undefined}
        aria-label={label}
        className="flex min-w-0 flex-1 items-center gap-2 rounded px-2 py-1 text-left"
        style={active ? { background: "var(--ss-accent)", color: "var(--ss-accent-contrast)" } : { color: "var(--ss-text)" }}
        onClick={onClick}
      >
        {children}
      </button>
      {tools && <div className="flex shrink-0 gap-0.5 opacity-60 group-hover:opacity-100 group-focus-within:opacity-100">{tools}</div>}
    </div>
  );
}

export function ToolButton({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <button type="button" className="rounded px-1 text-xs" style={{ color: "var(--ss-muted)" }} aria-label={label} title={label} onClick={onClick}>
      {children}
    </button>
  );
}
