# Account email aliases can allow two accounts together

Status: remaining documented limit. Agent/MCP request coverage, response and
event filtering, and the connect-time profile handshake are implemented.

`allowed_accounts` accepts account ids, config keys and email addresses. Two
accounts with the same email therefore match an email-based entry together.
Use distinct account ids or config keys when those accounts need different
access. Changing email matching is outside the connect-time handshake fix.

A connection now declares its client kind before receiving events or making
ordinary requests. The kind remains fixed until disconnect. Paired agent/MCP
calls require the selected host's profile as well as the registration's grant.
These are cooperative-client guardrails: a hostile same-user process can
still declare itself `cli`. Declaration does not authenticate a process.

See `site/src/content/docs/reference/config.md` for profile configuration and
`docs/blueprint/20-transports.md` for the wire handshake and compatibility rules.
