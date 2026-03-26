-- Created: 2026-03-26
-- Copyright (c) 2025-2026 Gabriel Lars Sabadin
-- Licensed under the MIT License. See LICENSE file in the project root.

-- Federation: Cross-network agent discovery and dispatch.
--
-- This example demonstrates PACT's federation system — declaring
-- remote agent registries with trust boundaries, and dispatching
-- work to both local and remote agents.

-- ── Permissions ───────────────────────────────────────────────────────────

permit_tree {
    ^llm  { ^llm.query }
    ^net  { ^net.read }
    ^data { ^data.read, ^data.write }
}

-- ── Federation registries ─────────────────────────────────────────────────
-- Each registry declares a URL and a trust boundary: the maximum set of
-- permissions any agent discovered through that registry may hold.
-- If a remote agent claims permissions outside the trust boundary,
-- the dispatcher rejects the call.

federation {
    "https://agents.example.com/registry" trust: [^llm.query, ^net.read]
    "https://internal.corp.net/agents"    trust: [^llm.query, ^data.read, ^data.write]
}

-- ── Tools ─────────────────────────────────────────────────────────────────

tool #search_web {
    description: <<Search the web for recent information on a topic.>>
    requires: [^net.read]
    source: ^search.duckduckgo(query)
    params { query :: String }
    returns :: List<String>
}

tool #summarize {
    description: <<Condense a list of search results into a concise summary.>>
    requires: [^llm.query]
    params { results :: String }
    returns :: String
}

tool #store_findings {
    description: <<Persist research findings to the internal data store.>>
    requires: [^data.write]
    params { summary :: String }
    returns :: String
}

tool #retrieve_context {
    description: <<Retrieve previously stored research context.>>
    requires: [^data.read]
    params { topic :: String }
    returns :: String
}

-- ── Local agent ───────────────────────────────────────────────────────────

agent @researcher {
    permits: [^net.read, ^llm.query]
    tools: [#search_web, #summarize]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a research assistant. You search for information and
    produce concise, accurate summaries. Cite sources when possible.>>
}

-- ── Remote agents ─────────────────────────────────────────────────────────
-- Remote agents declare an endpoint URL. When dispatched, the federated
-- dispatcher routes the call over HTTP to the remote agent's service
-- instead of executing locally.

agent @archivist {
    permits: [^data.read, ^data.write]
    tools: [#store_findings, #retrieve_context]
    endpoint: "https://internal.corp.net/agents/archivist"
    prompt: <<You are a data archivist. You store and retrieve research
    findings with full provenance metadata.>>
}

-- ── Flows ─────────────────────────────────────────────────────────────────

flow research_and_archive(topic :: String) -> String {
    -- Local agent searches and summarizes.
    results = @researcher -> #search_web(topic)
    summary = @researcher -> #summarize(results)

    -- Remote agent stores findings via federation.
    saved = @archivist -> #store_findings(summary)

    return saved
}

flow contextual_research(topic :: String) -> String {
    -- Pull prior context from remote archivist.
    context = @archivist -> #retrieve_context(topic)

    -- Local research enriched with prior context.
    results = @researcher -> #search_web(topic)
    summary = @researcher -> #summarize(results)

    return summary
}

-- ── Tests ─────────────────────────────────────────────────────────────────

test "research and archive produces a result" {
    result = @researcher -> #search_web("quantum computing")
    summary = @researcher -> #summarize(result)
    assert summary == "summarize_result"
}

test "remote agent can store findings" {
    saved = @archivist -> #store_findings("test summary")
    assert saved == "store_findings_result"
}
