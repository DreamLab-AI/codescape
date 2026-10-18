**How to work against this pack** (engineering and build-with-quality agents start here):

The ADR pack for Codescape is **its living governing document plus the ledger records below that amend it**. The living document is normative: its *Invariants* section is the compliance surface and its *Change process* section says how to amend it.

| Domain | Governing document |
|---|---|
| Crates, licences, wire mapping, isolation, deployment contract | [`../BASELINE-codescape.md`](../BASELINE-codescape.md) |

**Lookup order:** governing document, then its `file:line` citations into code, then the ledger records below. For decisions taken before extraction, VisionClaw's ADR-2143 and its amendments A1 to A4 are the rationale and history; they are not authority in this repository once a record here covers the same ground.

**Making a decision:** copy [`TEMPLATE.md`](TEMPLATE.md) to `ADR-NNNN-slug.md` (next free number, four digits from 0001), fill the three status axes honestly (decision, implementation, activation are independent), update the governing document **in the same change**, and add the record's row to [`README.md`](README.md). Set `verified_commit` to the full 40-character SHA at which implementation status was checked. Cross-repository references (VisionClaw, agentbox) are links in the body, never `supersedes` or `depends_on` edges, which name records in this ledger only.
