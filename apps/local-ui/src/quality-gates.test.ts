import { describe, expect, test } from "bun:test";

import {
  accessibilityChecks,
  browserProfiles,
  browserScenarios,
  breakpoints,
  buildUiQualityEvidence,
  criticalJourneys,
  technicalDataFields,
  validateUiQualityEvidence,
} from "./quality-gates.ts";

const requiredCriticalJourneys = [
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

const requiredAccessibilityChecks = [
  "automated",
  "keyboard_only",
  "focus_order",
  "screen_reader_spot",
  "reduced_motion",
  "color_blindness",
  "no_color_only",
] as const;

const requiredScenarios = [
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

describe("local UI quality gates", () => {
  test("accessibility evidence covers every critical journey and failure locator", () => {
    const evidence = buildUiQualityEvidence();

    expect(criticalJourneys).toEqual(requiredCriticalJourneys);
    expect(accessibilityChecks).toEqual(requiredAccessibilityChecks);
    expect(validateUiQualityEvidence(evidence)).toEqual([]);

    for (const journey of requiredCriticalJourneys) {
      const journeyEvidence = evidence.accessibility.find((entry) => entry.journey === journey);
      expect(journeyEvidence).toBeDefined();
      expect(journeyEvidence?.component_state).toBe("baseline");
      expect(journeyEvidence?.failure_locator).toBe(`${journey}:baseline`);
      expect(journeyEvidence?.checks.map((check) => check.kind)).toEqual(
        requiredAccessibilityChecks,
      );
      expect(journeyEvidence?.checks.every((check) => check.status === "passed")).toBe(true);
    }
  });

  test("responsive fixtures preserve pane model, event order, and selected detail", () => {
    const evidence = buildUiQualityEvidence();

    expect(breakpoints.map((breakpoint) => breakpoint.name)).toEqual([
      "mobile",
      "tablet",
      "desktop",
      "wide_desktop",
    ]);
    expect(breakpoints.map((breakpoint) => breakpoint.panes)).toEqual([1, 2, 3, 3]);

    for (const responsive of evidence.responsive) {
      expect(responsive.trace_event_order).toEqual([1001, 1002, 1003, 1004]);
      expect(responsive.delivery_state_visible).toBe(true);
      expect(responsive.failure_state_visible).toBe(true);
      expect(responsive.selected_detail_stable).toBe("evt-1003");
      expect(responsive.hidden_states).toEqual([]);
    }
  });

  test("long technical text stays readable, copyable where needed, and labelled", () => {
    const evidence = buildUiQualityEvidence();

    expect(technicalDataFields.map((field) => field.name)).toEqual([
      "agent_id",
      "correlation_id",
      "subject",
      "timestamp",
      "payload_summary",
      "cli_command",
      "error_message",
    ]);

    for (const field of evidence.technical_text) {
      expect(field.readable_at).toEqual(["mobile", "tablet", "desktop", "wide_desktop"]);
      expect(field.accessible_label.length).toBeGreaterThan(0);
      expect(field.breaks_timeline_chronology).toBe(false);
      expect(field.hides_controls_or_status).toBe(false);
      if (field.copyable) {
        expect(field.copy_label).toMatch(/^Copy /);
      }
    }
  });

  test("browser matrix covers every supported browser and required scenario", () => {
    const evidence = buildUiQualityEvidence();

    expect(browserProfiles.map((browser) => browser.engine)).toEqual([
      "chromium",
      "firefox",
      "webkit_safari",
    ]);
    expect(browserProfiles.every((browser) => browser.channel === "current_stable")).toBe(true);
    expect(browserScenarios).toEqual(requiredScenarios);

    for (const browser of browserProfiles) {
      const coveredScenarios = evidence.browser
        .filter((entry) => entry.engine === browser.engine)
        .map((entry) => entry.scenario);
      expect(coveredScenarios).toEqual(requiredScenarios);
    }

    for (const browserEvidence of evidence.browser) {
      expect(["passed", "unsupported_state", "defect"]).toContain(browserEvidence.status);
      if (browserEvidence.status !== "passed") {
        expect(browserEvidence.evidence).toMatch(/^(unsupported|defect):/);
      }
    }
  });

  test("readiness evidence is stable and missing tooling fails explicitly", async () => {
    const evidence = buildUiQualityEvidence();
    const fixture = await Bun.file(
      new URL("../../../fixtures/ui/quality-readiness.json", import.meta.url),
    ).json();

    expect(evidence).toEqual(fixture);
    expect(evidence.results.map((result) => result.area)).toEqual([
      "accessibility",
      "responsive",
      "browser",
      "offline_asset",
      "critical_journey",
    ]);

    const missingBrowserTooling = {
      ...evidence,
      required_tooling: evidence.required_tooling.map((tool) =>
        tool.name === "browser_fixture_matrix" ? { ...tool, available: false } : tool,
      ),
    };
    const missingAccessibilityTooling = {
      ...evidence,
      required_tooling: evidence.required_tooling.map((tool) =>
        tool.name === "accessibility_fixture_audit" ? { ...tool, available: false } : tool,
      ),
    };

    expect(validateUiQualityEvidence(missingBrowserTooling)).toContain(
      "required tooling unavailable: browser_fixture_matrix",
    );
    expect(validateUiQualityEvidence(missingAccessibilityTooling)).toContain(
      "required tooling unavailable: accessibility_fixture_audit",
    );
  });
});
