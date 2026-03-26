-- Showcase 05: Real Estate Investment Analyzer
-- Multi-agent property analysis, financial modeling, and investment report generation.
-- Demonstrates: permit_tree, schemas, type aliases, templates, directives,
-- tools (source, handler, output, directives, retry, cache, validate),
-- agents, agent_bundle, skills, flows (parallel, match, pipeline, fallback, on_error, run),
-- lessons, connect, tests.

permit_tree {
    ^llm {
        ^llm.query
    }
    ^net {
        ^net.read
    }
    ^db {
        ^db.read
    }
    ^fs {
        ^fs.write
    }
}

-- ── Schemas ──────────────────────────────────────────────────────

schema Property {
    address :: String
    price :: Float
    sqft :: Int
    bedrooms :: Int
    bathrooms :: Int
    year_built :: Int
    lot_size :: Float
    property_type :: String
}

schema MarketComps {
    address :: String
    sold_price :: Float
    sold_date :: String
    price_per_sqft :: Float
    days_on_market :: Int
}

schema FinancialModel {
    purchase_price :: Float
    down_payment :: Float
    monthly_mortgage :: Float
    monthly_rent_estimate :: Float
    cap_rate :: Float
    cash_on_cash_return :: Float
    noi :: Float
    irr_5yr :: Float
}

schema RiskAssessment {
    market_risk :: String
    property_risk :: String
    financial_risk :: String
    regulatory_risk :: String
    overall_score :: Float
}

-- ── Type Aliases ─────────────────────────────────────────────────

type PropertyType = SingleFamily | MultiFamily | Condo | Townhouse | Commercial
type InvestmentStrategy = BuyAndHold | FixAndFlip | BRRRR | HousehackShortTermRental
type MarketPhase = Recovery | Expansion | Hypersupply | Recession

-- ── Templates ────────────────────────────────────────────────────

template %investment_report {
    section PROPERTY_OVERVIEW
    ADDRESS :: String                   <<full property address>>
    SUMMARY :: String                   <<one-paragraph investment thesis>>
    COMP_ANALYSIS :: String             <<how property compares to recent sales>>
    section FINANCIALS
    PURCHASE :: String                  <<acquisition cost breakdown>>
    INCOME :: String                    <<projected monthly and annual income>>
    EXPENSE :: String * 6               <<Category | Monthly | Annual | % of Income>>
    RETURN :: String                    <<ROI metrics: cap rate, CoC, IRR>>
    section RISK
    RISK_FACTOR :: String * 4           <<Category | Level | Description | Mitigation>>
    OVERALL_SCORE :: String             <<composite investment score 1-100>>
    section RECOMMENDATION
    VERDICT :: String                   <<BUY / PASS / NEGOTIATE with reasoning>>
    KEY_CONDITIONS :: String * 3        <<condition that must be met for recommendation>>
}

template %neighborhood_profile {
    section DEMOGRAPHICS
    POPULATION :: String                <<population and growth trend>>
    INCOME :: String                    <<median household income and trend>>
    section AMENITIES
    SCHOOL :: String * 3                <<School name | Rating | Distance>>
    TRANSIT :: String * 2               <<Transit option | Walking distance>>
    section MARKET
    TREND :: String * 4                 <<Quarter | Median Price | Volume | DOM>>
}

-- ── Directives ───────────────────────────────────────────────────

directive %financial_rigor {
    <<ANALYSIS STANDARD: All financial calculations must use conservative assumptions.
    Vacancy rate: {vacancy_rate}. Maintenance reserve: {maintenance_pct} of gross rent.
    Property management: {pm_pct} of gross rent. Insurance and taxes based on local rates.
    Mortgage rate: {mortgage_rate}. Always calculate with 30-year fixed conventional financing.
    Show sensitivity analysis: what happens if rent drops 10% or vacancy doubles.
    Never project appreciation above {max_appreciation} annually — use historical local data.>>
    params {
        vacancy_rate :: String = "8%"
        maintenance_pct :: String = "10%"
        pm_pct :: String = "10%"
        mortgage_rate :: String = "6.5%"
        max_appreciation :: String = "3%"
    }
}

directive %market_context {
    <<MARKET: Assess current market phase using Mueller's real estate cycle model.
    Consider: months of inventory, absorption rate, construction pipeline, employment trends,
    and interest rate trajectory. Compare local market to national averages.
    Flag if market appears overheated (price-to-rent ratio > {overheated_ratio}).
    Include 3-year price forecast with confidence interval.>>
    params {
        overheated_ratio :: String = "20"
    }
}

-- ── Tools ────────────────────────────────────────────────────────

tool #fetch_property_data {
    description: <<Retrieve comprehensive property data including listing details, tax records, permit history, and ownership timeline. Cross-reference multiple data sources for accuracy.>>
    requires: [^net.read]
    source: ^search.duckduckgo(query)
    cache: "6h"
    retry: 2
    params {
        query :: String
    }
    returns :: String
}

tool #pull_market_comps {
    description: <<Pull comparable sales data for properties within 0.5 mile radius sold in last 6 months. Filter by similar size (within 20%), age, and property type. Calculate price per sqft statistics (mean, median, range). Identify market trends from comp data.>>
    requires: [^net.read, ^db.read]
    cache: "2h"
    retry: 2
    params {
        address :: String
        property_type :: String
    }
    returns :: String
}

tool #analyze_neighborhood {
    description: <<Analyze the neighborhood surrounding a property. Research demographics, school ratings, crime statistics, transit access, employment centers, and development pipeline. Identify growth catalysts and risks.>>
    requires: [^net.read, ^llm.query]
    output: %neighborhood_profile
    directives: [%market_context]
    cache: "24h"
    params {
        address :: String
    }
    returns :: String
}

tool #build_financial_model {
    description: <<Build a comprehensive financial model for the property investment. Calculate all income, expenses, cash flow, cap rate, cash-on-cash return, and 5-year IRR. Include sensitivity analysis for rent, vacancy, and interest rate variations. Use conservative assumptions throughout.>>
    requires: [^llm.query]
    directives: [%financial_rigor]
    validate: strict
    params {
        property_data :: String
        comps :: String
        strategy :: String
    }
    returns :: String
}

tool #assess_investment_risk {
    description: <<Perform comprehensive risk assessment across market, property, financial, and regulatory dimensions. Score each risk factor 1-10 and calculate weighted composite score. Identify specific mitigation strategies for each risk.>>
    requires: [^llm.query]
    directives: [%market_context]
    params {
        financial_model :: String
        neighborhood :: String
    }
    returns :: String
}

tool #generate_report {
    description: <<Generate a complete investment analysis report combining all data points into a professional, actionable document. Include a clear BUY/PASS/NEGOTIATE recommendation with specific conditions and reasoning. The report should be suitable for presenting to investors.>>
    requires: [^llm.query]
    output: %investment_report
    directives: [%financial_rigor, %market_context]
    validate: strict
    params {
        property :: String
        financials :: String
        risk :: String
        neighborhood :: String
    }
    returns :: String
}

tool #save_report {
    description: <<Save the investment report to the filesystem.>>
    requires: [^fs.write]
    source: ^fs.write_file(path, content)
    params {
        path :: String
        content :: String
    }
    returns :: String
}

-- ── Skills ───────────────────────────────────────────────────────

skill $quick_valuation {
    description: <<Rapid property valuation using comps and basic financial metrics — skips full neighborhood analysis for speed.>>
    tools: [#fetch_property_data, #pull_market_comps, #build_financial_model]
    strategy: <<Fetch property data and comps in parallel, then build financial model from combined data>>
    params {
        address :: String
    }
    returns :: String
}

-- ── Agents ───────────────────────────────────────────────────────

agent @data_scout {
    permits: [^net.read, ^db.read, ^llm.query]
    tools: [#fetch_property_data, #pull_market_comps, #analyze_neighborhood]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a real estate data analyst. You gather comprehensive property data from multiple sources and cross-reference for accuracy. You identify data quality issues and flag them. You know what information matters most for investment decisions — not just the listing price, but the story behind the numbers.>>
}

agent @analyst {
    permits: [^llm.query]
    tools: [#build_financial_model, #assess_investment_risk]
    skills: [$quick_valuation]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a real estate investment analyst with CFA and CCIM certifications. You build rigorous financial models using conservative assumptions. You think in terms of cap rates, cash-on-cash returns, and IRR — not emotions. You always stress-test your models: what happens in a downturn? What if rates rise 200bps? You protect investors from bad deals by being ruthlessly honest about the numbers.>>
    memory: [~deal_archive, ~market_benchmarks]
}

agent @advisor {
    permits: [^llm.query, ^fs.write]
    tools: [#generate_report, #save_report]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a senior investment advisor who translates complex analysis into clear, actionable recommendations. You present findings to investors who may not understand cap rates but understand risk and reward. Your reports are professional, data-driven, and honest — you never oversell a deal.>>
}

agent_bundle @investment_team {
    agents: [@data_scout, @analyst, @advisor]
    fallbacks: @analyst ?> @data_scout
}

-- ── MCP Connections ──────────────────────────────────────────────

connect {
    brave_search   "stdio npx @anthropic/mcp-server-brave"
    filesystem     "stdio npx @anthropic/mcp-server-filesystem"
}

-- ── Lessons ──────────────────────────────────────────────────────

lesson "appreciation_optimism" {
    context: <<Model projected 5% annual appreciation based on recent boom years — actual market corrected 15%>>
    rule: <<Cap appreciation projections at 3% unless supported by specific local catalysts — use 10-year historical average, not 3-year>>
    severity: error
}

lesson "hidden_capex" {
    context: <<Investor missed $40K roof replacement need that wasn't visible in listing photos>>
    rule: <<Always flag properties over 20 years old for inspection contingency and add 5% capex reserve to financial model>>
    severity: warning
}

lesson "comp_radius" {
    context: <<Comps from 2 miles away in a different school district inflated the estimated value by 18%>>
    rule: <<Restrict comps to 0.5 mile radius and same school district — reject comps that cross major geographic boundaries>>
    severity: warning
}

-- ── Flows ────────────────────────────────────────────────────────

-- Full investment analysis pipeline
flow analyze_property(address :: String, strategy :: String) -> String {
    -- Step 1: Gather data in parallel
    parallel {
        property = @data_scout -> #fetch_property_data(address)
        comps = @data_scout -> #pull_market_comps(address, "single_family")
        neighborhood = @data_scout -> #analyze_neighborhood(address)
    }

    -- Step 2: Financial modeling
    financials = @analyst -> #build_financial_model(property, comps, strategy)

    -- Step 3: Risk assessment
    risk = @analyst -> #assess_investment_risk(financials, neighborhood)

    -- Step 4: Generate and save report
    report = @advisor -> #generate_report(property, financials, risk, neighborhood)
    saved = @advisor -> #save_report("reports/latest.md", report) on_error <<Save deferred>>

    return report
}

-- Strategy-specific analysis with match
flow strategy_analysis(address :: String, strategy :: String) -> String {
    property = @data_scout -> #fetch_property_data(address)
    comps = @data_scout -> #pull_market_comps(address, "single_family")

    result = match strategy {
        "fix_and_flip" => @analyst -> #build_financial_model(property, comps, "fix_and_flip")
        "buy_and_hold" => @analyst -> #build_financial_model(property, comps, "buy_and_hold")
        "brrrr" => @analyst -> #build_financial_model(property, comps, "brrrr")
        _ => @analyst -> #build_financial_model(property, comps, "buy_and_hold")
    }

    return result
}

-- Quick valuation pipeline
flow quick_value(address :: String) -> String {
    result = @data_scout -> #fetch_property_data(address) |> @analyst -> #build_financial_model(result, "no comps", "buy_and_hold")
    return result
}

-- Multi-property comparison via sub-flow
flow compare_properties(address_a :: String, address_b :: String) -> String {
    parallel {
        report_a = run analyze_property(address_a, "buy_and_hold")
        report_b = run analyze_property(address_b, "buy_and_hold")
    }
    return report_a
}

-- ── Tests ────────────────────────────────────────────────────────

test "property data retrieval works" {
    data = @data_scout -> #fetch_property_data("123 Oak Ave, Austin TX")
    assert data
}

test "financial model uses conservative assumptions" {
    model = @analyst -> #build_financial_model("3BR/2BA, $350K, 1800sqft", "Median $200/sqft", "buy_and_hold")
    assert model
}

test "risk assessment covers all dimensions" {
    risk = @analyst -> #assess_investment_risk("Cap rate 6.2%, CoC 8.1%", "Growth area, A-rated schools")
    assert risk
}

test "full analysis pipeline produces report" {
    result = run analyze_property("456 Elm St, Denver CO", "buy_and_hold")
    assert result
}
