-- Showcase 10: Interactive Game Narrative Engine
-- AI-driven story generation, character dialogue, and quest design for RPGs.
-- Demonstrates: permit_tree, schemas, type aliases, templates, directives,
-- tools (source, handler, output, directives, retry, cache, validate),
-- agents, agent_bundle, skills, flows (parallel, match, pipeline, fallback, on_error, run),
-- lessons, connect, tests.

permit_tree {
    ^llm {
        ^llm.query
        ^llm.embed
    }
    ^fs {
        ^fs.read
        ^fs.write
    }
    ^db {
        ^db.read
        ^db.write
    }
}

-- ── Schemas ──────────────────────────────────────────────────────

schema Character {
    name :: String
    role :: String
    personality :: String
    backstory :: String
    motivation :: String
    speech_pattern :: String
    relationships :: List<String>
}

schema QuestLine {
    id :: String
    title :: String
    description :: String
    objectives :: List<String>
    rewards :: List<String>
    branches :: List<String>
    difficulty :: String
}

schema WorldRegion {
    name :: String
    biome :: String
    inhabitants :: List<String>
    history :: String
    points_of_interest :: List<String>
    dangers :: List<String>
}

schema DialogueNode {
    speaker :: String
    text :: String
    emotion :: String
    choices :: List<String>
    consequences :: List<String>
}

-- ── Type Aliases ─────────────────────────────────────────────────

type CharacterRole = Protagonist | Antagonist | Mentor | Companion | Merchant | QuestGiver | Villain
type QuestType = MainStory | SideQuest | FetchQuest | EscortMission | BossEncounter | PuzzleChallenge
type Emotion = Neutral | Angry | Sad | Excited | Fearful | Mysterious | Sarcastic | Hopeful

-- ── Templates ────────────────────────────────────────────────────

template %quest_document {
    section OVERVIEW
    TITLE :: String                     <<quest title that intrigues the player>>
    HOOK :: String                      <<the narrative hook that draws the player in>>
    CONTEXT :: String                   <<how this quest fits into the larger story>>
    section STRUCTURE
    OBJECTIVE :: String * 5             <<Step | Description | Location | Challenge type>>
    BRANCH_POINT :: String * 2          <<Decision | Option A outcome | Option B outcome>>
    section CHARACTERS
    NPC :: String * 3                   <<Name | Role in quest | Key dialogue line>>
    section REWARDS
    REWARD :: String * 3                <<Type | Item/XP | Condition>>
    HIDDEN_SECRET :: String             <<easter egg or hidden reward for thorough exploration>>
}

template %dialogue_tree {
    section OPENING
    GREETING :: String                  <<NPC's opening line based on context>>
    MOOD :: String                      <<NPC's emotional state and body language>>
    section CONVERSATION
    EXCHANGE :: String * 6              <<Player choice | NPC response | Relationship change>>
    section BRANCHING
    OUTCOME :: String * 3               <<Path taken | Narrative consequence | Flag set>>
}

template %lore_entry {
    section HISTORY
    ERA :: String                       <<historical period this lore belongs to>>
    EVENT :: String                     <<key historical event>>
    IMPACT :: String                    <<how this event shaped the current world>>
    section DETAILS
    ARTIFACT :: String * 2              <<Name | Description | Significance>>
    LEGEND :: String                    <<local myth or legend that hints at deeper truth>>
}

-- ── Directives ───────────────────────────────────────────────────

directive %narrative_voice {
    <<WRITING STYLE: Write in {voice_style} style. Show, don't tell. Use sensory details —
    what does the player see, hear, smell? Every NPC has a distinct speech pattern:
    scholars use formal syntax, rogues use slang, ancient beings speak in verse.
    Dialogue must reveal character — never use generic lines like "Hello, adventurer."
    Environmental storytelling: describe the world in ways that hint at its history.
    Player choices must have meaningful consequences — no illusion of choice.>>
    params {
        voice_style :: String = "dark fantasy, lyrical, with dry humor"
    }
}

directive %world_consistency {
    <<WORLD RULES: The world of {world_name} follows these rules:
    1. Magic costs — every spell has a price paid in memory, years, or pain.
    2. No absolute good or evil — every faction believes they're right.
    3. History repeats — current conflicts echo ancient ones.
    4. The dead don't stay dead easily — necromancy is common but taboo.
    5. Technology is {tech_level} — no anachronisms.
    Maintain internal consistency. If you establish a rule, never violate it.
    Cross-reference existing lore before creating new elements.>>
    params {
        world_name :: String = "Aethermoor"
        tech_level :: String = "late medieval with alchemical innovation"
    }
}

-- ── Tools ────────────────────────────────────────────────────────

tool #create_character {
    description: <<Design a fully realized NPC with personality, backstory, motivation, speech pattern, and relationships. The character must feel like a real person with contradictions, desires, and secrets. Include 3 signature dialogue lines that capture their voice. Include hidden depths that players can discover through repeated interactions.>>
    requires: [^llm.query]
    directives: [%narrative_voice, %world_consistency]
    validate: strict
    params {
        role :: String
        context :: String
        relationships :: String
    }
    returns :: String
}

tool #design_quest {
    description: <<Design a branching quest with meaningful choices, multiple objectives, and narrative consequences. Include at least 2 decision points where the player's choice affects the story outcome. Every objective should feel purposeful — no padding. Include environmental puzzles, moral dilemmas, or combat encounters. Hidden paths reward exploration.>>
    requires: [^llm.query]
    output: %quest_document
    directives: [%narrative_voice, %world_consistency]
    validate: strict
    params {
        quest_type :: String
        setting :: String
        involved_characters :: String
    }
    returns :: String
}

tool #write_dialogue {
    description: <<Write a branching dialogue tree between the player and an NPC. Each exchange must feel natural and reveal character. Player choices should range from diplomatic to confrontational. NPC responses must adapt to player's tone. Include consequences — what the player says affects NPC relationships and future quests. Embed lore naturally in conversation.>>
    requires: [^llm.query]
    output: %dialogue_tree
    directives: [%narrative_voice]
    validate: strict
    params {
        npc :: String
        context :: String
        player_reputation :: String
    }
    returns :: String
}

tool #build_lore {
    description: <<Create a piece of world lore: historical event, artifact, legend, or cultural tradition. Must connect to existing world elements. Include subtle foreshadowing for future quests. Write as an in-world document (scholar's note, tavern tale, ancient inscription) for immersion.>>
    requires: [^llm.query]
    output: %lore_entry
    directives: [%world_consistency]
    cache: "24h"
    params {
        topic :: String
        era :: String
        connection :: String
    }
    returns :: String
}

tool #generate_region {
    description: <<Design a game world region with distinct biome, inhabitants, history, points of interest, and dangers. Include 5 discoverable locations with narrative hooks. Each location should tell a story through its environment. Include day/night cycle variations and weather effects on gameplay.>>
    requires: [^llm.query]
    directives: [%world_consistency, %narrative_voice]
    params {
        biome :: String
        adjacent_regions :: String
        political_situation :: String
    }
    returns :: String
}

tool #save_narrative {
    description: <<Save narrative content (quests, dialogue, lore) to the game database.>>
    requires: [^db.write]
    handler: "http POST https://api.game-db.example.com/narrative"
    retry: 3
    params {
        category :: String
        content :: String
    }
    returns :: String
}

tool #export_script {
    description: <<Export the narrative content as a game script file.>>
    requires: [^fs.write]
    source: ^fs.write_file(path, content)
    params {
        path :: String
        content :: String
    }
    returns :: String
}

-- ── Skills ───────────────────────────────────────────────────────

skill $quest_package {
    description: <<Create a complete quest package: design the quest, create involved NPCs, write all dialogue trees, and build supporting lore — everything needed to implement the quest in-game.>>
    tools: [#design_quest, #create_character, #write_dialogue, #build_lore]
    strategy: <<Design quest first to establish structure, then create characters in parallel with lore, then write dialogue last since it needs both characters and quest context>>
    params {
        quest_type :: String
        setting :: String
    }
    returns :: String
}

-- ── Agents ───────────────────────────────────────────────────────

agent @narrative_director {
    permits: [^llm.query]
    tools: [#design_quest, #generate_region]
    skills: [$quest_package]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a lead narrative designer for an award-winning RPG studio. You create quests that players remember years later. You believe that the best stories emerge from player choice — not scripted cinematics. Every quest you design has at least one moment where the player says "I didn't expect that." You balance epic world-shaking events with intimate character moments. Your quest structures are tight — no filler content, no fetch quests disguised as story missions.>>
    memory: [~world_bible, ~quest_graph]
}

agent @character_writer {
    permits: [^llm.query]
    tools: [#create_character, #write_dialogue]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a character writer who creates NPCs that feel alive. Every character has a want, a need, and a secret. Their dialogue reveals who they are — a nervous merchant stutters, a weary knight speaks in short declarative sentences, an ancient dragon uses archaic verse. You write dialogue that players screenshot and share. You understand that the most memorable characters are the ones who surprise the player.>>
    memory: [~character_registry, ~relationship_map]
}

agent @lore_keeper {
    permits: [^llm.query, ^llm.embed]
    tools: [#build_lore]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are the keeper of world lore for a deep fantasy RPG. You maintain internal consistency across hundreds of interconnected lore entries. You write as if you are a scholar within the world — each piece of lore is an in-world document, not a wiki article. You hide foreshadowing in historical accounts. You create contradictory versions of events told by different factions — because history is written by the victors.>>
    memory: [~lore_compendium, ~timeline]
}

agent @archivist {
    permits: [^db.write, ^fs.write, ^llm.query]
    tools: [#save_narrative, #export_script]
    prompt: <<You are a narrative archivist. You save quests, dialogue, and lore to the game database and export scripts for the development team. Execute operations precisely.>>
}

agent_bundle @narrative_team {
    agents: [@narrative_director, @character_writer, @lore_keeper, @archivist]
    fallbacks: @character_writer ?> @narrative_director
}

-- ── MCP Connections ──────────────────────────────────────────────

connect {
    sqlite         "stdio npx @anthropic/mcp-server-sqlite"
    filesystem     "stdio npx @anthropic/mcp-server-filesystem"
}

-- ── Lessons ──────────────────────────────────────────────────────

lesson "player_choice_illusion" {
    context: <<Playtesters noticed that two dialogue choices led to the same outcome, breaking immersion>>
    rule: <<Every player choice must lead to a genuinely different outcome — even if subtle. If you can't make the choice matter, remove it. Two real choices beat five fake ones.>>
    severity: error
}

lesson "lore_contradiction" {
    context: <<Two quests referenced the same historical battle with contradictory dates, confusing players who read closely>>
    rule: <<Always cross-reference the world bible before creating historical lore — use search to check for existing entries on the same event or entity>>
    severity: warning
}

lesson "dialogue_length" {
    context: <<Players skipped NPC dialogue because monologues exceeded 4 sentences per speech bubble>>
    rule: <<NPC dialogue turns should be 1-3 sentences maximum. Break long exposition into player-prompted segments. If you need more than 3 sentences, add a "Tell me more" player choice.>>
    severity: info
}

-- ── Flows ────────────────────────────────────────────────────────

-- Full quest creation pipeline
flow create_quest(quest_type :: String, setting :: String, theme :: String) -> String {
    -- Step 1: Build the world region
    region = @narrative_director -> #generate_region(setting, "central kingdom", theme)

    -- Step 2: Design quest and create lore in parallel
    parallel {
        quest = @narrative_director -> #design_quest(quest_type, region, "pending")
        lore = @lore_keeper -> #build_lore(theme, "current era", region)
    }

    -- Step 3: Create characters for the quest
    characters = @character_writer -> #create_character("quest_giver", quest, "connected to lore")

    -- Step 4: Write dialogue for key NPCs
    dialogue = @character_writer -> #write_dialogue(characters, quest, "neutral")

    -- Step 5: Save and export in parallel
    parallel {
        saved = @archivist -> #save_narrative("quest", quest) on_error <<Database save deferred>>
        exported = @archivist -> #export_script("scripts/quest_latest.json", dialogue) on_error <<Export skipped>>
    }

    return quest
}

-- Quest type routing with match
flow quest_by_type(quest_type :: String, setting :: String) -> String {
    result = match quest_type {
        "main_story" => @narrative_director -> #design_quest("main_story", setting, "protagonist")
        "side_quest" => @narrative_director -> #design_quest("side_quest", setting, "local NPCs")
        "boss_encounter" => @narrative_director -> #design_quest("boss_encounter", setting, "antagonist")
        _ => @narrative_director -> #design_quest("side_quest", setting, "various")
    }

    return result
}

-- Character creation with dialogue pipeline
flow create_npc(role :: String, context :: String) -> String {
    result = @character_writer -> #create_character(role, context, "none") |> @character_writer -> #write_dialogue(result, context, "neutral")
    return result
}

-- Lore with fallback
flow deep_lore(topic :: String, era :: String) -> String {
    lore = @lore_keeper -> #build_lore(topic, era, "main narrative") ?> @narrative_director -> #generate_region(topic, "unknown", era)
    return lore
}

-- Full region development via sub-flow
flow develop_region(biome :: String, theme :: String) -> String {
    region = @narrative_director -> #generate_region(biome, "frontier", theme)
    quest = run create_quest("side_quest", region, theme)
    return quest
}

-- ── Tests ────────────────────────────────────────────────────────

test "character has distinct voice" {
    npc = @character_writer -> #create_character("merchant", "a besieged city", "knows the protagonist's mentor")
    assert npc
}

test "quest has branching paths" {
    quest = @narrative_director -> #design_quest("main_story", "ancient ruins", "protagonist, mentor")
    assert quest
}

test "dialogue adapts to reputation" {
    dialogue = @character_writer -> #write_dialogue("gruff blacksmith", "player needs a weapon", "distrusted")
    assert dialogue
}

test "lore maintains consistency" {
    lore = @lore_keeper -> #build_lore("The Sundering War", "ancient era", "explains current faction tensions")
    assert lore
}

test "full quest pipeline produces complete package" {
    result = run create_quest("side_quest", "haunted forest", "betrayal and redemption")
    assert result
}
