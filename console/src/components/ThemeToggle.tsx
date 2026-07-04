"use client";

import { useEffect, useState } from "react";
import { TYPE } from "@/lib/tokens";

type ThemeMode = "dark" | "light";

function readTheme(): ThemeMode {
  if (typeof document === "undefined") return "dark";
  return document.documentElement.getAttribute("data-theme") === "light" ? "light" : "dark";
}

function applyTheme(mode: ThemeMode) {
  document.documentElement.setAttribute("data-theme", mode);
  try {
    localStorage.setItem("ap-theme", mode);
  } catch {
    // Storage may be unavailable in private or restricted browser contexts.
  }
}

export function ThemeToggle({ compact = false }: { compact?: boolean }) {
  const [mode, setMode] = useState<ThemeMode>("dark");

  useEffect(() => {
    setMode(readTheme());
  }, []);

  const nextMode = mode === "dark" ? "light" : "dark";

  return (
    <button
      type="button"
      className="ap-card ap-washable ap-register-chrome min-h-10 rounded-full px-3 py-2"
      style={{ borderColor: "var(--hairline)", fontSize: compact ? TYPE.scale.xs : TYPE.scale.sm, fontWeight: 600 }}
      onClick={() => {
        applyTheme(nextMode);
        setMode(nextMode);
      }}
      aria-label={`Switch to ${nextMode} mode`}
      data-testid="theme-toggle"
    >
      {mode === "dark" ? "Light mode" : "Dark mode"}
    </button>
  );
}
