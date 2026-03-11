-- Created: 2025-11-28
-- Copyright (c) 2025-2026 Gabriel Lars Sabadin
-- Licensed under the MIT License. See LICENSE file in the project root.

-- RAG Pipeline: Retrieval-Augmented Generation
--
-- Searches the web for information, synthesizes findings into a
-- coherent answer with citations, then validates answer quality.
-- Demonstrates source providers, retry, run (flow composition),
-- templates, and multi-agent quality checking.

-- ── Permissions ─────────────────────────────────────────────────
permit_tree {
    ^llm {
        ^llm.query
    }
    ^net {
        ^net.read
    }
}

-- ── Templates ───────────────────────────────────────────────────

template %cited_answer {
    QUESTION :: String              <<the original user question>>
    ANSWER :: String                <<comprehensive answer in clear prose>>
    CITATION :: String * 5          <<[N] Title | URL | Relevant excerpt>>
    CONFIDENCE :: String            <<high, medium, or low>>
}

template %quality_verdict {
    ACCURACY :: String              <<does the answer match the sources?>>
    COMPLETENESS :: String          <<are all aspects of the question covered?>>
    CITATIONS_VALID :: String       <<do citations support the claims?>>
    VERDICT :: String               <<PASS or FAIL with one-line reason>>
}

-- ── Tools ───────────────────────────────────────────────────────

tool #web_search {
    description: <<Search the web using DuckDuckGo for information relevant to the query. Returns a list of result snippets with titles and URLs.>>
    requires: [^net.read]
    source: ^search.duckduckgo(query)
    retry: 3
    params {
        query :: String
    }
    returns :: List<String>
}

tool #synthesize {
    description: <<Synthesize multiple search result snippets into a coherent, well-cited answer. Use inline citation markers [1], [2], etc. matching the source order.>>
    requires: [^llm.query]
    output: %cited_answer
    params {
        question :: String
        sources :: String
    }
    returns :: String
}

tool #check_quality {
    description: <<Validate that an answer is accurate, complete, and properly cited. Compare claims against the provided source material. Return a structured quality verdict.>>
    requires: [^llm.query]
    output: %quality_verdict
    params {
        answer :: String
        sources :: String
    }
    returns :: String
}

-- ── Agents ──────────────────────────────────────────────────────

agent @retriever {
    permits: [^net.read, ^llm.query]
    tools: [#web_search, #synthesize]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a research assistant. Search for accurate, up-to-date information and synthesize it into clear answers. Always cite your sources with numbered references.>>
}

agent @quality_checker {
    permits: [^llm.query]
    tools: [#check_quality]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a fact-checker. Rigorously compare answers against source material. Flag any unsupported claims, missing context, or citation errors. Be strict but fair.>>
}

agent_bundle @rag_team {
    agents: [@retriever, @quality_checker]
}

-- ── Flows ───────────────────────────────────────────────────────

flow search_and_cite(question :: String) -> String {
    -- Step 1: Retrieve relevant information from the web
    sources = @retriever -> #web_search(question)

    -- Step 2: Synthesize sources into a cited answer
    answer = @retriever -> #synthesize(question, sources)

    return answer
}

flow verified_answer(question :: String) -> String {
    -- Compose flows: first retrieve and synthesize, then validate
    answer = run search_and_cite(question)

    -- Step 3: Quality check — validate citations and accuracy
    sources = @retriever -> #web_search(question)
    verdict = @quality_checker -> #check_quality(answer, sources)

    -- If quality check fails, try with a more specific query
    refined = @retriever -> #synthesize(question, verdict) ?> answer

    return refined
}

-- ── Tests ───────────────────────────────────────────────────────

test "search returns results" {
    result = @retriever -> #web_search("what is PACT language")
    assert result == "web_search_result"
}

test "synthesis produces cited answer" {
    result = @retriever -> #synthesize("test question", "source data")
    assert result == "synthesize_result"
}

test "quality check validates answers" {
    result = @quality_checker -> #check_quality("test answer", "test sources")
    assert result == "check_quality_result"
}
