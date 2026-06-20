import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

afterEach(() => {
  cleanup();
  // FC-A1: don't let a minted session leak between tests.
  if (typeof sessionStorage !== "undefined") sessionStorage.clear();
});
