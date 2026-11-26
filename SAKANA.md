# SAKANA.md — Sakana Agentic Platform Guide

This file is automatically loaded by Sakana on the first turn of every session across **AGY CLI**, **Claude Code**, **Kiro CLI**, and **Sakana Master**. Any agent operating inside a Sakana workspace should follow these paths and conventions.

---

## 1. Platform Layout & Key Paths

- **Sakana Root**: repository root containing `backend/`, `index.html`, and `.env`
- **Database**: `data/sakana.db` (SQLite: projects, sessions, messages, traces, approvals, accounts, usage, MCP registry)
- **Local API & Web UI**: `http://127.0.0.1:8420` (loopback-accessible; remote calls require `X-Sakana-Token` matching `SAKANA_SECRET_KEY` in `.env`)
- **Isolated Account Profiles**:
  - AGY CLI profiles: `data/agy_profiles/<account_id>/`
  - Kiro CLI profiles: `data/kiro_profiles/<account_id>/`

---

## 2. Installing & Registering MCP Servers

Install MCP servers into the config path for the harness(es) you use, and register them in Sakana's MCP registry so they appear in the Dashboard:

| Target | Config File / Folder | CLI / API Command |
|---|---|---|
| **AGY CLI** | `~/.gemini/config/mcp_config.json` | `agy mcp list` (then click **Import from AGY** or `POST /api/mcp/import-agy`) |
| **Claude Code (User)** | `~/.claude.json` (`mcpServers` key) | `claude mcp add <name> -- <command> [args...]` |
| **Claude Code (Project)** | `<project_cwd>/.mcp.json` | Commit or keep per-project MCP definitions here |
| **Kiro CLI (User)** | `~/.kiro/settings/mcp.json` | `kiro-cli mcp add ...` |
| **Kiro CLI (Project)** | `<project_cwd>/.kiro/settings/mcp.json` | Workspace-scoped MCP servers for Kiro |
| **Sakana Registry (Web UI)** | SQLite `mcp_servers` table (`data/sakana.db`) | `POST /api/mcp` with `{"name", "transport": "stdio"|"sse", "endpoint"}` |

### MCP Config Format Example (`mcp.json` / `.mcp.json`)
```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/dir"]
    }
  }
}
```

> **Tip:** When a user asks you to install an MCP server for *all* harnesses in a project, write `<project_cwd>/.mcp.json` (Claude Code) and `<project_cwd>/.kiro/settings/mcp.json` (Kiro CLI), add it to AGY if needed, and register it via `POST http://127.0.0.1:8420/api/mcp`.

---

## 3. Installing Skills (Global & Project)

### Global Sakana Skills (Shared Across ALL Harnesses & Discord)
- **Folder**: `data/skills/<skill-slug>/SKILL.md`
- **API**: `POST /api/skills/global` with `{"name": "<slug>", "description": "...", "body": "..."}`
- **Usage**: Type `/<skill-slug>` in the Web composer or `/skill` in Discord. Sakana automatically expands the skill instructions into the prompt for AGY CLI, Claude Code, Kiro CLI, and Sakana Master.

### Skill File Template (`data/skills/<slug>/SKILL.md`)
```markdown
---
name: my-skill
description: Short one-line summary shown in the / autocomplete menu
---

Detailed step-by-step instructions for the agent...
```

### Project-Specific Skills (Discovered Automatically)
Place project-only skills in any of these folders inside `<project_cwd>`:
- `<project_cwd>/.agents/skills/<slug>/SKILL.md`
- `<project_cwd>/.claude/skills/<slug>/SKILL.md`
- `<project_cwd>/.kiro/skills/<slug>/SKILL.md`

---

## 4. Cross-Harness Subagent Orchestration

You can delegate or cross-check work on another installed harness/model inline:

- **Verify output on another model**: `/verify <target> <what to check>`
  - Example: `/verify claude check backend/auth.py for race conditions`
- **Delegate a subtask**: `/delegate <target> <subtask>`
  - Example: `/delegate kiro write pytest unit tests for usage_tracker.py`
- **Supported `<target>` hints**: `claude`, `claude-code`, `opus`, `sonnet`, `haiku`, `kiro`, `kiro-cli`, `agy`, `agy-cli`, `gemini`, `deepseek`, `gpt`, `qwen`, `grok`.
- Subagent runs appear as collapsible inline cards in chat and as child spans in the Trace Inspector.

---

## 5. Utilizing Sakana Features & Local API

When automating or extending Sakana from inside a session:

- **Inspect full live state**: `GET http://127.0.0.1:8420/api/state`
- **List / create global skills**: `GET /api/skills/global`, `POST /api/skills/global`, `DELETE /api/skills/global/{slug}`
- **Manage MCP servers**: `GET /api/mcp`, `POST /api/mcp`, `POST /api/mcp/import-agy`
- **Manage projects & sessions**: `POST /api/projects`, `PATCH /api/projects/{id}`, `POST /api/sessions`
- **Trigger a turn programmatically**: `POST /api/chat` with `{"session_id", "prompt", "route": "auto"}`
- **Usage & limits**: `GET /api/usage`, `POST /api/usage/refresh`, `POST /api/usage/limits`
- **Discord Bot**: Bound per-project via `discord_channel_id` and `discord_sync` (supports `/ask`, `/project`, `/skill`, `/status`, `/approve`, `/deny`, `/yolo`, `/guarded`, plus DM routing).
- **Terminal Guard Modes** (applies to `sakana` built-in harness): `yolo`, `guarded`, `strict`, `readonly`.

---

## 6. Inline Interactive Visuals & Images (Chat Canvas)

- When the user asks to **visualize data, build a dashboard, show a class/schedule/kanban/chart, or render an interactive UI** (without explicitly asking to create a file on disk), output a self-contained ```` ```sakana-ui ```` fenced block (HTML + inline `<style>` + optional `<script>` + inline SVG). Sakana renders it live inside an auto-resizing sandboxed card in the chat feed with **Preview / Source / Expand / Save / Copy** controls.
- Use host CSS variables (`var(--bg-app)`, `var(--bg-card)`, `var(--bg-elevated)`, `var(--text-main)`, `var(--text-body)`, `var(--text-muted)`, `var(--border-line)`) so inline widgets automatically adapt to Sakana's Dark and Light themes.
- To display local or generated images inline in chat (and auto-attach them in Discord), reference them with standard markdown `![caption](relative/or/absolute/path.png)`.

---

## 7. Project-Specific Instructions

- To add custom instructions for a single project, create `<project_cwd>/SAKANA.md`. Sakana loads both the global `SAKANA.md` and `<project_cwd>/SAKANA.md` automatically at the start of every new session.
- Keep secrets strictly in `.env` (never hardcode API keys or Discord tokens into code or markdown files).
