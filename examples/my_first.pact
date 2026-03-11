-- Created: 2026-02-05
-- My first PACT agent contract

permit_tree {
    ^llm {
        ^llm.query
    }
    ^net {
        ^net.read
    }
}

tool #search {
    description: <<Search the web for information.>>
    requires: [^net.read]
    handler: "http GET https://api.duckduckgo.com/?q={query}&format=json&no_html=1"
    params {
        query :: String
    }
    returns :: List<String>
}

tool #summarize {
    description: <<Summarize content into a concise paragraph.>>
    requires: [^llm.query]
    params {
        content :: String
    }
    returns :: String
}

agent @researcher {
    permits: [^net.read, ^llm.query]
    tools: [#search, #summarize]
    prompt: <<You are a thorough research assistant. Find accurate information and provide clear summaries.>>
}

schema Report {
    title :: String
    body :: String
}

flow investigate(topic :: String) -> String {
    results = @researcher -> #search(topic)
    summary = @researcher -> #summarize(results)
    return summary
}

flow safe_investigate(topic :: String) -> String {
    result = @researcher -> #search(topic) ?> @researcher -> #summarize(topic)
    return result
}

test "researcher can search" {
    result = @researcher -> #search("quantum computing")
    assert result == "search_result"
}
