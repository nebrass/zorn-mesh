// Entry point for the v0.1 local UI shell.
//
// This file documents the bootstrapping contract; future stories implement
// the renderer wiring. The shell never makes external runtime requests and
// never declares Next.js server features.

import { taxonomies } from "./taxonomies.ts";
import { tokens } from "./tokens.ts";
import { fixtureMatrix } from "./components/index.ts";
import { inScopeSurfaces } from "./scope.ts";

export const shellManifest = {
  framework_pin:
    "Bun-managed React app, locally bundled and offline-served by the daemon UI gateway on loopback only",
  taxonomies,
  tokens,
  in_scope_surfaces: inScopeSurfaces,
  fixture_count: fixtureMatrix().length,
};
