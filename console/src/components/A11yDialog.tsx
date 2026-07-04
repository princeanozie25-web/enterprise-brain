"use client";

import { useCallback, useEffect, useRef } from "react";

/**
 * SHARED DIALOG FOCUS PRIMITIVE (comprehension pass, B6) — extracted from the
 * /me dashboard drawer (the one place that had it) so every drawer/side-sheet
 * gets identical semantics instead of hand-rolled copies:
 *
 * - on open: remember the opener, move focus into the dialog (WCAG 2.4.3)
 * - while open: trap Tab / Shift+Tab inside (WCAG 4.1.2 dialog behavior)
 * - Escape closes
 * - on close: restore focus to the opener
 *
 * The consumer renders its own element (plain <aside>, motion.aside, …) and
 * spreads: ref={dialogRef} onKeyDown={onKeyDown} plus role="dialog"
 * aria-modal="true" aria-label tabIndex={-1}.
 */
export function useModalDialogFocus({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}): {
  dialogRef: React.MutableRefObject<HTMLElement | null>;
  onKeyDown: (event: React.KeyboardEvent<HTMLElement>) => void;
} {
  const dialogRef = useRef<HTMLElement | null>(null);
  const restoreRef = useRef<HTMLElement | null>(null);
  const prevOpenRef = useRef(false);

  useEffect(() => {
    const prev = prevOpenRef.current;
    if (open && !prev) {
      restoreRef.current = (document.activeElement as HTMLElement | null) ?? null;
    }
    if (open) {
      dialogRef.current?.focus();
    }
    if (!open && prev) {
      restoreRef.current?.focus?.();
      restoreRef.current = null;
    }
    prevOpenRef.current = open;
  }, [open]);

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLElement>) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== "Tab") return;
      const root = dialogRef.current;
      if (root === null) return;
      const focusables = Array.from(
        root.querySelectorAll<HTMLElement>(
          'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ),
      );
      if (focusables.length === 0) {
        event.preventDefault();
        root.focus();
        return;
      }
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      const active = document.activeElement as HTMLElement | null;
      if (event.shiftKey) {
        if (active === first || active === root) {
          event.preventDefault();
          last.focus();
        }
      } else if (active === last) {
        event.preventDefault();
        first.focus();
      }
    },
    [onClose],
  );

  return { dialogRef, onKeyDown };
}
