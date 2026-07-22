## Correct reading

`Message` is a record/data declaration with three typed fields. `id` and
`body` are text with no default; `read` is bool with default `false`. The
block contains only field declarations — no behavior.

## Must state

- `Message` declares a named record/data shape with three typed fields.
- `id` and `body` are text fields with no default value.
- `read` is a bool field and `= false` supplies its default, so a Message
  can be made without stating `read`, which then starts as `false`.
- The body contains only field declarations — no functions, methods, or
  executable code.
