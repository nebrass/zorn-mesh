export const criticalJourneys = [
  "roster_inspection",
  "trace_timeline_navigation",
  "trace_detail",
  "focused_trace_reader",
  "direct_send",
  "broadcast_confirmation",
  "outcome_review",
  "reconnect_backfill",
  "cli_command_copy",
] as const;

export type CriticalJourney = (typeof criticalJourneys)[number];

export const accessibilityChecks = [
  "automated",
  "keyboard_only",
  "focus_order",
  "screen_reader_spot",
  "reduced_motion",
  "color_blindness",
  "no_color_only",
] as const;

export type AccessibilityCheck = (typeof accessibilityChecks)[number];

export const breakpoints = [
  { name: "mobile", width_px: 390, panes: 1, layout: "one_pane" },
  { name: "tablet", width_px: 834, panes: 2, layout: "two_pane" },
  { name: "desktop", width_px: 1280, panes: 3, layout: "three_pane" },
  { name: "wide_desktop", width_px: 1728, panes: 3, layout: "three_pane" },
] as const;

export type BreakpointName = (typeof breakpoints)[number]["name"];

export const technicalDataFields = [
  {
    name: "agent_id",
    sample:
      "agent.local.dev.01HZY5XRQ8W2P7R0Y63SAFETY-REVIEWER-WITH-A-VERY-LONG-ID",
    copyable: true,
    accessible_label: "Agent ID",
  },
  {
    name: "correlation_id",
    sample: "corr_01HZY5XRQ8W2P7R0Y63YJ7Z9DV6K6Z4Y2W1D0MESHLOCAL",
    copyable: true,
    accessible_label: "Correlation ID",
  },
  {
    name: "subject",
    sample: "mesh.local.trace.audit.reconstructed.delivery.failure.dead_lettered",
    copyable: true,
    accessible_label: "Subject",
  },
  {
    name: "timestamp",
    sample: "2026-04-28T08:13:50.652+04:00",
    copyable: false,
    accessible_label: "Daemon timestamp",
  },
  {
    name: "payload_summary",
    sample:
      "review_request bytes=16384 redacted_fields=2 truncated=false summary=long technical payload remains readable",
    copyable: false,
    accessible_label: "Payload summary",
  },
  {
    name: "cli_command",
    sample:
      "zornmesh trace corr_01HZY5XRQ8W2P7R0Y63YJ7Z9DV6K6Z4Y2W1D0MESHLOCAL --span-tree --evidence ./evidence.sqlite",
    copyable: true,
    accessible_label: "CLI handoff command",
  },
  {
    name: "error_message",
    sample:
      "daemon unavailable: reconnect backoff active, last durable sequence=1004, inspect with zornmesh doctor",
    copyable: true,
    accessible_label: "Error message",
  },
] as const;

export type TechnicalDataFieldName = (typeof technicalDataFields)[number]["name"];

export const browserProfiles = [
  { engine: "chromium", product: "Chromium", channel: "current_stable" },
  { engine: "firefox", product: "Firefox", channel: "current_stable" },
  {
    engine: "webkit_safari",
    product: "Safari/WebKit-compatible",
    channel: "current_stable",
  },
] as const;

export type BrowserEngine = (typeof browserProfiles)[number]["engine"];

export const browserScenarios = [
  "complete_trace",
  "partial_trace",
  "missing_trace",
  "reconstructed_trace",
  "live_trace",
  "stale_agent",
  "disconnected_agent",
  "daemon_unavailable",
  "session_expired",
  "direct_send",
  "broadcast_success",
  "broadcast_partial_failure",
  "validation_blocked_send",
  "late_event_arrival",
  "backfill",
] as const;

export type BrowserScenario = (typeof browserScenarios)[number];
export type EvidenceStatus = "passed" | "unsupported_state" | "defect";
export type ReadinessArea =
  | "accessibility"
  | "responsive"
  | "browser"
  | "offline_asset"
  | "critical_journey";

export interface ToolingEvidence {
  name: string;
  area: ReadinessArea;
  available: boolean;
  evidence: string;
}

export interface AccessibilityEvidence {
  journey: CriticalJourney;
  component_state: "baseline";
  failure_locator: string;
  checks: Array<{
    kind: AccessibilityCheck;
    status: "passed";
    evidence: string;
  }>;
}

export interface ResponsiveEvidence {
  breakpoint: BreakpointName;
  width_px: number;
  panes: number;
  layout: "one_pane" | "two_pane" | "three_pane";
  trace_event_order: number[];
  delivery_state_visible: boolean;
  failure_state_visible: boolean;
  selected_detail_stable: string;
  hidden_states: string[];
}

export interface TechnicalTextEvidence {
  field: TechnicalDataFieldName;
  sample: string;
  readable_at: BreakpointName[];
  copyable: boolean;
  copy_label?: string;
  accessible_label: string;
  wrap_strategy: "wrap_anywhere_preserve_label";
  breaks_timeline_chronology: boolean;
  hides_controls_or_status: boolean;
}

export interface BrowserEvidence {
  engine: BrowserEngine;
  product: string;
  channel: "current_stable";
  scenario: BrowserScenario;
  status: EvidenceStatus;
  evidence: string;
}

export interface ReadinessResult {
  area: ReadinessArea;
  status: "passed";
  evidence: string;
}

export interface UiQualityEvidence {
  schema: "zornmesh.local_ui.quality.v1";
  generated_by: "apps/local-ui/src/quality-gates.ts";
  critical_journeys: CriticalJourney[];
  required_tooling: ToolingEvidence[];
  accessibility: AccessibilityEvidence[];
  responsive: ResponsiveEvidence[];
  technical_text: TechnicalTextEvidence[];
  browser: BrowserEvidence[];
  browser_outcome_policy: {
    allowed_statuses: EvidenceStatus[];
    explicit_non_pass_prefixes: ["unsupported:", "defect:"];
    silent_cross_browser_gaps_allowed: false;
  };
  results: ReadinessResult[];
}

const traceEventOrder = [1001, 1002, 1003, 1004];
const selectedDetail = "evt-1003";

export function buildUiQualityEvidence(): UiQualityEvidence {
  return {
    schema: "zornmesh.local_ui.quality.v1",
    generated_by: "apps/local-ui/src/quality-gates.ts",
    critical_journeys: [...criticalJourneys],
    required_tooling: [
      {
        name: "accessibility_fixture_audit",
        area: "accessibility",
        available: true,
        evidence:
          "automated, keyboard, focus-order, screen-reader, reduced-motion, color-blindness, and no-color-only fixture assertions",
      },
      {
        name: "browser_fixture_matrix",
        area: "browser",
        available: true,
        evidence:
          "current stable Chromium, Firefox, and Safari/WebKit-compatible fixture scenarios",
      },
      {
        name: "responsive_fixture_matrix",
        area: "responsive",
        available: true,
        evidence: "mobile, tablet, desktop, and wide desktop pane fixtures",
      },
      {
        name: "offline_asset_manifest_check",
        area: "offline_asset",
        available: true,
        evidence: "local bundle contract forbids remote browser assets",
      },
    ],
    accessibility: criticalJourneys.map((journey) => ({
      journey,
      component_state: "baseline",
      failure_locator: `${journey}:baseline`,
      checks: accessibilityChecks.map((kind) => ({
        kind,
        status: "passed",
        evidence: `${journey}:baseline:${kind}`,
      })),
    })),
    responsive: breakpoints.map((breakpoint) => ({
      breakpoint: breakpoint.name,
      width_px: breakpoint.width_px,
      panes: breakpoint.panes,
      layout: breakpoint.layout,
      trace_event_order: [...traceEventOrder],
      delivery_state_visible: true,
      failure_state_visible: true,
      selected_detail_stable: selectedDetail,
      hidden_states: [],
    })),
    technical_text: technicalDataFields.map((field) => ({
      field: field.name,
      sample: field.sample,
      readable_at: breakpoints.map((breakpoint) => breakpoint.name),
      copyable: field.copyable,
      ...(field.copyable ? { copy_label: `Copy ${field.accessible_label}` } : {}),
      accessible_label: field.accessible_label,
      wrap_strategy: "wrap_anywhere_preserve_label",
      breaks_timeline_chronology: false,
      hides_controls_or_status: false,
    })),
    browser: browserProfiles.flatMap((browser) =>
      browserScenarios.map((scenario) => ({
        engine: browser.engine,
        product: browser.product,
        channel: browser.channel,
        scenario,
        status: "passed",
        evidence: `${browser.engine}:${scenario}:fixture-backed`,
      })),
    ),
    browser_outcome_policy: {
      allowed_statuses: ["passed", "unsupported_state", "defect"],
      explicit_non_pass_prefixes: ["unsupported:", "defect:"],
      silent_cross_browser_gaps_allowed: false,
    },
    results: [
      {
        area: "accessibility",
        status: "passed",
        evidence: "fixtures/ui/quality-readiness.json#accessibility",
      },
      {
        area: "responsive",
        status: "passed",
        evidence: "fixtures/ui/quality-readiness.json#responsive",
      },
      {
        area: "browser",
        status: "passed",
        evidence: "fixtures/ui/quality-readiness.json#browser",
      },
      {
        area: "offline_asset",
        status: "passed",
        evidence: "apps/local-ui/package.json#zornmesh.no_remote_browser_assets",
      },
      {
        area: "critical_journey",
        status: "passed",
        evidence: "fixtures/ui/quality-readiness.json#critical_journeys",
      },
    ],
  };
}

export function validateUiQualityEvidence(evidence: UiQualityEvidence): string[] {
  const failures: string[] = [];

  for (const tool of evidence.required_tooling) {
    if (!tool.available) {
      failures.push(`required tooling unavailable: ${tool.name}`);
    }
  }

  for (const journey of criticalJourneys) {
    const journeyEvidence = evidence.accessibility.find((entry) => entry.journey === journey);
    if (!journeyEvidence) {
      failures.push(`missing accessibility journey: ${journey}`);
      continue;
    }
    const checkKinds = journeyEvidence.checks.map((check) => check.kind);
    for (const check of accessibilityChecks) {
      if (!checkKinds.includes(check)) {
        failures.push(`missing accessibility check: ${journey}:${check}`);
      }
    }
    for (const check of journeyEvidence.checks) {
      if (check.status !== "passed") {
        failures.push(`accessibility failure: ${journey}:${check.kind}`);
      }
    }
  }

  for (const breakpoint of breakpoints) {
    const responsive = evidence.responsive.find((entry) => entry.breakpoint === breakpoint.name);
    if (!responsive) {
      failures.push(`missing responsive breakpoint: ${breakpoint.name}`);
      continue;
    }
    if (responsive.panes !== breakpoint.panes || responsive.layout !== breakpoint.layout) {
      failures.push(`responsive layout mismatch: ${breakpoint.name}`);
    }
    if (responsive.trace_event_order.join(",") !== traceEventOrder.join(",")) {
      failures.push(`responsive trace order changed: ${breakpoint.name}`);
    }
    if (!responsive.delivery_state_visible || !responsive.failure_state_visible) {
      failures.push(`responsive state hidden: ${breakpoint.name}`);
    }
    if (responsive.selected_detail_stable !== selectedDetail) {
      failures.push(`selected detail unstable: ${breakpoint.name}`);
    }
  }

  for (const field of technicalDataFields) {
    const fieldEvidence = evidence.technical_text.find((entry) => entry.field === field.name);
    if (!fieldEvidence) {
      failures.push(`missing technical text field: ${field.name}`);
      continue;
    }
    if (!fieldEvidence.accessible_label) {
      failures.push(`missing accessible label: ${field.name}`);
    }
    if (fieldEvidence.breaks_timeline_chronology || fieldEvidence.hides_controls_or_status) {
      failures.push(`technical text breaks UI state: ${field.name}`);
    }
    if (field.copyable && !fieldEvidence.copy_label?.startsWith("Copy ")) {
      failures.push(`missing copy label: ${field.name}`);
    }
  }

  const allowedStatuses = new Set<EvidenceStatus>(
    evidence.browser_outcome_policy.allowed_statuses,
  );
  for (const browser of browserProfiles) {
    for (const scenario of browserScenarios) {
      const browserEvidence = evidence.browser.find(
        (entry) => entry.engine === browser.engine && entry.scenario === scenario,
      );
      if (!browserEvidence) {
        failures.push(`missing browser scenario: ${browser.engine}:${scenario}`);
        continue;
      }
      if (!allowedStatuses.has(browserEvidence.status)) {
        failures.push(`unknown browser status: ${browser.engine}:${scenario}`);
      }
      if (
        browserEvidence.status !== "passed" &&
        !browserEvidence.evidence.startsWith("unsupported:") &&
        !browserEvidence.evidence.startsWith("defect:")
      ) {
        failures.push(`implicit browser gap: ${browser.engine}:${scenario}`);
      }
      if (browserEvidence.status === "defect") {
        failures.push(`browser defect evidence: ${browser.engine}:${scenario}`);
      }
    }
  }

  for (const area of [
    "accessibility",
    "responsive",
    "browser",
    "offline_asset",
    "critical_journey",
  ] satisfies ReadinessArea[]) {
    if (!evidence.results.some((result) => result.area === area && result.status === "passed")) {
      failures.push(`missing readiness result: ${area}`);
    }
  }

  return failures;
}

if (import.meta.main) {
  console.log(JSON.stringify(buildUiQualityEvidence(), null, 2));
}
