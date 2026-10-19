# Codescape documentation

Index of the documents in this repository. The project overview is the [README](../README.md); agent instructions are in [AGENTS.md](../AGENTS.md).

## Governing documents

| Document | What it holds |
|---|---|
| [BASELINE-codescape.md](BASELINE-codescape.md) | The governing document. Its *Invariants* section is the compliance surface (wire, ids, isolation, PTX checks). |
| [adr/README.md](adr/README.md) | The decision ledger and routing table. Start at [PREAMBLE.md](adr/PREAMBLE.md); new records copy [TEMPLATE.md](adr/TEMPLATE.md). |
| [adr/ADR-0001-extraction-from-visionclaw.md](adr/ADR-0001-extraction-from-visionclaw.md) | Why Codescape is its own repository, and the per-crate licence split. |

## Evidence

[evidence/README.md](evidence/README.md) indexes the committed gate output. The import gate is [evidence/import-gate.txt](evidence/import-gate.txt). Phase P0 and P1 evidence predates the extraction and stays in VisionClaw.

## Deployment

[../deploy/README.md](../deploy/README.md) is the compose and nginx contract a host implements. VisionClaw is the reference host. [../client/README.md](../client/README.md) covers the desktop client build.

## Where this sits in the estate

Codescape is a section of the [VisionFlow](https://github.com/DreamLab-AI/VisionFlow) mesh. VisionFlow holds the ecosystem map, the compatibility matrix and the status ledger; the sibling repositories are listed in the README under [Where Codescape sits](../README.md#where-codescape-sits).
