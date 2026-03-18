-- Coffee Website Builder — Full PACT Feature Showcase
-- Demonstrates every PACT language construct for the Mermaid agentflow spec.
--
-- Constructs used:
--   permit_tree, schema, type (alias), template, directive, tool, skill,
--   agent (with model, prompt, memory, skills), agent_bundle (with fallbacks),
--   flow (with pipeline, fallback chain, parallel, match, on_error, run,
--         record literals, field access, env(), prompt interpolation, assert),
--   test, import

import "shared/types.pact"

-- ══════════════════════════════════════════════════════════════════
-- Permission Tree
-- ══════════════════════════════════════════════════════════════════

permit_tree {
    ^llm {
        ^llm.query
        ^llm.vision
    }
    ^net {
        ^net.read
        ^net.write
    }
    ^fs {
        ^fs.read
        ^fs.write
    }
    ^sh {
        ^sh.exec
    }
}

-- ══════════════════════════════════════════════════════════════════
-- Schemas
-- ══════════════════════════════════════════════════════════════════

schema CoffeeShop {
    name :: String
    city :: String
    style :: String
    menu_count :: Int
}

schema ReviewResult {
    score :: Float
    passed :: Bool
    feedback :: String
    suggestions :: List<String>
}

schema PageAsset {
    html :: String
    css :: String
    preview_url :: String
}

-- ══════════════════════════════════════════════════════════════════
-- Type Aliases
-- ══════════════════════════════════════════════════════════════════

type Language = English | Swedish | Norwegian | Finnish
type DesignStyle = Minimalist | Rustic | Modern | Industrial
type ContentTone = Warm | Professional | Playful | Bold

-- ══════════════════════════════════════════════════════════════════
-- Templates — reusable output format specifications
-- ══════════════════════════════════════════════════════════════════

template %coffee_copy {
    HERO_TAGLINE :: String      <<one punchy headline for the hero>>
    HERO_SUBTITLE :: String     <<one compelling subtitle, max 20 words>>
    ABOUT_STORY :: String       <<two paragraphs about the shop's origin and values>>
    ABOUT_SOURCING :: String    <<paragraph about ethical bean sourcing>>
    MENU_ITEM :: String * 8     <<Name | Price | Tasting Notes>>
    CONTACT_CTA :: String       <<a warm call-to-action for the contact form>>
}

template %seo_metadata {
    TITLE :: String             <<page title, 60 chars max>>
    DESCRIPTION :: String       <<meta description, 160 chars max>>
    OG_IMAGE_ALT :: String      <<alt text for social media card>>
    section KEYWORDS             <<comma-separated SEO keywords for a coffee shop>>
}

template %bilingual_page {
    section PRIMARY   <<the original copy in the primary language>>
    section SECONDARY <<faithful translation in the secondary language>>
}

-- ══════════════════════════════════════════════════════════════════
-- Directives — composable prompt blocks
-- ══════════════════════════════════════════════════════════════════

directive %nordic_design {
    <<DESIGN SYSTEM: Use {heading_font} for headings, {body_font} for body text.
    Color palette: espresso brown #3C2415, cream #FFF8F0, sage green #8B9D77,
    amber accent #D4A574, slate blue #5B7B8A. Use CSS custom properties for
    all colors. Implement dark mode with prefers-color-scheme. When the
    language switches to {secondary_lang}, shift accents to cooler Nordic tones.>>
    params {
        heading_font :: String = "Playfair Display"
        body_font :: String = "Inter"
        secondary_lang :: String = "sv"
    }
}

directive %glassmorphism {
    <<LAYOUT: Fixed glassmorphism navbar with backdrop-filter: blur(12px).
    Full-viewport hero with parallax scrolling. Menu section as CSS Grid
    with hover card animations (translateY -4px, box-shadow elevation).
    About section with a split layout — text left, SVG illustration right.
    Contact form with floating labels. Sticky footer.>>
}

directive %scroll_animations {
    <<ANIMATIONS: CSS @keyframes with IntersectionObserver for scroll-triggered
    fade-in-up on each section. Hero title: typewriter reveal. Menu cards:
    stagger entrance by 100ms. Smooth parallax on scroll. Language toggle:
    cross-fade with 300ms opacity transition.>>
}

directive %accessibility {
    <<A11Y: All images need descriptive alt text. Use semantic HTML (nav, main,
    section, article, footer). Ensure {min_contrast_ratio}:1 contrast ratio.
    Support prefers-reduced-motion. Add skip-to-content link. Use aria-labels
    on interactive elements. Ensure tab order is logical.>>
    params {
        min_contrast_ratio :: String = "4.5"
    }
}

directive %bilingual_toggle {
    <<LANGUAGE TOGGLE: Prominent {lang_a}/{lang_b} pill toggle in navbar.
    Use data-lang attributes. Cross-fade text on switch. Shift accent colors
    to {nordic_accent} in {lang_b} mode. Persist preference in localStorage.>>
    params {
        lang_a :: String = "en"
        lang_b :: String = "sv"
        nordic_accent :: String = "#5B7B8A"
    }
}

-- ══════════════════════════════════════════════════════════════════
-- Tools — with all supported features
-- ══════════════════════════════════════════════════════════════════

tool #research_location {
    description: <<Research a city for coffee culture context: local roasters,
    popular neighborhoods, demographics, climate, and competitor landscape.
    Return a concise research brief.>>
    requires: [^net.read]
    source: ^search.duckduckgo(query)
    params {
        query :: String
    }
    returns :: String
    cache: "24h"
}

tool #write_copy {
    description: <<Write vivid marketing copy for a coffee shop website.
    Reference the local area. Make it feel authentic and alive. Follow the
    output template structure exactly.>>
    requires: [^llm.query]
    output: %coffee_copy
    params {
        brief :: String
        tone :: String
    }
    returns :: String
    retry: 2
    validate: strict
}

tool #translate {
    description: <<Translate marketing copy faithfully. Adapt idioms naturally.
    Keep section labels unchanged. Use informal address form in the target
    language (du-form in Swedish, du-form in Norwegian).>>
    requires: [^llm.query]
    output: %bilingual_page
    params {
        content :: String
        target_language :: String
    }
    returns :: String
    retry: 1
}

tool #generate_html {
    description: <<Generate a complete single-page HTML website with inline CSS
    and JS. Return ONLY raw HTML starting with DOCTYPE. No markdown. No
    explanations. Production-quality code.>>
    requires: [^llm.query]
    directives: [%nordic_design, %glassmorphism, %scroll_animations, %accessibility, %bilingual_toggle]
    params {
        content :: String
        shop_name :: String
    }
    returns :: String
    retry: 1
}

tool #generate_seo {
    description: <<Generate SEO metadata optimized for local coffee shop
    discovery. Include location-specific keywords.>>
    requires: [^llm.query]
    output: %seo_metadata
    params {
        shop_name :: String
        city :: String
        copy :: String
    }
    returns :: String
}

tool #review_quality {
    description: <<Review HTML quality: check accessibility, responsive design,
    performance patterns, semantic HTML. Score 0-100 with specific feedback.>>
    requires: [^llm.query, ^llm.vision]
    params {
        html :: String
        checklist :: List<String>
    }
    returns :: String
    validate: strict
}

tool #save_to_disk {
    description: <<Save content to the filesystem at the given path.>>
    requires: [^fs.write]
    handler: "sh echo '{content}' > {path}"
    params {
        path :: String
        content :: String
    }
    returns :: String
}

tool #deploy_preview {
    description: <<Deploy HTML to a preview server and return the URL.>>
    requires: [^net.write, ^sh.exec]
    handler: "sh npx serve {directory} --single --listen {port}"
    params {
        directory :: String
        port :: Int
    }
    returns :: String
}

-- ══════════════════════════════════════════════════════════════════
-- Skills — composite capabilities
-- ══════════════════════════════════════════════════════════════════

skill $content_pipeline {
    description: <<Research a location, write marketing copy, and translate
    it — producing complete bilingual content ready for design.>>
    tools: [#research_location, #write_copy, #translate]
    strategy: <<First research the location. Use the research brief to write
    English copy. Then translate to the target language. Return bilingual output.>>
    params {
        query :: String
        target_language :: String
        tone :: String
    }
    returns :: String
}

skill $quality_assurance {
    description: <<Review, score, and optionally regenerate content until it
    meets the quality threshold.>>
    tools: [#review_quality, #write_copy]
    strategy: <<Review the content against the checklist. If score is below
    the threshold, regenerate with feedback. Max 3 iterations.>>
    params {
        content :: String
        threshold :: Int
    }
    returns :: String
}

-- ══════════════════════════════════════════════════════════════════
-- Agents — with full feature set
-- ══════════════════════════════════════════════════════════════════

agent @researcher {
    permits: [^net.read, ^llm.query]
    tools: [#research_location, #write_copy]
    skills: [$content_pipeline]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a market research specialist and copywriter for premium
    coffee brands. Deliver actual content, never describe what you will do.
    Reference the local area by name. Be evocative and specific.>>
    memory: [~research_cache, ~brand_voice]
}

agent @translator {
    permits: [^llm.query]
    tools: [#translate]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a native translator specializing in hospitality marketing.
    Translate immediately — never explain or describe your process. Use
    informal address (du-form). Adapt idioms naturally.>>
    memory: [~glossary]
}

agent @designer {
    permits: [^llm.query, ^llm.vision]
    tools: [#generate_html, #generate_seo]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a senior frontend developer at a top creative agency. Your
    websites win Awwwards. Production-quality HTML/CSS/JS. Modern CSS: grid,
    custom properties, clamp(), backdrop-filter. Smooth IntersectionObserver
    animations. SVG illustrations instead of placeholder images. Only raw HTML.>>
}

agent @reviewer {
    permits: [^llm.query, ^llm.vision]
    tools: [#review_quality]
    skills: [$quality_assurance]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a QA engineer specializing in web accessibility and
    performance. Score objectively. Provide specific, actionable feedback
    with line references.>>
}

agent @publisher {
    permits: [^fs.write, ^net.write, ^sh.exec]
    tools: [#save_to_disk, #deploy_preview]
    prompt: <<You are a deployment specialist. Save files cleanly. Deploy
    previews reliably. Return the preview URL.>>
}

-- ══════════════════════════════════════════════════════════════════
-- Agent Bundle — with fallback chain
-- ══════════════════════════════════════════════════════════════════

agent_bundle @coffee_team {
    agents: [@researcher, @translator, @designer, @reviewer, @publisher]
    fallbacks: [@designer ?> @researcher]
}

-- ══════════════════════════════════════════════════════════════════
-- Flows — demonstrating all expression types
-- ══════════════════════════════════════════════════════════════════

-- Helper flow: classify the design style from a request string
flow classify_style(request :: String) -> String {
    result = match request {
        "minimal" => "Minimalist"
        "cozy" => "Rustic"
        "sleek" => "Modern"
        _ => "Modern"
    }
    return result
}

-- Helper flow: build a review checklist
flow build_checklist(style :: String) -> List<String> {
    base = ["semantic HTML", "contrast ratio >= 4.5", "responsive layout", "performance"]
    return base
}

-- Main flow: full pipeline with all expression features
flow build_coffee_site(shop :: CoffeeShop, language :: Language) -> PageAsset {
    -- Environment variable for output directory
    output_dir = env("PACT_OUTPUT_DIR")

    -- Construct the research query with prompt interpolation
    query = <<Best coffee culture in {shop.city}, specialty roasters, local vibe>>

    -- Step 1: Research (with caching — tool has cache: "24h")
    research = @researcher -> #research_location(query)

    -- Step 2: Classify design style and build checklist in parallel
    parallel_results = parallel {
        run classify_style(shop.style),
        run build_checklist(shop.style)
    }

    -- Step 3: Write copy with on_error fallback
    tone = "Warm"
    english_copy = @researcher -> #write_copy(research, tone)
        on_error <<Fallback copy: Welcome to {shop.name}, your neighborhood coffee shop in {shop.city}.>>

    -- Step 4: Translate — using pipeline operator
    bilingual = english_copy |> @translator -> #translate(english_copy, "Swedish")

    -- Step 5: Generate HTML and SEO metadata in parallel
    site_assets = parallel {
        @designer -> #generate_html(bilingual, shop.name),
        @designer -> #generate_seo(shop.name, shop.city, english_copy)
    }

    -- Step 6: Quality review with fallback chain
    html = site_assets
    review_result = @reviewer -> #review_quality(html, ["a11y", "responsive", "performance"])
        ?> @reviewer -> #review_quality(html, ["basic"])

    -- Step 7: Build the result record
    result = {
        html: html,
        css: "inline",
        preview_url: "pending"
    }

    -- Step 8: Save to disk
    save_path = output_dir + "/" + shop.name + ".html"
    @publisher -> #save_to_disk(save_path, html)

    return result
}

-- Simple flow for quick preview (demonstrates run expression)
flow quick_preview(shop_name :: String, city :: String) -> String {
    shop = {
        name: shop_name,
        city: city,
        style: "Modern",
        menu_count: 6
    }
    result = run build_coffee_site(shop, "English")
    return result.html
}

-- ══════════════════════════════════════════════════════════════════
-- Tests — inline verification
-- ══════════════════════════════════════════════════════════════════

test "classify_style returns Modern for unknown input" {
    result = run classify_style("unknown")
    assert result == "Modern"
}

test "classify_style returns Minimalist for minimal" {
    result = run classify_style("minimal")
    assert result == "Minimalist"
}

test "classify_style returns Rustic for cozy" {
    result = run classify_style("cozy")
    assert result == "Rustic"
}

test "build_checklist returns a list" {
    checklist = run build_checklist("Modern")
    assert checklist != ""
}
