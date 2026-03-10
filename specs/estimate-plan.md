# chub-rs Estimate and Execution Plan

## Estimation Basis

- Source specs: `m0-skeleton`, `m1-core-read`, `m2-write-path`, `m3-build`, `m4-mcp-server`
- Estimation unit: agent rounds
- Conversion: `1 round ~= 3-4 minutes`
- Dependency chain: `m0 -> (m1, m2, m3) -> m4`
- Main risk driver: JS parity verification and lifecycle retry count, not raw code volume

## Task Estimates

| Spec | Scenarios | Decisions | Allowed Paths | Forbidden Rules | Estimated Rounds | Wallclock |
|------|-----------|-----------|---------------|-----------------|------------------|-----------|
| `m0-skeleton` | 8 | 5 | 7 | 2 | 19 | 57 - 76 min |
| `m1-core-read` | 12 | 9 | 9 | 2 | 37 | 111 - 148 min |
| `m2-write-path` | 12 | 7 | 10 | 2 | 43 | 129 - 172 min |
| `m3-build` | 11 | 7 | 4 | 2 | 35 | 105 - 140 min |
| `m4-mcp-server` | 9 | 6 | 4 | 2 | 35 | 105 - 140 min |

## Module Notes

- `m0-skeleton`: low risk foundation work, but it must lock down CLI/help/version/output contracts early.
- `m1-core-read`: first major parity milestone; `get` behavior and JSON shape are the main retry source.
- `m2-write-path`: highest risk because it combines file I/O, time, network, freshness checks, and non-blocking telemetry semantics.
- `m3-build`: moderate risk, mostly around deterministic output and exact `registry/search-index` compatibility.
- `m4-mcp-server`: moderate risk, mainly protocol cleanliness and stdout/stderr discipline.

## Capacity Summary

- Total estimated rounds: `169`
- Total serial wallclock: `507 - 676 minutes`
- Total serial wallclock in hours: `8.5 - 11.3 hours`

## Weekly Execution Plan

### Option A: One focused week, one primary agent

| Week | Goal | Tasks | Output |
|------|------|-------|--------|
| Week 1 | Foundation + core parity | `m0`, `m1` | CLI skeleton, config/output layer, registry/search/get parity |
| Week 2 | Stateful behaviors | `m2` | annotations, cache, update, feedback, telemetry, client id |
| Week 3 | Content pipeline + MCP | `m3`, `m4` | build pipeline, deterministic artifacts, MCP server parity |

### Option B: Compressed two-week plan

| Week | Goal | Tasks | Output |
|------|------|-------|--------|
| Week 1 | Foundation + read path | `m0`, `m1` | runnable CLI and core read workflow |
| Week 2 | Write/build/MCP | `m2`, `m3`, `m4` | write path parity, build pipeline, MCP server |

## Parallel Agent Plan

### 1 agent

| Order | Tasks | Notes |
|------|-------|-------|
| 1 | `m0` | mandatory foundation |
| 2 | `m1` | establishes shared lib behavior |
| 3 | `m2` | stateful and network-heavy follow-up |
| 4 | `m3` | build pipeline after foundation is stable |
| 5 | `m4` | best after `m1` and `m2` settle |

- Expected wallclock: `8.5 - 11.3 hours`

### 2 agents

| Phase | Agent A | Agent B | Notes |
|-------|---------|---------|-------|
| Phase 1 | `m0` | standby / fixture prep | avoid parallel drift before foundation exists |
| Phase 2 | `m1` | `m3` | good split: runtime read path vs build pipeline |
| Phase 3 | `m2` | test/golden fixture hardening | `m2` is too stateful to split casually |
| Phase 4 | `m4` | parity regression support | MCP should reuse stabilized lib behavior |

- Expected wallclock: `5.5 - 7.5 hours`

### 3 agents

| Phase | Agent A | Agent B | Agent C | Notes |
|-------|---------|---------|---------|-------|
| Phase 1 | `m0` | standby | standby | still do foundation first |
| Phase 2 | `m1` | `m2` | `m3` | highest throughput after `m0` |
| Phase 3 | `m4` | parity fixes | fixture/golden tightening | final convergence phase |

- Expected wallclock: `4.0 - 5.5 hours`
- Coordination cost rises sharply after 3 agents because `m1`/`m2`/`m4` share runtime contracts.

## Recommended Plan

- Default recommendation: `2 agents`, `2 weeks`
- Week 1: finish `m0 + m1`, and let the second agent drive `m3` once the shared output/config contracts are stable.
- Week 2: finish `m2`, then land `m4` on top of the stabilized lib layer.
- Keep `m2` and `m4` from diverging on telemetry/output contracts by running shared parity tests before merge.

## Review Checkpoints

- After `m0`: confirm CLI help/version/output contract is fixed before any feature work.
- After `m1`: freeze read-path JSON shapes and `get` behavior before starting MCP wrapping.
- After `m2`: freeze telemetry/update/cache semantics before broad integration.
- After `m3`: compare generated artifacts against golden fixtures.
- After `m4`: run end-to-end MCP parity and stdout cleanliness checks.
