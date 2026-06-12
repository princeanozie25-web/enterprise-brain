// THE CURSOR (flagged design, per the build prompt): Playwright recordings
// do not capture the OS cursor, so every context gets an init script that
// renders a 14px ring — ink #16160F (the token), 2px stroke, 40% fill —
// tracking real pointer events. This is a faithful rendering of real
// interactions, not decoration: the ring moves only because the scripted
// mouse moved.

export const CURSOR_RING_PX = 14;
export const CURSOR_STROKE_PX = 2;
export const CURSOR_INK = "#16160F";
export const CURSOR_FILL = "rgba(22, 22, 15, 0.4)";

export const cursorInitScript = `(() => {
  if (window.__ebReelCursor) { return; }
  window.__ebReelCursor = true;
  const ring = document.createElement("div");
  ring.setAttribute("data-eb-reel-cursor", "");
  ring.style.cssText = [
    "position: fixed",
    "left: 0",
    "top: 0",
    "width: ${CURSOR_RING_PX}px",
    "height: ${CURSOR_RING_PX}px",
    "border: ${CURSOR_STROKE_PX}px solid ${CURSOR_INK}",
    "background: ${CURSOR_FILL}",
    "border-radius: 50%",
    "pointer-events: none",
    "z-index: 2147483647",
    "transform: translate(-50%, -50%)",
    "display: none",
  ].join("; ");
  const attach = () => {
    if (document.body && !ring.isConnected) {
      document.body.appendChild(ring);
    }
  };
  document.addEventListener(
    "pointermove",
    (event) => {
      attach();
      ring.style.display = "block";
      ring.style.left = event.clientX + "px";
      ring.style.top = event.clientY + "px";
    },
    { capture: true, passive: true },
  );
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", attach);
  } else {
    attach();
  }
})();`;
