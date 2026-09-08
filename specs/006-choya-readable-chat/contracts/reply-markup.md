# Contract: reply markup

What Choya is asked to write, and what the bubble renders. The model side and the renderer side must agree on exactly this subset; anything else is shown as plain text, never dropped.

## Asked of the model (REPLY SHAPE paragraph, all three chat prompts)

- At most eight short lines in `explanation`; no paragraph longer than two sentences; no filler, no restating the question.
- `- ` starts a bullet; two leading spaces nest one level.
- `**Name**` marks a specialization, trait, skill, rune, sigil, relic or weapon.
- `_aside_` marks an aside (italic).
- `Skill A -> Skill B -> Skill C` is a rotation line, one per line.
- `! text` is a warning line.
- A web address is written bare, `https://…`.
- Newlines inside the JSON string as `\n`.

## Rendered by the bubble

| Written | Drawn |
|---|---|
| `- item` | `•` and the item, indented by depth |
| `1. item` | `1.` and the item |
| `**Name**` | faux bold (drawn twice, 1 px apart), accent colour when the game data knows the name |
| a known name without `**` | accent colour, normal weight |
| `_aside_` / `*aside*` | italic face when the font family has one on disk, else muted colour |
| `A -> B -> C` | each part on one line joined by `→` in the accent colour |
| `! text` | the line in `WARN` |
| `https://…` | underlined, accent colour, opens in the browser on click |
| anything else | plain text in `cream`, wrapped |

No length cap anywhere in the chain: parser, layout, history, storage.

## Test fixtures

`crates/addon/src/ui/chat_markup.rs` tests cover: a 3,000-character reply keeps every character; nested bullets; a rotation of six parts; `**` inside a bullet; an unknown name stays plain; a URL with trailing punctuation; a line with unmatched `**` renders literally; `->` inside prose without a second part is not a rotation.
