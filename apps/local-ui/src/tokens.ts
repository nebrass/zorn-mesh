// Project-owned design tokens for the v0.1 local UI shell.
//
// Tokens are intentionally project-owned (not pulled from a remote design
// system) so the bundle remains offline-served by the daemon UI gateway on
// loopback only. Changes here must keep the v0.1 dark-first/cyan-accent
// posture documented in docs/architecture/local-ui-amendment.md.

export const tokens = {
  surface: {
    "graphite-900": "#0f1115",
    "graphite-800": "#161a21",
    "charcoal-700": "#1d222b",
    "charcoal-600": "#262c38",
  },
  action: {
    "electric-blue-500": "#3b82f6",
    "electric-blue-400": "#60a5fa",
  },
  trust: {
    "cyan-local-400": "#22d3ee",
    "cyan-local-300": "#67e8f9",
  },
  semantic: {
    success: "#22c55e",
    warning: "#f59e0b",
    error: "#ef4444",
    neutral: "#9ca3af",
  },
  light: {
    surface: "#f8fafc",
    text: "#0f172a",
  },
  typography: {
    sans: "InterVariable, system-ui, sans-serif",
    mono: "JetBrains Mono, ui-monospace, SFMono-Regular, monospace",
  },
  spacing: {
    xs: "0.25rem",
    sm: "0.5rem",
    md: "0.75rem",
    lg: "1rem",
    xl: "1.5rem",
  },
  radius: {
    sm: "0.25rem",
    md: "0.375rem",
    lg: "0.5rem",
  },
  border: {
    subtle: "#1f2937",
    strong: "#334155",
  },
  focus: {
    ring: "0 0 0 2px #3b82f6",
    inset: "inset 0 0 0 1px #60a5fa",
  },
} as const;

export type Tokens = typeof tokens;
