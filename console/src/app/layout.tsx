import type { Metadata } from "next";
import { COLOR, DERIVED, FONT, MOTION, TYPE } from "@/lib/tokens";
import "@/fonts/fonts.css";
import "./globals.css";

export const metadata: Metadata = {
  title: "Ask Brain — Aperture",
  description: "Enterprise Brain realization surface — demo identity mode",
};

// Every CSS variable and base rule is GENERATED from tokens.ts: no color,
// duration, or font literal exists in this file or anywhere under src/app
// and src/components (U-6 enforces it). Whitespace is the elevation system;
// the hairline is the only shadow-like device.
const apertureBase = `
:root {
  --paper: ${COLOR.paper};
  --ink: ${COLOR.ink};
  --ink-soft: ${COLOR.inkSoft};
  --affordance: ${COLOR.affordance};
  --hairline: ${DERIVED.hairline};
  --wash: ${DERIVED.wash};
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
}
body {
  background: var(--paper);
  color: var(--ink);
  font-family: var(--font-chrome);
  font-size: var(--text-sm);
  line-height: var(--line-body);
}
.ap-register-chrome { font-family: var(--font-chrome); }
.ap-register-answer { font-family: var(--font-answer); }
.ap-register-evidence { font-family: var(--font-evidence); }
.ap-hairline { border-color: var(--hairline); }
.ap-card { background: var(--paper); border: 1px solid var(--hairline); }
.ap-soft { color: var(--ink-soft); }
.ap-washable { transition: background-color var(--dur-quick) var(--ease-out); }
.ap-washable:hover { background-color: var(--wash); }
.ap-affordance-button {
  background: var(--affordance);
  color: var(--paper);
  transition: opacity var(--dur-quick) var(--ease-out);
}
.ap-affordance-button:hover { opacity: 0.9; }
.ap-affordance-button:disabled { opacity: 0.4; }
a, .ap-affordance-text { color: var(--affordance); }
:focus-visible { outline: 2px solid var(--affordance); outline-offset: 1px; }
input[type="checkbox"] { accent-color: var(--affordance); }
input, textarea {
  background: var(--paper);
  color: var(--ink);
  border: 1px solid var(--hairline);
}
::placeholder { color: var(--ink-soft); opacity: 0.7; }
.ap-fade-view { animation: ap-fade var(--dur-view) var(--ease-out); }
.ap-skeleton-pulse { animation: ap-pulse var(--dur-skeleton) var(--ease-out) infinite alternate; }
@keyframes ap-fade { from { opacity: 0; } to { opacity: 1; } }
@keyframes ap-pulse { from { opacity: 0.45; } to { opacity: 0.9; } }
`;

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en">
      <head>
        <style dangerouslySetInnerHTML={{ __html: apertureBase }} />
      </head>
      <body>{children}</body>
    </html>
  );
}
