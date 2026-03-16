# Contributing Guide

This project is designed for open-source contribution programs. Most enhancements can be shipped as focused, reviewable pull requests with tests.

## Development Workflow

1. Create an issue (or pick one from GitHub)
2. Add or update tests first when practical
3. Implement the change with minimal surface area
4. Run:
   - `npm run test:rust`
   - `npm run typecheck`
   - `npm test`

## Good First Issues

- Reward rounding/dust handling improvements
- Gas optimization (reduce storage reads/writes)
- Additional security validation (pausing, caps, allowlists)
- Better event indexing keys (topics) for off-chain consumers

## Medium/Large Scope Issues

- Governance and admin handover patterns
- Multiple reward campaigns and time-based emissions
- Upgrade patterns that align with Soroban best practices
- Formal specification + property testing

## Code Organization

Contract modules:
- [lib.rs](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/contracts/vault-contract/src/lib.rs) — public interface and high-level flow
- [storage.rs](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/contracts/vault-contract/src/storage.rs) — storage schema and reward math
- [events.rs](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/contracts/vault-contract/src/events.rs) — event definitions and emitters
- [errors.rs](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/contracts/vault-contract/src/errors.rs) — error codes

## Contribution Standards

- Prefer small PRs with a clear objective.
- Include tests for behavior changes.
- Avoid adding dependencies unless necessary.
- Keep interfaces stable and document breaking changes.

## Reporting Security Issues

Do not open a public issue for vulnerabilities. Instead, follow your organization’s responsible disclosure process.
