# Codescape — Claude Code notes

@AGENTS.md

## Claude Code only

- Memory tools are `mcp__claude-flow__memory_*`, namespace `project-state`. Never the `claude-flow memory` CLI.
- When working on Codescape from inside a VisionClaw checkout, edit this repository (the `codescape/` submodule), never a copy under VisionClaw's own tree, and commit here before bumping the submodule pointer there.
- Build the deployed service through the host's launch script (`CODESCAPE=1 ./scripts/launch.sh up dev` in VisionClaw), never with plain `docker compose` from an agent shell.
