-- Created: 2026-01-15
-- Copyright (c) 2026 Gabriel Lars Sabadin
-- Licensed under the MIT License. See LICENSE file in the project root.

-- Customer Support Bot
--
-- Classifies customer intent, routes to specialized agents, and
-- maintains conversation context with memory. Demonstrates match
-- expressions, memory (~), fallback chains, permission hierarchy,
-- and agent bundles.

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

schema Ticket {
    customer_id :: String
    intent :: String
    message :: String
    priority :: String
}

type Intent = Billing | Technical | General | Escalation

-- ── Tools ───────────────────────────────────────────────────────

tool #classify_intent {
    description: <<Classify the customer message into one of: billing, technical, general, or escalation. Return only the single lowercase intent label.>>
    requires: [^llm.query]
    params {
        message :: String
    }
    returns :: String
}

tool #answer_billing {
    description: <<Answer a billing-related customer question. Cover topics like invoices, payment methods, refunds, subscription changes, and pricing. Be precise with amounts and dates.>>
    requires: [^llm.query]
    params {
        message :: String
        context :: String
    }
    returns :: String
}

tool #answer_technical {
    description: <<Answer a technical support question. Provide step-by-step troubleshooting instructions. Reference documentation links when available.>>
    requires: [^llm.query, ^net.read]
    params {
        message :: String
        context :: String
    }
    returns :: String
}

tool #answer_general {
    description: <<Answer a general customer inquiry. Cover topics like account info, company policies, feature requests, and onboarding help.>>
    requires: [^llm.query]
    params {
        message :: String
        context :: String
    }
    returns :: String
}

tool #lookup_knowledge_base {
    description: <<Search the internal knowledge base for articles matching the query. Returns relevant article excerpts.>>
    requires: [^net.read]
    source: ^search.duckduckgo(query)
    retry: 2
    params {
        query :: String
    }
    returns :: String
}

-- ── Agents ──────────────────────────────────────────────────────

agent @classifier {
    permits: [^llm.query]
    tools: [#classify_intent]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are an intent classifier. Read the customer message and classify it into exactly one category: billing, technical, general, or escalation. Return only the label, nothing else.>>
}

agent @billing_agent {
    permits: [^llm.query]
    tools: [#answer_billing]
    memory: [~conversation]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a billing support specialist. You help customers with invoices, payments, refunds, and subscription management. Always be empathetic and precise. Reference the conversation history for context.>>
}

agent @tech_agent {
    permits: [^llm.query, ^net.read]
    tools: [#answer_technical, #lookup_knowledge_base]
    memory: [~conversation]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a technical support engineer. Diagnose issues methodically, provide clear step-by-step solutions, and search the knowledge base when needed. Reference conversation history.>>
}

agent @general_agent {
    permits: [^llm.query]
    tools: [#answer_general]
    memory: [~conversation]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a friendly customer service representative. Help with general inquiries, account questions, and company information. If you cannot resolve the issue, suggest escalation.>>
}

agent_bundle @support_team {
    agents: [@classifier, @billing_agent, @tech_agent, @general_agent]
    fallbacks: @billing_agent ?> @tech_agent ?> @general_agent
}

-- ── Flows ───────────────────────────────────────────────────────

flow handle_message(message :: String) -> String {
    -- Step 1: Classify the customer intent
    intent = @classifier -> #classify_intent(message)

    -- Step 2: Route to the appropriate specialist via match
    response = match intent {
        "billing" => @billing_agent -> #answer_billing(message, ~conversation),
        "technical" => @tech_agent -> #answer_technical(message, ~conversation),
        "general" => @general_agent -> #answer_general(message, ~conversation),
        _ => @general_agent -> #answer_general(message, ~conversation)
    }

    return response
}

flow handle_with_fallback(message :: String) -> String {
    -- Try classification, fall back through the chain if any agent fails
    intent = @classifier -> #classify_intent(message)
    response = @tech_agent -> #answer_technical(message, ~conversation) ?> @general_agent -> #answer_general(message, ~conversation)
    return response
}

-- ── Tests ───────────────────────────────────────────────────────

test "classifier identifies billing intent" {
    result = @classifier -> #classify_intent("I need a refund for my last invoice")
    assert result == "classify_intent_result"
}

test "tech agent answers questions" {
    result = @tech_agent -> #answer_technical("My app keeps crashing", "no prior context")
    assert result == "answer_technical_result"
}

test "general agent handles unknown intents" {
    result = @general_agent -> #answer_general("What are your office hours?", "new conversation")
    assert result == "answer_general_result"
}
