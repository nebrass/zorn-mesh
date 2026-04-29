// Project-owned primitive wrappers for the v0.1 local UI shell.
//
// Each primitive is intentionally a thin wrapper so future stories can swap
// the underlying Radix/Tailwind composition without changing call sites. The
// wrappers establish the contract; deterministic fixture states for each one
// live under ../fixtures/.

export const primitives = [
  "Button",
  "Input",
  "Dialog",
  "Popover",
  "Tooltip",
  "Tabs",
  "Menu",
  "Toast",
  "Badge",
  "Panel",
  "Layout",
] as const;

export type Primitive = (typeof primitives)[number];

export const fixtureStates = [
  "baseline",
  "loading",
  "error",
  "disabled",
  "focus",
  "reduced_motion",
] as const;

export type FixtureState = (typeof fixtureStates)[number];

export interface FixtureContract {
  primitive: Primitive;
  state: FixtureState;
}

export function fixtureMatrix(): FixtureContract[] {
  const matrix: FixtureContract[] = [];
  for (const primitive of primitives) {
    for (const state of fixtureStates) {
      matrix.push({ primitive, state });
    }
  }
  return matrix;
}
