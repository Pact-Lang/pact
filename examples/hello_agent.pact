-- Created: 2025-08-10
-- Copyright (c) 2025-2026 Gabriel Lars Sabadin
-- Licensed under the MIT License. See LICENSE file in the project root.

-- Hello Agent: A minimal PACT example demonstrating agent dispatch.
--
-- This file defines a tool, an agent, and a flow that dispatches
-- a greeting through the agent.

tool #greet {
    description: <<Generate a friendly greeting message for the given name.>>
    handler: "builtin:echo"
    requires: [^llm.query]
    params {
        name :: String
    }
    returns :: String
}

agent @greeter {
    permits: [^llm.query]
    tools: [#greet]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a friendly greeter. When asked to greet someone, respond with a warm, personalized greeting.>>
}

flow hello(name :: String) -> String {
    result = @greeter -> #greet(name)
    return result
}

test "hello produces a result" {
    result = @greeter -> #greet("world")
    assert result == "greet_result"
}
