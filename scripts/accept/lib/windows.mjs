// Console-window probe (phase 3.4): does launching the app show a console window for the AI sidecar?
//
// The app is a GUI process; it starts `<venv>\Scripts\python.exe` with CREATE_NO_WINDOW, and that venv `python.exe`
// is a launcher that starts the real interpreter as its child (docs/lessons.md phase 3.1), so the flag on the parent
// alone does not prove the child shows nothing. Two independent signals, both from one PowerShell child (Win32 via
// Add-Type, inputs through env vars):
//   windows  every top-level window that becomes visible while the probe runs and was not visible when it started:
//            a WinEvent hook on EVENT_OBJECT_SHOW (catches a window shown and hidden again between polls) plus an
//            EnumWindows poll every 200 ms. A console opened by conhost is a `ConsoleWindowClass` window; with the
//            default terminal delegated to Windows Terminal (this machine) it is a `CASCADIA_HOSTING_WINDOW_CLASS`
//            window of WindowsTerminal.exe, which is NOT a descendant of the app, hence the class/name rules below.
//   consoles for each PID handed over at stop: AttachConsole(pid) + GetConsoleWindow(). A console created with
//            CREATE_NO_WINDOW has no window (hwnd 0); a visible or pseudo-console window (a Windows Terminal tab,
//            which adds no top-level window when it opens in an existing terminal) has one. A GUI process (pythonw)
//            has no console at all (attach fails).
// The probe writes its JSON result to a file, not stdout: it detaches from its own console to attach to others.
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { childEnv } from "./run.mjs";

/** Classes of a console window: conhost, Windows Terminal, and the ConPTY pseudo window. */
export const CONSOLE_CLASSES = ["ConsoleWindowClass", "CASCADIA_HOSTING_WINDOW_CLASS", "PseudoConsoleWindow"];
/** Processes whose window is a console (or the sidecar itself). */
export const CONSOLE_PROCESS = /^(python|pythonw|conhost|openconsole|windowsterminal)$/i;

const PROBE_PS = `
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

public class SsSeenWindow {
    public long Hwnd { get; set; }
    public int Pid { get; set; }
    public string Process { get; set; }
    public string Class { get; set; }
    public string Title { get; set; }
    public long AtMs { get; set; }
    public string Via { get; set; }
}

public class SsConsole {
    public int Pid { get; set; }
    public bool Attached { get; set; }
    public int Error { get; set; }
    public long Hwnd { get; set; }
    public bool Visible { get; set; }
    public string Class { get; set; }
}

public class SsProbeResult {
    public bool Hooked { get; set; }
    public int Baseline { get; set; }
    public long Ms { get; set; }
    public List<SsSeenWindow> Windows { get; set; }
    public List<SsConsole> Consoles { get; set; }
}

public static class SsWindowProbe {
    delegate bool EnumProc(IntPtr hwnd, IntPtr lParam);
    delegate void WinEventProc(IntPtr hook, uint ev, IntPtr hwnd, int idObject, int idChild, uint thread, uint time);

    [StructLayout(LayoutKind.Sequential)]
    struct MSG { public IntPtr hwnd; public uint message; public IntPtr wParam; public IntPtr lParam; public uint time; public int x; public int y; }

    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassNameW(IntPtr hwnd, StringBuilder sb, int max);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowTextW(IntPtr hwnd, StringBuilder sb, int max);
    [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr hwnd, uint flags);
    [DllImport("user32.dll")] static extern IntPtr SetWinEventHook(uint min, uint max, IntPtr mod, WinEventProc cb, uint pid, uint thread, uint flags);
    [DllImport("user32.dll")] static extern bool UnhookWinEvent(IntPtr hook);
    [DllImport("user32.dll")] static extern bool PeekMessageW(out MSG msg, IntPtr hwnd, uint min, uint max, uint remove);
    [DllImport("user32.dll")] static extern bool TranslateMessage(ref MSG msg);
    [DllImport("user32.dll")] static extern IntPtr DispatchMessageW(ref MSG msg);
    [DllImport("user32.dll")] static extern uint MsgWaitForMultipleObjects(uint count, IntPtr[] handles, bool waitAll, uint ms, uint mask);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool AttachConsole(uint pid);
    [DllImport("kernel32.dll")] static extern bool FreeConsole();
    [DllImport("kernel32.dll")] static extern IntPtr GetConsoleWindow();

    const uint EVENT_OBJECT_SHOW = 0x8002;
    const uint WINEVENT_SKIPOWNPROCESS = 2;
    const uint GA_ROOT = 2;
    const uint PM_REMOVE = 1;
    const uint QS_ALLINPUT = 0x04FF;

    static HashSet<long> baseline = new HashSet<long>();
    static Dictionary<long, SsSeenWindow> seen = new Dictionary<long, SsSeenWindow>();
    static Stopwatch clock;
    static long epochAtStart;
    static WinEventProc hookProc; // kept alive while the hook may call it

    static string Text(IntPtr hwnd, bool cls) {
        StringBuilder sb = new StringBuilder(512);
        if (cls) GetClassNameW(hwnd, sb, sb.Capacity); else GetWindowTextW(hwnd, sb, sb.Capacity);
        return sb.ToString();
    }

    static void Note(IntPtr hwnd, string via, bool mustBeVisible) {
        long key = hwnd.ToInt64();
        if (baseline.Contains(key) || seen.ContainsKey(key)) return;
        if (GetAncestor(hwnd, GA_ROOT) != hwnd) return;
        if (mustBeVisible && !IsWindowVisible(hwnd)) return;
        uint pid;
        GetWindowThreadProcessId(hwnd, out pid);
        string name;
        try { name = System.Diagnostics.Process.GetProcessById((int)pid).ProcessName; } catch (Exception) { name = "?"; }
        SsSeenWindow w = new SsSeenWindow();
        w.Hwnd = key; w.Pid = (int)pid; w.Process = name; w.Class = Text(hwnd, true); w.Title = Text(hwnd, false);
        w.AtMs = epochAtStart + clock.ElapsedMilliseconds; w.Via = via;
        seen[key] = w;
    }

    static SsConsole ConsoleOf(int pid) {
        SsConsole c = new SsConsole();
        c.Pid = pid; c.Class = "";
        c.Attached = AttachConsole((uint)pid);
        if (!c.Attached) { c.Error = Marshal.GetLastWin32Error(); return c; }
        IntPtr h = GetConsoleWindow();
        c.Hwnd = h.ToInt64();
        if (h != IntPtr.Zero) { c.Visible = IsWindowVisible(h); c.Class = Text(h, true); }
        FreeConsole();
        return c;
    }

    public static SsProbeResult Run(string readyFile, string stopFile, int maxMs) {
        clock = Stopwatch.StartNew();
        epochAtStart = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        EnumWindows(delegate(IntPtr h, IntPtr l) { if (IsWindowVisible(h)) baseline.Add(h.ToInt64()); return true; }, IntPtr.Zero);
        hookProc = delegate(IntPtr hook, uint ev, IntPtr hwnd, int idObject, int idChild, uint thread, uint time) {
            if (idObject == 0 && idChild == 0 && hwnd != IntPtr.Zero) Note(hwnd, "event", false);
        };
        IntPtr hk = SetWinEventHook(EVENT_OBJECT_SHOW, EVENT_OBJECT_SHOW, IntPtr.Zero, hookProc, 0, 0, WINEVENT_SKIPOWNPROCESS);
        File.WriteAllText(readyFile, "ready");
        long nextPoll = 0;
        while (clock.ElapsedMilliseconds < maxMs && !File.Exists(stopFile)) {
            MsgWaitForMultipleObjects(0, null, false, 50, QS_ALLINPUT);
            MSG m;
            while (PeekMessageW(out m, IntPtr.Zero, 0, 0, PM_REMOVE)) { TranslateMessage(ref m); DispatchMessageW(ref m); }
            if (clock.ElapsedMilliseconds >= nextPoll) {
                EnumWindows(delegate(IntPtr h, IntPtr l) { Note(h, "poll", true); return true; }, IntPtr.Zero);
                nextPoll = clock.ElapsedMilliseconds + 200;
            }
        }
        if (hk != IntPtr.Zero) UnhookWinEvent(hk);
        List<SsConsole> consoles = new List<SsConsole>();
        if (File.Exists(stopFile)) {
            string[] parts = File.ReadAllText(stopFile).Split(new char[] { ',', ' ', '\\n', '\\r' }, StringSplitOptions.RemoveEmptyEntries);
            if (parts.Length > 0) {
                FreeConsole();
                foreach (string s in parts) { int pid; if (int.TryParse(s, out pid)) consoles.Add(ConsoleOf(pid)); }
            }
        }
        SsProbeResult r = new SsProbeResult();
        r.Hooked = hk != IntPtr.Zero; r.Baseline = baseline.Count; r.Ms = clock.ElapsedMilliseconds;
        r.Windows = new List<SsSeenWindow>(seen.Values); r.Consoles = consoles;
        return r;
    }
}
'@
$r = [SsWindowProbe]::Run($env:SS_READY_FILE, $env:SS_STOP_FILE, [int]$env:SS_MAX_MS)
$json = ConvertTo-Json -Compress -Depth 4 -InputObject ([pscustomobject]@{
  hooked = $r.Hooked; baseline = $r.Baseline; ms = $r.Ms
  windows = @($r.Windows | ForEach-Object { [pscustomobject]@{ hwnd = $_.Hwnd; pid = $_.Pid; process = $_.Process; class = $_.Class; title = $_.Title; atMs = $_.AtMs; via = $_.Via } })
  consoles = @($r.Consoles | ForEach-Object { [pscustomobject]@{ pid = $_.Pid; attached = $_.Attached; error = $_.Error; hwnd = $_.Hwnd; visible = $_.Visible; class = $_.Class } })
})
[System.IO.File]::WriteAllText($env:SS_OUT_FILE, $json)
`;

const sleep = (ms) => new Promise((res) => setTimeout(res, ms));

/**
 * Starts the probe and resolves once its baseline is taken and its hook is set (launch the app after that).
 * `stop(pids)` hands over the PIDs whose console to inspect, ends the probe and resolves with its result:
 * { hooked, baseline, ms, windows: [{ hwnd, pid, process, class, title, atMs, via }], consoles: [{ pid, attached,
 * error, hwnd, visible, class }] }.
 */
export async function startWindowProbe({ maxMs = 300_000, readyTimeoutMs = 60_000 } = {}) {
  const dir = mkdtempSync(path.join(tmpdir(), "ss-winprobe-"));
  const files = { script: path.join(dir, "probe.ps1"), ready: path.join(dir, "ready"), stop: path.join(dir, "stop"), out: path.join(dir, "out.json") };
  writeFileSync(files.script, PROBE_PS, "utf8");
  const env = childEnv({ SS_READY_FILE: files.ready, SS_STOP_FILE: files.stop, SS_OUT_FILE: files.out, SS_MAX_MS: String(maxMs) });
  const proc = spawn("powershell.exe", ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", files.script], { env, stdio: ["ignore", "pipe", "pipe"] });
  let log = "";
  proc.stdout.on("data", (d) => (log += d));
  proc.stderr.on("data", (d) => (log += d));
  const exited = new Promise((res) => proc.once("exit", (code) => res(code)));
  const cleanup = () => rmSync(dir, { recursive: true, force: true });
  const end = Date.now() + readyTimeoutMs;
  while (!existsSync(files.ready)) {
    if (proc.exitCode !== null || Date.now() > end) {
      proc.kill();
      cleanup();
      throw new Error(`window probe did not start (exit ${proc.exitCode ?? "timeout"}): ${log.trim().slice(-500)}`);
    }
    await sleep(50);
  }
  return {
    async stop(pids = []) {
      writeFileSync(files.stop, pids.join(","));
      const code = await Promise.race([exited, sleep(30_000).then(() => "timeout")]);
      try {
        if (code === "timeout") proc.kill();
        if (!existsSync(files.out)) throw new Error(`window probe wrote no result (exit ${code}): ${log.trim().slice(-500)}`);
        return JSON.parse(readFileSync(files.out, "utf8").replace(/^﻿/, ""));
      } finally {
        cleanup();
      }
    },
  };
}

/** True for a window that is a console (by class or owning process) or belongs to one of `pids`. */
export const isConsoleWindow = (w, pids = []) => CONSOLE_CLASSES.includes(w.class) || CONSOLE_PROCESS.test(w.process) || pids.includes(w.pid);
