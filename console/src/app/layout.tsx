import type { Metadata } from "next";
import { ACCENT, COLOR, DARK, DERIVED, FONT, MATERIAL, MOTION, TYPE } from "@/lib/tokens";
import "@/fonts/fonts.css";
import "./globals.css";

export const metadata: Metadata = {
  title: "Ask Brain — Aperture",
  description: "Enterprise Brain realization surface — demo identity mode",
};

// Every CSS variable and base rule is GENERATED from tokens.ts: no color,
// duration, or font literal exists in this file or anywhere under src/app
// and src/components (U-6 enforces it). Material depth is tokenized here:
// subtle tone shifts, hairlines, and narrow shadows.
//
// THEME: the console defaults to DARK (the Org Brain command surface). Colour
// vars live in two theme blocks keyed on [data-theme]; everything else (fonts,
// type, motion) is theme-independent. data-theme is set before paint by the
// init script below (default dark, light remembered per browser).
const apertureBase = `
:root {
  --paper: ${DARK.paper};
  --ink: ${DARK.ink};
  --ink-soft: ${DARK.inkSoft};
  --affordance: ${DARK.affordance};
  --hairline: ${DARK.hairline};
  --wash: ${DARK.wash};
  --accent-warm: ${ACCENT.warm};
  --surface-0: ${MATERIAL.dark.surface0};
  --surface-1: ${MATERIAL.dark.surface1};
  --surface-2: ${MATERIAL.dark.surface2};
  --surface-3: ${MATERIAL.dark.surface3};
  --surface-focus: ${MATERIAL.dark.focus};
  --surface-glass: ${MATERIAL.dark.glass};
  --surface-chip: ${MATERIAL.dark.chip};
  --hairline-strong: ${MATERIAL.dark.hairlineStrong};
  --edge-highlight: ${MATERIAL.dark.edge};
  --shadow-1: ${MATERIAL.dark.shadow1};
  --shadow-2: ${MATERIAL.dark.shadow2};
  --shadow-focus: ${MATERIAL.dark.shadowFocus};
}
:root[data-theme="light"] {
  --paper: ${COLOR.paper};
  --ink: ${COLOR.ink};
  --ink-soft: ${COLOR.inkSoft};
  --affordance: ${COLOR.affordance};
  --hairline: ${DERIVED.hairline};
  --wash: ${DERIVED.wash};
  --surface-0: ${MATERIAL.light.surface0};
  --surface-1: ${MATERIAL.light.surface1};
  --surface-2: ${MATERIAL.light.surface2};
  --surface-3: ${MATERIAL.light.surface3};
  --surface-focus: ${MATERIAL.light.focus};
  --surface-glass: ${MATERIAL.light.glass};
  --surface-chip: ${MATERIAL.light.chip};
  --hairline-strong: ${MATERIAL.light.hairlineStrong};
  --edge-highlight: ${MATERIAL.light.edge};
  --shadow-1: ${MATERIAL.light.shadow1};
  --shadow-2: ${MATERIAL.light.shadow2};
  --shadow-focus: ${MATERIAL.light.shadowFocus};
}
:root {
  --font-chrome: ${FONT.chrome};
  --font-answer: ${FONT.answer};
  --font-evidence: ${FONT.evidence};
  --text-xs: ${TYPE.scale.xs}px;
  --text-sm: ${TYPE.scale.sm}px;
  --text-md: ${TYPE.scale.md}px;
  --text-lg: ${TYPE.scale.lg}px;
  --text-xl: ${TYPE.scale.xl}px;
  --line-body: ${TYPE.line.body};
  --line-display: ${TYPE.line.display};
  --dur-quick: ${MOTION.fadeQuick};
  --dur-view: ${MOTION.fadeView};
  --dur-iris: ${MOTION.iris};
  --ease-out: ${MOTION.easeOut};
  --ease-iris: ${MOTION.irisEase};
  --dur-skeleton: ${MOTION.skeletonPulse};
  --material-blur: ${MATERIAL.blur};
  --glass-fill: color-mix(in srgb, var(--surface-glass) 82%, transparent);
  --glass-border: color-mix(in srgb, var(--hairline-strong) 86%, var(--edge-highlight) 14%);
  --glass-highlight: color-mix(in srgb, var(--edge-highlight) 72%, transparent);
  --glass-scrim: color-mix(in srgb, var(--paper) 52%, transparent);
}
body {
  background:
    radial-gradient(circle at 12% 4%, color-mix(in srgb, var(--affordance) 16%, transparent), transparent 26rem),
    radial-gradient(circle at 92% 10%, color-mix(in srgb, var(--accent-warm) 10%, transparent), transparent 24rem),
    linear-gradient(180deg, var(--surface-0), color-mix(in srgb, var(--surface-0) 84%, var(--surface-1) 16%));
  color: var(--ink);
  font-family: var(--font-chrome);
  font-size: var(--text-sm);
  line-height: var(--line-body);
}
.ap-register-chrome { font-family: var(--font-chrome); }
.ap-register-answer { font-family: var(--font-answer); }
.ap-register-evidence { font-family: var(--font-evidence); }
.ap-hairline { border-color: var(--hairline); }
.ap-card {
  background: var(--surface-1);
  border: 1px solid var(--hairline);
  box-shadow: var(--shadow-1);
}
.ap-elevated {
  background: var(--surface-2);
  border-color: var(--hairline-strong);
  box-shadow: var(--shadow-2);
}
.ap-focus-surface {
  background: var(--surface-focus);
  border-color: var(--hairline-strong);
  box-shadow: var(--shadow-focus);
}
.ap-glass {
  background: var(--glass-fill);
  border-color: var(--glass-border);
  backdrop-filter: blur(var(--material-blur));
  -webkit-backdrop-filter: blur(var(--material-blur));
  box-shadow: var(--shadow-1), inset 0 1px 0 var(--glass-highlight);
}
.ap-glass-nav {
  background:
    linear-gradient(135deg, color-mix(in srgb, var(--surface-glass) 94%, transparent), color-mix(in srgb, var(--surface-glass) 66%, transparent)),
    var(--glass-fill);
  border: 1px solid var(--glass-border);
  backdrop-filter: blur(calc(var(--material-blur) * 1.2)) saturate(1.2);
  -webkit-backdrop-filter: blur(calc(var(--material-blur) * 1.2)) saturate(1.2);
  box-shadow: var(--shadow-2), inset 0 1px 0 var(--glass-highlight);
}
.ap-glass-panel {
  background:
    linear-gradient(180deg, color-mix(in srgb, var(--surface-glass) 94%, transparent), color-mix(in srgb, var(--surface-glass) 74%, transparent)),
    var(--glass-fill);
  border: 1px solid var(--glass-border);
  backdrop-filter: blur(calc(var(--material-blur) * 1.1)) saturate(1.16);
  -webkit-backdrop-filter: blur(calc(var(--material-blur) * 1.1)) saturate(1.16);
  box-shadow: var(--shadow-2), inset 0 1px 0 var(--glass-highlight);
}
.ap-glass-elevated {
  background:
    radial-gradient(circle at 10% 0%, color-mix(in srgb, var(--affordance) 13%, transparent), transparent 18rem),
    linear-gradient(145deg, color-mix(in srgb, var(--surface-glass) 96%, transparent), color-mix(in srgb, var(--surface-glass) 68%, transparent));
  border: 1px solid var(--glass-border);
  backdrop-filter: blur(calc(var(--material-blur) * 1.25)) saturate(1.22);
  -webkit-backdrop-filter: blur(calc(var(--material-blur) * 1.25)) saturate(1.22);
  box-shadow: var(--shadow-focus), inset 0 1px 0 var(--glass-highlight);
}
.ap-glass-popover {
  background: color-mix(in srgb, var(--surface-glass) 90%, transparent);
  border: 1px solid var(--glass-border);
  backdrop-filter: blur(calc(var(--material-blur) * 1.35)) saturate(1.25);
  -webkit-backdrop-filter: blur(calc(var(--material-blur) * 1.35)) saturate(1.25);
  box-shadow: var(--shadow-focus), inset 0 1px 0 var(--glass-highlight);
}
.ap-glass-scrim {
  background: var(--glass-scrim);
  backdrop-filter: blur(6px);
  -webkit-backdrop-filter: blur(6px);
}
.ap-flat {
  background: transparent;
  box-shadow: none;
}
.ap-chip {
  background: var(--surface-chip);
  border: 1px solid var(--hairline);
  color: var(--ink-soft);
}
.ap-soft { color: var(--ink-soft); }
.ap-washable {
  transition:
    background-color var(--dur-quick) var(--ease-out),
    border-color var(--dur-quick) var(--ease-out),
    box-shadow var(--dur-quick) var(--ease-out),
    transform var(--dur-quick) var(--ease-out);
}
.ap-washable:hover {
  background-color: var(--surface-2);
  border-color: var(--hairline-strong);
  box-shadow: var(--shadow-2);
  transform: translateY(-4px);
}
.ap-washable:active { transform: translateY(1px); }
.ap-affordance-button {
  background: var(--affordance);
  color: var(--paper);
  box-shadow: var(--shadow-1);
  transition:
    opacity var(--dur-quick) var(--ease-out),
    transform var(--dur-quick) var(--ease-out);
}
.ap-affordance-button:hover { opacity: 0.9; }
.ap-affordance-button:active { transform: translateY(1px); }
.ap-affordance-button:disabled { opacity: 0.4; }
a, .ap-affordance-text { color: var(--affordance); }
:focus-visible { outline: 2px solid var(--affordance); outline-offset: 1px; }
input[type="checkbox"] { accent-color: var(--affordance); }
input, textarea {
  background: var(--surface-1);
  color: var(--ink);
  border: 1px solid var(--hairline);
  box-shadow: inset 0 1px 0 var(--edge-highlight);
}
::placeholder { color: var(--ink-soft); opacity: 0.7; }
.ap-fade-view { animation: ap-fade var(--dur-view) var(--ease-out); }
.ap-skeleton-pulse { animation: ap-pulse var(--dur-skeleton) var(--ease-out) infinite alternate; }
@keyframes ap-fade { from { opacity: 0; } to { opacity: 1; } }
@keyframes ap-pulse { from { opacity: 0.45; } to { opacity: 0.9; } }
@media (prefers-reduced-motion: reduce) {
  .ap-fade-view,
  .ap-skeleton-pulse {
    animation: none;
  }
  .ap-washable,
  .ap-affordance-button {
    transition: none;
  }
  .ap-washable:hover,
  .ap-washable:active,
  .ap-affordance-button:active {
    transform: none;
  }
}
`;

// Set the theme attribute before first paint (no flash): remembered choice,
// else the dark default. The console ships dark; light is one click away.
const themeInit = `try{var t=localStorage.getItem('ap-theme');document.documentElement.setAttribute('data-theme',t==='light'?'light':'dark');}catch(e){}`;

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" data-theme="dark" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: themeInit }} />
        <style dangerouslySetInnerHTML={{ __html: apertureBase }} />
      </head>
      <body>{children}</body>
    </html>
  );
}
