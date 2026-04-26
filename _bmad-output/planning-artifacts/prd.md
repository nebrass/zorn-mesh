---
stepsCompleted: ['step-01-init', 'step-02-discovery', 'step-02b-vision', 'step-02c-executive-summary']
inputDocuments:
  - "_bmad-output/project-context.md"
  - "_bmad-output/planning-artifacts/product-brief-zorn-mesh.md"
  - "_bmad-output/planning-artifacts/product-brief-zorn-mesh-distillate.md"
documentCounts:
  briefs: 1
  distillates: 1
  research: 0
  brainstorming: 0
  projectDocs: 0
  projectContext: 1
projectStatus: 'greenfield'
workflowType: 'prd'
project_name: 'zorn-mesh'
user_name: 'Nebrass'
date: '2026-04-26'
classification:
  projectType: 'developer_tool'
  projectTypeAddenda: ['cli_tool', 'system_daemon_informal']
  domain: 'developer_infra'  # informal — supersedes BMad CSV's 'general'; complexity override required
  domainCsvFallback: 'general'  # for any tooling that reads CSV taxonomy
  complexity: 'high'
  projectContext: 'greenfield'
  primaryJob: 'observability_of_broken_multi_agent_system'  # "know exactly what happened, and when"
  primaryPersona: 'individual_developer'  # buyer = user at v0.1
  futurePersona: 'platform_engineer'  # buyer ≠ user, flagged so v0.1 decisions don't quietly foreclose
  launchGate: 'mcp_stdio_bridge_with_major_host'  # forcing function — Claude Desktop or Cursor at v0.1
  mandatoryAddenda:
    - 'operations_and_lifecycle_section'  # daemon state machine, socket ownership, SQLite contract, upgrade protocol, crash recovery, zornmesh doctor spec
    - 'compliance_and_regulatory_section'  # EU AI Act, NIST AI RMF, NIST SP 800-218A, GDPR, SOC 2, CISA SBOM
    - 'stakeholder_map_section'  # enterprise security reviewers, agent-runtime vendors, protocol stewards, OSS contributors, enterprise buyers, regulatory watch
  classificationProvenance: 'party-mode review by Mary (BA), John (PM), Winston (Architect) — 8 enrichments accepted by user'
---

# Product Requirements Document — zorn-mesh

**Author:** Nebrass
**Date:** 2026-04-26
**Status:** In progress (Step 1 of 11 complete)
