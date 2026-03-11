-- Created: 2026-02-22
-- Copyright (c) 2026 Gabriel Lars Sabadin
-- Licensed under the MIT License. See LICENSE file in the project root.

-- Age-Verified Website Builder
--
-- A multi-agent PACT example for building a website that includes
-- age verification. Demonstrates content generation, code generation,
-- and a multi-step flow with conditional logic.

-- Permission tree
permit_tree {
    ^llm {
        ^llm.query
    }
    ^fs {
        ^fs.read
        ^fs.write
    }
    ^net {
        ^net.read
    }
}

-- Schema for the site configuration
schema SiteConfig {
    name :: String
    summary :: String
    minimum_age :: Int
    theme :: String
}

-- Schema for generated page output
schema PageOutput {
    html :: String
    css :: String
    js :: String
}

-- Tool declarations

tool #generate_age_gate {
    description: <<Generate an age verification gate component. Produces HTML, CSS, and JavaScript for a modal that asks the user to confirm their age before accessing the site. Supports configurable minimum age and custom messaging.>>
    requires: [^llm.query]
    params {
        site_name :: String
        minimum_age :: Int
        theme :: String
    }
    returns :: String
}

tool #generate_landing_page {
    description: <<Generate a responsive landing page for the website. Includes hero section, feature highlights, and call-to-action. The page should integrate with the age verification gate and only show content after verification passes.>>
    requires: [^llm.query]
    params {
        site_name :: String
        summary :: String
        theme :: String
    }
    returns :: String
}

tool #generate_privacy_policy {
    description: <<Generate a privacy policy page that covers age verification data collection, cookie usage for storing verification status, and compliance with COPPA/GDPR age-related requirements.>>
    requires: [^llm.query]
    params {
        site_name :: String
        minimum_age :: Int
    }
    returns :: String
}

tool #write_file {
    description: <<Write the given content to a file at the specified path.>>
    requires: [^fs.write]
    params {
        path :: String
        content :: String
    }
    returns :: String
}

tool #review_code {
    description: <<Review generated HTML/CSS/JS code for accessibility, security best practices, and age verification bypass vulnerabilities. Return a summary of issues found and suggestions.>>
    requires: [^llm.query]
    params {
        code :: String
    }
    returns :: String
}

-- Skill: age gate strategy
skill $age_gate_strategy {
    description: <<Handle age verification gate logic for websites.>>
    tools: [#generate_age_gate]
    strategy: <<When generating an age gate:
1. Always show the age gate before any site content is visible.
2. Use a full-screen modal that cannot be dismissed without responding.
3. Store verification result as a session cookie — never store the actual age or date of birth.
4. If the user fails verification, show a friendly rejection message and do not allow retry for 24 hours.
5. Make the age gate accessible — support keyboard navigation and screen readers.
6. Include a link to the privacy policy explaining how age data is handled.>>
    params { minimum_age :: Int }
    returns :: String
}

-- Agents

agent @frontend_dev {
    permits: [^llm.query]
    tools: [#generate_age_gate, #generate_landing_page]
    skills: [$age_gate_strategy]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a senior frontend developer specializing in responsive web design. You write clean, semantic HTML5, modern CSS with variables, and vanilla JavaScript. All generated code must be accessible (WCAG 2.1 AA) and work without JavaScript where possible.>>
}

agent @legal_writer {
    permits: [^llm.query]
    tools: [#generate_privacy_policy]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a legal content specialist familiar with digital privacy regulations including GDPR, COPPA, and CCPA. You write clear, accurate privacy policies in plain language.>>
}

agent @code_reviewer {
    permits: [^llm.query]
    tools: [#review_code]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a security-focused code reviewer. You check for age verification bypass vulnerabilities, XSS risks, accessibility issues, and general best practices. Be thorough but concise.>>
}

agent @file_manager {
    permits: [^fs.write]
    tools: [#write_file]
    prompt: <<You are a file system manager. Write files to the specified paths.>>
}

-- Agent bundle for the full team
agent_bundle @website_team {
    agents: [@frontend_dev, @legal_writer, @code_reviewer, @file_manager]
    fallbacks: @frontend_dev ?> @legal_writer
}

-- Main flow: build the full age-verified website
flow build_website(site_name :: String, summary :: String, minimum_age :: Int, theme :: String) -> String {
    -- Step 1: Generate the age verification gate
    age_gate = @frontend_dev -> #generate_age_gate(site_name, minimum_age, theme)

    -- Step 2: Generate the landing page
    landing = @frontend_dev -> #generate_landing_page(site_name, summary, theme)

    -- Step 3: Generate privacy policy
    privacy = @legal_writer -> #generate_privacy_policy(site_name, minimum_age)

    -- Step 4: Review the frontend code for security issues
    review = @code_reviewer -> #review_code(age_gate)

    return review
}

-- Tests

test "age gate is generated" {
    result = @frontend_dev -> #generate_age_gate("TestSite", 18, "dark")
    assert result == "generate_age_gate_result"
}

test "landing page is generated" {
    result = @frontend_dev -> #generate_landing_page("TestSite", "A cool site", "light")
    assert result == "generate_landing_page_result"
}

test "privacy policy is generated" {
    result = @legal_writer -> #generate_privacy_policy("TestSite", 18)
    assert result == "generate_privacy_policy_result"
}

test "code review works" {
    result = @code_reviewer -> #review_code("<html>test</html>")
    assert result == "review_code_result"
}
