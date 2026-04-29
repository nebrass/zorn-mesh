// Scope-boundary manifest for the v0.1 local UI.
//
// In-scope surfaces are the ONLY routes/actions Stories 6.2-6.9 may build.
// Out-of-scope surfaces must either be absent from the shell or return an
// explicit out-of-scope error when reached. Adding a surface here without
// updating docs/architecture/local-ui-amendment.md is a contract violation.

export const inScopeSurfaces = [
  "observe",
  "inspect",
  "reconnect_backfill",
  "safe_direct_send",
  "safe_broadcast",
  "outcome_review",
  "cli_handoff",
] as const;

export const outOfScopeSurfaces = [
  "hosted_cloud_dashboard",
  "lan_public_console",
  "accounts_teams",
  "full_chat_workspace",
  "workflow_editor",
  "remote_browser_assets",
  "external_runtime_services",
] as const;

export type InScopeSurface = (typeof inScopeSurfaces)[number];
export type OutOfScopeSurface = (typeof outOfScopeSurfaces)[number];

export class OutOfScopeError extends Error {
  constructor(public surface: OutOfScopeSurface) {
    super(
      `surface '${surface}' is explicitly out of scope for v0.1 local UI; see docs/architecture/local-ui-amendment.md`,
    );
  }
}

export function assertInScope(surface: string): asserts surface is InScopeSurface {
  if (!(inScopeSurfaces as readonly string[]).includes(surface)) {
    if ((outOfScopeSurfaces as readonly string[]).includes(surface)) {
      throw new OutOfScopeError(surface as OutOfScopeSurface);
    }
    throw new Error(`surface '${surface}' is not declared in v0.1 scope manifest`);
  }
}
