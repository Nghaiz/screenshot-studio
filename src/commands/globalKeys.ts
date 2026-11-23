// Window-level shortcuts: Ctrl+K palette, Ctrl+F search box, Ctrl+Z undo (design-intent § Look and motion).
import { useEffect } from "react";
import { undo } from "../actions/images";
import { SEARCH_INPUT_ID } from "../components/TopBar";
import { useView } from "../state/view";

const isTyping = (el: Element | null) =>
  el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || (el instanceof HTMLElement && el.isContentEditable);

export function useGlobalKeys(): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey || e.metaKey) return;
      const key = e.key.toLowerCase();
      if (key === "k") {
        e.preventDefault();
        const v = useView.getState();
        v.setPalette(!v.paletteOpen);
      } else if (key === "f") {
        e.preventDefault();
        document.getElementById(SEARCH_INPUT_ID)?.focus();
      } else if (key === "z" && !e.shiftKey && !isTyping(document.activeElement)) {
        e.preventDefault();
        void undo(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
