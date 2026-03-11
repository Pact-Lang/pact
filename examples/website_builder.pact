-- Created: 2026-03-10
-- Website Builder Agent (Multi-Agent Edition)
-- Researches a location, translates to Swedish, and builds a bilingual website.
-- Demonstrates: templates, directives, source providers, multi-agent orchestration.

permit_tree {
    ^llm {
        ^llm.query
    }
    ^net {
        ^net.read
    }
}

-- ── Templates ────────────────────────────────────────────────────
-- Reusable output format specifications.

template %website_copy {
    HERO_TAGLINE :: String      <<one powerful headline>>
    HERO_SUBTITLE :: String     <<one compelling subtitle>>
    ABOUT :: String             <<two paragraphs about the coffee shop, its story, values>>
    MENU_ITEM :: String * 6     <<Name | Price | Description>>
}

template %bilingual {
    section ENGLISH  <<paste the original English copy exactly as received>>
    section SWEDISH  <<translate every line to Swedish, keep section labels unchanged>>
}

-- ── Directives ───────────────────────────────────────────────────
-- Composable prompt blocks for the designer agent.

directive %scandinavian_design {
    <<DESIGN: Use Google Fonts ({heading_font} for headings, {body_font} for body).
    Rich color palette matching a Scandinavian coffee brand — deep espresso browns,
    warm creams, muted sage greens, and golden amber accents. Use CSS custom properties
    for theming. When switching to Swedish, subtly shift the color palette to cooler
    Nordic tones (slate blue accents, softer whites).>>
    params {
        heading_font :: String = "Playfair Display"
        body_font :: String = "Inter"
    }
}

directive %glassmorphism_layout {
    <<LAYOUT: Fixed glassmorphism navbar with backdrop-filter blur. Full-viewport hero
    with parallax scrolling effect. Menu section as a CSS Grid with hover card animations.
    About section with a split layout. Contact form with floating labels.
    Sticky footer with {footer_text}.>>
    params {
        footer_text :: String = "Made with love by PACT"
    }
}

directive %scroll_animations {
    <<ANIMATIONS: Use CSS @keyframes and IntersectionObserver for scroll-triggered
    fade-in-up animations on every section. Hero title should have a typewriter reveal
    effect. Menu cards should stagger their entrance. Smooth parallax on scroll.
    Language toggle should animate with a slide transition — cross-fade the content.>>
}

directive %bilingual_toggle {
    <<LANGUAGE TOGGLE: Prominent {lang_a}/{lang_b} pill toggle in the navbar.
    Use data-{lang_a} and data-{lang_b} attributes. When switching languages,
    cross-fade all text with a 300ms opacity transition. Shift accent colors
    to Nordic palette in {lang_b} mode.>>
    params {
        lang_a :: String = "en"
        lang_b :: String = "sv"
    }
}

-- ── Tools ────────────────────────────────────────────────────────

tool #research_location {
    description: <<Research a city or location to gather useful local context for a business: culture, demographics, popular neighborhoods, local competitors, weather, and tips. Return a concise research brief.>>
    requires: [^net.read]
    source: ^search.duckduckgo(query)
    params {
        query :: String
    }
    returns :: String
}

tool #write_copy {
    description: <<Write vivid, evocative marketing copy for a coffee shop website. Reference the local area by name. Make it feel real and alive.>>
    requires: [^llm.query]
    output: %website_copy
    params {
        brief :: String
    }
    returns :: String
}

tool #translate_to_swedish {
    description: <<Translate marketing copy with warmth. Adapt idioms naturally. Use du-form. Keep section labels like HERO_TAGLINE, MENU_ITEM_1 etc unchanged — only translate the actual copy text.>>
    requires: [^llm.query]
    output: %bilingual
    params {
        english_copy :: String
    }
    returns :: String
}

tool #generate_html {
    description: <<Generate a complete one-page HTML website with inline CSS and JS. You receive bilingual copy (English + Swedish). Build a world-class bilingual site. Return ONLY raw HTML starting with DOCTYPE. No markdown fences. No explanations.>>
    requires: [^llm.query]
    directives: [%scandinavian_design, %glassmorphism_layout, %scroll_animations, %bilingual_toggle]
    params {
        content :: String
    }
    returns :: String
}

-- ── Agents ───────────────────────────────────────────────────────

agent @researcher {
    permits: [^net.read, ^llm.query]
    tools: [#research_location, #write_copy]
    prompt: <<You are a market research specialist and copywriter. When given a tool result, use the information to produce real output — never describe what you did, just deliver the content. When writing copy, write the actual words that will appear on the website. Be creative, specific, and evocative. Reference the local area by name.>>
}

agent @translator {
    permits: [^llm.query]
    tools: [#translate_to_swedish]
    prompt: <<You are a native Swedish translator specializing in marketing and hospitality. When you receive English copy, immediately translate it. Never describe what you are doing — just output the bilingual content in the exact format requested by the tool. Use du-form. Adapt idioms naturally.>>
}

agent @designer {
    permits: [^llm.query]
    tools: [#generate_html]
    prompt: <<You are a senior frontend developer and UI designer at a top creative agency. You obsess over typography, whitespace, micro-interactions, and visual hierarchy. Your websites win Awwwards. You write production-quality HTML/CSS/JS with no shortcuts. You use modern CSS (grid, custom properties, clamp(), backdrop-filter, scroll-behavior). You implement smooth scroll-triggered animations with IntersectionObserver. You never use placeholder images — instead you create beautiful SVG illustrations or CSS art. Your designs feel premium, editorial, and alive. Never output markdown — only raw HTML.>>
}

agent_bundle @website_team {
    agents: [@researcher, @translator, @designer]
}

-- ── Flow ─────────────────────────────────────────────────────────

flow build_bilingual_site(request :: String) -> String {
    -- Step 1: Research the location
    research = @researcher -> #research_location(request)

    -- Step 2: Write English marketing copy based on research
    english_copy = @researcher -> #write_copy(research)

    -- Step 3: Translate to Swedish
    swedish_copy = @translator -> #translate_to_swedish(english_copy)

    -- Step 4: Build the bilingual website with both languages
    html = @designer -> #generate_html(swedish_copy)

    return html
}
