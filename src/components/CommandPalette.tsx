// Ctrl+K command palette over PALETTE_COMMANDS. Arrow keys pick, Enter runs, Esc closes.
import { useMemo, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { filterCommands, PALETTE_COMMANDS, type CommandContext } from "../commands/palette";
import { useT } from "../i18n";
import { useView } from "../state/view";
import { useMotion } from "./useMotion";

export function CommandPalette({ ctx }: { ctx: CommandContext }) {
  const open = useView((s) => s.paletteOpen);
  const m = useMotion();
  return (
    <AnimatePresence>
      {open && (
        <motion.div
          className="fixed inset-0 z-50 flex items-start justify-center pt-[12vh]"
          style={{ background: "color-mix(in srgb, black 45%, transparent)" }}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={m.fade}
          onMouseDown={(e) => e.target === e.currentTarget && useView.getState().setPalette(false)}
        >
          <PaletteBody ctx={ctx} />
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function PaletteBody({ ctx }: { ctx: CommandContext }) {
  const t = useT();
  const [text, setText] = useState("");
  const [active, setActive] = useState(0);
  const shown = useMemo(() => filterCommands(PALETTE_COMMANDS, text, t), [text, t]);
  const close = () => useView.getState().setPalette(false);
  const runAt = (i: number) => {
    const cmd = shown[i];
    if (!cmd) return;
    close();
    void cmd.run(ctx);
  };

  return (
    <div role="dialog" aria-modal="true" aria-label={t("palette.label")} className="ss-panel w-[min(560px,92vw)] overflow-hidden">
      <input
        autoFocus
        className="w-full border-0 bg-transparent px-4 py-3 text-base outline-none"
        style={{ color: "var(--ss-text)", borderBottom: "1px solid var(--ss-border)" }}
        placeholder={t("palette.placeholder")}
        aria-label={t("palette.placeholder")}
        aria-controls="ss-palette-list"
        aria-activedescendant={shown[active] ? `ss-cmd-${shown[active].id}` : undefined}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          setActive(0);
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") setActive((a) => Math.min(shown.length - 1, a + 1));
          else if (e.key === "ArrowUp") setActive((a) => Math.max(0, a - 1));
          else if (e.key === "Enter") runAt(active);
          else if (e.key === "Escape") close();
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      />
      <ul id="ss-palette-list" role="listbox" className="m-0 max-h-[50vh] list-none overflow-y-auto p-1">
        {shown.length === 0 && <li className="ss-muted px-3 py-2 text-sm">{t("palette.empty")}</li>}
        {shown.map((c, i) => (
          <li
            key={c.id}
            id={`ss-cmd-${c.id}`}
            role="option"
            aria-selected={i === active}
            className="flex cursor-pointer items-center justify-between rounded px-3 py-1.5 text-sm"
            style={i === active ? { background: "var(--ss-accent)", color: "var(--ss-accent-contrast)" } : undefined}
            onMouseEnter={() => setActive(i)}
            onClick={() => runAt(i)}
          >
            <span>{c.label(t)}</span>
            {c.shortcut && <kbd className="text-xs opacity-70">{c.shortcut}</kbd>}
          </li>
        ))}
      </ul>
    </div>
  );
}
