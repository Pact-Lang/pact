-- Showcase 01: AI Podcast Studio
-- Generates podcast scripts, show notes, and audio production briefs.
-- Demonstrates: permit_tree, schemas, type aliases, templates, directives,
-- tools (source, handler, output, directives, retry, cache, validate),
-- agents, agent_bundle, skills, flows (parallel, pipeline, match, fallback, on_error),
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

schema Episode {
    id :: String
    title :: String
    topic :: String
    duration_minutes :: Int
    guests :: List<String>
    status :: String
}

schema ShowNotes {
    summary :: String
    timestamps :: List<String>
    links :: List<String>
    keywords :: List<String>
}

schema AudioBrief {
    intro_cue :: String
    music_style :: String
    transition_notes :: String
    outro_cue :: String
}

-- ── Type Aliases ─────────────────────────────────────────────────

type EpisodeFormat = Interview | Solo | Panel | Debate | Narrative
type AudioMood = Upbeat | Calm | Dramatic | Conversational | Energetic

-- ── Templates ────────────────────────────────────────────────────

template %script_format {
    section OPENING
    HOOK :: String                  <<attention-grabbing opening line>>
    INTRO :: String                 <<host introduction and episode preview>>
    section BODY
    SEGMENT :: String * 4           <<Segment title | Duration | Key points | Transition>>
    section CLOSING
    RECAP :: String                 <<key takeaways summary>>
    CALL_TO_ACTION :: String        <<listener engagement prompt>>
    OUTRO :: String                 <<sign-off with next episode teaser>>
}

template %show_notes_format {
    section METADATA
    TITLE :: String                 <<episode title for publishing>>
    DESCRIPTION :: String           <<2-3 sentence episode description>>
    section CONTENT
    TIMESTAMP :: String * 8         <<MM:SS | Topic discussed>>
    KEY_TAKEAWAY :: String * 3      <<numbered key insight>>
    section RESOURCES
    LINK :: String * 5              <<Resource name | URL | Context>>
}

-- ── Directives ───────────────────────────────────────────────────

directive %conversational_tone {
    <<TONE: Write as natural spoken dialogue. Use contractions, rhetorical questions,
    and conversational transitions like "So here's the thing..." or "You might be wondering...".
    Avoid academic jargon. Sound like two smart friends talking over coffee.
    Target reading level: {reading_level}. Average sentence length: 12-18 words.>>
    params {
        reading_level :: String = "grade 8"
    }
}

directive %audio_production {
    <<AUDIO CUES: Mark transitions with [MUSIC: style]. Mark emphasis with [PAUSE: Xs].
    Indicate sound effects with [SFX: description]. Music style: {default_music}.
    Each segment should open with a distinct musical cue to aid listener navigation.
    Include [TRANSITION] markers between segments.>>
    params {
        default_music :: String = "lo-fi ambient"
    }
}

-- ── Tools ────────────────────────────────────────────────────────

tool #research_topic {
    description: <<Research a podcast topic thoroughly. Gather recent developments, expert opinions, statistics, and interesting angles. Return a structured research brief with at least 5 key findings and 3 potential guest suggestions.>>
    requires: [^net.read]
    source: ^search.duckduckgo(query)
    retry: 2
    cache: "1h"
    params {
        query :: String
    }
    returns :: String
}

tool #write_script {
    description: <<Write an engaging podcast script from a research brief. Structure it with a hook, segmented body, and memorable closing. Include audio production cues. Make it sound natural when read aloud — not like a blog post.>>
    requires: [^llm.query]
    output: %script_format
    directives: [%conversational_tone, %audio_production]
    validate: strict
    params {
        research :: String
        format :: String
        duration :: Int
    }
    returns :: String
}

tool #generate_show_notes {
    description: <<Generate comprehensive show notes from a podcast script. Extract timestamps, key takeaways, and resource links. Optimize the description for podcast directories (Apple, Spotify). Include relevant keywords for discoverability.>>
    requires: [^llm.query]
    output: %show_notes_format
    params {
        script :: String
    }
    returns :: String
}

tool #create_audio_brief {
    description: <<Create a detailed audio production brief from a script. Specify music cues, transition styles, sound effects, and pacing notes for each segment. Output should be actionable by an audio engineer or AI audio tool.>>
    requires: [^llm.query]
    params {
        script :: String
        mood :: String
    }
    returns :: String
}

tool #publish_episode {
    description: <<Publish episode metadata to the podcast hosting platform via API.>>
    requires: [^net.write]
    handler: "http POST https://api.podcast-host.example.com/episodes"
    retry: 3
    params {
        title :: String
        notes :: String
        audio_url :: String
    }
    returns :: String
}

tool #save_script {
    description: <<Save the podcast script to the local filesystem.>>
    requires: [^fs.write]
    source: ^fs.write_file(path, content)
    params {
        path :: String
        content :: String
    }
    returns :: String
}

-- ── Skills ───────────────────────────────────────────────────────

skill $full_production {
    description: <<End-to-end podcast production: research, script, show notes, and audio brief in one coordinated workflow.>>
    tools: [#research_topic, #write_script, #generate_show_notes, #create_audio_brief]
    strategy: <<sequential — research first, then script from research, then show notes and audio brief can run in parallel since both depend only on the script>>
    params {
        topic :: String
        format :: String
    }
    returns :: String
}

-- ── Agents ───────────────────────────────────────────────────────

agent @researcher {
    permits: [^net.read, ^llm.query]
    tools: [#research_topic]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a podcast research specialist. You find compelling angles, recent data, and expert perspectives on any topic. You think like a producer — always asking "why would a listener care about this?" Deliver structured research briefs that a scriptwriter can immediately use. Never pad with filler.>>
}

agent @scriptwriter {
    permits: [^llm.query]
    tools: [#write_script]
    skills: [$full_production]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a veteran podcast scriptwriter. You have written for top-10 shows across tech, culture, and business. Your scripts sound natural, engaging, and paced for audio. You use the "hook-explore-resolve" structure. Every segment earns the listener's attention. You include audio cues for the production team.>>
    memory: [~episode_archive]
}

agent @producer {
    permits: [^llm.query]
    tools: [#generate_show_notes, #create_audio_brief]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a podcast producer who handles post-production planning. You create show notes optimized for discoverability and audio briefs that guide the engineering team. You think about the listener experience from discovery through the final second of the episode.>>
}

agent @publisher {
    permits: [^net.write, ^fs.write]
    tools: [#publish_episode, #save_script]
    prompt: <<You are a publishing automation agent. You handle saving files and pushing metadata to hosting platforms. Execute operations precisely and report results. Never fabricate confirmations.>>
}

agent_bundle @podcast_team {
    agents: [@researcher, @scriptwriter, @producer, @publisher]
    fallbacks: @scriptwriter ?> @researcher
}

-- ── MCP Connections ──────────────────────────────────────────────

connect {
    brave_search   "stdio npx @anthropic/mcp-server-brave"
    filesystem     "stdio npx @anthropic/mcp-server-filesystem"
}

-- ── Lessons ──────────────────────────────────────────────────────

lesson "script_length_overshoot" {
    context: <<Scripts consistently ran 20% longer than target duration when read aloud>>
    rule: <<Always target 130 words per minute for spoken content — a 30-minute episode needs ~3,900 words of script, not 5,000>>
    severity: warning
}

lesson "show_notes_seo" {
    context: <<Episodes without keyword-rich show notes got 40% fewer organic downloads>>
    rule: <<Include at least 5 relevant keywords in show notes description and use them naturally in timestamps>>
    severity: info
}

-- ── Flows ────────────────────────────────────────────────────────

-- Full production pipeline: research → script → parallel(notes, audio) → publish
flow produce_episode(topic :: String, format :: String, duration :: Int) -> String {
    -- Step 1: Deep research on the topic
    research = @researcher -> #research_topic(topic)

    -- Step 2: Write the script from research
    script = @scriptwriter -> #write_script(research, format, duration)

    -- Step 3: Generate show notes and audio brief in parallel
    parallel {
        notes = @producer -> #generate_show_notes(script)
        audio_brief = @producer -> #create_audio_brief(script, "conversational")
    }

    -- Step 4: Save script to filesystem (with error recovery)
    saved = @publisher -> #save_script("episodes/latest.md", script) on_error <<Save skipped — disk unavailable>>

    return script
}

-- Quick script: research piped directly into scriptwriting
flow quick_script(topic :: String) -> String {
    result = @researcher -> #research_topic(topic) |> @scriptwriter -> #write_script(result, "solo", 15)
    return result
}

-- Format-aware production: match on episode format
flow format_production(topic :: String, format :: String) -> String {
    research = @researcher -> #research_topic(topic)

    script = match format {
        "interview" => @scriptwriter -> #write_script(research, "interview", 45)
        "solo" => @scriptwriter -> #write_script(research, "solo", 20)
        "panel" => @scriptwriter -> #write_script(research, "panel", 60)
        _ => @scriptwriter -> #write_script(research, "narrative", 30)
    }

    return script
}

-- ── Tests ────────────────────────────────────────────────────────

test "research returns structured brief" {
    brief = @researcher -> #research_topic("AI in healthcare 2026")
    assert brief
}

test "script follows template format" {
    script = @scriptwriter -> #write_script("Sample research brief about AI trends", "solo", 20)
    assert script
}

test "parallel production completes" {
    notes = @producer -> #generate_show_notes("Sample script content")
    assert notes
}

test "full pipeline produces episode" {
    result = run produce_episode("quantum computing breakthroughs", "interview", 30)
    assert result
}
