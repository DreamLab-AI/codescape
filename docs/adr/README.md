# Codescape decision records

See [`PREAMBLE.md`](PREAMBLE.md) for how to work against this pack. The governing document is [`../BASELINE-codescape.md`](../BASELINE-codescape.md).

## Routing

| Question | Go to |
|---|---|
| Why is Codescape its own repository, and how is it licensed? | ADR-0001 |
| What must never change (wire, ids, isolation, PTX checks)? | BASELINE §Invariants |
| How does VisionClaw deploy it? | [`../../deploy/README.md`](../../deploy/README.md) |
| Why a separate workspace, port and isolation boundary at all? | VisionClaw ADR-2143 (history) |

## Ledger

| ID | Title | Decision | Implementation | Activation | Date |
|---|---|---|---|---|---|
| [ADR-0001](ADR-0001-extraction-from-visionclaw.md) | Codescape is extracted from VisionClaw into its own repository with a per-crate licence split | accepted | partial | inactive | 2026-10-18 |
