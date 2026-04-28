//! Story 6.1 verification tests.
//!
//! These tests guard the local UI architecture contract that Stories 6.2-6.9
//! depend on. They read the architecture amendment, the package manifest, and
//! the scaffold modules under `apps/local-ui/` and assert the documented
//! invariants. CI does not need a Bun runtime: the structural contract is
//! checked from Rust against the source files.

use std::{fs, path::PathBuf};

const FRAMEWORK_PIN: &str =
    "Bun-managed React app, locally bundled and offline-served by the daemon UI gateway on loopback only";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn read(path: &str) -> String {
    let full = workspace_root().join(path);
    fs::read_to_string(&full).unwrap_or_else(|err| {
        panic!("failed to read {}: {err}", full.display());
    })
}

#[test]
fn architecture_amendment_cites_existing_v01_sections_and_pins_framework_wording() {
    let body = read("docs/architecture/local-ui-amendment.md");

    for required_anchor in [
        "Architecture supersession note",
        "Local UI scope decision",
        "Local web companion UI",
    ] {
        assert!(
            body.contains(required_anchor),
            "amendment must cite section '{required_anchor}'"
        );
    }

    assert!(
        body.contains(FRAMEWORK_PIN),
        "amendment must pin the v0.1 framework wording verbatim"
    );

    for forbidden in [
        "Node-served runtime",
        "Hosted serving model",
        "Next.js server features",
        "Remote browser assets",
    ] {
        assert!(
            body.contains(forbidden),
            "amendment must explicitly record '{forbidden}' as out of scope"
        );
    }

    for nfr in ["NFR-S8", "NFR-S10", "NFR-S11", "NFR-C7"] {
        assert!(
            body.contains(nfr),
            "amendment must reference {nfr} for out-of-scope mapping"
        );
    }
}

#[test]
fn local_ui_package_manifest_declares_bun_only_toolchain() {
    let body = read("apps/local-ui/package.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&body).expect("package.json parses as JSON");

    assert_eq!(
        manifest["zornmesh"]["framework_pin"].as_str(),
        Some(FRAMEWORK_PIN),
        "package.json must pin the framework wording"
    );
    assert_eq!(manifest["zornmesh"]["no_node_served_runtime"], true);
    assert_eq!(manifest["zornmesh"]["no_hosted_serving_model"], true);
    assert_eq!(manifest["zornmesh"]["no_nextjs_server_features"], true);
    assert_eq!(manifest["zornmesh"]["no_remote_browser_assets"], true);
    assert_eq!(manifest["zornmesh"]["supported_package_manager"], "bun");

    assert!(
        manifest["packageManager"]
            .as_str()
            .unwrap_or_default()
            .starts_with("bun@"),
        "packageManager field must declare bun"
    );

    let lower = body.to_ascii_lowercase();
    for forbidden in [
        "next.js",
        "\"next\":",
        "ssr",
        "server-runtime",
        "node-server",
    ] {
        assert!(
            !lower.contains(forbidden),
            "package.json must not introduce {forbidden}"
        );
    }
}

#[test]
fn local_ui_scope_module_declares_in_and_out_of_scope_surfaces() {
    let body = read("apps/local-ui/src/scope.ts");
    for in_scope in [
        "observe",
        "inspect",
        "reconnect_backfill",
        "safe_direct_send",
        "safe_broadcast",
        "outcome_review",
        "cli_handoff",
    ] {
        assert!(
            body.contains(&format!("\"{in_scope}\"")),
            "in-scope surface '{in_scope}' must be listed"
        );
    }
    for out_of_scope in [
        "hosted_cloud_dashboard",
        "lan_public_console",
        "accounts_teams",
        "full_chat_workspace",
        "workflow_editor",
        "remote_browser_assets",
        "external_runtime_services",
    ] {
        assert!(
            body.contains(&format!("\"{out_of_scope}\"")),
            "out-of-scope surface '{out_of_scope}' must be listed"
        );
    }
    assert!(
        body.contains("OutOfScopeError"),
        "scope module must export the explicit out-of-scope error"
    );
}

#[test]
fn local_ui_taxonomies_module_enumerates_required_state_taxonomies() {
    let body = read("apps/local-ui/src/taxonomies.ts");
    for taxonomy in [
        "agentStatus",
        "deliveryState",
        "traceCompleteness",
        "daemonHealth",
        "trustPosture",
    ] {
        assert!(
            body.contains(taxonomy),
            "taxonomies module must export '{taxonomy}'"
        );
    }
    for fallback_state in ["\"unknown\""] {
        assert!(
            body.contains(fallback_state),
            "taxonomies must declare an explicit unknown fallback state"
        );
    }
    assert!(
        body.contains("fallbackLabel"),
        "taxonomies must export an unknown-fallback resolver"
    );
}

#[test]
fn local_ui_tokens_module_declares_required_design_tokens() {
    let body = read("apps/local-ui/src/tokens.ts");
    for required in [
        "graphite-900",
        "graphite-800",
        "charcoal-700",
        "electric-blue-500",
        "cyan-local-400",
        "success",
        "warning",
        "error",
        "neutral",
        "JetBrains Mono",
        "focus",
    ] {
        assert!(
            body.contains(required),
            "tokens module must declare '{required}'"
        );
    }
}

#[test]
fn local_ui_components_module_seeds_primitive_and_fixture_state_matrix() {
    let body = read("apps/local-ui/src/components/index.ts");
    for primitive in [
        "Button", "Input", "Dialog", "Popover", "Tooltip", "Tabs", "Menu", "Toast", "Badge",
        "Panel", "Layout",
    ] {
        assert!(
            body.contains(&format!("\"{primitive}\"")),
            "primitive '{primitive}' must appear in fixture matrix"
        );
    }
    for state in [
        "baseline",
        "loading",
        "error",
        "disabled",
        "focus",
        "reduced_motion",
    ] {
        assert!(
            body.contains(&format!("\"{state}\"")),
            "fixture state '{state}' must appear in matrix"
        );
    }
}

#[test]
fn local_ui_quality_gate_is_wired_into_fixtures_and_workspace_tests() {
    let package_body = read("apps/local-ui/package.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&package_body).expect("package.json parses as JSON");

    assert_eq!(
        manifest["scripts"]["test:quality"].as_str(),
        Some("bun test src/quality-gates.test.ts"),
        "local UI package must expose the quality gate test command"
    );
    assert_eq!(
        manifest["scripts"]["quality-evidence"].as_str(),
        Some("bun run src/quality-gates.ts"),
        "local UI package must emit stable readiness evidence"
    );

    let quality_gate = read("apps/local-ui/src/quality-gates.ts");
    for required in [
        "accessibility_fixture_audit",
        "browser_fixture_matrix",
        "mobile",
        "tablet",
        "desktop",
        "wide_desktop",
        "chromium",
        "firefox",
        "webkit_safari",
        "offline_asset",
        "critical_journey",
    ] {
        assert!(
            quality_gate.contains(required),
            "quality gate source must declare '{required}'"
        );
    }

    let fixture = read("fixtures/ui/quality-readiness.json");
    for required in [
        "\"accessibility\"",
        "\"responsive\"",
        "\"browser\"",
        "\"offline_asset\"",
        "\"critical_journey\"",
        "\"unsupported_state\"",
        "\"defect\"",
    ] {
        assert!(
            fixture.contains(required),
            "quality readiness fixture must record '{required}'"
        );
    }

    let xtask = read("crates/zornmesh-xtask/src/main.rs");
    assert!(
        xtask.contains("root.join(\"apps/local-ui\")"),
        "cargo xtask test must run local UI Bun tests"
    );
}
