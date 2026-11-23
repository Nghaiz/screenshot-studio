// Modal shell for every organize dialog: backdrop, Esc = cancel, focus back to where it was on close.
// Keys typed inside never reach the grid or the window shortcuts (stopPropagation), so Delete / Ctrl+Z in a
// text field stay text editing.
import { useEffect, useRef, type ReactNode } from "react";
import { useView } from "../../state/view";

export interface ModalProps {
  label: string;
  role?: "dialog" | "alertdialog";
  wide?: boolean;
  onCancel?: () => void;
  children: ReactNode;
}

export function Modal({ label, role = "dialog", wide, onCancel, children }: ModalProps) {
  const ref = useRef<HTMLDivElement>(null);
  const cancel = onCancel ?? (() => useView.getState().closeDialog());
  useEffect(() => {
    const back = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    if (!ref.current?.contains(document.activeElement)) ref.current?.focus();
    return () => back?.focus();
  }, []);
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" style={{ background: "color-mix(in srgb, black 50%, transparent)" }}
      onMouseDown={(e) => e.target === e.currentTarget && cancel()}>
      <div
        ref={ref}
        role={role}
        aria-modal="true"
        aria-label={label}
        tabIndex={-1}
        className={`ss-panel flex max-h-[85vh] flex-col gap-3 overflow-y-auto p-5 text-sm outline-none ${wide ? "w-[min(760px,94vw)]" : "w-[min(440px,94vw)]"}`}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Escape") {
            e.preventDefault();
            cancel();
          }
        }}
      >
        <h2 className="m-0 text-base font-semibold">{label}</h2>
        {children}
      </div>
    </div>
  );
}

export function Buttons({ children }: { children: ReactNode }) {
  return <div className="flex justify-end gap-2 pt-1">{children}</div>;
}
