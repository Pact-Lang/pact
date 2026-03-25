-- Showcase 08: Content Marketing Engine
-- Multi-channel content creation, SEO optimization, and distribution.
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
        ^net.write
    }
    ^fs {
        ^fs.read
        ^fs.write
    }
}

-- ── Schemas ──────────────────────────────────────────────────────

schema ContentBrief {
    topic :: String
    target_audience :: String
    keywords :: List<String>
    content_type :: String
    word_count :: Int
    tone :: String
}

schema SEOAnalysis {
    primary_keyword :: String
    search_volume :: Int
    difficulty :: Float
    related_keywords :: List<String>
    top_competitors :: List<String>
    content_gap :: String
}

schema DistributionPlan {
    channels :: List<String>
    schedule :: String
    adaptations :: List<String>
    tracking_links :: List<String>
}

-- ── Type Aliases ─────────────────────────────────────────────────

type ContentType = BlogPost | Newsletter | SocialThread | WhitePaper | CaseStudy | Infographic
type Channel = Blog | Twitter | LinkedIn | Newsletter | Medium | YouTube
type Tone = Professional | Casual | Technical | Inspirational | Educational

-- ── Templates ────────────────────────────────────────────────────

template %blog_post {
    section META
    TITLE :: String                     <<SEO-optimized title with primary keyword>>
    META_DESCRIPTION :: String          <<155-character meta description>>
    SLUG :: String                      <<URL-friendly slug>>
    section CONTENT
    INTRODUCTION :: String              <<hook + thesis statement + what reader will learn>>
    SECTION :: String * 5               <<H2 heading | Content (300-500 words) | Key takeaway>>
    section ENGAGEMENT
    CONCLUSION :: String                <<summary + CTA>>
    FAQ :: String * 3                   <<Question (from People Also Ask) | Answer>>
}

template %social_adaptation {
    section TWITTER
    TWEET :: String * 5                 <<tweet text (max 280 chars) with hashtags>>
    THREAD_HOOK :: String               <<first tweet that hooks readers into thread>>
    section LINKEDIN
    POST :: String                      <<professional long-form post (1300 chars)>>
    HEADLINE :: String                  <<attention-grabbing headline>>
    section NEWSLETTER
    SUBJECT_LINE :: String              <<email subject line (max 50 chars)>>
    PREVIEW :: String                   <<preview text (max 90 chars)>>
    BODY :: String                      <<newsletter adaptation of the content>>
}

-- ── Directives ───────────────────────────────────────────────────

directive %seo_optimization {
    <<SEO RULES: Primary keyword must appear in title, H1, first 100 words, and at least 2 H2s.
    Keyword density: {density_target} (not more). Use semantic variations and LSI keywords naturally.
    Include internal links to {internal_link_count} related articles. External links to authoritative
    sources only (DR 50+). Alt text on all images. Schema markup for FAQ sections.
    Target featured snippet format: paragraph (40-60 words) or list (5-8 items) matching query intent.
    Readability: Flesch score above {readability_target}.>>
    params {
        density_target :: String = "1-2%"
        internal_link_count :: String = "3-5"
        readability_target :: String = "60"
    }
}

directive %brand_voice {
    <<BRAND: Write as {brand_name}. Voice: {voice_traits}. Avoid: {avoid_words}.
    Always use active voice. Sentences max 20 words average. Paragraphs max 3 sentences.
    Use data and specific examples — never vague claims like "many" or "significant".
    Every section must deliver value — if it doesn't teach, persuade, or entertain, cut it.>>
    params {
        brand_name :: String = "our brand"
        voice_traits :: String = "confident, helpful, direct, slightly witty"
        avoid_words :: String = "leverage, synergy, utilize, robust, scalable"
    }
}

-- ── Tools ────────────────────────────────────────────────────────

tool #keyword_research {
    description: <<Research keywords for a content topic. Identify primary keyword, search volume, difficulty score, related long-tail keywords, and content gaps in competitor coverage. Analyze top-ranking content to identify what's missing. Return structured SEO analysis.>>
    requires: [^net.read, ^llm.query]
    source: ^search.duckduckgo(query)
    cache: "6h"
    retry: 2
    params {
        query :: String
        niche :: String
    }
    returns :: String
}

tool #write_article {
    description: <<Write a long-form blog article optimized for SEO and reader engagement. Structure with clear H2/H3 hierarchy. Include data points, examples, and actionable takeaways in every section. Write a compelling introduction that hooks within the first 2 sentences. End with a clear CTA and FAQ section targeting People Also Ask queries.>>
    requires: [^llm.query]
    output: %blog_post
    directives: [%seo_optimization, %brand_voice]
    validate: strict
    params {
        brief :: String
        seo_data :: String
        word_count :: Int
    }
    returns :: String
}

tool #adapt_for_social {
    description: <<Adapt long-form content into platform-specific social media posts. Twitter: punchy threads with hooks. LinkedIn: professional thought leadership. Newsletter: conversational email format. Maintain core message while optimizing for each platform's algorithm and audience expectations.>>
    requires: [^llm.query]
    output: %social_adaptation
    directives: [%brand_voice]
    params {
        article :: String
        platforms :: String
    }
    returns :: String
}

tool #analyze_competitors {
    description: <<Analyze top-ranking content for a keyword. Identify content structure, word count, topics covered, backlink profile, and engagement metrics. Find content gaps and angles not yet covered. Suggest differentiation strategy.>>
    requires: [^net.read, ^llm.query]
    cache: "12h"
    params {
        keyword :: String
        top_n :: Int
    }
    returns :: String
}

tool #schedule_distribution {
    description: <<Create a distribution schedule for content across all channels. Determine optimal posting times for each platform based on audience timezone. Generate tracking links for attribution. Plan content repurposing cadence.>>
    requires: [^llm.query]
    params {
        content :: String
        channels :: String
        timezone :: String
    }
    returns :: String
}

tool #publish_content {
    description: <<Publish content to the CMS and trigger distribution to configured channels.>>
    requires: [^net.write]
    handler: "http POST https://cms.example.com/api/posts"
    retry: 3
    params {
        content :: String
        metadata :: String
    }
    returns :: String
}

tool #save_draft {
    description: <<Save content draft to the local filesystem for review.>>
    requires: [^fs.write]
    source: ^fs.write_file(path, content)
    params {
        path :: String
        content :: String
    }
    returns :: String
}

-- ── Skills ───────────────────────────────────────────────────────

skill $content_creation {
    description: <<End-to-end content creation: keyword research, competitive analysis, article writing, and social adaptation in one coordinated workflow.>>
    tools: [#keyword_research, #analyze_competitors, #write_article, #adapt_for_social]
    strategy: <<Research and competitive analysis in parallel, then write article informed by both, then adapt for social channels>>
    params {
        topic :: String
        niche :: String
    }
    returns :: String
}

-- ── Agents ───────────────────────────────────────────────────────

agent @seo_strategist {
    permits: [^net.read, ^llm.query]
    tools: [#keyword_research, #analyze_competitors]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are an SEO strategist with deep understanding of search algorithms and user intent. You find keyword opportunities that balance search volume with competition difficulty. You analyze competitors not to copy them but to find what they missed. You think in terms of topical authority and content clusters, not individual keywords.>>
    memory: [~keyword_database, ~competitor_map]
}

agent @writer {
    permits: [^llm.query]
    tools: [#write_article]
    skills: [$content_creation]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a content marketing writer who combines storytelling with SEO expertise. You write articles that rank AND engage. Every sentence earns its place — you never pad with filler. You use data to persuade, stories to connect, and structure to guide. Your introductions hook within 2 sentences. Your conclusions inspire action. You write at a pace of 10 quality pieces per month.>>
    memory: [~style_guide, ~content_archive]
}

agent @social_manager {
    permits: [^llm.query, ^net.write]
    tools: [#adapt_for_social, #schedule_distribution, #publish_content]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a social media manager who understands each platform's algorithm and culture. You adapt content without diluting the message. Your Twitter threads get bookmarked. Your LinkedIn posts get shared by executives. Your newsletters get opened. You optimize posting times and track engagement relentlessly.>>
}

agent @editor {
    permits: [^fs.write, ^llm.query]
    tools: [#save_draft]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a content editor. You save drafts and review outputs for quality. You catch errors that writers miss. You ensure brand voice consistency across all content.>>
}

agent_bundle @content_team {
    agents: [@seo_strategist, @writer, @social_manager, @editor]
    fallbacks: @writer ?> @seo_strategist
}

-- ── MCP Connections ──────────────────────────────────────────────

connect {
    brave_search   "stdio npx @anthropic/mcp-server-brave"
    filesystem     "stdio npx @anthropic/mcp-server-filesystem"
}

-- ── Lessons ──────────────────────────────────────────────────────

lesson "keyword_cannibalization" {
    context: <<Two blog posts targeting the same keyword competed against each other, dropping both from page 1>>
    rule: <<Before writing new content, check existing content for keyword overlap — consolidate or differentiate, never compete with yourself>>
    severity: warning
}

lesson "social_adaptation_depth" {
    context: <<Shallow social posts that just quoted article headlines got 70% less engagement than posts with unique angles>>
    rule: <<Social adaptations must add unique value — a different angle, a hot take, a specific data point — not just summarize the article>>
    severity: info
}

lesson "publish_without_review" {
    context: <<Article published with factual error about a competitor's pricing, requiring public correction>>
    rule: <<All content containing specific claims, statistics, or competitor mentions must have a fact-check step before publishing>>
    severity: error
}

-- ── Flows ────────────────────────────────────────────────────────

-- Full content production pipeline
flow create_content(topic :: String, niche :: String, word_count :: Int) -> String {
    -- Step 1: Research in parallel
    parallel {
        seo = @seo_strategist -> #keyword_research(topic, niche)
        competitors = @seo_strategist -> #analyze_competitors(topic, 5)
    }

    -- Step 2: Write the article
    article = @writer -> #write_article(topic, seo, word_count)

    -- Step 3: Adapt and schedule in parallel
    parallel {
        social = @social_manager -> #adapt_for_social(article, "twitter,linkedin,newsletter")
        schedule = @social_manager -> #schedule_distribution(article, "all", "US-Eastern")
    }

    -- Step 4: Save draft for review
    saved = @editor -> #save_draft("drafts/latest.md", article) on_error <<Draft save skipped>>

    return article
}

-- Content type routing with match
flow create_by_type(topic :: String, content_type :: String) -> String {
    seo = @seo_strategist -> #keyword_research(topic, "technology")

    result = match content_type {
        "blog" => @writer -> #write_article(topic, seo, 2000)
        "whitepaper" => @writer -> #write_article(topic, seo, 5000)
        "newsletter" => @social_manager -> #adapt_for_social(topic, "newsletter")
        _ => @writer -> #write_article(topic, seo, 1500)
    }

    return result
}

-- Quick SEO research pipeline
flow quick_research(topic :: String) -> String {
    result = @seo_strategist -> #keyword_research(topic, "general") |> @seo_strategist -> #analyze_competitors(result, 3)
    return result
}

-- Publish with fallback
flow publish_article(topic :: String, niche :: String) -> String {
    article = run create_content(topic, niche, 2000)
    published = @social_manager -> #publish_content(article, "blog") ?> @editor -> #save_draft("drafts/failed_publish.md", article)
    return published
}

-- ── Tests ────────────────────────────────────────────────────────

test "keyword research returns actionable data" {
    seo = @seo_strategist -> #keyword_research("AI productivity tools", "SaaS")
    assert seo
}

test "article follows SEO template" {
    article = @writer -> #write_article("AI productivity", "keyword: ai tools, vol: 5400", 1500)
    assert article
}

test "social adaptation covers all platforms" {
    social = @social_manager -> #adapt_for_social("Sample article about AI trends", "twitter,linkedin")
    assert social
}

test "full content pipeline produces article" {
    result = run create_content("machine learning ops", "DevOps", 2000)
    assert result
}
