# Conventions

## Language

**Default: English** for all technical writing — code, comments,
commit messages, documentation, variable names, tool descriptions.

When updating existing content, follow the document's existing
language.

## File Operations (Hard Rule)

**Applies only within git repositories.**

| Operation | Method |
|-----------|--------|
| Move `.md` / `.txt` files | `mv` |
| Move other files | `mv` first, then delete-and-rewrite using the built-in Read/Write tools |
| Rename files | delete first, then Write/Edit to create the new file (not `git mv`) |
| Bulk content changes | built-in Edit/Write tools |
| Shell scripts | ask user for confirmation before executing |

**Write = Permission Granted** when:
- The file has been read in this session, **or**
- The user provided the content explicitly, **or**
- The file is being written new (never existed).

Read the file before writing it. Other directories are forbidden by
default unless explicitly referenced.

## Security

- No API keys, tokens, credentials, or private endpoints in any
  tracked file. Use environment variables or a local `.env` that's
  git-ignored.
- No customer data, internal hostnames, or proprietary URLs in
  examples unless they are obviously placeholder values.

## Banned Phrases

See `.agents/engineering-rules.md` for the banned-phrase list. It
applies here too: commit messages, code comments, progress notes.