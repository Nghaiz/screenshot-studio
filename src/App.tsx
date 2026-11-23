// Phase 1.1 skeleton: a themed, non-white window. The real shell lands in phase 1.3.
export default function App() {
  return (
    <main className="flex h-full items-center justify-center">
      <div
        className="px-10 py-8 text-center"
        style={{
          background: "var(--ss-surface)",
          backdropFilter: "blur(var(--ss-surface-blur))",
          border: "1px solid var(--ss-border)",
          borderRadius: "var(--ss-radius)",
          boxShadow: "var(--ss-shadow)",
        }}
      >
        <h1 className="text-2xl font-semibold" style={{ color: "var(--ss-accent)" }}>
          Screenshot Studio
        </h1>
        <p className="mt-2 text-sm" style={{ color: "var(--ss-muted)" }}>
          aurora-glass
        </p>
      </div>
    </main>
  );
}
