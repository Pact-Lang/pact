-- Created: 2025-08-15
-- Copyright (c) 2025-2026 Gabriel Lars Sabadin
-- Licensed under the MIT License. See LICENSE file in the project root.

-- Research Flow: A full PACT example demonstrating multi-agent coordination.
--
-- This file showcases tool declarations, agents, schemas, flows with
-- pipelines, fallback chains, and permission validation.

-- Permission tree defines the security boundary
permit_tree {
    ^net {
        ^net.read
        ^net.write
    }
    ^llm {
        ^llm.query
    }
    ^fs {
        ^fs.read
        ^fs.write
    }
}

-- Tool declarations with schemas and permissions
tool #web_search {
    description: <<Search the web for information about a given query. Returns a list of relevant results with titles, URLs, and snippets.>>
    handler: "http GET https://api.duckduckgo.com/?q={query}&format=json&no_html=1"
    requires: [^net.read]
    params {
        query :: String
    }
    returns :: List<String>
}

tool #summarize {
    description: <<Summarize the provided content into a concise, well-structured paragraph highlighting key points.>>
    requires: [^llm.query]
    params {
        content :: String
    }
    returns :: String
}

tool #draft_report {
    description: <<Draft a structured report from the provided summary. The report should include an introduction, key findings, and conclusion.>>
    requires: [^llm.query]
    params {
        summary :: String
    }
    returns :: String
}

-- Schema for the research report output
schema Report {
    title :: String
    body :: String
    sources :: List<String>
}

-- Type alias for report status
type ReportStatus = Draft | Published | Archived

-- The researcher agent can search the web and query an LLM
agent @researcher {
    permits: [^net.read, ^llm.query]
    tools: [#web_search, #summarize]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a thorough research assistant. Search for information and provide detailed, well-sourced summaries. Always cite your sources.>>
}

-- The writer agent can query an LLM to draft reports
agent @writer {
    permits: [^llm.query]
    tools: [#draft_report]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a professional technical writer. Create clear, well-structured reports with proper formatting and logical flow.>>
}

-- Agent bundle groups related agents
agent_bundle @research_team {
    agents: [@researcher, @writer]
    fallbacks: @researcher ?> @writer
}

-- Main research flow
flow research_and_report(topic :: String) -> String {
    -- Step 1: Search for information
    search_results = @researcher -> #web_search(topic)

    -- Step 2: Summarize findings
    summary = @researcher -> #summarize(search_results)

    -- Step 3: Draft a report
    report = @writer -> #draft_report(summary)

    return report
}

-- Flow demonstrating fallback chains
flow safe_search(query :: String) -> String {
    result = @researcher -> #web_search(query) ?> @writer -> #draft_report(query)
    return result
}

-- Tests
test "research flow produces output" {
    result = @researcher -> #web_search("AI safety")
    assert result == "web_search_result"
}

test "writer can draft reports" {
    report = @writer -> #draft_report("test summary")
    assert report == "draft_report_result"
}
