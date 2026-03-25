-- Showcase 09: Personal Finance Advisor
-- Portfolio analysis, tax optimization, and retirement planning with compliance guardrails.
-- Demonstrates: permit_tree, schemas, type aliases, templates, directives,
-- tools (source, handler, output, directives, retry, cache, validate),
-- agents, agent_bundle, skills, flows (parallel, match, pipeline, fallback, on_error, run),
-- lessons, connect, tests.

permit_tree {
    ^llm {
        ^llm.query
        ^llm.embed
    }
    ^net {
        ^net.read
    }
    ^db {
        ^db.read
        ^db.write
    }
    ^fs {
        ^fs.write
    }
}

-- ── Schemas ──────────────────────────────────────────────────────

schema Portfolio {
    owner :: String
    total_value :: Float
    allocation :: String
    risk_score :: Float
    last_rebalance :: String
    accounts :: List<String>
}

schema Holding {
    ticker :: String
    shares :: Float
    cost_basis :: Float
    current_value :: Float
    unrealized_gain :: Float
    sector :: String
    asset_class :: String
}

schema TaxSituation {
    filing_status :: String
    gross_income :: Float
    deductions :: Float
    tax_bracket :: String
    capital_gains :: Float
    estimated_tax :: Float
}

schema RetirementProjection {
    current_age :: Int
    retirement_age :: Int
    current_savings :: Float
    monthly_contribution :: Float
    projected_balance :: Float
    success_probability :: Float
    gap :: Float
}

-- ── Type Aliases ─────────────────────────────────────────────────

type AssetClass = USEquity | IntlEquity | FixedIncome | RealEstate | Commodities | Cash | Crypto
type RiskProfile = Conservative | Moderate | Aggressive | VeryAggressive
type AccountType = Taxable | TraditionalIRA | RothIRA | HSA | FiveZeroNineB

-- ── Templates ────────────────────────────────────────────────────

template %portfolio_review {
    section SUMMARY
    NET_WORTH :: String                 <<total portfolio value with change from last review>>
    ALLOCATION :: String                <<current vs target allocation with drift>>
    RISK_ASSESSMENT :: String           <<risk score and comparison to target>>
    section HOLDINGS
    TOP_PERFORMER :: String * 3         <<Ticker | Return | Weight>>
    UNDERPERFORMER :: String * 3        <<Ticker | Return | Issue>>
    section RECOMMENDATIONS
    REBALANCE :: String * 4             <<Action | Ticker | Shares | Reason>>
    TAX_HARVEST :: String * 2           <<Ticker | Unrealized Loss | Wash Sale Risk>>
    section OUTLOOK
    MARKET_VIEW :: String               <<current market conditions and positioning advice>>
}

template %retirement_plan {
    section CURRENT_STATE
    SAVINGS :: String                   <<current retirement savings summary>>
    PROJECTION :: String                <<projected balance at retirement age>>
    PROBABILITY :: String               <<Monte Carlo success probability>>
    section GAP_ANALYSIS
    GAP :: String                       <<shortfall or surplus amount>>
    SCENARIO :: String * 3              <<Scenario | Assumptions | Projected Balance>>
    section STRATEGY
    CONTRIBUTION :: String              <<recommended monthly contribution adjustment>>
    ALLOCATION_SHIFT :: String          <<recommended allocation change by decade>>
    SOCIAL_SECURITY :: String           <<optimal claiming strategy>>
    section ACTIONS
    ACTION :: String * 5                <<Priority | Action | Impact | Timeline>>
}

-- ── Directives ───────────────────────────────────────────────────

directive %fiduciary_standard {
    <<COMPLIANCE: All advice must meet {standard} fiduciary standard. Never recommend
    products with hidden fees or revenue-sharing arrangements. Disclose all material risks.
    Total expense ratio should not exceed {max_expense_ratio} for passive strategies.
    Always present at least 2 alternatives for any recommendation. Suitability: verify
    recommendation matches client's risk tolerance, time horizon, and liquidity needs.
    CRITICAL: This system provides FINANCIAL GUIDANCE only. All recommendations should
    be reviewed by a licensed financial advisor before implementation.>>
    params {
        standard :: String = "SEC Regulation Best Interest"
        max_expense_ratio :: String = "0.20%"
    }
}

directive %tax_awareness {
    <<TAX OPTIMIZATION: Consider tax implications of all recommendations.
    Prefer tax-efficient placement: bonds in tax-deferred, equities in taxable.
    Harvest losses when unrealized loss exceeds {harvest_threshold} and no wash sale risk.
    Track short-term vs long-term capital gains. Model Roth conversion opportunities
    in low-income years. Always note: "Consult a tax professional for your specific situation.">>
    params {
        harvest_threshold :: String = "$3,000"
    }
}

-- ── Tools ────────────────────────────────────────────────────────

tool #analyze_portfolio {
    description: <<Perform comprehensive portfolio analysis: calculate total value, sector allocation, risk metrics (Sharpe ratio, beta, max drawdown), and compare against target allocation. Identify concentration risks, overweight positions, and rebalancing needs. Include fee analysis.>>
    requires: [^llm.query, ^db.read]
    output: %portfolio_review
    directives: [%fiduciary_standard]
    validate: strict
    params {
        holdings :: String
        risk_profile :: String
    }
    returns :: String
}

tool #optimize_taxes {
    description: <<Analyze current tax situation and identify optimization opportunities: tax-loss harvesting candidates, Roth conversion timing, charitable giving strategies, and estimated tax payment adjustments. Model impact of each strategy on after-tax returns. Check for wash sale rule compliance.>>
    requires: [^llm.query]
    directives: [%tax_awareness, %fiduciary_standard]
    validate: strict
    params {
        portfolio :: String
        tax_data :: String
    }
    returns :: String
}

tool #project_retirement {
    description: <<Run Monte Carlo simulation for retirement projections using current savings, contribution rate, allocation, and assumptions about returns, inflation, and social security. Generate probability of success across multiple scenarios (base, optimistic, pessimistic). Identify savings gap and recommend adjustments.>>
    requires: [^llm.query]
    output: %retirement_plan
    directives: [%fiduciary_standard]
    validate: strict
    params {
        current_age :: Int
        retirement_age :: Int
        savings :: Float
        monthly_contribution :: Float
        risk_profile :: String
    }
    returns :: String
}

tool #fetch_market_data {
    description: <<Retrieve current market data including index levels, sector performance, interest rates, and economic indicators. Used to contextualize portfolio analysis and recommendations.>>
    requires: [^net.read]
    source: ^search.duckduckgo(query)
    cache: "1h"
    retry: 2
    params {
        query :: String
    }
    returns :: String
}

tool #generate_rebalance_orders {
    description: <<Generate specific trade orders to rebalance portfolio toward target allocation. Minimize tax impact by selling in most tax-efficient order. Include limit prices based on current market conditions. Flag any trades that trigger wash sale rules.>>
    requires: [^llm.query]
    directives: [%tax_awareness]
    params {
        portfolio :: String
        target_allocation :: String
    }
    returns :: String
}

tool #save_plan {
    description: <<Save the financial plan to client records.>>
    requires: [^db.write]
    handler: "http POST https://api.advisor-platform.example.com/plans"
    retry: 3
    params {
        client_id :: String
        plan :: String
    }
    returns :: String
}

-- ── Skills ───────────────────────────────────────────────────────

skill $comprehensive_review {
    description: <<Full financial review: portfolio analysis, tax optimization, and retirement projection in one coordinated assessment.>>
    tools: [#analyze_portfolio, #optimize_taxes, #project_retirement]
    strategy: <<Portfolio analysis first (provides data for both tax and retirement), then tax optimization and retirement projection can run in parallel>>
    params {
        client_id :: String
    }
    returns :: String
}

-- ── Agents ───────────────────────────────────────────────────────

agent @portfolio_manager {
    permits: [^llm.query, ^db.read, ^net.read]
    tools: [#analyze_portfolio, #fetch_market_data, #generate_rebalance_orders]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a chartered financial analyst (CFA) managing client portfolios. You think in risk-adjusted returns, not absolute returns. You rebalance systematically, not emotionally. You minimize costs, maximize tax efficiency, and never chase performance. You explain investment concepts clearly without jargon. IMPORTANT: You provide guidance only — a licensed advisor must review all recommendations.>>
    memory: [~client_profiles, ~market_outlook]
}

agent @tax_advisor {
    permits: [^llm.query]
    tools: [#optimize_taxes]
    skills: [$comprehensive_review]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a tax optimization specialist for investment portfolios. You see opportunities that most investors miss — Roth conversions in low-income years, tax-loss harvesting before year-end, charitable remainder trusts. You never let tax considerations override sound investment strategy, but you never ignore them either. IMPORTANT: You provide guidance only — consult a tax professional.>>
}

agent @retirement_planner {
    permits: [^llm.query]
    tools: [#project_retirement]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a certified financial planner specializing in retirement. You run Monte Carlo simulations with conservative assumptions. You help clients understand the difference between "enough" and "comfortable". You present scenarios honestly — if the numbers don't work, you say so and provide a path to fix it. Social Security timing is an art you've mastered.>>
    memory: [~retirement_benchmarks]
}

agent @records {
    permits: [^db.write]
    tools: [#save_plan]
    prompt: <<You are a financial records agent. You persist plans to client records. Execute writes precisely.>>
}

agent_bundle @advisory_team {
    agents: [@portfolio_manager, @tax_advisor, @retirement_planner, @records]
    fallbacks: @portfolio_manager ?> @tax_advisor
}

-- ── MCP Connections ──────────────────────────────────────────────

connect {
    brave_search   "stdio npx @anthropic/mcp-server-brave"
    postgres       "stdio npx @anthropic/mcp-server-postgres"
}

-- ── Lessons ──────────────────────────────────────────────────────

lesson "recency_bias" {
    context: <<Client wanted 100% tech allocation after strong tech year — portfolio crashed in subsequent correction>>
    rule: <<Never recommend sector concentration above 30% regardless of recent performance — diversification is non-negotiable>>
    severity: error
}

lesson "roth_conversion_window" {
    context: <<Client missed optimal Roth conversion window (job transition year with low income), costing $15K in lifetime tax savings>>
    rule: <<Proactively identify low-income years (job transitions, sabbaticals, early retirement) as Roth conversion opportunities — flag at every review>>
    severity: warning
}

lesson "inflation_underestimate" {
    context: <<Retirement projection used 2% inflation assumption during period of 5% actual inflation, overstating purchasing power by 40% at 30-year horizon>>
    rule: <<Use 3% minimum inflation assumption for retirement projections — run sensitivity analysis with 4% and 5% scenarios>>
    severity: warning
}

-- ── Flows ────────────────────────────────────────────────────────

-- Full financial review
flow annual_review(holdings :: String, risk_profile :: String, tax_data :: String, age :: Int) -> String {
    -- Step 1: Market context and portfolio analysis
    market = @portfolio_manager -> #fetch_market_data("market conditions 2026")

    -- Step 2: Core analysis in parallel
    parallel {
        portfolio = @portfolio_manager -> #analyze_portfolio(holdings, risk_profile)
        taxes = @tax_advisor -> #optimize_taxes(holdings, tax_data)
        retirement = @retirement_planner -> #project_retirement(age, 65, 500000.0, 2000.0, risk_profile)
    }

    -- Step 3: Generate rebalance orders
    orders = @portfolio_manager -> #generate_rebalance_orders(portfolio, risk_profile)

    -- Step 4: Save plan
    saved = @records -> #save_plan("CLIENT-001", orders) on_error <<Plan save deferred>>

    return orders
}

-- Risk-profile-specific review with match
flow profile_review(holdings :: String, profile :: String) -> String {
    result = match profile {
        "conservative" => @portfolio_manager -> #analyze_portfolio(holdings, "conservative")
        "moderate" => @portfolio_manager -> #analyze_portfolio(holdings, "moderate")
        "aggressive" => @portfolio_manager -> #analyze_portfolio(holdings, "aggressive")
        _ => @portfolio_manager -> #analyze_portfolio(holdings, "moderate")
    }

    return result
}

-- Quick portfolio check pipeline
flow quick_check(holdings :: String) -> String {
    result = @portfolio_manager -> #fetch_market_data("S&P 500 today") |> @portfolio_manager -> #analyze_portfolio(holdings, "moderate")
    return result
}

-- Retirement planning with sub-flow
flow retirement_review(holdings :: String, tax_data :: String, age :: Int) -> String {
    portfolio = run annual_review(holdings, "moderate", tax_data, age)
    projection = @retirement_planner -> #project_retirement(age, 65, 500000.0, 2000.0, "moderate")
    return projection
}

-- Tax optimization with fallback
flow tax_review(holdings :: String, tax_data :: String) -> String {
    result = @tax_advisor -> #optimize_taxes(holdings, tax_data) ?> @portfolio_manager -> #analyze_portfolio(holdings, "moderate")
    return result
}

-- ── Tests ────────────────────────────────────────────────────────

test "portfolio analysis identifies concentration" {
    review = @portfolio_manager -> #analyze_portfolio("AAPL:50%, MSFT:30%, GOOGL:20%", "moderate")
    assert review
}

test "tax optimization finds harvesting opportunities" {
    taxes = @tax_advisor -> #optimize_taxes("AAPL: -$5000 unrealized", "married filing jointly, 24% bracket")
    assert taxes
}

test "retirement projection runs Monte Carlo" {
    projection = @retirement_planner -> #project_retirement(35, 65, 250000.0, 1500.0, "moderate")
    assert projection
}

test "full review pipeline completes" {
    result = run annual_review("diversified portfolio", "moderate", "single filer, $120K income", 40)
    assert result
}
