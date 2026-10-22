# Project Knowledge Graph

The project knowledge graph is provided by the `megamemory` MCP server and skill tool. There is no implicit project memory between chats.

1. At the start of a chat, call `megamemory` with action `overview` (or `megamemory:list_roots`) before work.
2. Before each task, call `megamemory` with action `query` (or `megamemory:understand`) before reading source files for project understanding.
3. After each task, call `megamemory` with action `record` to create or update linked concepts. Include names, defaults, file locations, and rationale; keep concepts at most three levels deep.

# CGx2 first-wave collaboration

- Read `docs/language-design.md`, `docs/validation-plan.md`, `plans/README.md`, and your assigned `plans/<track>/plan.md` before coding.
- Follow the exclusive path ownership in your plan. Do not edit another track, the root Cargo workspace manifest, shared design documents, or the coordinator ledger.
- Work in the Git worktree attached to your chat. Do not merge branches or automatically start a follow-on task.
- Finish the one bounded coding deliverable, run its specified checks, commit your changes in your worktree, and stop for the user's review. Report files, verification, limitations, and the commit identifier.
- The product is CGx2; source files end in `.cg`, notebooks end in `.cgnb`, the command is `cgx2`, and the package manifest is `cgx2.toml`. The original project directory remains `/home/john-riley/projects/lang`; operate in your attached worktree path.
