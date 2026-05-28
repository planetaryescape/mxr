# Calendar Email Synthesis Notes

Obsidian-style concept and synthesis notes distilled from the calendar-email
work. These are not task trackers. They preserve reusable ideas that apply
beyond `mxr`, then point back to the code-truth docs when the idea has a local
implementation.

## Notes

| Note | Kind | Use it when |
|---|---|---|
| [Email invites are mail state before calendar state](email-invites-are-mail-state-before-calendar-state.md) | Concept | You need to decide whether an app should parse invites or become a calendar. |
| [Dry-run social mutations](dry-run-social-mutations.md) | Concept | A feature sends or mutates on another person's behalf. |
| [Capability slices before platform expansion](capability-slices-before-platform-expansion.md) | Concept | A small interoperability feature is tempting the product toward a larger platform. |
| [Calendar email implementation synthesis](calendar-email-implementation-synthesis.md) | Synthesis | You need the durable lessons from the shipped `mxr` calendar-email slice. |

## Code Truth

Start with the current-state audit, then use the concept notes for reusable
principles.

```bash
mxr invites list --limit 20
mxr invite reply MESSAGE_ID accept --dry-run --format json
```

See also:

- [Current state](../blueprint/02-current-state.md)
- [Standards](../blueprint/01-standards.md)
- [Security](../blueprint/07-security.md)
- [Surfaces](../blueprint/08-surfaces.md)
