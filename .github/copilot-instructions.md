# Copilot Instructions — pst2pdf

## Project

`pst2pdf` is a Node.js tool that converts Outlook PST email archives to PDF. The project is in early setup; source code is not yet scaffolded.

## Stack

- **Runtime:** Node.js
- **Package manager:** npm
- **OS:** Windows (primary development environment — use Windows-compatible paths and commands)

## Build, Test, Lint

No scripts are defined yet. When added, they will live in `package.json`. Update this file when commands are established.

## Team

| Name | Role | Domain |
|------|------|--------|
| Walt | Lead | Architecture, CMake, code review, scope |
| Mike | Core Dev | PST parsing with pstsdk |
| Gus | Core Dev | PDF generation with libHaru |
| Hank | Tester | Test cases, edge cases, validation |
| Scribe | Logger | Session memory (silent) |
| Ralph | Monitor | Work queue |

This repo uses **Squad**, an AI team framework. The coordinator is configured at `.github/agents/squad.agent.md`. Before working on issues autonomously:

1. Read `.squad/team.md` for the team roster and member roles.
2. Read `.squad/routing.md` for work routing rules.
3. Read `.squad/decisions.md` for active architectural and process decisions.
4. If an issue has a `squad:{member}` label, read `.squad/agents/{member}/charter.md` and work in that member's domain voice.

### Branch Naming

```
squad/{issue-number}-{kebab-case-slug}
```

Example: `squad/12-add-pst-parser`

### PRs

- Reference the issue: `Closes #{issue-number}`
- If the issue had a `squad:{member}` label, note: `Working as {member} ({role})`
- Follow decisions in `.squad/decisions.md`

### Decisions Drop-Box

If you make a team-relevant decision, write it to:
```
.squad/decisions/inbox/copilot-{brief-slug}.md
```
The Scribe will merge it into `.squad/decisions.md`.

## Key Paths

| Path | Purpose |
|------|---------|
| `.squad/team.md` | Team roster |
| `.squad/decisions.md` | Architectural / process decisions |
| `.squad/routing.md` | Work routing rules |
| `.squad/agents/` | Per-agent charters and history |
| `.github/agents/squad.agent.md` | Coordinator governance (authoritative) |
