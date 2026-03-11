# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in PACT, please report it responsibly.

**Do not open a public issue.** Instead, email: gabriel-pact-lang@users.noreply.github.com

Include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

We will acknowledge receipt within 48 hours and provide a timeline for a fix.

## Scope

PACT's permission system is designed to enforce agent contracts at compile time. Security issues in the following areas are especially relevant:

- Permission bypass (agent executing tools it lacks permissions for)
- Guardrail circumvention (compliance boundaries not enforced)
- Source provider injection (malicious input through built-in providers)
- Memory store access (unauthorized read/write to agent memory)

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |
