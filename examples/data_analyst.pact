-- Created: 2026-02-08
-- Copyright (c) 2026 Gabriel Lars Sabadin
-- Licensed under the MIT License. See LICENSE file in the project root.

-- Data Analysis Pipeline
--
-- Fetches data from an API, cleans and transforms it, then generates
-- insights and an HTML visualization. Demonstrates env(), validate,
-- pipeline (|>), templates for structured output, and source providers.

-- ── Permissions ─────────────────────────────────────────────────
permit_tree {
    ^llm {
        ^llm.query
    }
    ^net {
        ^net.read
    }
}

-- ── Schemas ─────────────────────────────────────────────────────

schema DataPoint {
    label :: String
    value :: Float
    category :: String
}

schema Insight {
    title :: String
    detail :: String
    metric :: String
    trend :: String
}

-- ── Templates ───────────────────────────────────────────────────

template %analysis_report {
    DATASET_NAME :: String          <<name of the analyzed dataset>>
    RECORD_COUNT :: String          <<number of records processed>>
    INSIGHT :: String * 4           <<Title | Metric | Trend direction | Description>>
    RECOMMENDATION :: String * 3   <<actionable recommendation based on data>>
}

-- ── Tools ───────────────────────────────────────────────────────

tool #fetch_data {
    description: <<Fetch JSON data from a REST API endpoint. Uses the provided API key for authentication via Bearer token header. Returns raw JSON response body.>>
    requires: [^net.read]
    handler: "http GET {endpoint}"
    retry: 2
    params {
        endpoint :: String
    }
    returns :: String
}

tool #clean_data {
    description: <<Clean and normalize raw JSON data. Remove null values, normalize date formats to ISO-8601, trim whitespace from strings, and deduplicate records. Return cleaned JSON.>>
    requires: [^llm.query]
    validate: strict
    params {
        raw_data :: String
    }
    returns :: String
}

tool #analyze_trends {
    description: <<Analyze cleaned data to identify key trends, outliers, and patterns. Compute summary statistics (mean, median, min, max, std dev). Return structured insights.>>
    requires: [^llm.query]
    output: %analysis_report
    validate: strict
    params {
        clean_data :: String
        api_key :: String
    }
    returns :: String
}

tool #generate_chart {
    description: <<Generate a self-contained HTML page with embedded SVG charts visualizing the data insights. Use inline CSS for styling. Include bar charts, trend lines, and a summary dashboard. Return raw HTML.>>
    requires: [^llm.query]
    params {
        insights :: String
        data :: String
    }
    returns :: String
}

-- ── Agents ──────────────────────────────────────────────────────

agent @data_engineer {
    permits: [^net.read, ^llm.query]
    tools: [#fetch_data, #clean_data]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a data engineer. Fetch, clean, and prepare data for analysis. Ensure data quality: handle missing values, normalize formats, and validate schemas. Never discard records silently — log what was removed and why.>>
}

agent @analyst {
    permits: [^llm.query]
    tools: [#analyze_trends, #generate_chart]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a senior data analyst. Identify actionable insights from data. Focus on trends, anomalies, and business impact. Produce clear visualizations that tell a story. Every chart must have a title, labeled axes, and a legend.>>
}

agent_bundle @data_team {
    agents: [@data_engineer, @analyst]
}

-- ── Flows ───────────────────────────────────────────────────────

flow analyze_endpoint(endpoint :: String) -> String {
    -- Pipeline: fetch -> clean -> analyze -> visualize
    api_key = env("DATA_API_KEY")

    -- Step 1: Fetch raw data from the API
    raw = @data_engineer -> #fetch_data(endpoint)

    -- Step 2: Clean and normalize
    clean = @data_engineer -> #clean_data(raw)

    -- Step 3: Analyze for insights
    insights = @analyst -> #analyze_trends(clean, api_key)

    -- Step 4: Generate visualization
    chart = @analyst -> #generate_chart(insights, clean)

    return chart
}

flow quick_insights(endpoint :: String) -> String {
    -- Compact pipeline using |> operator
    api_key = env("DATA_API_KEY")
    result = @data_engineer -> #fetch_data(endpoint) |> @data_engineer -> #clean_data(result) |> @analyst -> #analyze_trends(result, api_key)
    return result
}

-- ── Tests ───────────────────────────────────────────────────────

test "fetch returns data" {
    result = @data_engineer -> #fetch_data("https://api.example.com/data")
    assert result == "fetch_data_result"
}

test "cleaning produces valid output" {
    result = @data_engineer -> #clean_data("{\"items\": []}")
    assert result == "clean_data_result"
}

test "analysis generates insights" {
    result = @analyst -> #analyze_trends("clean data", "test-key")
    assert result == "analyze_trends_result"
}
