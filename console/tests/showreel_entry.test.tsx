import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { EntryScreen } from "@/components/EntryScreen";
import { FrontDoor } from "@/components/FrontDoor";

/**
 * SHOWREEL TRACK A — the cinematic entry screen + reframed picker.
 * The entry is CHROME: no real authentication, no dead credential
 * affordance, honest labels throughout (invariant 2). The CTA fires the ONE
 * entry->picker transition; the picker behavior is unchanged underneath.
 */

describe("showreel entry screen", () => {
  it("renders the cold open: wordmark, thesis, honesty line, CTA, bottom strip", () => {
    render(<EntryScreen onEnter={() => {}} />);
    expect(screen.getByRole("heading", { level: 1, name: "Enterprise Brain" })).toBeTruthy();
    expect(screen.getByTestId("entry-thesis").textContent).toContain(
      "every answer respects exactly what you're allowed to see",
    );
    expect(screen.getByTestId("entry-honesty-line").textContent).toContain(
      "A working demo on a synthetic company. Scope-proven, not certified secure.",
    );
    expect(screen.getByTestId("entry-cta").textContent).toBe("Enter the demo");
    expect(screen.getByTestId("entry-bottom-strip").textContent).toContain(
      "authorize before the act",
    );
  });

  it("carries NO credential affordance — real or fake (invariant 2)", () => {
    const { container } = render(<EntryScreen onEnter={() => {}} />);
    const text = container.textContent ?? "";
    // No OAuth/provider button, no password field, no account language as a
    // functional claim. A real credential button is broken; a fake one is a
    // dead affordance — both sink the film.
    expect(text).not.toMatch(/google|gmail|oauth|sso|sign in with/i);
    expect(container.querySelector("input")).toBeNull();
    expect(container.querySelectorAll("button")).toHaveLength(1);
  });

  it("backdrop is a still plate, aria-hidden, never a live graph", () => {
    const { container } = render(<EntryScreen onEnter={() => {}} />);
    const backdrop = container.querySelector('[aria-hidden="true"]');
    expect(backdrop).toBeTruthy();
    const img = backdrop?.querySelector("img.ap-entry-plate");
    expect(img?.getAttribute("src")).toBe("/entry-plate.png");
    expect(img?.getAttribute("alt")).toBe("");
    // No SVG/canvas — the org map is an image, so it cannot flake on film.
    expect(container.querySelector("svg")).toBeNull();
    expect(container.querySelector("canvas")).toBeNull();
  });

  it("'Enter the demo' reveals the reframed picker via the iris class", () => {
    render(<FrontDoor />);
    expect(screen.getByTestId("entry-screen")).toBeTruthy();
    expect(screen.queryByTestId("identity-picker")).toBeNull();
    fireEvent.click(screen.getByTestId("entry-cta"));
    // The picker replaces the entry inside the ONE transition wrapper.
    expect(screen.queryByTestId("entry-screen")).toBeNull();
    const wrapper = screen.getByTestId("front-door-picker");
    expect(wrapper.className).toContain("ap-entry-iris");
    expect(screen.getByRole("heading", { name: "Choose a work identity" })).toBeTruthy();
  });
});

describe("showreel picker reframe", () => {
  function openPicker() {
    render(<FrontDoor />);
    fireEvent.click(screen.getByTestId("entry-cta"));
  }

  it("labels are honest: demo access, no functional credential claims", () => {
    openPicker();
    const demoLine = screen.getByTestId("identity-picker-demo-line").textContent ?? "";
    expect(demoLine).toContain("Demo access — no account, no password.");
    expect(demoLine).toContain("exactly what that person is authorized to see");
    const picker = screen.getByTestId("identity-picker");
    expect(picker.textContent ?? "").not.toMatch(/google|gmail|oauth|sign in with/i);
    expect(picker.querySelector('input[type="password"]')).toBeNull();
  });

  it("Felix (p060) is the visually-primary card with unchanged behavior", () => {
    openPicker();
    const felix = screen.getByTestId("identity-option-p060");
    // Visual primacy: the focus surface treatment.
    expect(felix.className).toContain("ap-focus-surface");
    // Behavior identical to every card: a plain identity link.
    expect(felix.getAttribute("href")).toBe("/me?as=p060");
    const others = ["p088", "p_void"].map((id) => screen.getByTestId(`identity-option-${id}`));
    for (const card of others) {
      expect(card.className).toContain("ap-card");
      expect(card.className).not.toContain("ap-focus-surface");
    }
  });
});
