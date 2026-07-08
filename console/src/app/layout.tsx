import type { Metadata } from "next";
import { ACCENT, ATMOSPHERE, COLOR, DARK, DERIVED, FONT, MATERIAL, MOTION, TYPE } from "@/lib/tokens";
import "@/fonts/fonts.css";
import "./globals.css";

export const metadata: Metadata = {
  title: "Enterprise Brain",
  description:
    "Ask your company's knowledge. Every answer respects what you're allowed to see. Demo identity mode.",
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
  --dur-lift: ${MOTION.lift};
  --dur-iris: ${MOTION.iris};
  --ease-out: ${MOTION.easeOut};
  --ease-iris: ${MOTION.irisEase};
  --dur-skeleton: ${MOTION.skeletonPulse};
  --material-blur: ${MATERIAL.blur};
  --atmos-blue: ${ATMOSPHERE.blue};
  --atmos-violet: ${ATMOSPHERE.violet};
  --glass-fill: color-mix(in srgb, var(--surface-glass) 82%, transparent);
  --glass-border: color-mix(in srgb, var(--hairline-strong) 86%, var(--edge-highlight) 14%);
  --glass-highlight: color-mix(in srgb, var(--edge-highlight) 72%, transparent);
  --glass-scrim: color-mix(in srgb, var(--paper) 52%, transparent);
  /* The atmospheric depth wash: a desaturated blue->violet aurora behind all
     content. Strong but low-opacity glow on dark; a faint tint on light. The
     opacity vars are the ONE knob (dark reads richer than light). */
  --atmos-strength-a: 22%;
  --atmos-strength-b: 16%;
  --atmos-strength-c: 12%;
}
:root[data-theme="light"] {
  --atmos-strength-a: 9%;
  --atmos-strength-b: 7%;
  --atmos-strength-c: 5%;
}
body {
  background:
    radial-gradient(120% 90% at 8% -8%, color-mix(in srgb, var(--atmos-blue) var(--atmos-strength-a), transparent), transparent 60%),
    radial-gradient(110% 80% at 100% 0%, color-mix(in srgb, var(--atmos-violet) var(--atmos-strength-b), transparent), transparent 55%),
    radial-gradient(140% 120% at 50% 120%, color-mix(in srgb, var(--atmos-blue) var(--atmos-strength-c), transparent), transparent 60%),
    linear-gradient(180deg, var(--surface-0), color-mix(in srgb, var(--surface-0) 82%, var(--surface-1) 18%));
  background-attachment: fixed;
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
/* THE GLASS LAW (comprehension pass, B2): backdrop-filter exists ONLY on
   overlays — floating popovers/panels and the drawer scrim. Page-level
   surfaces use the solid elevation tokens (.ap-card / .ap-elevated /
   .ap-focus-surface). The old page-level glass classes are gone. */
.ap-nav {
  background: linear-gradient(180deg, var(--surface-2), var(--surface-1));
  border: 1px solid var(--hairline-strong);
  box-shadow: var(--shadow-1);
}
.ap-hero {
  background:
    radial-gradient(circle at 8% -10%, color-mix(in srgb, var(--atmos-violet) 16%, transparent), transparent 20rem),
    radial-gradient(circle at 100% 0%, color-mix(in srgb, var(--atmos-blue) 12%, transparent), transparent 18rem),
    linear-gradient(145deg, var(--surface-2), var(--surface-1));
  border: 1px solid var(--hairline-strong);
  box-shadow: var(--shadow-focus), inset 0 1px 0 var(--edge-highlight);
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
/* SHOWREEL TRACK A — the cinematic entry. The plate is a STILL image of the
   product (never a live graph); the scrim is overlay chrome, so the glass law
   holds (this is not a data surface). Nothing here animates ambiently. */
.ap-entry-plate {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
  user-select: none;
  pointer-events: none;
}
/* The entry scrim carries NO blur of its own — the glass comes from stacking
   the lawful .ap-glass-scrim overlay class beneath it, so the glass law's
   two-class allowlist stays exact. This layer only darkens. */
.ap-entry-scrim {
  position: absolute;
  inset: 0;
  background:
    radial-gradient(120% 90% at 50% 110%, color-mix(in srgb, var(--surface-0) 55%, transparent), transparent 70%),
    linear-gradient(180deg, color-mix(in srgb, var(--surface-0) 78%, transparent), color-mix(in srgb, var(--surface-0) 88%, transparent));
}
/* The ONE cold-open transition (entry -> picker): a single fade/scale inside
   the iris budget, fired by the button. Dead under prefers-reduced-motion. */
.ap-entry-iris { animation: ap-entry-iris var(--dur-iris) var(--ease-iris); }
@keyframes ap-entry-iris {
  from { opacity: 0; transform: scale(0.985); }
  to { opacity: 1; transform: scale(1); }
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
    transform var(--dur-lift) var(--ease-out);
}
.ap-washable:hover {
  background-color: var(--surface-2);
  border-color: var(--hairline-strong);
  box-shadow: var(--shadow-2);
  /* THE hover-lift (B3): one value repo-wide, shared with MOTION.framerHoverY. */
  transform: translateY(-2px);
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
/* Focus (B4): the desaturated interactive ink-blue token — 2px ring, 2px
   offset, both themes. Amber never marks focus; it is the signal color. */
:focus-visible { outline: 2px solid var(--affordance); outline-offset: 2px; }
.ap-skip-link {
  position: absolute;
  left: -9999px;
  top: 0;
  z-index: 100;
}
.ap-skip-link:focus-visible {
  left: 12px;
  top: 12px;
  position: fixed;
  background: var(--affordance);
  color: var(--paper);
  padding: 8px 16px;
  border-radius: 9999px;
  box-shadow: var(--shadow-2);
}
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
  .ap-skeleton-pulse,
  .ap-entry-iris {
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
      <body>
        <a href="#main" className="ap-skip-link ap-register-chrome">
          Skip to content
        </a>
        {children}
      </body>
    </html>
  );
}
