# Xerj evaluation

Source/documentation review on 2026-10-26; upstream `main` reports version
`1.0.0-rc.99`. No installation, benchmark, MCP registration or external report
was performed.

[Xerj](https://github.com/xerj-org/xerj) is a local Rust search engine with a
stdio MCP interface. It indexes source symbols, documents and structured files,
including JSONL, and returns ranked passages. Windows binaries are offered under
Apache-2.0. Its published speed/token savings have not been measured here.

The [search implementation](https://github.com/xerj-org/xerj/blob/main/engine/crates/xerj-autoindex/src/search.rs)
confirms that ordinary search is lexical, with bounded passage output. Default
semantic/hybrid embeddings use lexical feature hashing; neural embeddings require
an explicit mode and model. Local search needs no paid model API.

For this repository it could shorten discovery across binary-index JSONL,
recovered sources and evidence notes. Exact VA/symbol checks still use the index,
`rg`, raw assembly and original consumers. Retrieval does not establish semantics
or replace original comparisons.

Decision: a bounded retrieval trial is reasonable, but it is not a prerequisite
for continuing recovery. Use 10-20 existing questions with known file/VA answers;
compare correctness, returned text, latency and indexing cost against our current
bounded `rg` workflow. Keep it only if the measured result improves that workflow.
Start with selected evidence/source directories, excluding game data, generated
fixtures and build trees; keep the search database outside indexed directories.

The [MCP implementation](https://github.com/xerj-org/xerj/blob/main/engine/crates/xerj-mcp/src/lib.rs)
also exposes feedback tools that send externally. A trial should disable feedback
and external reranking and keep the node on loopback. No external feedback or
repository-wide rule changes are authorized by this evaluation.
